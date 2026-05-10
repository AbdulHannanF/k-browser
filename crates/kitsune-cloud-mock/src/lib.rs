/// KitsuneEngine local mock HTTP server.
///
/// Serves demo HTML pages and fake tracker endpoints so the investor demo
/// works fully offline. Start via [`start`].
///
/// Also provides real agent execution endpoints:
/// - `POST /api/settings` — configure API key and model
/// - `GET  /api/settings` — check if API key is configured
/// - `POST /api/agent-run` — execute an agent task (SSE stream)
/// - `POST /api/hil-response` — approve or reject a HIL gate action
pub mod agent_brain;
mod pages;

use std::sync::Arc;

use axum::{
    extract::{Json, State},
    http::{header, StatusCode},
    response::{
        sse::{Event, Sse},
        Html, IntoResponse, Response,
    },
    routing::{get, post},
    Router,
};

use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use tower_http::cors::CorsLayer;
use tracing::info;

use agent_brain::{AgentAction, AgentBrain, AiProvider, AiSettings};

// ---------------------------------------------------------------------------
// Application state
// ---------------------------------------------------------------------------

/// Shared application state held behind Arc<Mutex<>>.
pub struct AppState {
    /// AI settings (API key, endpoint, model).
    pub settings: AiSettings,
    /// Pending HIL response channel — the agent-run SSE waits on this.
    pub hil_tx: Option<tokio::sync::oneshot::Sender<bool>>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            settings: AiSettings::default(),
            hil_tx: None,
        }
    }
}

type SharedState = Arc<Mutex<AppState>>;

// ---------------------------------------------------------------------------
// Response helpers
// ---------------------------------------------------------------------------

struct HtmlPage(Html<&'static str>);

impl IntoResponse for HtmlPage {
    fn into_response(self) -> Response {
        ([(header::CONTENT_TYPE, "text/html; charset=utf-8")], self.0).into_response()
    }
}

// ---------------------------------------------------------------------------
// Page handlers
// ---------------------------------------------------------------------------

/// GET / — welcome / home page.
async fn serve_welcome() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
        include_str!("../static/index.html")
    )
}

/// GET /health — health check for Render.
async fn serve_health() -> impl IntoResponse {
    "OK"
}

/// GET /shop — demo e-commerce store.
async fn serve_shop() -> HtmlPage {
    HtmlPage(Html(pages::SHOP_HTML))
}

/// GET /privacy — privacy report.
async fn serve_privacy() -> HtmlPage {
    HtmlPage(Html(pages::PRIVACY_HTML))
}

/// GET /favicon.ico — minimal response so browsers don't log 404s.
async fn serve_favicon() -> impl IntoResponse {
    (StatusCode::NO_CONTENT, "")
}

// ---------------------------------------------------------------------------
// Fake tracker — intentionally trackable, demonstrating blocking
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct TrackerResponse {
    tracked: bool,
    id: &'static str,
}

async fn fake_tracker() -> Json<TrackerResponse> {
    // KitsuneEngine's privacy middleware should block requests to these
    // endpoints before they even leave the browser process. If a tracker
    // endpoint *does* respond, it means the request made it through the
    // privacy filter — which is intentionally shown in the demo as "not blocked."
    Json(TrackerResponse {
        tracked: true,
        id: "demo-tracker",
    })
}

// ---------------------------------------------------------------------------
// Checkout (legacy endpoint kept for compatibility)
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct CheckoutBody {
    name: Option<String>,
    email: Option<String>,
}

#[derive(Serialize)]
struct CheckoutResponse {
    success: bool,
    order_id: &'static str,
}

async fn handle_checkout(body: Option<Json<CheckoutBody>>) -> Json<CheckoutResponse> {
    let name = body
        .as_ref()
        .and_then(|b| b.name.as_deref())
        .unwrap_or("Anonymous");
    info!(customer = name, "Checkout completed");
    Json(CheckoutResponse {
        success: true,
        order_id: "DEMO-001",
    })
}

// ---------------------------------------------------------------------------
// AI action (legacy endpoint kept for compatibility)
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct AiActionResponse {
    action: &'static str,
    fields: serde_json::Value,
}

async fn handle_ai_action() -> Json<AiActionResponse> {
    Json(AiActionResponse {
        action: "fill_form",
        fields: serde_json::json!({
            "name": "Demo User",
            "email": "demo@kitsune.ai"
        }),
    })
}

// ---------------------------------------------------------------------------
// Settings endpoints
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct SettingsInput {
    #[serde(default)]
    provider: AiProvider,
    #[serde(default)]
    api_key: String,
    #[serde(default = "default_endpoint")]
    endpoint: String,
    #[serde(default = "default_model")]
    model: String,
}

fn default_endpoint() -> String {
    "https://api.openai.com/v1/chat/completions".to_string()
}

fn default_model() -> String {
    "gpt-4o-mini".to_string()
}

#[derive(Serialize)]
struct SettingsStatus {
    configured: bool,
    provider: AiProvider,
    endpoint: String,
    model: String,
}

/// POST /api/settings — save API key + model config.
async fn save_settings(
    State(state): State<SharedState>,
    Json(input): Json<SettingsInput>,
) -> impl IntoResponse {
    let mut state = state.lock().await;
    state.settings = AiSettings {
        provider: input.provider,
        api_key: input.api_key,
        endpoint: if input.endpoint.is_empty() {
            default_endpoint()
        } else {
            input.endpoint
        },
        model: if input.model.is_empty() {
            default_model()
        } else {
            input.model
        },
    };
    info!(
        provider = ?state.settings.provider,
        endpoint = %state.settings.endpoint,
        model = %state.settings.model,
        "AI settings saved"
    );
    (StatusCode::OK, Json(serde_json::json!({"status": "ok"})))
}

/// GET /api/settings — check current config status (never returns the key itself).
async fn get_settings(State(state): State<SharedState>) -> Json<SettingsStatus> {
    let state = state.lock().await;
    Json(SettingsStatus {
        configured: state.settings.is_configured(),
        provider: state.settings.provider,
        endpoint: state.settings.endpoint.clone(),
        model: state.settings.model.clone(),
    })
}

// ---------------------------------------------------------------------------
// Agent execution endpoint (SSE stream)
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct AgentRunInput {
    command: String,
}

/// POST /api/agent-run — execute an agent command. Returns an SSE stream.
async fn agent_run(
    State(state): State<SharedState>,
    Json(input): Json<AgentRunInput>,
) -> Response {
    let command = input.command;

    // Grab the settings
    let settings = {
        let s = state.lock().await;
        s.settings.clone()
    };

    info!(command = %command, configured = settings.is_configured(), "Agent run started");

    // Plan actions via LLM (falls back to demo actions if no API key)
    let brain = AgentBrain::new(settings);
    let actions = brain.plan_actions(&command).await;

    // Create an SSE stream from the actions
    let state_clone = state.clone();
    let stream = async_stream::stream! {
        for action in actions {
            // Handle waits as actual delays
            if let AgentAction::Wait { ms } = &action {
                tokio::time::sleep(std::time::Duration::from_millis(*ms)).await;
                continue;
            }

            // For HIL requests, we need to pause and wait for user response
            if let AgentAction::HilRequest { .. } = &action {
                let data = serde_json::to_string(&action).unwrap_or_default();
                yield Ok::<_, std::convert::Infallible>(
                    Event::default().event("action").data(data)
                );

                // Create a channel and store the sender in state
                let (tx, rx) = tokio::sync::oneshot::channel::<bool>();
                {
                    let mut s = state_clone.lock().await;
                    s.hil_tx = Some(tx);
                }

                // Wait for HIL response (with 60 second timeout)
                let approved = tokio::time::timeout(
                    std::time::Duration::from_secs(60),
                    rx
                ).await.unwrap_or(Ok(false)).unwrap_or(false);

                if approved {
                    let done_data = serde_json::json!({
                        "type": "hil_approved"
                    });
                    yield Ok(Event::default().event("action").data(done_data.to_string()));
                } else {
                    let cancel_data = serde_json::json!({
                        "type": "hil_cancelled"
                    });
                    yield Ok(Event::default().event("action").data(cancel_data.to_string()));
                    return;
                }
                continue;
            }

            let data = serde_json::to_string(&action).unwrap_or_default();
            yield Ok(Event::default().event("action").data(data));

            // Small delay between non-wait actions for natural pacing
            tokio::time::sleep(std::time::Duration::from_millis(120)).await;
        }

        // Final stream-end event
        yield Ok(Event::default().event("done").data("{}"));
    };

    Sse::new(stream)
        .keep_alive(
            axum::response::sse::KeepAlive::new()
                .interval(std::time::Duration::from_secs(15))
                .text("ping"),
        )
        .into_response()
}

// ---------------------------------------------------------------------------
// HIL response endpoint
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct HilResponseInput {
    approved: bool,
}

/// POST /api/hil-response — user approves or rejects the HIL gate.
async fn handle_hil_response(
    State(state): State<SharedState>,
    Json(input): Json<HilResponseInput>,
) -> impl IntoResponse {
    let mut s = state.lock().await;
    if let Some(tx) = s.hil_tx.take() {
        let _ = tx.send(input.approved);
        info!(approved = input.approved, "HIL response received");
        (StatusCode::OK, Json(serde_json::json!({"status": "ok"})))
    } else {
        (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "No pending HIL request"})),
        )
    }
}

// ---------------------------------------------------------------------------
// Swarm-plan demo endpoint
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct SwarmPlanInput {
    goal: String,
    #[serde(default)]
    mode: String,
    #[serde(default = "default_swarm_workers")]
    max_workers: usize,
}

fn default_swarm_workers() -> usize { 3 }

/// POST /api/swarm-plan — demo SSE stream that simulates a swarm run.
async fn swarm_plan(Json(input): Json<SwarmPlanInput>) -> Response {
    let goal = input.goal;
    let max_workers = input.max_workers.min(10).max(1);

    let stream = async_stream::stream! {
        // Coordinator planning
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        let update = serde_json::json!({
            "type": "SwarmUpdate",
            "swarm_id": "demo-swarm",
            "worker_id": "coordinator",
            "role": "Coordinator",
            "status": "Running",
            "message": "Decomposing goal into parallel tasks...",
            "tool_calls_used": 0u32
        });
        yield Ok::<_, std::convert::Infallible>(
            Event::default().event("swarm").data(update.to_string())
        );

        // Plan ready — emit N fake tasks
        tokio::time::sleep(std::time::Duration::from_millis(800)).await;
        let roles = ["Researcher", "Analyst", "Skeptic", "FactChecker", "Writer"];
        let tasks: Vec<serde_json::Value> = (0..max_workers).map(|i| {
            serde_json::json!({
                "id": format!("task-{}", i),
                "role": roles[i % roles.len()],
                "prompt": format!("Investigate aspect {} of: {}", i + 1, goal),
                "depends_on": [],
                "status": "Pending",
                "worker_id": null,
                "tool_calls_used": 0u32,
                "last_message": null
            })
        }).collect();
        let plan = serde_json::json!({
            "type": "SwarmPlanReady",
            "swarm_id": "demo-swarm",
            "goal": goal.clone(),
            "tasks": tasks.clone()
        });
        yield Ok(Event::default().event("swarm").data(plan.to_string()));

        // Workers starting
        for (i, task) in tasks.iter().enumerate() {
            tokio::time::sleep(std::time::Duration::from_millis(300)).await;
            let role = task["role"].as_str().unwrap_or("Worker");
            let running = serde_json::json!({
                "type": "SwarmUpdate",
                "swarm_id": "demo-swarm",
                "worker_id": format!("worker-{}-{}", role.to_lowercase(), i),
                "role": role,
                "status": "Running",
                "message": "Searching for information...",
                "tool_calls_used": 0u32
            });
            yield Ok(Event::default().event("swarm").data(running.to_string()));
        }

        // Workers completing
        for (i, task) in tasks.iter().enumerate() {
            tokio::time::sleep(std::time::Duration::from_millis(600)).await;
            let role = task["role"].as_str().unwrap_or("Worker");
            let done = serde_json::json!({
                "type": "SwarmUpdate",
                "swarm_id": "demo-swarm",
                "worker_id": format!("worker-{}-{}", role.to_lowercase(), i),
                "role": role,
                "status": "Completed",
                "message": format!("{} found {} relevant data points.", role, (i + 1) * 3),
                "tool_calls_used": (i as u32) + 2
            });
            yield Ok(Event::default().event("swarm").data(done.to_string()));
        }

        // Synthesis
        tokio::time::sleep(std::time::Duration::from_millis(800)).await;
        let synthesizing = serde_json::json!({
            "type": "SwarmUpdate",
            "swarm_id": "demo-swarm",
            "worker_id": "coordinator",
            "role": "Coordinator",
            "status": "Running",
            "message": "Reconciling all perspectives into final synthesis...",
            "tool_calls_used": 0u32
        });
        yield Ok(Event::default().event("swarm").data(synthesizing.to_string()));

        tokio::time::sleep(std::time::Duration::from_millis(1000)).await;
        let final_answer = format!(
            "## Swarm Analysis: {}\n\n**Agreement:** All {} agents confirmed the topic is well-documented.\n\n**Key Findings:**\n- Researcher: Found primary sources\n- Analyst: Identified trends\n- Skeptic: Challenged assumptions\n\n**Conclusion:** Comprehensive analysis complete.",
            goal, max_workers
        );
        let swarm_done = serde_json::json!({
            "type": "SwarmDone",
            "swarm_id": "demo-swarm",
            "final_answer": final_answer,
            "total_tool_calls": (max_workers * 3) as u32
        });
        yield Ok(Event::default().event("swarm").data(swarm_done.to_string()));

        yield Ok(Event::default().event("done").data("{}"));
    };

    Sse::new(stream)
        .keep_alive(
            axum::response::sse::KeepAlive::new()
                .interval(std::time::Duration::from_secs(15))
                .text("ping"),
        )
        .into_response()
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Build the Axum router for the demo server.
pub fn router() -> Router {
    let state: SharedState = Arc::new(Mutex::new(AppState::default()));

    Router::new()
        // Page routes
        .route("/", get(serve_welcome))
        .route("/health", get(serve_health))
        .route("/shop", get(serve_shop))
        .route("/privacy", get(serve_privacy))
        .route("/favicon.ico", get(serve_favicon))
        // Legacy demo endpoints
        .route("/checkout", post(handle_checkout))
        .route("/api/track", get(fake_tracker))
        .route("/api/doubleclick-tracker", get(fake_tracker))
        .route("/api/google-analytics", get(fake_tracker))
        .route("/api/ai-action", post(handle_ai_action))
        // New agent endpoints
        .route("/api/settings", get(get_settings).post(save_settings))
        .route("/api/agent-run", post(agent_run))
        .route("/api/hil-response", post(handle_hil_response))
        .route("/api/swarm-plan", post(swarm_plan))
        .with_state(state)
        .layer(CorsLayer::permissive())
}

/// Start the mock server at `addr` (e.g. `"127.0.0.1:7700"`).
///
/// This future runs forever (or until the process exits). Spawn it with
/// `tokio::spawn`.
pub async fn start(addr: &str) -> anyhow::Result<()> {
    let listener = tokio::net::TcpListener::bind(addr).await?;
    info!("KitsuneEngine demo server listening on http://{}", addr);
    axum::serve(listener, router()).await?;
    Ok(())
}
