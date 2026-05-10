// Page rendering delegated to WebView2 via wry
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::info;

use kitsune_agent::{executor::WebViewCommand, AgentRuntime};
use kitsune_hil::gate::HilGate;
use kitsune_ipc::IpcBus;
use kitsune_net::KitsuneHttpClient;
use kitsune_vault::backend::VaultBackend;

use crate::broker::ProcessManager;

/// The main KitsuneEngine instance.
pub struct KitsuneEngine {
    /// The IPC bus for inter-process communication.
    pub ipc_bus: Arc<IpcBus>,
    /// The HTTP client.
    pub http_client: Arc<KitsuneHttpClient>,
    /// The agent runtime.
    pub agent_runtime: AgentRuntime,
    /// Active tabs.
    pub tabs: Vec<super::tab::Tab>,
    /// Engine configuration.
    pub config: super::config::EngineConfig,
    /// Whether the engine is running.
    running: bool,
    /// Child process manager / broker.
    pub process_manager: ProcessManager,
}

impl KitsuneEngine {
    /// Create a new KitsuneEngine instance.
    pub fn new(config: super::config::EngineConfig) -> Self {
        info!(
            version = super::ENGINE_VERSION,
            "Initializing {}",
            super::ENGINE_NAME
        );

        let ipc_bus = Arc::new(IpcBus::new());
        let http_client = Arc::new(KitsuneHttpClient::new());

        // Create a channel for webview commands
        let (webview_tx, _webview_rx) = mpsc::channel::<WebViewCommand>(100);

        // Create vault with keyring-backed KDF salt; fall back to a dev-only
        // fixed salt if the OS keyring is unavailable (e.g. headless CI).
        let vault = Arc::new(
            VaultBackend::new_with_keyring("kitsune-dev")
                .unwrap_or_else(|_| {
                    tracing::warn!("keyring unavailable — using dev vault (never in production)");
                    VaultBackend::new("kitsune-dev", &[1u8; 32]).unwrap()
                }),
        );
        let (hil_gate, _hil_rx) = HilGate::new(100);
        let hil_gate = Arc::new(hil_gate);

        let agent_runtime = AgentRuntime::new(webview_tx, vault, hil_gate);
        let process_manager = ProcessManager::new();

        Self {
            ipc_bus,
            http_client,
            agent_runtime,
            tabs: Vec::new(),
            config,
            running: false,
            process_manager,
        }
    }

    /// Start the engine.
    ///
    /// Starts the mock demo server and registers in-process stubs for each
    /// child role (renderer, network, js). In a full production build the stubs
    /// are replaced by real sandboxed child processes via [`ProcessManager::spawn_child`].
    pub async fn start(&mut self) -> anyhow::Result<()> {
        info!("Starting KitsuneEngine");
        self.running = true;

        // Create an initial blank tab
        self.tabs
            .push(super::tab::Tab::new(0, "New Tab".to_string()));

        // Start the local demo/mock server so the welcome page is available
        // immediately on http://127.0.0.1:7700
        tokio::spawn(async {
            if let Err(e) = kitsune_cloud_mock::start("127.0.0.1:7700").await {
                tracing::error!("Demo mock server failed: {}", e);
            }
        });
        info!("Demo server starting at http://127.0.0.1:7700");

        // Register mock in-process stubs for each child role.
        // These replace real sandboxed processes for the single-process MVP.
        self.process_manager
            .register_mock(kitsune_ipc::message::ProcessRole::Renderer);
        self.process_manager
            .register_mock(kitsune_ipc::message::ProcessRole::Network);
        self.process_manager
            .register_mock(kitsune_ipc::message::ProcessRole::Js);

        info!("KitsuneEngine started successfully");
        Ok(())
    }

    /// Stop the engine.
    pub async fn shutdown(&mut self) {
        info!("Shutting down KitsuneEngine");
        self.running = false;
        self.tabs.clear();
        info!("KitsuneEngine shut down");
    }

    /// Check if the engine is running.
    pub fn is_running(&self) -> bool {
        self.running
    }

    /// Open a new tab.
    pub fn new_tab(&mut self) -> usize {
        let id = self.tabs.len();
        let tab = super::tab::Tab::new(id, "New Tab".to_string());

        self.tabs.push(tab);
        info!(tab_id = id, "New tab opened");
        id
    }

    /// Close a tab.
    pub fn close_tab(&mut self, id: usize) {
        if id < self.tabs.len() {
            self.tabs.remove(id);
            info!(tab_id = id, "Tab closed");
        }
    }

    /// Navigate a tab to a URL.
    pub async fn navigate(&mut self, tab_id: usize, url: &str) -> anyhow::Result<()> {
        if let Some(tab) = self.tabs.get_mut(tab_id) {
            tab.navigate(url);
            info!(tab_id = tab_id, url = %url, "Navigation started");
        }
        Ok(())
    }
}
