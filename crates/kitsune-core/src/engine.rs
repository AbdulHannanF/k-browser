/// KitsuneEngine — the core engine orchestrator.

use kitsune_agent::AgentRuntime;
use kitsune_ipc::IpcBus;
use kitsune_net::KitsuneHttpClient;

use std::sync::Arc;
use tracing::info;

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
        let agent_runtime = AgentRuntime::new();
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
        self.tabs.push(super::tab::Tab::new(
            0,
            "New Tab".to_string(),
        ));

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
        self.process_manager.register_mock(kitsune_ipc::message::ProcessRole::Renderer);
        self.process_manager.register_mock(kitsune_ipc::message::ProcessRole::Network);
        self.process_manager.register_mock(kitsune_ipc::message::ProcessRole::Js);

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
        let mut tab = super::tab::Tab::new(id, "New Tab".to_string());

        #[cfg(target_os = "windows")]
        {
            use std::process::Command;
            if let Ok(sandbox) = kitsune_sandbox::JobObjectSandbox::new(&format!("kitsune-renderer-{}", id)) {
                let _ = sandbox.configure();
                // Spawn a child process loop (dummy for now) to simulate the renderer process
                if let Ok(child) = Command::new(std::env::current_exe().unwrap_or_default())
                    .arg("--renderer")
                    .arg(id.to_string())
                    .spawn()
                {
                    let _ = sandbox.assign_process(child.id());
                    tab.renderer_pid = Some(child.id());
                }
                tab.sandbox = Some(std::sync::Arc::new(sandbox));
            }
        }

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
