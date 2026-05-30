//! SSE (Server-Sent Events) transport for MCP JSON-RPC communication with remote servers.
//!
//! The client opens a persistent `GET` connection to the SSE URL. The server responds
//! with an initial `event: endpoint` whose `data` is the HTTP POST URL. Subsequent
//! JSON-RPC responses arrive as `data:` events on that stream. Requests are sent via
//! HTTP `POST` to the endpoint URL.
//!
//! The SSE stream is consumed by a background Tokio task spawned at construction time.

use std::sync::Arc;
use std::time::Duration;

use futures::StreamExt;
use tokio::sync::{mpsc, watch, Mutex};

use super::{McpTransport, TransportError};

// ─── internal types ───────────────────────────────────────────────────────────

/// A parsed SSE event, accumulated line by line until the blank-line terminator.
#[derive(Debug, Default)]
struct SseEvent {
    event_type: String,
    data: String,
}

// ─── SseTransport ─────────────────────────────────────────────────────────────

/// MCP transport that communicates with a remote server via Server-Sent Events.
///
/// Protocol flow:
/// 1. A persistent `GET` to `sse_url` is opened in a background task on construction.
/// 2. The server streams back an initial `event: endpoint` whose `data` is the POST URL.
/// 3. JSON-RPC requests are sent via `POST` to that URL.
/// 4. JSON-RPC responses arrive as subsequent `data:` events on the SSE stream.
///
/// Authentication headers (e.g. `Authorization: Bearer …`) are injected on both the
/// initial `GET` and every subsequent `POST`.
///
/// Reconnection on disconnect is not implemented; a terminated SSE connection surfaces
/// as [`TransportError::Closed`] on the next [`recv`] call.
pub struct SseTransport {
    client: reqwest::Client,
    auth_headers: Arc<Vec<(String, String)>>,
    /// JSON-RPC response lines forwarded by the background SSE listener task.
    rx: Mutex<mpsc::Receiver<String>>,
    timeout: Duration,
    /// Resolves to the POST endpoint URL once the listener receives the `endpoint` event.
    post_endpoint_rx: Mutex<watch::Receiver<Option<String>>>,
    /// Signals the background SSE listener task to stop.
    shutdown_tx: watch::Sender<bool>,
}

impl SseTransport {
    /// Build an SSE transport and start the background SSE listener task.
    ///
    /// The `GET` connection to `sse_url` is opened immediately in a background task.
    /// No request/response exchange occurs until the first [`send`] call.
    ///
    /// `auth_headers` are added to both the initial `GET` and every subsequent `POST`.
    /// `timeout` limits both the maximum wait for the `endpoint` event and each `POST`.
    ///
    /// Must be called from within a Tokio runtime context (`tokio::spawn` is used internally).
    pub fn new(
        sse_url: impl Into<String>,
        auth_headers: Vec<(String, String)>,
        timeout: Duration,
    ) -> Result<Self, TransportError> {
        let client = reqwest::Client::builder()
            .build()
            .map_err(|e| TransportError::Io(e.to_string()))?;

        let (msg_tx, msg_rx) = mpsc::channel::<String>(64);
        let (endpoint_tx, endpoint_rx) = watch::channel::<Option<String>>(None);
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let auth = Arc::new(auth_headers);
        let sse_url: String = sse_url.into();

        tokio::spawn(sse_listener(
            SseListenerCtx {
                client: client.clone(),
                sse_url: sse_url.clone(),
                auth_headers: Arc::clone(&auth),
                endpoint_tx,
                msg_tx,
            },
            shutdown_rx,
        ));

        Ok(Self {
            client,
            auth_headers: auth,
            rx: Mutex::new(msg_rx),
            timeout,
            post_endpoint_rx: Mutex::new(endpoint_rx),
            shutdown_tx,
        })
    }

    /// Resolve and return the POST endpoint URL.
    ///
    /// Returns immediately when the endpoint is already known. Waits up to `timeout`
    /// for the SSE listener to extract the URL from the `endpoint` event otherwise.
    ///
    /// Returns [`TransportError::Io`] with a `"timeout"` message when the server does
    /// not send the `endpoint` event within the allowed duration.
    /// Returns [`TransportError::Closed`] when the SSE sender side is dropped.
    async fn post_endpoint(&self) -> Result<String, TransportError> {
        let mut rx = self.post_endpoint_rx.lock().await;
        let guard = tokio::time::timeout(self.timeout, rx.wait_for(|v| v.is_some()))
            .await
            .map_err(|_| TransportError::Io("timeout waiting for SSE endpoint".to_string()))?
            .map_err(|_| TransportError::Closed)?;
        guard
            .as_ref()
            .cloned()
            .ok_or_else(|| TransportError::Io("SSE endpoint not available".to_string()))
    }
}

#[async_trait::async_trait]
impl McpTransport for SseTransport {
    /// Send `message` via HTTP `POST` to the SSE server's JSON-RPC endpoint.
    ///
    /// Blocks until the `endpoint` event is received from the SSE stream (up to `timeout`),
    /// then POSTs the message. Returns [`TransportError::Io`] on HTTP error or timeout;
    /// [`TransportError::Closed`] if the SSE connection terminated before the endpoint arrived.
    async fn send(&self, message: &str) -> Result<(), TransportError> {
        let endpoint = self.post_endpoint().await?;

        let mut builder = self
            .client
            .post(&endpoint)
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .timeout(self.timeout)
            .body(message.to_string());

        for (name, value) in self.auth_headers.as_ref() {
            builder = builder.header(name.as_str(), value.as_str());
        }

        let response = builder.send().await.map_err(|e| {
            if e.is_timeout() {
                TransportError::Io("timeout".to_string())
            } else {
                TransportError::Io(e.to_string())
            }
        })?;

        if !response.status().is_success() {
            return Err(TransportError::Io(format!(
                "HTTP {}",
                response.status().as_u16()
            )));
        }

        Ok(())
    }

    /// Return the next JSON-RPC response message received from the SSE stream.
    ///
    /// Blocks until a message is available. Returns [`TransportError::Closed`] when the
    /// SSE connection is terminated and all buffered messages have been consumed.
    async fn recv(&self) -> Result<String, TransportError> {
        self.rx
            .lock()
            .await
            .recv()
            .await
            .ok_or(TransportError::Closed)
    }

    /// Signal the background SSE listener task to stop.
    ///
    /// Subsequent `recv` calls drain any messages buffered before shutdown and then
    /// return [`TransportError::Closed`]. Safe to call even if the background task has
    /// already exited.
    async fn shutdown(&self) -> Result<(), TransportError> {
        let _ = self.shutdown_tx.send(true);
        Ok(())
    }
}

// ─── background SSE listener ──────────────────────────────────────────────────

/// Owned context handed to the background SSE listener task.
///
/// Groups the channels and connection parameters the listener needs so the task
/// entry point and its inner loop take a single bundle instead of a long list.
struct SseListenerCtx {
    client: reqwest::Client,
    sse_url: String,
    auth_headers: Arc<Vec<(String, String)>>,
    endpoint_tx: watch::Sender<Option<String>>,
    msg_tx: mpsc::Sender<String>,
}

/// Entry point for the background SSE listener task.
///
/// Opens the `GET` connection and processes SSE events until the stream ends or a
/// shutdown signal is received. Dropping `msg_tx` on exit unblocks any pending
/// [`SseTransport::recv`] callers with [`TransportError::Closed`].
async fn sse_listener(ctx: SseListenerCtx, mut shutdown_rx: watch::Receiver<bool>) {
    if let Err(e) = run_sse_loop(&ctx, &mut shutdown_rx).await {
        tracing::warn!(error = %e, url = %ctx.sse_url, "SSE connection closed with error");
    }
    // `msg_tx` drops here, signalling pending recv() callers that the transport is closed.
}

/// Core SSE processing loop: opens the GET stream and dispatches events.
async fn run_sse_loop(
    ctx: &SseListenerCtx,
    shutdown_rx: &mut watch::Receiver<bool>,
) -> Result<(), TransportError> {
    let mut builder = ctx
        .client
        .get(&ctx.sse_url)
        .header(reqwest::header::ACCEPT, "text/event-stream")
        .header(reqwest::header::CACHE_CONTROL, "no-cache");

    for (name, value) in ctx.auth_headers.as_ref() {
        builder = builder.header(name.as_str(), value.as_str());
    }

    let response = builder
        .send()
        .await
        .map_err(|e| TransportError::Io(e.to_string()))?;

    if !response.status().is_success() {
        return Err(TransportError::Io(format!(
            "HTTP {}",
            response.status().as_u16()
        )));
    }

    let mut stream = response.bytes_stream();
    let mut buffer = String::new();
    let mut current = SseEvent::default();

    loop {
        tokio::select! {
            biased;

            // Check shutdown signal before processing more data.
            result = shutdown_rx.changed() => {
                if result.is_err() || *shutdown_rx.borrow() {
                    return Ok(());
                }
            }

            chunk = stream.next() => {
                match chunk {
                    None => return Ok(()), // Stream ended gracefully.
                    Some(Err(e)) => return Err(TransportError::Io(e.to_string())),
                    Some(Ok(bytes)) => {
                        buffer.push_str(&String::from_utf8_lossy(&bytes));
                        process_buffer(&mut buffer, &mut current, &ctx.endpoint_tx, &ctx.msg_tx)
                            .await;
                    }
                }
            }
        }
    }
}

/// Parse all complete SSE lines in `buffer` and dispatch any finished events.
///
/// Mutates `buffer` in place, consuming processed bytes. Incomplete trailing lines
/// (no `\n` yet) remain for the next chunk.
async fn process_buffer(
    buffer: &mut String,
    current: &mut SseEvent,
    endpoint_tx: &watch::Sender<Option<String>>,
    msg_tx: &mpsc::Sender<String>,
) {
    while let Some(nl) = buffer.find('\n') {
        let line = buffer[..nl].trim_end_matches('\r').to_string();
        buffer.drain(..nl + 1);

        if line.is_empty() {
            // Blank line terminates the current event.
            let event = std::mem::take(current);
            if !event.data.is_empty() {
                dispatch_event(event, endpoint_tx, msg_tx).await;
            }
        } else if let Some(rest) = line.strip_prefix("event:") {
            current.event_type = rest.trim().to_string();
        } else if let Some(rest) = line.strip_prefix("data:") {
            current.data = rest.trim().to_string();
        }
        // id:, retry:, and comment lines are intentionally ignored.
    }
}

/// Route a finished SSE event to the appropriate channel.
///
/// `endpoint` events set the POST URL in the watch channel, notifying any waiting
/// [`SseTransport::send`] call. All other non-empty events are forwarded to the
/// JSON-RPC message queue consumed by [`SseTransport::recv`].
async fn dispatch_event(
    event: SseEvent,
    endpoint_tx: &watch::Sender<Option<String>>,
    msg_tx: &mpsc::Sender<String>,
) {
    if event.event_type == "endpoint" {
        let _ = endpoint_tx.send(Some(event.data.clone()));
        tracing::debug!(endpoint = %event.data, "SSE post endpoint received");
    } else if msg_tx.send(event.data).await.is_err() {
        tracing::debug!("SSE listener: message receiver dropped, stopping");
    }
}

// ─── tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::convert::Infallible;
    use std::sync::Arc;

    use axum::body::{Body, Bytes};
    use axum::extract::State;
    use axum::http::{HeaderMap, StatusCode};
    use axum::response::{IntoResponse, Response};
    use axum::routing::{get, post};
    use axum::Router;
    use futures::stream;
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

    /// Build and start a minimal SSE test server.
    ///
    /// - `GET /sse`: streams an `endpoint` event pointing to `/post`, then optionally
    ///   a pre-crafted `data` event, then closes the stream.
    /// - `POST /post`: returns `202 Accepted`.
    ///
    /// Returns the bound address and the absolute POST URL.
    async fn start_sse_server(extra_data_event: Option<String>) -> (std::net::SocketAddr, String) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind must succeed in tests");
        let addr = listener.local_addr().expect("local_addr must be available");

        let post_url = format!("http://{addr}/post");
        let endpoint_event = format!("event: endpoint\ndata: {post_url}\n\n");

        let mut chunks: Vec<Bytes> = vec![Bytes::from(endpoint_event)];
        if let Some(data) = extra_data_event {
            chunks.push(Bytes::from(format!("data: {data}\n\n")));
        }

        let app = Router::new()
            .route(
                "/sse",
                get(move || {
                    let chunks = chunks.clone();
                    async move {
                        let body = Body::from_stream(stream::iter(
                            chunks.into_iter().map(Ok::<_, Infallible>),
                        ));
                        Response::builder()
                            .header("content-type", "text/event-stream")
                            .body(body)
                            .expect("valid SSE response")
                    }
                }),
            )
            .route("/post", post(|| async { StatusCode::ACCEPTED }));

        tokio::spawn(async move {
            axum::serve(listener, app)
                .await
                .expect("axum::serve must not fail");
        });

        (addr, post_url)
    }

    #[tokio::test]
    async fn test_sse_transport_connect_and_recv_endpoint() {
        // GIVEN an SSE server that sends only the endpoint event
        let (addr, _) = start_sse_server(None).await;

        // WHEN the transport is constructed (background task connects immediately)
        let transport =
            SseTransport::new(format!("http://{addr}/sse"), vec![], Duration::from_secs(5))
                .expect("SseTransport::new must succeed");

        // THEN send() resolves the endpoint without timing out and POSTs successfully
        let result = transport
            .send(r#"{"jsonrpc":"2.0","id":1,"method":"initialize"}"#)
            .await;
        assert!(
            result.is_ok(),
            "send must succeed when endpoint event is received: {result:?}"
        );
    }

    #[tokio::test]
    async fn test_sse_transport_send_recv_cycle() {
        // GIVEN an SSE server that sends the endpoint event, then a pre-crafted JSON-RPC response
        let response_body = r#"{"jsonrpc":"2.0","id":1,"result":{}}"#.to_string();
        let (addr, _) = start_sse_server(Some(response_body.clone())).await;

        let transport =
            SseTransport::new(format!("http://{addr}/sse"), vec![], Duration::from_secs(5))
                .expect("SseTransport::new must succeed");

        // WHEN send() is followed by recv()
        transport
            .send(r#"{"jsonrpc":"2.0","id":1,"method":"initialize"}"#)
            .await
            .expect("send must succeed");
        let received = transport.recv().await.expect("recv must succeed");

        // THEN the JSON-RPC message forwarded via the SSE data event is returned
        assert_eq!(received, response_body);
    }

    #[tokio::test]
    async fn test_sse_transport_auth_header_sent_on_post() {
        // GIVEN an SSE server that captures Authorization headers on POST requests
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind must succeed");
        let addr = listener.local_addr().expect("local_addr must be available");

        let post_url = format!("http://{addr}/post");
        let endpoint_event = Bytes::from(format!("event: endpoint\ndata: {post_url}\n\n"));

        let captured: Arc<TokioMutex<Vec<String>>> = Arc::new(TokioMutex::new(vec![]));
        let state = Arc::clone(&captured);

        let app = Router::new()
            .route(
                "/sse",
                get(move || {
                    let chunk = endpoint_event.clone();
                    async move {
                        let body = Body::from_stream(stream::iter([Ok::<_, Infallible>(chunk)]));
                        Response::builder()
                            .header("content-type", "text/event-stream")
                            .body(body)
                            .expect("valid SSE response")
                    }
                }),
            )
            .route(
                "/post",
                post(
                    |State(cap): State<Arc<TokioMutex<Vec<String>>>>,
                     headers: HeaderMap,
                     _body: String| async move {
                        if let Some(v) = headers.get("authorization") {
                            if let Ok(s) = v.to_str() {
                                cap.lock().await.push(s.to_string());
                            }
                        }
                        StatusCode::ACCEPTED.into_response()
                    },
                ),
            )
            .with_state(state);

        tokio::spawn(async move {
            axum::serve(listener, app)
                .await
                .expect("axum::serve must not fail");
        });

        // WHEN the transport is built with an Authorization header and send() is called
        let transport = SseTransport::new(
            format!("http://{addr}/sse"),
            vec![("Authorization".to_string(), "Bearer ssetoken".to_string())],
            Duration::from_secs(5),
        )
        .expect("SseTransport::new must succeed");

        transport
            .send(r#"{"jsonrpc":"2.0","id":1,"method":"initialize"}"#)
            .await
            .expect("send must succeed");

        // THEN the Authorization header was present in the POST request
        let headers = captured.lock().await;
        assert_eq!(headers.len(), 1);
        assert_eq!(headers[0], "Bearer ssetoken");
    }

    #[tokio::test]
    async fn test_sse_transport_timeout_on_missing_endpoint() {
        // GIVEN an SSE server that keeps the connection open but never sends any event
        // (stream::pending() yields no items and never ends, so the SSE listener blocks in
        //  stream.next(), so the endpoint watch channel is never updated)
        let app = Router::new().route(
            "/sse",
            get(|| async {
                let body = Body::from_stream(stream::pending::<Result<Bytes, Infallible>>());
                Response::builder()
                    .header("content-type", "text/event-stream")
                    .body(body)
                    .expect("valid response")
            }),
        );
        let addr = start_server(app).await;

        let transport = SseTransport::new(
            format!("http://{addr}/sse"),
            vec![],
            Duration::from_millis(200),
        )
        .expect("SseTransport::new must succeed");

        // WHEN send() is called with a short timeout
        let result = transport
            .send(r#"{"jsonrpc":"2.0","id":1,"method":"initialize"}"#)
            .await;

        // THEN a timeout error is returned
        assert!(matches!(result, Err(TransportError::Io(_))));
        if let Err(TransportError::Io(msg)) = result {
            assert!(
                msg.contains("timeout"),
                "error must mention timeout, got: {msg}"
            );
        }
    }

    #[tokio::test]
    async fn test_sse_transport_http_error_on_sse_endpoint() {
        // GIVEN an SSE server that returns HTTP 403 on the GET /sse request
        let app = Router::new().route("/sse", get(|| async { StatusCode::FORBIDDEN }));
        let addr = start_server(app).await;

        let transport = SseTransport::new(
            format!("http://{addr}/sse"),
            vec![],
            Duration::from_millis(300),
        )
        .expect("SseTransport::new must succeed");

        // WHEN send() is called: the background task will have failed on the GET
        let result = transport
            .send(r#"{"jsonrpc":"2.0","id":1,"method":"initialize"}"#)
            .await;

        // THEN an error is returned (either timeout or Closed, depending on task timing)
        assert!(
            result.is_err(),
            "send must fail when SSE connection is refused"
        );
    }

    #[tokio::test]
    async fn test_sse_transport_pid_is_none() {
        // GIVEN an SSE transport (no subprocess involved)
        let transport = SseTransport::new("http://127.0.0.1:1/sse", vec![], Duration::from_secs(5))
            .expect("SseTransport::new must succeed");

        // THEN pid() returns None
        assert!(transport.pid().is_none());
    }

    #[tokio::test]
    async fn test_sse_transport_shutdown_is_ok() {
        // GIVEN an SSE transport (background task may or may not have connected)
        let transport = SseTransport::new("http://127.0.0.1:1/sse", vec![], Duration::from_secs(5))
            .expect("SseTransport::new must succeed");

        // WHEN shutdown() is called
        let result = transport.shutdown().await;

        // THEN it succeeds regardless of the connection state
        assert!(result.is_ok());
    }
}
