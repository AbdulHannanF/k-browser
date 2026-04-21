/// Agent tool API — the structured, audited interface through which agents interact.
///
/// SECURITY: All vault operations produce tokens, never raw values.
/// All actions that could incur cost must call log_cost and check_budget.

use crate::error::{AgentError, AgentResult};
use crate::spec::MoneyAmount;
use serde::{Deserialize, Serialize};
use url::Url;

/// A snapshot of a page for agent inspection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageSnapshot {
    /// The page URL.
    pub url: Url,
    /// The page title.
    pub title: Option<String>,
    /// Whether the page has loaded.
    pub loaded: bool,
}

/// Summary of a DOM element (safe to share with agents).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DomElementSummary {
    /// CSS selector path.
    pub selector_path: String,
    /// Tag name.
    pub tag_name: String,
    /// Element ID.
    pub id: Option<String>,
    /// CSS classes.
    pub classes: Vec<String>,
    /// Text content (truncated).
    pub text_content: Option<String>,
    /// Attributes (sensitive values redacted).
    pub attributes: Vec<(String, String)>,
    /// Whether visible.
    pub visible: bool,
}

/// A form value — either plain text or a vault token reference.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FormValue {
    /// Plain text value (non-sensitive).
    PlainText(String),
    /// A vault token reference (vault handles substitution).
    VaultToken(String),
}

/// Result of form submission.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubmissionResult {
    /// Whether the submission succeeded.
    pub success: bool,
    /// The resulting URL after submission.
    pub resulting_url: Option<Url>,
    /// Any error message.
    pub error: Option<String>,
}

/// A question for the human user (agent-initiated pause).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HilQuestion {
    /// The question in plain language.
    pub question: String,
    /// Suggested options if applicable.
    pub options: Vec<String>,
    /// Context about why the agent is asking.
    pub context: String,
}

/// A human response to an agent question.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HumanResponse {
    /// The response text.
    pub response: String,
    /// Which option was selected (if options were offered).
    pub selected_option: Option<usize>,
}

/// Budget status for an agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BudgetStatus {
    /// Total spent this session.
    pub total_spent: MoneyAmount,
    /// Remaining budget (None if unlimited).
    pub remaining: Option<MoneyAmount>,
    /// Number of actions taken.
    pub actions_taken: u32,
    /// Maximum actions allowed.
    pub max_actions: u32,
    /// Whether the budget is exceeded.
    pub exceeded: bool,
}

/// Privacy-aware fetch request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrivacyAwareFetchRequest {
    /// URL to fetch.
    pub url: Url,
    /// HTTP method.
    pub method: String,
    /// Headers.
    pub headers: Vec<(String, String)>,
    /// Body.
    pub body: Option<Vec<u8>>,
}

/// Fetch response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FetchResponse {
    /// HTTP status code.
    pub status: u16,
    /// Response headers.
    pub headers: Vec<(String, String)>,
    /// Response body.
    pub body: Vec<u8>,
}
