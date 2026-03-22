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
│   ├── assets/icons/        # Lucide SVG icons (embedded via rust_embed)
│   └── src/
│       ├── main.rs          # Entry point, window setup, One Dark theme, TSX fix
│       ├── app.rs           # Root view orchestrating all panels
│       ├── assets.rs         # AssetSource impl (rust_embed for SVG icons)
│       ├── sidebar.rs       # Left icon strip for channel switching
│       ├── panels/
│       │   ├── search_panel.rs   # Input fields + virtualized result list
│       │   └── preview_panel.rs  # File preview via CodeEditor (read-only, tree-sitter)
│       ├── components/      # Reusable UI components (future)
│       └── theme.rs         # Color palette (One Dark-inspired, WCAG AA)
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

## Design Philosophy

**Leverage GPUI and gpui-component maximally.** Their code is battle-tested with Zed (a full production editor used by thousands). Before building anything custom, check if GPUI or gpui-component already provides it. Search the crate source in `~/.cargo/registry/src/`. Only go custom when no built-in exists.

**The preview panel uses gpui-component's CodeEditor.** `InputState::new(window, cx).code_editor("text")` provides tree-sitter syntax highlighting, text selection/copy, built-in Cmd+F search, line numbers, and scrolling — all battle-tested from Zed. The editor runs in read-only mode via `Input::new(&state).disabled(true).appearance(false)`. Language detection maps file extensions to tree-sitter language names via `language_for_path()`, then `state.set_highlighter(lang, cx)` switches the parser. This replaced ~680 lines of custom syntect + uniform_list rendering.

## Performance Rules

### Search Engine (`engine.rs`)
- Uses `ignore` crate's `WalkBuilder` with **`build_parallel()`** for multi-threaded directory traversal (respects .gitignore). This is what ripgrep does — do NOT add Rayon directly.
- Each parallel walker thread gets its own `Searcher` instance (not `Send`). Counters use `AtomicU64`/`AtomicUsize` for thread-safe progress tracking.
- `grep-searcher` + `grep-regex` for file content search (ripgrep's library crates).
- Cancellation via `AtomicBool` checked between files. Parallel walker returns `WalkState::Quit` on cancellation.

### UI Rendering
- **Search panel uses `uniform_list`** for virtualized rendering — only renders visible items. This is critical for large result sets (1000+ matches). Do NOT replace with a naive `for` loop.
- **Search results use a `FlatRow` enum** — file headers and match rows are flattened into a single indexed list so `uniform_list` can address them. `rebuild_flat_rows()` must be called after any mutation to `file_groups` (add result, toggle collapse, clear).
- **Never clone large data in render methods.** Render methods run every frame. The `uniform_list` closure captures a snapshot of `flat_rows` (cloned once per render).
- Use `flex_shrink_0()` on fixed-height containers (header, inputs, status bar) to prevent layout jank.
- Use `overflow_hidden()` on fixed-width panels to prevent width fluctuations.

### Syntax Highlighting (Tree-sitter via CodeEditor)
- The preview panel uses gpui-component's `InputState` in `code_editor` mode, which uses **tree-sitter** for AST-based syntax highlighting. This replaced the previous `syntect` (regex/TextMate grammar) approach.
- A custom **One Dark** highlight theme is set in `setup_highlight_theme()` in `main.rs` via `serde_json::from_value()` into `HighlightThemeStyle`, then assigned to `Theme::global_mut(cx).highlight_theme`. Must be called after `gpui_component::init(cx)`.
- **TSX fix:** gpui-component ships a 35-line TSX highlights query that's only TypeScript additions (missing JS base captures). `fix_tsx_highlighting()` in `main.rs` re-registers TSX with the full TypeScript query via `LanguageRegistry::register()`.
- **SyntaxColors serde quirk:** The `comment_doc` field has NO `#[serde(rename)]` — use `"comment_doc"` not `"comment.doc"` in the JSON theme definition. Other dotted fields like `punctuation.bracket` DO have renames.
- Language detection: `language_for_path()` in `preview_panel.rs` maps 30+ file extensions to tree-sitter language names. Uses exact names from `gpui_component::highlighter::Language` enum (e.g., `"typescript"`, `"tsx"`, `"csharp"`).
- Preview pane uses **Menlo 14px** — matches VS Code's macOS default. Do not use SF Mono (narrower character width makes indentation look shallow).

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

9. **Asset system for SVG icons.** `Application::new().with_assets(SurchAssets)` registers an `AssetSource` (implemented via `rust_embed` in `assets.rs`). Icons live in `assets/icons/*.svg` (Lucide icon set). Use `gpui_component::{Icon, IconName}` — the `icon` module is private, import from crate root. `Sizable` trait must be in scope for `.with_size()`.

10. **`uniform_list` click handlers.** Inside a `uniform_list` closure, you can't use `cx.listener()` (no `&mut self`). Instead, clone the entity handle and use `entity.update(cx, |this, cx| { ... })` in the click handler.

11. **gpui-component re-exports.** `pub use icon::*` and `pub use styled::*` at crate root means `Icon`, `IconName`, `Sizable`, `Size` are all at `gpui_component::`. The `spinner` module is NOT re-exported — use `gpui_component::spinner::Spinner`.

12. **Defer any click handler that changes the element tree.** If a click handler mutates state that causes the render output to change structurally — swapping the entire view (e.g., welcome screen ↔ main view), changing a `uniform_list` item count (clearing/adding results), or removing the clicked element — GPUI panics with `panic_cannot_unwind` inside `handle_view_event`. This happens because GPUI is still processing the mouseUp event on an element that no longer exists in the new tree. **Fix:** wrap the mutation in `cx.spawn(async move |_, cx| { cx.update(|cx| { entity.update(cx, |app, cx| { /* mutate here */ }); }); }).detach()` to defer it to the next frame. This applies to any button that triggers: search refresh (clears + rebuilds results), close project (swaps to welcome screen), clear results, or any action that changes `flat_rows` length.

13. **Menu bar actions need FocusHandle.** GPUI greys out menu items when `is_action_available()` can't find a handler in the focus dispatch path. The root view needs a `FocusHandle` with `track_focus()` on its root div, and ALL action handlers must be registered via `.on_action()` on that div. Register handlers on BOTH the welcome screen and main view divs, even if some are no-ops.

14. **Keyboard shortcuts via `actions!()` + `KeyBinding::new()`.** Define actions with `actions!(surch, [OpenFolder, ...])`, bind keys with `cx.bind_keys([KeyBinding::new("cmd-o", OpenFolder, Some("surch"))])`, add `key_context("surch")` to root divs, and handle with `.on_action(cx.listener(Self::handle_open_folder))`. **Never bind bare navigation keys** (`up`, `down`, `left`, `right`, `tab`) to custom app actions — GPUI intercepts them before Input components can process them. Always use modifier keys (e.g., `cmd-down` instead of `down`) for custom shortcuts.

15. **Arrow keys in single-line Inputs crash GPUI.** gpui-component registers `"up" -> MoveUp` and `"down" -> MoveDown` key bindings in the "Input" context, but single-line Input components do NOT register `.on_action()` handlers for MoveUp/MoveDown (only multi-line inputs do). When an unhandled action propagates through the tree with no handler, GPUI panics inside `do_command_by_selector` (an `extern "C"` macOS callback), causing `panic_cannot_unwind`. **Fix:** Register no-op handlers for `gpui_component::input::{MoveUp, MoveDown}` on the root div via `.on_action()`. This catches the propagated actions and prevents the crash. These imports are aliased as `InputMoveUp`/`InputMoveDown` to avoid naming conflicts.

16. **`uniform_list` scroll position: use content height, not item height.** `UniformListScrollState::last_item_size` stores `item` (the **viewport** size) and `contents` (total content size). To compute the top visible index: `per_item = contents.height / px(item_count)`, then `top_idx = (-scroll_offset.y) / per_item`. Do NOT use `item.height` — that's the viewport height, and dividing by it gives ~0 for most scroll positions. Also, `logical_scroll_top_index()` is gated behind `#[cfg(test)]` / `feature = "test-support"` so you must compute it manually in production code.

17. **`on_scroll_wheel` inside `render()` causes double-borrow.** If you add an `on_scroll_wheel` handler to trigger `cx.notify()` for reactive updates (e.g., sticky headers), the handler fires during the same frame while the entity is already borrowed by `render()`. Calling `entity.update(cx, ...)` directly will panic with "cannot read X while it is already being updated". **Fix:** Clone the entity handle and use `cx.defer(move |cx| { entity.update(cx, |_, cx| cx.notify()); })` to defer the notification to the next frame.

18. **Cross-panel callbacks can double-borrow the source panel.** When a callback fires from within a child panel's click handler (e.g., `on_close_project` set on SearchPanel), any code in the callback chain that calls `search_panel.read(cx)` or `search_panel.update(cx, ...)` will panic — the SearchPanel is already borrowed by the click handler. **Fix:** Defer the entire callback body (including any reads of the source panel) inside `cx.spawn(async move |_, cx| { ... }).detach()`.

## Preview Panel Architecture

The preview panel wraps gpui-component's CodeEditor (`InputState` in `code_editor` mode) in read-only mode. Key design decisions:

- **Read-only via `.disabled(true)`** — blocks edits but preserves selection, copy, and built-in Cmd+F search.
- **`.appearance(false)`** — removes the Input's default border/background so the parent div controls styling.
- **`.size_full()` on Input + `.flex().flex_col()` on container** — required for the InputElement's `height: relative(1.)` to resolve correctly. Without this, only one line renders.
- **Language switching:** `state.set_highlighter(lang, cx)` sets the language name, then `state.set_value(content, window, cx)` triggers `_pending_update = true`. The highlighter is lazily created during the next render cycle via `update_highlighter()`.
- **Font:** Menlo at configurable size (default 14px, zoom via Cmd+/Cmd-). Set on a parent div wrapping the Input.
- **Search result text truncation.** Search result lines should trim leading whitespace before display (show relevant content, not deep indentation). Adjust `match_ranges` byte offsets when trimming so highlights still align correctly.

## Color Accessibility

All text/background color pairings must meet WCAG AA contrast ratios:
- Normal text: ≥ 4.5:1 contrast ratio
- Large text (≥18px or ≥14px bold): ≥ 3:1
- `text_secondary` on dark backgrounds: lightness ≥ 0.68
- `text_muted` on dark backgrounds: lightness ≥ 0.52
- Test with macOS Accessibility Inspector

## Editor Auto-Discovery

Editors are detected by scanning `/Applications` for `.app` bundles, not by `which` (GUI apps don't inherit shell PATH). Each editor uses `open -a` as fallback if CLI isn't in PATH. Cursor uses `--goto file:line` (same as VS Code).

## Key Dependencies

| Crate | Purpose |
|---|---|
| `gpui` 0.2.2 | GPU-accelerated UI framework (Metal on macOS) |
| `gpui-component` 0.5.1 | CodeEditor, Input, scroll, Root, Icon components (with `tree-sitter-languages` feature for syntax highlighting) |
| `grep` + `grep-regex` + `grep-searcher` | ripgrep's search engine as a library |
| `ignore` 0.4 | Directory walking with .gitignore, include/exclude globs, parallel walking |
| `crossbeam-channel` | Background thread → UI communication |
| `serde_json` 1 | One Dark highlight theme deserialization |
| `rust-embed` 8 | Compile-time asset embedding (SVG icons) |
| `num_cpus` 1 | CPU count for parallel walker thread pool |

## Pre-flight Checklist

Before submitting any code change, verify these items. Each one has caused bugs multiple times:

### Adding a new action (keyboard shortcut / menu item)
1. Add the action to `actions!()` in `app.rs`
2. Add the handler method `handle_*()` on `SurchApp`
3. Add `.on_action(cx.listener(Self::handle_*))` **in `root_div()`** — this is the single source of truth. Do NOT add `.on_action()` directly on any view div.
4. Add `KeyBinding::new(...)` in `main.rs`
5. Add the menu item in `main.rs` if needed
6. Import the action in `main.rs`

### Adding a new icon button
1. Check that the SVG exists in `crates/surch-app/assets/icons/`. If using an `IconName::*` variant, the SVG must be in **our** assets folder — gpui-component's built-in icons are NOT included via `SurchAssets`.
2. Use `size_4()` minimum and `text_color(SurchTheme::text_heading())` — smaller sizes and muted colors are invisible on dark backgrounds.
3. If the icon name doesn't exist in `IconName`, download the Lucide SVG from https://lucide.dev and add it to `assets/icons/`.

### Click handlers that mutate state
1. If the handler changes `flat_rows` length, swaps views (welcome ↔ main), or removes the clicked element → **wrap in `cx.spawn(async move |_, cx| { ... }).detach()`** to defer to next frame. GPUI panics otherwise.
2. Call `cx.notify()` after mutations so the UI re-renders.

### Modifying uniform_list data
1. After any mutation to `file_groups` (add, remove, toggle collapse, clear), call `rebuild_flat_rows()`.
2. Never clone large vecs inside the render closure — snapshot once before the closure, share via `Rc` if needed.

### Adding or changing any feature
1. Add tests for the new behavior in the relevant crate's `#[cfg(test)]` module.
2. Run `cargo test --workspace` and confirm all tests pass before submitting.
3. Run `cargo tarpaulin --workspace --skip-clean --exclude surch-app` to check code coverage — aim for ≥80% on `surch-core` and `surch-file-search`.
