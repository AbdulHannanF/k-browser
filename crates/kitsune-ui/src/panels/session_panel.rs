use eframe::egui;
use crate::theme::KitsuneTheme;

pub fn session_panel(ctx: &egui::Context) {
    egui::SidePanel::right("session_panel")
        .resizable(true)
        .default_width(200.0)
        .frame(egui::Frame::none().fill(KitsuneTheme::BG_PANEL))
        .show(ctx, |ui| {
            ui.add_space(8.0);
            // Panel header
            ui.horizontal(|ui| {
                ui.add_space(8.0);
                ui.label(egui::RichText::new("Session").strong().color(KitsuneTheme::TEXT_PRIMARY));
            });
            ui.add_space(8.0);
            ui.separator();

            // Status rows
            ui.add_space(8.0);
            ui.label("Status rows placeholder");

            // Capabilities section
            ui.add_space(8.0);
            ui.label("Capabilities section placeholder");

            // Vault mini section
            ui.add_space(8.0);
            ui.label("Vault mini section placeholder");
        });
}
