use crate::app::KitsuneSettings;
use crate::theme;
use egui::{Ui, RichText, Rounding, Margin, Frame, FontId, Stroke};

pub fn render_settings_panel(ui: &mut Ui, settings: &mut KitsuneSettings) {
    ui.add_space(8.0);
    ui.heading(RichText::new("Settings").font(FontId::proportional(22.0)).strong());
    ui.add_space(20.0);

    let mut changed = false;

    Frame::none()
        .fill(theme::BG_SURFACE)
        .stroke(Stroke::new(1.0, theme::BORDER))
        .rounding(Rounding::same(8.0))
        .inner_margin(Margin::same(24.0))
        .show(ui, |ui| {
            ui.vertical(|ui| {
                ui.label(RichText::new("Privacy and Security").font(FontId::proportional(16.0)).strong());
                ui.add_space(16.0);
                
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        if ui.checkbox(&mut settings.telemetry, "Send anonymous usage data").changed() {
                            changed = true;
                        }
                        ui.label(RichText::new("Helps improve Kitsune Engine stability and performance.").font(FontId::proportional(12.0)).color(theme::TEXT_MUTED));
                    });
                });
                
                ui.add_space(20.0);
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        if ui.checkbox(&mut settings.auto_update, "Install updates automatically").changed() {
                            changed = true;
                        }
                        ui.label(RichText::new("Keeps your browser secure and up to date.").font(FontId::proportional(12.0)).color(theme::TEXT_MUTED));
                    });
                });
                
                ui.add_space(32.0);
                ui.separator();
                ui.add_space(32.0);
                
                ui.label(RichText::new("Data Management").font(FontId::proportional(16.0)).strong());
                ui.add_space(16.0);
                
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 12.0;
                    if ui.add(egui::Button::new(RichText::new("Clear Browsing Data").font(FontId::proportional(13.0))).rounding(Rounding::same(4.0))).clicked() { /* ... */ }
                    if ui.add(egui::Button::new(RichText::new("Export Vault Data").font(FontId::proportional(13.0))).rounding(Rounding::same(4.0))).clicked() { /* ... */ }
                });
            });
        });

    if changed {
        settings.save();
    }
}
