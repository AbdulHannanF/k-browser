use crate::app::{AgentRunState, AgentSseAction, AttachedFile, KitsuneBrowser, LogLevel};
use crate::panels::agent_card::{AgentCard, AgentStatus};
use crate::theme::KitsuneTheme;
use eframe::egui;
use kitsune_agent::spec::{
    AgentAuthor, AgentBudget, AgentConstraints, AgentGoal, AgentId, AgentSpec, AgentTool,
    DomainPolicy,
};
use kitsune_agent::{AgentEvent, FilePermSlot, LlmAgentRuntime, StopFlag};
use std::sync::atomic::Ordering;
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
                        if card.render(ui) && !is_busy {
                            browser.agent_command = match card.name {
                                "PriceTracker" => {
                                    "Find the cheapest option available. Search at least 2-3 websites, compare prices, and report the best deal with the URL.".to_string()
                                }
                                "FormFillAgent" => {
                                    if browser.attached_files.is_empty() {
                                        "Fill in the form on the current page. Attach a document first (📎) if you need me to use your information.".to_string()
                                    } else {
                                        format!(
                                            "Use the attached document '{}' to fill in the form or application on the current page.",
                                            browser.attached_files[0].name
                                        )
                                    }
                                }
                                "ResearchAgent" => {
                                    "Research this topic thoroughly. Visit multiple authoritative sources, extract key facts, and write a structured summary with your findings.".to_string()
                                }
                                _ => String::new(),
                            };
                        }
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
                                let is_think = matches!(entry.level, LogLevel::Think);
                                let txt = egui::RichText::new(&entry.text)
                                    .size(if is_think { 10.5 } else { 11.0 })
                                    .color(entry.color())
                                    .family(egui::FontFamily::Monospace);
                                let txt = if is_think { txt.italics() } else { txt };
                                ui.label(
                                    txt,
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
                    let bar_col = if frac > 0.8 {
                        KitsuneTheme::RED
                    } else if frac > 0.4 {
                        KitsuneTheme::AMBER
                    } else {
                        KitsuneTheme::GREEN
                    };
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

// ── In-process agent execution (Ollama-driven) ────────────────────────────────

fn start_agent_run(browser: &mut KitsuneBrowser) {
    browser.agent_state = AgentRunState::Running;
    browser.agent_log.clear();
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

    // Build context from any attached files.
    let context = if browser.attached_files.is_empty() {
        String::new()
    } else {
        browser
            .attached_files
            .iter()
            .map(|f| format!("=== ATTACHED: {} ===\n{}\n=== END ===", f.name, f.content))
            .collect::<Vec<_>>()
            .join("\n\n")
    };

    browser.runtime().spawn(async move {
        run_in_process_agent(spec, cmd, context, vault, hil_gate, webview_tx, agent_tx, file_perm_slot, stop_flag).await;
    });
}

fn build_runtime_spec(browser: &KitsuneBrowser) -> AgentSpec {
    let now = chrono::Utc::now();

    // Settings panel can override the Ollama endpoint and model. We treat
    // the Ollama provider as the only in-process backend for now.
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
    prompt: String,
    context: String,
    vault: std::sync::Arc<kitsune_vault::VaultBackend>,
    hil_gate: std::sync::Arc<kitsune_hil::HilGate>,
    webview_tx: tokio::sync::mpsc::Sender<kitsune_agent::executor::WebViewCommand>,
    ui_tx: Sender<AgentSseAction>,
    file_perm_slot: FilePermSlot,
    stop_flag: StopFlag,
) {
    let (events_tx, mut events_rx) = tokio::sync::mpsc::unbounded_channel::<AgentEvent>();

    let runtime = LlmAgentRuntime::new(spec, webview_tx, vault, hil_gate)
        .with_event_sink(events_tx)
        .with_file_perm_slot(file_perm_slot)
        .with_stop_flag(stop_flag);

    // Prepend attached file context to the prompt when files are attached.
    let full_prompt = if context.is_empty() {
        prompt
    } else {
        format!("{}\n\nUSER REQUEST: {}", context, prompt)
    };

    // Pump events to the UI as they happen.
    // NOTE: AgentEvent::Done is handled here — do NOT send another Done after
    // runtime.run() returns on the Ok path; that would duplicate the final answer.
    let pump_tx = ui_tx.clone();
    let pump = tokio::spawn(async move {
        while let Some(event) = events_rx.recv().await {
            let action = match event {
                AgentEvent::Log(m) => AgentSseAction::Log {
                    message: m,
                    class: "info".into(),
                },
                AgentEvent::Thinking(t) => AgentSseAction::Log {
                    message: format!("💭 {}", truncate_str(&t, 500)),
                    class: "think".into(),
                },
                AgentEvent::Navigated(u) => AgentSseAction::UrlUpdate { url: u },
                AgentEvent::Done(m) => AgentSseAction::Done { message: m },
                AgentEvent::Error(e) => AgentSseAction::Log {
                    message: e,
                    class: "block".into(),
                },
            };
            if pump_tx.send(action).is_err() {
                break;
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

fn truncate_str(s: &str, n: usize) -> String {
    let count = s.chars().count();
    if count <= n {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(n).collect();
        out.push('…');
        out
    }
}
