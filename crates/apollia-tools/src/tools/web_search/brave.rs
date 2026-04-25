//! Brave Search API backend.
//!
//! Uses Brave's public Web Search API — documented, rate-tier'd, and
//! authenticated by an API key delivered via the `X-Subscription-Token`
//! header. Free tier grants 2_000 queries per month as of 2025.
//!
//! API key lookup order (stopping at the first hit):
//! 1. `BRAVE_SEARCH_API_KEY` environment variable
//!
//! Keychain / system secret-store support is deliberately deferred until
//! the desktop UI ships a dedicated "search backend" configuration panel
//! (post-v0.1.0).

use std::time::Duration;

use async_trait::async_trait;
use regex::Regex;
use serde::Deserialize;

use super::backend::{
    SafeSearch, SearchBackend, SearchBackendError, SearchQuery, SearchResult, TimeRange,
};

/// Backend identifier.
pub(crate) const BACKEND_NAME: &str = "brave";

/// Env var consulted by [`BraveBackend::from_env`].
pub const ENV_API_KEY: &str = "BRAVE_SEARCH_API_KEY";

/// Default Brave Search endpoint. Overridable for tests.
const DEFAULT_ENDPOINT: &str = "https://api.search.brave.com/res/v1/web/search";

/// Per-request timeout in seconds.
const REQUEST_TIMEOUT_SECS: u64 = 15;

/// Brave JSON responses are consistently small (tens of KB). Cap at 512 KB.
const MAX_RESPONSE_BYTES: usize = 512 * 1024;

/// Brave Search backend.
pub struct BraveBackend {
    endpoint: String,
    api_key: String,
    client: reqwest::Client,
    timeout_secs: u64,
    max_response_bytes: usize,
    /// Operator-supplied cap on `count`. The per-request `max_results` from the
    /// caller is clamped down to this value before reaching Brave.
    backend_max_results: u32,
}

impl BraveBackend {
    /// Build a backend from a known API key, using the default Brave endpoint
    /// and built-in defaults.
    pub fn new(api_key: impl Into<String>) -> Self {
        Self::build(
            api_key,
            DEFAULT_ENDPOINT,
            REQUEST_TIMEOUT_SECS,
            MAX_RESPONSE_BYTES,
            u32::MAX,
        )
    }

    /// Build a backend from an [`apollia_core::BraveBackendConfig`] and an
    /// already-resolved API key.
    pub fn with_config(cfg: &apollia_core::BraveBackendConfig, api_key: impl Into<String>) -> Self {
        Self::build(
            api_key,
            DEFAULT_ENDPOINT,
            cfg.timeout_secs,
            MAX_RESPONSE_BYTES,
            cfg.max_results as u32,
        )
    }

    /// Build a backend with an arbitrary endpoint — intended for tests hitting
    /// an in-process mock server.
    pub fn with_endpoint(api_key: impl Into<String>, endpoint: impl Into<String>) -> Self {
        Self::build(
            api_key,
            endpoint,
            REQUEST_TIMEOUT_SECS,
            MAX_RESPONSE_BYTES,
            u32::MAX,
        )
    }

    fn build(
        api_key: impl Into<String>,
        endpoint: impl Into<String>,
        timeout_secs: u64,
        max_response_bytes: usize,
        backend_max_results: u32,
    ) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(timeout_secs))
            .build()
            .expect("reqwest::Client initialization is infallible");
        Self {
            endpoint: endpoint.into(),
            api_key: api_key.into(),
            client,
            timeout_secs,
            max_response_bytes,
            backend_max_results,
        }
    }

    /// Build a backend by reading the API key from the
    /// [`ENV_API_KEY`] environment variable.
    ///
    /// # Errors
    ///
    /// Returns [`SearchBackendError::MissingCredential`] when the env var is
    /// unset or empty. The dispatcher treats this as "skip and log info".
    pub fn from_env() -> Result<Self, SearchBackendError> {
        let raw = std::env::var(ENV_API_KEY).ok().unwrap_or_default();
        if raw.trim().is_empty() {
            return Err(SearchBackendError::MissingCredential {
                backend: BACKEND_NAME.to_string(),
                what: ENV_API_KEY.to_string(),
            });
        }
        Ok(Self::new(raw))
    }
}

#[async_trait]
impl SearchBackend for BraveBackend {
    fn name(&self) -> &str {
        BACKEND_NAME
    }

    async fn search(&self, query: &SearchQuery) -> Result<Vec<SearchResult>, SearchBackendError> {
        let effective_max = query.max_results.min(self.backend_max_results).max(1);
        let params = build_params(query, effective_max);

        let response = self
            .client
            .get(&self.endpoint)
            .query(&params)
            .header("X-Subscription-Token", &self.api_key)
            .header("Accept", "application/json")
            .send()
            .await
            .map_err(|e| classify_transport_error(e, self.timeout_secs))?;

        let status = response.status();
        if status == reqwest::StatusCode::UNAUTHORIZED {
            return Err(SearchBackendError::MissingCredential {
                backend: BACKEND_NAME.to_string(),
                what: format!("{ENV_API_KEY} (rejected by Brave — key invalid or revoked)"),
            });
        }
        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            return Err(SearchBackendError::RateLimited {
                backend: BACKEND_NAME.to_string(),
                status: status.as_u16(),
            });
        }
        if status == reqwest::StatusCode::FORBIDDEN {
            return Err(SearchBackendError::Blocked {
                backend: BACKEND_NAME.to_string(),
            });
        }
        if !status.is_success() {
            return Err(SearchBackendError::BadStatus {
                backend: BACKEND_NAME.to_string(),
                status: status.as_u16(),
            });
        }

        let body = read_body_capped(response, self.max_response_bytes, self.timeout_secs).await?;
        parse_brave_json(&body, effective_max)
    }
}

/// Build the query parameters sent to Brave.
fn build_params(query: &SearchQuery, count: u32) -> Vec<(&'static str, String)> {
    let mut params: Vec<(&'static str, String)> = vec![
        ("q", query.query.clone()),
        ("count", count.to_string()),
        ("result_filter", "web".to_string()),
    ];

    params.push((
        "safesearch",
        match query.safe_search {
            SafeSearch::Off => "off".to_string(),
            SafeSearch::Moderate => "moderate".to_string(),
            SafeSearch::Strict => "strict".to_string(),
        },
    ));

    if let Some(region) = query.region.as_ref() {
        // Brave wants an ISO 3166-1 alpha-2 country code. If the caller sent a
        // DDG-style hint like "fr-fr" or "us-en", keep only the first segment
        // and uppercase it. Unknown formats fall through verbatim.
        let country = region
            .split('-')
            .next()
            .unwrap_or(region.as_str())
            .to_uppercase();
        params.push(("country", country));
    }

    if let Some(range) = query.time_range {
        let freshness = match range {
            TimeRange::Day => "pd",
            TimeRange::Week => "pw",
            TimeRange::Month => "pm",
            TimeRange::Year => "py",
        };
        params.push(("freshness", freshness.to_string()));
    }

    params
}

fn classify_transport_error(e: reqwest::Error, timeout_secs: u64) -> SearchBackendError {
    if e.is_timeout() {
        SearchBackendError::Timeout {
            backend: BACKEND_NAME.to_string(),
            timeout_secs,
        }
    } else {
        SearchBackendError::RequestFailed {
            backend: BACKEND_NAME.to_string(),
            message: e.to_string(),
        }
    }
}

async fn read_body_capped(
    response: reqwest::Response,
    max_bytes: usize,
    timeout_secs: u64,
) -> Result<String, SearchBackendError> {
    let mut bytes: Vec<u8> = Vec::new();
    let mut response = response;
    loop {
        match response.chunk().await {
            Ok(Some(chunk)) => {
                bytes.extend_from_slice(&chunk);
                if bytes.len() > max_bytes {
                    return Err(SearchBackendError::BadStatus {
                        backend: BACKEND_NAME.to_string(),
                        status: 0,
                    });
                }
            }
            Ok(None) => break,
            Err(e) => return Err(classify_transport_error(e, timeout_secs)),
        }
    }
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

#[derive(Debug, Deserialize)]
struct BraveEnvelope {
    web: Option<BraveWebBlock>,
}

#[derive(Debug, Deserialize)]
struct BraveWebBlock {
    #[serde(default)]
    results: Vec<BraveResult>,
}

#[derive(Debug, Deserialize)]
struct BraveResult {
    title: Option<String>,
    url: Option<String>,
    #[serde(default)]
    description: String,
    #[serde(default)]
    age: Option<String>,
    #[serde(default)]
    page_age: Option<String>,
}

/// Parse a Brave response body and map it to [`SearchResult`].
pub(crate) fn parse_brave_json(
    body: &str,
    max_results: u32,
) -> Result<Vec<SearchResult>, SearchBackendError> {
    let envelope: BraveEnvelope =
        serde_json::from_str(body).map_err(|e| SearchBackendError::ParseError {
            backend: BACKEND_NAME.to_string(),
            reason: format!("invalid JSON: {e}"),
        })?;

    let web = match envelope.web {
        Some(w) => w,
        None => return Ok(Vec::new()),
    };

    let tag_re = tag_regex();

    let results: Vec<SearchResult> = web
        .results
        .into_iter()
        .take(max_results as usize)
        .enumerate()
        .filter_map(|(idx, r)| {
            let title = r.title?;
            let url = r.url?;
            Some(SearchResult {
                title,
                url,
                snippet: tag_re.replace_all(&r.description, "").into_owned(),
                rank: (idx + 1) as u32,
                age: r.age.or(r.page_age),
            })
        })
        .collect();

    Ok(results)
}

/// Cache the tag-stripping regex — compiled once per call is cheap but we can
/// do better. Using a plain function with a fresh Regex on every call is fine
/// for now; switch to `once_cell::sync::Lazy` if hot-path profiling says so.
fn tag_regex() -> Regex {
    Regex::new(r"<[^>]+>").expect("valid regex literal")
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::sync::Mutex;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    /// Serialises env-var mutations across all tests in this crate that touch
    /// `BRAVE_SEARCH_API_KEY`. `std::env::{set_var, remove_var}` are unsafe
    /// for a reason: parallel tests reading or writing the env race with each
    /// other. Take this lock before any unsafe env access.
    pub(crate) static ENV_LOCK: Mutex<()> = Mutex::new(());

    const FIXTURE: &str = include_str!("tests/fixtures/brave_response.json");

    fn sample_query() -> SearchQuery {
        SearchQuery {
            query: "apollia runtime rust".to_string(),
            max_results: 5,
            region: Some("fr-fr".to_string()),
            safe_search: SafeSearch::Moderate,
            time_range: Some(TimeRange::Week),
        }
    }

    async fn bind_local() -> (TcpListener, u16) {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let port = listener.local_addr().expect("addr").port();
        (listener, port)
    }

    async fn respond_once(listener: TcpListener, response: Vec<u8>) {
        let (mut socket, _) = listener.accept().await.expect("accept");
        let mut buf = vec![0u8; 8192];
        let _ = socket.read(&mut buf).await;
        socket.write_all(&response).await.expect("write");
    }

    #[test]
    fn parses_fixture_response() {
        // GIVEN the canned Brave JSON body
        let results = parse_brave_json(FIXTURE, 20).expect("parse ok");
        // THEN every web result is surfaced with rank starting at 1
        assert_eq!(results.len(), 3);
        assert_eq!(results[0].rank, 1);
        assert_eq!(results[0].url, "https://github.com/nidal-z/apollia-os");
    }

    #[test]
    fn strips_strong_tags_from_description() {
        let results = parse_brave_json(FIXTURE, 20).expect("parse ok");
        // THEN `<strong>` wrappers are gone
        assert!(!results[0].snippet.contains("<strong>"));
        assert!(!results[0].snippet.contains("</strong>"));
        assert!(results[0].snippet.contains("Apollia"));
    }

    #[test]
    fn prefers_age_over_page_age() {
        let results = parse_brave_json(FIXTURE, 20).expect("parse ok");
        // First result has both `age` and `page_age` — human-readable `age` wins.
        assert_eq!(results[0].age.as_deref(), Some("2 days ago"));
        // Second result has only `page_age`.
        assert_eq!(results[1].age.as_deref(), Some("2026-03-10T00:00:00"));
        // Third result has neither.
        assert!(results[2].age.is_none());
    }

    #[test]
    fn honours_max_results_cap() {
        let results = parse_brave_json(FIXTURE, 2).expect("parse ok");
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn empty_web_block_returns_empty_vec() {
        let body = r#"{"type":"search","query":{},"web":{"type":"search","results":[]}}"#;
        let results = parse_brave_json(body, 10).expect("parse ok");
        assert!(results.is_empty());
    }

    #[test]
    fn missing_web_block_returns_empty_vec() {
        let body = r#"{"type":"search","query":{}}"#;
        let results = parse_brave_json(body, 10).expect("parse ok");
        assert!(results.is_empty());
    }

    #[test]
    fn invalid_json_returns_parse_error() {
        let err = parse_brave_json("{ not json", 10).expect_err("should fail");
        assert!(
            matches!(err, SearchBackendError::ParseError { ref backend, .. } if backend == BACKEND_NAME)
        );
    }

    #[test]
    fn from_env_fails_when_unset() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let prev = std::env::var(ENV_API_KEY).ok();
        // SAFETY: the file-wide ENV_LOCK serialises mutation across tests.
        unsafe {
            std::env::remove_var(ENV_API_KEY);
        }

        let result = BraveBackend::from_env();
        match result {
            Err(SearchBackendError::MissingCredential { ref what, .. }) if what == ENV_API_KEY => {}
            Err(other) => panic!("expected MissingCredential, got: {other}"),
            Ok(_) => panic!("expected MissingCredential when env var unset"),
        }

        if let Some(v) = prev {
            unsafe {
                std::env::set_var(ENV_API_KEY, v);
            }
        }
    }

    #[test]
    fn build_params_translates_region_and_freshness() {
        let params = build_params(&sample_query(), sample_query().max_results);
        // region "fr-fr" → country "FR"
        assert!(params.iter().any(|(k, v)| *k == "country" && v == "FR"));
        // time_range Week → freshness pw
        assert!(params.iter().any(|(k, v)| *k == "freshness" && v == "pw"));
        // result_filter=web is always set
        assert!(params
            .iter()
            .any(|(k, v)| *k == "result_filter" && v == "web"));
    }

    #[tokio::test]
    async fn network_happy_path_returns_results() {
        let (listener, port) = bind_local().await;
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
            FIXTURE.len(),
            FIXTURE
        );
        tokio::spawn(respond_once(listener, response.into_bytes()));

        let backend = BraveBackend::with_endpoint(
            "test-key",
            format!("http://127.0.0.1:{port}/res/v1/web/search"),
        );

        let results = backend.search(&sample_query()).await.expect("ok");
        assert_eq!(results.len(), 3);
    }

    #[tokio::test]
    async fn network_401_maps_to_missing_credential() {
        let (listener, port) = bind_local().await;
        let response = b"HTTP/1.1 401 Unauthorized\r\nContent-Length: 0\r\n\r\n".to_vec();
        tokio::spawn(respond_once(listener, response));

        let backend =
            BraveBackend::with_endpoint("bad-key", format!("http://127.0.0.1:{port}/search"));

        let err = backend.search(&sample_query()).await.expect_err("401");
        assert!(matches!(err, SearchBackendError::MissingCredential { .. }));
    }

    #[tokio::test]
    async fn network_429_maps_to_rate_limited() {
        let (listener, port) = bind_local().await;
        let response = b"HTTP/1.1 429 Too Many Requests\r\nContent-Length: 0\r\n\r\n".to_vec();
        tokio::spawn(respond_once(listener, response));

        let backend =
            BraveBackend::with_endpoint("test-key", format!("http://127.0.0.1:{port}/search"));

        let err = backend.search(&sample_query()).await.expect_err("429");
        assert!(matches!(
            err,
            SearchBackendError::RateLimited { status: 429, .. }
        ));
    }
}
