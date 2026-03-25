# GitShrub - TODO

## Milestone 1: Skeleton

- [x] Parse CLI args (--all, path filter)
- [x] Open eframe window with dark theme
- [x] Define App struct with core state
- [x] Shell out to `git log` and parse structured output into Vec<Commit>
- [x] Display commit list as plain text rows (no graph yet)

## Milestone 2: Commit List

- [x] Render commit rows in a scrollable table/list
- [x] Columns: graph, refs, message, author, date
- [x] Show branch labels `[master]` `[origin/master]` on commits
- [x] Show tag labels `<v1.0.0>` on commits
- [x] Click a row to select it
- [x] Parse `--all` flag to show all branches vs current branch

## Milestone 3: Diff View

- [x] Bottom pane: show selected commit details
- [x] Display full commit SHA (clickable to copy)
- [x] Left side: commit message header + diff body
- [x] Right side: list of affected files
- [x] Clicking a file in the list jumps to that file in the diff

## Milestone 4: Graph Rendering

- [x] Parse parent SHAs to build commit graph
- [x] Assign graph columns to commits
- [x] Draw vertical lines and merge/branch connectors
- [x] Color-code graph lanes

## Milestone 5: File History Mode

- [x] Accept file/directory path as CLI arg
- [x] Run `git log -- <path>` instead of full log
- [x] Show filtered commit list for that path

## Milestone 6: Branch Operations (Context Menu)

- [x] Right-click on branch/tag label → context menu
- [x] Checkout branch
- [x] Delete branch (with confirmation)
- [x] Right-click on commit message → context menu
- [x] Create branch (prompt for name)
- [x] Reset --mixed (with confirmation)
- [x] Reset --hard (with confirmation)
- [x] Revert commit (with confirmation)
- [x] Cherry-pick commit
- [x] Refresh commit list after any mutating operation

## Milestone 7: Polish

- [x] Keyboard navigation (up/down in commit list)
- [x] Handle repos with no commits gracefully
- [x] Handle non-repo directories gracefully (error message)
- [x] Window title shows repo name and current branch
- [x] Performance: large diff view rendering is slow
  - Fixed: diff lines are pre-split into a `Vec<String>` at load time and
    rendered with `ScrollArea::show_rows()` so only the visible slice is
    laid out each frame. File header positions are pre-indexed for instant
    scroll-to-file. The per-frame `DiffOutput` clone was also eliminated.
  - Commit list loading (all 11,885 commits at once) is acceptable for now.
    Windowed loading is impractical because graph lane assignment needs the
    full history from the top, and `git log --skip=N` still walks N commits
    internally so it does not save wall-clock time for deep offsets.

---

## Future: Advanced Rebase & Multi-Commit Operations

> These milestones are scoped for after the core viewer is solid.

## Milestone 8: Multi-Select Cherry-Pick

- [ ] Multi-select commits in the list (click, shift+click range, ctrl+click toggle)
- [ ] Visual highlight for selected commits
- [ ] Right-click selection → "Cherry-pick N commits"
- [ ] Execute cherry-picks in topological order (`git cherry-pick <sha1> <sha2> ...`)
- [ ] Show progress / conflict state if a cherry-pick fails mid-way
- [ ] Stretch: drag and drop commits to reorder before cherry-picking

## Milestone 9: Interactive Rebase Dialog

- [ ] Right-click a branch → "Interactive rebase..."
- [ ] Modal dialog listing commits from HEAD back to the selected base
- [ ] Drag and drop to reorder commits in the list
- [ ] Per-commit action dropdown: pick, reword, edit, squash, fixup, drop
- [ ] "OK" generates and executes `git rebase -i` with the specified sequence
- [ ] Handle conflicts: show status, allow continue/abort from the UI

## Milestone 10: Abort / Cancel Operations

- [ ] Detect in-progress operations (rebase, cherry-pick, merge, bisect, revert)
- [ ] Show a visible "operation in progress" banner in the UI
- [ ] One-click abort: runs the appropriate cancel command:
  - `git rebase --abort`
  - `git cherry-pick --abort`
  - `git merge --abort`
  - `git bisect reset`
  - `git revert --abort`
- [ ] Refresh commit list after abort