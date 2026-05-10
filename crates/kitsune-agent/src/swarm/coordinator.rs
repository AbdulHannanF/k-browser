use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;

use tokio::sync::{mpsc, Semaphore};
use tracing::warn;
use uuid::Uuid;

use crate::ai_client::{AgentAiClient, AiProviderConfig, ModelTier};
use crate::error::AgentError;
use crate::executor::WebViewCommand;
use crate::loop_runtime::{AgentEvent, StopFlag};
use crate::spec::AgentSpec;
use crate::swarm::reconciler::reconcile;
use crate::swarm::types::{SwarmConfig, SwarmMode, SwarmTask, TaskStatus, WorkerRole};
use crate::swarm::worker::SwarmWorker;
use kitsune_hil::HilGate;
use kitsune_vault::VaultBackend;

pub struct SwarmCoordinator {
    pub goal: String,
    pub config: SwarmConfig,
    pub spec: AgentSpec,
    pub ai_client: Arc<AgentAiClient>,
    pub ai_config: AiProviderConfig,
    pub event_tx: mpsc::UnboundedSender<AgentEvent>,
    pub browser_tx: mpsc::Sender<WebViewCommand>,
    pub vault: Arc<VaultBackend>,
    pub hil_gate: Arc<HilGate>,
    pub stop_flag: StopFlag,
}

impl SwarmCoordinator {
    pub async fn run(self) -> Result<String, AgentError> {
        let swarm_id = Uuid::new_v4().to_string();

        let _ = self.event_tx.send(AgentEvent::SwarmUpdate {
            swarm_id: swarm_id.clone(),
            worker_id: "coordinator".into(),
            role: "Coordinator".into(),
            status: "Running".into(),
            message: "Decomposing goal into parallel tasks...".into(),
            tool_calls_used: 0,
        });

        let prompt = build_coordinator_prompt(
            &self.goal,
            &self.config.mode,
            self.config.max_workers,
            self.config.enable_disagreement,
        );

        let tasks = match self.ai_client.complete(&prompt, ModelTier::Orchestrator).await {
            Ok(raw) => match parse_plan(&raw) {
                Ok(t) => t,
                Err(_) => {
                    let corrective = format!(
                        "Your previous response could not be parsed as a JSON array. \
                         Return ONLY a valid JSON array — no prose, no code fences. \
                         Goal: {}\n\nFormat: \
                         [{{\"id\":\"t1\",\"role\":\"Researcher\",\"prompt\":\"...\",\"depends_on\":[]}}]",
                        self.goal
                    );
                    match self.ai_client.complete(&corrective, ModelTier::Orchestrator).await {
                        Ok(raw2) => parse_plan(&raw2).map_err(|_| {
                            AgentError::SwarmCoordinatorFailed(
                                "Could not parse task plan after retry".into(),
                            )
                        })?,
                        Err(e) => {
                            return Err(AgentError::SwarmCoordinatorFailed(format!(
                                "Retry LLM call failed: {e}"
                            )))
                        }
                    }
                }
            },
            Err(e) => {
                return Err(AgentError::SwarmCoordinatorFailed(format!(
                    "Planning LLM call failed: {e}"
                )))
            }
        };

        if tasks.is_empty() {
            let _ = self.event_tx.send(AgentEvent::SwarmError {
                swarm_id: swarm_id.clone(),
                error: "Coordinator returned empty plan. Rephrase your goal.".into(),
            });
            return Err(AgentError::SwarmCoordinatorFailed(
                "Coordinator returned empty plan".into(),
            ));
        }

        let mut tasks: Vec<SwarmTask> = tasks;
        if tasks.len() > self.config.max_workers {
            warn!(
                "Coordinator returned {} tasks, truncating to {}",
                tasks.len(),
                self.config.max_workers
            );
            tasks.truncate(self.config.max_workers);
        }

        let _ = self.event_tx.send(AgentEvent::SwarmPlanReady {
            swarm_id: swarm_id.clone(),
            goal: self.goal.clone(),
            tasks: tasks.clone(),
        });

        let shared = Arc::new(tokio::sync::RwLock::new(tasks));

        let max_concurrent = self.config.max_workers.max(1);
        let semaphore = Arc::new(Semaphore::new(max_concurrent));
        let browser_nav_lock = Arc::new(tokio::sync::Mutex::new(()));
        let hil_lock = Arc::new(tokio::sync::Mutex::new(()));

        let nav_timeout = Duration::from_secs(self.config.nav_lock_timeout_seconds);
        let worker_timeout = Duration::from_secs(self.config.worker_timeout_seconds);

        let mut handles: HashMap<
            String,
            tokio::task::JoinHandle<(String, Result<String, AgentError>)>,
        > = HashMap::new();
        let mut completed_outputs: Vec<(WorkerRole, String)> = Vec::new();

        loop {
            if self.stop_flag.load(Ordering::Relaxed) {
                for (_, h) in handles.drain() {
                    h.abort();
                }
                let mut tasks_guard = shared.write().await;
                for task in tasks_guard.iter_mut() {
                    if matches!(task.status, TaskStatus::Running | TaskStatus::Pending) {
                        task.status = TaskStatus::Cancelled;
                    }
                }
                break;
            }

            let finished_ids: Vec<String> = handles
                .iter()
                .filter(|(_, h)| h.is_finished())
                .map(|(id, _)| id.clone())
                .collect();

            for id in finished_ids {
                if let Some(handle) = handles.remove(&id) {
                    match handle.await {
                        Ok((task_id, Ok(output))) => {
                            let mut g = shared.write().await;
                            if let Some(t) = g.iter_mut().find(|t| t.id == task_id) {
                                t.status = TaskStatus::Completed(output.clone());
                                completed_outputs.push((t.role.clone(), output));
                            }
                        }
                        Ok((task_id, Err(e))) => {
                            let mut g = shared.write().await;
                            if let Some(t) = g.iter_mut().find(|t| t.id == task_id) {
                                t.status = TaskStatus::Failed(e.to_string());
                            }
                        }
                        Err(_) => {}
                    }
                }
            }

            let ready_ids: Vec<String> = {
                let g = shared.read().await;
                let completed_set: std::collections::HashSet<&str> = g
                    .iter()
                    .filter(|t| matches!(t.status, TaskStatus::Completed(_)))
                    .map(|t| t.id.as_str())
                    .collect();
                g.iter()
                    .filter(|t| {
                        t.status == TaskStatus::Pending
                            && !handles.contains_key(&t.id)
                            && t.depends_on
                                .iter()
                                .all(|dep| completed_set.contains(dep.as_str()))
                    })
                    .map(|t| t.id.clone())
                    .collect()
            };

            for task_id in ready_ids {
                let task = {
                    let mut g = shared.write().await;
                    match g.iter_mut().find(|t| t.id == task_id) {
                        Some(t) if t.status == TaskStatus::Pending => {
                            t.status = TaskStatus::Running;
                            t.clone()
                        }
                        _ => continue,
                    }
                };

                let permit = semaphore.clone().acquire_owned().await.unwrap();

                let worker = SwarmWorker {
                    task: task.clone(),
                    swarm_id: swarm_id.clone(),
                    shared: shared.clone(),
                    event_tx: self.event_tx.clone(),
                    browser_tx: self.browser_tx.clone(),
                    browser_nav_lock: browser_nav_lock.clone(),
                    hil_lock: hil_lock.clone(),
                    ai_config: self.ai_config.clone(),
                    spec: self.spec.clone(),
                    vault: self.vault.clone(),
                    hil_gate: self.hil_gate.clone(),
                    stop_flag: self.stop_flag.clone(),
                    nav_lock_timeout: nav_timeout,
                    worker_timeout,
                };

                let tid = task.id.clone();
                let handle = tokio::spawn(async move {
                    let id = tid.clone();
                    let result = worker.run().await;
                    drop(permit);
                    (id, result)
                });
                handles.insert(task.id.clone(), handle);
            }

            let all_terminal = {
                let g = shared.read().await;
                g.iter().all(|t| {
                    matches!(
                        t.status,
                        TaskStatus::Completed(_) | TaskStatus::Failed(_) | TaskStatus::Cancelled
                    )
                })
            };
            if all_terminal && handles.is_empty() {
                break;
            }

            tokio::time::sleep(Duration::from_millis(200)).await;
        }

        if completed_outputs.is_empty() {
            let _ = self.event_tx.send(AgentEvent::SwarmError {
                swarm_id: swarm_id.clone(),
                error: "All workers failed. Check LLM connection.".into(),
            });
            return Err(AgentError::SwarmCoordinatorFailed("All workers failed".into()));
        }

        let total_tool_calls: u32 = {
            let g = shared.read().await;
            g.iter().map(|t| t.tool_calls_used).sum()
        };

        let final_answer = if self.config.enable_reconciliation && completed_outputs.len() > 1 {
            reconcile(
                completed_outputs,
                self.goal.clone(),
                self.config.mode.clone(),
                &self.ai_client,
            )
            .await
            .unwrap_or_else(|e| format!("Synthesis failed: {e}"))
        } else {
            completed_outputs
                .into_iter()
                .next()
                .map(|(_, out)| out)
                .unwrap_or_default()
        };

        let _ = self.event_tx.send(AgentEvent::SwarmDone {
            swarm_id: swarm_id.clone(),
            final_answer: final_answer.clone(),
            total_tool_calls,
        });

        Ok(final_answer)
    }
}

fn build_coordinator_prompt(
    goal: &str,
    mode: &SwarmMode,
    max_workers: usize,
    enable_disagreement: bool,
) -> String {
    let mode_desc = match mode {
        SwarmMode::DiscoveryAtScale => "DiscoveryAtScale: Create agents that each find a different slice of information. All run fully in parallel.",
        SwarmMode::OutputAtScale => "OutputAtScale: Agents each write a document section. A Writer assembles at the end (depends_on all others).",
        SwarmMode::PerspectiveAtScale => "PerspectiveAtScale: Create agents with opposing roles (Analyst+Skeptic, Researcher+FactChecker). Explicitly create disagreement.",
    };
    format!(
        "You are a swarm coordinator. Decompose this goal into parallel sub-tasks for specialist agents.\n\n\
         GOAL: {goal}\n\nMODE: {mode_desc}\n\n\
         RULES:\n\
         - Return ONLY a JSON array. No prose, no markdown fences.\n\
         - Maximum {max} tasks.\n\
         - Each task must have a fully self-contained `prompt` — workers share no memory.\n\
         - Use `depends_on` (array of task id strings) for ordering.\n\
         - enable_disagreement={disagree}: if true, include at least one Skeptic or FactChecker.\n\n\
         SCHEMA (return exactly this shape, nothing else):\n\
         [{{\"id\":\"t1\",\"role\":\"Researcher|Analyst|FactChecker|Skeptic|Writer|Reviewer|Custom:name\",\"prompt\":\"...\",\"depends_on\":[]}}]",
        goal = goal,
        mode_desc = mode_desc,
        max = max_workers,
        disagree = enable_disagreement,
    )
}

fn parse_plan(s: &str) -> Result<Vec<SwarmTask>, AgentError> {
    let s = s.trim();
    let s = if let Some(rest) = s.strip_prefix("```") {
        rest.split_once('\n').map(|(_, body)| body).unwrap_or(rest)
            .trim_end_matches("```")
            .trim()
    } else {
        s
    };

    let start = s.find('[').ok_or_else(|| {
        AgentError::SwarmCoordinatorFailed("No JSON array found in response".into())
    })?;
    let s = &s[start..];

    #[derive(serde::Deserialize)]
    struct RawTask {
        id: String,
        role: String,
        prompt: String,
        #[serde(default)]
        depends_on: Vec<String>,
    }

    let raw: Vec<RawTask> = serde_json::from_str(s).map_err(|e| {
        AgentError::SwarmCoordinatorFailed(format!("JSON parse error: {e}"))
    })?;

    Ok(raw
        .into_iter()
        .map(|r| SwarmTask {
            id: r.id,
            role: WorkerRole::from_label(&r.role),
            prompt: r.prompt,
            depends_on: r.depends_on,
            status: TaskStatus::Pending,
            worker_id: None,
            tool_calls_used: 0,
            last_message: None,
        })
        .collect())
}
