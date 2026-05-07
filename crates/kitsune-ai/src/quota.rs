//! Quota tracking for the KitsuneCloud free tier.
//!
//! Free-tier users get 100 agent actions/month, tracked server-side and
//! cached locally at `data_dir()/kitsune/quota_cache.json`. The cache
//! survives process restarts. On quota exhaustion, agents pause and the
//! UI shows an upgrade prompt — we never silently retry.

use chrono::{DateTime, Datelike, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tracing::{debug, info, warn};

use crate::error::{AiError, AiResult};

// ─── QuotaStatus ─────────────────────────────────────────────────────────────

/// The result of a quota check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QuotaStatus {
    /// Actions are available.
    Available { remaining: u32 },
    /// Quota is exhausted for this month.
    Exhausted { resets_at: DateTime<Utc> },
    /// Pro / Enterprise — no limit.
    Unlimited,
}

impl QuotaStatus {
    /// Whether an action is permitted under this quota status.
    pub fn is_available(&self) -> bool {
        matches!(self, QuotaStatus::Available { .. } | QuotaStatus::Unlimited)
    }
}

// ─── QuotaTracker ─────────────────────────────────────────────────────────────

/// Tracks how many agent actions the user has consumed this month.
///
/// Persists to disk after every `consume()` call so it survives restarts.
/// Server-side is the authoritative count; local is a cache.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuotaTracker {
    /// How many actions have been used this period.
    used: u32,
    /// Monthly limit (`None` = unlimited / Pro tier).
    limit: Option<u32>,
    /// When the quota resets (1st of next month, UTC midnight).
    resets_at: DateTime<Utc>,
    /// When we last synced with the server.
    last_synced: DateTime<Utc>,
}

impl QuotaTracker {
    /// Create a new tracker for the free tier (100 actions/month).
    pub fn new_free() -> Self {
        Self {
            used: 0,
            limit: Some(100),
            resets_at: next_month_reset(),
            last_synced: Utc::now(),
        }
    }

    /// Create a new tracker for the Pro tier (unlimited).
    pub fn new_pro() -> Self {
        Self {
            used: 0,
            limit: None,
            resets_at: next_month_reset(),
            last_synced: Utc::now(),
        }
    }

    /// Load quota cache from disk, or create a fresh free-tier tracker.
    pub fn load_or_new() -> Self {
        let path = quota_cache_path();
        match std::fs::read_to_string(&path) {
            Ok(json) => match serde_json::from_str::<QuotaTracker>(&json) {
                Ok(mut tracker) => {
                    tracker.reset_if_due();
                    debug!(
                        "Loaded quota cache: {}/{:?} used",
                        tracker.used, tracker.limit
                    );
                    tracker
                }
                Err(e) => {
                    warn!("Quota cache parse error ({}), starting fresh", e);
                    Self::new_free()
                }
            },
            Err(_) => {
                debug!("No quota cache found, starting fresh");
                Self::new_free()
            }
        }
    }

    /// Check the current quota status.
    pub fn check(&self) -> QuotaStatus {
        match self.limit {
            None => QuotaStatus::Unlimited,
            Some(limit) => {
                if self.used >= limit {
                    QuotaStatus::Exhausted {
                        resets_at: self.resets_at,
                    }
                } else {
                    QuotaStatus::Available {
                        remaining: limit - self.used,
                    }
                }
            }
        }
    }

    /// Consume `actions` from the quota. Returns `Err(QuotaExhausted)` if over limit.
    pub fn consume(&mut self, actions: u32) -> AiResult<()> {
        self.reset_if_due();

        match self.limit {
            None => {
                // Unlimited — still track for analytics
                self.used += actions;
                self.persist();
                Ok(())
            }
            Some(limit) => {
                if self.used + actions > limit {
                    return Err(AiError::QuotaExhausted {
                        actions_used: self.used,
                        limit,
                        resets_at: self.resets_at.to_rfc3339(),
                    });
                }
                self.used += actions;
                info!(
                    used = self.used,
                    limit = limit,
                    remaining = limit - self.used,
                    "Quota consumed"
                );
                self.persist();
                Ok(())
            }
        }
    }

    /// Update local quota from a server response (called after cloud completions).
    pub fn update_from_server(&mut self, actions_remaining: u32) {
        if let Some(limit) = self.limit {
            let server_used = limit.saturating_sub(actions_remaining);
            if server_used != self.used {
                debug!(
                    local = self.used,
                    server = server_used,
                    "Syncing quota with server"
                );
                self.used = server_used;
                self.persist();
            }
        }
        self.last_synced = Utc::now();
    }

    /// Reset the quota if the reset date has passed.
    pub fn reset_if_due(&mut self) {
        if Utc::now() >= self.resets_at {
            info!(used = self.used, "Monthly quota reset");
            self.used = 0;
            self.resets_at = next_month_reset();
            self.persist();
        }
    }

    /// Whether the tracker needs to sync with the server.
    /// Syncs: on startup, every 10 actions, when quota looks wrong.
    pub fn needs_server_sync(&self) -> bool {
        let since_sync = Utc::now() - self.last_synced;
        since_sync.num_seconds() > 300 || self.used % 10 == 0
    }

    /// Current used action count.
    pub fn used(&self) -> u32 {
        self.used
    }

    /// Persist quota to disk.
    fn persist(&self) {
        let path = quota_cache_path();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        match serde_json::to_string_pretty(self) {
            Ok(json) => {
                if let Err(e) = std::fs::write(&path, json) {
                    warn!("Failed to persist quota cache: {}", e);
                }
            }
            Err(e) => warn!("Failed to serialize quota cache: {}", e),
        }
    }
}

/// Path for the local quota cache file.
fn quota_cache_path() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("kitsune")
        .join("quota_cache.json")
}

/// Compute next month's reset timestamp (1st of next month, UTC midnight).
fn next_month_reset() -> DateTime<Utc> {
    let now = Utc::now();
    // Move to next month
    let (year, month) = if now.month() == 12 {
        (now.year() + 1, 1)
    } else {
        (now.year(), now.month() + 1)
    };
    chrono::NaiveDate::from_ymd_opt(year, month, 1)
        .and_then(|d| d.and_hms_opt(0, 0, 0))
        .map(|ndt| DateTime::<Utc>::from_naive_utc_and_offset(ndt, Utc))
        .unwrap_or(now) // Safe fallback
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_quota_consume_decrements_correctly() {
        let mut tracker = QuotaTracker::new_free();
        tracker.consume(10).unwrap();
        assert_eq!(tracker.used(), 10);
        match tracker.check() {
            QuotaStatus::Available { remaining } => assert_eq!(remaining, 90),
            other => panic!("expected Available, got {:?}", other),
        }
    }

    #[test]
    fn test_quota_exhausted_after_100_free_actions() {
        let mut tracker = QuotaTracker::new_free();
        tracker.consume(100).unwrap();
        assert!(matches!(tracker.check(), QuotaStatus::Exhausted { .. }));

        // Next consume should error
        let result = tracker.consume(1);
        assert!(matches!(result, Err(AiError::QuotaExhausted { .. })));
    }

    #[test]
    fn test_quota_unlimited_for_pro_tier() {
        let mut tracker = QuotaTracker::new_pro();
        // Consuming 10000 should never fail
        for _ in 0..100 {
            tracker.consume(100).unwrap();
        }
        assert!(matches!(tracker.check(), QuotaStatus::Unlimited));
    }

    #[test]
    fn test_quota_partial_exhaustion_allows_up_to_limit() {
        let mut tracker = QuotaTracker::new_free();
        tracker.consume(99).unwrap();
        // One more should succeed
        tracker.consume(1).unwrap();
        // But not two
        let result = tracker.consume(1);
        assert!(result.is_err());
    }
}
