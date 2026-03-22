# surch

A standalone, lightweight desktop search tool built in Rust with [GPUI](https://gpui.rs) (Zed's GPU-accelerated UI framework). Replicates VS Code's search experience — without the editor.

![macOS](https://img.shields.io/badge/platform-macOS-lightgrey)

## Why

Search is one of the most important things a developer does, but it's trapped inside editors. You open VS Code not because you want to edit — you open it because you need to *find* something. That's a lot of overhead for a search box.

And search itself is under-explored. VS Code gives you find-in-files and that's about it. But developers search for all kinds of things — files, git branches, running pods, environment variables, logs, open PRs. Each of these has its own tool, its own CLI, its own mental context switch. There's no single place to search across everything in your workflow.

**Surch is that place.**

The vision: a fast, native search app where every *kind* of search is a pluggable channel. V1 ships with file content search — the thing you'd open VS Code for. But the architecture is built so that anyone can write a channel that searches anything — Kubernetes pods, Docker containers, git worktrees, AI agents, database records — with its own filters, its own preview pane, and its own actions. One app, one muscle memory, infinite search surfaces.

Think of it like [television](https://github.com/alexpasmantier/television) meets VS Code's search panel — the extensibility of a plugin system with the polish of a native desktop app, GPU-rendered at 120fps.

## Features

- **Find in files** — search text across an entire folder, powered by ripgrep's library crates
- **Results grouped by file** — collapsible file headers with match counts
- **File preview** — click a result to see the file with the matched line highlighted
- **Include/Exclude globs** — filter which files to search (e.g. `*.rs`, `!tests/`)
- **Open in editor** — open results directly in Cursor, VS Code, Zed, Sublime, Vim, or Neovim
- **Debounced live search** — results stream in as you type

## Architecture

Surch is built as an extensible search platform. The V1 searches file contents, but the architecture supports pluggable **channels** — each channel defines its own inputs, search logic, preview content, and actions.

```
surch/
├── crates/
│   ├── surch-app/             # GPUI desktop application
│   ├── surch-core/            # Channel trait & registry (framework-agnostic)
│   └── surch-file-search/     # Built-in file search channel
```

### The Channel Trait

Every search mode implements the `Channel` trait:

```rust
pub trait Channel: Send + Sync {
    fn metadata(&self) -> ChannelMetadata;
    fn input_fields(&self) -> Vec<InputFieldSpec>;
    fn search(&self, query: ChannelQuery, tx: Sender<SearchEvent>);
    fn cancel(&self);
    fn preview(&self, entry: &ResultEntry) -> PreviewContent;
    fn actions(&self, entry: &ResultEntry) -> Vec<ChannelAction>;
    fn execute_action(&self, action_id: &str, entry: &ResultEntry) -> Result<()>;
}
```

Future channels could search Kubernetes pods, git branches, running processes, AI agents — anything. Each channel gets its own sidebar icon, custom filter inputs, and context-specific actions.

## Prerequisites

- **macOS** (Metal rendering via GPUI)
- **Xcode** (full install, not just Command Line Tools — required for Metal shader compilation)
- **Rust** (install via [rustup](https://rustup.rs))

## Install

Download the latest `.dmg` from [Releases](https://github.com/akriti2591/surch/releases), open it, and drag **Surch.app** to `/Applications`.

Since the app is not yet code-signed, macOS will block it on first launch. Run this once to fix it:

```sh
xattr -cr /Applications/Surch.app
```

Then open Surch normally. On first launch, click **Open Folder** to select a directory to search.

## Build from Source

```sh
# Clone the repo
git clone <repo-url> && cd surch

# Build and run
cargo run
```

## Key Dependencies

| Crate | Purpose |
|---|---|
| `gpui` | GPU-accelerated UI framework |
| `gpui-component` | Pre-built input, scroll, and root components |
| `grep` + `grep-regex` + `grep-searcher` | ripgrep's search engine as a library |
| `ignore` | Directory walking with `.gitignore` support |
| `crossbeam-channel` | Background search thread to UI communication |

## License

MIT
