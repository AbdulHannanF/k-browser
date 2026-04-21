use eframe::egui;
use crate::theme::KitsuneTheme;
use crate::app::KitsuneBrowser;

pub fn top_bar(ctx: &egui::Context, browser: &mut KitsuneBrowser) {
    egui::TopBottomPanel::top("chrome")
        .frame(egui::Frame::none().fill(KitsuneTheme::BG_PANEL).inner_margin(6.0))
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                // Logo
                ui.colored_label(KitsuneTheme::AMBER, "🦊");
                ui.label(egui::RichText::new("Kitsune").strong().color(KitsuneTheme::TEXT_PRIMARY));
                ui.separator();

                // Nav buttons
                if ui.small_button("◀").clicked() { /* self.go_back(); */ }
                if ui.small_button("▶").clicked() { /* self.go_forward(); */ }
                if ui.small_button("↻").clicked() { /* self.reload(); */ }

                // Address bar
                let addr = egui::TextEdit::singleline(&mut browser.address_bar)
                    .desired_width(ui.available_width() - 120.0)
                    .frame(true)
                    .hint_text("Enter URL...");
                let r = ui.add(addr);
                if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    // self.navigate_to(self.address_bar.clone());
                }

                // Privacy pill
                let blocked = 0; // self.privacy_stats.trackers_blocked;
                let pill_color = if blocked > 0 { KitsuneTheme::GREEN_SAFE } else { KitsuneTheme::TEXT_MUTED };
                ui.colored_label(pill_color, format!("🛡 {blocked} blocked"));
            });
        });
}
