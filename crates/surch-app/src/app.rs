use crate::panels::preview_panel::PreviewPanel;
use crate::panels::search_panel::{SearchPanel, SearchResultItem};
use crate::sidebar::Sidebar;
use crate::theme::SurchTheme;
use crossbeam_channel::{Receiver, TryRecvError};
use gpui::*;
use gpui::prelude::FluentBuilder;
use gpui_component::input::{MoveUp as InputMoveUp, MoveDown as InputMoveDown};
use gpui_component::{Icon, IconName};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use surch_core::channel::{ChannelQuery, SearchEvent};
use surch_core::config::{AppConfig, WorkspaceState};
use surch_core::registry::ChannelRegistry;
use surch_file_search::FileSearchChannel;

// Define keyboard shortcut actions
actions!(
    surch,
    [
        OpenFolder,
        CloseProject,
        FocusFind,
        ToggleCaseSensitive,
        ToggleWholeWord,
        ToggleRegex,
        ToggleViewMode,
        SelectNextResult,
        SelectPreviousResult,
        OpenInEditor,
        ClearSearch,
        ZoomIn,
        ZoomOut,
        ZoomReset,
        GoToLine,
        Quit,
        Cut,
        Copy,
        Paste,
        SelectAll,
    ]
);

pub struct SurchApp {
    sidebar: Entity<Sidebar>,
    search_panel: Entity<SearchPanel>,
    preview_panel: Entity<PreviewPanel>,
    registry: ChannelRegistry,
    workspace_root: Option<PathBuf>,
    search_receiver: Option<Receiver<SearchEvent>>,
    pending_query: Option<HashMap<String, String>>,
    current_result: Option<SearchResultItem>,
    focus_handle: FocusHandle,
    app_config: AppConfig,
    /// Width of the search panel in pixels (draggable divider).
    search_panel_width: f32,
    /// X position when divider drag started, or None if not dragging.
    divider_drag_start: Option<(f32, f32)>, // (mouse_x, panel_width_at_start)
}

const MIN_SEARCH_PANEL_WIDTH: f32 = 200.0;
const MAX_SEARCH_PANEL_WIDTH: f32 = 600.0;
const DEFAULT_SEARCH_PANEL_WIDTH: f32 = 340.0;

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

        let app_config = AppConfig::load();

        let mut app = Self {
            sidebar,
            search_panel,
            preview_panel,
            registry,
            workspace_root: None,
            search_receiver: None,
            pending_query: None,
            current_result: None,
            focus_handle: cx.focus_handle(),
            app_config,
            search_panel_width: DEFAULT_SEARCH_PANEL_WIDTH,
            divider_drag_start: None,
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

        // Replace All callback
        let app_entity = cx.entity().clone();
        self.search_panel.update(cx, |panel, _cx| {
            panel.on_replace_all = Some(Box::new(move |replace_text, _window, cx| {
                let replace_text = replace_text.clone();
                app_entity.update(cx, |app, cx| {
                    app.handle_replace_all(replace_text, cx);
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

        let (case_sensitive, whole_word, is_regex, preserve_case) =
            self.search_panel.read(cx).search_options();

        let query = ChannelQuery {
            fields: values,
            workspace_root: workspace,
            is_regex,
            case_sensitive,
            whole_word,
            preserve_case,
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
        // Defer to next frame — clearing results during a click event on the
        // results list causes GPUI to panic (view tree changes mid-event).
        let entity = cx.entity().clone();
        cx.spawn(async move |_, cx| {
            let _ = cx.update(|cx| {
                entity.update(cx, |app, cx| {
                    let input_values = app.search_panel.update(cx, |panel, cx| {
                        let mut vals = HashMap::new();
                        for (id, input) in &panel.inputs {
                            vals.insert(id.clone(), input.read(cx).value().to_string());
                        }
                        vals
                    });
                    app.pending_query = Some(input_values);
                    app.execute_search(cx);
                });
            });
        })
        .detach();
    }

    fn handle_replace_all(&mut self, replace_text: String, cx: &mut Context<Self>) {
        let workspace = match &self.workspace_root {
            Some(root) => root.clone(),
            None => return,
        };

        // Build the query from current search inputs
        let input_values = self.search_panel.update(cx, |panel, cx| {
            let mut vals = HashMap::new();
            for (id, input) in &panel.inputs {
                vals.insert(id.clone(), input.read(cx).value().to_string());
            }
            vals
        });

        let find_text = input_values.get("find").cloned().unwrap_or_default();
        if find_text.is_empty() {
            return;
        }

        let (case_sensitive, whole_word, is_regex, preserve_case) =
            self.search_panel.read(cx).search_options();

        let query = ChannelQuery {
            fields: input_values,
            workspace_root: workspace,
            is_regex,
            case_sensitive,
            whole_word,
            preserve_case,
        };

        // Cancel any in-progress search before replacing
        if let Some(channel) = self.registry.active() {
            channel.cancel();
        }
        self.search_receiver = None;

        // Run replace on a background thread
        let cancelled = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let cancelled_clone = cancelled.clone();
        let replace_text_clone = replace_text.clone();

        std::thread::spawn(move || {
            let (tx, _rx) = crossbeam_channel::unbounded();
            let (files_modified, replacements_made) =
                surch_file_search::engine::run_replace(query, &replace_text_clone, tx, cancelled_clone);
            eprintln!(
                "Replace all: {} replacements in {} files",
                replacements_made, files_modified
            );
        });

        // Defer refresh to next frame to let the replace thread finish writing files
        let entity2 = cx.entity().clone();
        cx.spawn(async move |_, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(300))
                .await;
            let _ = cx.update(|cx| {
                entity2.update(cx, |app, cx| {
                    app.refresh_search(cx);
                });
            });
        })
        .detach();
    }

    fn close_project(&mut self, cx: &mut Context<Self>) {
        // Save workspace state before closing
        self.save_workspace_state(cx);

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

    fn handle_open_folder(&mut self, _: &OpenFolder, window: &mut Window, cx: &mut Context<Self>) {
        self.open_folder(window, cx);
    }

    fn handle_close_project(
        &mut self,
        _: &CloseProject,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.close_project(cx);
    }

    fn handle_focus_find(&mut self, _: &FocusFind, window: &mut Window, cx: &mut Context<Self>) {
        self.search_panel.update(cx, |panel, cx| {
            panel.focus_find(window, cx);
        });
    }

    fn handle_toggle_case(
        &mut self,
        _: &ToggleCaseSensitive,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.search_panel.update(cx, |panel, cx| {
            panel.toggle_case_sensitive(window, cx);
        });
    }

    fn handle_toggle_word(
        &mut self,
        _: &ToggleWholeWord,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.search_panel.update(cx, |panel, cx| {
            panel.toggle_whole_word(window, cx);
        });
    }

    fn handle_toggle_regex(
        &mut self,
        _: &ToggleRegex,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.search_panel.update(cx, |panel, cx| {
            panel.toggle_regex(window, cx);
        });
    }

    fn handle_select_next(
        &mut self,
        _: &SelectNextResult,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Defer to next frame — arrow keys route through macOS's inputContext →
        // do_command_by_selector (an extern "C" callback). Any state mutation
        // that triggers re-render inside that callback causes panic_cannot_unwind.
        // Use select_next_item (not select_next) to avoid the on_result_selected
        // callback which would re-entrantly update SurchApp → double lease panic.
        let entity = cx.entity().clone();
        window.on_next_frame(move |window, cx| {
            let _ = window;
            entity.update(cx, |app, cx| {
                let selected_item = app.search_panel.update(cx, |panel, cx| {
                    panel.select_next_item(cx)
                });
                if let Some(result) = selected_item {
                    app.handle_result_selected(result, cx);
                }
            });
        });
    }

    fn handle_select_previous(
        &mut self,
        _: &SelectPreviousResult,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Defer to next frame — same reason as handle_select_next.
        let entity = cx.entity().clone();
        window.on_next_frame(move |window, cx| {
            let _ = window;
            entity.update(cx, |app, cx| {
                let selected_item = app.search_panel.update(cx, |panel, cx| {
                    panel.select_previous_item(cx)
                });
                if let Some(result) = selected_item {
                    app.handle_result_selected(result, cx);
                }
            });
        });
    }

    fn handle_open_in_editor(
        &mut self,
        _: &OpenInEditor,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) {
        // Open the currently selected result in the default editor
        if let Some(ref result) = self.current_result {
            if let Some(channel) = self.registry.active() {
                let entry = surch_core::channel::ResultEntry {
                    id: result.id,
                    file_path: Some(result.file_path.clone()),
                    line_number: Some(result.line_number),
                    column: None,
                    line_content: result.line_content.clone(),
                    match_ranges: result.match_ranges.clone(),
                };
                let actions = channel.actions(&entry);
                // Use the first available editor action
                if let Some(action) = actions.first() {
                    if let Err(e) = channel.execute_action(&action.id, &entry) {
                        eprintln!("Open in editor error: {}", e);
                    }
                }
            }
        }
    }

    fn handle_clear_search(
        &mut self,
        _: &ClearSearch,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Cancel any in-progress search
        if let Some(channel) = self.registry.active() {
            channel.cancel();
        }
        self.search_receiver = None;

        // Clear results — defer to avoid GPUI crash
        let entity = cx.entity().clone();
        cx.spawn(async move |_, cx| {
            let _ = cx.update(|cx| {
                entity.update(cx, |app, cx| {
                    app.search_panel.update(cx, |panel, _cx| {
                        panel.clear_results();
                        panel.set_searching(false);
                    });
                    app.current_result = None;
                    app.preview_panel.update(cx, |panel, _cx| {
                        panel.load_empty();
                    });
                    cx.notify();
                });
            });
        })
        .detach();
    }

    fn handle_zoom_in(
        &mut self,
        _: &ZoomIn,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.preview_panel.update(cx, |panel, _cx| {
            panel.zoom_in();
        });
        cx.notify();
    }

    fn handle_zoom_out(
        &mut self,
        _: &ZoomOut,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.preview_panel.update(cx, |panel, _cx| {
            panel.zoom_out();
        });
        cx.notify();
    }

    fn handle_zoom_reset(
        &mut self,
        _: &ZoomReset,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.preview_panel.update(cx, |panel, _cx| {
            panel.zoom_reset();
        });
        cx.notify();
    }

    fn handle_go_to_line(
        &mut self,
        _: &GoToLine,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.preview_panel.update(cx, |panel, cx| {
            panel.show_go_to_line(window, cx);
        });
    }

    fn handle_toggle_view_mode(
        &mut self,
        _: &ToggleViewMode,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.search_panel.update(cx, |panel, _cx| {
            panel.toggle_view_mode();
        });
        cx.notify();
    }

    /// Set the active workspace, save to recent history, and load workspace state.
    fn set_workspace(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        // Save to recent workspaces
        self.app_config.add_recent_workspace(path.clone());
        let _ = self.app_config.save();

        // Load workspace-specific state (search history, filters, etc.)
        let ws_state = WorkspaceState::load(&path);

        self.workspace_root = Some(path.clone());
        self.preview_panel.update(cx, |panel, _cx| {
            panel.set_workspace_root(path.clone());
        });
        self.search_panel.update(cx, |panel, _cx| {
            panel.set_workspace_root(path);
            // Restore last search options from workspace state
            panel.restore_options(ws_state.case_sensitive, ws_state.whole_word, ws_state.is_regex, false);
        });
        cx.notify();
    }

    /// Save current workspace state before closing.
    fn save_workspace_state(&self, cx: &mut Context<Self>) {
        if let Some(ref workspace_path) = self.workspace_root {
            let (case_sensitive, whole_word, is_regex, _preserve_case) =
                self.search_panel.read(cx).search_options();
            let mut ws_state = WorkspaceState::load(workspace_path);
            ws_state.case_sensitive = case_sensitive;
            ws_state.whole_word = whole_word;
            ws_state.is_regex = is_regex;
            // TODO: Save search/replace/filter history when those features land
            let _ = ws_state.save(workspace_path);
        }
    }

    fn render_recent_workspaces(&self, cx: &mut Context<Self>) -> Option<Div> {
        if self.app_config.recent_workspaces.is_empty() {
            return None;
        }

        let mut list = div()
            .mt(px(24.0))
            .w(px(320.0))
            .flex()
            .flex_col();

        list = list.child(
            div()
                .text_size(px(11.0))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(SurchTheme::text_muted())
                .mb(px(8.0))
                .child("RECENT"),
        );

        for (idx, recent) in self.app_config.recent_workspaces.iter().enumerate() {
            let path = recent.path.clone();
            let folder_name = path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| path.to_string_lossy().to_string());
            let full_path = path.to_string_lossy().to_string();

            // Shorten home directory for display
            let display_path = if let Some(home) = dirs::home_dir() {
                if let Ok(stripped) = path.strip_prefix(&home) {
                    format!("~/{}", stripped.display())
                } else {
                    full_path.clone()
                }
            } else {
                full_path.clone()
            };

            list = list.child(
                div()
                    .id(ElementId::Name(format!("recent-{}", idx).into()))
                    .w_full()
                    .px(px(8.0))
                    .py(px(6.0))
                    .rounded(px(4.0))
                    .cursor_pointer()
                    .hover(|s| s.bg(SurchTheme::bg_surface()))
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(
                        Icon::new(IconName::Folder)
                            .size_4()
                            .text_color(SurchTheme::text_muted()),
                    )
                    .child(
                        div()
                            .flex_1()
                            .overflow_hidden()
                            .child(
                                div()
                                    .text_size(px(13.0))
                                    .text_color(SurchTheme::text_primary())
                                    .whitespace_nowrap()
                                    .child(folder_name),
                            )
                            .child(
                                div()
                                    .text_size(px(11.0))
                                    .text_color(SurchTheme::text_muted())
                                    .whitespace_nowrap()
                                    .child(display_path),
                            ),
                    )
                    .on_click(cx.listener(move |this, _, _window, cx| {
                        let p = path.clone();
                        this.set_workspace(p, cx);
                    })),
            );
        }

        Some(list)
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
                                app.set_workspace(path, cx);
                            });
                        });
                    }
                }
            }
        })
        .detach();
    }
}

impl SurchApp {
    /// Render a custom title bar that blends with the app theme.
    /// With appears_transparent + traffic_light_position, the OS title bar
    /// is invisible and we draw our own.
    fn render_title_bar(&self) -> Div {
        let title = if let Some(ref root) = self.workspace_root {
            root.file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "surch".to_string())
        } else {
            "surch".to_string()
        };

        div()
            .w_full()
            .h(px(32.0))
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_center()
            .bg(SurchTheme::bg_sidebar())
            .border_b_1()
            .border_color(SurchTheme::border())
            .child(
                div()
                    .text_size(px(12.0))
                    .text_color(SurchTheme::text_secondary())
                    .child(title),
            )
    }
}

impl SurchApp {
    /// Returns true if any input field (search panel or preview panel) has focus.
    #[allow(dead_code)]
    fn any_input_focused(&self, window: &Window, cx: &App) -> bool {
        self.search_panel.read(cx).any_input_focused(window, cx)
            || self.preview_panel.read(cx).any_input_focused(window, cx)
    }

    /// Create a root div with ALL action handlers registered.
    /// IMPORTANT: This is the single source of truth for action registration.
    /// Every new action MUST be added here — never register actions directly
    /// on individual view divs. GPUI greys out menu items when it can't find
    /// a handler in the focus dispatch path, so both the welcome screen and
    /// main view must have identical handler sets.
    fn root_div(&self, cx: &mut Context<Self>) -> Stateful<Div> {
        div()
            .id("surch-root")
            .key_context("surch")
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::handle_open_folder))
            .on_action(cx.listener(Self::handle_close_project))
            .on_action(cx.listener(Self::handle_focus_find))
            .on_action(cx.listener(Self::handle_toggle_case))
            .on_action(cx.listener(Self::handle_toggle_word))
            .on_action(cx.listener(Self::handle_toggle_regex))
            .on_action(cx.listener(Self::handle_select_next))
            .on_action(cx.listener(Self::handle_select_previous))
            .on_action(cx.listener(Self::handle_open_in_editor))
            .on_action(cx.listener(Self::handle_clear_search))
            .on_action(cx.listener(Self::handle_zoom_in))
            .on_action(cx.listener(Self::handle_zoom_out))
            .on_action(cx.listener(Self::handle_zoom_reset))
            .on_action(cx.listener(Self::handle_go_to_line))
            .on_action(cx.listener(Self::handle_toggle_view_mode))
            // Catch unhandled MoveUp/MoveDown from single-line Input components.
            // Without these, arrow keys in single-line inputs cause a panic in
            // GPUI's do_command_by_selector (macOS text input callback) because
            // the action propagates with no handler and panics inside extern "C".
            .on_action(cx.listener(|_this, _: &InputMoveUp, _window, _cx| {}))
            .on_action(cx.listener(|_this, _: &InputMoveDown, _window, _cx| {}))
    }
}

impl Render for SurchApp {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.workspace_root.is_none() {
            return self
                .root_div(cx)
                .size_full()
                .flex()
                .flex_col()
                .bg(SurchTheme::bg_primary())
                .child(self.render_title_bar())
                .child(
                    div()
                        .flex_1()
                        .flex()
                        .flex_col()
                        .items_center()
                        .justify_center()
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
                        )
                        .children(self.render_recent_workspaces(cx)),
                )
                .into_any_element();
        }

        let panel_width = self.search_panel_width;
        let is_dragging = self.divider_drag_start.is_some();

        let mut main_content = self.root_div(cx)
            .size_full()
            .flex()
            .flex_col()
            .bg(SurchTheme::bg_primary())
            .child(self.render_title_bar())
            .child(
                div()
                    .flex_1()
                    .flex()
                    .flex_row()
                    .overflow_hidden()
                    .child(self.sidebar.clone())
                    // Search panel with dynamic width
                    .child(
                        div()
                            .flex_shrink_0()
                            .w(px(panel_width))
                            .h_full()
                            .overflow_hidden()
                            .border_r_1()
                            .border_color(SurchTheme::border())
                            .child(self.search_panel.clone()),
                    )
                    // Draggable divider
                    .child(
                        div()
                            .id("panel-divider")
                            .w(px(4.0))
                            .h_full()
                            .flex_shrink_0()
                            .cursor(CursorStyle::ResizeLeftRight)
                            .hover(|s| s.bg(SurchTheme::accent()))
                            .when(is_dragging, |s| s.bg(SurchTheme::accent()))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |this, event: &MouseDownEvent, _window, _cx| {
                                    let x: f32 = event.position.x.into();
                                    this.divider_drag_start =
                                        Some((x, this.search_panel_width));
                                }),
                            )
                    )
                    .child(self.preview_panel.clone()),
            );

        // Track mouse move/up on the root div during drag
        if is_dragging {
            main_content = main_content
                .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _window, cx| {
                    if let Some((start_x, start_width)) = this.divider_drag_start {
                        let current_x: f32 = event.position.x.into();
                        let delta = current_x - start_x;
                        let new_width = (start_width + delta)
                            .clamp(MIN_SEARCH_PANEL_WIDTH, MAX_SEARCH_PANEL_WIDTH);
                        this.search_panel_width = new_width;
                        cx.notify();
                    }
                }))
                .on_mouse_up(
                    MouseButton::Left,
                    cx.listener(|this, _, _window, cx| {
                        this.divider_drag_start = None;
                        cx.notify();
                    }),
                );
        }

        main_content.into_any_element()
    }
}
