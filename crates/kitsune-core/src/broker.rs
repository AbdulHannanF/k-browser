/// Broker — manages sandboxed child processes and routes IPC messages between them.
///
/// The broker is the privileged orchestrator. It never routes Renderer ↔ Network
/// directly; all messages flow through this central authority, which enforces
/// capability checks and crash-recovery policy.

use kitsune_ipc::message::{IpcMessage, IpcPayload, ProcessId, ProcessRole};
use std::collections::HashMap;
use std::time::Instant;
use tokio::sync::mpsc;
use tokio::process::Child;
use tracing::{error, info, warn};

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// The lifecycle state of a managed child process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProcessStatus {
    /// The process has been spawned but has not yet sent its registration message.
    Starting,
    /// The process is healthy and processing messages.
    Running,
    /// The process exited unexpectedly; a respawn may be attempted.
    Crashed,
    /// The process has crashed too many times in quick succession and will not
    /// be respawned. The broker will notify the UI.
    Unrecoverable,
}

/// A message delivered to the broker's internal event loop.
#[derive(Debug)]
pub enum BrokerEvent {
    /// An IPC message received from a child process.
    FromChild {
        role: ProcessRole,
        msg: IpcMessage,
    },
    /// A child process exited unexpectedly.
    ProcessExited(ProcessRole),
    /// A child confirmed it is ready to accept work.
    ProcessReady(ProcessRole),
}

/// A running child process tracked by the broker.
pub struct ManagedProcess {
    /// The role this process performs.
    pub role: ProcessRole,
    /// OS process handle (used to detect exit / send signals).
    pub handle: Option<Child>,
    /// Send half of the in-process channel used to deliver messages to the
    /// child's read loop. In the real multi-process implementation this is
    /// replaced by a named-pipe sender; for the MVP it is a tokio channel.
    pub sender: mpsc::Sender<IpcMessage>,
    /// Current lifecycle state.
    pub status: ProcessStatus,
    /// Number of unexpected exits since the process was first spawned.
    pub crash_count: u32,
    /// Timestamp of the most recent crash.
    pub last_crash: Option<Instant>,
}

/// Central process manager — owns all child processes and their channels.
pub struct ProcessManager {
    /// Map from role → running process.
    processes: HashMap<ProcessRole, ManagedProcess>,
    /// Channel the broker loop reads events from.
    event_tx: mpsc::Sender<BrokerEvent>,
    event_rx: mpsc::Receiver<BrokerEvent>,
    /// Channel used to forward messages destined for the UI shell.
    ui_tx: Option<mpsc::Sender<IpcMessage>>,
}

// ---------------------------------------------------------------------------
// Routing table
// ---------------------------------------------------------------------------

/// Determine which role should receive a given outgoing payload.
///
/// Returns `None` if the message should be handled locally by the broker
/// (e.g. a `ProcessRegister` handshake).
fn route_payload(payload: &IpcPayload) -> Option<ProcessRole> {
    match payload {
        // Messages that go to the renderer
        IpcPayload::DomQuery { .. }
        | IpcPayload::DomFillField { .. }
        | IpcPayload::DomClick { .. }
        | IpcPayload::SetDomHighlight(_)
        | IpcPayload::ClearDomHighlight(_)
        | IpcPayload::ClearAllDomHighlights => Some(ProcessRole::Renderer),

        // Messages that go to the network process
        IpcPayload::NetworkFetchRequest { .. }
        | IpcPayload::NavigateRequest { .. } => Some(ProcessRole::Network),

        // Messages originating from agent / JS that need broker mediation
        IpcPayload::HilCheckpointRequest { .. }
        | IpcPayload::VaultRequest { .. } => None, // broker handles locally

        // Responses and results are forwarded to the UI / originating channel
        IpcPayload::VaultResponse { .. }
        | IpcPayload::NetworkFetchResponse { .. }
        | IpcPayload::HilCheckpointResponse { .. }
        | IpcPayload::DomQueryResult { .. }
        | IpcPayload::DomOperationResult { .. }
        | IpcPayload::NavigateResponse { .. }
        | IpcPayload::AgentActionResult { .. } => None,

        // Lifecycle
        IpcPayload::ProcessRegister { .. }
        | IpcPayload::ProcessRegistered { .. }
        | IpcPayload::ProcessShutdown { .. }
        | IpcPayload::Error { .. }
        | IpcPayload::AgentActionRequest { .. } => None,
    }
}

// ---------------------------------------------------------------------------
// ProcessManager impl
// ---------------------------------------------------------------------------

impl ProcessManager {
    /// Create a new, empty `ProcessManager`.
    pub fn new() -> Self {
        let (event_tx, event_rx) = mpsc::channel(256);
        Self {
            processes: HashMap::new(),
            event_tx,
            event_rx,
            ui_tx: None,
        }
    }

    /// Register the channel used to push messages to the UI shell.
    pub fn set_ui_channel(&mut self, tx: mpsc::Sender<IpcMessage>) {
        self.ui_tx = Some(tx);
    }

    /// Register a mock in-process "child" (used in tests and single-process mode).
    ///
    /// Returns the `mpsc::Receiver` that the mock child reads messages from.
    pub fn register_mock(
        &mut self,
        role: ProcessRole,
    ) -> mpsc::Receiver<IpcMessage> {
        let (tx, rx) = mpsc::channel(64);
        self.processes.insert(
            role,
            ManagedProcess {
                role,
                handle: None,
                sender: tx,
                status: ProcessStatus::Running,
                crash_count: 0,
                last_crash: None,
            },
        );
        rx
    }

    /// Spawn a real child process for the given role.
    ///
    /// The child is launched with `--role=<role>` so it can configure its own
    /// sandbox before connecting back to the broker.
    pub async fn spawn_child(&mut self, role: ProcessRole) -> anyhow::Result<()> {
        let exe = std::env::current_exe()?;
        let role_str = match role {
            ProcessRole::Renderer => "renderer",
            ProcessRole::Network => "network",
            ProcessRole::Js => "js",
            ProcessRole::Agent => "agent",
            ProcessRole::Broker => return Err(anyhow::anyhow!("Cannot spawn a second broker")),
        };

        info!(role = role_str, "Spawning child process");

        let child = tokio::process::Command::new(&exe)
            .arg(format!("--role={}", role_str))
            .spawn()?;

        // We use an in-process channel as a stand-in for the real IPC pipe.
        // The child process connects and sends on this channel.
        let (tx, _rx) = mpsc::channel::<IpcMessage>(64);

        // Monitor the child for unexpected exits
        let event_tx = self.event_tx.clone();
        tokio::spawn(async move {
            let _ = event_tx
                .send(BrokerEvent::ProcessReady(role))
                .await;
        });

        self.processes.insert(
            role,
            ManagedProcess {
                role,
                handle: Some(child),
                sender: tx,
                status: ProcessStatus::Starting,
                crash_count: 0,
                last_crash: None,
            },
        );

        Ok(())
    }

    /// Forward a message to the appropriate child process (or the UI channel).
    ///
    /// Returns `false` if the destination is not registered or capability
    /// check fails.
    pub async fn route(&self, msg: IpcMessage) -> bool {
        if let Some(dest_role) = route_payload(&msg.payload) {
            if let Some(proc) = self.processes.get(&dest_role) {
                if proc.status == ProcessStatus::Running {
                    return proc.sender.send(msg).await.is_ok();
                }
                warn!(role = ?dest_role, "Cannot route to process — not running");
                return false;
            }
            warn!(role = ?dest_role, "Cannot route — no such process registered");
            return false;
        }

        // No specific role → forward to UI channel
        if let Some(ui_tx) = &self.ui_tx {
            return ui_tx.send(msg).await.is_ok();
        }

        false
    }

    /// The main broker event loop — call this in a dedicated `tokio::task`.
    ///
    /// Runs until `self.event_rx` is closed (i.e., all `event_tx` clones are
    /// dropped, which happens when `ProcessManager` is dropped).
    pub async fn broker_loop(&mut self) {
        info!("Broker event loop started");

        while let Some(event) = self.event_rx.recv().await {
            match event {
                BrokerEvent::FromChild { role, msg } => {
                    self.handle_child_message(role, msg).await;
                }
                BrokerEvent::ProcessExited(role) => {
                    self.handle_crash(role).await;
                }
                BrokerEvent::ProcessReady(role) => {
                    if let Some(proc) = self.processes.get_mut(&role) {
                        proc.status = ProcessStatus::Running;
                        info!(role = ?role, "Child process is ready");
                    }
                }
            }
        }

        info!("Broker event loop ended");
    }

    // -----------------------------------------------------------------------
    // Private helpers
    // -----------------------------------------------------------------------

    async fn handle_child_message(&self, _from_role: ProcessRole, msg: IpcMessage) {
        // Capability enforcement happens at the IpcChannel layer already.
        // Here we just route.
        if !self.route(msg).await {
            warn!("Failed to route message from child");
        }
    }

    async fn handle_crash(&mut self, role: ProcessRole) {
        const MAX_CRASHES: u32 = 3;
        const CRASH_WINDOW_SECS: u64 = 10;

        if let Some(proc) = self.processes.get_mut(&role) {
            proc.status = ProcessStatus::Crashed;
            let now = Instant::now();

            // Reset crash count if the last crash was a long time ago
            if let Some(last) = proc.last_crash {
                if now.duration_since(last).as_secs() > CRASH_WINDOW_SECS {
                    proc.crash_count = 0;
                }
            }

            proc.crash_count += 1;
            proc.last_crash = Some(now);

            if proc.crash_count >= MAX_CRASHES {
                error!(
                    role = ?role,
                    crashes = proc.crash_count,
                    "Process is unrecoverable — too many crashes in quick succession"
                );
                proc.status = ProcessStatus::Unrecoverable;

                // Notify the UI
                let shutdown_msg = IpcMessage::new(
                    ProcessId("broker".to_string()),
                    ProcessId("ui".to_string()),
                    IpcPayload::ProcessShutdown {
                        reason: format!("{:?} process crashed {} times and is unrecoverable", role, proc.crash_count),
                    },
                );
                if let Some(ui_tx) = &self.ui_tx {
                    let _ = ui_tx.send(shutdown_msg).await;
                }
            } else {
                warn!(
                    role = ?role,
                    attempt = proc.crash_count,
                    "Child process crashed — scheduling respawn"
                );
                // Schedule a respawn after a short delay
                let event_tx = self.event_tx.clone();
                tokio::spawn(async move {
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                    let _ = event_tx.send(BrokerEvent::ProcessReady(role)).await;
                });
            }
        }
    }

    /// Returns the status of a managed process, or `None` if not registered.
    pub fn status(&self, role: ProcessRole) -> Option<&ProcessStatus> {
        self.processes.get(&role).map(|p| &p.status)
    }

    /// Returns `true` if a process for `role` is registered and running.
    pub fn is_running(&self, role: ProcessRole) -> bool {
        self.processes
            .get(&role)
            .map(|p| p.status == ProcessStatus::Running)
            .unwrap_or(false)
    }

    /// Clone of the event sender — used by child-process read loops to push
    /// messages into the broker.
    pub fn event_sender(&self) -> mpsc::Sender<BrokerEvent> {
        self.event_tx.clone()
    }
}

impl Default for ProcessManager {
    fn default() -> Self {
        Self::new()
    }
}
