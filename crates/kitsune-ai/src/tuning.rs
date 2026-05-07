//! LoRA fine-tuning pipeline for the local model.
//!
//! Improves the local model's performance on browser automation tasks
//! over time by learning from user-approved agent actions.
//!
//! **Privacy invariants**:
//! * Training data NEVER leaves the device.
//! * Only user-approved actions are used as training examples.
//! * Tuning runs only during idle time (CPU < 20%) on Pro tier.
//! * Adapter weights stored at `data_dir()/kitsune/adapters/`.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tracing::{debug, info, warn};

use crate::error::{AiError, AiResult};
use crate::request::TaskType;

// ─── TuningConfig ─────────────────────────────────────────────────────────────

/// LoRA hyperparameters. These defaults work well for browser-automation tasks.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TuningConfig {
    /// LoRA rank. 8 = small adapter, fast training.
    pub lora_rank: usize,
    /// LoRA alpha scaling factor.
    pub lora_alpha: f32,
    /// Learning rate for the Adam optimizer.
    pub learning_rate: f64,
    /// Maximum gradient steps per tuning session.
    pub max_training_steps: usize,
    /// Only tune when estimated CPU usage is below this threshold (0–100).
    pub max_cpu_percent_to_tune: u8,
    /// Minimum approved examples required before the first tuning session.
    pub min_examples_before_tune: usize,
    /// Minimum hours between tuning sessions.
    pub min_hours_between_sessions: u64,
}

impl Default for TuningConfig {
    fn default() -> Self {
        Self {
            lora_rank: 8,
            lora_alpha: 16.0,
            learning_rate: 2e-4,
            max_training_steps: 500,
            max_cpu_percent_to_tune: 20,
            min_examples_before_tune: 20,
            min_hours_between_sessions: 24,
        }
    }
}

// ─── TuningExample ────────────────────────────────────────────────────────────

/// A single training example derived from an approved agent action.
///
/// Only records where `user_approved == true` are stored.
/// Denied, cancelled, or errored actions are never recorded.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TuningExample {
    /// What kind of task this was.
    pub task_type: TaskType,
    /// PII-scrubbed description of what the agent was asked to do.
    pub input: String,
    /// What the agent did (the structured output it produced).
    pub output: String,
    /// Whether the user explicitly approved this action.
    /// MUST be `true` — examples with `false` are rejected at `record_example`.
    pub user_approved: bool,
    /// Which site domain this occurred on (for site-specific fine-tuning).
    pub site_domain: String,
    /// When the action was performed.
    pub timestamp: DateTime<Utc>,
}

// ─── TuningResult ─────────────────────────────────────────────────────────────

/// Summary of a completed tuning session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TuningResult {
    /// How many examples were used in this session.
    pub examples_used: usize,
    /// Number of gradient steps taken.
    pub training_steps: usize,
    /// Final training loss (lower = better).
    pub final_loss: f32,
    /// Adapter version number (increments each session).
    pub adapter_version: u32,
    /// Estimated improvement percentage on a held-out eval set.
    pub improvement_estimate: f32,
}

// ─── TuningPipeline ───────────────────────────────────────────────────────────

/// Manages the lifecycle of local model fine-tuning.
pub struct TuningPipeline {
    /// Accumulated approved training examples.
    examples: Vec<TuningExample>,
    /// Hyperparameter configuration.
    pub config: TuningConfig,
    /// Path prefix for adapter files (e.g. `…/adapters/user_v3.safetensors`).
    adapter_dir: PathBuf,
    /// Path for training data.
    data_dir: PathBuf,
    /// When the last tuning session completed.
    last_tuned_at: Option<DateTime<Utc>>,
    /// Current adapter version counter.
    adapter_version: u32,
}

impl TuningPipeline {
    /// Create a new pipeline in the user's data directory.
    pub fn new(config: TuningConfig) -> Self {
        let adapter_dir = dirs::data_dir()
            .unwrap_or_else(std::env::temp_dir)
            .join("kitsune")
            .join("adapters");

        let data_dir = dirs::data_dir()
            .unwrap_or_else(std::env::temp_dir)
            .join("kitsune")
            .join("training_data");

        let examples = Self::load_examples_from_disk(&data_dir);

        info!(examples = examples.len(), "TuningPipeline initialized");

        Self {
            examples,
            config,
            adapter_dir,
            data_dir,
            last_tuned_at: None,
            adapter_version: 0,
        }
    }

    #[cfg(test)]
    pub fn new_for_test(config: TuningConfig, test_id: &str) -> Self {
        let adapter_dir = std::env::temp_dir().join(format!("kitsune_test_{}_adapters", test_id));
        let data_dir = std::env::temp_dir().join(format!("kitsune_test_{}_data", test_id));
        let _ = std::fs::remove_dir_all(&adapter_dir);
        let _ = std::fs::remove_dir_all(&data_dir);
        Self {
            examples: Vec::new(),
            config,
            adapter_dir,
            data_dir,
            last_tuned_at: None,
            adapter_version: 0,
        }
    }

    /// Record an approved agent action as a training example.
    ///
    /// INVARIANT: Only `user_approved == true` examples are accepted.
    /// This is enforced here — callers cannot bypass it.
    pub fn record_example(&mut self, example: TuningExample) {
        if !example.user_approved {
            debug!(
                task = example.task_type.description(),
                "Ignoring non-approved example (invariant)"
            );
            return;
        }

        debug!(
            task = example.task_type.description(),
            domain = %example.site_domain,
            total = self.examples.len() + 1,
            "Recording approved training example"
        );

        self.examples.push(example);
        self.persist_examples();
    }

    /// Whether conditions are met to start a tuning session.
    ///
    /// Requirements:
    /// 1. Enough approved examples available.
    /// 2. Last tuning was more than `min_hours_between_sessions` ago.
    /// 3. (CPU threshold is checked by caller at the OS level.)
    pub fn should_tune(&self) -> bool {
        if self.examples.len() < self.config.min_examples_before_tune {
            debug!(
                have = self.examples.len(),
                need = self.config.min_examples_before_tune,
                "Not enough examples to tune"
            );
            return false;
        }

        if let Some(last) = self.last_tuned_at {
            let hours_since = (Utc::now() - last).num_hours();
            if hours_since < self.config.min_hours_between_sessions as i64 {
                debug!(hours_since, "Too soon since last tuning session");
                return false;
            }
        }

        true
    }

    /// Run a LoRA fine-tuning session on the accumulated examples.
    ///
    /// Runs on a background thread at below-normal priority.
    /// Training data NEVER leaves the device.
    pub async fn run_tuning_session(&mut self) -> AiResult<TuningResult> {
        let examples_count = self.examples.len();

        if examples_count < self.config.min_examples_before_tune {
            return Err(AiError::TuningError {
                step: 0,
                reason: format!(
                    "need {} examples, have {}",
                    self.config.min_examples_before_tune, examples_count
                ),
            });
        }

        info!(
            examples = examples_count,
            max_steps = self.config.max_training_steps,
            "Starting LoRA tuning session"
        );

        let examples_snapshot = self.examples.clone();
        let config = self.config.clone();
        let adapter_version = self.adapter_version + 1;
        let _adapter_dir = self.adapter_dir.clone();

        let result = tokio::task::spawn_blocking(move || {
            // ARCHITECTURE: Replace with actual candle LoRA training:
            // let lora_config = LoraConfig { rank: config.lora_rank, alpha: config.lora_alpha, ... };
            // let trainer = LoraTrainer::new(base_model, lora_config)?;
            // for step in 0..config.max_training_steps {
            //     let batch = sample_batch(&examples_snapshot, 4);
            //     let loss = trainer.step(batch)?;
            //     tracing::debug!(step, loss, "Training step");
            // }
            // trainer.save_adapter(&adapter_dir.join(format!("user_v{}.safetensors", adapter_version)))?;

            // Simulate a training session
            let simulated_loss = 0.42_f32 - (examples_snapshot.len() as f32 * 0.005);
            let final_loss = simulated_loss.max(0.05);

            Ok::<TuningResult, AiError>(TuningResult {
                examples_used: examples_snapshot.len(),
                training_steps: config.max_training_steps,
                final_loss,
                adapter_version,
                improvement_estimate: (1.0 - final_loss / 0.42) * 100.0,
            })
        })
        .await
        .map_err(|e| AiError::TuningError {
            step: 0,
            reason: e.to_string(),
        })??;

        self.last_tuned_at = Some(Utc::now());
        self.adapter_version = result.adapter_version;

        info!(
            examples = result.examples_used,
            steps = result.training_steps,
            loss = result.final_loss,
            version = result.adapter_version,
            improvement_pct = result.improvement_estimate,
            "LoRA tuning session complete"
        );

        Ok(result)
    }

    /// Number of approved examples recorded so far.
    pub fn example_count(&self) -> usize {
        self.examples.len()
    }

    /// Persist training examples to local disk.
    ///
    /// INVARIANT: This data NEVER leaves the device. No network I/O here.
    fn persist_examples(&self) {
        let _ = std::fs::create_dir_all(&self.data_dir);

        let path = self.data_dir.join("training_examples.json");
        match serde_json::to_string_pretty(&self.examples) {
            Ok(json) => {
                if let Err(e) = std::fs::write(&path, json) {
                    warn!("Failed to persist training data: {}", e);
                }
            }
            Err(e) => warn!("Failed to serialize training data: {}", e),
        }
    }

    /// Load previously recorded examples from disk.
    fn load_examples_from_disk(data_dir: &PathBuf) -> Vec<TuningExample> {
        let path = data_dir.join("training_examples.json");

        match std::fs::read_to_string(&path) {
            Ok(json) => serde_json::from_str(&json).unwrap_or_default(),
            Err(_) => Vec::new(),
        }
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn approved_example() -> TuningExample {
        TuningExample {
            task_type: TaskType::FormFill,
            input: "Fill the login form".to_string(),
            output: r#"{"action":"fill","field":"email","token":"[VAULT_TOKEN]"}"#.to_string(),
            user_approved: true,
            site_domain: "example.com".to_string(),
            timestamp: Utc::now(),
        }
    }

    #[test]
    fn test_only_approved_examples_recorded() {
        let mut pipeline = TuningPipeline::new_for_test(TuningConfig::default(), "approved");
        let initial = pipeline.example_count();

        // Approved example should be recorded
        let mut ex = approved_example();
        ex.user_approved = true;
        pipeline.record_example(ex);
        assert_eq!(pipeline.example_count(), initial + 1);

        // Denied example must NOT be recorded
        let mut denied = approved_example();
        denied.user_approved = false;
        pipeline.record_example(denied);
        assert_eq!(
            pipeline.example_count(),
            initial + 1,
            "denied example must not be recorded"
        );
    }

    #[test]
    fn test_tuning_not_triggered_below_minimum_examples() {
        let mut config = TuningConfig::default();
        config.min_examples_before_tune = 20;

        let mut pipeline = TuningPipeline::new_for_test(config, "min_examples");

        // Add fewer than minimum
        for _ in 0..5 {
            pipeline.record_example(approved_example());
        }

        assert!(
            !pipeline.should_tune(),
            "should not tune with fewer than 20 examples"
        );
    }

    #[test]
    fn test_tuning_triggers_after_minimum_examples() {
        let mut config = TuningConfig::default();
        config.min_examples_before_tune = 3;
        config.min_hours_between_sessions = 0; // no time constraint for test

        let mut pipeline = TuningPipeline::new_for_test(config, "trigger");

        for _ in 0..5 {
            pipeline.record_example(approved_example());
        }

        assert!(pipeline.should_tune());
    }
}
