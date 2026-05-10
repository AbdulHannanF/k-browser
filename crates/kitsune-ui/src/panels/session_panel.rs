use crate::animation::lerp_anim;
use crate::app::KitsuneBrowser;
use crate::panels::profile_panel::profile_panel;
use crate::panels::task_graph_panel::task_graph_panel;
use crate::theme::{colors, KitsuneTheme};
use eframe::egui;

pub fn session_panel(ctx: &egui::Context, browser: &KitsuneBrowser) {
    egui::SidePanel::right("session_panel")
        .resizable(true)
        .default_width(195.0)
        .min_width(160.0)
        .max_width(280.0)
        .frame(
            egui::Frame::none()
                .fill(colors::BG_PANEL)
                .stroke(egui::Stroke::new(1.0, KitsuneTheme::BORDER)),
        )
        .show(ctx, |ui| {
            // ── Header ────────────────────────────────────────────────────
            egui::Frame::none()
                .inner_margin(egui::Margin { left: 14.0, right: 12.0, top: 9.0, bottom: 9.0 })
                .show(ui, |ui| {
                    ui.label(
                        egui::RichText::new("SESSION")
                            .size(9.5)
                            .strong()
                            .color(KitsuneTheme::TEXT2)
                            .family(egui::FontFamily::Monospace),
                    );
                });
            paint_separator(ui);

            // ── Status grid ──────────────────────────────────────────────
            section(ui, ctx, "STATUS", true, |ui| {
                let n   = browser.privacy.trackers_blocked;
                let tls = browser.privacy.tls_version;
                stat_row(ui, "status",      "● Active",          KitsuneTheme::GREEN);
                stat_row(ui, "mode",        "Agent-First",       KitsuneTheme::TEXT_PRIMARY);
                stat_row(ui, "tls",         tls,                 KitsuneTheme::GREEN);
                stat_row(ui, "trackers",    &format!("{n} blocked"), if n > 0 { KitsuneTheme::GREEN } else { KitsuneTheme::TEXT3 });
                stat_row(ui, "referer",     "stripped",          KitsuneTheme::GREEN);
                stat_row(ui, "fingerprint", "hardened",          KitsuneTheme::GREEN);
                stat_row(ui, "hil gate",    "armed",             KitsuneTheme::AMBER);
            });

            paint_separator(ui);

            // ── Capabilities ──────────────────────────────────────────────
            section(ui, ctx, "CAPABILITIES", true, |ui| {
                cap_toggle(ui, ctx, "🧠", "DOM Control",  "cap_dom",         true);
                cap_toggle(ui, ctx, "🔐", "Vault Access", "cap_vault",       true);
                cap_toggle(ui, ctx, "📋", "Audit Log",    "cap_audit",       true);
                cap_toggle(ui, ctx, "🏖", "Sandbox",      "cap_sandbox",     true);
                cap_toggle(ui, ctx, "🌐", "Network",      "cap_network",     true);
                cap_toggle(ui, ctx, "📸", "Screenshot",   "cap_screenshot",  false);
            });

            paint_separator(ui);

            // ── Vault ─────────────────────────────────────────────────────
            section(ui, ctx, "🔐 VAULT", true, |ui| {
                vault_item(ui, "👤", "demo@kitsune.ai", "token");
                vault_item(ui, "💳", "•••• 4242",      "locked");
                vault_item(ui, "🏠", "Home address",   "token");
            });

            paint_separator(ui);

            // ── Profile ───────────────────────────────────────────────────
            section(ui, ctx, "PROFILE", false, |ui| {
                profile_panel(ui, browser.profile_summary.as_ref());
            });

            paint_separator(ui);

            // ── Task Graph ────────────────────────────────────────────────
            section(ui, ctx, "TASK GRAPH", false, |ui| {
                task_graph_panel(ui, &browser.swarm_state);
            });
        });
}

// ── Collapsible section wrapper ───────────────────────────────────────────────

fn section(
    ui: &mut egui::Ui,
    ctx: &egui::Context,
    title: &str,
    default_open: bool,
    body: impl FnOnce(&mut egui::Ui),
) {
    let sec_id = egui::Id::new("session_sec").with(title);
    let open   = ctx.data(|d| d.get_temp::<bool>(sec_id).unwrap_or(default_open));

    egui::Frame::none()
        .inner_margin(egui::Margin { left: 12.0, right: 12.0, top: 5.0, bottom: 5.0 })
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                let arrow = if open { "▾" } else { "▸" };
                let hdr_id = sec_id.with("hdr");
                let hdr_hov = ctx.data(|d| d.get_temp::<bool>(hdr_id).unwrap_or(false));
                let col = if hdr_hov { KitsuneTheme::TEXT_PRIMARY } else { KitsuneTheme::TEXT2 };
                let (rect, resp) = ui.allocate_at_least(
                    egui::vec2(ui.available_width(), 16.0),
                    egui::Sense::click(),
                );
                if resp.clicked() {
                    ctx.data_mut(|d| d.insert_temp(sec_id, !open));
                }
                ctx.data_mut(|d| d.insert_temp(hdr_id, resp.hovered()));
                let painter = ui.painter();
                painter.text(
                    egui::pos2(rect.left(), rect.center().y),
                    egui::Align2::LEFT_CENTER,
                    arrow,
                    egui::FontId::proportional(10.0),
                    col,
                );
                painter.text(
                    egui::pos2(rect.left() + 12.0, rect.center().y),
                    egui::Align2::LEFT_CENTER,
                    title,
                    egui::FontId::monospace(9.5),
                    col,
                );
            });

            if open {
                ui.add_space(3.0);
                body(ui);
            }
        });
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn paint_separator(ui: &mut egui::Ui) {
    let rect = ui.available_rect_before_wrap();
    let sep  = egui::Rect::from_min_size(rect.left_top(), egui::vec2(rect.width(), 1.0));
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

fn cap_toggle(ui: &mut egui::Ui, ctx: &egui::Context, icon: &str, name: &str, id_key: &str, on: bool) {
    // Lerp knob position (0 = off, 1 = on)
    let knob_id = egui::Id::new(id_key).with("knob");
    let knob_t  = lerp_anim(ctx, knob_id, if on { 1.0 } else { 0.0 }, 8.0);

    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(icon).size(11.0));
        ui.add_space(3.0);
        ui.label(egui::RichText::new(name).size(10.0).color(KitsuneTheme::TEXT1));

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            // Draw custom toggle track + knob
            let track_w = 26.0;
            let track_h = 13.0;
            let (track_rect, _) = ui.allocate_exact_size(egui::vec2(track_w, track_h), egui::Sense::hover());

            let track_col = if on {
                egui::Color32::from_rgb(
                    (74.0 * knob_t) as u8,
                    (222.0 * knob_t) as u8,
                    (128.0 * knob_t) as u8,
                )
            } else {
                egui::Color32::from_rgb(48, 48, 62)
            };
            ui.painter().rect_filled(track_rect, egui::Rounding::same(track_h / 2.0), track_col);

            // Knob
            let knob_r  = 5.0_f32;
            let knob_x  = track_rect.left() + knob_r + 1.0 + (track_w - 2.0 * knob_r - 2.0) * knob_t;
            let knob_col = egui::Color32::WHITE;
            ui.painter().circle_filled(
                egui::pos2(knob_x, track_rect.center().y),
                knob_r,
                knob_col,
            );
        });
    });
}

fn vault_item(ui: &mut egui::Ui, icon: &str, name: &str, status: &str) {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(icon).size(11.0));
        ui.add_space(2.0);
        ui.label(egui::RichText::new(name).size(10.0).color(KitsuneTheme::TEXT1));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let col = match status {
                "token"  => KitsuneTheme::AMBER,
                "locked" => KitsuneTheme::GREEN,
                _        => KitsuneTheme::TEXT3,
            };
            ui.label(egui::RichText::new(status).size(9.0).color(col).family(egui::FontFamily::Monospace));
        });
    });
}
