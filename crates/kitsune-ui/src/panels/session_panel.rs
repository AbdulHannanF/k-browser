use crate::app::KitsuneBrowser;
use crate::panels::profile_panel::profile_panel;
use crate::panels::task_graph_panel::task_graph_panel;
use crate::theme::KitsuneTheme;
use eframe::egui;

pub fn session_panel(ctx: &egui::Context, browser: &KitsuneBrowser) {
    egui::SidePanel::right("session_panel")
        .resizable(true)
        .default_width(190.0)
        .min_width(160.0)
        .max_width(280.0)
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
                    ui.label(
                        egui::RichText::new("SESSION")
                            .size(10.0)
                            .strong()
                            .color(KitsuneTheme::TEXT2)
                            .family(egui::FontFamily::Monospace),
                    );
                });
            paint_separator(ui);

            // ── Status grid ──────────────────────────────────────────────
            egui::Frame::none()
                .inner_margin(egui::Margin::symmetric(12.0, 6.0))
                .show(ui, |ui| {
                    let n = browser.privacy.trackers_blocked;
                    let tls = browser.privacy.tls_version;
                    stat_row(ui, "status", "● Active", KitsuneTheme::GREEN_SAFE);
                    stat_row(ui, "mode", "Agent-First", KitsuneTheme::TEXT_PRIMARY);
                    stat_row(ui, "tls", tls, KitsuneTheme::GREEN_SAFE);
                    stat_row(
                        ui,
                        "trackers",
                        &format!("{n} blocked"),
                        if n > 0 {
                            KitsuneTheme::GREEN_SAFE
                        } else {
                            KitsuneTheme::TEXT3
                        },
                    );
                    stat_row(ui, "referer", "stripped", KitsuneTheme::GREEN_SAFE);
                    stat_row(ui, "fingerprint", "hardened", KitsuneTheme::GREEN_SAFE);
                    stat_row(ui, "hil gate", "armed", KitsuneTheme::AMBER);
                });
            paint_separator(ui);

            // ── Capabilities ──────────────────────────────────────────────
            egui::Frame::none()
                .inner_margin(egui::Margin::symmetric(12.0, 6.0))
                .show(ui, |ui| {
                    ui.label(
                        egui::RichText::new("CAPABILITIES")
                            .size(10.0)
                            .strong()
                            .color(KitsuneTheme::TEXT2)
                            .family(egui::FontFamily::Monospace),
                    );
                    ui.add_space(4.0);
                    cap_row(ui, "🧠", "DOM Control", true);
                    cap_row(ui, "🔐", "Vault Access", true);
                    cap_row(ui, "📋", "Audit Log", true);
                    cap_row(ui, "🏖", "Sandbox", true);
                    cap_row(ui, "🌐", "Network", true);
                    cap_row(ui, "📸", "Screenshot", false);
                });
            paint_separator(ui);

            // ── Vault ─────────────────────────────────────────────────────
            egui::Frame::none()
                .inner_margin(egui::Margin::symmetric(12.0, 6.0))
                .show(ui, |ui| {
                    ui.label(
                        egui::RichText::new("🔐 VAULT")
                            .size(10.0)
                            .strong()
                            .color(KitsuneTheme::TEXT2)
                            .family(egui::FontFamily::Monospace),
                    );
                    ui.add_space(4.0);
                    vault_item(ui, "👤", "demo@kitsune.ai", "token");
                    vault_item(ui, "💳", "•••• 4242", "locked");
                    vault_item(ui, "🏠", "Home address", "token");
                });

            paint_separator(ui);

            // ── Profile ───────────────────────────────────────────────────
            egui::Frame::none()
                .inner_margin(egui::Margin::symmetric(12.0, 6.0))
                .show(ui, |ui| {
                    egui::CollapsingHeader::new(
                        egui::RichText::new("PROFILE")
                            .size(10.0)
                            .strong()
                            .color(KitsuneTheme::TEXT2)
                            .family(egui::FontFamily::Monospace),
                    )
                    .default_open(false)
                    .show(ui, |ui| {
                        profile_panel(ui, browser.profile_summary.as_ref());
                    });
                });

            paint_separator(ui);

            // ── Task Graph ────────────────────────────────────────────────
            egui::Frame::none()
                .inner_margin(egui::Margin::symmetric(12.0, 6.0))
                .show(ui, |ui| {
                    egui::CollapsingHeader::new(
                        egui::RichText::new("TASK GRAPH")
                            .size(10.0)
                            .strong()
                            .color(KitsuneTheme::TEXT2)
                            .family(egui::FontFamily::Monospace),
                    )
                    .default_open(false)
                    .show(ui, |ui| {
                        task_graph_panel(ui, &browser.task_nodes);
                    });
                });
        });
}

fn paint_separator(ui: &mut egui::Ui) {
    let rect = ui.available_rect_before_wrap();
    let sep = egui::Rect::from_min_size(rect.left_top(), egui::vec2(rect.width(), 1.0));
    ui.painter().rect_filled(sep, 0.0, KitsuneTheme::BORDER);
    ui.allocate_exact_size(egui::vec2(0.0, 1.0), egui::Sense::hover());
}

fn stat_row(ui: &mut egui::Ui, label: &str, value: &str, color: egui::Color32) {
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(label)
                .size(10.0)
                .color(KitsuneTheme::TEXT3)
                .family(egui::FontFamily::Monospace),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(
                egui::RichText::new(value)
                    .size(10.0)
                    .color(color)
                    .family(egui::FontFamily::Monospace),
            );
        });
    });
}

fn cap_row(ui: &mut egui::Ui, icon: &str, name: &str, on: bool) {
    let (col, txt) = if on {
        (KitsuneTheme::GREEN_SAFE, "ON")
    } else {
        (KitsuneTheme::RED_BLOCKED, "OFF")
    };
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(icon).size(11.0));
        ui.add_space(2.0);
        ui.label(
            egui::RichText::new(name)
                .size(10.0)
                .color(KitsuneTheme::TEXT_MUTED),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            egui::Frame::none()
                .fill(egui::Color32::from_rgba_unmultiplied(
                    col.r(),
                    col.g(),
                    col.b(),
                    25,
                ))
                .rounding(egui::Rounding::same(3.0))
                .inner_margin(egui::Margin::symmetric(4.0, 1.0))
                .show(ui, |ui| {
                    ui.label(
                        egui::RichText::new(txt)
                            .size(9.0)
                            .color(col)
                            .family(egui::FontFamily::Monospace),
                    );
                });
        });
    });
}

fn vault_item(ui: &mut egui::Ui, icon: &str, name: &str, status: &str) {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(icon).size(11.0));
        ui.add_space(2.0);
        ui.label(
            egui::RichText::new(name)
                .size(10.0)
                .color(KitsuneTheme::TEXT_MUTED),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(
                egui::RichText::new(status)
                    .size(9.0)
                    .color(KitsuneTheme::TEXT3)
                    .family(egui::FontFamily::Monospace),
            );
        });
    });
}
