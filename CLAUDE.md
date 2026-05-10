# CLAUDE.md

## Project Overview

KitsuneEngine is a privacy-first, agentic desktop browser built in Rust. It runs an `egui` native shell over an embedded WebView2 surface (via `wry`) and gives the user an in-browser AI agent that can navigate, read DOM, and fill forms — but every consequential action (payments, account creation, credential disclosure) is forced through a non-bypassable Human-in-the-Loop (HIL) gate that issues single-use, action-bound approval tokens. Sensitive data lives in a local age-encrypted vault keyed off Argon2id; agents only ever receive opaque token handles, never raw secrets. The project's distinctive bet: that an "AI does things for you" browser is only safe if the safety mechanism is structural (type system + IPC capability checks + always-local routing for sensitive task types) rather than prompt-engineered.

## Architecture & Crate Layout

Cargo workspace, resolver = "2", edition 2021, MSRV 1.75. Single binary target: `kitsune` (in `kitsune-ui`). Secondary binary: `kitsune-cloud-mock`.

```
kitsune-engine/
├── Cargo.toml                     # workspace root (12 members)
└── crates/
    ├── kitsune-core               # Broker process — orchestrator, owns vault/HIL/IPC
    ├── kitsune-ui                 # egui native shell + main `kitsune` binary
    ├── kitsune-cef                # WebView2 host via `wry` (legacy crate name; not actual CEF)
    ├── kitsune-agent              # LLM agent loop, AgentSpec, AgentRuntime, orchestrator, profile
    ├── kitsune-agent-builder      # No-code AgentSpec construction + validation
    ├── kitsune-ai                 # AiBackend trait + AiRouter (cloud vs local) + QuotaTracker
    ├── kitsune-hil                # HilGate, HilApproval (non-cloneable), HilTriggerClass
    ├── kitsune-vault              # age-encrypted SQLite store + SiteIsolationMap + audit log
    ├── kitsune-ipc                # IpcMessage/IpcPayload, IpcChannel, IpcServer (postcard + named pipes)
    ├── kitsune-net                # KitsuneHttpClient, PartitionedCookieJar, privacy header enforcement
    ├── kitsune-sandbox            # Per-OS process sandboxing (Windows Job Objects implemented)
    └── kitsune-cloud-mock         # axum SSE server for offline demo + agent-brain stub
```

Core: `kitsune-core`, `kitsune-ui`, `kitsune-ipc`, `kitsune-vault`, `kitsune-hil`, `kitsune-agent`.
Optional / feature-gated: `kitsune-ai` (`local-model` feature pulls in `candle-*`/`hf-hub`/`tokenizers`).
Platform-specific: `kitsune-sandbox` (only Windows path is implemented; Linux seccomp-BPF and macOS Seatbelt are stubs). `kitsune-cef` is Windows-only in practice (WebView2 host, `SetFocus` Win32 FFI).
Demo-only: `kitsune-cloud-mock`.

## Key Invariants (NEVER violate these)

These are derived from actual code patterns and explicit `INVARIANT:` comments — not invented.

1. **Vault never returns raw secrets across any boundary.** `VaultBackend::retrieve` returns a `TokenHandle` and binds the decrypted bytes in an in-memory `token_store` (30 s TTL, single-use via `consume_token`). IPC `VaultResponse` carries `token_handle: Option<String>` only — no raw bytes (`crates/kitsune-ipc/src/message.rs`). DOM fill uses `DomFillField { value_token }` — opaque tokens, never raw values.
2. **`TaskType::VaultDecision` and `TaskType::SensitiveForm` MUST stay local.** Enforced at the type level in `RoutingPolicy::always_local` in `crates/kitsune-ai/src/router.rs:43`. The field is private and not user-configurable. If local is unavailable, the request fails — there is no cloud fallback.
3. **HIL approvals are non-cloneable, single-use, action-bound, 30-second TTL.** `HilApproval` deliberately does NOT implement `Clone` (`crates/kitsune-hil/src/approval.rs:70`). `APPROVAL_EXPIRY_SECONDS = 30`. An approval consumed for a different `ActionId` errors out — bypass via token reuse is type-system-impossible.
4. **Vault KDF salt is generated on first run and stored in the OS keychain, never hardcoded.** `VaultBackend::new_with_keyring` reads/creates a random 32-byte KDF salt under `kitsune-vault` / `kdf-salt`. Falls back to a dev-only fixed salt in headless CI (`KitsuneEngine::new`) with a tracing warning — this fallback MUST NOT be used in production.
5. **Cross-origin identifiers are architecturally distinct.** `SiteIsolationMap::derive_identifier` hashes `seed || "kitsune-site-isolation-v1" || origin` via SHA-256 (`crates/kitsune-vault/src/site_isolation.rs:49-58`). `VaultBackend::origin_pseudonym` derives per-origin storage keys via HMAC-SHA256(secret_salt, origin). Two origins can never share an identifier; vault entries are looked up by pseudonym so cross-site reuse is impossible.
6. **Cloud quota exhaustion never silently retries.** On 429, `KitsuneCloudBackend` returns `AiError::QuotaExhausted` for the UI to surface as an upgrade prompt; only network errors and 5xx are retried (`crates/kitsune-ai/src/cloud.rs`).
7. **Agents inherit denials, not capabilities.** `AgentConstraints` defaults to `can_initiate_payments = false`, `can_create_accounts = false`, `can_send_communications = false`, `hil_required_for = ["all"]` (`crates/kitsune-agent/src/spec.rs:159-173`). Capabilities must be explicitly granted in the spec; soft instructions in the goal text are not capabilities.
8. **Cloud auth tokens live in the OS keychain, not on disk and not in env vars.** `KEYRING_SERVICE = "kitsune-engine"` / `KEYRING_USER = "cloud-token"` (`crates/kitsune-ai/src/cloud.rs:25-26`).
9. **Renderers and network processes never talk to each other directly.** All routing flows through `ProcessManager::route` in the broker, which uses the static `route_payload` table (`crates/kitsune-core/src/broker.rs:79-113`). There is no peer-to-peer IPC path.
10. **Vault token store is single-use and TTL-bound.** `VaultBackend::consume_token` removes the entry from the in-memory `token_store` on first call and rejects expired tokens with `VaultError::TokenExpired` or `VaultError::TokenNotFound`. This matches `HilApproval`'s 30 s window.

## Data Flow

Single-process MVP today; the type contracts are designed for the multi-process target. Trace of an agentic action ("Buy this item"):

1. **User → UI** (`kitsune-ui::app::KitsuneBrowser`, `panels::agent_panel::agent_panel`): natural-language prompt entered in the agent shelf. User optionally selects a specialist card (PriceTracker, FormFillAgent, ResearchAgent) which injects a specialist system-prompt context.
2. **UI → Agent runtime** (`kitsune-agent::LlmAgentRuntime::run`): prompt is bound to an `AgentSpec` whose `AgentConstraints` are the contract for what's allowed. Specialist context from the selected card is embedded in the system prompt via `with_agent_context`.
3. **LLM loop** (`loop_runtime.rs`): each turn observes the page (JS injected via `WebViewCommand::EvalJsWithCallback`), sends history to the LLM backend (Ollama or OpenAI-compatible cloud), parses one `AgentAction`, executes it. `<think>` blocks are extracted and emitted as `AgentEvent::Thinking` before the action JSON.
4. **Agent → AI** (`LlmBackend::chat`): the request goes to either `LlmBackend::Ollama` (local) or `LlmBackend::Cloud` (OpenAI-compatible HTTP). The `kitsune-ai::AiRouter` is a separate path used by the `AgentOrchestrator` pipeline — the `LlmAgentRuntime` calls `LlmBackend` directly.
5. **Agent → DOM** (`kitsune-agent::dom_observer` / `executor::WebViewCommand`): JS is generated, sent over `mpsc::Sender<WebViewCommand>` to the WebView2 host (`kitsune-cef::CefBrowser`). Results return on a `mpsc` channel via the `__kitsune_ipc` IPC handler.
6. **Sensitive action reached** → `HilGate::checkpoint(trigger_class, data_labels)` posts an `HilCheckpoint` to the UI, which renders `dialogs::hil_dialog`.
7. **User decides** → `respond_to_checkpoint` resolves the gate's `oneshot::Sender<ApprovalDecision>`. On approve, an `HilApproval` is constructed. On reject or timeout, `HilError::UserRejected` / `Dismissed` aborts the action.
8. **Vault disclosure** (only after approval): `VaultBackend::retrieve` decrypts via `CryptoBackend::decrypt` (age + Argon2id), produces a `TokenHandle`, binds decrypted bytes in `token_store` (30 s TTL), logs to `audit` table. `consume_token(token_id)` dereferences the handle once.
9. **Download action**: `AgentAction::Download { url, filename }` is executed directly in the loop runtime with a dedicated `reqwest::Client` (rustls-tls). File is saved to `dirs::download_dir()`.
10. **Network** flows out through `kitsune-net::KitsuneHttpClient` → `apply_privacy_protections` strips `Referer`, injects `DNT`/`Sec-GPC`, blocks known trackers, enforces TLS 1.3.
11. **Events → UI**: `AgentEvent` variants (Log, Step, Thinking, Navigated, Done, Error) flow through an `UnboundedSender<AgentEvent>` to a pump task that converts them to `AgentSseAction` and sends them on a `std::sync::mpsc::Sender<AgentSseAction>`. The egui frame processes them in `process_agent_events()`.

In the multi-process target, steps 5 and 10 cross process boundaries via the named-pipe IPC transport. Today, all child roles are `register_mock`'d as in-process tokio channels.

## kitsune-agent Module Map

`kitsune-agent` is the largest crate. Key modules:

| Module | Contents |
|---|---|
| `action.rs` | `AgentAction` enum (Navigate, Click, Fill, Read, ReadFile, **Download**, Done) + `parse_action_json` |
| `loop_runtime.rs` | `LlmAgentRuntime`, `LlmBackend` (Ollama + Cloud), `AgentEvent`, `FilePermSlot`, `StopFlag` |
| `spec.rs` | `AgentSpec`, `AgentConstraints`, `AgentBudget`, `DomainPolicy`, `VaultAccessLevel` |
| `executor.rs` | `WebViewCommand` (Navigate, EvalJs, EvalJsWithCallback, SetBounds) |
| `dom_observer.rs` | `observation_script()`, `ObservedPage`, `ObservedElement` |
| `dom_access.rs` | `DomAccessor` — HIL-gated DOM read/fill |
| `runtime.rs` | `AgentRuntime` — legacy scripted executor path |
| `budget.rs` | `BudgetTracker` |
| `ai_client.rs` | `AgentAiClient`, `AiProviderConfig`, `ModelSlots`, `ModelTier` (Orchestrator/Worker/Fast) |
| `orchestrator.rs` | `AgentOrchestrator`, `SubTask` enum, multi-agent task planning |
| `profile.rs` | `ProfileIndexer`, `ProfileSummary`, `EducationEntry`, `LanguageEntry` |
| `captcha.rs` | `CaptchaAgent`, `CaptchaKind` (reCAPTCHA v2/v3, hCaptcha, Cloudflare Turnstile), `CaptchaSolverConfig` |
| `ollama_client.rs` | `OllamaClient`, `DEFAULT_OLLAMA_URL`, `DEFAULT_OLLAMA_MODEL` |
| `agents/` | Specialist scripted-agent impls: `booking`, `form`, `search`, `submit` |
| `tools.rs` | `AgentTool` enum |

## kitsune-ui Module Map

| Module | Contents |
|---|---|
| `app.rs` | `KitsuneBrowser` (main state), `AgentRunState`, `LogLevel` (Info/Ok/Warn/Block/Cmd/**Step**/**Think**), `LogEntry`, `AgentSseAction`, `AttachedFile`, `DownloadItem`, `SettingsProvider`, `CloudPreset`, `SettingsTab` |
| `chrome/top_bar.rs` | Address bar, tab controls, privacy indicator |
| `chrome/tab_bar.rs` | Tab strip |
| `panels/agent_panel.rs` | Agent workspace panel, card selection, `start_agent_run`, log rendering, specialist context |
| `panels/agent_card.rs` | `AgentCard`, `AgentStatus` — clickable card with selection highlight |
| `panels/session_panel.rs` | Right-side session info panel (profile + task graph) |
| `panels/profile_panel.rs` | Profile indexer UI |
| `panels/task_graph_panel.rs` | `TaskNode`, orchestrator task graph visualization |
| `dialogs/settings_dialog.rs` | LLM / Profile / Agents settings tabs; cloud preset picker (Claude, OpenAI, Gemini, Groq, OpenRouter, Custom) |
| `dialogs/hil_dialog.rs` | HIL approval dialog with countdown timer |
| `dialogs/downloads_dialog.rs` | Downloads list dialog |
| `theme.rs` | `KitsuneTheme` color constants |

## Security Boundaries

| Component | Trust level | Notes |
|---|---|---|
| `kitsune-core` (broker) | Privileged | Owns vault, HIL gate, IPC bus, process manager. The only process allowed to spawn children. |
| `kitsune-agent` runtime | Semi-privileged | Vault access only via HIL-gated `TokenHandle`s. |
| `kitsune-cef` / WebView2 host | Sandboxed (target) | No filesystem, no direct broker IPC; sandbox profile in `SandboxProfile::renderer()`. |
| `kitsune-net` | Sandboxed | Network only; outbound 80/443/8080/8443 (`SandboxProfile::network_process`). |
| JS engine | Heavily sandboxed | No filesystem, no direct broker IPC. |

**Secret data**: master password (never stored, used to derive `SecretKey`), the derived `SecretKey` itself (`Zeroize`-on-drop, `Debug` redacted), the per-user 32-byte KDF salt (OS keyring under `kitsune-vault` / `kdf-salt`), a second 32-byte secret salt (OS keyring under `kitsune-vault` / `secret-salt`, used for HMAC origin pseudonymization), the cloud token (OS keyring `kitsune-engine` / `cloud-token`), all `SensitiveValue` byte buffers (`Zeroize`-on-drop).

**Encryption**: at-rest via `age` passphrase encryption with the Argon2id-derived key; KDF parameters `m_cost=65536, t_cost=3, p_cost=4` (`crates/kitsune-vault/src/crypto.rs`). Origin pseudonymization via HMAC-SHA256(secret_salt, origin). Site isolation identifiers via SHA256(seed || "kitsune-site-isolation-v1" || origin).

**Process isolation** (target architecture): broker is the only privileged process; renderer/network/JS/agent are spawned with `--role=<role>` (`crates/kitsune-core/src/broker.rs:159-198`) and apply their `SandboxProfile` early. On Windows: Job Objects. Linux seccomp-BPF and macOS Seatbelt are stubs.

## IPC & Protocol Contracts

- **Schema**: `kitsune-ipc::message::IpcMessage { correlation_id, sender, target, payload, timestamp }`. `payload: IpcPayload` is the discriminated union of every legal cross-process operation. New cross-process operations MUST be added as a variant here.
- **Serialization**: `postcard` (length-prefixed). Frame format is `u32 little-endian length || postcard bytes` (`crates/kitsune-ipc/src/transport.rs`).
- **Transport**: `interprocess::local_socket` over named pipes on Windows (`GenericNamespaced`), filesystem sockets at `/tmp/<name>` on Unix.
- **Capability enforcement**: `IpcChannel` carries a `HashSet<ProcessCapability>` and validates the payload before send. Capabilities: `VaultRead, VaultWrite, NetworkAccess, DomAccess, HilTrigger, ProcessSpawn, AgentRuntime, FileSystemAccess`.
- **Routing**: `crates/kitsune-core/src/broker.rs:79-113` is the authoritative routing table. DOM ops → Renderer; Network/Navigate → Network process; Vault/HIL → broker-local; everything else → UI.
- **Privilege levels**: `PrivilegeLevel::{Broker, SemiPrivileged, Sandboxed}`.
- **Correlation**: `CorrelationId(Uuid)` is set per request; `IpcMessage::respond` reuses it for replies.

## AgentAction Enum (complete, as of now)

```rust
pub enum AgentAction {
    Navigate { url: String },
    Click { element_id: usize },
    Fill { element_id: usize, value: String },
    Read { selector: String },
    ReadFile { path: String },
    Download { url: String, filename: Option<String> },
    Done { answer: String },
}
```

`Download` fetches via its own rustls reqwest client and saves to `dirs::download_dir()`.

## AgentEvent Enum (complete, as of now)

```rust
pub enum AgentEvent {
    Log(String),       // info-level free-form line → UI LogLevel::Info
    Step(String),      // indented sub-step (↳ Navigating…, ↳ Clicking…) → LogLevel::Step
    Thinking(String),  // raw <think>…</think> text → LogLevel::Think (collapsible)
    Navigated(String), // URL → mirrors in address bar
    Done(String),      // final answer → LogLevel::Ok
    Error(String),     // error → LogLevel::Block
}
```

## LLM Loop Behaviour

- **System prompt** is rebuilt each iteration with: user task, specialist context (if card selected), available actions, core/browsing/research rules, and a `<think>` block instruction.
- **`<think>` extraction**: `extract_thinking(raw)` splits model output at `<think>…</think>`; the reasoning is emitted as `AgentEvent::Thinking`, the remainder is parsed as the action JSON.
- **Empty action text** (model output only thinking, no JSON): emits `↳ Adjusting approach…` step and pushes a retry instruction into history.
- **Parse failure**: emits `↳ Response format unclear, retrying…` and retries.
- **History management**: observation pushed as `("user", …)` turn; action pushed as `("assistant", …)` turn. `trim_history` keeps first entry + 12 most recent.
- **Observation lines** go to `tracing` only — NOT to the UI log.
- **Raw JSON action** goes to `tracing` only — NOT to the UI log.
- **Navigate** shows domain only: `↳ Navigating to arxiv.org`.
- **Click** resolves element label via `elem_label()` (aria > text > placeholder > name > `[id]`): `↳ Clicking "Search"`.
- **Fill** shows label + truncated value: `↳ Typing "gaussian splatting" → "Search"`.
- **Done arm** does NOT self-emit — the caller (`run()`) emits `AgentEvent::Done` from `StepResult::Done`.
- **Max iterations**: 15.

## Specialist Agent Cards

Three hardcoded cards in `agent_panel.rs`:

| Card name | Icon | Specialist context |
|---|---|---|
| `PriceTracker` | ✈ | Price-tracking specialist — compare ≥2-3 sites, include best deal URL |
| `FormFillAgent` | 📝 | Form-filling specialist — read attached file first, request HIL before submit |
| `ResearchAgent` | 🔬 | Deep-research specialist — visit ≥3 authoritative sources, structured report |

Selecting a card calls `specialist_context(card_name)` which returns a string injected into the system prompt via `LlmAgentRuntime::with_agent_context`. Clicking an already-selected card deselects it.

## AgentOrchestrator Pipeline

A parallel pipeline (in addition to `LlmAgentRuntime`) powered by `AgentOrchestrator`:
- Takes a natural-language goal + `ProfileSummary`.
- Uses `AgentAiClient` with `ModelTier::Orchestrator` to plan a `Vec<SubTask>`.
- `SubTask` variants: `Search`, `Form`, `Submit`, `AccountCreate`, `Booking`.
- Dispatches each sub-task to the appropriate specialist agent (`SearchAgent`, `FormAgent`, `SubmitAgent`, `BookingAgent`).
- Results logged via `tracing::info!` — not yet surfaced in the UI task graph (stubs in `task_graph_panel.rs`).

This pipeline runs **concurrently** with `LlmAgentRuntime` when both `browser.orchestrator` and `browser.profile_summary` are available. It is additive — does not affect the LLM loop's execution.

## Cloud Presets (Settings → LLM → Cloud)

| Preset | Default endpoint | Default model |
|---|---|---|
| Claude | `https://api.anthropic.com/v1` | `claude-3-5-sonnet-20241022` |
| OpenAI | `https://api.openai.com/v1` | `gpt-4o-mini` |
| Gemini | `https://generativelanguage.googleapis.com/v1beta/openai` | `gemini-2.0-flash` |
| Groq | `https://api.groq.com/openai/v1` | `llama-3.3-70b-versatile` |
| OpenRouter | `https://openrouter.ai/api/v1` | `anthropic/claude-3.5-sonnet` |
| Custom | (user-entered) | (user-entered) |

All cloud presets go through `LlmBackend::Cloud` which POSTs to `{url}/chat/completions` (OpenAI-compatible wire format).

## External Dependencies (non-obvious only)

- `wry` 0.38 → WebView2 host. Crate named `kitsune-cef` for legacy reasons. Windows-tuned (Win32 `SetFocus` FFI, `WebViewBuilder::new_as_child`). `CefEvent` now includes `DownloadStarted`/`DownloadCompleted`. Window.open() redirected back to same tab via initialization script `NEW_WINDOW_REDIRECT_JS`.
- `age` 0.10 → vault encryption. Passphrase mode with Argon2id-derived key as passphrase.
- `argon2` → password-based KDF. Memory-hard parameters baked in.
- `keyring` 3 → OS keychain. Stores: KDF salt (`kitsune-vault/kdf-salt`), secret salt (`kitsune-vault/secret-salt`), cloud auth token (`kitsune-engine/cloud-token`).
- `interprocess` 2 → cross-platform local sockets / named pipes for IPC transport.
- `postcard` 1 → compact serialization for IPC wire format.
- `candle-core` / `candle-transformers` / `candle-nn` / `hf-hub` / `tokenizers` → feature-gated (`kitsune-ai/local-model`) for on-device inference. Pro tier only — not yet wired.
- `eframe` 0.30 / `egui` 0.30 → native shell (`glow` backend).
- `rusqlite` (bundled) → vault store.
- `rustls` 0.23 / `reqwest` with `rustls-tls` only → no native TLS; ensures TLS 1.3+ enforcement.
- `cookie_store` 0.21 → backs `PartitionedCookieJar` keyed by `(top_level_origin, request_origin)`.
- `axum` 0.7 + `tokio-stream` + `async-stream` → only used by `kitsune-cloud-mock`.
- `windows-sys` → Job Objects sandbox primitives.
- `zeroize` (with `derive`) → mandatory for every type holding a secret.
- `dirs` 5 → `dirs::download_dir()` used by `AgentAction::Download`.
- `rfd` → file picker dialog for attaching local files.
- `urlencoding` → URL encoding in `fallback_navigate`.
- `hex` → encoding for vault salt and origin pseudonyms.
- `parking_lot` → `RwLock` on HIL audit log.
- `dashmap` → concurrent map in `SiteIsolationMap`.
- `chrono` → timestamps on IPC messages, HIL audit log, vault entries.
- `tower-http` → CORS middleware in `kitsune-cloud-mock`.

## Build & Dev Workflow

```powershell
# Full debug build + run the browser
cargo run -p kitsune-ui

# Release build
cargo build --release -p kitsune-ui

# Mock cloud server (offline demo, also auto-started by KitsuneEngine::start)
cargo run -p kitsune-cloud-mock        # binds 127.0.0.1:7700

# Per-crate test
cargo test -p kitsune-vault
cargo test -p kitsune-hil

# Full test suite
cargo test --workspace

# Local-model AI (Pro tier path; pulls candle stack)
cargo build -p kitsune-ai --features local-model
```

Prereqs: Rust 1.75+. On Windows the Edge WebView2 runtime must be installed (Evergreen runtime, included with current Windows 11). `kitsune-cef::initialize()` calls `wry::webview_version()` on startup and fails fast if unavailable.

Logging: `RUST_LOG=info` (or `RUST_LOG=kitsune=debug`) — `tracing-subscriber` is initialized in `kitsune-ui/src/main.rs`. Demo server uses the same env filter.

The `kitsune-vault` tests construct an in-memory vault; they will prompt or fail in headless CI without keyring access — run them on a real desktop session.

## Current State & Known Gaps

Working:
- `egui` shell: tab bar, top bar, agent panel, session panel, HIL dialog, settings dialog (3 tabs: LLM / Profile / Agents), downloads dialog.
- Agent panel: multiline command input (Enter submits, Shift+Enter newline), file attach with binary detection, agent cards (PriceTracker / FormFillAgent / ResearchAgent) with selection toggle and specialist context injection.
- WebView2 embedding via `wry`, navigation, JS eval with callback, focus handoff, download events.
- Vault crypto path (Argon2id → age), audit table, site isolation map, per-origin pseudonymization. Token store with 30 s TTL and single-use `consume_token`.
- HIL gate flow (approve/reject/timeout) with audit log; non-cloneable approval tokens.
- AI router with `RoutingPolicy` invariant; cloud backend with retry-on-5xx-only and 429 surface.
- Network privacy layer: header strip/inject, tracker blocklist, partitioned cookie jar.
- Windows sandbox via Job Objects.
- IPC frame format (postcard + length-prefix) over named pipes.
- `kitsune-cloud-mock` SSE demo server: demo HTML pages, `POST /api/agent-run` SSE stream, `POST /api/hil-response`, `agent_brain` supports OpenAI-compatible + Ollama via `AiProvider`. Auto-started by `KitsuneEngine::start()` on `127.0.0.1:7700`.
- `LlmAgentRuntime` in-process agent loop: full `<think>` extraction + collapsible UI, step-level log messages with element labels and domain names (no raw IDs or full URLs in UI), `AgentAction::Download` with real reqwest + `dirs::download_dir()` save, file-read permission modal, cooperative stop flag, sensitive-field HIL gate, 15-iteration loop, local fallback planner on LLM-unavailable.
- `AgentOrchestrator` multi-agent pipeline wired in `start_agent_run` (parallel to LLM loop) when profile summary is available.
- `ProfileIndexer` / `ProfileSummary` with LLM-driven extraction.
- `CaptchaAgent` with detection for reCAPTCHA v2/v3, hCaptcha, Cloudflare Turnstile.
- Full workspace test suite passes. `cargo build --release -p kitsune-ui` succeeds.

Stubbed / partial:
- **Multi-process is a target, not the runtime.** `KitsuneEngine::start` registers all child roles as mock in-process channels. `ProcessManager::spawn_child` works but the spawned child does not connect back over the real IPC.
- **IPC transport privilege check** at `crates/kitsune-ipc/src/transport.rs` is a placeholder — real per-`ProcessRole` capability validation has yet to land.
- **Linux/macOS sandboxing**: `apply_linux_sandbox` and `apply_macos_sandbox` log and return `Ok` without doing anything.
- **`LocalAiBackend`** (in `kitsune-ai`): `local-model` feature-gated; candle wiring is scaffolded but inference is not implemented. End-user local LLM via Ollama works through `LlmBackend::Ollama` in `loop_runtime.rs` — separate concern.
- **Vault disclosure last mile**: `VaultBackend::consume_token` is implemented but the DOM injection path (`IpcPayload::DomFillField { value_token }` → renderer lookup → actual form fill) is not yet wired. The `Fill` action in the loop runtime currently injects the LLM-supplied value directly.
- **`request_access` on `VaultBackend`** checks `context.has_hil_approval` but returns a new `TokenHandle::new()` unconditionally (not bound to any stored secret).
- **Reversible action log** (hash-linked audit chain): the `kitsune-hil` audit log and vault audit table are unchained. No `prev_hash` column.
- **Task graph panel**: `TaskNode` struct exists but the UI only renders a stub; orchestrator sub-task results are not yet streamed into `browser.task_nodes`.
- **`set_request_handler`** on `CefBrowser` is a TODO — request inspection is not yet wired into WebView2 events.
- `kitsune-agent/src/lib.rs` carries `#![allow(warnings)]` — there is known dead/in-flux code.

## Next Up (priority order)

1. **Finish vault disclosure last mile.** Wire `IpcPayload::DomFillField { value_token }` to dereference the token via `consume_token` in the renderer just before injection. Replace `request_access`'s placeholder with a real ACL check against `SiteIsolationMap`.
2. **Hash-linked action log.** Add a table (in vault audit or a new `kitsune-core` table) with a `prev_hash BLOB` column populated by `sha256(canonical_postcard(entry) || prev_hash)`. Append from both HIL approvals and vault disclosures.
3. **Real IPC capability check.** Replace the placeholder in `crates/kitsune-ipc/src/transport.rs` with the typed path that already exists on `IpcChannel::validate_capability`. Fail closed; log denials with `correlation_id` and `role`.
4. **Task graph UI.** Wire `AgentOrchestrator::run` results into `browser.task_nodes` so the `task_graph_panel` actually renders sub-task state (Pending / Running / Completed / Failed).
5. **`set_request_handler` for the WebView2 host.** Wire `wry`'s navigation/request events into the existing `RequestHandler` extension point so the network privacy layer can inspect renderer-initiated requests.
6. **Smoke-test the multi-process path.** Get `ProcessManager::spawn_child` to produce a child that connects back over the named-pipe transport with a real `ProcessRole`.
7. **Cross-platform sandbox.** Linux seccomp-BPF and macOS Seatbelt are still stubs.
8. **`LocalAiBackend` candle wiring.** Pro-tier in-process inference. Lower priority because end-user local LLM is already covered via the Ollama HTTP path.
9. **README cleanup.** Replace remaining "CEF" references with "wry/WebView2" in user-facing docs.

## Files to Read First

1. `crates/kitsune-core/src/lib.rs` — process model and broker role.
2. `crates/kitsune-core/src/broker.rs` — `ProcessManager`, `route_payload` routing table, crash policy.
3. `crates/kitsune-ipc/src/message.rs` — `IpcMessage`, `IpcPayload` (the cross-process protocol).
4. `crates/kitsune-vault/src/backend.rs` — vault contract: `retrieve`, `consume_token`, `token_store`, `origin_pseudonym`.
5. `crates/kitsune-hil/src/gate.rs` and `crates/kitsune-hil/src/approval.rs` — HIL flow and the non-cloneable approval token.
6. `crates/kitsune-ai/src/router.rs` — `RoutingPolicy::always_local` invariant for sensitive task types.
7. `crates/kitsune-agent/src/loop_runtime.rs` — the active LLM agent loop: `LlmAgentRuntime`, `LlmBackend`, `AgentEvent`, `execute_action`, `build_system_prompt`, `extract_thinking`.
8. `crates/kitsune-agent/src/action.rs` — `AgentAction` enum (includes Download).
9. `crates/kitsune-agent/src/spec.rs` — `AgentSpec`, `AgentConstraints`, `VaultAccessLevel`.
10. `crates/kitsune-ui/src/app.rs` — top-level `KitsuneBrowser` state, `LogLevel`, `AgentSseAction`, `CloudPreset`.
11. `crates/kitsune-ui/src/panels/agent_panel.rs` — agent panel rendering, `start_agent_run`, `render_log_entry`, specialist card logic.
12. `crates/kitsune-cef/src/lib.rs` — `CefBrowser` (the wry/WebView2 wrapper), `CefEvent`, download events.
13. `crates/kitsune-sandbox/src/lib.rs` — `SandboxProfile` per role and the platform-specific application paths.

## Naming Conventions & Code Style

- Crate names are kebab-case `kitsune-<area>`, internal Rust idents are snake/PascalCase as standard. The crate prefix `kitsune_` does not appear inside types — `VaultBackend`, `HilGate`, `AgentRuntime`, not `KitsuneVault…`.
- Errors: every crate has its own `error.rs` with a `thiserror` enum (`VaultError`, `HilError`, `IpcError`, `AgentError`, `AiError`, `NetError`, `SandboxError`, `CefError`) and a `Result<T>` alias re-exported from `lib.rs`. Application-level glue uses `anyhow::Result`. Do not mix the two within a single library crate.
- Modules are flat per crate (`mod foo;`, `pub use foo::*;` from `lib.rs`); the only nested modules are `kitsune-ui::{chrome, dialogs, panels}` and `kitsune-agent::agents::*`.
- Tests live `#[cfg(test)] mod tests` in the same file as the unit being tested. Integration tests at `crates/<crate>/tests/<name>.rs`.
- Architectural rules are documented as `// ARCHITECTURE:` block comments at the top of `lib.rs` for each crate. Hard rules are tagged `// INVARIANT:`. Treat both as load-bearing documentation; preserve them when refactoring.
- Sensitive types: `#[derive(Zeroize)] #[zeroize(drop)]` plus a manual `Debug` that emits `[REDACTED]`. New types holding secrets must follow this pattern.
- Logging: `tracing` macros with structured fields. Don't log secret values. Observation detail and raw LLM JSON go to `tracing` only — never to the UI log.
- No comments in normal code. Only comment when the WHY is non-obvious (hidden constraint, subtle invariant, workaround). Never explain what the code does.
