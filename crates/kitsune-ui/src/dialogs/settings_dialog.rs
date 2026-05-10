use crate::app::{CloudPreset, KitsuneBrowser, SettingsProvider, SettingsTab};
use crate::theme::KitsuneTheme;
use eframe::egui;

pub fn settings_dialog(ctx: &egui::Context, browser: &mut KitsuneBrowser) {
    if !browser.show_settings {
        return;
    }

    let mut is_open = browser.show_settings;

    egui::Window::new("⚙ Settings")
        .open(&mut is_open)
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .frame(
            egui::Frame::window(&ctx.style())
                .fill(KitsuneTheme::BG1)
                .inner_margin(16.0)
                .rounding(8.0)
                .stroke(egui::Stroke::new(1.0, KitsuneTheme::BORDER)),
        )
        .show(ctx, |ui| {
            ui.set_width(460.0);

            // ── Tab selector ─────────────────────────────────────────────
            ui.horizontal(|ui| {
                tab_btn(ui, &mut browser.settings_tab, SettingsTab::Llm, "LLM");
                tab_btn(ui, &mut browser.settings_tab, SettingsTab::Profile, "Profile");
                tab_btn(ui, &mut browser.settings_tab, SettingsTab::Agents, "Agents");
            });
            ui.add_space(10.0);
            ui.separator();
            ui.add_space(8.0);

            // ── Tab content ──────────────────────────────────────────────
            match browser.settings_tab {
                SettingsTab::Llm => render_llm_tab(ui, browser),
                SettingsTab::Profile => render_profile_tab(ui, browser),
                SettingsTab::Agents => render_agents_tab(ui, browser),
            }
        });

    browser.show_settings = is_open;
}

// ─── Tab button helper ───────────────────────────────────────────────────────

fn tab_btn(ui: &mut egui::Ui, current: &mut SettingsTab, target: SettingsTab, label: &str) {
    let active = *current == target;
    let fill = if active { KitsuneTheme::AMBER } else { KitsuneTheme::BG3 };
    let text_color = if active {
        egui::Color32::BLACK
    } else {
        KitsuneTheme::TEXT1
    };
    let btn = egui::Button::new(egui::RichText::new(label).color(text_color).strong())
        .fill(fill)
        .min_size(egui::vec2(80.0, 26.0));
    if ui.add(btn).clicked() {
        *current = target;
    }
}

// ─── LLM tab (existing content) ─────────────────────────────────────────────

fn render_llm_tab(ui: &mut egui::Ui, browser: &mut KitsuneBrowser) {
    ui.heading(
        egui::RichText::new("Agent LLM Configuration")
            .color(KitsuneTheme::TEXT_PRIMARY)
            .size(16.0),
    );
    ui.add_space(8.0);

    ui.label(
        egui::RichText::new(
            "Choose how the agent thinks. The browser ships with a built-in offline planner — \
             configuring an LLM upgrades it.",
        )
        .color(KitsuneTheme::TEXT2)
        .size(12.0),
    );
    ui.add_space(14.0);

    // ── Provider toggle ──────────────────────────────────────────────────
    ui.label(
        egui::RichText::new("PROVIDER")
            .size(10.0)
            .strong()
            .color(KitsuneTheme::TEXT2)
            .family(egui::FontFamily::Monospace),
    );
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        let prev = browser.settings_provider;
        ui.radio_value(
            &mut browser.settings_provider,
            SettingsProvider::Cloud,
            "Cloud API",
        );
        ui.add_space(8.0);
        ui.radio_value(
            &mut browser.settings_provider,
            SettingsProvider::Ollama,
            "Local LLM (Ollama)",
        );
        if browser.settings_provider != prev {
            match browser.settings_provider {
                SettingsProvider::Cloud => {
                    let preset = browser.settings_cloud_preset;
                    if preset != CloudPreset::Custom {
                        browser.settings_endpoint = preset.default_endpoint().to_string();
                        browser.settings_model = preset.default_model().to_string();
                    }
                }
                SettingsProvider::Ollama => {
                    browser.settings_endpoint = "http://localhost:11434".to_string();
                    browser.settings_model = "llama3.2".to_string();
                }
            }
            browser.settings_saved = false;
            browser.settings_test_status = None;
        }
    });

    // ── Preset picker (Cloud only) ────────────────────────────────────────
    if browser.settings_provider == SettingsProvider::Cloud {
        ui.add_space(8.0);
        ui.label(
            egui::RichText::new("PRESET")
                .size(10.0)
                .strong()
                .color(KitsuneTheme::TEXT2)
                .family(egui::FontFamily::Monospace),
        );
        ui.add_space(4.0);
        ui.with_layout(egui::Layout::left_to_right(egui::Align::Center).with_main_wrap(false), |ui| {
            for preset in [
                CloudPreset::Claude,
                CloudPreset::OpenAI,
                CloudPreset::Gemini,
                CloudPreset::Groq,
                CloudPreset::OpenRouter,
                CloudPreset::Custom,
            ] {
                let active = browser.settings_cloud_preset == preset;
                let fill = if active { KitsuneTheme::AMBER } else { KitsuneTheme::BG3 };
                let text_color = if active { egui::Color32::BLACK } else { KitsuneTheme::TEXT1 };
                let btn = egui::Button::new(
                    egui::RichText::new(preset.label()).color(text_color).size(10.0).strong(),
                )
                .fill(fill)
                .min_size(egui::vec2(56.0, 22.0));
                if ui.add(btn).clicked() && browser.settings_cloud_preset != preset {
                    browser.settings_cloud_preset = preset;
                    if preset != CloudPreset::Custom {
                        browser.settings_endpoint = preset.default_endpoint().to_string();
                        browser.settings_model = preset.default_model().to_string();
                    }
                    browser.settings_saved = false;
                    browser.settings_test_status = None;
                }
                ui.add_space(2.0);
            }
        });
    }

    ui.add_space(14.0);

    // ── Per-provider fields ──────────────────────────────────────────────
    egui::Grid::new("settings_grid")
        .num_columns(2)
        .spacing([12.0, 10.0])
        .show(ui, |ui| {
            if browser.settings_provider == SettingsProvider::Cloud {
                ui.label(egui::RichText::new("API Key").color(KitsuneTheme::TEXT1));
                ui.add(
                    egui::TextEdit::singleline(&mut browser.settings_api_key)
                        .password(true)
                        .desired_width(260.0)
                        .hint_text(browser.settings_cloud_preset.key_hint()),
                );
                ui.end_row();
            }

            let endpoint_label = match browser.settings_provider {
                SettingsProvider::Cloud => "Endpoint",
                SettingsProvider::Ollama => "Ollama URL",
            };
            ui.label(egui::RichText::new(endpoint_label).color(KitsuneTheme::TEXT1));
            let endpoint_hint = match browser.settings_provider {
                SettingsProvider::Cloud => browser.settings_cloud_preset.default_endpoint(),
                SettingsProvider::Ollama => "http://localhost:11434",
            };
            ui.add(
                egui::TextEdit::singleline(&mut browser.settings_endpoint)
                    .desired_width(260.0)
                    .hint_text(endpoint_hint),
            );
            ui.end_row();

            ui.label(egui::RichText::new("Model").color(KitsuneTheme::TEXT1));
            let model_hint = match browser.settings_provider {
                SettingsProvider::Cloud => browser.settings_cloud_preset.default_model(),
                SettingsProvider::Ollama => "llama3.2",
            };
            ui.add(
                egui::TextEdit::singleline(&mut browser.settings_model)
                    .desired_width(260.0)
                    .hint_text(model_hint),
            );
            ui.end_row();
        });

    ui.add_space(8.0);

    if browser.settings_provider == SettingsProvider::Ollama {
        ui.label(
            egui::RichText::new(
                "Ollama runs entirely on your machine — no data leaves the device. \
                 Make sure `ollama serve` is running and the model is pulled \
                 (e.g. `ollama pull llama3.2`).",
            )
            .size(11.0)
            .color(KitsuneTheme::TEXT2),
        );
    } else {
        ui.label(
            egui::RichText::new(
                "Works with Claude (Anthropic), OpenAI, Gemini (Google), Groq, OpenRouter, \
                 or any provider with an OpenAI-compatible /v1/chat/completions endpoint. \
                 Select a preset above to auto-fill the URL.",
            )
            .size(11.0)
            .color(KitsuneTheme::TEXT2),
        );
    }

    ui.add_space(14.0);

    if let Some(status) = &browser.settings_test_status.clone() {
        ui.label(
            egui::RichText::new(status)
                .size(11.0)
                .color(KitsuneTheme::TEXT1)
                .family(egui::FontFamily::Monospace),
        );
        ui.add_space(8.0);
    }

    ui.horizontal(|ui| {
        if browser.settings_saved {
            ui.label(
                egui::RichText::new("✓ Saved")
                    .color(KitsuneTheme::GREEN_SAFE)
                    .strong(),
            );
        }

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let save_btn = egui::Button::new(
                egui::RichText::new("Save & Close")
                    .color(egui::Color32::BLACK)
                    .strong(),
            )
            .fill(KitsuneTheme::AMBER)
            .min_size(egui::vec2(110.0, 28.0));

            if ui.add(save_btn).clicked() {
                save_llm_settings(browser);
                browser.show_settings = false;
                browser.settings_saved = true;
            }

            ui.add_space(6.0);

            let test_btn = egui::Button::new(
                egui::RichText::new("Test").color(KitsuneTheme::TEXT_PRIMARY),
            )
            .fill(KitsuneTheme::BG3)
            .min_size(egui::vec2(70.0, 28.0));
            if ui.add(test_btn).clicked() {
                save_llm_settings(browser);
                browser.settings_test_status =
                    Some("Testing… run a query to verify the LLM responds.".to_string());
            }
        });
    });
}

// ─── Profile tab ─────────────────────────────────────────────────────────────

fn render_profile_tab(ui: &mut egui::Ui, browser: &mut KitsuneBrowser) {
    ui.heading(
        egui::RichText::new("Document Profile")
            .color(KitsuneTheme::TEXT_PRIMARY)
            .size(16.0),
    );
    ui.add_space(8.0);

    ui.label(
        egui::RichText::new(
            "Folder containing your CV, transcripts, and supporting documents:",
        )
        .color(KitsuneTheme::TEXT1)
        .size(12.0),
    );
    ui.add_space(6.0);

    ui.horizontal(|ui| {
        ui.add(
            egui::TextEdit::singleline(&mut browser.profile_folder)
                .desired_width(340.0)
                .hint_text("e.g. C:\\Users\\you\\Documents\\profile"),
        );
    });

    ui.add_space(4.0);
    ui.label(
        egui::RichText::new(
            "Supported formats: PDF, DOCX, TXT. Indexing runs locally — no data leaves the device.",
        )
        .color(KitsuneTheme::TEXT2)
        .size(11.0),
    );

    ui.add_space(12.0);

    let reindex_btn = egui::Button::new(
        egui::RichText::new("Re-index Now")
            .color(egui::Color32::BLACK)
            .strong(),
    )
    .fill(KitsuneTheme::AMBER)
    .min_size(egui::vec2(120.0, 28.0));

    if ui.add(reindex_btn).clicked() {
        browser.reindex_requested = true;
    }

    if browser.reindex_requested {
        ui.add_space(6.0);
        ui.label(
            egui::RichText::new("Re-index queued — will run on next agent invocation.")
                .color(KitsuneTheme::GREEN_SAFE)
                .size(11.0),
        );
    }
}

// ─── Agents tab ──────────────────────────────────────────────────────────────

fn render_agents_tab(ui: &mut egui::Ui, browser: &mut KitsuneBrowser) {
    ui.heading(
        egui::RichText::new("Agent Configuration")
            .color(KitsuneTheme::TEXT_PRIMARY)
            .size(16.0),
    );
    ui.add_space(8.0);

    ui.label(
        egui::RichText::new("Model names or provider IDs:")
            .color(KitsuneTheme::TEXT1)
            .size(12.0),
    );
    ui.add_space(6.0);

    egui::Grid::new("model_slots_grid")
        .num_columns(2)
        .spacing([8.0, 6.0])
        .show(ui, |ui| {
            ui.label(egui::RichText::new("Orchestrator").color(KitsuneTheme::TEXT1));
            ui.add(
                egui::TextEdit::singleline(&mut browser.orchestrator_model)
                    .desired_width(240.0)
                    .hint_text("e.g. llama3.2 or gpt-4o"),
            );
            ui.end_row();

            ui.label(egui::RichText::new("Worker").color(KitsuneTheme::TEXT1));
            ui.add(
                egui::TextEdit::singleline(&mut browser.worker_model)
                    .desired_width(240.0)
                    .hint_text("e.g. llama3.2"),
            );
            ui.end_row();

            ui.label(egui::RichText::new("Fast").color(KitsuneTheme::TEXT1));
            ui.add(
                egui::TextEdit::singleline(&mut browser.fast_model)
                    .desired_width(240.0)
                    .hint_text("e.g. llama3.2:1b"),
            );
            ui.end_row();
        });

    ui.add_space(12.0);
    ui.separator();
    ui.add_space(8.0);

    ui.label(
        egui::RichText::new("CAPTCHA API Solver (optional — Tier 3 bypass):")
            .color(KitsuneTheme::TEXT1)
            .size(12.0),
    );
    ui.add_space(6.0);

    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Endpoint:").color(KitsuneTheme::TEXT2));
        ui.add(
            egui::TextEdit::singleline(&mut browser.captcha_solver_url)
                .desired_width(280.0)
                .hint_text("https://api.2captcha.com"),
        );
    });

    ui.add_space(4.0);

    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("API Key:  ").color(KitsuneTheme::TEXT2));
        ui.add(
            egui::TextEdit::singleline(&mut browser.captcha_solver_key)
                .password(true)
                .desired_width(280.0)
                .hint_text("••••••••"),
        );
    });

    ui.add_space(8.0);

    let save_key_btn = egui::Button::new(
        egui::RichText::new("Save API Key to Vault")
            .color(egui::Color32::BLACK)
            .strong(),
    )
    .fill(KitsuneTheme::AMBER)
    .min_size(egui::vec2(160.0, 28.0));

    if ui.add(save_key_btn).clicked() {
        browser.save_captcha_key_requested = true;
    }

    ui.add_space(4.0);
    ui.label(
        egui::RichText::new("Key is stored encrypted; never shown again.")
            .color(KitsuneTheme::TEXT2)
            .size(11.0),
    );

    if browser.save_captcha_key_requested {
        ui.add_space(4.0);
        ui.label(
            egui::RichText::new("✓ Key queued for vault storage.")
                .color(KitsuneTheme::GREEN_SAFE)
                .size(11.0),
        );
    }
}

// ─── Persist LLM settings to the mock server ─────────────────────────────────

fn save_llm_settings(browser: &mut KitsuneBrowser) {
    let provider = browser.settings_provider.wire_value();
    let key = browser.settings_api_key.clone();
    let endpoint = browser.settings_endpoint.clone();
    let model = browser.settings_model.clone();

    browser.runtime().spawn(async move {
        let client = reqwest::Client::new();
        let _ = client
            .post("http://127.0.0.1:7700/api/settings")
            .json(&serde_json::json!({
                "provider": provider,
                "api_key": key,
                "endpoint": endpoint,
                "model": model,
            }))
            .send()
            .await;
    });
}
