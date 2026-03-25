# GitTea - TODO

## Milestone 1: Skeleton

- [x] Parse CLI args (--all, path filter)
- [x] Open eframe window with dark theme
- [x] Define App struct with core state
- [x] Shell out to `git log` and parse structured output into Vec<Commit>
- [x] Display commit list as plain text rows (no graph yet)

## Milestone 2: Commit List

- [ ] Render commit rows in a scrollable table/list
- [ ] Columns: graph, refs, message, author, date
- [ ] Show branch labels `[master]` `[origin/master]` on commits
- [ ] Show tag labels `<v1.0.0>` on commits
- [ ] Click a row to select it
- [ ] Parse `--all` flag to show all branches vs current branch

## Milestone 3: Diff View

- [ ] Bottom pane: show selected commit details
- [ ] Display full commit SHA (clickable to copy)
- [ ] Left side: commit message header + diff body
- [ ] Right side: list of affected files
- [ ] Clicking a file in the list jumps to that file in the diff

## Milestone 4: Graph Rendering

- [ ] Parse parent SHAs to build commit graph
- [ ] Assign graph columns to commits
- [ ] Draw vertical lines and merge/branch connectors
- [ ] Color-code graph lanes

## Milestone 5: File History Mode

- [ ] Accept file/directory path as CLI arg
- [ ] Run `git log -- <path>` instead of full log
- [ ] Show filtered commit list for that path

## Milestone 6: Branch Operations (Context Menu)

- [ ] Right-click on branch/tag label → context menu
- [ ] Checkout branch
- [ ] Delete branch (with confirmation)
- [ ] Right-click on commit message → context menu
- [ ] Create branch (prompt for name)
- [ ] Reset --mixed (with confirmation)
- [ ] Reset --hard (with confirmation)
- [ ] Revert commit (with confirmation)
- [ ] Cherry-pick commit
- [ ] Refresh commit list after any mutating operation

## Milestone 7: Polish

- [ ] Click commit SHA to copy to clipboard
- [ ] Keyboard navigation (up/down in commit list)
- [ ] Handle repos with no commits gracefully
- [ ] Handle non-repo directories gracefully (error message)
- [ ] Window title shows repo name and current branch
- [ ] Performance: lazy loading / pagination for large repos

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