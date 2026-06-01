//! DuckDuckGo HTML-endpoint backend.
//!
//! Posts to `https://html.duckduckgo.com/html/` (DDG's no-JS endpoint) and
//! parses the response with CSS selectors. The endpoint is undocumented; expect
//! to revisit selectors every 6–12 months. The failure taxonomy below
//! (blocked vs rate-limited vs parse-error) is designed to let agents decide
//! whether to retry, switch backend, or abort.

use std::time::Duration;

use apollia_core::DuckDuckGoBackendConfig;
use async_trait::async_trait;
use scraper::{Html, Selector};

use super::backend::{
    SafeSearch, SearchBackend, SearchBackendError, SearchQuery, SearchResult, TimeRange,
};

/// Backend name used in error attribution and [`SearchBackend::name`].
pub(crate) const BACKEND_NAME: &str = "duckduckgo";

/// Default DDG HTML endpoint. Overridable via [`DuckDuckGoBackend::with_endpoint`] for tests.
const DEFAULT_ENDPOINT: &str = "https://html.duckduckgo.com/html/";

/// Default per-request timeout in seconds when no [`DuckDuckGoBackendConfig`] is provided.
const REQUEST_TIMEOUT_SECS: u64 = 15;

/// Default maximum response body size when no [`DuckDuckGoBackendConfig`] is provided.
/// DDG pages are ~50 KB; 1 MB is ample head-room and guards against memory exhaustion
/// if the endpoint misbehaves.
const MAX_RESPONSE_BYTES: usize = 1_048_576;

/// Text-browser UA. Since 2025 DDG's WAF returns HTTP 202 + an `anomaly.js`
/// challenge for *every* mainstream browser UA (Firefox/Chrome on Linux, macOS
/// and Windows have all been verified blocked), while text-browser UAs such as
/// `w3m`, `lynx`, and `links` still receive HTTP 200 with real result rows.
/// Empty/missing UA → 403. Re-evaluate quarterly: if DDG starts challenging
/// w3m too, the next stable target is `lynx/2.9.0 libwww-FM/2.14`.
const USER_AGENT: &str = "w3m/0.5.3";

/// DuckDuckGo HTML-endpoint backend, the zero-config default.
pub struct DuckDuckGoBackend {
    endpoint: String,
    client: reqwest::Client,
    timeout_secs: u64,
    max_response_bytes: usize,
}

impl DuckDuckGoBackend {
    /// Create a backend hitting the public DDG HTML endpoint with built-in defaults.
    ///
    /// # Panics
    ///
    /// Panics only if the platform TLS backend refuses to initialise, which
    /// does not occur on supported targets.
    pub fn new() -> Self {
        Self::build(DEFAULT_ENDPOINT, REQUEST_TIMEOUT_SECS, MAX_RESPONSE_BYTES)
    }

    /// Build a backend honouring the operator-supplied [`DuckDuckGoBackendConfig`].
    pub fn with_config(cfg: &DuckDuckGoBackendConfig) -> Self {
        let max_response_bytes = (cfg.max_response_kb as usize).saturating_mul(1024);
        Self::build(DEFAULT_ENDPOINT, cfg.timeout_secs, max_response_bytes)
    }

    /// Build a backend that targets an arbitrary URL, intended for integration
    /// tests hitting an in-process mock server.
    pub fn with_endpoint(endpoint: impl Into<String>) -> Self {
        Self::build(endpoint, REQUEST_TIMEOUT_SECS, MAX_RESPONSE_BYTES)
    }

    fn build(endpoint: impl Into<String>, timeout_secs: u64, max_response_bytes: usize) -> Self {
        let client = reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .timeout(Duration::from_secs(timeout_secs))
            .build()
            .expect("reqwest::Client initialization is infallible");
        Self {
            endpoint: endpoint.into(),
            client,
            timeout_secs,
            max_response_bytes,
        }
    }
}

impl Default for DuckDuckGoBackend {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SearchBackend for DuckDuckGoBackend {
    fn name(&self) -> &str {
        BACKEND_NAME
    }

    async fn search(&self, query: &SearchQuery) -> Result<Vec<SearchResult>, SearchBackendError> {
        let form = build_form(query);

        let response = self
            .client
            .post(&self.endpoint)
            .form(&form)
            .send()
            .await
            .map_err(|e| classify_transport_error(e, self.timeout_secs))?;

        let status = response.status();
        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            return Err(SearchBackendError::RateLimited {
                backend: BACKEND_NAME.to_string(),
                status: status.as_u16(),
            });
        }
        if status == reqwest::StatusCode::FORBIDDEN || status == reqwest::StatusCode::ACCEPTED {
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
        parse_ddg_html(&body, query.max_results)
    }
}

/// Build the form payload posted to the DDG HTML endpoint.
fn build_form(query: &SearchQuery) -> Vec<(&'static str, String)> {
    let mut form: Vec<(&'static str, String)> = vec![("q", query.query.clone())];

    if let Some(region) = query.region.as_ref() {
        form.push(("kl", region.clone()));
    }

    match query.safe_search {
        SafeSearch::Strict => form.push(("kp", "1".to_string())),
        SafeSearch::Moderate => form.push(("kp", "-1".to_string())),
        SafeSearch::Off => form.push(("kp", "-2".to_string())),
    }

    if let Some(range) = query.time_range {
        let df = match range {
            TimeRange::Day => "d",
            TimeRange::Week => "w",
            TimeRange::Month => "m",
            TimeRange::Year => "y",
        };
        form.push(("df", df.to_string()));
    }

    form
}

/// Map a reqwest transport error to the appropriate [`SearchBackendError`].
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

/// Stream the response body, aborting once *max_bytes* is exceeded.
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

/// Minimum body length (bytes) a legitimate "no results" page must exceed. Anything
/// smaller is treated as a Cloudflare / WAF challenge rather than a real empty result.
const MIN_LEGITIMATE_BODY_BYTES: usize = 5 * 1024;

/// Parse a DDG HTML response into a vector of [`SearchResult`].
///
/// The caller has already validated the HTTP status (see
/// `duckduckgo::classify_status_body` at the network layer). This function
/// assumes *body came from a 200-OK response* and classifies it accordingly.
///
/// # Errors
///
/// * [`SearchBackendError::Blocked`] when the response is short and looks like
///   a WAF challenge (captcha, anomaly marker, forced refresh).
/// * [`SearchBackendError::ParseError`] when the body is long enough to be
///   plausible but contains no result rows and no explicit `no-results` marker.
/// * `Ok(vec![])` when the response contains an explicit `no-results` marker.
pub(crate) fn parse_ddg_html(
    body: &str,
    max_results: u32,
) -> Result<Vec<SearchResult>, SearchBackendError> {
    // Cheap signal check before firing the selector engine.
    if looks_blocked(body) {
        return Err(SearchBackendError::Blocked {
            backend: BACKEND_NAME.to_string(),
        });
    }

    let document = Html::parse_document(body);

    // Unwrap is fine: these selectors are compile-time constants, authored and tested here.
    let result_sel = Selector::parse("div.result").expect("valid selector");
    let title_sel = Selector::parse("a.result__a").expect("valid selector");
    let snippet_sel = Selector::parse("a.result__snippet").expect("valid selector");
    let no_results_sel = Selector::parse("div.no-results").expect("valid selector");

    let mut results: Vec<SearchResult> = Vec::new();
    let mut rank: u32 = 1;

    for row in document.select(&result_sel) {
        if results.len() >= max_results as usize {
            break;
        }

        // Title + URL come from the .result__a anchor. Skip rows where it's missing.
        let Some(title_node) = row.select(&title_sel).next() else {
            continue;
        };
        let title = clean_text(&title_node.text().collect::<String>());
        let Some(href) = title_node.value().attr("href") else {
            continue;
        };
        let Some(target_url) = extract_target_url(href) else {
            continue;
        };

        // Snippets are optional on some result types (e.g. direct answers).
        let snippet = row
            .select(&snippet_sel)
            .next()
            .map(|n| clean_text(&n.text().collect::<String>()))
            .unwrap_or_default();

        results.push(SearchResult {
            title,
            url: target_url,
            snippet,
            rank,
            age: None,
        });
        rank += 1;
    }

    if !results.is_empty() {
        return Ok(results);
    }

    // Zero rows: disambiguate between "legitimate empty page" and "selector drift".
    if document.select(&no_results_sel).next().is_some() {
        return Ok(Vec::new());
    }

    // Short body with no marker → treat as block (anomaly text was absent in the
    // cheap signal check, but the page still has no results and no rationale).
    if body.len() < MIN_LEGITIMATE_BODY_BYTES {
        return Err(SearchBackendError::Blocked {
            backend: BACKEND_NAME.to_string(),
        });
    }

    // Long body, zero results, no marker → the selectors drifted.
    Err(SearchBackendError::ParseError {
        backend: BACKEND_NAME.to_string(),
        reason: "no div.result rows and no div.no-results marker".to_string(),
    })
}

/// Heuristic: return `true` when *body* looks like a WAF/captcha challenge
/// rather than a real DDG results page.
fn looks_blocked(body: &str) -> bool {
    let lowered = body.to_ascii_lowercase();
    if lowered.contains("anomaly")
        || lowered.contains("captcha")
        || lowered.contains("please try again")
    {
        return true;
    }
    // Very short body with no useful markup (typical of bot-check pages).
    if body.len() < MIN_LEGITIMATE_BODY_BYTES && !lowered.contains("div.result") {
        // But only flag as blocked when we also see a reload-loop signature.
        if lowered.contains("http-equiv=\"refresh\"") || lowered.contains("meta refresh") {
            return true;
        }
    }
    false
}

/// Decode DDG redirect wrappers of the shape `//duckduckgo.com/l/?uddg=<url>&rut=...`.
///
/// Accepts protocol-relative (`//duckduckgo.com/...`), root-relative (`/l/?...`),
/// and fully-qualified variants. Returns `None` only if *href* is unparseable;
/// already-naked URLs pass through unchanged.
fn extract_target_url(href: &str) -> Option<String> {
    let normalized = if href.starts_with("//") {
        format!("https:{href}")
    } else if let Some(stripped) = href.strip_prefix('/') {
        format!("https://duckduckgo.com/{stripped}")
    } else {
        href.to_string()
    };

    let parsed = url::Url::parse(&normalized).ok()?;

    if parsed.host_str() != Some("duckduckgo.com") {
        // Naked URL (rare, but ads and some direct-answer widgets skip the wrapper).
        return Some(normalized);
    }

    parsed
        .query_pairs()
        .find(|(k, _)| k == "uddg")
        .map(|(_, v)| v.into_owned())
}

/// Normalise whitespace and decode a handful of common HTML entities that
/// `scraper`'s `.text()` iterator does not resolve for attribute text reused
/// as plain strings. We rely on `scraper` for element text (already decoded)
/// but still call this to collapse newlines/tabs to single spaces.
fn clean_text(input: &str) -> String {
    input.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE_RESULTS: &str = include_str!("tests/fixtures/ddg_results.html");
    const FIXTURE_EMPTY: &str = include_str!("tests/fixtures/ddg_empty.html");
    const FIXTURE_BLOCKED: &str = include_str!("tests/fixtures/ddg_blocked.html");

    #[test]
    fn parses_every_row_in_fixture() {
        // GIVEN the canned DDG response
        // WHEN asked for up to 20 results
        let results = parse_ddg_html(FIXTURE_RESULTS, 20).expect("parse ok");
        // THEN every .result is surfaced, with rank starting at 1
        assert_eq!(results.len(), 10);
        assert_eq!(results[0].rank, 1);
        assert_eq!(results[9].rank, 10);
    }

    #[test]
    fn honours_max_results_cap() {
        // GIVEN the fixture containing 10 results
        // WHEN asked for only 3
        let results = parse_ddg_html(FIXTURE_RESULTS, 3).expect("parse ok");
        // THEN we stop after the cap
        assert_eq!(results.len(), 3);
    }

    #[test]
    fn decodes_uddg_redirect_into_target_url() {
        // GIVEN the fixture whose first row wraps its URL in /l/?uddg=...
        let results = parse_ddg_html(FIXTURE_RESULTS, 20).expect("parse ok");
        // THEN the target URL is the decoded original, not the DDG wrapper
        assert_eq!(results[0].url, "https://github.com/Apollia-OS/apollia-os");
        // AND naked URLs pass through untouched
        assert_eq!(results[1].url, "https://www.rust-lang.org/");
    }

    #[test]
    fn strips_html_entities_from_title_and_snippet() {
        // GIVEN the fixture whose third result uses `&amp;` and `&#39;`
        let results = parse_ddg_html(FIXTURE_RESULTS, 20).expect("parse ok");
        let third = &results[2];
        // THEN entities are decoded (scraper `.text()` resolves them)
        assert!(
            third.title.contains("Book & Async"),
            "title: {}",
            third.title
        );
        assert!(
            third.snippet.contains("'The Book'"),
            "snippet: {}",
            third.snippet
        );
    }

    #[test]
    fn handles_missing_snippet_gracefully() {
        // GIVEN the fixture's fourth row has no .result__snippet element
        let results = parse_ddg_html(FIXTURE_RESULTS, 20).expect("parse ok");
        let fourth = &results[3];
        // THEN title is still extracted, snippet is empty
        assert!(fourth.title.contains("Tokio"));
        assert_eq!(fourth.snippet, "");
    }

    #[test]
    fn empty_results_fixture_returns_empty_vec() {
        // GIVEN a fixture with <div class="no-results"> and a >5 KB body
        let results = parse_ddg_html(FIXTURE_EMPTY, 20).expect("parse ok");
        // THEN Ok(empty)
        assert!(results.is_empty());
    }

    #[test]
    fn blocked_fixture_raises_blocked() {
        // GIVEN a short WAF challenge body with `anomaly` and a meta-refresh
        let err = parse_ddg_html(FIXTURE_BLOCKED, 20).expect_err("should detect block");
        // THEN we report Blocked (not ParseError) so the agent falls back
        assert!(
            matches!(err, SearchBackendError::Blocked { ref backend } if backend == BACKEND_NAME),
            "expected Blocked, got: {err}"
        );
    }

    #[test]
    fn long_body_without_markers_is_parse_error() {
        // GIVEN a long body (>5 KB) with no .result and no .no-results
        let body = format!(
            "<!DOCTYPE html><html><body><div class='results'>{}</div></body></html>",
            "x".repeat(10_000)
        );
        // WHEN parsed
        let err = parse_ddg_html(&body, 20).expect_err("should be parse error");
        // THEN ParseError (signals selector drift; not a network issue)
        assert!(
            matches!(err, SearchBackendError::ParseError { ref backend, .. } if backend == BACKEND_NAME),
            "expected ParseError, got: {err}"
        );
    }

    #[test]
    fn extract_target_url_handles_protocol_relative() {
        let decoded =
            extract_target_url("//duckduckgo.com/l/?uddg=https%3A%2F%2Fexample.com%2Fpath&rut=x");
        assert_eq!(decoded.as_deref(), Some("https://example.com/path"));
    }

    #[test]
    fn extract_target_url_handles_root_relative() {
        let decoded = extract_target_url("/l/?uddg=https%3A%2F%2Fexample.com%2Fx");
        assert_eq!(decoded.as_deref(), Some("https://example.com/x"));
    }

    #[test]
    fn extract_target_url_passes_naked_url_through() {
        let decoded = extract_target_url("https://example.com/direct");
        assert_eq!(decoded.as_deref(), Some("https://example.com/direct"));
    }

    #[test]
    fn extract_target_url_rejects_unparseable_input() {
        let decoded = extract_target_url("not a url !!!");
        assert!(decoded.is_none());
    }

    // -----------------------------------------------------------------------
    // Network layer tests: in-process mock server (pattern from http_fetch)
    // -----------------------------------------------------------------------

    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    async fn bind_local() -> (TcpListener, u16) {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let port = listener.local_addr().expect("addr").port();
        (listener, port)
    }

    /// Accept one request, discard it, respond with a pre-formed HTTP/1.1 reply.
    async fn respond_once(listener: TcpListener, response: Vec<u8>) {
        let (mut socket, _) = listener.accept().await.expect("accept");
        let mut buf = vec![0u8; 8192];
        let _ = socket.read(&mut buf).await;
        socket.write_all(&response).await.expect("write");
    }

    fn sample_query() -> SearchQuery {
        SearchQuery {
            query: "apollia runtime".to_string(),
            max_results: 5,
            region: None,
            safe_search: SafeSearch::Moderate,
            time_range: None,
        }
    }

    #[tokio::test]
    async fn network_happy_path_parses_results() {
        // GIVEN a mock DDG endpoint serving the fixture HTML
        let (listener, port) = bind_local().await;
        let body = FIXTURE_RESULTS;
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\n\r\n{}",
            body.len(),
            body
        );
        tokio::spawn(respond_once(listener, response.into_bytes()));

        let backend = DuckDuckGoBackend::with_endpoint(format!("http://127.0.0.1:{port}/html/"));

        // WHEN searching
        let results = backend
            .search(&sample_query())
            .await
            .expect("search should succeed");

        // THEN we get the first 5 results from the fixture
        assert_eq!(results.len(), 5);
        assert_eq!(results[0].rank, 1);
        assert!(results[0].title.contains("Apollia"));
    }

    #[tokio::test]
    async fn network_429_maps_to_rate_limited() {
        // GIVEN a mock endpoint returning 429
        let (listener, port) = bind_local().await;
        let response = b"HTTP/1.1 429 Too Many Requests\r\nContent-Length: 0\r\n\r\n".to_vec();
        tokio::spawn(respond_once(listener, response));

        let backend = DuckDuckGoBackend::with_endpoint(format!("http://127.0.0.1:{port}/html/"));

        // WHEN
        let err = backend
            .search(&sample_query())
            .await
            .expect_err("should be rate-limited");

        // THEN
        assert!(
            matches!(err, SearchBackendError::RateLimited { status: 429, .. }),
            "expected RateLimited(429), got: {err}"
        );
    }

    #[tokio::test]
    async fn network_403_maps_to_blocked() {
        // GIVEN a mock endpoint returning 403 (Cloudflare-style denial)
        let (listener, port) = bind_local().await;
        let response = b"HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\n\r\n".to_vec();
        tokio::spawn(respond_once(listener, response));

        let backend = DuckDuckGoBackend::with_endpoint(format!("http://127.0.0.1:{port}/html/"));

        // WHEN
        let err = backend
            .search(&sample_query())
            .await
            .expect_err("should be blocked");

        // THEN
        assert!(
            matches!(err, SearchBackendError::Blocked { .. }),
            "expected Blocked, got: {err}"
        );
    }

    #[tokio::test]
    async fn network_500_maps_to_bad_status() {
        // GIVEN a mock endpoint returning 500
        let (listener, port) = bind_local().await;
        let response = b"HTTP/1.1 500 Internal Server Error\r\nContent-Length: 0\r\n\r\n".to_vec();
        tokio::spawn(respond_once(listener, response));

        let backend = DuckDuckGoBackend::with_endpoint(format!("http://127.0.0.1:{port}/html/"));

        // WHEN
        let err = backend
            .search(&sample_query())
            .await
            .expect_err("should be bad status");

        // THEN
        assert!(
            matches!(err, SearchBackendError::BadStatus { status: 500, .. }),
            "expected BadStatus(500), got: {err}"
        );
    }

    #[tokio::test]
    async fn network_name_returns_duckduckgo() {
        // GIVEN a backend instance
        let backend = DuckDuckGoBackend::new();
        // THEN the name is the stable identifier used in error attribution
        assert_eq!(backend.name(), BACKEND_NAME);
    }
}
