/// Sandbox error types.
use thiserror::Error;

pub type SandboxResult<T> = Result<T, SandboxError>;

#[derive(Debug, Error)]
pub enum SandboxError {
    #[error("Failed to create sandbox: {0}")]
    CreationFailed(String),

    #[error("Sandbox policy violation: {0}")]
    PolicyViolation(String),

    #[error("Platform sandbox not available: {0}")]
    PlatformUnavailable(String),

    #[error("Sandbox error: {0}")]
    Internal(String),
}
