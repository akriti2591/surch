use crate::theme::SurchTheme;
use gpui::*;
use gpui_component::{Icon, IconName};
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

    fn icon_for_channel(channel: &ChannelMetadata) -> IconName {
        match channel.id.as_str() {
            "file_search" => IconName::Search,
            _ => IconName::Asterisk,
        }
    }
}

impl Render for Sidebar {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut sidebar = div()
            .flex()
            .flex_col()
            .w(px(44.0))
            .flex_shrink_0()
            .h_full()
            .bg(SurchTheme::bg_sidebar())
            .border_r_1()
            .border_color(SurchTheme::border())
            .pt_2()
            .gap_1();

        for (i, channel) in self.channels.iter().enumerate() {
            let is_active = i == self.active_index;
            let icon_name = Self::icon_for_channel(channel);

            // Fixed-width row: 2px indicator bar + icon
            let indicator = div()
                .w(px(2.0))
                .h(px(20.0))
                .rounded(px(1.0))
                .bg(if is_active {
                    SurchTheme::accent()
                } else {
                    gpui::transparent_black()
                });

            let mut icon_box = div()
                .id(ElementId::Name(format!("sidebar-icon-{}", i).into()))
                .flex()
                .items_center()
                .justify_center()
                .w(px(32.0))
                .h(px(32.0))
                .rounded(px(6.0))
                .cursor_pointer()
                .child(
                    Icon::new(icon_name.clone())
                        .size_4()
                        .text_color(if is_active {
                            SurchTheme::text_heading()
                        } else {
                            SurchTheme::text_secondary()
                        }),
                )
                .on_click(cx.listener(move |this, _, _window, cx| {
                    this.active_index = i;
                    cx.notify();
                }));

            if is_active {
                icon_box = icon_box.bg(SurchTheme::bg_hover());
            }

            // Wrap in a row so indicator doesn't change icon width
            let row = div()
                .flex()
                .flex_row()
                .items_center()
                .w_full()
                .gap(px(2.0))
                .child(indicator)
                .child(icon_box);

            sidebar = sidebar.child(row);
        }

        sidebar
    }
}
