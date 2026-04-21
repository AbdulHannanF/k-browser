use kitsune_agent::spec::{AgentSpec, AgentAction, AgentGoal, AgentAuthor, AgentBudget, AgentConstraints, AgentId};
use kitsune_agent::dom_access::DomAccessor;
use kitsune_agent::executor::ScriptedExecutor;
use kitsune_html::dom::DomTree;
use kitsune_vault::VaultBackend;
use kitsune_hil::HilGate;
use url::Url;
use std::sync::Arc;
use tokio::sync::Mutex;

fn build_mock_dom() -> DomTree {
    let mut tree = DomTree::new();
    let root = tree.create_document();
    let html = tree.create_element("html");
    let body = tree.create_element("body");
    tree.append_child(root, html);
    tree.append_child(html, body);
    tree
}

fn create_test_spec() -> AgentSpec {
    let now = chrono::Utc::now();
    AgentSpec {
        id: AgentId::new(),
        name: "TestScriptAgent".to_string(),
        description: "A test agent that does nothing".to_string(),
        goal: AgentGoal {
            description: "Test the executor".to_string(),
            structured_objective: None,
            success_criteria: vec![],
        },
        actions: vec![
            AgentAction::Navigate { url: "https://example.com".to_string() },
            AgentAction::Wait { ms: 100 },
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
async fn test_scripted_executor_runs_to_completion() {
    let dom = Arc::new(Mutex::new(build_mock_dom()));
    let vault = Arc::new(VaultBackend::new("password", &[0; 32]).unwrap());
    let hil_gate = Arc::new(HilGate::new_test_gate());
    let dom_accessor = Arc::new(Mutex::new(DomAccessor::new(dom, vault, hil_gate, Url::parse("https://initial.com").unwrap(), None, None)));

    let spec = create_test_spec();
    let executor = ScriptedExecutor::new(spec, dom_accessor.clone());

    let result = executor.run().await;
    assert!(result.is_ok());

    let final_url = dom_accessor.lock().await.get_current_url().await.unwrap();
    assert_eq!(final_url, "https://example.com/");
}
