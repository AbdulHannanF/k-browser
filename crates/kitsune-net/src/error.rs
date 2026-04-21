/// Network error types.
use thiserror::Error;

pub type NetResult<T> = Result<T, NetError>;

#[derive(Debug, Error)]
pub enum NetError {
    #[error("Network request failed: {0}")]
    RequestFailed(String),

    #[error("TLS error: {0}")]
    TlsError(String),

    #[error("DNS resolution failed for '{domain}'")]
    DnsResolutionFailed { domain: String },

    #[error("Connection refused by '{domain}'")]
    ConnectionRefused { domain: String },

    #[error("Request timed out after {timeout_ms}ms")]
    Timeout { timeout_ms: u64 },

    #[error("Tracker domain blocked: {domain}")]
    TrackerBlocked { domain: String },

    #[error("TLS version {version:?} is below minimum required")]
    TlsVersionTooLow { version: super::TlsVersion },

    #[error("Certificate verification failed for '{domain}': {reason}")]
    CertificateError { domain: String, reason: String },

    #[error("Internal network error: {0}")]
    Internal(String),

    #[error("Connection failed: {0}")]
    ConnectionFailed(String),
}
