use kitsune_vault::error::VaultError;
use thiserror::Error;
pub type AgentResult<T> = Result<T, AgentError>;

#[derive(Debug, Error)]
pub enum AgentError {
    #[error("Agent '{agent_id}' not found")]
    AgentNotFound { agent_id: String },

    #[error("Agent budget exceeded: spent {spent}, limit {limit}")]
    BudgetExceeded { spent: String, limit: String },

    #[error("Agent action limit reached: {current}/{max} actions this session")]
    ActionLimitReached { current: u32, max: u32 },

    #[error("Agent does not have permission: {capability}")]
    PermissionDenied { capability: String },

    #[error("Agent domain not allowed: {domain}")]
    DomainNotAllowed { domain: String },

    #[error("Agent execution error: {0}")]
    ExecutionError(String),

    #[error("HIL approval required for this action")]
    HilRequired,

    #[error("Vault access denied: {0}")]
    VaultAccessDenied(String),

    #[error("Agent tool error: {0}")]
    ToolError(String),

    #[error("Internal agent error: {0}")]
    Internal(String),

    #[error("IPC channel disconnected")]
    IpcDisconnected,

    #[error("Invalid parameter '{param}': {reason}")]
    InvalidParameters { param: String, reason: String },
}

impl From<VaultError> for AgentError {
    fn from(error: VaultError) -> Self {
        AgentError::VaultAccessDenied(error.to_string())
    }
}

