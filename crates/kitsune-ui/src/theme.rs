use eframe::egui::{self, Color32};

pub struct KitsuneTheme;

impl KitsuneTheme {
    pub const BG:         Color32 = Color32::from_rgb(8,   8,  13);
    pub const BG_PANEL:   Color32 = Color32::from_rgb(15,  15, 24);
    pub const BG_CARD:    Color32 = Color32::from_rgb(20,  20, 31);
    pub const BG3:        Color32 = Color32::from_rgb(28,  28, 44);
    pub const BG4:        Color32 = Color32::from_rgb(34,  34, 53);
    pub const AMBER:      Color32 = Color32::from_rgb(255, 122,  0);
    pub const AMBER2:     Color32 = Color32::from_rgb(255, 179, 71);
    pub const GREEN_SAFE: Color32 = Color32::from_rgb(57,  232, 143);
    pub const RED_BLOCKED:Color32 = Color32::from_rgb(255,  77, 106);
    pub const BLUE:       Color32 = Color32::from_rgb(74,  158, 255);
    pub const TEXT_PRIMARY:Color32 = Color32::from_rgb(242, 242, 250);
    pub const TEXT_MUTED: Color32 = Color32::from_rgb(184, 184, 208);
    pub const TEXT2:      Color32 = Color32::from_rgb(106, 106, 138);
    pub const BORDER:     Color32 = Color32::from_rgba_premultiplied(255,255,255,15);

    pub fn apply(ctx: &egui::Context) {
        let mut style = egui::Style {
            visuals: egui::Visuals::dark(),
            ..Default::default()
        };
        style.visuals.panel_fill = Self::BG_PANEL;
        style.visuals.window_fill = Self::BG;
        style.visuals.selection.bg_fill = Self::AMBER;
        style.visuals.hyperlink_color = Self::AMBER;
        style.visuals.widgets.active.bg_fill = Self::AMBER;
        style.visuals.widgets.hovered.bg_fill = Self::AMBER2;
        style.visuals.widgets.hovered.bg_stroke.color = Self::AMBER;
        style.visuals.window_rounding = egui::Rounding::same(8.0);
        style.visuals.widgets.noninteractive.rounding = egui::Rounding::same(6.0);
        style.visuals.widgets.inactive.rounding = egui::Rounding::same(6.0);
        ctx.set_style(style);
    }
}
