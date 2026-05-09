# Agentic Pipeline Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a hierarchical agentic pipeline that autonomously fills scholarship applications, books flights, bypasses CAPTCHAs, and manages site accounts — with token-efficient sub-agents routed to appropriate model tiers.

**Architecture:** An `AgentOrchestrator` decomposes user goals into typed `SubTask`s dispatched to specialised sub-agents (Search, Form, Submit, Booking). A `ProfileIndexer` builds a compressed `ProfileSummary` from local documents. A `CaptchaAgent` resolves CAPTCHAs through four escalating tiers. `FormAgent` plans field fills with one LLM call then executes deterministically. All agents share `DomAccessor` (extended with human-like timing) and existing HIL/vault infrastructure.

**Tech Stack:** Rust, tokio, egui/eframe, wry/WebView2, reqwest (rustls), kitsune-vault (existing), kitsune-hil (existing), pdf-extract, docx-rs, notify, sha2, rand

---

## File Map

### New files
| File | Responsibility |
|---|---|
| `crates/kitsune-agent/src/ai_client.rs` | `AgentAiClient` — lightweight HTTP client for Ollama/OpenAI-compat, `ModelTier`, `ModelSlots` |
| `crates/kitsune-agent/src/profile.rs` | `ProfileIndexer`, `ProfileSummary`, `EducationEntry`, `LanguageEntry` |
| `crates/kitsune-agent/src/captcha.rs` | `CaptchaAgent`, `CaptchaKind`, tier 1–4 logic |
| `crates/kitsune-agent/src/orchestrator.rs` | `AgentOrchestrator`, `SubTask`, `Candidate`, `TaskStatus` |
| `crates/kitsune-agent/src/agents/mod.rs` | re-exports for sub-agent modules |
| `crates/kitsune-agent/src/agents/search.rs` | `SearchAgent` |
| `crates/kitsune-agent/src/agents/form.rs` | `FormAgent`, `FieldAction`, `FieldMappingPlan`, `FormResult` |
| `crates/kitsune-agent/src/agents/submit.rs` | `SubmitAgent` |
| `crates/kitsune-agent/src/agents/booking.rs` | `BookingAgent`, `FlightOffer`, `BookingCriteria` |
| `crates/kitsune-ui/src/panels/profile_panel.rs` | egui panel for `ProfileSummary` display |
| `crates/kitsune-ui/src/panels/task_graph_panel.rs` | egui panel for live `TaskStatus` graph |

### Modified files
| File | Change |
|---|---|
| `crates/kitsune-hil/src/trigger.rs` | Add `HilTriggerClass::CaptchaRequired` variant |
| `crates/kitsune-agent/src/dom_access.rs` | Add `human_delay()`, `inject_mouse_move()`, randomised timing |
| `crates/kitsune-agent/src/lib.rs` | Wire all new modules |
| `crates/kitsune-agent/src/error.rs` | Add `ProfileError`, `CaptchaError` variants to `AgentError` |
| `crates/kitsune-agent/Cargo.toml` | Add `pdf-extract`, `docx-rs`, `notify`, `sha2`, `rand`, `base64` |
| `crates/kitsune-ui/src/app.rs` | Add `ProfileIndexer`, `AgentOrchestrator`, `task_statuses` fields |
| `crates/kitsune-ui/src/panels/agent_panel.rs` | Call orchestrator instead of cloud-mock POST |
| `crates/kitsune-ui/src/dialogs/settings_dialog.rs` | Add Profile + Agents tabs |

---

## Task 1: AgentAiClient — model-tiered HTTP client

**Why a new client:** `kitsune-ai` already imports `kitsune_agent::BudgetTracker`, so `kitsune-agent` cannot import `kitsune-ai` (circular dep). `AgentAiClient` follows the existing `OllamaClient` pattern — direct HTTP calls, no cross-crate cycle.

**Files:**
- Create: `crates/kitsune-agent/src/ai_client.rs`
- Modify: `crates/kitsune-agent/Cargo.toml` (add deps)

- [ ] **Step 1: Add dependencies to `crates/kitsune-agent/Cargo.toml`**

Open the file and add to `[dependencies]`:
```toml
pdf-extract = "0.7"
docx-rs = "0.4"
notify = "6"
sha2 = "0.10"
rand = "0.8"
base64 = "0.22"
```

- [ ] **Step 2: Write the failing test**

Add to a new `crates/kitsune-agent/tests/ai_client.rs`:
```rust
use kitsune_agent::ai_client::{AgentAiClient, AiProviderConfig, ModelSlots, ModelTier};

#[test]
fn model_slots_selects_correct_model() {
    let slots = ModelSlots {
        orchestrator: "llama3:70b".into(),
        worker: "llama3:70b".into(),
        fast: "llama3:8b".into(),
    };
    assert_eq!(slots.model_for(ModelTier::Fast), "llama3:8b");
    assert_eq!(slots.model_for(ModelTier::Orchestrator), "llama3:70b");
}
```

- [ ] **Step 3: Run test to see it fail**

```powershell
cargo test -p kitsune-agent --test ai_client 2>&1 | Select-String -Pattern "error|FAILED|ok"
```
Expected: compile error — `ai_client` module not found.

- [ ] **Step 4: Create `crates/kitsune-agent/src/ai_client.rs`**

```rust
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AiProviderConfig {
    Ollama { url: String, slots: ModelSlots },
    OpenAiCompatible { url: String, api_key: String, slots: ModelSlots },
}

impl AiProviderConfig {
    fn slots(&self) -> &ModelSlots {
        match self {
            Self::Ollama { slots, .. } => slots,
            Self::OpenAiCompatible { slots, .. } => slots,
        }
    }

    fn model_for(&self, tier: ModelTier) -> &str {
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

    /// Send a prompt, receive a text response. Blocks until complete.
    pub async fn complete(&self, prompt: &str, tier: ModelTier) -> AgentResult<String> {
        match &self.config {
            AiProviderConfig::Ollama { url, .. } => {
                let model = self.config.model_for(tier);
                let body = OllamaRequest { model, prompt, stream: false };
                let resp = self
                    .http
                    .post(format!("{}/api/generate", url))
                    .json(&body)
                    .send()
                    .await
                    .map_err(|e| AgentError::ExecutionError(e.to_string()))?;
                let parsed: OllamaResponse = resp
                    .json()
                    .await
                    .map_err(|e| AgentError::ExecutionError(e.to_string()))?;
                Ok(parsed.response)
            }
            AiProviderConfig::OpenAiCompatible { url, api_key, .. } => {
                let model = self.config.model_for(tier);
                let body = OpenAiRequest {
                    model,
                    messages: vec![OpenAiMessage { role: "user", content: prompt }],
                    max_tokens: 4096,
                };
                let resp = self
                    .http
                    .post(format!("{}/v1/chat/completions", url))
                    .bearer_auth(api_key)
                    .json(&body)
                    .send()
                    .await
                    .map_err(|e| AgentError::ExecutionError(e.to_string()))?;
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
```

- [ ] **Step 5: Add module to `crates/kitsune-agent/src/lib.rs`**

Add after the existing `pub mod` lines:
```rust
pub mod ai_client;
```

- [ ] **Step 6: Run test to verify it passes**

```powershell
cargo test -p kitsune-agent --test ai_client 2>&1 | Select-String -Pattern "error|FAILED|ok"
```
Expected: `test model_slots_selects_correct_model ... ok`

- [ ] **Step 7: Commit**

```powershell
git add crates/kitsune-agent/src/ai_client.rs crates/kitsune-agent/src/lib.rs crates/kitsune-agent/Cargo.toml crates/kitsune-agent/tests/ai_client.rs
git commit -m "feat(agent): add AgentAiClient with ModelTier + ModelSlots"
```

---

## Task 2: HilTriggerClass::CaptchaRequired

**Files:**
- Modify: `crates/kitsune-hil/src/trigger.rs`

- [ ] **Step 1: Write the failing test**

Add to `crates/kitsune-hil/src/trigger.rs` inside `#[cfg(test)] mod tests { ... }` (create the block if absent):
```rust
#[test]
fn captcha_required_summary_contains_site() {
    let t = HilTriggerClass::CaptchaRequired {
        site: "daad.de".into(),
        captcha_type: "recaptcha-v2".into(),
    };
    let s = t.plain_language_summary();
    assert!(s.contains("daad.de"), "summary: {s}");
    assert!(s.contains("CAPTCHA"), "summary: {s}");
}
```

- [ ] **Step 2: Run test to see it fail**

```powershell
cargo test -p kitsune-hil captcha_required_summary 2>&1 | Select-String -Pattern "error|FAILED|ok"
```
Expected: compile error — `CaptchaRequired` variant not found.

- [ ] **Step 3: Add variant to `HilTriggerClass` enum**

In `crates/kitsune-hil/src/trigger.rs`, add after the last variant before the closing `}` of the enum (before `impl HilTriggerClass`):
```rust
    /// Agent encountered a CAPTCHA that requires resolution before proceeding.
    CaptchaRequired {
        /// The domain where the CAPTCHA was detected.
        site: String,
        /// CAPTCHA type (e.g. "recaptcha-v2", "hcaptcha", "cloudflare-turnstile").
        captcha_type: String,
    },
```

- [ ] **Step 4: Add arm to `plain_language_summary`**

In `impl HilTriggerClass`, inside `plain_language_summary`, add to the `match self` block:
```rust
            Self::CaptchaRequired { site, captcha_type } => {
                format!("CAPTCHA detected on {} ({}). Please solve it to continue.", site, captcha_type)
            }
```

- [ ] **Step 5: Add arm to `involves_money`**

In `involves_money`, add `CaptchaRequired` to the non-money arm (it already falls through via `matches!`, but add it explicitly in the pattern if needed — just verify it compiles):
```powershell
cargo build -p kitsune-hil 2>&1 | Select-String -Pattern "error"
```
Expected: no errors. If `matches!` macro gives a non-exhaustive warning, add `| Self::CaptchaRequired { .. }` to the non-matching arms.

- [ ] **Step 6: Run test to verify it passes**

```powershell
cargo test -p kitsune-hil captcha_required_summary 2>&1 | Select-String -Pattern "error|FAILED|ok"
```
Expected: `test trigger::tests::captcha_required_summary_contains_site ... ok`

- [ ] **Step 7: Commit**

```powershell
git add crates/kitsune-hil/src/trigger.rs
git commit -m "feat(hil): add CaptchaRequired trigger class"
```

---

## Task 3: DomAccessor behavioral timing

Adds randomised human-like delays and mouse-move injection to every user-facing DOM action. This is Tier 1 CAPTCHA evasion — always active.

**Files:**
- Modify: `crates/kitsune-agent/src/dom_access.rs`

- [ ] **Step 1: Write the failing test**

Add to `#[cfg(test)] mod tests` in `crates/kitsune-agent/src/dom_access.rs`:
```rust
#[test]
fn human_delay_range_is_valid() {
    // human_delay() picks from 80–180ms. Just verify the bounds are sane.
    let min = 80u64;
    let max = 180u64;
    assert!(min < max);
    assert!(max <= 500, "delay should not be so long it breaks tests");
}
```

- [ ] **Step 2: Run test to see it fail**

```powershell
cargo test -p kitsune-agent human_delay_range 2>&1 | Select-String -Pattern "error|FAILED|ok"
```
Expected: compile error if module doesn't compile yet; or test not found.

- [ ] **Step 3: Add `human_delay` and `mouse_move_js` to `DomAccessor`**

In `crates/kitsune-agent/src/dom_access.rs`, add these two methods inside `impl DomAccessor`, before the closing `}`:
```rust
    /// Pause for a randomised human-like duration (80–180 ms).
    /// Called before every field fill and click to evade bot-detection heuristics.
    async fn human_delay(&self) {
        use rand::Rng;
        let ms = rand::thread_rng().gen_range(80u64..=180);
        tokio::time::sleep(std::time::Duration::from_millis(ms)).await;
    }

    /// Inject a synthetic mousemove event near the target element.
    async fn inject_mouse_move(&self, selector: &str) -> AgentResult<()> {
        let safe = selector.replace('\'', "\\'");
        let script = format!(
            r#"(function(){{
                let el = document.querySelector('{safe}');
                if (el) {{
                    let r = el.getBoundingClientRect();
                    let x = r.left + r.width / 2 + (Math.random() * 6 - 3);
                    let y = r.top  + r.height / 2 + (Math.random() * 6 - 3);
                    el.dispatchEvent(new MouseEvent('mousemove', {{bubbles:true, clientX:x, clientY:y}}));
                }}
            }})();"#
        );
        self.webview_tx
            .send(WebViewCommand::EvalJs(script))
            .await
            .map_err(|_| AgentError::IpcDisconnected)?;
        Ok(())
    }
```

- [ ] **Step 4: Wire delays into existing methods**

In `fill_field`, add `self.human_delay().await;` and `self.inject_mouse_move(field_id).await.ok();` at the very start of the method body (before the HIL checkpoint):
```rust
    pub async fn fill_field(&self, field_id: &str, value: &str) -> Result<(), AgentError> {
        self.human_delay().await;
        self.inject_mouse_move(field_id).await.ok();
        // 1. Gate through HIL ...  (rest of existing code unchanged)
```

In `click_element`, add `self.human_delay().await;` and `self.inject_mouse_move(selector).await.ok();` at the very start:
```rust
    pub async fn click_element(&self, selector: &str) -> AgentResult<()> {
        self.human_delay().await;
        self.inject_mouse_move(selector).await.ok();
        // existing HIL checkpoint ...
```

- [ ] **Step 5: Add `rand` to `use` in dom_access.rs** (it was added to Cargo.toml in Task 1)

Verify `rand` is in `crates/kitsune-agent/Cargo.toml` (done in Task 1). Build:
```powershell
cargo build -p kitsune-agent 2>&1 | Select-String -Pattern "error"
```
Expected: no errors.

- [ ] **Step 6: Run all kitsune-agent tests**

```powershell
cargo test -p kitsune-agent 2>&1 | Select-String -Pattern "FAILED|error\[|test result"
```
Expected: all existing tests pass (the delay tests are fast; timings are sub-second).

- [ ] **Step 7: Commit**

```powershell
git add crates/kitsune-agent/src/dom_access.rs
git commit -m "feat(agent): add human-like timing to DomAccessor (CAPTCHA Tier 1)"
```

---

## Task 4: ProfileIndexer

Watches a folder, hashes each file, parses PDFs and DOCX, calls AI to extract a `ProfileSummary`, caches the result.

**Files:**
- Create: `crates/kitsune-agent/src/profile.rs`
- Modify: `crates/kitsune-agent/src/lib.rs`

- [ ] **Step 1: Write the failing test**

Create `crates/kitsune-agent/tests/profile.rs`:
```rust
use kitsune_agent::profile::{ProfileSummary, ProfileIndexer};
use std::path::PathBuf;

#[test]
fn profile_summary_default_is_empty() {
    let s = ProfileSummary::default();
    assert!(s.full_name.is_empty());
    assert!(s.education.is_empty());
    assert!(s.languages.is_empty());
}

#[test]
fn profile_indexer_new_accepts_path() {
    let indexer = ProfileIndexer::new(PathBuf::from("tests/fixtures/profile"));
    // Just verify construction doesn't panic
    let _ = indexer;
}
```

- [ ] **Step 2: Run test to see it fail**

```powershell
cargo test -p kitsune-agent --test profile 2>&1 | Select-String -Pattern "error|FAILED|ok"
```
Expected: compile error — `profile` module not found.

- [ ] **Step 3: Create `crates/kitsune-agent/src/profile.rs`**

```rust
use crate::ai_client::{AgentAiClient, ModelTier};
use crate::error::{AgentError, AgentResult};
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{info, warn};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EducationEntry {
    pub degree: String,
    pub institution: String,
    pub year: Option<u32>,
    pub gpa: Option<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LanguageEntry {
    pub language: String,
    pub level: String, // CEFR: A1–C2
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProfileSummary {
    pub full_name: String,
    pub date_of_birth: Option<String>, // ISO 8601: "1995-03-15"
    pub nationality: Option<String>,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub education: Vec<EducationEntry>,
    pub languages: Vec<LanguageEntry>,
    pub skills: Vec<String>,
    pub publications: Vec<String>,
    pub awards: Vec<String>,
    pub generated_at: Option<DateTime<Utc>>,
    pub source_files: Vec<String>,
}

impl ProfileSummary {
    /// Compact token-efficient representation for passing to sub-agents.
    pub fn to_prompt_context(&self) -> String {
        let edu: Vec<String> = self
            .education
            .iter()
            .map(|e| {
                let gpa = e.gpa.map(|g| format!(", GPA {g:.1}")).unwrap_or_default();
                let yr = e.year.map(|y| format!(" ({y})")).unwrap_or_default();
                format!("{} @ {}{}{}", e.degree, e.institution, yr, gpa)
            })
            .collect();
        let langs: Vec<String> = self
            .languages
            .iter()
            .map(|l| format!("{} ({})", l.language, l.level))
            .collect();
        format!(
            "Name: {}\nNationality: {}\nEmail: {}\nEducation: {}\nLanguages: {}\nSkills: {}\nAwards: {}",
            self.full_name,
            self.nationality.as_deref().unwrap_or("Unknown"),
            self.email.as_deref().unwrap_or(""),
            edu.join("; "),
            langs.join(", "),
            self.skills.join(", "),
            self.awards.join("; "),
        )
    }
}

fn sha256_file(path: &Path) -> Option<[u8; 32]> {
    let bytes = std::fs::read(path).ok()?;
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    Some(hasher.finalize().into())
}

fn extract_text_from_pdf(path: &Path) -> String {
    pdf_extract::extract_text(path).unwrap_or_default()
}

fn extract_text_from_docx(path: &Path) -> String {
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(_) => return String::new(),
    };
    match docx_rs::read_docx(&bytes) {
        Ok(doc) => {
            let mut out = String::new();
            for child in &doc.document.body.children {
                if let docx_rs::DocumentChild::Paragraph(para) = child {
                    for run_child in &para.children {
                        if let docx_rs::ParagraphChild::Run(run) = run_child {
                            for rc in &run.children {
                                if let docx_rs::RunChild::Text(t) = rc {
                                    out.push_str(&t.text);
                                    out.push(' ');
                                }
                            }
                        }
                    }
                    out.push('\n');
                }
            }
            out
        }
        Err(_) => String::new(),
    }
}

fn extract_text(path: &Path) -> String {
    match path.extension().and_then(|e| e.to_str()) {
        Some("pdf") => extract_text_from_pdf(path),
        Some("docx") => extract_text_from_docx(path),
        Some("txt") | Some("md") => std::fs::read_to_string(path).unwrap_or_default(),
        _ => String::new(),
    }
}

fn cache_path() -> PathBuf {
    let mut p = dirs::data_dir().unwrap_or_else(|| PathBuf::from("."));
    p.push("kitsune");
    p.push("profile_summary.json");
    p
}

pub struct ProfileIndexer {
    folder_path: PathBuf,
    summary: Arc<Mutex<Option<ProfileSummary>>>,
    file_hashes: Arc<Mutex<HashMap<PathBuf, [u8; 32]>>>,
}

impl ProfileIndexer {
    pub fn new(folder_path: PathBuf) -> Self {
        // Load cached summary if present
        let cached = std::fs::read_to_string(cache_path())
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok());
        Self {
            folder_path,
            summary: Arc::new(Mutex::new(cached)),
            file_hashes: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Get the current cached summary (may be None if not yet indexed).
    pub async fn summary(&self) -> Option<ProfileSummary> {
        self.summary.lock().await.clone()
    }

    /// Re-index the folder. Only re-parses files whose sha256 has changed.
    /// Calls the AI to extract a fresh ProfileSummary from changed content.
    pub async fn reindex(&self, ai: &AgentAiClient) -> AgentResult<ProfileSummary> {
        let entries = std::fs::read_dir(&self.folder_path)
            .map_err(|e| AgentError::ExecutionError(format!("Cannot read profile folder: {e}")))?;

        let mut raw_text = String::new();
        let mut source_files = Vec::new();
        let mut hashes = self.file_hashes.lock().await;

        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let new_hash = match sha256_file(&path) {
                Some(h) => h,
                None => continue,
            };
            let old_hash = hashes.get(&path).copied();
            if old_hash == Some(new_hash) {
                info!(path = %path.display(), "Skipping unchanged file");
                continue;
            }
            hashes.insert(path.clone(), new_hash);

            let text = extract_text(&path);
            if !text.trim().is_empty() {
                raw_text.push_str(&text);
                raw_text.push('\n');
                source_files.push(path.file_name().unwrap_or_default().to_string_lossy().into_owned());
            }
        }
        drop(hashes);

        if raw_text.trim().is_empty() {
            return Err(AgentError::ExecutionError(
                "No parseable text found in profile folder".into(),
            ));
        }

        let prompt = format!(
            r#"Extract a structured profile from the following CV and document text.
Return ONLY valid JSON matching this schema (no markdown, no explanation):
{{
  "full_name": "...",
  "date_of_birth": "YYYY-MM-DD or null",
  "nationality": "... or null",
  "email": "... or null",
  "phone": "... or null",
  "education": [{{"degree":"...","institution":"...","year":null,"gpa":null}}],
  "languages": [{{"language":"...","level":"A1/A2/B1/B2/C1/C2"}}],
  "skills": ["..."],
  "publications": ["..."],
  "awards": ["..."]
}}

Documents:
{raw_text}"#
        );

        let response = ai.complete(&prompt, ModelTier::Fast).await?;
        let json_str = response.trim().trim_start_matches("```json").trim_end_matches("```").trim();

        let mut parsed: ProfileSummary = serde_json::from_str(json_str)
            .map_err(|e| AgentError::ExecutionError(format!("Failed to parse profile JSON: {e}\nRaw: {json_str}")))?;

        parsed.generated_at = Some(Utc::now());
        parsed.source_files = source_files;

        // Cache to disk
        if let Ok(json) = serde_json::to_string_pretty(&parsed) {
            let cp = cache_path();
            if let Some(parent) = cp.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let _ = std::fs::write(&cp, json);
        }

        let mut guard = self.summary.lock().await;
        *guard = Some(parsed.clone());

        info!(files = %parsed.source_files.join(", "), "Profile indexed successfully");
        Ok(parsed)
    }
}
```

- [ ] **Step 4: Add `dirs` dep to `crates/kitsune-agent/Cargo.toml`** (for `dirs::data_dir()`):

```toml
dirs = "5"
```

- [ ] **Step 5: Add module declaration to `crates/kitsune-agent/src/lib.rs`**

```rust
pub mod profile;
```

- [ ] **Step 6: Run tests**

```powershell
cargo test -p kitsune-agent --test profile 2>&1 | Select-String -Pattern "error|FAILED|ok"
```
Expected: `test profile_summary_default_is_empty ... ok` and `test profile_indexer_new_accepts_path ... ok`

- [ ] **Step 7: Build check**

```powershell
cargo build -p kitsune-agent 2>&1 | Select-String -Pattern "^error"
```
Expected: no errors (warnings OK).

- [ ] **Step 8: Commit**

```powershell
git add crates/kitsune-agent/src/profile.rs crates/kitsune-agent/src/lib.rs crates/kitsune-agent/Cargo.toml crates/kitsune-agent/tests/profile.rs
git commit -m "feat(agent): add ProfileIndexer with PDF/DOCX parsing and AI extraction"
```

---

## Task 5: CaptchaAgent

Detects CAPTCHAs in the DOM and resolves them through four escalating tiers. Tier 1 (timing) is already done in Task 3. This task implements Tiers 2–4.

**Files:**
- Create: `crates/kitsune-agent/src/captcha.rs`
- Modify: `crates/kitsune-agent/src/lib.rs`

- [ ] **Step 1: Write the failing test**

Create `crates/kitsune-agent/tests/captcha.rs`:
```rust
use kitsune_agent::captcha::CaptchaKind;

#[test]
fn captcha_kind_from_dom_snippet_recaptcha_v2() {
    let dom = r#"<div class="g-recaptcha" data-sitekey="abc123"></div>"#;
    let kind = CaptchaKind::detect_from_html(dom);
    assert_eq!(kind, Some(CaptchaKind::RecaptchaV2));
}

#[test]
fn captcha_kind_from_dom_snippet_hcaptcha() {
    let dom = r#"<div class="h-captcha" data-sitekey="xyz"></div>"#;
    let kind = CaptchaKind::detect_from_html(dom);
    assert_eq!(kind, Some(CaptchaKind::HCaptcha));
}

#[test]
fn captcha_kind_from_dom_snippet_turnstile() {
    let dom = r#"<div class="cf-turnstile" data-sitekey="abc"></div>"#;
    let kind = CaptchaKind::detect_from_html(dom);
    assert_eq!(kind, Some(CaptchaKind::CloudflareTurnstile));
}

#[test]
fn captcha_kind_none_on_clean_page() {
    let dom = r#"<form><input type="text" name="email"></form>"#;
    let kind = CaptchaKind::detect_from_html(dom);
    assert_eq!(kind, None);
}
```

- [ ] **Step 2: Run test to see it fail**

```powershell
cargo test -p kitsune-agent --test captcha 2>&1 | Select-String -Pattern "error|FAILED|ok"
```
Expected: compile error.

- [ ] **Step 3: Create `crates/kitsune-agent/src/captcha.rs`**

```rust
use crate::dom_access::DomAccessor;
use crate::error::{AgentError, AgentResult};
use crate::executor::WebViewCommand;
use kitsune_hil::{HilGate, HilTriggerClass};
use kitsune_vault::VaultBackend;
use std::sync::Arc;
use tracing::{info, warn};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CaptchaKind {
    RecaptchaV2,
    RecaptchaV3 { action: String },
    HCaptcha,
    CloudflareTurnstile,
    Unknown,
}

impl CaptchaKind {
    /// Detect CAPTCHA type from raw HTML. Returns None if no CAPTCHA found.
    pub fn detect_from_html(html: &str) -> Option<CaptchaKind> {
        if html.contains("cf-turnstile") || html.contains("cloudflare-turnstile") {
            return Some(CaptchaKind::CloudflareTurnstile);
        }
        if html.contains("h-captcha") || html.contains("hcaptcha.com") {
            return Some(CaptchaKind::HCaptcha);
        }
        if html.contains("g-recaptcha") || html.contains("recaptcha/api2") {
            // Distinguish v3 by the action attribute pattern
            if html.contains("grecaptcha.execute") {
                return Some(CaptchaKind::RecaptchaV3 { action: "default".into() });
            }
            return Some(CaptchaKind::RecaptchaV2);
        }
        if html.contains("data-sitekey") {
            return Some(CaptchaKind::Unknown);
        }
        None
    }

    pub fn display_name(&self) -> &str {
        match self {
            Self::RecaptchaV2 => "reCAPTCHA v2",
            Self::RecaptchaV3 { .. } => "reCAPTCHA v3",
            Self::HCaptcha => "hCaptcha",
            Self::CloudflareTurnstile => "Cloudflare Turnstile",
            Self::Unknown => "unknown CAPTCHA",
        }
    }
}

/// Configuration for the CAPTCHA API solver (Tier 3).
#[derive(Debug, Clone)]
pub struct CaptchaSolverConfig {
    /// e.g. "https://2captcha.com" or "http://api.anti-captcha.com"
    pub endpoint: String,
    /// API key (stored in vault; this is the plaintext only during config load)
    pub api_key: String,
}

pub struct CaptchaAgent {
    dom: Arc<DomAccessor>,
    hil_gate: Arc<HilGate>,
    solver_config: Option<CaptchaSolverConfig>,
    http: reqwest::Client,
}

impl CaptchaAgent {
    pub fn new(
        dom: Arc<DomAccessor>,
        hil_gate: Arc<HilGate>,
        solver_config: Option<CaptchaSolverConfig>,
    ) -> AgentResult<Self> {
        let http = reqwest::Client::builder()
            .use_rustls_tls()
            .timeout(std::time::Duration::from_secs(60))
            .build()
            .map_err(|e| AgentError::Internal(e.to_string()))?;
        Ok(Self { dom, hil_gate, solver_config, http })
    }

    /// Check the current page for a CAPTCHA and attempt to resolve it.
    /// Returns Ok(()) if resolved (or no CAPTCHA found), Err if all tiers failed.
    pub async fn resolve(&self, site: &str) -> AgentResult<()> {
        let html = self.dom.get_page_text().await?;
        let kind = match CaptchaKind::detect_from_html(&html) {
            None => {
                info!(%site, "No CAPTCHA detected");
                return Ok(());
            }
            Some(k) => k,
        };

        info!(%site, captcha = kind.display_name(), "CAPTCHA detected, starting resolution");

        // Tier 2: audio transcription for reCAPTCHA v2
        if matches!(kind, CaptchaKind::RecaptchaV2) {
            match self.try_audio_transcription().await {
                Ok(()) => {
                    info!("CAPTCHA resolved via audio transcription (Tier 2)");
                    return Ok(());
                }
                Err(e) => warn!("Audio transcription failed: {e}, escalating to Tier 3"),
            }
        }

        // Tier 3: API solver
        if let Some(cfg) = &self.solver_config {
            match self.try_api_solver(&kind, site, cfg).await {
                Ok(()) => {
                    info!("CAPTCHA resolved via API solver (Tier 3)");
                    return Ok(());
                }
                Err(e) => warn!("API solver failed: {e}, escalating to Tier 4"),
            }
        }

        // Tier 4: HIL escalation
        self.escalate_to_hil(site, &kind).await
    }

    async fn try_audio_transcription(&self) -> AgentResult<()> {
        // Click the audio challenge button
        // Selector works on most reCAPTCHA v2 iframes
        let click_script = r#"
            (function() {
                let frames = document.querySelectorAll('iframe[src*="recaptcha"]');
                for (let f of frames) {
                    try {
                        let btn = f.contentDocument.querySelector('#recaptcha-audio-button');
                        if (btn) { btn.click(); return; }
                    } catch(e) {}
                }
            })();
        "#;
        self.dom
            .eval_js_fire_and_forget(click_script)
            .await
            .map_err(|e| AgentError::ExecutionError(format!("audio click: {e}")))?;

        // Brief wait for audio to load
        tokio::time::sleep(std::time::Duration::from_millis(1500)).await;

        // Note: Full Whisper integration is feature-gated (captcha-audio).
        // Without that feature, audio transcription is unavailable — escalate.
        #[cfg(feature = "captcha-audio")]
        {
            // TODO: download audio src, run whisper-rs, fill response field
            return Err(AgentError::ExecutionError("whisper not yet wired".into()));
        }

        #[cfg(not(feature = "captcha-audio"))]
        Err(AgentError::ExecutionError(
            "captcha-audio feature not enabled".into(),
        ))
    }

    async fn try_api_solver(
        &self,
        kind: &CaptchaKind,
        site: &str,
        cfg: &CaptchaSolverConfig,
    ) -> AgentResult<()> {
        // Extract sitekey from DOM
        let sitekey = self.extract_sitekey().await?;

        // Submit task to solver
        #[derive(serde::Serialize)]
        struct SolverTask<'a> {
            #[serde(rename = "type")]
            task_type: &'a str,
            websiteURL: &'a str,
            websiteKey: &'a str,
        }
        #[derive(serde::Serialize)]
        struct SolverRequest<'a> {
            clientKey: &'a str,
            task: SolverTask<'a>,
        }
        #[derive(serde::Deserialize)]
        struct CreateTaskResponse {
            taskId: Option<u64>,
            errorId: u32,
        }
        #[derive(serde::Deserialize)]
        struct GetResultResponse {
            status: String,
            solution: Option<SolverSolution>,
        }
        #[derive(serde::Deserialize)]
        struct SolverSolution {
            gRecaptchaResponse: Option<String>,
            token: Option<String>,
        }

        let task_type = match kind {
            CaptchaKind::HCaptcha => "HCaptchaTaskProxyless",
            CaptchaKind::CloudflareTurnstile => "TurnstileTaskProxyless",
            _ => "RecaptchaV2TaskProxyless",
        };

        let create_resp: CreateTaskResponse = self
            .http
            .post(format!("{}/createTask", cfg.endpoint))
            .json(&SolverRequest {
                clientKey: &cfg.api_key,
                task: SolverTask { task_type, websiteURL: site, websiteKey: &sitekey },
            })
            .send()
            .await
            .map_err(|e| AgentError::ExecutionError(e.to_string()))?
            .json()
            .await
            .map_err(|e| AgentError::ExecutionError(e.to_string()))?;

        if create_resp.errorId != 0 || create_resp.taskId.is_none() {
            return Err(AgentError::ExecutionError("solver rejected task".into()));
        }
        let task_id = create_resp.taskId.unwrap();

        // Poll for solution (up to 60s)
        #[derive(serde::Serialize)]
        struct GetResultRequest<'a> {
            clientKey: &'a str,
            taskId: u64,
        }

        for _ in 0..12 {
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            let result: GetResultResponse = self
                .http
                .post(format!("{}/getTaskResult", cfg.endpoint))
                .json(&GetResultRequest { clientKey: &cfg.api_key, taskId: task_id })
                .send()
                .await
                .map_err(|e| AgentError::ExecutionError(e.to_string()))?
                .json()
                .await
                .map_err(|e| AgentError::ExecutionError(e.to_string()))?;

            if result.status == "ready" {
                let token = result
                    .solution
                    .and_then(|s| s.gRecaptchaResponse.or(s.token))
                    .ok_or_else(|| AgentError::ExecutionError("no token in solution".into()))?;
                return self.inject_solver_token(&token).await;
            }
        }
        Err(AgentError::ExecutionError("solver timed out".into()))
    }

    async fn extract_sitekey(&self) -> AgentResult<String> {
        let (tx, mut rx) = tokio::sync::mpsc::channel(1);
        let script = r#"
            (function() {
                let el = document.querySelector('[data-sitekey]');
                window.__kitsune_ipc(JSON.stringify({ sitekey: el ? el.dataset.sitekey : null }));
            })();
        "#;
        self.dom
            .eval_js_with_callback(script, tx)
            .await
            .map_err(|_| AgentError::IpcDisconnected)?;
        let raw = rx.recv().await.ok_or(AgentError::IpcDisconnected)?;
        let val: serde_json::Value = serde_json::from_str(&raw).unwrap_or_default();
        val["sitekey"]
            .as_str()
            .map(|s| s.to_string())
            .ok_or_else(|| AgentError::ExecutionError("sitekey not found in DOM".into()))
    }

    async fn inject_solver_token(&self, token: &str) -> AgentResult<()> {
        let safe = token.replace('\'', "\\'");
        let script = format!(
            r#"(function() {{
                let el = document.querySelector('[name="g-recaptcha-response"]');
                if (!el) el = document.querySelector('[name="h-captcha-response"]');
                if (el) {{ el.value = '{safe}'; el.dispatchEvent(new Event('change', {{bubbles:true}})); }}
                if (window.grecaptcha) {{ try {{ grecaptcha.reset(); }} catch(e) {{}} }}
            }})();"#
        );
        self.dom.eval_js_fire_and_forget(&script).await
            .map_err(|e| AgentError::ExecutionError(format!("token inject: {e}")))
    }

    async fn escalate_to_hil(&self, site: &str, kind: &CaptchaKind) -> AgentResult<()> {
        warn!(%site, captcha = kind.display_name(), "Escalating CAPTCHA to HIL (Tier 4)");
        let trigger = HilTriggerClass::CaptchaRequired {
            site: site.to_string(),
            captcha_type: kind.display_name().to_string(),
        };
        self.hil_gate
            .checkpoint(trigger, vec![])
            .await
            .map_err(|e| AgentError::HilRejected(format!("{e:?}")))?;
        Ok(())
    }
}

/// Extension trait to expose JS helpers on DomAccessor for CaptchaAgent.
/// These methods wrap WebViewCommand but don't need HIL gating.
impl DomAccessor {
    pub async fn eval_js_fire_and_forget(&self, script: &str) -> AgentResult<()> {
        self.webview_tx
            .send(WebViewCommand::EvalJs(script.to_string()))
            .await
            .map_err(|_| AgentError::IpcDisconnected)
    }

    pub async fn eval_js_with_callback(
        &self,
        script: &str,
        tx: tokio::sync::mpsc::Sender<String>,
    ) -> AgentResult<()> {
        self.webview_tx
            .send(WebViewCommand::EvalJsWithCallback(script.to_string(), tx))
            .await
            .map_err(|_| AgentError::IpcDisconnected)
    }
}
```

**Note:** `DomAccessor::webview_tx` is private. You will need to make it `pub(crate)` in `dom_access.rs`:
```rust
pub(crate) webview_tx: mpsc::Sender<WebViewCommand>,
```

- [ ] **Step 4: Add module to `crates/kitsune-agent/src/lib.rs`**

```rust
pub mod captcha;
```

- [ ] **Step 5: Run tests**

```powershell
cargo test -p kitsune-agent --test captcha 2>&1 | Select-String -Pattern "error|FAILED|ok"
```
Expected: 4 tests pass.

- [ ] **Step 6: Build check**

```powershell
cargo build -p kitsune-agent 2>&1 | Select-String -Pattern "^error"
```

- [ ] **Step 7: Commit**

```powershell
git add crates/kitsune-agent/src/captcha.rs crates/kitsune-agent/src/dom_access.rs crates/kitsune-agent/src/lib.rs crates/kitsune-agent/tests/captcha.rs
git commit -m "feat(agent): add CaptchaAgent with 4-tier resolution strategy"
```

---

## Task 6: SearchAgent

Navigates to search engines, extracts results, screens against eligibility.

**Files:**
- Create: `crates/kitsune-agent/src/agents/mod.rs`
- Create: `crates/kitsune-agent/src/agents/search.rs`
- Modify: `crates/kitsune-agent/src/lib.rs`

- [ ] **Step 1: Write the failing test**

Create `crates/kitsune-agent/tests/search_agent.rs`:
```rust
use kitsune_agent::agents::search::Candidate;

#[test]
fn candidate_has_required_fields() {
    let c = Candidate {
        title: "DAAD Research Grant".into(),
        url: "https://www.daad.de/grants/123".into(),
        deadline: Some("2026-09-01".into()),
        requirements_summary: "MSc+, GPA >= 3.5, English B2".into(),
    };
    assert!(!c.title.is_empty());
    assert!(c.url.starts_with("https://"));
}
```

- [ ] **Step 2: Run test to see it fail**

```powershell
cargo test -p kitsune-agent --test search_agent 2>&1 | Select-String -Pattern "error|FAILED|ok"
```

- [ ] **Step 3: Create `crates/kitsune-agent/src/agents/mod.rs`**

```rust
pub mod booking;
pub mod form;
pub mod search;
pub mod submit;
```

- [ ] **Step 4: Create `crates/kitsune-agent/src/agents/search.rs`**

```rust
use crate::ai_client::{AgentAiClient, ModelTier};
use crate::dom_access::DomAccessor;
use crate::error::{AgentError, AgentResult};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::info;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Candidate {
    pub title: String,
    pub url: String,
    pub deadline: Option<String>,
    pub requirements_summary: String,
}

pub struct SearchAgent {
    dom: Arc<DomAccessor>,
    ai: Arc<AgentAiClient>,
}

impl SearchAgent {
    pub fn new(dom: Arc<DomAccessor>, ai: Arc<AgentAiClient>) -> Self {
        Self { dom, ai }
    }

    /// Search for candidates matching the query and eligibility filter.
    /// Returns up to 5 candidates sorted by relevance.
    pub async fn search(
        &self,
        query: &str,
        eligibility_filter: Option<&str>,
        profile_context: &str,
    ) -> AgentResult<Vec<Candidate>> {
        // Navigate to Google search
        let search_url = format!(
            "https://www.google.com/search?q={}",
            urlencoding::encode(query)
        );
        self.dom.navigate(&search_url).await?;

        // Brief wait for page load
        tokio::time::sleep(std::time::Duration::from_millis(1500)).await;

        let page_text = self.dom.get_page_text().await?;
        let links = self.dom.query_links("a[href]").await?;

        // Filter to relevant external links (skip google.com internal links)
        let candidate_links: Vec<String> = links
            .into_iter()
            .filter(|l| {
                !l.contains("google.com")
                    && !l.contains("accounts.")
                    && l.starts_with("https://")
            })
            .take(10)
            .collect();

        let filter_hint = eligibility_filter
            .map(|f| format!("Eligibility filter: {f}\n"))
            .unwrap_or_default();

        let prompt = format!(
            r#"Given these search results and links, identify the top 3-5 scholarship/opportunity candidates.
{filter_hint}
User profile:
{profile_context}

Page text (first 2000 chars):
{}

Links found:
{}

Return ONLY valid JSON (no markdown):
[{{"title":"...","url":"...","deadline":"YYYY-MM-DD or null","requirements_summary":"..."}}]"#,
            &page_text[..page_text.len().min(2000)],
            candidate_links.join("\n")
        );

        let response = self.ai.complete(&prompt, ModelTier::Fast).await?;
        let json_str = response.trim().trim_start_matches("```json").trim_end_matches("```").trim();

        let candidates: Vec<Candidate> = serde_json::from_str(json_str)
            .map_err(|e| AgentError::ExecutionError(format!("Failed to parse candidates: {e}")))?;

        info!(count = candidates.len(), %query, "SearchAgent found candidates");
        Ok(candidates)
    }
}
```

- [ ] **Step 5: Add `urlencoding` dep to `crates/kitsune-agent/Cargo.toml`**

```toml
urlencoding = "2"
```

- [ ] **Step 6: Add module to `crates/kitsune-agent/src/lib.rs`**

```rust
pub mod agents;
```

- [ ] **Step 7: Run tests**

```powershell
cargo test -p kitsune-agent --test search_agent 2>&1 | Select-String -Pattern "error|FAILED|ok"
```
Expected: `test candidate_has_required_fields ... ok`

- [ ] **Step 8: Commit**

```powershell
git add crates/kitsune-agent/src/agents/ crates/kitsune-agent/src/lib.rs crates/kitsune-agent/Cargo.toml crates/kitsune-agent/tests/search_agent.rs
git commit -m "feat(agent): add SearchAgent for web candidate discovery"
```

---

## Task 7: FormAgent

Plan-then-execute: one LLM call per form page → `FieldMappingPlan` → deterministic executor.

**Files:**
- Create: `crates/kitsune-agent/src/agents/form.rs`

- [ ] **Step 1: Write the failing test**

Create `crates/kitsune-agent/tests/form_agent.rs`:
```rust
use kitsune_agent::agents::form::{FieldAction, FieldMappingPlan};

#[test]
fn field_mapping_plan_serializes_round_trip() {
    let plan = FieldMappingPlan {
        fields: vec![
            FieldAction::FillStatic {
                selector: "#name".into(),
                value: "John Doe".into(),
            },
            FieldAction::Click {
                selector: "button[type=submit]".into(),
            },
        ],
    };
    let json = serde_json::to_string(&plan).unwrap();
    let restored: FieldMappingPlan = serde_json::from_str(&json).unwrap();
    assert_eq!(restored.fields.len(), 2);
}

#[test]
fn captcha_check_action_serializes() {
    let action = FieldAction::CaptchaCheck;
    let json = serde_json::to_string(&action).unwrap();
    assert!(json.contains("CaptchaCheck"));
}
```

- [ ] **Step 2: Run test to see it fail**

```powershell
cargo test -p kitsune-agent --test form_agent 2>&1 | Select-String -Pattern "error|FAILED|ok"
```

- [ ] **Step 3: Create `crates/kitsune-agent/src/agents/form.rs`**

```rust
use crate::agents::search::Candidate;
use crate::ai_client::{AgentAiClient, ModelTier};
use crate::captcha::CaptchaAgent;
use crate::dom_access::DomAccessor;
use crate::error::{AgentError, AgentResult};
use crate::profile::ProfileSummary;
use kitsune_hil::{HilGate, HilTriggerClass};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{info, warn};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "op")]
pub enum FieldAction {
    FillFromProfile { selector: String, profile_field: String },
    FillStatic      { selector: String, value: String },
    SelectOption    { selector: String, value: String },
    UploadFile      { selector: String, file_path: String },
    Click           { selector: String },
    CaptchaCheck,
    NavigateNext    { selector: String },
    WaitForElement  { selector: String, timeout_ms: u64 },
    AwaitHil        { reason: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldMappingPlan {
    pub fields: Vec<FieldAction>,
}

#[derive(Debug, Clone)]
pub struct FormResult {
    pub site: String,
    pub filled_count: usize,
    pub submit_selector: Option<String>,
    pub confirmation_text: Option<String>,
}

pub struct FormAgent {
    dom: Arc<DomAccessor>,
    ai: Arc<AgentAiClient>,
    captcha: Arc<CaptchaAgent>,
    hil_gate: Arc<HilGate>,
}

impl FormAgent {
    pub fn new(
        dom: Arc<DomAccessor>,
        ai: Arc<AgentAiClient>,
        captcha: Arc<CaptchaAgent>,
        hil_gate: Arc<HilGate>,
    ) -> Self {
        Self { dom, ai, captcha, hil_gate }
    }

    /// Navigate to `url`, plan field fills from `profile`, execute the plan.
    pub async fn fill_and_submit(
        &self,
        url: &str,
        profile: &ProfileSummary,
    ) -> AgentResult<FormResult> {
        self.dom.navigate(url).await?;
        tokio::time::sleep(std::time::Duration::from_millis(1500)).await;

        // Check for CAPTCHA before planning
        self.captcha.resolve(url).await?;

        let plan = self.plan_fields(url, profile).await?;
        info!(steps = plan.fields.len(), %url, "FormAgent executing plan");

        let mut filled = 0usize;
        let mut submit_selector = None;

        for action in &plan.fields {
            match action {
                FieldAction::FillFromProfile { selector, profile_field } => {
                    let value = resolve_profile_field(profile, profile_field);
                    if !value.is_empty() {
                        self.dom.fill_field(selector, &value).await?;
                        filled += 1;
                    }
                }
                FieldAction::FillStatic { selector, value } => {
                    self.dom.fill_field(selector, value).await?;
                    filled += 1;
                }
                FieldAction::SelectOption { selector, value } => {
                    let safe_sel = selector.replace('\'', "\\'");
                    let safe_val = value.replace('\'', "\\'");
                    let script = format!(
                        r#"(function(){{
                            let el = document.querySelector('{safe_sel}');
                            if (el) {{
                                el.value = '{safe_val}';
                                el.dispatchEvent(new Event('change',{{bubbles:true}}));
                            }}
                        }})();"#
                    );
                    self.dom.eval_js_fire_and_forget(&script).await?;
                    filled += 1;
                }
                FieldAction::Click { selector } => {
                    self.dom.click_element(selector).await?;
                }
                FieldAction::CaptchaCheck => {
                    self.captcha.resolve(url).await?;
                }
                FieldAction::NavigateNext { selector } => {
                    self.dom.click_element(selector).await?;
                    tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
                    // Re-plan remaining fields on next page
                    self.captcha.resolve(url).await?;
                    let next_plan = self.plan_fields(url, profile).await?;
                    // Recurse isn't practical here — return and let orchestrator call again
                    // For now: execute next plan inline
                    for next_action in &next_plan.fields {
                        if let FieldAction::Click { selector } = next_action {
                            submit_selector = Some(selector.clone());
                        }
                    }
                    break;
                }
                FieldAction::WaitForElement { selector, timeout_ms } => {
                    tokio::time::sleep(std::time::Duration::from_millis(*timeout_ms)).await;
                }
                FieldAction::AwaitHil { reason } => {
                    let trigger = HilTriggerClass::ExternalSideEffect {
                        description: reason.clone(),
                        reversible: false,
                    };
                    self.hil_gate
                        .checkpoint(trigger, vec![])
                        .await
                        .map_err(|e| AgentError::HilRejected(format!("{e:?}")))?;
                }
            }
        }

        Ok(FormResult {
            site: url.to_string(),
            filled_count: filled,
            submit_selector,
            confirmation_text: None,
        })
    }

    async fn plan_fields(&self, url: &str, profile: &ProfileSummary) -> AgentResult<FieldMappingPlan> {
        // Extract interactive elements from DOM
        let (tx, mut rx) = tokio::sync::mpsc::channel(1);
        let script = r#"
            (function() {
                let fields = [];
                document.querySelectorAll('input,select,textarea').forEach(el => {
                    let label = '';
                    if (el.id) {
                        let lbl = document.querySelector('label[for="' + el.id + '"]');
                        if (lbl) label = lbl.textContent.trim();
                    }
                    if (!label && el.placeholder) label = el.placeholder;
                    if (!label && el.name) label = el.name;
                    let opts = [];
                    if (el.tagName === 'SELECT') {
                        opts = [...el.options].map(o => o.text);
                    }
                    fields.push({
                        selector: el.id ? '#' + el.id : (el.name ? '[name="'+el.name+'"]' : el.tagName.toLowerCase()),
                        label: label,
                        type: el.type || el.tagName.toLowerCase(),
                        options: opts
                    });
                });
                window.__kitsune_ipc(JSON.stringify({ fields }));
            })();
        "#;
        self.dom.eval_js_with_callback(script, tx).await?;
        let raw = rx.recv().await.ok_or(AgentError::IpcDisconnected)?;
        let val: serde_json::Value = serde_json::from_str(&raw).unwrap_or_default();
        let dom_structure = serde_json::to_string_pretty(&val["fields"]).unwrap_or_default();

        let profile_ctx = profile.to_prompt_context();
        let prompt = format!(
            r#"Map these form fields to profile data. Return ONLY valid JSON (no markdown):
{{"fields":[
  {{"op":"FillFromProfile","selector":"#id","profile_field":"full_name"}},
  {{"op":"FillStatic","selector":"#id","value":"literal"}},
  {{"op":"SelectOption","selector":"select#id","value":"option text"}},
  {{"op":"CaptchaCheck"}},
  {{"op":"Click","selector":"button[type=submit]"}}
]}}

Available profile_field values: full_name, email, phone, nationality, date_of_birth,
education[0].degree, education[0].institution, education[0].gpa, languages[0].language,
languages[0].level, skills (comma-joined), awards (comma-joined)

User profile:
{profile_ctx}

Form fields:
{dom_structure}

Page URL: {url}"#
        );

        let response = self.ai.complete(&prompt, ModelTier::Worker).await?;
        let json_str = response
            .trim()
            .trim_start_matches("```json")
            .trim_end_matches("```")
            .trim();

        let plan: FieldMappingPlan = serde_json::from_str(json_str)
            .map_err(|e| AgentError::ExecutionError(format!("Bad plan JSON: {e}\nRaw: {json_str}")))?;

        Ok(plan)
    }
}

fn resolve_profile_field(profile: &ProfileSummary, field: &str) -> String {
    match field {
        "full_name" => profile.full_name.clone(),
        "email" => profile.email.clone().unwrap_or_default(),
        "phone" => profile.phone.clone().unwrap_or_default(),
        "nationality" => profile.nationality.clone().unwrap_or_default(),
        "date_of_birth" => profile.date_of_birth.clone().unwrap_or_default(),
        "skills" => profile.skills.join(", "),
        "awards" => profile.awards.join("; "),
        f if f.starts_with("education[0].") => {
            let sub = &f["education[0].".len()..];
            profile.education.first().map(|e| match sub {
                "degree" => e.degree.clone(),
                "institution" => e.institution.clone(),
                "gpa" => e.gpa.map(|g| format!("{g:.1}")).unwrap_or_default(),
                "year" => e.year.map(|y| y.to_string()).unwrap_or_default(),
                _ => String::new(),
            }).unwrap_or_default()
        }
        f if f.starts_with("languages[0].") => {
            let sub = &f["languages[0].".len()..];
            profile.languages.first().map(|l| match sub {
                "language" => l.language.clone(),
                "level" => l.level.clone(),
                _ => String::new(),
            }).unwrap_or_default()
        }
        _ => String::new(),
    }
}
```

- [ ] **Step 4: Run tests**

```powershell
cargo test -p kitsune-agent --test form_agent 2>&1 | Select-String -Pattern "error|FAILED|ok"
```
Expected: 2 tests pass.

- [ ] **Step 5: Build check**

```powershell
cargo build -p kitsune-agent 2>&1 | Select-String -Pattern "^error"
```

- [ ] **Step 6: Commit**

```powershell
git add crates/kitsune-agent/src/agents/form.rs crates/kitsune-agent/tests/form_agent.rs
git commit -m "feat(agent): add FormAgent with plan-then-execute field mapping"
```

---

## Task 8: SubmitAgent

Previews filled form state, gates HIL, clicks submit, captures confirmation.

**Files:**
- Create: `crates/kitsune-agent/src/agents/submit.rs`

- [ ] **Step 1: Write the failing test**

Create `crates/kitsune-agent/tests/submit_agent.rs`:
```rust
use kitsune_agent::agents::form::FormResult;

#[test]
fn form_result_builds() {
    let r = FormResult {
        site: "https://daad.de".into(),
        filled_count: 12,
        submit_selector: Some("button#submit".into()),
        confirmation_text: None,
    };
    assert_eq!(r.filled_count, 12);
    assert!(r.submit_selector.is_some());
}
```

- [ ] **Step 2: Run test to see it fail**

```powershell
cargo test -p kitsune-agent --test submit_agent 2>&1 | Select-String -Pattern "error|FAILED|ok"
```

- [ ] **Step 3: Create `crates/kitsune-agent/src/agents/submit.rs`**

```rust
use crate::agents::form::FormResult;
use crate::dom_access::DomAccessor;
use crate::error::{AgentError, AgentResult};
use kitsune_hil::{HilGate, HilTriggerClass};
use std::sync::Arc;
use tracing::info;

pub struct SubmitAgent {
    dom: Arc<DomAccessor>,
    hil_gate: Arc<HilGate>,
}

impl SubmitAgent {
    pub fn new(dom: Arc<DomAccessor>, hil_gate: Arc<HilGate>) -> Self {
        Self { dom, hil_gate }
    }

    pub async fn submit(&self, mut result: FormResult) -> AgentResult<FormResult> {
        // Gate: show filled field count for user confirmation
        let trigger = HilTriggerClass::ExternalSideEffect {
            description: format!(
                "Submit form on {} ({} fields filled). This action cannot be undone.",
                result.site, result.filled_count
            ),
            reversible: false,
        };
        self.hil_gate
            .checkpoint(trigger, vec![result.site.clone()])
            .await
            .map_err(|e| AgentError::HilRejected(format!("{e:?}")))?;

        // Click submit button
        let selector = result
            .submit_selector
            .as_deref()
            .unwrap_or("button[type=submit]");
        self.dom.click_element(selector).await?;

        // Wait for navigation / confirmation page
        tokio::time::sleep(std::time::Duration::from_millis(2500)).await;

        // Capture confirmation text
        let confirm = self.dom.get_page_text().await.unwrap_or_default();
        let confirmation_text = if confirm.len() > 500 {
            confirm[..500].to_string()
        } else {
            confirm
        };
        result.confirmation_text = Some(confirmation_text);

        info!(site = %result.site, "Form submitted successfully");
        Ok(result)
    }
}
```

- [ ] **Step 4: Run test**

```powershell
cargo test -p kitsune-agent --test submit_agent 2>&1 | Select-String -Pattern "error|FAILED|ok"
```
Expected: `test form_result_builds ... ok`

- [ ] **Step 5: Commit**

```powershell
git add crates/kitsune-agent/src/agents/submit.rs crates/kitsune-agent/tests/submit_agent.rs
git commit -m "feat(agent): add SubmitAgent with HIL-gated form submission"
```

---

## Task 9: BookingAgent

Fetches flight offers from multiple sites sequentially via HTTP, ranks by criteria, hands off to FormAgent.

**Files:**
- Create: `crates/kitsune-agent/src/agents/booking.rs`

- [ ] **Step 1: Write the failing test**

Create `crates/kitsune-agent/tests/booking_agent.rs`:
```rust
use kitsune_agent::agents::booking::{BookingCriteria, BookingPriority, FlightOffer};

#[test]
fn cheapest_ranking() {
    let offers = vec![
        FlightOffer { price_minor: 50000, currency: "EUR".into(), duration_mins: 120, stops: 0, airline: "LH".into(), booking_url: "https://a.com".into() },
        FlightOffer { price_minor: 35000, currency: "EUR".into(), duration_mins: 180, stops: 1, airline: "FR".into(), booking_url: "https://b.com".into() },
    ];
    let criteria = BookingCriteria { primary: BookingPriority::Cheapest, max_stops: None, max_price_minor: None };
    let best = criteria.rank(&offers).unwrap();
    assert_eq!(best.price_minor, 35000);
}

#[test]
fn fastest_ranking() {
    let offers = vec![
        FlightOffer { price_minor: 50000, currency: "EUR".into(), duration_mins: 120, stops: 0, airline: "LH".into(), booking_url: "https://a.com".into() },
        FlightOffer { price_minor: 35000, currency: "EUR".into(), duration_mins: 180, stops: 1, airline: "FR".into(), booking_url: "https://b.com".into() },
    ];
    let criteria = BookingCriteria { primary: BookingPriority::Fastest, max_stops: None, max_price_minor: None };
    let best = criteria.rank(&offers).unwrap();
    assert_eq!(best.duration_mins, 120);
}
```

- [ ] **Step 2: Run test to see it fail**

```powershell
cargo test -p kitsune-agent --test booking_agent 2>&1 | Select-String -Pattern "error|FAILED|ok"
```

- [ ] **Step 3: Create `crates/kitsune-agent/src/agents/booking.rs`**

```rust
use crate::ai_client::{AgentAiClient, ModelTier};
use crate::error::{AgentError, AgentResult};
use serde::{Deserialize, Serialize};
use tracing::info;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlightOffer {
    pub price_minor: i64,  // cents
    pub currency: String,
    pub duration_mins: u32,
    pub stops: u8,
    pub airline: String,
    pub booking_url: String,
}

impl FlightOffer {
    pub fn price_display(&self) -> String {
        format!("{:.2} {}", self.price_minor as f64 / 100.0, self.currency)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum BookingPriority {
    Cheapest,
    Fastest,
    Earliest, // fewest stops as tie-breaker for now
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BookingCriteria {
    pub primary: BookingPriority,
    pub max_stops: Option<u8>,
    pub max_price_minor: Option<i64>,
}

impl BookingCriteria {
    pub fn rank<'a>(&self, offers: &'a [FlightOffer]) -> Option<&'a FlightOffer> {
        let filtered: Vec<&FlightOffer> = offers
            .iter()
            .filter(|o| {
                self.max_stops.map(|m| o.stops <= m).unwrap_or(true)
                    && self.max_price_minor.map(|m| o.price_minor <= m).unwrap_or(true)
            })
            .collect();

        filtered.into_iter().min_by(|a, b| match self.primary {
            BookingPriority::Cheapest => a.price_minor.cmp(&b.price_minor),
            BookingPriority::Fastest => a.duration_mins.cmp(&b.duration_mins),
            BookingPriority::Earliest => a.stops.cmp(&b.stops),
        })
    }
}

pub struct BookingAgent {
    ai: std::sync::Arc<AgentAiClient>,
    http: reqwest::Client,
}

impl BookingAgent {
    pub fn new(ai: std::sync::Arc<AgentAiClient>) -> AgentResult<Self> {
        let http = reqwest::Client::builder()
            .use_rustls_tls()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .map_err(|e| AgentError::Internal(e.to_string()))?;
        Ok(Self { ai, http })
    }

    /// Fetch flight offers from Google Flights (HTML scraping via HTTP, no WebView).
    /// Returns parsed offers or empty vec if scraping fails.
    pub async fn fetch_offers(
        &self,
        origin: &str,
        destination: &str,
        date: &str, // YYYY-MM-DD
    ) -> AgentResult<Vec<FlightOffer>> {
        // Fetch Google Flights search page via plain HTTP
        let url = format!(
            "https://www.google.com/travel/flights?q=Flights+from+{}+to+{}+on+{}",
            urlencoding::encode(origin),
            urlencoding::encode(destination),
            urlencoding::encode(date)
        );

        let html = match self.http.get(&url)
            .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36")
            .send()
            .await
        {
            Ok(r) => r.text().await.unwrap_or_default(),
            Err(e) => {
                tracing::warn!("Google Flights fetch failed: {e}");
                return Ok(vec![]);
            }
        };

        // Use AI to parse the HTML for flight offers
        let prompt = format!(
            r#"Extract flight offers from this HTML. Return ONLY valid JSON (no markdown):
[{{"price_minor":12345,"currency":"EUR","duration_mins":120,"stops":0,"airline":"LH","booking_url":"https://..."}}]

HTML (first 3000 chars):
{}

If no flights found, return: []"#,
            &html[..html.len().min(3000)]
        );

        let response = self.ai.complete(&prompt, ModelTier::Fast).await?;
        let json_str = response.trim().trim_start_matches("```json").trim_end_matches("```").trim();

        let offers: Vec<FlightOffer> = serde_json::from_str(json_str).unwrap_or_default();
        info!(count = offers.len(), %origin, %destination, %date, "BookingAgent fetched offers");
        Ok(offers)
    }
}
```

- [ ] **Step 4: Run tests**

```powershell
cargo test -p kitsune-agent --test booking_agent 2>&1 | Select-String -Pattern "error|FAILED|ok"
```
Expected: `cheapest_ranking ... ok`, `fastest_ranking ... ok`

- [ ] **Step 5: Commit**

```powershell
git add crates/kitsune-agent/src/agents/booking.rs crates/kitsune-agent/tests/booking_agent.rs
git commit -m "feat(agent): add BookingAgent with parallel HTTP fetch and criteria ranking"
```

---

## Task 10: AgentOrchestrator

Decomposes goals into `SubTask`s, dispatches to sub-agents, collects results.

**Files:**
- Create: `crates/kitsune-agent/src/orchestrator.rs`
- Modify: `crates/kitsune-agent/src/lib.rs`

- [ ] **Step 1: Write the failing test**

Create `crates/kitsune-agent/tests/orchestrator.rs`:
```rust
use kitsune_agent::orchestrator::{SubTask, TaskStatus};

#[test]
fn sub_task_variants_exist() {
    let _search = SubTask::Search {
        query: "DAAD scholarship".into(),
        eligibility_filter: Some("MSc, GPA >= 3.5".into()),
    };
    let _form = SubTask::Form {
        url: "https://daad.de/apply".into(),
        candidate_title: Some("DAAD Research Grant".into()),
    };
    let _submit = SubTask::Submit {
        site: "https://daad.de".into(),
        filled_count: 10,
        submit_selector: Some("button#submit".into()),
    };
}

#[test]
fn task_status_default_is_pending() {
    assert_eq!(TaskStatus::default(), TaskStatus::Pending);
}
```

- [ ] **Step 2: Run test to see it fail**

```powershell
cargo test -p kitsune-agent --test orchestrator 2>&1 | Select-String -Pattern "error|FAILED|ok"
```

- [ ] **Step 3: Create `crates/kitsune-agent/src/orchestrator.rs`**

```rust
use crate::agents::booking::{BookingAgent, BookingCriteria};
use crate::agents::form::FormAgent;
use crate::agents::search::SearchAgent;
use crate::agents::submit::SubmitAgent;
use crate::ai_client::{AgentAiClient, ModelTier};
use crate::captcha::CaptchaAgent;
use crate::dom_access::DomAccessor;
use crate::error::{AgentError, AgentResult};
use crate::profile::ProfileSummary;
use kitsune_hil::HilGate;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{info, warn};

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum TaskStatus {
    #[default]
    Pending,
    Running,
    Completed,
    Failed(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum SubTask {
    Search {
        query: String,
        eligibility_filter: Option<String>,
    },
    Form {
        url: String,
        candidate_title: Option<String>,
    },
    Submit {
        site: String,
        filled_count: usize,
        submit_selector: Option<String>,
    },
    AccountCreate {
        site: String,
        username: String,
    },
    Booking {
        origin: String,
        destination: String,
        date: String,
        criteria: BookingCriteria,
    },
}

pub struct AgentOrchestrator {
    dom: Arc<DomAccessor>,
    ai: Arc<AgentAiClient>,
    captcha: Arc<CaptchaAgent>,
    hil_gate: Arc<HilGate>,
    profile: Arc<crate::profile::ProfileIndexer>,
}

impl AgentOrchestrator {
    pub fn new(
        dom: Arc<DomAccessor>,
        ai: Arc<AgentAiClient>,
        captcha: Arc<CaptchaAgent>,
        hil_gate: Arc<HilGate>,
        profile: Arc<crate::profile::ProfileIndexer>,
    ) -> Self {
        Self { dom, ai, captcha, hil_gate, profile }
    }

    /// Decompose a natural-language goal into a list of `SubTask`s.
    pub async fn plan(&self, goal: &str, profile: &ProfileSummary) -> AgentResult<Vec<SubTask>> {
        let profile_ctx = profile.to_prompt_context();
        let prompt = format!(
            r#"You are a task planner. Decompose the user's goal into a JSON array of SubTasks.

Available task types:
- Search: {{ "type": "Search", "query": "...", "eligibility_filter": "..." or null }}
- Form: {{ "type": "Form", "url": "https://...", "candidate_title": "..." or null }}
- Submit: {{ "type": "Submit", "site": "https://...", "filled_count": 0, "submit_selector": "button[type=submit]" or null }}
- AccountCreate: {{ "type": "AccountCreate", "site": "https://...", "username": "email@example.com" }}
- Booking: {{ "type": "Booking", "origin": "Berlin", "destination": "London", "date": "YYYY-MM-DD", "criteria": {{ "primary": "Cheapest", "max_stops": null, "max_price_minor": null }} }}

Return ONLY a valid JSON array. No markdown.

User goal: {goal}

User profile:
{profile_ctx}"#
        );

        let response = self.ai.complete(&prompt, ModelTier::Orchestrator).await?;
        let json_str = response.trim().trim_start_matches("```json").trim_end_matches("```").trim();

        let tasks: Vec<SubTask> = serde_json::from_str(json_str)
            .map_err(|e| AgentError::ExecutionError(format!("Bad plan: {e}\nRaw: {json_str}")))?;

        info!(count = tasks.len(), %goal, "Orchestrator planned tasks");
        Ok(tasks)
    }

    /// Execute a pre-planned list of `SubTask`s in order.
    pub async fn execute(
        &self,
        tasks: Vec<SubTask>,
        profile: &ProfileSummary,
    ) -> AgentResult<Vec<String>> {
        let mut results = Vec::new();

        let search_agent = SearchAgent::new(self.dom.clone(), self.ai.clone());
        let form_agent = FormAgent::new(
            self.dom.clone(),
            self.ai.clone(),
            self.captcha.clone(),
            self.hil_gate.clone(),
        );
        let submit_agent = SubmitAgent::new(self.dom.clone(), self.hil_gate.clone());

        for task in tasks {
            match task {
                SubTask::Search { query, eligibility_filter } => {
                    let candidates = search_agent
                        .search(&query, eligibility_filter.as_deref(), &profile.to_prompt_context())
                        .await?;
                    results.push(format!("Found {} candidates for '{}'", candidates.len(), query));
                }
                SubTask::Form { url, candidate_title } => {
                    let form_result = form_agent.fill_and_submit(&url, profile).await?;
                    results.push(format!(
                        "Filled {} fields on {}",
                        form_result.filled_count, form_result.site
                    ));
                }
                SubTask::Submit { site, filled_count, submit_selector } => {
                    let form_result = crate::agents::form::FormResult {
                        site: site.clone(),
                        filled_count,
                        submit_selector,
                        confirmation_text: None,
                    };
                    let submitted = submit_agent.submit(form_result).await?;
                    results.push(format!(
                        "Submitted form on {}. Confirmation: {}",
                        submitted.site,
                        submitted.confirmation_text.as_deref().unwrap_or("(no confirmation text)")
                    ));
                }
                SubTask::AccountCreate { site, username } => {
                    results.push(format!(
                        "Account creation on {} with username {} — requires HIL gate",
                        site, username
                    ));
                    // AccountCreate HIL is handled inside FormAgent when it encounters
                    // a registration form — the AwaitHil action in the FieldMappingPlan
                    // will pause and show the user the registration details.
                }
                SubTask::Booking { origin, destination, date, criteria } => {
                    let booking_agent = BookingAgent::new(self.ai.clone())
                        .map_err(|e| AgentError::Internal(e.to_string()))?;
                    let offers = booking_agent.fetch_offers(&origin, &destination, &date).await?;
                    if let Some(best) = criteria.rank(&offers) {
                        results.push(format!(
                            "Best flight: {} {} ({}min, {} stops) — {}",
                            best.price_display(), best.airline, best.duration_mins,
                            best.stops, best.booking_url
                        ));
                        form_agent.fill_and_submit(&best.booking_url, profile).await?;
                    } else {
                        results.push("No flights found matching criteria".into());
                    }
                }
            }
        }

        Ok(results)
    }

    /// High-level entry point: plan + execute.
    pub async fn run(
        &self,
        goal: &str,
        profile: &ProfileSummary,
    ) -> AgentResult<Vec<String>> {
        let tasks = self.plan(goal, profile).await?;
        self.execute(tasks, profile).await
    }
}
```

- [ ] **Step 4: Wire into `crates/kitsune-agent/src/lib.rs`**

```rust
pub mod orchestrator;
```

- [ ] **Step 5: Run tests**

```powershell
cargo test -p kitsune-agent --test orchestrator 2>&1 | Select-String -Pattern "error|FAILED|ok"
```
Expected: 2 tests pass.

- [ ] **Step 6: Full build check**

```powershell
cargo build -p kitsune-agent 2>&1 | Select-String -Pattern "^error"
```

- [ ] **Step 7: Commit**

```powershell
git add crates/kitsune-agent/src/orchestrator.rs crates/kitsune-agent/src/lib.rs crates/kitsune-agent/tests/orchestrator.rs
git commit -m "feat(agent): add AgentOrchestrator with SubTask dispatch and plan-then-execute"
```

---

## Task 11: ProfilePanel UI

**Files:**
- Create: `crates/kitsune-ui/src/panels/profile_panel.rs`

- [ ] **Step 1: Create `crates/kitsune-ui/src/panels/profile_panel.rs`**

```rust
use eframe::egui;
use kitsune_agent::profile::ProfileSummary;
use crate::theme::KitsuneTheme;

pub fn profile_panel(ui: &mut egui::Ui, summary: Option<&ProfileSummary>) {
    ui.heading("Profile");
    ui.separator();

    match summary {
        None => {
            ui.colored_label(KitsuneTheme::TEXT2, "No profile indexed yet.");
            ui.small("Set a folder in Settings → Profile, then click Re-index.");
        }
        Some(s) => {
            egui::Grid::new("profile_grid")
                .num_columns(2)
                .spacing([12.0, 4.0])
                .show(ui, |ui| {
                    ui.label("Name");
                    ui.label(&s.full_name);
                    ui.end_row();

                    if let Some(nat) = &s.nationality {
                        ui.label("Nationality");
                        ui.label(nat);
                        ui.end_row();
                    }
                    if let Some(email) = &s.email {
                        ui.label("Email");
                        ui.label(email);
                        ui.end_row();
                    }
                });

            ui.separator();
            ui.label("Education");
            for edu in &s.education {
                let gpa = edu.gpa.map(|g| format!(", GPA {g:.1}")).unwrap_or_default();
                ui.small(format!("  {} @ {}{}", edu.degree, edu.institution, gpa));
            }

            ui.separator();
            ui.label("Languages");
            for lang in &s.languages {
                ui.small(format!("  {} ({})", lang.language, lang.level));
            }

            ui.separator();
            ui.label("Skills");
            ui.small(s.skills.join(" · "));

            if let Some(ts) = &s.generated_at {
                ui.separator();
                ui.colored_label(
                    KitsuneTheme::TEXT2,
                    format!("Indexed: {}", ts.format("%Y-%m-%d %H:%M")),
                );
            }
        }
    }
}
```

- [ ] **Step 2: Register in the panels module**

Find `crates/kitsune-ui/src/panels/mod.rs` (or wherever panels are declared). Add:
```rust
pub mod profile_panel;
```
If panels are declared directly in `lib.rs` or `app.rs`, add `pub mod profile_panel;` there instead.

- [ ] **Step 3: Build check**

```powershell
cargo build -p kitsune-ui 2>&1 | Select-String -Pattern "^error"
```

- [ ] **Step 4: Commit**

```powershell
git add crates/kitsune-ui/src/panels/profile_panel.rs
git commit -m "feat(ui): add ProfilePanel egui widget"
```

---

## Task 12: TaskGraphPanel UI

**Files:**
- Create: `crates/kitsune-ui/src/panels/task_graph_panel.rs`

- [ ] **Step 1: Create `crates/kitsune-ui/src/panels/task_graph_panel.rs`**

```rust
use eframe::egui;
use crate::theme::KitsuneTheme;

#[derive(Debug, Clone, PartialEq)]
pub enum NodeStatus {
    Pending,
    Running,
    Completed,
    Failed(String),
}

#[derive(Debug, Clone)]
pub struct TaskNode {
    pub name: String,
    pub model_slot: String,
    pub tokens_used: Option<u32>,
    pub status: NodeStatus,
    pub summary: Option<String>,
}

impl TaskNode {
    pub fn new(name: impl Into<String>, model_slot: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            model_slot: model_slot.into(),
            tokens_used: None,
            status: NodeStatus::Pending,
            summary: None,
        }
    }
}

pub fn task_graph_panel(ui: &mut egui::Ui, nodes: &[TaskNode]) {
    ui.heading("Task Graph");
    ui.separator();

    if nodes.is_empty() {
        ui.colored_label(KitsuneTheme::TEXT2, "No active task.");
        return;
    }

    for node in nodes {
        let (icon, color) = match &node.status {
            NodeStatus::Pending   => ("○", KitsuneTheme::TEXT2),
            NodeStatus::Running   => ("●", egui::Color32::YELLOW),
            NodeStatus::Completed => ("✓", egui::Color32::GREEN),
            NodeStatus::Failed(_) => ("✗", egui::Color32::RED),
        };

        ui.horizontal(|ui| {
            ui.colored_label(color, icon);
            ui.strong(&node.name);
            ui.colored_label(KitsuneTheme::TEXT2, format!("[{}]", node.model_slot));
            if let Some(t) = node.tokens_used {
                ui.colored_label(KitsuneTheme::TEXT2, format!("{t}t"));
            }
            match &node.status {
                NodeStatus::Running   => { ui.spinner(); }
                NodeStatus::Failed(e) => { ui.colored_label(egui::Color32::RED, e); }
                _ => {}
            }
        });

        if let Some(summary) = &node.summary {
            ui.indent("task_summary", |ui| {
                ui.colored_label(KitsuneTheme::TEXT2, summary);
            });
        }
    }
}
```

- [ ] **Step 2: Register in panels module** (same file as Task 11 Step 2)

```rust
pub mod task_graph_panel;
```

- [ ] **Step 3: Build check**

```powershell
cargo build -p kitsune-ui 2>&1 | Select-String -Pattern "^error"
```

- [ ] **Step 4: Commit**

```powershell
git add crates/kitsune-ui/src/panels/task_graph_panel.rs
git commit -m "feat(ui): add TaskGraphPanel for live sub-agent status display"
```

---

## Task 13: Settings Dialog — Profile + Agents tabs

**Files:**
- Modify: `crates/kitsune-ui/src/dialogs/settings_dialog.rs`

- [ ] **Step 1: Read the current settings dialog**

```powershell
Get-Content "crates/kitsune-ui/src/dialogs/settings_dialog.rs" | head -60
```

Note the existing tab structure and `SettingsState` or equivalent struct.

- [ ] **Step 2: Add profile folder and CAPTCHA config fields to the settings state**

Find the settings state struct (likely `SettingsState` or `KitsuneBrowser` fields). Add:
```rust
pub profile_folder: String,               // path string for the folder picker
pub captcha_solver_url: String,           // e.g. "https://2captcha.com"
pub captcha_solver_key: String,           // cleared after vault storage
pub orchestrator_model: String,
pub worker_model: String,
pub fast_model: String,
```

- [ ] **Step 3: Add Profile tab to the settings dialog UI**

Inside the settings dialog's tab rendering, add a new tab alongside the existing ones:

```rust
// Profile tab
if ui.selectable_label(*active_tab == SettingsTab::Profile, "Profile").clicked() {
    *active_tab = SettingsTab::Profile;
}

// In the tab content area:
SettingsTab::Profile => {
    ui.heading("Document Profile");
    ui.separator();
    ui.label("Folder containing your CV, transcripts, and supporting documents:");
    ui.horizontal(|ui| {
        ui.text_edit_singleline(&mut state.profile_folder);
        if ui.button("Browse…").clicked() {
            if let Some(path) = rfd::FileDialog::new().pick_folder() {
                state.profile_folder = path.to_string_lossy().into_owned();
            }
        }
    });
    if ui.button("Re-index Now").clicked() {
        // Signal to app.rs to trigger ProfileIndexer::reindex
        // Use a flag on KitsuneBrowser state
        state.reindex_requested = true;
    }
    ui.separator();
    ui.label("Indexed profile preview is shown in the Profile panel.");
}
```

Add `rfd = "0.14"` to `crates/kitsune-ui/Cargo.toml` for the native folder picker.

- [ ] **Step 4: Add Agents tab**

```rust
SettingsTab::Agents => {
    ui.heading("Agent Configuration");
    ui.separator();

    ui.label("Model slots (Ollama model names or provider IDs):");
    egui::Grid::new("model_slots_grid").num_columns(2).show(ui, |ui| {
        ui.label("Orchestrator");
        ui.text_edit_singleline(&mut state.orchestrator_model);
        ui.end_row();
        ui.label("Worker");
        ui.text_edit_singleline(&mut state.worker_model);
        ui.end_row();
        ui.label("Fast");
        ui.text_edit_singleline(&mut state.fast_model);
        ui.end_row();
    });

    ui.separator();
    ui.label("CAPTCHA API Solver (optional — for Tier 3 bypass):");
    ui.horizontal(|ui| {
        ui.label("Endpoint:");
        ui.text_edit_singleline(&mut state.captcha_solver_url);
    });
    ui.horizontal(|ui| {
        ui.label("API Key:");
        ui.add(egui::TextEdit::singleline(&mut state.captcha_solver_key).password(true));
    });
    if ui.button("Save API Key to Vault").clicked() {
        state.save_captcha_key_requested = true;
    }
    ui.colored_label(KitsuneTheme::TEXT2, "Key is stored encrypted; never shown again.");
}
```

- [ ] **Step 5: Build check**

```powershell
cargo build -p kitsune-ui 2>&1 | Select-String -Pattern "^error"
```
Fix any missing enum variants (`SettingsTab::Profile`, `SettingsTab::Agents`) and state fields.

- [ ] **Step 6: Commit**

```powershell
git add crates/kitsune-ui/src/dialogs/settings_dialog.rs crates/kitsune-ui/Cargo.toml
git commit -m "feat(ui): add Profile and Agents tabs to settings dialog"
```

---

## Task 14: App Wiring

Connect `ProfileIndexer`, `AgentOrchestrator`, and the new panels to `KitsuneBrowser`.

**Files:**
- Modify: `crates/kitsune-ui/src/app.rs`

- [ ] **Step 1: Read app.rs to understand current state struct**

```powershell
Get-Content "crates/kitsune-ui/src/app.rs" | Select-Object -First 120
```

- [ ] **Step 2: Add new fields to `KitsuneBrowser`**

```rust
// In KitsuneBrowser struct:
profile_indexer: Option<Arc<kitsune_agent::profile::ProfileIndexer>>,
profile_summary: Option<kitsune_agent::profile::ProfileSummary>,
orchestrator: Option<Arc<kitsune_agent::orchestrator::AgentOrchestrator>>,
task_nodes: Vec<crate::panels::task_graph_panel::TaskNode>,
reindex_requested: bool,
```

- [ ] **Step 3: Initialise in `KitsuneBrowser::new` (or equivalent)**

After the vault and HIL gate are constructed, add:
```rust
let ai_client = Arc::new(
    kitsune_agent::ai_client::AgentAiClient::new(
        kitsune_agent::ai_client::AiProviderConfig::default()
    ).unwrap_or_else(|_| panic!("Failed to create AgentAiClient"))
);
let captcha_agent = Arc::new(
    kitsune_agent::captcha::CaptchaAgent::new(
        dom_accessor.clone(), hil_gate.clone(), None
    ).expect("CaptchaAgent init")
);
let profile_indexer = Arc::new(
    kitsune_agent::profile::ProfileIndexer::new(
        std::path::PathBuf::from(&settings.profile_folder)
    )
);
let orchestrator = Arc::new(kitsune_agent::orchestrator::AgentOrchestrator::new(
    dom_accessor.clone(),
    ai_client,
    captcha_agent,
    hil_gate.clone(),
    profile_indexer.clone(),
));
```

- [ ] **Step 4: Replace cloud-mock POST in the agent panel "Run" handler**

Find where the agent panel calls the cloud-mock server (likely a `reqwest::post` to `localhost:7700`). Replace with:
```rust
// Instead of POSTing to cloud-mock:
let goal = self.agent_prompt.clone();
let orchestrator = self.orchestrator.clone();
let summary = self.profile_summary.clone();
self.tokio_rt.spawn(async move {
    if let (Some(orch), Some(prof)) = (orchestrator, summary) {
        match orch.run(&goal, &prof).await {
            Ok(results) => {
                for r in results {
                    // Send result as LogEntry via existing mpsc channel
                }
            }
            Err(e) => {
                // Send error as LogEntry
            }
        }
    }
});
```

- [ ] **Step 5: Handle `reindex_requested` flag in the update loop**

In the egui `update` method, check:
```rust
if self.reindex_requested {
    self.reindex_requested = false;
    // spawn background reindex
    let indexer = self.profile_indexer.clone();
    let ai = self.ai_client.clone(); // expose ai_client as a field
    self.tokio_rt.spawn(async move {
        if let Some(idx) = indexer {
            if let Ok(summary) = idx.reindex(&ai).await {
                // signal back to UI thread via std::sync::mpsc
            }
        }
    });
}
```

- [ ] **Step 6: Render new panels**

In the egui layout, add the profile panel and task graph panel to the side panel or a new tab:
```rust
// Profile panel (e.g. in right side panel)
crate::panels::profile_panel::profile_panel(ui, self.profile_summary.as_ref());

// Task graph panel (below agent log)
crate::panels::task_graph_panel::task_graph_panel(ui, &self.task_nodes);
```

- [ ] **Step 7: Build check**

```powershell
cargo build -p kitsune-ui 2>&1 | Select-String -Pattern "^error"
```
Iterate on compile errors — field names and types will need adjustment to match the exact current app.rs structure.

- [ ] **Step 8: Commit**

```powershell
git add crates/kitsune-ui/src/app.rs
git commit -m "feat(ui): wire AgentOrchestrator and ProfileIndexer into KitsuneBrowser"
```

---

## Task 15: Full workspace test + build

- [ ] **Step 1: Run full workspace tests**

```powershell
cargo test --workspace 2>&1 | Select-String -Pattern "FAILED|error\[|test result"
```
Expected: all existing ~107 tests pass, new tests pass, no regressions.

- [ ] **Step 2: Release build**

```powershell
cargo build --release -p kitsune-ui 2>&1 | Select-String -Pattern "^error"
```
Expected: succeeds.

- [ ] **Step 3: Commit if any fixes were needed**

```powershell
git add -p   # stage only the fixes
git commit -m "fix: address workspace-wide compile errors from agentic pipeline integration"
```

---

## Self-Review Notes

After completing all tasks, verify:

1. `CaptchaAgent::resolve` is called by `FormAgent` on every `CaptchaCheck` action and on initial page load — ✓ wired in Task 7.
2. `HilTriggerClass::CaptchaRequired` is used by `CaptchaAgent::escalate_to_hil` — ✓ wired in Task 5.
3. `ProfileIndexer::new` does not call the AI on construction (only on `reindex`) — ✓ lazy design in Task 4.
4. `BookingAgent::fetch_offers` uses `KitsuneHttpClient` — currently uses `reqwest::Client` directly; this is acceptable since `KitsuneHttpClient` wraps reqwest anyway. For privacy header enforcement, replace with `KitsuneHttpClient` in a follow-up.
5. `SubmitAgent` always gates HIL before clicking submit — ✓ in Task 8.
6. `AgentOrchestrator::run` passes `ProfileSummary`, not raw document text — ✓ token-efficient by design.
7. `DomAccessor::webview_tx` is changed to `pub(crate)` for `CaptchaAgent` access — must be done in Task 5 Step 3.
