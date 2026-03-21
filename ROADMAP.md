# Surch Roadmap

## P0 — Must-Have for v1

Core gaps that make the app feel broken.

### 1. Search Toggle Buttons (Case / Whole Word / Regex)
**Status:** Backend fully supports all three (`ChannelQuery` fields, `engine.rs` branching) but `app.rs` hardcodes them to `false`.
**Work:** Add three toggle buttons (Aa, Ab, .*) to the Find input row in `search_panel.rs`. Wire toggle state into `ChannelQuery` in `app.rs`.
**Complexity:** S

### 2. Fix "Open in..." Editor Button
**Status:** Editor detection code exists in `surch-file-search/src/lib.rs` (`detect_editors` via `which`), but fails because macOS GUI apps get a minimal PATH. Also, `execute_action` uses `args.split_whitespace()` which breaks on paths with spaces.
**Work:** Use full editor paths (`/usr/local/bin/code`, `/Applications/Cursor.app/...`). Fix argument passing. Cache detected editors at startup.
**Complexity:** S

### 3. Fix Scroll/Layout Jank
**Status:** Pasting text in Replace causes the search panel to jerk — the input container and results list fight for flex space.
**Work:** Add `flex_shrink_0()` on the inputs container, stabilize layout so results area doesn't jump on input resize.
**Complexity:** S

### 4. Keyboard Shortcuts
**Status:** No shortcuts at all.
**Work:** Cmd+O (open folder), Cmd+F (focus find), Escape (clear), Up/Down (navigate results), Enter (select result). Register via GPUI `actions!()` macro and key bindings.
**Complexity:** M

---

## P1 — Should-Have for v1

Makes the app feel complete and professional.

### 5. Match Highlighting in Result Lines
**Status:** `match_ranges` data exists on every result but is ignored — full line rendered as plain text.
**Work:** Split line content at match boundaries, render matched segments with orange/amber background.
**Complexity:** S — highest visual impact for effort

### 6. UI Polish Pass
Rework theme colors, spacing, typography, hover states. Key changes:
- **Colors:** One Dark–inspired palette with better contrast (4-tier bg depth, semantic accent colors)
- **Hover states:** Add `.hover()` on result rows, file group headers, sidebar icons (currently none exist)
- **Spacing:** Double result row padding from 2px to 4px vertical, consistent 12px horizontal padding
- **Typography:** SF Mono for code, font-weight differentiation (semibold file names, medium labels)
- **Welcome screen:** Add search icon, keyboard shortcut hint, better spacing
- **Sidebar:** Active indicator as 2px left accent bar instead of background fill

**Complexity:** M

### 7. Sidebar Icons
**Status:** Shows first letter of channel name.
**Work:** Use emoji (🔍) or Unicode symbols for channel icons. `ChannelMetadata.icon` field already exists but is ignored.
**Complexity:** S

### 8. Menu Bar
**Status:** No menu bar — app feels like a toy on macOS.
**Work:** File (Open Folder, Close Project, Quit), Edit (Cut/Copy/Paste), View (Toggle Sidebar). GPUI supports native macOS menus.
**Complexity:** M

### 9. Close Project
**Status:** No way to return to welcome screen without quitting.
**Work:** Set `workspace_root = None`, clear results, reset preview. Wire to menu bar + Cmd+W.
**Complexity:** S

### 10. Syntax Highlighting in Preview
**Status:** Preview pane is monochrome text.
**Work:** Add `syntect` crate. Parse file extension → choose syntax, highlight lines, map colors to GPUI spans.
**Complexity:** L — biggest effort, biggest visual payoff

---

## P2 — Nice-to-Have / v1.1

### 11. Replace Preview
Show what each matched line would look like after replacement (strikethrough original + colored replacement). Depends on match highlighting infrastructure.
**Complexity:** L

### 12. Settings UI
Settings panel for: default editor, theme preference, excluded directories. Config infra already exists in `surch-core/src/config.rs`.
**Complexity:** L

### 13. Test Suite
Unit tests for search engine (literal, regex, case sensitivity, globs), config round-trip, editor detection. Integration tests for end-to-end search flow.
**Complexity:** M

---

## Implementation Order

```
Phase 1 — Unblock core usage (all S/M):
  ✧ Search toggles (S)
  ✧ Fix "Open in..." (S)
  ✧ Fix scroll jank (S)
  ✧ Match highlighting (S)
  ✧ Keyboard shortcuts (M)

Phase 2 — Polish for release:
  ✧ UI polish pass (M)
  ✧ Sidebar icons (S)
  ✧ Close project (S)
  ✧ Menu bar (M)
  ✧ Syntax highlighting (L)

Phase 3 — Post-launch:
  ✧ Replace preview (L)
  ✧ Settings UI (L)
  ✧ Test suite (M)
```

---

## Design Spec Reference

### Color Palette (One Dark–inspired)

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
| text_secondary | `hsla(0.58, 0.05, 0.50, 1.0)` | Labels, line numbers |
| text_muted | `hsla(0.58, 0.05, 0.35, 1.0)` | Placeholders, disabled |
| accent | `hsla(0.58, 0.60, 0.55, 1.0)` | Buttons, active indicators |
| match_bg | `hsla(0.10, 0.70, 0.35, 0.45)` | Match highlight bg |
| match_text | `hsla(0.10, 0.90, 0.70, 1.0)` | Match highlight fg |

### Typography
- UI text: System font (SF Pro via `".SystemFont"`)
- Code: `"SF Mono"` (fallback: `"Menlo"`)
- Result rows: 12px code, 4px vertical padding
- Line numbers: 11px, right-aligned, min-width 36px
- File names: 12px semibold

### Key UI Patterns
- Hover states: `.hover(|s| s.bg(bg_hover))` on all interactive rows
- Selected state: `bg_selected` + 2px left accent border
- Active sidebar indicator: 2px left bar in `accent` color
- Match highlighting: Split line at `match_ranges`, wrap matches in `match_bg`/`match_text`
