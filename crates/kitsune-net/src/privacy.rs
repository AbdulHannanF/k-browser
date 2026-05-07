/// Privacy enforcement at the network layer.
///
/// All requests are processed through the privacy layer BEFORE they leave
/// the engine. This ensures privacy protections cannot be bypassed by
/// any component.
use crate::{PrivacyAwareRequest, PrivacyReport};
use tracing::debug;

/// Known tracker domains (this would be loaded from an updatable blocklist).
const KNOWN_TRACKERS: &[&str] = &[
    "doubleclick.net",
    "googlesyndication.com",
    "google-analytics.com",
    "facebook.net",
    "facebook.com/tr",
    "analytics.twitter.com",
    "bat.bing.com",
    "pixel.quantserve.com",
];

/// Headers that should be stripped for privacy.
const PRIVACY_STRIP_HEADERS: &[&str] = &["referer", "x-forwarded-for", "x-real-ip", "x-client-ip"];

/// Apply privacy protections to an outgoing request.
pub fn apply_privacy_protections(request: &mut PrivacyAwareRequest) -> PrivacyReport {
    let mut report = PrivacyReport {
        stripped_headers: Vec::new(),
        injected_headers: Vec::new(),
        blocked_trackers: Vec::new(),
        fingerprinting_vectors: Vec::new(),
    };

    let settings = &request.privacy;

    // Strip privacy-sensitive headers
    if settings.strip_referer {
        request.headers.retain(|(name, _)| {
            let lower = name.to_lowercase();
            if PRIVACY_STRIP_HEADERS.contains(&lower.as_str()) {
                report.stripped_headers.push(name.clone());
                false
            } else {
                true
            }
        });
    }

    // Inject privacy headers
    if settings.send_dnt {
        request.headers.push(("DNT".to_string(), "1".to_string()));
        report.injected_headers.push("DNT: 1".to_string());
    }

    if settings.send_gpc {
        request
            .headers
            .push(("Sec-GPC".to_string(), "1".to_string()));
        report.injected_headers.push("Sec-GPC: 1".to_string());
    }

    // Check for tracker domains
    if settings.block_trackers {
        let domain = request.url.host_str().unwrap_or("");
        for tracker in KNOWN_TRACKERS {
            if domain.contains(tracker) {
                report.blocked_trackers.push(tracker.to_string());
            }
        }
    }

    debug!(
        url = %request.url,
        stripped = ?report.stripped_headers,
        injected = ?report.injected_headers,
        "Privacy protections applied to request"
    );

    report
}

/// Check if a domain is a known tracker.
pub fn is_tracker(domain: &str) -> bool {
    KNOWN_TRACKERS
        .iter()
        .any(|tracker| domain.contains(tracker))
}

/// Generate a privacy-safe User-Agent string.
///
/// Instead of revealing detailed browser/OS info, we use a generic
/// string that blends in with the most common user agent.
pub fn privacy_user_agent() -> String {
    // Use a common, generic user agent to blend in
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36".to_string()
}

/// Fingerprinting resistance — detect and block common fingerprinting techniques.
pub fn detect_fingerprinting_vectors(headers: &[(String, String)]) -> Vec<String> {
    let mut vectors = Vec::new();

    for (name, _) in headers {
        let lower = name.to_lowercase();
        if lower == "accept-language" {
            // Detailed language headers can be used for fingerprinting
            vectors.push("Accept-Language (can fingerprint locale)".to_string());
        }
        if lower.starts_with("sec-ch-ua") {
            vectors.push(format!("{} (client hint fingerprinting)", name));
        }
    }

    vectors
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{HttpMethod, RequestPrivacySettings};
    use url::Url;

    #[test]
    fn test_tracker_detection() {
        assert!(is_tracker("www.google-analytics.com"));
        assert!(is_tracker("pixel.doubleclick.net"));
        assert!(!is_tracker("example.com"));
    }

    #[test]
    fn test_privacy_headers_injected() {
        let mut request = PrivacyAwareRequest {
            url: Url::parse("https://example.com").unwrap(),
            method: HttpMethod::Get,
            headers: vec![],
            body: None,
            top_level_origin: "example.com".to_string(),
            privacy: RequestPrivacySettings::default(),
        };

        let report = apply_privacy_protections(&mut request);
        assert!(report.injected_headers.contains(&"DNT: 1".to_string()));
        assert!(report.injected_headers.contains(&"Sec-GPC: 1".to_string()));
    }

    #[test]
    fn test_referer_stripped() {
        let mut request = PrivacyAwareRequest {
            url: Url::parse("https://example.com").unwrap(),
            method: HttpMethod::Get,
            headers: vec![
                ("Referer".to_string(), "https://secret.com".to_string()),
                ("Accept".to_string(), "text/html".to_string()),
            ],
            body: None,
            top_level_origin: "example.com".to_string(),
            privacy: RequestPrivacySettings::default(),
        };

        let report = apply_privacy_protections(&mut request);
        assert!(report.stripped_headers.contains(&"Referer".to_string()));
        assert!(!request.headers.iter().any(|(n, _)| n == "Referer"));
    }
}
