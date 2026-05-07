/// HIL Gate — the checkpoint through which all consequential actions must pass.
///
/// The gate translates low-level agent actions into plain-language confirmations,
/// presents them to the user, and produces approval tokens that are consumed
/// by the action executor.
use crate::approval::{ActionId, ApprovalDecision, HilApproval};
use crate::error::{HilError, HilResult};
use crate::presentation::HilPresentation;
use crate::trigger::HilTriggerClass;

use chrono::Utc;
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot};
use tracing::{info, warn};
use uuid::Uuid;

/// A pending HIL checkpoint waiting for user decision.
#[derive(Debug)]
pub struct HilCheckpoint {
    /// Unique ID for this checkpoint.
    pub checkpoint_id: Uuid,
    /// The action ID that will be approved/rejected.
    pub action_id: ActionId,
    /// The trigger class for this action.
    pub trigger_class: HilTriggerClass,
    /// The presentation to show the user.
    pub presentation: HilPresentation,
    /// Channel to send the user's decision back.
    response_tx: oneshot::Sender<ApprovalDecision>,
}

/// The HIL gate — produces approval tokens after user confirmation.
pub struct HilGate {
    /// Channel for sending checkpoints to the UI layer.
    checkpoint_tx: mpsc::Sender<HilCheckpoint>,
    /// Audit log of all HIL decisions.
    audit_log: Arc<parking_lot::RwLock<Vec<HilAuditEntry>>>,
    /// Whether the gate is in test mode (auto-approve all checkpoints).
    test_mode: bool,
}

/// An entry in the HIL audit log.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct HilAuditEntry {
    /// Timestamp of the decision.
    pub timestamp: chrono::DateTime<Utc>,
    /// The action ID.
    pub action_id: String,
    /// Plain-language summary of what was being confirmed.
    pub action_summary: String,
    /// Whether the user approved the action.
    pub approved: bool,
    /// How long the user took to decide (in milliseconds).
    pub decision_time_ms: u64,
    /// Optional user note.
    pub user_note: Option<String>,
}

impl HilGate {
    /// Create a new HIL gate.
    ///
    /// Returns the gate and a receiver for the UI layer to listen for checkpoints.
    pub fn new(buffer_size: usize) -> (Self, mpsc::Receiver<HilCheckpoint>) {
        let (checkpoint_tx, checkpoint_rx) = mpsc::channel(buffer_size);

        let gate = Self {
            checkpoint_tx,
            audit_log: Arc::new(parking_lot::RwLock::new(Vec::new())),
            test_mode: false,
        };

        (gate, checkpoint_rx)
    }

    /// Create a new HIL gate in test mode (auto-approves all checkpoints).
    pub fn new_test_gate() -> Self {
        let (checkpoint_tx, _) = mpsc::channel(1);
        Self {
            checkpoint_tx,
            audit_log: Arc::new(parking_lot::RwLock::new(Vec::new())),
            test_mode: true,
        }
    }

    /// Submit an action for HIL confirmation.
    ///
    /// This blocks until the user approves or rejects the action.
    /// Returns a non-cloneable, time-limited approval token on success.
    pub async fn checkpoint(
        &self,
        trigger_class: HilTriggerClass,
        data_labels: Vec<String>,
    ) -> HilResult<HilApproval> {
        if self.test_mode {
            let action_id = ActionId::new();
            let decision = ApprovalDecision {
                approved: true,
                user_note: Some("Auto-approved in test mode".to_string()),
                decided_at: Utc::now(),
            };
            return Ok(HilApproval::new(action_id, decision));
        }

        let action_id = ActionId::new();
        let checkpoint_id = Uuid::new_v4();
        let checkpoint_time = Utc::now();

        // Build the presentation for the UI
        let presentation = HilPresentation::from_trigger(&trigger_class, &data_labels);

        info!(
            checkpoint_id = %checkpoint_id,
            action_id = %action_id,
            trigger = ?std::mem::discriminant(&trigger_class),
            "HIL checkpoint initiated"
        );

        // Create response channel
        let (response_tx, response_rx) = oneshot::channel();

        // Send checkpoint to UI
        let checkpoint = HilCheckpoint {
            checkpoint_id,
            action_id: action_id.clone(),
            trigger_class: trigger_class.clone(),
            presentation,
            response_tx,
        };

        self.checkpoint_tx
            .send(checkpoint)
            .await
            .map_err(|_| HilError::Internal("HIL UI channel closed".to_string()))?;

        // Wait for user decision
        let decision = response_rx.await.map_err(|_| HilError::Dismissed)?;

        let decision_time = Utc::now() - checkpoint_time;

        // Log the decision
        let audit_entry = HilAuditEntry {
            timestamp: Utc::now(),
            action_id: action_id.to_string(),
            action_summary: trigger_class.plain_language_summary(),
            approved: decision.approved,
            decision_time_ms: decision_time.num_milliseconds() as u64,
            user_note: decision.user_note.clone(),
        };
        self.audit_log.write().push(audit_entry);

        if decision.approved {
            info!(action_id = %action_id, "HIL checkpoint approved by user");
            Ok(HilApproval::new(action_id, decision))
        } else {
            let reason = decision
                .user_note
                .clone()
                .unwrap_or_else(|| "No reason provided".to_string());
            warn!(action_id = %action_id, reason = %reason, "HIL checkpoint rejected by user");
            Err(HilError::UserRejected { reason })
        }
    }

    /// Get the audit log of all HIL decisions.
    pub fn audit_log(&self) -> Vec<HilAuditEntry> {
        self.audit_log.read().clone()
    }

    /// Clear the audit log.
    pub fn clear_audit_log(&self) {
        self.audit_log.write().clear();
    }
}

/// Respond to a HIL checkpoint with the user's decision.
///
/// This is called by the UI layer after the user interacts with the confirmation dialog.
pub fn respond_to_checkpoint(checkpoint: HilCheckpoint, approved: bool, note: Option<String>) {
    let decision = ApprovalDecision {
        approved,
        user_note: note,
        decided_at: Utc::now(),
    };

    // If the receiver has been dropped, the checkpoint was cancelled
    let _ = checkpoint.response_tx.send(decision);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trigger::{HilTriggerClass, Money};

    #[tokio::test]
    async fn test_hil_gate_approval_flow() {
        let (gate, mut rx) = HilGate::new(16);

        // Spawn a task that will approve the checkpoint
        let approve_task = tokio::spawn(async move {
            let checkpoint = rx.recv().await.expect("Should receive checkpoint");
            assert!(!checkpoint.presentation.what_will_happen.is_empty());
            respond_to_checkpoint(checkpoint, true, Some("Looks good".to_string()));
        });

        // Submit a checkpoint
        let trigger = HilTriggerClass::FinancialFormSubmission {
            institution: "Test Bank".to_string(),
            amount: Some(Money::new(1234, "USD")),
            fields_involved: vec![],
        };

        let result = gate.checkpoint(trigger, vec!["email".to_string()]).await;
        assert!(result.is_ok());

        let approval = result.unwrap();
        assert!(approval.is_valid());

        approve_task.await.unwrap();
    }

    #[tokio::test]
    async fn test_hil_gate_rejection_flow() {
        let (gate, mut rx) = HilGate::new(16);

        let reject_task = tokio::spawn(async move {
            let checkpoint = rx.recv().await.expect("Should receive checkpoint");
            respond_to_checkpoint(checkpoint, false, Some("Too risky".to_string()));
        });

        let trigger = HilTriggerClass::AccountCreation {
            service: "Suspicious Service".to_string(),
            implied_cost: None,
            terms_url: None,
        };

        let result = gate.checkpoint(trigger, vec![]).await;
        assert!(matches!(result, Err(HilError::UserRejected { .. })));

        reject_task.await.unwrap();
    }

    #[tokio::test]
    async fn test_hil_audit_log() {
        let (gate, mut rx) = HilGate::new(16);

        let task = tokio::spawn(async move {
            let checkpoint = rx.recv().await.unwrap();
            respond_to_checkpoint(checkpoint, true, None);
        });

        let trigger = HilTriggerClass::ExternalSideEffect {
            description: "Test action".to_string(),
            reversible: true,
        };

        let _ = gate.checkpoint(trigger, vec![]).await;
        task.await.unwrap();

        let log = gate.audit_log();
        assert_eq!(log.len(), 1);
        assert!(log[0].approved);
    }
}
