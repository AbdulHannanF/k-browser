use eframe::egui;

pub struct KitsuneTheme;

impl KitsuneTheme {
    pub const ORANGE: egui::Color32 = egui::Color32::from_rgb(255, 119, 0);
    pub const ORANGE_HOVER: egui::Color32 = egui::Color32::from_rgb(255, 155, 50);
    pub const BG_DARK: egui::Color32 = egui::Color32::from_rgb(15, 15, 18);
    pub const BG_PANEL: egui::Color32 = egui::Color32::from_rgb(22, 22, 28);
    pub const BG_CARD: egui::Color32 = egui::Color32::from_rgb(30, 30, 38);
    pub const TEXT_PRIMARY: egui::Color32 = egui::Color32::from_rgb(240, 240, 245);
    pub const TEXT_MUTED: egui::Color32 = egui::Color32::from_rgb(140, 140, 155);
    pub const GREEN_SAFE: egui::Color32 = egui::Color32::from_rgb(74, 222, 128);
    pub const RED_BLOCKED: egui::Color32 = egui::Color32::from_rgb(248, 113, 113);
    pub const BLUE_ACTING: egui::Color32 = egui::Color32::from_rgb(74, 158, 255);

    // Constants that were in the old theme file
    pub const BG_BASE: egui::Color32 = egui::Color32::from_rgb(28, 27, 34);
    pub const BG_SURFACE: egui::Color32 = egui::Color32::from_rgb(43, 42, 51);
    pub const BG_ELEVATED: egui::Color32 = egui::Color32::from_rgb(66, 65, 77);
    pub const BORDER: egui::Color32 = egui::Color32::from_rgb(82, 82, 94);
    pub const BORDER_BRIGHT: egui::Color32 = egui::Color32::from_rgb(143, 143, 157);
    pub const ACCENT: egui::Color32 = egui::Color32::from_rgb(0, 97, 224);
    pub const SUCCESS: egui::Color32 = egui::Color32::from_rgb(46, 204, 113);
    pub const WARNING: egui::Color32 = egui::Color32::from_rgb(241, 196, 15);
    pub const ERROR: egui::Color32 = egui::Color32::from_rgb(231, 76, 60);
    pub const AGENT_READING: egui::Color32 = Self::WARNING;
    pub const AGENT_ACTING: egui::Color32 = Self::ACCENT;


    pub fn apply(ctx: &egui::Context) {
        let mut style = egui::Style {
            visuals: egui::Visuals::dark(),
            ..Default::default()
        };
        style.visuals.panel_fill = Self::BG_PANEL;
        style.visuals.window_fill = Self::BG_DARK;
        style.visuals.selection.bg_fill = Self::ORANGE;
        style.visuals.hyperlink_color = Self::ORANGE;
        style.visuals.widgets.active.bg_fill = Self::ORANGE;
        style.visuals.widgets.hovered.bg_fill = Self::ORANGE_HOVER;
        style.visuals.widgets.hovered.bg_stroke.color = Self::ORANGE;
        // Rounded everything
        style.visuals.window_rounding = egui::Rounding::same(8.0);
        style.visuals.widgets.noninteractive.rounding = egui::Rounding::same(6.0);
        style.visuals.widgets.inactive.rounding = egui::Rounding::same(6.0);
        ctx.set_style(style);
    }
}
