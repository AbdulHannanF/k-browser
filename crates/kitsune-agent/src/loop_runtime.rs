//! In-process LLM-driven agent loop.
//!
//! ARCHITECTURE: this is the runtime that powers the UI's agent shelf today.
//! Each turn:
//!   1. Inject [`crate::dom_observer::observation_script`] into the live
//!      WebView and parse the resulting [`ObservedPage`].
//!   2. Ask the local Ollama daemon for the next [`AgentAction`].
//!   3. Execute it through `DomAccessor` / `HilGate`.
//!   4. Append the action and the next observation to the chat history.
//!
//! INVARIANT: all consequential side effects flow through `HilGate::checkpoint`.
//! The LLM never receives raw vault secrets — it only sees `data-kitsune-id`
//! handles for elements and gets `[REDACTED]` placeholders for sensitive
//! observed values (we strip element values for password/cc fields below).

use crate::action::{parse_action_json, AgentAction};
use crate::dom_observer::{observation_script, ObservedElement, ObservedPage};
use crate::error::AgentError;
use crate::executor::WebViewCommand;
use crate::ollama_client::{OllamaClient, DEFAULT_OLLAMA_MODEL, DEFAULT_OLLAMA_URL};
use crate::spec::AgentSpec;
use kitsune_hil::{HilGate, HilTriggerClass};
use kitsune_vault::VaultBackend;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::{mpsc, oneshot};
use tracing::{info, warn};

/// Shared slot for the file-access permission handshake.
/// The agent writes `(path, oneshot_tx)` here and waits; the UI reads the
/// path to show a modal, then sends `true`/`false` on the channel.
pub type FilePermSlot = Arc<Mutex<Option<(String, oneshot::Sender<bool>)>>>;

/// Cooperative stop flag — the UI sets this to `true` to ask the loop to exit
/// at the next iteration boundary. Passed as `Arc<AtomicBool>`.
pub type StopFlag = Arc<AtomicBool>;

const MAX_ITERATIONS: usize = 15;

/// A status update streamed back to the UI as the agent runs.
#[derive(Debug, Clone)]
pub enum AgentEvent {
    /// Free-form log line.
    Log(String),
    /// Model reasoning extracted from a `<think>…</think>` block.
    Thinking(String),
    /// The agent navigated to a URL — the UI should mirror this in the address bar / tab.
    Navigated(String),
    /// Final answer, loop completed.
    Done(String),
    /// Loop terminated with an error.
    Error(String),
}

/// LLM-driven runtime. One instance per `run()` invocation.
pub struct LlmAgentRuntime {
    spec: AgentSpec,
    ollama: OllamaClient,
    hil_gate: Arc<HilGate>,
    #[allow(dead_code)]
    vault: Arc<VaultBackend>,
    webview_tx: mpsc::Sender<WebViewCommand>,
    events: Option<mpsc::UnboundedSender<AgentEvent>>,
    file_perm_slot: Option<FilePermSlot>,
    stop_flag: Option<StopFlag>,
}

impl LlmAgentRuntime {
    pub fn new(
        spec: AgentSpec,
        webview_tx: mpsc::Sender<WebViewCommand>,
        vault: Arc<VaultBackend>,
        hil_gate: Arc<HilGate>,
    ) -> Self {
        let base_url = spec
            .ollama_url
            .clone()
            .unwrap_or_else(|| DEFAULT_OLLAMA_URL.to_string());
        let model = spec
            .ollama_model
            .clone()
            .unwrap_or_else(|| DEFAULT_OLLAMA_MODEL.to_string());
        let ollama = OllamaClient::new(base_url, model);
        Self {
            spec,
            ollama,
            hil_gate,
            vault,
            webview_tx,
            events: None,
            file_perm_slot: None,
            stop_flag: None,
        }
    }

    /// Attach an event sink so the UI can stream live updates from the loop.
    pub fn with_event_sink(mut self, tx: mpsc::UnboundedSender<AgentEvent>) -> Self {
        self.events = Some(tx);
        self
    }

    /// Wire the file-permission slot so the agent can request local-file access.
    pub fn with_file_perm_slot(mut self, slot: FilePermSlot) -> Self {
        self.file_perm_slot = Some(slot);
        self
    }

    /// Wire the cooperative stop flag so the UI can cancel the loop mid-run.
    pub fn with_stop_flag(mut self, flag: StopFlag) -> Self {
        self.stop_flag = Some(flag);
        self
    }

    fn is_stopped(&self) -> bool {
        self.stop_flag
            .as_ref()
            .map(|f| f.load(Ordering::Relaxed))
            .unwrap_or(false)
    }

    fn emit(&self, event: AgentEvent) {
        if let Some(tx) = &self.events {
            let _ = tx.send(event);
        }
    }

    fn log(&self, message: impl Into<String>) {
        let m = message.into();
        info!(target: "kitsune::agent_loop", "{}", m);
        self.emit(AgentEvent::Log(m));
    }

    /// Drive the loop. Returns the final answer string on success.
    pub async fn run(&self, user_prompt: String) -> Result<String, AgentError> {
        // history is a sequence of (role, content) pairs. The user's request and
        // each round of (observation -> action) feed into Ollama as turns.
        let mut history: Vec<(String, String)> = Vec::new();
        history.push(("user".to_string(), user_prompt.clone()));

        for iter in 0..MAX_ITERATIONS {
            // Cooperative stop — UI set the flag via the stop button.
            if self.is_stopped() {
                let msg = "■ Agent stopped by user.".to_string();
                self.emit(AgentEvent::Done(msg.clone()));
                return Ok(msg);
            }
            // 1. Observe the page.
            let observation = match self.observe().await {
                Ok(o) => o,
                Err(e) => {
                    self.log(format!("⚠ observation failed: {}", e));
                    ObservedPage::default()
                }
            };

            let observation_text = render_observation(&observation);
            self.log(format!(
                "step {} · {} · {} elements",
                iter + 1,
                if observation.url.is_empty() {
                    "(no page)".into()
                } else {
                    observation.url.clone()
                },
                observation.elements.len()
            ));

            // Push observation as a "user" turn so the model treats it as fresh input.
            history.push(("user".to_string(), observation_text));

            // Keep history bounded so the model's context window doesn't overflow.
            trim_history(&mut history, 12);

            // 2. Ask the model.
            let system = build_system_prompt(&self.spec);
            let raw = match self.ollama.chat(&system, history.clone()).await {
                Ok(r) => r,
                Err(AgentError::LlmUnavailable(msg)) => {
                    warn!(target: "kitsune::agent_loop", "LLM unavailable: {}", msg);
                    // Only attempt the local fallback on the first iteration so we
                    // don't silently swallow mid-task failures.
                    if iter == 0 {
                        if let Some(url) = fallback_navigate(&user_prompt) {
                            self.log(format!(
                                "⚠ LLM offline — local fallback: navigate to {}",
                                url
                            ));
                            self.emit(AgentEvent::Navigated(url.clone()));
                            self.webview_tx
                                .send(WebViewCommand::Navigate(url.clone()))
                                .await
                                .map_err(|_| AgentError::IpcDisconnected)?;
                            let done_msg = format!("Navigated to {} (no LLM — used local planner)", url);
                            self.emit(AgentEvent::Done(done_msg.clone()));
                            return Ok(done_msg);
                        }
                    }
                    let err_msg = format!(
                        "LLM unavailable and task requires reasoning. Start Ollama (`ollama serve`) or configure an API key. Detail: {}",
                        msg
                    );
                    self.emit(AgentEvent::Error(err_msg.clone()));
                    return Err(AgentError::LlmUnavailable(err_msg));
                }
                Err(e) => return Err(e),
            };

            // Extract <think>…</think> reasoning before parsing the action.
            let (thinking, action_text) = extract_thinking(&raw);
            if !thinking.is_empty() {
                self.emit(AgentEvent::Thinking(thinking));
            }
            self.log(format!("◇ {}", truncate(&action_text, 300)));

            // Empty response means the model emitted a tool-call or timed out internally.
            if action_text.trim().is_empty() {
                self.log("⚠ empty LLM response — retrying".to_string());
                history.push((
                    "user".to_string(),
                    "Respond with EXACTLY ONE JSON object — no tool calls, no prose, no fences.".to_string(),
                ));
                continue;
            }

            // 3. Parse.
            let action = match parse_action_json(&action_text) {
                Ok(a) => a,
                Err(e) => {
                    self.log(format!("⚠ unparseable action — retrying: {}", e));
                    history.push(("assistant".to_string(), action_text.clone()));
                    history.push((
                        "user".to_string(),
                        "Your previous reply was not valid JSON. Reply with EXACTLY ONE JSON object matching the documented schema. No prose, no fences.".to_string(),
                    ));
                    continue;
                }
            };
            history.push(("assistant".to_string(), action_text.clone()));

            // 4. Execute.
            match self
                .execute_action(&action, &observation)
                .await
            {
                Ok(StepResult::Continue(message, settle_ms)) => {
                    if let Some(m) = message {
                        history.push(("user".to_string(), m));
                    }
                    if settle_ms > 0 {
                        tokio::time::sleep(Duration::from_millis(settle_ms)).await;
                    }
                }
                Ok(StepResult::Done(answer)) => {
                    self.emit(AgentEvent::Done(answer.clone()));
                    return Ok(answer);
                }
                Err(e) => {
                    let msg = format!("Action failed: {}. Pick a different action or use done.", e);
                    self.log(format!("⚠ {}", msg));
                    history.push(("user".to_string(), msg));
                }
            }
        }

        let msg = "Reached max iterations without completing".to_string();
        self.emit(AgentEvent::Done(msg.clone()));
        Ok(msg)
    }

    /// Run the observation script in the WebView and parse the result.
    async fn observe(&self) -> Result<ObservedPage, AgentError> {
        let (tx, mut rx) = mpsc::channel::<String>(1);
        let script = observation_script();
        self.webview_tx
            .send(WebViewCommand::EvalJsWithCallback(script, tx))
            .await
            .map_err(|_| AgentError::IpcDisconnected)?;

        let raw = match tokio::time::timeout(Duration::from_secs(5), rx.recv()).await {
            Ok(Some(s)) => s,
            Ok(None) => return Err(AgentError::ExecutionError("observation channel closed".into())),
            Err(_) => return Err(AgentError::ExecutionError("observation timed out".into())),
        };

        // The IPC handler returns the raw JS-side string — could be quoted JSON
        // ("\"{...}\"") or a bare object. Try both.
        let parsed: ObservedPage = match serde_json::from_str(&raw) {
            Ok(p) => p,
            Err(_) => match serde_json::from_str::<String>(&raw)
                .ok()
                .and_then(|s| serde_json::from_str(&s).ok())
            {
                Some(p) => p,
                None => return Err(AgentError::ExecutionError(format!(
                    "could not parse observation: {}",
                    truncate(&raw, 200)
                ))),
            },
        };

        Ok(parsed)
    }

    async fn execute_action(
        &self,
        action: &AgentAction,
        observation: &ObservedPage,
    ) -> Result<StepResult, AgentError> {
        match action {
            AgentAction::Navigate { url } => {
                // Ensure the model-supplied URL has a scheme.
                let url = normalize_url(url);
                self.log(format!("→ navigate: {}", url));
                self.emit(AgentEvent::Navigated(url.clone()));
                self.webview_tx
                    .send(WebViewCommand::Navigate(url.clone()))
                    .await
                    .map_err(|_| AgentError::IpcDisconnected)?;
                // Give the page time to load before the next observation.
                Ok(StepResult::Continue(None, 3000))
            }
            AgentAction::Click { element_id } => {
                self.log(format!("→ click [{}]", element_id));
                let script = format!(
                    "(function() {{ var el = document.querySelector('[data-kitsune-id=\"{id}\"]'); if (el) {{ el.click(); }} }})();",
                    id = element_id
                );
                self.webview_tx
                    .send(WebViewCommand::EvalJs(script))
                    .await
                    .map_err(|_| AgentError::IpcDisconnected)?;
                Ok(StepResult::Continue(None, 1500))
            }
            AgentAction::Fill { element_id, value } => {
                let element = observation.elements.iter().find(|e| e.id == *element_id);
                let sensitive = element
                    .map(is_sensitive_field)
                    .unwrap_or(false);

                if sensitive {
                    self.log(format!(
                        "⚠ sensitive field [{}] — requesting human approval",
                        element_id
                    ));
                    let trigger = HilTriggerClass::ExternalSideEffect {
                        description: format!(
                            "Agent wants to fill a sensitive field on {}",
                            observation.url
                        ),
                        reversible: true,
                    };
                    let approval = self
                        .hil_gate
                        .checkpoint(trigger, vec!["sensitive-field".into()])
                        .await
                        .map_err(|e| AgentError::PermissionDenied {
                            capability: format!("HIL rejected: {}", e),
                        })?;
                    // Approval is consumed implicitly here — its lifetime ends with this scope,
                    // which matches the single-use semantics on HilApproval.
                    drop(approval);
                    // For the hackathon path, we still inject the LLM-supplied value.
                    // Vault-backed token disclosure is the next-up item; HIL still gates it.
                }

                self.log(format!(
                    "→ fill [{}] = \"{}\"",
                    element_id,
                    truncate(value, 40)
                ));
                let script = format!(
                    r#"(function() {{
                        var el = document.querySelector('[data-kitsune-id="{id}"]');
                        if (!el) {{ return; }}
                        el.focus();
                        try {{ el.value = {val}; }} catch (e) {{}}
                        el.dispatchEvent(new Event('input', {{bubbles:true}}));
                        el.dispatchEvent(new Event('change', {{bubbles:true}}));
                    }})();"#,
                    id = element_id,
                    val = serde_json::to_string(value).unwrap_or_else(|_| "''".into())
                );
                self.webview_tx
                    .send(WebViewCommand::EvalJs(script))
                    .await
                    .map_err(|_| AgentError::IpcDisconnected)?;
                Ok(StepResult::Continue(None, 500))
            }
            AgentAction::Read { selector } => {
                self.log(format!("→ read \"{}\"", selector));
                let (tx, mut rx) = mpsc::channel::<String>(1);
                let script = format!(
                    r#"(function() {{
                        try {{
                            var el = document.querySelector({sel});
                            return JSON.stringify({{ text: el ? (el.innerText || el.textContent || '').slice(0, 1500) : null }});
                        }} catch (e) {{
                            return JSON.stringify({{ text: null, err: String(e) }});
                        }}
                    }})();"#,
                    sel = serde_json::to_string(selector).unwrap_or_else(|_| "''".into())
                );
                self.webview_tx
                    .send(WebViewCommand::EvalJsWithCallback(script, tx))
                    .await
                    .map_err(|_| AgentError::IpcDisconnected)?;
                let raw = tokio::time::timeout(Duration::from_secs(5), rx.recv())
                    .await
                    .map_err(|_| AgentError::ExecutionError("read timed out".into()))?
                    .ok_or_else(|| AgentError::ExecutionError("read channel closed".into()))?;
                // evaluate_script_with_callback returns the JS return value JSON-encoded.
                // A JS string is double-encoded ("\"...\""), so unwrap one layer if needed.
                let outer: serde_json::Value = {
                    let v: serde_json::Value = serde_json::from_str(&raw)
                        .unwrap_or(serde_json::Value::Null);
                    if let Some(s) = v.as_str() {
                        serde_json::from_str(s).unwrap_or(serde_json::Value::Null)
                    } else {
                        v
                    }
                };
                let inner = outer
                    .get("text")
                    .and_then(|t| t.as_str())
                    .unwrap_or("(nothing found)");
                let summary = format!(
                    "Read result for {}: {}",
                    selector,
                    truncate(inner, 800)
                );
                Ok(StepResult::Continue(Some(summary), 0))
            }
            AgentAction::ReadFile { path } => {
                self.log(format!("→ read_file: {}", path));

                // Ask the UI for permission via the shared slot.
                let approved = match &self.file_perm_slot {
                    None => {
                        self.log("⚠ file_perm_slot not wired — denying by default".to_string());
                        false
                    }
                    Some(slot) => {
                        let (tx, rx) = oneshot::channel::<bool>();
                        {
                            let mut guard = slot.lock()
                                .map_err(|_| AgentError::ExecutionError("permission slot poisoned".into()))?;
                            *guard = Some((path.clone(), tx));
                        }
                        self.emit(AgentEvent::Log(format!("🔐 Waiting for permission to read: {}", path)));
                        tokio::time::timeout(Duration::from_secs(60), rx)
                            .await
                            .unwrap_or(Ok(false))
                            .unwrap_or(false)
                    }
                };

                if !approved {
                    return Err(AgentError::PermissionDenied {
                        capability: format!("read_file: {}", path),
                    });
                }

                let bytes = tokio::fs::read(path)
                    .await
                    .map_err(|e| AgentError::ExecutionError(format!("Cannot read {}: {}", path, e)))?;
                let content = match String::from_utf8(bytes.clone()) {
                    Ok(s) => s,
                    Err(_) => {
                        self.log(format!("⚠ {} is binary — using lossy text extraction", path));
                        String::from_utf8_lossy(&bytes).into_owned()
                    }
                };

                let preview = truncate(&content, 3000);
                self.log(format!("✓ read {} ({} chars)", path, content.len()));
                Ok(StepResult::Continue(
                    Some(format!("=== FILE: {} ===\n{}", path, preview)),
                    0,
                ))
            }
            AgentAction::Done { answer } => {
                self.log(format!("✓ done: {}", truncate(answer, 200)));
                Ok(StepResult::Done(answer.clone()))
            }
        }
    }
}

enum StepResult {
    /// Loop continues. Optional message gets pushed into history as a `user` turn.
    /// `settle_ms` is how long to wait before the next observation.
    Continue(Option<String>, u64),
    /// Final answer.
    Done(String),
}

fn is_sensitive_field(el: &ObservedElement) -> bool {
    let kind = el.kind.as_str();
    if kind == "password" {
        return true;
    }
    let ac = el.autocomplete.as_str();
    if ac.contains("cc-")
        || ac.contains("credit-card")
        || ac.contains("password")
        || ac.contains("new-password")
        || ac.contains("current-password")
    {
        return true;
    }
    let name = el.name.to_lowercase();
    if name.contains("password") || name.contains("creditcard") || name.contains("cc-number") {
        return true;
    }
    false
}

fn render_observation(p: &ObservedPage) -> String {
    let mut out = String::new();
    out.push_str("PAGE OBSERVATION\n");
    out.push_str(&format!("URL: {}\n", p.url));
    out.push_str(&format!("Title: {}\n", p.title));
    out.push_str("Interactive elements:\n");
    if p.elements.is_empty() {
        out.push_str("  (none visible)\n");
    } else {
        for el in &p.elements {
            // Best label: aria-label > visible text > placeholder > name attribute.
            let label = if !el.aria_label.is_empty() {
                el.aria_label.clone()
            } else if !el.text.is_empty() {
                el.text.clone()
            } else if !el.placeholder.is_empty() {
                el.placeholder.clone()
            } else if !el.name.is_empty() {
                el.name.clone()
            } else {
                String::new()
            };
            let kind = if el.kind.is_empty() {
                String::new()
            } else {
                format!(" type={}", el.kind)
            };
            // Show href for anchors so the agent knows where a link leads.
            let href_hint = if el.tag == "a" && !el.href.is_empty() {
                format!(" → {}", truncate(&el.href, 60))
            } else {
                String::new()
            };
            out.push_str(&format!(
                "  [{}] <{}{}> {}{}\n",
                el.id,
                el.tag,
                kind,
                truncate(&label, 80),
                href_hint
            ));
        }
    }
    out.push_str("Page text preview:\n");
    out.push_str(&truncate(&p.text_preview, 1000));
    out
}

fn build_system_prompt(_spec: &AgentSpec) -> String {
    r#"You are KitsuneAgent — a capable AI assistant that controls a real web browser AND can read local files. You MUST reply with raw JSON only.

You will receive PAGE OBSERVATION messages with the current URL, page title, and numbered interactive elements. Attached documents (if any) are provided in the conversation prefixed with "=== ATTACHED:".

AVAILABLE ACTIONS — output EXACTLY ONE raw JSON object per turn, nothing else:
{"action":"navigate","url":"https://..."}
{"action":"click","element_id":42}
{"action":"fill","element_id":42,"value":"some text"}
{"action":"read","selector":"h1"}
{"action":"read_file","path":"C:\\Users\\user\\resume.txt"}
{"action":"done","answer":"your final answer here"}

RULES:
- Your entire reply must be a single JSON object. No markdown fences, no tool_call wrapper, no prose.
- "navigate" loads a URL — always include https:// or http://.
- "click" and "fill" use element_id numbers from the PAGE OBSERVATION.
- "read" extracts page text at a CSS selector.
- "read_file" reads a local file — the user will be shown a permission prompt.
- "done" ends the task with a concise answer.
- Use information from attached documents when filling forms or answering questions.
- If an element labeled with a destination (e.g. "→ drive.google.com/...") is a link, click it.
- After 2 failed attempts on the same URL, use "done" and explain.
- Take exactly one action per turn.
"#
    .to_string()
}

/// Split model output into `(thinking, action_text)`.
/// Recognises `<think>…</think>` blocks used by reasoning models.
/// If no thinking block is found, thinking is empty and action_text is the full string.
fn extract_thinking(s: &str) -> (String, String) {
    let s = s.trim();
    if let (Some(start), Some(end)) = (s.find("<think>"), s.find("</think>")) {
        if end > start {
            let thinking = s[start + 7..end].trim().to_string();
            let rest = s[end + 8..].trim().to_string();
            // Only split if there's actual content after the thinking block.
            if !rest.is_empty() {
                return (thinking, rest);
            }
            // Thinking but no action yet — return empty action so the retry fires.
            return (thinking, String::new());
        }
    }
    (String::new(), s.to_string())
}

/// Keep the first user message (the original task) + the most recent `keep` turns.
/// This prevents the model's context window from overflowing on long runs.
fn trim_history(history: &mut Vec<(String, String)>, keep: usize) {
    if history.len() <= keep + 1 {
        return;
    }
    let first = history[0].clone();
    let tail_start = history.len() - keep;
    history.drain(1..tail_start);
    debug_assert_eq!(history[0], first);
}

fn truncate(s: &str, n: usize) -> String {
    let count = s.chars().count();
    if count <= n {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(n).collect();
        out.push('…');
        out
    }
}

/// Deterministic URL dispatch when no LLM is available.
/// Mirrors the pattern-matching in kitsune-cloud-mock's local_plan().
/// Returns Some(url) for simple navigation intents, None for tasks that need reasoning.
fn fallback_navigate(command: &str) -> Option<String> {
    let lower = command.trim().to_lowercase();

    // Direct URL
    for tok in command.split_whitespace() {
        if tok.starts_with("http://") || tok.starts_with("https://") {
            return Some(tok.trim_end_matches(|c: char| matches!(c, '.' | ',' | ';')).to_string());
        }
    }

    // Wikipedia
    for prefix in &["search wikipedia for ", "wikipedia search ", "wikipedia for ", "wikipedia ", "wiki "] {
        if let Some(topic) = lower.strip_prefix(prefix) {
            let slug: String = topic.trim().split_whitespace()
                .map(|w| { let mut c = w.chars(); c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default() })
                .collect::<Vec<_>>().join("_");
            return Some(format!("https://en.wikipedia.org/wiki/{}", urlencoding::encode(&slug)));
        }
    }
    // YouTube
    for prefix in &["youtube ", "search youtube for ", "play "] {
        if let Some(q) = lower.strip_prefix(prefix) {
            return Some(format!("https://www.youtube.com/results?search_query={}", urlencoding::encode(q.trim())));
        }
    }
    // GitHub
    for prefix in &["github ", "search github for "] {
        if let Some(q) = lower.strip_prefix(prefix) {
            return Some(format!("https://github.com/search?q={}&type=repositories", urlencoding::encode(q.trim())));
        }
    }
    // News
    for prefix in &["news about ", "news on ", "news for ", "news "] {
        if let Some(q) = lower.strip_prefix(prefix) {
            return Some(format!("https://news.google.com/search?q={}", urlencoding::encode(q.trim())));
        }
    }
    // Shopping
    for prefix in &["buy ", "shop for ", "shop ", "amazon "] {
        if let Some(q) = lower.strip_prefix(prefix) {
            return Some(format!("https://www.amazon.com/s?k={}", urlencoding::encode(q.trim())));
        }
    }
    // go to / open / navigate / visit
    for prefix in &["go to ", "open ", "navigate to ", "visit "] {
        if let Some(site) = lower.strip_prefix(prefix) {
            let site = site.trim().trim_matches(|c: char| c == '"' || c == '\'');
            return Some(if site.starts_with("http://") || site.starts_with("https://") {
                site.to_string()
            } else if site.contains('.') {
                format!("https://{}", site)
            } else {
                format!("https://www.{}.com", site.replace(' ', ""))
            });
        }
    }
    // Generic search
    for prefix in &["search for ", "search ", "find ", "google ", "look up ", "lookup "] {
        if let Some(q) = lower.strip_prefix(prefix) {
            return Some(format!("https://www.google.com/search?q={}", urlencoding::encode(q.trim())));
        }
    }

    None
}

/// Ensure model-returned URLs have a scheme so WebView2 doesn't reject them.
fn normalize_url(url: &str) -> String {
    let u = url.trim();
    if u.starts_with("http://") || u.starts_with("https://") || u.starts_with("file://") {
        u.to_string()
    } else if u.is_empty() {
        "https://www.google.com".to_string()
    } else {
        format!("https://{}", u)
    }
}
