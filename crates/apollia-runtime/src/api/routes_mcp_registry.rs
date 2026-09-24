//! REST routes over the public MCP registry, under `/api/v1/mcp/registry`.
//!
//! The desktop wizard has always browsed `registry.modelcontextprotocol.io`
//! through [`McpRegistryClient`]; a caller without a desktop had no catalogue
//! but one written by hand. These routes read the same client, so they share
//! its cache (`<data_dir>/mcp-registry.json`, fresh for fifteen minutes), its
//! `APOLLIA_MCP_REGISTRY_URL` override, and its fallback: when the registry
//! does not answer, the cached catalogue is served, and only when there is no
//! cache either does the route fail.

use axum::{
    extract::{Query, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use serde::Deserialize;

use apollia_mcp::registry::{McpRegistryClient, RegistryClientError, RegistryServer};

use crate::api::openapi::ApiErrorBody;
use crate::api::server::AppState;
use crate::coordinator::ExecutionBackend;

/// Build the MCP registry router.
///
/// Mounted under the root router by [`crate::api::server::build_router`].
pub fn mcp_registry_router<B: ExecutionBackend + Clone>() -> Router<AppState<B>> {
    Router::new()
        .route("/api/v1/mcp/registry", get(list_registry_servers::<B>))
        .route("/api/v1/mcp/registry/server", get(get_registry_server::<B>))
}

type ApiError = (StatusCode, Json<ApiErrorBody>);

fn api_err(code: StatusCode, msg: impl std::fmt::Display) -> ApiError {
    (
        code,
        Json(ApiErrorBody {
            error: msg.to_string(),
        }),
    )
}

fn registry_err(err: RegistryClientError) -> ApiError {
    let code = match err {
        RegistryClientError::Offline => StatusCode::SERVICE_UNAVAILABLE,
        _ => StatusCode::BAD_GATEWAY,
    };
    api_err(code, err)
}

fn client<B: ExecutionBackend + Clone>(state: &AppState<B>) -> Result<McpRegistryClient, ApiError> {
    McpRegistryClient::new(&state.data_dir)
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, e))
}

/// Query of `GET /api/v1/mcp/registry`.
#[derive(Debug, Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct RegistryListQuery {
    /// Keyword forwarded to the registry. A search always asks the network;
    /// without one, a fresh cache answers.
    pub search: Option<String>,
}

/// Query of `GET /api/v1/mcp/registry/server`.
#[derive(Debug, Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct RegistryServerQuery {
    /// Registry name of the server, e.g. `io.github.owner/server`. A query
    /// parameter rather than a path segment, because registry names carry a
    /// slash.
    pub name: String,
}

/// `GET /api/v1/mcp/registry`: the servers the MCP registry lists, the latest
/// version of each, in the registry's own shape.
#[utoipa::path(
    get,
    path = "/api/v1/mcp/registry",
    tag = "mcp",
    params(RegistryListQuery),
    responses(
        (status = 200, description = "Registry entries, latest version per server; from the cache when the registry is unreachable"),
        (status = 502, description = "The registry answered something unreadable and no cache covers it", body = ApiErrorBody),
        (status = 503, description = "The registry is unreachable and no cache exists", body = ApiErrorBody),
    )
)]
pub(crate) async fn list_registry_servers<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Query(query): Query<RegistryListQuery>,
) -> Result<Json<Vec<RegistryServer>>, ApiError> {
    let search = query
        .search
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let servers = client(&state)?
        .fetch_servers(search)
        .await
        .map_err(registry_err)?;
    Ok(Json(servers))
}

/// `GET /api/v1/mcp/registry/server?name=`: one registry entry, its latest
/// version, read from the same catalogue as the listing.
#[utoipa::path(
    get,
    path = "/api/v1/mcp/registry/server",
    tag = "mcp",
    params(RegistryServerQuery),
    responses(
        (status = 200, description = "The registry entry"),
        (status = 404, description = "No server of that name in the registry", body = ApiErrorBody),
        (status = 502, description = "The registry answered something unreadable and no cache covers it", body = ApiErrorBody),
        (status = 503, description = "The registry is unreachable and no cache exists", body = ApiErrorBody),
    )
)]
pub(crate) async fn get_registry_server<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Query(query): Query<RegistryServerQuery>,
) -> Result<Json<RegistryServer>, ApiError> {
    let servers = client(&state)?
        .fetch_servers(None)
        .await
        .map_err(registry_err)?;
    servers
        .into_iter()
        .find(|entry| entry.server.name == query.name)
        .map(Json)
        .ok_or_else(|| {
            api_err(
                StatusCode::NOT_FOUND,
                format!("no server named {} in the registry", query.name),
            )
        })
}
