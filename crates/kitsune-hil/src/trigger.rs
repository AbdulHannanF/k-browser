/// HIL trigger classification — determines when a human confirmation is required.
///
/// Each variant represents a class of action that requires human approval
/// before it can be executed. The trigger class determines what information
/// is presented to the user in the confirmation dialog.
use serde::{Deserialize, Serialize};
use url::Url;

/// Monetary amount with currency.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Money {
    /// Amount in the smallest unit (e.g., cents for USD).
    pub amount_minor: i64,
    /// ISO 4217 currency code (e.g., "USD", "EUR").
    pub currency: String,
}

impl Money {
    /// Create a new monetary amount.
    pub fn new(amount_minor: i64, currency: impl Into<String>) -> Self {
        Self {
            amount_minor,
            currency: currency.into(),
        }
    }

    /// Format the amount for display (e.g., "$12.34").
    pub fn display(&self) -> String {
        let major = self.amount_minor / 100;
        let minor = (self.amount_minor % 100).abs();
        let symbol = match self.currency.as_str() {
            "USD" => "$",
            "EUR" => "€",
            "GBP" => "£",
            "JPY" => "¥",
            _ => &self.currency,
        };
        format!("{}{}.{:02}", symbol, major, minor)
    }
}

/// Summary of a form field involved in an action.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldSummary {
    /// Human-readable label for this field.
    pub label: String,
    /// Type of data (e.g., "email", "credit card", "password").
    pub data_type: String,
    /// Whether this field contains sensitive data.
    pub is_sensitive: bool,
}

/// Pricing information for a service.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PricingInfo {
    /// Cost description in plain language.
    pub description: String,
    /// Monthly cost if applicable.
    pub monthly_cost: Option<Money>,
    /// One-time cost if applicable.
    pub one_time_cost: Option<Money>,
    /// Whether there's a free trial.
    pub has_free_trial: bool,
    /// Free trial duration in days.
    pub trial_days: Option<u32>,
}

/// Type of credential being used.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CredentialType {
    /// Username/password combination.
    UsernamePassword,
    /// API key or token.
    ApiKey,
    /// OAuth token.
    OAuthToken,
    /// Certificate-based authentication.
    Certificate,
    /// Biometric authentication.
    Biometric,
    /// Other credential type.
    Other(String),
}

/// Classification of actions that trigger a HIL confirmation.
///
/// Each variant carries enough context to present a meaningful,
/// plain-language explanation to the user.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HilTriggerClass {
    /// Agent is about to submit a form to a financial institution.
    FinancialFormSubmission {
        /// Name of the financial institution.
        institution: String,
        /// Amount involved, if known.
        amount: Option<Money>,
        /// Summary of form fields being submitted.
        fields_involved: Vec<FieldSummary>,
    },

    /// Agent is about to create an account or subscription.
    AccountCreation {
        /// Name of the service.
        service: String,
        /// Pricing information if available.
        implied_cost: Option<PricingInfo>,
        /// URL to the terms of service.
        terms_url: Option<Url>,
    },

    /// Agent is about to use a stored credential on a new site.
    NewAuthenticationSite {
        /// The domain being authenticated to.
        domain: String,
        /// Type of credential being used.
        credential_type: CredentialType,
    },

    /// Agent wants to call a third-party API that has usage billing.
    BilledApiCall {
        /// The API provider.
        provider: String,
        /// Estimated cost of this API call.
        estimated_cost: Option<Money>,
        /// Plain-language description of what the API call does.
        action_description: String,
    },

    /// Agent wants to send a communication on behalf of the user.
    CommunicationOnBehalf {
        /// Communication channel (email, SMS, etc.).
        channel: String,
        /// Summary of who the message is going to.
        recipient_summary: String,
        /// Preview of the message content.
        content_preview: String,
    },

    /// Agent wants to download and execute something.
    ExecutionRequest {
        /// Source URL of the executable.
        source: Url,
        /// Description of what will be executed.
        description: String,
    },

    /// Catch-all for actions with external side effects.
    ExternalSideEffect {
        /// Plain-language description of the action.
        description: String,
        /// Whether this action can be reversed.
        reversible: bool,
    },

    /// Agent encountered a CAPTCHA that requires resolution before proceeding.
    CaptchaRequired {
        /// The domain where the CAPTCHA was detected.
        site: String,
        /// CAPTCHA type (e.g. "recaptcha-v2", "hcaptcha", "cloudflare-turnstile").
        captcha_type: String,
    },
}

impl HilTriggerClass {
    /// Get a plain-language summary of this trigger for the user.
    pub fn plain_language_summary(&self) -> String {
        match self {
            Self::FinancialFormSubmission {
                institution,
                amount,
                ..
            } => {
                if let Some(money) = amount {
                    format!("Send a payment of {} to {}", money.display(), institution)
                } else {
                    format!("Submit financial information to {}", institution)
                }
            }
            Self::AccountCreation {
                service,
                implied_cost,
                ..
            } => {
                if let Some(pricing) = implied_cost {
                    format!("Create an account on {} — {}", service, pricing.description)
                } else {
                    format!("Create an account on {}", service)
                }
            }
            Self::NewAuthenticationSite { domain, .. } => {
                format!("Sign in to {} using your saved credentials", domain)
            }
            Self::BilledApiCall {
                provider,
                estimated_cost,
                action_description,
            } => {
                if let Some(cost) = estimated_cost {
                    format!(
                        "{} via {} (estimated cost: {})",
                        action_description,
                        provider,
                        cost.display()
                    )
                } else {
                    format!("{} via {}", action_description, provider)
                }
            }
            Self::CommunicationOnBehalf {
                channel,
                recipient_summary,
                ..
            } => {
                format!("Send a {} to {}", channel, recipient_summary)
            }
            Self::ExecutionRequest { description, .. } => {
                format!("Download and run: {}", description)
            }
            Self::ExternalSideEffect {
                description,
                reversible,
            } => {
                if *reversible {
                    format!("{} (can be undone)", description)
                } else {
                    format!("{} (cannot be undone)", description)
                }
            }
            Self::CaptchaRequired { site, captcha_type } => {
                format!("CAPTCHA detected on {} ({}). Please solve it to continue.", site, captcha_type)
            }
        }
    }

    /// Whether this trigger class involves money.
    pub fn involves_money(&self) -> bool {
        matches!(
            self,
            Self::FinancialFormSubmission { .. }
                | Self::AccountCreation {
                    implied_cost: Some(_),
                    ..
                }
                | Self::BilledApiCall { .. }
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn captcha_required_summary_contains_site() {
        let t = HilTriggerClass::CaptchaRequired {
            site: "daad.de".into(),
            captcha_type: "recaptcha-v2".into(),
        };
        let s = t.plain_language_summary();
        assert!(s.contains("daad.de"), "summary: {s}");
        assert!(s.contains("CAPTCHA"), "summary: {s}");
    }
}
