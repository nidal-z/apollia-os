//! Transport abstraction for MCP JSON-RPC communication.
//!
//! The [`McpTransport`] trait decouples session logic from the underlying
//! byte-level channel. Implementations:
//! - [`StdioTransport`]: subprocess stdio pipes.
//! - [`StreamableHttpTransport`]: single HTTP POST per request to a remote server.
//! - [`SseTransport`]: persistent SSE stream with HTTP POST for requests.

pub mod http;
pub mod sse;
pub mod stdio;

use std::collections::HashMap;
use std::time::Duration;

use crate::config::McpServerConfig;

pub use http::StreamableHttpTransport;
pub use sse::SseTransport;
pub use stdio::StdioTransport;

// ─── errors ──────────────────────────────────────────────────────────────────

/// Errors that can occur at the transport layer.
#[derive(Debug, thiserror::Error)]
pub enum TransportError {
    /// A transport-level I/O operation failed.
    #[error("transport I/O error: {0}")]
    Io(String),

    /// The subprocess could not be spawned.
    #[error("subprocess spawn failed: {0}")]
    SpawnFailed(String),

    /// The transport channel was closed (remote end exited or pipe broken).
    #[error("transport closed")]
    Closed,

    /// A single server response exceeded the configured byte cap.
    ///
    /// MCP servers are untrusted; this bounds memory when a server never
    /// terminates a line, streams without end, or returns an oversized body.
    /// `limit` is the `max_response_bytes` ceiling that was exceeded.
    #[error("MCP server response exceeded the {limit}-byte cap")]
    ResponseTooLarge {
        /// The `max_response_bytes` ceiling that was exceeded.
        limit: u64,
    },

    /// The requested transport variant is not supported.
    #[error("unsupported transport: {0}")]
    Unsupported(String),

    /// The remote MCP server responded with HTTP 401 Unauthorized.
    ///
    /// `www_authenticate` carries the `WWW-Authenticate` header value verbatim
    /// (or an empty string when absent). The orchestration layer is expected
    /// to parse it (RFC 9728 `Bearer resource_metadata="..."` form) and drive
    /// the MCP OAuth 2.1 handshake before retrying the transport.
    #[error("MCP server returned 401 Unauthorized; OAuth handshake required")]
    Unauthorized {
        /// `WWW-Authenticate` header from the 401 response, empty if absent.
        www_authenticate: String,
    },
}

// ─── trait ───────────────────────────────────────────────────────────────────

/// Abstract transport for MCP JSON-RPC communication.
///
/// Each method operates on a single newline-terminated JSON-RPC message.
/// Implementors are responsible for framing; callers must not include a
/// trailing newline in `send`; the transport adds it.
///
/// The trait is `Send + Sync + 'static` so it can be held behind an `Arc`
/// and shared between the session main task and the background dispatch task.
#[async_trait::async_trait]
pub trait McpTransport: Send + Sync + 'static {
    /// Write a single JSON-RPC message to the server.
    ///
    /// The transport appends the required newline terminator.
    async fn send(&self, message: &str) -> Result<(), TransportError>;

    /// Read the next JSON-RPC message from the server.
    ///
    /// Blocks until a complete newline-terminated line is available.
    /// Returns [`TransportError::Closed`] when the server output stream ends.
    async fn recv(&self) -> Result<String, TransportError>;

    /// Gracefully terminate the transport connection.
    ///
    /// After `shutdown` returns, further `send` and `recv` calls may fail.
    async fn shutdown(&self) -> Result<(), TransportError>;

    /// Returns the OS process ID of the server process, if applicable.
    ///
    /// Subprocess-based transports return the child PID captured at spawn time.
    /// Network-based transports return `None`.
    fn pid(&self) -> Option<u32> {
        None
    }

    /// Returns the most recent stderr lines produced by the server, oldest
    /// first. Only meaningful for subprocess-based transports; network
    /// transports return an empty vec. Used by the session layer to enrich
    /// `TransportError` messages on handshake / tool-call failures so the
    /// operator sees what the subprocess actually wrote before stalling.
    fn stderr_tail(&self) -> Vec<String> {
        Vec::new()
    }
}

// factory

/// Create and connect a transport from a server configuration.
///
/// Dispatches on `config.transport`:
/// - `"stdio"`: spawns the subprocess and returns a [`StdioTransport`].
/// - `"streamable-http"`: builds a [`StreamableHttpTransport`] targeting `config.url`.
/// - `"sse"`: builds an [`SseTransport`] targeting `config.url`.
/// - anything else: returns [`TransportError::Unsupported`].
///
/// The `resolved_env` map must already have all `${VAR}` placeholders resolved;
/// use [`McpServerConfig::resolve_env`] before calling this function. For
/// network transports, entries in `resolved_env` are forwarded as HTTP headers
/// (e.g. `Authorization: Bearer …`).
pub fn create_transport(
    config: &McpServerConfig,
    resolved_env: HashMap<String, String>,
) -> Result<Box<dyn McpTransport>, TransportError> {
    let max_response_bytes = config.max_response_bytes;
    match config.transport.as_str() {
        "stdio" => {
            let transport = StdioTransport::spawn(
                &config.name,
                &config.command,
                &config.args,
                resolved_env,
                max_response_bytes,
            )?;
            Ok(Box::new(transport))
        }
        "streamable-http" => {
            let url = config.url.as_deref().ok_or_else(|| {
                TransportError::Unsupported(
                    "streamable-http transport requires a 'url' field in config".to_string(),
                )
            })?;
            let auth_headers: Vec<(String, String)> = resolved_env.into_iter().collect();
            let timeout = Duration::from_secs(config.call_timeout_secs);
            let transport =
                StreamableHttpTransport::new(url, auth_headers, timeout, max_response_bytes)?;
            Ok(Box::new(transport))
        }
        "sse" => {
            let url = config.url.as_deref().ok_or_else(|| {
                TransportError::Unsupported(
                    "sse transport requires a 'url' field in config".to_string(),
                )
            })?;
            let auth_headers: Vec<(String, String)> = resolved_env.into_iter().collect();
            let timeout = Duration::from_secs(config.call_timeout_secs);
            let transport = SseTransport::new(url, auth_headers, timeout, max_response_bytes)?;
            Ok(Box::new(transport))
        }
        other => Err(TransportError::Unsupported(other.to_string())),
    }
}

// ─── tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn make_config(transport: &str) -> McpServerConfig {
        McpServerConfig {
            name: "test".to_string(),
            command: "cat".to_string(),
            args: vec![],
            env: HashMap::new(),
            transport: transport.to_string(),
            url: None,
            requires_approval: false,
            init_timeout_secs: 5,
            call_timeout_secs: 5,
            max_response_bytes: 8 * 1024 * 1024,
            tags: vec![],
        }
    }

    #[tokio::test]
    async fn test_create_transport_stdio() {
        // GIVEN a config with transport = "stdio" and a valid command
        let config = make_config("stdio");
        // WHEN the factory is called
        let result = create_transport(&config, HashMap::new());
        // THEN a transport is returned successfully
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_create_transport_unknown_returns_error() {
        // GIVEN a config with an unknown transport value
        let config = make_config("unknown");
        // WHEN the factory is called
        let result = create_transport(&config, HashMap::new());
        // THEN an Unsupported error is returned
        assert!(matches!(result, Err(TransportError::Unsupported(_))));
    }

    #[tokio::test]
    async fn test_create_transport_streamable_http_returns_ok() {
        // GIVEN a config with transport = "streamable-http" and a url
        let mut config = make_config("streamable-http");
        config.url = Some("https://mcp.example.com/mcp".to_string());
        // WHEN the factory is called
        let result = create_transport(&config, HashMap::new());
        // THEN a transport is returned (no connection is made at construction time)
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_create_transport_streamable_http_without_url_returns_error() {
        // GIVEN a config with transport = "streamable-http" but no url
        let config = make_config("streamable-http");
        // WHEN the factory is called
        let result = create_transport(&config, HashMap::new());
        // THEN an Unsupported error is returned
        assert!(matches!(result, Err(TransportError::Unsupported(_))));
    }

    #[tokio::test]
    async fn test_create_transport_sse_returns_ok() {
        // GIVEN a config with transport = "sse" and a url (no connection is made at construction)
        let mut config = make_config("sse");
        config.url = Some("https://mcp.example.com/sse".to_string());
        // WHEN the factory is called
        let result = create_transport(&config, HashMap::new());
        // THEN an SseTransport is returned successfully
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_create_transport_sse_without_url_returns_error() {
        // GIVEN a config with transport = "sse" but no url
        let config = make_config("sse");
        // WHEN the factory is called
        let result = create_transport(&config, HashMap::new());
        // THEN an Unsupported error is returned
        assert!(matches!(result, Err(TransportError::Unsupported(_))));
    }
}
