use egui::{style::Widgets, Color32, Rounding, Stroke, Style, Visuals};

// Modern Browser Theme (Firefox/Chromium style)
pub const BG_BASE: Color32 = Color32::from_rgb(28, 27, 34);       // Firefox Dark theme background
pub const BG_SURFACE: Color32 = Color32::from_rgb(43, 42, 51);    // Firefox Tab/Toolbar background
pub const BG_ELEVATED: Color32 = Color32::from_rgb(66, 65, 77);   // Elevated/hovered
pub const BORDER: Color32 = Color32::from_rgb(82, 82, 94);        // Borders
pub const BORDER_BRIGHT: Color32 = Color32::from_rgb(143, 143, 157);

pub const ACCENT: Color32 = Color32::from_rgb(0, 97, 224);      // Firefox blue
pub const ACCENT_HOVER: Color32 = Color32::from_rgb(2, 80, 187);
pub const TEXT_PRIMARY: Color32 = Color32::from_rgb(251, 251, 254);
pub const TEXT_MUTED: Color32 = Color32::from_rgb(170, 170, 170);

pub const SUCCESS: Color32 = Color32::from_rgb(46, 204, 113);
pub const WARNING: Color32 = Color32::from_rgb(241, 196, 15);
pub const ERROR: Color32 = Color32::from_rgb(231, 76, 60);

// Status Colors
pub const AGENT_READING: Color32 = WARNING;
pub const AGENT_ACTING: Color32 = ACCENT;
pub const AGENT_DONE: Color32 = SUCCESS;

#[derive(Debug, Clone)]
pub struct KitsuneTheme {
    pub name: String,
    pub style: Style,
}

impl KitsuneTheme {
    pub fn dark() -> Self {
        let mut widgets = Widgets::dark();

        let rounding = Rounding::same(6.0);

        widgets.inactive.bg_fill = BG_ELEVATED;
        widgets.inactive.bg_stroke = Stroke::new(1.0, BORDER);
        widgets.inactive.rounding = rounding;

        widgets.hovered.bg_fill = Color32::from_rgb(82, 82, 94);
        widgets.hovered.bg_stroke = Stroke::new(1.0, BORDER_BRIGHT);
        widgets.hovered.rounding = rounding;

        widgets.active.bg_fill = ACCENT;
        widgets.active.rounding = rounding;

        Self {
            name: "modern_browser".to_string(),
            style: Style {
                visuals: Visuals {
                    dark_mode: true,
                    override_text_color: Some(TEXT_PRIMARY),
                    widgets,
                    window_fill: BG_SURFACE,
                    panel_fill: BG_BASE,
                    extreme_bg_color: Color32::from_rgb(20, 20, 25),
                    window_stroke: Stroke::new(1.0, BORDER),
                    selection: egui::style::Selection {
                        bg_fill: ACCENT.linear_multiply(0.5),
                        stroke: Stroke::new(1.0, ACCENT),
                    },
                    hyperlink_color: Color32::from_rgb(0, 150, 255),
                    window_rounding: rounding,
                    button_frame: true,
                    ..Default::default()
                },
                spacing: egui::style::Spacing {
                    item_spacing: egui::vec2(8.0, 8.0),
                    window_margin: egui::Margin::same(12.0),
                    button_padding: egui::vec2(12.0, 6.0),
                    ..Default::default()
                },
                ..Default::default()
            },
        }
    }
}

