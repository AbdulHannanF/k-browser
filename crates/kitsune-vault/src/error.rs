/// Vault error types.
use thiserror::Error;

/// Result type for vault operations.
pub type VaultResult<T> = Result<T, VaultError>;

/// Errors that can occur in the privacy vault.
#[derive(Debug, Error)]
pub enum VaultError {
    /// The vault key was not found.
    #[error("Vault key '{key}' not found")]
    KeyNotFound { key: String },

    /// The requested operation violates the entry's disclosure policy.
    #[error("Disclosure policy violation: {reason}")]
    PolicyViolation { reason: String },

    /// The requester is not authorized to access this entry.
    #[error("Access denied for requester '{requester}': {reason}")]
    AccessDenied { requester: String, reason: String },

    /// HIL approval is required but was not provided.
    #[error("This operation requires your confirmation before proceeding")]
    HilRequired,

    /// The vault's encryption key could not be derived.
    #[error("Failed to derive encryption key: {0}")]
    KeyDerivationError(String),

    /// Secure enclave key storage failed and the vault refuses to fall back to unencrypted.
    /// INVARIANT: This causes a hard failure, never a silent fallback.
    #[error("Secure storage is not available on this device: {0}")]
    SecureStorageUnavailable(String),

    /// Encryption or decryption error.
    #[error("Encryption error: {0}")]
    CryptoError(String),

    /// Decryption failed, likely due to an incorrect password.
    #[error("Decryption failed: {0}")]
    DecryptionFailed(String),

    /// Serialization error.
    #[error("Data serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),

    /// Storage I/O error.
    #[error("Storage I/O error: {0}")]
    StorageError(String),

    /// The vault is locked and must be unlocked first.
    #[error("The vault is locked. Please enter your passphrase to unlock it.")]
    VaultLocked,

    /// Internal vault error.
    #[error("Internal vault error: {0}")]
    Internal(String),
}
