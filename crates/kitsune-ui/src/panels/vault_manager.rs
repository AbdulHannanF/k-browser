use egui::{Ui, RichText, FontId, Rounding, Margin, Stroke};
use crate::theme::{self, KitsuneTheme};

pub fn render_vault_manager(ui: &mut Ui, _theme: &KitsuneTheme) {
    ui.add_space(8.0);
    ui.heading(RichText::new("Secure Vault").font(FontId::proportional(20.0)).strong());
    ui.add_space(16.0);

    ui.label(RichText::new("Hardware-Backed Storage").font(FontId::proportional(14.0)).color(theme::TEXT_MUTED));
    ui.add_space(24.0);

    egui::Frame::none()
        .fill(theme::BG_ELEVATED)
        .stroke(Stroke::new(1.0, theme::BORDER))
        .rounding(Rounding::same(8.0))
        .inner_margin(Margin::same(32.0))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.vertical_centered(|ui| {
                ui.label(RichText::new("🔒").font(FontId::proportional(48.0)).color(theme::ERROR));
                ui.add_space(16.0);
                ui.label(RichText::new("Vault is Locked").font(FontId::proportional(16.0)).strong());
                ui.add_space(8.0);
                ui.label(RichText::new("Authentication required to access saved passwords and identities").font(FontId::proportional(13.0)).color(theme::TEXT_MUTED));
                ui.add_space(24.0);
                
                let btn = ui.add_sized([200.0, 36.0], egui::Button::new(RichText::new("Unlock Vault").font(FontId::proportional(14.0)).strong())
                    .fill(theme::ACCENT)
                    .stroke(Stroke::new(1.0, theme::ACCENT))
                    .rounding(Rounding::same(6.0)));
                
                if btn.clicked() {
                    // Unlock logic
                }
            });
        });

    ui.add_space(32.0);
    ui.label(RichText::new("Saved Identities").font(FontId::proportional(16.0)).strong());
    ui.add_space(12.0);
    
    // Dummy entries
    vault_entry(ui, "Primary Identity", "Personal", theme::SUCCESS);
    ui.add_space(8.0);
    vault_entry(ui, "Anonymous Persona 1", "Private", theme::WARNING);
}

fn vault_entry(ui: &mut Ui, name: &str, type_str: &str, color: egui::Color32) {
    egui::Frame::none()
        .fill(theme::BG_SURFACE)
        .stroke(Stroke::new(1.0, theme::BORDER))
        .rounding(Rounding::same(6.0))
        .inner_margin(Margin::symmetric(16.0, 12.0))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.painter().circle_filled(ui.cursor().min + egui::vec2(6.0, 10.0), 4.0, color);
                ui.add_space(16.0);
                ui.label(RichText::new(name).font(FontId::proportional(14.0)));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.add(egui::Button::new(RichText::new("Manage").font(FontId::proportional(12.0))).rounding(Rounding::same(4.0))).clicked() {}
                    ui.label(RichText::new(type_str).font(FontId::proportional(13.0)).color(theme::TEXT_MUTED));
                });
            });
        });
}
