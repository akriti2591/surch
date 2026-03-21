use crate::theme::SurchTheme;
use gpui::*;
use gpui::ScrollStrategy;
use gpui_component::input::{InputEvent, InputState};
use gpui_component::spinner::Spinner;
use gpui_component::{Icon, IconName, Sizable};
use std::collections::HashMap;
use std::ops::Range;
use std::path::PathBuf;
use surch_core::channel::InputFieldSpec;

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

/// A flattened row for the virtualized list — either a file header or a match.
#[derive(Clone)]
enum FlatRow {
    FileHeader {
        group_idx: usize,
        relative_path: String,
        match_count: usize,
        collapsed: bool,
    },
    MatchRow {
        item: SearchResultItem,
    },
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
    all_collapsed: bool,
    search_completed: bool,
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
            all_collapsed: false,
            search_completed: false,
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

    pub fn search_options(&self) -> (bool, bool, bool) {
        (self.case_sensitive, self.whole_word, self.is_regex)
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
        self.on_input_changed("find", window, cx);
        cx.notify();
    }

    /// Restore search options from persisted workspace state.
    pub fn restore_options(&mut self, case_sensitive: bool, whole_word: bool, is_regex: bool) {
        self.case_sensitive = case_sensitive;
        self.whole_word = whole_word;
        self.is_regex = is_regex;
    }

    /// Returns true if any input field currently has focus.
    pub fn any_input_focused(&self, window: &Window, cx: &App) -> bool {
        self.inputs.values().any(|input| {
            input.read(cx).focus_handle(cx).is_focused(window)
        })
    }

    /// Select the next match row in the results list.
    pub fn select_next(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.flat_rows.is_empty() {
            return;
        }

        // Find the current selected index in flat_rows
        let current_idx = self.selected_result.and_then(|selected_id| {
            self.flat_rows.iter().position(|row| matches!(row, FlatRow::MatchRow { item } if item.id == selected_id))
        });

        // Find the next MatchRow after current
        let start = current_idx.map(|i| i + 1).unwrap_or(0);
        for i in start..self.flat_rows.len() {
            if let FlatRow::MatchRow { item } = &self.flat_rows[i] {
                self.selected_result = Some(item.id);
                // Scroll to make the selected item visible
                self.results_scroll_handle.scroll_to_item(i, ScrollStrategy::Center);
                // Fire the selection callback
                if let Some(ref handler) = self.on_result_selected {
                    handler(item, window, cx);
                }
                cx.notify();
                return;
            }
        }
    }

    /// Select the previous match row in the results list.
    pub fn select_previous(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.flat_rows.is_empty() {
            return;
        }

        let current_idx = self.selected_result.and_then(|selected_id| {
            self.flat_rows.iter().position(|row| matches!(row, FlatRow::MatchRow { item } if item.id == selected_id))
        });

        let end = match current_idx {
            Some(0) | None => return,
            Some(i) => i,
        };

        // Find the previous MatchRow before current
        for i in (0..end).rev() {
            if let FlatRow::MatchRow { item } = &self.flat_rows[i] {
                self.selected_result = Some(item.id);
                self.results_scroll_handle.scroll_to_item(i, ScrollStrategy::Center);
                if let Some(ref handler) = self.on_result_selected {
                    handler(item, window, cx);
                }
                cx.notify();
                return;
            }
        }
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
        for (group_idx, group) in self.file_groups.iter().enumerate() {
            self.flat_rows.push(FlatRow::FileHeader {
                group_idx,
                relative_path: group.relative_path.clone(),
                match_count: group.matches.len(),
                collapsed: group.collapsed,
            });
            if !group.collapsed {
                for item in &group.matches {
                    self.flat_rows.push(FlatRow::MatchRow {
                        item: item.clone(),
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
        self.all_collapsed = true;
        self.rebuild_flat_rows();
    }

    fn expand_all(&mut self) {
        for group in &mut self.file_groups {
            group.collapsed = false;
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

        panel = panel.child(
            uniform_list("search-results", row_count, {
                let cx_listener = cx.entity().clone();
                move |range, _window, _cx| {
                    let mut items = Vec::new();
                    for i in range {
                        let row = &rows_snapshot[i];
                        match row {
                            FlatRow::FileHeader {
                                group_idx,
                                relative_path,
                                match_count,
                                collapsed,
                            } => {
                                let group_idx = *group_idx;
                                let collapsed = *collapsed;
                                let entity = cx_listener.clone();
                                items.push(
                                    div()
                                        .id(ElementId::Name(
                                            format!("file-group-{}", group_idx).into(),
                                        ))
                                        .w_full()
                                        .px(px(12.0))
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
                                                .child(relative_path.clone()),
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

                                let mut row = div()
                                    .id(ElementId::Name(format!("result-{}", id).into()))
                                    .w_full()
                                    .pl(px(28.0))
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
            .track_scroll(self.results_scroll_handle.clone()),
        );

        panel
    }
}
