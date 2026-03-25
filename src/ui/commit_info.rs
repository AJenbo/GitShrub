use crate::git::Commit;

/// Renders the commit info bar (middle section).
/// Shows the full SHA, author, date, and commit message.
pub fn show(ui: &mut egui::Ui, commit: &Commit) {
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;

        ui.label(
            egui::RichText::new("Commit:")
                .strong()
                .color(egui::Color32::from_rgb(180, 180, 180)),
        );

        ui.label(
            egui::RichText::new(&commit.full_sha)
                .monospace()
                .color(egui::Color32::from_rgb(130, 170, 255)),
        );
    });

    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;

        ui.label(
            egui::RichText::new("Author:")
                .strong()
                .color(egui::Color32::from_rgb(180, 180, 180)),
        );

        ui.label(
            egui::RichText::new(format!("{} <{}>", commit.author_name, commit.author_email))
                .color(egui::Color32::from_rgb(200, 200, 200)),
        );

        ui.label(egui::RichText::new(&commit.date).color(egui::Color32::from_rgb(140, 140, 140)));
    });

    ui.add_space(4.0);

    // Subject line (bold)
    ui.label(
        egui::RichText::new(&commit.subject)
            .strong()
            .color(egui::Color32::from_rgb(220, 220, 220)),
    );

    // Body (if any)
    let body = commit.body.trim();
    if !body.is_empty() {
        ui.label(egui::RichText::new(body).color(egui::Color32::from_rgb(160, 160, 160)));
    }
}
