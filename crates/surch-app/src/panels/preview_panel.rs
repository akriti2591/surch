use crate::theme::SurchTheme;
use gpui::*;
use gpui_component::input::{InputEvent, InputState, Position};
use gpui_component::{Icon, IconName};
use std::path::PathBuf;
use surch_core::channel::ChannelAction;

const DEFAULT_FONT_SIZE: f32 = 14.0;
const MIN_FONT_SIZE: f32 = 8.0;
const MAX_FONT_SIZE: f32 = 32.0;
const FONT_SIZE_STEP: f32 = 2.0;

pub struct PreviewPanel {
    workspace_root: Option<PathBuf>,
    file_path: Option<PathBuf>,
    /// The code editor state (tree-sitter highlighting, selection, copy, search).
    editor_state: Entity<InputState>,
    focus_line: Option<usize>,
    actions: Vec<ChannelAction>,
    show_actions_menu: bool,
    font_size: f32,
    word_wrap: bool,
    line_numbers: bool,
    indent_guides: bool,
    go_to_line_active: bool,
    go_to_line_input: Option<Entity<InputState>>,
    pub on_action_selected: Option<Box<dyn Fn(&str, &mut Window, &mut Context<Self>)>>,
}

/// Map a file extension to a tree-sitter language name supported by gpui-component.
fn language_for_path(path: &std::path::Path) -> &'static str {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");
    let filename = path
        .file_name()
        .and_then(|f| f.to_str())
        .unwrap_or("");

    // Check special filenames first
    match filename {
        "Makefile" | "makefile" | "GNUmakefile" => return "make",
        "CMakeLists.txt" => return "cmake",
        "Dockerfile" => return "bash",
        _ => {}
    }

    match ext {
        // Rust
        "rs" => "rust",
        // JavaScript / TypeScript
        "js" | "mjs" | "cjs" | "jsx" => "javascript",
        "ts" | "mts" | "cts" => "typescript",
        "tsx" => "tsx",
        // Web
        "html" | "htm" | "xhtml" => "html",
        "css" | "scss" | "less" => "css",
        // Systems
        "c" | "h" => "c",
        "cpp" | "cc" | "cxx" | "hpp" | "hxx" | "hh" => "cpp",
        "go" => "go",
        "zig" => "zig",
        "swift" => "swift",
        // Scripting
        "py" | "pyw" | "pyi" => "python",
        "rb" | "rake" | "gemspec" => "ruby",
        "sh" | "bash" | "zsh" | "fish" => "bash",
        "ex" | "exs" => "elixir",
        // JVM
        "java" => "java",
        "scala" | "sc" => "scala",
        "cs" => "csharp",
        // Data / Config
        "json" | "jsonc" | "geojson" => "json",
        "yaml" | "yml" => "yaml",
        "toml" => "toml",
        "sql" => "sql",
        "graphql" | "gql" => "graphql",
        "proto" => "proto",
        // Markup
        "md" | "markdown" => "markdown",
        "erb" => "erb",
        "ejs" => "ejs",
        // Build
        "cmake" => "cmake",
        "mk" => "make",
        "diff" | "patch" => "diff",
        // Default: plain text
        _ => "text",
    }
}

impl PreviewPanel {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let editor_state = cx.new(|cx| {
            InputState::new(window, cx)
                .code_editor("text")
                .searchable(true)
                .soft_wrap(false)
                .line_number(true)
        });

        Self {
            workspace_root: None,
            file_path: None,
            editor_state,
            focus_line: None,
            actions: Vec::new(),
            show_actions_menu: false,
            font_size: DEFAULT_FONT_SIZE,
            word_wrap: false,
            line_numbers: true,
            indent_guides: false,
            go_to_line_active: false,
            go_to_line_input: None,
            on_action_selected: None,
        }
    }

    pub fn load_file(
        &mut self,
        path: PathBuf,
        focus_line: usize,
        _pattern: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match std::fs::read_to_string(&path) {
            Ok(content) => {
                // Detect language from file extension
                let lang = language_for_path(&path);

                // Update the editor state
                self.editor_state.update(cx, |state, cx| {
                    state.set_highlighter(lang, cx);
                    state.set_value(content, window, cx);

                    // Scroll to focus line (0-based for Position)
                    if focus_line > 0 {
                        let target_line = focus_line.saturating_sub(1) as u32;
                        state.set_cursor_position(
                            Position::new(target_line, 0),
                            window,
                            cx,
                        );
                    }
                });

                self.file_path = Some(path);
                self.focus_line = Some(focus_line);
                self.show_actions_menu = false;
            }
            Err(_) => {
                self.editor_state.update(cx, |state, cx| {
                    state.set_highlighter("text", cx);
                    state.set_value("Error: Could not read file", window, cx);
                });
                self.file_path = Some(path);
                self.focus_line = None;
            }
        }
    }

    pub fn load_empty(&mut self) {
        self.file_path = None;
        self.focus_line = None;
        self.actions.clear();
        self.show_actions_menu = false;
        // Editor content stays stale but is hidden — render_empty() is shown
        // when file_path is None, so the editor div is never rendered.
    }

    /// Returns true if any input (go-to-line) currently has focus.
    pub fn any_input_focused(&self, window: &Window, cx: &App) -> bool {
        let go_to_line_focused = self.go_to_line_input.as_ref().map_or(false, |input| {
            input.read(cx).focus_handle(cx).is_focused(window)
        });
        let editor_focused = self.editor_state.read(cx).focus_handle(cx).is_focused(window);
        go_to_line_focused || editor_focused
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

    pub fn toggle_word_wrap(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.word_wrap = !self.word_wrap;
        self.editor_state.update(cx, |state, cx| {
            state.set_soft_wrap(self.word_wrap, window, cx);
        });
    }

    pub fn toggle_line_numbers(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.line_numbers = !self.line_numbers;
        self.editor_state.update(cx, |state, cx| {
            state.set_line_number(self.line_numbers, window, cx);
        });
    }

    pub fn toggle_indent_guides(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.indent_guides = !self.indent_guides;
        self.editor_state.update(cx, |state, cx| {
            state.set_indent_guides(self.indent_guides, window, cx);
        });
    }

    pub fn show_go_to_line(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.file_path.is_none() {
            return;
        }

        let input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Go to line...")
        });

        cx.subscribe_in(&input, window, {
            move |this: &mut PreviewPanel, _state, event: &InputEvent, window, cx| {
                if matches!(event, InputEvent::PressEnter { .. }) {
                    this.execute_go_to_line(window, cx);
                }
            }
        })
        .detach();

        input.update(cx, |state, cx| {
            state.focus(window, cx);
        });

        self.go_to_line_input = Some(input);
        self.go_to_line_active = true;
        cx.notify();
    }

    fn execute_go_to_line(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(ref input) = self.go_to_line_input {
            let value = input.read(cx).value().to_string();
            if let Ok(line_num) = value.trim().parse::<usize>() {
                let line = line_num.max(1);
                self.focus_line = Some(line);
                // Scroll editor to the target line (0-based)
                let target = (line.saturating_sub(1)) as u32;
                self.editor_state.update(cx, |state, cx| {
                    state.set_cursor_position(
                        Position::new(target, 0),
                        window,
                        cx,
                    );
                });
            }
        }
        self.go_to_line_active = false;
        self.go_to_line_input = None;
        cx.notify();
    }

    // Find in preview is now handled by the built-in CodeEditor search (Cmd+F).
    // This stub exists so app.rs handlers don't break during transition.
    pub fn show_find(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        // The CodeEditor's built-in search is triggered by Cmd+F when it has focus.
        // No custom find bar needed.
    }

    fn render_go_to_line_overlay(&self) -> Div {
        let input = self.go_to_line_input.as_ref().unwrap();
        div()
            .absolute()
            .top(px(40.0))
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

        // Breadcrumb path
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

        // Render the code editor in read-only mode. appearance(false) skips the
        // default border/bg so we control styling from the parent. size_full()
        // ensures the Input fills the flex container so the InputElement's
        // relative(1.) height resolves correctly.
        //
        // Input internally sets `.line_height(Rems(1.25))` and
        // `.input_text_size(size)` on its div, but then applies our Styled
        // overrides via `.refine_style()` AFTER those defaults. So we can
        // set `.text_size()` and `.line_height()` directly on the Input to
        // control font size and line spacing for zoom.
        let line_h = self.font_size * 1.5;
        let editor = gpui_component::input::Input::new(&self.editor_state)
            .disabled(true)
            .appearance(false)
            .size_full()
            .font_family("Menlo")
            .text_size(px(self.font_size))
            .line_height(px(line_h));

        panel = panel.child(
            div()
                .flex_1()
                .flex()
                .flex_col()
                .overflow_hidden()
                .child(editor),
        );

        panel
    }
}
