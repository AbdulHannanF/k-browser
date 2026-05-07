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
    /// Returns true if the run (▶) button was clicked.
    pub fn render(&self, ui: &mut egui::Ui) -> bool {
        let run_clicked = false;
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

        // Hover glow effect
        if resp.hovered() {
            ui.painter().rect_stroke(
                resp.rect,
                egui::Rounding::same(7.0),
                egui::Stroke::new(1.0, KitsuneTheme::BORDER2),
            );
        }

        run_clicked
    }
}
