//! APIServer module — axum HTTP server on Unix socket + TCP.
//!
//! Provides the external API surface for the runtime (CLI, SDK, integrations).

pub mod server;

pub use server::{APIServer, APIServerConfig, APIServerError, APIServerHandle, AppState};
