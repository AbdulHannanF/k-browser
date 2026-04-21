/// KitsuneEngine local mock HTTP server.
///
/// Serves demo HTML pages and fake tracker endpoints so the investor demo
/// works fully offline. Start via [`start`].

mod pages;

use axum::{
    extract::Json,
    http::{header, StatusCode},
    response::{Html, IntoResponse, Response},
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use tower_http::cors::CorsLayer;
use tracing::info;

// ---------------------------------------------------------------------------
// Response helpers
// ---------------------------------------------------------------------------

struct HtmlPage(Html<&'static str>);

impl IntoResponse for HtmlPage {
    fn into_response(self) -> Response {
        (
            [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
            self.0,
        )
            .into_response()
    }
}

// ---------------------------------------------------------------------------
// Route handlers
// ---------------------------------------------------------------------------

/// GET / — welcome / home page.
async fn serve_welcome() -> HtmlPage {
    HtmlPage(Html(pages::WELCOME_HTML))
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
// Checkout
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
// AI action (agent demo endpoint)
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
// Public API
// ---------------------------------------------------------------------------

/// Build the Axum router for the demo server.
pub fn router() -> Router {
    Router::new()
        .route("/", get(serve_welcome))
        .route("/shop", get(serve_shop))
        .route("/privacy", get(serve_privacy))
        .route("/checkout", post(handle_checkout))
        .route("/api/track", get(fake_tracker))
        .route("/api/doubleclick-tracker", get(fake_tracker))
        .route("/api/google-analytics", get(fake_tracker))
        .route("/api/ai-action", post(handle_ai_action))
        .route("/favicon.ico", get(serve_favicon))
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
