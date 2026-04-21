use eframe::egui;
use crate::theme::KitsuneTheme;
use raw_window_handle::HasWindowHandle;

pub struct KitsuneBrowser {
    address_bar: String,
    // cef: Option<CefBrowser>,      // None until first frame (need HWND)
}

impl KitsuneBrowser {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        KitsuneTheme::apply(&cc.egui_ctx);
        Self {
            address_bar: "http://127.0.0.1:7700/".to_string(),
                        // cef: None,
        }
    }

    fn render_top_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("chrome")
            .frame(egui::Frame::none().fill(KitsuneTheme::BG_PANEL).inner_margin(6.0))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    // Logo
                    ui.colored_label(KitsuneTheme::ORANGE, "🦊");
                    ui.label(egui::RichText::new("Kitsune").strong().color(KitsuneTheme::TEXT_PRIMARY));
                    ui.separator();

                    // Nav buttons
                    if ui.small_button("◀").clicked() { /* self.go_back(); */ }
                    if ui.small_button("▶").clicked() { /* self.go_forward(); */ }
                    if ui.small_button("↻").clicked() { /* self.reload(); */ }

                    // Address bar
                    let addr = egui::TextEdit::singleline(&mut self.address_bar)
                        .desired_width(ui.available_width() - 120.0)
                        .frame(true)
                        .hint_text("Enter URL...");
                    let r = ui.add(addr);
                    if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        // self.navigate_to(self.address_bar.clone());
                    }

                    // Privacy pill
                    let blocked = 0; // self.privacy_stats.trackers_blocked;
                    let pill_color = if blocked > 0 { KitsuneTheme::GREEN_SAFE } else { KitsuneTheme::TEXT_MUTED };
                    ui.colored_label(pill_color, format!("🛡 {blocked} blocked"));
                });
            });
    }
}

impl eframe::App for KitsuneBrowser {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        // Initialize webview on first frame (HWND is available now)
        // Initialize CEF on first frame (HWND is available now)
        // if self.cef.is_none() {
        //     #[cfg(target_os = "windows")]
        //     {
        //         if let Ok(handle) = frame.window_handle() {
        //             if let raw_window_handle::RawWindowHandle::Win32(handle) = handle.as_raw() {
        //                 // self.cef = CefBrowser::new(handle.hwnd.get() as *mut _, &self.address_bar)
        //                 //     .map_err(|e| tracing::error!("CEF init failed: {e}"))
        //                 //     .ok();
        //             }
        //         }
        //     }
        // }

        self.render_top_bar(ctx);

        // Central panel — just measure its rect, the webview fills it
        let central_rect = egui::CentralPanel::default()
            .show(ctx, |_ui| {}) // empty — webview sits here
            .response
            .rect;

        // Reposition CEF every frame to track panel size
        // if let Some(cef) = &self.cef {
        //     cef.set_bounds(central_rect.into());
        // }
    }
}
