use crate::theme;
use crate::theme::KitsuneTheme;
use chrono::{DateTime, Local};
use egui::{
    Align, Align2, Color32, FontId, Frame, Layout, Margin, Rect, RichText, Rounding, ScrollArea,
    Stroke, Ui, Vec2,
};
use kitsune_agent::runtime::AgentStatus;
use kitsune_agent::spec::AgentId;
use std::collections::VecDeque;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActivityEntryType {
    Info,
    Reading,
    Acting,
    Success,
    Warning,
    Error,
}

#[derive(Debug, Clone)]
pub struct AgentActivityEntry {
    pub timestamp: DateTime<Local>,
    pub message: String,
    pub entry_type: ActivityEntryType,
}

#[derive(Debug, Clone)]
pub struct AgentCardState {
    pub agent_id: AgentId,
    pub template_name: String,
    pub status: AgentStatus,
    pub budget_used_usd: f32,
    pub budget_limit_usd: f32,
    pub activity_log: VecDeque<AgentActivityEntry>, // max 50
    pub is_expanded: bool,
}

impl Default for AgentCardState {
    fn default() -> Self {
        Self {
            agent_id: AgentId::new(),
            template_name: String::from("New Agent"),
            status: AgentStatus::Idle,
            budget_used_usd: 0.0,
            budget_limit_usd: 1.0,
            activity_log: VecDeque::with_capacity(50),
            is_expanded: false,
        }
    }
}

impl AgentCardState {
    pub fn add_activity(&mut self, entry: AgentActivityEntry) {
        if self.activity_log.len() == 50 {
            self.activity_log.pop_front();
        }
        self.activity_log.push_back(entry);
    }
}

pub fn render_agent_shelf(
    ctx: &egui::Context,
    _theme: &KitsuneTheme,
    shelf_open_t: f32,
    agent_cards: &mut [AgentCardState],
    mut on_new_agent: impl FnMut(),
) {
    if shelf_open_t <= 0.0 {
        return;
    }

    let shelf_width = 340.0;
    let screen_rect = ctx.screen_rect();
    let offset_x = shelf_width * (1.0 - shelf_open_t);

    let panel_rect = Rect::from_min_size(
        egui::pos2(screen_rect.max.x - shelf_width + offset_x, screen_rect.min.y),
        Vec2::new(shelf_width, screen_rect.height()),
    );

    // Solid panel background
    ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::new("shelf_bg")))
        .rect_filled(panel_rect, Rounding::ZERO, theme::BG_SURFACE);

    // Left edge border
    ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::new("shelf_stroke")))
        .line_segment(
            [panel_rect.left_top(), panel_rect.left_bottom()],
            Stroke::new(1.0, theme::BORDER),
        );

    egui::Window::new("agent_shelf_window")
        .fixed_pos(panel_rect.min)
        .fixed_size(panel_rect.size())
        .title_bar(false)
        .resizable(false)
        .collapsible(false)
        .frame(Frame::none().inner_margin(Margin::same(20.0)))
        .show(ctx, |ui| {
            // Header
            ui.horizontal(|ui| {
                ui.label(RichText::new("🦊 Automation Agents").font(FontId::proportional(16.0)).strong().color(theme::TEXT_PRIMARY));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.button("≡").clicked() {} // Settings icon
                });
            });

            ui.add_space(8.0);
            ui.painter().line_segment(
                [ui.cursor().min, ui.cursor().min + egui::vec2(ui.available_width(), 0.0)],
                Stroke::new(1.0, theme::BORDER),
            );
            ui.add_space(16.0);

            // Agent Cards
            ScrollArea::vertical()
                .auto_shrink([false; 2])
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 12.0;

                    for card in agent_cards.iter_mut() {
                        render_agent_card(ui, card);
                    }

                    ui.add_space(8.0);

                    // Add Agent button
                    ui.vertical_centered(|ui| {
                        let btn = ui.add_sized(
                            [ui.available_width(), 36.0],
                            egui::Button::new(RichText::new("+ Create New Agent").font(FontId::proportional(14.0)))
                                .fill(theme::ACCENT.linear_multiply(0.8))
                                .stroke(Stroke::new(1.0, theme::ACCENT))
                                .rounding(Rounding::same(6.0))
                        );
                        if btn.clicked() {
                            on_new_agent();
                        }
                    });
                });
        });
}

fn render_agent_card(ui: &mut Ui, state: &mut AgentCardState) {
    let is_running = state.status == AgentStatus::Running;

    let stroke = if state.is_expanded {
        Stroke::new(1.5, theme::ACCENT)
    } else {
        Stroke::new(1.0, theme::BORDER)
    };

    Frame::none()
        .fill(theme::BG_BASE)
        .rounding(Rounding::same(8.0))
        .stroke(stroke)
        .inner_margin(Margin::same(16.0))
        .show(ui, |ui| {
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    // Status Indicator
                    let dot_color = get_status_color(&state.status, ui);
                    let (dot_rect, _) = ui.allocate_exact_size(Vec2::splat(12.0), egui::Sense::hover());
                    ui.painter().circle_filled(dot_rect.center(), 4.0, dot_color);

                    if is_running {
                        let t = ui.input(|i| i.time as f32 * 2.0);
                        let ring_radius = 5.0 + (t.cos().abs() * 3.0);
                        ui.painter().circle_stroke(dot_rect.center(), ring_radius, Stroke::new(1.5, dot_color.linear_multiply(0.4)));
                        ui.ctx().request_repaint();
                    }

                    ui.add_space(8.0);

                    let title_resp = ui.add(egui::Label::new(
                        RichText::new(&state.template_name).font(FontId::proportional(15.0)).color(theme::TEXT_PRIMARY)
                    ).sense(egui::Sense::click()));

                    if title_resp.clicked() {
                        state.is_expanded = !state.is_expanded;
                    }

                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(RichText::new(status_text(&state.status)).size(12.0).color(theme::TEXT_MUTED));
                    });
                });

                ui.add_space(12.0);
                render_budget_gauge(ui, state.budget_used_usd, state.budget_limit_usd);

                if state.is_expanded {
                    ui.add_space(12.0);
                    ui.painter().line_segment(
                        [ui.cursor().min, ui.cursor().min + egui::vec2(ui.available_width(), 0.0)],
                        Stroke::new(1.0, theme::BORDER),
                    );
                    ui.add_space(10.0);

                    render_activity_feed(ui, &state.activity_log);

                    ui.add_space(12.0);
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 8.0;
                        if ui.add(egui::Button::new(RichText::new("Pause").font(FontId::proportional(12.0))).rounding(Rounding::same(4.0))).clicked() { /* TODO */ }
                        if ui.add(egui::Button::new(RichText::new("Stop").font(FontId::proportional(12.0))).rounding(Rounding::same(4.0)).fill(theme::ERROR.linear_multiply(0.3))).clicked() { /* TODO */ }
                    });
                }
            });
        });
}

fn get_status_color(status: &AgentStatus, _ui: &mut Ui) -> Color32 {
    match status {
        AgentStatus::Idle => theme::TEXT_MUTED,
        AgentStatus::Running => theme::SUCCESS,
        AgentStatus::WaitingForUser => theme::WARNING,
        AgentStatus::Completed => theme::SUCCESS,
        AgentStatus::Failed => theme::ERROR,
        AgentStatus::Paused => theme::WARNING,
    }
}

fn status_text(status: &AgentStatus) -> &'static str {
    match status {
        AgentStatus::Idle => "Idle",
        AgentStatus::Running => "Active",
        AgentStatus::WaitingForUser => "Waiting",
        AgentStatus::Completed => "Done",
        AgentStatus::Failed => "Error",
        AgentStatus::Paused => "Paused",
    }
}

fn render_budget_gauge(ui: &mut Ui, used: f32, limit: f32) {
    let pct = (used / limit).clamp(0.0, 1.0);
    ui.vertical(|ui| {
        ui.horizontal(|ui| {
            ui.label(RichText::new("Usage").size(11.0).color(theme::TEXT_MUTED));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.label(RichText::new(format!("${:.2} / ${:.2}", used, limit)).size(11.0).color(theme::TEXT_PRIMARY));
            });
        });
        ui.add_space(4.0);

        let bar_height = 4.0;
        let (_, rect) = ui.allocate_space(Vec2::new(ui.available_width(), bar_height));
        ui.painter().rect_filled(rect, Rounding::same(2.0), theme::BG_ELEVATED);

        let fill_width = rect.width() * pct;
        let fill_rect = Rect::from_min_size(rect.min, Vec2::new(fill_width, bar_height));
        let color = if pct < 0.8 { theme::ACCENT } else { theme::ERROR };
        ui.painter().rect_filled(fill_rect, Rounding::same(2.0), color);
    });
}

fn render_activity_feed(ui: &mut Ui, log: &VecDeque<AgentActivityEntry>) {
    ScrollArea::vertical()
        .max_height(140.0)
        .stick_to_bottom(true)
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 8.0;
            for entry in log {
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        let dot_color = match entry.entry_type {
                            ActivityEntryType::Info => theme::TEXT_MUTED,
                            ActivityEntryType::Reading => theme::AGENT_READING,
                            ActivityEntryType::Acting => theme::AGENT_ACTING,
                            ActivityEntryType::Success => theme::SUCCESS,
                            ActivityEntryType::Warning => theme::WARNING,
                            ActivityEntryType::Error => theme::ERROR,
                        };
                        ui.painter().circle_filled(ui.cursor().min + Vec2::new(3.0, 8.0), 3.0, dot_color);
                        ui.add_space(10.0);
                        ui.label(RichText::new(&entry.message).size(12.0).color(theme::TEXT_PRIMARY));
                    });
                    ui.horizontal(|ui| {
                        ui.add_space(14.0);
                        ui.label(RichText::new(entry.timestamp.format("%H:%M:%S").to_string()).size(10.0).color(theme::TEXT_MUTED));
                    });
                });
            }
            if log.is_empty() {
                ui.centered_and_justified(|ui| {
                    ui.label(RichText::new("No activity yet").size(12.0).color(theme::TEXT_MUTED));
                });
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_card_state_default() {
        let default = AgentCardState::default();
        assert!(default.activity_log.is_empty());
        assert!(!default.is_expanded);
        assert_eq!(default.budget_used_usd, 0.0);
    }
}
