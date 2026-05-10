# Agent Swarm Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a full multi-agent swarm system — coordinator, parallel workers, reconciler, live UI, and cloud-mock demo endpoint — to KitsuneEngine without breaking the existing single-agent path.

**Architecture:** `kitsune-agent::swarm` (a module inside the existing crate) contains `SwarmCoordinator`, `SwarmWorker`, and a `reconcile` function. The coordinator makes one `AgentAiClient::complete` planning call, dispatches parallel workers capped by a `Semaphore`, each running a `LlmAgentRuntime` with injected `browser_nav_lock` and `hil_lock`. All swarm events flow through the existing `AgentEvent` channel into the existing UI event loop via new `AgentSseAction` variants. The existing single-agent path is completely unchanged when `swarm_mode = false`.

**Tech Stack:** Rust 1.75+, tokio, egui 0.30, axum 0.7, reqwest (rustls-tls), uuid, serde_json, async-stream

---

## File Map

### New Files
| File | Responsibility |
|------|---------------|
| `crates/kitsune-agent/src/swarm/mod.rs` | Module re-exports |
| `crates/kitsune-agent/src/swarm/types.rs` | `SwarmConfig`, `SwarmMode`, `SwarmTask`, `TaskStatus`, `WorkerRole`, `SwarmState` |
| `crates/kitsune-agent/src/swarm/coordinator.rs` | `SwarmCoordinator::run()` — plan + dispatch + reconcile |
| `crates/kitsune-agent/src/swarm/worker.rs` | `SwarmWorker::run()` — single task execution via `LlmAgentRuntime` |
| `crates/kitsune-agent/src/swarm/reconciler.rs` | `reconcile()` — synthesis LLM call |

### Modified Files
| File | What Changes |
|------|-------------|
| `crates/kitsune-agent/src/error.rs` | +3 error variants: `SwarmCoordinatorFailed`, `SwarmWorkerFailed`, `Cancelled` |
| `crates/kitsune-agent/src/lib.rs` | `pub mod swarm;` + re-exports |
| `crates/kitsune-agent/src/loop_runtime.rs` | +4 `AgentEvent` variants; +`nav_lock`, `hil_lock`, `worker_id`, `swarm_id` fields on `LlmAgentRuntime`; +builder methods; `execute_action` nav/hil lock logic |
| `crates/kitsune-ui/src/app.rs` | +4 `AgentSseAction` variants; +`swarm_mode`, `swarm_config`, `swarm_state` on `KitsuneBrowser`; swarm event handling in `process_agent_events`; remove `task_nodes: Vec<TaskNode>` |
| `crates/kitsune-ui/src/panels/agent_panel.rs` | Swarm toggle, config bar, preset cards, status bar, `start_agent_run` branch, `run_swarm` function, pump refactor |
| `crates/kitsune-ui/src/panels/task_graph_panel.rs` | Replace `task_graph_panel(ui, nodes)` with `task_graph_panel(ui, swarm_state)` |
| `crates/kitsune-ui/src/panels/session_panel.rs` | Update call site: `task_graph_panel(ui, &browser.swarm_state)` |
| `crates/kitsune-cloud-mock/src/lib.rs` | +`POST /api/swarm-plan` SSE endpoint |

---

## Critical Pre-Implementation Notes

1. **`LlmBackend` is private** to `loop_runtime.rs`. Workers cannot share it. Each worker constructs its own `LlmAgentRuntime::new_with_config(spec, ai_config, ...)`. Pass `AiProviderConfig` (which is `Clone`) to workers.

2. **`AgentAiClient` is not `Clone`** but lives behind `Arc`. Coordinator receives `Arc<AgentAiClient>` built from the same `AiProviderConfig` as the single-agent path. Workers do NOT use `AgentAiClient` — they use `LlmAgentRuntime`.

3. **`orchestrator::TaskStatus` conflict**: `kitsune-agent/src/lib.rs` already re-exports `pub use orchestrator::TaskStatus`. The swarm `TaskStatus` (different type, has `Completed(String)`) lives only in `swarm::types` and is NOT re-exported from `lib.rs`. Import it as `kitsune_agent::swarm::types::TaskStatus` in UI code.

4. **`run()` return type**: `LlmAgentRuntime::run()` already returns `Result<String, AgentError>`. No change needed to the signature. Call sites that ignore the result keep `let _ = runtime.run(...).await;`.

5. **`AgentEvent::SwarmPlanReady`** includes a `goal: String` field so the UI can populate `SwarmState.goal` without a separate channel.

6. **`task_nodes: Vec<TaskNode>`** is removed from `KitsuneBrowser` (it was an unpopulated stub). The import `use crate::panels::task_graph_panel::TaskNode` in `app.rs` is also removed. `TaskNode` struct remains in `task_graph_panel.rs` for future use.

7. **Pump refactoring**: The existing `match event { ... }` in `run_in_process_agent` returns `AgentSseAction` directly. After adding swarm variants to `AgentEvent`, this match becomes non-exhaustive. Refactor to return `Option<AgentSseAction>` — swarm arms return `None` in the single-agent pump.

8. **`AgentError::Cancelled`** enables workers to distinguish user-stop from other errors.

---

## Task 1: Add error variants, create swarm types, add swarm module to lib

**Files:**
- Modify: `crates/kitsune-agent/src/error.rs`
- Create: `crates/kitsune-agent/src/swarm/types.rs`
- Create: `crates/kitsune-agent/src/swarm/mod.rs` (stub)
- Modify: `crates/kitsune-agent/src/lib.rs`

- [ ] **Step 1: Add error variants to `error.rs`**

Open `crates/kitsune-agent/src/error.rs`. After the `InvalidParameters` variant, add:

```rust
    #[error("Swarm coordinator failed: {0}")]
    SwarmCoordinatorFailed(String),

    #[error("Swarm worker '{worker_id}' failed: {reason}")]
    SwarmWorkerFailed { worker_id: String, reason: String },

    #[error("Agent operation cancelled")]
    Cancelled,
```

- [ ] **Step 2: Create `crates/kitsune-agent/src/swarm/types.rs`**

```rust
use serde::{Deserialize, Serialize};

pub type SwarmId = String;
pub type WorkerId = String;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SwarmConfig {
    pub max_workers: usize,
    pub mode: SwarmMode,
    pub enable_reconciliation: bool,
    pub enable_disagreement: bool,
    pub worker_timeout_seconds: u64,
    pub nav_lock_timeout_seconds: u64,
}

impl Default for SwarmConfig {
    fn default() -> Self {
        Self {
            max_workers: 10,
            mode: SwarmMode::PerspectiveAtScale,
            enable_reconciliation: true,
            enable_disagreement: true,
            worker_timeout_seconds: 120,
            nav_lock_timeout_seconds: 30,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum SwarmMode {
    DiscoveryAtScale,
    OutputAtScale,
    PerspectiveAtScale,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum WorkerRole {
    Coordinator,
    Researcher,
    Analyst,
    FactChecker,
    Writer,
    Reviewer,
    Skeptic,
    Synthesizer,
    Custom(String),
}

impl WorkerRole {
    pub fn as_str(&self) -> &str {
        match self {
            WorkerRole::Coordinator => "Coordinator",
            WorkerRole::Researcher => "Researcher",
            WorkerRole::Analyst => "Analyst",
            WorkerRole::FactChecker => "FactChecker",
            WorkerRole::Writer => "Writer",
            WorkerRole::Reviewer => "Reviewer",
            WorkerRole::Skeptic => "Skeptic",
            WorkerRole::Synthesizer => "Synthesizer",
            WorkerRole::Custom(s) => s.as_str(),
        }
    }

    pub fn from_label(s: &str) -> Self {
        match s {
            "Coordinator" => WorkerRole::Coordinator,
            "Researcher" => WorkerRole::Researcher,
            "Analyst" => WorkerRole::Analyst,
            "FactChecker" => WorkerRole::FactChecker,
            "Writer" => WorkerRole::Writer,
            "Reviewer" => WorkerRole::Reviewer,
            "Skeptic" => WorkerRole::Skeptic,
            "Synthesizer" => WorkerRole::Synthesizer,
            s if s.starts_with("Custom:") => WorkerRole::Custom(s[7..].to_string()),
            other => WorkerRole::Custom(other.to_string()),
        }
    }

    pub fn persona_prompt(&self) -> String {
        match self {
            WorkerRole::Researcher => "You are a Researcher. Your specialty is finding authoritative sources. Prefer Download and ReadFile over navigation. Cite everything.".into(),
            WorkerRole::Analyst => "You are an Analyst. Synthesize data into structured insights. Be concise and evidence-based.".into(),
            WorkerRole::FactChecker => "You are a Fact-Checker. Challenge every claim. Look for primary sources. Flag unverified assertions explicitly.".into(),
            WorkerRole::Skeptic => "You are a Skeptic. Find weaknesses, counterarguments, and missing evidence in any conclusion. Be rigorous.".into(),
            WorkerRole::Writer => "You are a Writer. Produce clear, well-structured prose. Organize findings into logical sections with headers.".into(),
            WorkerRole::Reviewer => "You are a Reviewer. Evaluate content quality. Flag logical gaps, unsupported claims, and structural problems.".into(),
            WorkerRole::Synthesizer => "You are a Synthesizer. Merge multiple perspectives into a unified, balanced report that explicitly acknowledges disagreements.".into(),
            _ => String::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum TaskStatus {
    Pending,
    Running,
    Completed(String),
    Failed(String),
    Cancelled,
}

impl TaskStatus {
    pub fn emoji(&self) -> &str {
        match self {
            TaskStatus::Pending => "🟡",
            TaskStatus::Running => "🔵",
            TaskStatus::Completed(_) => "✅",
            TaskStatus::Failed(_) => "🔴",
            TaskStatus::Cancelled => "⬛",
        }
    }

    pub fn label(&self) -> &str {
        match self {
            TaskStatus::Pending => "Pending",
            TaskStatus::Running => "Running",
            TaskStatus::Completed(_) => "Completed",
            TaskStatus::Failed(_) => "Failed",
            TaskStatus::Cancelled => "Cancelled",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SwarmTask {
    pub id: String,
    pub role: WorkerRole,
    pub prompt: String,
    pub depends_on: Vec<String>,
    pub status: TaskStatus,
    pub worker_id: Option<WorkerId>,
    pub tool_calls_used: u32,
    pub last_message: Option<String>,
}

pub struct SwarmState {
    pub swarm_id: SwarmId,
    pub goal: String,
    pub config: SwarmConfig,
    pub tasks: Vec<SwarmTask>,
    pub final_answer: Option<String>,
    pub total_tool_calls: u32,
    pub started_at: std::time::Instant,
}

impl SwarmState {
    pub fn active_count(&self) -> usize {
        self.tasks.iter().filter(|t| t.status == TaskStatus::Running).count()
    }
    pub fn completed_count(&self) -> usize {
        self.tasks.iter().filter(|t| matches!(t.status, TaskStatus::Completed(_))).count()
    }
    pub fn pending_count(&self) -> usize {
        self.tasks.iter().filter(|t| t.status == TaskStatus::Pending).count()
    }
}
```

- [ ] **Step 3: Create `crates/kitsune-agent/src/swarm/mod.rs`** (minimal stub for now)

```rust
// ARCHITECTURE: kitsune_agent::swarm — swarm lifecycle.
// SwarmCoordinator: plan → dispatch → reconcile → done.
// SwarmWorkers: ephemeral; each runs one LlmAgentRuntime with injected nav/hil locks.
// INVARIANT: max_workers enforced by Semaphore in coordinator.
// INVARIANT: browser_nav_lock held for every Navigate/Click/Fill action.
// INVARIANT: hil_lock held for every HilGate::checkpoint call from a worker.

pub mod coordinator;
pub mod reconciler;
pub mod types;
pub mod worker;

pub use coordinator::SwarmCoordinator;
pub use reconciler::reconcile;
pub use types::{
    SwarmConfig, SwarmId, SwarmMode, SwarmState, SwarmTask, TaskStatus, WorkerRole, WorkerId,
};
```

But `coordinator`, `worker`, `reconciler` don't exist yet. Create empty stubs so it compiles:

- Create `crates/kitsune-agent/src/swarm/coordinator.rs`:
```rust
// Stub — implemented in Task 10
use crate::error::AgentError;
pub struct SwarmCoordinator;
impl SwarmCoordinator {
    pub async fn run(self) -> Result<String, AgentError> {
        Err(AgentError::SwarmCoordinatorFailed("not yet implemented".into()))
    }
}
```

- Create `crates/kitsune-agent/src/swarm/worker.rs`:
```rust
// Stub — implemented in Task 11
use crate::error::AgentError;
pub struct SwarmWorker;
impl SwarmWorker {
    pub async fn run(self) -> Result<String, AgentError> {
        Err(AgentError::SwarmCoordinatorFailed("not yet implemented".into()))
    }
}
```

- Create `crates/kitsune-agent/src/swarm/reconciler.rs`:
```rust
// Stub — implemented in Task 12
use crate::error::AgentError;
pub async fn reconcile(
    _inputs: Vec<(super::types::WorkerRole, String)>,
    _goal: String,
    _mode: super::types::SwarmMode,
    _ai_client: &crate::ai_client::AgentAiClient,
) -> Result<String, AgentError> {
    Ok(_inputs.into_iter().map(|(_, o)| o).collect::<Vec<_>>().join("\n---\n"))
}
```

- [ ] **Step 4: Add `pub mod swarm;` to `lib.rs`**

In `crates/kitsune-agent/src/lib.rs`, after the existing `pub mod tools;` line, add:

```rust
pub mod swarm;
pub use swarm::{SwarmConfig, SwarmCoordinator, SwarmMode, SwarmState, SwarmTask};
```

Note: do NOT re-export `swarm::TaskStatus` — it would conflict with `orchestrator::TaskStatus` already re-exported via `pub use orchestrator::{..., TaskStatus}`.

- [ ] **Step 5: Compile check**

```powershell
cargo check -p kitsune-agent 2>&1
```

Expected: zero errors. Fix any type mismatches before continuing.

- [ ] **Step 6: Commit**

```powershell
git add crates/kitsune-agent/src/error.rs `
        crates/kitsune-agent/src/swarm/ `
        crates/kitsune-agent/src/lib.rs
git commit -m "feat(agent): add swarm module skeleton — types, error variants, stubs"
```

---

## Task 2: Add `AgentEvent` swarm variants + `AgentSseAction` swarm variants

**Files:**
- Modify: `crates/kitsune-agent/src/loop_runtime.rs`
- Modify: `crates/kitsune-ui/src/app.rs`

- [ ] **Step 1: Add four variants to `AgentEvent` in `loop_runtime.rs`**

After the existing `TokenUsage` variant, add:

```rust
    /// Swarm worker status update. Emitted by coordinator and workers.
    SwarmUpdate {
        swarm_id: String,
        worker_id: String,
        role: String,
        status: String,
        message: String,
        tool_calls_used: u32,
    },
    /// Coordinator finished planning — task list is ready.
    SwarmPlanReady {
        swarm_id: String,
        goal: String,
        tasks: Vec<crate::swarm::types::SwarmTask>,
    },
    /// Final swarm answer — all workers done, reconciliation complete.
    SwarmDone {
        swarm_id: String,
        final_answer: String,
        total_tool_calls: u32,
    },
    /// Swarm-level error — coordinator or all workers failed.
    SwarmError {
        swarm_id: String,
        error: String,
    },
```

- [ ] **Step 2: Compile check `kitsune-agent`**

```powershell
cargo check -p kitsune-agent 2>&1
```

Fix any issues before continuing.

- [ ] **Step 3: Add four variants to `AgentSseAction` in `app.rs`**

Open `crates/kitsune-ui/src/app.rs`. After the existing `TokenUsage { input: u32, output: u32 }` variant in the `AgentSseAction` enum, add:

```rust
    SwarmPlanReady {
        swarm_id: String,
        goal: String,
        tasks: Vec<kitsune_agent::swarm::types::SwarmTask>,
    },
    SwarmUpdate {
        swarm_id: String,
        worker_id: String,
        role: String,
        status: String,
        message: String,
        tool_calls_used: u32,
    },
    SwarmDone {
        swarm_id: String,
        final_answer: String,
        total_tool_calls: u32,
    },
    SwarmError {
        swarm_id: String,
        error: String,
    },
```

- [ ] **Step 4: Add placeholder handling in `process_agent_events` in `app.rs`**

In the `process_agent_events` method, after the existing match arms, add:

```rust
                AgentSseAction::SwarmPlanReady { .. } => {
                    // Handled in Task 4
                }
                AgentSseAction::SwarmUpdate { role, message, .. } => {
                    self.push_log(format!("[{}] {}", role, message), LogLevel::Step);
                }
                AgentSseAction::SwarmDone { final_answer, .. } => {
                    if !final_answer.is_empty() {
                        self.push_log(final_answer, LogLevel::Ok);
                    }
                    self.agent_state = AgentRunState::Idle;
                }
                AgentSseAction::SwarmError { error, .. } => {
                    self.push_log(format!("Swarm failed: {}", error), LogLevel::Block);
                    self.agent_state = AgentRunState::Idle;
                }
```

- [ ] **Step 5: Compile check workspace**

```powershell
cargo check --workspace 2>&1
```

Fix any issues. The `kitsune-ui` crate will now need `kitsune_agent::swarm::types::SwarmTask` to be importable — verify `swarm` module and `SwarmTask` are public (done in Task 1).

- [ ] **Step 6: Commit**

```powershell
git add crates/kitsune-agent/src/loop_runtime.rs `
        crates/kitsune-ui/src/app.rs
git commit -m "feat: add AgentEvent and AgentSseAction swarm variants with placeholder handlers"
```

---

## Task 3: Extend `LlmAgentRuntime` with nav/hil lock fields and builder methods

**Files:**
- Modify: `crates/kitsune-agent/src/loop_runtime.rs`

This task adds `nav_lock`, `hil_lock`, `worker_id`, `swarm_id` to the runtime struct, four builder methods, helper methods for acquiring locks, and updates `execute_action` and the stop-check.

- [ ] **Step 1: Add fields to `LlmAgentRuntime` struct**

After the existing `agent_context: Option<String>` field, add:

```rust
    /// Shared mutex serializing WebView navigate/click/fill across swarm workers.
    /// `None` in single-agent mode.
    nav_lock: Option<Arc<tokio::sync::Mutex<()>>>,
    /// Shared mutex serializing HIL checkpoint dialogs across swarm workers.
    /// `None` in single-agent mode.
    hil_lock: Option<Arc<tokio::sync::Mutex<()>>>,
    /// Worker ID for log prefixing. `None` in single-agent mode.
    worker_id: Option<String>,
    /// Swarm ID this worker belongs to. `None` in single-agent mode.
    swarm_id: Option<String>,
```

- [ ] **Step 2: Initialize new fields in both constructors**

In both `LlmAgentRuntime::new` and `LlmAgentRuntime::new_with_config`, add to the `Self { }` block:

```rust
            nav_lock: None,
            hil_lock: None,
            worker_id: None,
            swarm_id: None,
```

- [ ] **Step 3: Add builder methods**

After the existing `with_agent_context` method, add:

```rust
    /// Inject a shared browser nav lock (swarm mode only).
    pub fn with_nav_lock(mut self, lock: Arc<tokio::sync::Mutex<()>>) -> Self {
        self.nav_lock = Some(lock);
        self
    }

    /// Inject a shared HIL serialization lock (swarm mode only).
    pub fn with_hil_lock(mut self, lock: Arc<tokio::sync::Mutex<()>>) -> Self {
        self.hil_lock = Some(lock);
        self
    }

    /// Tag this runtime as belonging to a swarm worker (enables log prefixing).
    pub fn with_worker_id(mut self, worker_id: String, swarm_id: String) -> Self {
        self.worker_id = Some(worker_id);
        self.swarm_id = Some(swarm_id);
        self
    }
```

- [ ] **Step 4: Add private helpers for lock acquisition**

After `is_stopped`, add:

```rust
    async fn acquire_nav_lock(&self) -> Result<Option<tokio::sync::OwnedMutexGuard<()>>, AgentError> {
        let Some(lock) = &self.nav_lock else { return Ok(None); };
        let timeout = Duration::from_secs(30);
        tokio::time::timeout(timeout, lock.clone().lock_owned())
            .await
            .map(Some)
            .map_err(|_| AgentError::SwarmWorkerFailed {
                worker_id: self.worker_id.clone().unwrap_or_default(),
                reason: "browser nav lock timed out".into(),
            })
    }

    async fn acquire_hil_lock(&self) -> Result<Option<tokio::sync::OwnedMutexGuard<()>>, AgentError> {
        let Some(lock) = &self.hil_lock else { return Ok(None); };
        tokio::time::timeout(
            Duration::from_secs(120),
            lock.clone().lock_owned(),
        )
        .await
        .map(Some)
        .map_err(|_| AgentError::PermissionDenied { capability: "hil_lock timed out".into() })
    }
```

- [ ] **Step 5: Update the `emit` method to prefix messages for swarm workers**

Replace the existing `emit` method:

```rust
    fn emit(&self, event: AgentEvent) {
        if let Some(tx) = &self.events {
            let event = if let Some(wid) = &self.worker_id {
                match event {
                    AgentEvent::Log(m) => AgentEvent::Log(format!("[{}] {}", wid, m)),
                    AgentEvent::Step(m) => AgentEvent::Step(format!("[{}] {}", wid, m)),
                    AgentEvent::Thinking(m) => AgentEvent::Thinking(format!("[{}] {}", wid, m)),
                    other => other,
                }
            } else {
                event
            };
            let _ = tx.send(event);
        }
    }
```

- [ ] **Step 6: Update `execute_action` — add nav lock for Navigate, Click, Fill**

At the top of the `AgentAction::Navigate { url }` match arm (before the existing `let url = normalize_url(url);`), add:

```rust
                let _nav_guard = self.acquire_nav_lock().await?;
```

At the top of the `AgentAction::Click { element_id }` match arm (before `let label = elem_label(...)`), add:

```rust
                let _nav_guard = self.acquire_nav_lock().await?;
```

At the top of the `AgentAction::Fill { element_id, value }` match arm (before the `let element = ...` line), add:

```rust
                let _nav_guard = self.acquire_nav_lock().await?;
```

The `_nav_guard` is dropped at the end of each arm's block. This serializes concurrent workers.

- [ ] **Step 7: Add HIL lock around `hil_gate.checkpoint()` in the Fill arm**

In the `if sensitive { ... }` block inside the Fill arm, wrap the `self.hil_gate.checkpoint(...)` call:

Replace the existing sensitive block:

```rust
                if sensitive {
                    self.log(format!(
                        "⚠ sensitive field [{}] — requesting human approval",
                        element_id
                    ));
                    let trigger = HilTriggerClass::ExternalSideEffect {
                        description: format!(
                            "Agent wants to fill a sensitive field on {}",
                            observation.url
                        ),
                        reversible: true,
                    };
                    let approval = self
                        .hil_gate
                        .checkpoint(trigger, vec!["sensitive-field".into()])
                        .await
                        .map_err(|e| AgentError::PermissionDenied {
                            capability: format!("HIL rejected: {}", e),
                        })?;
                    drop(approval);
                }
```

With:

```rust
                if sensitive {
                    self.log(format!(
                        "⚠ sensitive field [{}] — requesting human approval",
                        element_id
                    ));
                    // Serialize HIL dialogs across swarm workers — only one at a time.
                    let _hil_guard = self.acquire_hil_lock().await?;
                    let trigger = HilTriggerClass::ExternalSideEffect {
                        description: format!(
                            "Agent wants to fill a sensitive field on {}",
                            observation.url
                        ),
                        reversible: true,
                    };
                    let approval = self
                        .hil_gate
                        .checkpoint(trigger, vec!["sensitive-field".into()])
                        .await
                        .map_err(|e| AgentError::PermissionDenied {
                            capability: format!("HIL rejected: {}", e),
                        })?;
                    drop(approval);
                    // _hil_guard dropped here — next worker can request HIL
                }
```

- [ ] **Step 8: Update the stop-check to return `Cancelled` for swarm workers**

In the `run` method, replace the existing stop check:

```rust
            if self.is_stopped() {
                let msg = "■ Agent stopped by user.".to_string();
                self.emit(AgentEvent::Done(msg.clone()));
                return Ok(msg);
            }
```

With:

```rust
            if self.is_stopped() {
                if self.worker_id.is_some() {
                    return Err(AgentError::Cancelled);
                }
                let msg = "■ Agent stopped by user.".to_string();
                self.emit(AgentEvent::Done(msg.clone()));
                return Ok(msg);
            }
```

- [ ] **Step 9: Compile check**

```powershell
cargo check -p kitsune-agent 2>&1
```

Fix any issues (likely: `tokio::sync::Mutex` needs to be used instead of `std::sync::Mutex` for `OwnedMutexGuard`; ensure the `Arc<tokio::sync::Mutex<()>>` types match).

- [ ] **Step 10: Commit**

```powershell
git add crates/kitsune-agent/src/loop_runtime.rs
git commit -m "feat(agent): add nav_lock/hil_lock to LlmAgentRuntime for swarm serialization"
```

---

## Task 4: Add swarm state to `KitsuneBrowser` + update `session_panel.rs`

**Files:**
- Modify: `crates/kitsune-ui/src/app.rs`
- Modify: `crates/kitsune-ui/src/panels/session_panel.rs`
- Modify: `crates/kitsune-ui/src/panels/task_graph_panel.rs`

- [ ] **Step 1: Remove `task_nodes` field from `KitsuneBrowser`**

In `crates/kitsune-ui/src/app.rs`:

1. Remove the import line `use crate::panels::task_graph_panel::TaskNode;`
2. Remove `pub task_nodes: Vec<TaskNode>,` from the struct
3. Remove `task_nodes: Vec::new(),` from `KitsuneBrowser::new`

- [ ] **Step 2: Add swarm state imports and fields**

At the top of `app.rs`, add imports (after existing kitsune_agent imports):

```rust
use kitsune_agent::swarm::types::{SwarmConfig, SwarmMode, SwarmState, SwarmTask, TaskStatus as SwarmTaskStatus};
```

In the `KitsuneBrowser` struct, after the `selected_agent_card` field, add:

```rust
    // ── Swarm ────────────────────────────────────────────────────────────────
    /// Whether the next run uses the swarm multi-agent coordinator.
    pub swarm_mode: bool,
    /// Configuration for the next swarm run.
    pub swarm_config: SwarmConfig,
    /// Live state of the currently active swarm (None when idle).
    pub swarm_state: Option<SwarmState>,
```

- [ ] **Step 3: Initialize swarm fields in `KitsuneBrowser::new`**

After the existing `selected_agent_card: None,` line in `new`, add:

```rust
            swarm_mode: false,
            swarm_config: SwarmConfig::default(),
            swarm_state: None,
```

- [ ] **Step 4: Implement `SwarmPlanReady` in `process_agent_events`**

Replace the placeholder body added in Task 2:

```rust
                AgentSseAction::SwarmPlanReady { .. } => {
                    // Handled in Task 4
                }
```

With:

```rust
                AgentSseAction::SwarmPlanReady { swarm_id, goal, tasks } => {
                    self.swarm_state = Some(SwarmState {
                        swarm_id,
                        goal,
                        config: self.swarm_config.clone(),
                        tasks,
                        final_answer: None,
                        total_tool_calls: 0,
                        started_at: std::time::Instant::now(),
                    });
                }
```

- [ ] **Step 5: Update `SwarmUpdate` handler to update task state**

Replace the Step 4 placeholder from Task 2:

```rust
                AgentSseAction::SwarmUpdate { role, message, .. } => {
                    self.push_log(format!("[{}] {}", role, message), LogLevel::Step);
                }
```

With the full handler:

```rust
                AgentSseAction::SwarmUpdate { swarm_id: _, worker_id, role, status, message, tool_calls_used } => {
                    if let Some(state) = &mut self.swarm_state {
                        if let Some(task) = state.tasks.iter_mut().find(|t| {
                            t.worker_id.as_deref() == Some(worker_id.as_str())
                                || t.role.as_str() == role.as_str()
                        }) {
                            task.tool_calls_used = tool_calls_used;
                            task.last_message = Some(message.clone());
                            task.status = match status.as_str() {
                                "Running" => SwarmTaskStatus::Running,
                                "Completed" => SwarmTaskStatus::Completed(message.clone()),
                                "Failed" => SwarmTaskStatus::Failed(message.clone()),
                                "Cancelled" => SwarmTaskStatus::Cancelled,
                                _ => task.status.clone(),
                            };
                        }
                        state.total_tool_calls = state.tasks.iter().map(|t| t.tool_calls_used).sum();
                    }
                    self.push_log(format!("[{}] {}", role, message), LogLevel::Step);
                }
```

- [ ] **Step 6: Update `SwarmDone` handler**

Replace the placeholder:

```rust
                AgentSseAction::SwarmDone { final_answer, .. } => {
                    if !final_answer.is_empty() {
                        self.push_log(final_answer, LogLevel::Ok);
                    }
                    self.agent_state = AgentRunState::Idle;
                }
```

With:

```rust
                AgentSseAction::SwarmDone { swarm_id: _, final_answer, total_tool_calls } => {
                    if let Some(state) = &mut self.swarm_state {
                        state.final_answer = Some(final_answer.clone());
                        state.total_tool_calls = total_tool_calls;
                    }
                    if !final_answer.is_empty() {
                        self.push_log(final_answer, LogLevel::Ok);
                    }
                    self.agent_state = AgentRunState::Idle;
                }
```

- [ ] **Step 7: Replace `task_graph_panel` in `task_graph_panel.rs`**

Completely replace the contents of `crates/kitsune-ui/src/panels/task_graph_panel.rs` with:

```rust
use crate::theme::KitsuneTheme;
use eframe::egui;
use kitsune_agent::swarm::types::{SwarmState, TaskStatus};

// TaskNode kept for potential future use by the AgentOrchestrator pipeline.
#[derive(Debug, Clone, PartialEq)]
pub enum NodeStatus {
    Pending,
    Running,
    Completed,
    Failed(String),
}

#[derive(Debug, Clone)]
pub struct TaskNode {
    pub name: String,
    pub model_slot: String,
    pub tokens_used: Option<u32>,
    pub status: NodeStatus,
    pub summary: Option<String>,
}

pub fn task_graph_panel(ui: &mut egui::Ui, swarm_state: &Option<SwarmState>) {
    let Some(state) = swarm_state else {
        ui.centered_and_justified(|ui| {
            ui.label(
                egui::RichText::new("No swarm running.\nEnable Swarm Mode in the agent panel.")
                    .weak()
                    .color(KitsuneTheme::TEXT3),
            );
        });
        return;
    };

    // Metrics bar
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(format!("Active: {}", state.active_count())).color(KitsuneTheme::BLUE).size(10.0));
        ui.separator();
        ui.label(egui::RichText::new(format!("Done: {}", state.completed_count())).color(KitsuneTheme::GREEN).size(10.0));
        ui.separator();
        ui.label(egui::RichText::new(format!("Pending: {}", state.pending_count())).color(KitsuneTheme::AMBER).size(10.0));
        ui.separator();
        ui.label(egui::RichText::new(format!("{}s", state.started_at.elapsed().as_secs())).color(KitsuneTheme::TEXT3).size(10.0));
    });
    ui.add_space(4.0);

    // Task list
    egui::ScrollArea::vertical()
        .id_source("swarm_task_scroll")
        .max_height(200.0)
        .show(ui, |ui| {
            for task in &state.tasks {
                ui.horizontal(|ui| {
                    ui.label(task.status.emoji());
                    ui.label(
                        egui::RichText::new(task.role.as_str())
                            .strong()
                            .color(KitsuneTheme::TEXT_PRIMARY)
                            .size(10.0),
                    );
                    ui.label(
                        egui::RichText::new(format!("[{}]", task.status.label()))
                            .color(KitsuneTheme::TEXT3)
                            .size(9.5),
                    );
                });

                match &task.status {
                    TaskStatus::Completed(output) => {
                        let preview: String = output.chars().take(100).collect();
                        let preview = if output.chars().count() > 100 {
                            format!("{}…", preview)
                        } else {
                            preview
                        };
                        ui.indent(task.id.as_str(), |ui| {
                            ui.label(
                                egui::RichText::new(preview)
                                    .weak()
                                    .color(KitsuneTheme::TEXT2)
                                    .size(9.5),
                            );
                        });
                    }
                    TaskStatus::Running => {
                        if let Some(msg) = &task.last_message {
                            ui.indent(task.id.as_str(), |ui| {
                                ui.label(
                                    egui::RichText::new(msg)
                                        .italics()
                                        .color(KitsuneTheme::TEXT3)
                                        .size(9.5),
                                );
                            });
                        }
                    }
                    TaskStatus::Failed(reason) => {
                        ui.indent(task.id.as_str(), |ui| {
                            ui.label(
                                egui::RichText::new(reason)
                                    .color(KitsuneTheme::RED)
                                    .size(9.5),
                            );
                        });
                    }
                    _ => {}
                }
                ui.add_space(2.0);
            }
        });

    // Final synthesis output
    if let Some(answer) = &state.final_answer {
        ui.separator();
        ui.label(egui::RichText::new("Synthesis").strong().color(KitsuneTheme::AMBER).size(10.0));
        egui::ScrollArea::vertical()
            .id_source("synthesis_scroll")
            .max_height(150.0)
            .show(ui, |ui| {
                ui.label(
                    egui::RichText::new(answer)
                        .color(KitsuneTheme::TEXT_PRIMARY)
                        .size(10.0),
                );
            });
        if ui.small_button("Copy").clicked() {
            ui.output_mut(|o| o.copied_text = answer.clone());
        }
        if ui.small_button("Save .md").clicked() {
            if let Some(dir) = dirs::download_dir().or_else(dirs::home_dir) {
                let path = dir.join(format!(
                    "swarm-{}.md",
                    chrono::Local::now().format("%Y%m%d-%H%M%S")
                ));
                let _ = std::fs::write(&path, answer);
                tracing::info!("Swarm report saved to {:?}", path);
            }
        }
    }
}
```

- [ ] **Step 8: Update `session_panel.rs` call site**

In `crates/kitsune-ui/src/panels/session_panel.rs`, change:

```rust
                        task_graph_panel(ui, &browser.task_nodes);
```

to:

```rust
                        task_graph_panel(ui, &browser.swarm_state);
```

- [ ] **Step 9: Compile check**

```powershell
cargo check --workspace 2>&1
```

The `task_graph_panel.rs` imports `dirs` and `chrono` — verify `kitsune-ui/Cargo.toml` has them. If not, check the workspace Cargo.toml (both are in workspace.dependencies and present in other UI-adjacent crates). Add to `kitsune-ui/Cargo.toml` if missing.

- [ ] **Step 10: Commit**

```powershell
git add crates/kitsune-ui/src/app.rs `
        crates/kitsune-ui/src/panels/task_graph_panel.rs `
        crates/kitsune-ui/src/panels/session_panel.rs
git commit -m "feat(ui): add swarm state fields, live task graph panel, session panel update"
```

---

## Task 5: Add swarm UI to `agent_panel.rs`

**Files:**
- Modify: `crates/kitsune-ui/src/panels/agent_panel.rs`

This task adds: swarm mode toggle, config bar, three preset cards, swarm status bar. Does NOT wire `start_agent_run` yet (that is Task 8).

- [ ] **Step 1: Add imports at top of `agent_panel.rs`**

After the existing imports, add:

```rust
use kitsune_agent::swarm::types::{SwarmConfig, SwarmMode};
```

- [ ] **Step 2: Add swarm preset cards after the existing three agent cards**

In the agent cards section (after the `for card in agents { ... }` loop and before the closing of its `egui::Frame`), add:

```rust
                    // ── Swarm preset cards ────────────────────────────────
                    ui.add_space(6.0);
                    ui.label(
                        egui::RichText::new("SWARM PRESETS")
                            .size(9.0)
                            .color(KitsuneTheme::TEXT3)
                            .family(egui::FontFamily::Monospace),
                    );
                    ui.add_space(3.0);
                    ui.horizontal_wrapped(|ui| {
                        if ui.selectable_label(
                            browser.swarm_mode && browser.swarm_config.mode == SwarmMode::DiscoveryAtScale,
                            "🔍 Discovery",
                        ).clicked() && !is_busy {
                            browser.swarm_mode = true;
                            browser.swarm_config.mode = SwarmMode::DiscoveryAtScale;
                            browser.swarm_config.max_workers = 20;
                        }
                        if ui.selectable_label(
                            browser.swarm_mode && browser.swarm_config.mode == SwarmMode::OutputAtScale,
                            "📄 Report",
                        ).clicked() && !is_busy {
                            browser.swarm_mode = true;
                            browser.swarm_config.mode = SwarmMode::OutputAtScale;
                            browser.swarm_config.max_workers = 10;
                        }
                        if ui.selectable_label(
                            browser.swarm_mode && browser.swarm_config.mode == SwarmMode::PerspectiveAtScale,
                            "🧠 Expert Panel",
                        ).clicked() && !is_busy {
                            browser.swarm_mode = true;
                            browser.swarm_config.mode = SwarmMode::PerspectiveAtScale;
                            browser.swarm_config.max_workers = 5;
                            browser.swarm_config.enable_disagreement = true;
                        }
                    });
```

- [ ] **Step 3: Add swarm toggle button in the button row**

In the button row (`ui.horizontal(|ui| { ... })` containing Run/Stop), after the existing Stop or Run button and before the Clear button, add:

```rust
                        ui.add_space(4.0);
                        let swarm_text = if browser.swarm_mode {
                            egui::RichText::new("🐝 Swarm")
                                .size(10.0)
                                .color(egui::Color32::from_rgb(255, 200, 0))
                                .strong()
                        } else {
                            egui::RichText::new("🐝 Swarm")
                                .size(10.0)
                                .color(KitsuneTheme::TEXT3)
                        };
                        if ui.button(swarm_text).clicked() && !is_busy {
                            browser.swarm_mode = !browser.swarm_mode;
                        }
```

- [ ] **Step 4: Add swarm config bar below the command input, visible only when swarm_mode is on**

After the `ui.add_space(4.0);` that follows the attached files chips section (and before the Button row), add:

```rust
                    // ── Swarm config bar (visible only when swarm mode active) ──
                    if browser.swarm_mode {
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new("Workers:").size(9.5).color(KitsuneTheme::TEXT3));
                            egui::ComboBox::from_id_source("swarm_max_workers")
                                .selected_text(browser.swarm_config.max_workers.to_string())
                                .width(48.0)
                                .show_ui(ui, |ui| {
                                    for n in [3usize, 5, 10, 20, 50] {
                                        ui.selectable_value(
                                            &mut browser.swarm_config.max_workers,
                                            n,
                                            n.to_string(),
                                        );
                                    }
                                });
                            ui.separator();
                            ui.label(egui::RichText::new("Mode:").size(9.5).color(KitsuneTheme::TEXT3));
                            egui::ComboBox::from_id_source("swarm_mode_select")
                                .selected_text(match browser.swarm_config.mode {
                                    SwarmMode::DiscoveryAtScale => "Discovery",
                                    SwarmMode::OutputAtScale => "Output",
                                    SwarmMode::PerspectiveAtScale => "Perspective",
                                })
                                .width(82.0)
                                .show_ui(ui, |ui| {
                                    ui.selectable_value(
                                        &mut browser.swarm_config.mode,
                                        SwarmMode::DiscoveryAtScale,
                                        "Discovery",
                                    );
                                    ui.selectable_value(
                                        &mut browser.swarm_config.mode,
                                        SwarmMode::OutputAtScale,
                                        "Output",
                                    );
                                    ui.selectable_value(
                                        &mut browser.swarm_config.mode,
                                        SwarmMode::PerspectiveAtScale,
                                        "Perspective",
                                    );
                                });
                            ui.separator();
                            ui.checkbox(
                                &mut browser.swarm_config.enable_disagreement,
                                egui::RichText::new("Disagree").size(9.5),
                            );
                        });
                        ui.add_space(3.0);
                    }
```

- [ ] **Step 5: Add swarm status bar (visible when swarm is active)**

After `paint_separator(ui)` (the one before the agent cards), and before the agent cards section itself, add:

```rust
            // ── Swarm status bar (only when swarm is active) ──────────────
            if let Some(state) = &browser.swarm_state {
                egui::Frame::none()
                    .inner_margin(egui::Margin::symmetric(12.0, 5.0))
                    .fill(KitsuneTheme::BG2)
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new("🐝 SWARM")
                                    .size(9.5)
                                    .strong()
                                    .color(egui::Color32::from_rgb(255, 200, 0))
                                    .family(egui::FontFamily::Monospace),
                            );
                            ui.separator();
                            ui.label(egui::RichText::new(format!("🔵 {}", state.active_count())).size(9.5));
                            ui.label(egui::RichText::new(format!("✅ {}", state.completed_count())).size(9.5));
                            ui.label(egui::RichText::new(format!("🟡 {}", state.pending_count())).size(9.5));
                        });
                    });
                paint_separator(ui);
            }
```

- [ ] **Step 6: Compile check**

```powershell
cargo check -p kitsune-ui 2>&1
```

- [ ] **Step 7: Commit**

```powershell
git add crates/kitsune-ui/src/panels/agent_panel.rs
git commit -m "feat(ui): add swarm toggle, config bar, preset cards, and status bar"
```

---

## Task 6: Wire `start_agent_run` for swarm + `run_swarm` function

**Files:**
- Modify: `crates/kitsune-ui/src/panels/agent_panel.rs`

This task:
1. Refactors the existing `run_in_process_agent` pump to handle new `AgentEvent` variants (makes it `Option<AgentSseAction>`)
2. Adds `run_swarm` async function
3. Adds the swarm branch at the top of `start_agent_run`

- [ ] **Step 1: Refactor the pump in `run_in_process_agent` to return `Option<AgentSseAction>`**

In the `run_in_process_agent` function, replace:

```rust
    let pump = tokio::spawn(async move {
        while let Some(event) = events_rx.recv().await {
            let action = match event {
                AgentEvent::Log(m) => AgentSseAction::Log { message: m, class: "info".into() },
                AgentEvent::Step(m) => AgentSseAction::Log { message: m, class: "step".into() },
                AgentEvent::Thinking(t) => AgentSseAction::Log { message: t, class: "think".into() },
                AgentEvent::Navigated(u) => AgentSseAction::UrlUpdate { url: u },
                AgentEvent::Done(m) => AgentSseAction::Done { message: m },
                AgentEvent::Error(e) => AgentSseAction::Log { message: e, class: "block".into() },
                AgentEvent::TokenUsage { input, output } => {
                    AgentSseAction::TokenUsage { input, output }
                }
            };
            if pump_tx.send(action).is_err() {
                break;
            }
        }
    });
```

With:

```rust
    let pump = tokio::spawn(async move {
        while let Some(event) = events_rx.recv().await {
            let maybe_action: Option<AgentSseAction> = match event {
                AgentEvent::Log(m) => Some(AgentSseAction::Log { message: m, class: "info".into() }),
                AgentEvent::Step(m) => Some(AgentSseAction::Log { message: m, class: "step".into() }),
                AgentEvent::Thinking(t) => Some(AgentSseAction::Log { message: t, class: "think".into() }),
                AgentEvent::Navigated(u) => Some(AgentSseAction::UrlUpdate { url: u }),
                AgentEvent::Done(m) => Some(AgentSseAction::Done { message: m }),
                AgentEvent::Error(e) => Some(AgentSseAction::Log { message: e, class: "block".into() }),
                AgentEvent::TokenUsage { input, output } => Some(AgentSseAction::TokenUsage { input, output }),
                // Swarm events don't appear on the single-agent path — no-op.
                AgentEvent::SwarmUpdate { .. }
                | AgentEvent::SwarmPlanReady { .. }
                | AgentEvent::SwarmDone { .. }
                | AgentEvent::SwarmError { .. } => None,
            };
            if let Some(action) = maybe_action {
                if pump_tx.send(action).is_err() {
                    break;
                }
            }
        }
    });
```

- [ ] **Step 2: Add necessary imports to `agent_panel.rs`**

At the top of `agent_panel.rs`, after existing imports, add:

```rust
use kitsune_agent::ai_client::AgentAiClient;
use kitsune_agent::swarm::coordinator::SwarmCoordinator;
use std::sync::Arc;
```

- [ ] **Step 3: Add `run_swarm` function**

After the closing brace of `run_in_process_agent`, add:

```rust
async fn run_swarm(
    spec: kitsune_agent::spec::AgentSpec,
    ai_config: kitsune_agent::ai_client::AiProviderConfig,
    ai_client: Arc<AgentAiClient>,
    config: kitsune_agent::swarm::types::SwarmConfig,
    goal: String,
    vault: Arc<kitsune_vault::VaultBackend>,
    hil_gate: Arc<kitsune_hil::HilGate>,
    webview_tx: tokio::sync::mpsc::Sender<kitsune_agent::executor::WebViewCommand>,
    ui_tx: std::sync::mpsc::Sender<AgentSseAction>,
    stop_flag: kitsune_agent::StopFlag,
) {
    use kitsune_agent::AgentEvent;

    let (events_tx, mut events_rx) =
        tokio::sync::mpsc::unbounded_channel::<AgentEvent>();

    let coordinator = SwarmCoordinator {
        goal,
        config,
        spec,
        ai_client,
        ai_config,
        event_tx: events_tx.clone(),
        browser_tx: webview_tx,
        vault,
        hil_gate,
        stop_flag,
    };

    let pump_tx = ui_tx.clone();
    let pump = tokio::spawn(async move {
        while let Some(event) = events_rx.recv().await {
            let maybe_action: Option<AgentSseAction> = match event {
                AgentEvent::Log(m) => Some(AgentSseAction::Log { message: m, class: "info".into() }),
                AgentEvent::Step(m) => Some(AgentSseAction::Log { message: m, class: "step".into() }),
                AgentEvent::Thinking(t) => Some(AgentSseAction::Log { message: t, class: "think".into() }),
                AgentEvent::Navigated(u) => Some(AgentSseAction::UrlUpdate { url: u }),
                AgentEvent::Done(m) => Some(AgentSseAction::Log { message: m, class: "ok".into() }),
                AgentEvent::Error(e) => Some(AgentSseAction::Log { message: e, class: "block".into() }),
                AgentEvent::TokenUsage { input, output } => Some(AgentSseAction::TokenUsage { input, output }),
                AgentEvent::SwarmUpdate { swarm_id, worker_id, role, status, message, tool_calls_used } => {
                    Some(AgentSseAction::SwarmUpdate { swarm_id, worker_id, role, status, message, tool_calls_used })
                }
                AgentEvent::SwarmPlanReady { swarm_id, goal, tasks } => {
                    Some(AgentSseAction::SwarmPlanReady { swarm_id, goal, tasks })
                }
                AgentEvent::SwarmDone { swarm_id, final_answer, total_tool_calls } => {
                    Some(AgentSseAction::SwarmDone { swarm_id, final_answer, total_tool_calls })
                }
                AgentEvent::SwarmError { swarm_id, error } => {
                    Some(AgentSseAction::SwarmError { swarm_id, error })
                }
            };
            if let Some(action) = maybe_action {
                if pump_tx.send(action).is_err() {
                    break;
                }
            }
        }
    });

    let result = coordinator.run().await;
    drop(events_tx);
    let _ = pump.await;

    if let Err(e) = result {
        tracing::error!("Swarm coordinator error: {:?}", e);
        // SwarmError was emitted inside coordinator.run() for most failures.
        // Ensure UI is unblocked regardless.
        let _ = ui_tx.send(AgentSseAction::Done { message: String::new() });
    }
}
```

- [ ] **Step 4: Add swarm branch at the top of `start_agent_run`**

At the top of `start_agent_run`, after `browser.agent_stop_flag.store(false, Ordering::Relaxed);` and after building `cmd` and logging it, and after the vault check — but BEFORE the existing `let endpoint = ...` / `ai_config` build — add a swarm guard that builds `ai_config` first (so both paths can use it). Restructure like this:

The existing code builds `ai_config` after the vault check. We need `ai_config` in the swarm branch too. **Extract the `ai_config` build to before the swarm branch:**

```rust
    // ... existing vault check ...
    let hil_gate = browser.hil_gate.clone();
    let webview_tx = browser.webview_cmd_tx();
    let agent_tx = browser.agent_tx();
    let file_perm_slot = browser.file_perm_slot.clone();
    let stop_flag = browser.agent_stop_flag.clone();
    let spec = build_runtime_spec(browser);
    let agent_context = browser.selected_agent_card
        .as_deref()
        .map(specialist_context)
        .unwrap_or_default();

    let endpoint = browser.settings_endpoint.trim().to_string();
    let model = browser.settings_model.trim().to_string();
    let api_key = browser.settings_api_key.clone();
    let preset = browser.settings_cloud_preset;
    let ai_config = match browser.settings_provider {
        // ... exact same ai_config build as before (copy verbatim) ...
    };

    // ── Swarm branch ─────────────────────────────────────────────────────────
    if browser.swarm_mode {
        let swarm_config = browser.swarm_config.clone();
        // Clamp max_workers to at least 1
        let mut swarm_config = swarm_config;
        if swarm_config.max_workers == 0 {
            swarm_config.max_workers = 1;
        }

        let ai_client = match AgentAiClient::new(ai_config.clone()) {
            Ok(c) => Arc::new(c),
            Err(e) => {
                browser.push_log(format!("Failed to build AI client for swarm: {}", e), LogLevel::Block);
                browser.agent_state = AgentRunState::Idle;
                return;
            }
        };

        let goal = cmd.clone();
        browser.runtime().spawn(async move {
            run_swarm(
                spec, ai_config, ai_client, swarm_config, goal,
                vault, hil_gate, webview_tx, agent_tx, stop_flag,
            )
            .await;
        });
        return; // Do NOT fall through to single-agent path
    }

    // ── Existing single-agent path (unchanged below) ──────────────────────────
    // ... rest of start_agent_run ...
```

**Important:** the exact placement is after the `ai_config` build and before the `let context = ...` / `browser.runtime().spawn(...)` call of the single-agent path. The single-agent path's `file_perm_slot` and `context` build should remain below the `return`.

- [ ] **Step 5: Compile check**

```powershell
cargo check --workspace 2>&1
```

Likely issues:
- `SwarmCoordinator` struct fields not matching (the stub has no fields). That's OK for now — the real struct is implemented in Task 10.
- `run_swarm` references `SwarmCoordinator { goal, config, ... }` with fields that the stub doesn't have. **Fix by temporarily using the stub constructor pattern:** change the stub in `coordinator.rs` to a struct that has all the needed fields but `run()` still returns an error. This allows the compiler to resolve.

Update `crates/kitsune-agent/src/swarm/coordinator.rs` stub to have the real fields:

```rust
// Stub — real implementation in Task 10
use std::sync::Arc;
use tokio::sync::mpsc;
use crate::ai_client::{AgentAiClient, AiProviderConfig};
use crate::error::AgentError;
use crate::executor::WebViewCommand;
use crate::loop_runtime::{AgentEvent, StopFlag};
use crate::spec::AgentSpec;
use crate::swarm::types::SwarmConfig;
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
        let _ = self.event_tx.send(AgentEvent::SwarmError {
            swarm_id: "stub".into(),
            error: "SwarmCoordinator not yet implemented — see Task 10".into(),
        });
        Err(AgentError::SwarmCoordinatorFailed("stub".into()))
    }
}
```

- [ ] **Step 6: Compile check again**

```powershell
cargo check --workspace 2>&1
```

- [ ] **Step 7: Commit**

```powershell
git add crates/kitsune-ui/src/panels/agent_panel.rs `
        crates/kitsune-agent/src/swarm/coordinator.rs
git commit -m "feat(ui): wire start_agent_run swarm branch and run_swarm function"
```

---

## Task 7: Add `/api/swarm-plan` SSE endpoint to cloud mock

**Files:**
- Modify: `crates/kitsune-cloud-mock/src/lib.rs`

- [ ] **Step 1: Add request/response types**

In `lib.rs`, before the `router()` function, add:

```rust
// ---------------------------------------------------------------------------
// Swarm-plan demo endpoint
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct SwarmPlanInput {
    goal: String,
    #[serde(default)]
    mode: String,
    #[serde(default = "default_swarm_workers")]
    max_workers: usize,
}

fn default_swarm_workers() -> usize { 3 }
```

- [ ] **Step 2: Add the handler function**

```rust
/// POST /api/swarm-plan — demo SSE stream that simulates a swarm run.
async fn swarm_plan(Json(input): Json<SwarmPlanInput>) -> Response {
    let goal = input.goal;
    let max_workers = input.max_workers.min(10).max(1);

    let stream = async_stream::stream! {
        use axum::response::sse::Event;

        // Coordinator planning
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        let update = serde_json::json!({
            "type": "SwarmUpdate",
            "swarm_id": "demo-swarm",
            "worker_id": "coordinator",
            "role": "Coordinator",
            "status": "Running",
            "message": "Decomposing goal into parallel tasks...",
            "tool_calls_used": 0
        });
        yield Ok::<_, std::convert::Infallible>(
            Event::default().event("swarm").data(update.to_string())
        );

        // Plan ready — emit N fake tasks
        tokio::time::sleep(std::time::Duration::from_millis(800)).await;
        let roles = ["Researcher", "Analyst", "Skeptic", "FactChecker", "Writer"];
        let tasks: Vec<serde_json::Value> = (0..max_workers).map(|i| {
            serde_json::json!({
                "id": format!("task-{}", i),
                "role": roles[i % roles.len()],
                "prompt": format!("Investigate aspect {} of: {}", i + 1, goal),
                "depends_on": [],
                "status": "Pending",
                "worker_id": null,
                "tool_calls_used": 0,
                "last_message": null
            })
        }).collect();
        let plan = serde_json::json!({
            "type": "SwarmPlanReady",
            "swarm_id": "demo-swarm",
            "goal": goal.clone(),
            "tasks": tasks.clone()
        });
        yield Ok(Event::default().event("swarm").data(plan.to_string()));

        // Workers running
        for (i, task) in tasks.iter().enumerate() {
            tokio::time::sleep(std::time::Duration::from_millis(300)).await;
            let role = task["role"].as_str().unwrap_or("Worker");
            let running = serde_json::json!({
                "type": "SwarmUpdate",
                "swarm_id": "demo-swarm",
                "worker_id": format!("worker-{}-{}", role.to_lowercase(), i),
                "role": role,
                "status": "Running",
                "message": format!("Searching for information..."),
                "tool_calls_used": 0
            });
            yield Ok(Event::default().event("swarm").data(running.to_string()));
        }

        // Workers completing
        for (i, task) in tasks.iter().enumerate() {
            tokio::time::sleep(std::time::Duration::from_millis(600)).await;
            let role = task["role"].as_str().unwrap_or("Worker");
            let done = serde_json::json!({
                "type": "SwarmUpdate",
                "swarm_id": "demo-swarm",
                "worker_id": format!("worker-{}-{}", role.to_lowercase(), i),
                "role": role,
                "status": "Completed",
                "message": format!("{} found {} relevant data points.", role, (i + 1) * 3),
                "tool_calls_used": i as u32 + 2
            });
            yield Ok(Event::default().event("swarm").data(done.to_string()));
        }

        // Synthesis
        tokio::time::sleep(std::time::Duration::from_millis(800)).await;
        let synthesizing = serde_json::json!({
            "type": "SwarmUpdate",
            "swarm_id": "demo-swarm",
            "worker_id": "coordinator",
            "role": "Coordinator",
            "status": "Running",
            "message": "Reconciling all perspectives into final synthesis...",
            "tool_calls_used": 0
        });
        yield Ok(Event::default().event("swarm").data(synthesizing.to_string()));

        tokio::time::sleep(std::time::Duration::from_millis(1000)).await;
        let final_answer = format!(
            "## Swarm Analysis: {}\n\n**Agreement:** All {} agents confirmed the topic is well-documented.\n\n**Key Findings:**\n- Researcher: Found primary sources\n- Analyst: Identified trends\n- Skeptic: Challenged assumptions\n\n**Conclusion:** Comprehensive analysis complete.",
            goal, max_workers
        );
        let swarm_done = serde_json::json!({
            "type": "SwarmDone",
            "swarm_id": "demo-swarm",
            "final_answer": final_answer,
            "total_tool_calls": max_workers * 3
        });
        yield Ok(Event::default().event("swarm").data(swarm_done.to_string()));

        yield Ok(Event::default().event("done").data("{}"));
    };

    Sse::new(stream)
        .keep_alive(
            axum::response::sse::KeepAlive::new()
                .interval(std::time::Duration::from_secs(15))
                .text("ping"),
        )
        .into_response()
}
```

- [ ] **Step 3: Register route in `router()`**

In the `router()` function, add:

```rust
        .route("/api/swarm-plan", post(swarm_plan))
```

(Place after the existing `.route("/api/hil-response", ...)` line.)

Note: the `swarm_plan` handler has no `State(...)` extractor, so no `with_state` conflict.

- [ ] **Step 4: Compile check**

```powershell
cargo check -p kitsune-cloud-mock 2>&1
```

- [ ] **Step 5: Commit**

```powershell
git add crates/kitsune-cloud-mock/src/lib.rs
git commit -m "feat(mock): add /api/swarm-plan SSE demo endpoint"
```

---

## Task 8: Implement `SwarmCoordinator` (real LLM call)

**Files:**
- Modify: `crates/kitsune-agent/src/swarm/coordinator.rs`

Replace the stub with the full implementation.

- [ ] **Step 1: Write the full `coordinator.rs`**

```rust
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

        // First attempt at parsing the plan
        let tasks = match self.ai_client.complete(&prompt, ModelTier::Orchestrator).await {
            Ok(raw) => match parse_plan(&raw) {
                Ok(t) => t,
                Err(_) => {
                    // Retry with corrective message
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

        // Enforce max_workers cap
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

        // Shared mutable task state
        let shared = Arc::new(tokio::sync::RwLock::new(tasks));

        // Concurrency control
        let max_concurrent = self.config.max_workers.max(1);
        let semaphore = Arc::new(Semaphore::new(max_concurrent));
        let browser_nav_lock = Arc::new(tokio::sync::Mutex::new(()));
        let hil_lock = Arc::new(tokio::sync::Mutex::new(()));

        let nav_timeout = Duration::from_secs(self.config.nav_lock_timeout_seconds);
        let worker_timeout = Duration::from_secs(self.config.worker_timeout_seconds);

        // Track running task handles: task_id → JoinHandle<(task_id, Result<String>)>
        let mut handles: HashMap<
            String,
            tokio::task::JoinHandle<(String, Result<String, AgentError>)>,
        > = HashMap::new();
        let mut completed_outputs: Vec<(WorkerRole, String)> = Vec::new();

        loop {
            // Check stop flag
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

            // Harvest finished handles
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
                        Err(_) => {} // task panicked or was aborted
                    }
                }
            }

            // Resolve dependency graph — find Pending tasks whose deps are all Completed
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
                // Mark Running in shared state
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

            // Check if all tasks are in a terminal state and no handles remain
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

        // Reconcile or use single output
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
    // Strip markdown fences
    let s = s.trim();
    let s = if let Some(rest) = s.strip_prefix("```") {
        rest.split_once('\n').map(|(_, body)| body).unwrap_or(rest)
            .trim_end_matches("```")
            .trim()
    } else {
        s
    };

    // Find start of JSON array
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
```

- [ ] **Step 2: Compile check**

```powershell
cargo check -p kitsune-agent 2>&1
```

- [ ] **Step 3: Commit**

```powershell
git add crates/kitsune-agent/src/swarm/coordinator.rs
git commit -m "feat(agent): implement SwarmCoordinator with LLM planning and dependency resolution"
```

---

## Task 9: Implement `SwarmWorker`

**Files:**
- Modify: `crates/kitsune-agent/src/swarm/worker.rs`

- [ ] **Step 1: Write the full `worker.rs`**

```rust
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
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

        // Tag the task with this worker's ID
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

        // Build the LlmAgentRuntime — inherits the parent AgentSpec constraints unchanged
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

        // Run under timeout
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
```

- [ ] **Step 2: Compile check**

```powershell
cargo check -p kitsune-agent 2>&1
```

- [ ] **Step 3: Commit**

```powershell
git add crates/kitsune-agent/src/swarm/worker.rs
git commit -m "feat(agent): implement SwarmWorker — runs LlmAgentRuntime with nav/hil locks"
```

---

## Task 10: Implement `reconcile`

**Files:**
- Modify: `crates/kitsune-agent/src/swarm/reconciler.rs`

- [ ] **Step 1: Write the full `reconciler.rs`**

```rust
use crate::ai_client::{AgentAiClient, ModelTier};
use crate::error::AgentError;
use crate::swarm::types::{SwarmMode, WorkerRole};

pub async fn reconcile(
    inputs: Vec<(WorkerRole, String)>,
    goal: String,
    mode: SwarmMode,
    ai_client: &AgentAiClient,
) -> Result<String, AgentError> {
    if inputs.is_empty() {
        return Ok(String::new());
    }
    if inputs.len() == 1 {
        return Ok(inputs.into_iter().next().unwrap().1);
    }

    let n = inputs.len();
    let mode_instruction = match mode {
        SwarmMode::DiscoveryAtScale => format!(
            "You are a senior analyst. {} researchers each gathered information on a different \
             slice of the topic: '{}'.\n\
             Instructions:\n\
             1. Deduplicate overlapping findings.\n\
             2. Merge into one unified, comprehensive list.\n\
             3. Preserve unique findings from each agent.\n\
             4. Cite which agent found each key point.",
            n, goal
        ),
        SwarmMode::OutputAtScale => format!(
            "You are a senior editor. {} writers each produced a different section of a \
             document on: '{}'.\n\
             Instructions:\n\
             1. Assemble sections in logical order.\n\
             2. Smooth transitions between sections.\n\
             3. Preserve all citations and data points.\n\
             4. Produce a coherent, publication-ready document.",
            n, goal
        ),
        SwarmMode::PerspectiveAtScale => format!(
            "You are a senior analyst reviewing {} expert perspectives on: '{}'.\n\n\
             REQUIRED — address each of these four points explicitly:\n\
             (a) Points of AGREEMENT across all agents.\n\
             (b) Points of DISAGREEMENT — do NOT paper over them; name them.\n\
             (c) Evaluate which position is better supported by evidence and why.\n\
             (d) Final balanced conclusion that acknowledges dissenting views.",
            n, goal
        ),
    };

    let mut prompt = mode_instruction;
    for (role, output) in &inputs {
        prompt.push_str(&format!("\n\n=== {} ===\n{}", role.as_str(), output));
    }

    ai_client
        .complete(&prompt, ModelTier::Orchestrator)
        .await
        .map_err(|e| AgentError::SwarmCoordinatorFailed(format!("Reconciliation failed: {e}")))
}
```

- [ ] **Step 2: Compile check**

```powershell
cargo check --workspace 2>&1
```

- [ ] **Step 3: Full release build**

```powershell
cargo build --release -p kitsune-ui 2>&1
```

- [ ] **Step 4: Commit**

```powershell
git add crates/kitsune-agent/src/swarm/reconciler.rs
git commit -m "feat(agent): implement reconcile — LLM synthesis of multi-worker outputs"
```

---

## Task 11: Add tests

**Files:**
- Modify: `crates/kitsune-agent/src/swarm/mod.rs` (add test module)

- [ ] **Step 1: Add test module to `swarm/mod.rs`**

At the bottom of `swarm/mod.rs`, add:

```rust
#[cfg(test)]
mod tests {
    use super::types::*;
    use crate::error::AgentError;

    // ── Task dependency resolution ───────────────────────────────────────────

    #[test]
    fn test_task_dependency_resolution_ready_when_deps_met() {
        let tasks = vec![
            SwarmTask {
                id: "a".into(), role: WorkerRole::Researcher,
                prompt: "Find sources".into(), depends_on: vec![],
                status: TaskStatus::Completed("done".into()),
                worker_id: None, tool_calls_used: 0, last_message: None,
            },
            SwarmTask {
                id: "b".into(), role: WorkerRole::Analyst,
                prompt: "Analyze".into(), depends_on: vec!["a".into()],
                status: TaskStatus::Pending,
                worker_id: None, tool_calls_used: 0, last_message: None,
            },
            SwarmTask {
                id: "c".into(), role: WorkerRole::Skeptic,
                prompt: "Challenge".into(), depends_on: vec!["a".into()],
                status: TaskStatus::Pending,
                worker_id: None, tool_calls_used: 0, last_message: None,
            },
        ];

        let completed_ids: std::collections::HashSet<&str> = tasks.iter()
            .filter(|t| matches!(t.status, TaskStatus::Completed(_)))
            .map(|t| t.id.as_str())
            .collect();

        let ready: Vec<&str> = tasks.iter()
            .filter(|t| {
                t.status == TaskStatus::Pending
                    && t.depends_on.iter().all(|dep| completed_ids.contains(dep.as_str()))
            })
            .map(|t| t.id.as_str())
            .collect();

        assert_eq!(ready.len(), 2, "b and c should be ready once a completes");
        assert!(ready.contains(&"b"));
        assert!(ready.contains(&"c"));
    }

    #[test]
    fn test_task_dependency_not_ready_when_dep_pending() {
        let tasks = vec![
            SwarmTask {
                id: "a".into(), role: WorkerRole::Researcher,
                prompt: "Find".into(), depends_on: vec![],
                status: TaskStatus::Pending,  // still pending
                worker_id: None, tool_calls_used: 0, last_message: None,
            },
            SwarmTask {
                id: "b".into(), role: WorkerRole::Analyst,
                prompt: "Analyze".into(), depends_on: vec!["a".into()],
                status: TaskStatus::Pending,
                worker_id: None, tool_calls_used: 0, last_message: None,
            },
        ];

        let completed_ids: std::collections::HashSet<&str> = tasks.iter()
            .filter(|t| matches!(t.status, TaskStatus::Completed(_)))
            .map(|t| t.id.as_str())
            .collect();

        let ready: Vec<&str> = tasks.iter()
            .filter(|t| {
                t.status == TaskStatus::Pending
                    && t.depends_on.iter().all(|dep| completed_ids.contains(dep.as_str()))
            })
            .map(|t| t.id.as_str())
            .collect();

        // Only "a" is ready (no deps). "b" must wait.
        assert_eq!(ready, vec!["a"]);
    }

    // ── Coordinator plan parsing ─────────────────────────────────────────────

    #[test]
    fn test_parse_plan_valid_json() {
        let json = r#"[
            {"id":"t1","role":"Researcher","prompt":"Find sources on AI safety","depends_on":[]},
            {"id":"t2","role":"Analyst","prompt":"Analyze findings","depends_on":["t1"]},
            {"id":"t3","role":"Skeptic","prompt":"Challenge conclusions","depends_on":["t1"]}
        ]"#;
        let tasks = parse_plan_for_test(json).unwrap();
        assert_eq!(tasks.len(), 3);
        assert_eq!(tasks[0].id, "t1");
        assert_eq!(tasks[1].depends_on, vec!["t1"]);
        assert!(matches!(tasks[2].role, WorkerRole::Skeptic));
    }

    #[test]
    fn test_parse_plan_strips_markdown_fences() {
        let json = "```json\n[{\"id\":\"t1\",\"role\":\"Researcher\",\"prompt\":\"Find info\",\"depends_on\":[]}]\n```";
        let tasks = parse_plan_for_test(json).unwrap();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].id, "t1");
    }

    #[test]
    fn test_parse_plan_preamble_before_array() {
        let json = "Here is the plan:\n[{\"id\":\"t1\",\"role\":\"Writer\",\"prompt\":\"Write report\",\"depends_on\":[]}]";
        let tasks = parse_plan_for_test(json).unwrap();
        assert_eq!(tasks.len(), 1);
    }

    #[test]
    fn test_parse_plan_empty_returns_empty_vec() {
        let json = "[]";
        let tasks = parse_plan_for_test(json).unwrap();
        assert!(tasks.is_empty());
    }

    #[test]
    fn test_parse_plan_invalid_json_returns_error() {
        let result = parse_plan_for_test("not json at all");
        assert!(result.is_err());
    }

    // ── Worker role parsing ──────────────────────────────────────────────────

    #[test]
    fn test_worker_role_from_label_known_roles() {
        assert!(matches!(WorkerRole::from_label("Researcher"), WorkerRole::Researcher));
        assert!(matches!(WorkerRole::from_label("Skeptic"), WorkerRole::Skeptic));
        assert!(matches!(WorkerRole::from_label("FactChecker"), WorkerRole::FactChecker));
        assert!(matches!(WorkerRole::from_label("Writer"), WorkerRole::Writer));
    }

    #[test]
    fn test_worker_role_custom_prefix() {
        let role = WorkerRole::from_label("Custom:DomainExpert");
        match &role {
            WorkerRole::Custom(s) => assert_eq!(s, "DomainExpert"),
            _ => panic!("Expected Custom variant"),
        }
    }

    #[test]
    fn test_worker_role_unknown_becomes_custom() {
        let role = WorkerRole::from_label("RandomRole");
        assert!(matches!(role, WorkerRole::Custom(_)));
    }

    // ── AgentConstraints inheritance invariant ───────────────────────────────

    #[test]
    fn test_agent_constraints_defaults_deny_sensitive_ops() {
        use crate::spec::AgentConstraints;
        let constraints = AgentConstraints::default();
        assert!(!constraints.can_initiate_payments, "payments must default false");
        assert!(!constraints.can_create_accounts, "account creation must default false");
        assert!(!constraints.can_send_communications, "communications must default false");
        assert_eq!(constraints.hil_required_for, vec!["all"]);
    }

    #[test]
    fn test_swarm_config_default_values() {
        let cfg = SwarmConfig::default();
        assert_eq!(cfg.max_workers, 10);
        assert!(cfg.enable_reconciliation);
        assert!(cfg.enable_disagreement);
        assert_eq!(cfg.worker_timeout_seconds, 120);
        assert_eq!(cfg.nav_lock_timeout_seconds, 30);
    }

    #[test]
    fn test_task_status_emoji_and_label() {
        assert_eq!(TaskStatus::Pending.emoji(), "🟡");
        assert_eq!(TaskStatus::Running.emoji(), "🔵");
        assert_eq!(TaskStatus::Completed("x".into()).emoji(), "✅");
        assert_eq!(TaskStatus::Failed("x".into()).emoji(), "🔴");
        assert_eq!(TaskStatus::Cancelled.emoji(), "⬛");

        assert_eq!(TaskStatus::Pending.label(), "Pending");
        assert_eq!(TaskStatus::Running.label(), "Running");
    }

    // ── SwarmState counts ───────────────────────────────────────────────────

    #[test]
    fn test_swarm_state_counts() {
        let state = SwarmState {
            swarm_id: "s1".into(),
            goal: "test goal".into(),
            config: SwarmConfig::default(),
            tasks: vec![
                make_task("t1", TaskStatus::Pending),
                make_task("t2", TaskStatus::Running),
                make_task("t3", TaskStatus::Completed("output".into())),
                make_task("t4", TaskStatus::Failed("err".into())),
            ],
            final_answer: None,
            total_tool_calls: 0,
            started_at: std::time::Instant::now(),
        };
        assert_eq!(state.pending_count(), 1);
        assert_eq!(state.active_count(), 1);
        assert_eq!(state.completed_count(), 1);
    }

    // ── Helpers ─────────────────────────────────────────────────────────────

    fn make_task(id: &str, status: TaskStatus) -> SwarmTask {
        SwarmTask {
            id: id.into(),
            role: WorkerRole::Researcher,
            prompt: "test".into(),
            depends_on: vec![],
            status,
            worker_id: None,
            tool_calls_used: 0,
            last_message: None,
        }
    }

    // Duplicate parse_plan here so tests don't need coordinator as a dep
    fn parse_plan_for_test(s: &str) -> Result<Vec<SwarmTask>, AgentError> {
        let s = s.trim();
        let s = if let Some(rest) = s.strip_prefix("```") {
            rest.split_once('\n').map(|(_, b)| b).unwrap_or(rest)
                .trim_end_matches("```")
                .trim()
        } else { s };
        let start = s.find('[').ok_or_else(|| {
            AgentError::SwarmCoordinatorFailed("No array".into())
        })?;
        let s = &s[start..];
        #[derive(serde::Deserialize)]
        struct R { id: String, role: String, prompt: String, #[serde(default)] depends_on: Vec<String> }
        let raw: Vec<R> = serde_json::from_str(s)
            .map_err(|e| AgentError::SwarmCoordinatorFailed(e.to_string()))?;
        Ok(raw.into_iter().map(|r| SwarmTask {
            id: r.id, role: WorkerRole::from_label(&r.role), prompt: r.prompt,
            depends_on: r.depends_on, status: TaskStatus::Pending,
            worker_id: None, tool_calls_used: 0, last_message: None,
        }).collect())
    }
}
```

- [ ] **Step 2: Run tests**

```powershell
cargo test -p kitsune-agent 2>&1
```

Expected: all tests pass. Fix any failures before continuing.

- [ ] **Step 3: Run full workspace tests**

```powershell
cargo test --workspace 2>&1
```

Fix any regressions.

- [ ] **Step 4: Commit**

```powershell
git add crates/kitsune-agent/src/swarm/mod.rs
git commit -m "test(agent): add swarm unit tests — dependency resolution, parsing, invariants"
```

---

## Task 12: Final verification and cleanup

- [ ] **Step 1: Architecture invariant grep checks**

```powershell
# HilApproval must not have Clone in swarm code
grep -r "HilApproval" crates/kitsune-agent/src/swarm/

# No raw vault data in swarm
grep -rn "decrypt\|raw_secret\|plaintext" crates/kitsune-agent/src/swarm/

# Semaphore exists in coordinator
grep -n "Semaphore" crates/kitsune-agent/src/swarm/coordinator.rs

# nav_lock acquired for Navigate/Click/Fill
grep -n "acquire_nav_lock" crates/kitsune-agent/src/loop_runtime.rs

# hil_lock acquired for checkpoint
grep -n "acquire_hil_lock" crates/kitsune-agent/src/loop_runtime.rs

# RoutingPolicy untouched
grep -n "always_local" crates/kitsune-ai/src/router.rs
```

- [ ] **Step 2: Full compilation check — zero warnings on new code**

```powershell
cargo check --workspace 2>&1
cargo build --release -p kitsune-ui 2>&1
cargo build --release -p kitsune-cloud-mock 2>&1
```

- [ ] **Step 3: All tests pass**

```powershell
cargo test --workspace 2>&1
```

- [ ] **Step 4: Verify no new external crates added**

```powershell
# All new code uses workspace deps already present: tokio, uuid, serde_json, reqwest
# No new lines in any Cargo.toml
git diff HEAD -- "*/Cargo.toml"
```

Expected: no Cargo.toml changes (all dependencies already in workspace).

- [ ] **Step 5: Final commit**

```powershell
git add -A
git commit -m "feat: agent swarm — coordinator, parallel workers, reconciler, live UI, mock endpoint

- SwarmCoordinator: one planning LLM call, dependency-aware dispatch, semaphore cap
- SwarmWorker: inherits AgentConstraints, nav_lock/hil_lock injected
- reconcile(): mode-aware synthesis (Discovery/Output/Perspective)
- egui: swarm toggle, config bar, preset cards, live task graph
- cloud-mock: /api/swarm-plan SSE demo endpoint
- 12 unit tests covering parsing, deps, invariants, status counts"
```

---

## Self-Review: Spec Coverage

| Requirement | Task |
|-------------|------|
| SwarmConfig, SwarmMode, WorkerRole, TaskStatus, SwarmTask, SwarmState types | Task 1 |
| SwarmCoordinatorFailed, SwarmWorkerFailed, Cancelled errors | Task 1 |
| AgentEvent::Swarm* variants | Task 2 |
| AgentSseAction::Swarm* variants + process_agent_events handlers | Tasks 2, 4 |
| nav_lock, hil_lock, worker_id, swarm_id on LlmAgentRuntime | Task 3 |
| Builder methods: with_nav_lock, with_hil_lock, with_worker_id | Task 3 |
| execute_action: nav lock for Navigate/Click/Fill | Task 3 |
| execute_action: hil lock for sensitive Fill checkpoint | Task 3 |
| Stop returns Cancelled for swarm workers | Task 3 |
| emit() prefixes log messages with worker_id | Task 3 |
| KitsuneBrowser: swarm_mode, swarm_config, swarm_state | Task 4 |
| SwarmPlanReady initializes swarm_state | Task 4 |
| SwarmUpdate updates task status live | Task 4 |
| SwarmDone sets final_answer, idle state | Task 4 |
| task_graph_panel replaced with SwarmState rendering | Task 4 |
| session_panel.rs call site updated | Task 4 |
| task_nodes removed (was stub) | Task 4 |
| Swarm toggle button | Task 5 |
| Config bar (workers, mode, disagree) | Task 5 |
| Three preset cards (Discovery, Report, Expert Panel) | Task 5 |
| Swarm status bar (active/done/pending counts) | Task 5 |
| run_in_process_agent pump refactored to Option<AgentSseAction> | Task 6 |
| run_swarm async function | Task 6 |
| start_agent_run swarm branch + early return | Task 6 |
| /api/swarm-plan SSE demo endpoint | Task 7 |
| SwarmCoordinator: planning LLM call, retry, parse_plan | Task 8 |
| SwarmCoordinator: dependency resolution loop, semaphore, stop flag | Task 8 |
| SwarmCoordinator: reconciliation path | Task 8 |
| SwarmCoordinator: all error cases (empty plan, all failed) | Task 8 |
| SwarmWorker: inherits spec, builds LlmAgentRuntime | Task 9 |
| SwarmWorker: timeout, cancelled, failed, completed paths | Task 9 |
| reconcile(): all three modes | Task 10 |
| Tests: dep resolution, plan parsing, role parsing, invariants, state counts | Task 11 |
| Architecture invariant grep checks | Task 12 |

**No gaps found.**
