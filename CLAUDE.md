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
│       ├── main.rs          # Entry point, window setup, Root wrapper
│       ├── app.rs           # Root view orchestrating all panels
│       ├── assets.rs         # AssetSource impl (rust_embed for SVG icons)
│       ├── sidebar.rs       # Left icon strip for channel switching
│       ├── panels/
│       │   ├── search_panel.rs   # Input fields + virtualized result list
│       │   └── preview_panel.rs  # File preview with syntax highlighting
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

**The preview panel is intentionally custom.** It renders syntax-highlighted code using `uniform_list` + `syntect` spans + manual div layout. This is the right approach — gpui-component's `TextView` is Markdown/HTML-first and would add parsing overhead for raw code files. For text selection + copy, look at gpui-component's `Inline` component (`text/inline.rs`) which handles selection and clipboard at the span level. For Markdown file preview (READMEs), consider adopting `TextView` directly.

## Performance Rules

### Search Engine (`engine.rs`)
- Uses `ignore` crate's `WalkBuilder` for directory traversal (respects .gitignore).
- **Do NOT add Rayon directly** — use `ignore`'s `build_parallel()` instead of `build()` for parallel directory walking. This is what ripgrep does.
- `grep-searcher` + `grep-regex` for file content search (ripgrep's library crates).
- Cancellation via `AtomicBool` checked between files.

### UI Rendering
- **Both panels use `uniform_list`** for virtualized rendering — only renders visible items. This is critical for large result sets (1000+ matches) and large files (1000+ lines). Do NOT replace with a naive `for` loop.
- **Search results use a `FlatRow` enum** — file headers and match rows are flattened into a single indexed list so `uniform_list` can address them. `rebuild_flat_rows()` must be called after any mutation to `file_groups` (add result, toggle collapse, clear).
- **Never clone large data in render methods.** Render methods run every frame. The `uniform_list` closure captures a snapshot of `flat_rows` (cloned once per render) and `highlighted_lines` (shared via `Rc`).
- Use `flex_shrink_0()` on fixed-height containers (header, inputs, status bar) to prevent layout jank.
- Use `overflow_hidden()` on fixed-width panels to prevent width fluctuations.

### Syntax Highlighting
- Uses `syntect` with a custom **One Dark** theme (`assets/themes/one-dark.tmTheme`). Switched from `base16-ocean.dark` which was too muted/dull.
- **Use `find_syntax_for_file()`** — it tries filename, then extension, then first-line detection (shebangs). Do NOT use `find_syntax_by_extension()` (fails on compound extensions like `.htmltemplate`).
- **Append `\n` to each line** before calling `highlight_line()` when using `SyntaxSet::load_defaults_newlines()`. Without the trailing newline, syntect's parser state drifts and highlighting breaks after ~100 lines.
- Highlighted spans are pre-computed in `load_file()` and stored as `Rc<Vec<Vec<(Hsla, String)>>>` for sharing with the uniform_list render closure.
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

## Syntect Gotchas

12. **Do NOT filter empty spans before storing.** When processing `highlight_line()` output, spans with empty text still carry parser state. Filtering them with `.filter(|(_, text)| !text.is_empty())` causes syntect's parse state to desync, breaking highlighting after ~50-100 lines. Instead, keep all spans in the stored data and skip empty ones only at render time.

13. **Search result text truncation.** Search result lines should trim leading whitespace before display (show relevant content, not deep indentation). Adjust `match_ranges` byte offsets when trimming so highlights still align correctly.

## Future: Tree-sitter for Syntax Highlighting

`syntect` (regex-based, TextMate grammars) works but is line-by-line and can drift on complex files. **Tree-sitter** (C library with Rust bindings via `tree-sitter` crate) is what Zed, Neovim, Helix, and Atom use — it parses into a full AST and supports incremental re-parsing (only re-highlights changed regions). This is a post-v1.0 consideration for better highlighting accuracy and performance on large files. Tree-sitter also provides the AST needed for a future Symbol Search channel (function/class/type definitions for free).

## Preview Panel Architecture

The preview panel is intentionally custom — `uniform_list` + `syntect` spans + manual div layout. This is correct because:
- gpui-component's `TextView` is Markdown/HTML-first, not designed for raw code files
- `Inline` component (inside `TextView`) handles text selection + clipboard at the span level — adopt this for text selection without taking the full `TextView`
- For Markdown files (README preview), consider using `TextView` directly
- C++ interop for text editors (e.g., Scintilla) won't work because GPUI owns the Metal rendering surface — can't embed foreign widgets

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
| `gpui-component` 0.5.1 | Input, scroll, Root, Icon, Spinner components |
| `grep` + `grep-regex` + `grep-searcher` | ripgrep's search engine as a library |
| `ignore` 0.4 | Directory walking with .gitignore, include/exclude globs, parallel walking |
| `crossbeam-channel` | Background thread → UI communication |
| `syntect` 5 | Syntax highlighting (base16-ocean.dark theme) |
| `rust-embed` 8 | Compile-time asset embedding (SVG icons) |
