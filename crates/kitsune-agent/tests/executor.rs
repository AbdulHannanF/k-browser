//! Smoke test for `ScriptedExecutor`.
//!
//! Runs a minimal navigate-then-wait spec through the executor against a
//! mock WebViewCommand receiver, verifying the navigation command reaches
//! the bridge and the executor completes cleanly.

use kitsune_agent::dom_access::DomAccessor;
use kitsune_agent::executor::{ScriptedExecutor, WebViewCommand};
use kitsune_agent::spec::{
    AgentAction, AgentAuthor, AgentBudget, AgentConstraints, AgentGoal, AgentId, AgentSpec,
};
use kitsune_hil::HilGate;
use kitsune_vault::VaultBackend;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};
use url::Url;

fn build_test_spec() -> AgentSpec {
    let now = chrono::Utc::now();
    AgentSpec {
        id: AgentId::new(),
        name: "TestScriptAgent".to_string(),
        description: "Smoke-test agent that navigates and waits".to_string(),
        goal: AgentGoal {
            description: "Test the executor".to_string(),
            structured_objective: None,
            success_criteria: vec![],
        },
        actions: vec![
            AgentAction::Navigate {
                url: "https://example.com".to_string(),
            },
            AgentAction::Wait { ms: 10 },
        ],
        allowed_tools: vec![],
        constraints: AgentConstraints::default(),
        triggers: vec![],
        budget: AgentBudget::default(),
        created_by: AgentAuthor::System,
        version: "1.0.0".to_string(),
        created_at: now,
        modified_at: now,
    }
}

#[tokio::test]
async fn scripted_executor_navigates_and_completes() {
    let vault = Arc::new(VaultBackend::new("password", &[0; 32]).unwrap());
    let hil_gate = Arc::new(HilGate::new_test_gate());
    let (tx, mut rx) = mpsc::channel::<WebViewCommand>(8);

    let dom_accessor = Arc::new(Mutex::new(DomAccessor::new(
        vault,
        hil_gate,
        Url::parse("https://initial.com").unwrap(),
        tx,
    )));

    let executor = ScriptedExecutor::new(build_test_spec(), dom_accessor.clone());
    executor.run().await.expect("executor should complete");

    let final_url = dom_accessor.lock().await.get_current_url().await.unwrap();
    assert_eq!(final_url, "https://example.com/");

    // Drain the bridge — we expect at least one Navigate command.
    let mut saw_navigate = false;
    while let Ok(cmd) = rx.try_recv() {
        if let WebViewCommand::Navigate(url) = cmd {
            assert_eq!(url, "https://example.com/");
            saw_navigate = true;
        }
    }
    assert!(saw_navigate, "executor should emit a Navigate command");
}
