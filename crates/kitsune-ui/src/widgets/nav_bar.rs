use crate::theme;
use egui::{FontId, Rounding, Stroke, Vec2, Align2, RichText};

pub struct NavBarResponse {
    pub navigate_back: bool,
    pub navigate_forward: bool,
    pub reload: bool,
    pub go_home: bool,
    pub toggle_shelf: bool,
    pub toggle_privacy: bool,
    pub toggle_settings: bool,
    pub url_submitted: Option<String>,
}

pub fn nav_bar(
    ui: &mut egui::Ui,
    url_bar: &mut String,
    can_go_back: bool,
    can_go_forward: bool,
) -> NavBarResponse {
    let mut response = NavBarResponse {
        navigate_back: false,
        navigate_forward: false,
        reload: false,
        go_home: false,
        toggle_shelf: false,
        toggle_privacy: false,
        toggle_settings: false,
        url_submitted: None,
    };

    let bar_height = 44.0;
    let available_width = ui.available_width();

    let (_, bar_rect) = ui.allocate_space(Vec2::new(available_width, bar_height));

    // Background color for nav bar matches the active tab (surface)
    ui.painter().rect_filled(bar_rect, Rounding::ZERO, theme::BG_SURFACE);

    // Subtle bottom border
    ui.painter().line_segment(
        [bar_rect.left_bottom(), bar_rect.right_bottom()],
        Stroke::new(1.0, theme::BORDER.linear_multiply(0.5)),
    );

    ui.allocate_new_ui(egui::UiBuilder::new().max_rect(bar_rect), |ui| {
        ui.horizontal(|ui| {
            ui.set_height(bar_height);
            ui.add_space(8.0);

            // --- Navigation Controls ---
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;

                if nav_button(ui, "←", can_go_back).on_hover_text("Back").clicked() {
                    response.navigate_back = true;
                }
                if nav_button(ui, "→", can_go_forward).on_hover_text("Forward").clicked() {
                    response.navigate_forward = true;
                }
                if nav_button(ui, "⟳", true).on_hover_text("Reload").clicked() {
                    response.reload = true;
                }
                if nav_button(ui, "🏠", true).on_hover_text("Home").clicked() {
                    response.go_home = true;
                }
            });

            ui.add_space(8.0);

            // --- Address Bar ---
            let address_bar_width = ui.available_width() - 140.0;
            let (address_rect, address_resp) = ui.allocate_exact_size(
                Vec2::new(address_bar_width, 32.0),
                egui::Sense::click(),
            );

            let is_focused = address_resp.has_focus() || address_resp.hovered();

            // Rounded address bar
            let rounding = Rounding::same(16.0); // Pill-shaped or heavily rounded
            ui.painter().rect_filled(
                address_rect,
                rounding,
                if is_focused { theme::BG_ELEVATED } else { theme::BG_BASE },
            );
            ui.painter().rect_stroke(
                address_rect,
                rounding,
                Stroke::new(1.5, if is_focused { theme::BORDER_BRIGHT } else { theme::BORDER }),
            );

            let mut address_ui = ui.new_child(egui::UiBuilder::new().max_rect(address_rect).layout(*ui.layout()));
            address_ui.horizontal(|ui| {
                ui.add_space(12.0);

                let shield_color = if url_bar.starts_with("https://") {
                    theme::SUCCESS
                } else if url_bar.starts_with("kitsune://") {
                    theme::ACCENT
                } else {
                    theme::TEXT_MUTED
                };
                ui.label(RichText::new("🔒").color(shield_color).size(14.0));

                ui.add_space(4.0);

                let text_edit_resp = ui.add_sized(
                    ui.available_size() - egui::vec2(12.0, 0.0),
                    egui::TextEdit::singleline(url_bar)
                        .frame(false)
                        .hint_text("Search or enter address")
                        .font(FontId::proportional(14.0)) // Proportional for standard browser look
                        .text_color(theme::TEXT_PRIMARY)
                        .vertical_align(egui::Align::Center)
                );

                if text_edit_resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    response.url_submitted = Some(url_bar.clone());
                }
            });

            ui.add_space(8.0);

            // --- Action Icons ---
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;

                if nav_button(ui, "🦊", true).on_hover_text("Agents").clicked() {
                    response.toggle_shelf = true;
                }
                if nav_button(ui, "🛡", true).on_hover_text("Privacy Protections").clicked() {
                    response.toggle_privacy = true;
                }
                if nav_button(ui, "≡", true).on_hover_text("Open Application Menu").clicked() {
                    response.toggle_settings = true;
                }
            });
        });
    });

    response
}

fn nav_button(ui: &mut egui::Ui, text: &str, enabled: bool) -> egui::Response {
    let size = Vec2::splat(34.0);
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());

    if enabled {
        let text_color = if response.clicked() {
            theme::ACCENT
        } else if response.hovered() {
            theme::TEXT_PRIMARY
        } else {
            theme::TEXT_MUTED
        };

        if response.hovered() {
            ui.painter().rect_filled(rect, Rounding::same(17.0), theme::BG_ELEVATED);
        }

        ui.painter().text(
            rect.center(),
            Align2::CENTER_CENTER,
            text,
            FontId::proportional(18.0),
            text_color,
        );
    } else {
        ui.painter().text(
            rect.center(),
            Align2::CENTER_CENTER,
            text,
            FontId::proportional(18.0),
            theme::TEXT_MUTED.linear_multiply(0.2),
        );
    }

    response
}
