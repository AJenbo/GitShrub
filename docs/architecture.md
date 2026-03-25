# GitTea Architecture

## Philosophy

GitTea is a **gitk replacement**. Nothing more, nothing less.

It's not an IDE. It's not a full Git client. It's a commit history viewer with
just enough actions to be useful. Like gitk, the heavy lifting is done via CLI
flags — the GUI stays simple.

### Principles

1. **Simple over clever** — Shell out to `git`. Parse stdout. No libgit2.
2. **Fast over pretty** — egui immediate mode, GPU-accelerated. No web tech.
3. **Single binary** — `cargo build --release` and you're done. No config files,
   no runtime dependencies beyond `git` on your PATH.
4. **Dark mode first** — Light mode is optional, dark is the default.
5. **Keyboard and mouse** — Context menus for actions, click to copy, CLI for filtering.

## Tech Stack

| Component        | Choice                          | Why                                      |
|------------------|---------------------------------|------------------------------------------|
| Language         | Rust (edition 2024)             | Fast, single binary, strong typing       |
| GUI framework    | egui + eframe                   | Immediate mode, GPU-accelerated, simple  |
| Git interaction  | `std::process::Command` → `git` | Like gitk — no libgit2 complexity        |
| File dialogs     | rfd                             | Native OS file dialogs                   |

## CLI Interface

GitTea is invoked from the terminal. Filtering is done via flags, not GUI widgets.

```sh
gittea                     # Current branch history
gittea --all               # All branches
gittea <path>              # File/directory history
gittea --all <path>        # All branches, filtered to path
```

Run from inside a git repo, or it exits with an error.

## GUI Layout

The window is split into three vertical sections:

```
┌─────────────────────────────────────────────────────────────────────────────┐
│ Commit List (top pane, scrollable)                                          │
│                                                                             │
│  * [origin/master]─[master] Refactoring   │ Jane Doe │ 2006-05-03 12:32:11 │
│  * <v1.0.0> Bump version                  │ Jane Doe │ 2006-05-03 12:32:11 │
│  │\                                       │          │                     │
│  * │ Fix login bug                        │ Jon Doe  │ 2006-05-02 09:14:22 │
│  │/                                       │          │                     │
│  * Initial commit                         │ Jane Doe │ 2006-05-01 08:00:00 │
├─────────────────────────────────────────────────────────────────────────────┤
│ Commit Info (middle bar)                                                    │
│                                                                             │
│  Commit: abc1234  (click SHA to copy)                                       │
├──────────────────────────────────────┬──────────────────────────────────────┤
│ Diff View (bottom-left)              │ Affected Files (bottom-right)        │
│                                      │                                      │
│  diff --git a/src/main.rs ...        │  src/main.rs                         │
│  - old line                          │  src/app.rs                          │
│  + new line                          │  README.md                           │
│                                      │                                      │
└──────────────────────────────────────┴──────────────────────────────────────┘
```

### Commit List

- Graph lines on the left (like gitk tree view)
- Branch labels shown as `[branch-name]` on their commit — clickable
- Tag labels shown as `<tag-name>` on their commit — clickable
- Columns: graph + message, author, date
- Selecting a commit populates the bottom panes

### Commit Info Bar

- Shows full SHA — **clicking it copies to clipboard**
- Shows full commit message (subject + body)

### Diff View

- Shows the unified diff for the selected commit
- When a file is clicked in the affected files list, the diff scrolls to that file

### Affected Files List

- Lists all files changed in the selected commit
- Clicking a file jumps to that file's section in the diff view

## Context Menu Actions

Right-clicking a **branch/tag label** or a **commit row** opens a context menu:

| Action          | Target              | Git command                              |
|-----------------|---------------------|------------------------------------------|
| Create branch   | Commit              | `git branch <name> <sha>`               |
| Checkout        | Branch label        | `git checkout <branch>`                  |
| Delete branch   | Branch label        | `git branch -d <branch>`                |
| Reset --mixed   | Commit              | `git reset --mixed <sha>`               |
| Reset --hard    | Commit              | `git reset --hard <sha>`                |
| Revert          | Commit              | `git revert <sha>`                      |
| Cherry-pick     | Commit              | `git cherry-pick <sha>`                 |

After any mutating action, the commit list is refreshed.

### Multi-Select (Future)

The commit list supports multi-select for batch operations:

- **Click** — select one commit
- **Shift+Click** — select a range
- **Ctrl+Click** — toggle individual commits in/out of selection

Right-clicking a multi-selection offers "Cherry-pick N commits", executed in topological order.

### Interactive Rebase Dialog (Future)

Right-clicking a branch label offers "Interactive rebase...". This opens a modal with:

- Draggable commit list (reorder by drag and drop)
- Per-commit action dropdown: pick, reword, edit, squash, fixup, drop
- OK button generates and runs the `git rebase -i` sequence

### Abort Operations (Future)

When an operation is in progress (rebase, cherry-pick, merge, bisect, revert),
a banner appears at the top of the window with a one-click abort button that runs
the appropriate cancel command (`git rebase --abort`, `git cherry-pick --abort`, etc.).

## Module Structure

```
src/
├── main.rs          Entry point: parse CLI args, launch eframe
├── app.rs           App struct, state, top-level UI layout
├── git.rs           All git CLI calls (log, branch, diff, checkout, reset, etc.)
├── graph.rs         Commit graph/tree line rendering
└── ui/
    ├── mod.rs       UI module root
    ├── commit_list.rs   Top pane: scrollable commit list with graph
    ├── commit_info.rs   Middle bar: SHA, full message
    ├── diff_view.rs     Bottom-left: unified diff display
    └── file_list.rs     Bottom-right: affected files list
```

### Data Flow

1. **Startup:** `main.rs` parses CLI args (`--all`, `<path>`), finds the repo root
2. **Init:** `App::new()` calls `git.rs` to load commit log, branches, tags
3. **Render loop:** egui calls `App::update()` every frame
   - `commit_list` renders the top pane from cached commit data
   - Selecting a commit triggers `git.rs` to load diff + file list
   - `commit_info`, `diff_view`, `file_list` render from that data
4. **Actions:** Context menu items call `git.rs` functions, then refresh state

### Git CLI Patterns

Use `std::process::Command` for all git operations:

```rust
use std::process::Command;

let output = Command::new("git")
    .args(["log", "--oneline", "-n", "50"])
    .current_dir(&repo_path)
    .output()
    .expect("failed to run git");

let stdout = String::from_utf8_lossy(&output.stdout);
```

For structured commit parsing:

```
git log --pretty=format:"%H%n%h%n%P%n%an%n%ae%n%ai%n%s%n%b%n---END---"
```

For the graph:

```
git log --graph --oneline --decorate [--all]
```

For diffs:

```
git diff-tree -p <sha>
```

For detecting in-progress operations:

```
# Check for rebase, cherry-pick, merge, bisect, revert in progress
test -d .git/rebase-merge || test -d .git/rebase-apply   # rebase
test -f .git/CHERRY_PICK_HEAD                             # cherry-pick
test -f .git/MERGE_HEAD                                   # merge
test -f .git/BISECT_LOG                                   # bisect
test -f .git/REVERT_HEAD                                  # revert
```
