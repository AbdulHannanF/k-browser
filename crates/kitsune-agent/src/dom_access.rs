use kitsune_vault::{VaultBackend, VaultKey, RequesterId, RequestContext, types::VaultCategory};
use kitsune_hil::{HilGate, HilTriggerClass};
use url::Url;
use std::sync::Arc;
use tokio::sync::Mutex;
use uuid::Uuid;
use crate::error::{AgentError, AgentResult};
use tokio::sync::mpsc;
use crate::executor::WebViewCommand;

/// Provides structural, read-only, and safe interaction access to the DOM for agents.
pub struct DomAccessor {
    vault: Arc<VaultBackend>,
    hil_gate: Arc<HilGate>,
    current_url: Mutex<Url>,
    webview_tx: mpsc::Sender<WebViewCommand>,
}

impl DomAccessor {
    pub fn new(
        vault: Arc<VaultBackend>,
        hil_gate: Arc<HilGate>,
        initial_url: Url,
        webview_tx: mpsc::Sender<WebViewCommand>,
    ) -> Self {
        Self {
            vault,
            hil_gate,
            current_url: Mutex::new(initial_url),
            webview_tx,
        }
    }

    /// Query text content by a simple selector (id, class, or tag).
    pub async fn query_text(&self, selector: &str) -> AgentResult<Option<String>> {
        let (tx, mut rx) = mpsc::channel(1);
        let script = format!(
            r#"
            (function() {{
                let el = document.querySelector('{selector}');
                if (el) {{
                    window.__kitsune_ipc(JSON.stringify({{ text: el.textContent }}));
                }} else {{
                    window.__kitsune_ipc(JSON.stringify({{ text: null }}));
                }}
            }})();
            "#,
            selector = selector
        );
        self.webview_tx
            .send(WebViewCommand::EvalJsWithCallback(script, tx))
            .await
            .map_err(|_| AgentError::IpcDisconnected)?;

        if let Some(response) = rx.recv().await {
            // Assuming response is a JSON string with a "text" field
            let json: serde_json::Value = serde_json::from_str(&response).unwrap_or_default();
            if let Some(text) = json["text"].as_str() {
                return Ok(Some(text.to_string()));
            }
        }
        Ok(None)
    }

    /// Query all href links matching a selector, filtering out JS and Data schemes.
    pub async fn query_links(&self, selector: &str) -> AgentResult<Vec<String>> {
        let (tx, mut rx) = mpsc::channel(1);
        let script = format!(
            r#"
            (function() {{
                let links = [...document.querySelectorAll('{selector}')]
                    .map(a => a.href)
                    .filter(href => !href.toLowerCase().startsWith('javascript:') && !href.toLowerCase().startsWith('data:'));
                window.__kitsune_ipc(JSON.stringify({{ links }}));
            }})();
            "#,
            selector = selector
        );
        self.webview_tx
            .send(WebViewCommand::EvalJsWithCallback(script, tx))
            .await
            .map_err(|_| AgentError::IpcDisconnected)?;

        if let Some(response) = rx.recv().await {
            let json: serde_json::Value = serde_json::from_str(&response).unwrap_or_default();
            if let Some(links_val) = json["links"].as_array() {
                let links = links_val
                    .iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect();
                return Ok(links);
            }
        }
        Ok(vec![])
    }

    /// Fill a field using a Vault injection token.
    pub async fn fill_field(&self, field_id: &str, value: &str) -> Result<(), AgentError> {
        // 1. Ask vault for a token (unchanged — vault API untouched)
        let token = self.vault.request_access(field_id).await?;

        // 2. Inject the value via JavaScript into the live WebView
        let script = format!(
            r#"
            (function() {{
                // Highlight field yellow (Reading phase)
                let el = document.getElementById('{field_id}');
                if (!el) el = document.querySelector('[name="{field_id}"]');
                if (!el) {{ window.__kitsune_ipc(JSON.stringify({{err: 'field_not_found'}})); return; }}

                el.style.outline = '2px solid #FFD700';
                el.style.boxShadow = '0 0 8px #FFD70088';

                setTimeout(() => {{
                    // Acting phase — blue
                    el.style.outline = '2px solid #4A9EFF';
                    el.style.boxShadow = '0 0 8px #4A9EFF88';
                    el.value = '{value}';
                    el.dispatchEvent(new Event('input', {{bubbles: true}}));
                    el.dispatchEvent(new Event('change', {{bubbles: true}}));

                    setTimeout(() => {{
                        // Done phase — green
                        el.style.outline = '2px solid #4ADE80';
                        el.style.boxShadow = '0 0 8px #4ADE8088';
                        setTimeout(() => {{
                            el.style.outline = '';
                            el.style.boxShadow = '';
                        }}, 1500);
                    }}, 600);
                }}, 400);
            }})();
            "#,
            field_id = field_id,
            value = token.display_value(), // opaque token, not raw secret
        );

        self.webview_tx.send(WebViewCommand::EvalJs(script)).await
            .map_err(|_| AgentError::IpcDisconnected)?;

        Ok(())
    }


    /// Click an element. Trips HIL if it's a submit button.
    pub async fn click_element(&self, selector: &str) -> AgentResult<()> {
        let trigger = HilTriggerClass::ExternalSideEffect {
            description: "Agent is trying to click an element".to_string(),
            reversible: false,
        };
        self.hil_gate.checkpoint(trigger, vec![]).await.map_err(|e| {
            AgentError::PermissionDenied {
                capability: format!("HIL Checkpoint failed: {}", e),
            }
        })?;

        let script = format!(
            r#"
            (function() {{
                let el = document.querySelector('{selector}');
                if (el) {{
                    el.click();
                }}
            }})();
            "#,
            selector = selector
        );
        self.webview_tx
            .send(WebViewCommand::EvalJs(script))
            .await
            .map_err(|_| AgentError::IpcDisconnected)?;
        Ok(())
    }

    /// Navigate to a URL.
    pub async fn navigate(&self, url_str: &str) -> AgentResult<()> {
        let url = Url::parse(url_str).map_err(|_| AgentError::InvalidParameters {
            param: "url".to_string(),
            reason: "Invalid URL format".to_string(),
        })?;
        self.webview_tx
            .send(WebViewCommand::Navigate(url.to_string()))
            .await
            .map_err(|_| AgentError::IpcDisconnected)?;
        *self.current_url.lock().await = url;
        Ok(())
    }

    pub async fn get_page_title(&self) -> AgentResult<Option<String>> {
        self.query_text("title").await
    }

    pub async fn get_current_url(&self) -> AgentResult<String> {
        Ok(self.current_url.lock().await.to_string())
    }

    pub async fn get_page_text(&self) -> AgentResult<String> {
        self.query_text("body").await.map(|opt| opt.unwrap_or_default())
    }
}
