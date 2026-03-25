use egui;

use crate::app::App;

/// Renders the commit list in the top pane.
/// Each row shows: graph placeholder | refs | message | author | date
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

    // Track which commit the user clicked this frame (if any).
    let mut clicked_index: Option<usize> = None;

    egui::ScrollArea::vertical()
        .id_salt("commit_list_scroll")
        .auto_shrink([false, false])
        .show_rows(ui, row_height, num_commits, |ui, row_range| {
            for idx in row_range {
                let is_selected = app.selected_index == Some(idx);

                // We need to read commit fields before any mutable borrow.
                let commit = &app.commits[idx];
                let refs = commit.refs.clone();
                let subject = commit.subject.clone();
                let author_name = commit.author_name.clone();
                let date = commit.date.clone();

                let response = ui.horizontal(|ui| {
                    // Graph placeholder column — just show * for now.
                    // Milestone 4 will replace this with real graph lines.
                    let graph_text = egui::RichText::new("  *  ")
                        .monospace()
                        .color(egui::Color32::from_rgb(120, 180, 255));
                    ui.label(graph_text);

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

                // Handle selection
                let row_response = response.response.interact(egui::Sense::click());
                if row_response.clicked() {
                    clicked_index = Some(idx);
                }

                // Highlight selected row (paint behind the text)
                if is_selected {
                    let rect = row_response.rect;
                    ui.painter().rect_filled(
                        rect,
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
