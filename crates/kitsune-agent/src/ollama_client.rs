//! Minimal client for the local Ollama daemon.
//!
//! We use the `/api/chat` endpoint with `format: "json"` so the model is forced
//! to emit a single JSON object that the runtime then parses into an
//! [`crate::action::AgentAction`].
//!
//! INVARIANT: this client is intentionally local-only. It never falls back to
//! a cloud endpoint and carries no API-key state — all sensitive routing for
//! agent traffic stays on the user's machine.

use crate::error::AgentError;
use serde::{Deserialize, Serialize};

pub const DEFAULT_OLLAMA_URL: &str = "http://localhost:11434";
pub const DEFAULT_OLLAMA_MODEL: &str = "llama3";

#[derive(Debug, Clone)]
pub struct OllamaClient {
    pub base_url: String,
    pub model: String,
    client: reqwest::Client,
}

#[derive(Debug, Serialize)]
struct ChatMessage<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(Debug, Serialize)]
struct ChatRequest<'a> {
    model: &'a str,
    messages: Vec<ChatMessage<'a>>,
    stream: bool,
    format: &'a str,
}

#[derive(Debug, Deserialize)]
struct ChatResponseMessage {
    #[serde(default)]
    content: String,
}

#[derive(Debug, Deserialize)]
struct ChatResponse {
    #[serde(default)]
    message: Option<ChatResponseMessage>,
}

impl OllamaClient {
    pub fn new(base_url: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            model: model.into(),
            // Local-only client: accept self-signed certs on LAN Ollama instances.
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(120))
                .danger_accept_invalid_certs(true)
                .build()
                .unwrap_or_else(|_| reqwest::Client::new()),
        }
    }

    pub fn default_local() -> Self {
        Self::new(DEFAULT_OLLAMA_URL, DEFAULT_OLLAMA_MODEL)
    }

    /// Send a chat request. `history` is `(role, content)` — typically alternating
    /// `user`/`assistant` entries. The system prompt is supplied separately.
    pub async fn chat(
        &self,
        system: &str,
        history: Vec<(String, String)>,
    ) -> Result<String, AgentError> {
        let mut messages = Vec::with_capacity(history.len() + 1);
        messages.push(ChatMessage {
            role: "system",
            content: system,
        });
        for (role, content) in &history {
            messages.push(ChatMessage {
                role,
                content,
            });
        }

        let url = format!("{}/api/chat", self.base_url.trim_end_matches('/'));
        let req = ChatRequest {
            model: &self.model,
            messages,
            stream: false,
            format: "json",
        };

        let resp = self
            .client
            .post(&url)
            .json(&req)
            .send()
            .await
            .map_err(|e| {
                if e.is_connect() || e.is_timeout() {
                    AgentError::ExecutionError(format!(
                        "Ollama not responding at {}: {}",
                        self.base_url, e
                    ))
                } else {
                    AgentError::ExecutionError(format!("Ollama request failed: {}", e))
                }
            })?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(AgentError::ExecutionError(format!(
                "Ollama returned HTTP {}: {}",
                status, body
            )));
        }

        let parsed: ChatResponse = resp
            .json()
            .await
            .map_err(|e| AgentError::ExecutionError(format!("Ollama bad JSON: {}", e)))?;

        Ok(parsed.message.map(|m| m.content).unwrap_or_default())
    }
}
