//! HTTP fetch tool with network allowlist enforcement.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use apollia_core::SandboxProfile;
use serde::{Deserialize, Serialize};
use serde_json::json;
use thiserror::Error;

use crate::descriptor::{ToolDescriptor, ToolKind};

/// Maximum response body size accepted by this tool.
const MAX_RESPONSE_BYTES: usize = 1_048_576;

/// Default per-request timeout when the caller does not supply one.
const DEFAULT_TIMEOUT_SECS: u64 = 30;

/// Make HTTP requests with network allowlist enforcement.
///
/// The URL's hostname is validated against the configured `allowlist` **before** any
/// network I/O is attempted. If `allowlist` is `None`, every request is denied
/// immediately. Response bodies are capped at 1 MB to prevent memory exhaustion.
pub struct HttpFetch {
    allowlist: Option<Vec<String>>,
    client: reqwest::Client,
}

/// Errors produced by [`HttpFetch`].
#[derive(Debug, Error)]
pub enum HttpFetchError {
    /// Hostname is not in the configured allowlist.
    #[error("network not allowed: hostname '{hostname}' is not in the allowlist")]
    HostNotAllowed {
        /// The rejected hostname extracted from the request URL.
        hostname: String,
    },

    /// No `network_allowlist` was configured — all network access is denied.
    #[error("no network_allowlist configured — all network access is denied")]
    NoAllowlist,

    /// The URL could not be parsed or is missing a hostname.
    #[error("invalid URL: {0}")]
    InvalidUrl(String),

    /// The HTTP request failed (connection refused, DNS error, protocol error, etc.).
    #[error("HTTP request failed: {0}")]
    RequestFailed(String),

    /// Response body exceeded the 1 MB safety limit.
    #[error("response too large: {size} bytes exceeds limit of {limit} bytes")]
    ResponseTooLarge {
        /// Accumulated size at the point where reading was aborted.
        size: usize,
        /// Configured limit (always 1 MB).
        limit: usize,
    },

    /// The request did not complete within the configured timeout.
    #[error("request timed out after {timeout_secs}s")]
    Timeout {
        /// Timeout value that was in effect.
        timeout_secs: u64,
    },
}

/// Input for an HTTP fetch operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpFetchInput {
    /// Target URL (e.g. `https://api.example.com/v1/data`).
    pub url: String,
    /// HTTP method. Defaults to `GET`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub method: Option<String>,
    /// Additional request headers sent verbatim to the server.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub headers: Option<HashMap<String, String>>,
    /// Request body (for POST, PUT, PATCH). Sent as raw bytes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
    /// Per-request timeout in seconds. Defaults to 30 s.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout_secs: Option<u64>,
}

/// Output of a successful HTTP fetch operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpFetchOutput {
    /// HTTP response status code (e.g. 200, 404).
    pub status: u16,
    /// Response headers as `name → value` pairs. Non-ASCII values are dropped.
    pub headers: HashMap<String, String>,
    /// Response body decoded as UTF-8. Invalid byte sequences are replaced with U+FFFD.
    pub body: String,
    /// Round-trip duration in milliseconds, from the start of `run()` to body fully received.
    pub duration_ms: u64,
}

impl HttpFetch {
    /// Create a new `HttpFetch` instance.
    ///
    /// `allowlist` lists every hostname (or parent domain) that agents are permitted to
    /// contact. Pass `None` to deny all network access. The `reqwest::Client` is created
    /// once and shared across calls for connection pooling.
    ///
    /// # Panics
    ///
    /// Panics only if the TLS backend fails to initialise, which cannot occur in practice
    /// with the default platform TLS.
    pub fn new(allowlist: Option<Vec<String>>) -> Self {
        let client = reqwest::Client::builder()
            .build()
            .expect("reqwest::Client initialization is infallible");
        Self { allowlist, client }
    }

    /// Perform the HTTP request described by `input`.
    ///
    /// Execution order: parse URL → validate hostname → build request → send → stream body
    /// (up to 1 MB). The allowlist check happens before any socket is opened.
    ///
    /// # Errors
    ///
    /// Returns [`HttpFetchError::NoAllowlist`] when no allowlist was provided.
    /// Returns [`HttpFetchError::HostNotAllowed`] when the hostname is not permitted.
    /// Returns [`HttpFetchError::InvalidUrl`] for malformed URLs or missing hostnames.
    /// Returns [`HttpFetchError::Timeout`] when the request exceeds `timeout_secs`.
    /// Returns [`HttpFetchError::RequestFailed`] on any network or HTTP-level error.
    /// Returns [`HttpFetchError::ResponseTooLarge`] when the body exceeds 1 MB.
    pub async fn run(&self, input: HttpFetchInput) -> Result<HttpFetchOutput, HttpFetchError> {
        let started = Instant::now();

        let parsed_url = self.validate_url(&input.url)?;

        let timeout_secs = input.timeout_secs.unwrap_or(DEFAULT_TIMEOUT_SECS);
        let method_str = input.method.as_deref().unwrap_or("GET").to_uppercase();

        let method = reqwest::Method::from_bytes(method_str.as_bytes()).map_err(|_| {
            HttpFetchError::InvalidUrl(format!("invalid HTTP method: {method_str}"))
        })?;

        let mut builder = self
            .client
            .request(method, parsed_url.as_str())
            .timeout(Duration::from_secs(timeout_secs));

        if let Some(headers) = &input.headers {
            for (name, value) in headers {
                let header_name = reqwest::header::HeaderName::from_bytes(name.as_bytes())
                    .map_err(|_| {
                        HttpFetchError::RequestFailed(format!("invalid header name: {name}"))
                    })?;
                let header_value = reqwest::header::HeaderValue::from_str(value).map_err(|_| {
                    HttpFetchError::RequestFailed(format!("invalid header value for: {name}"))
                })?;
                builder = builder.header(header_name, header_value);
            }
        }

        if let Some(body) = input.body {
            builder = builder.body(body);
        }

        let response = builder.send().await.map_err(|e| {
            if e.is_timeout() {
                HttpFetchError::Timeout { timeout_secs }
            } else {
                HttpFetchError::RequestFailed(e.to_string())
            }
        })?;

        let status = response.status().as_u16();

        let resp_headers: HashMap<String, String> = response
            .headers()
            .iter()
            .filter_map(|(name, value)| {
                value
                    .to_str()
                    .ok()
                    .map(|v| (name.to_string(), v.to_owned()))
            })
            .collect();

        // Stream the body in chunks, aborting once MAX_RESPONSE_BYTES is exceeded.
        let mut body_bytes: Vec<u8> = Vec::new();
        let mut response = response;
        loop {
            match response.chunk().await {
                Ok(Some(chunk)) => {
                    body_bytes.extend_from_slice(&chunk);
                    if body_bytes.len() > MAX_RESPONSE_BYTES {
                        return Err(HttpFetchError::ResponseTooLarge {
                            size: body_bytes.len(),
                            limit: MAX_RESPONSE_BYTES,
                        });
                    }
                }
                Ok(None) => break,
                Err(e) => {
                    return Err(if e.is_timeout() {
                        HttpFetchError::Timeout { timeout_secs }
                    } else {
                        HttpFetchError::RequestFailed(e.to_string())
                    });
                }
            }
        }

        let body = String::from_utf8_lossy(&body_bytes).into_owned();
        let duration_ms = started.elapsed().as_millis() as u64;

        Ok(HttpFetchOutput {
            status,
            headers: resp_headers,
            body,
            duration_ms,
        })
    }

    /// Parse `url` and validate its hostname against the allowlist.
    ///
    /// Exact match **and** subdomain match are accepted: `api.example.com` passes when
    /// `example.com` is in the allowlist.
    ///
    /// # Errors
    ///
    /// Returns [`HttpFetchError::NoAllowlist`] when no allowlist is configured.
    /// Returns [`HttpFetchError::InvalidUrl`] for unparseable URLs or those without a host.
    /// Returns [`HttpFetchError::HostNotAllowed`] when the hostname is not permitted.
    fn validate_url(&self, url: &str) -> Result<url::Url, HttpFetchError> {
        let allowlist = self.allowlist.as_ref().ok_or(HttpFetchError::NoAllowlist)?;

        let parsed = url::Url::parse(url).map_err(|e| HttpFetchError::InvalidUrl(e.to_string()))?;

        let hostname = parsed
            .host_str()
            .ok_or_else(|| HttpFetchError::InvalidUrl("URL has no hostname".to_string()))?;

        let allowed = allowlist
            .iter()
            .any(|entry| hostname == entry.as_str() || hostname.ends_with(&format!(".{entry}")));

        if !allowed {
            return Err(HttpFetchError::HostNotAllowed {
                hostname: hostname.to_string(),
            });
        }

        Ok(parsed)
    }

    /// Return the [`ToolDescriptor`] for registration in the `ToolRegistry`.
    pub fn descriptor() -> ToolDescriptor {
        ToolDescriptor {
            name: "http_fetch".to_string(),
            version: "1.0.0".to_string(),
            description: "Perform HTTP requests with network allowlist enforcement. \
                The URL hostname is validated against the agent's network_allowlist before any \
                network I/O. Response bodies are limited to 1 MB. \
                Supports GET, POST, PUT, PATCH, DELETE."
                .to_string(),
            kind: ToolKind::Native,
            input_schema: json!({
                "type": "object",
                "properties": {
                    "url": {
                        "type": "string",
                        "description": "Target URL (e.g. https://api.example.com/v1/data)"
                    },
                    "method": {
                        "type": "string",
                        "enum": ["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD"],
                        "description": "HTTP method. Defaults to GET."
                    },
                    "headers": {
                        "type": "object",
                        "additionalProperties": { "type": "string" },
                        "description": "Additional request headers."
                    },
                    "body": {
                        "type": "string",
                        "description": "Request body for POST, PUT, or PATCH requests."
                    },
                    "timeout_secs": {
                        "type": "integer",
                        "minimum": 1,
                        "maximum": 300,
                        "description": "Per-request timeout in seconds. Defaults to 30."
                    }
                },
                "required": ["url"]
            }),
            output_schema: Some(json!({
                "type": "object",
                "properties": {
                    "status": {
                        "type": "integer",
                        "description": "HTTP response status code"
                    },
                    "headers": {
                        "type": "object",
                        "additionalProperties": { "type": "string" },
                        "description": "Response headers"
                    },
                    "body": {
                        "type": "string",
                        "description": "Response body decoded as UTF-8"
                    },
                    "duration_ms": {
                        "type": "integer",
                        "description": "Round-trip time in milliseconds"
                    }
                },
                "required": ["status", "headers", "body", "duration_ms"]
            })),
            sandbox_profile: SandboxProfile::NetworkRestricted,
            tags: vec![
                "network".to_string(),
                "http".to_string(),
                "fetch".to_string(),
                "api".to_string(),
            ],
            dangerous: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    // ---------------------------------------------------------------------------
    // Helpers — minimal in-process HTTP/1.1 mock servers
    // ---------------------------------------------------------------------------

    /// Bind a TCP listener on 127.0.0.1:0 and return it alongside the allocated port.
    async fn bind_local() -> (TcpListener, u16) {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind local listener");
        let port = listener.local_addr().expect("local addr").port();
        (listener, port)
    }

    /// Consume one HTTP/1.1 request, ignoring its content, then write `response` and close.
    async fn serve_once(listener: TcpListener, response: &'static [u8]) {
        let (mut socket, _) = listener.accept().await.expect("accept");
        // drain incoming request bytes (stop at double CRLF)
        let mut buf = vec![0u8; 8192];
        let _ = socket.read(&mut buf).await;
        socket.write_all(response).await.expect("write response");
    }

    /// Consume one HTTP/1.1 request, capture its raw bytes into `captured`, then respond.
    async fn serve_capture(
        listener: TcpListener,
        captured: Arc<tokio::sync::Mutex<Vec<u8>>>,
        response: &'static [u8],
    ) {
        let (mut socket, _) = listener.accept().await.expect("accept");
        let mut buf = vec![0u8; 8192];
        let n = socket.read(&mut buf).await.expect("read request");
        *captured.lock().await = buf[..n].to_vec();
        socket.write_all(response).await.expect("write response");
    }

    // ---------------------------------------------------------------------------
    // Tests
    // ---------------------------------------------------------------------------

    #[tokio::test]
    async fn fetch_allowed_host_returns_body() {
        // GIVEN: mock server on 127.0.0.1, allowlist = ["127.0.0.1"]
        let (listener, port) = bind_local().await;
        let response = b"HTTP/1.1 200 OK\r\nContent-Length: 4\r\n\r\nPONG";
        tokio::spawn(serve_once(listener, response));

        let tool = HttpFetch::new(Some(vec!["127.0.0.1".to_string()]));

        // WHEN
        let result = tool
            .run(HttpFetchInput {
                url: format!("http://127.0.0.1:{port}/ping"),
                method: None,
                headers: None,
                body: None,
                timeout_secs: Some(5),
            })
            .await
            .expect("fetch should succeed");

        // THEN
        assert_eq!(result.status, 200);
        assert_eq!(result.body, "PONG");
    }

    #[tokio::test]
    async fn fetch_disallowed_host_errors_before_io() {
        // GIVEN: allowlist = ["safe.example.com"] — no server bound (would timeout if reached)
        let tool = HttpFetch::new(Some(vec!["safe.example.com".to_string()]));

        // WHEN
        let err = tool
            .run(HttpFetchInput {
                url: "http://evil.example.com/steal".to_string(),
                method: None,
                headers: None,
                body: None,
                timeout_secs: Some(1),
            })
            .await
            .expect_err("should be blocked");

        // THEN: error before any network I/O
        assert!(
            matches!(err, HttpFetchError::HostNotAllowed { ref hostname } if hostname == "evil.example.com"),
            "expected HostNotAllowed, got: {err}"
        );
    }

    #[tokio::test]
    async fn fetch_no_allowlist_denies_all() {
        // GIVEN: allowlist = None
        let tool = HttpFetch::new(None);

        // WHEN
        let err = tool
            .run(HttpFetchInput {
                url: "http://127.0.0.1/any".to_string(),
                method: None,
                headers: None,
                body: None,
                timeout_secs: None,
            })
            .await
            .expect_err("should be denied");

        // THEN
        assert!(
            matches!(err, HttpFetchError::NoAllowlist),
            "expected NoAllowlist, got: {err}"
        );
    }

    #[tokio::test]
    async fn fetch_post_with_body_and_headers_reaches_server() {
        // GIVEN: mock server expecting POST
        let (listener, port) = bind_local().await;
        let captured = Arc::new(tokio::sync::Mutex::new(Vec::new()));
        let captured_clone = Arc::clone(&captured);
        let response = b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nOK";
        tokio::spawn(serve_capture(listener, captured_clone, response));

        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json".to_string());

        let tool = HttpFetch::new(Some(vec!["127.0.0.1".to_string()]));

        // WHEN
        let result = tool
            .run(HttpFetchInput {
                url: format!("http://127.0.0.1:{port}/echo"),
                method: Some("POST".to_string()),
                headers: Some(headers),
                body: Some(r#"{"hello":"world"}"#.to_string()),
                timeout_secs: Some(5),
            })
            .await
            .expect("POST should succeed");

        // THEN: response received
        assert_eq!(result.status, 200);

        // AND the raw request bytes contain the body and Content-Type header
        let raw = captured.lock().await;
        let raw_str = String::from_utf8_lossy(&raw);
        assert!(raw_str.contains("POST"), "expected POST method in request");
        assert!(
            raw_str.contains("application/json"),
            "expected Content-Type header in request"
        );
        assert!(
            raw_str.contains(r#"{"hello":"world"}"#),
            "expected body in request"
        );
    }

    #[tokio::test]
    async fn fetch_timeout_is_respected() {
        // GIVEN: server that accepts but never responds
        let (listener, port) = bind_local().await;
        tokio::spawn(async move {
            let (_socket, _) = listener.accept().await.expect("accept");
            // hold the connection open without writing anything
            tokio::time::sleep(Duration::from_secs(60)).await;
        });

        let tool = HttpFetch::new(Some(vec!["127.0.0.1".to_string()]));

        // WHEN: timeout_secs = 1
        let err = tool
            .run(HttpFetchInput {
                url: format!("http://127.0.0.1:{port}/slow"),
                method: None,
                headers: None,
                body: None,
                timeout_secs: Some(1),
            })
            .await
            .expect_err("should time out");

        // THEN
        assert!(
            matches!(err, HttpFetchError::Timeout { timeout_secs: 1 }),
            "expected Timeout(1), got: {err}"
        );
    }

    #[tokio::test]
    async fn fetch_large_response_is_rejected() {
        // GIVEN: server returning a response body of 2 MB
        let (listener, port) = bind_local().await;
        tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.expect("accept");
            let mut buf = vec![0u8; 4096];
            let _ = socket.read(&mut buf).await;

            // 2 MB body
            let body = vec![b'x'; 2 * 1024 * 1024];
            let header = format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n", body.len());
            socket
                .write_all(header.as_bytes())
                .await
                .expect("write header");
            socket.write_all(&body).await.expect("write body");
        });

        let tool = HttpFetch::new(Some(vec!["127.0.0.1".to_string()]));

        // WHEN
        let err = tool
            .run(HttpFetchInput {
                url: format!("http://127.0.0.1:{port}/large"),
                method: None,
                headers: None,
                body: None,
                timeout_secs: Some(10),
            })
            .await
            .expect_err("should be rejected");

        // THEN
        assert!(
            matches!(err, HttpFetchError::ResponseTooLarge { .. }),
            "expected ResponseTooLarge, got: {err}"
        );
    }

    #[test]
    fn validate_url_allows_exact_hostname_match() {
        // GIVEN: allowlist = ["example.com"]
        let tool = HttpFetch::new(Some(vec!["example.com".to_string()]));

        // WHEN: exact match
        let result = tool.validate_url("https://example.com/path");

        // THEN
        assert!(result.is_ok(), "exact match should be allowed");
    }

    #[test]
    fn validate_url_allows_subdomain_match() {
        // GIVEN: allowlist = ["example.com"]
        let tool = HttpFetch::new(Some(vec!["example.com".to_string()]));

        // WHEN: subdomain of allowed entry
        let result = tool.validate_url("https://api.example.com/data");

        // THEN
        assert!(result.is_ok(), "subdomain should be allowed");
    }

    #[test]
    fn validate_url_rejects_unrelated_domain() {
        // GIVEN: allowlist = ["safe.example.com"]
        let tool = HttpFetch::new(Some(vec!["safe.example.com".to_string()]));

        // WHEN: completely different domain
        let result = tool.validate_url("https://evil.com/");

        // THEN
        assert!(
            matches!(result, Err(HttpFetchError::HostNotAllowed { .. })),
            "unrelated domain should be blocked"
        );
    }

    #[test]
    fn validate_url_rejects_sibling_domain() {
        // GIVEN: allowlist = ["sub.example.com"]
        // Ensure "notexample.com" does not slip through via suffix match
        let tool = HttpFetch::new(Some(vec!["example.com".to_string()]));

        // WHEN: a domain that ends with "example.com" but is not a subdomain
        let result = tool.validate_url("https://notexample.com/");

        // THEN: must be blocked
        assert!(
            matches!(result, Err(HttpFetchError::HostNotAllowed { .. })),
            "notexample.com must not match allowlist entry 'example.com'"
        );
    }

    #[test]
    fn validate_url_rejects_malformed_url() {
        // GIVEN: allowlist = ["example.com"]
        let tool = HttpFetch::new(Some(vec!["example.com".to_string()]));

        // WHEN: not a URL
        let result = tool.validate_url("not a url !!!");

        // THEN
        assert!(
            matches!(result, Err(HttpFetchError::InvalidUrl(_))),
            "malformed URL should return InvalidUrl"
        );
    }

    #[test]
    fn descriptor_passes_validation() {
        let descriptor = HttpFetch::descriptor();
        assert!(
            descriptor.validate().is_ok(),
            "descriptor must be valid for ToolRegistry registration"
        );
    }
}
