use crate::panels::preview_panel::PreviewPanel;
use crate::panels::search_panel::{SearchPanel, SearchResultItem};
use crate::sidebar::Sidebar;
use crate::theme::SurchTheme;
use crossbeam_channel::{Receiver, TryRecvError};
use gpui::*;
use gpui_component::{Icon, IconName};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use surch_core::channel::{ChannelQuery, SearchEvent};
use surch_core::registry::ChannelRegistry;
use surch_file_search::FileSearchChannel;

pub struct SurchApp {
    sidebar: Entity<Sidebar>,
    search_panel: Entity<SearchPanel>,
    preview_panel: Entity<PreviewPanel>,
    registry: ChannelRegistry,
    workspace_root: Option<PathBuf>,
    search_receiver: Option<Receiver<SearchEvent>>,
    pending_query: Option<HashMap<String, String>>,
    current_result: Option<SearchResultItem>,
}

impl SurchApp {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let file_search = Arc::new(FileSearchChannel::new());
        let mut registry = ChannelRegistry::new();
        registry.register(file_search.clone());

        let channel_metas: Vec<_> = registry
            .channels()
            .iter()
            .map(|c| c.metadata())
            .collect();

        let input_fields = registry
            .active()
            .map(|c| c.input_fields())
            .unwrap_or_default();

        let sidebar = cx.new(|_cx| Sidebar::new(channel_metas));
        let search_panel = cx.new(|cx| SearchPanel::new(input_fields, window, cx));
        let preview_panel = cx.new(|_cx| PreviewPanel::new());

        let mut app = Self {
            sidebar,
            search_panel,
            preview_panel,
            registry,
            workspace_root: None,
            search_receiver: None,
            pending_query: None,
            current_result: None,
        };

        app.setup_callbacks(window, cx);
        app
    }

    fn setup_callbacks(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let app_entity = cx.entity().clone();
        self.search_panel.update(cx, |panel, _cx| {
            panel.on_query_changed = Some(Box::new(move |values, _window, cx| {
                app_entity.update(cx, |app, cx| {
                    app.handle_query_changed(values, cx);
                });
            }));
        });

        let app_entity = cx.entity().clone();
        self.search_panel.update(cx, |panel, _cx| {
            panel.on_result_selected = Some(Box::new(move |result, _window, cx| {
                let result = result.clone();
                app_entity.update(cx, |app, cx| {
                    app.handle_result_selected(result, cx);
                });
            }));
        });

        let app_entity = cx.entity().clone();
        self.preview_panel.update(cx, |panel, _cx| {
            panel.on_action_selected = Some(Box::new(move |action_id, _window, cx| {
                let action_id = action_id.to_string();
                app_entity.update(cx, |app, cx| {
                    app.handle_action_selected(&action_id, cx);
                });
            }));
        });

        // Refresh search callback
        let app_entity = cx.entity().clone();
        self.search_panel.update(cx, |panel, _cx| {
            panel.on_refresh = Some(Box::new(move |_window, cx| {
                app_entity.update(cx, |app, cx| {
                    app.refresh_search(cx);
                });
            }));
        });

        // Close project callback
        let app_entity = cx.entity().clone();
        self.search_panel.update(cx, |panel, _cx| {
            panel.on_close_project = Some(Box::new(move |_window, cx| {
                app_entity.update(cx, |app, cx| {
                    app.close_project(cx);
                });
            }));
        });
    }

    fn handle_query_changed(&mut self, values: HashMap<String, String>, cx: &mut Context<Self>) {
        self.pending_query = Some(values);

        let entity = cx.entity().clone();
        cx.spawn(async move |_, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(150))
                .await;
            let _ = cx.update(|cx| {
                entity.update(cx, |app, cx| {
                    app.execute_search(cx);
                });
            });
        })
        .detach();
    }

    fn execute_search(&mut self, cx: &mut Context<Self>) {
        let values = match self.pending_query.take() {
            Some(v) => v,
            None => return,
        };

        if let Some(channel) = self.registry.active() {
            channel.cancel();
        }

        let workspace = match &self.workspace_root {
            Some(root) => root.clone(),
            None => return,
        };

        self.search_panel.update(cx, |panel, _cx| {
            panel.clear_results();
            panel.set_searching(true);
        });

        let (case_sensitive, whole_word, is_regex) =
            self.search_panel.read(cx).search_options();

        let query = ChannelQuery {
            fields: values,
            workspace_root: workspace,
            is_regex,
            case_sensitive,
            whole_word,
        };

        let (tx, rx) = crossbeam_channel::unbounded();
        self.search_receiver = Some(rx);

        if let Some(channel) = self.registry.active().cloned() {
            std::thread::spawn(move || {
                channel.search(query, tx);
            });
        }

        self.poll_search_results(cx);
    }

    fn poll_search_results(&mut self, cx: &mut Context<Self>) {
        let rx = match &self.search_receiver {
            Some(rx) => rx.clone(),
            None => return,
        };

        let mut batch_count = 0;
        loop {
            match rx.try_recv() {
                Ok(event) => match event {
                    SearchEvent::Match(entry) => {
                        let item = SearchResultItem {
                            id: entry.id,
                            file_path: entry.file_path.clone().unwrap_or_default(),
                            line_number: entry.line_number.unwrap_or(0),
                            line_content: entry.line_content.clone(),
                            match_ranges: entry.match_ranges.clone(),
                        };
                        self.search_panel.update(cx, |panel, _cx| {
                            panel.add_result(item);
                        });
                        batch_count += 1;
                    }
                    SearchEvent::Complete {
                        total_files,
                        total_matches,
                    } => {
                        self.search_panel.update(cx, |panel, _cx| {
                            panel.set_complete(total_files, total_matches);
                        });
                        self.search_receiver = None;
                        cx.notify();
                        return;
                    }
                    SearchEvent::Error(_) => {}
                    SearchEvent::Progress { .. } => {}
                },
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    self.search_receiver = None;
                    self.search_panel.update(cx, |panel, _cx| {
                        panel.set_searching(false);
                    });
                    cx.notify();
                    return;
                }
            }

            if batch_count >= 100 {
                break;
            }
        }

        cx.notify();

        if self.search_receiver.is_some() {
            let entity = cx.entity().clone();
            cx.spawn(async move |_, cx| {
                cx.background_executor()
                    .timer(Duration::from_millis(16))
                    .await;
                let _ = cx.update(|cx| {
                    entity.update(cx, |app, cx| {
                        app.poll_search_results(cx);
                    });
                });
            })
            .detach();
        }
    }

    fn handle_result_selected(&mut self, result: SearchResultItem, cx: &mut Context<Self>) {
        self.current_result = Some(result.clone());

        let actions = if let Some(channel) = self.registry.active() {
            let entry = surch_core::channel::ResultEntry {
                id: result.id,
                file_path: Some(result.file_path.clone()),
                line_number: Some(result.line_number),
                column: None,
                line_content: result.line_content.clone(),
                match_ranges: result.match_ranges.clone(),
            };
            channel.actions(&entry)
        } else {
            Vec::new()
        };

        self.preview_panel.update(cx, |panel, _cx| {
            panel.load_file(result.file_path.clone(), result.line_number, None);
            panel.set_actions(actions);
        });

        cx.notify();
    }

    fn refresh_search(&mut self, cx: &mut Context<Self>) {
        // Re-execute the search with current input values (skip debounce)
        let input_values = self.search_panel.update(cx, |panel, cx| {
            let mut vals = HashMap::new();
            for (id, input) in &panel.inputs {
                vals.insert(id.clone(), input.read(cx).value().to_string());
            }
            vals
        });
        self.pending_query = Some(input_values);
        self.execute_search(cx);
    }

    fn close_project(&mut self, cx: &mut Context<Self>) {
        // Defer the actual state change to avoid crashing GPUI
        // when the view tree changes during click event processing.
        let entity = cx.entity().clone();
        cx.spawn(async move |_, cx| {
            let _ = cx.update(|cx| {
                entity.update(cx, |app, cx| {
                    // Cancel any in-progress search
                    if let Some(channel) = app.registry.active() {
                        channel.cancel();
                    }
                    app.search_receiver = None;
                    app.workspace_root = None;
                    app.current_result = None;

                    app.search_panel.update(cx, |panel, _cx| {
                        panel.clear_results();
                        panel.set_searching(false);
                    });
                    app.preview_panel.update(cx, |panel, _cx| {
                        panel.load_empty();
                    });
                    cx.notify();
                });
            });
        })
        .detach();
    }

    fn handle_action_selected(&mut self, action_id: &str, _cx: &mut Context<Self>) {
        if let (Some(channel), Some(result)) = (self.registry.active(), &self.current_result) {
            let entry = surch_core::channel::ResultEntry {
                id: result.id,
                file_path: Some(result.file_path.clone()),
                line_number: Some(result.line_number),
                column: None,
                line_content: result.line_content.clone(),
                match_ranges: result.match_ranges.clone(),
            };
            if let Err(e) = channel.execute_action(action_id, &entry) {
                eprintln!("Action error: {}", e);
            }
        }
    }

    pub fn open_folder(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let entity = cx.entity().clone();
        cx.spawn(async move |_, cx| {
            let output = std::process::Command::new("osascript")
                .args([
                    "-e",
                    "POSIX path of (choose folder with prompt \"Select a folder to search\")",
                ])
                .output();

            if let Ok(output) = output {
                if output.status.success() {
                    let path_str = String::from_utf8_lossy(&output.stdout)
                        .trim()
                        .to_string();
                    if !path_str.is_empty() {
                        let path = PathBuf::from(&path_str);
                        let _ = cx.update(|cx| {
                            entity.update(cx, |app, cx| {
                                app.workspace_root = Some(path.clone());
                                app.search_panel.update(cx, |panel, _cx| {
                                    panel.set_workspace_root(path);
                                });
                                cx.notify();
                            });
                        });
                    }
                }
            }
        })
        .detach();
    }
}

impl Render for SurchApp {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.workspace_root.is_none() {
            return div()
                .size_full()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .bg(SurchTheme::bg_primary())
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .items_center()
                        .child(
                            div()
                                .mb(px(4.0))
                                .child(
                                    Icon::new(IconName::Search)
                                        .size(px(48.0))
                                        .text_color(SurchTheme::text_muted()),
                                ),
                        )
                        .child(
                            div()
                                .text_size(px(28.0))
                                .font_weight(FontWeight::BOLD)
                                .text_color(SurchTheme::text_heading())
                                .mb(px(8.0))
                                .child("surch"),
                        )
                        .child(
                            div()
                                .text_size(px(13.0))
                                .text_color(SurchTheme::text_secondary())
                                .mb(px(24.0))
                                .child("Search across your projects"),
                        )
                        .child(
                            div()
                                .id("open-folder-btn")
                                .px(px(20.0))
                                .py(px(10.0))
                                .rounded(px(8.0))
                                .cursor_pointer()
                                .bg(SurchTheme::accent())
                                .text_size(px(13.0))
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(SurchTheme::text_heading())
                                .flex()
                                .items_center()
                                .gap(px(6.0))
                                .child(
                                    Icon::new(IconName::FolderOpen)
                                        .size_4()
                                        .text_color(SurchTheme::text_heading()),
                                )
                                .child("Open Folder")
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.open_folder(window, cx);
                                })),
                        )
                        .child(
                            div()
                                .text_size(px(11.0))
                                .text_color(SurchTheme::text_muted())
                                .mt(px(12.0))
                                .child("\u{2318}O to open a folder"),
                        ),
                )
                .into_any_element();
        }

        div()
            .size_full()
            .flex()
            .flex_row()
            .bg(SurchTheme::bg_primary())
            .child(self.sidebar.clone())
            .child(self.search_panel.clone())
            .child(self.preview_panel.clone())
            .into_any_element()
    }
}
