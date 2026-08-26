//! HTTP client for communicating with the Apollia runtime.
//!
//! On **Unix** (macOS, Linux): connects via `tokio::net::UnixStream` to the
//! `--socket` path (default `~/.apollia/runtime.sock`). Filesystem-based
//! security: the runtime chmods the socket to `0o600` after binding.
//!
//! On **Windows**: `tokio::net::TcpStream` on `127.0.0.1:DEFAULT_TCP_PORT`.
//! The runtime always listens on TCP in parallel, and Windows has no native
//! support for Unix domain sockets in hyper 1.x.

use std::path::{Path, PathBuf};

use futures::channel::mpsc;
use futures::SinkExt;
use http_body_util::{BodyExt, Full};
use hyper::body::Bytes;
use hyper::client::conn::http1;
use hyper_util::rt::TokioIo;

#[cfg(windows)]
use tokio::net::TcpStream;
#[cfg(unix)]
use tokio::net::UnixStream;

/// Low-level I/O type used for the daemon to CLI connection.
#[cfg(unix)]
type RuntimeStream = UnixStream;
#[cfg(windows)]
type RuntimeStream = TcpStream;

/// Opens a connection to the runtime.
///
/// On Unix, uses `socket_path` (Unix domain socket).
/// On Windows, ignores the path and connects to `127.0.0.1:DEFAULT_TCP_PORT`.
#[cfg(unix)]
async fn connect_runtime(socket_path: &Path) -> std::io::Result<RuntimeStream> {
    UnixStream::connect(socket_path).await
}

#[cfg(windows)]
async fn connect_runtime(_socket_path: &Path) -> std::io::Result<RuntimeStream> {
    TcpStream::connect(format!("127.0.0.1:{}", DEFAULT_TCP_PORT)).await
}

/// Default Unix socket path for the Apollia runtime: `~/.apollia/runtime.sock`.
///
/// A function rather than a constant because the path depends on the user's
/// home directory. It used to be the literal `/tmp/apollia.sock`, a name any
/// account on the machine could take first.
pub fn default_socket_path() -> PathBuf {
    apollia_core::paths::socket_path_or_temp()
}

/// Default TCP port for the Apollia runtime.
pub const DEFAULT_TCP_PORT: u16 = 7771;

/// Parameters for [`RuntimeClient::authorize_tool`].
///
/// `decision` is `"accept"`, `"refuse"`, or `"always_accept"`. `reason` is only
/// honoured for `"refuse"`, `scope` only for `"always_accept"` (one of
/// `this_tool`, `this_session`, `this_agent`, `this_project`, `global`).
pub struct AuthorizeToolArgs<'a> {
    /// Chat session id.
    pub session_id: &'a str,
    /// Id of the message that triggered the tool call.
    pub message_id: &'a str,
    /// Name of the tool awaiting approval.
    pub tool_name: &'a str,
    /// Decision keyword sent to the runtime.
    pub decision: &'a str,
    /// Optional rejection reason (only for `"refuse"`).
    pub reason: Option<&'a str>,
    /// Optional always-accept scope (only for `"always_accept"`).
    pub scope: Option<&'a str>,
}

/// Client for communicating with the Apollia runtime via Unix socket.
///
/// Each method opens a new connection (HTTP/1.1 per-request). This is fine
/// for CLI usage where request frequency is low.
pub struct RuntimeClient {
    socket_path: PathBuf,
    /// Bearer token attached to every request. Loaded best-effort from
    /// `~/.apollia/api-token`. Ignored by the Unix socket (never token-gated),
    /// required by an authenticated TCP listener (Windows, or a remote host).
    api_token: Option<String>,
}

/// Result returned by `POST /api/v1/notifications/test` for one channel.
#[derive(Debug, serde::Deserialize)]
pub struct ChannelTestResult {
    /// Unique channel identifier.
    pub channel_id: String,
    /// Channel type: `"desktop"`, `"webhook"`, or `"sse"`.
    #[serde(rename = "type")]
    pub kind: String,
    /// Status: `"ok"`, `"error"`, or `"disabled"`.
    pub status: String,
    /// Error message when `status == "error"`.
    pub error: Option<String>,
    /// Measured latency in milliseconds (`None` if the channel is disabled).
    pub latency_ms: Option<u64>,
}

/// Errors that can occur when communicating with the runtime.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ClientError {
    /// Failed to connect to the Unix socket (runtime not started).
    #[error("runtime not started (connection refused)")]
    ConnectionRefused,

    /// IO error during communication.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    /// HTTP protocol error.
    #[error("http error: {0}")]
    Http(String),

    /// Failed to parse response body as JSON.
    #[error("invalid JSON response: {0}")]
    InvalidJson(#[from] serde_json::Error),

    /// Server returned a non-success status code.
    #[error("server error ({status}): {body}")]
    ServerError {
        /// HTTP status code.
        status: u16,
        /// Response body text.
        body: String,
    },
}

/// Raw HTTP response from the runtime.
#[derive(Debug)]
pub struct RawResponse {
    /// HTTP status code.
    pub status: u16,
    /// Response body as string.
    pub body: String,
}

/// Extract a human-readable error message from a response body.
///
/// Tries to parse the body as JSON and read the `"error"` field.
/// Falls back to the raw body, or a generic message if the body is empty.
/// Percent-encode a value for use as a single URL path segment.
///
/// MCP tool names look like `mcp:server/tool`; the `/` would otherwise split the
/// path and break `:tool` routing (the tool then reads as not-found). Encodes
/// everything outside the RFC 3986 unreserved set.
fn encode_path_segment(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(b as char);
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

fn extract_error(body: &str, status: u16) -> String {
    serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|j| j.get("error").and_then(|e| e.as_str()).map(str::to_string))
        .unwrap_or_else(|| {
            if body.is_empty() {
                format!("server error (status {status}): no response body")
            } else {
                body.to_string()
            }
        })
}

/// Best-effort read of the local API bearer token from `~/.apollia/api-token`.
///
/// Returns `None` when the file is absent or empty. The token lets the CLI
/// drive an authenticated TCP listener; it is harmless on the Unix socket,
/// which is never token-gated.
fn load_default_api_token() -> Option<String> {
    let path = apollia_core::paths::data_dir()?.join("api-token");
    std::fs::read_to_string(path)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

mod agents;
mod automation;
mod backends;
mod chat;
mod integrations;

impl RuntimeClient {
    /// Create a new client targeting the given Unix socket path.
    pub fn new(socket_path: PathBuf) -> Self {
        Self {
            socket_path,
            api_token: load_default_api_token(),
        }
    }

    /// Create a client with the default socket path (`~/.apollia/runtime.sock`).
    pub fn default_client() -> Self {
        Self::new(default_socket_path())
    }

    /// Override the bearer token used for TCP authentication.
    pub fn with_token(mut self, token: Option<String>) -> Self {
        self.api_token = token;
        self
    }

    /// The `Authorization` header value, when a token is configured.
    fn auth_header(&self) -> Option<String> {
        self.api_token.as_ref().map(|t| format!("Bearer {t}"))
    }

    /// Return the socket path this client connects to.
    pub fn socket_path(&self) -> &Path {
        &self.socket_path
    }

    /// Send a GET request and return the raw response.
    pub async fn get(&self, uri: &str) -> Result<RawResponse, ClientError> {
        self.request("GET", uri, None).await
    }

    /// Send a POST request with an optional JSON body and return the raw response.
    pub async fn post(
        &self,
        uri: &str,
        body: Option<&serde_json::Value>,
    ) -> Result<RawResponse, ClientError> {
        self.request("POST", uri, body).await
    }

    /// Send a DELETE request and return the raw response.
    pub async fn delete(&self, uri: &str) -> Result<RawResponse, ClientError> {
        self.request("DELETE", uri, None).await
    }

    /// Send a PUT request with an optional JSON body and return the raw response.
    pub async fn put(
        &self,
        uri: &str,
        body: Option<&serde_json::Value>,
    ) -> Result<RawResponse, ClientError> {
        self.request("PUT", uri, body).await
    }

    /// Send a POST request with a raw body and custom content-type.
    ///
    /// Used for multipart uploads where the body is pre-built by the caller.
    pub async fn post_multipart(
        &self,
        uri: &str,
        body: &[u8],
        content_type: &str,
    ) -> Result<RawResponse, ClientError> {
        self.request_raw("POST", uri, body, content_type).await
    }
    /// Internal: send an HTTP request with a raw byte body and explicit content-type.
    async fn request_raw(
        &self,
        method: &str,
        uri: &str,
        body: &[u8],
        content_type: &str,
    ) -> Result<RawResponse, ClientError> {
        let stream = connect_runtime(&self.socket_path).await.map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound
                || e.kind() == std::io::ErrorKind::ConnectionRefused
            {
                ClientError::ConnectionRefused
            } else {
                ClientError::Io(e)
            }
        })?;

        let io = TokioIo::new(stream);
        let (mut sender, conn) = http1::handshake(io)
            .await
            .map_err(|e| ClientError::Http(e.to_string()))?;

        tokio::spawn(async move {
            if let Err(e) = conn.await {
                tracing::debug!(error = %e, "http.connection.closed");
            }
        });

        let mut builder = hyper::Request::builder()
            .method(method)
            .uri(uri)
            .header("host", "localhost")
            .header("content-type", content_type);
        if let Some(auth) = self.auth_header() {
            builder = builder.header("authorization", auth);
        }
        let req = builder
            .body(Full::new(Bytes::from(body.to_vec())))
            .map_err(|e| ClientError::Http(e.to_string()))?;

        let resp = sender
            .send_request(req)
            .await
            .map_err(|e| ClientError::Http(e.to_string()))?;

        let status = resp.status().as_u16();

        if status >= 400 {
            let body_bytes = resp
                .into_body()
                .collect()
                .await
                .map_err(|e| ClientError::Http(e.to_string()))?
                .to_bytes();
            let body_str = String::from_utf8_lossy(&body_bytes).to_string();
            return Err(ClientError::ServerError {
                status,
                body: extract_error(&body_str, status),
            });
        }

        let body_bytes = resp
            .into_body()
            .collect()
            .await
            .map_err(|e| ClientError::Http(e.to_string()))?
            .to_bytes();
        let body_str = String::from_utf8_lossy(&body_bytes).to_string();

        Ok(RawResponse {
            status,
            body: body_str,
        })
    }

    /// Internal: send an HTTP request over Unix socket.
    async fn request(
        &self,
        method: &str,
        uri: &str,
        body: Option<&serde_json::Value>,
    ) -> Result<RawResponse, ClientError> {
        let stream = connect_runtime(&self.socket_path).await.map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound
                || e.kind() == std::io::ErrorKind::ConnectionRefused
            {
                ClientError::ConnectionRefused
            } else {
                ClientError::Io(e)
            }
        })?;

        let io = TokioIo::new(stream);
        let (mut sender, conn) = http1::handshake(io)
            .await
            .map_err(|e| ClientError::Http(e.to_string()))?;

        tokio::spawn(async move {
            if let Err(e) = conn.await {
                tracing::debug!(error = %e, "http.connection.closed");
            }
        });

        let req_body = match body {
            Some(json) => Full::new(Bytes::from(
                serde_json::to_vec(json).map_err(|e| ClientError::Http(e.to_string()))?,
            )),
            None => Full::new(Bytes::new()),
        };

        let mut builder = hyper::Request::builder()
            .method(method)
            .uri(uri)
            .header("host", "localhost");

        if body.is_some() {
            builder = builder.header("content-type", "application/json");
        }
        if let Some(auth) = self.auth_header() {
            builder = builder.header("authorization", auth);
        }

        let req = builder
            .body(req_body)
            .map_err(|e| ClientError::Http(e.to_string()))?;

        let resp = sender
            .send_request(req)
            .await
            .map_err(|e| ClientError::Http(e.to_string()))?;

        let status = resp.status().as_u16();
        let body_bytes = resp
            .into_body()
            .collect()
            .await
            .map_err(|e| ClientError::Http(e.to_string()))?
            .to_bytes();
        let body_str = String::from_utf8_lossy(&body_bytes).to_string();

        Ok(RawResponse {
            status,
            body: body_str,
        })
    }
}

/// Reads HTTP body frames from an SSE response, accumulates a line buffer, and
/// pushes complete lines (stripped of trailing '\r') to `tx`. Exits when the
/// connection closes or when the receiver is dropped.
async fn pump_sse_body(
    body: hyper::body::Incoming,
    mut tx: mpsc::Sender<Result<String, ClientError>>,
) {
    let mut buffer = String::new();
    let mut pinned_body = Box::pin(body);

    loop {
        let frame_opt =
            std::future::poll_fn(|cx| hyper::body::Body::poll_frame(pinned_body.as_mut(), cx))
                .await;

        match frame_opt {
            None => break, // Server closed the connection
            Some(Err(e)) => {
                let _ = tx.send(Err(ClientError::Http(e.to_string()))).await;
                return;
            }
            Some(Ok(frame)) => {
                let Ok(data) = frame.into_data() else {
                    continue; // Skip HTTP trailers
                };
                buffer.push_str(&String::from_utf8_lossy(&data));
                if drain_sse_lines(&mut buffer, &mut tx).await.is_err() {
                    return; // Receiver dropped, stop reading
                }
            }
        }
    }

    // Flush any remaining content that arrived without a trailing newline.
    if !buffer.is_empty() {
        let _ = tx.send(Ok(std::mem::take(&mut buffer))).await;
    }
}

/// Drains all complete ('\n'-terminated) lines from `buffer`, sending each to
/// `tx`. Returns `Err(())` if the receiver has been dropped.
async fn drain_sse_lines(
    buffer: &mut String,
    tx: &mut mpsc::Sender<Result<String, ClientError>>,
) -> Result<(), ()> {
    while let Some(pos) = buffer.find('\n') {
        let line = buffer[..pos].trim_end_matches('\r').to_string();
        buffer.drain(..=pos);
        if tx.send(Ok(line)).await.is_err() {
            return Err(());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_runtime_client_connection_refused() {
        // GIVEN a nonexistent socket
        let client = RuntimeClient::new(PathBuf::from("/tmp/apollia-test-nonexistent.sock"));

        // WHEN health() is called
        let result = client.health().await;

        // THEN connection refused error
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            matches!(err, ClientError::ConnectionRefused),
            "expected ConnectionRefused, got: {err}"
        );
    }

    #[test]
    fn test_default_socket_path() {
        // GIVEN a client built with no explicit socket
        // WHEN its socket path is read
        let client = RuntimeClient::default_client();
        // THEN it is the default one, which is where the runtime listens
        assert_eq!(client.socket_path(), default_socket_path().as_path());
    }

    #[test]
    fn test_custom_socket_path() {
        // GIVEN a client built on an explicit socket path
        // WHEN its socket path is read
        let client = RuntimeClient::new(PathBuf::from("/custom/path.sock"));
        // THEN it is the one given rather than the default
        assert_eq!(client.socket_path(), Path::new("/custom/path.sock"));
    }
}
