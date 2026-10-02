//! Model presets, the model of each agent, and the default model.
//!
//! - `GET    /api/v1/llm/presets`              , list presets (optionally of one model)
//! - `POST   /api/v1/llm/presets`              , create a preset
//! - `PUT    /api/v1/llm/presets/{id}`         , replace a preset's name and settings
//! - `DELETE /api/v1/llm/presets/{id}`         , delete a preset
//! - `GET    /api/v1/llm/agents/{agent}/model` , read the model assigned to an agent
//! - `PUT    /api/v1/llm/agents/{agent}/model` , assign a model (and a preset) to an agent
//! - `DELETE /api/v1/llm/agents/{agent}/model` , remove the assignment
//! - `GET    /api/v1/llm/default-model`        , read the default model
//! - `PUT    /api/v1/llm/default-model`        , set the default model
//!
//! These routes store; they resolve nothing at call time.

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{get, put};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use apollia_core::model_preset::{
    AgentModel, DefaultModel, ModelPreset, ModelSource, PresetDraft, RequestSettings,
};

use crate::api::routes_llm::backends::{map_backend_error, sync_toml_after_mutation};
use crate::api::routes_llm::servers::{api_err, with_repo, ApiError};
use crate::api::server::AppState;
use crate::coordinator::ExecutionBackend;

/// Where a model runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ModelSourceDto {
    /// On this machine, served by the local engine.
    Local,
    /// On a declared inference server.
    Server,
}

impl From<ModelSourceDto> for ModelSource {
    fn from(s: ModelSourceDto) -> Self {
        match s {
            ModelSourceDto::Local => ModelSource::Local,
            ModelSourceDto::Server => ModelSource::Server,
        }
    }
}

impl From<ModelSource> for ModelSourceDto {
    fn from(s: ModelSource) -> Self {
        match s {
            ModelSource::Local => ModelSourceDto::Local,
            ModelSource::Server => ModelSourceDto::Server,
        }
    }
}

/// One preset.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct PresetResponse {
    /// Identifier.
    pub id: i64,
    /// Name, unique among the presets of the model.
    pub name: String,
    /// Where the model runs.
    pub model_source: ModelSourceDto,
    /// The model: a local registry identifier, or `<server>/<model>`.
    pub model_id: String,
    /// Load settings, applied over the automatic computation.
    #[schema(value_type = Object)]
    pub load: serde_json::Map<String, serde_json::Value>,
    /// Request settings: `temperature`, `max_tokens`, `stop`, `top_k`,
    /// `top_p`, `min_p`, `repeat_penalty`, `presence_penalty`, `json_schema`.
    #[schema(value_type = Object)]
    pub request: RequestSettings,
    /// Whether this is the default preset of its model.
    pub is_model_default: bool,
}

impl From<ModelPreset> for PresetResponse {
    fn from(p: ModelPreset) -> Self {
        Self {
            id: p.id,
            name: p.name,
            model_source: p.model_source.into(),
            model_id: p.model_id,
            load: p.load,
            request: p.request,
            is_model_default: p.is_model_default,
        }
    }
}

/// Body of `GET /api/v1/llm/presets`.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct PresetsListResponse {
    /// Presets, sorted by model then name.
    pub presets: Vec<PresetResponse>,
}

/// Query of `GET /api/v1/llm/presets`: both fields, or neither.
#[derive(Debug, Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct PresetsQuery {
    /// Only the presets of a model running here.
    pub model_source: Option<ModelSourceDto>,
    /// Only the presets of this model.
    pub model_id: Option<String>,
}

/// Body of `POST /api/v1/llm/presets`.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct CreatePresetRequest {
    /// Name, unique among the presets of the model.
    pub name: String,
    /// Where the model runs.
    pub model_source: ModelSourceDto,
    /// The model the preset is attached to. Fixed once created.
    pub model_id: String,
    /// Load settings. Never a system prompt.
    #[serde(default)]
    #[schema(value_type = Object)]
    pub load: serde_json::Map<String, serde_json::Value>,
    /// Request settings. Unknown fields, a system prompt among them, are refused.
    #[serde(default)]
    #[schema(value_type = Object)]
    pub request: RequestSettings,
    /// Make it the default preset of its model (default: `false`).
    #[serde(default)]
    pub is_model_default: bool,
}

/// Body of `PUT /api/v1/llm/presets/{id}`.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct UpdatePresetRequest {
    /// Name, unique among the presets of the model.
    pub name: String,
    /// Load settings. Never a system prompt.
    #[serde(default)]
    #[schema(value_type = Object)]
    pub load: serde_json::Map<String, serde_json::Value>,
    /// Request settings. Unknown fields, a system prompt among them, are refused.
    #[serde(default)]
    #[schema(value_type = Object)]
    pub request: RequestSettings,
    /// Make it the default preset of its model (default: `false`).
    #[serde(default)]
    pub is_model_default: bool,
}

/// Body of `DELETE /api/v1/llm/presets/{id}`.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct DeletePresetResponse {
    /// Identifier of the deleted preset.
    pub deleted: i64,
}

/// The model assigned to an agent.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct AgentModelResponse {
    /// Agent name.
    pub agent: String,
    /// Where the model runs.
    pub model_source: ModelSourceDto,
    /// The model: a local registry identifier, or `<server>/<model>`.
    pub model_id: String,
    /// The preset chosen for the agent; `null` leaves the model's default.
    pub preset_id: Option<i64>,
}

impl From<AgentModel> for AgentModelResponse {
    fn from(a: AgentModel) -> Self {
        Self {
            agent: a.agent,
            model_source: a.model_source.into(),
            model_id: a.model_id,
            preset_id: a.preset_id,
        }
    }
}

/// Body of `PUT /api/v1/llm/agents/{agent}/model`.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct SetAgentModelRequest {
    /// Where the model runs.
    pub model_source: ModelSourceDto,
    /// The model: a local registry identifier, or `<server>/<model>` of a
    /// declared server.
    pub model_id: String,
    /// A preset of that same model, or `null` for the model's default.
    #[serde(default)]
    pub preset_id: Option<i64>,
}

/// Body of `DELETE /api/v1/llm/agents/{agent}/model`.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ClearAgentModelResponse {
    /// Agent name.
    pub agent: String,
    /// Whether an assignment existed.
    pub cleared: bool,
}

/// The default model of the machine.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct DefaultModelResponse {
    /// Name of the backend that holds the default, the name the router routes by.
    pub backend: String,
    /// Where the model runs.
    pub model_source: ModelSourceDto,
    /// The model, in the form `PUT` accepts.
    pub model_id: String,
}

impl From<DefaultModel> for DefaultModelResponse {
    fn from(d: DefaultModel) -> Self {
        Self {
            backend: d.backend,
            model_source: d.model_source.into(),
            model_id: d.model_id,
        }
    }
}

/// Body of `PUT /api/v1/llm/default-model`.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct SetDefaultModelRequest {
    /// Where the model runs.
    pub model_source: ModelSourceDto,
    /// `local`: the name of a local engine backend. `server`: `<server>/<model>`
    /// of a declared server, or the name of a server backend.
    pub model_id: String,
}

/// `GET /api/v1/llm/presets`: every preset, or those of one model.
#[utoipa::path(
    get,
    path = "/api/v1/llm/presets",
    tag = "llm",
    params(PresetsQuery),
    responses(
        (status = 200, description = "Presets", body = PresetsListResponse),
        (status = 422, description = "Only one of model_source and model_id given", body = crate::api::openapi::ApiErrorBody),
        (status = 503, description = "Backend repository not available", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub async fn list_llm_presets<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Query(query): Query<PresetsQuery>,
) -> Result<Json<PresetsListResponse>, ApiError> {
    let model = match (query.model_source, query.model_id.as_deref()) {
        (Some(source), Some(id)) => Some((ModelSource::from(source), id)),
        (None, None) => None,
        _ => {
            return Err(api_err(
                StatusCode::UNPROCESSABLE_ENTITY,
                "model_source and model_id go together",
            ))
        }
    };
    let presets = with_repo(&state, |repo| {
        repo.list_presets(model).map_err(map_backend_error)
    })?;
    Ok(Json(PresetsListResponse {
        presets: presets.into_iter().map(PresetResponse::from).collect(),
    }))
}

/// `POST /api/v1/llm/presets`: create a preset for a model.
#[utoipa::path(
    post,
    path = "/api/v1/llm/presets",
    tag = "llm",
    request_body = CreatePresetRequest,
    responses(
        (status = 201, description = "Preset created", body = PresetResponse),
        (status = 409, description = "The model already has a preset of that name", body = crate::api::openapi::ApiErrorBody),
        (status = 422, description = "Empty name or model, or a system prompt among the settings", body = crate::api::openapi::ApiErrorBody),
        (status = 503, description = "Backend repository not available", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub async fn create_llm_preset<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Json(req): Json<CreatePresetRequest>,
) -> Result<(StatusCode, Json<PresetResponse>), ApiError> {
    let draft = PresetDraft {
        name: req.name,
        load: req.load,
        request: req.request,
        is_model_default: req.is_model_default,
    };
    let preset = with_repo(&state, |repo| {
        repo.create_preset(req.model_source.into(), &req.model_id, &draft)
            .map_err(map_backend_error)
    })?;
    tracing::event!(
        tracing::Level::INFO,
        preset = preset.id,
        model = %preset.model_id,
        "llm.preset.created"
    );
    Ok((StatusCode::CREATED, Json(preset.into())))
}

/// `PUT /api/v1/llm/presets/{id}`: replace a preset's name and settings.
#[utoipa::path(
    put,
    path = "/api/v1/llm/presets/{id}",
    tag = "llm",
    params(("id" = i64, Path, description = "Preset identifier")),
    request_body = UpdatePresetRequest,
    responses(
        (status = 200, description = "Preset updated", body = PresetResponse),
        (status = 404, description = "Preset not found", body = crate::api::openapi::ApiErrorBody),
        (status = 409, description = "The model already has a preset of that name", body = crate::api::openapi::ApiErrorBody),
        (status = 422, description = "Empty name, or a system prompt among the settings", body = crate::api::openapi::ApiErrorBody),
        (status = 503, description = "Backend repository not available", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub async fn update_llm_preset<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path(id): Path<i64>,
    Json(req): Json<UpdatePresetRequest>,
) -> Result<Json<PresetResponse>, ApiError> {
    let draft = PresetDraft {
        name: req.name,
        load: req.load,
        request: req.request,
        is_model_default: req.is_model_default,
    };
    let preset = with_repo(&state, |repo| {
        repo.update_preset(id, &draft).map_err(map_backend_error)
    })?;
    tracing::event!(tracing::Level::INFO, preset = id, "llm.preset.updated");
    Ok(Json(preset.into()))
}

/// `DELETE /api/v1/llm/presets/{id}`: delete a preset. An agent that had
/// chosen it goes back to its model's default preset.
#[utoipa::path(
    delete,
    path = "/api/v1/llm/presets/{id}",
    tag = "llm",
    params(("id" = i64, Path, description = "Preset identifier")),
    responses(
        (status = 200, description = "Preset deleted", body = DeletePresetResponse),
        (status = 404, description = "Preset not found", body = crate::api::openapi::ApiErrorBody),
        (status = 503, description = "Backend repository not available", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub async fn delete_llm_preset<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path(id): Path<i64>,
) -> Result<Json<DeletePresetResponse>, ApiError> {
    with_repo(&state, |repo| {
        repo.delete_preset(id).map_err(map_backend_error)
    })?;
    tracing::event!(tracing::Level::INFO, preset = id, "llm.preset.deleted");
    Ok(Json(DeletePresetResponse { deleted: id }))
}

/// `GET /api/v1/llm/agents/{agent}/model`: the model the user assigned to an agent.
#[utoipa::path(
    get,
    path = "/api/v1/llm/agents/{agent}/model",
    tag = "llm",
    params(("agent" = String, Path, description = "Agent name")),
    responses(
        (status = 200, description = "The agent's model", body = AgentModelResponse),
        (status = 404, description = "No model assigned to this agent", body = crate::api::openapi::ApiErrorBody),
        (status = 503, description = "Backend repository not available", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub async fn get_agent_model<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path(agent): Path<String>,
) -> Result<Json<AgentModelResponse>, ApiError> {
    let assigned = with_repo(&state, |repo| {
        repo.agent_model(&agent).map_err(map_backend_error)
    })?;
    assigned.map(|a| Json(a.into())).ok_or_else(|| {
        api_err(
            StatusCode::NOT_FOUND,
            format!("no model assigned to agent '{agent}'"),
        )
    })
}

/// `PUT /api/v1/llm/agents/{agent}/model`: assign a model, and optionally a
/// preset of it, to an agent. Kept apart from the installed package.
#[utoipa::path(
    put,
    path = "/api/v1/llm/agents/{agent}/model",
    tag = "llm",
    params(("agent" = String, Path, description = "Agent name")),
    request_body = SetAgentModelRequest,
    responses(
        (status = 200, description = "Model assigned", body = AgentModelResponse),
        (status = 404, description = "Preset not found", body = crate::api::openapi::ApiErrorBody),
        (status = 422, description = "Empty model, undeclared server, or a preset of another model", body = crate::api::openapi::ApiErrorBody),
        (status = 503, description = "Backend repository not available", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub async fn set_agent_model<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path(agent): Path<String>,
    Json(req): Json<SetAgentModelRequest>,
) -> Result<Json<AgentModelResponse>, ApiError> {
    let assignment = AgentModel {
        agent,
        model_source: req.model_source.into(),
        model_id: req.model_id,
        preset_id: req.preset_id,
    };
    with_repo(&state, |repo| {
        repo.set_agent_model(&assignment).map_err(map_backend_error)
    })?;
    tracing::event!(
        tracing::Level::INFO,
        agent = %assignment.agent,
        model = %assignment.model_id,
        "llm.agent_model.set"
    );
    Ok(Json(assignment.into()))
}

/// `DELETE /api/v1/llm/agents/{agent}/model`: remove the assignment; the agent
/// goes back to the model its package declares, or to the default.
#[utoipa::path(
    delete,
    path = "/api/v1/llm/agents/{agent}/model",
    tag = "llm",
    params(("agent" = String, Path, description = "Agent name")),
    responses(
        (status = 200, description = "Assignment removed, or there was none", body = ClearAgentModelResponse),
        (status = 503, description = "Backend repository not available", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub async fn clear_agent_model<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path(agent): Path<String>,
) -> Result<Json<ClearAgentModelResponse>, ApiError> {
    let cleared = with_repo(&state, |repo| {
        repo.clear_agent_model(&agent).map_err(map_backend_error)
    })?;
    Ok(Json(ClearAgentModelResponse { agent, cleared }))
}

/// The last time an agent's model was not available, so a call ran on the
/// default model instead.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct AgentModelFallbackResponse {
    /// Agent name.
    pub agent: String,
    /// Model the agent asked for: the user's assignment, or the backend its
    /// manifest declares.
    pub requested: String,
    /// Model that served the call instead.
    pub used: String,
    /// When it happened, RFC 3339.
    pub at: String,
}

/// `GET /api/v1/llm/agents/{agent}/model/fallback`: the last time the agent's
/// model was not available since the runtime started, for its sheet.
#[utoipa::path(
    get,
    path = "/api/v1/llm/agents/{agent}/model/fallback",
    tag = "llm",
    params(("agent" = String, Path, description = "Agent name")),
    responses(
        (status = 200, description = "The agent's last fallback", body = AgentModelFallbackResponse),
        (status = 404, description = "The agent's model has not fallen back since the runtime started", body = crate::api::openapi::ApiErrorBody),
        (status = 503, description = "No LLM router configured", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub async fn get_agent_model_fallback<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path(agent): Path<String>,
) -> Result<Json<AgentModelFallbackResponse>, ApiError> {
    let router = state.llm_router.read().await.clone().ok_or_else(|| {
        api_err(
            StatusCode::SERVICE_UNAVAILABLE,
            "no LLM router configured".to_string(),
        )
    })?;
    let fallback = router
        .latest()
        .agent_model_fallback(&agent)
        .ok_or_else(|| {
            api_err(
                StatusCode::NOT_FOUND,
                format!("the model of agent '{agent}' has not fallen back"),
            )
        })?;
    Ok(Json(AgentModelFallbackResponse {
        agent: fallback.agent,
        requested: fallback.requested,
        used: fallback.used,
        at: fallback.at,
    }))
}

/// `GET /api/v1/llm/default-model`: the default model of the machine.
#[utoipa::path(
    get,
    path = "/api/v1/llm/default-model",
    tag = "llm",
    responses(
        (status = 200, description = "The default model", body = DefaultModelResponse),
        (status = 404, description = "No default model", body = crate::api::openapi::ApiErrorBody),
        (status = 503, description = "Backend repository not available", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub async fn get_default_model<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
) -> Result<Json<DefaultModelResponse>, ApiError> {
    let default = with_repo(&state, |repo| {
        repo.default_model().map_err(map_backend_error)
    })?;
    default
        .map(|d| Json(d.into()))
        .ok_or_else(|| api_err(StatusCode::NOT_FOUND, "no default model"))
}

/// `PUT /api/v1/llm/default-model`: make a local model or a model of a
/// declared server the single default of the machine.
#[utoipa::path(
    put,
    path = "/api/v1/llm/default-model",
    tag = "llm",
    request_body = SetDefaultModelRequest,
    responses(
        (status = 200, description = "Default model set", body = DefaultModelResponse),
        (status = 404, description = "Backend not found", body = crate::api::openapi::ApiErrorBody),
        (status = 422, description = "A model of the wrong kind, or an undeclared server", body = crate::api::openapi::ApiErrorBody),
        (status = 503, description = "Backend repository not available", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub async fn set_default_model<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Json(req): Json<SetDefaultModelRequest>,
) -> Result<Json<DefaultModelResponse>, ApiError> {
    let default = with_repo(&state, |repo| {
        let default = repo
            .set_default_model(req.model_source.into(), &req.model_id)
            .map_err(map_backend_error)?;
        sync_toml_after_mutation(&state, repo);
        Ok(default)
    })?;
    tracing::event!(
        tracing::Level::INFO,
        backend = %default.backend,
        model = %default.model_id,
        "llm.default_model.set"
    );
    Ok(Json(default.into()))
}

/// The preset, agent-model and default-model routes, merged into [`super::llm_routes`].
pub fn preset_routes<B: ExecutionBackend + Clone>() -> Router<AppState<B>> {
    Router::new()
        .route(
            "/api/v1/llm/presets",
            get(list_llm_presets::<B>).post(create_llm_preset::<B>),
        )
        .route(
            "/api/v1/llm/presets/:id",
            put(update_llm_preset::<B>).delete(delete_llm_preset::<B>),
        )
        .route(
            "/api/v1/llm/agents/:agent/model",
            get(get_agent_model::<B>)
                .put(set_agent_model::<B>)
                .delete(clear_agent_model::<B>),
        )
        .route(
            "/api/v1/llm/agents/:agent/model/fallback",
            get(get_agent_model_fallback::<B>),
        )
        .route(
            "/api/v1/llm/default-model",
            get(get_default_model::<B>).put(set_default_model::<B>),
        )
}

#[cfg(test)]
mod tests;
