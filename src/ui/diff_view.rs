use egui::scroll_area::ScrollSource;
use egui::{self, Color32, RichText, ScrollArea, Ui};

/// Renders the diff view pane (bottom-left).
///
/// Shows the unified diff for the selected commit. Diff lines are color-coded:
/// - Green for additions (`+`)
/// - Red for deletions (`-`)
/// - Blue/bold for file headers (`diff --git`, `---`, `+++`)
/// - Dim for hunk headers (`@@`)
///
/// Uses `show_rows()` to only lay out the lines currently visible on screen,
/// which keeps large diffs fast.
pub fn show(ui: &mut Ui, lines: &[String], scroll_to_line: &mut Option<usize>) {
    if lines.is_empty() {
        ui.centered_and_justified(|ui| {
            ui.label(
                RichText::new("Select a commit to view its diff").color(Color32::from_gray(120)),
            );
        });
        return;
    }

    let text_height = ui.text_style_height(&egui::TextStyle::Monospace);
    let item_spacing_y = ui.spacing().item_spacing.y;
    // show_rows() expects the row height WITHOUT spacing — it adds
    // item_spacing.y internally. The actual step between row tops
    // (used for scroll offset math) is text_height + item_spacing.y.
    let row_height_sans_spacing = text_height;
    let row_step = text_height + item_spacing_y;
    let num_lines = lines.len();

    let mut scroll_area = ScrollArea::both()
        .id_salt("diff_scroll")
        .auto_shrink([false, false])
        .scroll_source(ScrollSource {
            scroll_bar: true,
            drag: false,
            mouse_wheel: true,
        });

    // If a scroll-to-line request is pending, set the vertical offset directly.
    if let Some(target_line) = scroll_to_line.take() {
        let offset = target_line as f32 * row_step;
        scroll_area = scroll_area.vertical_scroll_offset(offset);
    }

    scroll_area.show_rows(ui, row_height_sans_spacing, num_lines, |ui, row_range| {
        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);

        for idx in row_range {
            if let Some(line) = lines.get(idx) {
                let rich = colorize_diff_line(line);
                ui.label(rich);
            }
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
