//! HTTP helper shared by all connector implementations.
//!
//! Wraps `reqwest::Client` with the policies every native connector needs:
//! - Exponential backoff on 5xx and network errors (3 attempts).
//! - Honour `Retry-After` on 429, capped at a reasonable ceiling.
//! - Refresh-once on 401: invoke a caller-provided closure to obtain a fresh
//!   bearer token, then retry the request once. A second 401 surfaces as
//!   [`ConnectorError::Unauthorized`].
//!
//! Each connector owns one [`HttpClient`]; the client is cheap to clone
//! (internally an `Arc<reqwest::Client>`).

use std::time::Duration;

use reqwest::{header::HeaderMap, Method, Response};
use serde::de::DeserializeOwned;

use crate::error::ConnectorError;

/// Default per-attempt timeout. Connector-specific overrides via the builder.
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

/// Maximum retries on transient failures (5xx, network).
const MAX_RETRIES: u32 = 3;

/// Backoff cap to avoid waiting forever on a 429 with a giant `Retry-After`.
const MAX_BACKOFF: Duration = Duration::from_secs(30);

/// A JSON-bodied request descriptor passed to [`HttpClient::json_request`].
///
/// Groups the request shape (method, url, body) so callers don't thread a
/// long positional argument list; auth (`bearer` + `refresh`) stays separate.
pub struct JsonRequest<'a, B: ?Sized> {
    /// HTTP method.
    pub method: Method,
    /// Target URL.
    pub url: &'a str,
    /// Request body, serialised to JSON.
    pub body: &'a B,
}

/// A raw-bytes request descriptor passed to [`HttpClient::send`].
///
/// Groups the request shape (method, url, body bytes, optional content type)
/// so callers don't thread a long positional argument list; auth
/// (`bearer` + `refresh`) stays separate.
pub struct RawRequest<'a> {
    /// HTTP method.
    pub method: Method,
    /// Target URL.
    pub url: &'a str,
    /// Optional raw request body bytes.
    pub body: Option<Vec<u8>>,
    /// Explicit `Content-Type` override; `None` defaults to
    /// `application/json` when a body is present.
    pub content_type: Option<&'a str>,
}

/// Shared HTTP client for connector implementations.
#[derive(Clone)]
pub struct HttpClient {
    inner: reqwest::Client,
    provider_id: &'static str,
}

impl HttpClient {
    /// Build a client bound to a provider id (for telemetry and error mapping).
    pub fn new(provider_id: &'static str) -> Result<Self, ConnectorError> {
        let inner = apollia_core::net::safe_client_builder()
            .timeout(DEFAULT_TIMEOUT)
            .build()
            .map_err(|e| ConnectorError::Network(e.to_string()))?;
        Ok(Self { inner, provider_id })
    }

    /// The provider identifier this client is bound to.
    pub fn provider_id(&self) -> &'static str {
        self.provider_id
    }

    /// Execute a GET against `url` with bearer auth and JSON-decoded response.
    ///
    /// `refresh` is invoked when the upstream returns 401 to obtain a fresh
    /// bearer token; the request is retried exactly once after refresh.
    pub async fn get_json<T, F, Fut>(
        &self,
        url: &str,
        bearer: &str,
        refresh: F,
    ) -> Result<T, ConnectorError>
    where
        T: DeserializeOwned,
        F: FnOnce() -> Fut + Send,
        Fut: std::future::Future<Output = Result<String, ConnectorError>> + Send,
    {
        let response = self
            .send(
                RawRequest {
                    method: Method::GET,
                    url,
                    body: None,
                    content_type: None,
                },
                bearer,
                refresh,
            )
            .await?;
        deserialize_json(self.provider_id, response).await
    }

    /// Execute a JSON-bodied request (POST/PUT/PATCH/DELETE) and decode the response.
    pub async fn json_request<B, T, F, Fut>(
        &self,
        request: JsonRequest<'_, B>,
        bearer: &str,
        refresh: F,
    ) -> Result<T, ConnectorError>
    where
        B: serde::Serialize + ?Sized,
        T: DeserializeOwned,
        F: FnOnce() -> Fut + Send,
        Fut: std::future::Future<Output = Result<String, ConnectorError>> + Send,
    {
        let body_bytes = serde_json::to_vec(request.body)
            .map_err(|e| ConnectorError::Decoding(e.to_string()))?;
        let response = self
            .send(
                RawRequest {
                    method: request.method,
                    url: request.url,
                    body: Some(body_bytes),
                    content_type: None,
                },
                bearer,
                refresh,
            )
            .await?;
        deserialize_json(self.provider_id, response).await
    }

    /// Execute a request and return the raw response, useful for streaming
    /// downloads where the caller drives the body. The 401-refresh and 429
    /// policies still apply.
    ///
    /// When [`RawRequest::content_type`] is `Some`, it replaces the default
    /// `application/json` applied to bodied requests. Drive's `uploadType=media`
    /// PATCH needs the real upstream MIME (`text/plain`, `text/markdown`, etc.) so
    /// Google doesn't auto-detect a wrong type from the bytes. When `None`,
    /// bodied requests default to `Content-Type: application/json`.
    pub async fn send<F, Fut>(
        &self,
        request: RawRequest<'_>,
        bearer: &str,
        refresh: F,
    ) -> Result<Response, ConnectorError>
    where
        F: FnOnce() -> Fut + Send,
        Fut: std::future::Future<Output = Result<String, ConnectorError>> + Send,
    {
        let RawRequest {
            method,
            url,
            body,
            content_type,
        } = request;
        let mut effective_bearer: String = bearer.to_owned();
        let mut refresh_slot: Option<F> = Some(refresh);
        let mut refreshed_once = false;

        for attempt in 0..=MAX_RETRIES {
            let mut req = self
                .inner
                .request(method.clone(), url)
                .bearer_auth(&effective_bearer);
            if let Some(b) = &body {
                let ct = content_type.unwrap_or("application/json");
                req = req.header("Content-Type", ct).body(b.clone());
            }

            let response = match req.send().await {
                Ok(r) => r,
                Err(e) => match self.on_network_error(e, attempt).await {
                    Some(err) => return Err(err),
                    None => continue,
                },
            };

            let status = response.status();

            // 401: refresh once and retry exactly once.
            if status.as_u16() == 401 && !refreshed_once {
                refreshed_once = true;
                let f = refresh_slot.take().ok_or_else(|| {
                    ConnectorError::InvalidArgument(
                        "refresh closure consumed before being needed".into(),
                    )
                })?;
                let fresh = f().await?;
                effective_bearer = fresh;
                continue;
            }

            match self.classify_response(&response, attempt) {
                ResponseAction::Return => return Ok(response),
                ResponseAction::Fail(err) => return Err(err),
                ResponseAction::FailUpstream => return Err(self.upstream_error(response).await),
                ResponseAction::Retry(wait) => {
                    tokio::time::sleep(wait).await;
                    continue;
                }
            }
        }

        Err(ConnectorError::RateLimited {
            provider: self.provider_id,
            retries: MAX_RETRIES,
        })
    }

    /// Convenience overload for callers that already hold a valid bearer and
    /// know a refresh won't be necessary (e.g. a fresh token just returned).
    pub async fn json_request_no_refresh<B, T>(
        &self,
        method: Method,
        url: &str,
        body: &B,
        bearer: &str,
    ) -> Result<T, ConnectorError>
    where
        B: serde::Serialize + ?Sized,
        T: DeserializeOwned,
    {
        self.json_request(JsonRequest { method, url, body }, bearer, || async move {
            Err(ConnectorError::Unauthorized {
                provider: self.provider_id,
            })
        })
        .await
    }

    /// Decide what to do after a request failed to even produce a response.
    ///
    /// Returns `None` when the caller should back off and retry the attempt,
    /// or `Some(err)` when the error is terminal.
    async fn on_network_error(&self, e: reqwest::Error, attempt: u32) -> Option<ConnectorError> {
        if attempt < MAX_RETRIES && (e.is_connect() || e.is_timeout()) {
            let backoff = exponential_backoff(attempt);
            tracing::warn!(
                provider = %self.provider_id,
                attempt,
                err = %e,
                "connector.http.network_error.retrying"
            );
            tokio::time::sleep(backoff).await;
            return None;
        }
        Some(ConnectorError::Network(e.to_string()))
    }

    /// Classify a received response (after the 401-refresh branch) into the
    /// next action: succeed, retry after a wait, or fail.
    fn classify_response(&self, response: &Response, attempt: u32) -> ResponseAction {
        let status = response.status();

        if status.as_u16() == 401 {
            return ResponseAction::Fail(ConnectorError::Unauthorized {
                provider: self.provider_id,
            });
        }

        // 429: honour Retry-After up to MAX_BACKOFF, then retry.
        if status.as_u16() == 429 {
            if attempt >= MAX_RETRIES {
                return ResponseAction::Fail(ConnectorError::RateLimited {
                    provider: self.provider_id,
                    retries: attempt,
                });
            }
            let wait = retry_after_or_backoff(response.headers(), attempt);
            tracing::warn!(
                provider = %self.provider_id,
                attempt,
                wait_ms = wait.as_millis() as u64,
                "connector.http.rate_limited.retrying"
            );
            return ResponseAction::Retry(wait);
        }

        // 5xx: exponential backoff, retry.
        if status.is_server_error() {
            if attempt >= MAX_RETRIES {
                return ResponseAction::FailUpstream;
            }
            let wait = exponential_backoff(attempt);
            tracing::warn!(
                provider = %self.provider_id,
                attempt,
                status = status.as_u16(),
                wait_ms = wait.as_millis() as u64,
                "connector.http.server_error.retrying"
            );
            return ResponseAction::Retry(wait);
        }

        if !status.is_success() {
            return ResponseAction::FailUpstream;
        }

        ResponseAction::Return
    }

    /// Build an [`ConnectorError::Upstream`] from a non-success response,
    /// reading its body for the surfaced error message.
    async fn upstream_error(&self, response: Response) -> ConnectorError {
        let status = response.status();
        let raw = response
            .text()
            .await
            .unwrap_or_else(|_| "<unreadable>".into());
        ConnectorError::Upstream {
            provider: self.provider_id,
            status: status.as_u16(),
            body: extract_error_message(&raw),
        }
    }
}

/// Outcome of classifying a received HTTP response in the retry loop.
enum ResponseAction {
    /// The response is successful: return it to the caller.
    Return,
    /// Terminal error that doesn't need the response body.
    Fail(ConnectorError),
    /// Terminal error built from the response body (`Upstream`).
    FailUpstream,
    /// Retry after sleeping for the given duration.
    Retry(Duration),
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

async fn deserialize_json<T: DeserializeOwned>(
    provider_id: &'static str,
    response: Response,
) -> Result<T, ConnectorError> {
    let bytes = response
        .bytes()
        .await
        .map_err(|e| ConnectorError::Network(e.to_string()))?;
    serde_json::from_slice(&bytes).map_err(|e| {
        tracing::warn!(
            provider = %provider_id,
            err = %e,
            "connector.http.decode_failed"
        );
        ConnectorError::Decoding(e.to_string())
    })
}

fn exponential_backoff(attempt: u32) -> Duration {
    let secs = 1u64 << attempt; // 1, 2, 4, 8...
    Duration::from_secs(secs.min(MAX_BACKOFF.as_secs()))
}

fn retry_after_or_backoff(headers: &HeaderMap, attempt: u32) -> Duration {
    if let Some(value) = headers.get("Retry-After") {
        if let Ok(s) = value.to_str() {
            if let Ok(secs) = s.parse::<u64>() {
                return Duration::from_secs(secs.min(MAX_BACKOFF.as_secs()));
            }
        }
    }
    exponential_backoff(attempt)
}

/// Extract a human-readable error message from a Google-style error envelope.
///
/// Both Google (Drive, Sheets, Docs, etc.) and Microsoft Graph return errors
/// shaped like:
/// ```json
/// {"error": {"code": 400, "message": "Unable to parse range: ..."}}
/// ```
/// Surfacing only `error.message` (truncated to 512 chars) keeps tool-call
/// failure reports tight enough for the LLM to act on instead of drowning in
/// the full HTML/JSON envelope. Falls back to a truncated raw body when the
/// payload doesn't match the standard shape.
fn extract_error_message(raw: &str) -> String {
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(raw) {
        if let Some(msg) = value
            .get("error")
            .and_then(|e| e.get("message"))
            .and_then(serde_json::Value::as_str)
        {
            return truncate(msg, 512);
        }
        if let Some(msg) = value
            .get("error_description")
            .and_then(serde_json::Value::as_str)
        {
            return truncate(msg, 512);
        }
    }
    truncate(raw, 512)
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_owned()
    } else {
        let cut = apollia_core::floor_char_boundary(s, max);
        format!("{}…", &s[..cut])
    }
}

/// Fuzzing-only shim exposing the private [`truncate`] to the fuzz harness.
/// Compiled only under `--cfg fuzzing` (cargo-fuzz).
#[cfg(fuzzing)]
pub fn __fuzz_truncate(s: &str, max: usize) -> String {
    truncate(s, max)
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[test]
    fn truncate_cuts_on_char_boundary() {
        // GIVEN a body of multibyte characters longer than the byte budget, with
        // a cut that would fall inside a 3-byte code point
        let s = "€".repeat(50);
        // WHEN truncating to a limit that lands mid-code-point
        let out = truncate(&s, 10);
        // THEN it does not panic and yields valid UTF-8
        assert!(std::str::from_utf8(out.as_bytes()).is_ok());
        assert!(out.ends_with('…'));
    }

    /// GIVEN a long multibyte string whose max-byte offset lands inside a code point
    /// WHEN truncate is called
    /// THEN it backs off to a boundary and never panics on a split code point
    #[test]
    fn test_truncate_multibyte_input_does_not_panic() {
        // GIVEN "a" + 300 four-byte emoji (1201 bytes); byte 512 is mid-emoji
        let s = format!("a{}", "😀".repeat(300));

        // WHEN truncated to 512 bytes
        let out = truncate(&s, 512);

        // THEN it backs off to byte 509 (the boundary before an emoji) plus ellipsis
        assert!(out.ends_with('…'));
        assert_eq!(out, format!("a{}…", "😀".repeat(127)));
    }

    #[tokio::test]
    async fn test_get_json_success_returns_decoded_body() {
        // GIVEN a mock server returning a simple JSON object on /me
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/me"))
            .and(header("authorization", "Bearer fresh-token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "email": "test@example.com"
            })))
            .mount(&server)
            .await;

        let client = HttpClient::new("test").expect("client");
        let url = format!("{}/me", server.uri());

        // WHEN we GET the endpoint
        let resp: serde_json::Value = client
            .get_json(&url, "fresh-token", || async {
                panic!("refresh should not run when first request succeeds");
            })
            .await
            .expect("get");

        // THEN the body is decoded
        assert_eq!(resp["email"], "test@example.com");
    }

    #[tokio::test]
    async fn test_get_json_401_triggers_single_refresh_then_succeeds() {
        // GIVEN a server that returns 401 to the first request and 200 to the second
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/me"))
            .and(header("authorization", "Bearer stale-token"))
            .respond_with(ResponseTemplate::new(401).set_body_string("expired"))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/me"))
            .and(header("authorization", "Bearer fresh-token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "ok": true
            })))
            .expect(1)
            .mount(&server)
            .await;

        let client = HttpClient::new("test").expect("client");
        let url = format!("{}/me", server.uri());

        // WHEN we GET with a stale token + a refresh closure that returns a fresh token
        let resp: serde_json::Value = client
            .get_json(&url, "stale-token", || async {
                Ok::<_, ConnectorError>("fresh-token".to_owned())
            })
            .await
            .expect("get");

        // THEN the body is decoded after the implicit refresh
        assert_eq!(resp["ok"], true);
    }

    #[tokio::test]
    async fn test_get_json_401_twice_returns_unauthorized() {
        // GIVEN a server that always returns 401
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/me"))
            .respond_with(ResponseTemplate::new(401))
            .mount(&server)
            .await;

        let client = HttpClient::new("test").expect("client");
        let url = format!("{}/me", server.uri());

        // WHEN we GET with a refresh that returns a token, but the upstream still rejects
        let err = client
            .get_json::<serde_json::Value, _, _>(&url, "stale", || async {
                Ok::<_, ConnectorError>("also-stale".to_owned())
            })
            .await
            .unwrap_err();

        // THEN we surface Unauthorized
        match err {
            ConnectorError::Unauthorized { provider } => assert_eq!(provider, "test"),
            other => panic!("expected Unauthorized, got: {other}"),
        }
    }

    #[tokio::test]
    async fn test_4xx_non_401_returns_upstream_error() {
        // GIVEN a server returning 404
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/missing"))
            .respond_with(ResponseTemplate::new(404).set_body_string("not found"))
            .mount(&server)
            .await;

        let client = HttpClient::new("test").expect("client");
        let url = format!("{}/missing", server.uri());

        let err = client
            .get_json::<serde_json::Value, _, _>(&url, "tok", || async {
                Ok::<_, ConnectorError>("never".into())
            })
            .await
            .unwrap_err();

        match err {
            ConnectorError::Upstream {
                provider, status, ..
            } => {
                assert_eq!(provider, "test");
                assert_eq!(status, 404);
            }
            other => panic!("expected Upstream, got: {other}"),
        }
    }

    #[test]
    fn test_exponential_backoff_doubles_per_attempt() {
        assert_eq!(exponential_backoff(0), Duration::from_secs(1));
        assert_eq!(exponential_backoff(1), Duration::from_secs(2));
        assert_eq!(exponential_backoff(2), Duration::from_secs(4));
        assert_eq!(exponential_backoff(3), Duration::from_secs(8));
    }

    #[test]
    fn test_exponential_backoff_capped_at_max() {
        assert!(exponential_backoff(10) <= MAX_BACKOFF);
    }

    #[test]
    fn test_extract_error_message_from_google_envelope() {
        let raw = r#"{"error":{"code":400,"message":"Unable to parse range: Apollia Test!A1:C1","errors":[]}}"#;
        assert_eq!(
            extract_error_message(raw),
            "Unable to parse range: Apollia Test!A1:C1"
        );
    }

    #[test]
    fn test_extract_error_message_from_oauth_envelope() {
        let raw =
            r#"{"error":"invalid_grant","error_description":"Token has been expired or revoked."}"#;
        assert_eq!(
            extract_error_message(raw),
            "Token has been expired or revoked."
        );
    }

    #[test]
    fn test_extract_error_message_falls_back_to_raw() {
        let raw = "<html>503 Service Unavailable</html>";
        assert_eq!(extract_error_message(raw), raw);
    }

    #[tokio::test]
    async fn test_send_overrides_content_type() {
        // GIVEN a server that asserts Content-Type: text/plain
        let server = MockServer::start().await;
        Mock::given(method("PATCH"))
            .and(path("/upload"))
            .and(header("content-type", "text/plain"))
            .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
            .expect(1)
            .mount(&server)
            .await;

        let client = HttpClient::new("test").expect("client");
        let url = format!("{}/upload", server.uri());

        let _ = client
            .send(
                RawRequest {
                    method: Method::PATCH,
                    url: &url,
                    body: Some(b"hello".to_vec()),
                    content_type: Some("text/plain"),
                },
                "tok",
                || async { Ok::<_, ConnectorError>("never".into()) },
            )
            .await
            .expect("patch ok");
    }

    #[test]
    fn test_truncate_preserves_short_strings() {
        assert_eq!(truncate("short", 100), "short");
    }

    #[test]
    fn test_truncate_clips_long_strings_with_ellipsis() {
        let s = "0".repeat(1000);
        let t = truncate(&s, 50);
        // 50 ASCII bytes + the ellipsis char (3 bytes UTF-8) = 53 bytes total
        assert_eq!(t.len(), 53);
        assert!(t.ends_with('…'));
    }
}
