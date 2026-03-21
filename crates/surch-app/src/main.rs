mod app;
mod assets;
mod components;
mod panels;
mod sidebar;
mod theme;

use app::{
    ClearSearch, CloseProject, Copy, Cut, FindInPreview, FocusFind, OpenFolder, OpenInEditor,
    Paste, Quit, SelectAll, SelectNextResult, SelectPreviousResult, SurchApp,
    GoToLine, ToggleCaseSensitive, ToggleRegex, ToggleWholeWord, ZoomIn, ZoomOut,
    ZoomReset,
};
use assets::SurchAssets;
use gpui::*;
use gpui_component::Root;

fn main() {
    Application::new().with_assets(SurchAssets).run(|cx| {
        gpui_component::init(cx);

        // Register keyboard shortcuts
        cx.bind_keys([
            KeyBinding::new("cmd-o", OpenFolder, Some("surch")),
            KeyBinding::new("cmd-w", CloseProject, Some("surch")),
            KeyBinding::new("cmd-f", FocusFind, Some("surch")),
            KeyBinding::new("cmd-q", Quit, None),
            KeyBinding::new("alt-c", ToggleCaseSensitive, Some("surch")),
            KeyBinding::new("alt-w", ToggleWholeWord, Some("surch")),
            KeyBinding::new("alt-r", ToggleRegex, Some("surch")),
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
            KeyBinding::new("cmd-shift-f", FindInPreview, Some("surch")),
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
                    MenuItem::action("Find", FocusFind),
                    MenuItem::action("Find in Preview", FindInPreview),
                    MenuItem::action("Clear Search", ClearSearch),
                    MenuItem::separator(),
                    MenuItem::action("Next Result", SelectNextResult),
                    MenuItem::action("Previous Result", SelectPreviousResult),
                    MenuItem::action("Open in Editor", OpenInEditor),
                    MenuItem::separator(),
                    MenuItem::action("Toggle Case Sensitive", ToggleCaseSensitive),
                    MenuItem::action("Toggle Whole Word", ToggleWholeWord),
                    MenuItem::action("Toggle Regex", ToggleRegex),
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
                ..Default::default()
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
