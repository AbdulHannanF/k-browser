use crate::app::KitsuneBrowser;
use crate::chrome::top_bar::TabAction;
use crate::theme::KitsuneTheme;
use eframe::egui;

/// Maximum display characters for a tab title before truncation
const MAX_TAB_CHARS: usize = 18;

pub fn tab_bar(ui: &mut egui::Ui, browser: &KitsuneBrowser) -> TabAction {
    let mut result = TabAction::None;

    ui.horizontal(|ui| {
        for tab in browser.tabs.iter() {
            let is_active = tab.active;

            // ── Tab capsule ──────────────────────────────────────────
            let fill = if is_active {
                KitsuneTheme::BG_CARD
            } else {
                egui::Color32::TRANSPARENT
            };
            let border = if is_active {
                egui::Stroke::new(1.0, KitsuneTheme::BORDER2)
            } else {
                egui::Stroke::NONE
            };

            let tab_frame = egui::Frame::none()
                .inner_margin(egui::Margin::symmetric(10.0, 5.0))
                .rounding(egui::Rounding::same(5.0))
                .fill(fill)
                .stroke(border);

            let tab_response = tab_frame.show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;

                    // Favicon dot
                    let (dot_rect, _) =
                        ui.allocate_exact_size(egui::vec2(10.0, 10.0), egui::Sense::hover());
                    let color = tab
                        .favicon_color
                        .map_or(KitsuneTheme::TEXT3, |[r, g, b]| {
                            egui::Color32::from_rgb(r, g, b)
                        });
                    ui.painter()
                        .circle_filled(dot_rect.center(), 4.0, color);

                    // Safe title truncation (UTF-8 aware)
                    let title: String = if tab.title.chars().count() > MAX_TAB_CHARS {
                        let truncated: String = tab.title.chars().take(MAX_TAB_CHARS).collect();
                        format!("{truncated}…")
                    } else {
                        tab.title.clone()
                    };
                    let text_col = if is_active {
                        KitsuneTheme::TEXT_PRIMARY
                    } else {
                        KitsuneTheme::TEXT_MUTED
                    };
                    ui.label(
                        egui::RichText::new(title)
                            .size(11.5)
                            .color(text_col),
                    );

                    // Loading spinner or close button
                    if tab.is_loading {
                        ui.label(
                            egui::RichText::new("O")
                                .color(KitsuneTheme::AMBER)
                                .size(10.0),
                        );
                    } else if browser.tabs.len() > 1 {
                        // Only show close if there's more than one tab
                        let close = ui.add(
                            egui::Button::new(
                                egui::RichText::new("x")
                                    .color(KitsuneTheme::TEXT3)
                                    .size(11.0)
                                    .strong(),
                            )
                            .frame(false)
                            .min_size(egui::vec2(18.0, 18.0)),
                        );
                        if close.clicked() {
                            result = TabAction::Close(tab.id);
                        }
                    }
                });
            }).response;

            // Click to switch tab (only if nothing else consumed the click)
            let tab_click = ui.interact(tab_response.rect, tab_response.id.with("tab_click"), egui::Sense::click());
            if tab_click.clicked() && !matches!(result, TabAction::Close(_)) {
                result = TabAction::Switch(tab.id);
            }
        }

        // ── New tab button ───────────────────────────────────────────
        let add_btn = ui.add(
            egui::Button::new(
                egui::RichText::new("+")
                    .size(14.0)
                    .color(KitsuneTheme::TEXT2),
            )
            .frame(false)
            .min_size(egui::vec2(22.0, 22.0)),
        );
        if add_btn.clicked() {
            result = TabAction::New;
        }
    });

    result
}
