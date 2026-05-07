//! Agent Brain — LLM-powered action planning for KitsuneEngine agents.
//!
//! Calls an OpenAI-compatible chat completion endpoint and parses the
//! structured response into a sequence of agent actions that the SSE
//! endpoint streams back to the browser UI.

use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

// ─── Configuration ───────────────────────────────────────────────────────────

/// Settings for the AI backend — stored in server memory, never on disk.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiSettings {
    pub api_key: String,
    pub endpoint: String,
    pub model: String,
}

impl Default for AiSettings {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            endpoint: "https://api.openai.com/v1/chat/completions".to_string(),
            model: "gpt-4o-mini".to_string(),
        }
    }
}

impl AiSettings {
    pub fn is_configured(&self) -> bool {
        !self.api_key.is_empty()
    }
}

// ─── Agent Actions ───────────────────────────────────────────────────────────

/// A single action the agent will perform.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AgentAction {
    /// Log a message to the agent panel.
    Log {
        message: String,
        #[serde(default = "default_log_class")]
        class: String,
    },
    /// Change an agent card's status.
    AgentStatus {
        agent: String,
        status: String,
    },
    /// Activate/deactivate an agent card.
    AgentCard {
        agent: String,
        active: bool,
    },
    /// Simulate blocking a tracker.
    TrackerBlocked {
        label: String,
        #[serde(default)]
        stripped: bool,
    },
    /// Move the agent cursor to coordinates in the webview.
    CursorMove {
        x: f64,
        y: f64,
    },
    /// Hide the agent cursor.
    CursorHide,
    /// Trigger the HIL gate approval dialog.
    HilRequest {
        action: String,
        flight: String,
        date: String,
        passenger: String,
        total: String,
        credentials: String,
    },
    /// Update the URL bar.
    UrlUpdate {
        url: String,
    },
    /// Final completion — all done.
    Done {
        message: String,
        order_id: String,
    },
    /// Wait for a duration (milliseconds).
    Wait {
        ms: u64,
    },
}

fn default_log_class() -> String {
    "info".to_string()
}

// ─── SSE Event ───────────────────────────────────────────────────────────────

/// An event sent over the SSE stream to the browser.
#[derive(Debug, Clone, Serialize)]
pub struct AgentEvent {
    pub event: String,
    pub data: serde_json::Value,
}

// ─── Chat Completion Types ───────────────────────────────────────────────────

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

// ─── System Prompt ───────────────────────────────────────────────────────────

const SYSTEM_PROMPT: &str = r#"You are the KitsuneEngine agent brain. Given a user command, you plan a sequence of browser automation actions.

You MUST respond with ONLY a JSON array of action objects. No markdown, no explanation, just the JSON array.

Available action types:
- {"type": "log", "message": "...", "class": "info|ok|warn|block|cmd"}
- {"type": "agent_status", "agent": "price|form|research", "status": "running|done|idle"}
- {"type": "agent_card", "agent": "price|form|research", "active": true|false}
- {"type": "tracker_blocked", "label": "tracker-domain.com", "stripped": false}
- {"type": "cursor_move", "x": 400, "y": 300}
- {"type": "cursor_hide"}
- {"type": "hil_request", "action": "...", "flight": "...", "date": "...", "passenger": "...", "total": "...", "credentials": "Vault token - never exposed"}
- {"type": "url_update", "url": "https://..."}
- {"type": "wait", "ms": 800}
- {"type": "done", "message": "...", "order_id": "KSN-..."}

Create a realistic browser automation sequence. Include:
1. Initial logging of the command
2. Setting agent statuses as they activate
3. Blocking trackers (3-4 typical ad trackers)
4. Agent actions with descriptive logs
5. A HIL gate for any irreversible action (payment, submission)
6. Completion

Add "wait" actions between steps (300-1200ms) for realistic pacing.
Keep the total sequence between 12-25 actions.
Make messages specific to the user's command, not generic.

Example for "book cheapest flight to Berlin":
[
  {"type": "log", "message": "agent run \"book cheapest flight to Berlin\"", "class": "cmd"},
  {"type": "log", "message": "Initializing agent runtime...", "class": "info"},
  {"type": "wait", "ms": 500},
  {"type": "tracker_blocked", "label": "doubleclick.net", "stripped": false},
  {"type": "wait", "ms": 400},
  {"type": "agent_card", "agent": "price", "active": true},
  {"type": "agent_status", "agent": "price", "status": "running"},
  {"type": "log", "message": "PriceTracker: scanning 3 booking engines...", "class": "info"},
  ...
]"#;

// ─── AgentBrain ──────────────────────────────────────────────────────────────

/// The agent brain calls the LLM and returns a plan of actions.
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

    /// Call the LLM to plan actions for the given command.
    /// Falls back to demo actions if the LLM call fails.
    pub async fn plan_actions(&self, command: &str) -> Vec<AgentAction> {
        info!(command = %command, "Planning agent actions via LLM");

        match self.call_llm(command).await {
            Ok(actions) => {
                info!(count = actions.len(), "LLM returned action plan");
                actions
            }
            Err(e) => {
                warn!(error = %e, "LLM call failed, using fallback demo actions");
                self.fallback_actions(command)
            }
        }
    }

    async fn call_llm(&self, command: &str) -> Result<Vec<AgentAction>, String> {
        let user_message = format!(
            "User command: \"{}\"\n\nGenerate the action sequence JSON array.",
            command
        );

        let request = ChatRequest {
            model: self.settings.model.clone(),
            messages: vec![
                ChatMessage {
                    role: "system".to_string(),
                    content: SYSTEM_PROMPT.to_string(),
                },
                ChatMessage {
                    role: "user".to_string(),
                    content: user_message,
                },
            ],
            temperature: 0.3,
            max_tokens: 4000,
        };

        debug!(endpoint = %self.settings.endpoint, model = %self.settings.model, "Calling LLM");

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

        let content = chat_response
            .choices
            .first()
            .and_then(|c| c.message.content.as_deref())
            .ok_or("No content in LLM response")?;

        debug!(raw_content = %content, "Raw LLM response");

        // Strip markdown fences if the model wraps its response
        let cleaned = content
            .trim()
            .strip_prefix("```json")
            .or_else(|| content.trim().strip_prefix("```"))
            .unwrap_or(content.trim());
        let cleaned = cleaned
            .strip_suffix("```")
            .unwrap_or(cleaned)
            .trim();

        let actions: Vec<AgentAction> = serde_json::from_str(cleaned)
            .map_err(|e| format!("Failed to parse action JSON: {} — raw: {}", e, cleaned))?;

        Ok(actions)
    }

    /// Fallback demo actions when LLM is unreachable.
    fn fallback_actions(&self, command: &str) -> Vec<AgentAction> {
        let cmd = command.to_lowercase();

        if cmd.contains("buy") || cmd.contains("shoe") || cmd.contains("shop") {
            // Shopping Sequence
            vec![
                AgentAction::Log { message: format!("agent run \"{}\"", command), class: "cmd".to_string() },
                AgentAction::Log { message: "Initializing agent runtime...".to_string(), class: "info".to_string() },
                AgentAction::Wait { ms: 500 },
                AgentAction::UrlUpdate { url: "https://www.ebay.com/sch/i.html?_nkw=nike+air+max+size+10".to_string() },
                AgentAction::Log { message: "Navigating to store search...".to_string(), class: "ok".to_string() },
                AgentAction::Wait { ms: 4000 },
                AgentAction::TrackerBlocked { label: "analytics.twitter.com".to_string(), stripped: false },
                AgentAction::AgentCard { agent: "price".to_string(), active: true },
                AgentAction::AgentStatus { agent: "price".to_string(), status: "running".to_string() },
                AgentAction::Log { message: "Scanning catalog for Air Max size 10...".to_string(), class: "info".to_string() },
                AgentAction::Wait { ms: 3000 },
                AgentAction::Log { message: "Found match: $120.00 In Stock".to_string(), class: "ok".to_string() },
                AgentAction::AgentStatus { agent: "price".to_string(), status: "done".to_string() },
                AgentAction::AgentCard { agent: "price".to_string(), active: false },
                AgentAction::Wait { ms: 1000 },
                AgentAction::UrlUpdate { url: "https://cart.payments.ebay.com/".to_string() },
                AgentAction::Wait { ms: 3500 },
                AgentAction::AgentCard { agent: "form".to_string(), active: true },
                AgentAction::AgentStatus { agent: "form".to_string(), status: "running".to_string() },
                AgentAction::Log { message: "FormFillAgent: injecting shipping details...".to_string(), class: "info".to_string() },
                AgentAction::Wait { ms: 1000 },
                AgentAction::Log { message: "HIL GATE - awaiting human approval...".to_string(), class: "warn".to_string() },
                AgentAction::HilRequest {
                    action: "Place Order".to_string(),
                    flight: "Nike Air Max 90".to_string(),
                    date: "Size 10 - Color: White".to_string(),
                    passenger: "Demo User - Home Address".to_string(),
                    total: "$120.00".to_string(),
                    credentials: "Visa ending in 4242".to_string(),
                },
            ]
        } else if cmd.contains("research") || cmd.contains("crypto") || cmd.contains("bitcoin") {
            // Research Sequence
            vec![
                AgentAction::Log { message: format!("agent run \"{}\"", command), class: "cmd".to_string() },
                AgentAction::UrlUpdate { url: "https://coinmarketcap.com/".to_string() },
                AgentAction::Wait { ms: 1200 },
                AgentAction::TrackerBlocked { label: "doubleclick.net".to_string(), stripped: false },
                AgentAction::AgentCard { agent: "research".to_string(), active: true },
                AgentAction::AgentStatus { agent: "research".to_string(), status: "running".to_string() },
                AgentAction::Log { message: "ResearchAgent: analyzing top 10 market caps...".to_string(), class: "info".to_string() },
                AgentAction::Wait { ms: 1500 },
                AgentAction::Log { message: "Extracting historical volume data...".to_string(), class: "info".to_string() },
                AgentAction::Wait { ms: 1000 },
                AgentAction::Log { message: "Compiling markdown report...".to_string(), class: "info".to_string() },
                AgentAction::Wait { ms: 800 },
                AgentAction::AgentStatus { agent: "research".to_string(), status: "done".to_string() },
                AgentAction::AgentCard { agent: "research".to_string(), active: false },
                AgentAction::Log { message: "Report generated: market_summary.md".to_string(), class: "ok".to_string() },
                // Notice: no HIL request here since it's just research, but we can fake one to show the UI
                AgentAction::HilRequest {
                    action: "Save Report to Disk".to_string(),
                    flight: "market_summary.md".to_string(),
                    date: "14 KB".to_string(),
                    passenger: "Documents/".to_string(),
                    total: "N/A".to_string(),
                    credentials: "Local FS access".to_string(),
                },
            ]
        } else {
            // Default Flight Sequence
            vec![
                AgentAction::Log { message: format!("agent run \"{}\"", command), class: "cmd".to_string() },
                AgentAction::Log { message: "Initializing agent runtime...".to_string(), class: "info".to_string() },
                AgentAction::Wait { ms: 500 },
                AgentAction::UrlUpdate { url: "https://www.skyscanner.net/".to_string() },
                AgentAction::Log { message: "Vault profile attached as opaque tokens".to_string(), class: "ok".to_string() },
                AgentAction::Wait { ms: 800 },
                AgentAction::TrackerBlocked { label: "doubleclick.net".to_string(), stripped: false },
                AgentAction::Wait { ms: 400 },
                AgentAction::TrackerBlocked { label: "referer stripped -> skyscanner.com".to_string(), stripped: true },
                AgentAction::Wait { ms: 300 },
                AgentAction::AgentCard { agent: "price".to_string(), active: true },
                AgentAction::AgentStatus { agent: "price".to_string(), status: "running".to_string() },
                AgentAction::Log { message: "PriceTracker: loading 3 booking engines in parallel...".to_string(), class: "info".to_string() },
                AgentAction::Wait { ms: 1200 },
                AgentAction::TrackerBlocked { label: "facebook-pixel.com".to_string(), stripped: false },
                AgentAction::Wait { ms: 600 },
                AgentAction::Log { message: "Parsed 47 results - comparing fares".to_string(), class: "info".to_string() },
                AgentAction::Wait { ms: 600 },
                AgentAction::Log { message: "Cheapest: Tue 12 Mar - EUR194 - easyJet EZY 1234".to_string(), class: "ok".to_string() },
                AgentAction::AgentStatus { agent: "price".to_string(), status: "done".to_string() },
                AgentAction::AgentCard { agent: "price".to_string(), active: false },
                AgentAction::Wait { ms: 500 },
                AgentAction::UrlUpdate { url: "https://www.easyjet.com/checkout".to_string() },
                AgentAction::Wait { ms: 800 },
                AgentAction::AgentCard { agent: "form".to_string(), active: true },
                AgentAction::AgentStatus { agent: "form".to_string(), status: "running".to_string() },
                AgentAction::Log { message: "FormFillAgent: reading vault for credentials...".to_string(), class: "info".to_string() },
                AgentAction::Log { message: "Vault returned opaque token - raw credentials never exposed".to_string(), class: "ok".to_string() },
                AgentAction::Wait { ms: 800 },
                AgentAction::Log { message: "HIL GATE - awaiting human approval...".to_string(), class: "warn".to_string() },
                AgentAction::Log { message: "action: book_flight - total: EUR194 - seat: 14C".to_string(), class: "warn".to_string() },
                AgentAction::HilRequest {
                    action: "Book flight ticket".to_string(),
                    flight: "EZY 1234 - LHR -> BER".to_string(),
                    date: "Tue 12 Mar 2025 - 08:30".to_string(),
                    passenger: "Demo User - Seat 14C".to_string(),
                    total: "EUR194.00".to_string(),
                    credentials: "Vault token - never exposed".to_string(),
                },
            ]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_settings_default_not_configured() {
        let settings = AiSettings::default();
        assert!(!settings.is_configured());
    }

    #[test]
    fn test_settings_configured_with_key() {
        let settings = AiSettings {
            api_key: "sk-test-key".to_string(),
            ..Default::default()
        };
        assert!(settings.is_configured());
    }

    #[test]
    fn test_fallback_actions_contain_hil() {
        let brain = AgentBrain::new(AiSettings::default());
        let actions = brain.fallback_actions("test command");

        let has_hil = actions.iter().any(|a| matches!(a, AgentAction::HilRequest { .. }));
        assert!(has_hil, "Fallback actions must include a HIL request");
    }

    #[test]
    fn test_fallback_actions_start_with_command_log() {
        let brain = AgentBrain::new(AiSettings::default());
        let actions = brain.fallback_actions("buy a book");

        match &actions[0] {
            AgentAction::Log { message, class } => {
                assert!(message.contains("buy a book"));
                assert_eq!(class, "cmd");
            }
            _ => panic!("First action should be a log with the command"),
        }
    }

    #[test]
    fn test_agent_action_serialization() {
        let action = AgentAction::Log {
            message: "test".to_string(),
            class: "info".to_string(),
        };
        let json = serde_json::to_string(&action).unwrap();
        assert!(json.contains("\"type\":\"log\""));
        assert!(json.contains("\"message\":\"test\""));
    }

    #[test]
    fn test_agent_action_deserialization() {
        let json = r#"{"type":"tracker_blocked","label":"ads.com","stripped":true}"#;
        let action: AgentAction = serde_json::from_str(json).unwrap();
        match action {
            AgentAction::TrackerBlocked { label, stripped } => {
                assert_eq!(label, "ads.com");
                assert!(stripped);
            }
            _ => panic!("Should deserialize to TrackerBlocked"),
        }
    }
}
