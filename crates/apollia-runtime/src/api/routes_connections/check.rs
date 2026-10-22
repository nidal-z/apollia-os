//! `POST /api/v1/connections/{id}/check`: check one connection now.
//!
//! The check of each kind is [`crate::connections::check`]: a native account
//! obtains a valid token, a mailbox logs in, a folder is looked up, an MCP
//! server is tested through its live session, started first when it has none.
//! A check reaches the service of its connection and nothing else; a
//! connection the organisation forbids is not reached at all. What it found
//! is kept in `connection_states` and announced as `ConnectionStateChanged`,
//! and the route answers the connection as the list now reads it.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;

use apollia_auth::{AuthManager, ConnectorProvider};
use apollia_connectors::imap::{ConnectionProbe, ImapAccountStore, ImapClient};
use apollia_core::{ConnectionStateRecord, ConnectionStatus};
use apollia_mcp::manager::ProbeSpec;

use crate::api::openapi::ApiErrorBody;
use crate::api::server::AppState;
use crate::connections::check;
use crate::connections::{Connection, ConnectionKind, NativeAccounts};
use crate::coordinator::ExecutionBackend;

use super::{api_err, collect_connections, native_accounts, ApiError};

/// Body of `POST /api/v1/connections/{id}/check`, which may be left out.
#[derive(Debug, Default, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CheckConnectionBody {
    /// Read-only tool call an MCP server is probed with after its handshake,
    /// when its connector declares one; ignored for any other kind.
    #[serde(default)]
    #[schema(value_type = Option<Object>)]
    pub probe: Option<ProbeSpec>,
}

fn find(connections: Vec<Connection>, id: &str) -> Result<Connection, ApiError> {
    connections
        .into_iter()
        .find(|connection| connection.id == id)
        .ok_or_else(|| {
            api_err(
                StatusCode::NOT_FOUND,
                format!("connection '{id}' not found"),
            )
        })
}

fn internal(e: impl std::fmt::Display) -> ApiError {
    api_err(StatusCode::INTERNAL_SERVER_ERROR, e)
}

/// Check the MCP server `name`: start its session when it has none, then test
/// it.
async fn check_mcp<B: ExecutionBackend + Clone>(
    state: &AppState<B>,
    id: &str,
    name: &str,
    probe: Option<ProbeSpec>,
) -> Result<ConnectionStateRecord, ApiError> {
    let handle = state
        .mcp_handle
        .as_ref()
        .ok_or_else(|| api_err(StatusCode::SERVICE_UNAVAILABLE, "MCP not configured"))?;
    let live = handle.status().await.iter().any(|s| s.name == name);
    if !live {
        let config = state
            .mcp_server_repo
            .as_ref()
            .ok_or_else(|| api_err(StatusCode::SERVICE_UNAVAILABLE, "MCP not configured"))?
            .lock()
            .map_err(|_| internal("repository lock poisoned"))?
            .find_by_name(name)
            .map_err(internal)?
            .ok_or_else(|| {
                api_err(
                    StatusCode::NOT_FOUND,
                    format!("connection '{id}' not found"),
                )
            })?;
        if let Err(e) = handle.add_server(config).await {
            return Ok(check::mcp_start_failed(id, &e));
        }
    }
    Ok(check::mcp_tested(
        id,
        &handle.test_live_server(name, probe).await,
    ))
}

/// What reaches the service of a native account and of a mailbox.
pub(crate) struct Reach<'a, P> {
    /// The tokens of the native accounts.
    pub auth: Option<&'a AuthManager>,
    /// The mailbox client.
    pub imap: Option<&'a P>,
}

/// Check `connection` the way its kind is checked, with `reach` for a native
/// account or a mailbox.
async fn check_one<B: ExecutionBackend + Clone, P: ConnectionProbe>(
    state: &AppState<B>,
    connection: &Connection,
    reach: &Reach<'_, P>,
    probe: Option<ProbeSpec>,
) -> Result<ConnectionStateRecord, ApiError> {
    let (auth, imap) = (reach.auth, reach.imap);
    let id = connection.id.as_str();
    if state.connection_governance.forbidden().ids().contains(id) {
        return Ok(check::blocked(id));
    }
    let account = connection.account.as_deref().unwrap_or_default();
    match connection.kind {
        ConnectionKind::Native => {
            let provider = id
                .split(':')
                .nth(1)
                .and_then(ConnectorProvider::from_id)
                .ok_or_else(|| internal(format!("connection '{id}' names no provider")))?;
            let auth = auth.ok_or_else(|| internal("no authentication manager"))?;
            Ok(
                check::check_native(id, auth, provider, account, &apollia_auth::HttpTokenRefresh)
                    .await,
            )
        }
        ConnectionKind::Imap => {
            let store = ImapAccountStore::open(&state.data_dir).map_err(internal)?;
            let mailbox = store.get(account).map_err(internal)?;
            let imap = imap.ok_or_else(|| internal("no mailbox client"))?;
            Ok(check::check_imap(id, &mailbox, store.password(account), imap).await)
        }
        ConnectionKind::Folder => {
            let path = connection.path.as_deref().unwrap_or_default();
            Ok(check::check_folder(id, std::path::Path::new(path)))
        }
        ConnectionKind::Mcp => {
            let name = id.strip_prefix("mcp:").unwrap_or(id);
            check_mcp(state, id, name, probe).await
        }
    }
}

/// Check the connection `id`, keep and announce what was found, and answer
/// the connection as the list now reads it.
///
/// A connection in pause is not reached: an MCP server in pause is not
/// started for a check, nor any other service called. It is answered paused,
/// as the list reads it, and nothing is kept.
pub(crate) async fn check_connection_with<B: ExecutionBackend + Clone, P: ConnectionProbe>(
    state: &AppState<B>,
    accounts: Vec<NativeAccounts>,
    reach: &Reach<'_, P>,
    id: &str,
    body: CheckConnectionBody,
) -> Result<Connection, ApiError> {
    let connection = find(collect_connections(state, accounts.clone()).await?, id)?;
    if connection.status == ConnectionStatus::Paused {
        tracing::event!(
            tracing::Level::INFO,
            connection = %id,
            "connections.check.paused"
        );
        return Ok(connection);
    }
    let found = check_one(state, &connection, reach, body.probe).await?;
    let dir = state.data_dir.clone();
    let bus = state.event_sender.clone();
    let kept = found.clone();
    tokio::task::spawn_blocking(move || check::record(&dir, &bus, &kept))
        .await
        .map_err(internal)?
        .map_err(internal)?;
    tracing::event!(
        tracing::Level::INFO,
        connection = %id,
        state = ?found.state,
        "connections.check.done"
    );
    find(collect_connections(state, accounts).await?, id)
}

/// `POST /api/v1/connections/{id}/check`: check one connection now, and
/// answer it with the state found, dated.
#[utoipa::path(
    post,
    path = "/api/v1/connections/{id}/check",
    tag = "connections",
    params(("id" = String, Path, description = "Connection identifier, as `GET /api/v1/connections` lists it")),
    request_body(content = Option<CheckConnectionBody>, description = "Optional; the read-only probe of an MCP server"),
    responses(
        (status = 200, description = "Checked; the connection with the state found, kept and announced", body = Connection),
        (status = 404, description = "Connection not found", body = ApiErrorBody),
        (status = 500, description = "A store could not be read or written", body = ApiErrorBody),
        (status = 503, description = "MCP not configured, for an MCP server", body = ApiErrorBody),
    )
)]
pub(crate) async fn check_connection<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path(id): Path<String>,
    body: Option<Json<CheckConnectionBody>>,
) -> Result<Json<Connection>, ApiError> {
    let accounts = native_accounts().await?;
    let auth = if id.starts_with("native:") {
        Some(
            crate::connectors_bridge::get_auth()
                .await
                .map_err(internal)?,
        )
    } else {
        None
    };
    let imap = if id.starts_with("imap:") {
        Some(ImapClient::with_system_roots().map_err(internal)?)
    } else {
        None
    };
    let body = body.map(|Json(b)| b).unwrap_or_default();
    let reach = Reach {
        auth: auth.as_deref(),
        imap: imap.as_ref(),
    };
    check_connection_with(&state, accounts, &reach, &id, body)
        .await
        .map(Json)
}

#[cfg(test)]
mod tests;
