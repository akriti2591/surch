use crate::theme::SurchTheme;
use gpui::*;
use gpui::prelude::FluentBuilder;
use gpui::ScrollStrategy;
use gpui_component::input::{InputEvent, InputState};
use gpui_component::spinner::Spinner;
use gpui_component::{Icon, IconName, Sizable};
use std::collections::HashMap;
use std::ops::Range;
use std::path::PathBuf;
use surch_core::channel::InputFieldSpec;
use surch_core::path_trie::{self, TrieInput};

/// A single match result displayed in the result list.
#[derive(Debug, Clone)]
pub struct SearchResultItem {
    pub id: u64,
    pub file_path: PathBuf,
    pub line_number: usize,
    pub line_content: String,
    pub match_ranges: Vec<Range<usize>>,
}

/// Results grouped by file.
#[derive(Debug, Clone)]
pub struct FileGroup {
    pub file_path: PathBuf,
    pub relative_path: String,
    pub matches: Vec<SearchResultItem>,
    pub collapsed: bool,
}

/// Whether results are shown as a flat list or directory tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewMode {
    Flat,
    Tree,
}

/// A flattened row for the virtualized list — either a file header, match, or directory header.
#[derive(Clone)]
#[allow(dead_code)]
enum FlatRow {
    FileHeader {
        group_idx: usize,
        relative_path: String,
        /// Display name: full relative path in Flat mode, just filename in Tree mode.
        display_name: String,
        match_count: usize,
        collapsed: bool,
        /// Indentation depth (0 in Flat mode, directory depth in Tree mode).
        depth: usize,
    },
    MatchRow {
        item: SearchResultItem,
        /// Indentation depth for the match row.
        depth: usize,
    },
    DirectoryHeader {
        /// Unique ID for this directory node (index into dir_collapsed map).
        dir_id: usize,
        /// Display name (just the directory segment, e.g. "components").
        name: String,
        /// Aggregate match count for all files under this directory.
        match_count: usize,
        /// Whether this directory is collapsed.
        collapsed: bool,
        /// Indentation depth.
        depth: usize,
    },
}

/// One entry in the sticky header stack rendered at the top of the results list.
struct StickyEntry {
    name: String,
    match_count: usize,
    depth: usize,
    is_dir: bool,
    /// Pixel offset to push this header upward during transition.
    push_up: f32,
}

pub struct SearchPanel {
    input_fields: Vec<InputFieldSpec>,
    pub(crate) inputs: HashMap<String, Entity<InputState>>,
    file_groups: Vec<FileGroup>,
    flat_rows: Vec<FlatRow>,
    selected_result: Option<u64>,
    total_matches: usize,
    total_files: usize,
    is_searching: bool,
    workspace_root: Option<PathBuf>,
    results_scroll_handle: UniformListScrollHandle,
    case_sensitive: bool,
    whole_word: bool,
    is_regex: bool,
    preserve_case: bool,
    fuzzy: bool,
    all_collapsed: bool,
    search_completed: bool,
    view_mode: ViewMode,
    /// Collapsed state for directory nodes in tree view, keyed by directory path.
    dir_collapsed: HashMap<String, bool>,
    /// Stable mapping from dir_id -> dir_path, rebuilt each time.
    dir_id_to_path: Vec<String>,
    pub on_query_changed:
        Option<Box<dyn Fn(HashMap<String, String>, &mut Window, &mut Context<Self>)>>,
    pub on_result_selected:
        Option<Box<dyn Fn(&SearchResultItem, &mut Window, &mut Context<Self>)>>,
    pub on_refresh: Option<Box<dyn Fn(&mut Window, &mut Context<Self>)>>,
    pub on_close_project: Option<Box<dyn Fn(&mut Window, &mut Context<Self>)>>,
    pub on_replace_all: Option<Box<dyn Fn(String, &mut Window, &mut Context<Self>)>>,
}

impl SearchPanel {
    pub fn new(
        input_fields: Vec<InputFieldSpec>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut inputs = HashMap::new();
        for field in &input_fields {
            let placeholder = field.placeholder.clone();
            let field_id = field.id.clone();
            let input = cx.new(|cx| InputState::new(window, cx).placeholder(placeholder));

            cx.subscribe_in(&input, window, {
                let field_id = field_id.clone();
                move |this: &mut SearchPanel, _state, event: &InputEvent, window, cx| {
                    if matches!(event, InputEvent::Change) {
                        this.on_input_changed(&field_id, window, cx);
                    }
                }
            })
            .detach();

            inputs.insert(field_id, input);
        }

        Self {
            input_fields,
            inputs,
            file_groups: Vec::new(),
            flat_rows: Vec::new(),
            selected_result: None,
            total_matches: 0,
            total_files: 0,
            is_searching: false,
            workspace_root: None,
            results_scroll_handle: UniformListScrollHandle::default(),
            case_sensitive: false,
            whole_word: false,
            is_regex: false,
            preserve_case: false,
            fuzzy: false,
            all_collapsed: false,
            search_completed: false,
            view_mode: ViewMode::Flat,
            dir_collapsed: HashMap::new(),
            dir_id_to_path: Vec::new(),
            on_query_changed: None,
            on_result_selected: None,
            on_refresh: None,
            on_close_project: None,
            on_replace_all: None,
        }
    }

    pub fn set_workspace_root(&mut self, root: PathBuf) {
        self.workspace_root = Some(root);
    }

    pub fn search_options(&self) -> (bool, bool, bool, bool, bool) {
        (self.case_sensitive, self.whole_word, self.is_regex, self.preserve_case, self.fuzzy)
    }

    pub fn focus_find(&self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(input) = self.inputs.get("find") {
            input.update(cx, |state, cx| {
                state.focus(window, cx);
            });
        }
    }

    pub fn toggle_case_sensitive(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.case_sensitive = !self.case_sensitive;
        self.on_input_changed("find", window, cx);
        cx.notify();
    }

    pub fn toggle_whole_word(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.whole_word = !self.whole_word;
        self.on_input_changed("find", window, cx);
        cx.notify();
    }

    pub fn toggle_regex(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.is_regex = !self.is_regex;
        // Regex and fuzzy are mutually exclusive
        if self.is_regex && self.fuzzy {
            self.fuzzy = false;
        }
        self.on_input_changed("find", window, cx);
        cx.notify();
    }

    pub fn toggle_fuzzy(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.fuzzy = !self.fuzzy;
        // Fuzzy and regex are mutually exclusive
        if self.fuzzy && self.is_regex {
            self.is_regex = false;
        }
        self.on_input_changed("find", window, cx);
        cx.notify();
    }

    /// Restore search options from persisted workspace state.
    pub fn restore_options(&mut self, case_sensitive: bool, whole_word: bool, is_regex: bool, preserve_case: bool, fuzzy: bool) {
        self.case_sensitive = case_sensitive;
        self.whole_word = whole_word;
        self.is_regex = is_regex;
        self.preserve_case = preserve_case;
        self.fuzzy = fuzzy;
    }

    /// Returns true if any input field currently has focus.
    pub fn any_input_focused(&self, window: &Window, cx: &App) -> bool {
        self.inputs.values().any(|input| {
            input.read(cx).focus_handle(cx).is_focused(window)
        })
    }

    /// Toggle between Flat and Tree view modes.
    pub fn toggle_view_mode(&mut self) {
        self.view_mode = match self.view_mode {
            ViewMode::Flat => ViewMode::Tree,
            ViewMode::Tree => ViewMode::Flat,
        };
        self.rebuild_flat_rows();
    }

    /// Select the next match row, update state, and return the selected item.
    /// Does NOT fire `on_result_selected` — caller handles that to avoid re-entrant entity updates.
    pub fn select_next_item(&mut self, cx: &mut Context<Self>) -> Option<SearchResultItem> {
        if self.flat_rows.is_empty() {
            return None;
        }

        let current_idx = self.selected_result.and_then(|selected_id| {
            self.flat_rows.iter().position(|row| matches!(row, FlatRow::MatchRow { item, .. } if item.id == selected_id))
        });

        let start = current_idx.map(|i| i + 1).unwrap_or(0);
        for i in start..self.flat_rows.len() {
            if let FlatRow::MatchRow { item, .. } = &self.flat_rows[i] {
                self.selected_result = Some(item.id);
                self.results_scroll_handle.scroll_to_item(i, ScrollStrategy::Center);
                cx.notify();
                return Some(item.clone());
            }
        }
        None
    }

    /// Select the previous match row, update state, and return the selected item.
    /// Does NOT fire `on_result_selected` — caller handles that to avoid re-entrant entity updates.
    pub fn select_previous_item(&mut self, cx: &mut Context<Self>) -> Option<SearchResultItem> {
        if self.flat_rows.is_empty() {
            return None;
        }

        let current_idx = self.selected_result.and_then(|selected_id| {
            self.flat_rows.iter().position(|row| matches!(row, FlatRow::MatchRow { item, .. } if item.id == selected_id))
        });

        let end = match current_idx {
            Some(0) | None => return None,
            Some(i) => i,
        };

        for i in (0..end).rev() {
            if let FlatRow::MatchRow { item, .. } = &self.flat_rows[i] {
                self.selected_result = Some(item.id);
                self.results_scroll_handle.scroll_to_item(i, ScrollStrategy::Center);
                cx.notify();
                return Some(item.clone());
            }
        }
        None
    }

    fn on_input_changed(&mut self, _field_id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let values = self.collect_input_values(cx);
        if let Some(ref handler) = self.on_query_changed {
            handler(values, window, cx);
        }
    }

    fn collect_input_values(&self, cx: &Context<Self>) -> HashMap<String, String> {
        let mut values = HashMap::new();
        for (id, input) in &self.inputs {
            let value = input.read(cx).value().to_string();
            values.insert(id.clone(), value);
        }
        values
    }

    /// Rebuild the flat row list from file_groups.
    fn rebuild_flat_rows(&mut self) {
        self.flat_rows.clear();
        match self.view_mode {
            ViewMode::Flat => self.rebuild_flat_rows_flat(),
            ViewMode::Tree => self.rebuild_flat_rows_tree(),
        }
    }

    /// Flat mode: simple file header + match rows (original behavior).
    fn rebuild_flat_rows_flat(&mut self) {
        for (group_idx, group) in self.file_groups.iter().enumerate() {
            self.flat_rows.push(FlatRow::FileHeader {
                group_idx,
                relative_path: group.relative_path.clone(),
                display_name: group.relative_path.clone(),
                match_count: group.matches.len(),
                collapsed: group.collapsed,
                depth: 0,
            });
            if !group.collapsed {
                for item in &group.matches {
                    self.flat_rows.push(FlatRow::MatchRow {
                        item: item.clone(),
                        depth: 1,
                    });
                }
            }
        }
    }

    /// Tree mode: build a directory trie, then flatten it with directory headers.
    fn rebuild_flat_rows_tree(&mut self) {
        // Build trie inputs from file_groups
        let inputs: Vec<TrieInput> = self
            .file_groups
            .iter()
            .enumerate()
            .map(|(idx, group)| TrieInput {
                relative_path: group.relative_path.clone(),
                group_index: idx,
                match_count: group.matches.len(),
            })
            .collect();
        let trie = path_trie::build_path_trie(&inputs);

        // Clear and rebuild dir_id_to_path
        self.dir_id_to_path.clear();

        // Flatten the trie into flat_rows
        self.flatten_trie_node(&trie, 0);
    }

    fn flatten_trie_node(&mut self, node: &path_trie::TrieNode, depth: usize) {
        // Sort children: directories first (alphabetical), then files (alphabetical)
        let mut dir_keys: Vec<&String> = node.children.keys().collect();
        dir_keys.sort();

        let mut file_keys: Vec<&String> = node.files.keys().collect();
        file_keys.sort();

        // Emit directory children
        for dir_name in dir_keys {
            let child = &node.children[dir_name];
            let dir_path = if node.path.is_empty() {
                dir_name.clone()
            } else {
                format!("{}/{}", node.path, dir_name)
            };

            let match_count = child.total_match_count();
            let dir_id = self.dir_id_to_path.len();
            self.dir_id_to_path.push(dir_path.clone());

            let collapsed = *self.dir_collapsed.get(&dir_path).unwrap_or(&false);

            self.flat_rows.push(FlatRow::DirectoryHeader {
                dir_id,
                name: dir_name.clone(),
                match_count,
                collapsed,
                depth,
            });

            if !collapsed {
                self.flatten_trie_node(child, depth + 1);
            }
        }

        // Emit file children
        for file_name in file_keys {
            let (group_idx, _match_count) = node.files[file_name];
            let group = &self.file_groups[group_idx];

            self.flat_rows.push(FlatRow::FileHeader {
                group_idx,
                relative_path: group.relative_path.clone(),
                display_name: file_name.clone(),
                match_count: group.matches.len(),
                collapsed: group.collapsed,
                depth,
            });

            if !group.collapsed {
                for item in &group.matches {
                    self.flat_rows.push(FlatRow::MatchRow {
                        item: item.clone(),
                        depth: depth + 1,
                    });
                }
            }
        }
    }

    pub fn clear_results(&mut self) {
        self.file_groups.clear();
        self.flat_rows.clear();
        self.selected_result = None;
        self.total_matches = 0;
        self.total_files = 0;
        self.search_completed = false;
    }

    /// Find the file header info that should be "sticky" at the top of the results list.
    /// Returns None if the first visible row is already a header (no sticky needed).
    /// Returns Some((display_name, match_count, depth, collapsed, is_dir)) if a sticky header should show.
    /// Compute the stack of sticky headers and their push-up offsets.
    ///
    /// Algorithm (follows VS Code's tree sticky scroll approach):
    /// 1. Find the first visible row index from scroll offset.
    /// 2. Walk backwards to collect ancestor headers at each depth level.
    /// 3. For each sticky entry, find its "last descendant" — the last row
    ///    before the next header at the same depth or shallower. The bottom
    ///    of that descendant is the boundary where the sticky entry should
    ///    start being pushed upward.
    /// 4. Enforce a max height of 40% of the viewport.
    fn sticky_headers(&self) -> Option<Vec<StickyEntry>> {
        if self.flat_rows.is_empty() {
            return None;
        }

        let (top_idx, per_item_px, scroll_y_px, viewport_h) = {
            let state = self.results_scroll_handle.0.borrow();
            if state.deferred_scroll_to_item.is_some() {
                return None;
            }
            let item_count = self.flat_rows.len();
            if item_count == 0 {
                return None;
            }
            let item_size = state.last_item_size.as_ref()?;
            let content_h = item_size.contents.height;
            let viewport_h: f32 = item_size.item.height / px(1.0);
            let per_item: f32 = content_h / px(item_count as f32);
            if per_item <= 0.0 {
                return None;
            }
            let offset = state.base_handle.offset();
            let scroll_y: f32 = (px(0.0) - offset.y) / px(1.0);
            let idx = (scroll_y / per_item).floor().max(0.0) as usize;
            (idx, per_item, scroll_y, viewport_h)
        };

        if top_idx >= self.flat_rows.len() {
            return None;
        }

        // Determine the starting depth for the backwards walk.
        // If top_idx is a header, we don't include it (it's already visible),
        // but we still need to collect its parent headers as sticky.
        let start_depth = match &self.flat_rows[top_idx] {
            FlatRow::FileHeader { depth, .. } => {
                if *depth == 0 { return None; } // No parents to show
                *depth // Start looking for ancestors shallower than this
            }
            FlatRow::DirectoryHeader { depth, .. } => {
                if *depth == 0 { return None; }
                *depth
            }
            FlatRow::MatchRow { .. } => usize::MAX, // Collect all ancestors
        };

        // Walk backwards to collect ancestor headers at each depth level.
        let mut stack: Vec<(usize, StickyEntry)> = Vec::new();
        let mut min_depth_seen = start_depth;

        for i in (0..top_idx).rev() {
            let (name, match_count, depth, is_dir) = match &self.flat_rows[i] {
                FlatRow::FileHeader { display_name, match_count, depth, relative_path, .. } => {
                    let name = if self.view_mode == ViewMode::Flat {
                        relative_path.clone()
                    } else {
                        display_name.clone()
                    };
                    (name, *match_count, *depth, false)
                }
                FlatRow::DirectoryHeader { name, match_count, depth, .. } => {
                    (name.clone(), *match_count, *depth, true)
                }
                FlatRow::MatchRow { .. } => continue,
            };

            if depth < min_depth_seen {
                min_depth_seen = depth;
                stack.push((i, StickyEntry {
                    name,
                    match_count,
                    depth,
                    is_dir,
                    push_up: 0.0,
                }));
            }

            if depth == 0 {
                break;
            }
        }

        if stack.is_empty() {
            return None;
        }

        // Reverse so shallowest depth (outermost ancestor) is first
        stack.reverse();

        let sticky_row_height: f32 = 28.0;
        let max_sticky_height = viewport_h * 0.4;
        let max_count = 7usize;

        // Constrain: remove deepest entries until within limits
        while stack.len() > max_count
            || (stack.len() as f32 * sticky_row_height) > max_sticky_height
        {
            if stack.is_empty() {
                break;
            }
            // Remove the deepest (last) entry
            stack.pop();
        }

        if stack.is_empty() {
            return None;
        }

        // Compute push-up for each entry using VS Code's approach:
        // Find the last descendant of each sticky node's subtree.
        // The boundary is the bottom of that last descendant.
        for (entry_position, (header_row_idx, entry)) in stack.iter_mut().enumerate() {
            let entry_top = entry_position as f32 * sticky_row_height;

            // Find the last descendant: scan forward from header_row_idx
            // until we hit a header at the same depth or shallower.
            let mut last_descendant_idx = *header_row_idx;
            for j in (*header_row_idx + 1)..self.flat_rows.len() {
                let row_depth = match &self.flat_rows[j] {
                    FlatRow::FileHeader { depth, .. } => Some(*depth),
                    FlatRow::DirectoryHeader { depth, .. } => Some(*depth),
                    FlatRow::MatchRow { depth, .. } => Some(*depth),
                };
                if let Some(d) = row_depth {
                    if d <= entry.depth {
                        // This row is at the same level or above — not a descendant
                        match &self.flat_rows[j] {
                            FlatRow::FileHeader { .. } | FlatRow::DirectoryHeader { .. } => break,
                            _ => {}
                        }
                    }
                }
                last_descendant_idx = j;
            }

            // Bottom of last descendant in viewport coordinates
            let last_desc_bottom = (last_descendant_idx + 1) as f32 * per_item_px - scroll_y_px;
            let sticky_bottom = entry_top + sticky_row_height;

            if last_desc_bottom < sticky_bottom {
                entry.push_up = sticky_bottom - last_desc_bottom;
            }
        }

        // Cascade push-up: when a parent entry is pushed, all deeper entries
        // below it must be pushed at least as much. E.g., if a depth-0 directory
        // is being pushed out, the depth-1 dir and depth-2 file under it must
        // also be pushed out together.
        for i in 1..stack.len() {
            let parent_push = stack[i - 1].1.push_up;
            if stack[i].1.push_up < parent_push {
                stack[i].1.push_up = parent_push;
            }
        }

        // Remove entries that are fully pushed out of view
        let entries: Vec<StickyEntry> = stack
            .into_iter()
            .map(|(_, e)| e)
            .filter(|e| e.push_up < sticky_row_height)
            .collect();

        if entries.is_empty() {
            return None;
        }

        Some(entries)
    }

    pub fn set_searching(&mut self, searching: bool) {
        self.is_searching = searching;
    }

    pub fn add_result(&mut self, item: SearchResultItem) {
        let file_path = item.file_path.clone();
        let relative = if let Some(ref root) = self.workspace_root {
            file_path
                .strip_prefix(root)
                .unwrap_or(&file_path)
                .to_string_lossy()
                .to_string()
        } else {
            file_path.to_string_lossy().to_string()
        };

        if let Some(group) = self
            .file_groups
            .iter_mut()
            .find(|g| g.file_path == file_path)
        {
            group.matches.push(item);
        } else {
            self.file_groups.push(FileGroup {
                file_path,
                relative_path: relative,
                matches: vec![item],
                collapsed: false,
            });
        }
        self.total_matches += 1;
        self.rebuild_flat_rows();
    }

    pub fn set_complete(&mut self, total_files: usize, total_matches: usize) {
        self.total_files = total_files;
        self.total_matches = total_matches;
        self.is_searching = false;
        self.search_completed = true;
    }

    fn collapse_all(&mut self) {
        for group in &mut self.file_groups {
            group.collapsed = true;
        }
        // In tree mode, also collapse all directories
        for (_, collapsed) in self.dir_collapsed.iter_mut() {
            *collapsed = true;
        }
        self.all_collapsed = true;
        self.rebuild_flat_rows();
    }

    fn expand_all(&mut self) {
        for group in &mut self.file_groups {
            group.collapsed = false;
        }
        // In tree mode, also expand all directories
        for (_, collapsed) in self.dir_collapsed.iter_mut() {
            *collapsed = false;
        }
        self.all_collapsed = false;
        self.rebuild_flat_rows();
    }

    fn render_toggle_button(
        &self,
        id: &str,
        label: &str,
        active: bool,
        cx: &mut Context<Self>,
        on_toggle: fn(&mut Self) -> &mut bool,
    ) -> impl IntoElement {
        let is_fuzzy_toggle = id == "toggle-fuzzy";
        let is_regex_toggle = id == "toggle-regex";
        let mut btn = div()
            .id(ElementId::Name(id.to_string().into()))
            .w(px(22.0))
            .h(px(22.0))
            .rounded(px(3.0))
            .flex()
            .items_center()
            .justify_center()
            .cursor_pointer()
            .text_size(px(11.0))
            .font_family("Menlo")
            .child(label.to_string());
        if active {
            btn = btn
                .bg(SurchTheme::toggle_active_bg())
                .text_color(SurchTheme::text_primary());
        } else {
            btn = btn.text_color(SurchTheme::text_secondary());
        }
        btn.on_click(cx.listener(move |this, _, window, cx| {
            let field = on_toggle(this);
            *field = !*field;
            // Fuzzy and regex are mutually exclusive
            if is_fuzzy_toggle && this.fuzzy {
                this.is_regex = false;
            } else if is_regex_toggle && this.is_regex {
                this.fuzzy = false;
            }
            this.on_input_changed("find", window, cx);
            cx.notify();
        }))
    }

    fn render_highlighted_line(
        content: &str,
        ranges: &[Range<usize>],
        replace_text: Option<&str>,
    ) -> Div {
        // Trim leading whitespace and adjust match ranges accordingly
        let trimmed_start = content.len() - content.trim_start().len();
        let display_content = content.trim_start();

        let mut container = div()
            .flex_1()
            .flex()
            .overflow_hidden()
            .text_ellipsis()
            .whitespace_nowrap()
            .text_size(px(12.0))
            .font_family("Menlo");

        if ranges.is_empty() || display_content.is_empty() {
            return container
                .text_color(SurchTheme::text_primary())
                .child(display_content.to_string());
        }

        // Adjust ranges to account for trimmed leading whitespace
        let adjusted_ranges: Vec<Range<usize>> = ranges
            .iter()
            .filter_map(|r| {
                let start = r.start.max(trimmed_start).saturating_sub(trimmed_start);
                let end = r.end.saturating_sub(trimmed_start).min(display_content.len());
                if end > start {
                    Some(start..end)
                } else {
                    None
                }
            })
            .collect();

        let mut last_end = 0;
        for range in &adjusted_ranges {
            let start = range.start.min(display_content.len());
            let end = range.end.min(display_content.len());
            if start > last_end {
                container = container.child(
                    div()
                        .text_color(SurchTheme::text_primary())
                        .child(display_content[last_end..start].to_string()),
                );
            }
            if end > start {
                if let Some(replacement) = replace_text {
                    // Replace preview mode: strikethrough old + green new
                    container = container
                        .child(
                            div()
                                .bg(SurchTheme::replace_old_bg())
                                .text_color(SurchTheme::text_primary())
                                .line_through()
                                .rounded(px(2.0))
                                .px(px(1.0))
                                .child(display_content[start..end].to_string()),
                        )
                        .child(
                            div()
                                .bg(SurchTheme::replace_new_bg())
                                .text_color(SurchTheme::text_heading())
                                .rounded(px(2.0))
                                .px(px(1.0))
                                .child(replacement.to_string()),
                        );
                } else {
                    // Normal match highlight
                    container = container.child(
                        div()
                            .bg(SurchTheme::match_bg())
                            .text_color(SurchTheme::text_match())
                            .rounded(px(2.0))
                            .px(px(1.0))
                            .child(display_content[start..end].to_string()),
                    );
                }
            }
            last_end = end;
        }
        if last_end < display_content.len() {
            container = container.child(
                div()
                    .text_color(SurchTheme::text_primary())
                    .child(display_content[last_end..].to_string()),
            );
        }
        container
    }

    fn render_input_field(
        &self,
        field: &InputFieldSpec,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let input = self.inputs.get(&field.id).unwrap();
        let is_find = field.id == "find";

        let mut container = div().w_full().mb_1();

        container = container.child(
            div()
                .text_size(px(11.0))
                .font_weight(FontWeight::MEDIUM)
                .text_color(SurchTheme::text_secondary())
                .mb(px(4.0))
                .child(field.label.clone()),
        );

        let is_replace = field.id == "replace";

        if is_find {
            container = container.child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(2.0))
                    .child(
                        div()
                            .flex_1()
                            .child(gpui_component::input::Input::new(input).w_full()),
                    )
                    .child(self.render_toggle_button(
                        "toggle-case",
                        "Aa",
                        self.case_sensitive,
                        cx,
                        |s| &mut s.case_sensitive,
                    ))
                    .child(self.render_toggle_button(
                        "toggle-word",
                        "Ab",
                        self.whole_word,
                        cx,
                        |s| &mut s.whole_word,
                    ))
                    .child(self.render_toggle_button(
                        "toggle-regex",
                        ".*",
                        self.is_regex,
                        cx,
                        |s| &mut s.is_regex,
                    ))
                    .child(self.render_toggle_button(
                        "toggle-fuzzy",
                        "Fz",
                        self.fuzzy,
                        cx,
                        |s| &mut s.fuzzy,
                    )),
            );
        } else if is_replace {
            container = container.child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(2.0))
                    .child(
                        div()
                            .flex_1()
                            .child(gpui_component::input::Input::new(input).w_full()),
                    )
                    .child(self.render_toggle_button(
                        "toggle-preserve-case",
                        "AB",
                        self.preserve_case,
                        cx,
                        |s| &mut s.preserve_case,
                    ))
                    .child(
                        div()
                            .id("btn-replace-all")
                            .w(px(22.0))
                            .h(px(22.0))
                            .rounded(px(3.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .cursor_pointer()
                            .hover(|s| s.bg(SurchTheme::bg_hover()))
                            .child(
                                Icon::new(IconName::Replace)
                                    .size_4()
                                    .text_color(SurchTheme::text_heading()),
                            )
                            .on_click(cx.listener(|this, _, window, cx| {
                                if let Some(ref handler) = this.on_replace_all {
                                    // Get the replace text from the input
                                    let replace_text = this
                                        .inputs
                                        .get("replace")
                                        .map(|input| input.read(cx).value().to_string())
                                        .unwrap_or_default();
                                    handler(replace_text, window, cx);
                                }
                            })),
                    ),
            );
        } else {
            container =
                container.child(gpui_component::input::Input::new(input).w_full());
        }

        container
    }

    fn render_status_and_toolbar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let mut container = div()
            .px(px(12.0))
            .py(px(6.0))
            .flex()
            .items_center()
            .gap(px(6.0))
            .text_size(px(11.0))
            .text_color(SurchTheme::text_secondary())
            .flex_shrink_0();

        if self.is_searching {
            container = container
                .child(
                    Spinner::new()
                        .with_size(gpui_component::Size::XSmall)
                        .color(SurchTheme::accent()),
                )
                .child("Searching...");
        } else if self.total_matches > 0 {
            container = container.child(format!(
                "{} results in {} files",
                self.total_matches,
                self.file_groups.len()
            ));
        }

        // Spacer to push toolbar buttons to the right
        container = container.child(div().flex_1());

        // Toolbar buttons
        if self.total_matches > 0 {
            // View mode toggle (flat list vs tree)
            let is_tree = self.view_mode == ViewMode::Tree;
            container = container.child(
                div()
                    .id("btn-view-mode")
                    .w(px(22.0))
                    .h(px(22.0))
                    .rounded(px(3.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor_pointer()
                    .hover(|s| s.bg(SurchTheme::bg_hover()))
                    .when(is_tree, |s| s.bg(SurchTheme::toggle_active_bg()))
                    .child(
                        // Use list-tree icon when in flat mode (clicking switches to tree),
                        // use list icon when in tree mode (clicking switches to flat).
                        svg()
                            .path(if is_tree {
                                "icons/list.svg"
                            } else {
                                "icons/list-tree.svg"
                            })
                            .size_3()
                            .text_color(if is_tree {
                                SurchTheme::text_heading()
                            } else {
                                SurchTheme::text_secondary()
                            }),
                    )
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.toggle_view_mode();
                        cx.notify();
                    })),
            );

            // Refresh search
            container = container.child(
                div()
                    .id("btn-refresh")
                    .w(px(22.0))
                    .h(px(22.0))
                    .rounded(px(3.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor_pointer()
                    .hover(|s| s.bg(SurchTheme::bg_hover()))
                    .child(
                        Icon::new(IconName::Redo)
                            .size_3()
                            .text_color(SurchTheme::text_secondary()),
                    )
                    .on_click(cx.listener(|this, _, window, cx| {
                        if let Some(ref handler) = this.on_refresh {
                            handler(window, cx);
                        }
                    })),
            );

            // Collapse/Expand all
            let all_collapsed = self.all_collapsed;
            container = container.child(
                div()
                    .id("btn-collapse-all")
                    .w(px(22.0))
                    .h(px(22.0))
                    .rounded(px(3.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor_pointer()
                    .hover(|s| s.bg(SurchTheme::bg_hover()))
                    .child(
                        Icon::new(if all_collapsed {
                            IconName::ChevronsUpDown
                        } else {
                            IconName::Minimize
                        })
                        .size_3()
                        .text_color(SurchTheme::text_secondary()),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if all_collapsed {
                            this.expand_all();
                        } else {
                            this.collapse_all();
                        }
                        cx.notify();
                    })),
            );
        }

        container
    }

}

impl Render for SearchPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut panel = div()
            .flex()
            .flex_col()
            .w_full()
            .h_full()
            .overflow_hidden()
            .bg(SurchTheme::bg_secondary());

        // Header with close project button
        panel = panel.child(
            div()
                .px(px(12.0))
                .py(px(8.0))
                .flex()
                .items_center()
                .text_size(px(11.0))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(SurchTheme::text_secondary())
                .border_b_1()
                .border_color(SurchTheme::border())
                .flex_shrink_0()
                .child("SEARCH")
                .child(div().flex_1())
                .child(
                    div()
                        .id("btn-close-project")
                        .w(px(20.0))
                        .h(px(20.0))
                        .rounded(px(3.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .cursor_pointer()
                        .hover(|s| s.bg(SurchTheme::bg_hover()))
                        .child(
                            Icon::new(IconName::Close)
                                .size_3()
                                .text_color(SurchTheme::text_secondary()),
                        )
                        .on_click(cx.listener(|this, _, window, cx| {
                            if let Some(ref handler) = this.on_close_project {
                                handler(window, cx);
                            }
                        })),
                ),
        );

        // Input fields — flex_shrink_0 prevents jank when results push against inputs
        let mut inputs_container = div()
            .flex()
            .flex_col()
            .px(px(12.0))
            .py(px(8.0))
            .gap(px(6.0))
            .flex_shrink_0();
        for field in self.input_fields.clone() {
            inputs_container = inputs_container.child(self.render_input_field(&field, cx));
        }
        panel = panel.child(inputs_container);

        // Status bar with toolbar buttons
        panel = panel.child(
            div()
                .flex_shrink_0()
                .child(self.render_status_and_toolbar(cx)),
        );

        // "No results found" empty state
        if !self.is_searching && self.search_completed && self.total_matches == 0 {
            panel = panel.child(
                div()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .text_size(px(12.0))
                    .text_color(SurchTheme::text_muted())
                    .child("No results found"),
            );
            return panel;
        }

        // Virtualized results list using uniform_list
        let row_count = self.flat_rows.len();
        let rows_snapshot = self.flat_rows.clone();
        let selected = self.selected_result;
        let replace_text: Option<String> = self
            .inputs
            .get("replace")
            .map(|input| input.read(cx).value().to_string())
            .filter(|s| !s.is_empty());

        // Compute sticky headers before building the uniform_list
        let sticky_headers = self.sticky_headers();

        let results_list = uniform_list("search-results", row_count, {
                let cx_listener = cx.entity().clone();
                move |range, _window, _cx| {
                    let mut items = Vec::new();
                    for i in range {
                        let row = &rows_snapshot[i];
                        match row {
                            FlatRow::DirectoryHeader {
                                dir_id,
                                name,
                                match_count,
                                collapsed,
                                depth,
                            } => {
                                let dir_id = *dir_id;
                                let collapsed = *collapsed;
                                let depth = *depth;
                                let entity = cx_listener.clone();
                                let indent = depth as f32 * 16.0 + 12.0;
                                items.push(
                                    div()
                                        .id(ElementId::Name(
                                            format!("dir-{}", dir_id).into(),
                                        ))
                                        .w_full()
                                        .pl(px(indent))
                                        .pr(px(12.0))
                                        .py(px(5.0))
                                        .flex()
                                        .items_center()
                                        .gap_1()
                                        .cursor_pointer()
                                        .bg(SurchTheme::bg_surface())
                                        .hover(|s| s.bg(SurchTheme::bg_hover()))
                                        .on_click(move |_, _, cx| {
                                            entity.update(cx, |this, cx| {
                                                if dir_id < this.dir_id_to_path.len() {
                                                    let path = this.dir_id_to_path[dir_id].clone();
                                                    let entry = this.dir_collapsed.entry(path).or_insert(false);
                                                    *entry = !*entry;
                                                }
                                                this.rebuild_flat_rows();
                                                cx.notify();
                                            });
                                        })
                                        .child(
                                            Icon::new(if collapsed {
                                                IconName::ChevronRight
                                            } else {
                                                IconName::ChevronDown
                                            })
                                            .size_3()
                                            .text_color(SurchTheme::text_muted()),
                                        )
                                        .child(
                                            Icon::new(if collapsed {
                                                IconName::Folder
                                            } else {
                                                IconName::FolderOpen
                                            })
                                            .size_3()
                                            .text_color(SurchTheme::text_secondary()),
                                        )
                                        .child(
                                            div()
                                                .flex_1()
                                                .text_size(px(12.0))
                                                .font_weight(FontWeight::SEMIBOLD)
                                                .text_color(SurchTheme::text_heading())
                                                .overflow_hidden()
                                                .whitespace_nowrap()
                                                .child(name.clone()),
                                        )
                                        .child(
                                            div()
                                                .text_size(px(10.0))
                                                .text_color(SurchTheme::text_secondary())
                                                .px(px(6.0))
                                                .py(px(1.0))
                                                .rounded(px(8.0))
                                                .bg(SurchTheme::bg_hover())
                                                .child(format!("{}", match_count)),
                                        ),
                                );
                            }
                            FlatRow::FileHeader {
                                group_idx,
                                display_name,
                                match_count,
                                collapsed,
                                depth,
                                ..
                            } => {
                                let group_idx = *group_idx;
                                let collapsed = *collapsed;
                                let depth = *depth;
                                let entity = cx_listener.clone();
                                let indent = depth as f32 * 16.0 + 12.0;
                                items.push(
                                    div()
                                        .id(ElementId::Name(
                                            format!("file-group-{}", group_idx).into(),
                                        ))
                                        .w_full()
                                        .pl(px(indent))
                                        .pr(px(12.0))
                                        .py(px(5.0))
                                        .flex()
                                        .items_center()
                                        .gap_1()
                                        .cursor_pointer()
                                        .bg(SurchTheme::bg_surface())
                                        .hover(|s| s.bg(SurchTheme::bg_hover()))
                                        .on_click(move |_, _, cx| {
                                            entity.update(cx, |this, cx| {
                                                if let Some(group) =
                                                    this.file_groups.get_mut(group_idx)
                                                {
                                                    group.collapsed = !group.collapsed;
                                                }
                                                this.rebuild_flat_rows();
                                                cx.notify();
                                            });
                                        })
                                        .child(
                                            Icon::new(if collapsed {
                                                IconName::ChevronRight
                                            } else {
                                                IconName::ChevronDown
                                            })
                                            .size_3()
                                            .text_color(SurchTheme::text_muted()),
                                        )
                                        .child(
                                            div()
                                                .flex_1()
                                                .text_size(px(12.0))
                                                .font_weight(FontWeight::SEMIBOLD)
                                                .text_color(SurchTheme::text_heading())
                                                .overflow_hidden()
                                                .whitespace_nowrap()
                                                .child(display_name.clone()),
                                        )
                                        .child(
                                            div()
                                                .text_size(px(10.0))
                                                .text_color(SurchTheme::text_secondary())
                                                .px(px(6.0))
                                                .py(px(1.0))
                                                .rounded(px(8.0))
                                                .bg(SurchTheme::bg_hover())
                                                .child(format!("{}", match_count)),
                                        ),
                                );
                            }
                            FlatRow::MatchRow { item, .. } => {
                                let is_selected = selected == Some(item.id);
                                let item_clone = item.clone();
                                let line_num = item.line_number;
                                let content = item.line_content.clone();
                                let match_ranges = item.match_ranges.clone();
                                let id = item.id;
                                let entity = cx_listener.clone();
                                // Fixed indent for match rows — no tree depth indentation
                                // to maximize horizontal space for code content
                                let indent = 28.0_f32;

                                let mut row = div()
                                    .id(ElementId::Name(format!("result-{}", id).into()))
                                    .w_full()
                                    .pl(px(indent))
                                    .pr(px(12.0))
                                    .py(px(4.0))
                                    .flex()
                                    .items_center()
                                    .gap(px(8.0))
                                    .cursor_pointer()
                                    .hover(|s| s.bg(SurchTheme::bg_hover()))
                                    .on_click(move |_, window, cx| {
                                        entity.update(cx, |this, cx| {
                                            this.selected_result = Some(id);
                                            if let Some(ref handler) = this.on_result_selected {
                                                handler(&item_clone, window, cx);
                                            }
                                            cx.notify();
                                        });
                                    })
                                    .child(
                                        div()
                                            .text_size(px(11.0))
                                            .font_family("Menlo")
                                            .text_color(SurchTheme::text_secondary())
                                            .min_w(px(36.0))
                                            .flex()
                                            .justify_end()
                                            .child(format!("{}", line_num)),
                                    )
                                    .child(SearchPanel::render_highlighted_line(
                                        &content,
                                        &match_ranges,
                                        replace_text.as_deref(),
                                    ));

                                if is_selected {
                                    row = row
                                        .bg(SurchTheme::bg_selected())
                                        .border_l_2()
                                        .border_color(SurchTheme::accent());
                                }

                                items.push(row);
                            }
                        }
                    }
                    items
                }
            })
            .flex_1()
            .track_scroll(self.results_scroll_handle.clone());

        // Wrap the results list with a sticky header overlay.
        // on_scroll_wheel triggers cx.notify() so the sticky header recomputes on scroll.
        let entity_for_scroll = cx.entity().clone();
        let mut results_container = div()
            .id("results-container")
            .flex_1()
            .flex()
            .flex_col()
            .relative()
            .overflow_hidden()
            .child(results_list)
            .on_scroll_wheel(move |_, _window, cx| {
                let entity = entity_for_scroll.clone();
                cx.defer(move |cx| {
                    entity.update(cx, |_, cx| cx.notify());
                });
            });

        if let Some(entries) = sticky_headers {
            // Build a clipping container for the sticky stack so push-up animations
            // don't leak outside the results area.
            let total_height = entries.len() as f32 * 28.0;
            let mut sticky_clip = div()
                .absolute()
                .top_0()
                .left_0()
                .w_full()
                .h(px(total_height))
                .overflow_hidden();

            for (i, entry) in entries.iter().enumerate() {
                let y_offset = i as f32 * 28.0 - entry.push_up;
                let indent = entry.depth as f32 * 16.0 + 12.0;
                let row = div()
                    .absolute()
                    .top(px(y_offset))
                    .left_0()
                    .w_full()
                    .h(px(28.0))
                    .pl(px(indent))
                    .pr(px(12.0))
                    .py(px(5.0))
                    .flex()
                    .items_center()
                    .gap_1()
                    .bg(SurchTheme::bg_surface())
                    .border_b_1()
                    .border_color(SurchTheme::bg_hover())
                    .child(
                        Icon::new(if entry.is_dir {
                            IconName::FolderOpen
                        } else {
                            IconName::File
                        })
                        .size_3()
                        .text_color(SurchTheme::text_secondary()),
                    )
                    .child(
                        div()
                            .flex_1()
                            .text_size(px(12.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(SurchTheme::text_heading())
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .child(entry.name.clone()),
                    )
                    .child(
                        div()
                            .text_size(px(10.0))
                            .text_color(SurchTheme::text_secondary())
                            .px(px(6.0))
                            .py(px(1.0))
                            .rounded(px(8.0))
                            .bg(SurchTheme::bg_hover())
                            .child(format!("{}", entry.match_count)),
                    );
                sticky_clip = sticky_clip.child(row);
            }
            results_container = results_container.child(sticky_clip);
        }

        panel = panel.child(results_container);

        panel
    }
}

