//! Structured JSON Lines logger on stderr.
//!
//! Critical: stdout is reserved for the handshake communication with the
//! parent (daemon). All logs must go to stderr to avoid polluting it.

use std::io;

/// Initializes the logger in JSON Lines mode to stderr.
///
/// The default log level is `info`. It can be overridden via the `RUST_LOG`
/// environment variable (standard `tracing_subscriber::EnvFilter` format).
pub fn init() {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));

    tracing_subscriber::fmt()
        .json()
        .with_env_filter(filter)
        .with_writer(io::stderr)
        .with_target(true)
        .with_thread_ids(false)
        .with_thread_names(false)
        .init();

    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        "runner.logger.initialized"
    );
}
