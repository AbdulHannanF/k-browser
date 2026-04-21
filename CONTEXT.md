# KitsuneEngine — Complete A-Z Project Description

## 1. What It Is

**KitsuneEngine** is a privacy-first, AI-agent-native browser engine built entirely from scratch in Rust. It is **not** a Chromium fork or wrapper — every component (HTML parser, CSS engine, layout engine, renderer, JS runtime, network stack, sandbox, vault, and AI layer) is implemented natively.

- **Version**: 0.1.0 (release candidate), transitioning to 0.2.0-preview for Phase 6
- **License**: Apache 2.0
- **Website**: https://kengine.tech
- **Platform**: Windows primary (Linux/macOS sandbox stubs exist)

### Core Philosophy

- **Zero telemetry** — no data ever leaves the device without explicit consent
- **Human-in-the-Loop (HIL)** — consequential actions (payments, form submissions, credential usage) require explicit human confirmation via a non-bypassable gate
- **Sandboxed multi-process** — renderer, network, JS, and agent processes are isolated via Windows Job Objects
- **Encrypted vault** — all sensitive data stored with per-entry `age` encryption and Argon2id key derivation
- **AI agents as first-class citizens** — structured, auditable, budget-tracked agents that can never bypass HIL

---

## 2. Architecture Overview

The engine follows a **multi-process broker model**:

```
┌─────────────────────────────────────────────────┐
│              kitsune-core (Broker)               │
│  Orchestrates vault, HIL gate, IPC bus, tabs    │
├─────────┬──────────┬──────────┬─────────────────┤
│ Renderer│ Network  │ JS Engine│  Agent Runtime  │
│(sandbox)│(sandbox) │(sandbox) │  (sandboxed)    │
└─────────┴──────────┴──────────┴─────────────────┘
         ↕ IPC Bus (kitsune-ipc) ↕
┌─────────────────────────────────────────────────┐
│              kitsune-ui (UI Shell)               │
│  egui native window, panels, HIL dialog         │
└─────────────────────────────────────────────────┘
```

---

## 3. The 15 Crates — Role and Implementation Status

### `kitsune-core` — Privileged Broker Process

- **Role**: Central orchestrator; owns the vault, HIL gate, IPC bus, tab list
- **Key files**:
  - `crates/kitsune-core/src/engine.rs:1-123` — `KitsuneEngine` struct with start/shutdown/tab management, Windows sandbox spawning
  - `crates/kitsune-core/src/pipeline.rs:1-570` — `PagePipeline`: fetch → parse → style → layout → paint; CSS transitions (opacity, color, background-color); JS execution with 5s timeout; error types
  - `crates/kitsune-core/src/tab.rs:1-91` — `Tab` struct with ID, title, URL, loading state, fingerprint score, `JobObjectSandbox` + `renderer_pid`
  - `crates/kitsune-core/src/config.rs:1-97` — `EngineConfig` aggregating `PrivacyConfig`, `AgentConfig`, `UiConfig` with defaults
- **Status**: Complete — pipeline, engine lifecycle, tab management all functional

### `kitsune-ui` — Native UI Shell (egui)

- **Role**: Browser chrome — panels, dialogs, onboarding, tab bar
- **Key files**:
  - `crates/kitsune-ui/src/main.rs:1-40` — Entry point: logging init, `eframe::run_native` with window options
  - `crates/kitsune-ui/src/app.rs:1-100` — `KitsuneApp` state machine, `KitsuneSettings` persistence, onboarding flow
  - `crates/kitsune-ui/src/theme.rs:1-198` — `KitsuneTheme` dark-first palette, `egui::Visuals` conversion, `apply_panel_style`
  - `crates/kitsune-ui/src/panels.rs:1-230` — Privacy Dashboard, Agent Shelf, Vault Manager, HIL Confirmation, Onboarding panels
  - `crates/kitsune-ui/src/hil_window.rs:1-294` — Separate always-on-top OS window for HIL; `HilTriggerClass`, `HilRequest`, `HilDecision`, countdown timer, blocking dialog
  - `crates/kitsune-ui/src/widgets/agent_shelf.rs:1-325` — Slide-out shelf with agent cards, animated status dots, budget gauge, activity log
  - `crates/kitsune-ui/src/widgets/tab_bar.rs:1-274` — Custom tab bar with favicon placeholders, close buttons, new-tab button, hover effects
- **Demo pages**: `welcome.html`, `shop.html`, `privacy.html` — curated for investor demo
- **Status**: Complete (Phase 5); Phase 6 upgrades needed (custom visuals, DOM highlight overlay)

### `kitsune-vault` — Encrypted Key-Value Store

- **Role**: Local-only, encrypted credential store with disclosure policies
- **Security properties**:
  1. Never returns raw secrets — returns `GrantedAccess` tokens only
  2. Each origin gets unique, stable pseudonymous identifier
  3. Cross-site tracking via shared identifiers is architecturally impossible
  4. All access logged in audit trail
  5. At-rest encryption with Argon2id-derived keys
  6. **INVARIANT**: If secure enclave key storage fails, vault REFUSES to initialize
- **Key files**:
  - `crates/kitsune-vault/src/lib.rs:1-34` — Modules: backend, types, policy, access, audit, crypto, site_isolation, error, db
  - `crates/kitsune-vault/src/types.rs:1-264` — `VaultKey`, `VaultCategory` (Password/Address/Payment/Identity/ApiKey/Custom), `SensitiveValue` (Zeroize + `[REDACTED]` Debug), `TokenHandle` (5-min expiry), `RequestContext`, `DomainPattern` (wildcard matching)
- **Status**: Complete — 100% test coverage required per CONTRIBUTING.md

### `kitsune-hil` — Human-in-the-Loop Gate

- **Role**: Ensures no consequential action executes without explicit human confirmation
- **Security properties**:
  - `HilApproval` is **deliberately NOT Clone** — approvals consumed exactly once (move semantics)
  - 30-second expiry window
  - Action-bound — cannot be used for a different action
  - `ActionId` binding enforced at `consume()` time
- **Key files**:
  - `crates/kitsune-hil/src/lib.rs:1-21` — Modules: gate, trigger, approval, presentation, error
  - `crates/kitsune-hil/src/approval.rs:1-195` — `HilApproval` (non-Clone), `ApprovalDecision`, `consume()` with mismatch/expiry checks, `ActionId`
- **Status**: Complete — 100% test coverage required per CONTRIBUTING.md

### `kitsune-agent` — AI Agent Runtime

- **Role**: Structured, auditable agent configurations executing browser automation within strict safety constraints
- **Security properties**:
  1. Agents can NEVER bypass HIL for consequential actions
  2. Capabilities declared in `AgentConstraints` (not soft instructions)
  3. Cost accounting mandatory for all external interactions
  4. Agents receive opaque tokens from vault, never raw secrets
  5. Agent lineage tracked — sub-agents inherit intersection of parent constraints
- **Key files**:
  - `crates/kitsune-agent/src/spec.rs:1-232` — `AgentSpec` (id, name, goal, tools, constraints, triggers, budget, author), `AgentTool` enum (Navigate/DomRead/FormFill/Click/FormSubmit/NetworkFetch/VaultAccess/Screenshot/Wait/TextExtract/HumanInput), `AgentConstraints`, `VaultAccessLevel`, `DomainPolicy`, `AgentBudget`, `MoneyAmount`, `AgentAuthor` (Human/Agent/System lineage)
  - `crates/kitsune-agent/src/runtime.rs:1-293` — `AgentRuntime` (load/validate/get agents), `AgentInstance`, `AgentStatus` (Idle/Running/WaitingForUser/Paused/Completed/Failed), `AgentActionLog`, `get_system_templates()` returning 5 core agents: **ResearchAgent**, **FormFillAgent**, **PriceTracker**, **InboxManager**, **LoginAuditor**
  - `crates/kitsune-agent/src/dom_access.rs:1-239` — `DomAccessor` with `query_text()`, `query_links()`, `fill_field()` (vault injection), `click_element()` (HIL for submit buttons), `navigate()`, DOM highlight IPC (Reading/Acting/Done styles with fade-in/pulse/fade-out phases)
- **Status**: Complete

### `kitsune-ai` — AI Power Layer

- **Role**: Intelligence backend for agents; two modes: KitsuneCloud (free, 100 actions/month) and LocalModel (Pro, on-device Phi-3-mini via candle)
- **Key invariants**:
  - `VaultDecision` and `SensitiveForm` task types **never** route to cloud
  - PII scrubbed before any request leaves the device
  - User token stored in OS keychain only
  - Quota exhausted → agents pause → UI upgrade prompt → never silent retry
- **Key files**:
  - `crates/kitsune-ai/src/lib.rs:1-101` — `BackendType` (KitsuneCloud/LocalModel/UserApiKey), `UserTier` (Free/Pro/Enterprise), `AiBackend` trait
  - `crates/kitsune-ai/src/router.rs:1-215` — `AiRouter` with routing rules: always-local → local-preferred → cloud → quota fallback; `RoutingPolicy` with hardcoded `always_local` for VaultDecision/SensitiveForm
- **Status**: Complete (cloud backend stub; local model behind `local-model` feature flag)

### `kitsune-net` — Privacy-Aware Network Stack

- **Role**: All network I/O in sandboxed process; enforces privacy at protocol level
- **Features**: Referer stripping, DNT + Sec-GPC injection, tracker blocking, TLS 1.3+ only, fingerprinting detection
- **Key files**:
  - `crates/kitsune-net/src/lib.rs:1-129` — `PrivacyAwareRequest`, `RequestPrivacySettings` (defaults: strip_referer=true, send_dnt=true, send_gpc=true, min_tls=Tls13, block_trackers=true), `HttpResponse`, `PrivacyReport`
  - `crates/kitsune-net/src/privacy.rs:1-168` — `apply_privacy_protections()`, `is_tracker()`, `privacy_user_agent()`, `detect_fingerprinting_vectors()`, known tracker blocklist (doubleclick, google-analytics, facebook, etc.)
- **Status**: Complete

### `kitsune-html` — HTML Parser

- **Role**: Parses HTML into a structured DOM tree
- **Dependencies**: html5ever + markup5ever_rcdom
- **Status**: Complete

### `kitsune-css` — CSS Parser and Cascade Engine

- **Role**: CSS parsing, cascade resolution, computed style generation
- **Key files**:
  - `crates/kitsune-css/src/lib.rs:1-272` — `ComputedStyle` (display, position, box model, colors, fonts, grid, transitions), `GridStyle`, `GridTrackDef`, `GridPlacement`, `TransitionSpec`, `DisplayType`, `PositionType`, `CssValue`, `CssColor`, `BoxEdges`
- **Status**: Complete; Phase 6 needs `position: absolute/fixed` support

### `kitsune-layout` — Layout Engine

- **Role**: Box model, flexbox, and CSS Grid layout; produces layout tree from styled DOM
- **Key files**:
  - `crates/kitsune-layout/src/lib.rs:1-41` — `LayoutRect`, modules: box_model, flex, grid, layout_tree, engine
  - `crates/kitsune-layout/src/grid.rs:1-390` — `GridEngine` backed by Taffy 0.5; maps `StyledNode` → Taffy tree → `LayoutNode`; supports fr/px/auto tracks, `repeat()`, gap, child span; 5 comprehensive tests
- **Status**: Complete; Phase 6 needs Taffy Grid API upgrade for full spec compliance

### `kitsune-render` — GPU-Accelerated Renderer

- **Role**: Takes layout tree → display list → GPU surface via wgpu
- **Key files**:
  - `crates/kitsune-render/src/lib.rs:1-60` — `RenderCommand` (FillRect, DrawText, DrawBorder, DrawImage), `DisplayList`
  - `crates/kitsune-render/src/painter.rs:1-256` — `paint()` with frame hash caching, `paint_node()` (background, text, children), `paint_highlights()` with phase-based animation (FadingIn→Active/Pulsing→FadingOut) and Reading (yellow)/Acting (blue)/Done (green) styles
- **Status**: Complete; Phase 6 needs GPU paint path integration

### `kitsune-js` — Embedded JavaScript Engine

- **Role**: V8 isolate with heavy sandboxing for in-page JS execution
- **Key files**:
  - `crates/kitsune-js/src/engine.rs:1-189` — `JsEngine` wrapping `v8::OwnedIsolate`; `execute()` with try-catch error boundaries; `inject_bindings()` shim that:
    - Provides `console.log/warn/error` via `__kitsune_native`
    - **Hard-blocks**: `eval()`, `fetch()`, `XMLHttpRequest`, `open()` (popup windows)
    - Mock DOM: `DOMElement`, `document` (cookie writes blocked, querySelector returns null), `window`, `setTimeout`, `MutationObserver`, `IntersectionObserver`
    - `navigator.userAgent` locked to `"KitsuneEngine/0.1"`
- **Status**: Complete

### `kitsune-ipc` — Inter-Process Communication

- **Role**: Capability-based IPC bus connecting all sandboxed processes to the broker
- **Key files**:
  - `crates/kitsune-ipc/src/message.rs:1-299` — `IpcMessage` envelope with correlation IDs; `IpcPayload` enum covering: Vault ops, Network fetch, HIL checkpoint, DOM query/fill/click, DOM highlights (`SetDomHighlight`/`ClearDomHighlight`/`ClearAllDomHighlights`), Navigation, Process lifecycle, Agent actions; `ProcessCapability` flags; `PrivilegeLevel` (Broker/SemiPrivileged/Sandboxed); `DomHighlight` with `HighlightRect`, `HighlightStyle`, `HighlightPhase`
- **Status**: Complete

### `kitsune-sandbox` — Process Sandboxing

- **Role**: Unified sandbox interface across platforms (Windows Job Objects, Linux seccomp-BPF, macOS Seatbelt)
- **Key files**:
  - `crates/kitsune-sandbox/src/lib.rs:1-212` — `SandboxProfile` with presets: `maximum_restriction()`, `renderer()` (GPU allowed, 1GB mem), `network_process()` (outbound 80/443/8080/8443, 256MB), `agent()` (256MB, 60s CPU), `js_engine()` (512MB, 10s CPU); `FileSystemPolicy`, `NetworkPolicy`; Windows implementation is TODO stub
- **Status**: Partially complete — profiles defined, Windows Job Object implementation is TODO

### `kitsune-agent-builder` — No-Code Agent Builder UI

- **Role**: Visual drag-and-drop interface for creating agent specs without coding
- **Status**: Complete (UI scaffolding exists)

---

## 4. The 7 Absolute Security Invariants

1. **Vault never returns raw secrets** — only `GrantedAccess` / `OpaqueToken` handles
2. **HIL gate is non-bypassable** — `HilApproval` is non-Clone, 30s expiry, action-bound
3. **Agent constraints are structural, not advisory** — `AgentConstraints` enforced at type level
4. **Cross-site tracking is architecturally impossible** — per-origin pseudonymous identifiers
5. **JS eval/fetch/XHR/popup/cookie-write are hard-blocked** — enforced in V8 shim
6. **VaultDecision and SensitiveForm never route to cloud** — hardcoded in `RoutingPolicy`
7. **Vault refuses to initialize if secure key storage fails** — no fallback to unencrypted

---

## 5. Development Phase History

| Phase | Focus | Tests |
|-------|-------|-------|
| 1 | Project scaffold, HTML parser, CSS parser | ~20 |
| 2 | Layout engine (flexbox), renderer, JS engine | ~45 |
| 3 | Vault, HIL gate, IPC bus, sandbox profiles | ~70 |
| 4 | Agent runtime, AI layer, UI shell | ~90 |
| 5 | Integration, demo pages, MSI packaging | ~100+ |

All phases 1–5 are **complete**. The project reached v0.1.0 release candidate status.

---

## 6. What Has Been Done (Complete)

- Full HTML5 parser (html5ever-based)
- CSS parser with cascade resolution and Grid support
- Layout engine with flexbox + CSS Grid (Taffy 0.5)
- GPU-accelerated renderer with display list + frame caching
- DOM highlight overlay system (Reading/Acting/Done with phase animations)
- V8 JavaScript engine with sandboxed API surface
- Privacy-aware network stack (Referer stripping, DNT, GPC, tracker blocking, TLS 1.3)
- Encrypted vault with per-entry `age` encryption, Argon2id, Zeroize, audit trail
- HIL gate with non-Clone approval tokens, 30s expiry, action binding
- Agent runtime with 5 system templates, budget tracking, domain policies
- AI router with cloud/local routing, always-local invariant for sensitive tasks
- Capability-based IPC with privilege levels and correlation IDs
- Sandbox profiles for renderer/network/agent/JS processes
- egui UI shell with dark theme, tab bar, agent shelf, privacy dashboard, vault manager
- HIL confirmation window (separate OS window, always-on-top, countdown)
- Onboarding flow
- 3 curated demo pages (welcome, shop, privacy)
- Web compatibility tested against Wikipedia, GitHub, HackerNews, Reddit
- 100+ tests across all crates

---

## 7. What Remains (Phase 6 — Investor Preview Build)

Phase 6 goal: **Make the engine demo-ready for investors**. Version 0.2.0-preview.

### 7.1 Rendering Baseline

- **CSS `position: absolute/fixed`** — currently maps to Taffy `Absolute` but needs full coordinate resolution
- **`<details>/<summary>` rendering** — noted in COMPAT.md
- **GPU paint path** — integrate wgpu surface rendering with the display list
- **Scroll support** — basic overflow scrolling

### 7.2 UI Overhaul

- **Custom egui visuals** — move beyond default egui look; implement `KitsuneTheme` fully
- **Tab bar polish** — smooth animations, favicon loading states
- **Agent shelf refinement** — budget gauge, activity log improvements
- **DOM highlight overlay** — render highlights from IPC messages onto the page surface

### 7.3 Agent Demo Loop

- **End-to-end agent execution** — wire `AgentRuntime` → `DomAccessor` → IPC → UI in a live demo
- **Agent activity visualization** — show Reading/Acting/Done highlights in real-time
- **HIL flow demo** — trigger HIL from agent actions, show confirmation window

### 7.4 KitsuneCloud Mock

- **axum mock server** — local HTTP server serving demo pages with embedded trackers
- **tower/tower-http** middleware for CORS, logging
- **Demo script** — predefined investor demo flow (welcome page → shop → agent fills form → HIL → privacy report)

### 7.5 Build & Packaging

- **MSI packaging** — via `cargo wix` + WiX Toolset (was remaining at Phase 5 close)
- **Windows antivirus exclusion** — documented workaround for OS error 32 during builds

### 7.6 Known Build Issues

- **OS Error 32** — Windows antivirus (Defender) locks build artifacts during compilation; workaround: exclude `target/` directory or disable real-time protection during builds
- **`markup5ever_rcdom` version** — semver metadata warning in `kitsune-html/Cargo.toml`

---

## 8. Approved Dependencies (Key)

| Category | Crates |
|----------|--------|
| Async | tokio, futures |
| Serialization | serde, serde_json |
| Error | thiserror, anyhow |
| Logging | tracing, tracing-subscriber |
| Crypto | argon2, age, zeroize, ring |
| Networking | hyper, rustls, tokio-rustls |
| HTML/CSS | html5ever, markup5ever_rcdom, cssparser, selectors |
| Layout | taffy 0.5 |
| GPU | wgpu, egui, eframe |
| IPC | tokio (channels) |
| JS | rusty_v8 (v8 0.92) |
| Data | sqlite (via rusqlite), uuid, chrono |
| Sandbox | windows-sys |
| AI | candle (feature-gated), async-trait |
| Phase 6 | axum, tower, tower-http |

---

## 9. Build & Run Commands

```bash
cargo build --release          # Full release build
cargo run --bin kitsune        # Launch the browser
cargo nextest run -j1          # Run tests (single-threaded for vault/HIL)
cargo clippy --all-targets --all-features -- -D warnings  # Lint
cargo doc --no-deps --open     # Generate docs
cargo wix                      # Build MSI installer
```

---

## 10. Testing Strategy

- All crates have unit tests
- **`kitsune-vault`** and **`kitsune-hil`** require **100% test coverage** (security-critical)
- Tests run single-threaded (`-j1`) to avoid vault/HIL state races
- Grid layout has 5 integration tests (equal fr, mixed columns, repeat, gap, child span)
- Privacy layer has tests for tracker detection, header injection, referer stripping
- HIL approval has tests for consume success, wrong action, non-cloneable verification
- DOM highlight painter has tests for fade-in alpha and auto-removal

---

## 11. Investor Demo Script (Phase 6 Target)

1. Launch KitsuneEngine → **Welcome page** with tracker/referer/JS stats
2. Navigate to **Demo Shop** → 8-product grid with CSS Grid layout
3. Activate **FormFillAgent** → agent highlights fields (yellow Reading → blue Acting → green Done)
4. Agent attempts form submit → **HIL confirmation window** appears (always-on-top, countdown)
5. User approves → form submitted via vault token (no raw credentials exposed)
6. Navigate to **Privacy Report** → shows blocked trackers, stripped referers, active settings
7. Open **Agent Shelf** → see agent cards, budget gauge, activity log

---

*This is the complete A-Z description of KitsuneEngine as it stands today — a fully architected, security-hardened browser engine with 15 crates, 7 inviolable security invariants, and a clear path to the Phase 6 investor demo milestone.*