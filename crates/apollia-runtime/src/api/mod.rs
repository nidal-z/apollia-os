//! APIServer module — axum HTTP server on Unix socket + TCP.
//!
//! Provides the external API surface for the runtime (CLI, SDK, integrations).

pub mod middleware;
pub mod routes_a2a;
pub mod routes_agents;
pub mod routes_approvals;
pub mod routes_audit;
pub mod routes_chat;
pub mod routes_llm;
pub mod routes_mcp;
pub mod routes_messages;
pub mod routes_model_hub;
pub mod routes_notifications;
pub mod routes_pipelines;
pub mod routes_plan_cache;
pub mod routes_review;
pub mod routes_sse;
pub mod routes_stt;
pub mod routes_tasks;
pub mod routes_timeline;
pub mod routes_tools;
pub mod routes_triggers;
pub mod routes_user;
pub mod routes_webhooks;
pub mod server;

pub use middleware::{load_or_generate_token, AuthError, TokenAuthLayer, TokenFileError};
pub use server::{APIServer, APIServerConfig, APIServerError, APIServerHandle, AppState};
