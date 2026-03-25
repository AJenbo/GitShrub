# GitShrub 🥦

Sit back, trim your shrub, and enjoy a good overview of your project.

A lightweight Git history viewer built with Rust and egui. Think gitk, but with a modern UI that doesn't look like it escaped from 1997.

## Why?

| Tool | Problem |
|------|---------|
| gitk | Works but Tcl/Tk looks like garbage |
| Sourcetree | No Linux support |
| GitKraken | Fantastic but slow (Electron) |
| gitg | Somehow slow for a simple app |
| tig/lazygit | TUIs are not for everyone |
| git cola | Great, but relies on gitk for tree view |

GitShrub fills the gap: a fast, good-looking, lightweight commit history viewer for Linux (and beyond).

## Usage

```sh
# Show current branch history
gitshrub

# Show all branch history
gitshrub --all

# Show history for a specific file or directory
gitshrub path/to/file.rs

# Combine them
gitshrub --all path/to/file.rs
```

## Building

```sh
cargo build --release
```

The binary will be at `target/release/gitshrub`. Only runtime dependency is `git` on your PATH.

## Tech Stack

- **Rust** — Fast, single binary, strong typing
- **egui/eframe** — Immediate mode GPU-accelerated GUI
- **git CLI** — Shells out to git for all operations (no libgit2)

See [docs/architecture.md](docs/architecture.md) for design details and [docs/todo.md](docs/todo.md) for the roadmap.

## License

MIT
