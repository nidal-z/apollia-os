//! REST routes for MCP server management.
//!
//! Exposes the MCP client manager through the Apollia HTTP API under `/api/v1/mcp/`.
//! All routes extract [`McpClientManagerHandle`] from the shared [`AppState`].
//! When no MCP configuration is present (`mcp_handle` is `None`), routes that
//! require an active manager return `503 Service Unavailable`.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};

use apollia_mcp::manager::{McpServerDetail, McpServerStatus};

use crate::api::server::AppState;
use crate::coordinator::ExecutionBackend;

/// Build the MCP router with read and restart routes.
///
/// Mounted under the root router by [`crate::api::server::build_router`].
/// CRUD mutation routes will be added in a future story.
pub fn mcp_router<B: ExecutionBackend + Clone>() -> Router<AppState<B>> {
    Router::new()
        .route("/api/v1/mcp/servers", get(list_servers::<B>))
        .route("/api/v1/mcp/servers/:name", get(get_server_detail::<B>))
        .route(
            "/api/v1/mcp/servers/:name/restart",
            post(restart_server::<B>),
        )
}

/// `GET /api/v1/mcp/servers` — List all connected MCP servers with their status.
///
/// Returns an empty array when no MCP configuration is active.
async fn list_servers<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
) -> Json<Vec<McpServerStatus>> {
    let statuses = match &state.mcp_handle {
        Some(handle) => handle.status().await,
        None => Vec::new(),
    };
    Json(statuses)
}

/// `GET /api/v1/mcp/servers/:name` — Get detailed info for a specific MCP server.
///
/// Returns `404 Not Found` when no server with the given name is connected.
/// Returns `503 Service Unavailable` when MCP is not configured.
async fn get_server_detail<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path(name): Path<String>,
) -> Result<Json<McpServerDetail>, StatusCode> {
    let handle = state
        .mcp_handle
        .as_ref()
        .ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    handle
        .server_detail(&name)
        .await
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

/// `POST /api/v1/mcp/servers/:name/restart` — Restart a specific MCP server.
///
/// Stops the current session and spawns a new one with the original configuration.
/// Returns `404 Not Found` when no server with the given name is connected.
/// Returns `503 Service Unavailable` when MCP is not configured.
async fn restart_server<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path(name): Path<String>,
) -> Result<Json<McpServerStatus>, StatusCode> {
    let handle = state
        .mcp_handle
        .as_ref()
        .ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    handle
        .restart_server(&name)
        .await
        .map(Json)
        .map_err(|_| StatusCode::NOT_FOUND)
}

// ─── tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use apollia_mcp::manager::{McpServerStatus, McpToolSummary};

    #[test]
    fn test_list_servers_status_serialization() {
        // GIVEN two server status snapshots
        let statuses = vec![
            McpServerStatus {
                name: "notion".to_string(),
                server_info: "notion-mcp-server".to_string(),
                tools_count: 5,
                requires_approval: true,
                connected: true,
                pid: None,
                uptime_secs: Some(120),
                last_call_at: None,
                error: None,
                package: None,
                transport: "stdio".to_string(),
            },
            McpServerStatus {
                name: "sqlite".to_string(),
                server_info: "mcp-server-sqlite".to_string(),
                tools_count: 3,
                requires_approval: false,
                connected: true,
                pid: None,
                uptime_secs: Some(90),
                last_call_at: None,
                error: None,
                package: None,
                transport: "stdio".to_string(),
            },
        ];
        // WHEN serialized
        let json = serde_json::to_value(&statuses).unwrap();
        // THEN the array length and fields are correct
        assert_eq!(json.as_array().unwrap().len(), 2);
        assert_eq!(json[0]["name"], "notion");
        assert_eq!(json[1]["tools_count"], 3);
    }

    #[tokio::test]
    async fn test_list_servers_empty_without_mcp_handle() {
        // GIVEN no MCP handle is configured
        let mcp_handle: Option<apollia_mcp::manager::McpClientManagerHandle> = None;
        // WHEN the list is built
        let statuses = match &mcp_handle {
            Some(handle) => handle.status().await,
            None => Vec::new(),
        };
        // THEN the result is an empty list
        assert!(statuses.is_empty());
    }

    #[test]
    fn test_get_server_detail_tool_summary_fields() {
        // GIVEN a tool summary for "notion/search_pages"
        let summary = McpToolSummary {
            full_name: "mcp:notion/search_pages".to_string(),
            local_name: "search_pages".to_string(),
            description: Some("Search Notion pages".to_string()),
            input_schema: serde_json::json!({"type": "object"}),
        };
        // WHEN serialized
        let json = serde_json::to_value(&summary).unwrap();
        // THEN qualified and local names are present
        assert_eq!(json["full_name"], "mcp:notion/search_pages");
        assert_eq!(json["local_name"], "search_pages");
        assert!(json["description"].is_string());
    }
}
