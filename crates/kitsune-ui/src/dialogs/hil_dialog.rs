use crate::app::{AgentRunState, KitsuneBrowser, LogLevel};
use crate::animation::lerp_anim;
use crate::theme::{colors, KitsuneTheme};
use eframe::egui;
use kitsune_hil::gate::respond_to_checkpoint;

pub fn hil_dialog(ctx: &egui::Context, browser: &mut KitsuneBrowser) {
    let Some(action) = &browser.hil_action else {
        return;
    };

    let elapsed   = ctx.input(|i| i.time) - action.started_at;
    let remaining = (action.total_secs as f64 - elapsed).max(0.0);
    let fraction  = (remaining / action.total_secs as f64) as f32;
    let secs_left = remaining as u32;

    let title    = action.title.clone();
    let subtitle = action.subtitle.clone();
    let rows: Vec<(String, String)> = action.rows.iter().filter(|(_, v)| !v.is_empty()).cloned().collect();
    let timed_out = remaining <= 0.0;

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
    let mut cancel  = false;

    // Lerp scale-in on open (stored in ctx data, starts at 0.85)
    let scale_id = egui::Id::new("hil_scale_in");
    let scale = lerp_anim(ctx, scale_id, 1.0, 12.0);

    // Dim overlay
    let screen = ctx.screen_rect();
    ctx.layer_painter(egui::LayerId::new(egui::Order::Background, egui::Id::new("hil_dim")))
        .rect_filled(screen, 0.0, egui::Color32::from_rgba_premultiplied(6, 6, 10, 210));

    egui::Window::new("hil_gate")
        .title_bar(false)
        .resizable(false)
        .collapsible(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .fixed_size([400.0 * scale, 0.0])
        .frame(
            egui::Frame::none()
                .fill(colors::BG_CARD)
                .rounding(egui::Rounding::same(12.0))
                .stroke(egui::Stroke::new(1.5, KitsuneTheme::RED))
                .shadow(egui::Shadow {
                    offset: [0.0, 8.0].into(),
                    blur: 40.0,
                    spread: 0.0,
                    color: egui::Color32::from_rgba_premultiplied(24, 11, 11, 60),
                }),
        )
        .show(ctx, |ui| {
            // ── RED top accent bar ────────────────────────────────────────
            let top_bar = egui::Rect::from_min_size(
                ui.min_rect().left_top(),
                egui::vec2(ui.available_width(), 3.0),
            );
            ui.painter().rect_filled(top_bar, egui::Rounding { nw: 12.0, ne: 12.0, sw: 0.0, se: 0.0 }, KitsuneTheme::RED);
            ui.add_space(3.0);

            // ── Header section ────────────────────────────────────────────
            egui::Frame::none()
                .fill(egui::Color32::from_rgba_premultiplied(24, 11, 11, 20))
                .inner_margin(egui::Margin::symmetric(20.0, 14.0))
                .show(ui, |ui| {
                    // APPROVAL REQUIRED badge in red
                    egui::Frame::none()
                        .fill(egui::Color32::from_rgba_premultiplied(24, 11, 11, 40))
                        .rounding(egui::Rounding::same(20.0))
                        .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgba_premultiplied(62, 28, 28, 80)))
                        .inner_margin(egui::Margin::symmetric(8.0, 3.0))
                        .show(ui, |ui| {
                            ui.label(
                                egui::RichText::new("⚠  APPROVAL REQUIRED")
                                    .size(10.0)
                                    .color(KitsuneTheme::RED)
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
                            .color(KitsuneTheme::TEXT1),
                    );
                });

            paint_separator(ui);

            // ── Action detail card ────────────────────────────────────────
            if !rows.is_empty() {
                egui::Frame::none()
                    .fill(colors::BG_ELEVATED)
                    .rounding(egui::Rounding::same(7.0))
                    .stroke(egui::Stroke::new(1.0, KitsuneTheme::BORDER))
                    .inner_margin(egui::Margin::symmetric(14.0, 10.0))
                    .outer_margin(egui::Margin::symmetric(16.0, 10.0))
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
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    let col = if k == "Total" || k == "Cost" {
                                        KitsuneTheme::AMBER
                                    } else if k == "Credential" {
                                        KitsuneTheme::GREEN
                                    } else {
                                        KitsuneTheme::TEXT_PRIMARY
                                    };
                                    let sz = if k == "Total" || k == "Cost" { 14.0 } else { 11.0 };
                                    ui.label(egui::RichText::new(v).size(sz).color(col).strong());
                                });
                            });
                            if i < rows.len() - 1 {
                                let rect = ui.available_rect_before_wrap();
                                let line = egui::Rect::from_min_size(rect.left_top(), egui::vec2(rect.width(), 1.0));
                                ui.painter().rect_filled(line, 0.0, KitsuneTheme::BORDER);
                                ui.allocate_exact_size(egui::vec2(0.0, 3.0), egui::Sense::hover());
                            }
                        }
                    });
            }

            // ── Countdown timer ───────────────────────────────────────────
            egui::Frame::none()
                .inner_margin(egui::Margin { left: 20.0, right: 20.0, top: 0.0, bottom: 8.0 })
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        let timer_col = if fraction > 0.4 { KitsuneTheme::TEXT2 } else { KitsuneTheme::RED };
                        ui.label(
                            egui::RichText::new(format!("Auto-cancel in {secs_left}s"))
                                .size(10.0)
                                .color(timer_col)
                                .family(egui::FontFamily::Monospace),
                        );
                    });
                    ui.add_space(3.0);
                    let (bar_rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 3.0), egui::Sense::hover());
                    ui.painter().rect_filled(bar_rect, egui::Rounding::same(2.0), colors::BG_ELEVATED);
                    // Bar depletes right→left (fraction shrinks as time runs out)
                    let mut fill = bar_rect;
                    fill.set_right(bar_rect.left() + bar_rect.width() * fraction);
                    let bar_col = if fraction > 0.4 { KitsuneTheme::AMBER } else { KitsuneTheme::RED };
                    ui.painter().rect_filled(fill, egui::Rounding::same(2.0), bar_col);
                });

            paint_separator(ui);

            // ── Buttons ───────────────────────────────────────────────────
            egui::Frame::none()
                .inner_margin(egui::Margin::symmetric(20.0, 14.0))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        // GREEN confirm button
                        let confirm_btn = egui::Button::new(
                            egui::RichText::new("✓ Approve & Execute")
                                .size(12.0)
                                .color(egui::Color32::BLACK)
                                .strong(),
                        )
                        .fill(KitsuneTheme::GREEN)
                        .rounding(egui::Rounding::same(6.0))
                        .min_size(egui::vec2(180.0, 34.0));
                        if ui.add(confirm_btn).clicked() {
                            confirm = true;
                        }

                        ui.add_space(8.0);

                        // Outline-only RED deny button
                        let cancel_btn = egui::Button::new(
                            egui::RichText::new("Deny")
                                .size(12.0)
                                .color(KitsuneTheme::RED),
                        )
                        .fill(egui::Color32::TRANSPARENT)
                        .stroke(egui::Stroke::new(1.0, KitsuneTheme::RED))
                        .rounding(egui::Rounding::same(6.0))
                        .min_size(egui::vec2(72.0, 34.0));
                        if ui.add(cancel_btn).clicked() {
                            cancel = true;
                        }
                    });
                });
        });

    // ── Handle button results ─────────────────────────────────────────────────
    if confirm {
        if let Some(cp) = browser.hil_pending_checkpoint.take() {
            respond_to_checkpoint(cp, true, None);
        } else {
            browser.runtime().spawn(async move {
                let client = reqwest::Client::new();
                let _ = client.post("http://127.0.0.1:7700/api/hil-response")
                    .json(&serde_json::json!({"approved": true}))
                    .send().await;
            });
        }
        browser.push_log("✓  User approved — executing…", LogLevel::Ok);
        browser.hil_action = None;
        // Reset scale-in for next open
        ctx.data_mut(|d| d.insert_temp(egui::Id::new("hil_scale_in"), 0.85_f32));
    } else if cancel {
        if let Some(cp) = browser.hil_pending_checkpoint.take() {
            respond_to_checkpoint(cp, false, Some("Cancelled by user".to_string()));
        } else {
            browser.runtime().spawn(async move {
                let client = reqwest::Client::new();
                let _ = client.post("http://127.0.0.1:7700/api/hil-response")
                    .json(&serde_json::json!({"approved": false}))
                    .send().await;
            });
        }
        browser.push_log("✕  User denied — action aborted", LogLevel::Warn);
        browser.hil_action = None;
        browser.agent_state = AgentRunState::Idle;
        ctx.data_mut(|d| d.insert_temp(egui::Id::new("hil_scale_in"), 0.85_f32));
    }

    ctx.request_repaint();
}

fn paint_separator(ui: &mut egui::Ui) {
    let rect = ui.available_rect_before_wrap();
    let sep  = egui::Rect::from_min_size(rect.left_top(), egui::vec2(rect.width(), 1.0));
    ui.painter().rect_filled(sep, 0.0, KitsuneTheme::BORDER);
    ui.allocate_exact_size(egui::vec2(0.0, 1.0), egui::Sense::hover());
}
