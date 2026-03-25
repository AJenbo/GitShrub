use egui::{self, Pos2, Rect, Stroke, Vec2};

use crate::app::App;
use crate::graph;

/// Horizontal spacing between lane centers in the graph column.
const LANE_WIDTH: f32 = 16.0;

/// Radius of the commit node circle.
const NODE_RADIUS: f32 = 4.0;

/// Minimum width reserved for the graph column even when there are few lanes.
const MIN_GRAPH_WIDTH: f32 = 32.0;

/// Renders the commit list in the top pane.
/// Each row shows: graph | refs | message | author | date
/// Clicking a row selects that commit and loads its diff.
pub fn show(app: &mut App, ui: &mut egui::Ui) {
    let text_height = ui.text_style_height(&egui::TextStyle::Monospace);
    let row_height = text_height + 4.0;

    let num_commits = app.commits.len();
    if num_commits == 0 {
        ui.centered_and_justified(|ui| {
            ui.label("No commits to display.");
        });
        return;
    }

    // Determine graph column width from the maximum number of active lanes.
    let max_lanes = app
        .graph_rows
        .iter()
        .map(|r| r.num_lanes)
        .max()
        .unwrap_or(1)
        .max(1);
    let graph_col_width = (max_lanes as f32 * LANE_WIDTH + LANE_WIDTH).max(MIN_GRAPH_WIDTH);

    // Track which commit the user clicked this frame (if any).
    let mut clicked_index: Option<usize> = None;

    egui::ScrollArea::vertical()
        .id_salt("commit_list_scroll")
        .auto_shrink([false, false])
        .show_rows(ui, row_height, num_commits, |ui, row_range| {
            for idx in row_range.clone() {
                let is_selected = app.selected_index == Some(idx);

                // Read commit fields before any mutable borrow.
                let commit = &app.commits[idx];
                let refs = commit.refs.clone();
                let subject = commit.subject.clone();
                let author_name = commit.author_name.clone();
                let date = commit.date.clone();

                let response = ui.horizontal(|ui| {
                    // Disable selectable labels so clicks pass through text
                    // to the row's interaction rect instead of being consumed.
                    ui.style_mut().interaction.selectable_labels = false;
                    // Reserve space for the graph column.
                    let (graph_rect, _) = ui.allocate_exact_size(
                        Vec2::new(graph_col_width, row_height),
                        egui::Sense::hover(),
                    );

                    // Draw graph elements into the reserved rect.
                    if let Some(graph_row) = app.graph_rows.get(idx) {
                        paint_graph_row(ui, graph_rect, graph_row, row_height);
                    }

                    // Ref labels (branches and tags)
                    for ref_name in &refs {
                        let (label_text, bg_color, text_color) = if ref_name.starts_with("tag: ") {
                            let tag = ref_name.trim_start_matches("tag: ");
                            (
                                format!("<{}>", tag),
                                egui::Color32::from_rgb(80, 60, 20),
                                egui::Color32::from_rgb(240, 200, 80),
                            )
                        } else if ref_name.contains('/') {
                            // Remote branch
                            (
                                format!("[{}]", ref_name),
                                egui::Color32::from_rgb(30, 60, 30),
                                egui::Color32::from_rgb(130, 220, 130),
                            )
                        } else {
                            // Local branch
                            (
                                format!("[{}]", ref_name),
                                egui::Color32::from_rgb(20, 50, 80),
                                egui::Color32::from_rgb(100, 180, 255),
                            )
                        };

                        let label = egui::RichText::new(&label_text)
                            .monospace()
                            .color(text_color)
                            .background_color(bg_color);
                        ui.label(label);
                    }

                    // Commit message
                    let msg_color = if is_selected {
                        egui::Color32::WHITE
                    } else {
                        egui::Color32::from_rgb(220, 220, 220)
                    };
                    let message_text = egui::RichText::new(&subject).monospace().color(msg_color);
                    ui.label(message_text);

                    // Push author and date to the right
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        // Date (rightmost)
                        let date_text = egui::RichText::new(&date)
                            .monospace()
                            .color(egui::Color32::from_rgb(140, 140, 140));
                        ui.label(date_text);

                        ui.label(
                            egui::RichText::new("|")
                                .monospace()
                                .color(egui::Color32::from_rgb(60, 60, 60)),
                        );

                        // Author
                        let author_text = egui::RichText::new(&author_name)
                            .monospace()
                            .color(egui::Color32::from_rgb(180, 160, 200));
                        ui.label(author_text);

                        ui.label(
                            egui::RichText::new("|")
                                .monospace()
                                .color(egui::Color32::from_rgb(60, 60, 60)),
                        );
                    });
                });

                // Place an invisible click-sensing rect over the entire row.
                let row_rect = response.response.rect;
                let row_id = ui.id().with(("commit_row", idx));
                let row_response = ui.interact(row_rect, row_id, egui::Sense::click());
                if row_response.clicked() {
                    clicked_index = Some(idx);
                }

                // Highlight selected row (paint behind the text)
                if is_selected {
                    ui.painter().rect_filled(
                        row_rect,
                        0.0,
                        egui::Color32::from_rgba_premultiplied(60, 80, 120, 80),
                    );
                }
            }
        });

    // Apply the click outside the borrow of commits
    if let Some(idx) = clicked_index {
        app.select_commit(idx);
    }
}

/// Paint the graph column for a single row: edges (lines) and the commit node (circle).
fn paint_graph_row(ui: &egui::Ui, rect: Rect, row: &graph::GraphRow, row_height: f32) {
    let painter = ui.painter_at(rect);
    let line_width = 1.8;

    let center_y = rect.top() + row_height * 0.5;

    // Helper: x position for a given lane column within the graph rect.
    let lane_x = |col: usize| -> f32 { rect.left() + LANE_WIDTH * 0.5 + col as f32 * LANE_WIDTH };

    // Draw edges first (behind the node).
    for edge in &row.edges {
        let color = graph::lane_color(edge.color_index);
        let stroke = Stroke::new(line_width, color);

        let from_x = lane_x(edge.from_col);
        let to_x = lane_x(edge.to_col);

        if edge.from_col == edge.to_col {
            // Straight vertical line through the full row height.
            painter.line_segment(
                [
                    Pos2::new(from_x, rect.top()),
                    Pos2::new(to_x, rect.bottom()),
                ],
                stroke,
            );
        } else {
            // Diagonal connector: go from (from_x, center) to (to_x, bottom).
            // Draw in two segments for a smoother look:
            // 1. Vertical from top to center (at from_x).
            // 2. Diagonal from center to bottom (from from_x to to_x).
            painter.line_segment(
                [Pos2::new(from_x, rect.top()), Pos2::new(from_x, center_y)],
                stroke,
            );
            painter.line_segment(
                [Pos2::new(from_x, center_y), Pos2::new(to_x, rect.bottom())],
                stroke,
            );
        }
    }

    // Draw the commit node circle on top of the edges.
    let node_x = lane_x(row.node_col);
    let node_center = Pos2::new(node_x, center_y);
    let node_color = graph::lane_color(row.node_color_index);

    painter.circle_filled(node_center, NODE_RADIUS, node_color);
    // Dark outline to make the node pop against the lines.
    painter.circle_stroke(
        node_center,
        NODE_RADIUS,
        Stroke::new(1.0, egui::Color32::from_rgb(30, 30, 30)),
    );
}
