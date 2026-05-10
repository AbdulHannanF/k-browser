use crate::app::{AgentRunState, KitsuneBrowser, LogLevel};
use crate::theme::KitsuneTheme;
use eframe::egui;
use kitsune_hil::gate::respond_to_checkpoint;

pub fn hil_dialog(ctx: &egui::Context, browser: &mut KitsuneBrowser) {
    let Some(action) = &browser.hil_action else {
        return;
    };

    let elapsed = ctx.input(|i| i.time) - action.started_at;
    let remaining = (action.total_secs as f64 - elapsed).max(0.0);
    let fraction = (remaining / action.total_secs as f64) as f32;
    let secs_left = remaining as u32;

    // Clone display data before borrowing mutably below.
    let title = action.title.clone();
    let subtitle = action.subtitle.clone();
    let rows: Vec<(String, String)> = action.rows.iter()
        .filter(|(_, v)| !v.is_empty())
        .cloned()
        .collect();
    let timed_out = remaining <= 0.0;

    // Auto-cancel on timeout — resolve the real gate checkpoint too.
    if timed_out {
        browser.push_log("⚠  HIL timeout — action auto-cancelled", LogLevel::Warn);
        if let Some(cp) = browser.hil_pending_checkpoint.take() {
            respond_to_checkpoint(cp, false, Some("Timeout".to_string()));
        }
        browser.hil_action = None;
        browser.agent_state = AgentRunState::Idle;
        return;
    }

    let mut confirm = false;
    let mut cancel = false;

    // Dim overlay
    let screen = ctx.screen_rect();
    ctx.layer_painter(egui::LayerId::new(
        egui::Order::Background,
        egui::Id::new("hil_dim"),
    ))
    .rect_filled(
        screen,
        0.0,
        egui::Color32::from_rgba_premultiplied(8, 8, 13, 200),
    );

    egui::Window::new("hil_gate")
        .title_bar(false)
        .resizable(false)
        .collapsible(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .fixed_size([400.0, 0.0])
        .frame(
            egui::Frame::none()
                .fill(KitsuneTheme::BG_CARD)
                .rounding(egui::Rounding::same(12.0))
                .stroke(egui::Stroke::new(1.5, KitsuneTheme::AMBER))
                .shadow(egui::Shadow {
                    offset: [0.0, 6.0].into(),
                    blur: 32.0,
                    spread: 0.0,
                    color: egui::Color32::from_rgba_premultiplied(255, 122, 0, 35),
                }),
        )
        .show(ctx, |ui| {
            // ── Header section ────────────────────────────────────────
            egui::Frame::none()
                .fill(egui::Color32::from_rgba_premultiplied(255, 122, 0, 12))
                .inner_margin(egui::Margin::symmetric(20.0, 14.0))
                .show(ui, |ui| {
                    egui::Frame::none()
                        .fill(KitsuneTheme::AMBER_DIM)
                        .rounding(egui::Rounding::same(20.0))
                        .stroke(egui::Stroke::new(1.0, KitsuneTheme::BORDER_AMBER))
                        .inner_margin(egui::Margin::symmetric(8.0, 2.0))
                        .show(ui, |ui| {
                            ui.label(
                                egui::RichText::new("⚠  APPROVAL REQUIRED")
                                    .size(10.0)
                                    .color(KitsuneTheme::AMBER)
                                    .strong()
                                    .family(egui::FontFamily::Monospace),
                            );
                        });
                    ui.add_space(8.0);
                    ui.label(
                        egui::RichText::new(&title)
                            .size(16.0)
                            .strong()
                            .color(KitsuneTheme::TEXT_PRIMARY),
                    );
                    ui.add_space(2.0);
                    ui.label(
                        egui::RichText::new(&subtitle)
                            .size(11.0)
                            .color(KitsuneTheme::TEXT_MUTED),
                    );
                });

            // Separator
            let sep_rect = ui.available_rect_before_wrap();
            let sep = egui::Rect::from_min_size(sep_rect.left_top(), egui::vec2(sep_rect.width(), 1.0));
            ui.painter().rect_filled(sep, 0.0, KitsuneTheme::BORDER);
            ui.allocate_exact_size(egui::vec2(0.0, 1.0), egui::Sense::hover());

            // ── Action detail card ────────────────────────────────────
            if !rows.is_empty() {
                egui::Frame::none()
                    .fill(KitsuneTheme::BG3)
                    .rounding(egui::Rounding::same(7.0))
                    .stroke(egui::Stroke::new(1.0, KitsuneTheme::BORDER))
                    .inner_margin(egui::Margin::symmetric(12.0, 10.0))
                    .outer_margin(egui::Margin::symmetric(14.0, 10.0))
                    .show(ui, |ui| {
                        ui.label(
                            egui::RichText::new("PROPOSED ACTION")
                                .size(9.0)
                                .color(KitsuneTheme::TEXT2)
                                .strong()
                                .family(egui::FontFamily::Monospace),
                        );
                        ui.add_space(6.0);
                        for (i, (k, v)) in rows.iter().enumerate() {
                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new(k)
                                        .size(10.5)
                                        .color(KitsuneTheme::TEXT2)
                                        .family(egui::FontFamily::Monospace),
                                );
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        let col = if k == "Total" || k == "Cost" {
                                            KitsuneTheme::AMBER
                                        } else if k == "Credential" {
                                            KitsuneTheme::GREEN_SAFE
                                        } else {
                                            KitsuneTheme::TEXT_PRIMARY
                                        };
                                        let sz = if k == "Total" || k == "Cost" { 14.0 } else { 11.0 };
                                        ui.label(
                                            egui::RichText::new(v)
                                                .size(sz)
                                                .color(col)
                                                .strong(),
                                        );
                                    },
                                );
                            });
                            if i < rows.len() - 1 {
                                let rect = ui.available_rect_before_wrap();
                                let line = egui::Rect::from_min_size(
                                    rect.left_top(),
                                    egui::vec2(rect.width(), 1.0),
                                );
                                ui.painter().rect_filled(line, 0.0, KitsuneTheme::BORDER);
                                ui.allocate_exact_size(egui::vec2(0.0, 3.0), egui::Sense::hover());
                            }
                        }
                    });
            }

            // ── Countdown timer ───────────────────────────────────────
            egui::Frame::none()
                .inner_margin(egui::Margin { left: 20.0, right: 20.0, top: 0.0, bottom: 8.0 })
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        let timer_col = if fraction > 0.4 {
                            KitsuneTheme::TEXT2
                        } else {
                            KitsuneTheme::RED
                        };
                        ui.label(
                            egui::RichText::new(format!("Auto-cancel in {secs_left}s"))
                                .size(10.0)
                                .color(timer_col)
                                .family(egui::FontFamily::Monospace),
                        );
                    });
                    ui.add_space(3.0);
                    let (bar_rect, _) = ui.allocate_exact_size(
                        egui::vec2(ui.available_width(), 3.0),
                        egui::Sense::hover(),
                    );
                    ui.painter().rect_filled(bar_rect, egui::Rounding::same(2.0), KitsuneTheme::BG4);
                    let mut fill = bar_rect;
                    fill.set_right(bar_rect.left() + bar_rect.width() * fraction);
                    let bar_col = if fraction > 0.4 { KitsuneTheme::AMBER } else { KitsuneTheme::RED };
                    ui.painter().rect_filled(fill, egui::Rounding::same(2.0), bar_col);
                });

            // Separator
            let sep_rect = ui.available_rect_before_wrap();
            let sep = egui::Rect::from_min_size(sep_rect.left_top(), egui::vec2(sep_rect.width(), 1.0));
            ui.painter().rect_filled(sep, 0.0, KitsuneTheme::BORDER);
            ui.allocate_exact_size(egui::vec2(0.0, 1.0), egui::Sense::hover());

            // ── Buttons ───────────────────────────────────────────────
            egui::Frame::none()
                .inner_margin(egui::Margin::symmetric(20.0, 12.0))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        let confirm_btn = egui::Button::new(
                            egui::RichText::new("✓ Approve & Execute")
                                .size(12.0)
                                .color(egui::Color32::BLACK)
                                .strong(),
                        )
                        .fill(KitsuneTheme::AMBER)
                        .rounding(egui::Rounding::same(6.0))
                        .min_size(egui::vec2(180.0, 32.0));
                        if ui.add(confirm_btn).clicked() {
                            confirm = true;
                        }
                        ui.add_space(6.0);
                        let cancel_btn = egui::Button::new(
                            egui::RichText::new("Cancel")
                                .size(12.0)
                                .color(KitsuneTheme::TEXT_MUTED),
                        )
                        .fill(egui::Color32::TRANSPARENT)
                        .stroke(egui::Stroke::new(1.0, KitsuneTheme::BORDER2))
                        .rounding(egui::Rounding::same(6.0))
                        .min_size(egui::vec2(70.0, 32.0));
                        if ui.add(cancel_btn).clicked() {
                            cancel = true;
                        }
                    });
                });
        });

    // ── Handle button results ─────────────────────────────────────────────
    if confirm {
        // Resolve the real HIL gate checkpoint (in-process agent path).
        if let Some(cp) = browser.hil_pending_checkpoint.take() {
            respond_to_checkpoint(cp, true, None);
        } else {
            // SSE demo path: POST approval to cloud-mock server.
            browser.runtime().spawn(async move {
                let client = reqwest::Client::new();
                let _ = client
                    .post("http://127.0.0.1:7700/api/hil-response")
                    .json(&serde_json::json!({"approved": true}))
                    .send()
                    .await;
            });
        }
        browser.push_log("✓  User approved — executing…", LogLevel::Ok);
        browser.hil_action = None;
    } else if cancel {
        // Resolve the real HIL gate checkpoint (in-process agent path).
        if let Some(cp) = browser.hil_pending_checkpoint.take() {
            respond_to_checkpoint(cp, false, Some("Cancelled by user".to_string()));
        } else {
            // SSE demo path: POST rejection to cloud-mock server.
            browser.runtime().spawn(async move {
                let client = reqwest::Client::new();
                let _ = client
                    .post("http://127.0.0.1:7700/api/hil-response")
                    .json(&serde_json::json!({"approved": false}))
                    .send()
                    .await;
            });
        }
        browser.push_log("✕  User cancelled — action aborted", LogLevel::Warn);
        browser.hil_action = None;
        browser.agent_state = AgentRunState::Idle;
    }

    ctx.request_repaint(); // keep timer ticking
}
