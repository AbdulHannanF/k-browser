use crate::theme::KitsuneTheme;
use eframe::egui;
use kitsune_agent::swarm::types::{SwarmState, TaskStatus};

// Retained for future orchestrator task graph use.
#[derive(Debug, Clone, PartialEq)]
pub enum NodeStatus {
    Pending,
    Running,
    Completed,
    Failed(String),
}

#[derive(Debug, Clone)]
pub struct TaskNode {
    pub name: String,
    pub model_slot: String,
    pub tokens_used: Option<u32>,
    pub status: NodeStatus,
    pub summary: Option<String>,
}

impl TaskNode {
    pub fn new(name: impl Into<String>, model_slot: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            model_slot: model_slot.into(),
            tokens_used: None,
            status: NodeStatus::Pending,
            summary: None,
        }
    }
}

pub fn task_graph_panel(ui: &mut egui::Ui, swarm_state: &Option<SwarmState>) {
    ui.heading(egui::RichText::new("Task Graph").color(KitsuneTheme::TEXT_PRIMARY));
    ui.separator();

    let Some(state) = swarm_state else {
        ui.label(egui::RichText::new("No active swarm.").color(KitsuneTheme::TEXT2));
        return;
    };

    // Summary bar
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(format!("Goal: {}", state.goal)).color(KitsuneTheme::TEXT_PRIMARY));
    });
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(format!(
                "Workers: {} active · {} done · {} pending · {} tool calls",
                state.active_count(),
                state.completed_count(),
                state.pending_count(),
                state.total_tool_calls,
            ))
            .color(KitsuneTheme::TEXT2)
            .size(11.0),
        );
    });
    ui.separator();

    if state.tasks.is_empty() {
        ui.label(egui::RichText::new("Planning…").color(KitsuneTheme::TEXT2));
        ui.spinner();
        return;
    }

    for task in &state.tasks {
        let (icon, color) = match &task.status {
            TaskStatus::Pending => ("○", KitsuneTheme::TEXT3),
            TaskStatus::Running => ("●", KitsuneTheme::AMBER),
            TaskStatus::Completed(_) => ("✓", KitsuneTheme::GREEN),
            TaskStatus::Failed(_) => ("✗", KitsuneTheme::RED),
            TaskStatus::Cancelled => ("⬛", KitsuneTheme::TEXT3),
        };

        ui.horizontal(|ui| {
            ui.colored_label(color, icon);
            ui.label(
                egui::RichText::new(task.role.as_str())
                    .color(KitsuneTheme::TEXT_PRIMARY)
                    .strong(),
            );
            ui.label(
                egui::RichText::new(format!("[{}]", task.id))
                    .color(KitsuneTheme::TEXT2)
                    .size(11.0),
            );
            if task.tool_calls_used > 0 {
                ui.label(
                    egui::RichText::new(format!("{}t", task.tool_calls_used))
                        .color(KitsuneTheme::TEXT3)
                        .size(11.0),
                );
            }
            if matches!(task.status, TaskStatus::Running) {
                ui.spinner();
            }
        });

        if let Some(msg) = &task.last_message {
            ui.indent(format!("task_msg_{}", task.id), |ui| {
                ui.label(
                    egui::RichText::new(if msg.len() > 120 {
                        format!("{}…", &msg[..120])
                    } else {
                        msg.clone()
                    })
                    .color(KitsuneTheme::TEXT2)
                    .size(11.0),
                );
            });
        }

        if let TaskStatus::Failed(e) = &task.status {
            ui.indent(format!("task_err_{}", task.id), |ui| {
                ui.colored_label(KitsuneTheme::RED, e);
            });
        }
    }

    // Final answer section
    if let Some(answer) = &state.final_answer {
        ui.separator();
        ui.label(egui::RichText::new("Final Answer").color(KitsuneTheme::TEXT_PRIMARY).strong());
        egui::ScrollArea::vertical()
            .id_salt("swarm_answer_scroll")
            .max_height(120.0)
            .show(ui, |ui| {
                ui.label(egui::RichText::new(answer).color(KitsuneTheme::TEXT2).size(11.0));
            });
        if ui.small_button("Copy").clicked() {
            ui.output_mut(|o| o.copied_text = answer.clone());
        }
    }
}
