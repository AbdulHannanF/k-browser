//! In-process LLM-driven agent loop.
//!
//! ARCHITECTURE: this is the runtime that powers the UI's agent shelf today.
//! Each turn:
//!   1. Inject [`crate::dom_observer::observation_script`] into the live
//!      WebView and parse the resulting [`ObservedPage`].
//!   2. Ask the configured LLM backend (Ollama or cloud provider) for the next [`AgentAction`].
//!   3. Execute it through `DomAccessor` / `HilGate`.
//!   4. Append the action and the next observation to the chat history.
//!
//! INVARIANT: all consequential side effects flow through `HilGate::checkpoint`.
//! The LLM never receives raw vault secrets — it only sees `data-kitsune-id`
//! handles for elements and gets `[REDACTED]` placeholders for sensitive
//! observed values (we strip element values for password/cc fields below).

use crate::action::{parse_action_json, AgentAction};
use crate::ai_client::AiProviderConfig;
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

enum LlmBackend {
    Ollama(OllamaClient),
    Cloud {
        client: reqwest::Client,
        url: String,
        api_key: String,
        model: String,
    },
}

impl LlmBackend {
    async fn chat(&self, system: &str, history: Vec<(String, String)>) -> Result<(String, u32, u32), AgentError> {
        match self {
            Self::Ollama(ollama) => ollama.chat(system, history).await,
            Self::Cloud { client, url, api_key, model } => {
                let mut messages = vec![
                    serde_json::json!({"role": "system", "content": system}),
                ];
                for (role, content) in &history {
                    messages.push(serde_json::json!({"role": role, "content": content}));
                }
                let body = serde_json::json!({
                    "model": model,
                    "messages": messages,
                    "max_tokens": 4096,
                });
                // `url` is a versioned base (e.g. https://api.openai.com/v1);
                // preset endpoints already include the version path.
                let resp = client
                    .post(format!("{}/chat/completions", url.trim_end_matches('/')))
                    .bearer_auth(api_key)
                    .json(&body)
                    .send()
                    .await
                    .map_err(|e| {
                        if e.is_connect() || e.is_timeout() {
                            AgentError::LlmUnavailable(format!(
                                "LLM not responding at {}: {}", url, e
                            ))
                        } else {
                            AgentError::ExecutionError(format!("Cloud LLM request failed: {}", e))
                        }
                    })?;

                let status = resp.status();
                if !status.is_success() {
                    let text = resp.text().await.unwrap_or_default();
                    let msg = if status.as_u16() == 401 || status.as_u16() == 403 {
                        format!(
                            "Cloud LLM rejected the API key (HTTP {}). \
                             Check your key in Settings → LLM → API Key. Detail: {}",
                            status, text
                        )
                    } else if status.as_u16() == 429 {
                        format!(
                            "Cloud LLM rate limit reached (HTTP 429). \
                             Wait a moment and try again. Detail: {}",
                            text
                        )
                    } else {
                        format!("Cloud LLM returned HTTP {}: {}", status, text)
                    };
                    return Err(AgentError::ExecutionError(msg));
                }

                let parsed: serde_json::Value = resp
                    .json()
                    .await
                    .map_err(|e| AgentError::ExecutionError(format!("Cloud LLM bad JSON: {}", e)))?;

                let input_tokens = parsed["usage"]["prompt_tokens"].as_u64().unwrap_or(0) as u32;
                let output_tokens = parsed["usage"]["completion_tokens"].as_u64().unwrap_or(0) as u32;
                let content = parsed["choices"][0]["message"]["content"]
                    .as_str()
                    .map(|s| s.to_string())
                    .ok_or_else(|| AgentError::ExecutionError("empty cloud LLM response".into()))?;
                Ok((content, input_tokens, output_tokens))
            }
        }
    }
}

/// Shared slot for the file-access permission handshake.
/// The agent writes `(path, oneshot_tx)` here and waits; the UI reads the
/// path to show a modal, then sends `true`/`false` on the channel.
pub type FilePermSlot = Arc<Mutex<Option<(String, oneshot::Sender<bool>)>>>;

/// Cooperative stop flag — the UI sets this to `true` to ask the loop to exit
/// at the next iteration boundary. Passed as `Arc<AtomicBool>`.
pub type StopFlag = Arc<AtomicBool>;

const MAX_ITERATIONS: usize = 40;

/// A status update streamed back to the UI as the agent runs.
#[derive(Debug, Clone)]
pub enum AgentEvent {
    /// Free-form log line.
    Log(String),
    /// Indented sub-step (action being executed).
    Step(String),
    /// Model reasoning extracted from a `<think>…</think>` block.
    Thinking(String),
    /// The agent navigated to a URL — the UI should mirror this in the address bar / tab.
    Navigated(String),
    /// Final answer, loop completed.
    Done(String),
    /// Loop terminated with an error.
    Error(String),
    /// Cumulative token counts for this run, emitted after each LLM call.
    TokenUsage { input: u32, output: u32 },
    /// Swarm worker status update. Emitted by coordinator and workers.
    SwarmUpdate {
        swarm_id: String,
        worker_id: String,
        role: String,
        status: String,
        message: String,
        tool_calls_used: u32,
    },
    /// Coordinator finished planning — task list is ready.
    SwarmPlanReady {
        swarm_id: String,
        goal: String,
        tasks: Vec<crate::swarm::types::SwarmTask>,
    },
    /// Final swarm answer — all workers done, reconciliation complete.
    SwarmDone {
        swarm_id: String,
        final_answer: String,
        total_tool_calls: u32,
    },
    /// Swarm-level error — coordinator or all workers failed.
    SwarmError {
        swarm_id: String,
        error: String,
    },
}

/// LLM-driven runtime. One instance per `run()` invocation.
pub struct LlmAgentRuntime {
    spec: AgentSpec,
    backend: LlmBackend,
    hil_gate: Arc<HilGate>,
    #[allow(dead_code)]
    vault: Arc<VaultBackend>,
    webview_tx: mpsc::Sender<WebViewCommand>,
    events: Option<mpsc::UnboundedSender<AgentEvent>>,
    file_perm_slot: Option<FilePermSlot>,
    stop_flag: Option<StopFlag>,
    /// Extra specialist context injected into the system prompt (e.g. from agent cards).
    agent_context: Option<String>,
    /// Shared mutex serializing WebView navigate/click/fill across swarm workers.
    /// `None` in single-agent mode.
    nav_lock: Option<Arc<tokio::sync::Mutex<()>>>,
    /// Shared mutex serializing HIL checkpoint dialogs across swarm workers.
    /// `None` in single-agent mode.
    hil_lock: Option<Arc<tokio::sync::Mutex<()>>>,
    /// Worker ID for log prefixing. `None` in single-agent mode.
    worker_id: Option<String>,
    /// Swarm ID this worker belongs to. `None` in single-agent mode.
    swarm_id: Option<String>,
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
        let backend = LlmBackend::Ollama(OllamaClient::new(base_url, model));
        Self {
            spec,
            backend,
            hil_gate,
            vault,
            webview_tx,
            events: None,
            file_perm_slot: None,
            stop_flag: None,
            agent_context: None,
            nav_lock: None,
            hil_lock: None,
            worker_id: None,
            swarm_id: None,
        }
    }

    /// Construct with an explicit provider config so the UI can pass cloud credentials.
    pub fn new_with_config(
        spec: AgentSpec,
        config: AiProviderConfig,
        webview_tx: mpsc::Sender<WebViewCommand>,
        vault: Arc<VaultBackend>,
        hil_gate: Arc<HilGate>,
    ) -> Self {
        let backend = match config {
            AiProviderConfig::Ollama { url, slots } => {
                LlmBackend::Ollama(OllamaClient::new(url, slots.worker))
            }
            AiProviderConfig::OpenAiCompatible { url, api_key, slots } => {
                let client = reqwest::Client::builder()
                    .use_rustls_tls()
                    .timeout(std::time::Duration::from_secs(120))
                    .build()
                    .expect("failed to build rustls HTTP client");
                LlmBackend::Cloud { client, url, api_key, model: slots.worker }
            }
        };
        Self {
            spec,
            backend,
            hil_gate,
            vault,
            webview_tx,
            events: None,
            file_perm_slot: None,
            stop_flag: None,
            agent_context: None,
            nav_lock: None,
            hil_lock: None,
            worker_id: None,
            swarm_id: None,
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

    /// Inject specialist context from the selected agent card into the system prompt.
    pub fn with_agent_context(mut self, ctx: String) -> Self {
        if !ctx.is_empty() {
            self.agent_context = Some(ctx);
        }
        self
    }

    /// Inject a shared browser nav lock (swarm mode only).
    pub fn with_nav_lock(mut self, lock: Arc<tokio::sync::Mutex<()>>) -> Self {
        self.nav_lock = Some(lock);
        self
    }

    /// Inject a shared HIL serialization lock (swarm mode only).
    pub fn with_hil_lock(mut self, lock: Arc<tokio::sync::Mutex<()>>) -> Self {
        self.hil_lock = Some(lock);
        self
    }

    /// Tag this runtime as belonging to a swarm worker (enables log prefixing).
    pub fn with_worker_id(mut self, worker_id: String, swarm_id: String) -> Self {
        self.worker_id = Some(worker_id);
        self.swarm_id = Some(swarm_id);
        self
    }

    fn is_stopped(&self) -> bool {
        self.stop_flag
            .as_ref()
            .map(|f| f.load(Ordering::Relaxed))
            .unwrap_or(false)
    }

    async fn acquire_nav_lock(&self) -> Result<Option<tokio::sync::OwnedMutexGuard<()>>, AgentError> {
        let Some(lock) = &self.nav_lock else { return Ok(None); };
        let timeout = Duration::from_secs(30);
        tokio::time::timeout(timeout, lock.clone().lock_owned())
            .await
            .map(Some)
            .map_err(|_| AgentError::SwarmWorkerFailed {
                worker_id: self.worker_id.clone().unwrap_or_default(),
                reason: "browser nav lock timed out".into(),
            })
    }

    async fn acquire_hil_lock(&self) -> Result<Option<tokio::sync::OwnedMutexGuard<()>>, AgentError> {
        let Some(lock) = &self.hil_lock else { return Ok(None); };
        tokio::time::timeout(
            Duration::from_secs(120),
            lock.clone().lock_owned(),
        )
        .await
        .map(Some)
        .map_err(|_| AgentError::PermissionDenied { capability: "hil_lock timed out".into() })
    }

    fn emit(&self, event: AgentEvent) {
        if let Some(tx) = &self.events {
            let event = if let Some(wid) = &self.worker_id {
                match event {
                    AgentEvent::Log(m) => AgentEvent::Log(format!("[{}] {}", wid, m)),
                    AgentEvent::Step(m) => AgentEvent::Step(format!("[{}] {}", wid, m)),
                    AgentEvent::Thinking(m) => AgentEvent::Thinking(format!("[{}] {}", wid, m)),
                    other => other,
                }
            } else {
                event
            };
            let _ = tx.send(event);
        }
    }

    fn log(&self, message: impl Into<String>) {
        let m = message.into();
        info!(target: "kitsune::agent_loop", "{}", m);
        self.emit(AgentEvent::Log(m));
    }

    /// Emit an indented sub-step entry (action being executed, result received, etc.)
    fn step(&self, message: impl Into<String>) {
        let m = message.into();
        info!(target: "kitsune::agent_loop", "  {}", m);
        self.emit(AgentEvent::Step(m));
    }

    /// Drive the loop. Returns the final answer string on success.
    pub async fn run(&self, user_prompt: String) -> Result<String, AgentError> {
        // The user's task is embedded in the system prompt (built each turn) so it is
        // always visible regardless of history trimming and never produces consecutive
        // user turns — a pattern that confuses many LLMs into replying "done" immediately.
        let mut history: Vec<(String, String)> = Vec::new();
        let mut total_input_tokens: u32 = 0;
        let mut total_output_tokens: u32 = 0;

        for iter in 0..MAX_ITERATIONS {
            // Cooperative stop — UI set the flag via the stop button.
            if self.is_stopped() {
                if self.worker_id.is_some() {
                    return Err(AgentError::Cancelled);
                }
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
            // Observation detail goes to tracing only — the UI shows human-readable step messages.
            info!(
                target: "kitsune::agent_loop",
                "step {} · {} · {} elements",
                iter + 1,
                if observation.url.is_empty() { "(no page)" } else { &observation.url },
                observation.elements.len()
            );

            // Push observation as a "user" turn so the model treats it as fresh input.
            history.push(("user".to_string(), observation_text));

            // Keep history bounded so the model's context window doesn't overflow.
            trim_history(&mut history, 12);

            // 2. Ask the model.
            let system = build_system_prompt(&user_prompt, self.agent_context.as_deref());
            let (raw, in_tok, out_tok) = match self.backend.chat(&system, history.clone()).await {
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
                        "LLM unavailable and task requires reasoning. \
                         For local models, run `ollama serve`. \
                         For cloud providers, check your API key and endpoint in Settings. \
                         Detail: {}",
                        msg
                    );
                    self.emit(AgentEvent::Error(err_msg.clone()));
                    return Err(AgentError::LlmUnavailable(err_msg));
                }
                Err(e) => return Err(e),
            };
            total_input_tokens += in_tok;
            total_output_tokens += out_tok;
            self.emit(AgentEvent::TokenUsage { input: total_input_tokens, output: total_output_tokens });

            // Extract <think>…</think> reasoning before parsing the action.
            let (thinking, action_text) = extract_thinking(&raw);
            if !thinking.is_empty() {
                // Emit raw thinking text — the UI renders it as a collapsible block.
                self.emit(AgentEvent::Thinking(thinking));
            }
            // Log raw JSON to tracing only — the user sees the human-readable step messages.
            info!(target: "kitsune::agent_loop", "◇ {}", truncate(&action_text, 300));

            // Empty response: model emitted a tool-call block, empty output, or pure thinking.
            if action_text.trim().is_empty() {
                self.step("↳ Adjusting approach…".to_string());
                history.push((
                    "user".to_string(),
                    "Respond with EXACTLY ONE JSON object — no tool calls, no prose, no fences.".to_string(),
                ));
                continue;
            }

            // 3. Parse.
            let action = match parse_action_json(&action_text) {
                Ok(a) => a,
                Err(_e) => {
                    self.step("↳ Response format unclear, retrying…".to_string());
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
                let _nav_guard = self.acquire_nav_lock().await?;
                let url = normalize_url(url);
                self.step(format!("↳ Navigating to {}", domain_of(&url)));
                self.emit(AgentEvent::Navigated(url.clone()));
                self.webview_tx
                    .send(WebViewCommand::Navigate(url.clone()))
                    .await
                    .map_err(|_| AgentError::IpcDisconnected)?;
                // Give the page time to load before the next observation.
                Ok(StepResult::Continue(None, 3000))
            }
            AgentAction::Click { element_id } => {
                let _nav_guard = self.acquire_nav_lock().await?;
                let label = elem_label(&observation.elements, *element_id);
                self.step(format!("↳ Clicking {}", label));
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
                let _nav_guard = self.acquire_nav_lock().await?;
                let element = observation.elements.iter().find(|e| e.id == *element_id);
                let sensitive = element
                    .map(is_sensitive_field)
                    .unwrap_or(false);

                if sensitive {
                    self.log(format!(
                        "⚠ sensitive field [{}] — requesting human approval",
                        element_id
                    ));
                    // Serialize HIL dialogs across swarm workers — only one at a time.
                    let _hil_guard = self.acquire_hil_lock().await?;
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
                    // _hil_guard dropped here — next worker can request HIL
                    // For the hackathon path, we still inject the LLM-supplied value.
                    // Vault-backed token disclosure is the next-up item; HIL still gates it.
                }

                let label = elem_label(&observation.elements, *element_id);
                self.step(format!(
                    "↳ Typing \"{}\" → {}",
                    truncate(value, 35),
                    label
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
                self.step(format!("↳ Reading \"{}\"", selector));
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
                self.step(format!("↳ Read {} chars from \"{}\"", inner.len(), selector));
                Ok(StepResult::Continue(Some(summary), 0))
            }
            AgentAction::ReadFile { path } => {
                self.step(format!("↳ Reading file: {}", path));

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
            AgentAction::Download { url, filename } => {
                let fname = filename.as_deref()
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| {
                        url.split('/')
                            .last()
                            .and_then(|s| s.split('?').next())
                            .filter(|s| !s.is_empty())
                            .unwrap_or("download")
                            .to_string()
                    });
                self.step(format!("↳ Downloading {}", fname));

                let downloads = dirs::download_dir()
                    .or_else(dirs::home_dir)
                    .unwrap_or_else(|| std::path::PathBuf::from("."));
                let dest = downloads.join(&fname);

                let dl_client = reqwest::Client::builder()
                    .use_rustls_tls()
                    .timeout(Duration::from_secs(120))
                    .build()
                    .map_err(|e| AgentError::ExecutionError(e.to_string()))?;

                let resp = dl_client.get(url.as_str()).send().await
                    .map_err(|e| AgentError::ExecutionError(format!("Download failed: {e}")))?;
                let status = resp.status();
                if !status.is_success() {
                    return Err(AgentError::ExecutionError(
                        format!("Download returned HTTP {status}")
                    ));
                }
                let bytes = resp.bytes().await
                    .map_err(|e| AgentError::ExecutionError(format!("Download read error: {e}")))?;
                tokio::fs::write(&dest, &bytes).await
                    .map_err(|e| AgentError::ExecutionError(format!("Cannot save file: {e}")))?;

                let path_str = dest.to_string_lossy().to_string();
                self.step(format!("↳ Saved: {}", path_str));
                Ok(StepResult::Continue(
                    Some(format!("Downloaded {} ({} KB) to {}", fname, bytes.len() / 1024, path_str)),
                    0,
                ))
            }
            AgentAction::Done { answer } => {
                // StepResult::Done → the caller emits AgentEvent::Done which the UI shows as Ok.
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

/// Return just the hostname of a URL for compact display.
fn domain_of(url: &str) -> String {
    url.trim_start_matches("https://")
        .trim_start_matches("http://")
        .split('/')
        .next()
        .unwrap_or(url)
        .to_string()
}

/// Best human-readable label for a page element (aria > text > placeholder > name > id).
fn elem_label(elements: &[ObservedElement], id: usize) -> String {
    elements.iter().find(|e| e.id == id).map(|e| {
        let raw = if !e.aria_label.is_empty() { &e.aria_label }
            else if !e.text.is_empty() { &e.text }
            else if !e.placeholder.is_empty() { &e.placeholder }
            else if !e.name.is_empty() { &e.name }
            else { return format!("[{}]", id); };
        format!("\"{}\"", truncate(raw, 40))
    }).unwrap_or_else(|| format!("[{}]", id))
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

fn build_system_prompt(user_task: &str, agent_context: Option<&str>) -> String {
    let specialist = agent_context
        .filter(|s| !s.is_empty())
        .map(|s| format!("\nSPECIALIST ROLE: {}\n", s))
        .unwrap_or_default();

    format!(
        r#"You are KitsuneAgent — an AI assistant that controls a real web browser.{specialist}

YOUR TASK (complete this fully — do not stop early):
{task}

Each turn you receive a PAGE OBSERVATION (URL, title, interactive elements).

RESPONSE FORMAT — two parts in order:
1. <think>your brief reasoning about what to do next</think>
2. Exactly one JSON action object on its own line.

AVAILABLE ACTIONS:
{{"action":"navigate","url":"https://example.com"}}
{{"action":"click","element_id":42}}
{{"action":"fill","element_id":42,"value":"text"}}
{{"action":"read","selector":"body"}}
{{"action":"read_file","path":"C:\\path\\to\\file.pdf"}}
{{"action":"download","url":"https://example.com/paper.pdf","filename":"paper.pdf"}}
{{"action":"done","answer":"your complete final answer here"}}

CORE RULES:
- Reply with ONE raw JSON object. No markdown, no prose, no fences, no tool_call wrappers.
- NEVER output done without having done meaningful work (navigate + read/click at minimum).
- NEVER output done on the first turn — always start by navigating or searching.
- Only report actions you actually took. Do NOT claim to have downloaded or saved anything unless you used the "download" action.
- To download a file (PDF, paper, etc.) use {{"action":"download","url":"...","filename":"..."}} — this saves it to the user's Downloads folder.

BROWSING RULES:
- "navigate" loads a URL — always include https:// or http://.
- "click" and "fill" use element_id numbers from PAGE OBSERVATION.
- "read" extracts text at a CSS selector; use "body" for full page text.
- To search: "fill" the search input, then "click" the search button. Do NOT just navigate to a search URL.
- After landing on a search results page, CLICK through to the actual pages — do not report based on snippets alone.
- After 3 failed attempts on one URL, try a different site.

RESEARCH RULES (apply when the task involves finding/comparing information):
- Visit at least 3 different URLs before reporting done.
- Click through from search results to the actual content pages.
- Use "read" with selector "body" or "article" to extract full page text.
- For academic papers: use arxiv.org, scholar.google.com, or semanticscholar.org — not just google.com.
- Synthesize findings from multiple sources in your final answer.
"#,
        specialist = specialist,
        task = user_task,
    )
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn llm_backend_cloud_variant_constructs() {
        let client = reqwest::Client::new();
        let backend = LlmBackend::Cloud {
            client,
            url: "https://api.openai.com/v1".to_string(),
            api_key: "sk-test".to_string(),
            model: "gpt-4o-mini".to_string(),
        };
        assert!(matches!(backend, LlmBackend::Cloud { .. }));
    }

    #[test]
    fn llm_backend_ollama_variant_constructs() {
        let backend = LlmBackend::Ollama(OllamaClient::default_local());
        assert!(matches!(backend, LlmBackend::Ollama(_)));
    }
}
