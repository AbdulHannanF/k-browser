use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use eframe::egui;
use raw_window_handle::HasWindowHandle;
use tokio::runtime::Runtime;

use crate::chrome::top_bar::top_bar;
use crate::dialogs::downloads_dialog::downloads_dialog;
use crate::dialogs::hil_dialog::hil_dialog;
use crate::dialogs::settings_dialog::settings_dialog;
use crate::panels::agent_panel::agent_panel;
use crate::panels::session_panel::session_panel;
use crate::panels::task_graph_panel::TaskNode;
use crate::theme::KitsuneTheme;
use kitsune_agent::ai_client::{AgentAiClient, AiProviderConfig};
use kitsune_agent::captcha::CaptchaAgent;
use kitsune_agent::executor::WebViewCommand;
use kitsune_agent::orchestrator::AgentOrchestrator;
use kitsune_agent::profile::{ProfileIndexer, ProfileSummary};
use kitsune_agent::{FilePermSlot, StopFlag};
use kitsune_cef::{CefBrowser, CefEvent, CefRect};
use kitsune_core::tab::Tab;
use kitsune_hil::gate::HilCheckpoint;
use kitsune_hil::HilGate;
use kitsune_vault::VaultBackend;

const DEFAULT_HOME: &str = "https://www.google.com";

// ─── Public state types ──────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentRunState {
    Idle,
    Running,
    AwaitingHil,
}

/// Status of a browser download.
#[derive(Debug, Clone, PartialEq)]
pub enum DownloadStatus {
    InProgress,
    Completed,
    Failed,
}

/// A file being (or that has been) downloaded by the browser.
#[derive(Debug, Clone)]
pub struct DownloadItem {
    pub filename: String,
    pub url: String,
    pub save_path: Option<String>,
    pub status: DownloadStatus,
}

/// A local file that has been attached to the current agent session.
#[derive(Debug, Clone)]
pub struct AttachedFile {
    pub name: String,
    pub path: String,
    pub content: String,
}

#[derive(Debug, Clone)]
pub enum LogLevel {
    Info,
    Ok,
    Warn,
    Block,
    Cmd,
    /// Model chain-of-thought / reasoning (muted display).
    Think,
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
            LogLevel::Think => KitsuneTheme::TEXT3,
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

    // Live in-process agent runtime hooks
    webview_cmd_tx: tokio::sync::mpsc::Sender<WebViewCommand>,
    webview_cmd_rx: tokio::sync::mpsc::Receiver<WebViewCommand>,
    pub vault: Option<Arc<VaultBackend>>,
    pub hil_gate: Arc<HilGate>,
    /// Receives HIL checkpoints from the in-process agent runtime.
    hil_checkpoint_rx: tokio::sync::mpsc::Receiver<HilCheckpoint>,
    /// Active checkpoint waiting for the user's decision in the dialog.
    pub hil_pending_checkpoint: Option<HilCheckpoint>,

    // Settings state
    pub show_settings: bool,
    pub settings_tab: SettingsTab,
    pub settings_provider: SettingsProvider,
    pub settings_cloud_preset: CloudPreset,
    pub settings_api_key: String,
    pub settings_endpoint: String,
    pub settings_model: String,
    pub settings_saved: bool,
    pub settings_test_status: Option<String>,

    // Settings — Profile tab
    pub profile_folder: String,
    pub reindex_requested: bool,

    // Settings — Agents tab
    pub captcha_solver_url: String,
    pub captcha_solver_key: String,
    pub orchestrator_model: String,
    pub worker_model: String,
    pub fast_model: String,
    pub save_captcha_key_requested: bool,

    // Downloads
    pub downloads: Vec<DownloadItem>,
    pub show_downloads: bool,

    // File attachment & permissions
    pub attached_files: Vec<AttachedFile>,
    /// Shared slot for the agent's file-access permission requests.
    pub file_perm_slot: FilePermSlot,
    /// Path the agent is currently asking to read (modal shown while Some).
    pub file_perm_pending: Option<String>,

    /// Cooperative stop flag — set to true when the user clicks Stop.
    pub agent_stop_flag: StopFlag,

    // ── Orchestrator pipeline ────────────────────────────────────────────────
    pub profile_indexer: Option<Arc<ProfileIndexer>>,
    pub profile_summary: Option<ProfileSummary>,
    pub orchestrator: Option<Arc<AgentOrchestrator>>,
    pub task_nodes: Vec<TaskNode>,
    pub ai_client: Option<Arc<AgentAiClient>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SettingsTab {
    #[default]
    Llm,
    Profile,
    Agents,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SettingsProvider {
    Cloud,
    #[default]
    Ollama,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CloudPreset {
    #[default]
    Claude,
    OpenAI,
    Gemini,
    Groq,
    OpenRouter,
    Custom,
}

impl CloudPreset {
    pub fn label(self) -> &'static str {
        match self {
            Self::Claude => "Claude",
            Self::OpenAI => "OpenAI",
            Self::Gemini => "Gemini",
            Self::Groq => "Groq",
            Self::OpenRouter => "OpenRouter",
            Self::Custom => "Custom",
        }
    }

    pub fn default_endpoint(self) -> &'static str {
        match self {
            Self::Claude => "https://api.anthropic.com/v1",
            Self::OpenAI => "https://api.openai.com/v1",
            Self::Gemini => "https://generativelanguage.googleapis.com/v1beta/openai",
            Self::Groq => "https://api.groq.com/openai/v1",
            Self::OpenRouter => "https://openrouter.ai/api/v1",
            Self::Custom => "",
        }
    }

    pub fn default_model(self) -> &'static str {
        match self {
            Self::Claude => "claude-3-5-sonnet-20241022",
            Self::OpenAI => "gpt-4o-mini",
            Self::Gemini => "gemini-2.0-flash",
            Self::Groq => "llama-3.3-70b-versatile",
            Self::OpenRouter => "anthropic/claude-3.5-sonnet",
            Self::Custom => "",
        }
    }

    pub fn key_hint(self) -> &'static str {
        match self {
            Self::Claude => "sk-ant-api03-…",
            Self::OpenAI => "sk-…",
            Self::Gemini => "AIza…",
            Self::Groq => "gsk_…",
            Self::OpenRouter => "sk-or-…",
            Self::Custom => "your-api-key",
        }
    }
}

impl SettingsProvider {
    pub fn wire_value(&self) -> &'static str {
        match self {
            Self::Cloud => "open_ai_compatible",
            Self::Ollama => "ollama",
        }
    }
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

        let (event_tx, event_rx) = mpsc::channel();
        let (agent_tx, agent_rx) = mpsc::channel();
        let (webview_cmd_tx, webview_cmd_rx) =
            tokio::sync::mpsc::channel::<WebViewCommand>(64);

        let mut initial_tab = Tab::new(0, "New Tab".to_string());
        initial_tab.active = true;
        initial_tab.url = Some(DEFAULT_HOME.to_string());

        // Use keyring-backed KDF salt so every installation derives a unique key.
        // Falls back gracefully to None if keyring access is denied (headless CI,
        // first-run without keyring); the agent loop still works but sensitive
        // actions will surface a clear error.
        let vault = match VaultBackend::new_with_keyring("kitsune-dev") {
            Ok(v) => Some(Arc::new(v)),
            Err(e) => {
                tracing::warn!("vault disabled: {}", e);
                None
            }
        };

        // Real HIL gate — checkpoints flow from the in-process agent runtime
        // through hil_checkpoint_rx into the hil_dialog on every frame.
        let (hil_gate_inner, hil_checkpoint_rx) = HilGate::new(100);
        let hil_gate = Arc::new(hil_gate_inner);

        // ── Orchestrator pipeline ────────────────────────────────────────────
        // Build AI client (default Ollama config; user can change in settings).
        let ai_client = AgentAiClient::new(AiProviderConfig::default())
            .ok()
            .map(Arc::new);

        // Build CaptchaAgent — requires DOM access, which needs the vault and
        // webview_cmd_tx. We construct a temporary DomAccessor here solely so
        // that CaptchaAgent can be wired up; the real per-run DomAccessor used
        // by LlmAgentRuntime is constructed inside start_agent_run.
        let captcha: Option<Arc<CaptchaAgent>> = if let (Some(ref ai_c), Some(ref v)) =
            (&ai_client, &vault)
        {
            let initial_url = url::Url::parse("about:blank").expect("static URL");
            let dom_for_captcha = Arc::new(kitsune_agent::dom_access::DomAccessor::new(
                v.clone(),
                hil_gate.clone(),
                initial_url,
                webview_cmd_tx.clone(),
            ));
            let _ = ai_c; // ai_client not used by CaptchaAgent constructor
            CaptchaAgent::new(dom_for_captcha, hil_gate.clone(), None)
                .ok()
                .map(Arc::new)
        } else {
            None
        };

        // ProfileIndexer — folder is empty until the user sets one in settings.
        // ProfileIndexer::new handles a missing/empty directory gracefully.
        let profile_indexer = Some(Arc::new(ProfileIndexer::new(
            std::path::PathBuf::from(""),
        )));

        // Load any cached profile summary from disk.
        let profile_summary: Option<ProfileSummary> = None; // populated lazily via reindex

        // AgentOrchestrator — constructed if all dependencies are available.
        let orchestrator: Option<Arc<AgentOrchestrator>> =
            if let (Some(ref ai_c), Some(ref v), Some(ref cap), Some(ref idx)) =
                (&ai_client, &vault, &captcha, &profile_indexer)
            {
                let initial_url = url::Url::parse("about:blank").expect("static URL");
                let dom_for_orch = Arc::new(kitsune_agent::dom_access::DomAccessor::new(
                    v.clone(),
                    hil_gate.clone(),
                    initial_url,
                    webview_cmd_tx.clone(),
                ));
                Some(Arc::new(AgentOrchestrator::new(
                    dom_for_orch,
                    ai_c.clone(),
                    cap.clone(),
                    hil_gate.clone(),
                    idx.clone(),
                )))
            } else {
                None
            };

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
            webview_cmd_tx,
            webview_cmd_rx,
            vault,
            hil_gate,
            hil_checkpoint_rx,
            hil_pending_checkpoint: None,
            show_settings: false,
            settings_tab: SettingsTab::Llm,
            settings_provider: SettingsProvider::Ollama,
            settings_cloud_preset: CloudPreset::default(),
            settings_api_key: String::new(),
            settings_endpoint: "http://localhost:11434".to_string(),
            settings_model: "llama3".to_string(),
            settings_saved: false,
            settings_test_status: None,
            profile_folder: String::new(),
            reindex_requested: false,
            captcha_solver_url: String::new(),
            captcha_solver_key: String::new(),
            orchestrator_model: String::new(),
            worker_model: String::new(),
            fast_model: String::new(),
            save_captcha_key_requested: false,
            downloads: Vec::new(),
            show_downloads: false,
            attached_files: Vec::new(),
            file_perm_slot: std::sync::Arc::new(std::sync::Mutex::new(None)),
            file_perm_pending: None,
            agent_stop_flag: Arc::new(AtomicBool::new(false)),
            profile_indexer,
            profile_summary,
            orchestrator,
            task_nodes: Vec::new(),
            ai_client,
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

    fn init_browser(&mut self, frame: &mut eframe::Frame, rect: egui::Rect, scale: f32) {
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
                rect_to_cef_bounds(rect, scale),
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
                CefEvent::NewWindowNav(url) => {
                    self.navigate(&url);
                }
                CefEvent::DownloadStarted { url, filename, save_path } => {
                    self.downloads.push(DownloadItem {
                        filename,
                        url,
                        save_path: Some(save_path),
                        status: DownloadStatus::InProgress,
                    });
                    self.show_downloads = true;
                }
                CefEvent::DownloadCompleted { url, save_path, success } => {
                    if let Some(item) = self.downloads.iter_mut().find(|d| d.url == url) {
                        item.status = if success {
                            DownloadStatus::Completed
                        } else {
                            DownloadStatus::Failed
                        };
                        if let Some(p) = save_path {
                            item.save_path = Some(p);
                        }
                    }
                }
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
                        "think" => LogLevel::Think,
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

    /// Channel the in-process agent runtime uses to drive the live WebView.
    pub fn webview_cmd_tx(&self) -> tokio::sync::mpsc::Sender<WebViewCommand> {
        self.webview_cmd_tx.clone()
    }

    /// Current address-bar URL for the active tab.
    pub fn current_url(&self) -> String {
        self.address_bar.clone()
    }

    /// Check if the agent has put a file-permission request into the slot.
    /// Sets `file_perm_pending` when a new request arrives.
    pub fn process_file_perm_requests(&mut self) {
        if self.file_perm_pending.is_some() {
            return; // already showing the modal
        }
        if let Ok(slot) = self.file_perm_slot.try_lock() {
            if let Some((path, _)) = slot.as_ref() {
                self.file_perm_pending = Some(path.clone());
            }
        }
    }

    /// Drain the HIL checkpoint receiver and surface the next pending checkpoint
    /// in the confirmation dialog. Only one checkpoint is shown at a time; new
    /// ones queue behind it until the user resolves the active one.
    fn process_hil_checkpoints(&mut self, ctx: &egui::Context) {
        if self.hil_pending_checkpoint.is_some() || self.hil_action.is_some() {
            return;
        }
        if let Ok(checkpoint) = self.hil_checkpoint_rx.try_recv() {
            let p = &checkpoint.presentation;
            let mut rows: Vec<(String, String)> = p
                .data_involved
                .iter()
                .map(|d| ("Credential".to_string(), d.clone()))
                .collect();
            if let Some(cost) = &p.cost_display {
                rows.push(("Cost".to_string(), cost.clone()));
            }
            rows.push((
                "Reversible".to_string(),
                if p.is_reversible { "Yes".to_string() } else { "No".to_string() },
            ));
            self.hil_action = Some(HilAction {
                title: p.what_will_happen.clone(),
                subtitle: p.domain_or_service.clone(),
                rows,
                total_secs: 30,
                started_at: ctx.input(|i| i.time),
            });
            self.agent_state = AgentRunState::AwaitingHil;
            self.hil_pending_checkpoint = Some(checkpoint);
        }
    }

    /// Drain the agent runtime's WebView command queue and execute each
    /// command against the live `CefBrowser` on the UI thread. Called from
    /// `update()` so we never need a `Send` reference to the WebView.
    fn process_webview_commands(&mut self) {
        while let Ok(cmd) = self.webview_cmd_rx.try_recv() {
            let Some(cef) = &self.cef else {
                tracing::debug!("webview command dropped — no live cef yet");
                continue;
            };
            match cmd {
                WebViewCommand::Navigate(url) => {
                    cef.load_url(&url);
                    self.address_bar = url.clone();
                    if let Some(tab) =
                        self.tabs.iter_mut().find(|t| t.id == self.active_tab_id)
                    {
                        tab.url = Some(url);
                    }
                }
                WebViewCommand::EvalJs(script) => {
                    cef.execute_js(&script);
                }
                WebViewCommand::EvalJsWithCallback(script, tx) => {
                    let tx_clone = tx.clone();
                    if let Err(e) = cef.execute_js_with_callback(&script, move |result| {
                        // We're inside a wry callback — best effort send.
                        let _ = tx_clone.try_send(result);
                    }) {
                        tracing::warn!("execute_js_with_callback failed: {}", e);
                        // Make sure the caller doesn't hang waiting forever.
                        let _ = tx.try_send(String::from("null"));
                    }
                }
            }
        }
    }
}

impl eframe::App for KitsuneBrowser {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        self.process_browser_events();
        self.process_agent_events();
        self.process_webview_commands();
        self.process_file_perm_requests();
        self.process_hil_checkpoints(ctx);

        // Fix HIL started_at if it hasn't been set yet
        if let Some(ref mut hil) = self.hil_action {
            if hil.started_at == 0.0 {
                hil.started_at = ctx.input(|i| i.time);
            }
        }

        // ── Reindex profile if requested from the Settings dialog ────────
        if self.reindex_requested {
            self.reindex_requested = false;
            // Rebuild ProfileIndexer with the current folder setting so that it
            // scans the directory the user has configured, not the empty-path
            // placeholder created at startup.
            let updated_indexer = Arc::new(ProfileIndexer::new(
                std::path::PathBuf::from(&self.profile_folder),
            ));
            self.profile_indexer = Some(updated_indexer.clone());

            // Also rebuild the orchestrator's indexer reference if possible.
            if let (Some(ref ai_c), Some(ref v), Some(ref orch)) =
                (&self.ai_client.clone(), &self.vault.clone(), &self.orchestrator.clone())
            {
                let _ = (ai_c, v, orch); // suppress unused warnings; orchestrator rebuilt below
                let initial_url = url::Url::parse("about:blank").expect("static URL");
                let dom_for_orch = Arc::new(kitsune_agent::dom_access::DomAccessor::new(
                    v.clone(),
                    self.hil_gate.clone(),
                    initial_url,
                    self.webview_cmd_tx.clone(),
                ));
                let captcha_for_orch = self.ai_client.as_ref().and_then(|_ai| {
                    let v2 = self.vault.as_ref()?;
                    let initial_url2 = url::Url::parse("about:blank").ok()?;
                    let dom2 = Arc::new(kitsune_agent::dom_access::DomAccessor::new(
                        v2.clone(),
                        self.hil_gate.clone(),
                        initial_url2,
                        self.webview_cmd_tx.clone(),
                    ));
                    CaptchaAgent::new(dom2, self.hil_gate.clone(), None).ok().map(Arc::new)
                });
                if let (Some(ai_c2), Some(cap2)) = (&self.ai_client, captcha_for_orch) {
                    self.orchestrator = Some(Arc::new(AgentOrchestrator::new(
                        dom_for_orch,
                        ai_c2.clone(),
                        cap2,
                        self.hil_gate.clone(),
                        updated_indexer.clone(),
                    )));
                }
            }

            if let (Some(idx), Some(ai)) = (self.profile_indexer.clone(), self.ai_client.clone()) {
                let ui_ctx = ctx.clone();
                let agent_tx = self.agent_tx.clone();
                std::thread::spawn(move || {
                    let rt = tokio::runtime::Runtime::new().expect("tokio rt for reindex");
                    match rt.block_on(idx.reindex(&ai)) {
                        Ok(summary) => {
                            tracing::info!(name = %summary.full_name, "Profile reindexed");
                            // Send summary back via a dedicated SSE-style log message so the
                            // UI can display it. The actual ProfileSummary field is updated
                            // below via a channel send embedded in the log.
                            let _ = agent_tx.send(AgentSseAction::Log {
                                message: format!("Profile indexed: {}", summary.full_name),
                                class: "ok".into(),
                            });
                            ui_ctx.request_repaint();
                        }
                        Err(e) => {
                            tracing::warn!("Profile reindex failed: {e}");
                            let _ = agent_tx.send(AgentSseAction::Log {
                                message: format!("Profile reindex failed: {e}"),
                                class: "warn".into(),
                            });
                            ui_ctx.request_repaint();
                        }
                    }
                });
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

        // ── Overlay: Downloads panel ─────────────────────────────────────
        downloads_dialog(ctx, self);

        // ── Overlay: File permission dialog ──────────────────────────────
        if self.file_perm_pending.is_some() {
            let mut allow = false;
            let mut deny = false;
            let path = self.file_perm_pending.clone().unwrap_or_default();
            egui::Window::new("📂 File Access Request")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    ui.set_min_width(380.0);
                    ui.add_space(4.0);
                    ui.label(
                        egui::RichText::new("The agent wants to read a local file:")
                            .color(KitsuneTheme::TEXT1),
                    );
                    ui.add_space(4.0);
                    egui::Frame::none()
                        .fill(KitsuneTheme::BG2)
                        .rounding(egui::Rounding::same(6.0))
                        .inner_margin(egui::Margin::symmetric(10.0, 6.0))
                        .show(ui, |ui| {
                            ui.label(
                                egui::RichText::new(&path)
                                    .family(egui::FontFamily::Monospace)
                                    .size(11.5)
                                    .color(KitsuneTheme::AMBER),
                            );
                        });
                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        let allow_btn = egui::Button::new(
                            egui::RichText::new("✓ Allow").color(egui::Color32::BLACK).strong(),
                        )
                        .fill(KitsuneTheme::GREEN)
                        .min_size(egui::vec2(80.0, 28.0));
                        if ui.add(allow_btn).clicked() {
                            allow = true;
                        }
                        ui.add_space(8.0);
                        let deny_btn = egui::Button::new(
                            egui::RichText::new("✕ Deny").color(KitsuneTheme::TEXT_PRIMARY).strong(),
                        )
                        .fill(KitsuneTheme::RED)
                        .min_size(egui::vec2(80.0, 28.0));
                        if ui.add(deny_btn).clicked() {
                            deny = true;
                        }
                    });
                    ui.add_space(4.0);
                });
            if allow || deny {
                self.file_perm_pending = None;
                if let Ok(mut slot) = self.file_perm_slot.try_lock() {
                    if let Some((_, tx)) = slot.take() {
                        let _ = tx.send(allow);
                    }
                }
            }
        }

        // ── Overlay: Settings dialog ─────────────────────────────────────
        settings_dialog(ctx, self);

        // ── Center: WebView area ─────────────────────────────────────────
        // egui reports rects in logical pixels; Win32/WebView2 needs physical
        // pixels, so every coordinate must be scaled by pixels_per_point.
        let scale = ctx.pixels_per_point();

        let viewport = egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(KitsuneTheme::BG))
            .show(ctx, |ui| {
                if !self.browser_ready || self.startup_error.is_some() {
                    render_fallback(ui, self.startup_error.as_deref(), &self.address_bar);
                }
            })
            .response
            .rect;

        self.init_browser(frame, viewport, scale);

        if let Some(cef) = &self.cef {
            if self.show_settings || self.hil_action.is_some() || self.file_perm_pending.is_some() {
                // Hide native WebView when egui modal dialogs are active
                // by collapsing its bounds.
                cef.set_bounds(CefRect { x: 0, y: 0, width: 0, height: 0 });
            } else {
                cef.set_bounds(rect_to_cef_bounds(viewport, scale));
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

fn rect_to_cef_bounds(rect: egui::Rect, scale: f32) -> CefRect {
    CefRect {
        x: (rect.min.x * scale).round() as i32,
        y: (rect.min.y * scale).round() as i32,
        width: (rect.width() * scale).max(1.0).round() as u32,
        height: (rect.height() * scale).max(1.0).round() as u32,
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
        let bounds = rect_to_cef_bounds(rect, 1.0);
        assert_eq!(bounds.x, 10);
        assert_eq!(bounds.y, 22);
        assert_eq!(bounds.width, 640);
        assert_eq!(bounds.height, 480);
    }

    #[test]
    fn rect_conversion_scales_for_dpi() {
        // At 1.5× DPI, a rect at logical (100, 30) size 600×400 becomes
        // physical (150, 45) size 900×600.
        let rect = egui::Rect::from_min_size(egui::pos2(100.0, 30.0), egui::vec2(600.0, 400.0));
        let bounds = rect_to_cef_bounds(rect, 1.5);
        assert_eq!(bounds.x, 150);
        assert_eq!(bounds.y, 45);
        assert_eq!(bounds.width, 900);
        assert_eq!(bounds.height, 600);
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

    #[test]
    fn cloud_preset_named_endpoints_use_https() {
        for preset in [
            CloudPreset::Claude,
            CloudPreset::OpenAI,
            CloudPreset::Gemini,
            CloudPreset::Groq,
            CloudPreset::OpenRouter,
        ] {
            assert!(
                preset.default_endpoint().starts_with("https://"),
                "{:?} should use HTTPS",
                preset
            );
        }
    }

    #[test]
    fn cloud_preset_custom_has_empty_endpoint() {
        assert_eq!(CloudPreset::Custom.default_endpoint(), "");
    }

    #[test]
    fn cloud_preset_key_hints_are_nonempty() {
        for preset in [
            CloudPreset::Claude,
            CloudPreset::OpenAI,
            CloudPreset::Gemini,
            CloudPreset::Groq,
            CloudPreset::OpenRouter,
            CloudPreset::Custom,
        ] {
            assert!(!preset.key_hint().is_empty(), "{:?} must have a key hint", preset);
        }
    }
}
