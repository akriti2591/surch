use crate::theme::SurchTheme;
use gpui::*;
use gpui_component::input::{InputEvent, InputState};
use gpui_component::{Icon, IconName};
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

        uniform_list("code-lines", line_count, move |range, _window, _cx| {
            let mut items = Vec::new();
            for i in range {
                let line_num = i + 1;
                let is_focus = line_num == focus;

                let line_content = if let Some(spans) = highlighted.get(i) {
                    let mut span_container =
                        div().flex_1().flex().flex_row().whitespace_nowrap();
                    for (color, text) in spans {
                        if !text.is_empty() {
                            span_container = span_container
                                .child(div().text_color(*color).child(text.clone()));
                        }
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
                    .py(px(1.0))
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
        .font_family("Menlo")
        .text_size(px(font_size))
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

        if self.go_to_line_active && self.go_to_line_input.is_some() {
            panel = panel.child(self.render_go_to_line_overlay());
        }

        panel = panel.child(self.render_code_lines(cx));

        panel
    }
}
