use crate::agents::form::FormResult;
use crate::dom_access::DomAccessor;
use crate::error::{AgentError, AgentResult};
use kitsune_hil::{HilGate, HilTriggerClass};
use std::sync::Arc;
use tracing::info;

pub struct SubmitAgent {
    dom: Arc<DomAccessor>,
    hil_gate: Arc<HilGate>,
}

impl SubmitAgent {
    pub fn new(dom: Arc<DomAccessor>, hil_gate: Arc<HilGate>) -> Self {
        Self { dom, hil_gate }
    }

    pub async fn submit(&self, mut result: FormResult) -> AgentResult<FormResult> {
        let trigger = HilTriggerClass::ExternalSideEffect {
            description: format!(
                "Submit form on {} ({} fields filled). This action cannot be undone.",
                result.site, result.filled_count
            ),
            reversible: false,
        };
        self.hil_gate
            .checkpoint(trigger, vec![result.site.clone()])
            .await
            .map_err(|e| AgentError::HilRejected(format!("{e:?}")))?;

        let selector = result.submit_selector.as_deref().unwrap_or("button[type=submit]");
        self.dom.click_element(selector).await?;

        tokio::time::sleep(std::time::Duration::from_millis(2500)).await;

        let confirm = self.dom.get_page_text().await.unwrap_or_default();
        result.confirmation_text = Some(confirm[..confirm.len().min(500)].to_string());

        info!(site = %result.site, "Form submitted");
        Ok(result)
    }
}
