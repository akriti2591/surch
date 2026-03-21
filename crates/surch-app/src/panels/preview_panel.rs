use crate::theme::SurchTheme;
use gpui::*;
use gpui_component::scroll::ScrollableElement;
use std::path::PathBuf;
use surch_core::channel::ChannelAction;

pub struct PreviewPanel {
    file_path: Option<PathBuf>,
    file_content: Vec<String>,
    focus_line: Option<usize>,
    match_pattern: Option<String>,
    actions: Vec<ChannelAction>,
    show_actions_menu: bool,
    scroll_handle: ScrollHandle,
    pub on_action_selected: Option<Box<dyn Fn(&str, &mut Window, &mut Context<Self>)>>,
}

impl PreviewPanel {
    pub fn new() -> Self {
        Self {
            file_path: None,
            file_content: Vec::new(),
            focus_line: None,
            match_pattern: None,
            actions: Vec::new(),
            show_actions_menu: false,
            scroll_handle: ScrollHandle::new(),
            on_action_selected: None,
        }
    }

    pub fn load_file(&mut self, path: PathBuf, focus_line: usize, pattern: Option<String>) {
        match std::fs::read_to_string(&path) {
            Ok(content) => {
                self.file_content = content.lines().map(|l| l.to_string()).collect();
                self.file_path = Some(path);
                self.focus_line = Some(focus_line);
                self.match_pattern = pattern;
                self.show_actions_menu = false;
            }
            Err(_) => {
                self.file_content = vec!["Error: Could not read file".to_string()];
                self.file_path = Some(path);
                self.focus_line = None;
                self.match_pattern = None;
            }
        }
    }

    pub fn set_actions(&mut self, actions: Vec<ChannelAction>) {
        self.actions = actions;
    }

    fn render_empty(&self) -> Div {
        div()
            .flex_1()
            .flex()
            .items_center()
            .justify_center()
            .child(
                div()
                    .text_size(px(14.0))
                    .text_color(SurchTheme::text_secondary())
                    .child("Select a search result to preview"),
            )
    }

    fn render_header(&self, cx: &mut Context<Self>) -> Div {
        let path_display = self
            .file_path
            .as_ref()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default();

        let mut header = div()
            .w_full()
            .px_3()
            .py_1()
            .flex()
            .items_center()
            .border_b_1()
            .border_color(SurchTheme::border())
            .bg(SurchTheme::bg_secondary());

        // File path
        header = header.child(
            div()
                .flex_1()
                .text_size(px(12.0))
                .text_color(SurchTheme::text_primary())
                .overflow_hidden()
                .whitespace_nowrap()
                .child(path_display),
        );

        // "Open in" button
        if !self.actions.is_empty() {
            header = header.child(
                div()
                    .id("open-in-button")
                    .px_2()
                    .py(px(2.0))
                    .rounded(px(4.0))
                    .cursor_pointer()
                    .bg(SurchTheme::accent())
                    .text_size(px(11.0))
                    .text_color(SurchTheme::text_primary())
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
            .min_w(px(180.0))
            .bg(SurchTheme::bg_secondary())
            .border_1()
            .border_color(SurchTheme::border())
            .rounded(px(6.0))
            .shadow_lg()
            .py_1();

        for action in &self.actions {
            let action_id = action.id.clone();
            let label = action.label.clone();

            menu = menu.child(
                div()
                    .id(ElementId::Name(format!("action-{}", action_id).into()))
                    .w_full()
                    .px_3()
                    .py(px(4.0))
                    .cursor_pointer()
                    .text_size(px(12.0))
                    .text_color(SurchTheme::text_primary())
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

        let mut code_container = div()
            .flex_1()
            .overflow_y_scrollbar()
            .w_full()
            .font_family("Monaco")
            .text_size(px(12.0));

        for (i, line) in self.file_content.iter().enumerate() {
            let line_num = i + 1;
            let is_focus = line_num == focus;

            let mut line_div = div()
                .id(ElementId::Name(format!("line-{}", line_num).into()))
                .w_full()
                .flex()
                .px_1()
                // Line number gutter
                .child(
                    div()
                        .min_w(px(48.0))
                        .text_color(SurchTheme::text_secondary())
                        .text_size(px(11.0))
                        .pr_2()
                        .flex()
                        .justify_end()
                        .child(format!("{}", line_num)),
                )
                // Line content
                .child(
                    div()
                        .flex_1()
                        .text_color(SurchTheme::text_primary())
                        .whitespace_nowrap()
                        .child(line.clone()),
                );

            if is_focus {
                line_div = line_div.bg(hsla(0.15, 0.5, 0.2, 0.3));
            }

            code_container = code_container.child(line_div);
        }

        code_container
    }
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

        panel = panel.child(self.render_code_lines(cx));

        panel
    }
}
