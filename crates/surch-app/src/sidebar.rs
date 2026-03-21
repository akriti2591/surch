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

    fn icon_for_channel(channel: &ChannelMetadata) -> &'static str {
        match channel.id.as_str() {
            "file_search" => "\u{1F50D}", // magnifying glass emoji
            _ => "\u{2726}",              // four-pointed star fallback
        }
    }
}

impl Render for Sidebar {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut sidebar = div()
            .flex()
            .flex_col()
            .items_center()
            .w(px(44.0))
            .h_full()
            .bg(SurchTheme::bg_sidebar())
            .border_r_1()
            .border_color(SurchTheme::border())
            .pt_2()
            .gap_1();

        for (i, channel) in self.channels.iter().enumerate() {
            let is_active = i == self.active_index;
            let icon_str = Self::icon_for_channel(channel);

            let mut icon = div()
                .id(ElementId::Name(format!("sidebar-icon-{}", i).into()))
                .flex()
                .items_center()
                .justify_center()
                .w(px(32.0))
                .h(px(32.0))
                .rounded(px(6.0))
                .cursor_pointer()
                .child(
                    div()
                        .text_size(px(16.0))
                        .child(icon_str),
                )
                .on_click(cx.listener(move |this, _, _window, cx| {
                    this.active_index = i;
                    cx.notify();
                }));

            if is_active {
                icon = icon
                    .bg(SurchTheme::bg_hover())
                    .border_l_2()
                    .border_color(SurchTheme::accent());
            }

            sidebar = sidebar.child(icon);
        }

        sidebar
    }
}
