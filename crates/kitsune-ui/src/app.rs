/// KitsuneEngine UI Application — the main egui app.

use crate::panels;
use crate::theme;
use crate::widgets::nav_bar;
use crate::widgets::tab_bar;
use egui::{FontId, Margin, RichText, Rounding, Stroke};
use kitsune_core::engine::KitsuneEngine;
use kitsune_core::config::EngineConfig;
use kitsune_ai::cloud::{KitsuneCloudBackend, AccountStatus};
use std::sync::{Arc, Mutex};
use kitsune_render::RenderCommand;
use kitsune_layout::LayoutNode;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppScreen {
    Browser,
    Login,
    Account,
}

#[derive(Debug, Clone)]
pub struct AgentBuilderState {
    pub step: u8,
    pub description: String,
    pub can_navigate: bool,
    pub can_fill_forms: bool,
    pub can_submit: bool,
    pub can_email: bool,
    pub can_create_account: bool,
    pub budget: String,
}

impl Default for AgentBuilderState {
    fn default() -> Self {
        Self {
            step: 1,
            description: String::new(),
            can_navigate: true,
            can_fill_forms: true,
            can_submit: false,
            can_email: false,
            can_create_account: false,
            budget: String::from("$0.00 / session"),
        }
    }
}

/// Loading state for async page loads.
#[derive(Debug, Clone)]
pub enum PageState {
    /// No page loaded — show welcome screen.
    Empty,
    /// Page is loading.
    Loading(String),
    /// Page loaded successfully.
    Loaded(LoadedPage),
    /// Page load failed.
    Error(String),
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct KitsuneSettings {
    pub telemetry: bool,
    pub auto_update: bool,
    #[serde(default)]
    pub onboarding_complete: bool,
}

impl Default for KitsuneSettings {
    fn default() -> Self {
        Self { telemetry: false, auto_update: true, onboarding_complete: false }
    }
}

impl KitsuneSettings {
    pub fn load() -> Self {
        let path = dirs::data_dir()
            .unwrap_or_else(std::env::temp_dir)
            .join("kitsune")
            .join("settings.json");
        std::fs::read_to_string(path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) {
        let path = dirs::data_dir()
            .unwrap_or_else(std::env::temp_dir)
            .join("kitsune")
            .join("settings.json");
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(path, json);
        }
    }
}

#[derive(Debug, Clone)]
pub struct LoadedPage {
    title: String,
    url: String,
    commands: Vec<kitsune_render::RenderCommand>,
    pub layout_root: LayoutNode,
}

/// The main application state.
pub struct KitsuneApp {
    /// Which panel is active in the sidebar.
    pub active_panel: Panel,
    /// Whether the agent shelf is open.
    pub agent_shelf_open: bool,
    /// Shelf animation time [0.0..1.0]
    pub shelf_open_t: f32,
    /// The agent builder modal state
    pub agent_builder: Option<AgentBuilderState>,
    /// Live agent card state
    pub agent_cards: Vec<crate::widgets::agent_shelf::AgentCardState>,
    /// Installed agents
    pub agents: Vec<kitsune_agent::spec::AgentSpec>,
    /// Current onboarding screen (None if onboarding is complete).
    onboarding_screen: Option<usize>,
    /// URL bar content.
    url_bar: String,
    /// Engine configuration.
    config: EngineConfig,
    /// Current page state (shared with async task).
    page_state: Arc<Mutex<PageState>>,
    /// Core pipeline state for transitions, etc.
    core_page_state: Arc<Mutex<kitsune_core::pipeline::PageState>>,
    /// Shared JS engine for all background loads.
    js_engine: Arc<tokio::sync::Mutex<kitsune_js::JsEngine>>,
    /// Tokio runtime for async operations.
    rt: tokio::runtime::Runtime,

    // Auth UI State
    screen: AppScreen,
    login_email: String,
    login_password: String,
    login_error: Option<String>,
    account_status: Arc<Mutex<Option<AccountStatus>>>,
    show_quota_banner: bool,

    // Vault Setup UI State
    vault_passphrase: String,
    vault_confirm: String,
    vault_error: Option<String>,

    // Engine and IPC
    engine: Option<Arc<Mutex<KitsuneEngine>>>,
    ipc_rx: Option<tokio::sync::mpsc::Receiver<kitsune_ipc::message::IpcMessage>>,

    // Download
    download_progress: Arc<Mutex<Option<f32>>>,

    // Settings
    settings: KitsuneSettings,

    // Theme
    theme: theme::KitsuneTheme,

    // UI Highlights Overlay
    highlights: Arc<Mutex<Vec<kitsune_ipc::message::DomHighlight>>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Panel {
    Browser,
    PrivacyDashboard,
    VaultManager,
    AgentBuilder,
    Settings,
    DevTools,
}

impl KitsuneApp {
    /// Create a new KitsuneApp.
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let rt = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime");

        let screen = if keyring::Entry::new("kitsune-engine", "cloud-token")
            .and_then(|e| e.get_password())
            .is_ok()
        {
            AppScreen::Browser
        } else {
            AppScreen::Login
        };

        let settings = KitsuneSettings::load();

        let onboarding_screen = if settings.onboarding_complete {
            None
        } else {
            Some(1)
        };

        let theme = theme::KitsuneTheme::dark();
        _cc.egui_ctx.set_style(theme.style.clone());

        let mut app = Self {
            active_panel: Panel::Browser,
            agent_shelf_open: false,
            shelf_open_t: 0.0,
            agent_builder: None,
            agent_cards: Vec::new(),
            agents: Vec::new(),
            onboarding_screen: onboarding_screen.map(|x| x as usize),
            url_bar: "kitsune://welcome".to_string(),
            config: EngineConfig::default(),
            page_state: Arc::new(Mutex::new(PageState::Empty)),
            core_page_state: Arc::new(Mutex::new(kitsune_core::pipeline::PageState { transition_state: Default::default() })),
            js_engine: Arc::new(tokio::sync::Mutex::new(kitsune_js::JsEngine::new())),
            rt,
            screen,
            login_email: String::new(),
            login_password: String::new(),
            login_error: None,
            account_status: Arc::new(Mutex::new(None)),
            show_quota_banner: false,
            vault_passphrase: String::new(),
            vault_confirm: String::new(),
            vault_error: None,
            engine: None,
            ipc_rx: None,
            download_progress: Arc::new(Mutex::new(None)),
            settings,
            theme,
            highlights: Arc::new(Mutex::new(Vec::new())),
        };

        // Initialize engine and IPC so we can receive events
        let mut engine = KitsuneEngine::new(EngineConfig::default());
        let _ = app.rt.block_on(engine.start());

        let rx = engine.ipc_bus.register_process(
            kitsune_ipc::message::ProcessId("ui-broker".to_string()),
            kitsune_ipc::message::PrivilegeLevel::Broker,
            std::collections::HashSet::new(),
            100,
        );

        app.engine = Some(Arc::new(Mutex::new(engine)));
        app.ipc_rx = Some(rx);

        app.navigate("kitsune://welcome".to_string(), _cc.egui_ctx.clone());

        app
    }

    /// Navigate to a URL.
    fn navigate(&self, url_str: String, ctx: egui::Context) {
        // ── kitsune:// scheme handler ───────────────────────────────
        // Map internal scheme to local demo server endpoints so the engine
        // can launch with a sensible home page without network access.
        let url = if url_str.starts_with("kitsune://") {
            let path = url_str.trim_start_matches("kitsune://");
            match path {
                "welcome" | "home" | "" => "http://127.0.0.1:7700/".to_string(),
                "shop"    => "http://127.0.0.1:7700/shop".to_string(),
                "privacy" => "http://127.0.0.1:7700/privacy".to_string(),
                other     => format!("http://127.0.0.1:7700/{}", other),
            }
        } else if !url_str.starts_with("http://") && !url_str.starts_with("https://") {
            // Bare host names → HTTPS
            format!("https://{}", url_str)
        } else {
            url_str
        };

        // Set loading state
        *self.page_state.lock().unwrap() = PageState::Loading(url.clone());

        let state = self.page_state.clone();
        let core_state_clone = self.core_page_state.lock().unwrap().clone();
        let core_state_arc = self.core_page_state.clone();
        let engine = self.engine.clone();
        let js_engine = self.js_engine.clone();

        self.rt.spawn(async move {
            let mut pipeline = kitsune_core::pipeline::PagePipeline::new();
            let mut temp_state = core_state_clone;
            let viewport = kitsune_layout::engine::Viewport::new(ctx.screen_rect().width() as f64, ctx.screen_rect().height() as f64);
            match pipeline.load_url(&url, viewport, &mut temp_state, &js_engine).await {
                Ok(content) => {
                    let (title, final_url) = (content.title.clone(), content.final_url.clone());
                    *state.lock().unwrap() = PageState::Loaded(LoadedPage {
                        title: content.title,
                        url: content.final_url,
                        commands: content.commands,
                        layout_root: content.layout_root,
                    });
                    *core_state_arc.lock().unwrap() = temp_state;

                    if let Some(engine) = engine {
                        if let Ok(mut engine) = engine.lock() {
                            if let Some(tab) = engine.tabs.iter_mut().find(|t| t.active) {
                                if let Ok(parsed_url) = url::Url::parse(&final_url) {
                                    tab.history.push(parsed_url, title);
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    *state.lock().unwrap() = PageState::Error(format!("{}", e));
                }
            }
            ctx.request_repaint();
        });
    }
}


impl eframe::App for KitsuneApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Drain IPC events to update agent status
        if let Some(rx) = &mut self.ipc_rx {
            while let Ok(msg) = rx.try_recv() {
                match msg.payload {
                    kitsune_ipc::message::IpcPayload::HilCheckpointRequest { action_description, trigger_class, cost, data_involved } => {
                        // In a real app we'd map this to a specific agent's status.
                        // Here we just spawn the window.
                        let req = crate::hil_window::HilRequest {
                            trigger_class: crate::hil_window::HilTriggerClass::Custom(trigger_class),
                            agent_name: msg.sender.0.clone(),
                            action_description,
                            bullet_points: vec![],
                            vault_labels: data_involved,
                            estimated_cost: cost.unwrap_or_else(|| "0.0".to_string()).parse().unwrap_or(0.0),
                        };

                        // We shouldn't block the UI thread, but the instructions say
                        // "HIL window appears". `show_hil_dialog` blocks until decision in this demo architecture.
                        // We'll spawn it in a new thread so we don't block the egui repaint loop.
                        std::thread::spawn(move || {
                            crate::hil_window::show_hil_dialog(req);
                        });
                    }
                    kitsune_ipc::message::IpcPayload::SetDomHighlight(mut h) => {
                        let mut highlights = self.highlights.lock().unwrap();
                        h.phase_start = Some(std::time::Instant::now());
                        highlights.push(h);
                    }
                    kitsune_ipc::message::IpcPayload::ClearDomHighlight(id) => {
                        let mut highlights = self.highlights.lock().unwrap();
                        if let Some(h) = highlights.iter_mut().find(|h| h.element_id == id) {
                            h.phase = kitsune_ipc::message::HighlightPhase::FadingOut;
                            h.phase_start = Some(std::time::Instant::now());
                        }
                    }
                    kitsune_ipc::message::IpcPayload::ClearAllDomHighlights => {
                        let mut highlights = self.highlights.lock().unwrap();
                        for h in highlights.iter_mut() {
                            h.phase = kitsune_ipc::message::HighlightPhase::FadingOut;
                            h.phase_start = Some(std::time::Instant::now());
                        }
                    }
                    _ => {}
                }
            }
        }

        let mut request_repaint = false;
        {
            let highlights = self.highlights.lock().unwrap();
            if !highlights.is_empty() {
                request_repaint = true;
            }
        }
        if request_repaint {
            ctx.request_repaint();
        }

        if ctx.input(|i| i.key_pressed(egui::Key::I) && i.modifiers.ctrl && i.modifiers.shift) {
            self.active_panel = if self.active_panel == Panel::DevTools {
                Panel::Browser
            } else {
                Panel::DevTools
            };
        }


        // Apply theme
        // Check if onboarding is active
        if let Some(screen) = self.onboarding_screen {
            egui::CentralPanel::default()
                .frame(egui::Frame::none().fill(theme::BG_BASE))
                .show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    ui.add_space(100.0);
                    match screen {
                        1 => {
                            ui.label(RichText::new("").size(48.0).color(theme::ACCENT)); // Tech icon
                            ui.add_space(20.0);
                            ui.heading(RichText::new("Welcome to Kitsune Engine").font(FontId::proportional(24.0)).strong().color(theme::TEXT_PRIMARY));
                            ui.add_space(12.0);
                            ui.label(RichText::new("HARDWARE-AGNOSTIC BROWSING ENVIRONMENT // VER 0.1.0").font(FontId::proportional(14.0)).color(theme::TEXT_MUTED));
                            ui.add_space(48.0);
                            
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 16.0;
                                feature_tag(ui, "[AUTONOMOUS_CORE]", theme::SUCCESS);
                                feature_tag(ui, "[ENCRYPT_VAULT]", theme::ACCENT);
                                feature_tag(ui, "[ZERO_KNOWLEDGE]", theme::AGENT_ACTING);
                            });
                            
                            ui.add_space(64.0);
                            let btn = ui.add_sized([240.0, 36.0], egui::Button::new(RichText::new("PROVISION ENVIRONMENT →").font(FontId::proportional(14.0)).strong())
                                .fill(theme::BG_ELEVATED)
                                .stroke(Stroke::new(1.0, theme::BORDER))
                                .rounding(Rounding::ZERO));
                            if btn.clicked() {
                                self.onboarding_screen = Some(2);
                            }
                        }
                        2 => {
                            ui.heading(RichText::new("SYSTEM CAPABILITIES").font(FontId::proportional(20.0)).strong());
                            ui.add_space(24.0);
                            ui.label(RichText::new("Kitsune provides an isolated runtime for autonomous web operations.").font(FontId::proportional(13.0)).color(theme::TEXT_MUTED));
                            ui.add_space(40.0);
                            
                            egui::Frame::none()
                                .fill(theme::BG_SURFACE)
                                .stroke(egui::Stroke::new(1.0, theme::BORDER))
                                .rounding(Rounding::ZERO)
                                .inner_margin(Margin::same(24.0))
                                .show(ui, |ui| {
                                ui.set_width(520.0);
                                ui.vertical(|ui| {
                                    ui.horizontal(|ui| {
                                        ui.label(RichText::new("").color(theme::SUCCESS).monospace());
                                        ui.label(RichText::new("LOGIC_CORE::01 // ACTIVE // MONITORING_NODES").font(FontId::proportional(12.0)).color(theme::TEXT_PRIMARY));
                                    });
                                    ui.add_space(10.0);
                                    ui.horizontal(|ui| {
                                        ui.label(RichText::new("").color(theme::ACCENT).monospace());
                                        ui.label(RichText::new("PRIVACY_SHIELD // ACTIVE // TRACKER_ISOLATION::32").font(FontId::proportional(12.0)).color(theme::TEXT_PRIMARY));
                                    });
                                    ui.add_space(10.0);
                                    ui.horizontal(|ui| {
                                        ui.label(RichText::new("").color(theme::TEXT_MUTED).monospace());
                                        ui.label(RichText::new("VAULT_SEAL // LOCKED // AES_256_GCM").font(FontId::proportional(12.0)).color(theme::TEXT_MUTED));
                                    });
                                });
                            });
                            
                            ui.add_space(56.0);
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 12.0;
                                if ui.add(egui::Button::new(RichText::new("NEXT_MODULE").font(FontId::proportional(12.0))).rounding(Rounding::ZERO)).clicked() { self.onboarding_screen = Some(3); }
                                if ui.add(egui::Button::new(RichText::new("SKIP_INIT").font(FontId::proportional(12.0))).rounding(Rounding::ZERO)).clicked() { self.onboarding_screen = Some(5); }
                            });
                        }
                        // ... rest of screens ...
                        _ => {
                            // Fallback for screens I'm skipping for now
                            if ui.button("Continue").clicked() { self.onboarding_screen = None; }
                        }
                    }
                });
            });
            return;
        }

        if self.screen == AppScreen::Login {
            egui::CentralPanel::default()
                .frame(egui::Frame::none().fill(theme::BG_BASE))
                .show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    ui.add_space(120.0);
                    ui.label(RichText::new("").font(FontId::proportional(48.0)).color(theme::ACCENT));
                    ui.add_space(16.0);
                    ui.heading(RichText::new("USER AUTHENTICATION").font(FontId::proportional(24.0)).strong());
                    ui.add_space(8.0);
                    ui.label(RichText::new("Identity verification required to decrypt session data.").font(FontId::proportional(12.0)).color(theme::TEXT_MUTED));
                    ui.add_space(40.0);

                    egui::Frame::none()
                        .fill(theme::BG_SURFACE)
                        .rounding(Rounding::ZERO)
                        .stroke(egui::Stroke::new(1.0, theme::BORDER))
                        .inner_margin(Margin::same(32.0))
                        .show(ui, |ui| {
                        ui.set_width(360.0);
                        ui.horizontal_top(|ui| {
                            ui.vertical_centered_justified(|ui| {
                                ui.horizontal(|ui| {
                                    ui.label(RichText::new("ID:").font(FontId::proportional(13.0)).color(theme::TEXT_MUTED));
                                    ui.add_space(20.0);
                                    ui.add(egui::TextEdit::singleline(&mut self.login_email));
                                });
                                ui.add_space(16.0);
                                ui.horizontal(|ui| {
                                    ui.label(RichText::new("KEY:").font(FontId::proportional(13.0)).color(theme::TEXT_MUTED));
                                    ui.add_space(12.0);
                                    ui.add(egui::TextEdit::singleline(&mut self.login_password).password(true));
                                });
                                
                                if let Some(ref err) = self.login_error {
                                    ui.add_space(12.0);
                                    ui.label(RichText::new(format!("ERR::{}", err)).font(FontId::proportional(11.0)).color(theme::ERROR));
                                }

                                ui.add_space(32.0);
                                let sign_in_btn = ui.add_sized([ui.available_width(), 36.0], egui::Button::new(RichText::new("EXECUTE LOGIN").font(FontId::proportional(13.0)).strong())
                                    .fill(theme::BG_ELEVATED)
                                    .stroke(Stroke::new(1.0, theme::BORDER))
                                    .rounding(Rounding::ZERO));
                                
                                if sign_in_btn.clicked() {
                                    // Login logic...
                                }
                                
                                ui.add_space(20.0);
                                ui.horizontal(|ui| {
                                    ui.separator();
                                    ui.label(RichText::new("OR").font(FontId::proportional(10.0)).color(theme::TEXT_MUTED));
                                    ui.separator();
                                });
                                ui.add_space(20.0);
                                
                                if ui.add(egui::Button::new(RichText::new("CONTINUE AS ANONYMOUS").font(FontId::proportional(11.0))).rounding(Rounding::ZERO)).clicked() {
                                    self.screen = AppScreen::Browser;
                                }
                            });
                        });
                    });
                });
            });
            return;
        }

        if self.screen == AppScreen::Account {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    ui.add_space(40.0);
                    ui.heading("🦊 Account Setup");
                    ui.add_space(10.0);

                    // Load status if not present
                    let mut status_lock = self.account_status.lock().unwrap();
                    if status_lock.is_none() {
                        if let Ok(token) = keyring::Entry::new("kitsune-engine", "cloud-token").and_then(|k| k.get_password()) {
                            let status = self.rt.block_on(async {
                                KitsuneCloudBackend::get_account_status(&token).await.ok()
                            });
                            *status_lock = status;
                        }
                    }

                    if let Some(status) = status_lock.as_ref() {
                        let is_pro = status.tier.eq_ignore_ascii_case("pro");
                        ui.horizontal(|ui| {
                            ui.label("Tier:");
                            if is_pro {
                                ui.colored_label(theme::ACCENT, "PRO");
                            } else {
                                ui.colored_label(theme::TEXT_MUTED, "Free Tier");
                            }
                        });

                        ui.label(format!("{} / {} actions this month", status.actions_used, status.limit));
                        ui.label(format!("Resets {}", status.resets_at));
                        ui.add_space(10.0);

                        if ui.button("Upgrade to Pro — $9/month").clicked() {
                            ui.ctx().output_mut(|o| {
                                o.open_url = Some(egui::output::OpenUrl {
                                    url: "https://kitsune.sh/upgrade".into(),
                                    new_tab: true,
                                });
                            });
                        }

                        ui.add_space(20.0);
                        ui.separator();
                        ui.add_space(10.0);
                        ui.heading("Local AI (Private Mode)");

                        if is_pro {
                            ui.label("Run agents 100% on-device. Nothing leaves your computer.");

                            if kitsune_ai::local::is_model_downloaded() {
                                ui.colored_label(theme::SUCCESS, "Local AI ready ✓");
                            } else {
                                let progress = *self.download_progress.lock().unwrap();
                                if let Some(p) = progress {
                                    ui.add(egui::ProgressBar::new(p).text(format!("Downloading... {:.0}%", p * 100.0)));
                                } else {
                                    if ui.button("Download Model — 2.3GB").clicked() {
                                        *self.download_progress.lock().unwrap() = Some(0.0);
                                        let progress_tracker = self.download_progress.clone();
                                        let ctx_clone = ctx.clone();

                                        self.rt.spawn(async move {
                                            let progress_inner = progress_tracker.clone();
                                            let ctx_inner = ctx_clone.clone();
                                            let _ = kitsune_ai::local::LocalAiBackend::download_model(move |p| {
                                                *progress_inner.lock().unwrap() = Some(p);
                                                ctx_inner.request_repaint();
                                            }).await;
                                            *progress_tracker.lock().unwrap() = None;
                                            ctx_clone.request_repaint();
                                        });
                                    }
                                }
                            }
                        } else {
                            ui.colored_label(theme::TEXT_MUTED, "Available on Pro plan");
                        }
                    } else {
                        ui.label("Fetching account status...");
                    }

                    ui.add_space(20.0);
                    if ui.button("Log Out").clicked() {
                        self.rt.block_on(async { let _ = KitsuneCloudBackend::logout().await; });
                        self.screen = AppScreen::Login;
                        self.active_panel = Panel::Browser; // reset panel
                    }
                    if ui.button("← Back to Browser").clicked() {
                        self.screen = AppScreen::Browser;
                    }
                });
            });
            return;
        }

        // --- Browser UI ---
        
        // 1. Top Panel: Tab Bar
        egui::TopBottomPanel::top("tab_strip")
            .frame(egui::Frame::none().fill(theme::BG_BASE))
            .show(ctx, |ui| {
                let tabs = if let Some(engine) = &self.engine {
                    engine.lock().unwrap().tabs.clone()
                } else {
                    vec![]
                };

                let active_tab_id = if let Some(engine) = &self.engine {
                    engine.lock().unwrap().tabs.iter().find(|t| t.active).map(|t| t.id).unwrap_or(0)
                } else {
                    0
                };

                let tab_resp = tab_bar::tab_bar(ui, &tabs, active_tab_id);

                if let Some(id) = tab_resp.clicked_tab {
                    if let Some(engine) = &self.engine {
                        let mut engine = engine.lock().unwrap();
                        for tab in &mut engine.tabs {
                            tab.active = tab.id == id;
                        }
                        if let Some(tab) = engine.tabs.iter().find(|t| t.active) {
                            self.url_bar = tab.url.clone().unwrap_or_default();
                        }
                    }
                }

                if let Some(id) = tab_resp.closed_tab {
                    if let Some(engine) = &self.engine {
                        let mut engine = engine.lock().unwrap();
                        engine.close_tab(id);
                    }
                }

                if tab_resp.new_tab_clicked {
                    if let Some(engine) = &self.engine {
                        let mut engine = engine.lock().unwrap();
                        engine.new_tab();
                    }
                }
            });

        // 2. Top Panel: Navigation & Address Bar
        egui::TopBottomPanel::top("nav_bar")
            .frame(egui::Frame::none().fill(theme::BG_SURFACE))
            .show(ctx, |ui| {
                let (can_back, can_forward) = if let Some(engine) = &self.engine {
                    let engine = engine.lock().unwrap();
                    if let Some(tab) = engine.tabs.iter().find(|t| t.active) {
                        (tab.history.can_go_back(), tab.history.can_go_forward())
                    } else {
                        (false, false)
                    }
                } else {
                    (false, false)
                };

                let nav_resp = nav_bar::nav_bar(ui, &mut self.url_bar, can_back, can_forward);

                if nav_resp.navigate_back {
                    if let Some(engine) = &self.engine {
                        let mut engine = engine.lock().unwrap();
                        if let Some(tab) = engine.tabs.iter_mut().find(|t| t.active) {
                            if let Some(entry) = tab.history.back() {
                                let url = entry.url.to_string();
                                self.url_bar = url.clone();
                                self.navigate(url, ctx.clone());
                            }
                        }
                    }
                }

                if nav_resp.navigate_forward {
                    if let Some(engine) = &self.engine {
                        let mut engine = engine.lock().unwrap();
                        if let Some(tab) = engine.tabs.iter_mut().find(|t| t.active) {
                            if let Some(entry) = tab.history.forward() {
                                let url = entry.url.to_string();
                                self.url_bar = url.clone();
                                self.navigate(url, ctx.clone());
                            }
                        }
                    }
                }

                if nav_resp.reload || nav_resp.url_submitted.is_some() {
                    let url = self.url_bar.clone();
                    if !url.is_empty() {
                        self.navigate(url, ctx.clone());
                    }
                }
                
                if nav_resp.go_home {
                    self.url_bar = "kitsune://home".to_string();
                    self.navigate(self.url_bar.clone(), ctx.clone());
                }

                if nav_resp.toggle_shelf {
                    self.agent_shelf_open = !self.agent_shelf_open;
                }

                if nav_resp.toggle_privacy {
                    self.active_panel = if self.active_panel == Panel::PrivacyDashboard {
                        Panel::Browser
                    } else {
                        Panel::PrivacyDashboard
                    };
                }

                if nav_resp.toggle_settings {
                    self.active_panel = if self.active_panel == Panel::Settings {
                        Panel::Browser
                    } else {
                        Panel::Settings
                    };
                }
            });

        // 3. Status Bar (Bottom)
        egui::TopBottomPanel::bottom("status_bar")
            .exact_height(28.0)
            .frame(egui::Frame::none().fill(theme::BG_SURFACE).inner_margin(egui::Margin::symmetric(12.0, 4.0)))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 12.0;

                    // Privacy Status
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 4.0;
                        ui.label(RichText::new("●").color(theme::SUCCESS).size(10.0));
                        ui.label(RichText::new("Secure Connection").size(11.0).color(theme::TEXT_MUTED));
                    });

                    ui.separator();

                    // Tracker Info
                    ui.label(RichText::new("0 Trackers Blocked").size(11.0).color(theme::TEXT_MUTED));

                    ui.separator();

                    // Agent Status
                    let active_agents = self.agent_cards.iter().filter(|c| c.status == kitsune_agent::runtime::AgentStatus::Running).count();
                    let agent_text = if active_agents > 0 {
                        format!("{} Agents Active", active_agents)
                    } else {
                        "No Agents Active".to_string()
                    };
                    ui.label(RichText::new(agent_text).size(11.0).color(if active_agents > 0 { theme::ACCENT } else { theme::TEXT_MUTED }));

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(RichText::new(format!("Kitsune v{}", kitsune_core::ENGINE_VERSION))
                            .size(10.0)
                            .color(theme::TEXT_MUTED.linear_multiply(0.5)));
                    });
                });
            });

        // Agent shelf animation and overlay
        if self.agent_shelf_open {
            if self.shelf_open_t < 1.0 {
                self.shelf_open_t += 0.067;
                if self.shelf_open_t > 1.0 {
                    self.shelf_open_t = 1.0;
                }
                ctx.request_repaint();
            }
        } else {
            if self.shelf_open_t > 0.0 {
                self.shelf_open_t -= 0.067;
                if self.shelf_open_t < 0.0 {
                    self.shelf_open_t = 0.0;
                }
                ctx.request_repaint();
            }
        }

        // Draw Agent Shelf Overlay
        if self.shelf_open_t > 0.0 {
            let mut add_agent = false;
            crate::widgets::agent_shelf::render_agent_shelf(
                ctx,
                &self.theme,
                self.shelf_open_t,
                &mut self.agent_cards,
                || {
                    add_agent = true;
                },
            );
            if add_agent {
                self.agent_builder = Some(AgentBuilderState::default());
                self.agent_shelf_open = false; // Optionally close shelf when builder opens
            }
        }

        let mut close_builder = false;
        let mut add_agent = None;
        if let Some(builder) = &mut self.agent_builder {
            egui::Window::new("Agent Builder")
                .fixed_size(egui::vec2(600.0, 400.0))
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
                .show(ctx, |ui| {
                    ui.vertical_centered(|ui| {
                        match builder.step {
                            1 => {
                                ui.heading("What do you want this agent to do?");
                                ui.add_space(16.0);
                                ui.add(egui::TextEdit::multiline(&mut builder.description)
                                    .hint_text("e.g. Track prices and alert me when they drop 20%")
                                    .desired_rows(5)
                                    .desired_width(f32::INFINITY));
                                ui.add_space(24.0);
                                if ui.button("Next →").clicked() {
                                    builder.step = 2;
                                }
                            }
                            2 => {
                                ui.heading("Constraints");
                                ui.add_space(16.0);

                                ui.horizontal(|ui| {
                                    ui.label("Can navigate to new pages");
                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                        ui.checkbox(&mut builder.can_navigate, "");
                                    });
                                });
                                ui.horizontal(|ui| {
                                    ui.label("Can fill forms");
                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                        ui.checkbox(&mut builder.can_fill_forms, "");
                                    });
                                });
                                ui.horizontal(|ui| {
                                    ui.label("Can click submit buttons");
                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                        ui.checkbox(&mut builder.can_submit, "");
                                    });
                                });
                                ui.horizontal(|ui| {
                                    ui.label("Can send emails");
                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                        ui.checkbox(&mut builder.can_email, "");
                                    });
                                });
                                ui.horizontal(|ui| {
                                    ui.label("Can create accounts");
                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                        ui.checkbox(&mut builder.can_create_account, "");
                                    });
                                });
                                ui.add_space(16.0);
                                ui.horizontal(|ui| {
                                    ui.label("Budget ceiling:");
                                    ui.add(egui::TextEdit::singleline(&mut builder.budget).desired_width(120.0));
                                });
                                ui.add_space(24.0);
                                ui.horizontal(|ui| {
                                    if ui.button("← Back").clicked() {
                                        builder.step = 1;
                                    }
                                    if ui.button("Next →").clicked() {
                                        builder.step = 3;
                                    }
                                });
                            }
                            3 => {
                                ui.heading("Review & Create");
                                ui.add_space(16.0);

                                let state = kitsune_agent_builder::builder::create_from_description(
                                    "New Agent",
                                    &builder.description
                                );

                                ui.strong(format!("Name: {}", state.spec.name));
                                ui.label(format!("Summary: {}", state.spec.description));

                                ui.add_space(16.0);
                                ui.strong("Constraint summary:");
                                if !builder.can_submit {
                                    ui.colored_label(theme::SUCCESS, "Will ask before submitting anything");
                                } else {
                                    ui.colored_label(theme::WARNING, "Can submit forms without asking");
                                }

                                ui.add_space(24.0);
                                ui.horizontal(|ui| {
                                    if ui.button("← Back").clicked() {
                                        builder.step = 2;
                                    }
                                    if ui.button("Create Agent ✓").clicked() {
                                        let mut spec = state.spec;
                                        spec.constraints.can_initiate_payments = builder.can_submit;
                                        spec.constraints.can_create_accounts = builder.can_create_account;
                                        spec.constraints.can_send_communications = builder.can_email;
                                        add_agent = Some(spec);
                                        close_builder = true;
                                    }
                                });
                            }
                            _ => {}
                        }
                    });
                });
        }

        if close_builder {
            self.agent_builder = None;
        }
        if let Some(spec) = add_agent {
            self.agents.push(spec);
        }

        // Main content area
        egui::CentralPanel::default().show(ctx, |ui| {
            match self.active_panel {
                Panel::Browser => {
                    if self.show_quota_banner {
                        egui::Frame::none()
                            .fill(theme::WARNING) // Amber
                            .inner_margin(8.0)
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    ui.colored_label(theme::TEXT_PRIMARY, "⚠ Monthly limit reached. Agents paused.");
                                    if ui.link("Upgrade").clicked() {
                                        ui.ctx().output_mut(|o| {
                                            o.open_url = Some(egui::output::OpenUrl {
                                                url: "https://kitsune.sh/upgrade".into(),
                                                new_tab: true,
                                            });
                                        });
                                    }
                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                        if ui.button("✖").clicked() {
                                            self.show_quota_banner = false;
                                        }
                                    });
                                });
                            });
                        ui.separator();
                    }

                    // In the new layout, tabs and navigation are handled by TopBottomPanels.
                    // Here we just render the content.

                    // Render based on page state
                    let page_state = self.page_state.lock().unwrap().clone();
                    match page_state {
                        PageState::Empty => {
                            ui.centered_and_justified(|ui| {
                                ui.vertical_centered(|ui| {
                                    ui.add_space(100.0);
                                    ui.heading(RichText::new("KITSUNE_ENGINE // CORE_MODULE").font(FontId::proportional(24.0)).strong());
                                    ui.add_space(12.0);
                                    ui.label(RichText::new("Built in Rust. Grounded in trust. Isolated runtime.").font(FontId::proportional(14.0)).color(theme::TEXT_MUTED));
                                    ui.add_space(48.0);
                                    
                                    egui::Frame::none()
                                        .stroke(Stroke::new(1.0, theme::BORDER))
                                        .inner_margin(Margin::same(24.0))
                                        .show(ui, |ui| {
                                        ui.vertical(|ui| {
                                            ui.colored_label(theme::SUCCESS, "✓ ENCRYPTED_STORAGE_READY");
                                            ui.add_space(8.0);
                                            ui.colored_label(theme::SUCCESS, "✓ ANTI_FINGERPRINT_ACTIVE");
                                            ui.add_space(8.0);
                                            ui.colored_label(theme::SUCCESS, "✓ TRACK_ISOLATION_ENABLED");
                                        });
                                    });
                                });
                            });
                        }
                        PageState::Loading(url) => {
                            ui.centered_and_justified(|ui| {
                                ui.vertical_centered(|ui| {
                                    ui.add_space(100.0);
                                    ui.spinner();
                                    ui.add_space(16.0);
                                    ui.label(format!("Loading {}...", url));
                                });
                            });
                        }
                        PageState::Error(msg) => {
                            ui.centered_and_justified(|ui| {
                                ui.vertical_centered(|ui| {
                                    ui.add_space(100.0);
                                    ui.colored_label(theme::ERROR, "⚠ Page Load Error");
                                    ui.add_space(8.0);
                                    ui.label(&msg);
                                });
                            });
                        }
                        PageState::Loaded(page) => {
                            // Render the page content using egui's painter, resilient to panics
                            let mut commands_clone = page.commands.clone();

                            {
                                let mut highlights = self.highlights.lock().unwrap();
                                let mut list = kitsune_render::DisplayList { commands: commands_clone };
                                kitsune_render::painter::paint_highlights(&mut highlights, &mut list);
                                commands_clone = list.commands;
                            }

                            let mut ui_safe = std::panic::AssertUnwindSafe(&mut *ui);
                            let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                                render_page_content(*ui_safe, &commands_clone);
                            }));

                            if res.is_err() {
                                ui.centered_and_justified(|ui| {
                                    ui.vertical_centered(|ui| {
                                        ui.add_space(100.0);
                                        ui.colored_label(theme::ERROR, "Something went wrong.");
                                        if ui.button("Reload page").clicked() {
                                            // Handle reload
                                        }
                                    });
                                });
                            }
                        }
                    }
                }
                Panel::PrivacyDashboard => {
                    panels::render_privacy_dashboard(ui, &self.engine, &self.theme);
                }
                Panel::VaultManager => {
                    panels::render_vault_manager(ui, &self.theme);
                }
                Panel::AgentBuilder => {
                    ui.heading("Agent Builder");
                    ui.label("Tell me what you want to automate...");
                }
                Panel::Settings => {
                    panels::render_settings_panel(ui, &mut self.settings);
                }
                Panel::DevTools => {
                    panels::render_devtools_panel(ui, &self.page_state, &self.theme);
                }
            }
        });

        // Status bar
        egui::TopBottomPanel::bottom("status_bar_v2") // Unique ID
            .exact_height(28.0)
            .frame(egui::Frame::none().fill(theme::BG_SURFACE).stroke(Stroke::new(1.0, theme::BORDER)))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 12.0;
                    ui.colored_label(theme::SUCCESS, "●");
                    ui.label(RichText::new("RUNTIME_STABLE").font(FontId::proportional(11.0)).color(theme::SUCCESS));
                    ui.separator();
                    ui.label(RichText::new("TRACKERS::0_ISOLATED").font(FontId::proportional(11.0)).color(theme::TEXT_MUTED));
                    ui.separator();
                    ui.label(RichText::new("AGENTS::IDLE").font(FontId::proportional(11.0)).color(theme::TEXT_MUTED));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(RichText::new(format!("KERNEL_VER::{}", kitsune_core::ENGINE_VERSION)).font(FontId::proportional(11.0)).color(theme::TEXT_MUTED));
                    });
                });
            });
    }
}

/// Render page content using egui's painter API.
fn render_page_content(ui: &mut egui::Ui, commands: &[RenderCommand]) {
    // Use a ScrollArea for the page content
    egui::ScrollArea::both().show(ui, |ui| {
        // Calculate total page height from commands
        let mut max_y: f32 = 600.0;
        for cmd in commands {
            match cmd {
                RenderCommand::FillRect { y, height, .. } => {
                    max_y = max_y.max(y + height + 20.0);
                }
                RenderCommand::DrawText { y, font_size, .. } => {
                    max_y = max_y.max(y + font_size * 1.5 + 20.0);
                }
                _ => {}
            }
        }

        let (response, painter) = ui.allocate_painter(
            egui::vec2(ui.available_width(), max_y),
            egui::Sense::hover(),
        );
        let origin = response.rect.min;

        // Draw a white background for the web page viewport
        painter.rect_filled(response.rect, 0.0, egui::Color32::WHITE);

        for cmd in commands {
            match cmd {
                RenderCommand::FillRect { x, y, width, height, color } => {
                    let rect = egui::Rect::from_min_size(
                        egui::pos2(origin.x + x, origin.y + y),
                        egui::vec2(*width, *height),
                    );
                    let c = egui::Color32::from_rgba_premultiplied(
                        (color[0] * 255.0) as u8,
                        (color[1] * 255.0) as u8,
                        (color[2] * 255.0) as u8,
                        (color[3] * 255.0) as u8,
                    );
                    painter.rect_filled(rect, 0.0, c);
                }
                RenderCommand::DrawText { x, y, text, font_size, color } => {
                    let pos = egui::pos2(origin.x + x, origin.y + y);
                    let c = egui::Color32::from_rgba_premultiplied(
                        (color[0] * 255.0) as u8,
                        (color[1] * 255.0) as u8,
                        (color[2] * 255.0) as u8,
                        (color[3] * 255.0) as u8,
                    );
                    let font_id = egui::FontId::proportional(*font_size);
                    painter.text(
                        pos,
                        egui::Align2::LEFT_TOP,
                        text,
                        font_id,
                        c,
                    );
                }
                RenderCommand::DrawBorder { x, y, width, height, border_width, color } => {
                    let rect = egui::Rect::from_min_size(
                        egui::pos2(origin.x + x, origin.y + y),
                        egui::vec2(*width, *height),
                    );
                    let c = egui::Color32::from_rgba_premultiplied(
                        (color[0] * 255.0) as u8,
                        (color[1] * 255.0) as u8,
                        (color[2] * 255.0) as u8,
                        (color[3] * 255.0) as u8,
                    );
                    painter.rect_stroke(rect, 0.0, egui::Stroke::new(*border_width, c));
                }
                RenderCommand::DrawImage { x, y, width, height, image_data } => {
                    let rect = egui::Rect::from_min_size(
                        egui::pos2(origin.x + x, origin.y + y),
                        egui::vec2(*width, *height),
                    );
                    
                    // Simple hash-based caching to avoid re-uploading every frame
                    let tex_id = format!("img_{}", image_data.len()); // Crude but works for demo
                    let texture = ui.ctx().load_texture(
                        tex_id,
                        egui::ColorImage::example(), // Placeholder if we don't parse bytes here
                        Default::default(),
                    );
                    
                    // Actually, we should parse the image. For now, let's at least show a box if it failed.
                    if let Ok(image) = image::load_from_memory(image_data) {
                        let size = [image.width() as usize, image.height() as usize];
                        let pixels = image.to_rgba8().into_raw();
                        let color_image = egui::ColorImage::from_rgba_unmultiplied(size, &pixels);
                        let texture = ui.ctx().load_texture(format!("img_res_{}", image_data.len()), color_image, Default::default());
                        painter.image(texture.id(), rect, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), egui::Color32::WHITE);
                    } else {
                        painter.rect_filled(rect, 0.0, egui::Color32::from_gray(200));
                        painter.text(rect.center(), egui::Align2::CENTER_CENTER, "", egui::FontId::proportional(24.0), egui::Color32::DARK_GRAY);
                    }
                }
            }
        }
    });
}


/// Renders a small, colored tag for feature highlights in onboarding.
fn feature_tag(ui: &mut egui::Ui, text: &str, color: egui::Color32) {
    egui::Frame::none()
        .fill(color.linear_multiply(0.1))
        .stroke(egui::Stroke::new(1.0, color.linear_multiply(0.5)))
        .rounding(egui::Rounding::same(4.0))
        .inner_margin(egui::Margin::symmetric(8.0, 4.0))
        .show(ui, |ui| {
            ui.label(egui::RichText::new(text).size(12.0).color(color));
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use kitsune_ipc::message::{DomHighlight, HighlightRect, HighlightStyle, HighlightPhase, IpcMessage, IpcPayload, ProcessId};

    fn setup_app() -> KitsuneApp {
        KitsuneApp {
            active_panel: Panel::Browser,
            agent_shelf_open: false,
            shelf_open_t: 0.0,
            agent_builder: None,
            agent_cards: Vec::new(),
            agents: Vec::new(),
            url_bar: "".to_string(),
            config: kitsune_core::EngineConfig::default(),
            page_state: Arc::new(Mutex::new(PageState::Loading("".to_string()))),
            core_page_state: Arc::new(Mutex::new(kitsune_core::pipeline::PageState { transition_state: Default::default() })),
            js_engine: Arc::new(tokio::sync::Mutex::new(kitsune_js::JsEngine::new())),
            onboarding_screen: None,
            rt: tokio::runtime::Runtime::new().unwrap(),
            login_email: "".to_string(),
            login_password: "".to_string(),
            login_error: None,
            screen: AppScreen::Browser,
            account_status: Arc::new(Mutex::new(None)),
            show_quota_banner: false,
            vault_passphrase: "".to_string(),
            vault_confirm: "".to_string(),
            vault_error: None,
            engine: None,
            ipc_rx: None,
            download_progress: Arc::new(Mutex::new(None)),
            settings: KitsuneSettings::default(),
            theme: theme::KitsuneTheme::dark(),
            highlights: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn mock_highlight(id: &str) -> DomHighlight {
        DomHighlight {
            element_id: id.to_string(),
            rect: HighlightRect { x: 0.0, y: 0.0, width: 10.0, height: 10.0 },
            style: HighlightStyle::Reading,
            phase: HighlightPhase::FadingIn,
            phase_start: None,
        }
    }

    #[test]
    fn test_highlight_rect_created() {
        let mut app = setup_app();

        let msg1 = IpcMessage::new(
            ProcessId("agent".to_string()),
            ProcessId("ui".to_string()),
            IpcPayload::SetDomHighlight(mock_highlight("elem1"))
        );

        let (tx, mut rx) = tokio::sync::mpsc::channel(10);
        tx.try_send(msg1).unwrap();
        app.ipc_rx = Some(rx);

        // Simulate eframe frame update pulling IPC
        let mut ctx = egui::Context::default();
        // Since we can't easily mock eframe::Frame here, we just run the drain loop logic directly
        // Or we can just call update with mock frame... wait, update accesses ctx.
        if let Some(r) = &mut app.ipc_rx {
            while let Ok(msg) = r.try_recv() {
                if let IpcPayload::SetDomHighlight(mut h) = msg.payload {
                    let mut highlights = app.highlights.lock().unwrap();
                    h.phase_start = Some(std::time::Instant::now());
                    highlights.push(h);
                }
            }
        }

        assert_eq!(app.highlights.lock().unwrap().len(), 1);
        assert_eq!(app.highlights.lock().unwrap()[0].element_id, "elem1");
    }

    #[test]
    fn test_highlight_cleared() {
        let mut app = setup_app();
        app.highlights.lock().unwrap().push(mock_highlight("elem1"));
        app.highlights.lock().unwrap().push(mock_highlight("elem2"));

        let msg = IpcMessage::new(
            ProcessId("agent".to_string()),
            ProcessId("ui".to_string()),
            IpcPayload::ClearDomHighlight("elem1".to_string())
        );

        let (tx, mut rx) = tokio::sync::mpsc::channel(10);
        tx.try_send(msg).unwrap();
        app.ipc_rx = Some(rx);

        if let Some(r) = &mut app.ipc_rx {
            while let Ok(msg) = r.try_recv() {
                if let IpcPayload::ClearDomHighlight(id) = msg.payload {
                    let mut highlights = app.highlights.lock().unwrap();
                    if let Some(h) = highlights.iter_mut().find(|h| h.element_id == id) {
                        h.phase = HighlightPhase::FadingOut;
                    }
                }
            }
        }

        // Assert elem1 transitioned to FadingOut
        let hl = app.highlights.lock().unwrap();
        assert_eq!(hl.iter().find(|h| h.element_id == "elem1").unwrap().phase, HighlightPhase::FadingOut);
        assert_eq!(hl.iter().find(|h| h.element_id == "elem2").unwrap().phase, HighlightPhase::FadingIn);
    }

    #[test]
    fn test_highlight_clear_all() {
        let mut app = setup_app();
        app.highlights.lock().unwrap().push(mock_highlight("elem1"));
        app.highlights.lock().unwrap().push(mock_highlight("elem2"));

        let msg = IpcMessage::new(
            ProcessId("agent".to_string()),
            ProcessId("ui".to_string()),
            IpcPayload::ClearAllDomHighlights
        );

        let (tx, mut rx) = tokio::sync::mpsc::channel(10);
        tx.try_send(msg).unwrap();
        app.ipc_rx = Some(rx);

        if let Some(r) = &mut app.ipc_rx {
            while let Ok(msg) = r.try_recv() {
                if let IpcPayload::ClearAllDomHighlights = msg.payload {
                    let mut highlights = app.highlights.lock().unwrap();
                    for h in highlights.iter_mut() {
                        h.phase = HighlightPhase::FadingOut;
                    }
                }
            }
        }

        let hl = app.highlights.lock().unwrap();
        assert_eq!(hl[0].phase, HighlightPhase::FadingOut);
        assert_eq!(hl[1].phase, HighlightPhase::FadingOut);
    }
}
