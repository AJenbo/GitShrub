use egui;

/// Response from the file list widget.
pub struct FileListResponse {
    /// Index of the file the user clicked, if any.
    pub clicked_file_index: Option<usize>,
}

/// Renders the affected files list for the selected commit.
///
/// Shows a scrollable list of file paths. Clicking a file signals
/// the parent to scroll the diff view to that file's section.
pub fn show(ui: &mut egui::Ui, files: &[String], selected_file: Option<usize>) -> FileListResponse {
    let mut clicked = None;

    ui.vertical(|ui| {
        ui.label(
            egui::RichText::new("Files")
                .strong()
                .color(egui::Color32::from_rgb(200, 200, 200)),
        );
        ui.separator();

        egui::ScrollArea::vertical()
            .id_salt("files_scroll")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                if files.is_empty() {
                    ui.colored_label(egui::Color32::from_rgb(120, 120, 120), "No files changed");
                    return;
                }

                let available_width = ui.available_width();
                let row_height = ui.text_style_height(&egui::TextStyle::Monospace) + 4.0;

                for (i, path) in files.iter().enumerate() {
                    let is_selected = selected_file == Some(i);

                    let response = ui.horizontal(|ui| {
                        // Disable selectable labels so clicks pass through text
                        // to the row's interaction rect instead of being consumed.
                        ui.style_mut().interaction.selectable_labels = false;
                        ui.set_min_width(available_width);
                        ui.set_height(row_height);

                        let text = format_file_path(path, is_selected);
                        ui.label(text);
                    });

                    // Place an invisible click-sensing rect over the entire row.
                    let row_rect = response.response.rect;
                    let row_id = ui.id().with(("file_row", i));
                    let row_response = ui.interact(row_rect, row_id, egui::Sense::click());

                    if row_response.clicked() {
                        clicked = Some(i);
                    }

                    // Highlight selected row.
                    if is_selected {
                        ui.painter().rect_filled(
                            row_rect,
                            2.0,
                            egui::Color32::from_rgba_premultiplied(60, 80, 120, 80),
                        );
                    }

                    // Hover highlight.
                    if row_response.hovered() && !is_selected {
                        ui.painter().rect_filled(
                            row_rect,
                            2.0,
                            egui::Color32::from_rgba_premultiplied(50, 50, 70, 40),
                        );
                    }
                }
            });
    });

    FileListResponse {
        clicked_file_index: clicked,
    }
}

/// Format a file path for display.
/// The filename portion is brighter than the directory portion.
fn format_file_path(path: &str, is_selected: bool) -> egui::RichText {
    let color = if is_selected {
        egui::Color32::WHITE
    } else {
        egui::Color32::from_rgb(170, 200, 255)
    };

    egui::RichText::new(path).monospace().color(color)
}
