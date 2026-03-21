use crate::theme::SurchTheme;
use gpui::*;
use surch_core::channel::ChannelMetadata;

pub struct Sidebar {
    channels: Vec<ChannelMetadata>,
    active_index: usize,
}

impl Sidebar {
    pub fn new(channels: Vec<ChannelMetadata>) -> Self {
        Self {
            channels,
            active_index: 0,
        }
    }

    pub fn set_active(&mut self, index: usize) {
        self.active_index = index;
    }
}

impl Render for Sidebar {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut sidebar = div()
            .flex()
            .flex_col()
            .items_center()
            .w(px(48.0))
            .h_full()
            .bg(SurchTheme::bg_sidebar())
            .border_r_1()
            .border_color(SurchTheme::border())
            .pt_2()
            .gap_1();

        for (i, channel) in self.channels.iter().enumerate() {
            let is_active = i == self.active_index;
            let label = channel.name.chars().next().unwrap_or('?').to_string();

            let icon = div()
                .id(ElementId::Name(format!("sidebar-icon-{}", i).into()))
                .flex()
                .items_center()
                .justify_center()
                .w(px(36.0))
                .h(px(36.0))
                .rounded(px(6.0))
                .cursor_pointer()
                .child(
                    div()
                        .text_size(px(18.0))
                        .text_color(if is_active {
                            SurchTheme::text_primary()
                        } else {
                            SurchTheme::text_secondary()
                        })
                        .child(label),
                )
                .on_click(cx.listener(move |this, _, _window, cx| {
                    this.active_index = i;
                    cx.notify();
                }));

            let icon = if is_active {
                icon.bg(SurchTheme::bg_selected())
            } else {
                icon
            };

            sidebar = sidebar.child(icon);
        }

        sidebar
    }
}
