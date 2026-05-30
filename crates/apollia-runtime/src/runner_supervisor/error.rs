//! Error types for the supervisor and the runner proxy.

use thiserror::Error;

/// Possible errors during spawn, health check, or execution of a runner.
#[derive(Debug, Error)]
pub enum RunnerError {
    /// The expected runner binary was not found on disk.
    #[error("runner binary not found: {0}")]
    BinaryNotFound(String),

    /// I/O error at spawn or during communication.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    /// The runner did not emit `READY <port>\n` within the allotted time (10 sec).
    #[error("runner handshake timeout after {timeout_secs}s")]
    HandshakeTimeout { timeout_secs: u64 },

    /// The `READY <port>` line is malformed (port missing, non-numeric, etc.).
    #[error("malformed READY line: {0}")]
    MalformedReady(String),

    /// The HTTP request to the runner failed (timeout, connection refused, etc.).
    #[error("http error: {0}")]
    Http(String),

    /// The runner returned a normalized IPC error.
    #[error("runner ipc error ({code:?}): {message}")]
    Ipc { code: String, message: String },

    /// The runner stopped unexpectedly (crash or non-zero exit).
    #[error("runner crashed (exit code: {0:?})")]
    Crashed(Option<i32>),

    /// JSON serialization/deserialization error.
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),

    /// The supervisor is already shutting down and refuses new calls.
    #[error("supervisor shutting down")]
    ShuttingDown,
}
