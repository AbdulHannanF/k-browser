/// Core vault types — keys, values, metadata, and identifiers.
///
/// All sensitive types implement Zeroize so they are scrubbed from memory
/// when dropped. Debug implementations use [REDACTED] for sensitive fields.

use serde::{Deserialize, Serialize};
use uuid::Uuid;
use zeroize::Zeroize;

/// A key in the vault — identifies a stored entry.
#[derive(Debug, Clone, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub struct VaultKey {
    /// Unique identifier for this key.
    pub id: Uuid,
    /// Human-readable label (e.g., "Work email — Gmail").
    pub label: String,
    /// Category of this entry.
    pub category: VaultCategory,
}

impl VaultKey {
    /// Create a new vault key.
    pub fn new(label: impl Into<String>, category: VaultCategory) -> Self {
        Self {
            id: Uuid::new_v4(),
            label: label.into(),
            category,
        }
    }
}

/// Categories for vault entries.
#[derive(Debug, Clone, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub enum VaultCategory {
    /// Login credentials (username + password).
    Password,
    /// Physical or mailing address.
    Address,
    /// Payment method (card, bank account).
    Payment,
    /// Personal information (name, DOB, etc.).
    Identity,
    /// API keys and tokens.
    ApiKey,
    /// Custom category defined by the user.
    Custom(String),
}

impl std::fmt::Display for VaultCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Password => write!(f, "Passwords"),
            Self::Address => write!(f, "Addresses"),
            Self::Payment => write!(f, "Payment Methods"),
            Self::Identity => write!(f, "Personal Info"),
            Self::ApiKey => write!(f, "API Keys"),
            Self::Custom(name) => write!(f, "{}", name),
        }
    }
}

impl std::str::FromStr for VaultCategory {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "Passwords" => Ok(Self::Password),
            "Addresses" => Ok(Self::Address),
            "Payment Methods" => Ok(Self::Payment),
            "Personal Info" => Ok(Self::Identity),
            "API Keys" => Ok(Self::ApiKey),
            _ => Ok(Self::Custom(s.to_string())),
        }
    }
}

/// A sensitive value stored in the vault.
///
/// # Security
/// - Implements `Zeroize` — scrubbed from memory on drop
/// - Debug output is always [REDACTED]
/// - Never logged, never serialized to disk without encryption
#[derive(Clone, Serialize, Deserialize, Zeroize)]
#[zeroize(drop)]
pub struct SensitiveValue {
    /// The encrypted value bytes.
    data: Vec<u8>,
}

impl SensitiveValue {
    /// Create a new sensitive value from raw bytes.
    pub fn new(data: Vec<u8>) -> Self {
        Self { data }
    }

    /// Create from a string (e.g., a password).
    pub fn from_string(s: impl Into<String>) -> Self {
        Self {
            data: s.into().into_bytes(),
        }
    }

    /// Get the raw bytes — this should ONLY be called by encryption/decryption routines.
    /// NEVER use this for logging, display, or IPC.
    pub(crate) fn as_bytes(&self) -> &[u8] {
        &self.data
    }

    /// Get the length of the value.
    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }
}

// SECURITY: Debug output NEVER includes the actual value
impl std::fmt::Debug for SensitiveValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "SensitiveValue([REDACTED], {} bytes)", self.data.len())
    }
}

/// Metadata about a vault entry (never contains the actual value).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultKeyMetadata {
    /// The vault key.
    pub key: VaultKey,
    /// When this entry was created.
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// When this entry was last modified.
    pub modified_at: chrono::DateTime<chrono::Utc>,
    /// When this entry was last accessed.
    pub last_accessed: Option<chrono::DateTime<chrono::Utc>>,
    /// Who last accessed this entry.
    pub last_accessed_by: Option<String>,
    /// Plain-English description of the disclosure policy.
    pub policy_description: String,
}

/// Unique identifier for a requester (agent, process, etc.).
#[derive(Debug, Clone, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub struct RequesterId(pub String);

impl RequesterId {
    /// Create a new requester ID.
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }
}

impl std::fmt::Display for RequesterId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Context about a vault access request — used for policy evaluation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestContext {
    /// The domain the request is being made for.
    pub domain: Option<String>,
    /// The purpose of the request in plain language.
    pub purpose: String,
    /// The agent making the request (if any).
    pub agent_id: Option<String>,
    /// Whether this request has HIL approval.
    pub has_hil_approval: bool,
    pub action_id: Uuid,
}

/// Unique identifier for a DOM form field.
#[derive(Debug, Clone, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub struct DomFieldId(pub String);

/// An opaque token handle — the vault uses this internally to track
/// one-time-use credential tokens without exposing the raw value.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenHandle {
    /// Unique token identifier.
    pub id: Uuid,
    /// When this token expires.
    pub expires_at: chrono::DateTime<chrono::Utc>,
    /// Whether this token has been consumed.
    pub consumed: bool,
}

impl TokenHandle {
    /// Create a new token handle with a 5-minute expiry.
    pub fn new() -> Self {
        Self {
            id: Uuid::new_v4(),
            expires_at: chrono::Utc::now() + chrono::Duration::minutes(5),
            consumed: false,
        }
    }

    /// Check if this token is still valid.
    pub fn is_valid(&self) -> bool {
        !self.consumed && chrono::Utc::now() < self.expires_at
    }
}

impl Default for TokenHandle {
    fn default() -> Self {
        Self::new()
    }
}

/// Unique identifier for an agent.
#[derive(Debug, Clone, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub struct AgentId(pub String);

impl AgentId {
    /// Create a new agent ID.
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }
}

/// A domain pattern for matching URLs (e.g., "*.example.com", "bank.com").
#[derive(Debug, Clone, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub struct DomainPattern(pub String);

/// A vault entry, as stored in the database.
#[derive(Debug, Clone)]
pub struct VaultEntry {
    pub id: Uuid,
    pub category: VaultCategory,
    pub label: String,
    pub origin_pseudonym: String,
    pub encrypted_value: Vec<u8>,
    pub created_at: i64,
    pub updated_at: i64,
}

impl DomainPattern {
    /// Create a new domain pattern.
    pub fn new(pattern: impl Into<String>) -> Self {
        Self(pattern.into())
    }

    /// Check if a domain matches this pattern.
    pub fn matches(&self, domain: &str) -> bool {
        if self.0.starts_with("*.") {
            let suffix = &self.0[2..];
            domain == suffix || domain.ends_with(&format!(".{}", suffix))
        } else {
            domain == self.0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_domain_pattern_exact() {
        let pattern = DomainPattern::new("example.com");
        assert!(pattern.matches("example.com"));
        assert!(!pattern.matches("sub.example.com"));
        assert!(!pattern.matches("notexample.com"));
    }

    #[test]
    fn test_domain_pattern_wildcard() {
        let pattern = DomainPattern::new("*.example.com");
        assert!(pattern.matches("sub.example.com"));
        assert!(pattern.matches("deep.sub.example.com"));
        assert!(pattern.matches("example.com"));
        assert!(!pattern.matches("notexample.com"));
    }

    #[test]
    fn test_sensitive_value_debug_redacted() {
        let val = SensitiveValue::from_string("super-secret-password");
        let debug = format!("{:?}", val);
        assert!(!debug.contains("super-secret-password"));
        assert!(debug.contains("[REDACTED]"));
    }

    #[test]
    fn test_token_handle_validity() {
        let token = TokenHandle::new();
        assert!(token.is_valid());
    }
}
