//! `PUT /api/v1/connections/{id}/tools/{tool}/family`: the operator gives a
//! tool of a connection another action family.
//!
//! Only a tool of an MCP connection the curated selection does not classify
//! is reclassified: a connection added from the selection, and a tool the
//! selection classifies, keep the selection's family, and the native tools
//! keep the one Apollia's code fixes. The family is stored in `mcp.db` apart
//! from the server row, so a connection added again keeps it; it wins over
//! the hints the server declares, and the live manager takes it in without a
//! restart. Each change is journaled as `tool_reclassified`, with the family
//! before and after.

use apollia_mcp::Provenance;
use apollia_tools::{ActionFamily, FamilySource};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;

use crate::api::openapi::ApiErrorBody;
use crate::api::server::AppState;
use crate::connections::{Connection, ConnectionKind, NativeAccounts};
use crate::coordinator::ExecutionBackend;
use crate::governance::ConnectionEvent;

use super::{api_err, collect_connections, native_accounts, ApiError};

/// Who the journal names for a reclassification made through the API.
const API_ACTOR: &str = "api";

/// Body of `PUT /api/v1/connections/{id}/tools/{tool}/family`.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ToolFamilyBody {
    /// The family the tool takes: `read`, `read_personal`,
    /// `write_or_delete`, `send_external`, `execute_code` or `unclassified`.
    #[schema(value_type = String)]
    pub family: ActionFamily,
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

/// Gives `tool` of the connection `id` the family `family`, and answers the
/// connection as the list then shows it. `accounts` are the native accounts
/// connected.
pub(crate) async fn reclassify<B: ExecutionBackend + Clone>(
    state: &AppState<B>,
    accounts: Vec<NativeAccounts>,
    id: &str,
    tool: &str,
    family: ActionFamily,
) -> Result<Connection, ApiError> {
    if family == ActionFamily::Exempt {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "the exempt family is reserved for Apollia's own tools",
        ));
    }
    let connection = find(collect_connections(state, accounts.clone()).await?, id)?;
    let server = match (connection.kind, id.strip_prefix("mcp:")) {
        (ConnectionKind::Mcp, Some(server)) => server.to_owned(),
        _ => {
            return Err(api_err(
                StatusCode::CONFLICT,
                format!("the tools of '{id}' keep the family Apollia gives them"),
            ))
        }
    };
    if connection.provenance == Provenance::Selection {
        return Err(api_err(
            StatusCode::CONFLICT,
            format!("'{id}' comes from the selection, which classifies its tools"),
        ));
    }
    let tool_id = format!("{id}/{tool}");
    let current = connection
        .tools
        .iter()
        .find(|t| t.id == tool_id)
        .ok_or_else(|| {
            api_err(
                StatusCode::NOT_FOUND,
                format!("tool '{tool}' not found on '{id}'"),
            )
        })?;
    if current.family_source == FamilySource::Selection {
        return Err(api_err(
            StatusCode::CONFLICT,
            format!("the selection classifies '{tool}' of '{id}'"),
        ));
    }
    let from = current.family;

    let repo = state.mcp_server_repo.as_ref().ok_or_else(|| {
        api_err(
            StatusCode::SERVICE_UNAVAILABLE,
            "the MCP repository is not available",
        )
    })?;
    repo.lock()
        .map_err(|_| {
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "repository lock poisoned",
            )
        })?
        .set_operator_family(&server, tool, family)
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, e))?;
    if let Some(handle) = &state.mcp_handle {
        if let Err(e) = handle.set_operator_family(&server, tool, family).await {
            tracing::event!(
                tracing::Level::WARN,
                connection = %id,
                error = %e,
                "connections.reclassify.live_failed"
            );
        }
    }
    state
        .connection_governance
        .record_or_warn(&ConnectionEvent::ToolReclassified {
            connection_id: id.to_owned(),
            tool: tool.to_owned(),
            from,
            to: family,
            actor: API_ACTOR.to_owned(),
        });
    tracing::event!(
        tracing::Level::INFO,
        connection = %id,
        from = %from.as_str(),
        to = %family.as_str(),
        "connections.tool.reclassified"
    );
    find(collect_connections(state, accounts).await?, id)
}

/// `PUT /api/v1/connections/{id}/tools/{tool}/family`: give a tool of a
/// connection the selection does not classify another action family.
#[utoipa::path(
    put,
    path = "/api/v1/connections/{id}/tools/{tool}/family",
    tag = "connections",
    params(
        ("id" = String, Path, description = "Connection identifier, as `GET /api/v1/connections` lists it"),
        ("tool" = String, Path, description = "Name of the tool on its server"),
    ),
    request_body = ToolFamilyBody,
    responses(
        (status = 200, description = "Reclassified and journaled; the connection as the list now shows it", body = Connection),
        (status = 400, description = "A family the operator cannot give; nothing written", body = ApiErrorBody),
        (status = 404, description = "Connection or tool not found; nothing written", body = ApiErrorBody),
        (status = 409, description = "The selection, or Apollia's code, classifies this tool; nothing written", body = ApiErrorBody),
        (status = 500, description = "A store could not be read or written", body = ApiErrorBody),
        (status = 503, description = "The MCP repository is not available; nothing written", body = ApiErrorBody),
    )
)]
pub(crate) async fn set_tool_family<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path((id, tool)): Path<(String, String)>,
    Json(body): Json<ToolFamilyBody>,
) -> Result<Json<Connection>, ApiError> {
    let accounts = native_accounts().await?;
    reclassify(&state, accounts, &id, &tool, body.family)
        .await
        .map(Json)
}

#[cfg(test)]
mod tests;
