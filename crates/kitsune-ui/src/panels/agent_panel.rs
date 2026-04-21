use eframe::egui;
use crate::theme::KitsuneTheme;
use crate::app::KitsuneBrowser;

pub fn agent_panel(ctx: &egui::Context, browser: &mut KitsuneBrowser) {
    egui::SidePanel::left("agent_panel")
        .resizable(true)
        .default_width(300.0)
        .frame(egui::Frame::none().fill(KitsuneTheme::BG_PANEL))
        .show(ctx, |ui| {
            ui.add_space(8.0);
            // Panel header
            ui.horizontal(|ui| {
                ui.add_space(8.0);
                ui.label(egui::RichText::new("Agent Workspace").strong().color(KitsuneTheme::TEXT_PRIMARY));
                // TODO: Add live status dot
            });
            ui.add_space(8.0);
            ui.separator();

            // Command input
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.add_space(8.0);
                let command_input = egui::TextEdit::singleline(&mut browser.agent_command)
                    .desired_width(ui.available_width() - 40.0)
                    .hint_text("Ask agent to do anything…");
                ui.add(command_input);

                if ui.button("▶").clicked() {
                    // TODO: Run agent
                }
                ui.add_space(8.0);
            });


            // Agent cards
            ui.add_space(8.0);
            ui.label("Agent cards placeholder");

            // Agent log
            ui.add_space(8.0);
            ui.label("Agent log placeholder");

            // Budget gauge
            ui.add_space(8.0);
            ui.label("Budget gauge placeholder");
        });
}
