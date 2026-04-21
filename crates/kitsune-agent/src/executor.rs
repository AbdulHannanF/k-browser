
use crate::spec::{AgentSpec, AgentAction};
use crate::dom_access::DomAccessor;
use crate::error::AgentResult;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{info, error};

/// Executes a fixed, scripted sequence of actions from an agent spec.
pub struct ScriptedExecutor {
    spec: AgentSpec,
    dom_accessor: Arc<Mutex<DomAccessor>>,
}

impl ScriptedExecutor {
    /// Create a new scripted executor.
    pub fn new(spec: AgentSpec, dom_accessor: Arc<Mutex<DomAccessor>>) -> Self {
        Self { spec, dom_accessor }
    }

    /// Run the scripted demo agent.
    pub async fn run(&self) -> AgentResult<()> {
        info!("Starting scripted demo agent: {}", self.spec.name);

        for action in &self.spec.actions {
            if let Err(e) = self.execute_action(action).await {
                error!("Action failed: {:?}", e);
                // For a demo, we'll continue, but a real agent might stop.
            }
        }

        info!("Scripted demo agent finished.");
        Ok(())
    }

    /// Execute a single agent action.
    async fn execute_action(&self, action: &AgentAction) -> AgentResult<()> {
        let accessor = self.dom_accessor.lock().await;
        match action {
            AgentAction::Navigate { url } => {
                accessor.navigate(url).await?;
            }
            AgentAction::QueryText { selector, .. } => {
                let text = accessor.query_text(selector).await?;
                info!("Queried text: {:?}", text);
            }
            AgentAction::QueryLinks { selector, .. } => {
                let links = accessor.query_links(selector).await?;
                info!("Queried links: {:?}", links);
            }
            AgentAction::FillField { selector, vault_key, .. } => {
                accessor.fill_field(selector, vault_key).await?;
            }
            AgentAction::Click { selector, .. } => {
                accessor.click_element(selector).await?;
            }
            // Wait action for demo purposes
            AgentAction::Wait { ms } => {
                tokio::time::sleep(std::time::Duration::from_millis(*ms)).await;
            }
        }
        Ok(())
    }
}
