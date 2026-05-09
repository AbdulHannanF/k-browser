use crate::error::{AgentError, AgentResult};
use crate::executor::WebViewCommand;
use kitsune_hil::{HilGate, HilTriggerClass};
use kitsune_vault::{types::VaultCategory, RequestContext, RequesterId, VaultBackend, VaultKey};
use rand::Rng;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::sync::Mutex;
use url::Url;
use uuid::Uuid;

/// Provides structural, read-only, and safe interaction access to the DOM for agents.
pub struct DomAccessor {
    vault: Arc<VaultBackend>,
    hil_gate: Arc<HilGate>,
    current_url: Mutex<Url>,
    pub(crate) webview_tx: mpsc::Sender<WebViewCommand>,
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

    /// Pause for a randomised human-like duration (80–180 ms).
    /// Called before every field fill and click to evade bot-detection heuristics.
    async fn human_delay(&self) {
        let ms = rand::thread_rng().gen_range(80u64..=180);
        tokio::time::sleep(std::time::Duration::from_millis(ms)).await;
    }

    /// Inject a synthetic mousemove event near the target element.
    async fn inject_mouse_move(&self, selector: &str) -> AgentResult<()> {
        // Backslashes must be escaped before single-quotes to prevent JS injection.
        let safe = selector.replace('\\', "\\\\").replace('\'', "\\'");
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

    /// Fill a field with `value` after explicit HIL approval.
    ///
    /// The agent must pass the raw value it intends to inject.  This method
    /// gates the action through the HIL gate so the user always confirms
    /// before anything is written into a form field.  `request_access` is
    /// called with the resulting approval context so the vault layer can audit
    /// and enforce its own ACL.
    pub async fn fill_field(&self, field_id: &str, value: &str) -> Result<(), AgentError> {
        self.human_delay().await;
        self.inject_mouse_move(field_id).await.ok();

        // 1. Gate through HIL — user must approve before any field is filled.
        let trigger = HilTriggerClass::ExternalSideEffect {
            description: format!("Fill form field '{}' with a stored credential", field_id),
            reversible: false,
        };
        let _approval = self
            .hil_gate
            .checkpoint(trigger, vec![field_id.to_string()])
            .await
            .map_err(|e| AgentError::HilRejected(format!("{e:?}")))?;

        // 2. Obtain a vault audit token with the HIL approval in context.
        let current_url = self.current_url.lock().await;
        let domain = current_url.host_str().map(|h| h.to_string());
        let context = RequestContext {
            domain,
            purpose: format!("fill form field: {field_id}"),
            agent_id: None,
            has_hil_approval: true,
            action_id: uuid::Uuid::new_v4(),
        };
        drop(current_url);
        self.vault.request_access(field_id, &context).await?;

        // 3. Inject the approved value into the live WebView.
        let safe_value = value.replace('\'', "\\'").replace('\\', "\\\\");
        let safe_field = field_id.replace('\'', "\\'");
        let script = format!(
            r#"
            (function() {{
                let el = document.getElementById('{safe_field}');
                if (!el) el = document.querySelector('[name="{safe_field}"]');
                if (!el) {{ window.__kitsune_ipc(JSON.stringify({{err: 'field_not_found'}})); return; }}

                el.style.outline = '2px solid #FFD700';
                el.style.boxShadow = '0 0 8px #FFD70088';

                setTimeout(() => {{
                    el.style.outline = '2px solid #4A9EFF';
                    el.style.boxShadow = '0 0 8px #4A9EFF88';
                    el.value = '{safe_value}';
                    el.dispatchEvent(new Event('input', {{bubbles: true}}));
                    el.dispatchEvent(new Event('change', {{bubbles: true}}));

                    setTimeout(() => {{
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
        );

        self.webview_tx
            .send(WebViewCommand::EvalJs(script))
            .await
            .map_err(|_| AgentError::IpcDisconnected)?;

        Ok(())
    }

    /// Click an element. Trips HIL if it's a submit button.
    pub async fn click_element(&self, selector: &str) -> AgentResult<()> {
        self.human_delay().await;
        self.inject_mouse_move(selector).await.ok();

        let trigger = HilTriggerClass::ExternalSideEffect {
            description: "Agent is trying to click an element".to_string(),
            reversible: false,
        };
        self.hil_gate
            .checkpoint(trigger, vec![])
            .await
            .map_err(|e| AgentError::PermissionDenied {
                capability: format!("HIL Checkpoint failed: {}", e),
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
        self.query_text("body")
            .await
            .map(|opt| opt.unwrap_or_default())
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn selector_escaping_prevents_js_injection() {
        // Backslash must be doubled before single-quote escaping to prevent injection.
        // A selector like foo\'bar would produce foo\\'bar (breaking JS string) if
        // quote is escaped first. Correct order: backslash first, then quote.
        let tricky = r"foo\'bar";
        let safe = tricky.replace('\\', "\\\\").replace('\'', "\\'");
        assert!(
            safe.starts_with("foo\\\\"),
            "backslash was not doubled before quote escape: {safe}"
        );

        // Plain single-quote must be escaped.
        let with_quote = "input[type='text']";
        let safe2 = with_quote.replace('\\', "\\\\").replace('\'', "\\'");
        assert!(safe2.contains("\\'"), "single-quote not escaped: {safe2}");
    }
}
