//! AI request and response types for `kitsune-ai`.
//!
//! These are the public-facing types that `kitsune-agent` passes into
//! `AiRouter::route()`. They are intentionally simple — no raw secrets,
//! no vault values, no PII (PII is scrubbed before construction).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// ─── TaskType ─────────────────────────────────────────────────────────────────

/// The type of task the agent is performing.
///
/// This determines **routing** (cloud vs local) and is enforced structurally —
/// `VaultDecision` and `SensitiveForm` can never reach the cloud backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskType {
    /// Filling web forms with vault-sourced data.
    FormFill,
    /// Summarizing page content.
    PageSummary,
    /// Multi-page research and information gathering.
    WebResearch,
    /// Reading and drafting emails.
    InboxManagement,
    /// Monitoring product pages for price changes.
    PriceTracking,
    /// Repeating a task the agent has done before (cached patterns).
    RoutineRepeat,
    /// Deciding whether vault access is permitted.
    ///
    /// **INVARIANT**: This task type ALWAYS uses local model.
    /// It NEVER goes to cloud. Enforced in `AiRouter`.
    VaultDecision,
    /// Handling forms that contain passwords or payment data.
    ///
    /// **INVARIANT**: This task type ALWAYS uses local model.
    /// It NEVER goes to cloud. Enforced in `AiRouter`.
    SensitiveForm,
    /// Multi-step planning requiring stronger reasoning.
    /// Local model may not be capable — routes to cloud.
    ComplexReasoning,
}

impl TaskType {
    /// Whether this task type must NEVER be sent to a cloud backend.
    /// Enforced at the router level — not a configuration option.
    pub fn requires_local_only(&self) -> bool {
        matches!(self, TaskType::VaultDecision | TaskType::SensitiveForm)
    }

    /// A human-readable description for logging (never contains any user data).
    pub fn description(&self) -> &'static str {
        match self {
            TaskType::FormFill => "form fill",
            TaskType::PageSummary => "page summary",
            TaskType::WebResearch => "web research",
            TaskType::InboxManagement => "inbox management",
            TaskType::PriceTracking => "price tracking",
            TaskType::RoutineRepeat => "routine repeat",
            TaskType::VaultDecision => "vault decision [LOCAL ONLY]",
            TaskType::SensitiveForm => "sensitive form [LOCAL ONLY]",
            TaskType::ComplexReasoning => "complex reasoning",
        }
    }
}

// ─── AiRequest ────────────────────────────────────────────────────────────────

/// A request to the AI backend.
///
/// **IMPORTANT**: By the time this struct is constructed, PII must already
/// be scrubbed from `context`. The cloud backend runs an additional scrub as
/// a defence-in-depth measure, but do not rely on it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiRequest {
    /// What kind of task this is (controls routing).
    pub task_type: TaskType,
    /// PII-scrubbed context for the model. Never contains raw credentials.
    pub context: String,
    /// Maximum tokens to generate in the response.
    pub max_tokens: u32,
    /// Require a structured JSON response that `kitsune-agent` can parse.
    pub structured_output: bool,
    /// Which agent is making the request (for audit logging, never sent to cloud).
    pub agent_id: String,
    /// When this request was created.
    pub created_at: DateTime<Utc>,
}

impl AiRequest {
    /// Create a new AI request.
    pub fn new(
        task_type: TaskType,
        context: impl Into<String>,
        agent_id: impl Into<String>,
    ) -> Self {
        Self {
            task_type,
            context: context.into(),
            max_tokens: 4000,
            structured_output: true,
            agent_id: agent_id.into(),
            created_at: Utc::now(),
        }
    }

    /// Create a request with a custom token limit.
    pub fn with_max_tokens(mut self, max_tokens: u32) -> Self {
        self.max_tokens = max_tokens;
        self
    }
}

// ─── AiResponse ───────────────────────────────────────────────────────────────

/// A response from the AI backend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiResponse {
    /// Parsed response content. Always valid JSON when `structured_output` was true.
    pub content: String,
    /// How many tokens were consumed (cloud tracks this for quota).
    pub tokens_used: u32,
    /// Which backend actually served this response.
    pub backend_used: crate::BackendType,
    /// Actions remaining this month (populated by cloud backend, `None` for local).
    pub actions_remaining: Option<u32>,
    /// Wall-clock time the inference took, in milliseconds.
    pub latency_ms: u64,
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vault_decision_requires_local() {
        assert!(TaskType::VaultDecision.requires_local_only());
        assert!(TaskType::SensitiveForm.requires_local_only());
    }

    #[test]
    fn test_other_tasks_do_not_require_local() {
        assert!(!TaskType::FormFill.requires_local_only());
        assert!(!TaskType::WebResearch.requires_local_only());
        assert!(!TaskType::ComplexReasoning.requires_local_only());
    }

    #[test]
    fn test_ai_request_construction() {
        let req = AiRequest::new(TaskType::PageSummary, "Some context text", "agent-001");
        assert_eq!(req.task_type, TaskType::PageSummary);
        assert_eq!(req.max_tokens, 4000);
        assert!(req.structured_output);
    }
}
