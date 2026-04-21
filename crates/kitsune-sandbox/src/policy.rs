/// Sandbox policy types and validation.

use serde::{Deserialize, Serialize};

/// A sandbox policy rule.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxRule {
    /// Rule name.
    pub name: String,
    /// Whether this rule is enforced (vs. audit-only).
    pub enforced: bool,
    /// Description of the rule.
    pub description: String,
}

/// Validate a sandbox profile before applying it.
pub fn validate_profile(profile: &super::SandboxProfile) -> Vec<String> {
    let mut warnings = Vec::new();

    if profile.memory_limit_bytes == 0 {
        warnings.push("No memory limit set — process may consume unlimited memory".to_string());
    }

    if profile.allow_process_spawn {
        warnings.push("Process spawning is allowed — sandboxed process can create children".to_string());
    }

    if matches!(profile.allow_network, super::NetworkPolicy::Outbound { .. }) && profile.name != "Network" {
        warnings.push("Outbound network access granted to non-network process".to_string());
    }

    warnings
}
