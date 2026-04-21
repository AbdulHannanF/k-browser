//! Error types for `kitsune-ai`.

use crate::request::TaskType;

/// Alias for `Result<T, AiError>`.
pub type AiResult<T> = Result<T, AiError>;

/// All errors that can occur in the AI layer.
#[derive(Debug, thiserror::Error)]
pub enum AiError {
    /// User is not authenticated with KitsuneCloud.
    #[error("not authenticated — log in to KitsuneEngine account to use cloud AI")]
    NotAuthenticated,

    /// Free tier monthly action quota used up.
    #[error("quota exhausted — {actions_used}/{limit} actions used, resets {resets_at}")]
    QuotaExhausted {
        actions_used: u32,
        limit: u32,
        resets_at: String,
    },

    /// Local model is not downloaded or Pro tier not active.
    #[error("local model unavailable — download required or Pro tier needed")]
    LocalModelUnavailable,

    /// Local model timed out; caller should fall back to cloud.
    #[error("local model timeout after {ms}ms")]
    LocalTimeout { ms: u64 },

    /// Task type requires local but local is unavailable.
    #[error("task type {task:?} must use local model (security invariant) but local is unavailable")]
    RequiresLocal { task: TaskType },

    /// PII scrubbing pipeline failed.
    #[error("PII scrub error: {0}")]
    PiiScrubError(String),

    /// Cloud backend returned an error response.
    #[error("cloud request failed: HTTP {status} — {message}")]
    CloudError { status: u16, message: String },

    /// Local inference engine returned an error.
    #[error("local inference failed: {0}")]
    InferenceError(String),

    /// LoRA fine-tuning session failed.
    #[error("tuning failed at step {step}: {reason}")]
    TuningError { step: usize, reason: String },

    /// Model download from HuggingFace failed.
    #[error("model download failed: {0}")]
    DownloadError(String),

    /// Agent budget ceiling would be exceeded.
    #[error("budget exceeded — action would cost ${cost:.6}, remaining ${remaining:.6}")]
    BudgetExceeded { cost: f64, remaining: f64 },

    /// Network error (from reqwest).
    #[error("network error: {0}")]
    NetworkError(String),

    /// JSON parsing failed on server response.
    #[error("response parse error: {0}")]
    ParseError(String),

    /// OS keychain access failed.
    #[error("keychain error: {0}")]
    KeychainError(String),

    /// Quota persistence I/O error.
    #[error("quota cache I/O error: {0}")]
    IoError(String),
}

impl From<reqwest::Error> for AiError {
    fn from(e: reqwest::Error) -> Self {
        AiError::NetworkError(e.to_string())
    }
}

impl From<serde_json::Error> for AiError {
    fn from(e: serde_json::Error) -> Self {
        AiError::ParseError(e.to_string())
    }
}
