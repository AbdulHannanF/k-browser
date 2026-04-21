//! AI request router — decides cloud vs local per request.
//!
//! The central safety rule: `TaskType::VaultDecision` and
//! `TaskType::SensitiveForm` NEVER go to cloud, enforced here at the
//! type level — not via configuration or a flag.

use async_trait::async_trait;
use std::sync::Arc;
use tracing::{debug, info, warn};

use crate::cloud::KitsuneCloudBackend;
use crate::error::{AiError, AiResult};
use crate::local::LocalAiBackend;
use crate::request::{AiRequest, AiResponse, TaskType};
use crate::{AiBackend, BackendType};
use kitsune_agent::BudgetTracker;

// ─── RoutingPolicy ────────────────────────────────────────────────────────────

/// Controls which backend handles which task types.
#[derive(Debug, Clone)]
pub struct RoutingPolicy {
    /// Task types to prefer local for when local is available.
    pub local_preferred: Vec<TaskType>,
    /// Task types that MUST use local (security invariant — not configurable).
    /// These types can never go to cloud regardless of any setting.
    always_local: Vec<TaskType>,
    /// Milliseconds before local is considered timed out; falls back to cloud.
    pub local_timeout_ms: u64,
    /// If cloud returns QuotaExhausted and local is available, use local.
    pub fallback_to_local_on_quota: bool,
}

impl Default for RoutingPolicy {
    fn default() -> Self {
        Self {
            local_preferred: vec![
                TaskType::FormFill,
                TaskType::PageSummary,
                TaskType::RoutineRepeat,
            ],
            // INVARIANT: These two NEVER go to cloud.
            // Hardcoded, not user-configurable.
            always_local: vec![TaskType::VaultDecision, TaskType::SensitiveForm],
            local_timeout_ms: 3_000,
            fallback_to_local_on_quota: true,
        }
    }
}

impl RoutingPolicy {
    /// Returns true if this task type must stay local (security invariant).
    pub fn is_always_local(&self, task: TaskType) -> bool {
        self.always_local.contains(&task)
    }

    /// Returns true if this task type prefers local when local is available.
    pub fn prefers_local(&self, task: TaskType) -> bool {
        self.local_preferred.contains(&task)
    }
}

// ─── AiRouter ─────────────────────────────────────────────────────────────────

/// Routes AI requests to the correct backend based on task type and availability.
///
/// Routing rules (in priority order):
/// 1. `VaultDecision` / `SensitiveForm` → local ONLY, error if unavailable.
/// 2. `local_preferred` tasks + local available → local first, cloud fallback on timeout.
/// 3. Everything else → cloud.
/// 4. Cloud `QuotaExhausted` + local available → local fallback + notify user.
/// 5. Cloud `QuotaExhausted` + no local → `Err(QuotaExhausted)` → UI upgrade prompt.
pub struct AiRouter {
    cloud: Arc<KitsuneCloudBackend>,
    local: Option<Arc<LocalAiBackend>>,
    policy: RoutingPolicy,
}

impl AiRouter {
    /// Create a new router with only cloud backend (free tier / no local model).
    pub fn cloud_only(cloud: Arc<KitsuneCloudBackend>) -> Self {
        Self {
            cloud,
            local: None,
            policy: RoutingPolicy::default(),
        }
    }

    /// Create a router with both cloud and local backends (Pro tier).
    pub fn with_local(cloud: Arc<KitsuneCloudBackend>, local: Arc<LocalAiBackend>) -> Self {
        Self {
            cloud,
            local: Some(local),
            policy: RoutingPolicy::default(),
        }
    }

    /// Override the routing policy.
    pub fn with_policy(mut self, policy: RoutingPolicy) -> Self {
        self.policy = policy;
        self
    }

    /// Route a request to the correct backend.
    pub async fn route(
        &self,
        request: AiRequest,
        budget: &mut BudgetTracker,
    ) -> AiResult<AiResponse> {
        let task = request.task_type;

        // ── Rule 1: always_local tasks ────────────────────────────────────────
        if self.policy.is_always_local(task) {
            debug!(
                task = task.description(),
                "Task requires local backend (security invariant)"
            );
            return match &self.local {
                Some(local) if local.is_available() => {
                    local.complete(request, budget).await
                }
                _ => {
                    warn!(
                        task = task.description(),
                        "Task requires local but local is unavailable"
                    );
                    Err(AiError::RequiresLocal { task })
                }
            };
        }

        // ── Rule 2: local_preferred tasks ────────────────────────────────────
        if self.policy.prefers_local(task) {
            if let Some(local) = &self.local {
                if local.is_available() {
                    debug!(task = task.description(), "Trying local backend (preferred)");
                    match local.complete(request.clone(), budget).await {
                        Ok(resp) => {
                            info!(task = task.description(), latency_ms = resp.latency_ms, "Local backend succeeded");
                            return Ok(resp);
                        }
                        Err(AiError::LocalTimeout { ms }) => {
                            warn!(
                                task = task.description(),
                                ms, "Local timed out, falling back to cloud"
                            );
                            // fall through to cloud
                        }
                        Err(e) => {
                            warn!(task = task.description(), error = %e, "Local error, falling back to cloud");
                            // fall through to cloud
                        }
                    }
                }
            }
        }

        // ── Rule 3 / 4 / 5: cloud ────────────────────────────────────────────
        debug!(task = task.description(), "Routing to cloud backend");
        match self.cloud.complete(request.clone(), budget).await {
            Ok(resp) => Ok(resp),
            Err(AiError::QuotaExhausted { .. }) => {
                warn!("Cloud quota exhausted");
                // Rule 4: quota fallback to local
                if self.policy.fallback_to_local_on_quota {
                    if let Some(local) = &self.local {
                        if local.is_available() {
                            info!("Falling back to local due to quota exhaustion");
                            return local.complete(request, budget).await;
                        }
                    }
                }
                // Rule 5: no local available — surface to UI
                Err(AiError::QuotaExhausted {
                    actions_used: 100,
                    limit: 100,
                    resets_at: "next month".to_string(),
                })
            }
            Err(e) => Err(e),
        }
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vault_decision_is_always_local() {
        let policy = RoutingPolicy::default();
        assert!(policy.is_always_local(TaskType::VaultDecision));
        assert!(policy.is_always_local(TaskType::SensitiveForm));
    }

    #[test]
    fn test_other_tasks_not_always_local() {
        let policy = RoutingPolicy::default();
        assert!(!policy.is_always_local(TaskType::WebResearch));
        assert!(!policy.is_always_local(TaskType::ComplexReasoning));
        assert!(!policy.is_always_local(TaskType::FormFill));
    }

    #[test]
    fn test_local_preferred_tasks() {
        let policy = RoutingPolicy::default();
        assert!(policy.prefers_local(TaskType::FormFill));
        assert!(policy.prefers_local(TaskType::PageSummary));
        assert!(policy.prefers_local(TaskType::RoutineRepeat));
        assert!(!policy.prefers_local(TaskType::WebResearch));
    }
}
