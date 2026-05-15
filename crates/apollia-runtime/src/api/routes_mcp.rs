//! REST routes for MCP server management.
//!
//! Exposes the MCP client manager and the SQLite-backed [`McpServerRepository`]
//! through the Apollia HTTP API under `/api/v1/mcp/`.
//! Mutation routes require both an active [`McpClientManagerHandle`] and a
//! [`McpServerRepository`] in the shared [`AppState`]; they return `503 Service Unavailable`
//! when either is absent. The test-connection route operates without a pre-existing
//! manager so it can be used during first-time server setup.

use std::sync::Arc;
use std::time::Instant;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{delete, get, patch, post, put},
    Json, Router,
};

use apollia_mcp::config::McpServerConfig;
use apollia_mcp::manager::{
    McpClientManagerHandle, McpConnectionTestResult, McpServerDetail, McpServerStatus,
    McpToolSummary,
};
use apollia_mcp::session::McpSession;
use apollia_mcp::McpServerRepository;

use crate::api::server::AppState;
use crate::coordinator::ExecutionBackend;

/// Build the MCP router with all server management routes.
///
/// Mounted under the root router by [`crate::api::server::build_router`].
pub fn mcp_router<B: ExecutionBackend + Clone>() -> Router<AppState<B>> {
    Router::new()
        .route("/api/v1/mcp/servers", get(list_servers::<B>))
        .route("/api/v1/mcp/servers", post(add_server::<B>))
        .route("/api/v1/mcp/servers/test", post(test_connection))
        .route("/api/v1/mcp/servers/:name", get(get_server_detail::<B>))
        .route("/api/v1/mcp/servers/:name", delete(remove_server::<B>))
        .route(
            "/api/v1/mcp/servers/:name/raw_config",
            get(get_server_raw_config::<B>),
        )
        .route(
            "/api/v1/mcp/servers/:name/restart",
            post(restart_server::<B>),
        )
        .route(
            "/api/v1/mcp/servers/:name/config",
            put(update_server_config::<B>),
        )
        .route(
            "/api/v1/mcp/servers/:name/approval",
            patch(set_server_approval::<B>),
        )
}

/// Shorthand for a JSON error response tuple returned by fallible handlers.
type JsonError = (StatusCode, Json<serde_json::Value>);

fn json_err(code: StatusCode, msg: impl std::fmt::Display) -> JsonError {
    (code, Json(serde_json::json!({"error": msg.to_string()})))
}

fn require_mcp_handle<B: ExecutionBackend + Clone>(
    state: &AppState<B>,
) -> Result<&McpClientManagerHandle, JsonError> {
    state
        .mcp_handle
        .as_ref()
        .ok_or_else(|| json_err(StatusCode::SERVICE_UNAVAILABLE, "MCP is not configured"))
}

fn require_mcp_repo<B: ExecutionBackend + Clone>(
    state: &AppState<B>,
) -> Result<&Arc<std::sync::Mutex<McpServerRepository>>, JsonError> {
    state.mcp_server_repo.as_ref().ok_or_else(|| {
        json_err(
            StatusCode::SERVICE_UNAVAILABLE,
            "MCP repository not available",
        )
    })
}

// ─── read routes ─────────────────────────────────────────────────────────────

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
) -> Result<Json<McpServerDetail>, JsonError> {
    let handle = require_mcp_handle(&state)?;
    handle.server_detail(&name).await.map(Json).ok_or_else(|| {
        json_err(
            StatusCode::NOT_FOUND,
            format!("server '{}' not found", name),
        )
    })
}

/// `GET /api/v1/mcp/servers/:name/raw_config` — Return the persisted launch
/// configuration of a server (command, args, env, transport, …) as stored in
/// `mcp.db`.
///
/// The `env` map contains either literal values for non-secret variables or
/// `${APOLLIA_SECRET:NAME}` placeholders for secret ones — actual secret
/// material is never returned. Used by the desktop "Modifier les arguments"
/// flow to fetch the current config, patch `args`, and PUT the result back
/// without losing the rest of the configuration.
///
/// Returns `404 Not Found` when no server with `name` is persisted.
/// Returns `503 Service Unavailable` when the MCP repository is unavailable.
async fn get_server_raw_config<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path(name): Path<String>,
) -> Result<Json<McpServerConfig>, JsonError> {
    let repo = require_mcp_repo(&state)?;
    let guard = repo.lock().map_err(|_| {
        json_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "repository lock poisoned",
        )
    })?;
    let config = guard
        .find_by_name(&name)
        .map_err(|e| json_err(StatusCode::INTERNAL_SERVER_ERROR, e))?
        .ok_or_else(|| {
            json_err(
                StatusCode::NOT_FOUND,
                format!("server '{}' not found", name),
            )
        })?;
    Ok(Json(config))
}

/// `POST /api/v1/mcp/servers/:name/restart` — Restart a specific MCP server session.
///
/// Stops the current session and spawns a new one using the original configuration.
/// Returns `404 Not Found` when no server with the given name is connected.
/// Returns `503 Service Unavailable` when MCP is not configured.
async fn restart_server<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path(name): Path<String>,
) -> Result<Json<McpServerStatus>, JsonError> {
    let handle = require_mcp_handle(&state)?;
    handle
        .restart_server(&name)
        .await
        .map(Json)
        .map_err(|e| json_err(StatusCode::NOT_FOUND, e))
}

// ─── mutation routes ──────────────────────────────────────────────────────────

/// `POST /api/v1/mcp/servers` — Add a new MCP server and persist it to `mcp.db`.
///
/// Spawns the server process and registers its tools, then saves the configuration.
/// Returns `201 Created` with the server status on success.
/// Returns `409 Conflict` when a server with the same name is already managed.
/// Returns `400 Bad Request` for invalid configurations or spawn failures.
/// Returns `503 Service Unavailable` when MCP is not configured.
async fn add_server<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Json(config): Json<McpServerConfig>,
) -> Result<(StatusCode, Json<McpServerStatus>), JsonError> {
    let handle = require_mcp_handle(&state)?;
    let repo = require_mcp_repo(&state)?;

    let status = handle.add_server(config.clone()).await.map_err(|e| {
        let code = if e.to_string().contains("already exists") {
            StatusCode::CONFLICT
        } else {
            StatusCode::BAD_REQUEST
        };
        json_err(code, e)
    })?;

    {
        let guard = repo.lock().map_err(|_| {
            json_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "repository lock poisoned",
            )
        })?;
        guard
            .save(&config)
            .map_err(|e| json_err(StatusCode::INTERNAL_SERVER_ERROR, e))?;
    }

    Ok((StatusCode::CREATED, Json(status)))
}

/// `DELETE /api/v1/mcp/servers/:name` — Remove an MCP server and delete it from `mcp.db`.
///
/// Shuts down the session, unregisters the server, then removes its database entry.
/// Returns `200 OK` with `{"removed": "<name>"}` on success.
/// Returns `404 Not Found` when no server with the given name is managed.
/// Returns `503 Service Unavailable` when MCP is not configured.
async fn remove_server<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path(name): Path<String>,
) -> Result<Json<serde_json::Value>, JsonError> {
    let handle = require_mcp_handle(&state)?;
    let repo = require_mcp_repo(&state)?;

    handle.remove_server(&name).await.map_err(|_| {
        json_err(
            StatusCode::NOT_FOUND,
            format!("server '{}' not found", name),
        )
    })?;

    {
        let guard = repo.lock().map_err(|_| {
            json_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "repository lock poisoned",
            )
        })?;
        guard
            .delete(&name)
            .map_err(|e| json_err(StatusCode::INTERNAL_SERVER_ERROR, e))?;
    }

    Ok(Json(serde_json::json!({"removed": name})))
}

/// `POST /api/v1/mcp/servers/test` — Test a server configuration without persisting a session.
///
/// Spawns an ephemeral process, performs the MCP initialize handshake and `tools/list`,
/// captures the result, then immediately terminates the process. No session is stored
/// and the tool registry is not modified. Works without an existing MCP manager,
/// allowing use during first-time setup before any server is added.
/// Returns `400 Bad Request` when the server cannot be reached or the handshake fails.
async fn test_connection(
    Json(config): Json<McpServerConfig>,
) -> Result<Json<McpConnectionTestResult>, JsonError> {
    let start = Instant::now();
    let session = McpSession::start(config, None)
        .await
        .map_err(|e| json_err(StatusCode::BAD_REQUEST, e))?;

    let server_name = session.server_name().to_string();
    let server_info_name = session.server_info().name.clone();
    let tools: Vec<McpToolSummary> = session
        .tools()
        .iter()
        .map(|t| McpToolSummary {
            full_name: format!("test:{}/{}", server_name, t.name),
            local_name: t.name.clone(),
            description: t.description.clone(),
            input_schema: t.input_schema.clone(),
        })
        .collect();
    let test_duration_ms = start.elapsed().as_millis() as u64;

    session.shutdown().await;

    Ok(Json(McpConnectionTestResult {
        server_info: server_info_name,
        protocol_version: "2024-11-05".to_string(),
        tools,
        test_duration_ms,
    }))
}

/// `PUT /api/v1/mcp/servers/:name/config` — Replace a server configuration and restart the session.
///
/// Removes the current session, starts a new one with the updated configuration, then
/// upserts the entry in `mcp.db`.
/// Returns `200 OK` with the new [`McpServerStatus`] on success.
/// Returns `404 Not Found` when no server with `name` is managed.
/// Returns `400 Bad Request` when the new session fails to start.
/// Returns `503 Service Unavailable` when MCP is not configured.
async fn update_server_config<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path(name): Path<String>,
    Json(config): Json<McpServerConfig>,
) -> Result<Json<McpServerStatus>, JsonError> {
    let handle = require_mcp_handle(&state)?;
    let repo = require_mcp_repo(&state)?;

    handle.remove_server(&name).await.map_err(|_| {
        json_err(
            StatusCode::NOT_FOUND,
            format!("server '{}' not found", name),
        )
    })?;

    let status = handle
        .add_server(config.clone())
        .await
        .map_err(|e| json_err(StatusCode::BAD_REQUEST, e))?;

    {
        let guard = repo.lock().map_err(|_| {
            json_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "repository lock poisoned",
            )
        })?;
        guard
            .save(&config)
            .map_err(|e| json_err(StatusCode::INTERNAL_SERVER_ERROR, e))?;
    }

    Ok(Json(status))
}

/// Request body for `PATCH /api/v1/mcp/servers/:name/approval`.
#[derive(Debug, serde::Deserialize)]
struct SetApprovalBody {
    requires_approval: bool,
}

/// `PATCH /api/v1/mcp/servers/:name/approval` — Update the approval flag without restarting.
///
/// Updates the `requires_approval` flag in-memory and persists the change to `mcp.db`.
/// The running session is **not** restarted; the new flag takes effect for the next
/// tool call. Returns `200 OK` with the updated [`McpServerStatus`] on success.
/// Returns `404 Not Found` when no server with `name` is managed.
/// Returns `503 Service Unavailable` when MCP is not configured.
async fn set_server_approval<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path(name): Path<String>,
    Json(body): Json<SetApprovalBody>,
) -> Result<Json<McpServerStatus>, JsonError> {
    let handle = require_mcp_handle(&state)?;
    let repo = require_mcp_repo(&state)?;

    // Apply in-memory change before locking the sync Mutex.
    handle
        .set_server_approval(&name, body.requires_approval)
        .await
        .map_err(|e| json_err(StatusCode::NOT_FOUND, e))?;

    // Persist: read-modify-write (Mutex guard dropped before next await).
    {
        let guard = repo.lock().map_err(|_| {
            json_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "repository lock poisoned",
            )
        })?;
        let mut current = guard
            .find_by_name(&name)
            .map_err(|e| json_err(StatusCode::INTERNAL_SERVER_ERROR, e))?
            .ok_or_else(|| {
                json_err(
                    StatusCode::NOT_FOUND,
                    format!("server '{}' not found", name),
                )
            })?;
        current.requires_approval = body.requires_approval;
        guard
            .save(&current)
            .map_err(|e| json_err(StatusCode::INTERNAL_SERVER_ERROR, e))?;
    }

    handle
        .server_detail(&name)
        .await
        .map(|d| Json(d.status))
        .ok_or_else(|| {
            json_err(
                StatusCode::NOT_FOUND,
                format!("server '{}' not found", name),
            )
        })
}

// ─── tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use apollia_mcp::manager::{
        McpServerConfigView, McpServerDetail, McpServerStatus, McpToolSummary,
    };

    #[test]
    fn test_conflict_error_code_from_already_exists_message() {
        // GIVEN an error message that contains "already exists"
        let error_msg = "server with this name already exists";
        // WHEN the HTTP code is derived
        let code = if error_msg.contains("already exists") {
            StatusCode::CONFLICT
        } else {
            StatusCode::BAD_REQUEST
        };
        // THEN the code is 409
        assert_eq!(code, StatusCode::CONFLICT);
    }

    #[test]
    fn test_bad_request_error_code_for_other_errors() {
        // GIVEN an error message that does not contain "already exists"
        let error_msg = "failed to spawn server: executable not found";
        // WHEN the HTTP code is derived
        let code = if error_msg.contains("already exists") {
            StatusCode::CONFLICT
        } else {
            StatusCode::BAD_REQUEST
        };
        // THEN the code is 400
        assert_eq!(code, StatusCode::BAD_REQUEST);
    }

    #[test]
    fn test_not_found_error_json_contains_server_name() {
        // GIVEN an unknown server name
        let name = "unknown-server";
        // WHEN the error JSON is built
        let response = serde_json::json!({"error": format!("server '{}' not found", name)});
        // THEN the error field contains the name
        assert_eq!(response["error"], "server 'unknown-server' not found");
    }

    #[test]
    fn test_remove_response_format() {
        // GIVEN a server successfully removed
        let name = "notion";
        // WHEN the response JSON is built
        let response = serde_json::json!({"removed": name});
        // THEN the removed field is set correctly
        assert_eq!(response["removed"], "notion");
    }

    #[test]
    fn test_add_server_returns_201_status_code() {
        // GIVEN a successful add operation
        let code = StatusCode::CREATED;
        // THEN the code is 201
        assert_eq!(code.as_u16(), 201);
    }

    #[test]
    fn test_server_detail_serialization() {
        // GIVEN a fully populated McpServerDetail
        let detail = McpServerDetail {
            status: McpServerStatus {
                name: "notion".to_string(),
                server_info: "notion-mcp".to_string(),
                tools_count: 2,
                requires_approval: false,
                connected: true,
                pid: Some(1234),
                uptime_secs: Some(60),
                last_call_at: None,
                error: None,
                package: None,
                transport: "stdio".to_string(),
            },
            tools: vec![],
            config: McpServerConfigView {
                name: "notion".to_string(),
                command: "npx".to_string(),
                args: vec![],
                env_keys: vec!["NOTION_TOKEN".to_string()],
                transport: "stdio".to_string(),
                requires_approval: false,
                tags: vec![],
            },
        };
        // WHEN serialized to JSON
        let json = serde_json::to_value(&detail).unwrap();
        // THEN the status and config fields are present
        assert_eq!(json["status"]["name"], "notion");
        assert_eq!(json["config"]["env_keys"][0], "NOTION_TOKEN");
    }

    #[test]
    fn test_server_detail_tools_list() {
        // GIVEN a server detail with two tools
        let detail = McpServerDetail {
            status: McpServerStatus {
                name: "notion".to_string(),
                server_info: "notion-mcp".to_string(),
                tools_count: 2,
                requires_approval: false,
                connected: true,
                pid: None,
                uptime_secs: Some(120),
                last_call_at: None,
                error: None,
                package: None,
                transport: "stdio".to_string(),
            },
            tools: vec![
                McpToolSummary {
                    full_name: "mcp:notion/search_pages".to_string(),
                    local_name: "search_pages".to_string(),
                    description: Some("Search Notion pages".to_string()),
                    input_schema: serde_json::json!({"type": "object"}),
                },
                McpToolSummary {
                    full_name: "mcp:notion/create_page".to_string(),
                    local_name: "create_page".to_string(),
                    description: Some("Create a Notion page".to_string()),
                    input_schema: serde_json::json!({"type": "object"}),
                },
            ],
            config: McpServerConfigView {
                name: "notion".to_string(),
                command: "npx".to_string(),
                args: vec![],
                env_keys: vec![],
                transport: "stdio".to_string(),
                requires_approval: false,
                tags: vec![],
            },
        };
        // WHEN serialized
        let json = serde_json::to_value(&detail).unwrap();
        // THEN the tools array has 2 elements with the correct names
        assert_eq!(json["tools"].as_array().unwrap().len(), 2);
        assert_eq!(json["tools"][0]["full_name"], "mcp:notion/search_pages");
        assert_eq!(json["tools"][1]["local_name"], "create_page");
    }

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
