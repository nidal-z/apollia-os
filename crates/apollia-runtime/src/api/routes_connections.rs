//! `GET /api/v1/connections`: every connection of this machine in one list.
//!
//! The list is assembled by [`crate::connections::assemble`] from the stores
//! each kind keeps: `mcp.db` and the live MCP sessions, the tool registry, the
//! native account index, the mailbox list, the folders of `system.db`, and
//! `package_items` in `agents.db` when it exists. A folder is added and
//! removed under `/api/v1/connections/folders` (see [`folders`]).

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
    assemble, package_connection_names, Connection, ConnectionSources, NativeAccounts,
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
    let manager = crate::connectors_bridge::get_auth()
        .await
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, e))?;
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
    let mcp_statuses = match &state.mcp_handle {
        Some(handle) => handle.status().await,
        None => Vec::new(),
    };
    let descriptors = match &state.tool_registry_handle {
        Some(registry) => registry
            .list()
            .await
            .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, e))?,
        None => Vec::new(),
    };
    let package_servers = package_connection_names(
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
        package_servers,
        native_accounts,
        imap_accounts,
        forbidden: state.connection_governance.forbidden().ids().clone(),
        folders,
    }))
}

pub(crate) mod folders;

#[cfg(test)]
mod tests;
