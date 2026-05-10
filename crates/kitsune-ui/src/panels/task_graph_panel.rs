use crate::theme::KitsuneTheme;
use eframe::egui;

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

pub fn task_graph_panel(ui: &mut egui::Ui, nodes: &[TaskNode]) {
    ui.heading(egui::RichText::new("Task Graph").color(KitsuneTheme::TEXT_PRIMARY));
    ui.separator();

    if nodes.is_empty() {
        ui.label(egui::RichText::new("No active task.").color(KitsuneTheme::TEXT2));
        return;
    }

    for node in nodes {
        let (icon, color) = match &node.status {
            NodeStatus::Pending => ("○", KitsuneTheme::TEXT3),
            NodeStatus::Running => ("●", KitsuneTheme::AMBER),
            NodeStatus::Completed => ("✓", KitsuneTheme::GREEN),
            NodeStatus::Failed(_) => ("✗", KitsuneTheme::RED),
        };

        ui.horizontal(|ui| {
            ui.colored_label(color, icon);
            ui.label(egui::RichText::new(&node.name).color(KitsuneTheme::TEXT_PRIMARY).strong());
            ui.label(
                egui::RichText::new(format!("[{}]", node.model_slot))
                    .color(KitsuneTheme::TEXT2)
                    .size(11.0),
            );
            if let Some(t) = node.tokens_used {
                ui.label(
                    egui::RichText::new(format!("{t}t"))
                        .color(KitsuneTheme::TEXT3)
                        .size(11.0),
                );
            }
            if let NodeStatus::Running = &node.status {
                ui.spinner();
            }
            if let NodeStatus::Failed(e) = &node.status {
                ui.colored_label(KitsuneTheme::RED, e);
            }
        });

        if let Some(summary) = &node.summary {
            ui.indent("task_summary", |ui| {
                ui.label(
                    egui::RichText::new(summary)
                        .color(KitsuneTheme::TEXT2)
                        .size(11.0),
                );
            });
        }
    }
}
