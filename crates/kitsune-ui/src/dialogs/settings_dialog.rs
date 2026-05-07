use crate::app::{KitsuneBrowser, SettingsProvider};
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
            ui.set_width(420.0);

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

            // ── Provider toggle ──────────────────────────────────────────
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
                    SettingsProvider::OpenAiCompatible,
                    "OpenAI-compatible API",
                );
                ui.add_space(8.0);
                ui.radio_value(
                    &mut browser.settings_provider,
                    SettingsProvider::Ollama,
                    "Local LLM (Ollama)",
                );
                if browser.settings_provider != prev {
                    // Reset endpoint/model defaults when switching providers
                    match browser.settings_provider {
                        SettingsProvider::OpenAiCompatible => {
                            browser.settings_endpoint =
                                "https://api.openai.com/v1/chat/completions".to_string();
                            if browser.settings_model.is_empty() {
                                browser.settings_model = "gpt-4o-mini".to_string();
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

            ui.add_space(14.0);

            // ── Per-provider fields ──────────────────────────────────────
            egui::Grid::new("settings_grid")
                .num_columns(2)
                .spacing([12.0, 10.0])
                .show(ui, |ui| {
                    if browser.settings_provider == SettingsProvider::OpenAiCompatible {
                        ui.label(egui::RichText::new("API Key").color(KitsuneTheme::TEXT1));
                        ui.add(
                            egui::TextEdit::singleline(&mut browser.settings_api_key)
                                .password(true)
                                .desired_width(260.0)
                                .hint_text("sk-..."),
                        );
                        ui.end_row();
                    }

                    let endpoint_label = match browser.settings_provider {
                        SettingsProvider::OpenAiCompatible => "Endpoint",
                        SettingsProvider::Ollama => "Ollama URL",
                    };
                    ui.label(egui::RichText::new(endpoint_label).color(KitsuneTheme::TEXT1));
                    let endpoint_hint = match browser.settings_provider {
                        SettingsProvider::OpenAiCompatible => {
                            "https://api.openai.com/v1/chat/completions"
                        }
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
                        SettingsProvider::OpenAiCompatible => "gpt-4o-mini",
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
                        "Works with OpenAI, Groq, Together, OpenRouter, or any other provider \
                         that exposes an OpenAI /v1/chat/completions endpoint.",
                    )
                    .size(11.0)
                    .color(KitsuneTheme::TEXT2),
                );
            }

            ui.add_space(14.0);

            if let Some(status) = &browser.settings_test_status {
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
                        save_settings(browser);
                        browser.show_settings = false;
                        browser.settings_saved = true;
                    }

                    ui.add_space(6.0);

                    let test_btn = egui::Button::new(
                        egui::RichText::new("Test")
                            .color(KitsuneTheme::TEXT_PRIMARY),
                    )
                    .fill(KitsuneTheme::BG3)
                    .min_size(egui::vec2(70.0, 28.0));
                    if ui.add(test_btn).clicked() {
                        save_settings(browser);
                        browser.settings_test_status =
                            Some("Testing… run a query to verify the LLM responds.".to_string());
                    }
                });
            });
        });

    browser.show_settings = is_open;
}

fn save_settings(browser: &mut KitsuneBrowser) {
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
