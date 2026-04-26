use eframe::egui;
use crate::theme::KitsuneTheme;
use kitsune_cef::{CefBrowser, CefRect};
use raw_window_handle::HasWindowHandle;
use crate::chrome::top_bar::top_bar;
use crate::panels::agent_panel::agent_panel;
use crate::panels::session_panel::session_panel;
use crate::dialogs::hil_dialog::hil_dialog;
use kitsune_core::tab::Tab;

// ── Log entry ────────────────────────────────────────────────────────────────
#[derive(Clone, PartialEq)]
pub enum LogLevel { Cmd, Info, Ok, Warn, Block }

#[derive(Clone)]
pub struct LogEntry {
    pub text:  String,
    pub level: LogLevel,
}

impl LogEntry {
    pub fn new(text: impl Into<String>, level: LogLevel) -> Self {
        Self { text: text.into(), level }
    }
    pub fn color(&self) -> egui::Color32 {
        match self.level {
            LogLevel::Cmd   => KitsuneTheme::TEXT_PRIMARY,
            LogLevel::Info  => KitsuneTheme::TEXT_MUTED,
            LogLevel::Ok    => KitsuneTheme::GREEN_SAFE,
            LogLevel::Warn  => KitsuneTheme::AMBER,
            LogLevel::Block => KitsuneTheme::RED_BLOCKED,
        }
    }
}

// ── Privacy stats ────────────────────────────────────────────────────────────
#[derive(Default)]
pub struct PrivacyStats {
    pub trackers_blocked:   u32,
    pub referrers_stripped: u32,
    pub tls_version:        &'static str,
}

// ── Budget ───────────────────────────────────────────────────────────────────
pub struct BudgetState {
    pub used:  u32,
    pub total: u32,
}
impl Default for BudgetState {
    fn default() -> Self { Self { used: 0, total: 100 } }
}
impl BudgetState {
    pub fn fraction(&self) -> f32 {
        if self.total == 0 { 0.0 } else { self.used as f32 / self.total as f32 }
    }
}

// ── Agent run state ──────────────────────────────────────────────────────────
#[derive(Clone, PartialEq)]
pub enum AgentRunState { Idle, Running, AwaitingHil }

// ── HIL pending action ───────────────────────────────────────────────────────
#[derive(Clone)]
pub struct HilAction {
    pub title:      String,
    pub subtitle:   String,
    pub rows:       Vec<(String, String)>,
    pub total_secs: u32,
    pub started_at: f64,   // egui time
}

// ── Main application struct ──────────────────────────────────────────────────
pub struct KitsuneBrowser {
    // Navigation
    pub address_bar:   String,
    pub navigate_to:   Option<String>,

    // Agent
    pub agent_command: String,
    pub agent_state:   AgentRunState,
    pub agent_log:     Vec<LogEntry>,
    pub budget:        BudgetState,
    pub privacy:       PrivacyStats,

    // HIL gate
    pub hil_action:    Option<HilAction>,
    pub show_hil_dialog: bool,
    pub hil_dialog_open_time: Option<std::time::Instant>,

    // Tabs / CEF
    pub tabs:          Vec<Tab>,
    pub cef:           Option<CefBrowser>,
    pub cef_init_attempted: bool,
    pub cef_bounds:    Option<CefRect>,
}

impl KitsuneBrowser {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        KitsuneTheme::apply(&cc.egui_ctx);
        Self {
            address_bar:   "https://www.google.com".to_string(),
            navigate_to:   None,
            agent_command: String::new(),
            agent_state:   AgentRunState::Idle,
            agent_log:     Vec::new(),
            budget:        BudgetState::default(),
            privacy:       PrivacyStats { tls_version: "1.3", ..Default::default() },
            hil_action:    None,
            show_hil_dialog: false,
            hil_dialog_open_time: None,
            tabs:          vec![Tab::new(0, "New Tab".to_string())],
            cef:           None,
            cef_init_attempted: false,
            cef_bounds:    None,
        }
    }

    /// Append a line to the agent log (capped at 200 lines).
    pub fn push_log(&mut self, text: impl Into<String>, level: LogLevel) {
        self.agent_log.push(LogEntry::new(text, level));
        if self.agent_log.len() > 200 { self.agent_log.remove(0); }
    }

    /// Navigate the browser to a URL, normalising bare hosts.
    pub fn navigate(&mut self, url: &str) {
        let url = if url.starts_with("http://") || url.starts_with("https://")
                  || url.starts_with("about:") || url.starts_with("file://") {
            url.to_string()
        } else {
            format!("https://{url}")
        };
        self.address_bar = url.clone();
        self.navigate_to = Some(url.clone());
        for tab in &mut self.tabs {
            if tab.active { tab.navigate(&url); }
        }
    }
    
    /// Placeholder for processing agent messages (e.g. from background tasks)
    pub fn process_agent_messages(&mut self) {
        // TODO: Poll channel from agent runtime
    }
}

impl eframe::App for KitsuneBrowser {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Process any pending messages from the agent thread
        self.process_agent_messages();

        // ── 3-Pane Layout ──────────────────────────────────────────────────
        
        // 1. Left Panel (Agent Workspace)
        agent_panel(ctx, self);
        
        // 2. Right Panel (Session State)
        session_panel(ctx, self);

        // 3. Center Panel
        // The top bar renders *between* the side panels when called here.
        top_bar(ctx, self);

        // Central panel — egui placeholder while CEF is absent
        let central_rect = egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(KitsuneTheme::BG))
            .show(ctx, |ui| {
                if self.cef.is_none() {
                    ui.centered_and_justified(|ui| {
                        ui.label(
                            egui::RichText::new("🦊  KitsuneEngine")
                                .size(28.0)
                                .color(KitsuneTheme::ACCENT)
                                .strong(),
                        );
                    });
                }
            })
            .response
            .rect;

        // Init CEF on first frame (HWND available)
        if self.cef.is_none() && !self.cef_init_attempted && should_init_cef(central_rect) {
            #[cfg(target_os = "windows")]
            if let Ok(handle) = _frame.window_handle() {
                if let raw_window_handle::RawWindowHandle::Win32(h) = handle.as_raw() {
                    self.cef_init_attempted = true;
                    let r = central_rect;
                    let bounds = CefRect {
                        x: r.min.x as i32,
                        y: r.min.y as i32,
                        width: r.width() as u32,
                        height: r.height() as u32,
                    };
                    self.cef = CefBrowser::new(
                        h.hwnd.get() as isize,
                        &self.address_bar,
                        bounds,
                    )
                    .map_err(|e| tracing::error!(
                        x = bounds.x,
                        y = bounds.y,
                        width = bounds.width,
                        height = bounds.height,
                        "CEF init failed: {e}"
                    ))
                    .ok();
                    self.cef_bounds = self.cef.as_ref().map(|_| bounds);
                }
            }
        }

        if let Some(cef) = &self.cef {
            let bounds = CefRect {
                x: central_rect.min.x as i32,
                y: central_rect.min.y as i32,
                width: central_rect.width().max(1.0) as u32,
                height: central_rect.height().max(1.0) as u32,
            };

            if self.cef_bounds != Some(bounds) {
                cef.set_bounds(bounds);
                self.cef_bounds = Some(bounds);
            }
        }

        // Flush pending navigation
        if let Some(url) = self.navigate_to.take() {
            if let Some(cef) = &self.cef { cef.navigate(&url); }
        }

        // HIL dialog
        if self.show_hil_dialog { hil_dialog(ctx, self); }

        // Keep repainting while animated
        if self.agent_state == AgentRunState::Running || self.show_hil_dialog {
            ctx.request_repaint();
        }
    }
}

fn should_init_cef(rect: egui::Rect) -> bool {
    rect.width() >= 64.0 && rect.height() >= 64.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cef_init_waits_for_real_viewport() {
        let too_small = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(40.0, 200.0));
        let valid = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(400.0, 300.0));

        assert!(!should_init_cef(too_small));
        assert!(should_init_cef(valid));
    }
}
