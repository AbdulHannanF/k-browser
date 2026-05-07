/// Disclosure policies — govern when and how vault entries can be disclosed.
///
/// Every vault entry carries a DisclosurePolicy. This policy is evaluated
/// BEFORE any value is returned, and the vault NEVER bypasses the policy.
use crate::types::{AgentId, DomainPattern};
use serde::{Deserialize, Serialize};

/// A disclosure policy that governs how a vault entry can be used.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DisclosurePolicy {
    /// Entry is NEVER sent to any external party — only used locally (e.g., master password).
    NeverDisclose,

    /// Entry can be used to fill a form field locally, but the value is never sent
    /// to an agent or logged. The vault writes directly into the DOM.
    LocalFormFillOnly {
        /// Domains where this form fill is allowed.
        allowed_domains: Vec<DomainPattern>,
    },

    /// Entry can be passed to an agent ONLY after HIL confirmation,
    /// and only as a one-time token (never raw value).
    AgentAccessWithHIL {
        /// Agents allowed to request this entry.
        allowed_agents: Vec<AgentId>,
        /// Maximum number of times this entry can be used.
        max_uses: u32,
        /// Current usage count.
        current_uses: u32,
    },

    /// Entry can be used for automated fill on trusted, pre-approved domains
    /// without HIL (e.g., a user's own app where they've built the agent).
    TrustedAutomation {
        /// Domains where automated fill is allowed.
        allowed_domains: Vec<DomainPattern>,
        /// Agents allowed to use this entry.
        allowed_agents: Vec<AgentId>,
        /// Whether biometric confirmation is required.
        require_biometric: bool,
    },
}

impl DisclosurePolicy {
    /// Get a plain-English description of this policy.
    pub fn describe(&self) -> String {
        match self {
            Self::NeverDisclose => {
                "This is stored securely on your device and is never shared with any website or assistant.".to_string()
            }
            Self::LocalFormFillOnly { allowed_domains } => {
                if allowed_domains.is_empty() {
                    "Used to fill in forms on websites. Never shared with your assistants.".to_string()
                } else {
                    let domains: Vec<String> = allowed_domains.iter().map(|d| d.0.clone()).collect();
                    format!(
                        "Used to fill in forms on: {}. Never shared with your assistants.",
                        domains.join(", ")
                    )
                }
            }
            Self::AgentAccessWithHIL { allowed_agents, max_uses, current_uses } => {
                let remaining = max_uses - current_uses;
                format!(
                    "Can be used by your assistants, but only after you confirm each time. {} uses remaining.",
                    remaining
                )
            }
            Self::TrustedAutomation { allowed_domains, require_biometric, .. } => {
                let domains: Vec<String> = allowed_domains.iter().map(|d| d.0.clone()).collect();
                let bio = if *require_biometric {
                    " Requires fingerprint/face unlock."
                } else {
                    ""
                };
                format!(
                    "Automatically used on: {}.{}",
                    domains.join(", "),
                    bio
                )
            }
        }
    }

    /// Check if this policy allows access from the given domain.
    pub fn allows_domain(&self, domain: &str) -> bool {
        match self {
            Self::NeverDisclose => false,
            Self::LocalFormFillOnly { allowed_domains } => {
                allowed_domains.is_empty() || allowed_domains.iter().any(|p| p.matches(domain))
            }
            Self::AgentAccessWithHIL { .. } => true, // Domain doesn't matter for agent access
            Self::TrustedAutomation {
                allowed_domains, ..
            } => allowed_domains.iter().any(|p| p.matches(domain)),
        }
    }

    /// Check if this policy allows access from the given agent.
    pub fn allows_agent(&self, agent_id: &AgentId) -> bool {
        match self {
            Self::NeverDisclose => false,
            Self::LocalFormFillOnly { .. } => false,
            Self::AgentAccessWithHIL {
                allowed_agents,
                max_uses,
                current_uses,
            } => *current_uses < *max_uses && allowed_agents.iter().any(|a| a == agent_id),
            Self::TrustedAutomation { allowed_agents, .. } => {
                allowed_agents.iter().any(|a| a == agent_id)
            }
        }
    }

    /// Check if HIL approval is required for this policy.
    pub fn requires_hil(&self) -> bool {
        matches!(self, Self::AgentAccessWithHIL { .. })
    }

    /// Check if biometric confirmation is required.
    pub fn requires_biometric(&self) -> bool {
        matches!(
            self,
            Self::TrustedAutomation {
                require_biometric: true,
                ..
            }
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_never_disclose() {
        let policy = DisclosurePolicy::NeverDisclose;
        assert!(!policy.allows_domain("anything.com"));
        assert!(!policy.allows_agent(&AgentId::new("any")));
        assert!(!policy.requires_hil());
    }

    #[test]
    fn test_local_form_fill() {
        let policy = DisclosurePolicy::LocalFormFillOnly {
            allowed_domains: vec![
                DomainPattern::new("example.com"),
                DomainPattern::new("*.bank.com"),
            ],
        };
        assert!(policy.allows_domain("example.com"));
        assert!(policy.allows_domain("my.bank.com"));
        assert!(!policy.allows_domain("evil.com"));
        assert!(!policy.allows_agent(&AgentId::new("any")));
    }

    #[test]
    fn test_agent_access_with_hil() {
        let agent = AgentId::new("autofill-agent");
        let policy = DisclosurePolicy::AgentAccessWithHIL {
            allowed_agents: vec![agent.clone()],
            max_uses: 5,
            current_uses: 3,
        };
        assert!(policy.allows_agent(&agent));
        assert!(!policy.allows_agent(&AgentId::new("other")));
        assert!(policy.requires_hil());
    }

    #[test]
    fn test_agent_access_exhausted() {
        let agent = AgentId::new("autofill-agent");
        let policy = DisclosurePolicy::AgentAccessWithHIL {
            allowed_agents: vec![agent.clone()],
            max_uses: 5,
            current_uses: 5,
        };
        assert!(!policy.allows_agent(&agent)); // Max uses reached
    }

    #[test]
    fn test_policy_descriptions() {
        let never = DisclosurePolicy::NeverDisclose;
        assert!(never.describe().contains("never shared"));

        let form = DisclosurePolicy::LocalFormFillOnly {
            allowed_domains: vec![DomainPattern::new("gmail.com")],
        };
        assert!(form.describe().contains("gmail.com"));
    }
}
