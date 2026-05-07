/// Agent budget tracking — mandatory for all external interactions.
///
/// INVARIANT: If budget is exceeded, the agent MUST pause and escalate via HIL.
use crate::error::{AgentError, AgentResult};
use crate::spec::MoneyAmount;
use crate::tools::BudgetStatus;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

/// Tracks costs incurred by an agent session.
#[derive(Debug)]
pub struct BudgetTracker {
    /// Total spent this session (in minor units).
    total_spent_minor: RwLock<i64>,
    /// Currency.
    currency: String,
    /// Maximum per-session cost (minor units), or None if unlimited.
    max_session_cost: Option<i64>,
    /// Maximum per-action cost (minor units), or None if unlimited.
    max_action_cost: Option<i64>,
    /// Number of actions taken.
    actions_taken: RwLock<u32>,
    /// Maximum actions allowed.
    max_actions: u32,
}

impl BudgetTracker {
    /// Create a new budget tracker.
    pub fn new(
        max_session_cost: Option<MoneyAmount>,
        max_action_cost: Option<MoneyAmount>,
        max_actions: u32,
    ) -> Self {
        Self {
            total_spent_minor: RwLock::new(0),
            currency: max_session_cost
                .as_ref()
                .map(|m| m.currency.clone())
                .unwrap_or_else(|| "USD".to_string()),
            max_session_cost: max_session_cost.map(|m| m.amount_minor),
            max_action_cost: max_action_cost.map(|m| m.amount_minor),
            actions_taken: RwLock::new(0),
            max_actions,
        }
    }

    /// Log a cost incurred by an action.
    pub fn log_cost(&self, amount_minor: i64, description: &str) -> AgentResult<()> {
        // Check per-action limit
        if let Some(max) = self.max_action_cost {
            if amount_minor > max {
                warn!(
                    amount = amount_minor,
                    max = max,
                    description = %description,
                    "Per-action cost limit exceeded"
                );
                return Err(AgentError::BudgetExceeded {
                    spent: format!("{}", amount_minor),
                    limit: format!("{}", max),
                });
            }
        }

        // Add to total
        let mut total = self.total_spent_minor.write();
        *total += amount_minor;

        // Check session limit
        if let Some(max) = self.max_session_cost {
            if *total > max {
                warn!(total = *total, max = max, "Session cost limit exceeded");
                return Err(AgentError::BudgetExceeded {
                    spent: format!("{}", *total),
                    limit: format!("{}", max),
                });
            }
        }

        info!(
            amount = amount_minor,
            total = *total,
            description = %description,
            "Cost logged"
        );

        Ok(())
    }

    /// Increment the action counter.
    pub fn log_action(&self) -> AgentResult<()> {
        let mut count = self.actions_taken.write();
        *count += 1;

        if *count > self.max_actions {
            return Err(AgentError::ActionLimitReached {
                current: *count,
                max: self.max_actions,
            });
        }

        Ok(())
    }

    /// Get the current budget status.
    pub fn status(&self) -> BudgetStatus {
        let total = *self.total_spent_minor.read();
        let actions = *self.actions_taken.read();

        let remaining = self.max_session_cost.map(|max| MoneyAmount {
            amount_minor: (max - total).max(0),
            currency: self.currency.clone(),
        });

        BudgetStatus {
            total_spent: MoneyAmount {
                amount_minor: total,
                currency: self.currency.clone(),
            },
            remaining,
            actions_taken: actions,
            max_actions: self.max_actions,
            exceeded: self
                .max_session_cost
                .map(|max| total > max)
                .unwrap_or(false)
                || actions > self.max_actions,
        }
    }

    /// Check if the budget is still available.
    pub fn check_budget(&self) -> AgentResult<BudgetStatus> {
        let status = self.status();
        if status.exceeded {
            Err(AgentError::BudgetExceeded {
                spent: status.total_spent.display(),
                limit: status
                    .remaining
                    .map(|r| r.display())
                    .unwrap_or_else(|| "unlimited".to_string()),
            })
        } else {
            Ok(status)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_budget_within_limits() {
        let tracker = BudgetTracker::new(
            Some(MoneyAmount::usd(10.0)), // $10 session max
            Some(MoneyAmount::usd(2.0)),  // $2 per action max
            100,
        );

        assert!(tracker.log_cost(100, "API call 1").is_ok()); // $1.00
        assert!(tracker.log_cost(150, "API call 2").is_ok()); // $1.50
        assert!(tracker.log_action().is_ok());
    }

    #[test]
    fn test_budget_per_action_exceeded() {
        let tracker = BudgetTracker::new(
            Some(MoneyAmount::usd(10.0)),
            Some(MoneyAmount::usd(1.0)), // $1 per action max
            100,
        );

        let result = tracker.log_cost(200, "Expensive call"); // $2 > $1 limit
        assert!(matches!(result, Err(AgentError::BudgetExceeded { .. })));
    }

    #[test]
    fn test_budget_session_exceeded() {
        let tracker = BudgetTracker::new(
            Some(MoneyAmount::usd(1.0)), // $1 session max
            None,
            100,
        );

        assert!(tracker.log_cost(80, "Call 1").is_ok()); // $0.80
        let result = tracker.log_cost(50, "Call 2"); // $0.50 → total $1.30 > $1.00
        assert!(matches!(result, Err(AgentError::BudgetExceeded { .. })));
    }

    #[test]
    fn test_action_limit() {
        let tracker = BudgetTracker::new(None, None, 2);

        assert!(tracker.log_action().is_ok());
        assert!(tracker.log_action().is_ok());
        let result = tracker.log_action();
        assert!(matches!(result, Err(AgentError::ActionLimitReached { .. })));
    }
}
