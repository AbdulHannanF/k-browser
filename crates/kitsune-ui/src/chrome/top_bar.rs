use crate::app::KitsuneBrowser;
use crate::chrome::tab_bar::tab_bar;
use crate::theme::KitsuneTheme;
use eframe::egui;

pub fn top_bar(ctx: &egui::Context, browser: &mut KitsuneBrowser) {
    // ── Row 1: Tab strip ─────────────────────────────────────────────────
    egui::TopBottomPanel::top("tab_strip")
        .exact_height(34.0)
        .frame(
            egui::Frame::none()
                .fill(KitsuneTheme::BG)
                .inner_margin(egui::Margin { left: 10.0, right: 10.0, top: 4.0, bottom: 0.0 }),
        )
        .show(ctx, |ui| {
            ui.horizontal_centered(|ui| {
                // Logo
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

                // Tabs
                let tab_action = tab_bar(ui, browser);
                match tab_action {
                    TabAction::None => {}
                    TabAction::Switch(id) => browser.switch_tab(id),
                    TabAction::Close(id) => browser.close_tab(id),
                    TabAction::New => browser.new_tab(),
                }
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
