use crate::app::{AgentRunState, AgentSseAction, KitsuneBrowser, LogLevel};
use crate::panels::agent_card::{AgentCard, AgentStatus};
use crate::theme::KitsuneTheme;
use eframe::egui;
use std::sync::mpsc::Sender;

pub fn agent_panel(ctx: &egui::Context, browser: &mut KitsuneBrowser) {
    let is_running = browser.agent_state == AgentRunState::Running;
    let is_hil = browser.agent_state == AgentRunState::AwaitingHil;
    let is_busy = is_running || is_hil;

    egui::SidePanel::left("agent_panel")
        .resizable(true)
        .default_width(280.0)
        .min_width(220.0)
        .max_width(420.0)
        .frame(
            egui::Frame::none()
                .fill(KitsuneTheme::BG_PANEL)
                .stroke(egui::Stroke::new(1.0, KitsuneTheme::BORDER)),
        )
        .show(ctx, |ui| {
            // ── Header ────────────────────────────────────────────────────
            egui::Frame::none()
                .inner_margin(egui::Margin::symmetric(12.0, 8.0))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new("AGENT WORKSPACE")
                                .size(10.0)
                                .strong()
                                .color(KitsuneTheme::TEXT2)
                                .family(egui::FontFamily::Monospace),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let dot_col = if is_running {
                                KitsuneTheme::GREEN_SAFE
                            } else if is_hil {
                                KitsuneTheme::AMBER
                            } else {
                                KitsuneTheme::TEXT3
                            };
                            let (rect, _) =
                                ui.allocate_exact_size(egui::vec2(7.0, 7.0), egui::Sense::hover());
                            ui.painter().circle_filled(rect.center(), 3.5, dot_col);
                        });
                    });
                });

            // Thin separator line
            let sep_rect = ui.available_rect_before_wrap();
            let sep_rect = egui::Rect::from_min_size(
                sep_rect.left_top(),
                egui::vec2(sep_rect.width(), 1.0),
            );
            ui.painter().rect_filled(sep_rect, 0.0, KitsuneTheme::BORDER);
            ui.allocate_exact_size(egui::vec2(0.0, 1.0), egui::Sense::hover());

            // ── Command input ─────────────────────────────────────────
            egui::Frame::none()
                .inner_margin(egui::Margin::symmetric(12.0, 8.0))
                .show(ui, |ui| {
                    // Text input row
                    let te = egui::TextEdit::singleline(&mut browser.agent_command)
                        .desired_width(ui.available_width())
                        .frame(true)
                        .hint_text("Ask agent to do anything…")
                        .text_color(KitsuneTheme::TEXT_PRIMARY);
                    let r = ui.add(te);
                    let enter_pressed = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));

                    ui.add_space(4.0);

                    // Button row
                    ui.horizontal(|ui| {
                        if is_busy {
                            // Stop button (red)
                            let stop_btn = egui::Button::new(
                                egui::RichText::new("■ Stop")
                                    .size(11.0)
                                    .color(KitsuneTheme::TEXT_PRIMARY)
                                    .strong(),
                            )
                            .fill(KitsuneTheme::RED)
                            .min_size(egui::vec2(60.0, 26.0));
                            if ui.add(stop_btn).clicked() {
                                browser.agent_state = AgentRunState::Idle;
                                browser.hil_action = None;
                                browser.push_log("■  Agent stopped by user", LogLevel::Warn);
                            }
                        } else {
                            // Run button (amber)
                            let run_btn = egui::Button::new(
                                egui::RichText::new("▶ Run")
                                    .size(11.0)
                                    .color(egui::Color32::BLACK)
                                    .strong(),
                            )
                            .fill(KitsuneTheme::AMBER)
                            .min_size(egui::vec2(60.0, 26.0));
                            if ui.add(run_btn).clicked() || (enter_pressed && !is_busy) {
                                start_agent_run(browser);
                            }
                        }
                        ui.add_space(4.0);
                        // Clear log button
                        if !browser.agent_log.is_empty() {
                            let clear_btn = egui::Button::new(
                                egui::RichText::new("Clear")
                                    .size(10.0)
                                    .color(KitsuneTheme::TEXT2),
                            )
                            .frame(false);
                            if ui.add(clear_btn).clicked() {
                                browser.agent_log.clear();
                            }
                        }
                    });
                });

            // Separator
            paint_separator(ui);

            // ── Agent cards ───────────────────────────────────────────
            let agents: &[AgentCard] = &[
                AgentCard {
                    icon: "✈",
                    name: "PriceTracker",
                    description: "Find the cheapest flight, hotel, or product",
                    status: if is_running { AgentStatus::Running } else { AgentStatus::Idle },
                },
                AgentCard {
                    icon: "📝",
                    name: "FormFillAgent",
                    description: "Fill forms with your saved credentials",
                    status: AgentStatus::Idle,
                },
                AgentCard {
                    icon: "🔬",
                    name: "ResearchAgent",
                    description: "Research a topic and generate a report",
                    status: AgentStatus::Idle,
                },
            ];
            egui::Frame::none()
                .inner_margin(egui::Margin::symmetric(12.0, 6.0))
                .show(ui, |ui| {
                    for card in agents {
                        card.render(ui);
                        ui.add_space(4.0);
                    }
                });

            paint_separator(ui);

            // ── Agent log ─────────────────────────────────────────────
            egui::Frame::none()
                .inner_margin(egui::Margin::symmetric(12.0, 4.0))
                .show(ui, |ui| {
                    ui.label(
                        egui::RichText::new("AGENT LOG")
                            .size(10.0)
                            .strong()
                            .color(KitsuneTheme::TEXT2)
                            .family(egui::FontFamily::Monospace),
                    );
                });

            let log_height = (ui.available_height() - 48.0).max(40.0);
            egui::ScrollArea::vertical()
                .max_height(log_height)
                .stick_to_bottom(true)
                .auto_shrink([false; 2])
                .show(ui, |ui| {
                    egui::Frame::none()
                        .inner_margin(egui::Margin::symmetric(12.0, 4.0))
                        .show(ui, |ui| {
                            if browser.agent_log.is_empty() {
                                ui.label(
                                    egui::RichText::new("No activity yet.")
                                        .size(11.0)
                                        .color(KitsuneTheme::TEXT3)
                                        .family(egui::FontFamily::Monospace),
                                );
                            }
                            for entry in &browser.agent_log {
                                ui.label(
                                    egui::RichText::new(&entry.text)
                                        .size(11.0)
                                        .color(entry.color())
                                        .family(egui::FontFamily::Monospace),
                                );
                            }
                        });
                });

            // ── Budget gauge (sticky bottom) ──────────────────────────
            paint_separator(ui);
            egui::Frame::none()
                .inner_margin(egui::Margin::symmetric(12.0, 8.0))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new("Budget")
                                .size(10.0)
                                .color(KitsuneTheme::TEXT2),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(
                                egui::RichText::new(format!(
                                    "{}/{}",
                                    browser.budget.used, browser.budget.total
                                ))
                                .size(10.0)
                                .color(KitsuneTheme::TEXT2)
                                .family(egui::FontFamily::Monospace),
                            );
                        });
                    });
                    ui.add_space(3.0);
                    let frac = browser.budget.fraction();
                    let (bar_rect, _) = ui.allocate_exact_size(
                        egui::vec2(ui.available_width(), 3.0),
                        egui::Sense::hover(),
                    );
                    ui.painter().rect_filled(
                        bar_rect,
                        egui::Rounding::same(2.0),
                        KitsuneTheme::BG4,
                    );
                    let mut fill_rect = bar_rect;
                    fill_rect.set_right(bar_rect.left() + bar_rect.width() * frac);
                    let bar_col = if frac > 0.8 { KitsuneTheme::RED } else { KitsuneTheme::AMBER };
                    ui.painter().rect_filled(
                        fill_rect,
                        egui::Rounding::same(2.0),
                        bar_col,
                    );
                });
        });
}

fn paint_separator(ui: &mut egui::Ui) {
    let rect = ui.available_rect_before_wrap();
    let sep = egui::Rect::from_min_size(rect.left_top(), egui::vec2(rect.width(), 1.0));
    ui.painter().rect_filled(sep, 0.0, KitsuneTheme::BORDER);
    ui.allocate_exact_size(egui::vec2(0.0, 1.0), egui::Sense::hover());
}

// ── Real agent execution via SSE ──────────────────────────────────────────────

fn start_agent_run(browser: &mut KitsuneBrowser) {
    browser.agent_state = AgentRunState::Running;
    browser.agent_log.clear();
    browser.privacy.trackers_blocked = 0;
    browser.privacy.referrers_stripped = 0;

    let cmd = if browser.agent_command.is_empty() {
        "book cheapest flight to Berlin".to_string()
    } else {
        browser.agent_command.clone()
    };

    browser.push_log(format!("▸ agent run \"{}\"", cmd), LogLevel::Cmd);

    let tx = browser.agent_tx();

    // Spawn async task to call the agent-run SSE endpoint
    browser.runtime().spawn(async move {
        run_agent_sse(cmd, tx).await;
    });
}

async fn run_agent_sse(command: String, tx: Sender<AgentSseAction>) {
    let client = reqwest::Client::new();

    let resp = match client
        .post("http://127.0.0.1:7700/api/agent-run")
        .json(&serde_json::json!({ "command": command }))
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => {
            let _ = tx.send(AgentSseAction::Log {
                message: format!("Connection error: {}", e),
                class: "block".into(),
            });
            let _ = tx.send(AgentSseAction::Done { message: String::new() });
            return;
        }
    };

    if !resp.status().is_success() {
        let _ = tx.send(AgentSseAction::Log {
            message: format!("API error: {}", resp.status()),
            class: "block".into(),
        });
        let _ = tx.send(AgentSseAction::Done { message: String::new() });
        return;
    }

    // Read SSE stream
    // Axum SSE sends lines like:
    //   event: action
    //   data: {"type":"log","message":"..."}
    //   \n
    //   event: done
    //   data: {}
    //   \n
    let mut buffer = String::new();
    let mut stream = resp.bytes_stream();
    let mut current_event = String::new();
    use futures::StreamExt;

    while let Some(chunk) = stream.next().await {
        let chunk = match chunk {
            Ok(c) => c,
            Err(_) => break,
        };
        buffer.push_str(&String::from_utf8_lossy(&chunk));

        while let Some(idx) = buffer.find('\n') {
            let line = buffer[..idx].trim_end().to_string();
            buffer = buffer[idx + 1..].to_string();

            // Track the event type
            if let Some(evt) = line.strip_prefix("event:") {
                current_event = evt.trim().to_string();
                continue;
            }

            // Process data lines
            if let Some(data) = line.strip_prefix("data:") {
                let data = data.trim();

                // Handle stream-end event
                if current_event == "done" {
                    let _ = tx.send(AgentSseAction::Done { message: String::new() });
                    current_event.clear();
                    continue;
                }

                if data.is_empty() {
                    continue;
                }

                if let Ok(action) = serde_json::from_str::<serde_json::Value>(data) {
                    if let Some(act) = parse_sse_action(&action) {
                        let _ = tx.send(act);
                    }
                }
                current_event.clear();
                continue;
            }

            // Empty line = end of SSE message, reset event
            if line.is_empty() {
                current_event.clear();
            }
        }
    }

    // Stream ended — ensure we always signal done
    let _ = tx.send(AgentSseAction::Done { message: String::new() });
}

fn parse_sse_action(v: &serde_json::Value) -> Option<AgentSseAction> {
    let action_type = v.get("type")?.as_str()?;
    match action_type {
        "log" => Some(AgentSseAction::Log {
            message: v.get("message")?.as_str()?.to_string(),
            class: v.get("class").and_then(|c| c.as_str()).unwrap_or("info").to_string(),
        }),
        "agent_status" => Some(AgentSseAction::AgentStatus {
            agent: v.get("agent")?.as_str()?.to_string(),
            status: v.get("status")?.as_str()?.to_string(),
        }),
        "tracker_blocked" => Some(AgentSseAction::TrackerBlocked {
            label: v.get("label")?.as_str()?.to_string(),
            stripped: v.get("stripped").and_then(|s| s.as_bool()).unwrap_or(false),
        }),
        "hil_request" => Some(AgentSseAction::HilRequest {
            action: v.get("action").and_then(|s| s.as_str()).unwrap_or("").to_string(),
            flight: v.get("flight").and_then(|s| s.as_str()).unwrap_or("").to_string(),
            date: v.get("date").and_then(|s| s.as_str()).unwrap_or("").to_string(),
            passenger: v.get("passenger").and_then(|s| s.as_str()).unwrap_or("").to_string(),
            total: v.get("total").and_then(|s| s.as_str()).unwrap_or("").to_string(),
            credentials: v.get("credentials").and_then(|s| s.as_str()).unwrap_or("").to_string(),
        }),
        "url_update" => Some(AgentSseAction::UrlUpdate {
            url: v.get("url")?.as_str()?.to_string(),
        }),
        "hil_approved" => Some(AgentSseAction::HilApproved),
        "hil_cancelled" => Some(AgentSseAction::HilCancelled),
        "done" => Some(AgentSseAction::Done {
            message: v.get("message").and_then(|s| s.as_str()).unwrap_or("").to_string(),
        }),
        _ => None,
    }
}
