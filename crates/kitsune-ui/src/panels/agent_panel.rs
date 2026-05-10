use crate::app::{AgentRunState, AgentSseAction, AttachedFile, KitsuneBrowser, LogEntry, LogLevel, SettingsProvider, TokenUsageState};
use kitsune_agent::ai_client::{AgentAiClient, AiProviderConfig, ModelSlots};
use crate::panels::agent_card::{AgentCard, AgentStatus};
use crate::theme::KitsuneTheme;
use eframe::egui;
use kitsune_agent::spec::{
    AgentAuthor, AgentBudget, AgentConstraints, AgentGoal, AgentId, AgentSpec, AgentTool,
    DomainPolicy,
};
use kitsune_agent::swarm::types::{SwarmConfig, SwarmMode};
use kitsune_agent::{AgentEvent, FilePermSlot, LlmAgentRuntime, StopFlag};
use std::sync::atomic::Ordering;
use std::sync::mpsc::Sender;
use std::sync::Arc;

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
                    // ── Attach button row ─────────────────────────────────
                    // Keep the button on its own row so the text area below
                    // gets the full panel width — avoids the horizontal-
                    // overflow bug where singleline TextEdit inside horizontal()
                    // would widen the panel as text grew.
                    ui.horizontal(|ui| {
                        let attach_btn = egui::Button::new(
                            egui::RichText::new("📎").size(14.0),
                        )
                        .frame(true)
                        .min_size(egui::vec2(28.0, 28.0));
                        if ui.add(attach_btn).on_hover_text("Attach a local file").clicked() {
                            if let Some(path) = rfd::FileDialog::new().pick_file() {
                                let name = path
                                    .file_name()
                                    .map(|n| n.to_string_lossy().to_string())
                                    .unwrap_or_default();
                                match std::fs::read(&path) {
                                    Ok(bytes) => {
                                        // Try strict UTF-8 first; fall back to lossy for
                                        // PDFs and other binary formats.
                                        let (content, lossy) = match String::from_utf8(bytes.clone()) {
                                            Ok(s) => (s, false),
                                            Err(_) => (String::from_utf8_lossy(&bytes).into_owned(), true),
                                        };
                                        browser.attached_files.push(AttachedFile {
                                            name: name.clone(),
                                            path: path.to_string_lossy().to_string(),
                                            content,
                                        });
                                        if lossy {
                                            browser.push_log(
                                                format!("📄 Attached: {} (binary file — text extraction may be partial)", name),
                                                LogLevel::Warn,
                                            );
                                        } else {
                                            browser.push_log(
                                                format!("📄 Attached: {}", name),
                                                LogLevel::Ok,
                                            );
                                        }
                                    }
                                    Err(e) => {
                                        browser.push_log(
                                            format!("Cannot read file: {}", e),
                                            LogLevel::Warn,
                                        );
                                    }
                                }
                            }
                        }
                        // Character counter — muted, right-aligned.
                        let char_count = browser.agent_command.trim().chars().count();
                        if char_count > 0 {
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                ui.label(
                                    egui::RichText::new(format!("{}", char_count))
                                        .size(9.5)
                                        .color(KitsuneTheme::TEXT3)
                                        .family(egui::FontFamily::Monospace),
                                );
                            });
                        }
                    });

                    ui.add_space(3.0);

                    // ── Multiline command input ────────────────────────────
                    // Multiline wraps text vertically so the panel never
                    // expands horizontally as the user types.
                    // Plain Enter submits; Shift+Enter inserts a newline.
                    let te = egui::TextEdit::multiline(&mut browser.agent_command)
                        .desired_width(f32::INFINITY) // fill panel width
                        .desired_rows(2)
                        .frame(true)
                        .hint_text("Ask agent to do anything… (Enter to run, Shift+Enter for newline)")
                        .text_color(KitsuneTheme::TEXT_PRIMARY);
                    let te_resp = ui.add(te);

                    // Strip newline that multiline inserts on plain Enter,
                    // keeping Shift+Enter for intentional line breaks.
                    let plain_enter = ctx.input(|i| {
                        i.key_pressed(egui::Key::Enter) && !i.modifiers.shift
                    });
                    if te_resp.has_focus() && plain_enter {
                        // Remove the trailing newline the TextEdit just appended.
                        if browser.agent_command.ends_with('\n') {
                            browser.agent_command.pop();
                        }
                    }

                    // Only trigger Enter-to-run when not blocked by open modal dialogs.
                    let enter_pressed = plain_enter
                        && te_resp.has_focus()
                        && !browser.show_settings
                        && browser.hil_action.is_none()
                        && browser.file_perm_pending.is_none();

                    // Attached files chips
                    if !browser.attached_files.is_empty() {
                        ui.add_space(4.0);
                        let mut to_remove: Option<usize> = None;
                        for (i, file) in browser.attached_files.iter().enumerate() {
                            ui.horizontal(|ui| {
                                egui::Frame::none()
                                    .fill(KitsuneTheme::BG2)
                                    .rounding(egui::Rounding::same(4.0))
                                    .inner_margin(egui::Margin::symmetric(6.0, 2.0))
                                    .show(ui, |ui| {
                                        ui.label(
                                            egui::RichText::new(format!("📄 {}", file.name))
                                                .size(10.0)
                                                .color(KitsuneTheme::TEXT2),
                                        );
                                        if ui
                                            .small_button(
                                                egui::RichText::new("×").color(KitsuneTheme::TEXT3),
                                            )
                                            .clicked()
                                        {
                                            to_remove = Some(i);
                                        }
                                    });
                            });
                        }
                        if let Some(i) = to_remove {
                            browser.attached_files.remove(i);
                        }
                    }

                    ui.add_space(4.0);

                    // ── Swarm config bar (visible only when swarm mode active) ──
                    if browser.swarm_mode {
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new("Workers:").size(9.5).color(KitsuneTheme::TEXT3));
                            egui::ComboBox::from_id_salt("swarm_max_workers")
                                .selected_text(browser.swarm_config.max_workers.to_string())
                                .width(48.0)
                                .show_ui(ui, |ui| {
                                    for n in [3usize, 5, 10, 20, 50] {
                                        ui.selectable_value(
                                            &mut browser.swarm_config.max_workers,
                                            n,
                                            n.to_string(),
                                        );
                                    }
                                });
                            ui.separator();
                            ui.label(egui::RichText::new("Mode:").size(9.5).color(KitsuneTheme::TEXT3));
                            egui::ComboBox::from_id_salt("swarm_mode_select")
                                .selected_text(match browser.swarm_config.mode {
                                    SwarmMode::DiscoveryAtScale => "Discovery",
                                    SwarmMode::OutputAtScale => "Output",
                                    SwarmMode::PerspectiveAtScale => "Perspective",
                                })
                                .width(82.0)
                                .show_ui(ui, |ui| {
                                    ui.selectable_value(
                                        &mut browser.swarm_config.mode,
                                        SwarmMode::DiscoveryAtScale,
                                        "Discovery",
                                    );
                                    ui.selectable_value(
                                        &mut browser.swarm_config.mode,
                                        SwarmMode::OutputAtScale,
                                        "Output",
                                    );
                                    ui.selectable_value(
                                        &mut browser.swarm_config.mode,
                                        SwarmMode::PerspectiveAtScale,
                                        "Perspective",
                                    );
                                });
                            ui.separator();
                            ui.checkbox(
                                &mut browser.swarm_config.enable_disagreement,
                                egui::RichText::new("Disagree").size(9.5),
                            );
                        });
                        ui.add_space(3.0);
                    }

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
                                // Signal the async loop to exit at the next iteration.
                                browser.agent_stop_flag.store(true, Ordering::Relaxed);
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
                        let swarm_text = if browser.swarm_mode {
                            egui::RichText::new("🐝 Swarm")
                                .size(10.0)
                                .color(egui::Color32::from_rgb(255, 200, 0))
                                .strong()
                        } else {
                            egui::RichText::new("🐝 Swarm")
                                .size(10.0)
                                .color(KitsuneTheme::TEXT3)
                        };
                        if ui.button(swarm_text).clicked() && !is_busy {
                            browser.swarm_mode = !browser.swarm_mode;
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
                    description: "Compare prices across sites — flights, hotels, products",
                    status: if is_running { AgentStatus::Running } else { AgentStatus::Idle },
                },
                AgentCard {
                    icon: "📝",
                    name: "FormFillAgent",
                    description: "Fill forms using your attached CV or document",
                    status: AgentStatus::Idle,
                },
                AgentCard {
                    icon: "🔬",
                    name: "ResearchAgent",
                    description: "Deep-research a topic and write a structured report",
                    status: AgentStatus::Idle,
                },
            ];
            egui::Frame::none()
                .inner_margin(egui::Margin::symmetric(12.0, 6.0))
                .show(ui, |ui| {
                    for card in agents {
                        let selected = browser.selected_agent_card.as_deref() == Some(card.name);
                        if card.render(ui, selected) && !is_busy {
                            if selected {
                                // Deselect if clicking the active card.
                                browser.selected_agent_card = None;
                            } else {
                                browser.selected_agent_card = Some(card.name.to_string());
                                // Pre-fill only when the command box is empty.
                                if browser.agent_command.trim().is_empty() {
                                    browser.agent_command = default_command_for_card(card.name);
                                }
                            }
                        }
                        ui.add_space(4.0);
                    }

                    // ── Swarm preset cards ────────────────────────────────
                    ui.add_space(6.0);
                    ui.label(
                        egui::RichText::new("SWARM PRESETS")
                            .size(9.0)
                            .color(KitsuneTheme::TEXT3)
                            .family(egui::FontFamily::Monospace),
                    );
                    ui.add_space(3.0);
                    ui.horizontal_wrapped(|ui| {
                        if ui.selectable_label(
                            browser.swarm_mode && browser.swarm_config.mode == SwarmMode::DiscoveryAtScale,
                            "🔍 Discovery",
                        ).clicked() && !is_busy {
                            browser.swarm_mode = true;
                            browser.swarm_config.mode = SwarmMode::DiscoveryAtScale;
                            browser.swarm_config.max_workers = 20;
                        }
                        if ui.selectable_label(
                            browser.swarm_mode && browser.swarm_config.mode == SwarmMode::OutputAtScale,
                            "📄 Report",
                        ).clicked() && !is_busy {
                            browser.swarm_mode = true;
                            browser.swarm_config.mode = SwarmMode::OutputAtScale;
                            browser.swarm_config.max_workers = 10;
                        }
                        if ui.selectable_label(
                            browser.swarm_mode && browser.swarm_config.mode == SwarmMode::PerspectiveAtScale,
                            "🧠 Expert Panel",
                        ).clicked() && !is_busy {
                            browser.swarm_mode = true;
                            browser.swarm_config.mode = SwarmMode::PerspectiveAtScale;
                            browser.swarm_config.max_workers = 5;
                            browser.swarm_config.enable_disagreement = true;
                        }
                    });
                });

            // ── Swarm status bar (only when swarm is active) ──────────────
            if let Some(state) = &browser.swarm_state {
                egui::Frame::none()
                    .inner_margin(egui::Margin::symmetric(12.0, 5.0))
                    .fill(KitsuneTheme::BG2)
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new("🐝 SWARM")
                                    .size(9.5)
                                    .strong()
                                    .color(egui::Color32::from_rgb(255, 200, 0))
                                    .family(egui::FontFamily::Monospace),
                            );
                            ui.separator();
                            ui.label(egui::RichText::new(format!("🔵 {}", state.active_count())).size(9.5));
                            ui.label(egui::RichText::new(format!("✅ {}", state.completed_count())).size(9.5));
                            ui.label(egui::RichText::new(format!("🟡 {}", state.pending_count())).size(9.5));
                        });
                    });
                paint_separator(ui);
            }

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

            // Detect new log entries so we can scroll to show them without
            // using stick_to_bottom (which pins content to the bottom of the
            // allocated space and produces a dark gap above when few entries exist).
            let log_scroll_id = egui::Id::new("agent_log_scroll_state");
            let prev_count = ui.ctx().data(|d| d.get_temp::<usize>(log_scroll_id).unwrap_or(0));
            let curr_count = browser.agent_log.len();
            ui.ctx().data_mut(|d| d.insert_temp(log_scroll_id, curr_count));
            let has_new = curr_count > prev_count;

            let log_height = (ui.available_height() - 48.0).max(40.0);
            egui::ScrollArea::vertical()
                .max_height(log_height)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    egui::Frame::none()
                        .inner_margin(egui::Margin::symmetric(12.0, 6.0))
                        .show(ui, |ui| {
                            if browser.agent_log.is_empty() {
                                ui.label(
                                    egui::RichText::new("No activity yet.")
                                        .size(11.0)
                                        .color(KitsuneTheme::TEXT3)
                                        .family(egui::FontFamily::Monospace),
                                );
                            }
                            for (i, entry) in browser.agent_log.iter().enumerate() {
                                ui.push_id(i, |ui| {
                                    render_log_entry(ui, entry);
                                });
                            }
                            if has_new {
                                ui.scroll_to_cursor(Some(egui::Align::BOTTOM));
                            }
                        });
                });

            // ── Token usage (sticky bottom) ───────────────────────────
            paint_separator(ui);
            egui::Frame::none()
                .inner_margin(egui::Margin::symmetric(12.0, 7.0))
                .show(ui, |ui| {
                    let inp = browser.token_usage.input_tokens;
                    let out = browser.token_usage.output_tokens;
                    let cost_str = token_cost_display(&browser.settings_model, inp, out);
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new("in")
                                .size(9.0)
                                .color(KitsuneTheme::TEXT3)
                                .family(egui::FontFamily::Monospace),
                        );
                        ui.label(
                            egui::RichText::new(fmt_tokens(inp))
                                .size(10.0)
                                .color(KitsuneTheme::TEXT2)
                                .family(egui::FontFamily::Monospace),
                        );
                        ui.add_space(8.0);
                        ui.label(
                            egui::RichText::new("out")
                                .size(9.0)
                                .color(KitsuneTheme::TEXT3)
                                .family(egui::FontFamily::Monospace),
                        );
                        ui.label(
                            egui::RichText::new(fmt_tokens(out))
                                .size(10.0)
                                .color(KitsuneTheme::TEXT2)
                                .family(egui::FontFamily::Monospace),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let col = if cost_str == "free" || cost_str == "local" {
                                KitsuneTheme::GREEN
                            } else {
                                KitsuneTheme::TEXT2
                            };
                            ui.label(
                                egui::RichText::new(&cost_str)
                                    .size(10.0)
                                    .color(col)
                                    .family(egui::FontFamily::Monospace),
                            );
                        });
                    });
                });
        });
}

/// Render a single log entry with styled prefix, color, and collapsible thinking.
fn render_log_entry(ui: &mut egui::Ui, entry: &LogEntry) {
    match entry.level {
        LogLevel::Think => {
            // Collapsible block — short preview in the header, full text when expanded.
            let preview: String = entry.text.chars().take(72).collect();
            let preview = if entry.text.chars().count() > 72 {
                format!("{}…", preview)
            } else {
                preview.clone()
            };
            let header_text = egui::RichText::new(format!("💭 {}", preview))
                .size(10.0)
                .italics()
                .color(KitsuneTheme::TEXT3)
                .family(egui::FontFamily::Monospace);
            egui::CollapsingHeader::new(header_text)
                .default_open(false)
                .show(ui, |ui| {
                    egui::Frame::none()
                        .fill(KitsuneTheme::BG2)
                        .rounding(egui::Rounding::same(4.0))
                        .inner_margin(egui::Margin::symmetric(8.0, 6.0))
                        .show(ui, |ui| {
                            ui.label(
                                egui::RichText::new(&entry.text)
                                    .size(10.0)
                                    .italics()
                                    .color(KitsuneTheme::TEXT3)
                                    .family(egui::FontFamily::Monospace),
                            );
                        });
                });
        }
        LogLevel::Cmd => {
            ui.add_space(2.0);
            ui.label(
                egui::RichText::new(&entry.text)
                    .size(12.0)
                    .strong()
                    .color(KitsuneTheme::BLUE)
                    .family(egui::FontFamily::Monospace),
            );
            ui.add_space(1.0);
        }
        LogLevel::Ok => {
            ui.label(
                egui::RichText::new(&entry.text)
                    .size(11.0)
                    .strong()
                    .color(KitsuneTheme::GREEN)
                    .family(egui::FontFamily::Monospace),
            );
        }
        LogLevel::Warn => {
            ui.label(
                egui::RichText::new(&entry.text)
                    .size(11.0)
                    .color(KitsuneTheme::AMBER)
                    .family(egui::FontFamily::Monospace),
            );
        }
        LogLevel::Block => {
            ui.label(
                egui::RichText::new(&entry.text)
                    .size(11.0)
                    .color(KitsuneTheme::RED)
                    .family(egui::FontFamily::Monospace),
            );
        }
        LogLevel::Step => {
            ui.horizontal(|ui| {
                ui.add_space(10.0);
                ui.label(
                    egui::RichText::new(&entry.text)
                        .size(10.5)
                        .color(KitsuneTheme::TEXT1)
                        .family(egui::FontFamily::Monospace),
                );
            });
        }
        LogLevel::Info => {
            ui.label(
                egui::RichText::new(&entry.text)
                    .size(10.5)
                    .color(KitsuneTheme::TEXT2)
                    .family(egui::FontFamily::Monospace),
            );
        }
    }
}

fn paint_separator(ui: &mut egui::Ui) {
    let rect = ui.available_rect_before_wrap();
    let sep = egui::Rect::from_min_size(rect.left_top(), egui::vec2(rect.width(), 1.0));
    ui.painter().rect_filled(sep, 0.0, KitsuneTheme::BORDER);
    ui.allocate_exact_size(egui::vec2(0.0, 1.0), egui::Sense::hover());
}

// ── In-process agent execution (LLM-driven — Ollama or cloud provider) ──────────

fn start_agent_run(browser: &mut KitsuneBrowser) {
    browser.agent_state = AgentRunState::Running;
    browser.agent_log.clear();
    browser.token_usage = TokenUsageState::default();
    browser.privacy.trackers_blocked = 0;
    browser.privacy.referrers_stripped = 0;

    // Reset the stop flag for this new run.
    browser.agent_stop_flag.store(false, Ordering::Relaxed);

    let cmd = {
        let trimmed = browser.agent_command.trim().to_string();
        if trimmed.is_empty() {
            "go to wikipedia.org and tell me what the featured article is".to_string()
        } else {
            trimmed
        }
    };

    // Show the run command once in the log (the runtime itself does NOT re-log it).
    browser.push_log(format!("▸ {}", cmd), LogLevel::Cmd);

    // Vault is the only piece that may be missing on dev systems without a keyring.
    let Some(vault) = browser.vault.clone() else {
        browser.push_log(
            "vault unavailable — check OS keyring access and restart",
            LogLevel::Block,
        );
        browser.agent_state = AgentRunState::Idle;
        return;
    };
    let hil_gate = browser.hil_gate.clone();
    let webview_tx = browser.webview_cmd_tx();
    let agent_tx = browser.agent_tx();
    let file_perm_slot = browser.file_perm_slot.clone();
    let stop_flag = browser.agent_stop_flag.clone();
    let spec = build_runtime_spec(browser);
    let agent_context = browser.selected_agent_card
        .as_deref()
        .map(specialist_context)
        .unwrap_or_default();

    let endpoint = browser.settings_endpoint.trim().to_string();
    let model = browser.settings_model.trim().to_string();
    let api_key = browser.settings_api_key.clone();
    let preset = browser.settings_cloud_preset;
    let ai_config = match browser.settings_provider {
        SettingsProvider::Ollama => {
            let url = if endpoint.is_empty() {
                "http://localhost:11434".to_string()
            } else {
                endpoint
            };
            let m = if model.is_empty() { "llama3".to_string() } else { model };
            AiProviderConfig::Ollama {
                url,
                slots: ModelSlots { orchestrator: m.clone(), worker: m.clone(), fast: m },
            }
        }
        SettingsProvider::Cloud => {
            if endpoint.is_empty() {
                browser.push_log(
                    "Cloud endpoint is not set — open Settings → LLM and enter the API base URL",
                    LogLevel::Block,
                );
                browser.agent_state = AgentRunState::Idle;
                return;
            }
            // Fall back to the active preset's default model so Claude/Gemini/Groq users
            // don't get an OpenAI-specific model name when they leave the field empty.
            let m = if model.is_empty() {
                preset.default_model().to_string()
            } else {
                model
            };
            AiProviderConfig::OpenAiCompatible {
                url: endpoint,
                api_key,
                slots: ModelSlots { orchestrator: m.clone(), worker: m.clone(), fast: m },
            }
        }
    };

    // ── Swarm branch ─────────────────────────────────────────────────────────
    if browser.swarm_mode {
        let mut swarm_config = browser.swarm_config.clone();
        if swarm_config.max_workers == 0 {
            swarm_config.max_workers = 1;
        }
        let ai_client = match AgentAiClient::new(ai_config.clone()) {
            Ok(c) => Arc::new(c),
            Err(e) => {
                browser.push_log(format!("Failed to build AI client for swarm: {}", e), LogLevel::Block);
                browser.agent_state = AgentRunState::Idle;
                return;
            }
        };
        browser.swarm_state = None;
        let goal = cmd.clone();
        browser.runtime().spawn(async move {
            run_swarm(
                spec, ai_config, ai_client, swarm_config, goal,
                vault, hil_gate, webview_tx, agent_tx, stop_flag,
            )
            .await;
        });
        return;
    }

    // Build context from any attached files.
    // Binary files (PDF, DOCX, …) produce garbage when read as raw bytes — reference
    // them by name only so the model can use read_file to access them on demand.
    let context = if browser.attached_files.is_empty() {
        String::new()
    } else {
        browser
            .attached_files
            .iter()
            .map(|f| {
                let ext = f.name.rsplit('.').next().unwrap_or("").to_lowercase();
                let is_binary = matches!(ext.as_str(), "pdf" | "docx" | "xlsx" | "pptx" | "doc" | "xls" | "ppt" | "zip" | "png" | "jpg" | "jpeg" | "gif" | "webp");
                if is_binary {
                    format!(
                        "=== ATTACHED FILE: {} ===\n(Binary file — use {{\"action\":\"read_file\",\"path\":\"{}\"}} to read it)\n=== END ===",
                        f.name, f.path
                    )
                } else {
                    // Truncate very large text files so they don't overflow the context window.
                    let content = if f.content.len() > 12_000 {
                        format!("{}\n… (truncated)", &f.content[..12_000])
                    } else {
                        f.content.clone()
                    };
                    format!("=== ATTACHED: {} ===\n{}\n=== END ===", f.name, content)
                }
            })
            .collect::<Vec<_>>()
            .join("\n\n")
    };

    browser.runtime().spawn(async move {
        run_in_process_agent(spec, ai_config, cmd, context, agent_context, vault, hil_gate, webview_tx, agent_tx, file_perm_slot, stop_flag).await;
    });

    // ── Orchestrator parallel path ────────────────────────────────────────
    // When the orchestrator is available AND a profile has been indexed, run
    // the goal through the full multi-agent pipeline alongside the LLM loop.
    // This is additive — both paths can run concurrently; the orchestrator
    // result is logged but does not affect the LLM agent's execution.
    if let (Some(orch), Some(summary)) =
        (browser.orchestrator.clone(), browser.profile_summary.clone())
    {
        let goal = browser.agent_command.trim().to_string();
        if !goal.is_empty() {
            std::thread::spawn(move || {
                let rt = tokio::runtime::Runtime::new().expect("tokio rt for orchestrator");
                match rt.block_on(orch.run(&goal, &summary)) {
                    Ok(results) => tracing::info!("Orchestrator results: {:?}", results),
                    Err(e) => tracing::error!("Orchestrator error: {e}"),
                }
            });
        }
    }
}

fn build_runtime_spec(browser: &KitsuneBrowser) -> AgentSpec {
    let now = chrono::Utc::now();

    // Settings panel drives both the AiProviderConfig (built in start_agent_run) and
    // the AgentSpec compat fields below. ollama_url/ollama_model on the spec are
    // legacy fields used only by LlmAgentRuntime::new; they are ignored when
    // new_with_config is called.
    let endpoint = browser.settings_endpoint.trim();
    let model = browser.settings_model.trim();

    AgentSpec {
        id: AgentId::new(),
        name: "InProcessAgent".to_string(),
        description: "LLM-driven in-process browser agent".to_string(),
        goal: AgentGoal {
            description: "Drive the live WebView to satisfy the user's request".to_string(),
            structured_objective: None,
            success_criteria: vec!["Agent emits done with an answer".to_string()],
        },
        actions: vec![],
        allowed_tools: vec![
            AgentTool::Navigate,
            AgentTool::DomRead,
            AgentTool::Click,
            AgentTool::FormFill,
            AgentTool::TextExtract,
        ],
        constraints: AgentConstraints {
            allowed_domains: DomainPolicy::AllowAll,
            ..Default::default()
        },
        triggers: vec![],
        budget: AgentBudget::default(),
        created_by: AgentAuthor::System,
        version: "0.1.0".to_string(),
        created_at: now,
        modified_at: now,
        ollama_url: if endpoint.is_empty() {
            None
        } else {
            Some(endpoint.to_string())
        },
        ollama_model: if model.is_empty() {
            None
        } else {
            Some(model.to_string())
        },
    }
}

async fn run_in_process_agent(
    spec: AgentSpec,
    ai_config: AiProviderConfig,
    prompt: String,
    context: String,
    agent_context: String,
    vault: std::sync::Arc<kitsune_vault::VaultBackend>,
    hil_gate: std::sync::Arc<kitsune_hil::HilGate>,
    webview_tx: tokio::sync::mpsc::Sender<kitsune_agent::executor::WebViewCommand>,
    ui_tx: Sender<AgentSseAction>,
    file_perm_slot: FilePermSlot,
    stop_flag: StopFlag,
) {
    let (events_tx, mut events_rx) = tokio::sync::mpsc::unbounded_channel::<AgentEvent>();

    let runtime = LlmAgentRuntime::new_with_config(spec, ai_config, webview_tx, vault, hil_gate)
        .with_event_sink(events_tx)
        .with_file_perm_slot(file_perm_slot)
        .with_stop_flag(stop_flag)
        .with_agent_context(agent_context);

    // Prepend attached file context to the prompt when files are attached.
    let full_prompt = if context.is_empty() {
        prompt
    } else {
        format!("{}\n\nUSER REQUEST: {}", context, prompt)
    };

    // Pump events to the UI as they happen.
    // NOTE: AgentEvent::Done is handled here — do NOT send another Done after
    // runtime.run() returns on the Ok path; that would duplicate the final answer.
    // Swarm variants (SwarmUpdate, SwarmPlanReady, SwarmDone, SwarmError) are
    // forwarded directly; returning None skips the send for unhandled variants.
    let pump_tx = ui_tx.clone();
    let pump = tokio::spawn(async move {
        while let Some(event) = events_rx.recv().await {
            let action: Option<AgentSseAction> = match event {
                AgentEvent::Log(m) => Some(AgentSseAction::Log {
                    message: m,
                    class: "info".into(),
                }),
                AgentEvent::Step(m) => Some(AgentSseAction::Log {
                    message: m,
                    class: "step".into(),
                }),
                // Raw thinking text — no prefix/truncation; the UI renders it collapsibly.
                AgentEvent::Thinking(t) => Some(AgentSseAction::Log {
                    message: t,
                    class: "think".into(),
                }),
                AgentEvent::Navigated(u) => Some(AgentSseAction::UrlUpdate { url: u }),
                AgentEvent::Done(m) => Some(AgentSseAction::Done { message: m }),
                AgentEvent::Error(e) => Some(AgentSseAction::Log {
                    message: e,
                    class: "block".into(),
                }),
                AgentEvent::TokenUsage { input, output } => {
                    Some(AgentSseAction::TokenUsage { input, output })
                }
                AgentEvent::SwarmPlanReady { swarm_id, goal, tasks } => {
                    Some(AgentSseAction::SwarmPlanReady { swarm_id, goal, tasks })
                }
                AgentEvent::SwarmUpdate { swarm_id, worker_id, role, status, message, tool_calls_used } => {
                    Some(AgentSseAction::SwarmUpdate { swarm_id, worker_id, role, status, message, tool_calls_used })
                }
                AgentEvent::SwarmDone { swarm_id, final_answer, total_tool_calls } => {
                    Some(AgentSseAction::SwarmDone { swarm_id, final_answer, total_tool_calls })
                }
                AgentEvent::SwarmError { swarm_id, error } => {
                    Some(AgentSseAction::SwarmError { swarm_id, error })
                }
            };
            if let Some(a) = action {
                if pump_tx.send(a).is_err() {
                    break;
                }
            }
        }
    });

    let result = runtime.run(full_prompt).await;
    // Drop the runtime to close the event channel, allowing the pump to drain and exit.
    drop(runtime);
    let _ = pump.await;

    // On success the pump already delivered AgentEvent::Done to the UI.
    // Only send a Done here on the error path (errors propagate as Err without emitting Done).
    if let Err(e) = result {
        let _ = ui_tx.send(AgentSseAction::Log {
            message: format!("Agent error: {}", e),
            class: "block".into(),
        });
        let _ = ui_tx.send(AgentSseAction::Done {
            message: String::new(),
        });
    }
}

async fn run_swarm(
    spec: kitsune_agent::spec::AgentSpec,
    ai_config: kitsune_agent::ai_client::AiProviderConfig,
    ai_client: Arc<kitsune_agent::ai_client::AgentAiClient>,
    config: kitsune_agent::swarm::types::SwarmConfig,
    goal: String,
    vault: Arc<kitsune_vault::VaultBackend>,
    hil_gate: Arc<kitsune_hil::HilGate>,
    webview_tx: tokio::sync::mpsc::Sender<kitsune_agent::executor::WebViewCommand>,
    ui_tx: std::sync::mpsc::Sender<AgentSseAction>,
    stop_flag: kitsune_agent::StopFlag,
) {
    use kitsune_agent::swarm::coordinator::SwarmCoordinator;
    use kitsune_agent::AgentEvent;

    let (events_tx, mut events_rx) = tokio::sync::mpsc::unbounded_channel::<AgentEvent>();

    let coordinator = SwarmCoordinator {
        goal,
        config,
        spec,
        ai_client,
        ai_config,
        event_tx: events_tx.clone(),
        browser_tx: webview_tx,
        vault,
        hil_gate,
        stop_flag,
    };

    let pump_tx = ui_tx.clone();
    let pump = tokio::spawn(async move {
        while let Some(event) = events_rx.recv().await {
            let maybe_action: Option<AgentSseAction> = match event {
                AgentEvent::Log(m) => Some(AgentSseAction::Log { message: m, class: "info".into() }),
                AgentEvent::Step(m) => Some(AgentSseAction::Log { message: m, class: "step".into() }),
                AgentEvent::Thinking(t) => Some(AgentSseAction::Log { message: t, class: "think".into() }),
                AgentEvent::Navigated(u) => Some(AgentSseAction::UrlUpdate { url: u }),
                AgentEvent::Done(m) => Some(AgentSseAction::Log { message: m, class: "ok".into() }),
                AgentEvent::Error(e) => Some(AgentSseAction::Log { message: e, class: "block".into() }),
                AgentEvent::TokenUsage { input, output } => Some(AgentSseAction::TokenUsage { input, output }),
                AgentEvent::SwarmUpdate { swarm_id, worker_id, role, status, message, tool_calls_used } => {
                    Some(AgentSseAction::SwarmUpdate { swarm_id, worker_id, role, status, message, tool_calls_used })
                }
                AgentEvent::SwarmPlanReady { swarm_id, goal, tasks } => {
                    Some(AgentSseAction::SwarmPlanReady { swarm_id, goal, tasks })
                }
                AgentEvent::SwarmDone { swarm_id, final_answer, total_tool_calls } => {
                    Some(AgentSseAction::SwarmDone { swarm_id, final_answer, total_tool_calls })
                }
                AgentEvent::SwarmError { swarm_id, error } => {
                    Some(AgentSseAction::SwarmError { swarm_id, error })
                }
            };
            if let Some(action) = maybe_action {
                if pump_tx.send(action).is_err() {
                    break;
                }
            }
        }
    });

    let result = coordinator.run().await;
    drop(events_tx);
    let _ = pump.await;

    if let Err(e) = result {
        tracing::error!("Swarm coordinator error: {:?}", e);
        let _ = ui_tx.send(AgentSseAction::Done { message: String::new() });
    }
}

fn default_command_for_card(card_name: &str) -> String {
    match card_name {
        "PriceTracker" => "Find the cheapest option available. Search at least 2-3 websites, compare prices, and report the best deal with the URL.".to_string(),
        "FormFillAgent" => "Fill in the form on the current page using my attached document.".to_string(),
        "ResearchAgent" => "Research this topic thoroughly. Visit multiple authoritative sources, extract key facts, and write a structured summary.".to_string(),
        _ => String::new(),
    }
}

fn fmt_tokens(n: u32) -> String {
    if n == 0 {
        "--".into()
    } else if n < 1_000 {
        format!("{}", n)
    } else {
        format!("{:.1}k", n as f32 / 1_000.0)
    }
}

/// Estimate cost in USD based on model name patterns and cumulative token counts.
/// Returns "local" for Ollama/unknown models, or a "$X.XXXX" string for cloud models.
fn token_cost_display(model: &str, input: u32, output: u32) -> String {
    if input == 0 && output == 0 {
        return "--".into();
    }
    let m = model.to_lowercase();
    let (in_per_m, out_per_m): (f64, f64) = if m.contains("claude") {
        if m.contains("haiku") {
            (0.80, 4.0)
        } else if m.contains("opus") {
            (15.0, 75.0)
        } else {
            (3.0, 15.0) // sonnet default
        }
    } else if m.contains("gpt-4o-mini") {
        (0.15, 0.60)
    } else if m.contains("gpt-4o") {
        (2.50, 10.0)
    } else if m.contains("gemini-2.0-flash") || m.contains("gemini-flash") {
        (0.075, 0.30)
    } else if m.contains("gemini") {
        (1.25, 5.0)
    } else if m.contains("llama") || m.contains("mixtral") || m.contains("mistral") {
        (0.27, 0.27) // Groq blended approximate
    } else {
        return "local".into();
    };
    let cost = (input as f64 / 1_000_000.0 * in_per_m) + (output as f64 / 1_000_000.0 * out_per_m);
    if cost < 0.000_1 {
        "$0.0000".into()
    } else if cost < 0.01 {
        format!("${:.4}", cost)
    } else {
        format!("${:.3}", cost)
    }
}

fn specialist_context(card_name: &str) -> String {
    match card_name {
        "PriceTracker" => {
            "You are a price-tracking specialist. Your goal is to find the absolute cheapest \
             option across at least 2-3 different websites. Always navigate to comparison sites \
             or individual retailers, extract prices, and compare them before reporting. \
             Include the exact URL of the best deal in your final answer."
                .to_string()
        }
        "FormFillAgent" => {
            "You are a form-filling specialist. Your goal is to fill in web forms accurately \
             using the information from any attached documents. Read the attached file first, \
             then locate form fields on the page and fill them in. \
             Ask for HIL approval before submitting any form."
                .to_string()
        }
        "ResearchAgent" => {
            "You are a deep-research specialist. Your goal is to visit multiple authoritative \
             sources (at least 3), extract key facts, compare perspectives, and produce a \
             structured, comprehensive report. Do not stop after visiting a single page — \
             keep researching until you have enough depth to give a thorough answer."
                .to_string()
        }
        _ => String::new(),
    }
}
