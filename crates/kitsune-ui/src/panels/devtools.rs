use crate::app::PageState;
use crate::theme::{self, KitsuneTheme};
use egui::{Ui, RichText, FontId, Rounding};
use kitsune_layout::LayoutNode;
use std::sync::{Arc, Mutex};

pub fn render_devtools_panel(ui: &mut Ui, page_state: &Arc<Mutex<PageState>>, _theme: &KitsuneTheme) {
    ui.add_space(8.0);
    ui.heading(RichText::new("Developer Tools").font(FontId::proportional(20.0)).strong());
    ui.separator();
    ui.add_space(12.0);

    let page_state_lock = page_state.lock().unwrap();

    match &*page_state_lock {
        PageState::Loaded(page) => {
            ui.label(RichText::new("DOM Layout Tree").font(FontId::proportional(14.0)).strong().color(theme::TEXT_MUTED));
            ui.add_space(12.0);
            egui::ScrollArea::vertical().show(ui, |ui| {
                render_layout_node_tree(ui, &page.layout_root);
            });
        }
        PageState::Loading(_) => {
            ui.vertical_centered(|ui| {
                ui.add_space(40.0);
                ui.spinner();
                ui.add_space(12.0);
                ui.label(RichText::new("Analyzing Layout...").font(FontId::proportional(13.0)));
            });
        }
        PageState::Error(err) => {
            ui.colored_label(theme::ERROR, format!("Debugger Error: {}", err));
        }
        PageState::Empty => {
            ui.label(RichText::new("No active document context").font(FontId::proportional(13.0)).italics().color(theme::TEXT_MUTED));
        }
    }
}

fn render_layout_node_tree(ui: &mut Ui, node: &LayoutNode) {
    let node_label = if let Some(text) = &node.text {
        format!("\"{}\"", text.trim().chars().take(40).collect::<String>())
    } else {
        format!("<{:?}>", node.style.display)
    };

    if node.children.is_empty() && node.text.is_some() {
        ui.horizontal(|ui| {
            ui.painter().circle_filled(ui.cursor().min + egui::vec2(3.0, 8.0), 2.0, theme::TEXT_MUTED);
            ui.add_space(8.0);
            ui.label(RichText::new(node_label).font(FontId::monospace(12.0)));
        });
    } else {
        egui::CollapsingHeader::new(RichText::new(node_label).font(FontId::monospace(12.0)).color(theme::ACCENT))
            .default_open(false)
            .show(ui, |ui| {
                ui.indent("node_info", |ui| {
                    ui.label(RichText::new(format!("Display: {:?}", node.style.display)).font(FontId::monospace(11.0)).color(theme::TEXT_MUTED));
                    ui.label(RichText::new(format!("Position: {:?}", node.style.position)).font(FontId::monospace(11.0)).color(theme::TEXT_MUTED));
                    
                    for child in &node.children {
                        render_layout_node_tree(ui, child);
                    }
                });
            });
    }
}
