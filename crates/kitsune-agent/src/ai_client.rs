//! Provider-agnostic AI client for agent sub-tasks.
//!
//! ARCHITECTURE: This crate cannot depend on `kitsune-ai` (circular dep —
//! kitsune-ai imports BudgetTracker from kitsune-agent). AgentAiClient
//! makes direct HTTP calls to Ollama or OpenAI-compatible endpoints.
//! The reqwest::Client is reused across calls — never constructed per-request.

use crate::error::{AgentError, AgentResult};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelTier {
    Orchestrator,
    Worker,
    Fast,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelSlots {
    pub orchestrator: String,
    pub worker: String,
    pub fast: String,
}

impl ModelSlots {
    pub fn model_for(&self, tier: ModelTier) -> &str {
        match tier {
            ModelTier::Orchestrator => &self.orchestrator,
            ModelTier::Worker => &self.worker,
            ModelTier::Fast => &self.fast,
        }
    }
}

impl Default for ModelSlots {
    fn default() -> Self {
        Self {
            orchestrator: "llama3:70b".into(),
            worker: "llama3:70b".into(),
            fast: "llama3:8b".into(),
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub enum AiProviderConfig {
    Ollama { url: String, slots: ModelSlots },
    OpenAiCompatible { url: String, api_key: String, slots: ModelSlots },
}

impl std::fmt::Debug for AiProviderConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Ollama { url, slots } => f
                .debug_struct("Ollama")
                .field("url", url)
                .field("slots", slots)
                .finish(),
            Self::OpenAiCompatible { url, slots, .. } => f
                .debug_struct("OpenAiCompatible")
                .field("url", url)
                .field("api_key", &"[REDACTED]")
                .field("slots", slots)
                .finish(),
        }
    }
}

impl AiProviderConfig {
    fn slots(&self) -> &ModelSlots {
        match self {
            Self::Ollama { slots, .. } => slots,
            Self::OpenAiCompatible { slots, .. } => slots,
        }
    }

    pub fn model_for(&self, tier: ModelTier) -> &str {
        self.slots().model_for(tier)
    }
}

impl Default for AiProviderConfig {
    fn default() -> Self {
        Self::Ollama {
            url: "http://localhost:11434".into(),
            slots: ModelSlots::default(),
        }
    }
}

#[derive(Serialize)]
struct OllamaRequest<'a> {
    model: &'a str,
    prompt: &'a str,
    stream: bool,
}

#[derive(Deserialize)]
struct OllamaResponse {
    response: String,
}

#[derive(Serialize)]
struct OpenAiMessage<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(Serialize)]
struct OpenAiRequest<'a> {
    model: &'a str,
    messages: Vec<OpenAiMessage<'a>>,
    max_tokens: u32,
}

#[derive(Deserialize)]
struct OpenAiChoice {
    message: OpenAiMessageContent,
}

#[derive(Deserialize)]
struct OpenAiMessageContent {
    content: String,
}

#[derive(Deserialize)]
struct OpenAiResponse {
    choices: Vec<OpenAiChoice>,
}

/// Lightweight AI client for agent sub-tasks.
/// Avoids the kitsune-ai → kitsune-agent circular dependency.
pub struct AgentAiClient {
    http: reqwest::Client,
    config: AiProviderConfig,
}

impl AgentAiClient {
    pub fn new(config: AiProviderConfig) -> AgentResult<Self> {
        let http = reqwest::Client::builder()
            .use_rustls_tls()
            .timeout(std::time::Duration::from_secs(120))
            .build()
            .map_err(|e| AgentError::Internal(e.to_string()))?;
        Ok(Self { http, config })
    }

    /// Send a prompt, receive a text response.
    pub async fn complete(&self, prompt: &str, tier: ModelTier) -> AgentResult<String> {
        match &self.config {
            AiProviderConfig::Ollama { url, .. } => {
                let model = self.config.model_for(tier);
                let base = url.trim_end_matches('/');
                let body = OllamaRequest { model, prompt, stream: false };
                let resp = self
                    .http
                    .post(format!("{}/api/generate", base))
                    .json(&body)
                    .send()
                    .await
                    .map_err(|e| AgentError::ExecutionError(e.to_string()))?;
                let status = resp.status();
                if !status.is_success() {
                    let body = resp.text().await.unwrap_or_default();
                    return Err(AgentError::ExecutionError(format!(
                        "provider returned HTTP {}: {}",
                        status, body
                    )));
                }
                let parsed: OllamaResponse = resp
                    .json()
                    .await
                    .map_err(|e| AgentError::ExecutionError(e.to_string()))?;
                Ok(parsed.response)
            }
            AiProviderConfig::OpenAiCompatible { url, api_key, .. } => {
                let model = self.config.model_for(tier);
                let base = url.trim_end_matches('/');
                let body = OpenAiRequest {
                    model,
                    messages: vec![OpenAiMessage { role: "user", content: prompt }],
                    max_tokens: 4096,
                };
                let resp = self
                    .http
                    .post(format!("{}/v1/chat/completions", base))
                    .bearer_auth(api_key)
                    .json(&body)
                    .send()
                    .await
                    .map_err(|e| AgentError::ExecutionError(e.to_string()))?;
                let status = resp.status();
                if !status.is_success() {
                    let body = resp.text().await.unwrap_or_default();
                    return Err(AgentError::ExecutionError(format!(
                        "provider returned HTTP {}: {}",
                        status, body
                    )));
                }
                let parsed: OpenAiResponse = resp
                    .json()
                    .await
                    .map_err(|e| AgentError::ExecutionError(e.to_string()))?;
                parsed
                    .choices
                    .into_iter()
                    .next()
                    .map(|c| c.message.content)
                    .ok_or_else(|| AgentError::ExecutionError("empty response".into()))
            }
        }
    }
}
