use egui::scroll_area::ScrollSource;
use egui::{self, Color32, RichText, ScrollArea, Ui};

/// Renders the diff view pane (bottom-left).
///
/// Shows the unified diff for the selected commit. Diff lines are color-coded:
/// - Green for additions (`+`)
/// - Red for deletions (`-`)
/// - Blue/bold for file headers (`diff --git`, `---`, `+++`)
/// - Dim for hunk headers (`@@`)
pub fn show(ui: &mut Ui, diff_text: &str, scroll_to_file: &mut Option<String>) {
    if diff_text.is_empty() {
        ui.centered_and_justified(|ui| {
            ui.label(
                RichText::new("Select a commit to view its diff").color(Color32::from_gray(120)),
            );
        });
        return;
    }

    ScrollArea::both()
        .id_salt("diff_scroll")
        .auto_shrink([false, false])
        .scroll_source(ScrollSource {
            scroll_bar: true,
            drag: false,
            mouse_wheel: true,
        })
        .show(ui, |ui| {
            ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);

            for line in diff_text.lines() {
                // Check if this line is a file header that we should scroll to
                if let Some(target) = scroll_to_file.as_ref()
                    && line.starts_with("diff --git")
                    && line.contains(target.as_str())
                {
                    ui.scroll_to_cursor(Some(egui::Align::TOP));
                    *scroll_to_file = None;
                }

                let rich = colorize_diff_line(line);
                ui.label(rich);
            }
        });
}

fn colorize_diff_line(line: &str) -> RichText {
    let add_color = Color32::from_rgb(80, 200, 80);
    let remove_color = Color32::from_rgb(220, 80, 80);
    let header_color = Color32::from_rgb(100, 160, 255);
    let hunk_color = Color32::from_rgb(180, 140, 220);
    let context_color = Color32::from_gray(180);

    if line.starts_with("diff --git") {
        RichText::new(line).color(header_color).strong().monospace()
    } else if line.starts_with("---") || line.starts_with("+++") {
        RichText::new(line).color(header_color).monospace()
    } else if line.starts_with("@@") {
        RichText::new(line).color(hunk_color).monospace()
    } else if line.starts_with('+') {
        RichText::new(line).color(add_color).monospace()
    } else if line.starts_with('-') {
        RichText::new(line).color(remove_color).monospace()
    } else if line.starts_with("index ")
        || line.starts_with("new file")
        || line.starts_with("deleted file")
    {
        RichText::new(line)
            .color(Color32::from_gray(100))
            .monospace()
    } else {
        RichText::new(line).color(context_color).monospace()
    }
}
