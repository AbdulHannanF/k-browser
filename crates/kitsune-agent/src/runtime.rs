/// Agent runtime — executes agents within safety constraints.
use crate::budget::BudgetTracker;
use crate::error::{AgentError, AgentResult};
use crate::executor::WebViewCommand;
use crate::spec::{
    AgentAuthor, AgentBudget, AgentConstraints, AgentGoal, AgentId, AgentSpec, AgentTool,
    DomainPolicy, VaultAccessLevel,
};
use crate::tools::*;
use kitsune_hil::HilGate;
use kitsune_vault::VaultBackend;
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{info, warn};
use url::Url;

/// Context provided to an executing agent.
pub struct AgentContext {
    pub dom: Arc<crate::dom_access::DomAccessor>,
}

/// The agent runtime — manages agent lifecycle and execution.
pub struct AgentRuntime {
    /// Currently loaded agents.
    agents: Vec<AgentInstance>,
    webview_tx: mpsc::Sender<WebViewCommand>,
    vault: Arc<VaultBackend>,
    hil_gate: Arc<HilGate>,
}

/// A running agent instance.
pub struct AgentInstance {
    /// The agent specification.
    pub spec: AgentSpec,
    /// Budget tracker for this session.
    pub budget: Arc<BudgetTracker>,
    /// Current status.
    pub status: AgentStatus,
    /// Action log for this session.
    pub action_log: Vec<AgentActionLog>,
    /// System context including DOM access.
    pub context: Option<AgentContext>,
}

/// Agent status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum AgentStatus {
    /// Not running.
    Idle,
    /// Currently executing.
    Running,
    /// Waiting for user confirmation (HIL).
    WaitingForUser,
    /// Paused due to budget or error.
    Paused,
    /// Completed successfully.
    Completed,
    /// Failed with error.
    Failed,
}

impl std::fmt::Display for AgentStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Idle => write!(f, "Idle"),
            Self::Running => write!(f, "Running"),
            Self::WaitingForUser => write!(f, "Waiting for you"),
            Self::Paused => write!(f, "Paused"),
            Self::Completed => write!(f, "Completed"),
            Self::Failed => write!(f, "Error"),
        }
    }
}

/// Log entry for an agent action.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AgentActionLog {
    /// Timestamp.
    pub timestamp: chrono::DateTime<chrono::Utc>,
    /// Action description in plain language.
    pub description: String,
    /// The tool used.
    pub tool: String,
    /// Whether the action succeeded.
    pub success: bool,
    /// Cost incurred (if any).
    pub cost: Option<String>,
}

impl AgentRuntime {
    /// Create a new agent runtime.
    pub fn new(
        webview_tx: mpsc::Sender<WebViewCommand>,
        vault: Arc<VaultBackend>,
        hil_gate: Arc<HilGate>,
    ) -> Self {
        info!("Initializing agent runtime");
        Self {
            agents: Vec::new(),
            webview_tx,
            vault,
            hil_gate,
        }
    }

    /// Load an agent from a spec.
    pub fn load_agent(&mut self, spec: AgentSpec) -> AgentResult<usize> {
        let budget = Arc::new(BudgetTracker::new(
            spec.budget.max_cost_per_session.clone(),
            spec.budget.max_cost_per_action.clone(),
            spec.constraints.max_actions_per_session,
        ));

        let dom_accessor = Arc::new(crate::dom_access::DomAccessor::new(
            self.vault.clone(),
            self.hil_gate.clone(),
            Url::parse("about:blank").unwrap(),
            self.webview_tx.clone(),
        ));

        let instance = AgentInstance {
            spec: spec.clone(),
            budget,
            status: AgentStatus::Idle,
            action_log: Vec::new(),
            context: Some(AgentContext { dom: dom_accessor }),
        };

        let index = self.agents.len();
        self.agents.push(instance);

        info!(
            agent_name = %spec.name,
            agent_id = %spec.id,
            "Agent loaded"
        );

        Ok(index)
    }

    /// Get an agent by index.
    pub fn get_agent(&self, index: usize) -> Option<&AgentInstance> {
        self.agents.get(index)
    }

    /// Get a mutable agent by index.
    pub fn get_agent_mut(&mut self, index: usize) -> Option<&mut AgentInstance> {
        self.agents.get_mut(index)
    }

    /// Get all agents.
    pub fn agents(&self) -> &[AgentInstance] {
        &self.agents
    }

    /// Get the number of loaded agents.
    pub fn agent_count(&self) -> usize {
        self.agents.len()
    }

    /// Validate that an agent has a required tool.
    pub fn validate_tool(spec: &AgentSpec, tool: AgentTool) -> AgentResult<()> {
        if spec.allowed_tools.contains(&tool) {
            Ok(())
        } else {
            Err(AgentError::PermissionDenied {
                capability: format!("{:?}", tool),
            })
        }
    }

    /// Validate that a domain is allowed by the agent's policy.
    pub fn validate_domain(spec: &AgentSpec, domain: &str) -> AgentResult<()> {
        match &spec.constraints.allowed_domains {
            DomainPolicy::AllowList(list) => {
                if list.iter().any(|d| domain.contains(d.as_str())) {
                    Ok(())
                } else {
                    Err(AgentError::DomainNotAllowed {
                        domain: domain.to_string(),
                    })
                }
            }
            DomainPolicy::DenyList(list) => {
                if list.iter().any(|d| domain.contains(d.as_str())) {
                    Err(AgentError::DomainNotAllowed {
                        domain: domain.to_string(),
                    })
                } else {
                    Ok(())
                }
            }
            DomainPolicy::AllowAll => Ok(()),
        }
    }
}

/// Returns the 5 core system agent templates.
pub fn get_system_templates() -> Vec<AgentSpec> {
    let now = chrono::Utc::now();
    vec![
        AgentSpec {
            id: AgentId::new(),
            name: "ResearchAgent".to_string(),
            description: "Surfs the web to summarize information on topics.".to_string(),
            goal: AgentGoal {
                description: "Gather and summarize text spanning multiple domains".to_string(),
                structured_objective: None,
                success_criteria: vec!["Text extracted".to_string()],
            },
            actions: vec![],
            allowed_tools: vec![
                AgentTool::Navigate,
                AgentTool::DomRead,
                AgentTool::TextExtract,
            ],
            constraints: AgentConstraints {
                allowed_domains: DomainPolicy::AllowAll,
                ..Default::default()
            },
            triggers: vec![],
            budget: AgentBudget::default(),
            created_by: AgentAuthor::System,
            version: "1.0.0".to_string(),
            created_at: now,
            modified_at: now,
        },
        AgentSpec {
            id: AgentId::new(),
            name: "FormFillAgent".to_string(),
            description: "Safely fills out complex forms using Vault injection.".to_string(),
            goal: AgentGoal {
                description: "Complete and submit forms without leaking real data".to_string(),
                structured_objective: None,
                success_criteria: vec!["Form submitted".to_string()],
            },
            actions: vec![],
            allowed_tools: vec![
                AgentTool::DomRead,
                AgentTool::FormFill,
                AgentTool::FormSubmit,
                AgentTool::Click,
            ],
            constraints: AgentConstraints {
                can_access_vault: vec![VaultAccessLevel::FormFillWithHIL],
                hil_required_for: vec!["submit".to_string()],
                ..Default::default()
            },
            triggers: vec![],
            budget: AgentBudget::default(),
            created_by: AgentAuthor::System,
            version: "1.0.0".to_string(),
            created_at: now,
            modified_at: now,
        },
        AgentSpec {
            id: AgentId::new(),
            name: "PriceTracker".to_string(),
            description: "Monitors listed prices across e-commerce domains.".to_string(),
            goal: AgentGoal {
                description: "Find the lowest price for a given product".to_string(),
                structured_objective: None,
                success_criteria: vec!["Price found".to_string()],
            },
            actions: vec![],
            allowed_tools: vec![
                AgentTool::Navigate,
                AgentTool::DomRead,
                AgentTool::TextExtract,
            ],
            constraints: AgentConstraints {
                allowed_domains: DomainPolicy::AllowList(vec![
                    "amazon.com".into(),
                    "ebay.com".into(),
                ]),
                ..Default::default()
            },
            triggers: vec![],
            budget: AgentBudget::default(),
            created_by: AgentAuthor::System,
            version: "1.0.0".to_string(),
            created_at: now,
            modified_at: now,
        },
        AgentSpec {
            id: AgentId::new(),
            name: "InboxManager".to_string(),
            description: "Reads and prioritizes incoming webmail.".to_string(),
            goal: AgentGoal {
                description: "Organize inbox and flag important messages".to_string(),
                structured_objective: None,
                success_criteria: vec!["Inbox processed".to_string()],
            },
            actions: vec![],
            allowed_tools: vec![
                AgentTool::Navigate,
                AgentTool::DomRead,
                AgentTool::TextExtract,
                AgentTool::Click,
            ],
            constraints: AgentConstraints {
                can_access_vault: vec![VaultAccessLevel::TokenWithHIL],
                ..Default::default()
            },
            triggers: vec![],
            budget: AgentBudget::default(),
            created_by: AgentAuthor::System,
            version: "1.0.0".to_string(),
            created_at: now,
            modified_at: now,
        },
        AgentSpec {
            id: AgentId::new(),
            name: "LoginAuditor".to_string(),
            description: "Checks password health by attempting logins on saved sites.".to_string(),
            goal: AgentGoal {
                description: "Audit all vault credentials for correct login flow".to_string(),
                structured_objective: None,
                success_criteria: vec!["Audits complete".to_string()],
            },
            actions: vec![],
            allowed_tools: vec![
                AgentTool::Navigate,
                AgentTool::FormFill,
                AgentTool::FormSubmit,
                AgentTool::Click,
            ],
            constraints: AgentConstraints {
                can_access_vault: vec![VaultAccessLevel::TrustedAutomation],
                hil_required_for: vec!["start_audit".to_string()],
                ..Default::default()
            },
            triggers: vec![],
            budget: AgentBudget::default(),
            created_by: AgentAuthor::System,
            version: "1.0.0".to_string(),
            created_at: now,
            modified_at: now,
        },
    ]
}
