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
    ├── kitsune-agent              # Agent runtime: AgentSpec, AgentRuntime, BudgetTracker, DomAccessor
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
Platform-specific: `kitsune-sandbox` (only Windows path is implemented; Linux seccomp-BPF and macOS Seatbelt are stubs that log and return Ok). `kitsune-cef` is Windows-only in practice (WebView2 host, `SetFocus` Win32 FFI).
Demo-only: `kitsune-cloud-mock`.

## Key Invariants (NEVER violate these)

These are derived from the actual code patterns and explicit `INVARIANT:` comments — not invented.

1. **Vault never returns raw secrets across any boundary.** `VaultBackend::retrieve` returns a `TokenHandle`, never plaintext (see `crates/kitsune-vault/src/backend.rs:90-114`). IPC `VaultResponse` carries `token_handle: Option<String>` only — there is no payload variant for raw secret bytes (`crates/kitsune-ipc/src/message.rs:137-141`). DOM fill is done via `DomFillField { value_token }` — opaque tokens, never raw values.
2. **`TaskType::VaultDecision` and `TaskType::SensitiveForm` MUST stay local.** Enforced at the type level in `RoutingPolicy::always_local` in `crates/kitsune-ai/src/router.rs:43`. The field is private and not user-configurable. If local is unavailable, the request fails — there is no cloud fallback.
3. **HIL approvals are non-cloneable, single-use, action-bound, 30-second TTL.** `HilApproval` deliberately does NOT implement `Clone` (`crates/kitsune-hil/src/approval.rs:46`). `APPROVAL_EXPIRY_SECONDS = 30`. An approval consumed for a different `ActionId` errors out — bypass via token reuse is type-system-impossible.
4. **Vault refuses to initialize if the OS keyring is unavailable.** `VaultBackend::new` returns `VaultError::SecureStorageUnavailable` rather than falling back to unencrypted storage (`crates/kitsune-vault/src/backend.rs:31-44`, `crates/kitsune-vault/src/lib.rs:15-16`). No "best-effort" mode exists.
5. **Cross-origin identifiers are architecturally distinct.** Each origin gets a deterministic per-user pseudonym via HMAC over `seed || "kitsune-site-isolation-v1" || origin` (`crates/kitsune-vault/src/site_isolation.rs:49-59`). Two origins can never share an identifier; vault entries are looked up by `origin_pseudonym` so cross-site reuse is impossible.
6. **Cloud quota exhaustion never silently retries.** On 429, `KitsuneCloudBackend` returns `AiError::QuotaExhausted` for the UI to surface as an upgrade prompt; only network errors and 5xx are retried (`crates/kitsune-ai/src/cloud.rs:8-11, 33-37`).
7. **Agents inherit denials, not capabilities.** `AgentConstraints` defaults to `can_initiate_payments = false`, `can_create_accounts = false`, `can_send_communications = false`, `hil_required_for = ["all"]` (`crates/kitsune-agent/src/spec.rs:149-163`). Capabilities must be explicitly granted in the spec; soft instructions in the goal text are not capabilities.
8. **Cloud auth tokens live in the OS keychain, not on disk and not in env vars.** `KEYRING_SERVICE = "kitsune-engine"` / `KEYRING_USER = "cloud-token"` (`crates/kitsune-ai/src/cloud.rs:25-26`).
9. **Renderers and network processes never talk to each other directly.** All routing flows through `ProcessManager::route` in the broker, which uses the static `route_payload` table (`crates/kitsune-core/src/broker.rs:79-113`). There is no peer-to-peer IPC path.

## Data Flow

Single-process MVP today; the type contracts are designed for the multi-process target. Trace of an agentic action ("Buy this item"):

1. **User → UI** (`kitsune-ui::app::KitsuneBrowser`, `panels::agent_panel::agent_panel`): natural-language prompt entered in the agent shelf.
2. **UI → Agent runtime** (`kitsune-agent::AgentRuntime`): the prompt is bound to an `AgentSpec` whose `AgentConstraints` are the contract for what's allowed.
3. **Agent → AI** (`kitsune-ai::AiRouter::route`): the request is classified by `TaskType` and dispatched to either `KitsuneCloudBackend` (default) or `LocalAiBackend` (Pro). Vault/sensitive task types are forced local. `BudgetTracker::check_budget` is called by the caller, never the backend.
4. **Agent → DOM** (`kitsune-agent::dom_access::DomAccessor`): JS is generated, sent over `mpsc::Sender<WebViewCommand>` to the WebView2 host (`kitsune-cef::CefBrowser::execute_js_with_callback`). Results return on a `mpsc` channel via the `__kitsune_ipc` IPC handler.
5. **Sensitive action reached** → `HilGate::checkpoint(trigger_class, data_labels)` posts an `HilCheckpoint` to `kitsune-ui`, which renders `dialogs::hil_dialog`.
6. **User decides** → `respond_to_checkpoint` resolves the gate's `oneshot::Sender<ApprovalDecision>`. On approve, an `HilApproval` is constructed and consumed by the action executor. On reject or 30s timeout, `HilError::UserRejected` / `Dismissed` aborts the action.
7. **Vault disclosure** (only after approval): `VaultBackend::retrieve` decrypts via `CryptoBackend::decrypt` (age + Argon2id), produces a `TokenHandle`, logs to `audit` table. The token — not the secret — flows through `IpcPayload::DomFillField { value_token }`.
8. **Network** flows out through `kitsune-net::KitsuneHttpClient` → `apply_privacy_protections` strips `Referer`, injects `DNT`/`Sec-GPC`, blocks known trackers, enforces TLS 1.3.
9. **Result → UI**: agent log entry + budget update + privacy report rendered in side panels.

In the multi-process target (`crates/kitsune-core/src/lib.rs:1-10`), steps 4 and 8 cross process boundaries: agent → broker → renderer (DOM ops) and broker → network process (HTTP). The `ProcessRole` enum and `route_payload` table already encode the routing rules for this; today, all child roles are `register_mock`'d as in-process tokio mpsc channels.

## Security Boundaries

| Component | Trust level | Notes |
|---|---|---|
| `kitsune-core` (broker) | Privileged | Owns vault, HIL gate, IPC bus, process manager. The only process allowed to spawn children. |
| `kitsune-agent` runtime | Semi-privileged | Vault access only via HIL-gated `TokenHandle`s. |
| `kitsune-cef` / WebView2 host | Sandboxed (target) | No filesystem, no direct broker IPC; sandbox profile in `SandboxProfile::renderer()`. |
| `kitsune-net` | Sandboxed | Network only; outbound 80/443/8080/8443 (`SandboxProfile::network_process`). |
| JS engine | Heavily sandboxed | No filesystem, no direct broker IPC. |

**Secret data**: master password (never stored, used to derive `SecretKey`), the derived `SecretKey` itself (`Zeroize`-on-drop, `Debug` redacted), the per-user 32-byte secret salt (in OS keyring under `kitsune-vault` / `secret-salt`), the cloud token (OS keyring `kitsune-engine` / `cloud-token`), all `SensitiveValue` byte buffers (`Zeroize`-on-drop).

**Encryption**: at-rest via `age` passphrase encryption with the Argon2id-derived 32-byte key encoded as hex; KDF parameters `m_cost=65536, t_cost=3, p_cost=4` (`crates/kitsune-vault/src/crypto.rs:25-32`). Origin pseudonymization via HMAC-SHA256(secret_salt, origin).

**Process isolation** (target architecture): broker is the only privileged process; renderer/network/JS/agent are spawned with `--role=<role>` (`crates/kitsune-core/src/broker.rs:159-198`) and apply their `SandboxProfile` early. On Windows: Job Objects with `JOB_OBJECT_UILIMIT_DESKTOP | JOB_OBJECT_UILIMIT_GLOBALATOMS | JOB_OBJECT_UILIMIT_HANDLES` and process memory limit (`crates/kitsune-sandbox/src/lib.rs:181-247`). Linux seccomp-BPF and macOS Seatbelt are stubs.

## IPC & Protocol Contracts

- **Schema**: `kitsune-ipc::message::IpcMessage { correlation_id, sender, target, payload, timestamp }`. `payload: IpcPayload` is the discriminated union of every legal cross-process operation (`crates/kitsune-ipc/src/message.rs:131-234`). New cross-process operations MUST be added as a variant here — there is no escape hatch.
- **Serialization**: `postcard` (length-prefixed). Frame format is `u32 little-endian length || postcard bytes` (`crates/kitsune-ipc/src/transport.rs:189-211`).
- **Transport**: `interprocess::local_socket` over named pipes on Windows (`GenericNamespaced` with name e.g. `"kitsune-broker"`), filesystem sockets at `/tmp/<name>` on Unix.
- **Capability enforcement**: `IpcChannel` carries a `HashSet<ProcessCapability>` and validates the payload before send (`crates/kitsune-ipc/src/channel.rs:55-70`). Capabilities are: `VaultRead, VaultWrite, NetworkAccess, DomAccess, HilTrigger, ProcessSpawn, AgentRuntime, FileSystemAccess`.
- **Routing**: `crates/kitsune-core/src/broker.rs:79-113` is the authoritative routing table. DOM ops → Renderer; Network/Navigate → Network process; Vault/HIL → broker-local; everything else → UI.
- **Privilege levels**: `PrivilegeLevel::{Broker, SemiPrivileged, Sandboxed}` (`crates/kitsune-ipc/src/message.rs:60-68`).
- **Correlation**: `CorrelationId(Uuid)` is set per request; `IpcMessage::respond` reuses it for replies.

Note: `transport.rs:106-117` currently has a placeholder permission check that allows almost everything — see Current State below.

## External Dependencies (non-obvious only)

- `wry` 0.38 → WebView2 host. The crate is named `kitsune-cef` for legacy reasons but does not link CEF; it embeds the OS WebView2 runtime and is Windows-tuned (Win32 `SetFocus` FFI for keyboard handoff to egui, `WebViewBuilder::new_as_child`).
- `age` 0.10 → vault encryption. Chosen over raw AEAD because age handles versioning, header parsing, and authenticator binding for us. Used in passphrase mode with the Argon2id-derived key as the passphrase.
- `argon2` → password-based KDF for the vault `SecretKey`. Memory-hard parameters baked in.
- `keyring` 3 → OS keychain (Windows Credential Manager / macOS Keychain / Secret Service). Stores the per-user secret salt and the cloud auth token. Vault refuses to start without it.
- `interprocess` 2 → cross-platform local sockets / named pipes for the IPC transport (replaces hand-rolled OS-specific code).
- `postcard` 1 → compact, no_std-friendly serialization for the IPC wire format. Chosen over JSON to keep the IPC frame small and to avoid pulling a JSON parser into sandboxed processes.
- `candle-core` / `candle-transformers` / `candle-nn` / `hf-hub` / `tokenizers` → feature-gated (`kitsune-ai/local-model`) for on-device Phi-3-mini inference. Pro tier only.
- `eframe` 0.30 / `egui` 0.30 → native shell. `glow` backend (default-features = false on `eframe`).
- `rusqlite` (bundled) → vault store. `bundled` so we don't depend on a system sqlite.
- `rustls` 0.23 / `reqwest` with `rustls-tls` only → no native TLS; ensures consistent TLS 1.3+ enforcement across platforms.
- `cookie_store` 0.21 → backs the `PartitionedCookieJar` that keys cookies by `(top_level_origin, request_origin)`.
- `axum` 0.7 + `tokio-stream` + `async-stream` → only used by `kitsune-cloud-mock` for the SSE demo server.
- `windows-sys` / `windows` → Job Objects sandbox primitives.
- `zeroize` (with `derive`) → mandatory for every type that holds a secret.

## Build & Dev Workflow

```powershell
# Full debug build + run the browser
cargo run -p kitsune-ui

# Release build
cargo build --release -p kitsune-ui

# Mock cloud server (offline demo)
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

The `kitsune-vault` tests construct an in-memory vault that talks to the OS keyring; they will prompt or fail in headless CI without keyring access — run them on a real desktop session.

## Current State & Known Gaps

Working:
- `egui` shell, tab bar, top bar, agent panel, session panel, HIL dialog, settings dialog (with provider radio toggle: OpenAI-compatible vs Ollama).
- WebView2 embedding via `wry`, navigation, JS eval with callback, focus handoff.
- Vault crypto path (Argon2id → age), audit table, site isolation map, per-origin pseudonymization.
- HIL gate flow (approve/reject/timeout) with audit log; non-cloneable approval tokens.
- AI router with `RoutingPolicy` invariant; cloud backend with retry-on-5xx-only and 429 surface; quota cache at `data_dir()/kitsune/quota_cache.json`.
- Network privacy layer: header strip/inject, tracker blocklist, partitioned cookie jar.
- Windows sandbox via Job Objects.
- IPC frame format (postcard + length-prefix) over named pipes.
- `kitsune-cloud-mock` SSE demo server with end-to-end agent flow:
  - `agent_brain` supports both OpenAI-compatible APIs and Ollama via `AiProvider` enum.
  - `local_plan()` deterministic offline planner covers wikipedia / youtube / github / news / shopping / "go to X" / direct URLs / generic Google fallback (17 unit tests).
  - `parse_action_json` survives small-model preambles.
  - Verified: `POST /api/agent-run {"command":"search wikipedia for batman"}` emits SSE `url_update` to `https://en.wikipedia.org/wiki/Batman`.
- `kitsune-agent` integration tests rewritten against the current 4-arg `DomAccessor::new(vault, hil_gate, url, webview_tx)` API: `tests/dom_access.rs` (3 smoke tests) and `tests/executor.rs` (1 navigate-and-complete test).
- Full workspace test suite passes (~107 tests). `cargo build --release -p kitsune-ui` succeeds.

Stubbed / partial:
- **Multi-process is a target, not the runtime.** `KitsuneEngine::new` constructs the broker with placeholder vault password `"password"` and zero salt (`crates/kitsune-core/src/engine.rs:48`); child processes are `register_mock`'d in-process. `ProcessManager::spawn_child` works but the spawned child does not yet connect back over the real IPC.
- **IPC transport privilege check** at `crates/kitsune-ipc/src/transport.rs:106-117` is a placeholder that allows everything except a fallthrough — not the production rule. Real per-`ProcessRole` capability validation has yet to land here (the typed `IpcChannel::validate_capability` path is the intended enforcement point).
- **Linux/macOS sandboxing**: `apply_linux_sandbox` and `apply_macos_sandbox` log and return `Ok` without doing anything.
- **`LocalAiBackend`** (in `kitsune-ai`): feature-gated; the candle wiring is scaffolded (`LocalModelInner` has comments where the model/tokenizer fields will go) but inference is not implemented. (Note: end-user local LLM via Ollama is wired through `kitsune-cloud-mock`'s `agent_brain` and works today — this `LocalAiBackend` is the in-process Pro-tier path, separate concern.)
- **`set_request_handler`** on `CefBrowser` is a TODO — request inspection is not yet wired into WebView2 events.
- **`VaultBackend::retrieve` decrypts but does not return the plaintext** by design; today it also doesn't bind the returned `TokenHandle` to the decrypted bytes — the disclosure flow's last mile (token → injection at the renderer) is not yet built.
- **`request_access` on `VaultBackend`** is a placeholder returning `TokenHandle::new()` unconditionally.
- **Reversible action log** (hash-linked audit chain) is not implemented. Today there are two unchained audit tables: `kitsune-vault::audit` (vault disclosures) and `kitsune-hil` audit (HIL decisions). Neither has a `prev_hash` column.
- **`kitsune-agent` ↔ `kitsune-cloud-mock` are two separate agent paths.** The egui demo flow drives the cloud-mock server's `agent_brain` over HTTP/SSE, not the in-process `AgentRuntime`. The runtime's vault/HIL/DomAccessor wiring exists but isn't on the demo path.
- `kitsune-agent/src/lib.rs` carries `#![allow(warnings)]` — there is known dead/in-flux code; do not assume every public type is wired up.
- README references CEF; the actual rendering backend is `wry` (WebView2). The crate name was kept for compatibility.

## Next Up (priority order)

1. **Wire the in-process `AgentRuntime` into the demo path.** Today the egui agent panel POSTs to the cloud-mock server. Replace this with a direct call into `kitsune-agent::AgentRuntime` so the vault, HIL gate, and `DomAccessor` are actually exercised on the test query path. The cloud-mock can stay as a fixture site, not as the agent brain.
2. **Finish the vault disclosure last mile.** Make `VaultBackend::retrieve` bind the returned `TokenHandle` to the decrypted bytes (in-memory map keyed by token id, with TTL + single-use semantics matching `HilApproval`), and replace `request_access`'s placeholder with a real ACL check against `SiteIsolationMap`. Then wire `IpcPayload::DomFillField { value_token }` to dereference the token in the renderer just before injection.
3. **Hash-linked action log.** Add a `kitsune-core` table (or extend the vault audit) with a `prev_hash BLOB` column populated by `sha256(canonical_postcard(entry) || prev_hash)`. Append from both HIL approvals and vault disclosures so a single chain covers all consequential actions. No JSON/PDF export yet — the chain itself is the deliverable.
4. **Real IPC capability check.** Replace the placeholder in `crates/kitsune-ipc/src/transport.rs:106-117` with the typed path that already exists on `IpcChannel::validate_capability`. Fail closed; log denials with `correlation_id` and `role`.
5. **Replace placeholder vault password in `KitsuneEngine::new`.** Today it hardcodes `"password"` and zero salt (`crates/kitsune-core/src/engine.rs:48`). Prompt for the master password on first run, derive the salt from the OS keyring entry under `kitsune-vault` / `secret-salt`, and refuse to start otherwise (matches invariant #4).
6. **`set_request_handler` for the WebView2 host.** Wire `wry`'s navigation/request events into the existing `RequestHandler` extension point so the network privacy layer can inspect renderer-initiated requests, not just `KitsuneHttpClient`-initiated ones.
7. **Smoke-test the multi-process path.** Get `ProcessManager::spawn_child` to produce a child that actually connects back over the named-pipe transport with a real `ProcessRole`, even if it's just an echo handler. This is the prerequisite for retiring the `register_mock` shims.
8. **Cross-platform sandbox.** Linux seccomp-BPF and macOS Seatbelt are still stubs. Until these land, sandboxing claims should be Windows-only in any docs/README.
9. **`LocalAiBackend` candle wiring.** Pro-tier in-process inference. Lower priority than items 1–7 because end-user local LLM is already covered via the Ollama HTTP path.
10. **README cleanup.** Replace remaining "CEF" references with "wry/WebView2" (the `kitsune-cef` crate name stays for compat; the user-facing copy should not lie).

## Naming Conventions & Code Style

- Crate names are kebab-case `kitsune-<area>`, internal Rust idents are snake/PascalCase as standard. The crate prefix `kitsune_` does not appear inside types — `VaultBackend`, `HilGate`, `AgentRuntime`, not `KitsuneVault…`.
- Errors: every crate has its own `error.rs` with a `thiserror` enum (`VaultError`, `HilError`, `IpcError`, `AgentError`, `AiError`, `NetError`, `SandboxError`, `CefError`) and a `Result<T> = std::result::Result<T, ThisError>` alias re-exported from `lib.rs`. Application-level glue in `kitsune-core` and binaries uses `anyhow::Result`. Do not mix the two within a single library crate.
- Modules are flat per crate (`mod foo;`, `pub use foo::*;` from `lib.rs`); the only nested modules are `kitsune-ui::{chrome, dialogs, panels}` for grouping egui widgets.
- Tests live `#[cfg(test)] mod tests` in the same file as the unit being tested. Integration tests are at `crates/<crate>/tests/<name>.rs` (e.g. `kitsune-ai/tests/local.rs`).
- Architectural rules are documented as `// ARCHITECTURE:` block comments at the top of `lib.rs` for each crate. Hard rules are tagged `// INVARIANT:`. Treat both as load-bearing documentation; preserve them when refactoring.
- Sensitive types: `#[derive(Zeroize)] #[zeroize(drop)]` plus a manual `Debug` that emits `[REDACTED]`. New types holding secrets must follow this pattern.
- Logging: `tracing` macros — `info!`, `warn!`, `error!`, `debug!` with structured fields (`role = ?role`, `correlation_id = %id`). Don't log secret values. `Display` is implemented on identifier types so `%foo` works.

## Files Claude Should Always Read First

1. `crates/kitsune-core/src/lib.rs` — process model and broker role.
2. `crates/kitsune-core/src/broker.rs` — `ProcessManager`, `route_payload` routing table, crash policy.
3. `crates/kitsune-ipc/src/message.rs` — `IpcMessage`, `IpcPayload` (the cross-process protocol).
4. `crates/kitsune-vault/src/lib.rs` and `crates/kitsune-vault/src/backend.rs` — vault contract and disclosure rules.
5. `crates/kitsune-hil/src/gate.rs` and `crates/kitsune-hil/src/approval.rs` — HIL flow and the non-cloneable approval token.
6. `crates/kitsune-ai/src/router.rs` — `RoutingPolicy::always_local` invariant for sensitive task types.
7. `crates/kitsune-agent/src/spec.rs` — `AgentSpec`, `AgentConstraints`, `VaultAccessLevel` (the agent capability surface).
8. `crates/kitsune-ui/src/app.rs` — top-level `KitsuneBrowser` state, the egui glue, and the WebView2 hookup.
9. `crates/kitsune-cef/src/lib.rs` — `CefBrowser` (the wry/WebView2 wrapper) and `RequestHandler` extension point.
10. `crates/kitsune-sandbox/src/lib.rs` — `SandboxProfile` per role and the platform-specific application paths.
