use crate::theme::SurchTheme;
use gpui::*;
use gpui_component::{Icon, IconName};
use std::path::PathBuf;
use std::rc::Rc;
use surch_core::channel::ChannelAction;
use syntect::easy::HighlightLines;
use syntect::highlighting::ThemeSet;
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

pub struct PreviewPanel {
    file_path: Option<PathBuf>,
    file_content: Vec<String>,
    highlighted_lines: Rc<Vec<Vec<(Hsla, String)>>>,
    focus_line: Option<usize>,
    match_pattern: Option<String>,
    actions: Vec<ChannelAction>,
    show_actions_menu: bool,
    scroll_handle: UniformListScrollHandle,
    syntax_set: SyntaxSet,
    theme_set: ThemeSet,
    pub on_action_selected: Option<Box<dyn Fn(&str, &mut Window, &mut Context<Self>)>>,
}

impl PreviewPanel {
    pub fn new() -> Self {
        Self {
            file_path: None,
            file_content: Vec::new(),
            highlighted_lines: Rc::new(Vec::new()),
            focus_line: None,
            match_pattern: None,
            actions: Vec::new(),
            show_actions_menu: false,
            scroll_handle: UniformListScrollHandle::default(),
            syntax_set: SyntaxSet::load_defaults_newlines(),
            theme_set: ThemeSet::load_defaults(),
            on_action_selected: None,
        }
    }

    pub fn load_file(&mut self, path: PathBuf, focus_line: usize, pattern: Option<String>) {
        match std::fs::read_to_string(&path) {
            Ok(content) => {
                // Replace tabs with 4 spaces for consistent indentation rendering.
                // GPUI renders tabs at default 8-space width; there's no tab-size property.
                let raw_lines: Vec<String> = content.lines().map(|l| l.replace('\t', "    ")).collect();

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
                let theme = &self.theme_set.themes["base16-ocean.dark"];
                let mut h = HighlightLines::new(syntax, theme);

                let mut highlighted = Vec::with_capacity(raw_lines.len());
                for line in &raw_lines {
                    // highlight_line expects lines ending with \n when using
                    // load_defaults_newlines(). Without \n, syntect's parser
                    // state drifts and highlighting breaks after ~100 lines.
                    let line_with_newline = format!("{}\n", line);
                    let ranges = h
                        .highlight_line(&line_with_newline, &self.syntax_set)
                        .unwrap_or_default();
                    let spans: Vec<(Hsla, String)> = ranges
                        .into_iter()
                        .map(|(style, text)| {
                            // Strip the trailing \n we added for display
                            let display_text = text.trim_end_matches('\n').to_string();
                            (syntect_color_to_hsla(style.foreground), display_text)
                        })
                        .filter(|(_, text)| !text.is_empty())
                        .collect();
                    highlighted.push(spans);
                }

                self.file_content = raw_lines;
                self.highlighted_lines = Rc::new(highlighted);
                self.file_path = Some(path);
                self.focus_line = Some(focus_line);
                self.match_pattern = pattern;
                self.show_actions_menu = false;
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

    pub fn set_actions(&mut self, actions: Vec<ChannelAction>) {
        self.actions = actions;
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
        let path_display = self
            .file_path
            .as_ref()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default();

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

        uniform_list("code-lines", line_count, move |range, _window, _cx| {
            let mut items = Vec::new();
            for i in range {
                let line_num = i + 1;
                let is_focus = line_num == focus;

                let line_content = if let Some(spans) = highlighted.get(i) {
                    let mut span_container =
                        div().flex_1().flex().flex_row().whitespace_nowrap();
                    for (color, text) in spans {
                        span_container = span_container
                            .child(div().text_color(*color).child(text.clone()));
                    }
                    span_container
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
                    .child(
                        div()
                            .min_w(px(52.0))
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
        .font_family("SF Mono")
        .text_size(px(13.0))
        .track_scroll(self.scroll_handle.clone())
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
