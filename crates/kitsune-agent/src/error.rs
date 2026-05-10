use kitsune_hil::HilError;
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

    #[error("LLM unavailable: {0}")]
    LlmUnavailable(String),

    #[error("HIL approval required for this action")]
    HilRequired,

    #[error("HIL checkpoint rejected by user: {0}")]
    HilRejected(String),

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

    #[error("Swarm coordinator failed: {0}")]
    SwarmCoordinatorFailed(String),

    #[error("Swarm worker '{worker_id}' failed: {reason}")]
    SwarmWorkerFailed { worker_id: String, reason: String },

    #[error("Agent operation cancelled")]
    Cancelled,
}

impl From<VaultError> for AgentError {
    fn from(error: VaultError) -> Self {
        AgentError::VaultAccessDenied(error.to_string())
    }
}

impl From<HilError> for AgentError {
    fn from(error: HilError) -> Self {
        match error {
            HilError::UserRejected { reason } => AgentError::HilRejected(reason),
            HilError::Dismissed => AgentError::HilRejected("Dismissed".to_string()),
            e => AgentError::Internal(e.to_string()),
        }
    }
}
