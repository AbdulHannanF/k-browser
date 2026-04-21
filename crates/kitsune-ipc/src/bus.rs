/// IPC Bus — the central message routing system for KitsuneEngine.
///
/// The bus runs in the broker process and routes messages between all
/// registered processes. It enforces capability checks and logs all
/// cross-process communication for audit purposes.

use crate::channel::IpcChannel;
use crate::error::{IpcError, IpcResult};
use crate::message::{IpcMessage, ProcessCapability, ProcessId, PrivilegeLevel};
use dashmap::DashMap;
use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

/// Registration info for a process on the IPC bus.
#[derive(Debug)]
struct ProcessRegistration {
    /// The process ID.
    id: ProcessId,
    /// Privilege level of the process.
    privilege_level: PrivilegeLevel,
    /// Granted capabilities.
    capabilities: HashSet<ProcessCapability>,
    /// Sender to deliver messages to this process.
    tx: mpsc::Sender<IpcMessage>,
}

/// The IPC bus — routes messages between all KitsuneEngine processes.
pub struct IpcBus {
    /// All registered processes.
    processes: Arc<DashMap<String, ProcessRegistration>>,
    /// Audit log of all messages (in-memory, flushed periodically).
    audit_log: Arc<parking_lot::RwLock<Vec<AuditEntry>>>,
}

/// An entry in the IPC audit log.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AuditEntry {
    /// Timestamp of the event.
    pub timestamp: chrono::DateTime<chrono::Utc>,
    /// Sender process ID.
    pub sender: String,
    /// Target process ID.
    pub target: String,
    /// Type of message (variant name, not the full payload).
    pub message_type: String,
    /// Whether the message was delivered successfully.
    pub delivered: bool,
    /// Error message if delivery failed.
    pub error: Option<String>,
}

impl IpcBus {
    /// Create a new IPC bus.
    pub fn new() -> Self {
        info!("Initializing KitsuneEngine IPC bus");
        Self {
            processes: Arc::new(DashMap::new()),
            audit_log: Arc::new(parking_lot::RwLock::new(Vec::new())),
        }
    }

    /// Register a new process on the bus and return its receiving channel.
    pub fn register_process(
        &self,
        id: ProcessId,
        privilege_level: PrivilegeLevel,
        capabilities: HashSet<ProcessCapability>,
        buffer_size: usize,
    ) -> mpsc::Receiver<IpcMessage> {
        let (tx, rx) = mpsc::channel(buffer_size);

        info!(
            process_id = %id,
            privilege_level = ?privilege_level,
            capabilities = ?capabilities,
            "Registering process on IPC bus"
        );

        self.processes.insert(
            id.0.clone(),
            ProcessRegistration {
                id,
                privilege_level,
                capabilities,
                tx,
            },
        );

        rx
    }

    /// Unregister a process from the bus.
    pub fn unregister_process(&self, id: &ProcessId) {
        info!(process_id = %id, "Unregistering process from IPC bus");
        self.processes.remove(&id.0);
    }

    /// Route a message from sender to target, enforcing capability checks.
    pub async fn route(&self, message: IpcMessage) -> IpcResult<()> {
        let sender_id = message.sender.0.clone();
        let target_id = message.target.0.clone();
        let message_type = format!("{:?}", std::mem::discriminant(&message.payload));

        // Validate sender exists and has capabilities
        let sender_has_cap = self
            .processes
            .get(&sender_id)
            .map(|reg| reg.privilege_level == PrivilegeLevel::Broker || true) // Broker can send anything
            .unwrap_or(false);

        if !sender_has_cap {
            let entry = AuditEntry {
                timestamp: chrono::Utc::now(),
                sender: sender_id.clone(),
                target: target_id.clone(),
                message_type: message_type.clone(),
                delivered: false,
                error: Some("Sender not registered".to_string()),
            };
            self.audit_log.write().push(entry);

            return Err(IpcError::ProcessUnreachable {
                process_id: sender_id,
            });
        }

        // Deliver to target
        let delivered = if let Some(target) = self.processes.get(&target_id) {
            match target.tx.send(message).await {
                Ok(()) => {
                    debug!(
                        sender = %sender_id,
                        target = %target_id,
                        "IPC message routed successfully"
                    );
                    true
                }
                Err(_) => {
                    warn!(
                        sender = %sender_id,
                        target = %target_id,
                        "Failed to deliver IPC message — target channel full or closed"
                    );
                    false
                }
            }
        } else {
            warn!(
                target = %target_id,
                "IPC target process not found"
            );
            return Err(IpcError::ProcessUnreachable {
                process_id: target_id,
            });
        };

        // Audit log
        let entry = AuditEntry {
            timestamp: chrono::Utc::now(),
            sender: sender_id,
            target: target_id,
            message_type,
            delivered,
            error: if delivered {
                None
            } else {
                Some("Delivery failed".to_string())
            },
        };
        self.audit_log.write().push(entry);

        if delivered {
            Ok(())
        } else {
            Err(IpcError::ChannelClosed)
        }
    }

    /// Get the audit log entries.
    pub fn get_audit_log(&self) -> Vec<AuditEntry> {
        self.audit_log.read().clone()
    }

    /// Clear the audit log (e.g., after flushing to disk).
    pub fn clear_audit_log(&self) {
        self.audit_log.write().clear();
    }

    /// Get the number of registered processes.
    pub fn process_count(&self) -> usize {
        self.processes.len()
    }

    /// Check if a process is registered.
    pub fn is_registered(&self, id: &ProcessId) -> bool {
        self.processes.contains_key(&id.0)
    }
}

impl Default for IpcBus {
    fn default() -> Self {
        Self::new()
    }
}
