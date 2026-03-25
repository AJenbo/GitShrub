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

    // Active lanes: each entry is (expected_sha, color_index).
    // The position in the vec is the column number.
    let mut lanes: Vec<(String, usize)> = Vec::new();
    let mut next_color: usize = 0;
    let mut rows: Vec<GraphRow> = Vec::with_capacity(commits.len());

    for commit in commits {
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

        let parents = &commit.parents;

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
}
