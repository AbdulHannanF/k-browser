use crate::app::KitsuneBrowser;
use crate::chrome::top_bar::TabAction;
use crate::theme::{colors, KitsuneTheme};
use eframe::egui;

const MAX_TAB_CHARS: usize = 18;

pub fn tab_bar(ui: &mut egui::Ui, browser: &KitsuneBrowser) -> TabAction {
    let mut result = TabAction::None;

    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;

        for tab in browser.tabs.iter() {
            let is_active = tab.active;
            let tab_id    = egui::Id::new("tab").with(tab.id);

            let prev_hov = ui.ctx().data(|d| d.get_temp::<bool>(tab_id).unwrap_or(false));

            let fill = if is_active {
                colors::BG_ELEVATED
            } else if prev_hov {
                colors::BG_CARD
            } else {
                egui::Color32::TRANSPARENT
            };

            let stroke_col = if is_active {
                colors::BORDER_NORMAL
            } else {
                egui::Color32::TRANSPARENT
            };

            let resp = egui::Frame::none()
                .fill(fill)
                .rounding(egui::Rounding {
                    nw: 4.0, ne: 4.0, sw: 0.0, se: 0.0,
                })
                .stroke(egui::Stroke::new(1.0, stroke_col))
                .inner_margin(egui::Margin { left: 10.0, right: 8.0, top: 5.0, bottom: 4.0 })
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 5.0;

                        // Favicon dot
                        let (dot_rect, _) =
                            ui.allocate_exact_size(egui::vec2(9.0, 9.0), egui::Sense::hover());
                        let dot_col = tab.favicon_color.map_or(
                            if is_active { KitsuneTheme::AMBER } else { KitsuneTheme::TEXT3 },
                            |[r, g, b]| egui::Color32::from_rgb(r, g, b),
                        );
                        ui.painter().circle_filled(dot_rect.center(), 3.5, dot_col);

                        // Title — prefix ⟳ when loading
                        let raw_title: String = if tab.title.chars().count() > MAX_TAB_CHARS {
                            let t: String = tab.title.chars().take(MAX_TAB_CHARS).collect();
                            format!("{t}…")
                        } else {
                            tab.title.clone()
                        };
                        let display_title = if tab.is_loading {
                            format!("⟳ {}", raw_title)
                        } else {
                            raw_title
                        };

                        let text_col = if is_active { KitsuneTheme::TEXT0 } else { KitsuneTheme::TEXT3 };
                        ui.label(
                            egui::RichText::new(display_title)
                                .size(11.5)
                                .color(text_col),
                        );

                        // Close button — always allocate space to keep width stable
                        if browser.tabs.len() > 1 {
                            let close_id = tab_id.with("close");
                            let (r, _)   = ui.allocate_exact_size(egui::vec2(14.0, 14.0), egui::Sense::click());
                            let c_resp   = ui.interact(r, close_id, egui::Sense::click());
                            let c_col    = if c_resp.hovered() { KitsuneTheme::RED } else { KitsuneTheme::TEXT3 };
                            if ui.is_rect_visible(r) {
                                ui.painter().text(
                                    r.center(),
                                    egui::Align2::CENTER_CENTER,
                                    "✕",
                                    egui::FontId::proportional(9.5),
                                    c_col,
                                );
                            }
                            if c_resp.clicked() {
                                result = TabAction::Close(tab.id);
                            }
                        }
                    });
                })
                .response;

            // Active-tab orange top accent line
            if is_active {
                let top = egui::Rect::from_min_max(resp.rect.min, egui::pos2(resp.rect.max.x, resp.rect.min.y + 2.0));
                ui.painter().rect_filled(top, egui::Rounding::ZERO, KitsuneTheme::AMBER);
            }

            // Loading bar — thin orange strip at bottom of tab, depleting left→right
            if tab.is_loading {
                let load_id  = egui::Id::new("tab_load").with(tab.id);
                let progress = {
                    let t = ui.ctx().input(|i| i.time) as f32;
                    let p = (t * 0.5) % 1.0; // indeterminate: loops 0→1
                    ui.ctx().data_mut(|d| d.insert_temp(load_id, p));
                    p
                };
                let bar_y  = resp.rect.max.y - 2.0;
                let bar_w  = resp.rect.width() * progress;
                let bar    = egui::Rect::from_min_max(
                    egui::pos2(resp.rect.min.x, bar_y),
                    egui::pos2(resp.rect.min.x + bar_w, resp.rect.max.y),
                );
                ui.painter().rect_filled(bar, egui::Rounding::ZERO, KitsuneTheme::AMBER);
                ui.ctx().request_repaint();
            }

            // Tab hover / click interaction
            let tab_resp = ui.interact(resp.rect, tab_id.with("click"), egui::Sense::click());
            ui.ctx().data_mut(|d| d.insert_temp(tab_id, tab_resp.hovered()));
            if tab_resp.clicked() && !matches!(result, TabAction::Close(_)) {
                result = TabAction::Switch(tab.id);
            }
        }

        // ── New tab button ─────────────────────────────────────────────────
        let add_id  = egui::Id::new("new_tab_btn");
        let add_hov = ui.ctx().data(|d| d.get_temp::<bool>(add_id).unwrap_or(false));
        let add_col = if add_hov { KitsuneTheme::AMBER } else { KitsuneTheme::TEXT2 };

        let add_btn = egui::Button::new(
            egui::RichText::new("+").size(15.0).color(add_col),
        )
        .frame(false)
        .min_size(egui::vec2(26.0, 26.0));
        let add_resp = ui.add(add_btn);
        ui.ctx().data_mut(|d| d.insert_temp(add_id, add_resp.hovered()));
        if add_resp.clicked() {
            result = TabAction::New;
        }
    });

    result
}
