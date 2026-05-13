//! Streamable HTTP transport for MCP JSON-RPC communication with remote servers.
//!
//! Each [`StreamableHttpTransport::send`] call POSTs one JSON-RPC message to the
//! configured URL and buffers the response body. The corresponding
//! [`StreamableHttpTransport::recv`] call dequeues and returns that body, making
//! the transport compatible with the session dispatch loop that alternates
//! `send` / `recv` calls.

use std::time::Duration;

use tokio::sync::{mpsc, Mutex};

use super::{McpTransport, TransportError};

// ─── StreamableHttpTransport ─────────────────────────────────────────────────

/// MCP transport that communicates with a remote server via HTTP POST.
///
/// Implements the MCP *Streamable HTTP* specification:
/// - Each request is a `POST` with `Content-Type: application/json`.
/// - The `Accept` header is `application/json, text/event-stream`.
/// - After the first response, the `Mcp-Session-Id` header returned by the
///   server is echoed on all subsequent requests to maintain session affinity.
///
/// Authentication headers (e.g. `Authorization: Bearer …`) can be supplied at
/// construction time and are injected on every request.
///
/// The transport is lazy: no network connection is opened until the first
/// [`send`] call. [`shutdown`] is a no-op because HTTP has no persistent
/// connection to close.
pub struct StreamableHttpTransport {
    client: reqwest::Client,
    url: String,
    /// Session ID extracted from the `Mcp-Session-Id` header of the first response.
    session_id: Mutex<Option<String>>,
    auth_headers: Vec<(String, String)>,
    timeout: Duration,
    /// Sender side: filled by `send` after each successful HTTP response.
    response_tx: mpsc::UnboundedSender<String>,
    /// Receiver side: drained by `recv`.
    response_rx: Mutex<mpsc::UnboundedReceiver<String>>,
}

impl StreamableHttpTransport {
    /// Build a transport targeting `url`.
    ///
    /// `auth_headers` are added to every request (e.g. `("Authorization", "Bearer tok")`).
    /// `timeout` is applied per request; expiry surfaces as [`TransportError::Io`] with
    /// the message `"timeout"`.
    ///
    /// No network connection is established by this constructor.
    pub fn new(
        url: impl Into<String>,
        auth_headers: Vec<(String, String)>,
        timeout: Duration,
    ) -> Result<Self, TransportError> {
        let client = reqwest::Client::builder()
            .build()
            .map_err(|e| TransportError::Io(e.to_string()))?;

        let (tx, rx) = mpsc::unbounded_channel();

        Ok(Self {
            client,
            url: url.into(),
            session_id: Mutex::new(None),
            auth_headers,
            timeout,
            response_tx: tx,
            response_rx: Mutex::new(rx),
        })
    }
}

#[async_trait::async_trait]
impl McpTransport for StreamableHttpTransport {
    /// POST `message` to the server URL, capture the `Mcp-Session-Id` header,
    /// and buffer the response body for the next [`recv`] call.
    ///
    /// Returns [`TransportError::Io`] when:
    /// - the server responds with a non-2xx status (message contains the code, e.g. `"HTTP 401"`)
    /// - the request exceeds `timeout` (message is `"timeout"`)
    /// - any other network-level error occurs
    async fn send(&self, message: &str) -> Result<(), TransportError> {
        let mut builder = self
            .client
            .post(&self.url)
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .header(
                reqwest::header::ACCEPT,
                "application/json, text/event-stream",
            )
            .timeout(self.timeout)
            .body(message.to_string());

        // Echo the session ID received from the server on all requests after the first.
        if let Some(id) = self.session_id.lock().await.as_deref() {
            builder = builder.header("Mcp-Session-Id", id);
        }

        for (name, value) in &self.auth_headers {
            builder = builder.header(name.as_str(), value.as_str());
        }

        let response = builder.send().await.map_err(|e| {
            if e.is_timeout() {
                TransportError::Io("timeout".to_string())
            } else {
                TransportError::Io(e.to_string())
            }
        })?;

        let status = response.status();
        if status == reqwest::StatusCode::UNAUTHORIZED {
            let www_authenticate = response
                .headers()
                .get(reqwest::header::WWW_AUTHENTICATE)
                .and_then(|v| v.to_str().ok())
                .unwrap_or("")
                .to_string();
            return Err(TransportError::Unauthorized { www_authenticate });
        }
        if !status.is_success() {
            return Err(TransportError::Io(format!("HTTP {}", status.as_u16())));
        }

        // Store the session ID for all subsequent requests.
        if let Some(id_header) = response.headers().get("Mcp-Session-Id") {
            if let Ok(id_str) = id_header.to_str() {
                *self.session_id.lock().await = Some(id_str.to_string());
            }
        }

        let body = response
            .text()
            .await
            .map_err(|e| TransportError::Io(e.to_string()))?;

        self.response_tx
            .send(body)
            .map_err(|_| TransportError::Closed)
    }

    /// Return the next JSON-RPC response buffered by a prior [`send`] call.
    ///
    /// Waits until `send` has queued a response. Returns [`TransportError::Closed`]
    /// when the sender side has been dropped (transport destroyed).
    async fn recv(&self) -> Result<String, TransportError> {
        self.response_rx
            .lock()
            .await
            .recv()
            .await
            .ok_or(TransportError::Closed)
    }

    /// No-op: HTTP has no persistent connection to close.
    async fn shutdown(&self) -> Result<(), TransportError> {
        Ok(())
    }
}

// ─── tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    use axum::extract::State;
    use axum::http::{HeaderMap, StatusCode};
    use axum::response::IntoResponse;
    use axum::routing::post;
    use axum::Router;
    use tokio::sync::Mutex as TokioMutex;

    /// Bind a random local port, serve `app`, and return the bound address.
    async fn start_server(app: Router) -> std::net::SocketAddr {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("TcpListener::bind must succeed in tests");
        let addr = listener.local_addr().expect("local_addr must be available");
        tokio::spawn(async move {
            axum::serve(listener, app)
                .await
                .expect("axum::serve must not fail");
        });
        addr
    }

    #[tokio::test]
    async fn test_http_transport_send_recv() {
        // GIVEN a server that responds with JSON-RPC and sets Mcp-Session-Id
        let app = Router::new().route(
            "/mcp",
            post(|| async {
                let mut headers = HeaderMap::new();
                headers.insert(
                    "Mcp-Session-Id",
                    "sess-abc123".parse().expect("valid header value"),
                );
                (
                    StatusCode::OK,
                    headers,
                    r#"{"jsonrpc":"2.0","id":1,"result":{}}"#,
                )
            }),
        );
        let addr = start_server(app).await;

        let transport = StreamableHttpTransport::new(
            format!("http://{addr}/mcp"),
            vec![],
            Duration::from_secs(5),
        )
        .expect("transport construction must succeed");

        // WHEN a message is sent and the response is received
        transport
            .send(r#"{"jsonrpc":"2.0","id":1,"method":"initialize"}"#)
            .await
            .expect("send must succeed");
        let body = transport.recv().await.expect("recv must succeed");

        // THEN the JSON-RPC response body is returned
        assert!(body.contains("result"));
    }

    #[tokio::test]
    async fn test_http_transport_session_id_stored_and_echoed() {
        // GIVEN a server that returns Mcp-Session-Id on the first response
        // and captures the header value from subsequent requests
        let received_ids: Arc<TokioMutex<Vec<String>>> = Arc::new(TokioMutex::new(vec![]));
        let state = Arc::clone(&received_ids);

        let app = Router::new()
            .route(
                "/mcp",
                post(
                    |State(captured): State<Arc<TokioMutex<Vec<String>>>>,
                     req_headers: HeaderMap,
                     _body: String| async move {
                        // Record the Mcp-Session-Id sent by the client (if any).
                        if let Some(v) = req_headers.get("Mcp-Session-Id") {
                            if let Ok(s) = v.to_str() {
                                captured.lock().await.push(s.to_string());
                            }
                        }
                        let mut resp_headers = HeaderMap::new();
                        resp_headers.insert(
                            "Mcp-Session-Id",
                            "session-xyz".parse().expect("valid header value"),
                        );
                        (
                            StatusCode::OK,
                            resp_headers,
                            r#"{"jsonrpc":"2.0","id":1,"result":{}}"#,
                        )
                            .into_response()
                    },
                ),
            )
            .with_state(state);
        let addr = start_server(app).await;

        let transport = StreamableHttpTransport::new(
            format!("http://{addr}/mcp"),
            vec![],
            Duration::from_secs(5),
        )
        .expect("transport construction must succeed");

        // First request — no session ID sent yet
        transport
            .send(r#"{"jsonrpc":"2.0","id":1,"method":"initialize"}"#)
            .await
            .expect("first send must succeed");
        transport.recv().await.expect("first recv must succeed");

        // Second request — session ID must be echoed
        transport
            .send(r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#)
            .await
            .expect("second send must succeed");
        transport.recv().await.expect("second recv must succeed");

        // THEN only the second request carried the Mcp-Session-Id header
        let ids = received_ids.lock().await;
        assert_eq!(
            ids.len(),
            1,
            "only the second request should echo the session ID"
        );
        assert_eq!(ids[0], "session-xyz");
    }

    #[tokio::test]
    async fn test_http_transport_auth_header_sent() {
        // GIVEN a server that captures Authorization headers
        let received_auth: Arc<TokioMutex<Vec<String>>> = Arc::new(TokioMutex::new(vec![]));
        let state = Arc::clone(&received_auth);

        let app = Router::new()
            .route(
                "/mcp",
                post(
                    |State(captured): State<Arc<TokioMutex<Vec<String>>>>,
                     req_headers: HeaderMap,
                     _body: String| async move {
                        if let Some(v) = req_headers.get("Authorization") {
                            if let Ok(s) = v.to_str() {
                                captured.lock().await.push(s.to_string());
                            }
                        }
                        (StatusCode::OK, r#"{"jsonrpc":"2.0","id":1,"result":{}}"#).into_response()
                    },
                ),
            )
            .with_state(state);
        let addr = start_server(app).await;

        // WHEN the transport is configured with an Authorization header
        let transport = StreamableHttpTransport::new(
            format!("http://{addr}/mcp"),
            vec![("Authorization".to_string(), "Bearer tok-xyz".to_string())],
            Duration::from_secs(5),
        )
        .expect("transport construction must succeed");

        transport
            .send(r#"{"jsonrpc":"2.0","id":1,"method":"initialize"}"#)
            .await
            .expect("send must succeed");
        transport.recv().await.expect("recv must succeed");

        // THEN the Authorization header was present in the request
        let auth = received_auth.lock().await;
        assert_eq!(auth.len(), 1);
        assert_eq!(auth[0], "Bearer tok-xyz");
    }

    #[tokio::test]
    async fn test_http_transport_error_status() {
        // GIVEN a server that returns HTTP 500
        let app = Router::new().route("/mcp", post(|| async { StatusCode::INTERNAL_SERVER_ERROR }));
        let addr = start_server(app).await;

        let transport = StreamableHttpTransport::new(
            format!("http://{addr}/mcp"),
            vec![],
            Duration::from_secs(5),
        )
        .expect("transport construction must succeed");

        // WHEN a message is sent
        let result = transport
            .send(r#"{"jsonrpc":"2.0","id":1,"method":"initialize"}"#)
            .await;

        // THEN a TransportError::Io containing the HTTP status code is returned
        assert!(matches!(result, Err(TransportError::Io(_))));
        if let Err(TransportError::Io(msg)) = result {
            assert!(
                msg.contains("500"),
                "error message must contain '500', got: {msg}"
            );
        }
    }

    #[tokio::test]
    async fn test_http_transport_401_surfaces_unauthorized_with_header() {
        // GIVEN a server that returns HTTP 401 with a WWW-Authenticate header
        // pointing at the protected-resource metadata document (RFC 9728)
        let app = Router::new().route(
            "/mcp",
            post(|| async {
                let mut headers = HeaderMap::new();
                headers.insert(
                    "WWW-Authenticate",
                    r#"Bearer resource_metadata="https://mcp.example.com/.well-known/oauth-protected-resource""#
                        .parse()
                        .expect("valid header value"),
                );
                (StatusCode::UNAUTHORIZED, headers).into_response()
            }),
        );
        let addr = start_server(app).await;

        let transport = StreamableHttpTransport::new(
            format!("http://{addr}/mcp"),
            vec![],
            Duration::from_secs(5),
        )
        .expect("transport construction must succeed");

        // WHEN a message is sent
        let result = transport
            .send(r#"{"jsonrpc":"2.0","id":1,"method":"initialize"}"#)
            .await;

        // THEN a typed Unauthorized error is returned with the WWW-Authenticate
        // value so the orchestration layer can drive RFC 9728 discovery.
        match result {
            Err(TransportError::Unauthorized { www_authenticate }) => {
                assert!(
                    www_authenticate.contains("resource_metadata"),
                    "expected resource_metadata in header, got: {www_authenticate}"
                );
            }
            other => panic!("expected Unauthorized, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_http_transport_401_without_header_returns_empty_unauthorized() {
        // GIVEN a server that returns HTTP 401 with no WWW-Authenticate header
        let app = Router::new().route("/mcp", post(|| async { StatusCode::UNAUTHORIZED }));
        let addr = start_server(app).await;

        let transport = StreamableHttpTransport::new(
            format!("http://{addr}/mcp"),
            vec![],
            Duration::from_secs(5),
        )
        .expect("transport construction must succeed");

        let result = transport
            .send(r#"{"jsonrpc":"2.0","id":1,"method":"initialize"}"#)
            .await;

        // THEN the error variant is still Unauthorized with an empty header value
        match result {
            Err(TransportError::Unauthorized { www_authenticate }) => {
                assert!(www_authenticate.is_empty());
            }
            other => panic!("expected Unauthorized, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_http_transport_timeout() {
        // GIVEN a server that never responds within the configured timeout
        let app = Router::new().route(
            "/mcp",
            post(|| async {
                tokio::time::sleep(Duration::from_secs(30)).await;
                StatusCode::OK
            }),
        );
        let addr = start_server(app).await;

        let transport = StreamableHttpTransport::new(
            format!("http://{addr}/mcp"),
            vec![],
            Duration::from_millis(100),
        )
        .expect("transport construction must succeed");

        // WHEN a message is sent
        let result = transport
            .send(r#"{"jsonrpc":"2.0","id":1,"method":"initialize"}"#)
            .await;

        // THEN a timeout error is returned
        assert!(matches!(result, Err(TransportError::Io(_))));
        if let Err(TransportError::Io(msg)) = result {
            assert_eq!(msg, "timeout");
        }
    }

    #[tokio::test]
    async fn test_http_transport_pid_is_none() {
        // GIVEN an HTTP transport (no subprocess)
        let transport = StreamableHttpTransport::new(
            "https://mcp.example.com/mcp",
            vec![],
            Duration::from_secs(5),
        )
        .expect("transport construction must succeed");

        // THEN pid() returns None
        assert!(transport.pid().is_none());
    }

    #[tokio::test]
    async fn test_http_transport_shutdown_is_noop() {
        // GIVEN an HTTP transport
        let transport = StreamableHttpTransport::new(
            "https://mcp.example.com/mcp",
            vec![],
            Duration::from_secs(5),
        )
        .expect("transport construction must succeed");

        // WHEN shutdown is called
        let result = transport.shutdown().await;

        // THEN it succeeds without error
        assert!(result.is_ok());
    }
}
