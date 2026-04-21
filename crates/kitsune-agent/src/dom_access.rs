use kitsune_html::dom::{DomTree, NodeType, NodeId};
use kitsune_vault::{VaultBackend, VaultKey, RequesterId, RequestContext, types::VaultCategory};
use kitsune_hil::{HilGate, HilTriggerClass};
use url::Url;
use std::sync::Arc;
use tokio::sync::Mutex;
use uuid::Uuid;
use crate::error::{AgentError, AgentResult};
use kitsune_layout::LayoutNode;
use kitsune_ipc::message::{IpcMessage, IpcPayload, DomHighlight, HighlightRect, HighlightStyle, HighlightPhase, ProcessId};
use tokio::sync::mpsc::Sender;

/// Provides structural, read-only, and safe interaction access to the DOM for agents.
pub struct DomAccessor {
    dom: Arc<Mutex<DomTree>>,
    vault: Arc<VaultBackend>,
    hil_gate: Arc<HilGate>,
    current_url: Mutex<Url>,
    layout: Option<Arc<Mutex<LayoutNode>>>,
    ipc_tx: Option<Sender<IpcMessage>>,
}

impl DomAccessor {
    pub fn new(dom: Arc<Mutex<DomTree>>, vault: Arc<VaultBackend>, hil_gate: Arc<HilGate>, initial_url: Url, layout: Option<Arc<Mutex<LayoutNode>>>, ipc_tx: Option<Sender<IpcMessage>>) -> Self {
        Self { dom, vault, hil_gate, current_url: Mutex::new(initial_url), layout, ipc_tx }
    }

    async fn send_highlight(&self, node_id: NodeId, style: HighlightStyle) {
        if let (Some(layout_ref), Some(tx)) = (&self.layout, &self.ipc_tx) {
            let layout = layout_ref.lock().await;
            if let Some(rect) = Self::find_rect_in_layout(&layout, node_id.0) {
                let msg = IpcMessage::new(
                    ProcessId("kitsune-agent".to_string()),
                    ProcessId("kitsune-ui".to_string()),
                    IpcPayload::SetDomHighlight(DomHighlight {
                        element_id: node_id.0.to_string(),
                        rect,
                        style,
                        phase: HighlightPhase::FadingIn,
                        phase_start: None,
                    })
                );
                let _ = tx.send(msg).await;
            }
        }
    }

    fn find_rect_in_layout(node: &LayoutNode, target_id: u64) -> Option<HighlightRect> {
        if node.dom_node_id == target_id {
            return Some(HighlightRect {
                x: node.dimensions.content.x as f32,
                y: node.dimensions.content.y as f32,
                width: node.dimensions.content.width as f32,
                height: node.dimensions.content.height as f32,
            });
        }
        for child in &node.children {
            if let Some(r) = Self::find_rect_in_layout(child, target_id) {
                return Some(r);
            }
        }
        None
    }

    /// Query text content by a simple selector (id, class, or tag).
    pub async fn query_text(&self, selector: &str) -> AgentResult<Option<String>> {
        let dom = self.dom.lock().await;
        let node_id = self.find_node_by_selector(&dom, selector);
        if let Some(id) = node_id {
            
            // Drop dom lock before async IPC
            drop(dom);
            self.send_highlight(id, HighlightStyle::Reading).await;
            
            let dom2 = self.dom.lock().await;
            let text = self.extract_text(&dom2, id);
            if !text.is_empty() {
                return Ok(Some(text));
            }
        }
        Ok(None)
    }

    /// Query all href links matching a selector, filtering out JS and Data schemes.
    pub async fn query_links(&self, selector: &str) -> AgentResult<Vec<String>> {
        let dom = self.dom.lock().await;
        let node_ids = self.find_all_nodes_by_selector(&dom, selector);
        
        let mut links = Vec::new();
        // Fire highlights outside lock
        let node_ids_clone = node_ids.clone();
        drop(dom);
        for id in node_ids_clone {
            self.send_highlight(id, HighlightStyle::Reading).await;
        }
        let dom = self.dom.lock().await;
        
        for id in node_ids {
            if let Some(node) = dom.get_node(id) {
                if let NodeType::Element(ref e) = node.node_type {
                    if let Some(href) = e.attributes.get("href") {
                        // Filter JS/Data
                        let lower = href.to_lowercase();
                        if !lower.starts_with("javascript:") && !lower.starts_with("data:") {
                            links.push(href.clone());
                        }
                    }
                }
            }
        }
        Ok(links)
    }

    /// Fill a field using a Vault injection token.
    pub async fn fill_field(&self, selector: &str, vault_key: &str) -> AgentResult<()> {
        let domain = self.current_url.lock().await.domain().map(|s| s.to_string());
        let context = RequestContext {
            domain,
            purpose: "Agent form fill".to_string(),
            agent_id: None,
            has_hil_approval: false,
            action_id: Uuid::new_v4(),
        };
        let key = VaultKey::new(vault_key, VaultCategory::Password); // Use password category for testing
        let token = self.vault.retrieve(&key, &context)
            .map_err(|e| AgentError::PermissionDenied { capability: format!("vault: {:?}", e) })?;

        let mut dom = self.dom.lock().await;
        if let Some(id) = self.find_node_by_selector(&dom, selector) {
            // In a real DOM we'd set the value attribute or proper value property
            // We'll set the "value" attribute to the OpaqueToken string representation
            if let Some(node) = dom.get_node_mut(id) {
                if let NodeType::Element(ref mut e) = node.node_type {
                    e.attributes.insert("value".to_string(), token.id.to_string());
                }
            }
        }
        // If element not found, we silently ignore or we could return an error.
        Ok(())
    }

    /// Click an element. Trips HIL if it's a submit button.
    pub async fn click_element(&self, selector: &str) -> AgentResult<()> {
        let is_submit = {
            let dom = self.dom.lock().await;
            if let Some(id) = self.find_node_by_selector(&dom, selector) {
                if let Some(node) = dom.get_node(id) {
                                        if let NodeType::Element(ref e) = node.node_type {
                        let el_type = e.attributes.get("type").map(|s| s.as_str());
                        if e.tag_name == "input" {
                            el_type == Some("submit")
                        } else if e.tag_name == "button" {
                            el_type.unwrap_or("submit") == "submit"
                        } else {
                            false
                        }
                    } else { false }
                } else { false }
            } else { false }
        };

        if is_submit {
            // Trigger HIL
            let trigger = HilTriggerClass::ExternalSideEffect {
                description: "Agent submitting form".to_string(),
                reversible: false,
            };
            self.hil_gate.checkpoint(trigger, vec![]).await.map_err(|_| AgentError::PermissionDenied { capability: "hil_submit".to_string() })?;
        }

        // Send Acting highlight and visual pause
        let id_opt = {
            let dom = self.dom.lock().await;
            self.find_node_by_selector(&dom, selector)
        };
        if let Some(id) = id_opt {
            self.send_highlight(id, HighlightStyle::Acting).await;
            tokio::time::sleep(std::time::Duration::from_millis(300)).await;
            // Firing click (for now, simply logging success)
            
            // Send Done highlight after pause
            self.send_highlight(id, HighlightStyle::Done).await;
        }

        Ok(())
    }

    /// Navigate to a URL. IPC interaction with core pipeline omitted for brevity; this updates current URL.
    pub async fn navigate(&self, url_str: &str) -> AgentResult<()> {
        let parsed = Url::parse(url_str).map_err(|e| AgentError::ExecutionError(format!("Navigation failed: {}", e)))?;
        *self.current_url.lock().await = parsed;
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

    // -- Helper naive selector matcher --

    fn find_node_by_selector(&self, dom: &DomTree, selector: &str) -> Option<NodeId> {
        self.find_all_nodes_by_selector(dom, selector).into_iter().next()
    }

    fn find_all_nodes_by_selector(&self, dom: &DomTree, selector: &str) -> Vec<NodeId> {
        if let Some(root) = dom.root() {
            let state = kitsune_css::selector::PseudoClassState { hovered: None, focused: None, active: None };
            kitsune_css::selector::query_all(selector, root.id, dom, &state)
        } else {
            Vec::new()
        }
    }

    fn extract_text(&self, dom: &DomTree, node_id: NodeId) -> String {
        let mut text = String::new();
        if let Some(node) = dom.get_node(node_id) {
            if let NodeType::Text(ref t) = node.node_type {
                text.push_str(t);
            }
            for &child_id in &node.children {
                text.push_str(&self.extract_text(dom, child_id));
            }
        }
        text.trim().to_string()
    }
}
