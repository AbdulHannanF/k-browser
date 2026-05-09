use crate::theme::KitsuneTheme;
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
}

impl AgentCard {
    /// Returns true if the card was clicked.
    pub fn render(&self, ui: &mut egui::Ui) -> bool {
        let (badge_col, badge_txt) = match self.status {
            AgentStatus::Idle => (KitsuneTheme::TEXT2, "idle"),
            AgentStatus::Running => (KitsuneTheme::AMBER, "running"),
            AgentStatus::Done => (KitsuneTheme::GREEN_SAFE, "done"),
            AgentStatus::Error => (KitsuneTheme::RED_BLOCKED, "error"),
        };
        let is_running = self.status == AgentStatus::Running;
        let fill = if is_running {
            KitsuneTheme::AMBER_DIM
        } else {
            KitsuneTheme::BG_CARD
        };
        let stroke_col = if is_running {
            KitsuneTheme::BORDER_AMBER
        } else {
            KitsuneTheme::BORDER
        };

        // Read last frame's hover state so the fill/stroke are correct this frame
        // (egui standard pattern: 1-frame delay is imperceptible at 60 fps).
        let card_id = egui::Id::new(self.name).with("card");
        let prev_hovered = ui.ctx().data(|d| d.get_temp::<bool>(card_id).unwrap_or(false));

        let actual_fill = if prev_hovered && !is_running {
            KitsuneTheme::BG3
        } else {
            fill
        };
        let actual_stroke = if prev_hovered {
            egui::Stroke::new(1.0, KitsuneTheme::AMBER)
        } else {
            egui::Stroke::new(1.0, stroke_col)
        };

        let resp = egui::Frame::none()
            .fill(actual_fill)
            .rounding(egui::Rounding::same(7.0))
            .stroke(actual_stroke)
            .inner_margin(egui::Margin::symmetric(10.0, 8.0))
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                ui.horizontal(|ui| {
                    // Icon
                    ui.label(egui::RichText::new(self.icon).size(15.0).color(KitsuneTheme::TEXT1));
                    ui.add_space(6.0);

                    // Name + description (takes remaining space)
                    ui.vertical(|ui| {
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new(self.name)
                                    .strong()
                                    .size(12.0)
                                    .color(KitsuneTheme::TEXT_PRIMARY),
                            );
                            ui.add_space(6.0);
                            // Status badge inline with name
                            egui::Frame::none()
                                .fill(egui::Color32::from_rgba_unmultiplied(
                                    badge_col.r(),
                                    badge_col.g(),
                                    badge_col.b(),
                                    30,
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

        // Compute full-card interaction and store hover state for next frame.
        let card_resp = ui.interact(resp.rect, card_id, egui::Sense::click());
        ui.ctx().data_mut(|d| d.insert_temp(card_id, card_resp.hovered()));
        card_resp.clicked()
    }
}
