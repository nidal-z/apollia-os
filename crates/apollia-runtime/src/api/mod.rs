//! APIServer module — axum HTTP server on Unix socket + TCP.
//!
//! Provides the external API surface for the runtime (CLI, SDK, integrations).

pub mod routes_agents;
pub mod routes_llm;
pub mod routes_sse;
pub mod routes_tasks;
pub mod routes_triggers;
pub mod routes_webhooks;
pub mod server;

pub use server::{APIServer, APIServerConfig, APIServerError, APIServerHandle, AppState};
