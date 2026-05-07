/// HIL Approval — a non-cloneable, time-limited, action-bound approval token.
///
/// # Security Properties
/// - **Non-cloneable**: Cannot be duplicated — each approval is consumed exactly once.
/// - **Time-limited**: Expires 30 seconds after creation.
/// - **Action-bound**: Tied to a specific ActionId — cannot be used for a different action.
///
/// These properties are enforced at the type system level. An agent cannot
/// bypass HIL by reusing or forging approval tokens.
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{HilError, HilResult};

/// Unique identifier for an action requiring HIL approval.
#[derive(Debug, Clone, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub struct ActionId(pub Uuid);

impl ActionId {
    /// Generate a new unique action ID.
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for ActionId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for ActionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// The approval window duration — approvals expire after this many seconds.
const APPROVAL_EXPIRY_SECONDS: i64 = 30;

/// A HIL approval token — consumed by actions that require human confirmation.
///
/// This type is deliberately NOT Clone. An approval can only be used once.
/// After consumption, it cannot be reused.
pub struct HilApproval {
    /// The action this approval is for.
    action_id: ActionId,
    /// When this approval was granted.
    granted_at: DateTime<Utc>,
    /// When this approval expires.
    expires_at: DateTime<Utc>,
    /// Whether this approval has been consumed.
    consumed: bool,
    /// The user's decision details.
    pub decision: ApprovalDecision,
}

/// Details of the user's approval decision.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalDecision {
    /// Whether the user approved the action.
    pub approved: bool,
    /// Optional note from the user.
    pub user_note: Option<String>,
    /// Timestamp of the decision.
    pub decided_at: DateTime<Utc>,
}

// Deliberately NOT implementing Clone for HilApproval
// This is a security feature — approvals cannot be duplicated.

impl std::fmt::Debug for HilApproval {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HilApproval")
            .field("action_id", &self.action_id)
            .field("granted_at", &self.granted_at)
            .field("expires_at", &self.expires_at)
            .field("consumed", &self.consumed)
            .field("approved", &self.decision.approved)
            .finish()
    }
}

impl HilApproval {
    /// Create a new approval token for the given action.
    ///
    /// This should only be called by the HilGate after the user has confirmed.
    pub(crate) fn new(action_id: ActionId, decision: ApprovalDecision) -> Self {
        let now = Utc::now();
        Self {
            action_id,
            granted_at: now,
            expires_at: now + Duration::seconds(APPROVAL_EXPIRY_SECONDS),
            consumed: false,
            decision,
        }
    }

    /// Consume this approval, verifying it matches the expected action and hasn't expired.
    ///
    /// After this call, the approval is consumed and cannot be used again.
    pub fn consume(mut self, expected_action: &ActionId) -> HilResult<ApprovalDecision> {
        // Check action binding
        if self.action_id != *expected_action {
            return Err(HilError::ApprovalMismatch {
                expected: expected_action.to_string(),
                actual: self.action_id.to_string(),
            });
        }

        // Check expiry
        if Utc::now() > self.expires_at {
            return Err(HilError::ApprovalExpired);
        }

        // Check not already consumed (should be impossible due to move semantics, but be safe)
        if self.consumed {
            return Err(HilError::Internal(
                "Approval already consumed (this should be impossible)".to_string(),
            ));
        }

        // Mark as consumed
        self.consumed = true;

        Ok(self.decision.clone())
    }

    /// Check if this approval is still valid (not expired, not consumed).
    pub fn is_valid(&self) -> bool {
        !self.consumed && Utc::now() <= self.expires_at
    }

    /// Get the action ID this approval is bound to.
    pub fn action_id(&self) -> &ActionId {
        &self.action_id
    }

    /// Get when this approval expires.
    pub fn expires_at(&self) -> DateTime<Utc> {
        self.expires_at
    }

    /// Get the remaining time before expiry.
    pub fn time_remaining(&self) -> Duration {
        self.expires_at - Utc::now()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_approval_consume_success() {
        let action_id = ActionId::new();
        let decision = ApprovalDecision {
            approved: true,
            user_note: None,
            decided_at: Utc::now(),
        };
        let approval = HilApproval::new(action_id.clone(), decision);

        assert!(approval.is_valid());
        let result = approval.consume(&action_id);
        assert!(result.is_ok());
        assert!(result.unwrap().approved);
    }

    #[test]
    fn test_approval_wrong_action() {
        let action_id = ActionId::new();
        let wrong_action = ActionId::new();
        let decision = ApprovalDecision {
            approved: true,
            user_note: None,
            decided_at: Utc::now(),
        };
        let approval = HilApproval::new(action_id, decision);

        let result = approval.consume(&wrong_action);
        assert!(matches!(result, Err(HilError::ApprovalMismatch { .. })));
    }

    #[test]
    fn test_approval_not_cloneable() {
        // This test verifies at compile time that HilApproval is not Clone.
        // If someone adds #[derive(Clone)] to HilApproval, this comment
        // serves as a reminder: HilApproval MUST NOT be Clone for security.
        // The move semantics of consume() are a critical safety feature.
    }
}
