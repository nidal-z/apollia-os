//! `GET /api/v1/connections`: every connection of this machine in one list.
//!
//! The list is assembled by [`crate::connections::assemble`] from the stores
//! each kind keeps: `mcp.db` and the live MCP sessions, the tool registry, the
//! native account index, the mailbox list, the folders and the last state of
//! each connection in `system.db`, and `package_items` in `agents.db` when it
//! exists. A folder is added and removed under `/api/v1/connections/folders`
//! (see [`folders`]). The agents of a connection are read, and the connection
//! granted to several agents, under `/api/v1/connections/{id}` (see
//! [`agents`]). A connection of any kind is removed, with what it leaves
//! behind, by `DELETE /api/v1/connections/{id}` (see [`remove`]).

use axum::{
    extract::State,
    http::StatusCode,
    routing::{delete, get, post},
    Json, Router,
};

use apollia_auth::ConnectorProvider;

use crate::api::openapi::ApiErrorBody;
use crate::api::server::AppState;
use crate::connections::{
    assemble, package_connections, Connection, ConnectionSources, NativeAccounts,
};
use crate::coordinator::ExecutionBackend;

type ApiError = (StatusCode, Json<ApiErrorBody>);

fn api_err(code: StatusCode, msg: impl std::fmt::Display) -> ApiError {
    (
        code,
        Json(ApiErrorBody {
            error: msg.to_string(),
        }),
    )
}

/// Build the connections router.
pub fn connections_router<B: ExecutionBackend + Clone>() -> Router<AppState<B>> {
    Router::new()
        .route("/api/v1/connections", get(list_connections::<B>))
        .route(
            "/api/v1/connections/folders",
            post(folders::add_folder_connection::<B>),
        )
        .route(
            "/api/v1/connections/folders/:id",
            delete(folders::remove_folder_connection::<B>),
        )
        .route(
            "/api/v1/connections/:id",
            delete(remove::remove_connection::<B>),
        )
        .route(
            "/api/v1/connections/:id/agents",
            get(agents::get_connection_agents::<B>),
        )
        .route(
            "/api/v1/connections/:id/grants",
            post(agents::grant_connection::<B>),
        )
}

/// `GET /api/v1/connections`: every connection, whatever its kind, with its
/// provenance, its remote attribute and its tools.
#[utoipa::path(
    get,
    path = "/api/v1/connections",
    tag = "connections",
    responses(
        (status = 200, description = "Every connection", body = [Connection]),
        (status = 500, description = "A store could not be read", body = ApiErrorBody),
    )
)]
pub(crate) async fn list_connections<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
) -> Result<Json<Vec<Connection>>, ApiError> {
    let accounts = native_accounts().await?;
    collect_connections(&state, accounts).await.map(Json)
}

/// Connected accounts of every native provider.
pub(crate) async fn native_accounts() -> Result<Vec<NativeAccounts>, ApiError> {
    let manager = auth_manager().await?;
    accounts_of(&manager).await
}

/// The auth manager the native connectors run with.
pub(crate) async fn auth_manager() -> Result<std::sync::Arc<apollia_auth::AuthManager>, ApiError> {
    crate::connectors_bridge::get_auth()
        .await
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, e))
}

/// Connected accounts of every native provider in `manager`.
pub(crate) async fn accounts_of(
    manager: &apollia_auth::AuthManager,
) -> Result<Vec<NativeAccounts>, ApiError> {
    let mut out = Vec::new();
    for provider in ConnectorProvider::ALL {
        let accounts = manager
            .list_accounts(provider)
            .await
            .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, e))?;
        out.push((provider, accounts.into_iter().map(|a| a.0).collect()));
    }
    Ok(out)
}

/// Read every other store from `state` and assemble the list.
pub(crate) async fn collect_connections<B: ExecutionBackend + Clone>(
    state: &AppState<B>,
    native_accounts: Vec<NativeAccounts>,
) -> Result<Vec<Connection>, ApiError> {
    let mcp_configs = match &state.mcp_server_repo {
        Some(repo) => repo
            .lock()
            .map_err(|_| {
                api_err(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "repository lock poisoned",
                )
            })?
            .list()
            .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, e))?,
        None => Vec::new(),
    };
    let (mcp_statuses, mcp_index) = match &state.mcp_handle {
        Some(handle) => (handle.status().await, handle.get_tool_index().await),
        None => (Vec::new(), Vec::new()),
    };
    let descriptors = match &state.tool_registry_handle {
        Some(registry) => registry
            .list()
            .await
            .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, e))?,
        None => Vec::new(),
    };
    let package_servers = package_connections(
        &state
            .data_dir
            .join(apollia_core::paths::DataFile::Agents.file_name()),
    );
    let imap_accounts = apollia_connectors::imap::ImapAccountStore::accounts_in(&state.data_dir)
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, e))?;
    let folders = folders::store(&state.data_dir)
        .and_then(|store| store.list())
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, e))?;
    Ok(assemble(&ConnectionSources {
        mcp_configs,
        mcp_statuses,
        descriptors,
        mcp_index,
        package_servers,
        native_accounts,
        imap_accounts,
        forbidden: state.connection_governance.forbidden().ids().clone(),
        folders,
        states: crate::connections::state::kept_states(&state.data_dir),
    }))
}

pub(crate) mod agents;
pub(crate) mod folders;
pub(crate) mod remove;

#[cfg(test)]
mod tests;
