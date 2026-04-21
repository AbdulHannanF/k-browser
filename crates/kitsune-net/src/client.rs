/// HTTP client — privacy-first networking for KitsuneEngine.

use crate::error::{NetError, NetResult};
use crate::privacy;
use crate::{HttpResponse, PrivacyAwareRequest, PrivacyReport, RequestPrivacySettings};
use tracing::{debug, info, warn};
use std::sync::{Arc, Mutex};
use std::collections::HashMap;

use std::path::PathBuf;

fn serve_internal_page(path: &str, url: &url::Url) -> NetResult<HttpResponse> {
    let clean_path = path.trim_start_matches('/');
    let body = match clean_path {
        "welcome" | "" => include_bytes!("../../kitsune-ui/assets/pages/welcome.html").to_vec(),
        "demo/shop" => include_bytes!("../../kitsune-ui/assets/pages/demo/shop.html").to_vec(),
        "privacy" => include_bytes!("../../kitsune-ui/assets/pages/privacy.html").to_vec(),
        _ => return Ok(HttpResponse {
            status: 404,
            headers: vec![("Content-Type".to_string(), "text/html".to_string())],
            body: b"<html><head><title>Not Found</title></head><body style=\"background:#0d0f14;color:#e2e8f0;font-family:sans-serif;text-align:center;padding:50px\"><h1>404 Not Found</h1><p>Internal page not found</p></body></html>".to_vec(),
            final_url: url.clone(),
            is_secure: true,
            is_internal: true,
            privacy_report: PrivacyReport {
                stripped_headers: vec![],
                injected_headers: vec![],
                blocked_trackers: vec![],
                fingerprinting_vectors: vec![],
            },
        }),
    };

    Ok(HttpResponse {
        status: 200,
        headers: vec![("Content-Type".to_string(), "text/html".to_string())],
        body,
        final_url: url.clone(),
        is_secure: true,
        is_internal: true,
        privacy_report: PrivacyReport {
            stripped_headers: vec![],
            injected_headers: vec![],
            blocked_trackers: vec![],
            fingerprinting_vectors: vec![],
        },
    })
}

/// The KitsuneEngine HTTP client.
pub struct KitsuneHttpClient {
    /// Shared reqwest client for connection reuse and HTTP/2
    inner: reqwest::Client,
    /// Directory for HTTP response caching
    cache_dir: Option<PathBuf>,
    pub cookie_jar: Arc<PartitionedCookieJar>,
    pub privacy_settings: RequestPrivacySettings,
}

pub struct PartitionedCookieJar {
    inner: Mutex<HashMap<(String, String), cookie_store::CookieStore>>,
}

impl PartitionedCookieJar {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(HashMap::new()),
        }
    }

    pub fn get_cookies(&self, url: &url::Url, top_level_origin: &str) -> Option<String> {
        let inner = self.inner.lock().unwrap();
        let key = (url.origin().ascii_serialization(), top_level_origin.to_string());
        if let Some(store) = inner.get(&key) {
            let cookies = store.matches(url).into_iter().map(|c| format!("{}={}", c.name(), c.value())).collect::<Vec<_>>();
            if !cookies.is_empty() {
                return Some(cookies.join("; "));
            }
        }
        None
    }

    pub fn store_cookie(&self, url: &url::Url, top_level_origin: &str, cookie_str: &str) {
        let mut inner = self.inner.lock().unwrap();
        let key = (url.origin().ascii_serialization(), top_level_origin.to_string());
        let store = inner.entry(key).or_insert_with(|| cookie_store::CookieStore::default());
        let _ = store.parse(cookie_str, url);
    }
}

impl KitsuneHttpClient {
    /// Create a new HTTP client with privacy protections enabled.
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .use_rustls_tls()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap_or_default();
            
        let cache_dir = dirs::data_dir().map(|d| {
            let p = d.join("kitsune").join("cache");
            let _ = std::fs::create_dir_all(&p);
            p
        });

        Self {
            inner: client,
            cache_dir,
            cookie_jar: Arc::new(PartitionedCookieJar::new()),
            privacy_settings: RequestPrivacySettings::default(),
        }
    }

    /// Send a privacy-aware HTTP request.
    pub async fn send(&self, mut request: PrivacyAwareRequest) -> NetResult<HttpResponse> {
        if request.url.scheme() == "kitsune" {
            let host = request.url.host_str().unwrap_or("");
            let path = request.url.path();
            let full_path = format!("{}{}", host, path);
            return serve_internal_page(&full_path, &request.url);
        }

        let mut current_url = request.url.clone();
        let top_level_origin = if request.top_level_origin.is_empty() {
            current_url.origin().ascii_serialization()
        } else {
            request.top_level_origin.clone()
        };
        let mut redirect_count = 0;
        let mut final_privacy_report: Option<PrivacyReport> = None;

        loop {
            // Apply privacy protections unconditionally
            request.url = current_url.clone();
            let privacy_report = privacy::apply_privacy_protections(&mut request);

            // Merge privacy report
            if let Some(ref mut pr) = final_privacy_report {
                pr.stripped_headers.extend(privacy_report.stripped_headers.clone());
                pr.injected_headers.extend(privacy_report.injected_headers.clone());
                pr.blocked_trackers.extend(privacy_report.blocked_trackers.clone());
                pr.fingerprinting_vectors.extend(privacy_report.fingerprinting_vectors.clone());
            } else {
                final_privacy_report = Some(privacy_report.clone());
            }

            // Block tracker domains
            if !privacy_report.blocked_trackers.is_empty() {
                warn!(
                    trackers = ?privacy_report.blocked_trackers,
                    "Blocked request to tracker domain"
                );
                return Err(NetError::TrackerBlocked {
                    domain: request.url.host_str().unwrap_or("unknown").to_string(),
                });
            }

            // Set privacy-safe User-Agent
            if !request.headers.iter().any(|(n, _)| n.to_lowercase() == "user-agent") {
                request.headers.push(("User-Agent".to_string(), privacy::privacy_user_agent()));
            }

            // Inject cookies
            if let Some(cookie_header) = self.cookie_jar.get_cookies(&current_url, &top_level_origin) {
                request.headers.retain(|(n, _)| !n.eq_ignore_ascii_case("cookie"));
                request.headers.push(("Cookie".to_string(), cookie_header));
            }

            let is_get = matches!(request.method, crate::HttpMethod::Get);
            let cache_key = if is_get {
                use sha2::{Digest, Sha256};
                let mut hasher = Sha256::new();
                hasher.update(request.url.as_str().as_bytes());
                Some(format!("{:x}", hasher.finalize()))
            } else {
                None
            };

            if let (Some(dir), Some(key)) = (&self.cache_dir, &cache_key) {
                let cache_path = dir.join(key);
                if cache_path.exists() {
                    if let Ok(meta) = std::fs::metadata(&cache_path) {
                        if let Ok(mod_time) = meta.modified() {
                            if let Ok(cached_json) = std::fs::read_to_string(&cache_path) {
                                if let Ok(mut cached_resp) = serde_json::from_str::<HttpResponse>(&cached_json) {
                                    let mut max_age = 0;
                                    for (k, v) in &cached_resp.headers {
                                        if k.eq_ignore_ascii_case("cache-control") {
                                            if let Some(pos) = v.find("max-age=") {
                                                if let Ok(age) = v[pos + 8..].split(',').next().unwrap_or("0").trim().parse::<u64>() {
                                                    max_age = age;
                                                }
                                            }
                                        }
                                    }
                                    if max_age > 0 {
                                        if let Ok(elapsed) = mod_time.elapsed() {
                                            if elapsed.as_secs() < max_age {
                                                info!(url = %request.url, "Cache hit");
                                                if let Some(ref pr) = final_privacy_report {
                                                    cached_resp.privacy_report = pr.clone();
                                                }
                                                return Ok(cached_resp);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            info!(
                url = %request.url,
                method = %request.method,
                "Sending privacy-protected HTTP request"
            );

            // Build a real reqwest request
            let mut req_builder = match request.method {
                crate::HttpMethod::Get => self.inner.get(request.url.as_str()),
                crate::HttpMethod::Post => self.inner.post(request.url.as_str()),
                crate::HttpMethod::Put => self.inner.put(request.url.as_str()),
                crate::HttpMethod::Delete => self.inner.delete(request.url.as_str()),
                crate::HttpMethod::Patch => self.inner.patch(request.url.as_str()),
                crate::HttpMethod::Head => self.inner.head(request.url.as_str()),
                crate::HttpMethod::Options => self.inner.request(reqwest::Method::OPTIONS, request.url.as_str()),
            };

            // Apply headers
            for (name, value) in &request.headers {
                req_builder = req_builder.header(name.as_str(), value.as_str());
            }

            // Apply body
            if let Some(ref body) = request.body {
                req_builder = req_builder.body(body.clone());
            }

            // Send the request
            let response = match tokio::time::timeout(std::time::Duration::from_secs(10), req_builder.send()).await {
                Ok(Ok(res)) => res,
                Ok(Err(e)) => {
                    warn!(error = %e, "HTTP request failed");
                    let error_msg = e.to_string();
                    let html_body = if e.is_timeout() {
                        b"<html><body><h1>Page is taking too long.</h1><p>[Stop] [Retry]</p></body></html>".to_vec()
                    } else if error_msg.contains("dns") || error_msg.contains("resolve") {
                        format!("<html><body><h1>Can't find {}. Check the address and try again.</h1></body></html>", request.url.host_str().unwrap_or("domain")).into_bytes()
                    } else if error_msg.contains("tls") || error_msg.contains("certificate") {
                        b"<html><body><h1>Connection not private.</h1><p>[Details] [Go back]</p></body></html>".to_vec()
                    } else {
                        b"<html><body><h1>Connection Error</h1></body></html>".to_vec()
                    };
                    
                    return Ok(HttpResponse {
                        status: 500,
                        headers: vec![("Content-Type".to_string(), "text/html".to_string())],
                        body: html_body,
                        final_url: request.url.clone(),
                        is_secure: false,
                        is_internal: false,
                        privacy_report: final_privacy_report.unwrap_or(privacy_report),
                    });
                },
                Err(_) => {
                    // Timeout
                    return Ok(HttpResponse {
                        status: 504,
                        headers: vec![("Content-Type".to_string(), "text/html".to_string())],
                        body: b"<html><body><h1>Page is taking too long.</h1><p>[Stop] [Retry]</p></body></html>".to_vec(),
                        final_url: request.url.clone(),
                        is_secure: false,
                        is_internal: false,
                        privacy_report: final_privacy_report.unwrap_or(privacy_report),
                    });
                }
            };

            let status = response.status().as_u16();

            // Handle Set-Cookie
            for (k, v) in response.headers() {
                if k.as_str().eq_ignore_ascii_case("set-cookie") {
                    if let Ok(s) = v.to_str() {
                        self.cookie_jar.store_cookie(&current_url, &top_level_origin, s);
                    }
                }
            }

            // Redirect handling
            if status >= 300 && status < 400 {
                if let Some(location) = response.headers().get("location") {
                    if let Ok(loc_str) = location.to_str() {
                        if let Ok(next_url) = current_url.join(loc_str) {
                            redirect_count += 1;
                            if redirect_count > 10 {
                                return Err(NetError::ConnectionFailed("Too many redirects".to_string()));
                            }
                            
                            let cross_origin = current_url.origin() != next_url.origin();
                            current_url = next_url;
                            
                            // Strip sensitive headers on cross-origin redirect
                            if cross_origin {
                                request.headers.retain(|(n, _)| {
                                    let lowercase = n.to_lowercase();
                                    if lowercase == "authorization" || lowercase == "cookie" || lowercase == "proxy-authorization" {
                                        if let Some(ref mut pr) = final_privacy_report {
                                            pr.stripped_headers.push(n.clone());
                                        }
                                        false
                                    } else {
                                        true
                                    }
                                });
                            }
                            continue;
                        }
                    }
                }
            }

            let final_url = url::Url::parse(response.url().as_str())
                .unwrap_or_else(|_| current_url.clone());
            let is_secure = final_url.scheme() == "https";

            // Collect response headers
            let resp_headers: Vec<(String, String)> = response
                .headers()
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or("").to_string()))
                .collect();

            // Handle error status codes with custom pages
            let body = if status == 404 {
                b"<html><body><h1>Page not found</h1><p>KitsuneEngine</p></body></html>".to_vec()
            } else if status >= 500 {
                b"<html><body><h1>Server error. Try again later.</h1></body></html>".to_vec()
            } else {
                // Read body bytes
                response.bytes().await.map_err(|e| {
                    NetError::ConnectionFailed(format!("Failed to read response body: {}", e))
                })?.to_vec()
            };

            info!(
                status,
                body_len = body.len(),
                url = %final_url,
                "HTTP response received"
            );

            let resp = HttpResponse {
                status,
                headers: resp_headers,
                body: body.to_vec(),
                final_url,
                is_secure,
                is_internal: false,
                privacy_report: final_privacy_report.unwrap_or(privacy_report),
            };

            // Save to cache
            if let (Some(dir), Some(key)) = (&self.cache_dir, &cache_key) {
                let mut cacheable = false;
                for (k, v) in &resp.headers {
                    if k.eq_ignore_ascii_case("cache-control") && v.contains("max-age=") && !v.contains("max-age=0") {
                        cacheable = true;
                        break;
                    }
                }
                if cacheable {
                    let cache_path = dir.join(key);
                    if let Ok(json) = serde_json::to_string(&resp) {
                        let _ = std::fs::write(cache_path, json);
                    }
                }
            }
            
            return Ok(resp);
        }
    }

    pub async fn get(&self, url: url::Url) -> NetResult<HttpResponse> {
        let top_level_origin = url.origin().ascii_serialization();
        let request = PrivacyAwareRequest {
            url,
            method: crate::HttpMethod::Get,
            headers: vec![],
            body: None,
            top_level_origin,
            privacy: crate::RequestPrivacySettings::default(),
        };
        self.send(request).await
    }

    /// Prefetch DNS for the given domain.
    pub fn prefetch_dns(&self, domain: &str) {
        // Apply privacy protections unconditionally
        if crate::privacy::is_tracker(domain) {
            warn!(domain, "Blocked DNS prefetch for tracker domain");
            return;
        }

        let domain = domain.to_string();
        tokio::spawn(async move {
            let addr = format!("{}:80", domain);
            if let Ok(addrs) = tokio::net::lookup_host(addr).await {
                debug!(domain = %domain, addrs_count = addrs.count(), "DNS prefetch complete");
            }
        });
    }
}

impl Default for KitsuneHttpClient {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_internal_url_serves_welcome() {
        let client = KitsuneHttpClient::new();
        let resp = client.get(url::Url::parse("kitsune://welcome").unwrap()).await.unwrap();
        assert_eq!(resp.status, 200);
        assert!(resp.is_internal);
        let body = String::from_utf8_lossy(&resp.body);
        assert!(body.contains("KitsuneEngine"));
    }

    #[tokio::test]
    async fn test_internal_url_serves_shop() {
        let client = KitsuneHttpClient::new();
        let resp = client.get(url::Url::parse("kitsune://demo/shop").unwrap()).await.unwrap();
        assert_eq!(resp.status, 200);
        assert!(resp.is_internal);
        let body = String::from_utf8_lossy(&resp.body);
        assert!(body.contains("add-cart-btn-1"));
    }

    #[tokio::test]
    async fn test_unknown_internal_url() {
        let client = KitsuneHttpClient::new();
        let resp = client.get(url::Url::parse("kitsune://nonexistent").unwrap()).await.unwrap();
        assert_eq!(resp.status, 404);
        assert!(resp.is_internal);
    }

    #[tokio::test]
    async fn test_http_url_not_intercepted() {
        let client = KitsuneHttpClient::new();
        let result = client.get(url::Url::parse("http://example.com").unwrap()).await;
        if let Ok(resp) = result {
            assert!(!resp.is_internal);
        }
    }
}
