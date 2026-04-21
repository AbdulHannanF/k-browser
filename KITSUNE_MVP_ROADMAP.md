# KitsuneEngine — MVP Roadmap
## From Lean Codebase → Full Investor-Ready Browser

> **Current state:** Refactor complete. Bloat crates deleted (`kitsune-html`, `kitsune-css`,
> `kitsune-layout`, `kitsune-render`, `kitsune-js`, `pipeline.rs`). Security core intact.
> Build time ~2 min. Codebase ~3,500 lines.
>
> **This document:** Everything needed to go from here to a shippable, demonstrable,
> full-featured privacy browser. Phases are sequential. Each phase is independently valuable.

---

## Rendering Decision: CEF, not wry

The refactor doc specified `wry`. **This is now changed to CEF.**

**Why:**
- `wry` on Windows shells into Microsoft Edge (WebView2). Error pages, right-click menus,
  certificate dialogs — all carry Edge branding you cannot remove
- CEF (Chromium Embedded Framework) is raw Chromium with no shell. Every pixel is yours
- CEF's `ResourceRequestHandler` fires **inside** the render process, before any byte hits the
  network — this is where `kitsune-net` belongs architecturally, not as an outer wrapper
- Discord, Spotify, Steam, VS Code, Adobe all use CEF. It is production-proven at massive scale

**What changes in the codebase:**
```toml
# Remove from kitsune-ui/Cargo.toml:
# wry = "0.38"

# Add:
# CEF Rust bindings — thin FFI over the stable CEF C API
# No published crate yet; write a minimal wrapper in crates/kitsune-cef/
# CEF C API is stable, versioned, and documented at: https://cef-builds.spotifycdn.com/
```

**Long-term (Phase 5+): Servo**
Servo is the Mozilla-originated browser engine written entirely in Rust, now under the Linux
Foundation. When it reaches ~80% web platform test coverage, Kitsune migrates the content
renderer to Servo — making the entire stack one language, one memory model, one security story.
This is the real vision. CEF is the bridge that gets you shipping.

---

## Architecture (Post-Refactor, Post-CEF)

```
┌─────────────────────────────────────────────────────────────────┐
│                      kitsune-ui  (egui)                          │
│                                                                   │
│  ┌────────────────┐  ┌──────────────────────────┐  ┌──────────┐ │
│  │  Agent Panel   │  │    CEF Content View       │  │ Session  │ │
│  │  (left, 300px) │  │  (center, fills rest)     │  │ (right,  │ │
│  │                │  │                           │  │  200px)  │ │
│  │  • cmd input   │  │  Real Chromium rendering  │  │          │ │
│  │  • agent cards │  │  Any URL, any page        │  │ • stats  │ │
│  │  • live log    │  │  Agent DOM highlights     │  │ • caps   │ │
│  │  • budget bar  │  │  Privacy report bar       │  │ • vault  │ │
│  └────────────────┘  └──────────────────────────┘  └──────────┘ │
│                                                                   │
│  ┌────────────────────── Top Bar ───────────────────────────────┐ │
│  │  🦊 Kitsune │ tabs │ ◀ ▶ ↻ │ address bar │ 🛡 N blocked   │ │
│  └───────────────────────────────────────────────────────────────┘│
└─────────────────────────────────────────────────────────────────┘
              ↕ channels (tokio mpsc)
┌─────────────────────────────────────────────────────────────────┐
│                     kitsune-core                                  │
│  KitsuneEngine • TabManager • NavigationHistory                   │
└─────────────────────────────────────────────────────────────────┘
              ↕
┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────┐
│ kitsune  │ │ kitsune  │ │ kitsune  │ │ kitsune  │ │ kitsune  │
│  -vault  │ │  -hil    │ │  -agent  │ │  -ai     │ │  -net    │
│          │ │          │ │          │ │          │ │          │
│ Encrypted│ │ Non-Clone│ │ Runtime  │ │ Router   │ │ Privacy  │
│ KV store │ │ approval │ │ 5 agents │ │ local/   │ │ middleware│
│ age+     │ │ 30s expiry│ │ budget  │ │ cloud    │ │ tracker  │
│ argon2id │ │ action   │ │ audit   │ │ always-  │ │ blocking │
│ tokens   │ │ binding  │ │ log     │ │ local inv│ │ TLS 1.3  │
└──────────┘ └──────────┘ └──────────┘ └──────────┘ └──────────┘
              ↕
┌─────────────────────────────────────────────────────────────────┐
│               kitsune-cef  (NEW — CEF FFI wrapper)               │
│                                                                   │
│  CefApp • CefClient • ResourceRequestHandler • SchemeHandler      │
│  ExecuteJavaScript • OnLoadEnd • OnTitleChange • DevTools         │
└─────────────────────────────────────────────────────────────────┘
```

---

## Complete MVP Feature List

This is every feature that must work before the build is demo-ready.
Checked = already done. Unchecked = needs building.

### 🌐 Browser Core
- [x] Tab model (`TabId`, `Tab` struct, `title`, `url`, `loading_state`)
- [x] Navigation history (back/forward stack per tab)
- [x] `EngineConfig`, `PrivacyConfig`, `AgentConfig`, `UiConfig`
- [ ] CEF process lifecycle (CefInitialize / CefShutdown in main thread)
- [ ] Multiple tabs — open, close, switch, reorder
- [ ] Real page loading via CEF (any URL, any website)
- [ ] Tab title update from page `<title>` tag
- [ ] Favicon loading per tab (CEF `OnFaviconURLChange`)
- [ ] Page load progress indicator (0–100% in top bar)
- [ ] Stop loading button (replaces reload during load)
- [ ] `kitsune://` custom scheme handler (welcome, privacy, vault pages)

### 🎨 Browser Chrome (egui)
The visual shell. Matches the prototype in `kitsune-mvp.html`.

**Top Bar:**
- [ ] 🦊 Logo + wordmark (left, fixed)
- [ ] Tab bar — tabs with title, favicon placeholder, close button (×), new tab (+)
- [ ] Active tab indicator (amber underline / background)
- [ ] Back button (◀) — disabled when no history
- [ ] Forward button (▶) — disabled when no forward stack
- [ ] Reload button (↻) — becomes stop (✕) during load
- [ ] Address bar — full-width text input, shows current URL
- [ ] Address bar — lock icon (🔒 green = TLS, ⚠ amber = mixed, 🔓 red = HTTP)
- [ ] Address bar — on Enter: navigate; on Escape: revert to current URL
- [ ] Privacy pill (🛡 N blocked) — updates live, green when >0

**Left Panel — Agent Workspace (300px):**
- [ ] Panel header: "Agent Workspace" + live status dot (pulsing green when running)
- [ ] Command input: full-width text field, placeholder "Ask agent to do anything…"
- [ ] Run button: amber, triggers agent with current command text
- [ ] Enter key in command field = Run
- [ ] Agent cards (one per system agent):
  - [ ] PriceTracker card — icon ✈, name, description, status badge
  - [ ] FormFillAgent card — icon 📝, name, description, status badge
  - [ ] ResearchAgent card — icon 🔬, name, description, status badge
  - [ ] InboxManager card — icon 📬, name, description, status badge
  - [ ] LoginAuditor card — icon 🔍, name, description, status badge
  - [ ] Status badge: `idle` (grey) / `running` (amber, pulsing) / `done` (green) / `error` (red)
  - [ ] Active card gets amber border highlight
- [ ] Agent Log scroll area:
  - [ ] Colored log entries: `cmd` (white), `info` (grey), `ok` (green), `warn` (amber), `block` (red)
  - [ ] Auto-scrolls to bottom on new entry
  - [ ] Timestamps on each entry (`HH:MM:SS`)
  - [ ] Clear log button
- [ ] Budget gauge:
  - [ ] Label: "Monthly Budget · N / 100 actions"
  - [ ] Progress bar with amber fill, glow effect
  - [ ] Turns red at >80% usage
  - [ ] Pro tier shows "Unlimited"

**Center — Content Area:**
- [ ] CEF browser fills this entire rect, repositioned every frame
- [ ] DOM highlight overlay (canvas layer on top of CEF, agent highlights):
  - [ ] Reading phase: yellow (#FFD700) outline + glow on target element
  - [ ] Acting phase: blue (#4A9EFF) outline + glow
  - [ ] Done phase: green (#4ADE80) outline, fades out after 1.5s
  - [ ] Overlay clears when agent stops
- [ ] Privacy report bar (slides up from bottom of content area after page load):
  - [ ] Trackers blocked count
  - [ ] Referers stripped count
  - [ ] TLS version
  - [ ] DNT + GPC status
  - [ ] Dismiss button (✕)
- [ ] Custom error page (served from `kitsune://error`):
  - [ ] 🦊 fox icon (large)
  - [ ] Error title + reason (connection refused, DNS failed, etc.)
  - [ ] Try Again button
  - [ ] No Microsoft, no Edge, no cloud — pure Kitsune

**Right Panel — Session State (200px):**
- [ ] Panel header: "Session"
- [ ] Status rows (label + value, monospace):
  - `status` → `● Active` (green dot)
  - `mode` → `Agent-First`
  - `tls` → `1.3` (green) or `1.2` (amber) or `none` (red)
  - `trackers` → `N blocked` (green when >0, grey when 0)
  - `referer` → `stripped` (green)
  - `fingerprint` → `hardened` (green)
  - `hil gate` → `armed` (green)
- [ ] Capabilities section:
  - DOM Control — ON/OFF badge
  - Vault Access — ON/OFF badge
  - Audit Log — ON/OFF badge
  - Sandbox — ON/OFF badge
  - Network — ON/OFF badge
  - Screenshot — ON/OFF badge (off by default)
- [ ] Vault mini section (bottom of right panel):
  - Shows masked vault entries (email, card last 4, address label)
  - Each entry shows type icon + masked value + "token" label
  - "Open Vault" button → navigates to `kitsune://vault`

### 🤖 Agent System
- [x] `AgentSpec` — id, name, goal, tools, constraints, budget, author
- [x] `AgentRuntime` — load/validate/run agents
- [x] `AgentStatus` — Idle/Running/WaitingForUser/Paused/Completed/Failed
- [x] `AgentActionLog` — per-action audit trail
- [x] 5 system agent templates (PriceTracker, FormFillAgent, ResearchAgent, InboxManager, LoginAuditor)
- [x] `AgentBudget` — per-session and per-action cost tracking
- [x] `DomAccessor` — query_text, query_links, fill_field, click_element, navigate
- [ ] Natural language command → structured action plan (via `kitsune-ai`)
- [ ] `DomAccessor` internals replaced with CEF `ExecuteJavaScript` calls
- [ ] DOM highlight phases wired through CEF → overlay layer → egui
- [ ] Agent → HIL gate → UI flow live end-to-end
- [ ] Agent activity visible in log in real-time
- [ ] Agent can be paused mid-task (pause button in workspace)
- [ ] Agent can be cancelled (cancel button, cleans up state)
- [ ] PriceTracker: navigate multiple sites, extract prices, return comparison
- [ ] FormFillAgent: identify form fields, fill via vault tokens, submit via HIL
- [ ] ResearchAgent: navigate to URL, extract text/links, summarise via AI
- [ ] InboxManager: read inbox page, list subjects/senders, flag urgent
- [ ] LoginAuditor: scan saved vault entries, flag weak/reused/old passwords

### 🔐 HIL Gate
- [x] `HilApproval` — non-Clone, 30s expiry, action binding
- [x] `HilGate` — blocks until human responds
- [x] `ActionId` — consumed exactly once at approve time
- [ ] HIL overlay dialog in egui:
  - [ ] Amber pulsing badge: "⚠ Approval Required"
  - [ ] Title: what the agent is trying to do
  - [ ] Action detail rows: flight/item, price, credentials note
  - [ ] Countdown timer bar (30s, drains to zero)
  - [ ] Countdown number updates every second
  - [ ] "Confirm →" button (amber, full width of left column)
  - [ ] "Cancel" button (right column, red on hover)
  - [ ] Backdrop blur behind dialog
  - [ ] Timer expiry → auto-cancel → log entry "HIL timeout"
  - [ ] Escape key = cancel
- [ ] HIL confirmation → agent proceeds → log "action executed"
- [ ] HIL cancel → agent aborts → state rolled back → log "action cancelled"
- [ ] HIL window is always-on-top (separate OS window for security)

### 🔐 Vault
- [x] Per-entry `age` encryption, Argon2id key derivation
- [x] `VaultCategory` — Password/Address/Payment/Identity/ApiKey/Custom
- [x] `TokenHandle` — 5-min expiry, never exposes raw secret
- [x] `DisclosurePolicy` — per-origin token uniqueness
- [x] Audit trail — every access logged with timestamp, requester, outcome
- [ ] `kitsune://vault` page (rendered as egui panel via custom scheme):
  - [ ] Entry list: icon + name + category + last-used date
  - [ ] Add entry form: category selector, name, value (never shown again after save)
  - [ ] Delete entry (requires HIL confirmation)
  - [ ] Audit log tab: chronological access log, filterable
  - [ ] Lock button: clears key from memory, requires password to reopen
  - [ ] Search/filter entries

### 🛡 Privacy Engine
- [x] `apply_privacy_protections()` — Referer strip, DNT/GPC inject
- [x] Tracker blocklist — doubleclick, google-analytics, facebook, etc.
- [x] TLS 1.3 minimum enforcement
- [x] Fingerprinting detection
- [ ] Wire into CEF `ResourceRequestHandler` (replaces outer-process interception):
  - [ ] `on_before_resource_load` — strip Referer, inject DNT, inject Sec-GPC
  - [ ] `on_resource_request` — block tracker domains, increment counter
  - [ ] Privacy stats update in real-time to egui top bar pill
- [ ] `kitsune://privacy` report page:
  - [ ] Per-session stats: trackers blocked, referers stripped, headers injected
  - [ ] Per-domain breakdown: which sites tried to track, what was blocked
  - [ ] TLS grade per site
  - [ ] Fingerprint vectors detected
  - [ ] Export report as JSON button
- [ ] Cookie isolation — partitioned per top-level origin
- [ ] `navigator.webdriver` spoofed to `false` (anti-bot-detection)

### 🤖 AI Layer
- [x] `AiRouter` — cloud/local routing with `always_local` invariant
- [x] `BackendType` — KitsuneCloud / LocalModel / UserApiKey
- [x] `UserTier` — Free/Pro/Enterprise
- [x] `QuotaTracker` — 100 actions/month free, unlimited Pro
- [ ] Wire natural language input → AI router → structured action plan
- [ ] AI response drives `AgentRuntime.run_plan()`
- [ ] Local model (Phi-3-mini via candle) — behind `local-model` feature flag
- [ ] KitsuneCloud mock endpoint (`/api/ai-action`) for demo
- [ ] Quota exhausted → UI shows upgrade prompt → agents pause gracefully

### 🌐 Network
- [x] `KitsuneHttpClient` — reqwest + cookie jar + TLS 1.3
- [x] `PartitionedCookieJar` — per-origin isolation
- [x] Privacy middleware wired into HTTP path
- [ ] Wire into CEF as custom `ResourceRequestHandler`
- [ ] HTTPS-only mode (redirect HTTP → HTTPS, warn if no redirect available)
- [ ] Request/response logging for audit trail (no body content — headers only)

### 🏗 kitsune:// Custom Pages
These are served from a custom scheme handler, rendered by CEF as real HTML.
Only the welcome page has Kitsune branding. Others are clean utility pages.

- [ ] `kitsune://welcome` — **Full Kitsune branding**:
  - 🦊 large logo + "KitsuneBrowser" wordmark
  - Tagline: "Privacy-first · Agent-native · Zero telemetry"
  - Live privacy stat cards (trackers blocked, referers stripped, TLS, DNT)
  - Quick-start agent cards (click to run)
  - Getting started CTA
  - Beautiful dark design, amber accents

- [ ] `kitsune://privacy` — Privacy report:
  - Per-session stats table
  - Per-domain tracker breakdown
  - No Kitsune logo — just clean data presentation

- [ ] `kitsune://vault` — Vault manager:
  - Entry list + add/delete
  - Audit log
  - No Kitsune logo — utility UI

- [ ] `kitsune://agent` — Agent builder:
  - Create custom agent specs
  - Test run interface
  - Constraint editor

- [ ] `kitsune://error?url=X&reason=Y` — Custom error page:
  - 🦊 fox icon (the only branding)
  - Error description
  - Try Again button
  - Replaces any CEF/Chrome default error page

### 📦 Demo Infrastructure
- [x] `kitsune-cloud-mock` — axum server on `127.0.0.1:7700`
- [x] Routes: `/`, `/shop`, `/privacy`, `/checkout`, `/api/ai-action`
- [x] Fake tracker endpoints for privacy demo
- [ ] Welcome page HTML (`/`) — modern, professional, investor-ready
- [ ] Shop page HTML (`/shop`) — 8-product grid, checkout form
- [ ] Privacy report page HTML (`/privacy`) — data table, tracker list
- [ ] Auto-start mock server before browser window opens
- [ ] Demo agent script — runs automatically, shows full flow
- [ ] `DEMO_SCRIPT.md` — step-by-step investor walkthrough

---

## Phase 1 — CEF Integration (Week 1)
**Goal: Real pages render. Any URL works. Edge branding gone.**

### 1.1 — Create `crates/kitsune-cef/`

This is a thin Rust wrapper over the CEF C API. You do not need a full binding — only the
subset Kitsune uses.

```
crates/kitsune-cef/
  src/
    lib.rs          — public API: CefBrowser, CefApp, init(), shutdown()
    app.rs          — CefApp impl: OnBeforeCommandLineProcessing, GetBrowserProcessHandler
    client.rs       — CefClient impl: GetLifeSpanHandler, GetLoadHandler, GetRequestHandler
    request.rs      — ResourceRequestHandler: on_before_resource_load, on_resource_response
    scheme.rs       — CefSchemeHandlerFactory: handles kitsune:// URLs
    js.rs           — execute_javascript(), evaluate_and_return()
    error.rs        — CefError type
  build.rs          — links to libcef.dll / libcef.so, sets rpath
  Cargo.toml
```

**CEF download:** https://cef-builds.spotifycdn.com/index.html
Use the minimal build for the target platform (Windows 64-bit).
Place `libcef.dll` in `target/release/` — CEF requires it alongside the binary.

**Minimal API surface to implement:**

```rust
// lib.rs — everything kitsune-ui needs from CEF
pub struct CefBrowser { /* opaque */ }

impl CefBrowser {
    /// Create a new browser, child of parent_hwnd, filling bounds rect.
    pub fn new(parent_hwnd: isize, url: &str, bounds: CefRect) -> Result<Self, CefError>;

    /// Navigate to URL.
    pub fn load_url(&self, url: &str);

    /// Go back in history.
    pub fn go_back(&self);

    /// Go forward in history.
    pub fn go_forward(&self);

    /// Reload current page.
    pub fn reload(&self);

    /// Stop loading.
    pub fn stop_load(&self);

    /// Execute JavaScript in the main frame. Fire and forget.
    pub fn execute_js(&self, script: &str);

    /// Resize/reposition to match egui central panel.
    pub fn set_bounds(&self, rect: CefRect);

    /// Inject ResourceRequestHandler (privacy middleware).
    pub fn set_request_handler(&self, handler: Box<dyn RequestHandler + Send + Sync>);
}

/// Called by CEF for every outbound request.
pub trait RequestHandler: Send + Sync {
    fn on_before_request(&self, url: &str, method: &str, headers: &mut Headers) -> RequestAction;
}

pub enum RequestAction {
    Allow,
    Block,
    Redirect(String),
}
```

### 1.2 — Wire CEF into egui central panel

In `kitsune-ui/src/app.rs`:

```rust
struct KitsuneBrowser {
    engine: KitsuneEngine,
    cef: Option<CefBrowser>,      // None until first frame (need HWND)
    address_bar: String,
    // ... rest of state
}

impl eframe::App for KitsuneBrowser {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        self.render_top_bar(ctx);
        self.render_agent_panel(ctx);
        self.render_session_panel(ctx);

        // Measure central panel rect
        let central = egui::CentralPanel::default()
            .show(ctx, |_| {})
            .response.rect;

        // Initialize CEF on first frame (HWND available now)
        if self.cef.is_none() {
            let hwnd = frame.info().window_info.native_window_handle;
            let browser = CefBrowser::new(hwnd, "kitsune://welcome", central.into())
                .expect("CEF init failed");
            browser.set_request_handler(Box::new(KitsuneRequestHandler::new(
                self.engine.privacy_config().clone(),
                self.privacy_tx.clone(),
            )));
            self.cef = Some(browser);
        }

        // Reposition CEF every frame to track panel size
        if let Some(cef) = &self.cef {
            cef.set_bounds(central.into());
        }

        // Process any pending messages from CEF callbacks
        self.process_cef_messages(ctx);
    }
}
```

### 1.3 — Wire kitsune-net into CEF RequestHandler

```rust
struct KitsuneRequestHandler {
    privacy: PrivacyConfig,
    stats_tx: mpsc::Sender<PrivacyEvent>,
}

impl RequestHandler for KitsuneRequestHandler {
    fn on_before_request(
        &self,
        url: &str,
        _method: &str,
        headers: &mut Headers,
    ) -> RequestAction {
        // Strip Referer
        headers.remove("Referer");

        // Inject privacy headers
        headers.set("DNT", "1");
        headers.set("Sec-GPC", "1");

        // Block known trackers
        if is_tracker(url) {
            let _ = self.stats_tx.try_send(PrivacyEvent::TrackerBlocked(url.to_string()));
            return RequestAction::Block;
        }

        RequestAction::Allow
    }
}
```

### 1.4 — Custom scheme handler for kitsune:// pages

```rust
// scheme.rs
struct KitsuneSchemeHandler;

impl CefSchemeHandlerFactory for KitsuneSchemeHandler {
    fn create(&self, url: &str) -> Option<Box<dyn ResourceHandler>> {
        let path = url.trim_start_matches("kitsune://");
        let (html, mime) = match path {
            "welcome"  | "" => (WELCOME_HTML,  "text/html"),
            "privacy"       => (PRIVACY_HTML,  "text/html"),
            "vault"         => (VAULT_HTML,    "text/html"),
            "agent"         => (AGENT_HTML,    "text/html"),
            _               => return None,
        };
        Some(Box::new(StaticResourceHandler::new(html, mime)))
    }
}
// Register in CefApp::OnRegisterCustomSchemes:
// CefSchemeRegistrar::add_custom_scheme("kitsune", ...)
```

### Phase 1 Verification
```bash
cargo build --workspace --release
# Browser opens → kitsune://welcome loads (your HTML)
# Type https://wikipedia.org → page loads, renders correctly
# Type https://github.com → loads correctly
# Privacy pill shows tracker count updating
# No Edge branding visible anywhere
```

---

## Phase 2 — Browser Chrome (Week 1–2)
**Goal: All UI elements from the prototype working in egui.**

All visual code lives in `crates/kitsune-ui/src/`. Match the prototype in `kitsune-mvp.html`
exactly — same layout, same colors, same spacing. Reference that file for every visual decision.

### 2.1 — Theme

```rust
// theme.rs — single source of truth for all colors
pub struct KitsuneTheme;
impl KitsuneTheme {
    pub const BG:         Color32 = Color32::from_rgb(8,   8,  13);
    pub const BG1:        Color32 = Color32::from_rgb(15,  15, 24);
    pub const BG2:        Color32 = Color32::from_rgb(20,  20, 31);
    pub const BG3:        Color32 = Color32::from_rgb(28,  28, 44);
    pub const BG4:        Color32 = Color32::from_rgb(34,  34, 53);
    pub const AMBER:      Color32 = Color32::from_rgb(255, 122,  0);
    pub const AMBER2:     Color32 = Color32::from_rgb(255, 179, 71);
    pub const GREEN:      Color32 = Color32::from_rgb(57,  232, 143);
    pub const RED:        Color32 = Color32::from_rgb(255,  77, 106);
    pub const BLUE:       Color32 = Color32::from_rgb(74,  158, 255);
    pub const TEXT0:      Color32 = Color32::from_rgb(242, 242, 250);
    pub const TEXT1:      Color32 = Color32::from_rgb(184, 184, 208);
    pub const TEXT2:      Color32 = Color32::from_rgb(106, 106, 138);
    pub const BORDER:     Color32 = Color32::from_rgba_premultiplied(255,255,255,15);
}
```

### 2.2 — Top Bar
File: `src/chrome/top_bar.rs`

Elements (left to right):
- Fox logo + "Kitsune" wordmark (Syne font weight 800 if custom fonts loaded, else strong)
- Separator
- Tab bar (see 2.3)
- Separator
- Back ◀ button (greyed, disabled when no back history)
- Forward ▶ button (greyed, disabled when no forward history)
- Reload ↻ / Stop ✕ button (toggles during page load)
- Address bar (fills remaining width)
- Privacy pill (🛡 N blocked, updates live)

### 2.3 — Tab Bar
File: `src/chrome/tab_bar.rs`

- Each tab: `[favicon] [title (truncated)] [×]`
- Active tab: amber bottom border, slightly lighter background
- Inactive tab: hover = background lightens
- New tab button: `+` at right end
- Tabs are scrollable horizontally if overflow
- Closing active tab → activates adjacent tab
- Middle-click closes tab

### 2.4 — Left Panel
File: `src/panels/agent_panel.rs`

All elements listed in the feature list above. Key implementation notes:
- Agent cards: `egui::Frame` with border, hover state, active state (amber border)
- Log scroll: `egui::ScrollArea::vertical().stick_to_bottom(true)`
- Budget bar: `egui::ProgressBar::new(fraction).fill(AMBER).animate(true)`
- Status dot: draw a filled circle with `ui.painter().circle_filled(pos, 4.0, color)`

### 2.5 — Right Panel
File: `src/panels/session_panel.rs`

- Status rows: `ui.horizontal` with label + right-aligned value
- Capability badges: custom `Frame` with colored border per state
- Vault mini: bottom-pinned section using `ui.with_layout(Layout::bottom_up(Align::LEFT), ...)`

### 2.6 — HIL Dialog
File: `src/dialogs/hil_dialog.rs`

- Rendered as an `egui::Window` with `collapsible(false)`, `resizable(false)`, centered
- Backdrop: draw semi-transparent rect over full screen before painting the window
- Amber pulsing badge: animate opacity via `ctx.request_repaint()` + time-based sine
- Countdown: updated each frame from `Instant::elapsed()`
- Timer bar: `egui::ProgressBar` counting down
- Always re-request repaint while HIL is active (so timer updates)

### Phase 2 Verification
```bash
# All panels visible with correct layout
# Tabs: open new, close, switch
# Address bar: type URL, Enter → navigates
# Back/Forward: works after navigation
# Reload: works
# HIL dialog: trigger from code, countdown animates, confirm/cancel work
# Privacy pill: updates when trackers blocked
```

---

## Phase 3 — Agent Demo Loop (Week 2)
**Goal: End-to-end agent run. Natural language → DOM action → HIL → done.**

### 3.1 — Wire AI command parsing

```rust
// In agent_panel.rs, on Run button click:
async fn handle_run_command(cmd: String, ai: Arc<AiRouter>, agent_tx: Sender<AgentCmd>) {
    let prompt = format!(
        "Convert this browser command to a JSON action plan: \"{cmd}\"
         Respond ONLY with JSON: {{
           \"agent\": \"PriceTracker|FormFillAgent|ResearchAgent\",
           \"navigate\": \"url or null\",
           \"goal\": \"specific goal string\",
           \"actions\": [\"action1\", \"action2\"]
         }}"
    );
    
    match ai.complete(&prompt).await {
        Ok(plan_json) => {
            let plan: ActionPlan = serde_json::from_str(&plan_json)?;
            agent_tx.send(AgentCmd::RunPlan(plan)).await?;
        }
        Err(e) => log_error(format!("AI parse failed: {e}"))
    }
}
```

### 3.2 — Wire DomAccessor to CEF JavaScript

Replace every fake DomAccessor operation with a real CEF JS call:

```rust
// dom_access.rs

pub async fn fill_field(&self, field_id: &str) -> Result<(), AgentError> {
    // 1. Get vault token (UNCHANGED — vault API untouched)
    let token = self.vault.request_access(field_id, &self.context).await?;

    // 2. Send highlight command to UI (Reading phase)
    self.ui_tx.send(UiCmd::HighlightElement {
        selector: format!("#{field_id}"),
        phase: HighlightPhase::Reading,
    }).await?;
    tokio::time::sleep(Duration::from_millis(400)).await;

    // 3. Inject value via CEF JavaScript (Acting phase)
    self.ui_tx.send(UiCmd::HighlightElement {
        selector: format!("#{field_id}"),
        phase: HighlightPhase::Acting,
    }).await?;

    let script = format!(
        r#"(function(){{
            var el = document.getElementById('{field_id}')
                  || document.querySelector('[name="{field_id}"]');
            if (el) {{
                el.value = '{display}';
                el.dispatchEvent(new Event('input', {{bubbles:true}}));
                el.dispatchEvent(new Event('change', {{bubbles:true}}));
            }}
        }})();"#,
        field_id = field_id,
        display = token.display_value(), // opaque token, never raw
    );
    self.cef_tx.send(CefCmd::ExecuteJs(script)).await?;
    tokio::time::sleep(Duration::from_millis(600)).await;

    // 4. Done phase
    self.ui_tx.send(UiCmd::HighlightElement {
        selector: format!("#{field_id}"),
        phase: HighlightPhase::Done,
    }).await?;

    Ok(())
}
```

### 3.3 — DOM Highlight Overlay

The highlight can't literally be painted over CEF (separate process). Two options:

**Option A (simpler):** Inject CSS into the page via CEF JS.
```javascript
// Injected by execute_js():
(function() {
  var el = document.querySelector(SELECTOR);
  if (!el) return;
  el.style.outline = '2px solid #FFD700';
  el.style.boxShadow = '0 0 8px rgba(255,215,0,0.5)';
  el.style.transition = 'outline 0.3s, box-shadow 0.3s';
})();
```
Phase changes just update the colors via another JS call. This is the correct approach —
the highlight is inside the page, where the elements actually are.

**Option B (if CSS injection blocked):** Draw colored rects on a transparent egui overlay
using coordinates returned from `element.getBoundingClientRect()` via JS.

Use Option A.

### 3.4 — Complete Demo Flow

Implement this exact sequence, triggerable from the command input:

```
User types: "book cheapest flight to Berlin next week" → Run

[00:00] ▸ agent run "book cheapest flight to Berlin next week"
[00:00] ↳ Parsing command via AI router…
[00:01] ↳ Plan: PriceTracker → navigate skyscanner → compare fares → book cheapest
[00:02] ↳ PriceTracker: opening skyscanner.com
[00:02] 🚫 Blocked: doubleclick.net
[00:02] 🚫 Blocked: google-analytics.com
[00:03] ✂ Referer stripped → skyscanner.com
[00:04] ↳ PriceTracker: parsing 47 results…
[00:06] ✓ Cheapest: Tue 12 Mar — €194 — easyJet EZY 1234
[00:06] ↳ Highlighting best result [yellow glow on flight card]
[00:07] ↳ FormFillAgent: reading vault for passenger credentials…
[00:07]   ↳ vault returns opaque token — raw credentials never exposed
[00:08] ↳ FormFillAgent: filling name field [blue glow]
[00:09] ↳ FormFillAgent: filling email field [blue glow]
[00:09] ✓ Form filled [green glow fades]
[00:10] ⚑ HIL GATE — agent wants to submit checkout
         action: book_flight · total: €194 · seat: 14C
         [HIL dialog appears, countdown starts]

→ User clicks Confirm

[00:15] ✓ User approved — submitting checkout
[00:16] ✓ POST /checkout → 200 OK · order #KITSUNE-001
[00:16] ✓ Audit entry written to vault
[00:16] ✓ Session cleared — no credentials retained
[toast] ✅ Flight booked · €194 · easyJet EZY 1234
[privacy bar slides up] 3 trackers blocked · 2 referers stripped
```

### Phase 3 Verification
```bash
cargo run --release --bin kitsune
# Type command → AI parses → agent starts → log updates in real-time
# CEF navigates to correct page
# Tracker blocking fires, pill updates
# Agent highlights form fields (CSS injection visible in CEF)
# HIL dialog appears with correct flight details
# Confirm → checkout POST → success toast
# Privacy report bar appears
```

---

## Phase 4 — Polish + All kitsune:// Pages (Week 3)
**Goal: Every screen looks finished. No rough edges.**

### 4.1 — kitsune://welcome

Full Kitsune branding. This is the only page with the logo.

```html
<!-- welcome/index.html — served from kitsune-cef scheme handler -->
<!DOCTYPE html>
<html>
<head>
  <meta charset="UTF-8">
  <link href="https://fonts.googleapis.com/css2?family=Syne:wght@700;800&family=Inter:wght@300;400;500&display=swap" rel="stylesheet">
  <style>
    /* Full dark design, amber accents, stat cards, quick-start agents */
    /* Identical aesthetic to kitsune-mvp.html prototype */
  </style>
</head>
<body>
  <main>
    <div class="hero">
      <span class="fox">🦊</span>
      <h1>KitsuneBrowser</h1>
      <p>Privacy-first · Agent-native · Zero telemetry</p>
    </div>
    
    <div class="stats" id="stats">
      <!-- Populated via postMessage from Rust when CEF reports privacy stats -->
      <div class="stat"><span id="blocked">0</span><label>Trackers Blocked</label></div>
      <div class="stat"><span id="stripped">0</span><label>Referers Stripped</label></div>
      <div class="stat"><span>TLS 1.3</span><label>Connection</label></div>
      <div class="stat"><span>Active</span><label>DNT + GPC</label></div>
    </div>

    <div class="agents">
      <h2>Quick Start</h2>
      <!-- Agent cards — clicking posts message to Rust to launch agent -->
      <div class="agent-card" onclick="runAgent('PriceTracker')">
        <span>✈</span>
        <div><strong>PriceTracker</strong><p>Find the cheapest flight, hotel, or product</p></div>
      </div>
      <!-- ... other agents ... -->
    </div>
  </main>

  <script>
    // Receive stats from Rust via CEF message routing
    window.addEventListener('message', e => {
      if (e.data.type === 'privacy_stats') {
        document.getElementById('blocked').textContent = e.data.blocked;
        document.getElementById('stripped').textContent = e.data.stripped;
      }
    });
    function runAgent(name) {
      // CEF IPC bridge: send message to Rust
      window.__kitsune_ipc(JSON.stringify({ type: 'run_agent', agent: name }));
    }
  </script>
</body>
</html>
```

### 4.2 — kitsune://privacy

Clean data table, no branding. Investors navigate here after the demo to see what was blocked.

### 4.3 — kitsune://vault

Entry list, add form, audit log, lock button. Functional, not decorative.

### 4.4 — kitsune://error

Fox emoji + error title + reason + Try Again. Replaces Chrome's default error page entirely.
Register in CEF as the handler for `net::ERR_CONNECTION_REFUSED`, `ERR_NAME_NOT_RESOLVED`, etc.

### 4.5 — Animation Polish

- Agent status dot: sine-wave alpha animation via `ctx.request_repaint()` + `ui.input(|i| i.time)`
- HIL countdown: smooth progress bar, color transitions amber→red as time runs out
- Log entries: slide-in animation (translate Y from +8 to 0, opacity 0 to 1 over 150ms)
- Privacy pill: pulse once when count increments, then settle
- Tab loading: spinner in favicon area during page load
- Toast: slide up from bottom, auto-dismiss after 4s

### 4.6 — Error States

Every possible failure has a UI treatment:
- AI quota exhausted → amber banner in agent panel: "Monthly limit reached · Upgrade to Pro"
- Vault locked → red badge in session panel, vault operations show lock icon
- HIL timeout → log entry + dialog auto-closes + agent status = `error`
- Page load failure → `kitsune://error` with actual error message
- Agent fails → status = `error` (red badge), log shows exception, retry button appears
- Network offline → top bar shows "No connection" pill replacing privacy pill

---

## Phase 5 — Investor Demo Package (Week 3–4)
**Goal: Repeatable, impressive, zero-failure demo.**

### 5.1 — Demo Script Mode

Add `--demo` flag to the binary. When active:
- Auto-starts mock server (`127.0.0.1:7700`)
- Navigates to `http://127.0.0.1:7700/` on launch
- Pre-populates command input with the demo command
- Slightly slower agent animation (easier to follow in a presentation)

```bash
cargo run --release --bin kitsune -- --demo
```

### 5.2 — DEMO_SCRIPT.md

Step-by-step narrative for whoever is presenting. Each step has:
- What to say
- What to click
- What the audience sees
- Why it matters (the security/privacy point)

### 5.3 — Build + Package

```bash
# Windows MSI
cargo build --workspace --release
cargo wix

# Verify
cargo nextest run --workspace -j1   # 0 failures
cargo clippy --workspace -- -D warnings  # 0 warnings
```

### 5.4 — Smoke Test Checklist

Before every demo:
- [ ] `kitsune://welcome` loads and shows live stats
- [ ] Navigate to `https://wikipedia.org` → renders correctly
- [ ] Navigate to `http://127.0.0.1:7700/shop` → 8-product grid renders
- [ ] Type demo command → agent runs → log populates
- [ ] Tracker blocking fires → pill updates
- [ ] Agent highlights shop page fields
- [ ] HIL dialog appears → countdown animates
- [ ] Confirm → checkout POST succeeds → toast appears
- [ ] Navigate to `kitsune://privacy` → report shows blocked items
- [ ] Back button returns to shop
- [ ] Vault badge shows entries
- [ ] Close and reopen → state restored from disk

---

## Phase 6 — Post-Demo Hardening (Month 2)

After the investor meeting. These make it a real product.

- [ ] Actual password manager UX (autofill on login forms)
- [ ] Extension system design doc (CEF supports Chrome extensions via `--load-extension`)
- [ ] Mobile companion app design (agent tasks dispatched from phone)
- [ ] Telemetry-free update mechanism (background check, user-approved install)
- [ ] Windows code signing (EV certificate for SmartScreen)
- [ ] Linux build (CEF is cross-platform, egui is cross-platform)
- [ ] macOS build (CEF uses WebKit on macOS, or force Chromium)
- [ ] Performance profiling (CEF startup, memory footprint)
- [ ] Accessibility (egui has basic a11y, CEF inherits Chromium's)

---

## Phase 7 — The Real Vision: Servo Migration (2027+)

**Why Servo matters:**
- Servo is written in Rust. Kitsune is written in Rust. The integration is seamless
- Privacy middleware moves from "intercepting CEF requests" to "inside the HTTP stack" of
  the engine itself — architecturally unbypassable
- The security story becomes: *"Our privacy layer and our rendering engine are the same codebase"*
- No dependency on Google's Chromium release cycle
- No CEF licensing conversations
- Servo is Apache 2.0

**Migration strategy:**
```
Phase 7.1 — Track Servo web platform tests
  Monitor https://wpt.fyi/results/?product=servo
  Migration begins when WPT pass rate > 75%

Phase 7.2 — Dual-render mode
  kitsune:// pages → Servo (you control these entirely)
  External pages  → CEF (fallback, everything still works)
  Add feature flag: --experimental-servo

Phase 7.3 — Servo for trusted sites
  User whitelist: "use Servo for these domains"
  Privacy story: "on whitelisted sites, we control every pixel of the renderer"

Phase 7.4 — Full migration
  CEF becomes the fallback for Servo-incompatible pages
  Default renderer = Servo
  CEF retained for compatibility

Phase 7.5 — CEF removal
  Servo handles all pages
  CEF dependency gone
  Pure Rust stack, end to end
```

**Timeline:** Servo is on track for broad web compatibility by 2026–2027. File a watch on
`servo/servo` on GitHub. Track the embedding API — it is stabilising:
```rust
// Future: this is how Servo embedding will look
use servo::Servo;

let servo = Servo::new(ServoCfg {
    viewport: Size2D::new(1280, 720),
    resource_dir: PathBuf::from("./resources"),
});
servo.load_url(Url::parse("https://example.com").unwrap());
servo.handle_events(); // in your event loop
```

---

## What Never Changes

These are the invariants that make Kitsune worth building. No phase touches them.

| Invariant | Crate | Why |
|-----------|-------|-----|
| `HilApproval` is non-Clone | `kitsune-hil` | Approvals consumed exactly once |
| Vault returns tokens, never secrets | `kitsune-vault` | Raw credentials never in memory |
| AI router never routes vault/PII to cloud | `kitsune-ai` | Privacy invariant |
| Agent constraints are type-enforced | `kitsune-agent` | Not advisory strings |
| HIL fires before every consequential action | `kitsune-hil` | Cannot be bypassed by agent |
| Vault refuses to init if key storage fails | `kitsune-vault` | No silent fallback |

---

## File Checklist — Everything That Gets Written

```
New files (Phase 1):
  crates/kitsune-cef/src/lib.rs
  crates/kitsune-cef/src/app.rs
  crates/kitsune-cef/src/client.rs
  crates/kitsune-cef/src/request.rs
  crates/kitsune-cef/src/scheme.rs
  crates/kitsune-cef/src/js.rs
  crates/kitsune-cef/src/error.rs
  crates/kitsune-cef/build.rs
  crates/kitsune-cef/Cargo.toml

New files (Phase 2):
  crates/kitsune-ui/src/chrome/top_bar.rs
  crates/kitsune-ui/src/chrome/tab_bar.rs
  crates/kitsune-ui/src/panels/agent_panel.rs
  crates/kitsune-ui/src/panels/session_panel.rs
  crates/kitsune-ui/src/dialogs/hil_dialog.rs
  crates/kitsune-ui/src/theme.rs              (rewrite existing)
  crates/kitsune-ui/src/app.rs                (rewrite existing)

New files (Phase 4):
  crates/kitsune-cef/assets/welcome/index.html
  crates/kitsune-cef/assets/privacy/index.html
  crates/kitsune-cef/assets/vault/index.html
  crates/kitsune-cef/assets/error/index.html

Modified files:
  crates/kitsune-agent/src/dom_access.rs      (replace internals with CEF JS)
  crates/kitsune-core/src/engine.rs           (remove pipeline refs, add cef start)
  Cargo.toml                                  (add kitsune-cef, remove wry)
  DEMO_SCRIPT.md                              (write)
  PROGRESS.md                                 (update all tasks)
```

---

*KitsuneEngine MVP Roadmap — April 2026*
*Phases 1–5: ~3–4 weeks focused execution*
*Vision: Full Rust browser stack via Servo — 2027*
*Security invariants compromised: 0*
