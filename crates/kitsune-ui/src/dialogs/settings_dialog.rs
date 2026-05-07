use crate::app::KitsuneBrowser;
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
            ui.set_width(350.0);

            ui.heading(
                egui::RichText::new("Agent API Configuration")
                    .color(KitsuneTheme::TEXT_PRIMARY)
                    .size(16.0),
            );
            ui.add_space(12.0);

            ui.label(
                egui::RichText::new("Configure the LLM backend used for agent execution.")
                    .color(KitsuneTheme::TEXT2)
                    .size(12.0),
            );
            ui.add_space(16.0);

            egui::Grid::new("settings_grid")
                .num_columns(2)
                .spacing([12.0, 12.0])
                .show(ui, |ui| {
                    ui.label(egui::RichText::new("API Key").color(KitsuneTheme::TEXT1));
                    ui.add(
                        egui::TextEdit::singleline(&mut browser.settings_api_key)
                            .password(true)
                            .hint_text("sk-..."),
                    );
                    ui.end_row();

                    ui.label(egui::RichText::new("Endpoint").color(KitsuneTheme::TEXT1));
                    ui.add(
                        egui::TextEdit::singleline(&mut browser.settings_endpoint)
                            .hint_text("https://api.openai.com/v1/chat/completions"),
                    );
                    ui.end_row();

                    ui.label(egui::RichText::new("Model").color(KitsuneTheme::TEXT1));
                    ui.add(
                        egui::TextEdit::singleline(&mut browser.settings_model)
                            .hint_text("gpt-4o-mini"),
                    );
                    ui.end_row();
                });

            ui.add_space(24.0);

            ui.horizontal(|ui| {
                if browser.settings_saved {
                    ui.label(
                        egui::RichText::new("✓ Saved")
                            .color(KitsuneTheme::GREEN_SAFE)
                            .strong(),
                    );
                } else {
                    ui.label("");
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let save_btn = egui::Button::new(
                        egui::RichText::new("Save & Close")
                            .color(egui::Color32::BLACK)
                            .strong(),
                    )
                    .fill(KitsuneTheme::AMBER)
                    .min_size(egui::vec2(100.0, 28.0));

                    if ui.add(save_btn).clicked() {
                        let key = browser.settings_api_key.clone();
                        let endpoint = browser.settings_endpoint.clone();
                        let model = browser.settings_model.clone();

                        // Fire and forget save
                        browser.runtime().spawn(async move {
                            let client = reqwest::Client::new();
                            let _ = client
                                .post("http://127.0.0.1:7700/api/settings")
                                .json(&serde_json::json!({
                                    "api_key": key,
                                    "endpoint": endpoint,
                                    "model": model,
                                }))
                                .send()
                                .await;
                        });

                        browser.show_settings = false;
                        browser.settings_saved = true;
                    }
                });
            });
        });

    browser.show_settings = is_open;
}
