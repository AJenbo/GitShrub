use std::collections::{HashMap, HashSet};

use crate::git::Commit;

/// The visual elements for a single row in the graph column.
#[derive(Debug, Clone)]
pub struct GraphRow {
    /// Which column this commit's node sits in.
    pub node_col: usize,
    /// Total number of active lanes for this row (determines graph width).
    pub num_lanes: usize,
    /// Edges to draw for this row. Each edge connects a column in this row
    /// to a column in the next row.
    pub edges: Vec<Edge>,
    /// Color index for this commit's lane (for color-coding).
    pub node_color_index: usize,
}

/// An edge connecting a lane in the current row to a lane in the next row.
#[derive(Debug, Clone)]
pub struct Edge {
    /// Column in the current row where the edge starts.
    pub from_col: usize,
    /// Column in the next row where the edge ends.
    pub to_col: usize,
    /// Color index for this edge.
    pub color_index: usize,
}

/// Number of distinct lane colors available.
pub const NUM_LANE_COLORS: usize = 8;

/// Returns the egui color for a given lane color index.
pub fn lane_color(index: usize) -> egui::Color32 {
    match index % NUM_LANE_COLORS {
        0 => egui::Color32::from_rgb(120, 180, 255), // blue
        1 => egui::Color32::from_rgb(255, 140, 100), // orange
        2 => egui::Color32::from_rgb(100, 220, 120), // green
        3 => egui::Color32::from_rgb(220, 130, 220), // purple
        4 => egui::Color32::from_rgb(255, 220, 80),  // yellow
        5 => egui::Color32::from_rgb(100, 220, 220), // cyan
        6 => egui::Color32::from_rgb(255, 100, 150), // pink
        7 => egui::Color32::from_rgb(180, 200, 140), // lime
        _ => egui::Color32::from_rgb(180, 180, 180), // fallback gray
    }
}

/// Compute the graph layout for all commits.
///
/// The algorithm walks commits from top (newest) to bottom (oldest) and
/// maintains a set of active lanes. Each lane tracks the SHA it is waiting
/// to see next. When a commit arrives, it occupies the lane expecting it.
/// Its parents then replace or extend the lane set.
///
/// Returns one `GraphRow` per commit, in the same order as the input slice.
pub fn compute_graph(commits: &[Commit]) -> Vec<GraphRow> {
    if commits.is_empty() {
        return Vec::new();
    }

    // Build a set of all SHAs present in the commit list. When history is
    // filtered (e.g. `git log -- <path>`), parent SHAs may point to commits
    // that aren't in the list. We filter those out so the graph only shows
    // relationships between commits that are actually visible.
    let known_shas: HashSet<&str> = commits.iter().map(|c| c.full_sha.as_str()).collect();

    // SHA → index lookup for efficient reachability analysis.
    let sha_to_idx: HashMap<&str, usize> = commits
        .iter()
        .enumerate()
        .map(|(i, c)| (c.full_sha.as_str(), i))
        .collect();

    // Precompute visible parents for each commit: keep only parents
    // whose SHA is in the visible commit set.
    let visible_parents: Vec<Vec<String>> = commits
        .iter()
        .map(|c| {
            c.parents
                .iter()
                .filter(|p| known_shas.contains(p.as_str()))
                .cloned()
                .collect()
        })
        .collect();

    // Lazily compute the set of commit indices reachable from a given
    // commit by following visible parent links and synthetic
    // continuations (bridging invisible gaps). This is only needed for
    // merge commits with invisible parents, so we compute on demand
    // rather than precomputing for all commits (which would be O(n²)
    // memory for linear histories).
    let compute_reachable = |start_idx: usize| -> HashSet<usize> {
        let mut reachable = HashSet::new();
        let mut stack = vec![start_idx];
        while let Some(ci) = stack.pop() {
            if !reachable.insert(ci) {
                continue; // already visited
            }
            let vp = &visible_parents[ci];
            if !vp.is_empty() {
                for p in vp {
                    if let Some(&pi) = sha_to_idx.get(p.as_str()) {
                        stack.push(pi);
                    }
                }
            } else if !commits[ci].parents.is_empty() && ci + 1 < commits.len() {
                // Synthetic continuation: bridge invisible gap.
                stack.push(ci + 1);
            }
        }
        reachable
    };

    // Active lanes: each entry is (expected_sha, color_index).
    // The position in the vec is the column number.
    let mut lanes: Vec<(String, usize)> = Vec::new();
    let mut next_color: usize = 0;
    let mut rows: Vec<GraphRow> = Vec::with_capacity(commits.len());

    for (idx, commit) in commits.iter().enumerate() {
        let sha = &commit.full_sha;

        // Find the column for this commit: the leftmost lane expecting this SHA.
        let node_col = find_lane(&lanes, sha);

        let (node_col, node_color_index) = if let Some(col) = node_col {
            let color = lanes[col].1;
            (col, color)
        } else {
            // No lane is expecting this commit. This happens for the first
            // commit or when a branch appears that we haven't seen before.
            // Add a new lane on the right.
            let color = next_color;
            next_color += 1;
            let col = lanes.len();
            lanes.push((sha.clone(), color));
            (col, color)
        };

        // Collect all lanes that are waiting for this same SHA (possible when
        // multiple branches converge). We need to close the duplicates.
        let duplicate_cols: Vec<usize> = lanes
            .iter()
            .enumerate()
            .filter(|(i, (s, _))| s == sha && *i != node_col)
            .map(|(i, _)| i)
            .collect();

        // Build edges from current lanes to the next row's lanes.
        // First, figure out what the lanes will look like after this commit.
        //
        // Strategy:
        // 1. The node_col lane gets replaced by this commit's first parent
        //    (continuing the same lane). If no parents, the lane closes.
        // 2. Additional parents get new lanes (or reuse closing duplicate lanes).
        // 3. Duplicate lanes for this SHA are removed.
        // 4. All other lanes pass through unchanged.

        // Reconstruct the full parent list, preserving the original order
        // from git. For each real parent:
        //   - If it's in the visible set, keep it as-is.
        //   - If it's NOT in the visible set, synthesize a continuation
        //     to the next commit in topo-order that belongs to this
        //     parent's branch (not reachable from other visible parents).
        let parents = &visible_parents[idx];
        let mut effective_parents: Vec<String> = Vec::new();

        if parents.len() == commit.parents.len() {
            // All parents are visible — use them directly.
            effective_parents = parents.clone();
        } else if !commit.parents.is_empty() && idx + 1 < commits.len() {
            let is_merge = commit.parents.len() > 1;

            // For merge commits, collect reachability sets for each
            // visible parent so we can exclude commits that belong to
            // other branches when scanning for synthetic targets.
            // This is computed lazily — only for merges that actually
            // need it (those with invisible parents).
            let visible_parent_indices: Vec<usize> = commit
                .parents
                .iter()
                .filter_map(|p| sha_to_idx.get(p.as_str()).copied())
                .collect();

            // Some parents are missing. Walk the real parent list and
            // fill in synthetic connections for the invisible ones.
            for real_parent in &commit.parents {
                if known_shas.contains(real_parent.as_str()) {
                    // This parent is visible — keep it.
                    effective_parents.push(real_parent.clone());
                } else if is_merge {
                    // Merge commit with an invisible parent: find the
                    // next commit in topo-order that:
                    //   1. No lane is currently expecting it.
                    //   2. Isn't already chosen as a synthetic/visible parent.
                    //   3. Isn't reachable from any VISIBLE parent of this
                    //      merge (those commits belong to a different branch).
                    //
                    // Build the "other side" reachable set: all commits
                    // reachable from visible parents of this merge.
                    let other_side_reachable: HashSet<usize> = visible_parent_indices
                        .iter()
                        .flat_map(|&vpi| compute_reachable(vpi))
                        .collect();

                    let mut found = false;
                    for scan_idx in (idx + 1)..commits.len() {
                        let candidate = &commits[scan_idx].full_sha;
                        let already_taken = lanes.iter().any(|(s, _)| s == candidate)
                            || effective_parents.iter().any(|p| p == candidate);
                        if already_taken {
                            continue;
                        }
                        // Skip commits reachable from visible parents —
                        // they belong to the other branch.
                        if other_side_reachable.contains(&scan_idx) {
                            continue;
                        }
                        effective_parents.push(candidate.clone());
                        found = true;
                        break;
                    }
                    if !found {
                        // All future commits belong to visible parents or
                        // are tracked; connect to next commit anyway.
                        effective_parents.push(commits[idx + 1].full_sha.clone());
                    }
                } else {
                    // Non-merge commit with invisible parent: connect to
                    // the next commit in the list. If it's already tracked
                    // by another lane, the edge will merge into that lane
                    // (the |/ pattern). This is correct because in
                    // topo-order the next commit is always the logical
                    // continuation of this branch.
                    effective_parents.push(commits[idx + 1].full_sha.clone());
                }
            }
            // If commit had real parents but we ended up with nothing
            // (edge case), fall back to connecting to the next commit.
            if effective_parents.is_empty() {
                effective_parents.push(commits[idx + 1].full_sha.clone());
            }
        }

        let parents = &effective_parents;

        // Prepare the next lane state.
        let mut next_lanes: Vec<(String, usize)> = Vec::with_capacity(lanes.len());
        let mut edges: Vec<Edge> = Vec::new();

        // Track which columns in the new lanes each old lane maps to.
        // Also track which parents still need a new lane.
        let mut remaining_parents: Vec<String> = parents.to_vec();

        // Pass 1: Map existing lanes to the next row.
        for (col, (lane_sha, lane_color)) in lanes.iter().enumerate() {
            if col == node_col {
                // This is the commit's lane. Replace with first parent.
                if let Some(first_parent) = parents.first() {
                    let new_col = next_lanes.len();
                    next_lanes.push((first_parent.clone(), *lane_color));
                    edges.push(Edge {
                        from_col: col,
                        to_col: new_col,
                        color_index: *lane_color,
                    });
                    remaining_parents.retain(|p| p != first_parent);
                }
                // If no parents, this lane just dies (no entry added).
            } else if duplicate_cols.contains(&col) {
                // This duplicate lane merges into the node column.
                // Draw an edge from this column to the node's continuing lane.
                // Find where the node_col's first parent ended up in next_lanes.
                if let Some(target) = find_lane(&next_lanes, parents.first().unwrap_or(sha)) {
                    edges.push(Edge {
                        from_col: col,
                        to_col: target,
                        color_index: *lane_color,
                    });
                }
                // Lane is consumed, don't add to next_lanes.
            } else {
                // This lane passes through.
                let new_col = next_lanes.len();
                next_lanes.push((lane_sha.clone(), *lane_color));
                edges.push(Edge {
                    from_col: col,
                    to_col: new_col,
                    color_index: *lane_color,
                });
            }
        }

        // Pass 2: Add new lanes for remaining parents (2nd, 3rd, etc.).
        for parent_sha in &remaining_parents {
            // Check if any existing lane in next_lanes is already waiting for
            // this parent (can happen if another branch already points here).
            if find_lane(&next_lanes, parent_sha).is_some() {
                // Already tracked. Draw an edge from node_col to that lane.
                let target = find_lane(&next_lanes, parent_sha).unwrap();
                edges.push(Edge {
                    from_col: node_col,
                    to_col: target,
                    color_index: next_color,
                });
                next_color += 1;
            } else {
                let color = next_color;
                next_color += 1;
                let new_col = next_lanes.len();
                next_lanes.push((parent_sha.clone(), color));
                edges.push(Edge {
                    from_col: node_col,
                    to_col: new_col,
                    color_index: color,
                });
            }
        }

        let num_lanes = lanes.len().max(next_lanes.len());

        rows.push(GraphRow {
            node_col,
            num_lanes,
            edges,
            node_color_index,
        });

        lanes = next_lanes;
    }

    rows
}

/// Find the leftmost lane that is expecting the given SHA.
fn find_lane(lanes: &[(String, usize)], sha: &str) -> Option<usize> {
    lanes.iter().position(|(s, _)| s == sha)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::Commit;

    fn make_commit(sha: &str, parents: &[&str]) -> Commit {
        Commit {
            full_sha: sha.to_string(),
            short_sha: sha[..7.min(sha.len())].to_string(),
            parents: parents.iter().map(|s| s.to_string()).collect(),
            author_name: "Test".to_string(),
            author_email: "test@test.com".to_string(),
            date: "2024-01-01".to_string(),
            subject: "test commit".to_string(),
            body: String::new(),
            refs: Vec::new(),
        }
    }

    #[test]
    fn linear_history() {
        let commits = vec![
            make_commit("aaa", &["bbb"]),
            make_commit("bbb", &["ccc"]),
            make_commit("ccc", &[]),
        ];

        let rows = compute_graph(&commits);
        assert_eq!(rows.len(), 3);

        // All commits should be in column 0 (linear history).
        for row in &rows {
            assert_eq!(row.node_col, 0);
        }
    }

    #[test]
    fn simple_branch_and_merge() {
        // A merge commit with two parents, then two parallel commits,
        // then the common ancestor.
        //
        //   * aaa (merge of bbb and ccc)
        //   |\
        //   * | bbb (parent: ddd)
        //   | * ccc (parent: ddd)
        //   |/
        //   * ddd (root)
        let commits = vec![
            make_commit("aaa", &["bbb", "ccc"]),
            make_commit("bbb", &["ddd"]),
            make_commit("ccc", &["ddd"]),
            make_commit("ddd", &[]),
        ];

        let rows = compute_graph(&commits);
        assert_eq!(rows.len(), 4);

        // First commit (merge) should be in col 0.
        assert_eq!(rows[0].node_col, 0);

        // After the merge, there should be 2 lanes.
        assert!(rows[0].num_lanes >= 2);

        // bbb should be in col 0 (first parent continues the lane).
        assert_eq!(rows[1].node_col, 0);

        // ccc should be in col 1 (second parent lane).
        assert_eq!(rows[2].node_col, 1);

        // ddd should be back to col 0 (lanes merge).
        assert_eq!(rows[3].node_col, 0);
    }

    #[test]
    fn empty_commits() {
        let rows = compute_graph(&[]);
        assert!(rows.is_empty());
    }

    #[test]
    fn single_root_commit() {
        let commits = vec![make_commit("aaa", &[])];
        let rows = compute_graph(&commits);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].node_col, 0);
        assert!(rows[0].edges.is_empty());
    }

    #[test]
    fn filtered_linear_history() {
        // Simulates `git log -- file` where commits touched the file
        // but their parents (which didn't touch it) are missing.
        // Parents point to SHAs not in the list.
        //
        // Expected: a straight vertical line, not a fan of branches.
        let commits = vec![
            make_commit("aaa", &["xxx"]), // xxx not in list
            make_commit("bbb", &["yyy"]), // yyy not in list
            make_commit("ccc", &["zzz"]), // zzz not in list
        ];

        let rows = compute_graph(&commits);
        assert_eq!(rows.len(), 3);

        // All commits should be in column 0 (linear continuation).
        for row in &rows {
            assert_eq!(row.node_col, 0, "all nodes should be in col 0");
        }

        // First two rows should have edges (continuing the lane).
        assert!(!rows[0].edges.is_empty(), "row 0 should have an edge");
        assert!(!rows[1].edges.is_empty(), "row 1 should have an edge");
        // Last row has no visible parents and its real parent (zzz) is not
        // in the list, but it's the last commit so the lane ends.
        assert!(rows[2].edges.is_empty(), "last row should have no edges");
    }

    #[test]
    fn filtered_with_visible_merge() {
        // Simulates path-filtered history where a merge commit has
        // both parents visible (both touched the file), producing
        // a real branch in the graph.
        //
        //   * aaa (parents: bbb, ccc — both visible)
        //   |\
        //   * | bbb (parent: xxx — not visible, lane dies)
        //   | * ccc (parent: xxx — not visible, lane dies)
        let commits = vec![
            make_commit("aaa", &["bbb", "ccc"]),
            make_commit("bbb", &["xxx"]),
            make_commit("ccc", &["xxx"]),
        ];

        let rows = compute_graph(&commits);
        assert_eq!(rows.len(), 3);

        // aaa is a merge with two visible parents — should branch.
        assert_eq!(rows[0].node_col, 0);
        assert!(rows[0].num_lanes >= 2);

        // bbb continues col 0, ccc is in col 1.
        assert_eq!(rows[1].node_col, 0);
        // After bbb's lane dies (next commit ccc is already claimed by
        // another lane), ccc's lane compacts left to col 0.
        assert_eq!(rows[2].node_col, 0);
    }

    #[test]
    fn filtered_mixed_parents() {
        // A merge commit where only one parent is visible in the
        // filtered list. The invisible parent should be dropped,
        // producing a straight line rather than a branch.
        let commits = vec![
            make_commit("aaa", &["bbb", "xxx"]), // xxx not visible
            make_commit("bbb", &["yyy"]),        // yyy not visible
            make_commit("ccc", &["zzz"]),        // zzz not visible
        ];

        let rows = compute_graph(&commits);
        assert_eq!(rows.len(), 3);

        // aaa has one visible parent (bbb), so it's a straight line.
        assert_eq!(rows[0].node_col, 0);
        // bbb has no visible parents, only 1 lane active, so synthetic
        // parent connects to ccc.
        assert_eq!(rows[1].node_col, 0);
        assert_eq!(rows[2].node_col, 0);
    }

    #[test]
    fn true_root_not_connected_to_next() {
        // A true root commit (no parents at all) should NOT get a
        // synthetic parent connecting it to the next commit.
        let commits = vec![
            make_commit("aaa", &[]), // true root
            make_commit("bbb", &[]), // another true root
        ];

        let rows = compute_graph(&commits);
        assert_eq!(rows.len(), 2);

        // Both are roots — aaa should NOT have an edge to bbb.
        assert!(
            rows[0].edges.is_empty(),
            "true root should not connect to next"
        );
        assert!(
            rows[1].edges.is_empty(),
            "true root should not connect to next"
        );
    }

    #[test]
    fn nested_merge_with_parallel_lanes() {
        // Models a pattern from real git history (git.log lines ~2610-2660):
        //
        //   * aaa (merge: parents bbb, ccc)         col 0
        //   |\
        //   | * ccc (parent: ddd)                    col 1
        //   | * ddd (parent: eee)                    col 1
        //   * | bbb (parent: eee)                    col 0
        //   |/
        //   * eee (root)                             col 0
        //
        // Topo-order: aaa, ccc, ddd, bbb, eee
        let commits = vec![
            make_commit("aaa", &["bbb", "ccc"]),
            make_commit("ccc", &["ddd"]),
            make_commit("ddd", &["eee"]),
            make_commit("bbb", &["eee"]),
            make_commit("eee", &[]),
        ];

        let rows = compute_graph(&commits);
        assert_eq!(rows.len(), 5);

        // aaa is a merge — should be in col 0 with 2 lanes created.
        assert_eq!(rows[0].node_col, 0);
        assert!(rows[0].num_lanes >= 2, "merge should create >= 2 lanes");

        // ccc should be in col 1 (second parent lane).
        assert_eq!(rows[1].node_col, 1);

        // ddd continues col 1.
        assert_eq!(rows[2].node_col, 1);

        // bbb should be in col 0 (first parent lane).
        assert_eq!(rows[3].node_col, 0);

        // eee: both lanes converge here, should be in col 0.
        assert_eq!(rows[4].node_col, 0);
        // eee is a root with no parents — no outgoing edges.
        assert!(rows[4].edges.is_empty());
    }

    #[test]
    fn merge_into_existing_lane() {
        // Models the |\| pattern from git.log (line ~3346):
        // A merge commit where the second parent is already tracked
        // by an existing lane (another branch already points there).
        //
        //   * | aaa (parent: ccc)            col 0, lane 1 passes through
        //   * | bbb (merge: ddd, ccc)        col 0, merge into existing lane at col 1
        //   |/
        //   * ccc (parent: ddd)              col 0
        //   * ddd (root)                     col 0
        //
        // Setup: two commits on col 0 with a lane on col 1 tracking ccc.
        // Then bbb merges into ccc which is already tracked by the col 1 lane.
        //
        // We model this with commits in topo-order:
        //   fff has parents: aaa, eee  (creates two lanes)
        //   aaa has parents: bbb      (continues col 0)
        //   eee has parents: ccc      (continues col 1)
        //   bbb has parents: ddd, ccc (merge — ccc already in col 1 lane!)
        //   ccc has parents: ddd
        //   ddd root
        let commits = vec![
            make_commit("fff", &["aaa", "eee"]),
            make_commit("aaa", &["bbb"]),
            make_commit("eee", &["ccc"]),
            make_commit("bbb", &["ddd", "ccc"]),
            make_commit("ccc", &["ddd"]),
            make_commit("ddd", &[]),
        ];

        let rows = compute_graph(&commits);
        assert_eq!(rows.len(), 6);

        // fff creates two lanes.
        assert_eq!(rows[0].node_col, 0);
        assert!(rows[0].num_lanes >= 2);

        // aaa continues col 0.
        assert_eq!(rows[1].node_col, 0);

        // eee continues col 1.
        assert_eq!(rows[2].node_col, 1);

        // bbb is in col 0. Its second parent (ccc) is already tracked
        // by the lane at col 1. The algorithm should draw an edge from
        // col 0 to the existing ccc lane rather than creating a new lane.
        assert_eq!(rows[3].node_col, 0);
        // After bbb, lane col 1 was already tracking ccc, and bbb's
        // first parent (ddd) replaces col 0. So we should have 2 lanes.
        assert!(
            rows[3].num_lanes <= 3,
            "should not create excessive lanes when merging into existing lane"
        );

        // ccc is in col 1 — the lane that was tracking it from eee's
        // first-parent replacement. Its parent ddd is already tracked
        // in col 0, so the two lanes merge here.
        assert_eq!(rows[4].node_col, 1, "ccc stays in col 1");

        // ddd: after ccc, the duplicate ddd lanes collapse so ddd
        // lands in col 0.
        assert_eq!(rows[5].node_col, 0, "ddd compacts to col 0");
        assert!(rows[5].edges.is_empty(), "root should have no edges");
    }

    #[test]
    fn deeply_nested_merge_many_lanes() {
        // Models the complex pattern from git.log (~lines 2634-2660):
        // A branch with repeated merges from master, creating many
        // parallel pass-through lanes that eventually collapse.
        //
        //   * aaa (merge: bbb, ccc)        — opens 2 lanes
        //   |\
        //   | * ccc (merge: ddd, eee)      — opens 3 lanes total
        //   | |\
        //   | * | ddd (parent: fff)        — col 1
        //   | | * eee (parent: fff)        — col 2
        //   * | | bbb (parent: fff)        — col 0
        //   |/ /
        //   * / fff (parent: ggg)          — lanes collapse
        //   |/
        //   * ggg (root)
        //
        // Topo-order: aaa, ccc, ddd, eee, bbb, fff, ggg
        let commits = vec![
            make_commit("aaa", &["bbb", "ccc"]),
            make_commit("ccc", &["ddd", "eee"]),
            make_commit("ddd", &["fff"]),
            make_commit("eee", &["fff"]),
            make_commit("bbb", &["fff"]),
            make_commit("fff", &["ggg"]),
            make_commit("ggg", &[]),
        ];

        let rows = compute_graph(&commits);
        assert_eq!(rows.len(), 7);

        // aaa opens the graph.
        assert_eq!(rows[0].node_col, 0);

        // ccc is on col 1 (second parent of aaa), and it's a merge
        // that opens a third lane.
        assert_eq!(rows[1].node_col, 1);
        assert!(
            rows[1].num_lanes >= 3,
            "nested merge should have >= 3 lanes"
        );

        // fff: all three lanes (bbb, ddd, eee) converge on fff.
        // It should be found in one of the lanes expecting it.
        assert_eq!(rows[5].node_col, 0, "fff should be in col 0");

        // ggg is the root.
        assert_eq!(rows[6].node_col, 0);
        assert!(rows[6].edges.is_empty());
    }

    #[test]
    fn lane_pass_through_preserves_order() {
        // When a commit in the middle of several lanes is processed,
        // other lanes should pass through without reordering.
        //
        //   * aaa (parents: bbb, ccc, ddd)   — 3 lanes
        //   |\|
        //   | * ccc (parent: eee)            — col 1 active, 0 and 2 pass through
        //   * | | bbb (parent: eee)
        //   |/ /
        //   * ddd (parent: eee)              — would be col 0 after compaction
        //   * eee (root)
        let commits = vec![
            make_commit("aaa", &["bbb", "ccc", "ddd"]),
            make_commit("ccc", &["eee"]),
            make_commit("bbb", &["eee"]),
            make_commit("ddd", &["eee"]),
            make_commit("eee", &[]),
        ];

        let rows = compute_graph(&commits);
        assert_eq!(rows.len(), 5);

        // aaa creates 3 lanes.
        assert_eq!(rows[0].node_col, 0);
        assert!(rows[0].num_lanes >= 3, "should have >= 3 lanes");

        // ccc is in col 1.
        assert_eq!(rows[1].node_col, 1);

        // bbb is in col 0.
        assert_eq!(rows[2].node_col, 0);

        // All lanes eventually converge to eee.
        assert_eq!(rows[4].node_col, 0);
        assert!(rows[4].edges.is_empty(), "root has no edges");
    }

    #[test]
    fn git_log_merge_pattern_lines_1_to_13() {
        // Exactly models the merge pattern from git.log lines 1-13,
        // using the REAL parent SHAs from the repository.
        //
        // Key detail: e94347aa70's real parent is 00a1534f83 which is
        // NOT in this commit list. The synthetic parent logic must
        // connect e94347aa70 to the next commit (5503102a2f) even
        // though 5503102a2f is already claimed by lane 0.
        //
        // Expected graph:
        // * 4a0882bf49 altered fixture path                          col 0
        // * f1a2d809d1 wip                                          col 0
        // * ef7aeac1ef set Development/ShortTextSeeder...            col 0
        // * 32c6e5eee5 wip                                          col 0
        // *   2648482859 Merge branch 'master' into optimize...      col 0  ← merge
        // |\
        // | * 6bd7e522cf Explore alternative image size...           col 1
        // | * c85aa06beb Command that fixes failed altapay...        col 1
        // | * e94347aa70 Do not get Hello Retail...                  col 1  ← parent 00a1 NOT in list!
        // * | 5503102a2f test                                        col 0  ← must show pass-through at col 1
        // |/
        // * 314248177e Resolve document type another way             col 0  ← both lanes merge here
        // * f8a7ab3d7c Fix issue with deviation messages             col 0

        let commits = vec![
            make_commit("4a0882bf49", &["f1a2d809d1"]),
            make_commit("f1a2d809d1", &["ef7aeac1ef"]),
            make_commit("ef7aeac1ef", &["32c6e5eee5"]),
            make_commit("32c6e5eee5", &["2648482859"]),
            make_commit("2648482859", &["5503102a2f", "6bd7e522cf"]), // merge
            make_commit("6bd7e522cf", &["c85aa06beb"]),
            make_commit("c85aa06beb", &["e94347aa70"]),
            make_commit("e94347aa70", &["00a1534f83"]), // parent NOT in list
            make_commit("5503102a2f", &["314248177e"]),
            make_commit("314248177e", &["f8a7ab3d7c"]),
            make_commit("f8a7ab3d7c", &[]),
        ];

        let rows = compute_graph(&commits);
        assert_eq!(rows.len(), 11);

        // Rows 0-3: linear history, all col 0, single lane.
        for i in 0..4 {
            assert_eq!(rows[i].node_col, 0, "row {} should be col 0", i);
        }

        // Row 4 (2648482859): merge commit, col 0, creates 2 lanes.
        assert_eq!(rows[4].node_col, 0, "merge commit should be col 0");
        assert!(
            rows[4].num_lanes >= 2,
            "merge should create >= 2 lanes, got {}",
            rows[4].num_lanes
        );
        assert!(
            rows[4].edges.len() >= 2,
            "merge should have >= 2 edges, got {}",
            rows[4].edges.len()
        );

        // Rows 5-6 (6bd7, c85a): second parent branch, col 1.
        // Lane at col 0 must pass through (waiting for 5503102a2f).
        for i in 5..7 {
            assert_eq!(rows[i].node_col, 1, "row {} should be col 1", i);
            assert!(
                rows[i].edges.len() >= 2,
                "row {} should have >= 2 edges (pass-through + continuation), got {}",
                i,
                rows[i].edges.len()
            );
            let has_pass_through = rows[i]
                .edges
                .iter()
                .any(|e| e.from_col == 0 && e.to_col == 0);
            assert!(
                has_pass_through,
                "row {} must have pass-through edge at col 0",
                i
            );
        }

        // Row 7 (e94347aa70): col 1, parent 00a1534f83 is NOT in the
        // list. Synthetic parent should connect to next commit
        // (5503102a2f) even though it's already claimed by lane 0.
        // This creates two lanes both waiting for 5503102a2f.
        assert_eq!(rows[7].node_col, 1, "e94347aa70 should be col 1");
        assert!(
            rows[7].edges.len() >= 2,
            "e94347aa70 should have >= 2 edges (pass-through at 0 + synthetic at 1), got {}",
            rows[7].edges.len()
        );

        // Row 8 (5503102a2f): first parent, col 0.
        // The duplicate lane at col 1 (from synthetic parent) should
        // produce a merge edge from col 1 → col 0.
        assert_eq!(rows[8].node_col, 0, "5503102a2f should be col 0");
        let has_merge_from_col1 = rows[8]
            .edges
            .iter()
            .any(|e| e.from_col == 1 && e.to_col == 0);
        assert!(
            has_merge_from_col1,
            "5503102a2f must have merge edge from col 1 (the |/ pattern)"
        );

        // Row 9 (314248177e): back to single lane, col 0.
        assert_eq!(rows[9].node_col, 0, "314248177e should be col 0");

        // Row 10 (f8a7ab3d7c): single lane, col 0.
        assert_eq!(rows[10].node_col, 0, "f8a7ab3d7c should be col 0");
    }

    #[test]
    fn nested_merge_with_invisible_first_parent() {
        // Models git.log lines 2607-2627: a merge where the nested
        // merge's first parent is invisible and its descendants appear
        // as orphans in topo-order.
        //
        // Real commit data:
        //   4bcccc (merge: b30190, f38d68)
        //   f38d68 (parent: 2f74ac)
        //   2f74ac (parent: f55d23)
        //   f55d23 (merge: 689e81[NOT IN LIST], f422a4[in list])
        //   53f905 (parent: 2a5388[NOT IN LIST])  ← orphan from 689e81's branch
        //   a95b3f (parent: dacdd3[NOT IN LIST])  ← orphan from 689e81's branch
        //   b30190 (parent: a90a82)
        //   a90a82 (parent: ff336c)
        //   ...
        //   f422a4 (parent: ...)
        //
        // Expected graph:
        //   *     4bcccc (merge)                col 0
        //   |\
        //   | *   f38d68                        col 1
        //   | *   2f74ac                        col 1
        //   | *   f55d23 (nested merge)         col 1, opens col 2
        //   | |\
        //   | * | 53f905                        col 1
        //   | * | a95b3f                        col 1
        //   * | | b30190                        col 0
        //   * | | a90a82                        col 0
        //   ...
        //
        // The key: f55d23's invisible first parent (689e81) should
        // synthesize a connection to 53f905 (the next untracked commit),
        // keeping it on col 1. The visible second parent (f422a4)
        // gets a new lane at col 2.

        let commits = vec![
            make_commit("4bcccc", &["b30190", "f38d68"]),       // merge
            make_commit("f38d68", &["2f74ac"]),
            make_commit("2f74ac", &["f55d23"]),
            make_commit("f55d23", &["689e81", "f422a4"]),       // nested merge, 689e81 NOT in list
            make_commit("53f905", &["2a5388"]),                 // orphan, NOT in list
            make_commit("a95b3f", &["dacdd3"]),                 // orphan, NOT in list
            make_commit("b30190", &["a90a82"]),
            make_commit("a90a82", &["ff336c"]),
            make_commit("ff336c", &["caa789"]),
            make_commit("caa789", &["f422a4"]),
            make_commit("f422a4", &[]),
        ];

        let rows = compute_graph(&commits);
        assert_eq!(rows.len(), 11);

        // Row 0: merge, col 0, opens 2 lanes.
        assert_eq!(rows[0].node_col, 0);
        assert!(rows[0].num_lanes >= 2);

        // Rows 1-2: branch commits, col 1.
        assert_eq!(rows[1].node_col, 1);
        assert_eq!(rows[2].node_col, 1);

        // Row 3 (f55d23): nested merge at col 1.
        // First parent 689e81 is NOT in the list — should synthesize
        // continuation to 53f905 (keeping col 1 alive).
        // Second parent f422a4 IS in the list — gets a new lane.
        assert_eq!(rows[3].node_col, 1, "f55d23 should be col 1");
        assert!(
            rows[3].num_lanes >= 3,
            "nested merge should open >= 3 lanes, got {}",
            rows[3].num_lanes
        );

        // Rows 4-5 (53f905, a95b3f): connected via synthetic parents,
        // should stay on the branch side (col 1), not jump to col 0.
        // The exact column may shift due to lane compaction, but they
        // must NOT be on col 0 (the master lane).
        assert_ne!(
            rows[4].node_col, 0,
            "53f905 should NOT be col 0 (it's on the branch side)"
        );
        assert_ne!(
            rows[5].node_col, 0,
            "a95b3f should NOT be col 0 (it's on the branch side)"
        );

        // Row 6 (b30190): first parent of the outer merge, col 0.
        assert_eq!(rows[6].node_col, 0, "b30190 should be col 0");
    }

    #[test]
    fn merge_invisible_first_parent_skips_visible_second_parent() {
        // Models git.log lines ~2686-2700: a merge where the first
        // parent is invisible (master side) and the second parent is
        // visible. The synthetic scan for the invisible first parent
        // must NOT pick commits reachable from the visible second
        // parent (they belong to the branch side).
        //
        // Real data:
        //   e12c43 (merge: 462a63[NOT IN LIST], 3add1e[visible])
        //   3add1e (parent: 503e7d)
        //   503e7d (parent: e3ec7d)
        //   e3ec7d (parent: 3add76[NOT IN LIST])
        //   8cd740 (parent: 731755[NOT IN LIST])
        //   a8b65f (parent: 9bf15f[visible, far down])
        //   f9eb5a (parent: b3f3ba)    ← this is on master, must stay on col 0
        //   b3f3ba (parent: 4271ff)
        //   ...
        //   9bf15f (parent: ...)
        //
        // Expected: e12c43's invisible first parent synthesizes to
        // f9eb5a (the first untracked master-side commit), NOT to
        // 503e7d or any commit reachable from 3add1e.

        let commits = vec![
            make_commit("e12c43", &["462a63", "3add1e"]),  // merge, 462a63 NOT in list
            make_commit("3add1e", &["503e7d"]),
            make_commit("503e7d", &["e3ec7d"]),
            make_commit("e3ec7d", &["3add76"]),             // parent NOT in list
            make_commit("8cd740", &["731755"]),              // parent NOT in list
            make_commit("a8b65f", &["9bf15f"]),              // parent visible (far down)
            make_commit("f9eb5a", &["b3f3ba"]),              // master side
            make_commit("b3f3ba", &["4271ff"]),
            make_commit("4271ff", &["e2cdcb"]),
            make_commit("e2cdcb", &["9bf15f"]),
            make_commit("9bf15f", &[]),
        ];

        let rows = compute_graph(&commits);
        assert_eq!(rows.len(), 11);

        // Row 0: merge at col 0, opens 2 lanes.
        assert_eq!(rows[0].node_col, 0, "merge should be col 0");
        assert!(rows[0].num_lanes >= 2);

        // Rows 1-2: visible second-parent branch at col 1.
        assert_eq!(rows[1].node_col, 1, "row 1 should be col 1 (branch)");
        assert_eq!(rows[2].node_col, 1, "row 2 should be col 1 (branch)");

        // Rows 3-5 (e3ec7d, 8cd740, a8b65f): branch side with invisible
        // parent gaps. They chain via synthetic parents and must NOT be
        // on the master lane (col 0).
        for i in 3..6 {
            assert_ne!(
                rows[i].node_col, 0,
                "row {} should NOT be col 0 (it's on the branch side)",
                i
            );
        }

        // Row 6 (f9eb5a): master side, must be at col 0, NOT on a new column.
        assert_eq!(
            rows[6].node_col, 0,
            "f9eb5a must be col 0 (master), not an orphan column"
        );

        // Row 7 (b3f3ba): continues master at col 0.
        assert_eq!(rows[7].node_col, 0, "b3f3ba should be col 0");
    }
}
