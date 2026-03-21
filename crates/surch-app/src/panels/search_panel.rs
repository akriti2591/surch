use crate::theme::SurchTheme;
use gpui::*;
use gpui_component::input::{InputEvent, InputState};
use gpui_component::scroll::ScrollableElement;
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

pub struct SearchPanel {
    input_fields: Vec<InputFieldSpec>,
    inputs: HashMap<String, Entity<InputState>>,
    file_groups: Vec<FileGroup>,
    selected_result: Option<u64>,
    total_matches: usize,
    total_files: usize,
    is_searching: bool,
    workspace_root: Option<PathBuf>,
    scroll_handle: ScrollHandle,
    case_sensitive: bool,
    whole_word: bool,
    is_regex: bool,
    pub on_query_changed:
        Option<Box<dyn Fn(HashMap<String, String>, &mut Window, &mut Context<Self>)>>,
    pub on_result_selected:
        Option<Box<dyn Fn(&SearchResultItem, &mut Window, &mut Context<Self>)>>,
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
            selected_result: None,
            total_matches: 0,
            total_files: 0,
            is_searching: false,
            workspace_root: None,
            scroll_handle: ScrollHandle::new(),
            case_sensitive: false,
            whole_word: false,
            is_regex: false,
            on_query_changed: None,
            on_result_selected: None,
        }
    }

    pub fn set_workspace_root(&mut self, root: PathBuf) {
        self.workspace_root = Some(root);
    }

    pub fn search_options(&self) -> (bool, bool, bool) {
        (self.case_sensitive, self.whole_word, self.is_regex)
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

    pub fn clear_results(&mut self) {
        self.file_groups.clear();
        self.selected_result = None;
        self.total_matches = 0;
        self.total_files = 0;
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
    }

    pub fn set_complete(&mut self, total_files: usize, total_matches: usize) {
        self.total_files = total_files;
        self.total_matches = total_matches;
        self.is_searching = false;
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
            .font_family("SF Mono")
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

    fn render_highlighted_line(content: &str, ranges: &[Range<usize>]) -> Div {
        let mut container = div()
            .flex_1()
            .flex()
            .overflow_hidden()
            .whitespace_nowrap()
            .text_size(px(12.0))
            .font_family("SF Mono");

        if ranges.is_empty() {
            return container
                .text_color(SurchTheme::text_primary())
                .child(content.to_string());
        }

        let mut last_end = 0;
        for range in ranges {
            let start = range.start.min(content.len());
            let end = range.end.min(content.len());
            if start > last_end {
                container = container.child(
                    div()
                        .text_color(SurchTheme::text_primary())
                        .child(content[last_end..start].to_string()),
                );
            }
            if end > start {
                container = container.child(
                    div()
                        .bg(SurchTheme::match_bg())
                        .text_color(SurchTheme::text_match())
                        .rounded(px(2.0))
                        .px(px(1.0))
                        .child(content[start..end].to_string()),
                );
            }
            last_end = end;
        }
        if last_end < content.len() {
            container = container.child(
                div()
                    .text_color(SurchTheme::text_primary())
                    .child(content[last_end..].to_string()),
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
        } else {
            container =
                container.child(gpui_component::input::Input::new(input).w_full());
        }

        container
    }

    fn render_status(&self) -> impl IntoElement {
        let status_text = if self.is_searching {
            "Searching...".to_string()
        } else if self.total_matches > 0 {
            format!(
                "{} results in {} files",
                self.total_matches,
                self.file_groups.len()
            )
        } else {
            String::new()
        };

        div()
            .px(px(12.0))
            .py(px(6.0))
            .text_size(px(11.0))
            .text_color(SurchTheme::text_secondary())
            .child(status_text)
    }

    fn render_file_group(
        &self,
        group_idx: usize,
        group: &FileGroup,
        cx: &mut Context<Self>,
    ) -> Div {
        let match_count = group.matches.len();
        let collapsed = group.collapsed;
        let relative_path = group.relative_path.clone();

        let mut container = div().w_full();

        // File header
        container = container.child(
            div()
                .id(ElementId::Name(format!("file-group-{}", group_idx).into()))
                .w_full()
                .px(px(12.0))
                .py(px(5.0))
                .flex()
                .items_center()
                .gap_1()
                .cursor_pointer()
                .bg(SurchTheme::bg_surface())
                .hover(|s| s.bg(SurchTheme::bg_hover()))
                .on_click(cx.listener(move |this, _, _window, cx| {
                    if let Some(group) = this.file_groups.get_mut(group_idx) {
                        group.collapsed = !group.collapsed;
                    }
                    cx.notify();
                }))
                .child(
                    div()
                        .text_size(px(8.0))
                        .text_color(SurchTheme::text_muted())
                        .child(if collapsed { "\u{25B6}" } else { "\u{25BC}" }),
                )
                .child(
                    div()
                        .flex_1()
                        .text_size(px(12.0))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(SurchTheme::text_heading())
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .child(relative_path),
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

        // Match lines (if not collapsed)
        if !collapsed {
            for match_item in &group.matches {
                let is_selected = self.selected_result == Some(match_item.id);
                let item_clone = match_item.clone();
                let line_num = match_item.line_number;
                let content = match_item.line_content.clone();
                let match_ranges = match_item.match_ranges.clone();
                let id = match_item.id;

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
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.selected_result = Some(id);
                        if let Some(ref handler) = this.on_result_selected {
                            handler(&item_clone, window, cx);
                        }
                        cx.notify();
                    }))
                    .child(
                        div()
                            .text_size(px(11.0))
                            .font_family("SF Mono")
                            .text_color(SurchTheme::text_secondary())
                            .min_w(px(36.0))
                            .flex()
                            .justify_end()
                            .child(format!("{}", line_num)),
                    )
                    .child(Self::render_highlighted_line(&content, &match_ranges));

                if is_selected {
                    row = row
                        .bg(SurchTheme::bg_selected())
                        .border_l_2()
                        .border_color(SurchTheme::accent());
                }

                container = container.child(row);
            }
        }

        container
    }
}

impl Render for SearchPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut panel = div()
            .flex()
            .flex_col()
            .w(px(340.0))
            .flex_shrink_0()
            .h_full()
            .overflow_hidden()
            .bg(SurchTheme::bg_secondary())
            .border_r_1()
            .border_color(SurchTheme::border());

        // Header
        panel = panel.child(
            div()
                .px(px(12.0))
                .py(px(8.0))
                .text_size(px(11.0))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(SurchTheme::text_secondary())
                .border_b_1()
                .border_color(SurchTheme::border())
                .flex_shrink_0()
                .child("SEARCH"),
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

        // Status bar
        panel = panel.child(
            div()
                .flex_shrink_0()
                .child(self.render_status()),
        );

        // Results list — render directly without cloning
        let mut results_container = div()
            .flex_1()
            .overflow_y_scrollbar()
            .w_full();

        let num_groups = self.file_groups.len();
        for group_idx in 0..num_groups {
            let group = &self.file_groups[group_idx];
            results_container =
                results_container.child(self.render_file_group(group_idx, group, cx));
        }

        panel = panel.child(results_container);

        panel
    }
}
