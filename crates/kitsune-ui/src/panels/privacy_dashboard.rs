use egui::{Ui, RichText, FontId, Rounding, Margin};
use crate::theme::{self, KitsuneTheme};
use std::sync::{Arc, Mutex};
use kitsune_core::engine::KitsuneEngine;

pub fn render_privacy_dashboard(ui: &mut Ui, _engine: &Option<Arc<Mutex<KitsuneEngine>>>, _theme: &KitsuneTheme) {
    ui.add_space(8.0);
    ui.heading(RichText::new("Privacy & Security").font(FontId::proportional(20.0)).strong());
    ui.add_space(16.0);

    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 12.0;
        stat_card(ui, "Trackers Blocked", "1,240", theme::SUCCESS);
        stat_card(ui, "Data Leaks Prevented", "42", theme::ACCENT);
        stat_card(ui, "Fingerprint Protection", "Active", theme::AGENT_ACTING);
    });

    ui.add_space(32.0);
    ui.label(RichText::new("Recent Activity").font(FontId::proportional(14.0)).strong());
    ui.add_space(8.0);
    
    egui::Frame::none()
        .fill(theme::BG_ELEVATED)
        .stroke(egui::Stroke::new(1.0, theme::BORDER))
        .rounding(Rounding::same(8.0))
        .inner_margin(Margin::same(16.0))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.vertical(|ui| {
                ui.label(RichText::new("• Blocked tracking cookie from google-analytics.com").font(FontId::proportional(13.0)).color(theme::TEXT_PRIMARY));
                ui.add_space(8.0);
                ui.label(RichText::new("• Prevented canvas fingerprinting attempt on example.com").font(FontId::proportional(13.0)).color(theme::TEXT_PRIMARY));
                ui.add_space(8.0);
                ui.label(RichText::new("• Hardware key rotation successful").font(FontId::proportional(13.0)).color(theme::TEXT_PRIMARY));
            });
        });
}

fn stat_card(ui: &mut Ui, label: &str, value: &str, color: egui::Color32) {
    egui::Frame::none()
        .fill(theme::BG_ELEVATED)
        .stroke(egui::Stroke::new(1.0, theme::BORDER))
        .rounding(Rounding::same(8.0))
        .inner_margin(Margin::same(16.0))
        .show(ui, |ui| {
            ui.set_width(200.0);
            ui.vertical(|ui| {
                ui.label(RichText::new(label).font(FontId::proportional(12.0)).color(theme::TEXT_MUTED));
                ui.add_space(8.0);
                ui.label(RichText::new(value).font(FontId::proportional(22.0)).strong().color(color));
            });
        });
}
