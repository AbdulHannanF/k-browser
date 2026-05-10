use std::sync::Arc;
use std::time::Duration;

use tokio::sync::mpsc;
use uuid::Uuid;

use crate::ai_client::AiProviderConfig;
use crate::error::AgentError;
use crate::executor::WebViewCommand;
use crate::loop_runtime::{AgentEvent, LlmAgentRuntime, StopFlag};
use crate::spec::AgentSpec;
use crate::swarm::types::{SwarmTask, TaskStatus, WorkerRole};
use kitsune_hil::HilGate;
use kitsune_vault::VaultBackend;

pub struct SwarmWorker {
    pub task: SwarmTask,
    pub swarm_id: String,
    pub shared: Arc<tokio::sync::RwLock<Vec<SwarmTask>>>,
    pub event_tx: mpsc::UnboundedSender<AgentEvent>,
    pub browser_tx: mpsc::Sender<WebViewCommand>,
    pub browser_nav_lock: Arc<tokio::sync::Mutex<()>>,
    pub hil_lock: Arc<tokio::sync::Mutex<()>>,
    pub ai_config: AiProviderConfig,
    pub spec: AgentSpec,
    pub vault: Arc<VaultBackend>,
    pub hil_gate: Arc<HilGate>,
    pub stop_flag: StopFlag,
    pub nav_lock_timeout: Duration,
    pub worker_timeout: Duration,
}

impl SwarmWorker {
    pub async fn run(self) -> Result<String, AgentError> {
        let worker_id = format!(
            "{}-{}",
            self.task.role.as_str(),
            &Uuid::new_v4().to_string()[..8]
        );
        let task_id = self.task.id.clone();
        let swarm_id = self.swarm_id.clone();
        let role = self.task.role.clone();

        {
            let mut g = self.shared.write().await;
            if let Some(t) = g.iter_mut().find(|t| t.id == task_id) {
                t.worker_id = Some(worker_id.clone());
            }
        }

        let _ = self.event_tx.send(AgentEvent::SwarmUpdate {
            swarm_id: swarm_id.clone(),
            worker_id: worker_id.clone(),
            role: role.as_str().to_string(),
            status: "Running".into(),
            message: "Starting task...".into(),
            tool_calls_used: 0,
        });

        let persona = self.task.role.persona_prompt();
        let runtime = LlmAgentRuntime::new_with_config(
            self.spec,
            self.ai_config,
            self.browser_tx,
            self.vault,
            self.hil_gate,
        )
        .with_event_sink(self.event_tx.clone())
        .with_stop_flag(self.stop_flag)
        .with_nav_lock(self.browser_nav_lock)
        .with_hil_lock(self.hil_lock)
        .with_agent_context(persona)
        .with_worker_id(worker_id.clone(), swarm_id.clone());

        let prompt = self.task.prompt.clone();
        let shared = self.shared.clone();
        let event_tx = self.event_tx.clone();

        let result = tokio::time::timeout(self.worker_timeout, runtime.run(prompt)).await;

        let wid_clone = worker_id.clone();
        let role_str = role.as_str().to_string();

        match result {
            Err(_elapsed) => {
                let reason = "Worker timed out".to_string();
                update_task_status(&shared, &task_id, TaskStatus::Failed(reason.clone())).await;
                let _ = event_tx.send(AgentEvent::SwarmUpdate {
                    swarm_id,
                    worker_id: wid_clone.clone(),
                    role: role_str,
                    status: "Failed".into(),
                    message: reason.clone(),
                    tool_calls_used: 0,
                });
                Err(AgentError::SwarmWorkerFailed {
                    worker_id: wid_clone,
                    reason,
                })
            }
            Ok(Err(AgentError::Cancelled)) => {
                update_task_status(&shared, &task_id, TaskStatus::Cancelled).await;
                let _ = event_tx.send(AgentEvent::SwarmUpdate {
                    swarm_id,
                    worker_id: wid_clone,
                    role: role_str,
                    status: "Cancelled".into(),
                    message: "Cancelled by user".into(),
                    tool_calls_used: 0,
                });
                Err(AgentError::Cancelled)
            }
            Ok(Err(e)) => {
                let reason = e.to_string();
                update_task_status(&shared, &task_id, TaskStatus::Failed(reason.clone())).await;
                let _ = event_tx.send(AgentEvent::SwarmUpdate {
                    swarm_id,
                    worker_id: wid_clone.clone(),
                    role: role_str,
                    status: "Failed".into(),
                    message: reason.clone(),
                    tool_calls_used: 0,
                });
                Err(AgentError::SwarmWorkerFailed {
                    worker_id: wid_clone,
                    reason,
                })
            }
            Ok(Ok(output)) => {
                let preview: String = output.chars().take(200).collect();
                let preview = if output.chars().count() > 200 {
                    format!("{}...", preview)
                } else {
                    preview.clone()
                };
                let tool_calls = {
                    let g = shared.read().await;
                    g.iter()
                        .find(|t| t.id == task_id)
                        .map(|t| t.tool_calls_used)
                        .unwrap_or(0)
                };
                update_task_status(&shared, &task_id, TaskStatus::Completed(output.clone())).await;
                {
                    let mut g = shared.write().await;
                    if let Some(t) = g.iter_mut().find(|t| t.id == task_id) {
                        t.last_message = Some(preview.clone());
                    }
                }
                let _ = event_tx.send(AgentEvent::SwarmUpdate {
                    swarm_id,
                    worker_id: wid_clone,
                    role: role_str,
                    status: "Completed".into(),
                    message: preview,
                    tool_calls_used: tool_calls,
                });
                Ok(output)
            }
        }
    }
}

async fn update_task_status(
    shared: &Arc<tokio::sync::RwLock<Vec<SwarmTask>>>,
    task_id: &str,
    status: TaskStatus,
) {
    let mut g = shared.write().await;
    if let Some(t) = g.iter_mut().find(|t| t.id == task_id) {
        t.status = status;
    }
}
