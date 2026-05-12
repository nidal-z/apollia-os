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

/// Shared HTTP client for connector implementations.
#[derive(Clone)]
pub struct HttpClient {
    inner: reqwest::Client,
    provider_id: &'static str,
}

impl HttpClient {
    /// Build a client bound to a provider id (for telemetry and error mapping).
    pub fn new(provider_id: &'static str) -> Result<Self, ConnectorError> {
        let inner = reqwest::Client::builder()
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
            .send_with_retries(Method::GET, url, None, bearer, refresh)
            .await?;
        deserialize_json(self.provider_id, response).await
    }

    /// Execute a JSON-bodied request (POST/PUT/PATCH/DELETE) and decode the response.
    pub async fn json_request<B, T, F, Fut>(
        &self,
        method: Method,
        url: &str,
        body: &B,
        bearer: &str,
        refresh: F,
    ) -> Result<T, ConnectorError>
    where
        B: serde::Serialize + ?Sized,
        T: DeserializeOwned,
        F: FnOnce() -> Fut + Send,
        Fut: std::future::Future<Output = Result<String, ConnectorError>> + Send,
    {
        let body_bytes =
            serde_json::to_vec(body).map_err(|e| ConnectorError::Decoding(e.to_string()))?;
        let response = self
            .send_with_retries(method, url, Some(body_bytes), bearer, refresh)
            .await?;
        deserialize_json(self.provider_id, response).await
    }

    /// Execute a request and return the raw response — useful for streaming
    /// downloads where the caller drives the body. The 401-refresh and 429
    /// policies still apply.
    pub async fn send_with_retries<F, Fut>(
        &self,
        method: Method,
        url: &str,
        body: Option<Vec<u8>>,
        bearer: &str,
        refresh: F,
    ) -> Result<Response, ConnectorError>
    where
        F: FnOnce() -> Fut + Send,
        Fut: std::future::Future<Output = Result<String, ConnectorError>> + Send,
    {
        let mut effective_bearer: String = bearer.to_owned();
        let mut refresh_slot: Option<F> = Some(refresh);
        let mut refreshed_once = false;

        for attempt in 0..=MAX_RETRIES {
            let mut req = self
                .inner
                .request(method.clone(), url)
                .bearer_auth(&effective_bearer);
            if let Some(b) = &body {
                req = req
                    .header("Content-Type", "application/json")
                    .body(b.clone());
            }

            let result = req.send().await;
            let response = match result {
                Ok(r) => r,
                Err(e) => {
                    if attempt < MAX_RETRIES && (e.is_connect() || e.is_timeout()) {
                        let backoff = exponential_backoff(attempt);
                        tracing::warn!(
                            provider = %self.provider_id,
                            attempt,
                            err = %e,
                            "connector.http.network_error.retrying"
                        );
                        tokio::time::sleep(backoff).await;
                        continue;
                    }
                    return Err(ConnectorError::Network(e.to_string()));
                }
            };

            let status = response.status();

            // 401 — refresh once and retry exactly once.
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

            if status.as_u16() == 401 {
                return Err(ConnectorError::Unauthorized {
                    provider: self.provider_id,
                });
            }

            // 429 — honour Retry-After up to MAX_BACKOFF, then retry.
            if status.as_u16() == 429 {
                if attempt >= MAX_RETRIES {
                    return Err(ConnectorError::RateLimited {
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
                tokio::time::sleep(wait).await;
                continue;
            }

            // 5xx — exponential backoff, retry.
            if status.is_server_error() {
                if attempt >= MAX_RETRIES {
                    let body = response
                        .text()
                        .await
                        .unwrap_or_else(|_| "<unreadable>".into());
                    return Err(ConnectorError::Upstream {
                        provider: self.provider_id,
                        status: status.as_u16(),
                        body: truncate(&body, 512),
                    });
                }
                let wait = exponential_backoff(attempt);
                tracing::warn!(
                    provider = %self.provider_id,
                    attempt,
                    status = status.as_u16(),
                    wait_ms = wait.as_millis() as u64,
                    "connector.http.server_error.retrying"
                );
                tokio::time::sleep(wait).await;
                continue;
            }

            if !status.is_success() {
                let body = response
                    .text()
                    .await
                    .unwrap_or_else(|_| "<unreadable>".into());
                return Err(ConnectorError::Upstream {
                    provider: self.provider_id,
                    status: status.as_u16(),
                    body: truncate(&body, 512),
                });
            }

            return Ok(response);
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
        self.json_request(method, url, body, bearer, || async move {
            Err(ConnectorError::Unauthorized {
                provider: self.provider_id,
            })
        })
        .await
    }
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

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_owned()
    } else {
        format!("{}…", &s[..max])
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

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
