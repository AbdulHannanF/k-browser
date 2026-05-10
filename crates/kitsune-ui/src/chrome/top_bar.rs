use crate::app::{BookmarkItem, KitsuneBrowser};
use crate::chrome::tab_bar::tab_bar;
use crate::theme::{colors, KitsuneTheme};
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

            let drag = ui.interact(panel_rect, egui::Id::new("titlebar_drag"), egui::Sense::click_and_drag());
            if drag.dragged() {
                ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
            }
            if drag.double_clicked() {
                let is_max = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
                ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!is_max));
            }

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let is_max = ctx.input(|i| i.viewport().maximized.unwrap_or(false));

                if wc_btn(ui, WcIcon::Close, "Close", true).clicked() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
                if wc_btn(ui, WcIcon::MaxRestore, if is_max { "Restore" } else { "Maximize" }, false).clicked() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!is_max));
                }
                if wc_btn(ui, WcIcon::Minimize, "Minimize", false).clicked() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
                }

                ui.add_space(2.0);
                ui.add(egui::Separator::default().vertical().spacing(4.0));
                ui.add_space(4.0);

                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    // Logo
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 0.0;
                        ui.label(egui::RichText::new("KIT").size(14.0).strong().color(KitsuneTheme::TEXT1));
                        ui.label(egui::RichText::new("SUNE").size(14.0).strong().color(KitsuneTheme::AMBER));
                    });
                    ui.add_space(6.0);
                    ui.add(egui::Separator::default().vertical().spacing(6.0));
                    ui.add_space(2.0);

                    let tab_action = tab_bar(ui, browser);
                    match tab_action {
                        TabAction::None       => {}
                        TabAction::Switch(id) => browser.switch_tab(id),
                        TabAction::Close(id)  => browser.close_tab(id),
                        TabAction::New        => browser.new_tab(),
                    }
                });
            });
        });

    // ── Row 2: Navigation + Address bar ──────────────────────────────────
    egui::TopBottomPanel::top("nav_bar")
        .exact_height(38.0)
        .frame(
            egui::Frame::none()
                .fill(KitsuneTheme::BG1)
                .inner_margin(egui::Margin::symmetric(10.0, 4.0))
                .stroke(egui::Stroke::new(1.0, KitsuneTheme::BORDER)),
        )
        .show(ctx, |ui| {
            ui.horizontal_centered(|ui| {
                // ── Nav buttons ──────────────────────────────────────────
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
                let reload_id  = egui::Id::new("reload_btn");
                let reload_hov = ctx.data(|d| d.get_temp::<bool>(reload_id).unwrap_or(false));
                let is_loading = browser.tabs.iter().any(|t| t.active && t.is_loading);
                let reload_icon = if is_loading { "✕" } else { "↻" };
                let r_btn = nav_btn(ui, reload_icon, if is_loading { "Stop" } else { "Reload" });
                ctx.data_mut(|d| d.insert_temp(reload_id, r_btn.hovered()));
                if r_btn.clicked() {
                    if let Some(cef) = &browser.cef {
                        if is_loading { cef.stop_load(); } else { cef.reload(); }
                    }
                }
                if nav_btn(ui, "⌂", "Home").clicked() {
                    browser.navigate("https://www.google.com");
                }
                ui.add_space(5.0);

                // ── URL bar ───────────────────────────────────────────────
                let is_https = browser.address_bar.starts_with("https://");
                let lock_icon = if is_https { "🔒" } else { "⚠" };
                let lock_col  = if is_https { KitsuneTheme::GREEN } else { KitsuneTheme::RED };

                let url_id  = egui::Id::new("url_bar");
                let has_foc = ctx.memory(|m| m.focused() == Some(url_id));

                let bar_w       = (ui.available_width() - 200.0).max(120.0);
                let border_col  = if has_foc { KitsuneTheme::AMBER } else { KitsuneTheme::BORDER2 };
                let bg_col      = if has_foc { colors::BG_INPUT } else { KitsuneTheme::BG2 };

                egui::Frame::none()
                    .fill(bg_col)
                    .rounding(egui::Rounding::same(6.0))
                    .stroke(egui::Stroke::new(1.0, border_col))
                    .inner_margin(egui::Margin { left: 8.0, right: 8.0, top: 3.0, bottom: 3.0 })
                    .show(ui, |ui| {
                        ui.horizontal_centered(|ui| {
                            ui.label(egui::RichText::new(lock_icon).size(10.0).color(lock_col));
                            ui.add_space(4.0);
                            let te = egui::TextEdit::singleline(&mut browser.address_bar)
                                .id(url_id)
                                .desired_width((bar_w - 48.0).max(80.0))
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

                ui.add_space(5.0);

                // ── Privacy pill ───────────────────────────────────────────
                let n = browser.privacy.trackers_blocked;
                let pill_col = if n > 0 { KitsuneTheme::GREEN } else { KitsuneTheme::TEXT3 };
                egui::Frame::none()
                    .fill(egui::Color32::from_rgba_premultiplied(
                        if n > 0 { 7 } else { 0 },
                        if n > 0 { 22 } else { 0 },
                        if n > 0 { 13 } else { 0 },
                        if n > 0 { 25 } else { 0 },
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

                ui.add_space(4.0);

                // ── Downloads button ──────────────────────────────────────
                let in_progress = browser.downloads.iter()
                    .filter(|d| d.status == crate::app::DownloadStatus::InProgress)
                    .count();
                let dl_label = if in_progress > 0 { format!("⬇{}", in_progress) } else { "⬇".into() };
                let dl_col   = if in_progress > 0 { KitsuneTheme::AMBER } else { KitsuneTheme::TEXT1 };
                let dl_btn   = egui::Button::new(
                    egui::RichText::new(&dl_label).size(13.0).color(dl_col),
                )
                .frame(false)
                .min_size(egui::vec2(28.0, 24.0));
                if ui.add(dl_btn).on_hover_text("Downloads").clicked() {
                    browser.show_downloads = !browser.show_downloads;
                }

                ui.add_space(2.0);

                // ── Find button ────────────────────────────────────────────
                let find_col = if browser.show_find_bar { KitsuneTheme::AMBER } else { KitsuneTheme::TEXT1 };
                let find_btn = egui::Button::new(
                    egui::RichText::new("🔍").size(13.0).color(find_col),
                )
                .frame(false)
                .min_size(egui::vec2(24.0, 24.0));
                if ui.add(find_btn).on_hover_text("Find in page (Ctrl+F)").clicked() {
                    browser.show_find_bar = !browser.show_find_bar;
                }

                ui.add_space(2.0);

                // ── Settings ───────────────────────────────────────────────
                if nav_btn(ui, "⚙", "API Settings").clicked() {
                    browser.show_settings = true;
                }
            });
        });

    // ── Row 3: Bookmarks bar (optional) ──────────────────────────────────
    if browser.show_bookmarks_bar {
        egui::TopBottomPanel::top("bookmarks_bar")
            .exact_height(28.0)
            .frame(
                egui::Frame::none()
                    .fill(KitsuneTheme::BG)
                    .inner_margin(egui::Margin { left: 10.0, right: 10.0, top: 3.0, bottom: 3.0 })
                    .stroke(egui::Stroke::new(1.0, KitsuneTheme::BORDER)),
            )
            .show(ctx, |ui| {
                ui.horizontal_centered(|ui| {
                    ui.spacing_mut().item_spacing.x = 2.0;

                    // Bookmark items
                    let navigate_to: Option<String> = {
                        let mut nav = None;
                        for bm in &browser.bookmarks {
                            let bm_id  = egui::Id::new("bm").with(bm.url.as_str());
                            let bm_hov = ctx.data(|d| d.get_temp::<bool>(bm_id).unwrap_or(false));
                            let fill   = if bm_hov { KitsuneTheme::BG3 } else { egui::Color32::TRANSPARENT };
                            let resp   = egui::Frame::none()
                                .fill(fill)
                                .rounding(egui::Rounding::same(4.0))
                                .inner_margin(egui::Margin::symmetric(7.0, 2.0))
                                .show(ui, |ui| {
                                    ui.label(
                                        egui::RichText::new(&bm.title)
                                            .size(11.0)
                                            .color(if bm_hov { KitsuneTheme::TEXT0 } else { KitsuneTheme::TEXT1 }),
                                    );
                                })
                                .response;
                            let click = ui.interact(resp.rect, bm_id.with("c"), egui::Sense::click());
                            ctx.data_mut(|d| d.insert_temp(bm_id, click.hovered()));
                            if click.clicked() {
                                nav = Some(bm.url.clone());
                            }
                        }
                        nav
                    };

                    if let Some(url) = navigate_to {
                        browser.navigate(&url);
                    }

                    // Add bookmark button
                    ui.add_space(4.0);
                    let is_bookmarked = browser.bookmarks.iter().any(|b| b.url == browser.address_bar);
                    let star_col = if is_bookmarked { KitsuneTheme::AMBER } else { KitsuneTheme::TEXT3 };
                    let star_btn = egui::Button::new(
                        egui::RichText::new(if is_bookmarked { "★" } else { "☆" }).size(13.0).color(star_col),
                    )
                    .frame(false);
                    if ui.add(star_btn).on_hover_text("Bookmark this page").clicked() {
                        if is_bookmarked {
                            browser.bookmarks.retain(|b| b.url != browser.address_bar);
                        } else {
                            let title = browser.tabs.iter()
                                .find(|t| t.active)
                                .map(|t| t.title.clone())
                                .unwrap_or_else(|| browser.address_bar.clone());
                            browser.bookmarks.push(BookmarkItem {
                                title,
                                url: browser.address_bar.clone(),
                            });
                        }
                    }
                });
            });
    }
}

// ── Public types ─────────────────────────────────────────────────────────────

pub enum TabAction {
    None,
    Switch(usize),
    Close(usize),
    New,
}

// ── Nav button ────────────────────────────────────────────────────────────────

fn nav_btn(ui: &mut egui::Ui, label: &str, tooltip: &str) -> egui::Response {
    let id  = egui::Id::new("nav").with(label);
    let hov = ui.ctx().data(|d| d.get_temp::<bool>(id).unwrap_or(false));
    let col = if hov { KitsuneTheme::TEXT0 } else { KitsuneTheme::TEXT1 };

    let btn = egui::Button::new(egui::RichText::new(label).size(14.0).color(col))
        .frame(false)
        .min_size(egui::vec2(26.0, 26.0));
    let r = ui.add(btn).on_hover_text(tooltip);
    ui.ctx().data_mut(|d| d.insert_temp(id, r.hovered()));
    r
}

// ── Window control button ─────────────────────────────────────────────────────

enum WcIcon { Minimize, MaxRestore, Close }

fn wc_btn(ui: &mut egui::Ui, icon: WcIcon, tooltip: &str, is_close: bool) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(34.0, 26.0), egui::Sense::click());
    let response = response.on_hover_text(tooltip);

    if ui.is_rect_visible(rect) {
        let hovered = response.hovered();
        let bg = if hovered && is_close {
            egui::Color32::from_rgb(196, 43, 28)
        } else if hovered {
            egui::Color32::from_rgba_premultiplied(18, 18, 18, 40)
        } else {
            egui::Color32::TRANSPARENT
        };
        ui.painter().rect_filled(rect, egui::Rounding::ZERO, bg);

        let ink    = if hovered && is_close { egui::Color32::WHITE } else { KitsuneTheme::TEXT1 };
        let stroke = egui::Stroke::new(1.2, ink);
        let c      = rect.center();

        match icon {
            WcIcon::Minimize => {
                let y = c.y + 2.0;
                ui.painter().line_segment(
                    [egui::pos2(c.x - 5.0, y), egui::pos2(c.x + 5.0, y)],
                    stroke,
                );
            }
            WcIcon::MaxRestore => {
                ui.painter().rect_stroke(
                    egui::Rect::from_center_size(c, egui::vec2(10.0, 9.0)),
                    egui::Rounding::ZERO,
                    stroke,
                );
            }
            WcIcon::Close => {
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
