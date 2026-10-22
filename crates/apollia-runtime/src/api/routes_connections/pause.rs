//! Put a connection in pause, and take it out of it.
//!
//! - `POST /api/v1/connections/{id}/pause`: none of its tools runs any more;
//!   a call of one is refused with the reason that it is paused. An MCP
//!   server is stopped and its `enabled` flag switched off, so it is not
//!   started again, at the next start or on request.
//! - `POST /api/v1/connections/{id}/resume`: its tools run again; an MCP
//!   server is switched on and started.
//!
//! Neither touches what the connection was given: the accesses of the agents
//! that hold its tools, its configuration and its secrets are kept as they
//! are, so a resumed connection works as it did before its pause. The pause is
//! held by governance in `governance.db` and read by the next decision,
//! without a restart.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};

use apollia_core::{ConnectionStatus, RuntimeEvent};

use crate::api::openapi::ApiErrorBody;
use crate::api::server::AppState;
use crate::connections::Connection;
use crate::coordinator::ExecutionBackend;
use crate::governance::GovernanceWriteError;

use super::{api_err, collect_connections, native_accounts, ApiError};

/// The connection `id` of `connections`, or a 404.
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

fn write_error(error: GovernanceWriteError) -> ApiError {
    match error {
        GovernanceWriteError::NoDatabase | GovernanceWriteError::ActorGone => {
            api_err(StatusCode::SERVICE_UNAVAILABLE, error)
        }
        other => api_err(StatusCode::INTERNAL_SERVER_ERROR, other),
    }
}

/// Switches the `enabled` flag of the MCP server `name` in `mcp.db`.
fn set_server_enabled<B: ExecutionBackend + Clone>(
    state: &AppState<B>,
    name: &str,
    enabled: bool,
) -> Result<(), ApiError> {
    let Some(repo) = &state.mcp_server_repo else {
        return Ok(());
    };
    repo.lock()
        .map_err(|_| {
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "repository lock poisoned",
            )
        })?
        .set_enabled(name, enabled)
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, e))
}

/// Starts the MCP server `name` again from its saved configuration, unless
/// the policy forbade it. A start that fails is not a failed resume: the
/// state of the connection says what its start found.
async fn start_server<B: ExecutionBackend + Clone>(state: &AppState<B>, name: &str) {
    let (Some(handle), Some(repo)) = (&state.mcp_handle, &state.mcp_server_repo) else {
        return;
    };
    if state
        .connection_governance
        .forbidden()
        .contains_mcp_server(name)
    {
        return;
    }
    let config = match repo.lock() {
        Ok(guard) => guard.find_by_name(name).ok().flatten(),
        Err(_) => None,
    };
    let Some(config) = config else {
        return;
    };
    if let Err(e) = handle.add_server(config).await {
        tracing::event!(
            tracing::Level::WARN,
            server = %name,
            error = %e,
            "connections.resume.start_failed"
        );
    }
}

/// Puts `id` in pause (`paused`) or takes it out of it, and answers the
/// connection as the list then shows it.
/// `accounts` are the native accounts connected.
pub(crate) async fn set_paused<B: ExecutionBackend + Clone>(
    state: &AppState<B>,
    accounts: Vec<crate::connections::NativeAccounts>,
    id: &str,
    paused: bool,
) -> Result<Connection, ApiError> {
    let governance = state.governance.as_ref().ok_or_else(|| {
        api_err(
            StatusCode::SERVICE_UNAVAILABLE,
            "governance is not available",
        )
    })?;
    find(collect_connections(state, accounts.clone()).await?, id)?;
    governance
        .set_paused(id, paused)
        .await
        .map_err(write_error)?;
    if let Some(name) = id.strip_prefix("mcp:") {
        set_server_enabled(state, name, !paused)?;
        if paused {
            if let Some(handle) = &state.mcp_handle {
                // A server with no live session has nothing to stop. What it
                // published stays kept, for the list and its next start.
                let _ = handle.stop_server(name).await;
            }
        } else {
            start_server(state, name).await;
        }
    }
    tracing::event!(
        tracing::Level::INFO,
        connection = %id,
        paused,
        "connections.pause.changed"
    );
    let connection = find(collect_connections(state, accounts).await?, id)?;
    if paused && connection.status == ConnectionStatus::Paused {
        let _ = state
            .event_sender
            .send(RuntimeEvent::ConnectionStateChanged {
                id: connection.id.clone(),
                state: ConnectionStatus::Paused,
                reason: None,
                checked_at: chrono::Utc::now().to_rfc3339(),
            });
    }
    Ok(connection)
}

/// `POST /api/v1/connections/{id}/pause`: none of the tools of the
/// connection runs until it is resumed; the accesses of the agents are kept.
#[utoipa::path(
    post,
    path = "/api/v1/connections/{id}/pause",
    tag = "connections",
    params(("id" = String, Path, description = "Connection identifier, as `GET /api/v1/connections` lists it")),
    responses(
        (status = 200, description = "In pause; the connection as the list now shows it", body = Connection),
        (status = 404, description = "Connection not found; nothing written", body = ApiErrorBody),
        (status = 500, description = "A store could not be read or written", body = ApiErrorBody),
        (status = 503, description = "Governance not available; nothing written", body = ApiErrorBody),
    )
)]
pub(crate) async fn pause_connection<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path(id): Path<String>,
) -> Result<Json<Connection>, ApiError> {
    let accounts = native_accounts().await?;
    set_paused(&state, accounts, &id, true).await.map(Json)
}

/// `POST /api/v1/connections/{id}/resume`: the tools of the connection run
/// again, with the accesses the agents held before its pause.
#[utoipa::path(
    post,
    path = "/api/v1/connections/{id}/resume",
    tag = "connections",
    params(("id" = String, Path, description = "Connection identifier, as `GET /api/v1/connections` lists it")),
    responses(
        (status = 200, description = "Resumed; the connection as the list now shows it", body = Connection),
        (status = 404, description = "Connection not found; nothing written", body = ApiErrorBody),
        (status = 500, description = "A store could not be read or written", body = ApiErrorBody),
        (status = 503, description = "Governance not available; nothing written", body = ApiErrorBody),
    )
)]
pub(crate) async fn resume_connection<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path(id): Path<String>,
) -> Result<Json<Connection>, ApiError> {
    let accounts = native_accounts().await?;
    set_paused(&state, accounts, &id, false).await.map(Json)
}

#[cfg(test)]
mod tests;
