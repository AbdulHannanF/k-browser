/// Vault audit log — every access to the vault is logged.
///
/// The audit log is stored locally and can be exported to SIEM systems
/// in the enterprise tier. It records who accessed what, when, and whether
/// the access was granted or denied.
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// An entry in the vault audit log.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    /// Unique ID for this audit entry.
    pub id: Uuid,
    /// Timestamp of the event.
    pub timestamp: chrono::DateTime<chrono::Utc>,
    /// The vault key that was accessed (label, not value).
    pub key_label: String,
    /// The vault key category.
    pub key_category: String,
    /// Who made the request.
    pub requester: String,
    /// What type of access was requested.
    pub access_type: AuditAccessType,
    /// The result of the access attempt.
    pub result: AuditResult,
    /// The domain involved, if applicable.
    pub domain: Option<String>,
    /// The agent involved, if applicable.
    pub agent_id: Option<String>,
    /// Whether HIL approval was required and obtained.
    pub hil_status: Option<HilAuditStatus>,
}

/// Type of vault access that was attempted.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AuditAccessType {
    /// Read/retrieve a credential.
    Read,
    /// Store a new credential.
    Store,
    /// Update an existing credential.
    Update,
    /// Delete a credential.
    Delete,
    /// List available keys (metadata only).
    List,
    /// Form fill injection.
    FormFill,
    /// Agent token request.
    AgentTokenRequest,
}

/// Result of a vault access attempt.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AuditResult {
    /// Access was granted.
    Granted,
    /// Access was denied due to policy violation.
    DeniedByPolicy { reason: String },
    /// Access was denied because the user rejected the HIL confirmation.
    DeniedByUser,
    /// Access failed due to an error.
    Error { message: String },
}

/// HIL status in the context of a vault access.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HilAuditStatus {
    /// HIL was required and the user approved.
    RequiredAndApproved,
    /// HIL was required and the user rejected.
    RequiredAndRejected,
    /// HIL was not required for this access.
    NotRequired,
}

/// The vault audit log — append-only, locally stored.
#[derive(Debug)]
pub struct VaultAuditLog {
    entries: parking_lot::RwLock<Vec<AuditEntry>>,
}

impl VaultAuditLog {
    /// Create a new, empty audit log.
    pub fn new() -> Self {
        Self {
            entries: parking_lot::RwLock::new(Vec::new()),
        }
    }

    /// Record an audit entry.
    pub fn record(&self, entry: AuditEntry) {
        tracing::info!(
            key = %entry.key_label,
            requester = %entry.requester,
            access_type = ?entry.access_type,
            result = ?entry.result,
            "Vault audit: access recorded"
        );
        self.entries.write().push(entry);
    }

    /// Get all audit entries.
    pub fn entries(&self) -> Vec<AuditEntry> {
        self.entries.read().clone()
    }

    /// Get entries for a specific requester.
    pub fn entries_for_requester(&self, requester: &str) -> Vec<AuditEntry> {
        self.entries
            .read()
            .iter()
            .filter(|e| e.requester == requester)
            .cloned()
            .collect()
    }

    /// Get entries for a specific key label.
    pub fn entries_for_key(&self, label: &str) -> Vec<AuditEntry> {
        self.entries
            .read()
            .iter()
            .filter(|e| e.key_label == label)
            .cloned()
            .collect()
    }

    /// Get the total number of entries.
    pub fn len(&self) -> usize {
        self.entries.read().len()
    }

    /// Check if the audit log is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.read().is_empty()
    }

    /// Export the audit log as JSON (for enterprise SIEM integration).
    pub fn export_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(&*self.entries.read())
    }
}

impl Default for VaultAuditLog {
    fn default() -> Self {
        Self::new()
    }
}
