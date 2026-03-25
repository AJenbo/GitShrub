# Agent Guidelines for GitShrub

## Before You Start

This is a Rust-based Git history viewer using egui. Read these files first:

- `src/app.rs` — Main application state and UI layout
- `src/git.rs` — All git CLI interactions (log, diff, branch, checkout, reset, etc.)
- `docs/architecture.md` — GUI layout, module structure, design principles
- `docs/todo.md` — Current backlog organised by milestone

## Project Overview

GitShrub is a lightweight Git history viewer built with Rust and egui.
It shells out to the git CLI for all git operations (no libgit2).

## Tech Stack

- **Language:** Rust (edition 2024)
- **GUI:** egui + eframe (immediate mode, GPU-accelerated)
- **Git interaction:** `std::process::Command` → `git` CLI
- **File dialogs:** rfd (native file dialogs)

## Module Structure

```
src/
├── main.rs          Entry point, CLI arg parsing, eframe launch
├── app.rs           App struct, state, top-level UI layout
├── git.rs           All git CLI calls (log, diff, branch, checkout, reset, etc.)
├── graph.rs         Commit graph/tree line rendering
└── ui/
    ├── mod.rs       UI module root
    ├── commit_list.rs   Top pane: scrollable commit list with graph
    ├── commit_info.rs   Middle bar: SHA, full message
    ├── diff_view.rs     Bottom-left: unified diff display
    └── file_list.rs     Bottom-right: affected files list
```

## Git CLI Pattern

Use `std::process::Command` to call git. Parse stdout.
Do NOT use libgit2 or git2-rs.

```rust
let output = Command::new("git")
    .args(["log", "--oneline", "-n", "50"])
    .current_dir(&repo_path)
    .output()
    .expect("failed to run git");

let stdout = String::from_utf8_lossy(&output.stdout);
```

## Building and CI

Run these checks before considering any change done:

```sh
cargo build
cargo clippy -- -D warnings
cargo fmt --check
```

All three must pass with zero errors and zero warnings (except
pre-existing `dead_code` warnings for fields reserved for future
milestones like `short_sha` and `parents`).

## ⚠️ Terminal / Shell Notes ⚠️

**IMPORTANT:** The default shell in this environment is `sh` (not bash).

- `sh` does not support all bash features (process substitution, arrays, `<<<` here-strings).
- Heredocs with parentheses, backticks, or special characters in content **will break** in `sh`. This has caused repeated timeouts and failures.
- **Prefer writing content via the `edit_file` tool or Python scripts.** Do not fight `sh` with complex heredocs.
- If you must use the terminal to write files, use `python3 -c` or write a `.py` helper script. Keep shell one-liners simple.
- When using `python3 -c`, avoid triple-quoted strings containing backticks or parentheses. Use `\x60` for backticks if needed.

## Completing a Feature

### 1. Write the code

- New data structures go in `git.rs` (e.g. `Commit`, `DiffOutput`)
- Git CLI calls go in `git.rs`
- UI rendering goes in `ui/` submodules
- App state and layout orchestration goes in `app.rs`
- Keep functions small and focused

### 2. Test manually

Run `cargo run` from inside a git repository and verify the feature
works visually. Since this is a GUI app, manual verification is the
primary testing method. Automated tests for git parsing functions
are welcome but not required at this stage.

### 3. Update docs/todo.md

Mark completed items with `[x]`. Remove or update items that are
no longer relevant. The todo is a living backlog of what remains.

### 4. Run CI checks

```sh
cargo build
cargo clippy -- -D warnings
cargo fmt --check
```

## One Task at a Time

Work through milestones and tasks sequentially. Complete one task
fully (code, manual test, CI, docs) before starting the next.

Do not use sub-agents to work on multiple milestone items in
parallel. Doing so produces:

- **One large, unreviewable change** instead of focused, verifiable
  steps the user can follow and approve individually.
- **Shortcuts and incomplete work** because the orchestrator is
  juggling too many moving parts to verify each one properly.
- **A messy QA process** where broken pieces from one task get
  tangled with another, making failures hard to diagnose.

The user needs to follow what is happening, verify each change
makes sense, and confirm it works before moving on.

## Sub-Agent Guidelines

Sub-agents are useful for parallelising work **within a single
task**, not across tasks. For example, a sub-agent can update
documentation files while the orchestrator fixes code, all for
the same feature.

When spawning sub-agents:

- **Sub-agents must not run builds or CI.** No `cargo build`,
  `cargo clippy`, or `cargo fmt --check` in sub-agents. The
  orchestrating agent runs CI once after all sub-agent work is
  complete. This avoids redundant build cycles.
- **Sub-agents must not run the application.** Only the
  orchestrator runs `cargo run` for manual verification.
- **Assign non-overlapping files.** When multiple sub-agents edit
  code in parallel, each agent should own a distinct set of files.
  State which files each agent is responsible for in the spawn message.
- **Keep sub-agent scope small.** A sub-agent should do one focused
  task (e.g. "update the README and todo.md to reflect the new name").
  Broad tasks like "implement the diff view" belong with the orchestrator.

## Code Conventions

- **Use rustfmt defaults.** Do not fight the formatter.
- **No `#[allow(clippy::...)]`** unless truly necessary. Fix the warning instead.
- **Group imports:** std, external crates, local modules.
- **Error handling:** use `Result` where appropriate, `.expect()` only for truly unrecoverable failures (e.g. "git not found on PATH").
- **Keep functions small and focused.** If a function is doing two things, split it.
- **Descriptive variable names.** `commit_list_width` not `clw`.
- **Prefer `&str` over `String`** in function parameters where ownership is not needed.
- **No em-dashes in user-facing text.** Use periods, commas, or start a new sentence.

## User-Facing Writing Style

In the README, changelog, and other user-facing docs:

- **Write for users, not developers.** Describe what the user sees, not what changed in the code.
- **Prefer general claims over checklists.** Enumerating what works implies the unlisted parts don't.
- **Fixes describe symptoms, not causes.** Say "Diff view now wraps long lines correctly" not "Changed `TextWrapMode::Truncate` to `TextWrapMode::Extend` in `diff_view.rs`."