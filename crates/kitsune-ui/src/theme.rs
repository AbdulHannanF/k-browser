use eframe::egui::{self, Color32, FontData, FontDefinitions, FontFamily};

// ── Spec-aligned color palette ───────────────────────────────────────────────

pub mod colors {
    use eframe::egui::Color32;

    pub const BG_VOID:     Color32 = Color32::from_rgb(8,   8,   10);
    pub const BG_BASE:     Color32 = Color32::from_rgb(12,  12,  15);
    pub const BG_PANEL:    Color32 = Color32::from_rgb(16,  16,  20);
    pub const BG_CARD:     Color32 = Color32::from_rgb(22,  22,  28);
    pub const BG_ELEVATED: Color32 = Color32::from_rgb(28,  28,  36);
    pub const BG_INPUT:    Color32 = Color32::from_rgb(18,  18,  24);

    pub const BORDER_DIM:    Color32 = Color32::from_rgb(32,  32,  42);
    pub const BORDER_NORMAL: Color32 = Color32::from_rgb(48,  48,  62);
    pub const BORDER_BRIGHT: Color32 = Color32::from_rgb(70,  70,  88);

    pub const ORANGE:      Color32 = Color32::from_rgb(249, 115, 22);
    pub const ORANGE_DIM:  Color32 = Color32::from_rgb(194, 88,  17);
    // premultiplied orange at 30/255 opacity: (249*30/255≈29, 115*30/255≈14, 22*30/255≈3)
    pub const ORANGE_GLOW: Color32 = Color32::from_rgba_premultiplied(29,  14,  3,  30);

    pub const GREEN:       Color32 = Color32::from_rgb(74,  222, 128);
    pub const GREEN_DIM:   Color32 = Color32::from_rgb(34,  197, 94);
    // premultiplied green at 25/255 opacity
    pub const GREEN_GLOW:  Color32 = Color32::from_rgba_premultiplied(7,   22,  13, 25);

    pub const RED:         Color32 = Color32::from_rgb(248, 113, 113);
    // premultiplied red at 25/255 opacity
    pub const RED_GLOW:    Color32 = Color32::from_rgba_premultiplied(24,  11,  11, 25);

    pub const YELLOW:      Color32 = Color32::from_rgb(251, 191, 36);
    pub const BLUE:        Color32 = Color32::from_rgb(96,  165, 250);

    pub const TEXT_PRIMARY:   Color32 = Color32::from_rgb(240, 240, 245);
    pub const TEXT_SECONDARY: Color32 = Color32::from_rgb(160, 160, 175);
    pub const TEXT_MUTED:     Color32 = Color32::from_rgb(90,  90,  105);
    pub const TEXT_ACCENT:    Color32 = ORANGE;
    pub const TEXT_MONO:      Color32 = Color32::from_rgb(180, 210, 180);
}

pub mod spacing {
    pub const PANEL_PAD:   f32 = 12.0;
    pub const CARD_PAD:    f32 = 10.0;
    pub const ITEM_GAP:    f32 = 6.0;
    pub const BORDER_R:    f32 = 6.0;
    pub const BORDER_R_SM: f32 = 4.0;
    pub const BORDER_R_LG: f32 = 8.0;
}

pub mod fonts {
    pub const SIZE_XS:   f32 = 10.0;
    pub const SIZE_SM:   f32 = 11.5;
    pub const SIZE_BASE: f32 = 12.5;
    pub const SIZE_MD:   f32 = 14.0;
    pub const SIZE_LG:   f32 = 16.0;
    pub const SIZE_XL:   f32 = 20.0;
    pub const SIZE_HERO: f32 = 26.0;
}

// ── KitsuneTheme — backward-compatible facade over the new palette ────────────

pub struct KitsuneTheme;

impl KitsuneTheme {
    // Backgrounds
    pub const BG:  Color32 = colors::BG_VOID;
    pub const BG1: Color32 = colors::BG_PANEL;
    pub const BG2: Color32 = colors::BG_CARD;
    pub const BG3: Color32 = colors::BG_ELEVATED;
    pub const BG4: Color32 = Color32::from_rgb(36, 36, 46);

    // Accents
    pub const AMBER:  Color32 = colors::ORANGE;
    pub const AMBER2: Color32 = Color32::from_rgb(255, 165, 60);
    pub const GREEN:  Color32 = colors::GREEN;
    pub const RED:    Color32 = colors::RED;
    pub const BLUE:   Color32 = colors::BLUE;
    pub const YELLOW: Color32 = colors::YELLOW;

    // Text
    pub const TEXT0: Color32 = colors::TEXT_PRIMARY;
    pub const TEXT1: Color32 = colors::TEXT_SECONDARY;
    pub const TEXT2: Color32 = Color32::from_rgb(120, 120, 145);
    pub const TEXT3: Color32 = colors::TEXT_MUTED;

    // Borders & tints
    pub const BORDER:       Color32 = colors::BORDER_DIM;
    pub const BORDER2:      Color32 = colors::BORDER_NORMAL;
    // premultiplied orange at ~27% alpha for border strokes
    pub const BORDER_AMBER: Color32 = Color32::from_rgba_premultiplied(66,  31,  6,  68);
    // premultiplied orange at ~7% alpha for card fills
    pub const AMBER_DIM:    Color32 = Color32::from_rgba_premultiplied(18,  8,   2,  18);
    // premultiplied green at ~7% alpha for card fills
    pub const GREEN_DIM:    Color32 = Color32::from_rgba_premultiplied(5,   16,  9,  18);
    pub const ORANGE_GLOW:  Color32 = colors::ORANGE_GLOW;

    // Semantic aliases (backward compat)
    pub const BG_PANEL:     Color32 = Self::BG1;
    pub const BG_CARD:      Color32 = Self::BG2;
    pub const BG_HOVER:     Color32 = Self::BG3;
    pub const TEXT_PRIMARY: Color32 = Self::TEXT0;
    pub const TEXT_MUTED:   Color32 = Self::TEXT1;
    pub const GREEN_SAFE:   Color32 = Self::GREEN;
    pub const RED_BLOCKED:  Color32 = Self::RED;

    pub fn apply(ctx: &egui::Context) {
        // ── Font loading ─────────────────────────────────────────────────────
        let mut fonts = FontDefinitions::default();

        let font_dirs: Vec<std::path::PathBuf> = {
            let mut dirs = Vec::new();
            let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
            dirs.push(manifest_dir.join("..").join("..").join("assets"));
            dirs.push(manifest_dir.join("assets"));
            if let Ok(exe) = std::env::current_exe() {
                if let Some(d) = exe.parent() {
                    dirs.push(d.join("assets"));
                }
            }
            if let Ok(cwd) = std::env::current_dir() {
                dirs.push(cwd.join("assets"));
            }
            dirs
        };

        for dir in &font_dirs {
            if let Ok(bytes) = std::fs::read(dir.join("Inter-Regular.ttf")) {
                fonts.font_data.insert("Inter".into(), FontData::from_owned(bytes).into());
                fonts.families.entry(FontFamily::Proportional).or_default().insert(0, "Inter".into());
                break;
            }
        }
        for dir in &font_dirs {
            if let Ok(bytes) = std::fs::read(dir.join("JetBrainsMono-Regular.ttf")) {
                fonts.font_data.insert("JetBrainsMono".into(), FontData::from_owned(bytes).into());
                fonts.families.entry(FontFamily::Monospace).or_default().insert(0, "JetBrainsMono".into());
                break;
            }
        }
        ctx.set_fonts(fonts);

        // ── Style ────────────────────────────────────────────────────────────
        let mut style = (*ctx.style()).clone();
        style.visuals = egui::Visuals::dark();

        style.visuals.panel_fill       = colors::BG_PANEL;
        style.visuals.window_fill      = colors::BG_CARD;
        style.visuals.faint_bg_color   = colors::BG_ELEVATED;
        style.visuals.extreme_bg_color = colors::BG_VOID;
        style.visuals.code_bg_color    = colors::BG_INPUT;
        style.visuals.window_stroke    = egui::Stroke::new(1.0, colors::BORDER_DIM);

        style.visuals.widgets.noninteractive.bg_fill   = colors::BG_CARD;
        style.visuals.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0, colors::BORDER_DIM);
        style.visuals.widgets.noninteractive.fg_stroke = egui::Stroke::new(1.0, colors::TEXT_SECONDARY);
        style.visuals.widgets.inactive.bg_fill         = colors::BG_ELEVATED;
        style.visuals.widgets.inactive.bg_stroke       = egui::Stroke::new(1.0, colors::BORDER_NORMAL);
        style.visuals.widgets.inactive.fg_stroke       = egui::Stroke::new(1.0, colors::TEXT_SECONDARY);
        style.visuals.widgets.hovered.bg_fill          = colors::BG_ELEVATED;
        style.visuals.widgets.hovered.bg_stroke        = egui::Stroke::new(1.0, colors::ORANGE_DIM);
        style.visuals.widgets.hovered.fg_stroke        = egui::Stroke::new(1.0, colors::TEXT_PRIMARY);
        style.visuals.widgets.active.bg_fill           = colors::ORANGE;
        style.visuals.widgets.active.bg_stroke         = egui::Stroke::new(1.0, colors::ORANGE);
        style.visuals.widgets.active.fg_stroke         = egui::Stroke::new(1.0, Color32::BLACK);

        style.visuals.selection.bg_fill = colors::ORANGE_GLOW;
        style.visuals.selection.stroke  = egui::Stroke::new(1.0, colors::ORANGE);

        let r    = egui::Rounding::same(spacing::BORDER_R);
        let r_lg = egui::Rounding::same(spacing::BORDER_R_LG);
        style.visuals.window_rounding                 = r_lg;
        style.visuals.widgets.noninteractive.rounding = r;
        style.visuals.widgets.inactive.rounding       = r;
        style.visuals.widgets.hovered.rounding        = r;
        style.visuals.widgets.active.rounding         = r;

        style.visuals.override_text_color       = Some(colors::TEXT_PRIMARY);
        style.spacing.scroll.bar_width          = 4.0;
        style.spacing.scroll.handle_min_length = 24.0;

        style.spacing.item_spacing   = egui::vec2(6.0, 4.0);
        style.spacing.button_padding = egui::vec2(8.0, 4.0);

        style.text_styles.insert(egui::TextStyle::Heading,   egui::FontId::proportional(18.0));
        style.text_styles.insert(egui::TextStyle::Body,      egui::FontId::proportional(13.0));
        style.text_styles.insert(egui::TextStyle::Monospace, egui::FontId::monospace(12.0));

        ctx.set_style(style);
    }
}
