// ARCHITECTURE: kitsune-net handles all network I/O for KitsuneEngine.
// It runs in a sandboxed network process and enforces privacy at the protocol level:
// - Strips Referer headers by default
// - Injects privacy headers (DNT, Sec-GPC)
// - Blocks fingerprinting vectors at the protocol layer
// - TLS 1.3+ only (no downgrade)
// - Certificate pinning support

pub mod client;
pub mod error;
pub mod privacy;

pub use client::*;
pub use error::{NetError, NetResult};
pub use privacy::*;

use serde::{Deserialize, Serialize};

/// A privacy-aware HTTP request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrivacyAwareRequest {
    /// The URL to fetch.
    pub url: url::Url,
    /// HTTP method.
    pub method: HttpMethod,
    /// Headers (privacy headers are injected automatically).
    pub headers: Vec<(String, String)>,
    /// Request body.
    pub body: Option<Vec<u8>>,
    /// Top level origin for partitioned cookies
    #[serde(default)]
    pub top_level_origin: String,
    /// Privacy settings for this request.
    pub privacy: RequestPrivacySettings,
}

/// HTTP methods.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum HttpMethod {
    Get,
    Post,
    Put,
    Delete,
    Patch,
    Head,
    Options,
}

impl std::fmt::Display for HttpMethod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Get => write!(f, "GET"),
            Self::Post => write!(f, "POST"),
            Self::Put => write!(f, "PUT"),
            Self::Delete => write!(f, "DELETE"),
            Self::Patch => write!(f, "PATCH"),
            Self::Head => write!(f, "HEAD"),
            Self::Options => write!(f, "OPTIONS"),
        }
    }
}

/// Privacy settings for a network request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestPrivacySettings {
    /// Strip the Referer header.
    pub strip_referer: bool,
    /// Send Do Not Track header.
    pub send_dnt: bool,
    /// Send Global Privacy Control header.
    pub send_gpc: bool,
    /// Minimum TLS version (default: 1.3).
    pub min_tls_version: TlsVersion,
    /// Block known tracker domains.
    pub block_trackers: bool,
    /// Randomize request timing to prevent timing analysis.
    pub randomize_timing: bool,
}

impl Default for RequestPrivacySettings {
    fn default() -> Self {
        Self {
            strip_referer: true,
            send_dnt: true,
            send_gpc: true,
            min_tls_version: TlsVersion::Tls13,
            block_trackers: true,
            randomize_timing: false,
        }
    }
}

/// TLS version.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TlsVersion {
    Tls12,
    Tls13,
}

/// An HTTP response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpResponse {
    /// HTTP status code.
    pub status: u16,
    /// Response headers.
    pub headers: Vec<(String, String)>,
    /// Response body.
    pub body: Vec<u8>,
    /// The final URL after redirects.
    pub final_url: url::Url,
    /// Whether the connection used TLS.
    pub is_secure: bool,
    /// Whether this is an internal KitsuneEngine page.
    #[serde(default)]
    pub is_internal: bool,
    /// Privacy report for this response.
    pub privacy_report: PrivacyReport,
}

/// Privacy report for a network response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrivacyReport {
    /// Headers that were stripped from the request.
    pub stripped_headers: Vec<String>,
    /// Privacy headers that were injected.
    pub injected_headers: Vec<String>,
    /// Tracker domains that were blocked.
    pub blocked_trackers: Vec<String>,
    /// Fingerprinting vectors detected.
    pub fingerprinting_vectors: Vec<String>,
}
