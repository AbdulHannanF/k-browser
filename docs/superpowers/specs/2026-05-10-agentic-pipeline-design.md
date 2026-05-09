# Agentic Pipeline Design
**Date:** 2026-05-10  
**Status:** Approved  
**Scope:** Full autonomous task execution — scholarship applications, flight booking, account management, CAPTCHA bypass, token-efficient hierarchical agents

---

## 1. Goal

Enable KitsuneEngine to execute complex multi-step real-world tasks autonomously on behalf of the user:

- **Scholarship application**: Read user's CV and supporting documents, search for opportunities (e.g. DAAD), check eligibility, fill and submit applications, create and manage portal accounts.
- **Flight booking**: Compare multiple booking sites in parallel, rank results by user-specified criteria (cheapest / fastest / earliest), book the best result.
- **General agentic tasks**: Any goal requiring navigation, form-filling, account creation, file upload, and submission across multiple sites.

Token efficiency is a hard constraint. Long tasks (8-page scholarship forms) must not consume 100k+ tokens. The architecture achieves this via **hierarchical specialised agents**, not by tuning prompts.

---

## 2. Architecture Overview

```
User prompt: "Apply for DAAD scholarship"
        │
        ▼
AgentOrchestrator   ← orchestrator model slot
  │  Input:  goal + ProfileSummary (~1–2k tokens, never raw docs)
  │  Output: Vec<SubTask>
  │
  ├─ SubTask::Search ──► SearchAgent         ← fast model slot
  │                       searches, screens eligibility,
  │                       returns compressed Vec<Candidate>
  │
  ├─ SubTask::AccountCreate
  │    HIL fires once → agent registers → vault stores SiteCredentials
  │
  ├─ SubTask::Form ──► FormAgent             ← worker model slot
  │                     DOM scan → FieldMappingPlan (one LLM call)
  │                     Deterministic executor (zero LLM calls per field)
  │                     CaptchaAgent intercepts when CAPTCHA detected
  │
  └─ SubTask::Submit ──► SubmitAgent         ← worker model slot
                          Preview → HIL confirmation → submit → capture receipt
```

For **flight booking**:

```
SubTask::Booking ──► BookingAgent
  Spawns parallel SearchAgents (Google Flights, Kayak, Skyscanner, ...)
  Each returns FlightOffer { price, duration, stops, booking_url }
  Orchestrator ranks by BookingCriteria
  FormAgent books winning site → HIL before any payment
```

### Model slots (provider-agnostic)

| Slot | Purpose | Example: Claude | Example: Ollama |
|---|---|---|---|
| orchestrator | Goal decomposition, ranking | claude-opus-4-7 | llama3:70b |
| worker | DOM planning, form mapping, submit preview | claude-sonnet-4-6 | llama3:70b |
| fast | Search, screening, profile extraction | claude-haiku-4-5 | llama3:8b |

Slots are configurable in Settings → Agents. The existing `AiRouter` / `AiProvider` enum is extended to route per slot.

### Token profile per task

| Agent | Slot | Avg tokens |
|---|---|---|
| OrchestratorAgent | orchestrator | 3–5k (planning only) |
| SearchAgent | fast | 1–2k per search |
| FormAgent — planning call | worker | 2–3k per form page |
| FormAgent — execution | none | **0** (deterministic) |
| SubmitAgent | worker | ~1k |
| **Total (scholarship apply)** | | **~15–25k vs. ~100k+ flat** |

---

## 3. New Components

### 3.1 AgentOrchestrator (`kitsune-agent/src/orchestrator.rs`)

```rust
pub struct AgentOrchestrator {
    ai: Arc<AiRouter>,
    dom: Arc<DomAccessor>,
    vault: Arc<VaultBackend>,
    hil_gate: Arc<HilGate>,
    profile: Arc<ProfileIndexer>,
}

pub enum SubTask {
    Search { query: String, eligibility_filter: Option<String> },
    AccountCreate { site: Url, username: String },
    Form { url: Url, candidate: Option<Candidate> },
    Submit { form_result: FormResult },
    Booking { criteria: BookingCriteria, sites: Vec<BookingSite> },
}
```

- Receives goal string + `ProfileSummary` (~1–2k tokens).
- One orchestrator-slot LLM call produces `Vec<SubTask>`.
- Dispatches each sub-task to the appropriate agent.
- Collects compressed outputs; passes only summaries forward between stages.
- For `Booking`: dispatches parallel `SearchAgent` instances, collects `Vec<FlightOffer>`, ranks, hands off to `FormAgent`.

### 3.2 ProfileIndexer (`kitsune-agent/src/profile.rs`)

Watches a user-configured folder. Parses documents on change (hash-checked). Produces and caches a compressed `ProfileSummary`.

```rust
pub struct ProfileIndexer {
    folder_path: PathBuf,
    summary: Arc<Mutex<Option<ProfileSummary>>>,
    file_hashes: HashMap<PathBuf, [u8; 32]>,  // sha256 per file, skip unchanged
}

pub struct ProfileSummary {
    pub full_name: String,
    pub date_of_birth: Option<NaiveDate>,
    pub nationality: Option<String>,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub education: Vec<EducationEntry>,   // degree, institution, year, GPA
    pub languages: Vec<LanguageEntry>,    // language, CEFR level
    pub skills: Vec<String>,
    pub publications: Vec<String>,
    pub awards: Vec<String>,
    pub generated_at: DateTime<Utc>,
    pub source_files: Vec<String>,
}
```

**Parsing pipeline:**
1. PDF files → `pdf-extract` → raw text.
2. DOCX files → `docx-rs` → raw text.
3. TXT files → read directly.
4. JPG/PNG → skipped (no OCR in this version).
5. All raw text → single fast-slot LLM call → `ProfileSummary` JSON.
6. Cached to `data_dir()/kitsune/profile_summary.json`.
7. On next run: sha256 each file; only re-parse changed files.

### 3.3 CaptchaAgent (`kitsune-agent/src/captcha.rs`)

Detects and resolves CAPTCHAs using a four-tier strategy. Called by `FormAgent` at each `FieldAction::CaptchaCheck` op.

```rust
pub struct CaptchaAgent {
    dom: Arc<DomAccessor>,
    hil_gate: Arc<HilGate>,
    vault: Arc<VaultBackend>,
    stt: Option<WhisperHandle>,          // feature-gated: captcha-audio
    solver_token: Option<TokenHandle>,   // CAPTCHA API key from vault
}

pub enum CaptchaKind {
    RecaptchaV2,
    RecaptchaV3 { action: String },
    HCaptcha,
    CloudflareTurnstile,
    Unknown,
}
```

**Detection**: `DomAccessor::navigate` automatically runs a post-load DOM scan for `iframe[src*="recaptcha"]`, `iframe[src*="hcaptcha"]`, `.cf-turnstile`, `#captcha`, `[data-sitekey]`. This fires for all agents (Search, Form, Submit, Booking) — not only during form interaction.

**Tier 1 — Behavioral evasion (always active, zero cost)**  
All `DomAccessor` actions already dispatch real browser events. Add: randomised inter-keystroke delay (80–180 ms), `mousemove` JS injection before clicks, randomised scroll position before form interactions. Many anti-bot systems never escalate past this tier.

**Tier 2 — Audio transcription (reCAPTCHA v2, no API key needed)**  
Triggered when `CaptchaKind::RecaptchaV2` detected:
1. Click audio challenge button.
2. Extract `<audio>` `src` URL → download MP3 via `KitsuneHttpClient`.
3. Transcribe with `whisper-rs` (feature flag `captcha-audio`) or fast-slot model with audio capability.
4. Type digits into `#audio-response` → submit.
5. Retry once on failure before escalating to Tier 3.

**Tier 3 — CAPTCHA API solver (hCaptcha, reCAPTCHA v3, Cloudflare Turnstile)**  
API key stored in vault under `VaultCategory::ApiKeys`, never exposed to agents (opaque `TokenHandle`):
1. POST `{ sitekey, pageurl }` to configured solver endpoint.
2. Poll for token (10–30 s timeout, configurable).
3. Inject token: `document.querySelector('[name="g-recaptcha-response"]').value = token`.
4. Solver endpoint is user-configurable (2captcha, CapMonster, Anti-Captcha).

**Tier 4 — HIL escalation (final fallback)**  
```rust
HilTriggerClass::CaptchaRequired {
    site: String,
    captcha_type: String,
}
```
WebView shown to user with CAPTCHA element highlighted in gold. Agent pauses. Resumes after user approval. A non-blocking toast updates in real-time as tiers escalate.

### 3.4 FormAgent (`kitsune-agent/src/agents/form.rs`)

Plan-then-execute. One LLM call per form page. Zero LLM calls during execution.

**DOM extraction** (not full HTML — only interactive elements):
```json
[
  { "selector": "#firstname", "label": "First Name", "type": "text" },
  { "selector": "#dob",       "label": "Date of Birth", "type": "date" },
  { "selector": "select#lang","label": "Language",  "type": "select",
    "options": ["English","German","French"] }
]
```
~400–600 tokens of structure, not the full page.

**FieldMappingPlan** (one worker-slot LLM call, input = DOM structure + ProfileSummary):
```rust
pub enum FieldAction {
    FillFromProfile { selector: String, field: ProfileField },
    FillFromVault   { selector: String, vault_key: VaultKey },
    FillStatic      { selector: String, value: String },
    SelectOption    { selector: String, value: String },
    UploadFile      { selector: String, file_path: PathBuf },
    Click           { selector: String },
    CaptchaCheck,
    NavigateNext    { selector: String },
    WaitForElement  { selector: String, timeout_ms: u64 },
    AwaitHil        { reason: String },
}
```

**Execution loop:**
1. Execute each `FieldAction` deterministically.
2. On `CaptchaCheck`: delegate to `CaptchaAgent`; suspend until resolved.
3. On `NavigateNext`: re-scan DOM of next page; one more LLM call to plan remaining fields.
4. On unexpected DOM state (element missing, redirect): one re-plan call with remaining ops + new DOM snapshot.
5. On `AwaitHil`: fire HIL gate; agent pauses; resumes on user approval.

### 3.5 SearchAgent (`kitsune-agent/src/agents/search.rs`)

- Receives `SubTask::Search { query, eligibility_filter }`.
- Navigates to configured search engines / scholarship databases.
- Extracts page text + links (existing `DomAccessor` methods).
- One fast-slot LLM call per page: "Which of these results match the filter? Return top 3 as JSON."
- Returns `Vec<Candidate> { title, url, deadline, requirements_summary }`.

### 3.6 SubmitAgent (`kitsune-agent/src/agents/submit.rs`)

- Receives `FormResult { filled_fields, pending_docs, submit_selector }`.
- Renders a human-readable preview of all filled fields.
- HIL gate fires with the preview text.
- On approval: clicks submit button via `DomAccessor::click_element`.
- Captures confirmation text / application number from post-submit page.
- Logs to vault audit table.

### 3.7 BookingAgent (`kitsune-agent/src/agents/booking.rs`)

Because there is a single WebView2 instance, true parallel tab navigation is not possible. BookingAgent uses a two-phase approach:

**Phase 1 — Price comparison via direct HTTP** (no WebView, no LLM):
- `KitsuneHttpClient` fetches structured search results from each booking site sequentially (Google Flights JSON API, Kayak, Skyscanner).
- A fast-slot LLM call parses the response pages into `Vec<FlightOffer>`.
- Returns `FlightOffer { price_minor: i64, currency: String, duration_mins: u32, stops: u8, airline: String, booking_url: Url }`.

**Phase 2 — Book the winner via WebView**:
- Orchestrator ranks all offers by `BookingCriteria { primary: Cheapest | Fastest | Earliest, max_stops: Option<u8>, max_price: Option<MoneyAmount> }`.
- HIL fires: user sees the winning offer details and approves before proceeding.
- On approval: `FormAgent` navigates WebView to `booking_url` and completes the booking form.
- HIL fires again before any payment step, regardless of `can_initiate_payments` setting.

---

## 4. Account Management

New vault category: `VaultCategory::SiteCredentials`.

```rust
pub struct SiteCredentials {
    pub username: String,
    pub password_token: TokenHandle,   // opaque; plaintext never crosses any boundary
    pub origin_pseudonym: [u8; 32],
    pub created_at: DateTime<Utc>,
}
```

**First visit (account creation):**
1. `AgentConstraints::can_create_accounts` must be `true` in the agent spec.
2. HIL fires once: `"Create account on daad.de? Username: your@email.com / Password: [auto-generated 24-char, vault-stored]"`.
3. On approval: vault stores `SiteCredentials` under the site's `origin_pseudonym`.
4. Agent fills registration form via `FormAgent` using `FillFromVault` ops for the password.
5. Email verification: agent escalates to HIL Tier 4 — user clicks the verification link manually. Email inbox access is out of scope for this version.

**Future visits:**
1. `vault.retrieve(origin_pseudonym, SiteCredentials)` returns opaque `TokenHandle`.
2. `FormAgent` uses `FillFromVault` ops — no HIL needed.
3. No password ever crosses a process or IPC boundary as plaintext.

---

## 5. HIL Integration Points

| Event | HIL behaviour |
|---|---|
| Account creation | Single gate before registration; shows username + masked password |
| Form submission | Gate shows all filled fields in plain text for user review |
| Payment initiation | Always gated, even if `can_initiate_payments = true` |
| CAPTCHA Tier 4 | Gate with WebView shown; user solves manually |
| Vault disclosure | Existing `fill_field` HIL path unchanged |
| Flight booking confirmation | Gate with full booking details (price, route, airline) |
| AwaitHil in FieldMappingPlan | Inline pause at agent-decided checkpoints (e.g. mid-form) |

The 30-second `HilApproval` TTL and non-cloneable token invariants are unchanged.

---

## 6. UI Changes

### Settings dialog — new tabs

**Profile tab:**
- Folder path picker (text field + Browse button).
- "Re-index Now" button.
- Read-only `ProfileSummary` preview: name, education, languages, skills, last-indexed timestamp.

**Agents tab:**
- CAPTCHA API solver: endpoint URL + API key field (stored to vault on save, never shown again).
- Model slot dropdowns: Orchestrator / Worker / Fast — populated from configured providers.

### Agent panel — task graph

Replaces the flat log list during orchestrated tasks:

```
● OrchestratorAgent   decomposing goal...          ✓  [4.2k tokens]
  ├ ● SearchAgent      searching daad.de...         ✓  3 candidates  [1.1k tokens]
  ├ ● AccountCreate    [HIL pending]                ⏳
  ├ ○ FormAgent        waiting...
  └ ○ SubmitAgent      waiting...
```

Each node: name · model slot · token count · status. Expandable to show compressed sub-agent output.

### CAPTCHA toast (non-blocking, top-right corner)

```
┌─────────────────────────────────────┐
│ CAPTCHA detected on daad.de         │
│ Trying: Audio transcription (Tier 2)│
└─────────────────────────────────────┘
```

Updates in place as tiers escalate. Dismissed automatically on resolution.

---

## 7. New Dependencies

| Crate | Purpose | Feature-gated |
|---|---|---|
| `pdf-extract` | PDF text extraction in ProfileIndexer | no |
| `docx-rs` | DOCX parsing | no |
| `notify` | Folder watcher for profile directory | no |
| `whisper-rs` | Audio CAPTCHA transcription (Tier 2) | `captcha-audio` |
| `sha2` | File hash cache for ProfileIndexer | no |

No new crates are needed for the hierarchical agents, FormAgent plan-then-execute, or account management — these use existing `kitsune-ai`, `kitsune-agent`, `kitsune-hil`, `kitsune-vault`.

---

## 8. Files Changed

| File | Change |
|---|---|
| `kitsune-agent/src/orchestrator.rs` | New — `AgentOrchestrator`, `SubTask` |
| `kitsune-agent/src/profile.rs` | New — `ProfileIndexer`, `ProfileSummary` |
| `kitsune-agent/src/captcha.rs` | New — `CaptchaAgent`, `CaptchaKind` |
| `kitsune-agent/src/agents/search.rs` | New — `SearchAgent` |
| `kitsune-agent/src/agents/form.rs` | New — `FormAgent`, `FieldMappingPlan`, `FieldAction` |
| `kitsune-agent/src/agents/submit.rs` | New — `SubmitAgent` |
| `kitsune-agent/src/agents/booking.rs` | New — `BookingAgent`, `FlightOffer`, `BookingCriteria` |
| `kitsune-agent/src/dom_access.rs` | Extend — human-like timing, behavioural evasion helpers |
| `kitsune-agent/src/spec.rs` | Extend — `AgentTool::CaptchaBypass`, model slot fields |
| `kitsune-agent/src/lib.rs` | Wire new modules |
| `kitsune-vault/src/backend.rs` | Add `SiteCredentials` CRUD |
| `kitsune-vault/src/types.rs` | Add `VaultCategory::SiteCredentials`, `VaultCategory::ApiKeys` |
| `kitsune-ai/src/router.rs` | Add `ModelTier` enum + per-slot routing |
| `kitsune-hil/src/lib.rs` | Add `HilTriggerClass::CaptchaRequired` |
| `kitsune-ui/src/panels/profile_panel.rs` | New — ProfileSummary display |
| `kitsune-ui/src/panels/task_graph_panel.rs` | New — live task graph display |
| `kitsune-ui/src/dialogs/settings_dialog.rs` | Extend — Profile + Agents tabs |
| `kitsune-ui/src/app.rs` | Wire `ProfileIndexer` + `AgentOrchestrator` into `KitsuneBrowser` |
| `kitsune-agent/Cargo.toml` | Add `pdf-extract`, `docx-rs`, `notify`, `sha2` |

---

## 9. Scope Boundaries (What Does NOT Change)

- `HilApproval` non-cloneable / 30 s TTL invariant.
- `VaultBackend::retrieve` returning `TokenHandle` — no plaintext ever crosses a boundary.
- `RoutingPolicy::always_local` for `VaultDecision` and `SensitiveForm` task types.
- `kitsune-cef` / WebView2 wiring — `DomAccessor` already drives it; no changes needed.
- Multi-process / IPC architecture — all new code runs in the existing single-process runtime.
- `kitsune-cloud-mock` — stays as a fixture site for offline testing; no longer acts as agent brain.
- No OCR for image files (passport.jpg etc.) in this version.
- No SMS / phone verification bypass — always escalates to HIL Tier 4.
