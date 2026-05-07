//! Agent Brain — LLM-powered action planning for KitsuneEngine agents.
//!
//! Calls an OpenAI-compatible chat completion endpoint OR a local Ollama
//! instance and parses the response into a sequence of agent actions that
//! the SSE endpoint streams back to the browser UI.

use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

// ─── Configuration ───────────────────────────────────────────────────────────

/// Which LLM backend to use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AiProvider {
    /// OpenAI-compatible /v1/chat/completions API (OpenAI, Anthropic via proxy,
    /// Together, Groq, OpenRouter, etc.). Requires an API key.
    OpenAiCompatible,
    /// Local Ollama daemon at http://localhost:11434. No API key.
    Ollama,
}

impl Default for AiProvider {
    fn default() -> Self {
        Self::OpenAiCompatible
    }
}

/// Settings for the AI backend — stored in server memory, never on disk.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiSettings {
    #[serde(default)]
    pub provider: AiProvider,
    pub api_key: String,
    pub endpoint: String,
    pub model: String,
}

impl Default for AiSettings {
    fn default() -> Self {
        Self {
            provider: AiProvider::OpenAiCompatible,
            api_key: String::new(),
            endpoint: "https://api.openai.com/v1/chat/completions".to_string(),
            model: "gpt-4o-mini".to_string(),
        }
    }
}

impl AiSettings {
    /// Whether the backend is usable (has API key for cloud, or always for Ollama).
    pub fn is_configured(&self) -> bool {
        match self.provider {
            AiProvider::OpenAiCompatible => !self.api_key.is_empty(),
            AiProvider::Ollama => true,
        }
    }
}

// ─── Agent Actions ───────────────────────────────────────────────────────────

/// A single action the agent will perform.
///
/// The set of actions is intentionally minimal — the agent's job is to navigate
/// the WebView and report progress. Anything more (form fill, payment) goes
/// through the HIL gate, which is a separate flow.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AgentAction {
    /// Log a message to the agent panel.
    Log {
        message: String,
        #[serde(default = "default_log_class")]
        class: String,
    },
    /// Update agent card status (price | form | research).
    AgentStatus { agent: String, status: String },
    /// Activate/deactivate an agent card.
    AgentCard { agent: String, active: bool },
    /// Notify the UI that a tracker request was blocked.
    TrackerBlocked {
        label: String,
        #[serde(default)]
        stripped: bool,
    },
    /// Navigate the WebView to a URL.
    UrlUpdate { url: String },
    /// HIL gate — pause for irreversible action.
    HilRequest {
        action: String,
        flight: String,
        date: String,
        passenger: String,
        total: String,
        credentials: String,
    },
    /// Final completion.
    Done {
        message: String,
        #[serde(default)]
        order_id: String,
    },
    /// Wait (ms) for natural pacing.
    Wait { ms: u64 },
}

fn default_log_class() -> String {
    "info".to_string()
}

// ─── Chat Completion Types (OpenAI-compatible) ───────────────────────────────

#[derive(Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<ChatMessage>,
    temperature: f64,
    max_tokens: u32,
}

#[derive(Serialize)]
struct ChatMessage {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct ChatResponse {
    choices: Vec<ChatChoice>,
}

#[derive(Deserialize)]
struct ChatChoice {
    message: ChatMessageResponse,
}

#[derive(Deserialize)]
struct ChatMessageResponse {
    content: Option<String>,
}

// ─── Ollama Types ────────────────────────────────────────────────────────────

#[derive(Serialize)]
struct OllamaRequest {
    model: String,
    messages: Vec<ChatMessage>,
    stream: bool,
    options: OllamaOptions,
}

#[derive(Serialize)]
struct OllamaOptions {
    temperature: f64,
    num_predict: i32,
}

#[derive(Deserialize)]
struct OllamaResponse {
    message: OllamaMessage,
}

#[derive(Deserialize)]
struct OllamaMessage {
    content: String,
}

// ─── System Prompt ───────────────────────────────────────────────────────────
//
// This prompt is tuned for *real* browsing, not the demo flight-booking flow.
// The LLM picks the right URL for the user's intent and emits a short action
// sequence that navigates the WebView there.

const SYSTEM_PROMPT: &str = r#"You are KitsuneEngine, an autonomous browsing agent. The user gives you a natural-language command; you plan a short sequence of browser actions that fulfils it.

OUTPUT FORMAT — CRITICAL:
Respond with ONLY a JSON array of action objects. No markdown fences, no prose, no code blocks. The first character of your reply must be `[` and the last must be `]`.

ALLOWED ACTIONS:
- {"type":"log","message":"<short status>","class":"info|ok|warn|cmd"}
- {"type":"url_update","url":"https://..."}            // navigate the WebView
- {"type":"tracker_blocked","label":"<domain>","stripped":false}   // optional, only if relevant
- {"type":"wait","ms":300}                              // pacing only, 200-1000
- {"type":"hil_request",...}                            // ONLY for irreversible actions (purchases, account creation, payments)
- {"type":"done","message":"<one-sentence result>","order_id":""}

URL PICKING RULES (most important):
- "search wikipedia for X" / "wikipedia X" → https://en.wikipedia.org/wiki/<X with underscores, capitalised>
- "go to <site>" / "open <site>" → https://www.<site>.com (or .org/.io if obvious)
- "search <query>" / "find <query>" / "google <query>" → https://www.google.com/search?q=<urlencoded query>
- "youtube X" → https://www.youtube.com/results?search_query=<urlencoded X>
- "github X" → https://github.com/search?q=<urlencoded X>
- News queries → https://news.google.com/search?q=<urlencoded X>
- Shopping ("buy X", "shop for X") → https://www.amazon.com/s?k=<urlencoded X>
- If the user gives a full URL, navigate there directly.
- Always use https. Never invent fake URLs.

SHAPE OF RESPONSE:
1. ONE log entry restating the intent (class:"cmd").
2. ONE log entry describing what you're about to do (class:"info").
3. ONE wait of 300-600ms.
4. ONE url_update.
5. ONE wait of 600-1200ms.
6. ONE log entry confirming what the user should now see (class:"ok").
7. ONE done with a one-sentence summary.

TOTAL: 6-9 actions. Keep all messages under 70 characters. Do NOT emit hil_request unless the command actually involves a payment or account creation."#;

// ─── AgentBrain ──────────────────────────────────────────────────────────────

pub struct AgentBrain {
    http: Client,
    settings: AiSettings,
}

impl AgentBrain {
    pub fn new(settings: AiSettings) -> Self {
        Self {
            http: Client::builder()
                .timeout(std::time::Duration::from_secs(60))
                .build()
                .unwrap_or_default(),
            settings,
        }
    }

    /// Plan actions for a command. If the LLM is unconfigured or fails, fall
    /// back to a deterministic local planner that handles common patterns.
    pub async fn plan_actions(&self, command: &str) -> Vec<AgentAction> {
        info!(command = %command, provider = ?self.settings.provider, "Planning agent actions");

        if self.settings.is_configured() {
            match self.call_llm(command).await {
                Ok(actions) if !actions.is_empty() => {
                    info!(count = actions.len(), "LLM returned action plan");
                    return actions;
                }
                Ok(_) => warn!("LLM returned empty action list; falling back"),
                Err(e) => warn!(error = %e, "LLM call failed; falling back"),
            }
        } else {
            info!("No LLM configured; using local planner");
        }

        local_plan(command)
    }

    async fn call_llm(&self, command: &str) -> Result<Vec<AgentAction>, String> {
        let user_message = format!(
            "User command: \"{}\"\n\nReturn the JSON array now.",
            command
        );

        let raw = match self.settings.provider {
            AiProvider::OpenAiCompatible => self.call_openai(&user_message).await?,
            AiProvider::Ollama => self.call_ollama(&user_message).await?,
        };

        debug!(raw_content = %raw, "Raw LLM response");
        parse_action_json(&raw)
    }

    async fn call_openai(&self, user_message: &str) -> Result<String, String> {
        let request = ChatRequest {
            model: self.settings.model.clone(),
            messages: vec![
                ChatMessage {
                    role: "system".to_string(),
                    content: SYSTEM_PROMPT.to_string(),
                },
                ChatMessage {
                    role: "user".to_string(),
                    content: user_message.to_string(),
                },
            ],
            temperature: 0.2,
            max_tokens: 1500,
        };

        let response = self
            .http
            .post(&self.settings.endpoint)
            .header("Authorization", format!("Bearer {}", self.settings.api_key))
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await
            .map_err(|e| format!("HTTP request failed: {}", e))?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(format!("LLM API returned {}: {}", status, body));
        }

        let chat_response: ChatResponse = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse LLM response: {}", e))?;

        chat_response
            .choices
            .into_iter()
            .next()
            .and_then(|c| c.message.content)
            .ok_or_else(|| "No content in LLM response".to_string())
    }

    async fn call_ollama(&self, user_message: &str) -> Result<String, String> {
        // Default Ollama chat endpoint. The settings `endpoint` field is
        // ignored for Ollama; we always hit /api/chat on the local daemon.
        let endpoint = if self.settings.endpoint.starts_with("http") {
            // Allow override (e.g. http://localhost:11434/api/chat or remote)
            if self.settings.endpoint.ends_with("/api/chat") {
                self.settings.endpoint.clone()
            } else {
                format!("{}/api/chat", self.settings.endpoint.trim_end_matches('/'))
            }
        } else {
            "http://localhost:11434/api/chat".to_string()
        };

        let request = OllamaRequest {
            model: if self.settings.model.is_empty() {
                "llama3.2".to_string()
            } else {
                self.settings.model.clone()
            },
            messages: vec![
                ChatMessage {
                    role: "system".to_string(),
                    content: SYSTEM_PROMPT.to_string(),
                },
                ChatMessage {
                    role: "user".to_string(),
                    content: user_message.to_string(),
                },
            ],
            stream: false,
            options: OllamaOptions {
                temperature: 0.2,
                num_predict: 1500,
            },
        };

        let response = self
            .http
            .post(&endpoint)
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await
            .map_err(|e| format!("Ollama request failed (is `ollama serve` running?): {}", e))?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(format!("Ollama returned {}: {}", status, body));
        }

        let resp: OllamaResponse = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse Ollama response: {}", e))?;

        Ok(resp.message.content)
    }
}

/// Strip markdown fences and parse a JSON action array.
fn parse_action_json(raw: &str) -> Result<Vec<AgentAction>, String> {
    let trimmed = raw.trim();
    // Strip ```json ... ``` or ``` ... ```
    let cleaned = trimmed
        .strip_prefix("```json")
        .or_else(|| trimmed.strip_prefix("```"))
        .unwrap_or(trimmed);
    let cleaned = cleaned.strip_suffix("```").unwrap_or(cleaned).trim();

    // Some smaller models add a preamble — try to find the first `[`.
    let cleaned = match cleaned.find('[') {
        Some(idx) => &cleaned[idx..],
        None => cleaned,
    };
    // And cut after the last `]`.
    let cleaned = match cleaned.rfind(']') {
        Some(idx) => &cleaned[..=idx],
        None => cleaned,
    };

    serde_json::from_str::<Vec<AgentAction>>(cleaned)
        .map_err(|e| format!("Failed to parse action JSON: {} — raw: {}", e, cleaned))
}

// ─── Local planner (no LLM needed) ───────────────────────────────────────────
//
// Handles common patterns deterministically so the demo works fully offline.
// Pattern priority is most-specific first.

fn local_plan(command: &str) -> Vec<AgentAction> {
    let cmd = command.trim();
    let lower = cmd.to_lowercase();

    let (intent, url, summary) = pick_url(&lower, cmd);

    vec![
        AgentAction::Log {
            message: format!("agent run \"{}\"", cmd),
            class: "cmd".to_string(),
        },
        AgentAction::Log {
            message: intent,
            class: "info".to_string(),
        },
        AgentAction::Wait { ms: 400 },
        AgentAction::TrackerBlocked {
            label: "doubleclick.net".to_string(),
            stripped: false,
        },
        AgentAction::Wait { ms: 200 },
        AgentAction::UrlUpdate { url },
        AgentAction::Wait { ms: 900 },
        AgentAction::Log {
            message: summary,
            class: "ok".to_string(),
        },
        AgentAction::Done {
            message: "Navigation complete. Read the page in the viewer.".to_string(),
            order_id: String::new(),
        },
    ]
}

/// Returns (intent message, target URL, success summary).
fn pick_url(lower: &str, original: &str) -> (String, String, String) {
    // 1. Direct URL pasted in?
    if let Some(url) = extract_url(original) {
        return (
            format!("Direct navigation to {}", trim_for_log(&url)),
            url.clone(),
            format!("Loaded {}", trim_for_log(&url)),
        );
    }

    // 2. Wikipedia
    if let Some(topic) = strip_prefix_any(
        lower,
        &[
            "search wikipedia for ",
            "wikipedia search ",
            "wikipedia for ",
            "wikipedia ",
            "wiki ",
        ],
    ) {
        let topic = topic.trim();
        let slug = topic
            .split_whitespace()
            .map(capitalise)
            .collect::<Vec<_>>()
            .join("_");
        let url = format!("https://en.wikipedia.org/wiki/{}", urlencoding::encode(&slug));
        return (
            format!("Looking up '{}' on Wikipedia", topic),
            url,
            format!("Loaded Wikipedia article: {}", topic),
        );
    }

    // 3. YouTube
    if let Some(q) = strip_prefix_any(lower, &["youtube ", "search youtube for ", "play "]) {
        let q = q.trim();
        let url = format!(
            "https://www.youtube.com/results?search_query={}",
            urlencoding::encode(q)
        );
        return (
            format!("Searching YouTube for '{}'", q),
            url,
            format!("YouTube results for: {}", q),
        );
    }

    // 4. GitHub
    if let Some(q) = strip_prefix_any(lower, &["github ", "search github for "]) {
        let q = q.trim();
        let url = format!(
            "https://github.com/search?q={}&type=repositories",
            urlencoding::encode(q)
        );
        return (
            format!("Searching GitHub for '{}'", q),
            url,
            format!("GitHub repositories matching: {}", q),
        );
    }

    // 5. News
    if let Some(q) = strip_prefix_any(lower, &["news about ", "news on ", "news for ", "news "]) {
        let q = q.trim();
        let url = format!(
            "https://news.google.com/search?q={}",
            urlencoding::encode(q)
        );
        return (
            format!("Fetching news on '{}'", q),
            url,
            format!("Google News results for: {}", q),
        );
    }

    // 6. Shopping
    if let Some(q) = strip_prefix_any(lower, &["buy ", "shop for ", "shop ", "amazon "]) {
        let q = q.trim();
        let url = format!("https://www.amazon.com/s?k={}", urlencoding::encode(q));
        return (
            format!("Shopping for '{}' on Amazon", q),
            url,
            format!("Amazon results for: {}", q),
        );
    }

    // 7. "go to X" / "open X"
    if let Some(site) = strip_prefix_any(lower, &["go to ", "open ", "navigate to ", "visit "]) {
        let site = site.trim().trim_matches(|c: char| c == '"' || c == '\'');
        let url = if site.starts_with("http://") || site.starts_with("https://") {
            site.to_string()
        } else if site.contains('.') {
            format!("https://{}", site)
        } else {
            // Try .com first
            format!("https://www.{}.com", site.replace(' ', ""))
        };
        return (
            format!("Opening {}", trim_for_log(&url)),
            url.clone(),
            format!("Loaded {}", trim_for_log(&url)),
        );
    }

    // 8. Generic search
    let query = strip_prefix_any(
        lower,
        &[
            "search for ",
            "search ",
            "find ",
            "google ",
            "look up ",
            "lookup ",
        ],
    )
    .unwrap_or(lower)
    .trim()
    .to_string();
    let url = format!(
        "https://www.google.com/search?q={}",
        urlencoding::encode(&query)
    );
    (
        format!("Searching the web for '{}'", query),
        url,
        format!("Google results for: {}", query),
    )
}

fn strip_prefix_any<'a>(s: &'a str, prefixes: &[&str]) -> Option<&'a str> {
    for p in prefixes {
        if let Some(rest) = s.strip_prefix(p) {
            return Some(rest);
        }
    }
    None
}

fn extract_url(s: &str) -> Option<String> {
    for tok in s.split_whitespace() {
        if tok.starts_with("http://") || tok.starts_with("https://") {
            return Some(tok.trim_end_matches(|c: char| matches!(c, '.' | ',' | ';' | '!' | '?')).to_string());
        }
    }
    None
}

fn capitalise(word: &str) -> String {
    let mut chars = word.chars();
    match chars.next() {
        Some(c) => c.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

fn trim_for_log(url: &str) -> String {
    if url.len() > 60 {
        format!("{}…", &url[..58])
    } else {
        url.to_string()
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_default_not_configured() {
        let settings = AiSettings::default();
        assert!(!settings.is_configured());
    }

    #[test]
    fn settings_with_key_is_configured() {
        let settings = AiSettings {
            api_key: "sk-test".to_string(),
            ..Default::default()
        };
        assert!(settings.is_configured());
    }

    #[test]
    fn ollama_is_always_configured() {
        let settings = AiSettings {
            provider: AiProvider::Ollama,
            api_key: String::new(),
            ..Default::default()
        };
        assert!(settings.is_configured());
    }

    #[test]
    fn local_plan_starts_with_command_log() {
        let actions = local_plan("search wikipedia for batman");
        match &actions[0] {
            AgentAction::Log { message, class } => {
                assert!(message.contains("search wikipedia for batman"));
                assert_eq!(class, "cmd");
            }
            _ => panic!("First action should be a Log"),
        }
    }

    #[test]
    fn local_plan_wikipedia_navigates_to_article() {
        let actions = local_plan("search wikipedia for batman");
        let urls: Vec<&str> = actions
            .iter()
            .filter_map(|a| match a {
                AgentAction::UrlUpdate { url } => Some(url.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(urls.len(), 1, "Should produce exactly one navigation");
        assert!(urls[0].contains("en.wikipedia.org/wiki/"));
        assert!(
            urls[0].to_lowercase().contains("batman"),
            "URL should contain Batman, got {}",
            urls[0]
        );
    }

    #[test]
    fn local_plan_wikipedia_short_form() {
        let actions = local_plan("wikipedia rust programming language");
        let url = actions
            .iter()
            .find_map(|a| match a {
                AgentAction::UrlUpdate { url } => Some(url.clone()),
                _ => None,
            })
            .expect("should have a URL");
        assert!(url.contains("en.wikipedia.org"));
    }

    #[test]
    fn local_plan_youtube() {
        let actions = local_plan("youtube lofi hip hop");
        let url = actions
            .iter()
            .find_map(|a| match a {
                AgentAction::UrlUpdate { url } => Some(url.clone()),
                _ => None,
            })
            .unwrap();
        assert!(url.contains("youtube.com/results"));
        assert!(url.contains("lofi"));
    }

    #[test]
    fn local_plan_go_to_site() {
        let actions = local_plan("go to github.com");
        let url = actions
            .iter()
            .find_map(|a| match a {
                AgentAction::UrlUpdate { url } => Some(url.clone()),
                _ => None,
            })
            .unwrap();
        assert_eq!(url, "https://github.com");
    }

    #[test]
    fn local_plan_open_bareword() {
        let actions = local_plan("open reddit");
        let url = actions
            .iter()
            .find_map(|a| match a {
                AgentAction::UrlUpdate { url } => Some(url.clone()),
                _ => None,
            })
            .unwrap();
        assert_eq!(url, "https://www.reddit.com");
    }

    #[test]
    fn local_plan_direct_url() {
        let actions = local_plan("https://example.com/foo");
        let url = actions
            .iter()
            .find_map(|a| match a {
                AgentAction::UrlUpdate { url } => Some(url.clone()),
                _ => None,
            })
            .unwrap();
        assert_eq!(url, "https://example.com/foo");
    }

    #[test]
    fn local_plan_generic_search_falls_back_to_google() {
        let actions = local_plan("how do quantum computers work");
        let url = actions
            .iter()
            .find_map(|a| match a {
                AgentAction::UrlUpdate { url } => Some(url.clone()),
                _ => None,
            })
            .unwrap();
        assert!(url.starts_with("https://www.google.com/search?q="));
    }

    #[test]
    fn local_plan_ends_with_done() {
        let actions = local_plan("search wikipedia for batman");
        assert!(matches!(actions.last(), Some(AgentAction::Done { .. })));
    }

    #[test]
    fn local_plan_no_hil_for_search() {
        let actions = local_plan("search wikipedia for batman");
        let has_hil = actions
            .iter()
            .any(|a| matches!(a, AgentAction::HilRequest { .. }));
        assert!(!has_hil, "Pure search/navigation must not trigger HIL");
    }

    #[test]
    fn parse_action_json_strips_markdown_fences() {
        let raw = "```json\n[{\"type\":\"log\",\"message\":\"hi\",\"class\":\"info\"}]\n```";
        let actions = parse_action_json(raw).unwrap();
        assert_eq!(actions.len(), 1);
    }

    #[test]
    fn parse_action_json_finds_array_in_preamble() {
        let raw = "Here you go:\n[{\"type\":\"log\",\"message\":\"hi\",\"class\":\"info\"}]";
        let actions = parse_action_json(raw).unwrap();
        assert_eq!(actions.len(), 1);
    }

    #[test]
    fn agent_action_serialises_with_type_tag() {
        let action = AgentAction::Log {
            message: "test".to_string(),
            class: "info".to_string(),
        };
        let json = serde_json::to_string(&action).unwrap();
        assert!(json.contains("\"type\":\"log\""));
    }

    #[test]
    fn agent_action_url_update_round_trips() {
        let json = r#"{"type":"url_update","url":"https://example.com"}"#;
        let action: AgentAction = serde_json::from_str(json).unwrap();
        match action {
            AgentAction::UrlUpdate { url } => assert_eq!(url, "https://example.com"),
            _ => panic!("Should deserialise to UrlUpdate"),
        }
    }
}
