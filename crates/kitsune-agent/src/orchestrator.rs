use crate::agents::booking::{BookingAgent, BookingCriteria};
use crate::agents::form::FormAgent;
use crate::agents::search::SearchAgent;
use crate::agents::submit::SubmitAgent;
use crate::ai_client::{AgentAiClient, ModelTier};
use crate::captcha::CaptchaAgent;
use crate::dom_access::DomAccessor;
use crate::error::{AgentError, AgentResult};
use crate::profile::{ProfileIndexer, ProfileSummary};
use kitsune_hil::HilGate;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::info;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum TaskStatus {
    #[default]
    Pending,
    Running,
    Completed,
    Failed(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum SubTask {
    Search { query: String, eligibility_filter: Option<String> },
    Form { url: String, candidate_title: Option<String> },
    Submit { site: String, filled_count: usize, submit_selector: Option<String> },
    AccountCreate { site: String, username: String },
    Booking { origin: String, destination: String, date: String, criteria: BookingCriteria },
}

pub struct AgentOrchestrator {
    dom: Arc<DomAccessor>,
    ai: Arc<AgentAiClient>,
    captcha: Arc<CaptchaAgent>,
    hil_gate: Arc<HilGate>,
    profile: Arc<ProfileIndexer>,
}

impl AgentOrchestrator {
    pub fn new(
        dom: Arc<DomAccessor>,
        ai: Arc<AgentAiClient>,
        captcha: Arc<CaptchaAgent>,
        hil_gate: Arc<HilGate>,
        profile: Arc<ProfileIndexer>,
    ) -> Self {
        Self { dom, ai, captcha, hil_gate, profile }
    }

    pub async fn plan(&self, goal: &str, profile: &ProfileSummary) -> AgentResult<Vec<SubTask>> {
        let profile_ctx = profile.to_prompt_context();
        let prompt = format!(
            r#"You are a task planner. Decompose the user's goal into a JSON array of SubTasks.

Available task types:
- Search: {{"type":"Search","query":"...","eligibility_filter":"..." or null}}
- Form: {{"type":"Form","url":"https://...","candidate_title":"..." or null}}
- Submit: {{"type":"Submit","site":"https://...","filled_count":0,"submit_selector":"button[type=submit]" or null}}
- AccountCreate: {{"type":"AccountCreate","site":"https://...","username":"email@example.com"}}
- Booking: {{"type":"Booking","origin":"Berlin","destination":"London","date":"YYYY-MM-DD","criteria":{{"primary":"Cheapest","max_stops":null,"max_price_minor":null}}}}

Return ONLY a valid JSON array. No markdown.

User goal: {goal}

User profile:
{profile_ctx}"#
        );

        let response = self.ai.complete(&prompt, ModelTier::Orchestrator).await?;
        let json_str = response.trim().trim_start_matches("```json").trim_end_matches("```").trim();

        let tasks: Vec<SubTask> = serde_json::from_str(json_str)
            .map_err(|e| AgentError::ExecutionError(format!("Bad plan: {e}")))?;
        info!(count = tasks.len(), %goal, "Orchestrator planned tasks");
        Ok(tasks)
    }

    pub async fn execute(&self, tasks: Vec<SubTask>, profile: &ProfileSummary) -> AgentResult<Vec<String>> {
        let mut results = Vec::new();
        let search_agent = SearchAgent::new(self.dom.clone(), self.ai.clone());
        let form_agent = FormAgent::new(self.dom.clone(), self.ai.clone(), self.captcha.clone(), self.hil_gate.clone());
        let submit_agent = SubmitAgent::new(self.dom.clone(), self.hil_gate.clone());
        let booking_agent = BookingAgent::new(self.ai.clone())
            .map_err(|e| AgentError::Internal(e.to_string()))?;

        for task in tasks {
            match task {
                SubTask::Search { query, eligibility_filter } => {
                    let candidates = search_agent
                        .search(&query, eligibility_filter.as_deref(), &profile.to_prompt_context())
                        .await?;
                    results.push(format!("Found {} candidates for '{}'", candidates.len(), query));
                }
                SubTask::Form { url, .. } => {
                    let form_result = form_agent.fill_and_submit(&url, profile).await?;
                    results.push(format!("Filled {} fields on {}", form_result.filled_count, form_result.site));
                }
                SubTask::Submit { site, filled_count, submit_selector } => {
                    let form_result = crate::agents::form::FormResult {
                        site: site.clone(),
                        filled_count,
                        submit_selector,
                        confirmation_text: None,
                    };
                    let submitted = submit_agent.submit(form_result).await?;
                    results.push(format!(
                        "Submitted on {}. Confirmation: {}",
                        submitted.site,
                        submitted.confirmation_text.as_deref().unwrap_or("(none)")
                    ));
                }
                SubTask::AccountCreate { site, username } => {
                    results.push(format!("Account creation on {site} as {username} — awaiting HIL gate"));
                }
                SubTask::Booking { origin, destination, date, criteria } => {
                    let offers = booking_agent.fetch_offers(&origin, &destination, &date).await?;
                    if let Some(best) = criteria.rank(&offers) {
                        results.push(format!(
                            "Best flight: {} {} ({}min, {} stops) — {}",
                            best.price_display(), best.airline, best.duration_mins,
                            best.stops, best.booking_url
                        ));
                        form_agent.fill_and_submit(&best.booking_url, profile).await?;
                    } else {
                        results.push("No flights found matching criteria".into());
                    }
                }
            }
        }
        Ok(results)
    }

    pub async fn run(&self, goal: &str, profile: &ProfileSummary) -> AgentResult<Vec<String>> {
        let tasks = self.plan(goal, profile).await?;
        self.execute(tasks, profile).await
    }
}
