# CLAUDE.md

## What is surch?

Surch is a standalone desktop search app for macOS, built in Rust on GPUI (Zed's GPU-accelerated framework). It provides VS Code's search UX as a dedicated app with an extensible channel architecture. macOS only — renders via Metal.

## Build & Run

```sh
cargo build          # compile all crates
cargo run            # launch the app
```

Requires **full Xcode** (not just Command Line Tools) for Metal shader compilation. If `cargo` isn't found, run `source ~/.cargo/env` first.

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
│       │   └── preview_panel.rs  # File preview with syntax highlighting
│       ├── components/      # Reusable UI components (future)
│       └── theme.rs         # Color palette (One Dark-inspired)
├── surch-core/              # Framework-agnostic core (library)
│   └── src/
│       ├── channel.rs       # Channel trait — the extension interface
│       ├── registry.rs      # Channel registration + active tracking
│       ├── workspace.rs     # Workspace/folder types
│       └── config.rs        # App config (~/.config/surch/)
└── surch-file-search/       # V1 channel: file content search (library)
    └── src/
        ├── lib.rs           # Implements Channel trait, editor auto-discovery
        └── engine.rs        # Search engine using grep + ignore crates
```

## Architecture Decisions

### Channel Trait
Every search mode implements `Channel` (`surch-core/src/channel.rs`). It defines inputs, search logic, preview content, and actions. The file search channel is V1. Future channels (k8s pods, git, etc.) just implement this trait.

### Event Streaming
Search runs on a **background thread**, streams `SearchEvent` results via `crossbeam_channel`. The UI polls results in batches of 100 on a 16ms timer (`poll_search_results` in `app.rs`). Never block the UI thread.

### Debounced Search
Input changes are debounced 150ms before triggering search (`handle_query_changed` in `app.rs`).

### Cross-Panel Communication
Panels communicate via public callback fields (`on_query_changed`, `on_result_selected`, `on_action_selected`) set in `SurchApp::setup_callbacks`. This avoids trait coupling between panels.

## Performance Rules

### Search Engine (`engine.rs`)
- Uses `ignore` crate's `WalkBuilder` for directory traversal (respects .gitignore).
- **Do NOT add Rayon directly** — use `ignore`'s `build_parallel()` instead of `build()` for parallel directory walking. This is what ripgrep does.
- `grep-searcher` + `grep-regex` for file content search (ripgrep's library crates).
- Cancellation via `AtomicBool` checked between files.

### UI Rendering
- **Preview pane uses `uniform_list`** for virtualized rendering — only renders visible lines. This is critical for large files (1000+ lines). Do NOT replace with a naive `for` loop.
- **Never clone large data in render methods.** The search panel's `render()` iterates `file_groups` by index, not by cloning. Render methods run every frame.
- Use `flex_shrink_0()` on fixed-height containers (header, inputs, status bar) to prevent layout jank.
- Use `overflow_hidden()` on fixed-width panels to prevent width fluctuations.

### Syntax Highlighting
- Uses `syntect` with `base16-ocean.dark` theme.
- **Use `find_syntax_for_file()`** — it tries filename, then extension, then first-line detection (shebangs). Do NOT use `find_syntax_by_extension()` (fails on compound extensions like `.htmltemplate`).
- Highlighted spans are pre-computed in `load_file()` and stored as `Rc<Vec<Vec<(Hsla, String)>>>` for sharing with the uniform_list render closure.

## GPUI Gotchas

These are hard-won lessons. Read before touching GPUI code:

1. **Subscriptions auto-cancel on drop.** `cx.subscribe_in()` returns a `Subscription`. You **must** call `.detach()` on it. `let _ = cx.subscribe_in(...)` silently unsubscribes — nothing will work and there's no error.

2. **Root wrapper required.** The top-level view must be wrapped in `gpui_component::Root::new()` in `main.rs`. Without it, `Input` components crash with an opaque unwrap panic.

3. **Async spawn pattern.** Always use:
   ```rust
   cx.spawn(async move |_, cx| {
       cx.update(|cx| { entity.update(cx, |app, cx| { ... }); });
   }).detach();
   ```
   Do NOT use `cx.to_async()` — it doesn't exist.

4. **`overflow_y_scrollbar()`** comes from `gpui_component::scroll::ScrollableElement` trait, not built-in GPUI.

5. **`uniform_list`** for large lists. Takes `(id, item_count, render_fn)` where render_fn receives a `Range<usize>` of visible items. Only renders what's on screen.

6. **FontWeight constants**: `BOLD`, `SEMIBOLD`, `MEDIUM`, `NORMAL` — note it's `SEMIBOLD` not `SEMI_BOLD`.

7. **Click handlers need `.id()`.** Use `cx.listener(|this, event, window, cx| ...)` on elements that have an `.id("something")`.

8. **Element borders add width.** `border_l_2()` adds 2px to element width. For indicators that shouldn't change layout, use a separate sibling div with fixed width.

## Editor Auto-Discovery

Editors are detected by scanning `/Applications` for `.app` bundles, not by `which` (GUI apps don't inherit shell PATH). Each editor uses `open -a` as fallback if CLI isn't in PATH. Cursor uses `--goto file:line` (same as VS Code).

## Key Dependencies

| Crate | Purpose |
|---|---|
| `gpui` 0.2.2 | GPU-accelerated UI framework (Metal on macOS) |
| `gpui-component` 0.5.1 | Input, scroll, Root components |
| `grep` + `grep-regex` + `grep-searcher` | ripgrep's search engine as a library |
| `ignore` 0.4 | Directory walking with .gitignore, include/exclude globs, parallel walking |
| `crossbeam-channel` | Background thread → UI communication |
| `syntect` 5 | Syntax highlighting (base16-ocean.dark theme) |
