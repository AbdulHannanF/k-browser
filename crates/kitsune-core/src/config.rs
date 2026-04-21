/// Engine configuration.

use serde::{Deserialize, Serialize};

/// KitsuneEngine configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineConfig {
    /// Privacy settings.
    pub privacy: PrivacyConfig,
    /// Agent settings.
    pub agents: AgentConfig,
    /// UI settings.
    pub ui: UiConfig,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            privacy: PrivacyConfig::default(),
            agents: AgentConfig::default(),
            ui: UiConfig::default(),
        }
    }
}

/// Privacy configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrivacyConfig {
    /// Strip referer headers.
    pub strip_referer: bool,
    /// Send DNT header.
    pub send_dnt: bool,
    /// Send GPC header.
    pub send_gpc: bool,
    /// Block known trackers.
    pub block_trackers: bool,
    /// Minimum TLS version.
    pub min_tls_version: String,
    /// Enable fingerprinting resistance.
    pub fingerprint_resistance: bool,
}

impl Default for PrivacyConfig {
    fn default() -> Self {
        Self {
            strip_referer: true,
            send_dnt: true,
            send_gpc: true,
            block_trackers: true,
            min_tls_version: "1.3".to_string(),
            fingerprint_resistance: true,
        }
    }
}

/// Agent configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    /// Whether agents are enabled.
    pub enabled: bool,
    /// Whether to use local AI models.
    pub local_models_only: bool,
    /// Maximum number of concurrent agents.
    pub max_concurrent_agents: u32,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            local_models_only: true,
            max_concurrent_agents: 3,
        }
    }
}

/// UI configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiConfig {
    /// Theme (dark/light).
    pub theme: String,
    /// Show privacy dashboard on startup.
    pub show_privacy_dashboard: bool,
    /// Font size.
    pub font_size: f32,
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            theme: "dark".to_string(),
            show_privacy_dashboard: true,
            font_size: 14.0,
        }
    }
}
