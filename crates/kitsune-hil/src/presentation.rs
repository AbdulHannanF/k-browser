/// HIL Presentation — data structure for presenting confirmation dialogs to the user.
///
/// The presentation is designed for non-technical users. It translates
/// low-level agent actions into clear, warm, plain-language descriptions.

use crate::trigger::HilTriggerClass;
use serde::{Deserialize, Serialize};

/// The data needed to render a HIL confirmation dialog.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HilPresentation {
    /// Plain-language summary: "What will happen"
    pub what_will_happen: String,

    /// The domain or service being interacted with.
    pub domain_or_service: String,

    /// List of vault entry labels involved (never values).
    pub data_involved: Vec<String>,

    /// Cost information if this involves money.
    pub cost_display: Option<String>,

    /// Whether this action can be reversed.
    pub is_reversible: bool,

    /// Suggested alternatives if the user wants to modify the approach.
    pub alternatives: Vec<String>,

    /// Minimum countdown before the confirm button becomes clickable (in seconds).
    pub countdown_seconds: u32,

    /// Severity level for visual styling.
    pub severity: ConfirmationSeverity,
}

/// Visual severity level for the confirmation dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConfirmationSeverity {
    /// Informational — low risk (e.g., navigating to a new site).
    Low,
    /// Medium risk — involves data or credentials.
    Medium,
    /// High risk — involves money, legal agreements, or irreversible actions.
    High,
    /// Critical — involves large sums of money or highly sensitive operations.
    Critical,
}

impl HilPresentation {
    /// Build a presentation from a trigger class and data labels.
    pub fn from_trigger(trigger: &HilTriggerClass, data_labels: &[String]) -> Self {
        match trigger {
            HilTriggerClass::FinancialFormSubmission {
                institution,
                amount,
                fields_involved,
            } => {
                let what_will_happen = if let Some(money) = amount {
                    format!(
                        "Your assistant wants to send {} to {}",
                        money.display(),
                        institution
                    )
                } else {
                    format!(
                        "Your assistant wants to submit financial information to {}",
                        institution
                    )
                };

                let cost_display = amount.as_ref().map(|m| m.display());

                let field_names: Vec<String> = fields_involved
                    .iter()
                    .map(|f| f.label.clone())
                    .collect();

                let mut all_data = data_labels.to_vec();
                all_data.extend(field_names);

                Self {
                    what_will_happen,
                    domain_or_service: institution.clone(),
                    data_involved: all_data,
                    cost_display,
                    is_reversible: false,
                    alternatives: vec![
                        "Do this yourself instead".to_string(),
                        "Change the amount".to_string(),
                    ],
                    countdown_seconds: 5,
                    severity: ConfirmationSeverity::Critical,
                }
            }

            HilTriggerClass::AccountCreation {
                service,
                implied_cost,
                terms_url,
            } => {
                let what_will_happen = format!(
                    "Your assistant wants to create an account on {}",
                    service
                );

                let cost_display = implied_cost.as_ref().map(|p| p.description.clone());

                let mut alternatives = vec!["Create the account yourself".to_string()];
                if let Some(url) = terms_url {
                    alternatives.push(format!("Read the terms first: {}", url));
                }

                Self {
                    what_will_happen,
                    domain_or_service: service.clone(),
                    data_involved: data_labels.to_vec(),
                    cost_display,
                    is_reversible: true,
                    alternatives,
                    countdown_seconds: 3,
                    severity: if implied_cost.is_some() {
                        ConfirmationSeverity::High
                    } else {
                        ConfirmationSeverity::Medium
                    },
                }
            }

            HilTriggerClass::NewAuthenticationSite {
                domain,
                credential_type,
            } => {
                let cred_desc = match credential_type {
                    crate::trigger::CredentialType::UsernamePassword => "your saved login",
                    crate::trigger::CredentialType::ApiKey => "an API key",
                    crate::trigger::CredentialType::OAuthToken => "a connected account",
                    crate::trigger::CredentialType::Certificate => "a security certificate",
                    crate::trigger::CredentialType::Biometric => "biometric verification",
                    crate::trigger::CredentialType::Other(s) => s.as_str(),
                };

                Self {
                    what_will_happen: format!(
                        "Your assistant wants to sign in to {} using {}",
                        domain, cred_desc
                    ),
                    domain_or_service: domain.clone(),
                    data_involved: data_labels.to_vec(),
                    cost_display: None,
                    is_reversible: true,
                    alternatives: vec!["Sign in yourself".to_string()],
                    countdown_seconds: 3,
                    severity: ConfirmationSeverity::Medium,
                }
            }

            HilTriggerClass::BilledApiCall {
                provider,
                estimated_cost,
                action_description,
            } => {
                let cost_display = estimated_cost.as_ref().map(|m| m.display());

                Self {
                    what_will_happen: format!(
                        "Your assistant wants to use {} to: {}",
                        provider, action_description
                    ),
                    domain_or_service: provider.clone(),
                    data_involved: data_labels.to_vec(),
                    cost_display,
                    is_reversible: false,
                    alternatives: vec!["Do this manually".to_string()],
                    countdown_seconds: 3,
                    severity: ConfirmationSeverity::High,
                }
            }

            HilTriggerClass::CommunicationOnBehalf {
                channel,
                recipient_summary,
                content_preview,
            } => Self {
                what_will_happen: format!(
                    "Your assistant wants to send a {} to {}",
                    channel, recipient_summary
                ),
                domain_or_service: channel.clone(),
                data_involved: {
                    let mut d = data_labels.to_vec();
                    d.push(format!("Message preview: {}", content_preview));
                    d
                },
                cost_display: None,
                is_reversible: false,
                alternatives: vec![
                    "Edit the message first".to_string(),
                    "Send it yourself".to_string(),
                ],
                countdown_seconds: 5,
                severity: ConfirmationSeverity::High,
            },

            HilTriggerClass::ExecutionRequest {
                source,
                description,
            } => Self {
                what_will_happen: format!(
                    "Your assistant wants to download and run something: {}",
                    description
                ),
                domain_or_service: source.host_str().unwrap_or("unknown").to_string(),
                data_involved: data_labels.to_vec(),
                cost_display: None,
                is_reversible: false,
                alternatives: vec!["Download it yourself to review first".to_string()],
                countdown_seconds: 5,
                severity: ConfirmationSeverity::Critical,
            },

            HilTriggerClass::ExternalSideEffect {
                description,
                reversible,
            } => Self {
                what_will_happen: format!("Your assistant wants to: {}", description),
                domain_or_service: "External service".to_string(),
                data_involved: data_labels.to_vec(),
                cost_display: None,
                is_reversible: *reversible,
                alternatives: vec!["Do this yourself instead".to_string()],
                countdown_seconds: 3,
                severity: if *reversible {
                    ConfirmationSeverity::Medium
                } else {
                    ConfirmationSeverity::High
                },
            },
        }
    }
}
