use crate::agents::search::Candidate;
use crate::ai_client::{AgentAiClient, ModelTier};
use crate::captcha::CaptchaAgent;
use crate::dom_access::DomAccessor;
use crate::error::{AgentError, AgentResult};
use crate::profile::ProfileSummary;
use kitsune_hil::{HilGate, HilTriggerClass};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::info;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "op")]
pub enum FieldAction {
    FillFromProfile { selector: String, profile_field: String },
    FillStatic { selector: String, value: String },
    SelectOption { selector: String, value: String },
    UploadFile { selector: String, file_path: String },
    Click { selector: String },
    CaptchaCheck,
    NavigateNext { selector: String },
    WaitForElement { selector: String, timeout_ms: u64 },
    AwaitHil { reason: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldMappingPlan {
    pub fields: Vec<FieldAction>,
}

#[derive(Debug, Clone)]
pub struct FormResult {
    pub site: String,
    pub filled_count: usize,
    pub submit_selector: Option<String>,
    pub confirmation_text: Option<String>,
}

pub struct FormAgent {
    dom: Arc<DomAccessor>,
    ai: Arc<AgentAiClient>,
    captcha: Arc<CaptchaAgent>,
    hil_gate: Arc<HilGate>,
}

impl FormAgent {
    pub fn new(
        dom: Arc<DomAccessor>,
        ai: Arc<AgentAiClient>,
        captcha: Arc<CaptchaAgent>,
        hil_gate: Arc<HilGate>,
    ) -> Self {
        Self { dom, ai, captcha, hil_gate }
    }

    pub async fn fill_and_submit(
        &self,
        url: &str,
        profile: &ProfileSummary,
    ) -> AgentResult<FormResult> {
        self.dom.navigate(url).await?;
        tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
        self.captcha.resolve(url).await?;

        let plan = self.plan_fields(url, profile).await?;
        info!(steps = plan.fields.len(), %url, "FormAgent executing plan");

        let mut filled = 0usize;
        let submit_selector = None;

        for action in &plan.fields {
            match action {
                FieldAction::FillFromProfile { selector, profile_field } => {
                    let value = resolve_profile_field(profile, profile_field);
                    if !value.is_empty() {
                        self.dom.fill_field(selector, &value).await?;
                        filled += 1;
                    }
                }
                FieldAction::FillStatic { selector, value } => {
                    self.dom.fill_field(selector, value).await?;
                    filled += 1;
                }
                FieldAction::SelectOption { selector, value } => {
                    let safe_sel = selector.replace('\\', "\\\\").replace('\'', "\\'");
                    let safe_val = value.replace('\\', "\\\\").replace('\'', "\\'");
                    let script = format!(
                        r#"(function(){{
                            let el = document.querySelector('{safe_sel}');
                            if (el) {{
                                el.value = '{safe_val}';
                                el.dispatchEvent(new Event('change',{{bubbles:true}}));
                            }}
                        }})();"#
                    );
                    self.dom.eval_js_fire_and_forget(&script).await?;
                    filled += 1;
                }
                FieldAction::Click { selector } => {
                    self.dom.click_element(selector).await?;
                }
                FieldAction::CaptchaCheck => {
                    self.captcha.resolve(url).await?;
                }
                FieldAction::NavigateNext { selector } => {
                    self.dom.click_element(selector).await?;
                    tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
                    self.captcha.resolve(url).await?;
                    break;
                }
                FieldAction::WaitForElement { timeout_ms, .. } => {
                    tokio::time::sleep(std::time::Duration::from_millis(*timeout_ms)).await;
                }
                FieldAction::AwaitHil { reason } => {
                    let trigger = HilTriggerClass::ExternalSideEffect {
                        description: reason.clone(),
                        reversible: false,
                    };
                    self.hil_gate
                        .checkpoint(trigger, vec![])
                        .await
                        .map_err(|e| AgentError::HilRejected(format!("{e:?}")))?;
                }
                FieldAction::UploadFile { .. } => {
                    // Upload requires HIL — not yet implemented
                }
            }
        }

        Ok(FormResult {
            site: url.to_string(),
            filled_count: filled,
            submit_selector,
            confirmation_text: None,
        })
    }

    async fn plan_fields(&self, url: &str, profile: &ProfileSummary) -> AgentResult<FieldMappingPlan> {
        let (tx, mut rx) = tokio::sync::mpsc::channel(1);
        let script = r#"(function(){
            let fields = [];
            document.querySelectorAll('input,select,textarea').forEach(el => {
                let label = '';
                if (el.id) {
                    let lbl = document.querySelector('label[for="' + el.id + '"]');
                    if (lbl) label = lbl.textContent.trim();
                }
                if (!label && el.placeholder) label = el.placeholder;
                if (!label && el.name) label = el.name;
                let opts = [];
                if (el.tagName === 'SELECT') opts = [...el.options].map(o => o.text);
                fields.push({
                    selector: el.id ? '#' + el.id : (el.name ? '[name="'+el.name+'"]' : el.tagName.toLowerCase()),
                    label, type: el.type || el.tagName.toLowerCase(), options: opts
                });
            });
            window.__kitsune_ipc(JSON.stringify({fields}));
        })();"#;
        self.dom.eval_js_with_callback(script, tx).await?;
        let raw = rx.recv().await.ok_or(AgentError::IpcDisconnected)?;
        let val: serde_json::Value = serde_json::from_str(&raw).unwrap_or_default();
        let dom_structure = serde_json::to_string_pretty(&val["fields"]).unwrap_or_default();

        let profile_ctx = profile.to_prompt_context();
        let prompt = format!(
            r##"Map these form fields to profile data. Return ONLY valid JSON (no markdown):
{{"fields":[
  {{"op":"FillFromProfile","selector":"#id","profile_field":"full_name"}},
  {{"op":"FillStatic","selector":"#id","value":"literal"}},
  {{"op":"SelectOption","selector":"select#id","value":"option text"}},
  {{"op":"CaptchaCheck"}},
  {{"op":"Click","selector":"button[type=submit]"}}
]}}

Available profile_field values: full_name, email, phone, nationality, date_of_birth,
education[0].degree, education[0].institution, education[0].gpa, languages[0].language,
languages[0].level, skills, awards

User profile:
{profile_ctx}

Form fields:
{dom_structure}

Page URL: {url}"##
        );

        let response = self.ai.complete(&prompt, ModelTier::Worker).await?;
        let json_str = response.trim().trim_start_matches("```json").trim_end_matches("```").trim();

        serde_json::from_str(json_str)
            .map_err(|e| AgentError::ExecutionError(format!("Bad plan JSON: {e}")))
    }
}

fn resolve_profile_field(profile: &ProfileSummary, field: &str) -> String {
    match field {
        "full_name" => profile.full_name.clone(),
        "email" => profile.email.clone().unwrap_or_default(),
        "phone" => profile.phone.clone().unwrap_or_default(),
        "nationality" => profile.nationality.clone().unwrap_or_default(),
        "date_of_birth" => profile.date_of_birth.clone().unwrap_or_default(),
        "skills" => profile.skills.join(", "),
        "awards" => profile.awards.join("; "),
        f if f.starts_with("education[0].") => {
            let sub = &f["education[0].".len()..];
            profile.education.first().map(|e| match sub {
                "degree" => e.degree.clone(),
                "institution" => e.institution.clone(),
                "gpa" => e.gpa.map(|g| format!("{g:.1}")).unwrap_or_default(),
                "year" => e.year.map(|y| y.to_string()).unwrap_or_default(),
                _ => String::new(),
            }).unwrap_or_default()
        }
        f if f.starts_with("languages[0].") => {
            let sub = &f["languages[0].".len()..];
            profile.languages.first().map(|l| match sub {
                "language" => l.language.clone(),
                "level" => l.level.clone(),
                _ => String::new(),
            }).unwrap_or_default()
        }
        _ => String::new(),
    }
}
