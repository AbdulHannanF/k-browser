use thiserror::Error;
pub type JsResult<T> = Result<T, JsError>;

#[derive(Debug, Error)]
pub enum JsError {
    #[error("JavaScript execution error: {0}")]
    ExecutionError(String),
    #[error("Script parse error at line {line}: {message}")]
    ParseError { line: u32, message: String },
    #[error("JS engine initialization failed: {0}")]
    InitializationError(String),
    #[error("Security violation: script attempted restricted operation '{operation}'")]
    SecurityViolation { operation: String },
    #[error("Script timed out after {timeout_ms}ms")]
    Timeout { timeout_ms: u64 },
}
