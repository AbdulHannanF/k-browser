/// IPC error types for KitsuneEngine inter-process communication.
use thiserror::Error;

/// Result type alias for IPC operations.
pub type IpcResult<T> = Result<T, IpcError>;

/// Errors that can occur during IPC operations.
#[derive(Debug, Error)]
pub enum IpcError {
    /// The target process is not reachable.
    #[error("Target process '{process_id}' is not reachable")]
    ProcessUnreachable { process_id: String },

    /// Message decoding failed.
    #[error("Failed to decode IPC message: {0}")]
    Decode(#[from] postcard::Error),

    /// IO Error
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// Process disconnected
    #[error("Process disconnected: {0:?}")]
    Disconnected(crate::message::ProcessRole),

    /// Channel is closed.
    #[error("IPC channel is closed")]
    ChannelClosed,

    /// Message timed out waiting for response.
    #[error("IPC message timed out after {timeout_ms}ms")]
    Timeout { timeout_ms: u64 },

    /// Permission denied — the requesting process lacks the required capability.
    #[error(
        "Permission denied: role '{role:?}' lacks required privilege '{required_privilege:?}'"
    )]
    PermissionDenied {
        role: crate::message::ProcessRole,
        required_privilege: crate::message::PrivilegeLevel,
    },

    /// Internal IPC infrastructure error.
    #[error("Internal IPC error: {0}")]
    Internal(String),
}
