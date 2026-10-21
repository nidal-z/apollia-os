//! The agents of a connection, and the grant of a connection to several agents.
//!
//! - `GET  /api/v1/connections/{id}/agents`: every agent that holds at least
//!   one tool of the connection, and which ones.
//! - `POST /api/v1/connections/{id}/grants`: grant tools of the connection to
//!   several agents at once.
//!
//! An agent's access is read as the runtime grants it: the default access
//! written in for an agent nobody chose one for, and the bare operations a
//! package declares (`gmail.send`) resolved against the connected accounts.
//! The matching itself is [`crate::connections::agents`].
//!
//! A grant writes every agent's access list in one transaction, then records
//! for each agent its override and one `AccessChanged` per tool granted.
//! Every agent and every tool is checked first, so a refusal writes nothing.
//! An agent holding the default access is given a fixed list: its default
//! access, plus the tools granted.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;

use crate::api::openapi::ApiErrorBody;
use crate::api::routes_agent_definitions as definitions;
use crate::api::server::AppState;
use crate::connections::agents::{
    foreign_tools, holders, with_granted, AgentAccessView, ConnectionAgents,
};
use crate::connections::{Connection, NativeAccounts};
use crate::connectors_bridge::connection_tools::resolve_bare_connector_access;
use crate::coordinator::ExecutionBackend;

use super::{api_err, collect_connections, native_accounts, ApiError};

/// Body of `POST /api/v1/connections/{id}/grants`.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct GrantConnectionBody {
    /// Identifiers of the agents to grant the tools to.
    pub agents: Vec<String>,
    /// Tools of the connection to grant, as `GET /api/v1/connections` lists
    /// them.
    pub tools: Vec<String>,
}

/// An error of the agent store, in the body this module answers.
fn agents_error(error: crate::api::routes_llm::servers::ApiError) -> ApiError {
    let (status, Json(body)) = error;
    api_err(status, body.error)
}

fn find<'a>(connections: &'a [Connection], id: &str) -> Result<&'a Connection, ApiError> {
    connections
        .iter()
        .find(|connection| connection.id == id)
        .ok_or_else(|| {
            api_err(
                StatusCode::NOT_FOUND,
                format!("connection '{id}' not found"),
            )
        })
}

/// Every stored agent with the access the runtime grants it, and the access
/// list its card shows, before the bare operations are resolved.
async fn agent_views<B: ExecutionBackend + Clone>(
    state: &AppState<B>,
    accounts: &[NativeAccounts],
) -> Result<Vec<(AgentAccessView, Vec<String>)>, ApiError> {
    let repo = definitions::open_agents(state).map_err(agents_error)?;
    let stored = repo
        .list_stored()
        .map_err(|e| agents_error(definitions::store_error(e)))?;
    let agents = stored
        .iter()
        .map(|agent| definitions::resolved(state, &repo, &agent.name))
        .collect::<Result<Vec<_>, _>>()
        .map_err(agents_error)?;
    drop(repo);
    let mut views = Vec::with_capacity(agents.len());
    for agent in agents {
        let by_default = apollia_tools::reads_by_default(&agent);
        let agent = definitions::in_force(state, agent).await;
        let listed = agent.access.clone();
        let access = resolve_bare_connector_access(&agent.id, agent.access, accounts);
        views.push((
            AgentAccessView {
                id: agent.id,
                name: agent.name,
                enabled: agent.enabled,
                by_default,
                access,
            },
            listed,
        ));
    }
    Ok(views)
}

/// The agents of the connection `id`, with `accounts` as the native accounts
/// connected.
pub(crate) async fn agents_of<B: ExecutionBackend + Clone>(
    state: &AppState<B>,
    accounts: Vec<NativeAccounts>,
    id: &str,
) -> Result<ConnectionAgents, ApiError> {
    let connections = collect_connections(state, accounts.clone()).await?;
    let connection = find(&connections, id)?;
    let views: Vec<AgentAccessView> = agent_views(state, &accounts)
        .await?
        .into_iter()
        .map(|(view, _)| view)
        .collect();
    Ok(holders(connection, &connections, &views))
}

/// Grant `body.tools` of the connection `id` to every agent of `body.agents`,
/// with `accounts` as the native accounts connected; answers the agents of
/// the connection after the grant.
pub(crate) async fn grant<B: ExecutionBackend + Clone>(
    state: &AppState<B>,
    accounts: Vec<NativeAccounts>,
    id: &str,
    body: &GrantConnectionBody,
) -> Result<ConnectionAgents, ApiError> {
    if body.agents.is_empty() || body.tools.is_empty() {
        return Err(api_err(
            StatusCode::UNPROCESSABLE_ENTITY,
            "a grant names at least one agent and one tool",
        ));
    }
    let connections = collect_connections(state, accounts.clone()).await?;
    let connection = find(&connections, id)?;
    let foreign = foreign_tools(connection, &body.tools);
    if !foreign.is_empty() {
        return Err(api_err(
            StatusCode::UNPROCESSABLE_ENTITY,
            format!("not tools of connection '{id}': {}", foreign.join(", ")),
        ));
    }
    let governance = definitions::governance(state).map_err(agents_error)?;
    let views = agent_views(state, &accounts).await?;
    let mut writes: Vec<(String, Vec<String>)> = Vec::new();
    for name in &body.agents {
        if writes.iter().any(|(written, _)| written == name) {
            continue;
        }
        let (_, listed) = views
            .iter()
            .find(|(view, _)| &view.id == name)
            .ok_or_else(|| api_err(StatusCode::NOT_FOUND, format!("agent '{name}' not found")))?;
        if let Some(access) = with_granted(listed, &body.tools) {
            writes.push((name.clone(), access));
        }
    }
    let repo = definitions::open_agents(state).map_err(agents_error)?;
    definitions::writer(&repo, &governance)
        .set_access_of_many(&writes)
        .map_err(|e| agents_error(definitions::override_error(e)))?;
    drop(repo);
    tracing::event!(
        tracing::Level::INFO,
        connection = %id,
        agents = writes.len(),
        tools = body.tools.len(),
        "connections.grant.written"
    );
    agents_of(state, accounts, id).await
}

/// `GET /api/v1/connections/{id}/agents`: the agents that hold at least one
/// tool of the connection.
#[utoipa::path(
    get,
    path = "/api/v1/connections/{id}/agents",
    tag = "connections",
    params(("id" = String, Path, description = "Connection identifier, as `GET /api/v1/connections` lists it")),
    responses(
        (status = 200, description = "The agents holding tools of the connection", body = ConnectionAgents),
        (status = 404, description = "Connection not found", body = ApiErrorBody),
        (status = 500, description = "A store could not be read", body = ApiErrorBody),
    )
)]
pub(crate) async fn get_connection_agents<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path(id): Path<String>,
) -> Result<Json<ConnectionAgents>, ApiError> {
    let accounts = native_accounts().await?;
    agents_of(&state, accounts, &id).await.map(Json)
}

/// `POST /api/v1/connections/{id}/grants`: grant tools of the connection to
/// several agents at once. Every agent's list is written, or none is.
#[utoipa::path(
    post,
    path = "/api/v1/connections/{id}/grants",
    tag = "connections",
    params(("id" = String, Path, description = "Connection identifier, as `GET /api/v1/connections` lists it")),
    request_body = GrantConnectionBody,
    responses(
        (status = 200, description = "Granted; the agents holding tools of the connection", body = ConnectionAgents),
        (status = 404, description = "Connection or agent not found; nothing written", body = ApiErrorBody),
        (status = 422, description = "No agent or no tool named, or a tool the connection does not bring; nothing written", body = ApiErrorBody),
        (status = 503, description = "Audit journal not available; nothing written", body = ApiErrorBody),
    )
)]
pub(crate) async fn grant_connection<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path(id): Path<String>,
    Json(body): Json<GrantConnectionBody>,
) -> Result<Json<ConnectionAgents>, ApiError> {
    let accounts = native_accounts().await?;
    grant(&state, accounts, &id, &body).await.map(Json)
}

#[cfg(test)]
mod tests;
