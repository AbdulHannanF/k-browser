/// Pre-built agent templates for common tasks.
///
/// Each template ships with maximum safety defaults.
use kitsune_agent::spec::*;

/// Get all available agent templates.
pub fn all_templates() -> Vec<AgentSpec> {
    vec![
        form_autofill_template(),
        subscription_watchdog_template(),
        price_tracker_template(),
        email_unsubscribe_template(),
        login_auditor_template(),
    ]
}

/// Form autofill — fills web forms using vault data (never raw values).
pub fn form_autofill_template() -> AgentSpec {
    AgentSpec {
        id: AgentId::new(),
        name: "Form Auto-Fill".to_string(),
        description: "Automatically fills web forms using your saved information. Your data is never shared — it's injected directly into the form.".to_string(),
        goal: AgentGoal {
            description: "Fill form fields using vault data".to_string(),
            structured_objective: None,
            success_criteria: vec!["All requested fields are filled".to_string()],
        },
        actions: vec![],
        allowed_tools: vec![
            AgentTool::DomRead,
            AgentTool::FormFill,
            AgentTool::VaultAccess,
        ],
        constraints: AgentConstraints {
            can_initiate_payments: false,
            can_create_accounts: false,
            can_send_communications: false,
            can_access_vault: vec![VaultAccessLevel::FormFillWithHIL],
            allowed_domains: DomainPolicy::AllowAll,
            max_actions_per_session: 20,
            hil_required_for: vec!["all".to_string()],
            cost_ceiling_per_action: None,
            cost_ceiling_per_session: None,
        },
        triggers: vec![AgentTrigger::UserGesture("Ctrl+Shift+F".to_string())],
        budget: AgentBudget::default(),
        created_by: AgentAuthor::System,
        version: "1.0.0".to_string(),
        created_at: chrono::Utc::now(),
        modified_at: chrono::Utc::now(),
    }
}

/// Subscription watchdog — monitors services for plan changes and alerts.
pub fn subscription_watchdog_template() -> AgentSpec {
    AgentSpec {
        id: AgentId::new(),
        name: "Subscription Watchdog".to_string(),
        description: "Monitors your subscriptions and alerts you when prices change, plans are modified, or renewals are upcoming.".to_string(),
        goal: AgentGoal {
            description: "Monitor subscriptions for changes".to_string(),
            structured_objective: None,
            success_criteria: vec!["Check all monitored subscriptions".to_string(), "Alert on any changes".to_string()],
        },
        actions: vec![],
        allowed_tools: vec![
            AgentTool::Navigate,
            AgentTool::DomRead,
            AgentTool::TextExtract,
        ],
        constraints: AgentConstraints {
            can_initiate_payments: false,
            can_create_accounts: false,
            can_send_communications: false,
            can_access_vault: vec![VaultAccessLevel::MetadataOnly],
            allowed_domains: DomainPolicy::AllowAll,
            max_actions_per_session: 50,
            hil_required_for: vec!["all".to_string()],
            cost_ceiling_per_action: None,
            cost_ceiling_per_session: None,
        },
        triggers: vec![AgentTrigger::Schedule("0 9 * * MON".to_string())], // Weekly Monday 9am
        budget: AgentBudget::default(),
        created_by: AgentAuthor::System,
        version: "1.0.0".to_string(),
        created_at: chrono::Utc::now(),
        modified_at: chrono::Utc::now(),
    }
}

/// Price tracker — watches product pages for price drops.
pub fn price_tracker_template() -> AgentSpec {
    AgentSpec {
        id: AgentId::new(),
        name: "Price Tracker".to_string(),
        description: "Watches product pages you care about and notifies you when prices drop."
            .to_string(),
        goal: AgentGoal {
            description: "Monitor product prices and notify on drops".to_string(),
            structured_objective: None,
            success_criteria: vec![
                "Check all tracked products".to_string(),
                "Notify on price drops".to_string(),
            ],
        },
        actions: vec![],
        allowed_tools: vec![
            AgentTool::Navigate,
            AgentTool::DomRead,
            AgentTool::TextExtract,
            AgentTool::Wait,
        ],
        constraints: AgentConstraints {
            can_initiate_payments: false,
            can_create_accounts: false,
            can_send_communications: false,
            can_access_vault: vec![],
            allowed_domains: DomainPolicy::AllowAll,
            max_actions_per_session: 100,
            hil_required_for: vec!["all".to_string()],
            cost_ceiling_per_action: None,
            cost_ceiling_per_session: None,
        },
        triggers: vec![AgentTrigger::Schedule("0 */6 * * *".to_string())], // Every 6 hours
        budget: AgentBudget::default(),
        created_by: AgentAuthor::System,
        version: "1.0.0".to_string(),
        created_at: chrono::Utc::now(),
        modified_at: chrono::Utc::now(),
    }
}

/// Email unsubscribe — finds and clicks unsubscribe links.
pub fn email_unsubscribe_template() -> AgentSpec {
    AgentSpec {
        id: AgentId::new(),
        name: "Email Unsubscriber".to_string(),
        description: "Finds unsubscribe links in emails and clicks them for you. Always asks for your confirmation first.".to_string(),
        goal: AgentGoal {
            description: "Find and activate unsubscribe links".to_string(),
            structured_objective: None,
            success_criteria: vec!["Identify unsubscribe links".to_string(), "Successfully unsubscribe with user confirmation".to_string()],
        },
        actions: vec![],
        allowed_tools: vec![
            AgentTool::Navigate,
            AgentTool::DomRead,
            AgentTool::Click,
            AgentTool::TextExtract,
            AgentTool::HumanInput,
        ],
        constraints: AgentConstraints {
            can_initiate_payments: false,
            can_create_accounts: false,
            can_send_communications: false,
            can_access_vault: vec![],
            allowed_domains: DomainPolicy::AllowAll,
            max_actions_per_session: 50,
            hil_required_for: vec!["all".to_string()],
            cost_ceiling_per_action: None,
            cost_ceiling_per_session: None,
        },
        triggers: vec![AgentTrigger::Manual],
        budget: AgentBudget::default(),
        created_by: AgentAuthor::System,
        version: "1.0.0".to_string(),
        created_at: chrono::Utc::now(),
        modified_at: chrono::Utc::now(),
    }
}

/// Login auditor — lists all logged-in sites and flags stale accounts.
pub fn login_auditor_template() -> AgentSpec {
    AgentSpec {
        id: AgentId::new(),
        name: "Login Auditor".to_string(),
        description: "Reviews your saved logins and helps you identify accounts you no longer use or that may need password updates.".to_string(),
        goal: AgentGoal {
            description: "Audit saved logins for stale or risky accounts".to_string(),
            structured_objective: None,
            success_criteria: vec!["Review all saved logins".to_string(), "Flag stale accounts".to_string()],
        },
        actions: vec![],
        allowed_tools: vec![
            AgentTool::VaultAccess,
            AgentTool::Navigate,
            AgentTool::DomRead,
            AgentTool::TextExtract,
        ],
        constraints: AgentConstraints {
            can_initiate_payments: false,
            can_create_accounts: false,
            can_send_communications: false,
            can_access_vault: vec![VaultAccessLevel::MetadataOnly],
            allowed_domains: DomainPolicy::AllowAll,
            max_actions_per_session: 200,
            hil_required_for: vec!["all".to_string()],
            cost_ceiling_per_action: None,
            cost_ceiling_per_session: None,
        },
        triggers: vec![AgentTrigger::Schedule("0 10 1 * *".to_string())], // Monthly
        budget: AgentBudget::default(),
        created_by: AgentAuthor::System,
        version: "1.0.0".to_string(),
        created_at: chrono::Utc::now(),
        modified_at: chrono::Utc::now(),
    }
}
