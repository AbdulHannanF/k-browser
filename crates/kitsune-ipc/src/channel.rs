/// IPC channel — provides typed, async communication between processes.
///
/// Channels enforce capability checks before allowing message delivery.
/// A sandboxed process cannot send a VaultRequest unless it has been
/// granted the VaultRead capability by the broker.
use crate::error::{IpcError, IpcResult};
use crate::message::{IpcMessage, IpcPayload, ProcessCapability, ProcessId};
use std::collections::HashSet;
use tokio::sync::{mpsc, oneshot};
use tracing::{debug, warn};

/// A bidirectional IPC channel endpoint.
#[derive(Debug)]
pub struct IpcChannel {
    /// The process that owns this channel endpoint.
    pub owner: ProcessId,
    /// Capabilities granted to this channel's owner.
    pub capabilities: HashSet<ProcessCapability>,
    /// Sender for outgoing messages.
    outgoing_tx: mpsc::Sender<IpcMessage>,
    /// Receiver for incoming messages.
    incoming_rx: mpsc::Receiver<IpcMessage>,
}

impl IpcChannel {
    /// Create a new IPC channel pair (local, remote) for two processes.
    pub fn pair(
        local_id: ProcessId,
        remote_id: ProcessId,
        local_capabilities: HashSet<ProcessCapability>,
        remote_capabilities: HashSet<ProcessCapability>,
        buffer_size: usize,
    ) -> (IpcChannel, IpcChannel) {
        let (local_tx, remote_rx) = mpsc::channel(buffer_size);
        let (remote_tx, local_rx) = mpsc::channel(buffer_size);

        let local = IpcChannel {
            owner: local_id,
            capabilities: local_capabilities,
            outgoing_tx: local_tx,
            incoming_rx: local_rx,
        };

        let remote = IpcChannel {
            owner: remote_id,
            capabilities: remote_capabilities,
            outgoing_tx: remote_tx,
            incoming_rx: remote_rx,
        };

        (local, remote)
    }

    /// Send a message through this channel after validating capabilities.
    pub async fn send(&self, message: IpcMessage) -> IpcResult<()> {
        // Validate that the sender has the required capability for this payload
        self.validate_capability(&message.payload)?;

        debug!(
            sender = %message.sender,
            target = %message.target,
            correlation_id = %message.correlation_id.0,
            "Sending IPC message"
        );

        self.outgoing_tx
            .send(message)
            .await
            .map_err(|_| IpcError::ChannelClosed)
    }

    /// Receive the next message from this channel.
    pub async fn recv(&mut self) -> IpcResult<IpcMessage> {
        self.incoming_rx.recv().await.ok_or(IpcError::ChannelClosed)
    }

    /// Try to receive a message without blocking.
    pub fn try_recv(&mut self) -> IpcResult<IpcMessage> {
        self.incoming_rx
            .try_recv()
            .map_err(|_| IpcError::ChannelClosed)
    }

    /// Validate that the channel owner has the required capability for a payload.
    fn validate_capability(&self, payload: &IpcPayload) -> IpcResult<()> {
        let required = Self::required_capability(payload);

        if let Some(cap) = required {
            if !self.capabilities.contains(&cap) {
                warn!(
                    process = %self.owner,
                    required_capability = ?cap,
                    "IPC capability check failed"
                );
                return Err(IpcError::PermissionDenied {
                    role: self.owner.0.clone().into(),
                    required_privilege: crate::message::PrivilegeLevel::Sandboxed,
                });
            }
        }

        Ok(())
    }

    /// Determine the required capability for a given message payload.
    fn required_capability(payload: &IpcPayload) -> Option<ProcessCapability> {
        match payload {
            IpcPayload::VaultRequest { .. } => Some(ProcessCapability::VaultRead),
            IpcPayload::VaultResponse { .. } => None, // Broker sends these
            IpcPayload::NetworkFetchRequest { .. } => Some(ProcessCapability::NetworkAccess),
            IpcPayload::NetworkFetchResponse { .. } => None,
            IpcPayload::HilCheckpointRequest { .. } => Some(ProcessCapability::HilTrigger),
            IpcPayload::HilCheckpointResponse { .. } => None,
            IpcPayload::DomQuery { .. } => Some(ProcessCapability::DomAccess),
            IpcPayload::DomQueryResult { .. } => None,
            IpcPayload::DomFillField { .. } => Some(ProcessCapability::DomAccess),
            IpcPayload::DomClick { .. } => Some(ProcessCapability::DomAccess),
            IpcPayload::DomOperationResult { .. } => None,
            IpcPayload::SetDomHighlight(_) => Some(ProcessCapability::DomAccess),
            IpcPayload::ClearDomHighlight(_) => Some(ProcessCapability::DomAccess),
            IpcPayload::ClearAllDomHighlights => Some(ProcessCapability::DomAccess),
            IpcPayload::NavigateRequest { .. } => Some(ProcessCapability::NetworkAccess),
            IpcPayload::NavigateResponse { .. } => None,
            IpcPayload::ProcessRegister { .. } => None,
            IpcPayload::ProcessRegistered { .. } => None,
            IpcPayload::ProcessShutdown { .. } => None,
            IpcPayload::AgentActionRequest { .. } => Some(ProcessCapability::AgentRuntime),
            IpcPayload::AgentActionResult { .. } => None,
            IpcPayload::Error { .. } => None,
        }
    }
}

/// A pending request tracker — stores oneshot senders for request-response patterns.
pub struct PendingRequests {
    pending: dashmap::DashMap<uuid::Uuid, oneshot::Sender<IpcMessage>>,
}

impl PendingRequests {
    /// Create a new pending request tracker.
    pub fn new() -> Self {
        Self {
            pending: dashmap::DashMap::new(),
        }
    }

    /// Register a new pending request and return the receiver.
    pub fn register(&self, correlation_id: uuid::Uuid) -> oneshot::Receiver<IpcMessage> {
        let (tx, rx) = oneshot::channel();
        self.pending.insert(correlation_id, tx);
        rx
    }

    /// Resolve a pending request with a response message.
    pub fn resolve(&self, correlation_id: &uuid::Uuid, message: IpcMessage) -> bool {
        if let Some((_, tx)) = self.pending.remove(correlation_id) {
            tx.send(message).is_ok()
        } else {
            false
        }
    }
}

impl Default for PendingRequests {
    fn default() -> Self {
        Self::new()
    }
}
