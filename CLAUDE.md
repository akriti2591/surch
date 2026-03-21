# CLAUDE.md

## What is surch?

Surch is a standalone desktop search app for macOS, built in Rust on GPUI (Zed's GPU-accelerated framework). It provides VS Code's search UX as a dedicated app with an extensible channel architecture.

## Build & Run

```sh
cargo build          # compile all crates
cargo run            # launch the app
```

Requires full Xcode (not just Command Line Tools) for Metal shader compilation.

## Project Structure

```
crates/
├── surch-app/              # GPUI desktop application (binary)
│   └── src/
│       ├── main.rs          # Entry point, window setup, Root wrapper
│       ├── app.rs           # Root view orchestrating all panels
│       ├── sidebar.rs       # Left icon strip for channel switching
│       ├── panels/
│       │   ├── search_panel.rs   # Input fields + grouped result list
│       │   └── preview_panel.rs  # File preview with line highlighting
│       ├── components/      # Reusable UI components (future)
│       └── theme.rs         # Color constants
├── surch-core/              # Framework-agnostic core (library)
│   └── src/
│       ├── channel.rs       # Channel trait — the extension interface
│       ├── registry.rs      # Channel registration + active tracking
│       ├── workspace.rs     # Workspace/folder types
│       └── config.rs        # App config (~/.config/surch/)
└── surch-file-search/       # V1 channel: file content search (library)
    └── src/
        ├── lib.rs           # Implements Channel trait, editor detection
        └── engine.rs        # Search engine using grep + ignore crates
```

## Key Patterns

- **Channel trait** (`surch-core/src/channel.rs`): Every search mode implements this. It defines inputs, search logic, preview content, and actions. The file search channel is the V1 implementation.
- **Event streaming**: Search runs on a background thread, streams `SearchEvent` results via `crossbeam_channel`. The UI polls results in batches of 100 on a 16ms timer.
- **Debounced search**: Input changes are debounced 150ms before triggering search.
- **Callback wiring**: Cross-panel communication uses public callback fields (`on_query_changed`, `on_result_selected`, `on_action_selected`) set in `SurchApp::setup_callbacks`.
- **Root wrapper**: The top-level view must be wrapped in `gpui_component::Root` for Input components to work (provides theme context).

## GPUI Gotchas

- `cx.subscribe_in()` returns a `Subscription` — you **must** call `.detach()` on it or store it. Dropping it immediately unsubscribes.
- Async spawns use the pattern: `cx.spawn(async move |_, cx| { cx.update(|cx| { entity.update(cx, |app, cx| { ... }); }); }).detach();`
- `overflow_y_scrollbar()` comes from `gpui_component::scroll::ScrollableElement` trait (not built into gpui).
- Use `cx.listener(|this, event, window, cx| ...)` for click handlers on elements with an `id`.
