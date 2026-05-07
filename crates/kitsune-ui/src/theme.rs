use eframe::egui::{self, Color32, FontData, FontDefinitions, FontFamily};

pub struct KitsuneTheme;

impl KitsuneTheme {
    // ── Background Ramp ──────────────────────────────────────────────────
    pub const BG: Color32 = Color32::from_rgb(8, 8, 13);
    pub const BG1: Color32 = Color32::from_rgb(15, 15, 24);
    pub const BG2: Color32 = Color32::from_rgb(20, 20, 31);
    pub const BG3: Color32 = Color32::from_rgb(28, 28, 44);
    pub const BG4: Color32 = Color32::from_rgb(34, 34, 53);

    // ── Accent Colors ────────────────────────────────────────────────────
    pub const AMBER: Color32 = Color32::from_rgb(255, 122, 0);
    pub const AMBER2: Color32 = Color32::from_rgb(255, 179, 71);
    pub const GREEN: Color32 = Color32::from_rgb(57, 232, 143);
    pub const RED: Color32 = Color32::from_rgb(255, 77, 106);
    pub const BLUE: Color32 = Color32::from_rgb(74, 158, 255);

    // ── Text Ramp ────────────────────────────────────────────────────────
    pub const TEXT0: Color32 = Color32::from_rgb(250, 250, 255);
    pub const TEXT1: Color32 = Color32::from_rgb(200, 200, 220);
    pub const TEXT2: Color32 = Color32::from_rgb(150, 150, 175);
    pub const TEXT3: Color32 = Color32::from_rgb(115, 115, 135);

    // ── Borders & Tints ──────────────────────────────────────────────────
    pub const BORDER: Color32 = Color32::from_rgba_premultiplied(15, 15, 15, 15);
    pub const BORDER2: Color32 = Color32::from_rgba_premultiplied(30, 30, 30, 30);
    pub const BORDER_AMBER: Color32 = Color32::from_rgba_premultiplied(70, 33, 0, 70);
    pub const AMBER_DIM: Color32 = Color32::from_rgba_premultiplied(24, 11, 0, 24);
    pub const GREEN_DIM: Color32 = Color32::from_rgba_premultiplied(5, 22, 13, 24);

    // ── Semantic Aliases ─────────────────────────────────────────────────
    pub const BG_PANEL: Color32 = Self::BG1;
    pub const BG_CARD: Color32 = Self::BG2;
    pub const BG_HOVER: Color32 = Self::BG4;
    pub const TEXT_PRIMARY: Color32 = Self::TEXT0;
    pub const TEXT_MUTED: Color32 = Self::TEXT1;
    pub const GREEN_SAFE: Color32 = Self::GREEN;
    pub const RED_BLOCKED: Color32 = Self::RED;

    pub fn apply(ctx: &egui::Context) {
        let mut fonts = FontDefinitions::default();

        // Try multiple candidate directories for font loading
        let font_dirs: Vec<std::path::PathBuf> = {
            let mut dirs = Vec::new();
            // Cargo workspace root (works during `cargo run`)
            let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
            dirs.push(manifest_dir.join("..").join("..").join("assets"));
            dirs.push(manifest_dir.join("assets"));
            // Next to the executable (works for release builds)
            if let Ok(exe) = std::env::current_exe() {
                if let Some(exe_dir) = exe.parent() {
                    dirs.push(exe_dir.join("assets"));
                }
            }
            // Current working directory
            if let Ok(cwd) = std::env::current_dir() {
                dirs.push(cwd.join("assets"));
            }
            dirs
        };

        for dir in &font_dirs {
            let inter = dir.join("Inter-Regular.ttf");
            if let Ok(bytes) = std::fs::read(&inter) {
                fonts
                    .font_data
                    .insert("Inter".into(), FontData::from_owned(bytes).into());
                fonts
                    .families
                    .entry(FontFamily::Proportional)
                    .or_default()
                    .insert(0, "Inter".into());
                break;
            }
        }

        for dir in &font_dirs {
            let mono = dir.join("JetBrainsMono-Regular.ttf");
            if let Ok(bytes) = std::fs::read(&mono) {
                fonts
                    .font_data
                    .insert("JetBrainsMono".into(), FontData::from_owned(bytes).into());
                fonts
                    .families
                    .entry(FontFamily::Monospace)
                    .or_default()
                    .insert(0, "JetBrainsMono".into());
                break;
            }
        }

        ctx.set_fonts(fonts);

        let mut style = (*ctx.style()).clone();
        style.visuals = egui::Visuals::dark();
        style.visuals.panel_fill = Self::BG;
        style.visuals.window_fill = Self::BG1;
        style.visuals.faint_bg_color = Self::BG2;
        style.visuals.extreme_bg_color = Self::BG;
        style.visuals.code_bg_color = Self::BG3;
        style.visuals.window_stroke = egui::Stroke::new(1.0, Self::BORDER);
        style.visuals.selection.bg_fill = Self::AMBER_DIM;
        style.visuals.selection.stroke = egui::Stroke::new(1.0, Self::AMBER);
        style.visuals.widgets.noninteractive.bg_fill = Self::BG2;
        style.visuals.widgets.noninteractive.fg_stroke = egui::Stroke::new(1.0, Self::TEXT1);
        style.visuals.widgets.inactive.bg_fill = Self::BG3;
        style.visuals.widgets.inactive.fg_stroke = egui::Stroke::new(1.0, Self::TEXT1);
        style.visuals.widgets.hovered.bg_fill = Self::BG4;
        style.visuals.widgets.hovered.fg_stroke = egui::Stroke::new(1.0, Self::TEXT0);
        style.visuals.widgets.active.bg_fill = Self::AMBER;
        style.visuals.widgets.active.fg_stroke = egui::Stroke::new(1.0, Color32::BLACK);
        style.spacing.item_spacing = egui::vec2(6.0, 4.0);
        style.spacing.button_padding = egui::vec2(8.0, 4.0);
        style
            .text_styles
            .insert(egui::TextStyle::Heading, egui::FontId::proportional(18.0));
        style
            .text_styles
            .insert(egui::TextStyle::Body, egui::FontId::proportional(13.0));
        style
            .text_styles
            .insert(egui::TextStyle::Monospace, egui::FontId::monospace(12.0));
        ctx.set_style(style);
    }
}
