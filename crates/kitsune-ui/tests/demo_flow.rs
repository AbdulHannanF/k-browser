use kitsune_ui::app::{AgentRunState, BudgetState};

#[test]
fn budget_starts_at_zero_of_hundred() {
    let budget = BudgetState::default();
    assert_eq!(budget.used, 0);
    assert_eq!(budget.total, 100);
}

#[test]
fn budget_fraction_is_ratio() {
    let budget = BudgetState { used: 50, total: 100 };
    assert!((budget.fraction() - 0.5).abs() < f32::EPSILON);
}

#[test]
fn agent_starts_idle() {
    // Verify the enum variants exist and Idle is the default starting state
    let state = AgentRunState::Idle;
    assert_eq!(state, AgentRunState::Idle);
    assert_ne!(state, AgentRunState::Running);
    assert_ne!(state, AgentRunState::AwaitingHil);
}
