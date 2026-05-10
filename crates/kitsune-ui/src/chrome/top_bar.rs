use crate::app::KitsuneBrowser;
use crate::chrome::tab_bar::tab_bar;
use crate::theme::KitsuneTheme;
use eframe::egui;

pub fn top_bar(ctx: &egui::Context, browser: &mut KitsuneBrowser) {
    // ── Row 1: Tab strip + Window controls ──────────────────────────────────
    egui::TopBottomPanel::top("tab_strip")
        .exact_height(34.0)
        .frame(
            egui::Frame::none()
                .fill(KitsuneTheme::BG)
                .inner_margin(egui::Margin { left: 10.0, right: 0.0, top: 4.0, bottom: 0.0 }),
        )
        .show(ctx, |ui| {
            let panel_rect = ui.max_rect();

            // Register the drag zone FIRST so buttons added later take hover/click priority.
            // (In egui, the last registered widget for a given point wins interaction.)
            let drag = ui.interact(
                panel_rect,
                egui::Id::new("titlebar_drag"),
                egui::Sense::click_and_drag(),
            );
            if drag.dragged() {
                ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
            }
            if drag.double_clicked() {
                let is_max = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
                ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!is_max));
            }

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let is_max = ctx.input(|i| i.viewport().maximized.unwrap_or(false));

                // ── Window controls (right → left: ✕, □, ─) ──────────────
                if wc_btn(ui, WcIcon::Close, "Close", true).clicked() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
                if wc_btn(
                    ui,
                    WcIcon::MaxRestore,
                    if is_max { "Restore" } else { "Maximize" },
                    false,
                )
                .clicked()
                {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!is_max));
                }
                if wc_btn(ui, WcIcon::Minimize, "Minimize", false).clicked() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
                }

                ui.add_space(2.0);
                ui.add(egui::Separator::default().vertical().spacing(4.0));
                ui.add_space(4.0);

                // ── Logo + Tabs fill the left ─────────────────────────────
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 0.0;
                        ui.label(
                            egui::RichText::new("KIT")
                                .size(14.0)
                                .strong()
                                .color(KitsuneTheme::TEXT1),
                        );
                        ui.label(
                            egui::RichText::new("SUNE")
                                .size(14.0)
                                .strong()
                                .color(KitsuneTheme::AMBER),
                        );
                    });
                    ui.add_space(6.0);
                    ui.add(egui::Separator::default().vertical().spacing(6.0));
                    ui.add_space(2.0);

                    let tab_action = tab_bar(ui, browser);
                    match tab_action {
                        TabAction::None => {}
                        TabAction::Switch(id) => browser.switch_tab(id),
                        TabAction::Close(id) => browser.close_tab(id),
                        TabAction::New => browser.new_tab(),
                    }
                });
            });
        });

    // ── Row 2: Navigation + Address bar ──────────────────────────────────
    egui::TopBottomPanel::top("nav_bar")
        .exact_height(36.0)
        .frame(
            egui::Frame::none()
                .fill(KitsuneTheme::BG_PANEL)
                .inner_margin(egui::Margin::symmetric(10.0, 4.0))
                .stroke(egui::Stroke::new(1.0, KitsuneTheme::BORDER)),
        )
        .show(ctx, |ui| {
            ui.horizontal_centered(|ui| {
                // ── Nav buttons ───────────────────────────────────────
                if nav_btn(ui, "◀", "Back").clicked() {
                    if let Some(cef) = &browser.cef {
                        cef.go_back();
                    }
                }
                if nav_btn(ui, "▶", "Forward").clicked() {
                    if let Some(cef) = &browser.cef {
                        cef.go_forward();
                    }
                }
                if nav_btn(ui, "↻", "Reload").clicked() {
                    if let Some(cef) = &browser.cef {
                        cef.reload();
                    }
                }
                if nav_btn(ui, "Home", "Home").clicked() {
                    browser.navigate("https://www.google.com");
                }
                ui.add_space(4.0);

                // ── Address bar ───────────────────────────────────────
                let is_https = browser.address_bar.starts_with("https://");
                let lock_icon = if is_https { "🔒" } else { "⚠" };
                let lock_col = if is_https { KitsuneTheme::GREEN_SAFE } else { KitsuneTheme::RED };

                let bar_w = (ui.available_width() - 240.0).max(100.0);
                egui::Frame::none()
                    .fill(KitsuneTheme::BG3)
                    .rounding(egui::Rounding::same(6.0))
                    .stroke(egui::Stroke::new(1.0, KitsuneTheme::BORDER))
                    .inner_margin(egui::Margin::symmetric(8.0, 3.0))
                    .show(ui, |ui| {
                        ui.horizontal_centered(|ui| {
                            ui.label(
                                egui::RichText::new(lock_icon)
                                    .size(10.0)
                                    .color(lock_col),
                            );
                            ui.add_space(4.0);
                            let te = egui::TextEdit::singleline(&mut browser.address_bar)
                                .desired_width((bar_w - 36.0).max(60.0))
                                .frame(false)
                                .font(egui::FontId::monospace(11.0))
                                .text_color(KitsuneTheme::TEXT_PRIMARY)
                                .hint_text("Enter URL or search…");
                            let r = ui.add(te);
                            if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                                let url = browser.address_bar.clone();
                                browser.navigate(&url);
                            }
                        });
                    });
                ui.add_space(4.0);

                // ── Privacy pill ──────────────────────────────────────
                let n = browser.privacy.trackers_blocked;
                let pill_col = if n > 0 {
                    KitsuneTheme::GREEN_SAFE
                } else {
                    KitsuneTheme::TEXT3
                };
                egui::Frame::none()
                    .fill(egui::Color32::from_rgba_unmultiplied(
                        pill_col.r(),
                        pill_col.g(),
                        pill_col.b(),
                        30,
                    ))
                    .rounding(egui::Rounding::same(20.0))
                    .inner_margin(egui::Margin::symmetric(8.0, 3.0))
                    .show(ui, |ui| {
                        ui.label(
                            egui::RichText::new(format!("🛡 {n}"))
                                .size(10.0)
                                .color(pill_col)
                                .strong(),
                        );
                    });
                
                ui.add_space(8.0);

                // Downloads button — shows count badge when downloads are active.
                let in_progress = browser
                    .downloads
                    .iter()
                    .filter(|d| d.status == crate::app::DownloadStatus::InProgress)
                    .count();
                let dl_label = if in_progress > 0 {
                    format!("⬇{}", in_progress)
                } else {
                    "⬇".to_string()
                };
                let dl_col = if in_progress > 0 {
                    KitsuneTheme::AMBER
                } else {
                    KitsuneTheme::TEXT1
                };
                let dl_btn = egui::Button::new(
                    egui::RichText::new(&dl_label).size(13.0).color(dl_col),
                )
                .frame(false)
                .min_size(egui::vec2(28.0, 24.0));
                if ui
                    .add(dl_btn)
                    .on_hover_text("Downloads")
                    .clicked()
                {
                    browser.show_downloads = !browser.show_downloads;
                }

                ui.add_space(4.0);
                if nav_btn(ui, "⚙", "API Settings").clicked() {
                    browser.show_settings = true;
                }
            });
        });
}

/// Result of tab bar interactions
pub enum TabAction {
    None,
    Switch(usize),
    Close(usize),
    New,
}

fn nav_btn(ui: &mut egui::Ui, label: &str, tooltip: &str) -> egui::Response {
    let btn = egui::Button::new(
        egui::RichText::new(label)
            .size(14.0)
            .color(KitsuneTheme::TEXT1),
    )
    .frame(false)
    .min_size(egui::vec2(24.0, 24.0));
    ui.add(btn).on_hover_text(tooltip)
}

enum WcIcon {
    Minimize,
    MaxRestore,
    Close,
}

/// Window control button drawn with vector shapes (avoids font-glyph availability issues).
/// `is_close` gives the close button a red hover background.
fn wc_btn(ui: &mut egui::Ui, icon: WcIcon, tooltip: &str, is_close: bool) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(34.0, 26.0), egui::Sense::click());
    let response = response.on_hover_text(tooltip);

    if ui.is_rect_visible(rect) {
        let hovered = response.hovered();
        let bg = if hovered && is_close {
            egui::Color32::from_rgb(196, 43, 28)
        } else if hovered {
            egui::Color32::from_rgba_unmultiplied(255, 255, 255, 22)
        } else {
            egui::Color32::TRANSPARENT
        };
        ui.painter().rect_filled(rect, egui::Rounding::ZERO, bg);

        let ink = if hovered && is_close { egui::Color32::WHITE } else { KitsuneTheme::TEXT1 };
        let stroke = egui::Stroke::new(1.2, ink);
        let c = rect.center();

        match icon {
            WcIcon::Minimize => {
                // Horizontal bar
                let y = c.y + 2.0;
                ui.painter().line_segment(
                    [egui::pos2(c.x - 5.0, y), egui::pos2(c.x + 5.0, y)],
                    stroke,
                );
            }
            WcIcon::MaxRestore => {
                // Square outline
                ui.painter().rect_stroke(
                    egui::Rect::from_center_size(c, egui::vec2(10.0, 9.0)),
                    egui::Rounding::ZERO,
                    stroke,
                );
            }
            WcIcon::Close => {
                // Diagonal cross
                let d = 4.5_f32;
                ui.painter().line_segment(
                    [egui::pos2(c.x - d, c.y - d), egui::pos2(c.x + d, c.y + d)],
                    stroke,
                );
                ui.painter().line_segment(
                    [egui::pos2(c.x + d, c.y - d), egui::pos2(c.x - d, c.y + d)],
                    stroke,
                );
            }
        }
    }

    response
}
