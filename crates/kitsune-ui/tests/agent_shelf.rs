use kitsune_ui::app::AgentRunState;

#[test]
fn agent_run_states_are_distinct() {
    assert_ne!(AgentRunState::Idle, AgentRunState::Running);
    assert_ne!(AgentRunState::Running, AgentRunState::AwaitingHil);
    assert_ne!(AgentRunState::Idle, AgentRunState::AwaitingHil);
}
