# Surch Roadmap

> Last updated: 2026-03-22

## Vision

Surch aims to be the best standalone search tool for macOS — bringing VS Code/Cursor's search UX into a dedicated, fast, GPU-accelerated app. The goal is feature parity with Cursor's search panel, plus unique advantages that come from being a standalone tool (multi-project search, channel extensibility).

## Vision & Inspiration

The power of good search is not limited to file contents. Tools like DevOps Toolbox's unified CLI (["This Tool Replaced 7 CLIs"](https://www.youtube.com/watch?v=wwJA9mDqIVc)) demonstrate that developers juggle dozens of separate tools for searching across different domains — git logs, Kubernetes pods, config files, sessions, bookmarks, and more. Each domain has its own CLI, its own flags, its own output format. The insight: a single search interface with fuzzy finding, live preview, and a consistent UX can replace all of them.

Surch's channel architecture is built for exactly this. Every search domain becomes a channel — same input fields, same result list, same preview pane, same keyboard shortcuts. A developer shouldn't need to context-switch between `grep`, `git log --grep`, `kubectl logs`, browser history search, and a dozen other tools. They should open Surch, pick a channel (or search across all channels), and find what they need.

The long-term north star: **universal search across anything a developer touches**, with each data source plugged in as a channel. File search is v1. Everything else is the same pattern.

---

## Milestone 1: Alpha (Core Functionality)

Fix what's broken. Make the app usable for daily work.

### P0 Bugs

#### 1.1 Fix Scroll Performance ✅
**Status:** Fixed — both search results and preview panel use `uniform_list` for virtualized rendering.
**Problem:** The search results panel renders all file groups in a single `div` with `overflow_y_scrollbar()`. For large result sets (500+ matches), this means hundreds of DOM elements are created every frame. The preview panel already uses `uniform_list` but still stutters on large files.
**Fix:**
- Replace the results list `for` loop in `search_panel.rs` `render()` with `uniform_list`. This requires flattening `file_groups` into a single indexed list of "rows" (file headers + match lines) so `uniform_list` can address them by index range. Each row type (header vs. match) renders differently based on its index.
- Audit preview panel's `uniform_list` implementation for unnecessary re-allocations in the render closure.
- Add `flex_shrink_0()` on inputs container (already done) and status bar to prevent layout fighting.
**Complexity:** M
**Files:** `search_panel.rs`, `preview_panel.rs`

#### 1.2 Fix Syntax Highlighting ✅
**Status:** Fixed — replaced syntect with gpui-component's CodeEditor (tree-sitter based). One Dark theme configured via `HighlightThemeStyle`. TSX highlighting fixed by re-registering with full TypeScript query.

#### 1.3 Fix Color Accessibility
**Status:** Broken — some text is hard to read against backgrounds.
**Problem:** Several color combinations fail WCAG AA contrast ratios. `text_secondary` on `bg_secondary` and `text_muted` on `bg_surface` are particularly bad.
**Fix:** Audit all `SurchTheme` color pairings. Ensure all text/background combinations meet at least 4.5:1 contrast ratio for normal text and 3:1 for large text. Increase lightness of `text_secondary` and `text_muted`. Test with macOS accessibility inspector.
**Complexity:** S
**Files:** `theme.rs`

### P0 Features

#### 1.4 Search Toggle Buttons (Case / Whole Word / Regex)
**Status:** Backend supports all three (`ChannelQuery` fields, `engine.rs` branching) but was previously hardcoded to `false`. Toggle buttons now exist in the UI.
**Work:** Verify toggle state is correctly wired through to `ChannelQuery` in `app.rs`. Ensure visual feedback (active/inactive styling) is clear. Add keyboard shortcuts: `Alt+C` (case), `Alt+W` (whole word), `Alt+R` (regex) — matching VS Code's shortcuts.
**Complexity:** S
**Files:** `search_panel.rs`, `app.rs`

#### 1.5 Keyboard Shortcuts ✅
**Status:** Implemented — Cmd+O, Cmd+W, Cmd+F, Cmd+Q, Alt+C, Alt+W, Alt+R, Up/Down arrows for result navigation, Cmd+Shift+Enter to open in editor, Escape to clear search. Uses GPUI `actions!()` macro, `KeyBinding::new()`, `key_context("surch")`, and `on_action(cx.listener())`.
**Work:** Register via GPUI `actions!()` macro and key bindings:

| Shortcut | Action |
|---|---|
| `Cmd+O` | Open folder |
| `Cmd+F` | Focus find input |
| `Cmd+H` | Focus replace input (toggle replace row visible) |
| `Escape` | Clear search / close replace / return focus |
| `Up/Down` | Navigate results list |
| `Enter` | Select highlighted result (open in preview) |
| `Cmd+Shift+Enter` | Open selected result in editor |
| `Cmd+W` | Close project (return to welcome screen) |
| `Cmd+Q` | Quit app |
| `Alt+C` | Toggle case sensitive |
| `Alt+W` | Toggle whole word |
| `Alt+R` | Toggle regex |

**Complexity:** M
**Files:** `app.rs`, `main.rs`, `search_panel.rs`

#### 1.6 Refresh Search Button ✅
**Status:** Implemented — refresh icon in toolbar, deferred click handler to avoid GPUI crash.
**UX Behavior:** A refresh icon button (circular arrow) in the search results header toolbar, next to the status text. Clicking it re-executes the current search query with the current input values and toggle states. This is useful when files on disk have changed since the last search (e.g., after a `git pull`, build step, or external edit). The button should be disabled (grayed out) when no search has been run yet or when a search is currently in progress. While re-running, the existing results are cleared and replaced with fresh results.
**Implementation:**
- Add a refresh button to the search panel header row (next to "SEARCH" title or in a toolbar row below it).
- On click, fire the `on_query_changed` callback with the current input values, which triggers the existing debounced search flow. Skip the debounce for explicit refresh — execute immediately.
- Store the "last executed query" in `SurchApp` so refresh can re-run even if inputs haven't changed (the current debounce logic skips if `pending_query` is `None`).
**Complexity:** S
**Files:** `search_panel.rs`, `app.rs`

#### 1.7 Collapse All / Expand All Button ✅
**Status:** Implemented — toggle button in toolbar, deferred click handler.
**UX Behavior:** A button in the search results toolbar (double-chevron icon, like `>>` or VS Code's collapse-all icon). Clicking it collapses every file group so only file names and match counts are visible. Clicking again (or a separate expand-all button) expands all groups. This is essential when searching produces hundreds of file groups — users want to scan file names first, then drill into specific files.
**Implementation:**
- Add a `collapse_all()` and `expand_all()` method to `SearchPanel` that iterates `file_groups` and sets `collapsed = true/false` on each.
- Add a toggle button to the toolbar. Track an `all_collapsed: bool` state to toggle between collapse/expand behavior.
- Keyboard shortcut: `Cmd+Shift+[` to collapse all.
**Complexity:** S
**Files:** `search_panel.rs`

---

## Milestone 2: Beta (Polish & Replace)

Make the app feel professional. Implement the full replace workflow.

### P1 Features

#### 2.1 Replace All Button ✅
**Status:** Implemented — Replace All icon button next to replace input, calls `engine::run_replace()` on background thread, auto-refreshes search after completion.
**UX Behavior:** A button (icon: double-page replace icon, or text "Replace All") next to the replace input field. Clicking it replaces every match across every file in the current result set with the replacement text. Before executing:
- Show a confirmation dialog: "Replace N occurrences across M files?" with Cancel / Replace buttons.
- Replacements are performed file-by-file on a background thread, streaming progress events back to the UI.
- After completion, show a status message: "Replaced N occurrences in M files."
- Automatically re-run the search to update results (replaced matches should disappear).

**Edge cases:**
- Files that are read-only or locked: skip and report in a summary.
- Binary files: never replace in binary files.
- Concurrent modification: if a file changed on disk between search and replace, warn or skip.

**Implementation:**
- Add a `replace_all` method to the `Channel` trait in `surch-core` that takes a query + replacement string + list of result entries.
- Implement in `FileSearchChannel`: for each file, read contents, perform replacements at the exact byte offsets from match_ranges, write back.
- Critical: process replacements in reverse offset order within each file to avoid invalidating subsequent byte offsets.
- Stream `ReplaceEvent` (similar to `SearchEvent`) back to UI for progress tracking.
**Complexity:** L
**Dependencies:** Match highlighting (done), replace input field (exists)
**Files:** `channel.rs`, `surch-file-search/src/lib.rs`, `app.rs`, `search_panel.rs`

#### 2.2 Replace: Preserve Case Toggle ✅
**Status:** Implemented — `apply_case_pattern()` utility in `surch-core/channel.rs`, `preserve_case` field on `ChannelQuery`, wired into `run_replace()` engine. UI toggle button still needed.
**UX Behavior:** A toggle button (AB icon with a small case indicator) on the replace input row, next to the replace field. When enabled, the replacement text automatically adapts to match the case pattern of each individual match. The rules (matching VS Code's behavior):

| Original match | Replacement text | Result |
|---|---|---|
| `foo` (all lower) | `bar` | `bar` |
| `FOO` (all upper) | `bar` | `BAR` |
| `Foo` (title case) | `bar` | `Bar` |
| `fooBar` (camelCase) | `bazQux` | `bazQux` (no change — mixed case passes through) |

Only three case patterns are detected and preserved: all-lowercase, all-uppercase, and title-case (first letter upper, rest lower). Mixed-case originals use the replacement text as-is.

**Implementation:**
- Add a `preserve_case: bool` field to the replace/query options.
- Implement a `apply_case_pattern(original: &str, replacement: &str) -> String` utility function in `surch-core`.
- Apply this function per-match during replace-all and single-replace operations.
**Complexity:** M
**Dependencies:** Replace All (2.1)
**Files:** `surch-core/src/channel.rs` (or new `replace.rs`), `search_panel.rs`

#### 2.3 View as Tree Toggle ✅
**Status:** Implemented — `ToggleViewMode` action with `Cmd+Shift+T` shortcut. Flat list and tree view modes using `path_trie` module in `surch-core`. Tree view shows collapsible directory nodes with aggregate match counts. View > Toggle Tree View menu item.

#### 2.4 Panel Resizing (Draggable Divider) ✅
**Status:** Implemented — draggable 4px divider between search and preview panels with `CursorStyle::ResizeLeftRight`. Min 200px, max 600px. Accent-colored on hover/drag.
**UX Behavior:** A draggable vertical divider between the search results panel and preview panel. Users can drag left/right to resize both panels. The cursor changes to a resize cursor on hover. Minimum widths should be enforced (e.g., 200px for search, 300px for preview) to prevent either panel from collapsing completely.
**Implementation:**
- Replace the fixed `w(px(340.0))` on the search panel with a dynamic width stored in `SurchApp` state.
- Render a 4-6px invisible drag handle div between the two panels. On mousedown + drag, update the search panel width.
- Persist the user's preferred width in config (`~/.config/surch/`).
**Complexity:** M
**Files:** `app.rs`, `search_panel.rs`, `surch-core/src/config.rs`

#### 2.5 Search Result Text Truncation ✅
**Status:** Implemented — leading whitespace trimmed with adjusted match ranges, `text_ellipsis()` for overflow.
**UX Behavior:** Search result lines in the results list should:
- **Left-trim leading whitespace** to show the relevant content, not deep indentation that wastes horizontal space.
- **Truncate with ellipsis** (`...`) when the line exceeds the panel width, rather than hard-clipping mid-character.
- Cursor/VS Code does this — result lines always show the meaningful part of the match, regardless of indentation depth.
**Implementation:**
- In the result row render, trim leading whitespace from `line_content` before display. Adjust `match_ranges` byte offsets accordingly.
- Apply `.text_ellipsis()` or equivalent on the line container to truncate with `...`.
**Complexity:** S
**Files:** `search_panel.rs`

#### 2.6 UI Polish Pass ✅
**Status:** Implemented — hover states on all interactive elements (sidebar icons, toggle buttons, welcome screen button, recent workspaces), spacing snapped to 4px grid, spacing scale constants added to theme.rs.
Rework theme colors, spacing, typography, hover states. Key changes:
- **Hover states:** Ensure all interactive rows have `.hover()` styling.
- **Spacing:** Consistent padding and margins throughout.
- **Typography:** SF Mono for code, system font for UI, proper weight differentiation.
- **Welcome screen:** Better visual hierarchy, centered layout.
- **Sidebar:** Active indicator as 2px left accent bar.
- **Toolbar:** Consistent icon button styling for all new toolbar buttons (Refresh, Collapse All, View as Tree, etc.).
**Complexity:** M
**Files:** `theme.rs`, `search_panel.rs`, `preview_panel.rs`, `sidebar.rs`, `app.rs`

#### 2.7 Menu Bar ✅
**Status:** Implemented — native macOS menu bar with surch/File/Edit/Find menus. FocusHandle on root div ensures menu items are never greyed out.
**Work:** Native macOS menu bar via GPUI:

| Menu | Items |
|---|---|
| **File** | Open Folder (`Cmd+O`), Close Project (`Cmd+W`), Quit (`Cmd+Q`) |
| **Edit** | Cut, Copy, Paste, Select All, Find (`Cmd+F`) |
| **View** | Toggle Sidebar, Toggle Replace, Collapse All Results, Word Wrap, Zoom In/Out/Reset |
| **Go** | Go to Line (`Cmd+G`) |
| **Help** | About Surch |

**Complexity:** M
**Files:** `main.rs`, `app.rs`

#### 2.8 Close Project ✅
**Status:** Implemented — X button in search panel header, Cmd+W shortcut, File > Close Project menu item. Saves workspace state before closing. Uses deferred click handler to avoid GPUI crash.

#### 2.9 History / Recently Opened ✅
**Status:** Implemented. Two-tier persistence:
- **Global** (`~/.config/surch/config.toml`): Recent workspaces with timestamps (max 10). Displayed on welcome screen with folder icon, name, and `~/`-shortened path. Click to reopen.
- **Per-workspace** (`~/.config/surch/workspaces/{hash}/state.json`): Search/replace/filter history (max 20), last used search options (case sensitive, whole word, regex). Saved on close, restored on open.

**TODO:** Add "x" button to remove individual entries, "Clear Recent" link, relative timestamps ("2 days ago"), validate paths exist on disk.

#### 2.10 Sidebar Icons ✅
**Status:** Implemented — `icon_for_channel()` maps channel IDs to `IconName` variants. Active channel has 2px accent bar indicator + background highlight.
**Work:** Use proper icons for channel sidebar. `ChannelMetadata.icon` field exists but is ignored. Use GPUI `IconName` variants (e.g., `IconName::Search` for file search).
**Complexity:** S
**Files:** `sidebar.rs`, `surch-file-search/src/lib.rs`

#### 2.11 Fuzzy Finding (Cross-Channel) ✅
**Status:** Implemented — "Fz" toggle button in search toolbar (Alt+F shortcut). Uses `nucleo-matcher` crate (same engine as Helix editor). Characters match in order but not contiguously, with matched ranges highlighted. Mutually exclusive with regex mode. Fuzzy state persists per-workspace. Shared `fuzzy_match()` utility in `surch-core/src/fuzzy.rs` for use by future channels.
**UX Behavior:** A toggleable "Fuzzy" mode (button in the search toolbar, next to Case/Whole Word/Regex toggles). When enabled, the search query is matched fuzzily — characters must appear in order but not contiguously (e.g., `srchpnl` matches `search_panel`). Matched characters are highlighted individually in the result. Fuzzy mode is mutually exclusive with Regex mode (regex requires exact pattern syntax).
**Why cross-channel:** Fuzzy finding is useful for every channel — file content, filesystem names, git messages, pod names, etc. It should live in `surch-core` as a shared capability, not be reimplemented per channel.
**Implementation:**
- Add a `fuzzy: bool` field to `ChannelQuery` in `surch-core/src/channel.rs`.
- Implement a `fuzzy_match(query: &str, text: &str) -> Option<(f64, Vec<usize>)>` function in `surch-core` that returns a relevance score and matched character indices. Use the `nucleo` or `fuzzy-matcher` crate (both are battle-tested — nucleo is what Helix uses, fuzzy-matcher is what fzf-rs uses).
- Each channel's `search()` can call into this shared fuzzy matcher when `query.fuzzy` is true.
- Results should be ranked by fuzzy score (best match first), not file order.
- Toggle button in the search panel toolbar: `Fz` or a tilde `~` icon.
- Keyboard shortcut: `Alt+Z` to toggle fuzzy mode.
**Complexity:** M
**Files:** `surch-core/src/channel.rs`, new `surch-core/src/fuzzy.rs`, `search_panel.rs`, `surch-file-search/src/engine.rs`

#### 2.11b Split "Open In" Button ✅
**Status:** Implemented — split button with left half showing preferred editor name for one-click open, right chevron half for dropdown to pick a different editor. Remembers last-used editor in `config.toml`. Falls back to simple button when only one editor is available.
**UX Behavior:** Replace the single "Open in..." button with a **split button** (dual button) like Cursor's preview panel:
- **Left half (major):** Shows the default/last-used editor name (e.g., "Cursor") and opens directly on click — no dropdown.
- **Right half (chevron):** Small dropdown chevron that opens the full editor menu on click.

This gives one-click access to the preferred editor while still allowing switching. The last-used editor choice should be persisted in config.

**Implementation:**
- Render two adjacent divs styled as a single pill-shaped button with a divider line between them.
- Store `preferred_editor: Option<String>` in `AppConfig` — set when user picks from the dropdown.
- Left half calls `execute_action(preferred_editor_id)` directly; right half toggles the dropdown menu.
- First time (no preference): behave like current single button.
**Complexity:** M
**Dependencies:** Editor auto-detection (done)
**Files:** `preview_panel.rs`, `config.rs`

#### 2.12 Replace Preview (Inline Diff) ✅
**Status:** Implemented — when replace input has text, match rows show strikethrough old text (red bg) + replacement text (green bg) inline. Uses `line_through()` styling.
**UX Behavior:** When a replacement string is entered, each match line in the results list shows a preview of the replacement. The original matched text is shown with strikethrough and a red-tinted background, and the replacement text is shown with a green-tinted background immediately after it. This gives users confidence about what will change before they click Replace All.
**Implementation:**
- In `render_highlighted_line()`, when a replace value is present, render each match span as: `[strikethrough old text] [green new text]` instead of just `[highlighted old text]`.
- The replace value comes from the "replace" input field, passed down to the render function.
**Complexity:** M
**Dependencies:** Replace All (2.1)
**Files:** `search_panel.rs`

#### 2.13 Theming System + Monokai Pro Theme
**Status:** Not implemented. Currently hardcoded to One Dark-inspired colors in `theme.rs` (app UI) and `HighlightThemeStyle` (syntax highlighting via tree-sitter).
**UX Behavior:** Surch should support multiple color themes, switchable at runtime (like Zed's theme system). The default theme should be **Monokai Pro** — a professional, high-contrast dark theme widely regarded as best-in-class for code readability.
**Implementation:**
- Define a `Theme` trait or struct in `surch-core` that contains all color tokens (bg_primary, text_primary, accent, match_bg, etc.).
- Create theme files as structured data (TOML, JSON, or Rust structs). Start with two themes: **Monokai Pro** (default) and **One Dark** (current).
- `SurchTheme` methods become dynamic, reading from the active theme instead of returning hardcoded values.
- Each theme needs a corresponding `HighlightThemeStyle` JSON definition for tree-sitter syntax highlighting colors (set via `Theme::global_mut(cx).highlight_theme`).
- Theme switching should be instant — update the active theme reference and call `cx.notify()` to re-render everything.
- Store the user's theme preference in `config.toml`.

**Monokai Pro palette reference:**
| Token | Color | Hex |
|---|---|---|
| Background | Dark charcoal | `#2d2a2e` |
| Surface | Slightly lighter | `#403e41` |
| Text | Warm white | `#fcfcfa` |
| Comment/muted | Grey | `#727072` |
| Accent (yellow) | Gold | `#ffd866` |
| String (green) | Lime | `#a9dc76` |
| Keyword (pink) | Magenta | `#ff6188` |
| Function (blue) | Sky | `#78dce8` |
| Number (purple) | Violet | `#ab9df2` |
| Type (orange) | Orange | `#fc9867` |

**Complexity:** L
**Dependencies:** Settings UI (4.1) for theme picker, or standalone menu item
**Files:** `theme.rs` (refactor to trait/struct), new `themes/` module, `config.rs`, `main.rs` (highlight theme)

#### 2.14 Custom Themed Title Bar ✅
**Status:** Implemented — transparent title bar with custom-drawn title div matching the app theme. Uses `appears_transparent: true` + `traffic_light_position` for macOS traffic lights. Shows workspace name when a folder is open.
**UX Behavior:** Replace the native macOS title bar with a custom-drawn title bar that matches the app's theme, similar to how Zed renders its title bar. The title bar should:
- Match the app's background color (e.g., Monokai Pro's dark charcoal)
- Show the workspace name / folder name as the window title
- Include the native macOS traffic light buttons (close/minimize/fullscreen) — these can be embedded in a custom title bar via `TitlebarOptions { appears_transparent: true }`
- Optional: show breadcrumb-style path to currently previewed file

**Implementation:**
- Set `TitlebarOptions { appears_transparent: true, traffic_light_position: Some(point(px(8.0), px(8.0))) }` to get transparent title bar with positioned traffic lights.
- Render a custom title bar div at the top of the root view that blends seamlessly with the sidebar/search panel.
- The title bar height should be ~28-32px (matching macOS standard).
**Complexity:** M
**Files:** `main.rs` (TitlebarOptions), `app.rs` (render custom title bar div)

#### 2.15 Preview Pane Zoom (Font Size +/-) ✅
**Status:** Implemented — Cmd+=/Cmd+-/Cmd+0 for zoom in/out/reset. Font size range 8-32px in 2px steps. View menu items. Uses `refine_style()` on the Input component to override both `text_size` and `line_height` (1.5x font size) so both scale together with zoom.

#### 2.16 Word Wrap Toggle ✅
**Status:** Implemented — `ToggleWordWrap` action with `Alt+Z` shortcut (matches VS Code). View > Word Wrap menu item. Uses `InputState::set_soft_wrap()` on the CodeEditor.

#### 2.17 Go to Line ✅
**Status:** Implemented — Cmd+G opens floating input overlay on preview pane. Type line number, press Enter to jump. Go menu in menu bar.
**UX Behavior:** `Cmd+G` opens a small input overlay (like VS Code's "Go to Line" dialog) in the preview pane. User types a line number, presses Enter, and the preview scrolls to that line and highlights it. Escape dismisses the overlay.
**Implementation:**
- Add a `GoToLine` action with `Cmd+G` keybinding.
- Render a floating input div at the top of the preview pane when active.
- On submit, call `scroll_to_item(line_number - 1)` on the preview's `uniform_list` scroll handle.
**Complexity:** S
**Files:** `preview_panel.rs`, `app.rs`, `main.rs`

#### 2.18 Find in Preview (Cmd+F in Preview Pane) ✅
**Status:** Implemented — provided by gpui-component's CodeEditor built-in search (`.searchable(true)`). Supports incremental highlighting, match count, Enter/Shift+Enter to cycle, Escape to dismiss, case/regex toggles.
**Known gap:** Cycling through find results doesn't center them in the editor viewport — matches near edges can be hard to see.

#### 2.19 Editor Configuration (View Menu) ✅
**Status:** Implemented — View menu exposes Word Wrap (`Alt+Z`), Line Numbers, and Indent Guides toggles. Each calls the corresponding `InputState` method (`set_soft_wrap`, `set_line_number`, `set_indent_guides`) on the CodeEditor. Zoom (Cmd+=/Cmd+-/Cmd+0) scales both font size and line height via `refine_style()`.

#### 2.20 Design System & Consistency
**Status:** No formal design system. Colors, spacing, typography, and component patterns are ad-hoc.
**Research needed:** Audit the current UI against best practices from Zed, VS Code, and Figma's design systems. Establish:
- **Spacing scale** — consistent 4px grid (4, 8, 12, 16, 24, 32, 48px)
- **Typography scale** — defined sizes for headings, body, caption, code (e.g., 10, 11, 12, 13, 14, 16, 20px)
- **Component library** — standardized button, icon button, input, toggle, badge, tooltip, dropdown, overlay components with consistent sizing, padding, border radius, and hover/active/disabled states
- **Color token naming** — semantic naming (e.g., `surface.primary`, `text.default`, `border.subtle`) instead of implementation-specific names
- **Icon sizing** — standardized icon sizes (12, 14, 16, 20, 24px) that align with text sizes
- **Animation/transition** — hover transition timing, fade durations

The design system should be documented and enforced through a component module in `surch-app/src/components/`.
**Complexity:** L
**Files:** New `components/` module, `theme.rs` (refactor), documentation

---

## Milestone 3: v1.0 Release

Ship it. Testing, packaging, branding.

### P1 Features

#### 3.1 Test Suite ✅
**Status:** 130 tests, 84.6% coverage across `surch-core` (79 tests) and `surch-file-search` (51 tests). Covers:
- Search engine: literal, regex, case sensitivity, whole word, glob include/exclude, cancellation, .gitignore, unicode
- Replace logic: basic, case-insensitive, preserve case, multi-file, globs, trailing newline preservation
- Config: round-trip serialization, workspace state persistence, recent workspaces, history management
- Channel trait: metadata, input fields, actions, query fields, result entries, search events
- Path trie: tree building, nesting, match counts
- Registry: registration, active switching
- Editor detection: always includes Reveal in Finder, Finder is last

**Remaining gaps:** Editor auto-discovery internals (depends on installed apps, ~64.7% coverage on `lib.rs`). `surch-app` has no tests (GPUI requires GPU context).

#### 3.2 App Logo & Icon
**Status:** No app icon.
**Work:** Design a logo (magnifying glass + "S" motif, matching the dark theme aesthetic). Export as `.icns` for macOS app bundle. Set in build config / `Info.plist`.
**Complexity:** S

#### 3.3 Release Pipeline (GitHub Actions)
**CI/CD:**
- On PR: `cargo build`, `cargo test`, `cargo clippy`
- On tag push (`v*`): build release binaries, create GitHub Release, upload artifacts
- Build matrix: `aarch64-apple-darwin` (Apple Silicon), `x86_64-apple-darwin` (Intel)
- Package as `.app` bundle with `cargo-bundle` or manual `Info.plist` + `icns`
- DMG installer via `create-dmg` for drag-to-Applications UX
- Optional: code-sign with Apple Developer ID for Gatekeeper

**Distribution format:**
```
surch-v1.0.0-macos-arm64.dmg
surch-v1.0.0-macos-x86_64.dmg
```
**Complexity:** M

#### 3.4 GitHub Pages Website
Landing page at surch.dev or via GH Pages:
- Hero section with app screenshot
- Feature list with icons
- Download links (DMG for arm64 / x86_64)
- Getting started guide
- Link to GitHub repo
**Complexity:** M

---

## Milestone 4: Post-v1.0

Future improvements and new capabilities.

### P2 Features

#### 4.1 Settings UI
Settings panel accessible from sidebar or menu bar. Must be configurable on the fly (changes take effect immediately, no restart). Key settings:
- **Default editor selection** — dropdown of detected editors
- **Theme selection** — switch between themes (Monokai Pro, One Dark, etc.)
- **Font size** — preview pane and result list font sizes
- **Tab size** — spaces per tab for preview rendering
- **Word wrap** — toggle for preview pane
- **Excluded directories** — global excludes beyond .gitignore
- **Max results limit** — cap on search results
- **Debounce delay** — ms to wait before auto-searching
- **Recent files limit** — max recent workspaces on welcome screen

Config infra already exists in `surch-core/src/config.rs`. Settings changes should persist to disk immediately.
**Complexity:** L

#### 4.2 Multi-root Workspace
Open multiple folders simultaneously. Results are grouped by workspace root. Each root shows as a top-level section in the tree view. Welcome screen allows adding multiple folders.
**Complexity:** L

#### 4.3 Search History
Dropdown on the find input showing the last 20 search queries. Press `Down` when the find input is focused and empty to show history. History persists across sessions (stored in per-workspace `state.json` — infrastructure already in place via `WorkspaceState.search_history`).
**Complexity:** M

#### 4.4 New Channels
The channel architecture is designed for extensibility. Future channels:
- **Filesystem Search** — search files and folders by name (fuzzy match, like Cmd+P in VS Code but as a full channel). Results show matching file/folder paths. Preview shows file content for text files, directory listing for folders, metadata (size, modified date) for all. Replace = rename files/folders (e.g., find `Button` → rename to `PrimaryButton`). Actions: Open, Rename, Delete, Reveal in Finder, Copy Path. Uses the same `ignore` crate for .gitignore-aware traversal.
- **Git Search** — search through git log messages, diffs, blame
- **Symbol Search** — search for function/class/type definitions using tree-sitter
- **Kubernetes** — search across pod logs, config maps, secrets, events
- **Browser Bookmarks** — search Chrome/Safari bookmarks and history
- **Docker / Container Logs** — search across running container logs, filter by container name or image
- **Environment & Dotfiles** — search shell history, environment variables, dotfiles, and config directories (`~/.config`, `~/.ssh`, etc.)
- **Documentation Search** — search local man pages, tldr pages, or project-local docs
- **Process Search** — search running processes, open ports, listening sockets
- **Notes & Scratch** — full-text search across markdown notes, scratch files, clipboard history
- **Cross-Channel Search** — a meta-channel that searches across all active channels simultaneously, ranking results by relevance regardless of source

Each channel implements the `Channel` trait and gets its own sidebar icon, input fields, and result format.
**Complexity:** XL (per channel)

#### 4.5 Tree-sitter Syntax Highlighting ✅ (partial)
**Status:** Implemented — preview panel uses gpui-component's CodeEditor which provides tree-sitter syntax highlighting via `InputState::code_editor()`. One Dark highlight theme configured via `HighlightThemeStyle`. TSX highlighting fixed by re-registering with full TypeScript highlights query.
**Remaining:** More file types need tree-sitter grammar support. Currently supports ~30 extensions via `language_for_path()` in `preview_panel.rs`, but some languages may have incomplete or missing highlights queries in gpui-component. Foundation exists for Symbol Search channel (tree-sitter AST).

#### 4.6 File Search: Context Lines
Show N lines of context above/below each match (like `grep -C`). Toggle in settings or via a button. Context lines are rendered in a muted style, non-clickable.
**Complexity:** M

#### 4.7 Find and Replace in Single File
**Status:** Find is done (CodeEditor built-in Cmd+F). Replace within a single file is not yet implemented — the CodeEditor runs in read-only mode (`.disabled(true)`).
**Complexity:** M

#### 4.8 Text Selection, Copy, and Find-in-Preview ✅
**Status:** Implemented — all provided by gpui-component's CodeEditor in read-only mode (`.disabled(true)`). Text selection via click/drag, Cmd+A select all, Cmd+C copy, Cmd+F find-in-preview with incremental highlighting, match count, and navigation.

---

## Implementation Order

```
Phase 1 — Fix what's broken (Alpha):                    ALL DONE ✅
  P0 Bug: Fix scroll performance              ✅    (M)
  P0 Bug: Fix syntax highlighting             ✅    (S)
  P0 Bug: Fix color accessibility             ✅    (S)
  Search toggle buttons                       ✅    (S)
  Refresh Search button                       ✅    (S)
  Collapse All / Expand All                   ✅    (S)
  Keyboard shortcuts                          ✅    (M)

Phase 2 — Replace workflow + polish (Beta):
  Replace All button                          ✅    (L)
  Replace: Preserve Case                      ✅    (M)
  Replace Preview (inline diff)               ✅    (M)
  Panel resizing (draggable divider)          ✅    (M)
  Search result text truncation               ✅    (S)
  View as Tree toggle                         ✅    (L)
  History / Recently Opened                   ✅    (M)
  Close Project                               ✅    (S)
  Menu bar                                    ✅    (M)
  Sidebar icons                               ✅    (S)
  Custom themed title bar                     ✅    (M)
  Preview pane zoom (Cmd+/Cmd-)               ✅    (S)
  Word wrap toggle                            ✅    (M)
  Go to line (Cmd+G)                          ✅    (S)
  Find in preview (Cmd+F)                     ✅    (M)
  Line numbers toggle                         ✅    (S)  — NEW
  Indent guides toggle                        ✅    (S)  — NEW
  Editor configuration (View menu)            ✅    (S)  — NEW
  Tree-sitter syntax highlighting             ✅    (L)  — moved from Phase 4
  Text selection, copy, find-in-preview       ✅    (L)  — moved from Phase 4
  Fuzzy finding (cross-channel)               ✅    (M)
  UI polish pass                              ✅    (M)
  Split "Open In" button                      ✅    (M)
  -- deferred to post-v1.0 --
  Theming system + Monokai Pro                      (L)
  Design system & consistency                       (L)

Phase 3 — Ship v1.0:
  Test suite                                  ✅    (L)  — 147 tests, 84.6% coverage
  App logo & icon                                   (S)
  Release pipeline                                  (M)
  GitHub Pages website                              (M)

Phase 4 — Post-launch:
  Theming system + Monokai Pro                      (L)  — moved from Phase 2
  Design system & consistency                       (L)  — moved from Phase 2
  Settings UI                                       (L)
  Multi-root workspace                              (L)
  Search history                                    (M)
  Context lines                                     (M)
  New channels                                      (XL)
```

---

## Sizing Guide

| Size | Effort | Description |
|---|---|---|
| **S** | < 1 day | Single file change, well-understood scope |
| **M** | 1-3 days | Multiple files, some design decisions |
| **L** | 3-7 days | Significant feature, new data structures or UI components |
| **XL** | 1-2 weeks | Major feature spanning multiple crates |

---

## Release Strategy

**Current stage: Beta (complete)** — all prioritized Beta items done. Theming and design system deferred to post-v1.0. Moving to Phase 3 (v1.0 release).

### Alpha Exit Criteria ✅ ALL MET
- All P0 bugs fixed (scroll, syntax highlighting, accessibility)
- Search toggles working end-to-end
- Keyboard shortcuts for core actions
- Refresh Search and Collapse All buttons

### Beta Entry Criteria ✅ ALL MET
- Full replace workflow (Replace All + Preserve Case + inline preview)
- View as Tree toggle
- Recently opened folders on welcome screen
- Menu bar with standard macOS menus
- UI polish pass — ongoing

### v1.0 Release Criteria
- Test suite with >80% coverage on core search/replace logic
- App icon and branding
- Release pipeline producing signed DMGs
- Landing page with download links
- No known P0 or P1 bugs

### Distribution Format
```
surch-v1.0.0-macos-arm64.dmg    # Apple Silicon
surch-v1.0.0-macos-x86_64.dmg   # Intel
```

### CI/CD (GitHub Actions)
- On tag push (`v*`): build release binaries, create GitHub Release, upload DMGs
- On PR: `cargo build` + `cargo test` + `cargo clippy`

---

## Design Spec Reference

### Search Panel Toolbar Layout

The search panel header gets a toolbar row with action buttons, matching VS Code/Cursor's layout:

```
+------------------------------------------+
| SEARCH                                    |
+------------------------------------------+
| Find:  [________________] [Aa] [Ab] [.*] |
| Replace: [______________] [AB]           |
+------------------------------------------+
| 42 results in 8 files                     |
| [Refresh] [Collapse All] [Tree/List] [Clear] |
+------------------------------------------+
| v src/app.rs  (5)                         |
|   12: let query = ...                     |
|   45: fn search(...) {                    |
| v src/main.rs  (2)                        |
|   3: use app::search;                     |
+------------------------------------------+
```

### Color Palette (One Dark-inspired)

| Token | HSLA | Use |
|---|---|---|
| bg_base | `hsla(0.63, 0.13, 0.09, 1.0)` | Sidebar |
| bg_primary | `hsla(0.63, 0.13, 0.11, 1.0)` | Preview pane |
| bg_secondary | `hsla(0.63, 0.13, 0.14, 1.0)` | Search panel |
| bg_surface | `hsla(0.63, 0.13, 0.17, 1.0)` | Inputs, file headers |
| bg_hover | `hsla(0.63, 0.10, 0.20, 1.0)` | Hover state |
| bg_selected | `hsla(0.58, 0.25, 0.18, 1.0)` | Selected row |
| text_primary | `hsla(0.58, 0.10, 0.85, 1.0)` | Body text |
| text_heading | `hsla(0.58, 0.10, 0.95, 1.0)` | File names, headings |
| text_secondary | `hsla(0.58, 0.08, 0.55, 1.0)` | Labels, line numbers (bumped for accessibility) |
| text_muted | `hsla(0.58, 0.05, 0.42, 1.0)` | Placeholders, disabled (bumped for accessibility) |
| accent | `hsla(0.58, 0.60, 0.55, 1.0)` | Buttons, active indicators |
| match_bg | `hsla(0.10, 0.70, 0.35, 0.45)` | Match highlight bg |
| match_text | `hsla(0.10, 0.90, 0.70, 1.0)` | Match highlight fg |
| replace_old_bg | `hsla(0.0, 0.60, 0.30, 0.40)` | Strikethrough replaced text bg |
| replace_new_bg | `hsla(0.35, 0.60, 0.30, 0.40)` | New replacement text bg |

### Typography
- UI text: System font (SF Pro via `".SystemFont"`)
- Code (preview): `"Menlo"` at configurable size (default 14px, zoom via Cmd+/-)
- Code (search results): 12px, 4px vertical padding
- Line numbers: rendered by CodeEditor, scale with zoom
- File names: 12px semibold

### Key UI Patterns
- Hover states: `.hover(|s| s.bg(bg_hover))` on all interactive rows
- Selected state: `bg_selected` + 2px left accent border
- Active sidebar indicator: 2px left bar in `accent` color
- Match highlighting: Split line at `match_ranges`, wrap matches in `match_bg`/`match_text`
- Replace preview: Strikethrough original in `replace_old_bg`, new text in `replace_new_bg`
- Toolbar buttons: 22x22px, rounded(3px), hover bg, icon centered, tooltip on hover
