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
            on_query_changed: None,
            on_result_selected: None,
        }
    }

    pub fn set_workspace_root(&mut self, root: PathBuf) {
        self.workspace_root = Some(root);
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

    fn render_input_field(
        &self,
        field: &InputFieldSpec,
        _cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let input = self.inputs.get(&field.id).unwrap();

        div()
            .w_full()
            .mb_1()
            .child(
                div()
                    .text_size(px(11.0))
                    .text_color(SurchTheme::text_secondary())
                    .mb(px(2.0))
                    .child(field.label.clone()),
            )
            .child(gpui_component::input::Input::new(input).w_full())
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
            .px_2()
            .py_1()
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
                .px_2()
                .py(px(3.0))
                .flex()
                .items_center()
                .gap_1()
                .cursor_pointer()
                .bg(SurchTheme::bg_secondary())
                .on_click(cx.listener(move |this, _, _window, cx| {
                    if let Some(group) = this.file_groups.get_mut(group_idx) {
                        group.collapsed = !group.collapsed;
                    }
                    cx.notify();
                }))
                .child(
                    div()
                        .text_size(px(10.0))
                        .text_color(SurchTheme::text_secondary())
                        .child(if collapsed { "▶" } else { "▼" }),
                )
                .child(
                    div()
                        .flex_1()
                        .text_size(px(12.0))
                        .text_color(SurchTheme::text_primary())
                        .overflow_hidden()
                        .child(relative_path),
                )
                .child(
                    div()
                        .text_size(px(10.0))
                        .text_color(SurchTheme::text_secondary())
                        .px_1()
                        .rounded(px(3.0))
                        .bg(SurchTheme::bg_primary())
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
                let id = match_item.id;

                let mut row = div()
                    .id(ElementId::Name(format!("result-{}", id).into()))
                    .w_full()
                    .pl(px(24.0))
                    .pr_2()
                    .py(px(2.0))
                    .flex()
                    .items_center()
                    .gap_1()
                    .cursor_pointer()
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
                            .text_color(SurchTheme::text_secondary())
                            .min_w(px(32.0))
                            .child(format!("{}", line_num)),
                    )
                    .child(
                        div()
                            .flex_1()
                            .text_size(px(12.0))
                            .text_color(SurchTheme::text_primary())
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .child(content),
                    );

                if is_selected {
                    row = row.bg(SurchTheme::bg_selected());
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
            .w(px(350.0))
            .h_full()
            .bg(SurchTheme::bg_secondary())
            .border_r_1()
            .border_color(SurchTheme::border());

        // Header
        panel = panel.child(
            div()
                .px_2()
                .py_1()
                .text_size(px(11.0))
                .text_color(SurchTheme::text_secondary())
                .border_b_1()
                .border_color(SurchTheme::border())
                .child("SEARCH"),
        );

        // Input fields
        let mut inputs_container = div().flex().flex_col().px_2().py_1().gap_1();
        for field in self.input_fields.clone() {
            inputs_container = inputs_container.child(self.render_input_field(&field, cx));
        }
        panel = panel.child(inputs_container);

        // Status bar
        panel = panel.child(self.render_status());

        // Results list
        let groups_snapshot: Vec<(usize, FileGroup)> = self
            .file_groups
            .iter()
            .enumerate()
            .map(|(i, g)| (i, g.clone()))
            .collect();

        let mut results_container = div()
            .flex_1()
            .overflow_y_scrollbar()
            .w_full();

        for (group_idx, group) in &groups_snapshot {
            results_container =
                results_container.child(self.render_file_group(*group_idx, group, cx));
        }

        panel = panel.child(results_container);

        panel
    }
}
