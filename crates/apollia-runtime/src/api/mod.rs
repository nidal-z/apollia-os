//! APIServer module, axum HTTP server on Unix socket + TCP.
//!
//! Provides the external API surface for the runtime (CLI, SDK, integrations).

pub mod middleware;
pub mod openapi;
mod port_file;
pub mod routes_a2a;
pub mod routes_agent_definitions;
pub mod routes_agent_packages;
pub mod routes_agents;
pub mod routes_app_lifecycle;
pub mod routes_approvals;
pub mod routes_artifacts;
pub mod routes_audit;
pub mod routes_batches;
pub mod routes_chat;
pub mod routes_chat_assists;
pub mod routes_companion;
pub mod routes_connections;
pub mod routes_connectors;
pub mod routes_drive_folders;
pub mod routes_engines;
pub mod routes_events;
pub mod routes_hooks;
pub mod routes_imap;
pub mod routes_llm;
pub mod routes_local_ai;
pub mod routes_mcp;
pub mod routes_mcp_bridge;
pub mod routes_mcp_registry;
pub mod routes_mcp_secrets;
pub mod routes_memory;
pub mod routes_memory_maintenance;
pub mod routes_messages;
pub mod routes_model_hub;
pub mod routes_notifications;
pub mod routes_oauth_clients;
pub mod routes_observability;
pub mod routes_onboarding;
pub mod routes_packages;
pub mod routes_pending;
pub mod routes_permissions;
pub mod routes_plan_cache;
pub mod routes_profile;
pub mod routes_resilience;
pub mod routes_review;
pub mod routes_server;
pub mod routes_sources;
pub mod routes_sse;
pub mod routes_stt;
pub mod routes_tasks;
pub mod routes_timeline;
pub mod routes_tool_credentials;
pub mod routes_tool_governance;
pub mod routes_tools;
pub mod routes_trace;
pub mod routes_triggers;
pub mod routes_webhooks;
pub mod routes_workflows;
pub mod server;

// The eleven API operations no CLI leaf addresses, probed against the real
// router. Test-only, and kept apart from the route modules because it is one
// instrument over eleven of them rather than a test of any single one.
#[cfg(test)]
mod unreached_by_cli;

// The TLS pair the handshake test writes to disk. Held apart from
// `server.rs` so that the private-key hook can be excused on one path
// instead of being bypassed on every commit that touched that module.
#[cfg(test)]
mod tls_test_material;

#[cfg(test)]
pub(crate) mod test_support;

pub use middleware::{load_or_generate_token, AuthError, TokenAuthLayer, TokenFileError};
pub use port_file::{read_port_file, remove_port_file, write_port_file};
pub use server::{
    APIServer, APIServerConfig, APIServerError, APIServerHandle, AppState, PRODUCT_MARKER,
};
