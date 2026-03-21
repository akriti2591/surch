use crate::theme::SurchTheme;
use gpui::*;
use gpui_component::input::{InputEvent, InputState};
use gpui_component::{Icon, IconName, Sizable};
use std::ops::Range;
use std::path::PathBuf;
use std::rc::Rc;
use surch_core::channel::ChannelAction;
use syntect::easy::HighlightLines;
use syntect::highlighting::{Theme, ThemeSet};
use syntect::parsing::SyntaxSet;

fn syntect_color_to_hsla(color: syntect::highlighting::Color) -> gpui::Hsla {
    gpui::rgba(
        ((color.r as u32) << 24)
            | ((color.g as u32) << 16)
            | ((color.b as u32) << 8)
            | (color.a as u32),
    )
    .into()
}

const DEFAULT_FONT_SIZE: f32 = 14.0;
const MIN_FONT_SIZE: f32 = 8.0;
const MAX_FONT_SIZE: f32 = 32.0;
const FONT_SIZE_STEP: f32 = 2.0;

/// A single find match: which line it's on and the byte ranges within that line.
#[derive(Clone, Debug)]
struct FindMatch {
    line_index: usize,
    ranges: Vec<Range<usize>>,
}

pub struct PreviewPanel {
    workspace_root: Option<PathBuf>,
    file_path: Option<PathBuf>,
    file_content: Vec<String>,
    highlighted_lines: Rc<Vec<Vec<(Hsla, String)>>>,
    focus_line: Option<usize>,
    match_pattern: Option<String>,
    actions: Vec<ChannelAction>,
    show_actions_menu: bool,
    scroll_handle: UniformListScrollHandle,
    syntax_set: SyntaxSet,
    theme: Theme,
    font_size: f32,
    go_to_line_active: bool,
    go_to_line_input: Option<Entity<InputState>>,
    // Find in preview state
    pub(crate) find_active: bool,
    find_query: String,
    find_input: Option<Entity<InputState>>,
    find_matches: Vec<FindMatch>,
    current_match_index: Option<usize>,
    find_case_sensitive: bool,
    pub on_action_selected: Option<Box<dyn Fn(&str, &mut Window, &mut Context<Self>)>>,
}

impl PreviewPanel {
    pub fn new() -> Self {
        // Load custom One Dark theme from embedded asset
        let one_dark_theme = Self::load_one_dark_theme();

        Self {
            workspace_root: None,
            file_path: None,
            file_content: Vec::new(),
            highlighted_lines: Rc::new(Vec::new()),
            focus_line: None,
            match_pattern: None,
            actions: Vec::new(),
            show_actions_menu: false,
            scroll_handle: UniformListScrollHandle::default(),
            syntax_set: SyntaxSet::load_defaults_newlines(),
            theme: one_dark_theme,
            font_size: DEFAULT_FONT_SIZE,
            go_to_line_active: false,
            go_to_line_input: None,
            find_active: false,
            find_query: String::new(),
            find_input: None,
            find_matches: Vec::new(),
            current_match_index: None,
            find_case_sensitive: false,
            on_action_selected: None,
        }
    }

    fn load_one_dark_theme() -> Theme {
        let theme_bytes = include_bytes!("../../assets/themes/one-dark.tmTheme");
        let cursor = std::io::Cursor::new(&theme_bytes[..]);
        ThemeSet::load_from_reader(&mut std::io::BufReader::new(cursor))
            .unwrap_or_else(|_| {
                // Fallback to base16-eighties.dark if custom theme fails
                let ts = ThemeSet::load_defaults();
                ts.themes["base16-eighties.dark"].clone()
            })
    }

    pub fn load_file(&mut self, path: PathBuf, focus_line: usize, pattern: Option<String>) {
        match std::fs::read_to_string(&path) {
            Ok(content) => {
                let raw_lines: Vec<String> = content.lines().map(|l| l.to_string()).collect();

                // Determine syntax — try filename, extension, then first line
                let syntax = self
                    .syntax_set
                    .find_syntax_for_file(&path)
                    .ok()
                    .flatten()
                    .or_else(|| {
                        raw_lines
                            .first()
                            .and_then(|line| self.syntax_set.find_syntax_by_first_line(line))
                    })
                    .unwrap_or_else(|| self.syntax_set.find_syntax_plain_text());
                let mut h = HighlightLines::new(syntax, &self.theme);

                let mut highlighted = Vec::with_capacity(raw_lines.len());
                for line in &raw_lines {
                    // Strip \r for Windows line endings that sneak through
                    let clean_line = line.trim_end_matches('\r');
                    // highlight_line expects lines ending with \n when using
                    // load_defaults_newlines(). Without \n, syntect's parser
                    // state drifts and highlighting breaks after ~100 lines.
                    let line_with_newline = format!("{}\n", clean_line);
                    match h.highlight_line(&line_with_newline, &self.syntax_set) {
                        Ok(ranges) => {
                            // Keep ALL spans including empty ones — filtering them
                            // causes syntect's parse state to desync. Skip empties
                            // only at render time (render_code_lines already does this).
                            let spans: Vec<(Hsla, String)> = ranges
                                .into_iter()
                                .map(|(style, text)| {
                                    // Strip the trailing \n we added for display
                                    let display_text =
                                        text.trim_end_matches('\n').to_string();
                                    (syntect_color_to_hsla(style.foreground), display_text)
                                })
                                .collect();
                            highlighted.push(spans);
                        }
                        Err(_) => {
                            // On error, push the raw line as plain text but DON'T
                            // reset the highlighter — let it try to recover on next line
                            highlighted.push(vec![(
                                SurchTheme::text_primary(),
                                clean_line.to_string(),
                            )]);
                        }
                    }
                }

                self.file_content = raw_lines;
                self.highlighted_lines = Rc::new(highlighted);
                self.file_path = Some(path);
                self.focus_line = Some(focus_line);
                self.match_pattern = pattern;
                self.show_actions_menu = false;

                // Re-run find if active so matches update for the new file
                if self.find_active && !self.find_query.is_empty() {
                    self.execute_find();
                }

                // Scroll to the focus line (with some context lines above)
                if focus_line > 0 {
                    let scroll_to = focus_line.saturating_sub(6); // 5 lines of context above
                    self.scroll_handle.scroll_to_item(scroll_to, ScrollStrategy::Top);
                }
            }
            Err(_) => {
                self.file_content = vec!["Error: Could not read file".to_string()];
                self.highlighted_lines = Rc::new(vec![vec![(SurchTheme::text_primary(), "Error: Could not read file".to_string())]]);
                self.file_path = Some(path);
                self.focus_line = None;
                self.match_pattern = None;
            }
        }
    }

    pub fn load_empty(&mut self) {
        self.file_path = None;
        self.file_content.clear();
        self.highlighted_lines = Rc::new(Vec::new());
        self.focus_line = None;
        self.match_pattern = None;
        self.actions.clear();
        self.show_actions_menu = false;
        // Clear find state when file is unloaded
        self.find_matches.clear();
        self.current_match_index = None;
    }

    /// Returns true if any input (go-to-line or find) currently has focus.
    pub fn any_input_focused(&self, window: &Window, cx: &App) -> bool {
        let go_to_line_focused = self.go_to_line_input.as_ref().map_or(false, |input| {
            input.read(cx).focus_handle(cx).is_focused(window)
        });
        let find_focused = self.find_input.as_ref().map_or(false, |input| {
            input.read(cx).focus_handle(cx).is_focused(window)
        });
        go_to_line_focused || find_focused
    }

    pub fn set_workspace_root(&mut self, root: PathBuf) {
        self.workspace_root = Some(root);
    }

    pub fn set_actions(&mut self, actions: Vec<ChannelAction>) {
        self.actions = actions;
    }

    pub fn zoom_in(&mut self) {
        self.font_size = (self.font_size + FONT_SIZE_STEP).min(MAX_FONT_SIZE);
    }

    pub fn zoom_out(&mut self) {
        self.font_size = (self.font_size - FONT_SIZE_STEP).max(MIN_FONT_SIZE);
    }

    pub fn zoom_reset(&mut self) {
        self.font_size = DEFAULT_FONT_SIZE;
    }

    pub fn show_go_to_line(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.file_path.is_none() {
            return;
        }
        let total_lines = self.file_content.len();
        let input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(format!("Go to line (1-{})", total_lines))
        });

        // Subscribe to input events to handle Enter key
        cx.subscribe_in(&input, window, {
            move |this: &mut PreviewPanel, _state, event: &InputEvent, _window, cx| {
                if matches!(event, InputEvent::PressEnter { .. }) {
                    this.execute_go_to_line(cx);
                }
            }
        })
        .detach();

        // Focus the input
        input.update(cx, |state, cx| {
            state.focus(window, cx);
        });

        self.go_to_line_input = Some(input);
        self.go_to_line_active = true;
        cx.notify();
    }

    #[allow(dead_code)]
    pub fn dismiss_go_to_line(&mut self) {
        self.go_to_line_active = false;
        self.go_to_line_input = None;
    }

    fn execute_go_to_line(&mut self, cx: &mut Context<Self>) {
        if let Some(ref input) = self.go_to_line_input {
            let value = input.read(cx).value().to_string();
            if let Ok(line_num) = value.trim().parse::<usize>() {
                let line = line_num.max(1).min(self.file_content.len());
                self.focus_line = Some(line);
                let scroll_to = line.saturating_sub(6);
                self.scroll_handle
                    .scroll_to_item(scroll_to, ScrollStrategy::Top);
            }
        }
        self.go_to_line_active = false;
        self.go_to_line_input = None;
        cx.notify();
    }

    // ==================== Find in Preview ====================

    pub fn show_find(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.file_path.is_none() {
            return;
        }

        // If find bar is already active, just re-focus the input
        if self.find_active {
            if let Some(ref input) = self.find_input {
                input.update(cx, |state, cx| {
                    state.focus(window, cx);
                });
            }
            return;
        }

        let input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Find in file...")
        });

        // Subscribe to input events for incremental search + Enter navigation
        cx.subscribe_in(&input, window, {
            move |this: &mut PreviewPanel, _state, event: &InputEvent, _window, cx| {
                match event {
                    InputEvent::Change => {
                        this.on_find_input_changed(cx);
                    }
                    InputEvent::PressEnter { .. } => {
                        this.find_next(cx);
                    }
                    _ => {}
                }
            }
        })
        .detach();

        // Focus the input
        input.update(cx, |state, cx| {
            state.focus(window, cx);
        });

        self.find_input = Some(input);
        self.find_active = true;
        cx.notify();
    }

    pub fn hide_find(&mut self) {
        self.find_active = false;
        self.find_input = None;
        self.find_matches.clear();
        self.current_match_index = None;
        self.find_query.clear();
    }

    fn on_find_input_changed(&mut self, cx: &mut Context<Self>) {
        let query = if let Some(ref input) = self.find_input {
            input.read(cx).value().to_string()
        } else {
            return;
        };
        self.find_query = query;
        self.execute_find();
        // Auto-jump to first match
        if !self.find_matches.is_empty() {
            self.current_match_index = Some(0);
            self.scroll_to_current_match();
        } else {
            self.current_match_index = None;
        }
        cx.notify();
    }

    fn execute_find(&mut self) {
        self.find_matches.clear();
        self.current_match_index = None;

        if self.find_query.is_empty() {
            return;
        }

        let query = if self.find_case_sensitive {
            self.find_query.clone()
        } else {
            self.find_query.to_lowercase()
        };

        for (line_idx, line) in self.file_content.iter().enumerate() {
            let search_line = if self.find_case_sensitive {
                line.clone()
            } else {
                line.to_lowercase()
            };

            let mut ranges = Vec::new();
            let mut start = 0;
            while let Some(pos) = search_line[start..].find(&query) {
                let abs_start = start + pos;
                let abs_end = abs_start + self.find_query.len();
                ranges.push(abs_start..abs_end);
                start = abs_end;
            }

            if !ranges.is_empty() {
                self.find_matches.push(FindMatch {
                    line_index: line_idx,
                    ranges,
                });
            }
        }
    }

    /// Total number of individual matches across all lines.
    fn total_match_count(&self) -> usize {
        self.find_matches.iter().map(|m| m.ranges.len()).sum()
    }

    /// Get the flat index of the current match (1-based for display).
    fn current_match_display(&self) -> Option<usize> {
        self.current_match_index.map(|i| i + 1)
    }

    pub fn find_next(&mut self, cx: &mut Context<Self>) {
        let total = self.total_match_count();
        if total == 0 {
            return;
        }
        let next = match self.current_match_index {
            Some(i) => (i + 1) % total,
            None => 0,
        };
        self.current_match_index = Some(next);
        self.scroll_to_current_match();
        cx.notify();
    }

    pub fn find_previous(&mut self, cx: &mut Context<Self>) {
        let total = self.total_match_count();
        if total == 0 {
            return;
        }
        let prev = match self.current_match_index {
            Some(0) => total - 1,
            Some(i) => i - 1,
            None => total - 1,
        };
        self.current_match_index = Some(prev);
        self.scroll_to_current_match();
        cx.notify();
    }

    fn scroll_to_current_match(&self) {
        if let Some(idx) = self.current_match_index {
            // Find which line the current match is on
            let mut count = 0;
            for m in &self.find_matches {
                if count + m.ranges.len() > idx {
                    let scroll_to = m.line_index.saturating_sub(4);
                    self.scroll_handle.scroll_to_item(scroll_to, ScrollStrategy::Top);
                    return;
                }
                count += m.ranges.len();
            }
        }
    }

    /// For a given flat match index, return (line_index, range_index_within_line).
    fn flat_match_to_line_range(&self, flat_idx: usize) -> Option<(usize, usize)> {
        let mut count = 0;
        for m in &self.find_matches {
            if count + m.ranges.len() > flat_idx {
                return Some((m.line_index, flat_idx - count));
            }
            count += m.ranges.len();
        }
        None
    }

    fn render_go_to_line_overlay(&self) -> Div {
        let input = self.go_to_line_input.as_ref().unwrap();
        div()
            .absolute()
            .top(px(40.0)) // Below the header
            .right(px(16.0))
            .w(px(250.0))
            .bg(SurchTheme::bg_surface())
            .border_1()
            .border_color(SurchTheme::border())
            .rounded(px(6.0))
            .shadow_lg()
            .p(px(8.0))
            .child(gpui_component::input::Input::new(input).w_full())
    }

    fn render_find_bar(&self, cx: &mut Context<Self>) -> Div {
        let input = match self.find_input.as_ref() {
            Some(i) => i,
            None => return div(),
        };

        let total = self.total_match_count();
        let match_text = if self.find_query.is_empty() {
            String::new()
        } else if total == 0 {
            "No results".to_string()
        } else if let Some(current) = self.current_match_display() {
            format!("{} of {}", current, total)
        } else {
            format!("{} found", total)
        };

        let case_active = self.find_case_sensitive;

        div()
            .w_full()
            .px(px(12.0))
            .py(px(6.0))
            .flex()
            .items_center()
            .gap(px(6.0))
            .bg(SurchTheme::bg_surface())
            .border_b_1()
            .border_color(SurchTheme::border())
            .flex_shrink_0()
            // Input field
            .child(
                div()
                    .flex_1()
                    .child(gpui_component::input::Input::new(input).w_full()),
            )
            // Case sensitive toggle
            .child({
                let mut btn = div()
                    .id("find-toggle-case")
                    .w(px(22.0))
                    .h(px(22.0))
                    .rounded(px(3.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor_pointer()
                    .text_size(px(11.0))
                    .font_family("Menlo")
                    .child("Aa");
                if case_active {
                    btn = btn
                        .bg(SurchTheme::toggle_active_bg())
                        .text_color(SurchTheme::text_primary());
                } else {
                    btn = btn.text_color(SurchTheme::text_secondary());
                }
                btn.on_click(cx.listener(|this, _, _, cx| {
                        this.find_case_sensitive = !this.find_case_sensitive;
                        this.execute_find();
                        if !this.find_matches.is_empty() {
                            this.current_match_index = Some(0);
                            this.scroll_to_current_match();
                        } else {
                            this.current_match_index = None;
                        }
                        cx.notify();
                    }))
            })
            // Previous match button
            .child(
                div()
                    .id("find-prev")
                    .w(px(22.0))
                    .h(px(22.0))
                    .rounded(px(3.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor_pointer()
                    .hover(|s| s.bg(SurchTheme::bg_hover()))
                    .child(
                        Icon::new(IconName::ChevronUp)
                            .with_size(gpui_component::Size::Small)
                            .text_color(SurchTheme::text_heading()),
                    )
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.find_previous(cx);
                    })),
            )
            // Next match button
            .child(
                div()
                    .id("find-next")
                    .w(px(22.0))
                    .h(px(22.0))
                    .rounded(px(3.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor_pointer()
                    .hover(|s| s.bg(SurchTheme::bg_hover()))
                    .child(
                        Icon::new(IconName::ChevronDown)
                            .with_size(gpui_component::Size::Small)
                            .text_color(SurchTheme::text_heading()),
                    )
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.find_next(cx);
                    })),
            )
            // Match count
            .child(
                div()
                    .text_size(px(11.0))
                    .text_color(SurchTheme::text_secondary())
                    .min_w(px(60.0))
                    .child(match_text),
            )
            // Close button
            .child(
                div()
                    .id("find-close")
                    .w(px(22.0))
                    .h(px(22.0))
                    .rounded(px(3.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor_pointer()
                    .hover(|s| s.bg(SurchTheme::bg_hover()))
                    .child(
                        Icon::new(IconName::Close)
                            .with_size(gpui_component::Size::Small)
                            .text_color(SurchTheme::text_heading()),
                    )
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.hide_find();
                        cx.notify();
                    })),
            )
    }

    fn render_empty(&self) -> Div {
        div()
            .flex_1()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .child(
                div()
                    .mb(px(8.0))
                    .child(
                        Icon::new(IconName::Search)
                            .size(px(32.0))
                            .text_color(SurchTheme::text_muted()),
                    ),
            )
            .child(
                div()
                    .text_size(px(13.0))
                    .text_color(SurchTheme::text_muted())
                    .child("Select a search result to preview"),
            )
    }

    fn render_header(&self, cx: &mut Context<Self>) -> Div {
        let mut header = div()
            .w_full()
            .px(px(16.0))
            .py(px(8.0))
            .flex()
            .items_center()
            .border_b_1()
            .border_color(SurchTheme::border())
            .bg(SurchTheme::bg_secondary())
            .flex_shrink_0();

        // Breadcrumb path — show relative segments with chevron separators
        if let Some(ref file_path) = self.file_path {
            let relative = if let Some(ref root) = self.workspace_root {
                file_path
                    .strip_prefix(root)
                    .unwrap_or(file_path)
            } else {
                file_path.as_path()
            };

            let segments: Vec<String> = relative
                .components()
                .map(|c| c.as_os_str().to_string_lossy().to_string())
                .collect();

            let mut breadcrumb = div()
                .flex_1()
                .flex()
                .items_center()
                .overflow_hidden()
                .whitespace_nowrap()
                .gap(px(2.0));

            for (i, segment) in segments.iter().enumerate() {
                let is_last = i == segments.len() - 1;

                if i > 0 {
                    // Chevron separator
                    breadcrumb = breadcrumb.child(
                        Icon::new(IconName::ChevronRight)
                            .size(px(12.0))
                            .text_color(SurchTheme::text_muted()),
                    );
                }

                let text_color = if is_last {
                    SurchTheme::text_heading()
                } else {
                    SurchTheme::text_secondary()
                };

                let mut seg_div = div()
                    .text_size(px(12.0))
                    .text_color(text_color)
                    .child(segment.clone());

                if is_last {
                    seg_div = seg_div.font_weight(FontWeight::MEDIUM);
                }

                breadcrumb = breadcrumb.child(seg_div);
            }

            header = header.child(breadcrumb);
        }

        // "Open in" button
        if !self.actions.is_empty() {
            header = header.child(
                div()
                    .id("open-in-button")
                    .px(px(10.0))
                    .py(px(4.0))
                    .rounded(px(4.0))
                    .cursor_pointer()
                    .bg(SurchTheme::accent())
                    .hover(|s| s.bg(SurchTheme::accent_hover()))
                    .text_size(px(11.0))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(SurchTheme::text_heading())
                    .flex()
                    .items_center()
                    .gap(px(4.0))
                    .child(
                        Icon::new(IconName::ExternalLink)
                            .size_3()
                            .text_color(SurchTheme::text_heading()),
                    )
                    .child("Open in...")
                    .on_click(cx.listener(|this, _, _window, cx| {
                        this.show_actions_menu = !this.show_actions_menu;
                        cx.notify();
                    })),
            );
        }

        header
    }

    fn render_actions_menu(&self, cx: &mut Context<Self>) -> Div {
        let mut menu = div()
            .absolute()
            .top(px(28.0))
            .right(px(8.0))
            .min_w(px(200.0))
            .bg(SurchTheme::bg_surface())
            .border_1()
            .border_color(SurchTheme::border())
            .rounded(px(8.0))
            .shadow_lg()
            .py(px(4.0));

        for action in &self.actions {
            let action_id = action.id.clone();
            let label = action.label.clone();

            menu = menu.child(
                div()
                    .id(ElementId::Name(format!("action-{}", action_id).into()))
                    .w_full()
                    .px(px(12.0))
                    .py(px(6.0))
                    .mx(px(4.0))
                    .rounded(px(4.0))
                    .cursor_pointer()
                    .text_size(px(12.0))
                    .text_color(SurchTheme::text_primary())
                    .hover(|s| s.bg(SurchTheme::bg_hover()))
                    .child(label)
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.show_actions_menu = false;
                        if let Some(ref handler) = this.on_action_selected {
                            handler(&action_id, window, cx);
                        }
                        cx.notify();
                    })),
            );
        }

        menu
    }

    fn render_code_lines(&self, _cx: &mut Context<Self>) -> impl IntoElement {
        let focus = self.focus_line.unwrap_or(0);
        let line_count = self.file_content.len();
        let highlighted = self.highlighted_lines.clone();
        let font_size = self.font_size;
        // Build a map of line_index -> (ranges, current_match_range_idx or None)
        // for the find-in-preview highlighting
        let find_highlights: Rc<std::collections::HashMap<usize, (Vec<Range<usize>>, Option<usize>)>> = {
            let mut map = std::collections::HashMap::new();
            if self.find_active && !self.find_matches.is_empty() {
                // Figure out which specific range is the "current" match
                let current_line_and_range = self.current_match_index
                    .and_then(|idx| self.flat_match_to_line_range(idx));

                for m in &self.find_matches {
                    let current_range_in_line = current_line_and_range
                        .and_then(|(line_idx, range_idx)| {
                            if line_idx == m.line_index {
                                Some(range_idx)
                            } else {
                                None
                            }
                        });
                    map.insert(m.line_index, (m.ranges.clone(), current_range_in_line));
                }
            }
            Rc::new(map)
        };

        uniform_list("code-lines", line_count, move |range, _window, _cx| {
            let mut items = Vec::new();
            for i in range {
                let line_num = i + 1;
                let is_focus = line_num == focus;

                let line_content = if let Some(spans) = highlighted.get(i) {
                    // Check if this line has find matches
                    if let Some((find_ranges, current_range_idx)) = find_highlights.get(&i) {
                        // Render with find highlights overlaid on syntax spans
                        render_line_with_find_highlights(spans, find_ranges, *current_range_idx)
                    } else {
                        let mut span_container =
                            div().flex_1().flex().flex_row().whitespace_nowrap();
                        for (color, text) in spans {
                            if !text.is_empty() {
                                span_container = span_container
                                    .child(div().text_color(*color).child(text.clone()));
                            }
                        }
                        span_container
                    }
                } else {
                    div()
                        .flex_1()
                        .flex()
                        .flex_row()
                        .whitespace_nowrap()
                        .text_color(SurchTheme::text_primary())
                        .child("")
                };

                let mut line_div = div()
                    .w_full()
                    .flex()
                    .px_1()
                    .py(px(1.0))
                    .child(
                        div()
                            .min_w(px(52.0))
                            .flex_shrink_0()
                            .text_color(SurchTheme::text_muted())
                            .text_size(px(11.0))
                            .pr(px(12.0))
                            .flex()
                            .justify_end()
                            .child(format!("{}", line_num)),
                    )
                    .child(line_content);

                if is_focus {
                    line_div = line_div
                        .bg(SurchTheme::bg_focus_line())
                        .border_l_2()
                        .border_color(hsla(0.15, 0.60, 0.50, 0.80));
                }

                items.push(line_div);
            }
            items
        })
        .flex_1()
        .font_family("Menlo")
        .text_size(px(font_size))
        .track_scroll(self.scroll_handle.clone())
    }
}

/// Render a code line's syntax-highlighted spans with find-match highlights overlaid.
/// `find_ranges` are byte ranges in the original line text.
/// `current_range_idx` is which range (if any) in this line is the "current" match.
fn render_line_with_find_highlights(
    spans: &[(Hsla, String)],
    find_ranges: &[Range<usize>],
    current_range_idx: Option<usize>,
) -> Div {
    let mut container = div().flex_1().flex().flex_row().whitespace_nowrap();

    // Build the full line text from spans to get correct byte offsets
    let mut span_offsets: Vec<(usize, usize, Hsla, &str)> = Vec::new();
    let mut offset = 0;
    for (color, text) in spans {
        let start = offset;
        let end = offset + text.len();
        span_offsets.push((start, end, *color, text.as_str()));
        offset = end;
    }

    // For each span, split it at find match boundaries and render segments
    let mut char_pos = 0;
    for (span_start, span_end, color, text) in &span_offsets {
        if text.is_empty() {
            continue;
        }
        let span_start = *span_start;
        let span_end = *span_end;
        let color = *color;

        // Collect the find-range intersections with this span
        let mut splits: Vec<(usize, usize, bool, bool)> = Vec::new(); // (start_in_span, end_in_span, is_match, is_current)
        let mut pos = span_start;

        for (range_idx, range) in find_ranges.iter().enumerate() {
            let r_start = range.start.max(span_start).min(span_end);
            let r_end = range.end.max(span_start).min(span_end);

            if r_start >= r_end {
                continue;
            }

            // Text before this match range
            if pos < r_start {
                splits.push((pos - span_start, r_start - span_start, false, false));
            }

            let is_current = current_range_idx == Some(range_idx);
            splits.push((r_start - span_start, r_end - span_start, true, is_current));
            pos = r_end;
        }

        // Remaining text after last match
        if pos < span_end {
            splits.push((pos - span_start, span_end - span_start, false, false));
        }

        if splits.is_empty() {
            // No find matches intersect this span — render normally
            container = container.child(div().text_color(color).child(text.to_string()));
        } else {
            for (s, e, is_match, is_current) in splits {
                let segment = &text[s..e];
                if segment.is_empty() {
                    continue;
                }
                if is_match {
                    let bg = if is_current {
                        // Current match: brighter orange highlight
                        hsla(0.10, 0.90, 0.45, 0.70)
                    } else {
                        // Other matches: standard yellow highlight
                        SurchTheme::match_bg()
                    };
                    container = container.child(
                        div()
                            .bg(bg)
                            .text_color(SurchTheme::text_match())
                            .rounded(px(2.0))
                            .child(segment.to_string()),
                    );
                } else {
                    container = container.child(
                        div().text_color(color).child(segment.to_string()),
                    );
                }
            }
        }

        char_pos += text.len();
    }

    let _ = char_pos; // suppress unused warning
    container
}

impl Render for PreviewPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut panel = div()
            .flex_1()
            .flex()
            .flex_col()
            .h_full()
            .bg(SurchTheme::bg_primary())
            .relative();

        if self.file_path.is_none() {
            return panel.child(self.render_empty());
        }

        panel = panel.child(self.render_header(cx));

        if self.show_actions_menu {
            panel = panel.child(self.render_actions_menu(cx));
        }

        if self.go_to_line_active && self.go_to_line_input.is_some() {
            panel = panel.child(self.render_go_to_line_overlay());
        }

        if self.find_active && self.find_input.is_some() {
            panel = panel.child(self.render_find_bar(cx));
        }

        panel = panel.child(self.render_code_lines(cx));

        panel
    }
}
