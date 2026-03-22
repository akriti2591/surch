mod app;
mod assets;
mod components;
mod panels;
mod sidebar;
mod theme;

use app::{
    ClearSearch, CloseProject, Copy, Cut, FindInPreview, FocusFind, GoToLine, OpenFolder,
    OpenInEditor, Paste, Quit, SelectAll, SelectNextResult, SelectPreviousResult, SurchApp,
    ToggleCaseSensitive, ToggleFuzzy, ToggleIndentGuides, ToggleLineNumbers, ToggleRegex,
    ToggleViewMode, ToggleWholeWord, ToggleWordWrap, ZoomIn, ZoomOut, ZoomReset,
};
use assets::SurchAssets;
use gpui::*;
use gpui_component::Root;

/// Set up a One Dark-inspired highlight theme for the code editor.
/// The default gpui-component theme has muted colors — many token types
/// (variable, punctuation, operator) are undefined and fall back to the
/// same gray foreground. This provides vibrant, distinct colors.
fn setup_highlight_theme(cx: &mut App) {
    use gpui_component::highlighter::{HighlightTheme, HighlightThemeStyle};
    use gpui_component::{Theme, ThemeMode};
    use std::sync::Arc;

    let style: HighlightThemeStyle = serde_json::from_value(serde_json::json!({
        "editor.foreground": "#ABB2BF",
        "editor.background": "#1a1d23",
        "editor.active_line.background": "#2c313a",
        "editor.line_number": "#636D83",
        "editor.active_line_number": "#ABB2BF",
        "syntax": {
            "attribute":              { "color": "#D19A66" },
            "boolean":                { "color": "#D19A66" },
            "comment":                { "color": "#7F848E", "font_style": "italic" },
            "comment_doc":            { "color": "#7F848E", "font_style": "italic" },
            "constant":               { "color": "#D19A66" },
            "constructor":            { "color": "#E5C07B" },
            "embedded":               { "color": "#ABB2BF" },
            "enum":                   { "color": "#E5C07B" },
            "function":               { "color": "#61AFEF" },
            "keyword":                { "color": "#C678DD" },
            "label":                  { "color": "#E5C07B" },
            "link_text":              { "color": "#61AFEF" },
            "link_uri":               { "color": "#98C379", "font_style": "italic" },
            "number":                 { "color": "#D19A66" },
            "operator":               { "color": "#56B6C2" },
            "preproc":                { "color": "#C678DD" },
            "property":               { "color": "#E06C75" },
            "punctuation":            { "color": "#ABB2BF" },
            "punctuation.bracket":    { "color": "#ABB2BF" },
            "punctuation.delimiter":  { "color": "#ABB2BF" },
            "punctuation.special":    { "color": "#56B6C2" },
            "string":                 { "color": "#98C379" },
            "string.escape":          { "color": "#56B6C2" },
            "string.regex":           { "color": "#56B6C2" },
            "string.special":         { "color": "#D19A66" },
            "string.special.symbol":  { "color": "#D19A66" },
            "tag":                    { "color": "#E06C75" },
            "tag.doctype":            { "color": "#C678DD" },
            "text.literal":           { "color": "#98C379" },
            "title":                  { "color": "#E5C07B", "font_weight": 700 },
            "type":                   { "color": "#E5C07B" },
            "variable":               { "color": "#E06C75" },
            "variable.special":       { "color": "#E06C75" },
            "variant":                { "color": "#E5C07B" }
        }
    }))
    .expect("one dark theme JSON");

    let highlight_theme = Arc::new(HighlightTheme {
        name: "One Dark".to_string(),
        appearance: ThemeMode::Dark,
        style,
    });

    Theme::global_mut(cx).highlight_theme = highlight_theme;
}

/// Fix TSX highlighting: gpui-component ships a 35-line TSX query that only
/// covers TypeScript-specific additions (types, interfaces). It's missing all
/// base JavaScript captures (strings, functions, keywords, comments, etc.).
/// We re-register TSX with the full TypeScript highlights query, which includes
/// both JS basics and TS additions and works with the TSX grammar.
fn fix_tsx_highlighting() {
    use gpui_component::highlighter::{LanguageConfig, LanguageRegistry};

    let registry = LanguageRegistry::singleton();
    let tsx_config = registry.language("tsx");
    let ts_config = registry.language("typescript");

    if let (Some(tsx), Some(ts)) = (tsx_config, ts_config) {
        let fixed = LanguageConfig::new(
            "tsx",
            tsx.language.clone(),
            vec![],
            &ts.highlights,
            &tsx.injections,
            &tsx.locals,
        );
        registry.register("tsx", &fixed);
    }
}

fn main() {
    Application::new().with_assets(SurchAssets).run(|cx| {
        gpui_component::init(cx);
        fix_tsx_highlighting();
        setup_highlight_theme(cx);

        // Register keyboard shortcuts
        cx.bind_keys([
            KeyBinding::new("cmd-o", OpenFolder, Some("surch")),
            KeyBinding::new("cmd-w", CloseProject, Some("surch")),
            KeyBinding::new("cmd-shift-f", FocusFind, Some("surch")),
            KeyBinding::new("cmd-q", Quit, None),
            KeyBinding::new("alt-c", ToggleCaseSensitive, Some("surch")),
            KeyBinding::new("alt-w", ToggleWholeWord, Some("surch")),
            KeyBinding::new("alt-r", ToggleRegex, Some("surch")),
            KeyBinding::new("alt-f", ToggleFuzzy, Some("surch")),
            KeyBinding::new("cmd-down", SelectNextResult, Some("surch")),
            KeyBinding::new("cmd-up", SelectPreviousResult, Some("surch")),
            KeyBinding::new("down", SelectNextResult, Some("surch")),
            KeyBinding::new("up", SelectPreviousResult, Some("surch")),
            KeyBinding::new("cmd-shift-enter", OpenInEditor, Some("surch")),
            KeyBinding::new("escape", ClearSearch, Some("surch")),
            KeyBinding::new("cmd-=", ZoomIn, Some("surch")),
            KeyBinding::new("cmd--", ZoomOut, Some("surch")),
            KeyBinding::new("cmd-0", ZoomReset, Some("surch")),
            KeyBinding::new("cmd-g", GoToLine, Some("surch")),
            KeyBinding::new("cmd-shift-t", ToggleViewMode, Some("surch")),
            KeyBinding::new("cmd-f", FindInPreview, Some("surch")),
            KeyBinding::new("alt-z", ToggleWordWrap, Some("surch")),
        ]);

        // Quit handler at app level
        cx.on_action(|_: &Quit, cx| {
            cx.quit();
        });

        // Set up native macOS menu bar
        cx.set_menus(vec![
            Menu {
                name: "Surch".into(),
                items: vec![
                    MenuItem::action("About Surch", Quit),
                    MenuItem::separator(),
                    MenuItem::action("Quit Surch", Quit),
                ],
            },
            Menu {
                name: "File".into(),
                items: vec![
                    MenuItem::action("Open Folder...", OpenFolder),
                    MenuItem::separator(),
                    MenuItem::action("Close Project", CloseProject),
                ],
            },
            Menu {
                name: "Edit".into(),
                items: vec![
                    MenuItem::os_action("Cut", Cut, OsAction::Cut),
                    MenuItem::os_action("Copy", Copy, OsAction::Copy),
                    MenuItem::os_action("Paste", Paste, OsAction::Paste),
                    MenuItem::os_action("Select All", SelectAll, OsAction::SelectAll),
                ],
            },
            Menu {
                name: "View".into(),
                items: vec![
                    MenuItem::action("Toggle Tree View", ToggleViewMode),
                    MenuItem::separator(),
                    MenuItem::action("Word Wrap", ToggleWordWrap),
                    MenuItem::action("Line Numbers", ToggleLineNumbers),
                    MenuItem::action("Indent Guides", ToggleIndentGuides),
                    MenuItem::separator(),
                    MenuItem::action("Zoom In", ZoomIn),
                    MenuItem::action("Zoom Out", ZoomOut),
                    MenuItem::action("Reset Zoom", ZoomReset),
                ],
            },
            Menu {
                name: "Go".into(),
                items: vec![MenuItem::action("Go to Line...", GoToLine)],
            },
            Menu {
                name: "Find".into(),
                items: vec![
                    MenuItem::action("Find in Preview", FindInPreview),
                    MenuItem::action("Find in Files", FocusFind),
                    MenuItem::action("Clear Search", ClearSearch),
                    MenuItem::separator(),
                    MenuItem::action("Next Result", SelectNextResult),
                    MenuItem::action("Previous Result", SelectPreviousResult),
                    MenuItem::action("Open in Editor", OpenInEditor),
                    MenuItem::separator(),
                    MenuItem::action("Toggle Case Sensitive", ToggleCaseSensitive),
                    MenuItem::action("Toggle Whole Word", ToggleWholeWord),
                    MenuItem::action("Toggle Regex", ToggleRegex),
                    MenuItem::action("Toggle Fuzzy", ToggleFuzzy),
                ],
            },
        ]);

        let window_options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                None,
                size(px(1200.0), px(800.0)),
                cx,
            ))),
            titlebar: Some(TitlebarOptions {
                title: Some("Surch".into()),
                appears_transparent: true,
                traffic_light_position: Some(point(px(9.0), px(9.0))),
            }),
            ..Default::default()
        };

        cx.open_window(window_options, |window, cx| {
            let app_view = cx.new(|cx| SurchApp::new(window, cx));
            cx.new(|cx| Root::new(app_view, window, cx))
        })
        .unwrap();
    });
}
