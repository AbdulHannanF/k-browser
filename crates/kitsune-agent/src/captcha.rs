use crate::dom_access::DomAccessor;
use crate::error::{AgentError, AgentResult};
use crate::executor::WebViewCommand;
use kitsune_hil::{HilGate, HilTriggerClass};
use std::sync::Arc;
use tracing::{info, warn};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CaptchaKind {
    RecaptchaV2,
    RecaptchaV3 { action: String },
    HCaptcha,
    CloudflareTurnstile,
    Unknown,
}

impl CaptchaKind {
    pub fn detect_from_html(html: &str) -> Option<CaptchaKind> {
        if html.contains("cf-turnstile") || html.contains("cloudflare-turnstile") {
            return Some(CaptchaKind::CloudflareTurnstile);
        }
        if html.contains("h-captcha") || html.contains("hcaptcha.com") {
            return Some(CaptchaKind::HCaptcha);
        }
        if html.contains("g-recaptcha") || html.contains("recaptcha/api2") {
            if html.contains("grecaptcha.execute") {
                return Some(CaptchaKind::RecaptchaV3 { action: "default".into() });
            }
            return Some(CaptchaKind::RecaptchaV2);
        }
        if html.contains("data-sitekey") {
            return Some(CaptchaKind::Unknown);
        }
        None
    }

    pub fn display_name(&self) -> &str {
        match self {
            Self::RecaptchaV2 => "reCAPTCHA v2",
            Self::RecaptchaV3 { .. } => "reCAPTCHA v3",
            Self::HCaptcha => "hCaptcha",
            Self::CloudflareTurnstile => "Cloudflare Turnstile",
            Self::Unknown => "unknown CAPTCHA",
        }
    }
}

#[derive(Debug, Clone)]
pub struct CaptchaSolverConfig {
    pub endpoint: String,
    pub api_key: String,
}

pub struct CaptchaAgent {
    dom: Arc<DomAccessor>,
    hil_gate: Arc<HilGate>,
    solver_config: Option<CaptchaSolverConfig>,
    http: reqwest::Client,
}

impl CaptchaAgent {
    pub fn new(
        dom: Arc<DomAccessor>,
        hil_gate: Arc<HilGate>,
        solver_config: Option<CaptchaSolverConfig>,
    ) -> AgentResult<Self> {
        let http = reqwest::Client::builder()
            .use_rustls_tls()
            .timeout(std::time::Duration::from_secs(60))
            .build()
            .map_err(|e| AgentError::Internal(e.to_string()))?;
        Ok(Self { dom, hil_gate, solver_config, http })
    }

    pub async fn resolve(&self, site: &str) -> AgentResult<()> {
        let html = self.dom.get_page_text().await?;
        let kind = match CaptchaKind::detect_from_html(&html) {
            None => {
                info!(%site, "No CAPTCHA detected");
                return Ok(());
            }
            Some(k) => k,
        };

        info!(%site, captcha = kind.display_name(), "CAPTCHA detected");

        if matches!(kind, CaptchaKind::RecaptchaV2) {
            match self.try_audio_transcription().await {
                Ok(()) => return Ok(()),
                Err(e) => warn!("Audio transcription failed: {e}"),
            }
        }

        if let Some(cfg) = &self.solver_config {
            match self.try_api_solver(&kind, site, cfg).await {
                Ok(()) => return Ok(()),
                Err(e) => warn!("API solver failed: {e}"),
            }
        }

        self.escalate_to_hil(site, &kind).await
    }

    async fn try_audio_transcription(&self) -> AgentResult<()> {
        #[cfg(not(feature = "captcha-audio"))]
        return Err(AgentError::ExecutionError("captcha-audio feature not enabled".into()));
        #[cfg(feature = "captcha-audio")]
        Err(AgentError::ExecutionError("whisper not yet wired".into()))
    }

    async fn try_api_solver(
        &self,
        kind: &CaptchaKind,
        site: &str,
        cfg: &CaptchaSolverConfig,
    ) -> AgentResult<()> {
        let sitekey = self.extract_sitekey().await?;

        #[derive(serde::Serialize)]
        struct Task<'a> {
            #[serde(rename = "type")]
            task_type: &'a str,
            #[serde(rename = "websiteURL")]
            website_url: &'a str,
            #[serde(rename = "websiteKey")]
            website_key: &'a str,
        }
        #[derive(serde::Serialize)]
        struct CreateReq<'a> {
            #[serde(rename = "clientKey")]
            client_key: &'a str,
            task: Task<'a>,
        }
        #[derive(serde::Deserialize)]
        struct CreateResp {
            #[serde(rename = "taskId")]
            task_id: Option<u64>,
            #[serde(rename = "errorId")]
            error_id: u32,
        }
        #[derive(serde::Deserialize)]
        struct Solution {
            #[serde(rename = "gRecaptchaResponse")]
            g_recaptcha_response: Option<String>,
            token: Option<String>,
        }
        #[derive(serde::Deserialize)]
        struct ResultResp {
            status: String,
            solution: Option<Solution>,
        }
        #[derive(serde::Serialize)]
        struct GetReq<'a> {
            #[serde(rename = "clientKey")]
            client_key: &'a str,
            #[serde(rename = "taskId")]
            task_id: u64,
        }

        let task_type = match kind {
            CaptchaKind::HCaptcha => "HCaptchaTaskProxyless",
            CaptchaKind::CloudflareTurnstile => "TurnstileTaskProxyless",
            _ => "RecaptchaV2TaskProxyless",
        };

        let resp = self
            .http
            .post(format!("{}/createTask", cfg.endpoint.trim_end_matches('/')))
            .json(&CreateReq {
                client_key: &cfg.api_key,
                task: Task { task_type, website_url: site, website_key: &sitekey },
            })
            .send()
            .await
            .map_err(|e| AgentError::ExecutionError(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(AgentError::ExecutionError(format!("solver HTTP {}", resp.status())));
        }
        let create: CreateResp = resp.json().await.map_err(|e| AgentError::ExecutionError(e.to_string()))?;

        if create.error_id != 0 || create.task_id.is_none() {
            return Err(AgentError::ExecutionError("solver rejected task".into()));
        }
        let task_id = create.task_id.unwrap();

        for _ in 0..12 {
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            let result_resp = self
                .http
                .post(format!("{}/getTaskResult", cfg.endpoint.trim_end_matches('/')))
                .json(&GetReq { client_key: &cfg.api_key, task_id })
                .send()
                .await
                .map_err(|e| AgentError::ExecutionError(e.to_string()))?;
            if !result_resp.status().is_success() {
                continue;
            }
            let result: ResultResp = result_resp.json().await.map_err(|e| AgentError::ExecutionError(e.to_string()))?;
            if result.status == "ready" {
                let token = result
                    .solution
                    .and_then(|s| s.g_recaptcha_response.or(s.token))
                    .ok_or_else(|| AgentError::ExecutionError("no token in solution".into()))?;
                return self.inject_solver_token(&token).await;
            }
        }
        Err(AgentError::ExecutionError("solver timed out".into()))
    }

    async fn extract_sitekey(&self) -> AgentResult<String> {
        let (tx, mut rx) = tokio::sync::mpsc::channel(1);
        let script = r#"(function(){
            let el = document.querySelector('[data-sitekey]');
            window.__kitsune_ipc(JSON.stringify({sitekey: el ? el.dataset.sitekey : null}));
        })();"#;
        self.dom.eval_js_with_callback(script, tx).await?;
        let raw = rx.recv().await.ok_or(AgentError::IpcDisconnected)?;
        let val: serde_json::Value = serde_json::from_str(&raw).unwrap_or_default();
        val["sitekey"]
            .as_str()
            .map(|s| s.to_string())
            .ok_or_else(|| AgentError::ExecutionError("sitekey not found in DOM".into()))
    }

    async fn inject_solver_token(&self, token: &str) -> AgentResult<()> {
        let safe = token.replace('\\', "\\\\").replace('\'', "\\'");
        let script = format!(
            r#"(function(){{
                let el = document.querySelector('[name="g-recaptcha-response"]');
                if (!el) el = document.querySelector('[name="h-captcha-response"]');
                if (el) {{ el.value = '{safe}'; el.dispatchEvent(new Event('change',{{bubbles:true}})); }}
                if (window.grecaptcha) {{ try {{ grecaptcha.reset(); }} catch(e) {{}} }}
            }})();"#
        );
        self.dom.eval_js_fire_and_forget(&script).await
            .map_err(|e| AgentError::ExecutionError(format!("token inject: {e}")))
    }

    async fn escalate_to_hil(&self, site: &str, kind: &CaptchaKind) -> AgentResult<()> {
        warn!(%site, captcha = kind.display_name(), "Escalating CAPTCHA to HIL (Tier 4)");
        let trigger = HilTriggerClass::CaptchaRequired {
            site: site.to_string(),
            captcha_type: kind.display_name().to_string(),
        };
        self.hil_gate
            .checkpoint(trigger, vec![])
            .await
            .map_err(|e| AgentError::HilRejected(format!("{e:?}")))?;
        Ok(())
    }
}

impl DomAccessor {
    pub async fn eval_js_fire_and_forget(&self, script: &str) -> AgentResult<()> {
        self.webview_tx
            .send(WebViewCommand::EvalJs(script.to_string()))
            .await
            .map_err(|_| AgentError::IpcDisconnected)
    }

    pub async fn eval_js_with_callback(
        &self,
        script: &str,
        tx: tokio::sync::mpsc::Sender<String>,
    ) -> AgentResult<()> {
        self.webview_tx
            .send(WebViewCommand::EvalJsWithCallback(script.to_string(), tx))
            .await
            .map_err(|_| AgentError::IpcDisconnected)
    }
}
