/// Agent builder — creates and modifies agent specs.

use crate::AgentBuilderState;
use kitsune_agent::spec::*;
use serde_json;

/// Create a new agent from a natural language description.
pub fn create_from_description(name: &str, description: &str) -> AgentBuilderState {
    let spec = AgentSpec {
        id: AgentId::new(),
        name: name.to_string(),
        description: description.to_string(),
        goal: AgentGoal {
            description: description.to_string(),
            structured_objective: None,
            success_criteria: Vec::new(),
        },
        actions: vec![],
        allowed_tools: vec![
            AgentTool::Navigate,
            AgentTool::DomRead,
            AgentTool::TextExtract,
            AgentTool::Wait,
        ],
        constraints: AgentConstraints::default(),
        triggers: vec![AgentTrigger::Manual],
        budget: AgentBudget::default(),
        created_by: AgentAuthor::System,
        version: "0.1.0".to_string(),
        created_at: chrono::Utc::now(),
        modified_at: chrono::Utc::now(),
    };

    let validation_errors = crate::validation::validate_spec(&spec);

    AgentBuilderState {
        spec,
        validation_errors,
        suggested_improvements: Vec::new(),
        test_results: None,
    }
}

/// Serialize an agent spec to JSON.
pub fn serialize_spec(spec: &AgentSpec) -> Result<String, serde_json::Error> {
    serde_json::to_string_pretty(spec)
}

/// Deserialize an agent spec from JSON.
pub fn deserialize_spec(json: &str) -> Result<AgentSpec, serde_json::Error> {
    serde_json::from_str(json)
}
