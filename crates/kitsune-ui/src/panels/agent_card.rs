use crate::animation::lerp_anim;
use crate::theme::{colors, KitsuneTheme};
use eframe::egui;

#[derive(Clone, PartialEq)]
pub enum AgentStatus {
    Idle,
    Running,
    Done,
    Error,
}

pub struct AgentCard {
    pub icon: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub status: AgentStatus,
    pub swarm_badge: bool,
}

impl AgentCard {
    pub fn render(&self, ui: &mut egui::Ui, selected: bool) -> bool {
        let is_running = self.status == AgentStatus::Running;
        let card_id = egui::Id::new(self.name).with("card");

        // Lerp hover brightness (0.0 = idle, 1.0 = hovered)
        let hov_raw = ui.ctx().data(|d| d.get_temp::<bool>(card_id).unwrap_or(false));
        let hov_t = lerp_anim(ui.ctx(), card_id.with("hov_t"), if hov_raw { 1.0 } else { 0.0 }, 10.0);

        let (badge_col, badge_txt) = match self.status {
            AgentStatus::Idle    => (KitsuneTheme::TEXT2,  "idle"),
            AgentStatus::Running => (KitsuneTheme::AMBER,  "running"),
            AgentStatus::Done    => (KitsuneTheme::GREEN,  "done"),
            AgentStatus::Error   => (KitsuneTheme::RED,    "error"),
        };

        // Base fill lerps between BG_CARD and BG_ELEVATED on hover
        let fill = if selected || is_running {
            KitsuneTheme::AMBER_DIM
        } else {
            // Linearly interpolate between BG_CARD and BG_ELEVATED
            let base = colors::BG_CARD;
            let high = colors::BG_ELEVATED;
            egui::Color32::from_rgb(
                (base.r() as f32 + (high.r() as f32 - base.r() as f32) * hov_t) as u8,
                (base.g() as f32 + (high.g() as f32 - base.g() as f32) * hov_t) as u8,
                (base.b() as f32 + (high.b() as f32 - base.b() as f32) * hov_t) as u8,
            )
        };

        let stroke_col = if selected || is_running {
            KitsuneTheme::BORDER_AMBER
        } else if hov_raw {
            KitsuneTheme::BORDER2
        } else {
            KitsuneTheme::BORDER
        };

        let resp = egui::Frame::none()
            .fill(fill)
            .rounding(egui::Rounding::same(7.0))
            .stroke(egui::Stroke::new(1.0, stroke_col))
            .inner_margin(egui::Margin::symmetric(10.0, 8.0))
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                ui.horizontal(|ui| {
                    // Icon
                    ui.label(egui::RichText::new(self.icon).size(15.0).color(KitsuneTheme::TEXT1));
                    ui.add_space(6.0);

                    ui.vertical(|ui| {
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new(self.name)
                                    .strong()
                                    .size(12.0)
                                    .color(KitsuneTheme::TEXT_PRIMARY),
                            );
                            ui.add_space(4.0);

                            // Status badge
                            let badge_r = badge_col.r();
                            let badge_g = badge_col.g();
                            let badge_b = badge_col.b();
                            egui::Frame::none()
                                .fill(egui::Color32::from_rgba_premultiplied(
                                    badge_r / 8, badge_g / 8, badge_b / 8, 30,
                                ))
                                .rounding(egui::Rounding::same(20.0))
                                .inner_margin(egui::Margin::symmetric(6.0, 1.0))
                                .show(ui, |ui| {
                                    ui.label(
                                        egui::RichText::new(badge_txt)
                                            .size(9.0)
                                            .color(badge_col)
                                            .family(egui::FontFamily::Monospace),
                                    );
                                });

                            // SWARM badge
                            if self.swarm_badge {
                                ui.add_space(3.0);
                                egui::Frame::none()
                                    .fill(egui::Color32::from_rgba_premultiplied(29, 14, 3, 40))
                                    .rounding(egui::Rounding::same(20.0))
                                    .stroke(egui::Stroke::new(1.0, KitsuneTheme::BORDER_AMBER))
                                    .inner_margin(egui::Margin::symmetric(6.0, 1.0))
                                    .show(ui, |ui| {
                                        ui.label(
                                            egui::RichText::new("SWARM")
                                                .size(8.5)
                                                .color(KitsuneTheme::AMBER)
                                                .strong()
                                                .family(egui::FontFamily::Monospace),
                                        );
                                    });
                            }
                        });
                        ui.label(
                            egui::RichText::new(self.description)
                                .size(10.5)
                                .color(KitsuneTheme::TEXT2),
                        );
                    });
                });
            })
            .response;

        // Left accent strip when selected or running
        if selected || is_running {
            let strip = egui::Rect::from_min_size(
                resp.rect.left_top(),
                egui::vec2(2.0, resp.rect.height()),
            );
            ui.painter().rect_filled(strip, egui::Rounding::ZERO, KitsuneTheme::AMBER);
        }

        let card_resp = ui.interact(resp.rect, card_id, egui::Sense::click());
        ui.ctx().data_mut(|d| d.insert_temp(card_id, card_resp.hovered()));
        card_resp.clicked()
    }
}
