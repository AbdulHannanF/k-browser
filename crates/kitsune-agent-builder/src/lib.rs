// ARCHITECTURE: kitsune-agent-builder provides no-code agent configuration.
// It must be usable by someone who has never written code — like building
// a workflow in Notion or Zapier, not like writing a Makefile.

pub mod builder;
pub mod templates;
pub mod validation;

pub use builder::*;
pub use validation::*;

use kitsune_agent::spec::AgentSpec;
use serde::{Deserialize, Serialize};

/// The state of the agent builder UI.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentBuilderState {
    /// The agent spec being built.
    pub spec: AgentSpec,
    /// Validation errors in the current spec.
    pub validation_errors: Vec<BuilderValidationError>,
    /// AI-powered suggestions for improving the agent.
    pub suggested_improvements: Vec<AgentSuggestion>,
    /// Results from the latest test run.
    pub test_results: Option<AgentTestRun>,
}

/// A validation error in the agent builder.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuilderValidationError {
    /// Which field has the error.
    pub field: String,
    /// Severity of the error.
    pub severity: ErrorSeverity,
    /// Error message in plain English (no jargon).
    pub message: String,
    /// Suggested fix in plain English.
    pub suggested_fix: Option<String>,
}

/// Error severity levels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ErrorSeverity {
    /// Must be fixed before the agent can run.
    Error,
    /// Should be fixed but won't prevent running.
    Warning,
    /// Informational — a suggestion for improvement.
    Info,
}

/// An AI-powered suggestion for the agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSuggestion {
    /// What the suggestion is about.
    pub title: String,
    /// Detailed explanation.
    pub description: String,
    /// Whether applying this suggestion changes the agent's constraints.
    pub affects_constraints: bool,
}

/// Results from an agent test run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentTestRun {
    /// Whether the test passed.
    pub passed: bool,
    /// Steps the agent took during the test.
    pub steps: Vec<AgentTestStep>,
    /// Total simulated cost.
    pub total_cost: Option<String>,
    /// Duration of the test run (in milliseconds).
    pub duration_ms: u64,
}

/// A single step in an agent test run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentTestStep {
    /// Step number.
    pub step: u32,
    /// Plain-language description of what the agent did.
    pub action: String,
    /// The result of the action.
    pub result: String,
    /// Whether this step would trigger HIL in production.
    pub would_trigger_hil: bool,
}
