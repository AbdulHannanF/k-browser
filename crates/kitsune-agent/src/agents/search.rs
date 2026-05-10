use crate::ai_client::{AgentAiClient, ModelTier};
use crate::dom_access::DomAccessor;
use crate::error::{AgentError, AgentResult};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::info;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Candidate {
    pub title: String,
    pub url: String,
    pub deadline: Option<String>,
    pub requirements_summary: String,
}

pub struct SearchAgent {
    dom: Arc<DomAccessor>,
    ai: Arc<AgentAiClient>,
}

impl SearchAgent {
    pub fn new(dom: Arc<DomAccessor>, ai: Arc<AgentAiClient>) -> Self {
        Self { dom, ai }
    }

    pub async fn search(
        &self,
        query: &str,
        eligibility_filter: Option<&str>,
        profile_context: &str,
    ) -> AgentResult<Vec<Candidate>> {
        let search_url = format!(
            "https://www.google.com/search?q={}",
            urlencoding::encode(query)
        );
        self.dom.navigate(&search_url).await?;
        tokio::time::sleep(std::time::Duration::from_millis(1500)).await;

        let page_text = self.dom.get_page_text().await?;
        let links = self.dom.query_links("a[href]").await?;

        let candidate_links: Vec<String> = links
            .into_iter()
            .filter(|l| {
                !l.contains("google.com")
                    && !l.contains("accounts.")
                    && l.starts_with("https://")
            })
            .take(10)
            .collect();

        let filter_hint = eligibility_filter
            .map(|f| format!("Eligibility filter: {f}\n"))
            .unwrap_or_default();

        let page_snippet = &page_text[..page_text.len().min(2000)];

        let prompt = format!(
            r#"Given these search results and links, identify the top 3-5 scholarship/opportunity candidates.
{filter_hint}User profile:
{profile_context}

Page text:
{page_snippet}

Links found:
{}

Return ONLY valid JSON (no markdown):
[{{"title":"...","url":"...","deadline":"YYYY-MM-DD or null","requirements_summary":"..."}}]"#,
            candidate_links.join("\n")
        );

        let response = self.ai.complete(&prompt, ModelTier::Fast).await?;
        let json_str = response.trim().trim_start_matches("```json").trim_end_matches("```").trim();

        let candidates: Vec<Candidate> = serde_json::from_str(json_str)
            .map_err(|e| AgentError::ExecutionError(format!("Failed to parse candidates: {e}")))?;

        info!(count = candidates.len(), %query, "SearchAgent found candidates");
        Ok(candidates)
    }
}
