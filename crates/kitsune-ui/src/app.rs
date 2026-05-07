use std::sync::mpsc::{self, Receiver, Sender};

use eframe::egui;
use raw_window_handle::HasWindowHandle;
use tokio::runtime::Runtime;

use crate::chrome::top_bar::top_bar;
use crate::dialogs::hil_dialog::hil_dialog;
use crate::dialogs::settings_dialog::settings_dialog;
use crate::panels::agent_panel::agent_panel;
use crate::panels::session_panel::session_panel;
use crate::theme::KitsuneTheme;
use kitsune_cef::{CefBrowser, CefEvent, CefRect};
use kitsune_core::tab::Tab;

const DEFAULT_HOME: &str = "https://www.google.com";

// ─── Public state types ──────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentRunState {
    Idle,
    Running,
    AwaitingHil,
}

#[derive(Debug, Clone)]
pub enum LogLevel {
    Info,
    Ok,
    Warn,
    Block,
    Cmd,
}

#[derive(Debug, Clone)]
pub struct LogEntry {
    pub text: String,
    pub level: LogLevel,
}

impl LogEntry {
    pub fn color(&self) -> egui::Color32 {
        match self.level {
            LogLevel::Info => KitsuneTheme::TEXT1,
            LogLevel::Ok => KitsuneTheme::GREEN,
            LogLevel::Warn => KitsuneTheme::AMBER,
            LogLevel::Block => KitsuneTheme::RED,
            LogLevel::Cmd => KitsuneTheme::BLUE,
        }
    }
}

#[derive(Debug, Clone)]
pub struct HilAction {
    pub title: String,
    pub subtitle: String,
    pub rows: Vec<(String, String)>,
    pub total_secs: u32,
    pub started_at: f64,
}

#[derive(Debug, Clone)]
pub struct PrivacyState {
    pub trackers_blocked: u32,
    pub referrers_stripped: u32,
    pub tls_version: &'static str,
}

impl Default for PrivacyState {
    fn default() -> Self {
        Self {
            trackers_blocked: 0,
            referrers_stripped: 0,
            tls_version: "TLS 1.3",
        }
    }
}

#[derive(Debug, Clone)]
pub struct BudgetState {
    pub used: u32,
    pub total: u32,
}

impl Default for BudgetState {
    fn default() -> Self {
        Self {
            used: 0,
            total: 100,
        }
    }
}

impl BudgetState {
    pub fn fraction(&self) -> f32 {
        if self.total == 0 {
            0.0
        } else {
            self.used as f32 / self.total as f32
        }
    }
}

// ─── Main browser struct ─────────────────────────────────────────────────────

pub struct KitsuneBrowser {
    // Browser chrome
    pub tabs: Vec<Tab>,
    pub address_bar: String,
    pub cef: Option<CefBrowser>,
    browser_ready: bool,

    // Agent workspace
    pub agent_command: String,
    pub agent_state: AgentRunState,
    pub agent_log: Vec<LogEntry>,
    pub hil_action: Option<HilAction>,
    pub budget: BudgetState,
    pub privacy: PrivacyState,

    // Internal
    startup_error: Option<String>,
    _runtime: Runtime,
    event_tx: Sender<CefEvent>,
    event_rx: Receiver<CefEvent>,
    active_tab_id: usize,
    next_tab_id: usize,

    // Agent SSE channel
    agent_rx: Receiver<AgentSseAction>,
    agent_tx: Sender<AgentSseAction>,

    // Settings state
    pub show_settings: bool,
    pub settings_api_key: String,
    pub settings_endpoint: String,
    pub settings_model: String,
    pub settings_saved: bool,
}

/// Actions received from the agent SSE stream
#[derive(Debug, Clone)]
pub enum AgentSseAction {
    Log { message: String, class: String },
    AgentStatus { agent: String, status: String },
    TrackerBlocked { label: String, stripped: bool },
    HilRequest { action: String, flight: String, date: String, passenger: String, total: String, credentials: String },
    HilApproved,
    HilCancelled,
    UrlUpdate { url: String },
    Done { message: String },
}

impl KitsuneBrowser {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        KitsuneTheme::apply(&cc.egui_ctx);

        let runtime = Runtime::new().expect("tokio runtime");

        // Start the mock server
        runtime.spawn(async {
            if let Err(err) = kitsune_cloud_mock::start("127.0.0.1:7700").await {
                tracing::warn!("mock server startup skipped: {}", err);
            }
        });

        let (event_tx, event_rx) = mpsc::channel();
        let (agent_tx, agent_rx) = mpsc::channel();

        let mut initial_tab = Tab::new(0, "New Tab".to_string());
        initial_tab.active = true;
        initial_tab.url = Some(DEFAULT_HOME.to_string());

        Self {
            tabs: vec![initial_tab],
            address_bar: DEFAULT_HOME.to_string(),
            cef: None,
            browser_ready: false,
            agent_command: String::new(),
            agent_state: AgentRunState::Idle,
            agent_log: Vec::new(),
            hil_action: None,
            budget: BudgetState::default(),
            privacy: PrivacyState::default(),
            startup_error: None,
            _runtime: runtime,
            event_tx,
            event_rx,
            active_tab_id: 0,
            next_tab_id: 1,
            agent_rx,
            agent_tx,
            show_settings: false,
            settings_api_key: String::new(),
            settings_endpoint: "https://api.openai.com/v1/chat/completions".to_string(),
            settings_model: "gpt-4o-mini".to_string(),
            settings_saved: false,
        }
    }

    /// Navigate the active tab's WebView to a URL.
    pub fn navigate(&mut self, url: &str) {
        let url = normalize_url(url);
        self.address_bar = url.clone();

        // Update the tab model
        if let Some(tab) = self.tabs.iter_mut().find(|t| t.id == self.active_tab_id) {
            tab.navigate(&url);
        }

        // Navigate the WebView
        if let Some(cef) = &self.cef {
            cef.load_url(&url);
        }
    }

    /// Open a new tab.
    pub fn new_tab(&mut self) {
        // Deactivate all tabs
        for tab in &mut self.tabs {
            tab.active = false;
        }
        let id = self.next_tab_id;
        self.next_tab_id += 1;
        let mut tab = Tab::new(id, "New Tab".to_string());
        tab.active = true;
        tab.url = Some(DEFAULT_HOME.to_string());
        self.tabs.push(tab);
        self.active_tab_id = id;
        self.address_bar = DEFAULT_HOME.to_string();

        if let Some(cef) = &self.cef {
            cef.load_url(DEFAULT_HOME);
        }
    }

    /// Switch to a tab by ID.
    pub fn switch_tab(&mut self, id: usize) {
        if id == self.active_tab_id {
            return;
        }
        for tab in &mut self.tabs {
            tab.active = tab.id == id;
        }
        self.active_tab_id = id;

        // Update address bar and navigate WebView to this tab's URL
        if let Some(tab) = self.tabs.iter().find(|t| t.id == id) {
            let url = tab.url.clone().unwrap_or_else(|| DEFAULT_HOME.to_string());
            self.address_bar = url.clone();
            if let Some(cef) = &self.cef {
                cef.load_url(&url);
            }
        }
    }

    /// Close a tab by ID.
    pub fn close_tab(&mut self, id: usize) {
        if self.tabs.len() <= 1 {
            return; // Don't close the last tab
        }
        self.tabs.retain(|t| t.id != id);
        if id == self.active_tab_id {
            // Activate the last tab
            if let Some(last) = self.tabs.last_mut() {
                last.active = true;
                self.active_tab_id = last.id;
                let url = last.url.clone().unwrap_or_else(|| DEFAULT_HOME.to_string());
                self.address_bar = url.clone();
                if let Some(cef) = &self.cef {
                    cef.load_url(&url);
                }
            }
        }
    }

    /// Add a log entry to the agent log.
    pub fn push_log(&mut self, text: impl Into<String>, level: LogLevel) {
        self.agent_log.push(LogEntry {
            text: text.into(),
            level,
        });
    }

    /// Get a reference to the WebView (for top_bar navigation buttons).
    pub fn webview(&self) -> Option<&CefBrowser> {
        self.cef.as_ref()
    }

    fn init_browser(&mut self, frame: &mut eframe::Frame, rect: egui::Rect) {
        if self.cef.is_some() || self.startup_error.is_some() || !should_init_cef(rect) {
            return;
        }

        #[cfg(target_os = "windows")]
        {
            let handle = match frame.window_handle() {
                Ok(handle) => handle,
                Err(err) => {
                    self.startup_error =
                        Some(format!("Failed to resolve native window handle: {err}"));
                    return;
                }
            };

            let raw = handle.as_raw();
            let raw_window_handle::RawWindowHandle::Win32(win32) = raw else {
                self.startup_error = Some("Kitsune UI requires a Win32 window handle.".to_string());
                return;
            };

            let url = self.tabs.first()
                .and_then(|t| t.url.as_deref())
                .unwrap_or(DEFAULT_HOME);

            match CefBrowser::new(
                win32.hwnd.get() as isize,
                url,
                rect_to_cef_bounds(rect),
                Some(self.event_tx.clone()),
            ) {
                Ok(browser) => {
                    self.cef = Some(browser);
                    self.browser_ready = true;
                }
                Err(err) => {
                    self.startup_error = Some(format!("Failed to create WebView host: {err}"));
                }
            }
        }
    }

    fn process_browser_events(&mut self) {
        while let Ok(event) = self.event_rx.try_recv() {
            match event {
                CefEvent::PageLoadStarted(url) => {
                    self.address_bar = url.clone();
                    if let Some(tab) = self.tabs.iter_mut().find(|t| t.id == self.active_tab_id) {
                        tab.url = Some(url);
                        tab.is_loading = true;
                    }
                }
                CefEvent::PageLoadFinished(url) => {
                    self.address_bar = url.clone();
                    if let Some(tab) = self.tabs.iter_mut().find(|t| t.id == self.active_tab_id) {
                        tab.url = Some(url);
                        tab.is_loading = false;
                        tab.state = kitsune_core::tab::TabState::Loaded;
                    }
                }
                CefEvent::TitleChanged(title) => {
                    if let Some(tab) = self.tabs.iter_mut().find(|t| t.id == self.active_tab_id) {
                        if !title.is_empty() {
                            tab.title = title;
                        }
                    }
                }
                CefEvent::IpcMessage(_msg) => {}
            }
        }
    }

    fn process_agent_events(&mut self) {
        while let Ok(action) = self.agent_rx.try_recv() {
            match action {
                AgentSseAction::Log { message, class } => {
                    let level = match class.as_str() {
                        "ok" => LogLevel::Ok,
                        "warn" => LogLevel::Warn,
                        "block" => LogLevel::Block,
                        "cmd" => LogLevel::Cmd,
                        _ => LogLevel::Info,
                    };
                    self.push_log(message, level);
                }
                AgentSseAction::AgentStatus { .. } => {
                    // Agent card status updates handled via log for now
                }
                AgentSseAction::TrackerBlocked { label, stripped } => {
                    self.privacy.trackers_blocked += 1;
                    if stripped {
                        self.privacy.referrers_stripped += 1;
                    }
                    self.push_log(format!("🚫 Blocked: {}", label), LogLevel::Block);
                }
                AgentSseAction::HilRequest { action, flight, date, passenger, total, credentials } => {
                    self.agent_state = AgentRunState::AwaitingHil;
                    self.hil_action = Some(HilAction {
                        title: "Agent wants to complete an action".to_string(),
                        subtitle: "Review the action below before it executes.".to_string(),
                        rows: vec![
                            ("Action".into(), action),
                            ("Flight".into(), flight),
                            ("Date".into(), date),
                            ("Passenger".into(), passenger),
                            ("Total".into(), total),
                            ("Credentials".into(), credentials),
                        ],
                        total_secs: 30,
                        started_at: 0.0, // will be set in hil_dialog
                    });
                }
                AgentSseAction::HilApproved => {
                    self.push_log("✓  Action approved and executed", LogLevel::Ok);
                    self.push_log("✓  Audit trail written to vault", LogLevel::Ok);
                    self.push_log("✓  Session cleared · no credentials retained", LogLevel::Ok);
                    self.agent_state = AgentRunState::Idle;
                    self.budget.used += 1;
                }
                AgentSseAction::HilCancelled => {
                    self.push_log("✕  Agent action was cancelled", LogLevel::Warn);
                    self.agent_state = AgentRunState::Idle;
                }
                AgentSseAction::UrlUpdate { url } => {
                    self.navigate(&url);
                }
                AgentSseAction::Done { message } => {
                    if !message.is_empty() {
                        self.push_log(message, LogLevel::Ok);
                    }
                    self.agent_state = AgentRunState::Idle;
                }
            }
        }
    }

    /// Get a clone of the agent TX for spawning agent tasks
    pub fn agent_tx(&self) -> Sender<AgentSseAction> {
        self.agent_tx.clone()
    }

    /// Get a handle to the tokio runtime for spawning async tasks
    pub fn runtime(&self) -> &Runtime {
        &self._runtime
    }
}

impl eframe::App for KitsuneBrowser {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        self.process_browser_events();
        self.process_agent_events();

        // Fix HIL started_at if it hasn't been set yet
        if let Some(ref mut hil) = self.hil_action {
            if hil.started_at == 0.0 {
                hil.started_at = ctx.input(|i| i.time);
            }
        }

        // ── Chrome: Top bar ──────────────────────────────────────────────
        top_bar(ctx, self);

        // ── Left panel: Agent workspace ──────────────────────────────────
        agent_panel(ctx, self);

        // ── Right panel: Session info ────────────────────────────────────
        session_panel(ctx, self);

        // ── Overlay: HIL dialog ──────────────────────────────────────────
        hil_dialog(ctx, self);

        // ── Overlay: Settings dialog ─────────────────────────────────────
        settings_dialog(ctx, self);

        // ── Center: WebView area ─────────────────────────────────────────
        let viewport = egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(KitsuneTheme::BG))
            .show(ctx, |ui| {
                if !self.browser_ready || self.startup_error.is_some() {
                    render_fallback(ui, self.startup_error.as_deref(), &self.address_bar);
                }
            })
            .response
            .rect;

        self.init_browser(frame, viewport);

        if let Some(cef) = &self.cef {
            if self.show_settings || self.hil_action.is_some() {
                // Hide native WebView when egui modal dialogs are active
                // by collapsing its bounds.
                cef.set_bounds(CefRect { x: 0, y: 0, width: 0, height: 0 });
            } else {
                cef.set_bounds(rect_to_cef_bounds(viewport));
            }
        }

        // ── Focus management ─────────────────────────────────────────────
        // The WebView2 child window captures ALL keyboard input at the OS
        // level. We must detect when the user interacts with egui widgets
        // and redirect focus back to the eframe parent HWND.
        if let Some(cef) = &self.cef {
            let has_egui_focus = ctx.memory(|m| m.focused().is_some());
            let pointer_pos = ctx.input(|i| i.pointer.latest_pos());
            let clicked = ctx.input(|i| i.pointer.any_pressed());

            if has_egui_focus {
                // An egui TextEdit or other input widget has focus —
                // redirect keyboard to the parent window
                cef.unfocus();
            } else if clicked {
                if let Some(pos) = pointer_pos {
                    if viewport.contains(pos) {
                        // Clicked inside the WebView area — let it have focus
                        cef.focus();
                    } else {
                        // Clicked on a panel/toolbar — keep focus on parent
                        cef.unfocus();
                    }
                }
            }
        }

        ctx.request_repaint_after(std::time::Duration::from_millis(16));
    }
}

// ─── Helper functions ────────────────────────────────────────────────────────

fn normalize_url(input: &str) -> String {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return DEFAULT_HOME.to_string();
    }
    if trimmed.starts_with("http://") || trimmed.starts_with("https://") || trimmed.starts_with("file://") {
        return trimmed.to_string();
    }
    // Check if it looks like a domain
    if trimmed.contains('.') && !trimmed.contains(' ') {
        return format!("https://{}", trimmed);
    }
    // Otherwise treat as a search query
    format!("https://www.google.com/search?q={}", urlencoding::encode(trimmed))
}

fn render_fallback(ui: &mut egui::Ui, startup_error: Option<&str>, current_url: &str) {
    ui.centered_and_justified(|ui| {
        egui::Frame::none()
            .fill(KitsuneTheme::BG2)
            .stroke(egui::Stroke::new(1.0, KitsuneTheme::BORDER_AMBER))
            .rounding(egui::Rounding::same(16.0))
            .inner_margin(egui::Margin::same(24.0))
            .show(ui, |ui| {
                ui.set_max_width(560.0);
                ui.vertical_centered(|ui| {
                    ui.label(
                        egui::RichText::new("🦊 KitsuneEngine")
                            .size(28.0)
                            .strong()
                            .color(KitsuneTheme::AMBER),
                    );
                    ui.add_space(8.0);
                    ui.label(
                        egui::RichText::new(startup_error.unwrap_or(
                            "Initializing WebView2 runtime…",
                        ))
                        .size(14.0)
                        .color(KitsuneTheme::TEXT1),
                    );
                    ui.add_space(10.0);
                    ui.label(
                        egui::RichText::new(current_url)
                            .size(12.0)
                            .family(egui::FontFamily::Monospace)
                            .color(KitsuneTheme::TEXT2),
                    );
                });
            });
    });
}

fn rect_to_cef_bounds(rect: egui::Rect) -> CefRect {
    CefRect {
        x: rect.min.x.round() as i32,
        y: rect.min.y.round() as i32,
        width: rect.width().max(1.0).round() as u32,
        height: rect.height().max(1.0).round() as u32,
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

    #[test]
    fn rect_conversion_preserves_geometry() {
        let rect = egui::Rect::from_min_size(egui::pos2(10.4, 21.6), egui::vec2(639.6, 479.5));
        let bounds = rect_to_cef_bounds(rect);
        assert_eq!(bounds.x, 10);
        assert_eq!(bounds.y, 22);
        assert_eq!(bounds.width, 640);
        assert_eq!(bounds.height, 480);
    }

    #[test]
    fn normalize_url_adds_https_to_domains() {
        assert_eq!(normalize_url("google.com"), "https://google.com");
        assert_eq!(normalize_url("example.org/path"), "https://example.org/path");
    }

    #[test]
    fn normalize_url_preserves_existing_scheme() {
        assert_eq!(normalize_url("https://foo.com"), "https://foo.com");
        assert_eq!(normalize_url("http://bar.com"), "http://bar.com");
    }

    #[test]
    fn normalize_url_searches_plain_text() {
        let result = normalize_url("hello world");
        assert!(result.starts_with("https://www.google.com/search?q="));
        assert!(result.contains("hello"));
    }

    #[test]
    fn budget_fraction_correct() {
        let budget = BudgetState { used: 35, total: 100 };
        assert!((budget.fraction() - 0.35).abs() < f32::EPSILON);
    }
}
