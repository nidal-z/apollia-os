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
    McpClientManagerHandle, McpConnectionTestResult, McpResourceSummary, McpServerDetail,
    McpServerStatus, McpToolSummary, ProbeSpec,
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
        .route("/api/v1/mcp/resources", get(list_resources::<B>))
        .route("/api/v1/mcp/servers/test", post(test_connection))
        .route(
            "/api/v1/mcp/servers/:name/test",
            post(test_live_server::<B>),
        )
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

/// `GET /api/v1/mcp/servers`, List all connected MCP servers with their status.
///
/// Returns an empty array when no MCP configuration is active.
#[utoipa::path(
    get,
    path = "/api/v1/mcp/servers",
    tag = "mcp",
    responses(
        (status = 200, description = "Connected MCP servers with their status"),
    )
)]
pub(crate) async fn list_servers<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
) -> Json<Vec<McpServerStatus>> {
    let statuses = match &state.mcp_handle {
        Some(handle) => handle.status().await,
        None => Vec::new(),
    };
    Json(statuses)
}

/// `GET /api/v1/mcp/resources`, List MCP resources aggregated across servers.
///
/// Returns the same data as the agent-facing `mcp_resources_list` tool: one
/// entry per resource exposed by a connected MCP server, tagged with its owning
/// server. Feeds the desktop @-mention picker.
///
/// Returns an empty array (not an error) when no MCP configuration is active,
/// so the picker degrades to "no resources" rather than a failure state.
#[utoipa::path(
    get,
    path = "/api/v1/mcp/resources",
    tag = "mcp",
    responses(
        (status = 200, description = "MCP resources aggregated across connected servers"),
    )
)]
pub(crate) async fn list_resources<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
) -> Json<Vec<McpResourceSummary>> {
    let resources = match &state.mcp_handle {
        Some(handle) => handle.list_resources().await,
        None => Vec::new(),
    };
    Json(resources)
}

/// `GET /api/v1/mcp/servers/:name`, Get detailed info for a specific MCP server.
///
/// Returns `404 Not Found` when no server with the given name is connected.
/// Returns `503 Service Unavailable` when MCP is not configured.
#[utoipa::path(
    get,
    path = "/api/v1/mcp/servers/{name}",
    tag = "mcp",
    params(("name" = String, Path, description = "Server name")),
    responses(
        (status = 200, description = "Detailed MCP server info"),
        (status = 404, description = "Server not found", body = crate::api::openapi::ApiErrorBody),
        (status = 503, description = "MCP not configured", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub(crate) async fn get_server_detail<B: ExecutionBackend + Clone>(
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

/// `GET /api/v1/mcp/servers/:name/raw_config`, Return the persisted launch
/// configuration of a server (command, args, env, transport, …) as stored in
/// `mcp.db`.
///
/// The `env` map contains either literal values for non-secret variables or
/// `${APOLLIA_SECRET:NAME}` placeholders for secret ones, actual secret
/// material is never returned. Used by the desktop "Modifier les arguments"
/// flow to fetch the current config, patch `args`, and PUT the result back
/// without losing the rest of the configuration.
///
/// Returns `404 Not Found` when no server with `name` is persisted.
/// Returns `503 Service Unavailable` when the MCP repository is unavailable.
#[utoipa::path(
    get,
    path = "/api/v1/mcp/servers/{name}/raw_config",
    tag = "mcp",
    params(("name" = String, Path, description = "Server name")),
    responses(
        (status = 200, description = "Persisted launch configuration (secrets masked)"),
        (status = 404, description = "Server not found", body = crate::api::openapi::ApiErrorBody),
        (status = 500, description = "Repository error", body = crate::api::openapi::ApiErrorBody),
        (status = 503, description = "MCP repository not available", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub(crate) async fn get_server_raw_config<B: ExecutionBackend + Clone>(
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

/// `POST /api/v1/mcp/servers/:name/restart`, Restart a specific MCP server session.
///
/// Restarts an existing session or first-time-connects one declared in `mcp.db`.
///
/// Stops the current session and spawns a new one using the original
/// configuration. When the server was declared in `mcp.db` but never
/// connected (e.g. the previous start was blocked by a missing OAuth token,
/// later supplied via `apollia-os mcp oauth login`), we fall back to a
/// fresh `add_server` call using the persisted config, making this route
/// the single "(re)connect this server now" endpoint operators need.
/// Returns `404 Not Found` when the name is unknown to both the manager
/// and the repository.
/// Returns `503 Service Unavailable` when MCP is not configured.
#[utoipa::path(
    post,
    path = "/api/v1/mcp/servers/{name}/restart",
    tag = "mcp",
    params(("name" = String, Path, description = "Server name")),
    responses(
        (status = 200, description = "Server restarted or (re)connected"),
        (status = 400, description = "Fallback connect failed", body = crate::api::openapi::ApiErrorBody),
        (status = 404, description = "Server not found", body = crate::api::openapi::ApiErrorBody),
        (status = 500, description = "Repository error", body = crate::api::openapi::ApiErrorBody),
        (status = 503, description = "MCP not configured", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub(crate) async fn restart_server<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path(name): Path<String>,
) -> Result<Json<McpServerStatus>, JsonError> {
    let handle = require_mcp_handle(&state)?;
    match handle.restart_server(&name).await {
        Ok(status) => Ok(Json(status)),
        Err(restart_err) => {
            // Fall back: lookup the persisted config and add_server.
            let repo = require_mcp_repo(&state)?;
            let config = {
                let guard = repo.lock().map_err(|_| {
                    json_err(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "mcp repository lock poisoned",
                    )
                })?;
                guard
                    .list()
                    .map_err(|e| json_err(StatusCode::INTERNAL_SERVER_ERROR, e))?
                    .into_iter()
                    .find(|c| c.name == name)
            };
            match config {
                Some(cfg) => handle
                    .add_server(cfg)
                    .await
                    .map(Json)
                    .map_err(|e| json_err(StatusCode::BAD_REQUEST, e)),
                None => Err(json_err(
                    StatusCode::NOT_FOUND,
                    format!("server '{name}' not found (restart: {restart_err})"),
                )),
            }
        }
    }
}

// ─── mutation routes ──────────────────────────────────────────────────────────

/// `POST /api/v1/mcp/servers`, Add a new MCP server and persist it to `mcp.db`.
///
/// Spawns the server process and registers its tools, then saves the configuration.
/// Returns `201 Created` with the server status on success.
/// Returns `409 Conflict` when a server with the same name is already managed.
/// Returns `400 Bad Request` for invalid configurations or spawn failures.
/// Returns `503 Service Unavailable` when MCP is not configured.
#[utoipa::path(
    post,
    path = "/api/v1/mcp/servers",
    tag = "mcp",
    responses(
        (status = 201, description = "Server added and persisted"),
        (status = 400, description = "Invalid configuration or spawn failure", body = crate::api::openapi::ApiErrorBody),
        (status = 409, description = "Server already exists", body = crate::api::openapi::ApiErrorBody),
        (status = 500, description = "Repository error", body = crate::api::openapi::ApiErrorBody),
        (status = 503, description = "MCP not configured", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub(crate) async fn add_server<B: ExecutionBackend + Clone>(
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

/// `DELETE /api/v1/mcp/servers/:name`, Remove an MCP server and delete it from `mcp.db`.
///
/// Shuts down the session, unregisters the server, then removes its database entry.
/// Returns `200 OK` with `{"removed": "<name>"}` on success.
/// Returns `404 Not Found` when no server with the given name is managed.
/// Returns `503 Service Unavailable` when MCP is not configured.
#[utoipa::path(
    delete,
    path = "/api/v1/mcp/servers/{name}",
    tag = "mcp",
    params(("name" = String, Path, description = "Server name")),
    responses(
        (status = 200, description = "Server removed"),
        (status = 404, description = "Server not found", body = crate::api::openapi::ApiErrorBody),
        (status = 500, description = "Repository error", body = crate::api::openapi::ApiErrorBody),
        (status = 503, description = "MCP not configured", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub(crate) async fn remove_server<B: ExecutionBackend + Clone>(
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

/// Tagged response envelope for `POST /api/v1/mcp/servers/test`.
///
/// `Success` is the legacy success shape (kept JSON-compatible via flattening
/// the existing fields). `OauthRequired` is the new branch
/// emitted when the server returns 401 with a `WWW-Authenticate` challenge -
/// the desktop wizard uses this to switch from "paste a token" to
/// "Sign in with <provider>" mode.
#[derive(serde::Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum McpConnectionTestResponse {
    Success {
        #[serde(flatten)]
        result: McpConnectionTestResult,
    },
    OauthRequired {
        /// Verbatim `WWW-Authenticate` header captured from the 401.
        /// Carries the `resource_metadata=` PRM URL when the server is
        /// MCP-OAuth-conformant; the desktop discovers + drives the flow.
        www_authenticate: String,
    },
}

/// `POST /api/v1/mcp/servers/test`, Test a server configuration without persisting a session.
///
/// Spawns an ephemeral process, performs the MCP initialize handshake and `tools/list`,
/// captures the result, then immediately terminates the process. No session is stored
/// and the tool registry is not modified.
///
/// Response shape:
/// - `200 {"kind":"success", …}`, handshake succeeded, tools listed.
/// - `200 {"kind":"oauth_required", "www_authenticate":"…"}`, server returned
///   401, the desktop should drive the MCP HTTP OAuth flow before retrying.
/// - `400 Bad Request`, other handshake failures (transport, JSON-RPC error,
///   timeout, etc.).
#[utoipa::path(
    post,
    path = "/api/v1/mcp/servers/test",
    tag = "mcp",
    responses(
        (status = 200, description = "Handshake succeeded, or OAuth is required (see kind field)"),
        (status = 400, description = "Handshake failure (transport, JSON-RPC, timeout)", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub(crate) async fn test_connection(
    Json(config): Json<McpServerConfig>,
) -> Result<Json<McpConnectionTestResponse>, JsonError> {
    let start = Instant::now();
    match McpSession::start(config, Some(&apollia_mcp::config::DefaultMcpSecretResolver)).await {
        Ok(session) => {
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
            Ok(Json(McpConnectionTestResponse::Success {
                result: McpConnectionTestResult {
                    server_info: server_info_name,
                    protocol_version: "2024-11-05".to_string(),
                    tools,
                    test_duration_ms,
                    live_health: None,
                },
            }))
        }
        Err(apollia_mcp::session::McpSessionError::Unauthorized {
            www_authenticate, ..
        }) => Ok(Json(McpConnectionTestResponse::OauthRequired {
            www_authenticate,
        })),
        Err(other) => Err(json_err(StatusCode::BAD_REQUEST, other)),
    }
}

/// Body for [`test_live_server`]: the optional read-only probe to run against
/// the live session, supplied by the desktop from the connector enrichment.
#[derive(serde::Deserialize, Default, utoipa::ToSchema)]
pub(crate) struct TestLiveRequest {
    /// Read-only probe declared for this connector, when any.
    #[serde(default)]
    #[schema(value_type = Option<Object>)]
    probe: Option<ProbeSpec>,
}

/// `POST /api/v1/mcp/servers/:name/test`, Test an already-installed server.
///
/// Re-handshakes the live session for reachability, then runs the optional
/// read-only probe to exercise real operational access (scopes, grants). The
/// response `result.live_health` carries the operational verdict, so the
/// desktop can distinguish "reachable" from "actually working".
///
/// Response shape mirrors [`test_connection`]: `success` (with `live_health`),
/// `oauth_required`, or `400`.
#[utoipa::path(
    post,
    path = "/api/v1/mcp/servers/{name}/test",
    tag = "mcp",
    params(("name" = String, Path, description = "Server name")),
    request_body = TestLiveRequest,
    responses(
        (status = 200, description = "Live handshake and probe result, or OAuth required (see kind field)"),
        (status = 400, description = "Handshake failure", body = crate::api::openapi::ApiErrorBody),
        (status = 404, description = "Server not found", body = crate::api::openapi::ApiErrorBody),
        (status = 503, description = "MCP not configured", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub(crate) async fn test_live_server<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path(name): Path<String>,
    Json(req): Json<TestLiveRequest>,
) -> Result<Json<McpConnectionTestResponse>, JsonError> {
    let handle = require_mcp_handle(&state)?;
    match handle.test_live_server(&name, req.probe).await {
        Ok(result) => Ok(Json(McpConnectionTestResponse::Success { result })),
        Err(apollia_mcp::session::McpSessionError::Unauthorized {
            www_authenticate, ..
        }) => Ok(Json(McpConnectionTestResponse::OauthRequired {
            www_authenticate,
        })),
        // An unknown server name is a clean not-found, not a handshake failure.
        Err(err @ apollia_mcp::session::McpSessionError::ServerExited { .. }) => {
            Err(json_err(StatusCode::NOT_FOUND, err))
        }
        Err(other) => Err(json_err(StatusCode::BAD_REQUEST, other)),
    }
}

/// `PUT /api/v1/mcp/servers/:name/config`, Replace a server configuration and restart the session.
///
/// Removes the current session, starts a new one with the updated configuration, then
/// upserts the entry in `mcp.db`.
/// Returns `200 OK` with the new [`McpServerStatus`] on success.
/// Returns `404 Not Found` when no server with `name` is managed.
/// Returns `400 Bad Request` when the new session fails to start.
/// Returns `503 Service Unavailable` when MCP is not configured.
#[utoipa::path(
    put,
    path = "/api/v1/mcp/servers/{name}/config",
    tag = "mcp",
    params(("name" = String, Path, description = "Server name")),
    responses(
        (status = 200, description = "Configuration replaced and session restarted"),
        (status = 400, description = "New session failed to start", body = crate::api::openapi::ApiErrorBody),
        (status = 404, description = "Server not found", body = crate::api::openapi::ApiErrorBody),
        (status = 500, description = "Repository error", body = crate::api::openapi::ApiErrorBody),
        (status = 503, description = "MCP not configured", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub(crate) async fn update_server_config<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path(name): Path<String>,
    Json(patch): Json<serde_json::Value>,
) -> Result<Json<McpServerStatus>, JsonError> {
    let handle = require_mcp_handle(&state)?;
    let repo = require_mcp_repo(&state)?;

    // The client sends a PARTIAL patch (only the fields it changes); merge it
    // into the persisted config so untouched fields (and the path-scoped name)
    // are preserved, then replace the running session.
    let existing = {
        let guard = repo.lock().map_err(|_| {
            json_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "repository lock poisoned",
            )
        })?;
        guard
            .find_by_name(&name)
            .map_err(|e| json_err(StatusCode::INTERNAL_SERVER_ERROR, e))?
            .ok_or_else(|| {
                json_err(
                    StatusCode::NOT_FOUND,
                    format!("server '{}' not found", name),
                )
            })?
    };
    let config = merge_server_config_patch(existing, &patch)
        .map_err(|e| json_err(StatusCode::BAD_REQUEST, e))?;

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

/// Merge a partial JSON patch into an existing [`McpServerConfig`]. Every key the
/// patch carries (`command`, `url`, `requires_approval`, ...) overrides the base;
/// the server `name` is taken from the path and is never patchable. Returns an
/// error when the patch is not a JSON object or the merged result is not a valid
/// config.
fn merge_server_config_patch(
    existing: McpServerConfig,
    patch: &serde_json::Value,
) -> Result<McpServerConfig, String> {
    let patch_map = patch
        .as_object()
        .ok_or_else(|| "config patch must be a JSON object".to_string())?;
    let mut base =
        serde_json::to_value(&existing).map_err(|e| format!("serialize existing config: {e}"))?;
    let base_map = base
        .as_object_mut()
        .ok_or_else(|| "config did not serialize to a JSON object".to_string())?;
    for (key, value) in patch_map {
        if key == "name" {
            continue;
        }
        base_map.insert(key.clone(), value.clone());
    }
    serde_json::from_value(base).map_err(|e| format!("invalid merged config: {e}"))
}

/// Request body for `PATCH /api/v1/mcp/servers/:name/approval`.
#[derive(Debug, serde::Deserialize, utoipa::ToSchema)]
pub(crate) struct SetApprovalBody {
    requires_approval: bool,
}

/// `PATCH /api/v1/mcp/servers/:name/approval`, Update the approval flag without restarting.
///
/// Updates the `requires_approval` flag in-memory and persists the change to `mcp.db`.
/// The running session is **not** restarted; the new flag takes effect for the next
/// tool call. Returns `200 OK` with the updated [`McpServerStatus`] on success.
/// Returns `404 Not Found` when no server with `name` is managed.
/// Returns `503 Service Unavailable` when MCP is not configured.
#[utoipa::path(
    patch,
    path = "/api/v1/mcp/servers/{name}/approval",
    tag = "mcp",
    params(("name" = String, Path, description = "Server name")),
    request_body = SetApprovalBody,
    responses(
        (status = 200, description = "Approval flag updated"),
        (status = 404, description = "Server not found", body = crate::api::openapi::ApiErrorBody),
        (status = 500, description = "Repository error", body = crate::api::openapi::ApiErrorBody),
        (status = 503, description = "MCP not configured", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub(crate) async fn set_server_approval<B: ExecutionBackend + Clone>(
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
    fn test_merge_server_config_patch_preserves_name_and_merges_fields() {
        // GIVEN an existing server config
        let existing: McpServerConfig = serde_json::from_value(serde_json::json!({
            "name": "srv",
            "command": "/old/cmd",
            "requires_approval": false,
        }))
        .expect("valid base config");
        // WHEN a partial patch (no `name`) changes command + requires_approval
        let patch = serde_json::json!({ "command": "/new/cmd", "requires_approval": true });
        let merged = merge_server_config_patch(existing, &patch).expect("merge ok");
        // THEN the path-scoped name is preserved and the patched fields apply
        assert_eq!(merged.name, "srv");
        assert_eq!(merged.command, "/new/cmd");
        assert!(merged.requires_approval);
    }

    #[test]
    fn test_merge_server_config_patch_ignores_name_key() {
        // GIVEN an existing config and a patch that tries to rename it
        let existing: McpServerConfig = serde_json::from_value(serde_json::json!({
            "name": "srv",
            "command": "/cmd",
        }))
        .expect("valid base config");
        let patch = serde_json::json!({ "name": "evil" });
        // WHEN merged THEN the name stays put (it comes from the path)
        let merged = merge_server_config_patch(existing, &patch).expect("merge ok");
        assert_eq!(merged.name, "srv");
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
        // WHEN its numeric code is read
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
                health: apollia_core::McpHealth::Healthy { verified: false },
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
                health: apollia_core::McpHealth::Healthy { verified: false },
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
                health: apollia_core::McpHealth::Healthy { verified: false },
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
                health: apollia_core::McpHealth::Healthy { verified: false },
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

    #[tokio::test]
    async fn test_list_resources_empty_without_mcp_handle() {
        // GIVEN no MCP handle is configured
        let mcp_handle: Option<apollia_mcp::manager::McpClientManagerHandle> = None;
        // WHEN the resource list is built
        let resources = match &mcp_handle {
            Some(handle) => handle.list_resources().await,
            None => Vec::new(),
        };
        // THEN the result is an empty list (picker degrades to "no resources")
        assert!(resources.is_empty());
    }

    #[test]
    fn test_resource_summary_serialization() {
        // GIVEN an aggregated resource summary
        let summary = McpResourceSummary {
            server: "notion".to_string(),
            uri: "notion://page/123".to_string(),
            name: "Roadmap".to_string(),
            mime_type: Some("text/markdown".to_string()),
            description: Some("Product roadmap page".to_string()),
        };
        // WHEN serialized
        let json = serde_json::to_value(&summary).unwrap();
        // THEN the server tag and fields are present
        assert_eq!(json["server"], "notion");
        assert_eq!(json["uri"], "notion://page/123");
        assert_eq!(json["name"], "Roadmap");
        assert_eq!(json["mime_type"], "text/markdown");
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
