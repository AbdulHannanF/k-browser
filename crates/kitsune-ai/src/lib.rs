//! KitsuneEngine AI Power Layer — `kitsune-ai`
//!
//! Provides the intelligence backend for KitsuneEngine agents. Two modes:
//!
//! * **KitsuneCloud** (default, free) — 100 actions/month via `api.kitsune.sh`.
//!   No API key needed. Budget tracked server-side; quota exhaustion surfaces
//!   an upgrade prompt in the UI, never a silent retry.
//!
//! * **LocalModel** (Pro only) — On-device Phi-3-mini via candle. Enabled with
//!   the `local-model` Cargo feature. Training data never leaves the device.
//!
//! **Key invariants (see also: the 6 KitsuneEngine invariants)**:
//! * `VaultDecision` and `SensitiveForm` task types **never** route to cloud.
//! * PII is scrubbed from every request before it leaves the device.
//! * User token stored in OS keychain only — never files, never env vars.
//! * Quota exhausted → agents pause → UI upgrade prompt — never silent retry.

pub mod cloud;
pub mod error;
pub mod local;
pub mod quota;
pub mod request;
pub mod router;
pub mod tuning;

pub use error::{AiError, AiResult};
pub use quota::{QuotaStatus, QuotaTracker};
pub use request::{AiRequest, AiResponse, TaskType};
pub use router::AiRouter;

use async_trait::async_trait;
use kitsune_agent::BudgetTracker;

// ─── BackendType ─────────────────────────────────────────────────────────────

/// Which underlying AI provider is servicing a request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum BackendType {
    /// Hosted KitsuneEngine cloud — freemium, primary.
    KitsuneCloud,
    /// On-device model — Pro only.
    LocalModel,
    /// Power-user brings their own API key (stored in vault, not here).
    UserApiKey,
}

// ─── UserTier ────────────────────────────────────────────────────────────────

/// The user's subscription tier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum UserTier {
    /// 100 actions/month, no local model.
    Free,
    /// Unlimited actions + local model + all templates.
    Pro,
    /// Self-hosted backend.
    Enterprise,
}

impl UserTier {
    /// Monthly action quota for this tier (`None` = unlimited).
    pub fn monthly_limit(&self) -> Option<u32> {
        match self {
            UserTier::Free => Some(100),
            UserTier::Pro | UserTier::Enterprise => None,
        }
    }

    /// Whether the local model feature is available.
    pub fn local_model_enabled(&self) -> bool {
        matches!(self, UserTier::Pro | UserTier::Enterprise)
    }
}

// ─── AiBackend trait ─────────────────────────────────────────────────────────

/// Common interface for all AI backends.
///
/// Implementors: [`cloud::KitsuneCloudBackend`], [`local::LocalAiBackend`].
#[async_trait]
pub trait AiBackend: Send + Sync {
    /// Run inference and return a structured response.
    ///
    /// Callers **must** call `budget.check_budget()` before and
    /// `budget.log_cost()` after — the backend never does this itself.
    async fn complete(
        &self,
        request: AiRequest,
        budget: &mut BudgetTracker,
    ) -> AiResult<AiResponse>;

    /// Which backend variant this is.
    fn backend_type(&self) -> BackendType;

    /// Whether this backend is currently usable (model loaded, auth valid, etc.).
    fn is_available(&self) -> bool;

    /// Estimated cost in USD per action (0.0 for free tier cloud).
    fn cost_per_action(&self) -> f64;
}
