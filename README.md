# KitsuneEngine

A privacy-first, agentic desktop browser built in Rust. Runs an `egui` native shell over an embedded WebView2 surface (via `wry`) and gives the user an in-browser AI agent that can navigate, read DOM, and fill forms — with every consequential action gated behind a non-bypassable Human-in-the-Loop (HIL) approval flow.

Sensitive data lives in a local `age`-encrypted vault keyed off Argon2id. Agents only ever receive opaque token handles, never raw secrets.

---

<img width="1919" height="1028" alt="image" src="https://github.com/user-attachments/assets/963872e3-2cef-425c-990d-78a9f783eb4f" />

## Core Philosophy

- **Structural safety, not prompt safety.** The HIL gate, vault token model, and task routing are enforced by the type system and IPC capability checks — not by instructions in a system prompt.
- **Privacy-first.** Zero telemetry. `Referer` stripped, `DNT`/`Sec-GPC` injected, tracker blocklist enforced, cookies partitioned by `(top_level_origin, request_origin)`. TLS 1.3+ enforced via rustls.
- **Local by default for sensitive data.** `TaskType::VaultDecision` and `TaskType::SensitiveForm` are statically routed to local — no cloud fallback, by design.

---

## Architecture

Cargo workspace, 12 crates, single binary target `kitsune` (in `kitsune-ui`).

```
kitsune-engine/
├── crates/
│   ├── kitsune-core          # Broker — orchestrator, owns vault/HIL/IPC
│   ├── kitsune-ui            # egui native shell + main binary
│   ├── kitsune-cef           # WebView2 host via wry (legacy crate name)
│   ├── kitsune-agent         # LLM agent loop, AgentSpec, orchestrator, profile
│   ├── kitsune-agent-builder # No-code AgentSpec construction + validation
│   ├── kitsune-ai            # AiBackend trait + AiRouter + QuotaTracker
│   ├── kitsune-hil           # HilGate, HilApproval (non-cloneable, 30s TTL)
│   ├── kitsune-vault         # age-encrypted SQLite + SiteIsolationMap + audit log
│   ├── kitsune-ipc           # IpcMessage/IpcPayload, IpcChannel, IpcServer
│   ├── kitsune-net           # KitsuneHttpClient, PartitionedCookieJar, privacy headers
│   ├── kitsune-sandbox       # Per-OS process sandboxing (Windows Job Objects)
│   └── kitsune-cloud-mock    # axum SSE server for offline demo
```

---

## Key Invariants

1. **Vault never returns raw secrets across any boundary.** `VaultBackend::retrieve` returns a `TokenHandle`; decrypted bytes are bound in an in-memory token store (30 s TTL, single-use `consume_token`). IPC carries `token_handle: Option<String>` only.
2. **`TaskType::VaultDecision` and `TaskType::SensitiveForm` are always local.** Enforced at the type level in `RoutingPolicy::always_local`. Private field, not user-configurable. Local unavailable → fails, no cloud fallback.
3. **HIL approvals are non-cloneable, single-use, action-bound, 30-second TTL.** `HilApproval` does not implement `Clone`. An approval bound to one `ActionId` cannot be reused for another.
4. **Vault KDF salt lives in the OS keychain, never hardcoded.** Random 32-byte salt under `kitsune-vault / kdf-salt`. Dev-only fixed-salt fallback emits a tracing warning and must not reach production.
5. **Cross-origin identifiers are architecturally distinct.** `SiteIsolationMap` derives identifiers via SHA-256; `VaultBackend` derives per-origin storage keys via HMAC-SHA256. Two origins can never share an identifier.
6. **Agents inherit denials, not capabilities.** `AgentConstraints` defaults to `can_initiate_payments = false`, `can_create_accounts = false`, `hil_required_for = ["all"]`. Capabilities must be explicitly granted in the spec.

---

## Agentic Action Flow

1. User enters a natural-language prompt in the agent shelf.
2. Prompt is bound to an `AgentSpec` (with `AgentConstraints` as the contract).
3. `LlmAgentRuntime` loop: observe page via injected JS → send to LLM (Ollama or OpenAI-compatible) → parse `AgentAction` → execute.
4. `<think>` blocks are extracted and emitted as collapsible `Thinking` log entries.
5. Sensitive action → `HilGate::checkpoint` posts an `HilCheckpoint` to the UI.
6. User approves/denies in the HIL dialog (scale-in animation, RED accent, depleting countdown bar).
7. On approval → `VaultBackend::retrieve` → `TokenHandle` → vault bytes bound 30 s, single-use.
8. Network flows through `KitsuneHttpClient` → privacy protections applied.

---

<img width="1919" height="1015" alt="image" src="https://github.com/user-attachments/assets/c53f04e1-338c-425e-a4c3-a89e45bec70d" />

## UI

Dark cyberpunk theme. All colors, spacing, and font sizes come from `theme::{colors, spacing, fonts}` — never hardcoded.

**Animation primitives** (`animation.rs`):

- `lerp_anim` — smooth per-widget float toward a target, state in `egui::Context::data`.
- `pulse_anim` — sine-wave pulse [0, 1] at configurable Hz.
- `spinner_char` — braille spinner at ~10 fps.

**Chrome**: 3-row top bar (tab strip + titlebar drag/window controls, nav bar with address + privacy pill + downloads, collapsible bookmarks bar).

**Agent panel**: pulsing status dot (1.5 Hz), focus-aware input card, color-coded log entries with 2 px left-border strips (Think=yellow, Cmd=blue, Ok=green, Warn=amber, Block=red). Swarm config bar.

**Agent cards**: PriceTracker, FormFillAgent, ResearchAgent — with lerp hover brightness, SWARM badge, left accent strip. Selecting a card injects a specialist system prompt.

**HIL dialog**: scale-in animation (0.85 → 1.0, speed 12), 1.5 px RED frame, RED-tinted header, GREEN "Approve" / outline RED "Deny" buttons, right-to-left depleting countdown bar.

**Session panel**: collapsible sections with animated open/close, `cap_toggle` animated switch, `vault_item` color-coded entries.

---

## LLM Backends

| Mode | How |
|---|---|
| Local (Ollama) | `LlmBackend::Ollama` → HTTP to `127.0.0.1:11434` |
| Cloud | `LlmBackend::Cloud` → OpenAI-compatible `POST {url}/chat/completions` |

Cloud presets: **Claude** (`api.anthropic.com`), **OpenAI**, **Gemini**, **Groq**, **OpenRouter**, Custom.

Cloud auth token lives in the OS keychain (`kitsune-engine / cloud-token`), not on disk or in env vars. 429 → `AiError::QuotaExhausted` surfaced as an upgrade prompt; only 5xx and network errors are retried.

---

## Build & Run

**Prerequisites**: Rust 1.75+, Edge WebView2 runtime (included with Windows 11).

```powershell
# Run the browser
cargo run -p kitsune-ui

# Release build
cargo build --release

# Local-model AI (Pro tier, pulls candle stack)
cargo build -p kitsune-ai --features local-model
```

Logging: `RUST_LOG=info` or `RUST_LOG=kitsune=debug`.

> Vault tests require keyring access — run on a real desktop session, not headless CI.

---

## Status

**Working**: Full dark UI with animation system, WebView2 embedding, LLM agent loop (15-iteration, `<think>` extraction, element labels, domain-only navigation log), HIL gate flow with audit log, vault crypto (Argon2id + age), AI router with local-only policy, network privacy layer, Windows Job Object sandbox, postcard+named-pipe IPC frame format, `AgentOrchestrator` multi-agent pipeline, `ProfileIndexer`, `CaptchaAgent`, full workspace test suite passes, release build succeeds.

**Stubbed**: Multi-process runtime (all child roles are mock in-process channels), vault disclosure last mile (DOM injection path not yet wired), IPC capability check (placeholder), task graph UI (struct exists, rendering is a stub), Linux/macOS sandboxing, `LocalAiBackend` candle inference, reversible hash-linked action log.

---

## Security Model Summary

| Component | Trust Level |
|---|---|
| `kitsune-core` (broker) | Privileged — owns vault, HIL gate, IPC bus |
| `kitsune-agent` runtime | Semi-privileged — vault access via HIL-gated tokens only |
| `kitsune-cef` / WebView2 host | Sandboxed — no filesystem, no direct broker IPC |
| `kitsune-net` | Sandboxed — outbound 80/443/8080/8443 only |
| JS engine | Heavily sandboxed |

Secret data is `Zeroize`-on-drop throughout. `Debug` impls on secret types emit `[REDACTED]`.
