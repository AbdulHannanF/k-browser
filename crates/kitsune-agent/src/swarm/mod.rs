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

#[cfg(test)]
mod tests {
    use super::types::*;
    use crate::error::AgentError;

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

        let mut ready: Vec<&str> = tasks.iter()
            .filter(|t| {
                t.status == TaskStatus::Pending
                    && t.depends_on.iter().all(|dep| completed_ids.contains(dep.as_str()))
            })
            .map(|t| t.id.as_str())
            .collect();
        ready.sort();

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
                status: TaskStatus::Pending,
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

        assert_eq!(ready, vec!["a"]);
    }

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
