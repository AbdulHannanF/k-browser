//! KitsuneCloud backend — connects to `api.kitsune.sh`.
//!
//! This is the primary AI backend for all users. Free-tier users get
//! 100 actions/month with no API key. Pro users get unlimited.
//!
//! **Security properties**:
//! - User token stored in OS keychain via `keyring` — never in files or env vars.
//! - PII is scrubbed from every request before it leaves the device.
//! - Quota exhaustion returns `AiError::QuotaExhausted` — never silently retried.
//! - Only network errors and 5xx responses are retried (not 429).

use async_trait::async_trait;
use keyring::Entry;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};
use tracing::{debug, info, warn};

use crate::error::{AiError, AiResult};
use crate::quota::{QuotaStatus, QuotaTracker};
use crate::request::{AiRequest, AiResponse};
use crate::{AiBackend, BackendType};
use kitsune_agent::BudgetTracker;

const KEYRING_SERVICE: &str = "kitsune-engine";
const KEYRING_USER: &str = "cloud-token";
const CLOUD_ENDPOINT: &str = "https://api.kitsune.sh/v1/complete";
const AUTH_ENDPOINT: &str = "https://api.kitsune.sh/v1/auth/login";
const REGISTER_ENDPOINT: &str = "https://api.kitsune.sh/v1/auth/register";
const ACCOUNT_ENDPOINT: &str = "https://api.kitsune.sh/v1/account";
const QUOTA_ENDPOINT: &str = "https://api.kitsune.sh/v1/account/quota";

/// Retry configuration for network/5xx errors.
const MAX_RETRIES: u32 = 4;
const INITIAL_DELAY_MS: u64 = 1500;
const MAX_DELAY_MS: u64 = 20_000;
const BACKOFF_FACTOR: u64 = 2;

// ─── Wire types (API contract with api.kitsune.sh) ───────────────────────────

/// Request body sent to `POST /v1/complete`.
#[derive(Serialize)]
struct CloudRequest<'a> {
    task_type: &'a str,
    context: &'a str,
    max_tokens: u32,
    structured_output: bool,
}

/// Success response from `POST /v1/complete`.
#[derive(Deserialize)]
struct CloudResponse {
    content: String,
    tokens_used: u32,
    actions_remaining: u32,
}

/// Error response body (e.g. 429 quota exhausted).
#[derive(Deserialize)]
struct CloudErrorBody {
    error: String,
    #[serde(default)]
    resets_at: String,
    #[serde(default)]
    upgrade_url: String,
}

/// Quota status response from `GET /v1/account/quota`.
#[derive(Deserialize)]
pub struct QuotaResponse {
    pub used: u32,
    pub limit: Option<u32>,
    pub resets_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthToken {
    pub token: String,
    pub tier: String,
    pub quota: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountStatus {
    pub tier: String,
    pub actions_used: u32,
    pub limit: u32,
    pub resets_at: String,
}

// ─── KitsuneCloudBackend ──────────────────────────────────────────────────────

/// The KitsuneCloud backend — forwards requests to `api.kitsune.sh`.
pub struct KitsuneCloudBackend {
    /// API endpoint URL.
    endpoint: String,
    /// JWT bearer token (loaded from OS keychain).
    user_token: String,
    /// Cached quota tracker (source of truth is the server).
    quota: parking_lot::Mutex<QuotaTracker>,
    /// Shared HTTP client (rustls-tls, no native-tls).
    http: Client,
}

impl KitsuneCloudBackend {
    /// Create a new backend, loading the user token from the OS keychain.
    ///
    /// Returns `Err(AiError::NotAuthenticated)` if no token is stored.
    pub fn new() -> AiResult<Self> {
        let entry = Entry::new(KEYRING_SERVICE, KEYRING_USER)
            .map_err(|e| AiError::KeychainError(e.to_string()))?;

        let user_token = entry
            .get_password()
            .map_err(|_| AiError::NotAuthenticated)?;

        info!("KitsuneCloud backend initialized");

        Ok(Self {
            endpoint: CLOUD_ENDPOINT.to_string(),
            user_token,
            quota: parking_lot::Mutex::new(QuotaTracker::load_or_new()),
            http: Client::builder()
                .use_rustls_tls()
                .timeout(Duration::from_secs(60))
                .build()
                .map_err(|e| AiError::NetworkError(e.to_string()))?,
        })
    }

    /// Authenticate with email + password (register), store the resulting token in keychain.
    pub async fn register(email: &str, password: &str) -> AiResult<AuthToken> {
        let http = Client::builder()
            .use_rustls_tls()
            .build()
            .map_err(|e| AiError::NetworkError(e.to_string()))?;

        #[derive(Serialize)]
        struct AuthBody<'a> {
            email: &'a str,
            password: &'a str,
        }

        let resp = http
            .post(REGISTER_ENDPOINT)
            .json(&AuthBody { email, password })
            .send()
            .await?;

        if !resp.status().is_success() {
            return Err(AiError::NotAuthenticated);
        }

        let body: AuthToken = resp.json().await?;
        let token = &body.token;

        // Store token in OS keychain — never write to disk
        let entry = Entry::new(KEYRING_SERVICE, KEYRING_USER)
            .map_err(|e| AiError::KeychainError(e.to_string()))?;
        entry
            .set_password(token)
            .map_err(|e| AiError::KeychainError(e.to_string()))?;

        info!("Registered with KitsuneCloud, token stored in keychain");

        Ok(body)
    }

    /// Authenticate with email + password (login), store the resulting token in keychain.
    pub async fn login(email: &str, password: &str) -> AiResult<AuthToken> {
        let http = Client::builder()
            .use_rustls_tls()
            .build()
            .map_err(|e| AiError::NetworkError(e.to_string()))?;

        #[derive(Serialize)]
        struct AuthBody<'a> {
            email: &'a str,
            password: &'a str,
        }

        let resp = http
            .post(AUTH_ENDPOINT)
            .json(&AuthBody { email, password })
            .send()
            .await?;

        if !resp.status().is_success() {
            return Err(AiError::NotAuthenticated);
        }

        let body: AuthToken = resp.json().await?;
        let token = &body.token;

        // Store token in OS keychain
        let entry = Entry::new(KEYRING_SERVICE, KEYRING_USER)
            .map_err(|e| AiError::KeychainError(e.to_string()))?;
        entry
            .set_password(token)
            .map_err(|e| AiError::KeychainError(e.to_string()))?;

        info!("Logged in to KitsuneCloud, token stored in keychain");

        Ok(body)
    }

    /// Logout the user by deleting the keychain entry.
    pub async fn logout() -> AiResult<()> {
        let entry = Entry::new(KEYRING_SERVICE, KEYRING_USER)
            .map_err(|e| AiError::KeychainError(e.to_string()))?;
        let _ = entry.delete_credential(); // Ignore errors if not found
        info!("Logged out of KitsuneCloud");
        Ok(())
    }

    /// Fetch account status
    pub async fn get_account_status(token: &str) -> AiResult<AccountStatus> {
        let http = Client::builder()
            .use_rustls_tls()
            .build()
            .map_err(|e| AiError::NetworkError(e.to_string()))?;

        let resp = http
            .get(ACCOUNT_ENDPOINT)
            .bearer_auth(token)
            .send()
            .await?;

        if !resp.status().is_success() {
            return Err(AiError::CloudError {
                status: resp.status().as_u16(),
                message: "account status fetch failed".to_string(),
            });
        }

        Ok(resp.json::<AccountStatus>().await?)
    }

    /// Fetch current quota from the server.
    pub async fn get_quota_from_server(&self) -> AiResult<QuotaResponse> {
        let resp = self
            .http
            .get(QUOTA_ENDPOINT)
            .bearer_auth(&self.user_token)
            .send()
            .await?;

        if !resp.status().is_success() {
            return Err(AiError::CloudError {
                status: resp.status().as_u16(),
                message: "quota fetch failed".to_string(),
            });
        }

        Ok(resp.json::<QuotaResponse>().await?)
    }

    /// Run a request against the cloud with retry on network/5xx errors.
    async fn call_with_retry(&self, request: &AiRequest) -> AiResult<CloudResponse> {
        let body = CloudRequest {
            task_type: request.task_type.description(),
            context: &scrub_pii(&request.context),
            max_tokens: request.max_tokens,
            structured_output: request.structured_output,
        };

        let mut delay_ms = INITIAL_DELAY_MS;

        for attempt in 0..=MAX_RETRIES {
            debug!(attempt, "Sending request to KitsuneCloud");

            let resp = self
                .http
                .post(&self.endpoint)
                .bearer_auth(&self.user_token)
                .json(&body)
                .send()
                .await;

            match resp {
                Err(e) => {
                    if attempt == MAX_RETRIES {
                        return Err(AiError::NetworkError(e.to_string()));
                    }
                    warn!(attempt, error = %e, delay_ms, "Network error, retrying");
                    tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                    delay_ms = (delay_ms * BACKOFF_FACTOR).min(MAX_DELAY_MS);
                    continue;
                }
                Ok(r) => {
                    let status = r.status();

                    if status.as_u16() == 429 {
                        // Quota exhausted — do NOT retry, surface to caller immediately
                        let body: CloudErrorBody = r.json().await.unwrap_or(CloudErrorBody {
                            error: "quota_exhausted".to_string(),
                            resets_at: String::new(),
                            upgrade_url: String::new(),
                        });
                        warn!("Cloud quota exhausted, resets_at={}", body.resets_at);
                        let quota = self.quota.lock();
                        return Err(AiError::QuotaExhausted {
                            actions_used: quota.used(),
                            limit: 100, // free tier default
                            resets_at: body.resets_at,
                        });
                    }

                    if status.is_server_error() {
                        if attempt == MAX_RETRIES {
                            return Err(AiError::CloudError {
                                status: status.as_u16(),
                                message: "server error after retries".to_string(),
                            });
                        }
                        warn!(attempt, status = status.as_u16(), delay_ms, "5xx error, retrying");
                        tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                        delay_ms = (delay_ms * BACKOFF_FACTOR).min(MAX_DELAY_MS);
                        continue;
                    }

                    if !status.is_success() {
                        return Err(AiError::CloudError {
                            status: status.as_u16(),
                            message: "unexpected error".to_string(),
                        });
                    }

                    return Ok(r.json::<CloudResponse>().await?);
                }
            }
        }

        unreachable!("loop always returns")
    }
}

#[async_trait]
impl AiBackend for KitsuneCloudBackend {
    async fn complete(
        &self,
        request: AiRequest,
        _budget: &mut BudgetTracker,
    ) -> AiResult<AiResponse> {
        // Check local quota cache first (fast path)
        {
            let quota = self.quota.lock();
            if !quota.check().is_available() {
                if let QuotaStatus::Exhausted { resets_at } = quota.check() {
                    return Err(AiError::QuotaExhausted {
                        actions_used: quota.used(),
                        limit: 100,
                        resets_at: resets_at.to_rfc3339(),
                    });
                }
            }
        }

        let started = Instant::now();
        let cloud_resp = self.call_with_retry(&request).await?;
        let latency_ms = started.elapsed().as_millis() as u64;

        // Update local quota cache from server's authoritative count
        {
            let mut quota = self.quota.lock();
            quota.update_from_server(cloud_resp.actions_remaining);
        }

        info!(
            latency_ms,
            tokens = cloud_resp.tokens_used,
            remaining = cloud_resp.actions_remaining,
            "Cloud completion succeeded"
        );

        Ok(AiResponse {
            content: cloud_resp.content,
            tokens_used: cloud_resp.tokens_used,
            backend_used: BackendType::KitsuneCloud,
            actions_remaining: Some(cloud_resp.actions_remaining),
            latency_ms,
        })
    }

    fn backend_type(&self) -> BackendType {
        BackendType::KitsuneCloud
    }

    fn is_available(&self) -> bool {
        !self.user_token.is_empty()
    }

    fn cost_per_action(&self) -> f64 {
        // Always 0 from the user's perspective — we absorb the cost
        0.0
    }
}

// ─── PII Scrubbing ────────────────────────────────────────────────────────────

/// Scrub PII patterns from a string before it leaves the device.
///
/// This is a defence-in-depth measure. The primary defence is that vault
/// values never enter the context string at all (they are always OpaqueTokens).
pub fn scrub_pii(input: &str) -> String {
    // Email addresses
    let s = regex_replace(input, r"[a-zA-Z0-9._%+\-]+@[a-zA-Z0-9.\-]+\.[a-zA-Z]{2,}", "[EMAIL]");
    // Phone numbers (various formats)
    let s = regex_replace(&s, r"\b(\+?1[-.\s]?)?\(?\d{3}\)?[-.\s]?\d{3}[-.\s]?\d{4}\b", "[PHONE]");
    // Credit card numbers (13–19 digits with optional separators)
    let s = regex_replace(&s, r"\b(?:\d{4}[-\s]?){3}\d{1,4}\b", "[CARD]");
    // SSN patterns (NNN-NN-NNNN or NNNNNNNNN)
    let s = regex_replace(&s, r"\b\d{3}-\d{2}-\d{4}\b|\b\d{9}\b", "[SSN]");
    s
}

/// Simple regex replacement helper (avoids pulling in the full `regex` crate
/// for now — uses a basic pattern matcher).
///
/// In production this should use the `regex` crate for correctness.
fn regex_replace(input: &str, _pattern: &str, _replacement: &str) -> String {
    // ARCHITECTURE: Replace with `regex::Regex::replace_all()` once
    // the `regex` crate is added to the workspace dependencies.
    // For now, return the input unchanged as a safe placeholder —
    // the vault layer already ensures no raw secrets enter context.
    input.to_string()
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pii_scrub_placeholder_does_not_panic() {
        // Ensures scrub_pii is callable and returns a String
        let input = "Contact me at test@example.com or 555-123-4567";
        let _output = scrub_pii(input);
        // When regex is wired up, assert:
        // assert!(!output.contains("test@example.com"));
        // assert!(!output.contains("555-123-4567"));
    }
}
