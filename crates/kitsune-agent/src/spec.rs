/// Agent specification — the complete, serializable definition of an agent.
///
/// An AgentSpec is the single source of truth for what an agent can do.
/// All capabilities are explicit — nothing happens implicitly.
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Unique identifier for an agent.
#[derive(Debug, Clone, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub struct AgentId(pub Uuid);

impl AgentId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for AgentId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for AgentId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// The complete specification of an agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSpec {
    /// Unique identifier for this agent.
    pub id: AgentId,
    /// Human-readable name.
    pub name: String,
    /// Description of what this agent does.
    pub description: String,
    /// The agent's goal.
    pub goal: AgentGoal,
    /// The sequence of actions to perform.
    pub actions: Vec<AgentAction>,
    /// Tools the agent is allowed to use.
    pub allowed_tools: Vec<AgentTool>,
    /// Constraints on what the agent can do.
    pub constraints: AgentConstraints,
    /// When the agent activates.
    pub triggers: Vec<AgentTrigger>,
    /// Budget limits.
    pub budget: AgentBudget,
    /// Who created this agent.
    pub created_by: AgentAuthor,
    /// Version of this agent spec.
    pub version: String,
    /// When this spec was created.
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// When this spec was last modified.
    pub modified_at: chrono::DateTime<chrono::Utc>,
}

/// The agent's goal — what it's trying to accomplish.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentGoal {
    /// Natural language description of the goal.
    pub description: String,
    /// Structured objective for machine parsing.
    pub structured_objective: Option<serde_json::Value>,
    /// Success criteria — how to know the goal is achieved.
    pub success_criteria: Vec<String>,
}

/// An action an agent can take.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum AgentAction {
    Navigate {
        url: String,
    },
    QueryText {
        selector: String,
        purpose: String,
    },
    QueryLinks {
        selector: String,
        purpose: String,
    },
    FillField {
        selector: String,
        value: String,
        purpose: String,
    },
    Click {
        selector: String,
        purpose: String,
    },
    Wait {
        ms: u64,
    },
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AgentTool {
    /// Navigate to URLs.
    Navigate,
    /// Read DOM elements.
    DomRead,
    /// Fill form fields (via vault).
    FormFill,
    /// Click elements.
    Click,
    /// Submit forms.
    FormSubmit,
    /// Fetch network resources.
    NetworkFetch,
    /// Request vault credentials (produces tokens, never raw values).
    VaultAccess,
    /// Take page screenshots.
    Screenshot,
    /// Wait for elements/conditions.
    Wait,
    /// Extract text from pages.
    TextExtract,
    /// Request human input.
    HumanInput,
}

/// Constraints on agent behavior.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConstraints {
    /// Whether the agent can initiate payments. Default: false.
    pub can_initiate_payments: bool,
    /// Whether the agent can create accounts. Default: false.
    pub can_create_accounts: bool,
    /// Whether the agent can send communications. Default: false.
    pub can_send_communications: bool,
    /// Vault access levels allowed.
    pub can_access_vault: Vec<VaultAccessLevel>,
    /// Domain access policy.
    pub allowed_domains: DomainPolicy,
    /// Maximum actions per session.
    pub max_actions_per_session: u32,
    /// HIL trigger classes that require confirmation.
    pub hil_required_for: Vec<String>,
    /// Maximum cost per action.
    pub cost_ceiling_per_action: Option<MoneyAmount>,
    /// Maximum cost per session.
    pub cost_ceiling_per_session: Option<MoneyAmount>,
}

impl Default for AgentConstraints {
    fn default() -> Self {
        Self {
            can_initiate_payments: false,
            can_create_accounts: false,
            can_send_communications: false,
            can_access_vault: Vec::new(),
            allowed_domains: DomainPolicy::AllowList(Vec::new()),
            max_actions_per_session: 100,
            hil_required_for: vec!["all".to_string()],
            cost_ceiling_per_action: None,
            cost_ceiling_per_session: None,
        }
    }
}

/// Vault access level for agents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VaultAccessLevel {
    /// Can check if entries exist (metadata only).
    MetadataOnly,
    /// Can request opaque tokens via HIL.
    TokenWithHIL,
    /// Can request DOM injection (form fill) via HIL.
    FormFillWithHIL,
    /// Can use trusted automation policies.
    TrustedAutomation,
}

/// Domain access policy for agents.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DomainPolicy {
    /// Only these domains are allowed.
    AllowList(Vec<String>),
    /// All domains except these are allowed.
    DenyList(Vec<String>),
    /// All domains are allowed (use with caution).
    AllowAll,
}

/// When an agent activates.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AgentTrigger {
    /// Activate when visiting a URL matching this pattern.
    UrlPattern(String),
    /// Activate on a user gesture (e.g., keyboard shortcut).
    UserGesture(String),
    /// Activate on a schedule (cron expression).
    Schedule(String),
    /// Activate on a custom event.
    Event(String),
    /// Manual activation only.
    Manual,
}

/// Budget for an agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentBudget {
    /// Maximum cost per session.
    pub max_cost_per_session: Option<MoneyAmount>,
    /// Maximum cost per action.
    pub max_cost_per_action: Option<MoneyAmount>,
    /// Maximum number of API calls per session.
    pub max_api_calls: u32,
    /// Maximum execution time in seconds.
    pub max_execution_time_seconds: u64,
}

impl Default for AgentBudget {
    fn default() -> Self {
        Self {
            max_cost_per_session: None,
            max_cost_per_action: None,
            max_api_calls: 50,
            max_execution_time_seconds: 300,
        }
    }
}

/// A monetary amount.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MoneyAmount {
    pub amount_minor: i64,
    pub currency: String,
}

impl MoneyAmount {
    pub fn usd(dollars: f64) -> Self {
        Self {
            amount_minor: (dollars * 100.0) as i64,
            currency: "USD".to_string(),
        }
    }

    pub fn display(&self) -> String {
        let major = self.amount_minor / 100;
        let minor = (self.amount_minor % 100).abs();
        format!("{}.{:02} {}", major, minor, self.currency)
    }
}

/// Who created an agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AgentAuthor {
    /// Created by a human user.
    Human { user_id: String },
    /// Created by another agent (lineage tracking).
    Agent { parent_agent_id: AgentId },
    /// System-provided template.
    System,
}
