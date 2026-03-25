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

                for (i, path) in files.iter().enumerate() {
                    let is_selected = selected_file == Some(i);

                    let text = format_file_path(path, is_selected);
                    let button = egui::Button::new(text).frame(false).selected(is_selected);

                    if ui.add(button).clicked() {
                        clicked = Some(i);
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
