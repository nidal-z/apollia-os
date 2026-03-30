//! Transport abstraction for MCP JSON-RPC communication.
//!
//! The [`McpTransport`] trait decouples session logic from the underlying
//! byte-level channel. Current implementation: [`StdioTransport`] (subprocess
//! stdio pipes). Future implementations: `StreamableHttpTransport`, `SseTransport`.

pub mod stdio;

use std::collections::HashMap;

use crate::config::McpServerConfig;

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

    /// The requested transport variant is not supported.
    #[error("unsupported transport: {0}")]
    Unsupported(String),
}

// ─── trait ───────────────────────────────────────────────────────────────────

/// Abstract transport for MCP JSON-RPC communication.
///
/// Each method operates on a single newline-terminated JSON-RPC message.
/// Implementors are responsible for framing; callers must not include a
/// trailing newline in `send` — the transport adds it.
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
}

// ─── factory ─────────────────────────────────────────────────────────────────

/// Create and connect a transport from a server configuration.
///
/// Dispatches on `config.transport`:
/// - `"stdio"` — spawns the subprocess and returns a [`StdioTransport`].
/// - anything else — returns [`TransportError::Unsupported`].
///
/// The `resolved_env` map must already have all `${VAR}` placeholders resolved;
/// use [`McpServerConfig::resolve_env`] before calling this function.
pub fn create_transport(
    config: &McpServerConfig,
    resolved_env: HashMap<String, String>,
) -> Result<Box<dyn McpTransport>, TransportError> {
    match config.transport.as_str() {
        "stdio" => {
            let transport = StdioTransport::spawn(&config.command, &config.args, resolved_env)?;
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
            requires_approval: false,
            init_timeout_secs: 5,
            call_timeout_secs: 5,
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
    async fn test_create_transport_streamable_http_returns_error() {
        // GIVEN a config with transport = "streamable-http" (not yet implemented)
        let config = make_config("streamable-http");
        // WHEN the factory is called
        let result = create_transport(&config, HashMap::new());
        // THEN an Unsupported error is returned
        assert!(matches!(result, Err(TransportError::Unsupported(_))));
    }
}
