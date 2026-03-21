mod app;
mod assets;
mod components;
mod panels;
mod sidebar;
mod theme;

use app::SurchApp;
use assets::SurchAssets;
use gpui::*;
use gpui_component::Root;

fn main() {
    Application::new().with_assets(SurchAssets).run(|cx| {
        gpui_component::init(cx);

        let window_options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                None,
                size(px(1200.0), px(800.0)),
                cx,
            ))),
            titlebar: Some(TitlebarOptions {
                title: Some("surch".into()),
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
