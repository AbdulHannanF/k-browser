# Universal API Provider & UI Fixes — Design Spec
**Date:** 2026-05-10

## Problem

The LLM settings dialog labels everything "OpenAI-compatible API" and uses `sk-...` as the API-key hint. This misleads users of Claude, Gemini, Groq, and OpenRouter. Worse, even when a user fills in cloud credentials, the in-process agent loop (`LlmAgentRuntime`) ignores them — it always constructs an `OllamaClient` regardless of the chosen provider. Cloud credentials are only sent to the cloud-mock server, not to the actual agent loop.

## Goals

1. Named presets for Claude, OpenAI, Gemini, Groq, OpenRouter, and Custom — each auto-filling the right endpoint and key hint.
2. Fix the runtime bug: in-process agent uses cloud credentials when a cloud provider is selected.
3. Clean up UI irregularities (mislabeled tabs, Ollama-specific labels in generic places).

## Out of Scope

- Adding new AI backends (no new crates, no candle inference).
- Persisting settings to disk (still in-memory; no config file format yet).
- Changing the cloud-mock server (`kitsune-cloud-mock`) — it stays `open_ai_compatible`/`ollama`.

---

## Design

### 1. New Types in `kitsune-ui/src/app.rs`

```rust
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
    pub fn label(&self) -> &'static str { … }
    pub fn default_endpoint(&self) -> &'static str { … }
    pub fn default_model(&self) -> &'static str { … }
    pub fn key_hint(&self) -> &'static str { … }
}
```

Preset defaults:

| Preset     | Endpoint                                                         | Default model           | Key hint          |
|------------|------------------------------------------------------------------|-------------------------|-------------------|
| Claude     | `https://api.anthropic.com/v1`                                   | `claude-3-5-sonnet-20241022` | `sk-ant-api03-…`  |
| OpenAI     | `https://api.openai.com/v1`                                      | `gpt-4o-mini`           | `sk-…`            |
| Gemini     | `https://generativelanguage.googleapis.com/v1beta/openai`        | `gemini-2.0-flash`      | `AIza…`           |
| Groq       | `https://api.groq.com/openai/v1`                                 | `llama-3.3-70b-versatile` | `gsk_…`         |
| OpenRouter | `https://openrouter.ai/api/v1`                                   | `anthropic/claude-3.5-sonnet` | `sk-or-…`    |
| Custom     | (empty, user fills in)                                           | (empty)                 | `your-api-key`    |

**`SettingsProvider`** rename: `OpenAiCompatible` → `Cloud`. Wire value stays `"open_ai_compatible"` (no mock-server change needed).

**New field on `KitsuneBrowser`**: `pub settings_cloud_preset: CloudPreset` — defaults to `CloudPreset::Claude`.

### 2. Settings Dialog Changes (`settings_dialog.rs`)

**Provider row** (unchanged structure, renamed label):
```
○ Cloud API    ○ Local LLM (Ollama)
```

**Preset picker row** (shown only when Cloud is active):
```
[ Claude ] [ OpenAI ] [ Gemini ] [ Groq ] [ OpenRouter ] [ Custom ]
```
Clicking a preset button:
- Sets `browser.settings_cloud_preset`
- Writes `browser.settings_endpoint = preset.default_endpoint()`
- Writes `browser.settings_model = preset.default_model()` (only if model field is empty or was a preset default)
- Clears `browser.settings_saved` and `browser.settings_test_status`

**Grid rows** (same structure, updated hints):
- API Key hint: `preset.key_hint()` instead of `"sk-..."`
- Endpoint hint: `preset.default_endpoint()`
- Model hint: `preset.default_model()`

**Description text** (bottom of Cloud section):
> Works with Claude (Anthropic), OpenAI, Gemini (Google), Groq, OpenRouter, or any provider with an OpenAI-compatible `/v1/chat/completions` endpoint. Select a preset above to auto-fill the URL.

**Agents tab label fix**: "Ollama model names or provider IDs:" → "Model names or provider IDs:"

### 3. Runtime Fix

#### 3a. `LlmBackend` enum in `loop_runtime.rs`

```rust
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
    async fn chat(&self, system: &str, history: Vec<(String, String)>) -> AgentResult<String>
}
```

- `Ollama` variant delegates to existing `OllamaClient::chat`.
- `Cloud` variant calls `POST {url}/chat/completions` with `Authorization: Bearer {api_key}` and the standard OpenAI messages array (system + history). Parses `choices[0].message.content`.

#### 3b. `LlmAgentRuntime` struct change

```rust
pub struct LlmAgentRuntime {
    spec: AgentSpec,
    backend: LlmBackend,   // replaces `ollama: OllamaClient`
    …
}
```

`LlmAgentRuntime::new` retains current signature (reads `spec.ollama_url`/`spec.ollama_model` → `LlmBackend::Ollama`). A new constructor `new_with_config(spec, config: AiProviderConfig, …)` picks the backend from `AiProviderConfig`:
- `AiProviderConfig::Ollama { url, slots }` → `LlmBackend::Ollama`
- `AiProviderConfig::OpenAiCompatible { url, api_key, slots }` → `LlmBackend::Cloud`

All call sites for `self.ollama.chat(…)` inside `loop_runtime.rs` become `self.backend.chat(…)`.

#### 3c. `build_runtime_spec` in `agent_panel.rs`

Currently builds an `AgentSpec` with `ollama_url`/`ollama_model` and calls `LlmAgentRuntime::new(spec, …)`.

After this change, it constructs an `AiProviderConfig` from browser settings and calls `LlmAgentRuntime::new_with_config(spec, config, …)`:

```rust
let config = match browser.settings_provider {
    SettingsProvider::Ollama => AiProviderConfig::Ollama {
        url: endpoint.to_string(),
        slots: ModelSlots { orchestrator: model.to_string(), worker: model.to_string(), fast: model.to_string() },
    },
    SettingsProvider::Cloud => AiProviderConfig::OpenAiCompatible {
        url: endpoint.to_string(),
        api_key: browser.settings_api_key.clone(),
        slots: ModelSlots { … },
    },
};
```

The `AgentSpec` fields `ollama_url`/`ollama_model` can stay for compatibility; the runtime just won't use them when `new_with_config` is called.

---

## File Change Summary

| File | Change |
|---|---|
| `kitsune-ui/src/app.rs` | Add `CloudPreset` enum + methods; rename `OpenAiCompatible` → `Cloud`; add `settings_cloud_preset` field |
| `kitsune-ui/src/dialogs/settings_dialog.rs` | Add preset button row; update hints/descriptions; fix Agents tab label |
| `kitsune-ui/src/panels/agent_panel.rs` | `build_runtime_spec` → constructs `AiProviderConfig`; calls `new_with_config` |
| `kitsune-agent/src/loop_runtime.rs` | Add `LlmBackend` enum; change `LlmAgentRuntime` to use it; add `new_with_config` constructor |

No changes to: `kitsune-cloud-mock`, `kitsune-ai`, `kitsune-agent/src/ai_client.rs`, `kitsune-agent/src/ollama_client.rs`.

---

## Invariants Preserved

- No raw secrets cross any boundary: API key flows from settings field → `AiProviderConfig` → `LlmBackend::Cloud` as a string; never logged, never stored in vault (no new vault usage).
- `TaskType::VaultDecision` / `SensitiveForm` routing policy is untouched.
- `HilGate` flow is untouched.
- The cloud-mock server's `AiSettings` wire format is unchanged.
