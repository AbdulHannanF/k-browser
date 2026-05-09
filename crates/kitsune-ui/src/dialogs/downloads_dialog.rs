use crate::app::{DownloadStatus, KitsuneBrowser};
use crate::theme::KitsuneTheme;
use eframe::egui;

pub fn downloads_dialog(ctx: &egui::Context, browser: &mut KitsuneBrowser) {
    if !browser.show_downloads {
        return;
    }

    let mut open = browser.show_downloads;
    egui::Window::new("⬇ Downloads")
        .open(&mut open)
        .collapsible(false)
        .resizable(true)
        .default_width(420.0)
        .anchor(egui::Align2::RIGHT_BOTTOM, [-12.0, -12.0])
        .frame(
            egui::Frame::window(&ctx.style())
                .fill(KitsuneTheme::BG_PANEL)
                .stroke(egui::Stroke::new(1.0, KitsuneTheme::BORDER)),
        )
        .show(ctx, |ui| {
            if browser.downloads.is_empty() {
                ui.label(
                    egui::RichText::new("No downloads yet.")
                        .color(KitsuneTheme::TEXT3)
                        .size(11.0),
                );
                return;
            }

            let mut clear_completed = false;

            egui::ScrollArea::vertical()
                .max_height(320.0)
                .show(ui, |ui| {
                    for item in &browser.downloads {
                        ui.add_space(2.0);
                        egui::Frame::none()
                            .fill(KitsuneTheme::BG2)
                            .rounding(egui::Rounding::same(6.0))
                            .inner_margin(egui::Margin::symmetric(10.0, 7.0))
                            .show(ui, |ui| {
                                ui.set_min_width(ui.available_width());
                                ui.horizontal(|ui| {
                                    // Status icon
                                    let (icon, col) = match item.status {
                                        DownloadStatus::InProgress => ("🔄", KitsuneTheme::AMBER),
                                        DownloadStatus::Completed => ("✓", KitsuneTheme::GREEN),
                                        DownloadStatus::Failed => ("✗", KitsuneTheme::RED),
                                    };
                                    ui.label(
                                        egui::RichText::new(icon).size(13.0).color(col).strong(),
                                    );
                                    ui.add_space(6.0);

                                    ui.vertical(|ui| {
                                        // Filename
                                        ui.label(
                                            egui::RichText::new(&item.filename)
                                                .size(11.5)
                                                .color(KitsuneTheme::TEXT_PRIMARY)
                                                .strong(),
                                        );
                                        // Save path (muted)
                                        if let Some(path) = &item.save_path {
                                            ui.label(
                                                egui::RichText::new(path)
                                                    .size(10.0)
                                                    .color(KitsuneTheme::TEXT3)
                                                    .family(egui::FontFamily::Monospace),
                                            );
                                        }
                                        // Status text
                                        let status_txt = match item.status {
                                            DownloadStatus::InProgress => "Downloading…",
                                            DownloadStatus::Completed => "Complete",
                                            DownloadStatus::Failed => "Failed",
                                        };
                                        ui.label(
                                            egui::RichText::new(status_txt)
                                                .size(10.0)
                                                .color(match item.status {
                                                    DownloadStatus::InProgress => {
                                                        KitsuneTheme::AMBER
                                                    }
                                                    DownloadStatus::Completed => {
                                                        KitsuneTheme::GREEN
                                                    }
                                                    DownloadStatus::Failed => KitsuneTheme::RED,
                                                }),
                                        );
                                    });

                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            if item.status == DownloadStatus::Completed {
                                                if let Some(path) = &item.save_path {
                                                    let open_btn = egui::Button::new(
                                                        egui::RichText::new("Open")
                                                            .size(10.0)
                                                            .color(KitsuneTheme::TEXT_PRIMARY),
                                                    )
                                                    .fill(KitsuneTheme::BG4)
                                                    .min_size(egui::vec2(44.0, 22.0));
                                                    if ui.add(open_btn).on_hover_text("Show in Explorer").clicked() {
                                                        reveal_in_explorer(path);
                                                    }
                                                }
                                            }
                                        },
                                    );
                                });
                            });
                        ui.add_space(2.0);
                    }
                });

            ui.add_space(6.0);
            ui.horizontal(|ui| {
                let has_completed = browser
                    .downloads
                    .iter()
                    .any(|d| d.status != DownloadStatus::InProgress);
                if has_completed {
                    let clear_btn = egui::Button::new(
                        egui::RichText::new("Clear completed")
                            .size(10.0)
                            .color(KitsuneTheme::TEXT2),
                    )
                    .frame(false);
                    if ui.add(clear_btn).clicked() {
                        clear_completed = true;
                    }
                }

                let in_progress = browser
                    .downloads
                    .iter()
                    .filter(|d| d.status == DownloadStatus::InProgress)
                    .count();
                if in_progress > 0 {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            egui::RichText::new(format!("{} in progress", in_progress))
                                .size(10.0)
                                .color(KitsuneTheme::AMBER),
                        );
                    });
                }
            });

            if clear_completed {
                browser.downloads.retain(|d| d.status == DownloadStatus::InProgress);
            }
        });

    browser.show_downloads = open;
}

/// Open Explorer with the file selected (Windows).
fn reveal_in_explorer(path: &str) {
    #[cfg(target_os = "windows")]
    {
        let _ = std::process::Command::new("explorer.exe")
            .arg(format!("/select,{}", path))
            .spawn();
    }
    #[cfg(not(target_os = "windows"))]
    {
        // On non-Windows, try xdg-open on the parent directory.
        if let Some(parent) = std::path::Path::new(path).parent() {
            let _ = std::process::Command::new("xdg-open").arg(parent).spawn();
        }
    }
}
