/// Granted access types — the vault NEVER returns raw secrets.
///
/// When an agent or form fill requests a credential, the vault evaluates
/// the disclosure policy and returns a GrantedAccess variant that either:
/// - Provides an opaque token the agent can use (vault handles substitution)
/// - Performs a direct DOM injection (agent never sees the value)
/// - Returns only metadata (no credential data at all)

use crate::types::{DomFieldId, TokenHandle, VaultKeyMetadata};
use serde::{Deserialize, Serialize};

/// The result of a successful vault access request.
///
/// The caller NEVER receives the raw secret value. The vault handles
/// the credential substitution internally.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GrantedAccess {
    /// A one-time token the agent can use.
    /// The vault tracks this token and handles the actual substitution
    /// (e.g., writing the password into an HTTP request body) internally.
    /// The agent receives this handle but never sees the raw value.
    OpaqueToken(TokenHandle),

    /// A DOM injection — the vault writes directly into a form field
    /// via secure IPC to the renderer process. The agent is notified
    /// of success/failure but never sees the raw value.
    DomInjection {
        /// The form field that was filled.
        field_id: DomFieldId,
        /// Whether the injection succeeded.
        success: bool,
    },

    /// For read-only metadata queries (e.g., "does user have a payment method?").
    /// No credential data is included.
    MetadataOnly(VaultEntryMetadata),
}

/// Metadata about a vault entry — safe to share with agents.
/// Contains no actual credential data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultEntryMetadata {
    /// Human-readable label.
    pub label: String,
    /// Category of the entry.
    pub category: String,
    /// Whether the entry has a value stored.
    pub has_value: bool,
    /// When the entry was last modified.
    pub last_modified: chrono::DateTime<chrono::Utc>,
    /// Plain-English policy description.
    pub policy_description: String,
}
