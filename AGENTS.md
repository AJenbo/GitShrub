# AGENTS.md - GitShrub Development Guide

## Project Overview

GitShrub is a lightweight Git history viewer built with Rust and egui.
It shells out to the git CLI for all git operations (no libgit2).

See `docs/architecture.md` for full design details and `docs/todo.md` for the roadmap.

## Tech Stack

- **Language:** Rust (edition 2024)
- **GUI:** egui + eframe (immediate mode, GPU-accelerated)
- **Git interaction:** Subprocess calls to git CLI
- **File dialogs:** rfd (native file dialogs)

## Module Structure

- `src/main.rs` - Entry point, CLI arg parsing, eframe launch
- `src/app.rs` - Main application state and UI layout
- `src/git.rs` - All git CLI interactions (log, diff, branch, checkout, reset, etc.)
- `src/graph.rs` - Commit graph/tree rendering logic
- `src/ui/` - UI components (commit list, diff view, context menus)

## Git CLI Pattern

Use `std::process::Command` to call git. Parse stdout. Do NOT use libgit2 or git2-rs.

```rust
let output = Command::new("git")
    .args(["log", "--oneline", "-n", "50"])
    .current_dir(&repo_path)
    .output()
    .expect("failed to run git");

let stdout = String::from_utf8_lossy(&output.stdout);
```

## Building

    cargo build --release

## Code Style

- Use rustfmt defaults
- Prefer descriptive variable names
- Group imports: std, external crates, local modules
- Error handling: use `Result` where appropriate, `.expect()` for unrecoverable git failures
- Keep functions small and focused

## Terminal / Shell Notes

**IMPORTANT:** The default shell in this environment is `sh` (not bash).
When using heredocs or shell features, be aware that:
- `sh` does not support all bash features (e.g. process substitution, arrays)
- Heredocs with parentheses in content can break in `sh`
- Prefer writing content via Python scripts or direct file writes when shell quoting gets complex
- Use `python3 -c` or write a `.py` helper script for generating files with special characters