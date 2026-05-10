# Universal API Provider & UI Fixes — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add named cloud-provider presets (Claude, OpenAI, Gemini, Groq, OpenRouter, Custom) to the LLM settings dialog and fix the runtime bug where cloud credentials were ignored by the in-process agent loop.

**Architecture:** A `CloudPreset` enum holds per-provider defaults (endpoint, model, key hint). The settings dialog gains a preset button row below the Cloud/Ollama radio toggle. `LlmAgentRuntime` gets a `LlmBackend` enum that wraps either `OllamaClient` or an inline OpenAI-compatible chat client, selected by a new `new_with_config` constructor. `agent_panel.rs` builds an `AiProviderConfig` from live settings and passes it through.

**Tech Stack:** Rust, egui 0.30, reqwest (rustls), serde_json — all already in the workspace.

---

## File Map

| File | Role after change |
|---|---|
| `crates/kitsune-ui/src/app.rs` | Owns `CloudPreset` enum + methods; `SettingsProvider::OpenAiCompatible` renamed `Cloud`; `settings_cloud_preset` field added to `KitsuneBrowser` |
| `crates/kitsune-ui/src/dialogs/settings_dialog.rs` | Preset button row; updated hints/descriptions; Agents tab label fix |
| `crates/kitsune-ui/src/panels/agent_panel.rs` | Builds `AiProviderConfig` from browser settings; calls `new_with_config` |
| `crates/kitsune-agent/src/loop_runtime.rs` | `LlmBackend` enum + `chat()`; `LlmAgentRuntime` struct uses `backend` not `ollama`; `new_with_config` added |

No changes to: `kitsune-cloud-mock`, `kitsune-ai`, `ai_client.rs`, `ollama_client.rs`.

---

## Task 1: Add `CloudPreset` + rename `SettingsProvider::Cloud` in `app.rs`

**Files:**
- Modify: `crates/kitsune-ui/src/app.rs:237-250` (SettingsProvider enum + wire_value)
- Modify: `crates/kitsune-ui/src/app.rs:188,383-388` (struct field + new() init)

> **Note:** After this task the codebase won't compile — `settings_dialog.rs` and `agent_panel.rs` still reference `OpenAiCompatible`. That's expected and fixed in Tasks 2 and 4.

- [ ] **Step 1: Add `CloudPreset` enum with methods after `SettingsProvider`**

In `crates/kitsune-ui/src/app.rs`, find the block starting at line 237:
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsProvider {
    OpenAiCompatible,
    Ollama,
}
```
Replace with:
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsProvider {
    Cloud,
    Ollama,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CloudPreset {
    #[default]
    Claude,
    OpenAI,
    Gemini,
    Groq,
    OpenRouter,
    Custom,
}

impl CloudPreset {
    pub fn label(self) -> &'static str {
        match self {
            Self::Claude => "Claude",
            Self::OpenAI => "OpenAI",
            Self::Gemini => "Gemini",
            Self::Groq => "Groq",
            Self::OpenRouter => "OpenRouter",
            Self::Custom => "Custom",
        }
    }

    pub fn default_endpoint(self) -> &'static str {
        match self {
            Self::Claude => "https://api.anthropic.com/v1",
            Self::OpenAI => "https://api.openai.com/v1",
            Self::Gemini => "https://generativelanguage.googleapis.com/v1beta/openai",
            Self::Groq => "https://api.groq.com/openai/v1",
            Self::OpenRouter => "https://openrouter.ai/api/v1",
            Self::Custom => "",
        }
    }

    pub fn default_model(self) -> &'static str {
        match self {
            Self::Claude => "claude-3-5-sonnet-20241022",
            Self::OpenAI => "gpt-4o-mini",
            Self::Gemini => "gemini-2.0-flash",
            Self::Groq => "llama-3.3-70b-versatile",
            Self::OpenRouter => "anthropic/claude-3.5-sonnet",
            Self::Custom => "",
        }
    }

    pub fn key_hint(self) -> &'static str {
        match self {
            Self::Claude => "sk-ant-api03-…",
            Self::OpenAI => "sk-…",
            Self::Gemini => "AIza…",
            Self::Groq => "gsk_…",
            Self::OpenRouter => "sk-or-…",
            Self::Custom => "your-api-key",
        }
    }
}
```

- [ ] **Step 2: Update `SettingsProvider::wire_value()`**

Find (lines ~243-250):
```rust
impl SettingsProvider {
    pub fn wire_value(&self) -> &'static str {
        match self {
            Self::OpenAiCompatible => "open_ai_compatible",
            Self::Ollama => "ollama",
        }
    }
}
```
Replace with:
```rust
impl SettingsProvider {
    pub fn wire_value(&self) -> &'static str {
        match self {
            Self::Cloud => "open_ai_compatible",
            Self::Ollama => "ollama",
        }
    }
}
```

- [ ] **Step 3: Add `settings_cloud_preset` field to `KitsuneBrowser` struct**

In the struct definition (around line 186-205), find the settings block:
```rust
    // Settings state
    pub show_settings: bool,
    pub settings_tab: SettingsTab,
    pub settings_provider: SettingsProvider,
    pub settings_api_key: String,
    pub settings_endpoint: String,
    pub settings_model: String,
    pub settings_saved: bool,
    pub settings_test_status: Option<String>,
```
Replace with:
```rust
    // Settings state
    pub show_settings: bool,
    pub settings_tab: SettingsTab,
    pub settings_provider: SettingsProvider,
    pub settings_cloud_preset: CloudPreset,
    pub settings_api_key: String,
    pub settings_endpoint: String,
    pub settings_model: String,
    pub settings_saved: bool,
    pub settings_test_status: Option<String>,
```

- [ ] **Step 4: Initialize the new field in `KitsuneBrowser::new()`**

Find the `Self { ... }` block in `new()` (around line 357). Locate:
```rust
            settings_provider: SettingsProvider::Ollama,
            settings_api_key: String::new(),
```
Replace with:
```rust
            settings_provider: SettingsProvider::Ollama,
            settings_cloud_preset: CloudPreset::default(),
            settings_api_key: String::new(),
```

- [ ] **Step 5: Write unit tests for `CloudPreset`**

At the bottom of `crates/kitsune-ui/src/app.rs`, before the closing `}` of the existing `#[cfg(test)] mod tests` block, add:
```rust
    #[test]
    fn cloud_preset_named_endpoints_use_https() {
        for preset in [
            CloudPreset::Claude,
            CloudPreset::OpenAI,
            CloudPreset::Gemini,
            CloudPreset::Groq,
            CloudPreset::OpenRouter,
        ] {
            assert!(
                preset.default_endpoint().starts_with("https://"),
                "{:?} should use HTTPS",
                preset
            );
        }
    }

    #[test]
    fn cloud_preset_custom_has_empty_endpoint() {
        assert_eq!(CloudPreset::Custom.default_endpoint(), "");
    }

    #[test]
    fn cloud_preset_key_hints_are_nonempty() {
        for preset in [
            CloudPreset::Claude,
            CloudPreset::OpenAI,
            CloudPreset::Gemini,
            CloudPreset::Groq,
            CloudPreset::OpenRouter,
            CloudPreset::Custom,
        ] {
            assert!(!preset.key_hint().is_empty(), "{:?} must have a key hint", preset);
        }
    }
```

- [ ] **Step 6: Commit**

```powershell
git add crates/kitsune-ui/src/app.rs
git commit -m "feat(ui): add CloudPreset enum and rename SettingsProvider::Cloud"
```

---

## Task 2: Update `settings_dialog.rs` — preset row, hints, label fixes

**Files:**
- Modify: `crates/kitsune-ui/src/dialogs/settings_dialog.rs`

- [ ] **Step 1: Fix the import and radio button for the Cloud variant**

At line 1, the file imports `SettingsProvider` and `SettingsTab` from `app`. Add `CloudPreset` to that import:
```rust
use crate::app::{CloudPreset, KitsuneBrowser, SettingsProvider, SettingsTab};
```

Find the radio button block (around line 96-108):
```rust
        ui.radio_value(
            &mut browser.settings_provider,
            SettingsProvider::OpenAiCompatible,
            "OpenAI-compatible API",
        );
        ui.add_space(8.0);
        ui.radio_value(
            &mut browser.settings_provider,
            SettingsProvider::Ollama,
            "Local LLM (Ollama)",
        );
        if browser.settings_provider != prev {
            match browser.settings_provider {
                SettingsProvider::OpenAiCompatible => {
                    browser.settings_endpoint =
                        "https://api.openai.com/v1/chat/completions".to_string();
                    if browser.settings_model.is_empty() {
                        browser.settings_model = "gpt-4o-mini".to_string();
                    }
                }
                SettingsProvider::Ollama => {
                    browser.settings_endpoint = "http://localhost:11434".to_string();
                    browser.settings_model = "llama3.2".to_string();
                }
            }
            browser.settings_saved = false;
            browser.settings_test_status = None;
        }
```
Replace with:
```rust
        ui.radio_value(
            &mut browser.settings_provider,
            SettingsProvider::Cloud,
            "Cloud API",
        );
        ui.add_space(8.0);
        ui.radio_value(
            &mut browser.settings_provider,
            SettingsProvider::Ollama,
            "Local LLM (Ollama)",
        );
        if browser.settings_provider != prev {
            match browser.settings_provider {
                SettingsProvider::Cloud => {
                    let preset = browser.settings_cloud_preset;
                    browser.settings_endpoint = preset.default_endpoint().to_string();
                    browser.settings_model = preset.default_model().to_string();
                }
                SettingsProvider::Ollama => {
                    browser.settings_endpoint = "http://localhost:11434".to_string();
                    browser.settings_model = "llama3.2".to_string();
                }
            }
            browser.settings_saved = false;
            browser.settings_test_status = None;
        }
```

- [ ] **Step 2: Add the preset button row after the provider radio block**

After the closing `});` of the `ui.horizontal(|ui| { ... })` provider radio block (around line 125), and before `ui.add_space(14.0);`, insert:

```rust
    // ── Preset picker (Cloud only) ────────────────────────────────────────
    if browser.settings_provider == SettingsProvider::Cloud {
        ui.add_space(8.0);
        ui.label(
            egui::RichText::new("PRESET")
                .size(10.0)
                .strong()
                .color(KitsuneTheme::TEXT2)
                .family(egui::FontFamily::Monospace),
        );
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            for preset in [
                CloudPreset::Claude,
                CloudPreset::OpenAI,
                CloudPreset::Gemini,
                CloudPreset::Groq,
                CloudPreset::OpenRouter,
                CloudPreset::Custom,
            ] {
                let active = browser.settings_cloud_preset == preset;
                let fill = if active { KitsuneTheme::AMBER } else { KitsuneTheme::BG3 };
                let text_color = if active { egui::Color32::BLACK } else { KitsuneTheme::TEXT1 };
                let btn = egui::Button::new(
                    egui::RichText::new(preset.label()).color(text_color).size(10.0).strong(),
                )
                .fill(fill)
                .min_size(egui::vec2(56.0, 22.0));
                if ui.add(btn).clicked() && browser.settings_cloud_preset != preset {
                    browser.settings_cloud_preset = preset;
                    browser.settings_endpoint = preset.default_endpoint().to_string();
                    browser.settings_model = preset.default_model().to_string();
                    browser.settings_saved = false;
                    browser.settings_test_status = None;
                }
                ui.add_space(2.0);
            }
        });
    }
```

- [ ] **Step 3: Update API key hint and grid hints**

Find the grid section (around line 130-173). The API Key field currently has `.hint_text("sk-...")`. Replace the whole grid block:

```rust
    egui::Grid::new("settings_grid")
        .num_columns(2)
        .spacing([12.0, 10.0])
        .show(ui, |ui| {
            if browser.settings_provider == SettingsProvider::Cloud {
                ui.label(egui::RichText::new("API Key").color(KitsuneTheme::TEXT1));
                ui.add(
                    egui::TextEdit::singleline(&mut browser.settings_api_key)
                        .password(true)
                        .desired_width(260.0)
                        .hint_text(browser.settings_cloud_preset.key_hint()),
                );
                ui.end_row();
            }

            let endpoint_label = match browser.settings_provider {
                SettingsProvider::Cloud => "Endpoint",
                SettingsProvider::Ollama => "Ollama URL",
            };
            ui.label(egui::RichText::new(endpoint_label).color(KitsuneTheme::TEXT1));
            let endpoint_hint = match browser.settings_provider {
                SettingsProvider::Cloud => browser.settings_cloud_preset.default_endpoint(),
                SettingsProvider::Ollama => "http://localhost:11434",
            };
            ui.add(
                egui::TextEdit::singleline(&mut browser.settings_endpoint)
                    .desired_width(260.0)
                    .hint_text(endpoint_hint),
            );
            ui.end_row();

            ui.label(egui::RichText::new("Model").color(KitsuneTheme::TEXT1));
            let model_hint = match browser.settings_provider {
                SettingsProvider::Cloud => browser.settings_cloud_preset.default_model(),
                SettingsProvider::Ollama => "llama3.2",
            };
            ui.add(
                egui::TextEdit::singleline(&mut browser.settings_model)
                    .desired_width(260.0)
                    .hint_text(model_hint),
            );
            ui.end_row();
        });
```

- [ ] **Step 4: Update description text**

Find the block (around line 178-197):
```rust
    if browser.settings_provider == SettingsProvider::Ollama {
        ui.label(
            egui::RichText::new(
                "Ollama runs entirely on your machine — no data leaves the device. \
                 Make sure `ollama serve` is running and the model is pulled \
                 (e.g. `ollama pull llama3.2`).",
            )
            .size(11.0)
            .color(KitsuneTheme::TEXT2),
        );
    } else {
        ui.label(
            egui::RichText::new(
                "Works with OpenAI, Groq, Together, OpenRouter, or any other provider \
                 that exposes an OpenAI /v1/chat/completions endpoint.",
            )
            .size(11.0)
            .color(KitsuneTheme::TEXT2),
        );
    }
```
Replace with:
```rust
    if browser.settings_provider == SettingsProvider::Ollama {
        ui.label(
            egui::RichText::new(
                "Ollama runs entirely on your machine — no data leaves the device. \
                 Make sure `ollama serve` is running and the model is pulled \
                 (e.g. `ollama pull llama3.2`).",
            )
            .size(11.0)
            .color(KitsuneTheme::TEXT2),
        );
    } else {
        ui.label(
            egui::RichText::new(
                "Works with Claude (Anthropic), OpenAI, Gemini (Google), Groq, OpenRouter, \
                 or any provider with an OpenAI-compatible /v1/chat/completions endpoint. \
                 Select a preset above to auto-fill the URL.",
            )
            .size(11.0)
            .color(KitsuneTheme::TEXT2),
        );
    }
```

- [ ] **Step 5: Fix the Agents tab label**

In `render_agents_tab`, find (around line 322):
```rust
    ui.label(
        egui::RichText::new("Model slots (Ollama model names or provider IDs):")
            .color(KitsuneTheme::TEXT1)
            .size(12.0),
    );
```
Replace with:
```rust
    ui.label(
        egui::RichText::new("Model names or provider IDs:")
            .color(KitsuneTheme::TEXT1)
            .size(12.0),
    );
```

- [ ] **Step 6: Build to verify no compile errors**

```powershell
cargo build -p kitsune-ui 2>&1 | Select-String -Pattern "error"
```
Expected: no lines containing `error[E`.

- [ ] **Step 7: Commit**

```powershell
git add crates/kitsune-ui/src/dialogs/settings_dialog.rs
git commit -m "feat(ui): universal provider presets in LLM settings dialog"
```

---

## Task 3: Add `LlmBackend` to `loop_runtime.rs` and fix the runtime

**Files:**
- Modify: `crates/kitsune-agent/src/loop_runtime.rs`

- [ ] **Step 1: Add `LlmBackend` enum with `chat()` method**

After the existing imports block at the top of `loop_runtime.rs`, add the `LlmBackend` enum before `const MAX_ITERATIONS`. Insert after line ~27 (`use tracing::{info, warn};`):

```rust
use crate::ai_client::AiProviderConfig;

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
    async fn chat(&self, system: &str, history: Vec<(String, String)>) -> Result<String, AgentError> {
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
                    return Err(AgentError::ExecutionError(format!(
                        "Cloud LLM returned HTTP {}: {}", status, text
                    )));
                }

                let parsed: serde_json::Value = resp
                    .json()
                    .await
                    .map_err(|e| AgentError::ExecutionError(format!("Cloud LLM bad JSON: {}", e)))?;

                parsed["choices"][0]["message"]["content"]
                    .as_str()
                    .map(|s| s.to_string())
                    .ok_or_else(|| AgentError::ExecutionError("empty cloud LLM response".into()))
            }
        }
    }
}
```

- [ ] **Step 2: Change `LlmAgentRuntime` struct field from `ollama` to `backend`**

Find the struct definition (lines 57-67):
```rust
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
```
Replace with:
```rust
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
}
```

- [ ] **Step 3: Update `LlmAgentRuntime::new()` to build `LlmBackend::Ollama`**

Find the `new()` impl (lines 69-95):
```rust
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
```
Replace with:
```rust
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
                    .unwrap_or_else(|_| reqwest::Client::new());
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
        }
    }
```

- [ ] **Step 4: Replace `self.ollama.chat(...)` with `self.backend.chat(...)`**

Find line 177 in `run()`:
```rust
            let raw = match self.ollama.chat(&system, history.clone()).await {
```
Replace with:
```rust
            let raw = match self.backend.chat(&system, history.clone()).await {
```

- [ ] **Step 5: Update the LlmUnavailable error message**

Find the error string (around line 199-204):
```rust
                    let err_msg = format!(
                        "LLM unavailable and task requires reasoning. Start Ollama (`ollama serve`) or configure an API key. Detail: {}",
                        msg
                    );
```
Replace with:
```rust
                    let err_msg = format!(
                        "LLM unavailable and task requires reasoning. \
                         For local models, run `ollama serve`. \
                         For cloud providers, check your API key and endpoint in Settings. \
                         Detail: {}",
                        msg
                    );
```

- [ ] **Step 6: Write a unit test for `LlmBackend` construction**

At the bottom of `loop_runtime.rs`, add:
```rust
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
```

- [ ] **Step 7: Run the agent crate tests**

```powershell
cargo test -p kitsune-agent 2>&1 | Select-String -Pattern "test result|FAILED|error\[E"
```
Expected: `test result: ok.` — no `FAILED` or `error[E` lines.

- [ ] **Step 8: Commit**

```powershell
git add crates/kitsune-agent/src/loop_runtime.rs
git commit -m "feat(agent): add LlmBackend enum, wire cloud credentials to in-process agent"
```

---

## Task 4: Wire `agent_panel.rs` to build `AiProviderConfig` and call `new_with_config`

**Files:**
- Modify: `crates/kitsune-ui/src/panels/agent_panel.rs`

- [ ] **Step 1: Update imports**

Find the existing import at the top:
```rust
use crate::app::{AgentRunState, AgentSseAction, AttachedFile, KitsuneBrowser, LogLevel};
```
Replace with:
```rust
use crate::app::{AgentRunState, AgentSseAction, AttachedFile, KitsuneBrowser, LogLevel, SettingsProvider};
use kitsune_agent::ai_client::{AiProviderConfig, ModelSlots};
```

- [ ] **Step 2: Add `AiProviderConfig` construction in `start_agent_run`**

In `start_agent_run`, after the line:
```rust
    let spec = build_runtime_spec(browser);
```
Add:
```rust
    let endpoint = browser.settings_endpoint.trim().to_string();
    let model = browser.settings_model.trim().to_string();
    let api_key = browser.settings_api_key.clone();
    let ai_config = match browser.settings_provider {
        SettingsProvider::Ollama => {
            let url = if endpoint.is_empty() {
                "http://localhost:11434".to_string()
            } else {
                endpoint
            };
            let m = if model.is_empty() { "llama3".to_string() } else { model };
            AiProviderConfig::Ollama {
                url,
                slots: ModelSlots { orchestrator: m.clone(), worker: m.clone(), fast: m },
            }
        }
        SettingsProvider::Cloud => {
            let m = if model.is_empty() { "gpt-4o-mini".to_string() } else { model };
            AiProviderConfig::OpenAiCompatible {
                url: endpoint,
                api_key,
                slots: ModelSlots { orchestrator: m.clone(), worker: m.clone(), fast: m },
            }
        }
    };
```

- [ ] **Step 3: Pass `ai_config` into the async spawn**

Find the `browser.runtime().spawn(async move {` block. The closure currently captures:
```rust
    browser.runtime().spawn(async move {
        run_in_process_agent(spec, cmd, context, vault, hil_gate, webview_tx, agent_tx, file_perm_slot, stop_flag).await;
    });
```
Replace with:
```rust
    browser.runtime().spawn(async move {
        run_in_process_agent(spec, ai_config, cmd, context, vault, hil_gate, webview_tx, agent_tx, file_perm_slot, stop_flag).await;
    });
```

- [ ] **Step 4: Update `run_in_process_agent` signature and body**

Find the function signature (around line 532):
```rust
async fn run_in_process_agent(
    spec: AgentSpec,
    prompt: String,
    context: String,
    vault: std::sync::Arc<kitsune_vault::VaultBackend>,
    hil_gate: std::sync::Arc<kitsune_hil::HilGate>,
    webview_tx: tokio::sync::mpsc::Sender<kitsune_agent::executor::WebViewCommand>,
    ui_tx: Sender<AgentSseAction>,
    file_perm_slot: FilePermSlot,
    stop_flag: StopFlag,
) {
```
Replace with:
```rust
async fn run_in_process_agent(
    spec: AgentSpec,
    ai_config: AiProviderConfig,
    prompt: String,
    context: String,
    vault: std::sync::Arc<kitsune_vault::VaultBackend>,
    hil_gate: std::sync::Arc<kitsune_hil::HilGate>,
    webview_tx: tokio::sync::mpsc::Sender<kitsune_agent::executor::WebViewCommand>,
    ui_tx: Sender<AgentSseAction>,
    file_perm_slot: FilePermSlot,
    stop_flag: StopFlag,
) {
```

Then find the `LlmAgentRuntime::new` call (around line 545):
```rust
    let runtime = LlmAgentRuntime::new(spec, webview_tx, vault, hil_gate)
        .with_event_sink(events_tx)
        .with_file_perm_slot(file_perm_slot)
        .with_stop_flag(stop_flag);
```
Replace with:
```rust
    let runtime = LlmAgentRuntime::new_with_config(spec, ai_config, webview_tx, vault, hil_gate)
        .with_event_sink(events_tx)
        .with_file_perm_slot(file_perm_slot)
        .with_stop_flag(stop_flag);
```

- [ ] **Step 5: Build the full workspace**

```powershell
cargo build -p kitsune-ui 2>&1 | Select-String -Pattern "error\[E|^error"
```
Expected: no error lines.

- [ ] **Step 6: Run the full workspace test suite**

```powershell
cargo test --workspace 2>&1 | Select-String -Pattern "test result|FAILED"
```
Expected: all crates show `test result: ok.`, no `FAILED`.

- [ ] **Step 7: Commit**

```powershell
git add crates/kitsune-ui/src/panels/agent_panel.rs
git commit -m "feat(ui): wire AiProviderConfig from settings into LlmAgentRuntime"
```

---

## Self-Review Checklist

**Spec coverage:**
- [x] `CloudPreset` enum with all 6 variants + 4 methods → Task 1
- [x] `SettingsProvider::Cloud` rename + `wire_value` unchanged → Task 1
- [x] `settings_cloud_preset` field on `KitsuneBrowser` → Task 1
- [x] Preset button row auto-fills endpoint + model → Task 2 Step 2
- [x] Per-preset key hint in API Key field → Task 2 Step 3
- [x] Updated description text mentioning Claude, Gemini, etc. → Task 2 Step 4
- [x] Agents tab label fix → Task 2 Step 5
- [x] `LlmBackend` enum with `Ollama` and `Cloud` variants → Task 3 Step 1
- [x] `LlmBackend::chat()` — Ollama delegates, Cloud calls `/chat/completions` → Task 3 Step 1
- [x] `LlmAgentRuntime::new_with_config` → Task 3 Step 3
- [x] `self.ollama.chat` → `self.backend.chat` → Task 3 Step 4
- [x] `agent_panel.rs` builds `AiProviderConfig` from live settings → Task 4 Step 2
- [x] `run_in_process_agent` gets `ai_config` and uses `new_with_config` → Task 4 Steps 3-4

**No placeholders found.**

**Type consistency:** `AiProviderConfig`, `ModelSlots`, `LlmBackend`, `CloudPreset`, `SettingsProvider::Cloud` — all used consistently across tasks. `slots.worker` is the model field read in `new_with_config`, matching `ModelSlots::worker: String`.
