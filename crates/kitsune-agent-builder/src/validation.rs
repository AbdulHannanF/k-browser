/// Agent spec validation — ensures safety invariants.

use crate::{BuilderValidationError, ErrorSeverity};
use kitsune_agent::spec::AgentSpec;

/// Validate an agent spec and return any errors.
pub fn validate_spec(spec: &AgentSpec) -> Vec<BuilderValidationError> {
    let mut errors = Vec::new();

    // Name must not be empty
    if spec.name.trim().is_empty() {
        errors.push(BuilderValidationError {
            field: "name".to_string(),
            severity: ErrorSeverity::Error,
            message: "Your assistant needs a name.".to_string(),
            suggested_fix: Some("Give your assistant a descriptive name like 'Price Tracker' or 'Form Filler'.".to_string()),
        });
    }

    // Description must not be empty
    if spec.description.trim().is_empty() {
        errors.push(BuilderValidationError {
            field: "description".to_string(),
            severity: ErrorSeverity::Warning,
            message: "A description helps you remember what this assistant does.".to_string(),
            suggested_fix: Some("Describe what this assistant will do for you in plain language.".to_string()),
        });
    }

    // Goal must have a description
    if spec.goal.description.trim().is_empty() {
        errors.push(BuilderValidationError {
            field: "goal".to_string(),
            severity: ErrorSeverity::Error,
            message: "Your assistant needs to know what to do.".to_string(),
            suggested_fix: Some("Tell your assistant what you want it to accomplish.".to_string()),
        });
    }

    // Must have at least one tool
    if spec.allowed_tools.is_empty() {
        errors.push(BuilderValidationError {
            field: "tools".to_string(),
            severity: ErrorSeverity::Error,
            message: "Your assistant needs at least one ability to work.".to_string(),
            suggested_fix: Some("Enable at least 'Navigate' and 'Read page content'.".to_string()),
        });
    }

    // Payment initiation requires explicit HIL
    if spec.constraints.can_initiate_payments
        && !spec.constraints.hil_required_for.iter().any(|h| h == "all" || h == "financial")
    {
        errors.push(BuilderValidationError {
            field: "constraints".to_string(),
            severity: ErrorSeverity::Error,
            message: "Assistants that can make payments must require your confirmation.".to_string(),
            suggested_fix: Some("Enable 'Ask before making payments' in safety settings.".to_string()),
        });
    }

    // Budget should be set if payments are enabled
    if spec.constraints.can_initiate_payments
        && spec.budget.max_cost_per_session.is_none()
    {
        errors.push(BuilderValidationError {
            field: "budget".to_string(),
            severity: ErrorSeverity::Warning,
            message: "This assistant can make payments but has no spending limit set.".to_string(),
            suggested_fix: Some("Set a maximum spending limit per session.".to_string()),
        });
    }

    // Must have at least one trigger
    if spec.triggers.is_empty() {
        errors.push(BuilderValidationError {
            field: "triggers".to_string(),
            severity: ErrorSeverity::Warning,
            message: "Your assistant has no triggers — you'll need to start it manually each time.".to_string(),
            suggested_fix: Some("Add a trigger like a URL pattern or a keyboard shortcut.".to_string()),
        });
    }

    errors
}
