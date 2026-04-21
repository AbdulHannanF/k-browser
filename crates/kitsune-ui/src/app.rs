use eframe::egui;
use crate::theme::KitsuneTheme;
use kitsune_cef::{CefBrowser, CefRect};
use raw_window_handle::HasWindowHandle;
use crate::chrome::top_bar::top_bar;
use crate::panels::agent_panel::agent_panel;
use crate::panels::session_panel::session_panel;

pub struct KitsuneBrowser {
    pub address_bar: String,
    pub agent_command: String,
    pub cef: Option<CefBrowser>,      // None until first frame (need HWND)
}

impl KitsuneBrowser {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        KitsuneTheme::apply(&cc.egui_ctx);
        Self {
            address_bar: "http://127.0.0.1:7700/".to_string(),
            agent_command: String::new(),
            cef: None,
        }
    }

}

impl eframe::App for KitsuneBrowser {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        top_bar(ctx, self);
        agent_panel(ctx, self);
        session_panel(ctx);

        // Central panel — just measure its rect, the webview fills it
        let _central_rect = egui::CentralPanel::default()
            .show(ctx, |_ui| {}) // empty — webview sits here
            .response
            .rect;

        // Initialize CEF on first frame (HWND available now)
        if self.cef.is_none() {
            #[cfg(target_os = "windows")]
            {
                if let Ok(handle) = _frame.window_handle() {
                    if let raw_window_handle::RawWindowHandle::Win32(handle) = handle.as_raw() {
                        let rect = _central_rect;
                        self.cef = CefBrowser::new(
                            handle.hwnd.get() as isize,
                            &self.address_bar,
                            CefRect {
                                x: rect.min.x as i32,
                                y: rect.min.y as i32,
                                width: rect.width() as u32,
                                height: rect.height() as u32,
                            },
                        )
                        .map_err(|e| tracing::error!("CEF init failed: {e}"))
                        .ok();
                    }
                }
            }
        }

        // Reposition CEF every frame to track panel size
        if let Some(cef) = &self.cef {
            let rect = _central_rect;
            cef.set_bounds(CefRect {
                x: rect.min.x as i32,
                y: rect.min.y as i32,
                width: rect.width() as u32,
                height: rect.height() as u32,
            });
        }
    }
}
