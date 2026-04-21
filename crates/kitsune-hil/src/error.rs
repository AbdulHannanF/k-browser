/// HIL error types.
use thiserror::Error;

/// Result type for HIL operations.
pub type HilResult<T> = Result<T, HilError>;

/// Errors that can occur in the HIL gate system.
#[derive(Debug, Error)]
pub enum HilError {
    /// The HIL approval has expired (30-second window).
    #[error("HIL approval has expired — confirmations are valid for 30 seconds")]
    ApprovalExpired,

    /// The HIL approval was for a different action.
    #[error("HIL approval token is bound to action '{expected}', not '{actual}'")]
    ApprovalMismatch { expected: String, actual: String },

    /// The user rejected the action.
    #[error("User rejected the action: {reason}")]
    UserRejected { reason: String },

    /// The HIL dialog was dismissed without a decision.
    #[error("HIL confirmation was dismissed without a decision")]
    Dismissed,

    /// The HIL gate timed out waiting for user input.
    #[error("HIL confirmation timed out after {timeout_seconds} seconds")]
    Timeout { timeout_seconds: u64 },

    /// Internal error in the HIL system.
    #[error("Internal HIL error: {0}")]
    Internal(String),
}
