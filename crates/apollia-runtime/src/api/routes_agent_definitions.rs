//! The stored agents: their definition, the user's overrides, and the agent
//! that results.
//!
//! - `GET    /api/v1/agent-definitions`                         , list the agents, resolved
//! - `GET    /api/v1/agent-definitions/{name}`                  , read one agent, resolved
//! - `PUT    /api/v1/agent-definitions/{name}/overrides/{field}`, set an override
//! - `DELETE /api/v1/agent-definitions/{name}/overrides/{field}`, remove an override
//! - `DELETE /api/v1/agent-definitions/{name}/overrides`        , back to defaults
//!
//! A resolved agent is its package definition, then the user's overrides
//! field by field, with the model assigned to it in `system.db`. Every write
//! is recorded in the audit journal through the governance contract; without
//! a journal, the write is refused rather than left unrecorded.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{get, put};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use apollia_tools::{
    AgentOrigin, AgentRepository, AgentRepositoryError, OverrideField, ResolvedAgent,
};

use crate::agents::overrides::{AgentOverrideError, OverrideWriter};
use crate::api::routes_llm::backends::map_backend_error;
use crate::api::routes_llm::presets::AgentModelResponse;
use crate::api::routes_llm::servers::{api_err, ApiError};
use crate::api::server::AppState;
use crate::coordinator::ExecutionBackend;
use crate::governance::GovernanceHandle;

/// Who the journal names for a change made through the API.
const API_ACTOR: &str = "api";

/// Where an agent comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum AgentOriginDto {
    /// Installed from a package or an agent file.
    Package,
    /// Created in the interface, without code.
    Interface,
}

impl From<AgentOrigin> for AgentOriginDto {
    fn from(origin: AgentOrigin) -> Self {
        match origin {
            AgentOrigin::Package => AgentOriginDto::Package,
            AgentOrigin::Interface => AgentOriginDto::Interface,
        }
    }
}

/// An agent in force: its definition, then the user's overrides.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ResolvedAgentResponse {
    /// Identifier.
    pub id: String,
    /// Where the agent comes from.
    pub origin: AgentOriginDto,
    /// Package the agent came with.
    pub package: Option<String>,
    /// Version of the definition.
    pub version: String,
    /// Whether the agent is active.
    pub enabled: bool,
    /// Display name.
    pub name: String,
    /// What the agent is for, used as its instructions.
    pub mission: String,
    /// Example requests.
    pub examples: Vec<String>,
    /// Tools granted.
    pub access: Vec<String>,
    /// Sources checked.
    pub sources: Vec<String>,
    /// The model assigned to the agent, if any.
    pub model: Option<AgentModelResponse>,
    /// Autonomy tier the agent runs at: `assisted`, `supervised`,
    /// `bounded_autonomous` or `long_autonomous`. The user's, else the
    /// package's, else the global default.
    pub autonomy: String,
    /// Identifier of the user profile designated for the agent; absent when
    /// it runs with the default profile.
    pub profile: Option<String>,
    /// Fields whose value is the user's rather than the package's:
    /// `name`, `mission`, `examples`, `access`, `sources`, `autonomy`,
    /// `profile`.
    pub overridden: Vec<String>,
    /// The definition as the package declares it, before any override.
    #[schema(value_type = Object)]
    pub definition: serde_json::Value,
}

impl From<ResolvedAgent> for ResolvedAgentResponse {
    fn from(agent: ResolvedAgent) -> Self {
        Self {
            id: agent.id,
            origin: agent.origin.into(),
            package: agent.package,
            version: agent.version,
            enabled: agent.enabled,
            name: agent.name,
            mission: agent.mission,
            examples: agent.examples,
            access: agent.access,
            sources: agent.sources,
            model: agent.model.map(Into::into),
            autonomy: agent.autonomy.as_str().to_owned(),
            profile: agent.profile,
            overridden: agent
                .overridden
                .into_iter()
                .map(|field| field.as_str().to_owned())
                .collect(),
            definition: agent.definition,
        }
    }
}

/// Body of `PUT /api/v1/agent-definitions/{name}/overrides/{field}`.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct SetOverrideRequest {
    /// The user's value: a string for `name` and `mission`, a tier name for
    /// `autonomy`, a profile identifier for `profile`, a list of strings for `examples`, `access` and `sources`.
    #[schema(value_type = Object)]
    pub value: serde_json::Value,
}

fn open_agents<B: ExecutionBackend + Clone>(
    state: &AppState<B>,
) -> Result<AgentRepository, ApiError> {
    let path = apollia_core::paths::DataFile::Agents.path(&state.data_dir);
    AgentRepository::open(&path).map_err(|e| {
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("cannot open agents.db: {e}"),
        )
    })
}

fn store_error(err: AgentRepositoryError) -> ApiError {
    match err {
        AgentRepositoryError::NotFound(name) => {
            api_err(StatusCode::NOT_FOUND, format!("agent '{name}' not found"))
        }
        AgentRepositoryError::InvalidOverride(e) => api_err(StatusCode::UNPROCESSABLE_ENTITY, e),
        other => api_err(StatusCode::INTERNAL_SERVER_ERROR, other),
    }
}

fn override_error(err: AgentOverrideError) -> ApiError {
    match err {
        AgentOverrideError::Store(e) => store_error(e),
        other => api_err(StatusCode::INTERNAL_SERVER_ERROR, other),
    }
}

fn parse_field(raw: &str) -> Result<OverrideField, ApiError> {
    OverrideField::parse(raw).map_err(|e| api_err(StatusCode::UNPROCESSABLE_ENTITY, e))
}

fn governance<B: ExecutionBackend + Clone>(
    state: &AppState<B>,
) -> Result<GovernanceHandle, ApiError> {
    state
        .governance
        .clone()
        .filter(GovernanceHandle::journaled)
        .ok_or_else(|| {
            api_err(
                StatusCode::SERVICE_UNAVAILABLE,
                "audit journal not available: an override is not written without its record",
            )
        })
}

/// The override writer of an API call.
fn writer<'a>(repo: &'a AgentRepository, governance: &'a GovernanceHandle) -> OverrideWriter<'a> {
    OverrideWriter {
        repo,
        governance,
        actor: API_ACTOR,
    }
}

/// The agent `name` resolved, with the model `system.db` assigns it.
fn resolved<B: ExecutionBackend + Clone>(
    state: &AppState<B>,
    repo: &AgentRepository,
    name: &str,
) -> Result<ResolvedAgent, ApiError> {
    let model = match state.llm_backend_repo.as_ref() {
        Some(llm) => {
            let guard = llm.lock().map_err(|_| {
                api_err(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "repository lock poisoned",
                )
            })?;
            guard.agent_model(name).map_err(map_backend_error)?
        }
        None => None,
    };
    repo.resolved(name, model, crate::agents::global_default_autonomy())
        .map_err(store_error)?
        .ok_or_else(|| api_err(StatusCode::NOT_FOUND, format!("agent '{name}' not found")))
}

/// `agent` as its card shows it: an agent created without code that reads by
/// default lists the reading family active on the machine.
async fn shown<B: ExecutionBackend + Clone>(
    state: &AppState<B>,
    agent: ResolvedAgent,
) -> ResolvedAgentResponse {
    match state.tool_registry_handle.as_ref() {
        Some(registry) => apollia_tools::with_default_access(agent, registry)
            .await
            .into(),
        None => agent.into(),
    }
}

/// `GET /api/v1/agent-definitions`: every stored agent, resolved.
#[utoipa::path(
    get,
    path = "/api/v1/agent-definitions",
    tag = "agents",
    responses(
        (status = 200, description = "Stored agents, by identifier", body = Vec<ResolvedAgentResponse>),
        (status = 500, description = "Store error", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub async fn list_agent_definitions<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
) -> Result<Json<Vec<ResolvedAgentResponse>>, ApiError> {
    let repo = open_agents(&state)?;
    let stored = repo.list_stored().map_err(store_error)?;
    let agents = stored
        .iter()
        .map(|agent| resolved(&state, &repo, &agent.name))
        .collect::<Result<Vec<_>, _>>()?;
    drop(repo);
    let mut shown_agents = Vec::with_capacity(agents.len());
    for agent in agents {
        shown_agents.push(shown(&state, agent).await);
    }
    Ok(Json(shown_agents))
}

/// `GET /api/v1/agent-definitions/{name}`: one agent, resolved.
#[utoipa::path(
    get,
    path = "/api/v1/agent-definitions/{name}",
    tag = "agents",
    params(("name" = String, Path, description = "Agent identifier")),
    responses(
        (status = 200, description = "The agent in force", body = ResolvedAgentResponse),
        (status = 404, description = "Agent not found", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub async fn get_agent_definition<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path(name): Path<String>,
) -> Result<Json<ResolvedAgentResponse>, ApiError> {
    let repo = open_agents(&state)?;
    let agent = resolved(&state, &repo, &name)?;
    drop(repo);
    Ok(Json(shown(&state, agent).await))
}

/// `PUT /api/v1/agent-definitions/{name}/overrides/{field}`: set the user's
/// value of a field. Kept through every update of the package.
#[utoipa::path(
    put,
    path = "/api/v1/agent-definitions/{name}/overrides/{field}",
    tag = "agents",
    params(
        ("name" = String, Path, description = "Agent identifier"),
        ("field" = String, Path, description = "name, mission, examples, access, sources, autonomy or profile"),
    ),
    request_body = SetOverrideRequest,
    responses(
        (status = 200, description = "Override set; the agent in force", body = ResolvedAgentResponse),
        (status = 404, description = "Agent not found", body = crate::api::openapi::ApiErrorBody),
        (status = 422, description = "Unknown field, or a value of the wrong shape", body = crate::api::openapi::ApiErrorBody),
        (status = 503, description = "Audit journal not available", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub async fn set_agent_override<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path((name, field)): Path<(String, String)>,
    Json(req): Json<SetOverrideRequest>,
) -> Result<Json<ResolvedAgentResponse>, ApiError> {
    let field = parse_field(&field)?;
    let governance = governance(&state)?;
    let repo = open_agents(&state)?;
    writer(&repo, &governance)
        .set(&name, field, &req.value)
        .map_err(override_error)?;
    tracing::event!(
        tracing::Level::INFO,
        agent = %name,
        field = %field.as_str(),
        "agents.override.set"
    );
    let agent = resolved(&state, &repo, &name)?;
    drop(repo);
    Ok(Json(shown(&state, agent).await))
}

/// `DELETE /api/v1/agent-definitions/{name}/overrides/{field}`: remove the
/// user's value of a field; the package default applies again.
#[utoipa::path(
    delete,
    path = "/api/v1/agent-definitions/{name}/overrides/{field}",
    tag = "agents",
    params(
        ("name" = String, Path, description = "Agent identifier"),
        ("field" = String, Path, description = "name, mission, examples, access, sources, autonomy or profile"),
    ),
    responses(
        (status = 200, description = "Override removed, or there was none; the agent in force", body = ResolvedAgentResponse),
        (status = 404, description = "Agent not found", body = crate::api::openapi::ApiErrorBody),
        (status = 422, description = "Unknown field", body = crate::api::openapi::ApiErrorBody),
        (status = 503, description = "Audit journal not available", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub async fn remove_agent_override<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path((name, field)): Path<(String, String)>,
) -> Result<Json<ResolvedAgentResponse>, ApiError> {
    let field = parse_field(&field)?;
    let governance = governance(&state)?;
    let repo = open_agents(&state)?;
    let removed = writer(&repo, &governance)
        .remove(&name, field)
        .map_err(override_error)?;
    tracing::event!(
        tracing::Level::INFO,
        agent = %name,
        field = %field.as_str(),
        removed = removed,
        "agents.override.removed"
    );
    let agent = resolved(&state, &repo, &name)?;
    drop(repo);
    Ok(Json(shown(&state, agent).await))
}

/// `DELETE /api/v1/agent-definitions/{name}/overrides`: back to defaults,
/// every override of the agent removed.
#[utoipa::path(
    delete,
    path = "/api/v1/agent-definitions/{name}/overrides",
    tag = "agents",
    params(("name" = String, Path, description = "Agent identifier")),
    responses(
        (status = 200, description = "Back to defaults; the agent in force", body = ResolvedAgentResponse),
        (status = 404, description = "Agent not found", body = crate::api::openapi::ApiErrorBody),
        (status = 503, description = "Audit journal not available", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub async fn reset_agent_overrides<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path(name): Path<String>,
) -> Result<Json<ResolvedAgentResponse>, ApiError> {
    let governance = governance(&state)?;
    let repo = open_agents(&state)?;
    let fields = writer(&repo, &governance)
        .reset(&name)
        .map_err(override_error)?;
    tracing::event!(
        tracing::Level::INFO,
        agent = %name,
        removed = fields.len(),
        "agents.override.reset"
    );
    let agent = resolved(&state, &repo, &name)?;
    drop(repo);
    Ok(Json(shown(&state, agent).await))
}

/// Routes of the stored agents.
pub fn agent_definition_routes<B: ExecutionBackend + Clone>() -> Router<AppState<B>> {
    Router::new()
        .route(
            "/api/v1/agent-definitions",
            get(list_agent_definitions::<B>),
        )
        .route(
            "/api/v1/agent-definitions/:name",
            get(get_agent_definition::<B>),
        )
        .route(
            "/api/v1/agent-definitions/:name/overrides",
            axum::routing::delete(reset_agent_overrides::<B>),
        )
        .route(
            "/api/v1/agent-definitions/:name/overrides/:field",
            put(set_agent_override::<B>).delete(remove_agent_override::<B>),
        )
}

#[cfg(test)]
mod tests;
