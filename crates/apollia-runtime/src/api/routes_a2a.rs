//! REST routes for A2A (Agent-to-Agent) routing.
//!
//! Exposes:
//! - `GET  /api/v1/a2a/agents`         , lists active A2A agents with their skills
//! - `POST /api/v1/a2a/delegate`        , delegates a task to a Worker Agent by skill ID
//! - `GET  /api/v1/a2a/skills`          , flat list of all available A2A skills
//! - `POST /api/v1/a2a/invoke`          , high-level invocation via [`A2AInvoker`]
//! - `GET  /api/v1/tasks/:id/sidechains`, delegation tree of a parent task's A2A calls

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};

use apollia_core::ProcessState;

use crate::a2a::invoker::{A2AError, SkillListing};
use crate::a2a::{
    delegate_inner, A2aDelegateResult, A2aError, A2aErrorResponse, DelegateInner,
    DEFAULT_A2A_MAX_HOPS,
};
use crate::api::server::AppState;
use crate::coordinator::ExecutionBackend;

/// Default timeout for A2A delegations, in seconds.
const DEFAULT_DELEGATE_TIMEOUT_SECS: u64 = 120;

// ─────────────────────────────────────────────────────────────────────────────
// Response types, GET /api/v1/a2a/agents
// ─────────────────────────────────────────────────────────────────────────────

/// Skill declared by an A2A agent.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct A2aSkillDto {
    /// Unique skill identifier.
    pub id: String,
    /// Human-readable skill name.
    pub name: String,
    /// Description of what the skill does.
    pub description: String,
    /// Supported input modes (e.g. `["text", "data"]`).
    pub input_modes: Vec<String>,
    /// Supported output modes (e.g. `["text", "file"]`).
    pub output_modes: Vec<String>,
}

/// Entry in the A2A agent list.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct A2aAgentDto {
    /// Unique agent identifier (UUID v4).
    pub agent_id: String,
    /// Agent name as declared in its manifest.
    pub name: String,
    /// Agent semver version.
    pub version: String,
    /// Current process state (`active`, `degraded`, etc.).
    pub state: String,
    /// Skills declared by this agent.
    pub skills: Vec<A2aSkillDto>,
}

/// Response body for `GET /api/v1/a2a/agents`.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct A2aAgentsResponse {
    /// Active A2A agents with their skills.
    pub agents: Vec<A2aAgentDto>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Request types, POST /api/v1/a2a/delegate
// ─────────────────────────────────────────────────────────────────────────────

/// Request body for `POST /api/v1/a2a/delegate`.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct DelegateRequest {
    /// Target skill identifier (e.g. `"read-excel"`).
    pub skill_id: String,
    /// JSON payload passed to the Worker Agent as input.
    #[schema(value_type = Object)]
    pub input: serde_json::Value,
    /// Delegation timeout in seconds (default: 120).
    #[serde(default)]
    pub timeout_secs: Option<u64>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Handlers
// ─────────────────────────────────────────────────────────────────────────────

/// Handler for `GET /api/v1/a2a/agents`.
///
/// Returns all agents with `supports_a2a = true` in active or degraded state,
/// along with their declared skills.
#[utoipa::path(
    get,
    path = "/api/v1/a2a/agents",
    tag = "a2a",
    responses(
        (status = 200, description = "Active A2A agents", body = A2aAgentsResponse),
        (status = 500, description = "Registry error", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub async fn list_a2a_agents<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
) -> Result<Json<A2aAgentsResponse>, (StatusCode, Json<A2aErrorResponse>)> {
    let entries = state
        .registry_handle
        .list_agents()
        .await
        .map_err(|e| registry_err_response(A2aError::Registry(e)))?;

    let mut agents: Vec<A2aAgentDto> = entries
        .into_iter()
        .filter(|e| {
            e.manifest.supports_a2a
                && matches!(
                    e.process_state,
                    ProcessState::Active | ProcessState::Degraded
                )
        })
        .map(|e| {
            let skills = e
                .manifest
                .skills
                .iter()
                .map(|s| A2aSkillDto {
                    id: s.id.clone(),
                    name: s.name.clone(),
                    description: s.description.clone(),
                    input_modes: s.input_modes.clone(),
                    output_modes: s.output_modes.clone(),
                })
                .collect();

            A2aAgentDto {
                agent_id: e.id.to_string(),
                name: e.manifest.name.clone(),
                version: e.manifest.version.clone(),
                state: state_label(&e.process_state),
                skills,
            }
        })
        .collect();

    agents.sort_by(|a, b| a.name.cmp(&b.name));

    Ok(Json(A2aAgentsResponse { agents }))
}

/// Handler for `POST /api/v1/a2a/delegate`.
///
/// Resolves `skill_id` to an active Worker Agent, submits the task, waits for
/// completion (with timeout), and returns the structured result.
#[utoipa::path(
    post,
    path = "/api/v1/a2a/delegate",
    tag = "a2a",
    request_body = DelegateRequest,
    responses(
        (status = 200, description = "Delegation result", body = A2aDelegateResult),
        (status = 404, description = "Skill not found", body = crate::api::openapi::ApiErrorBody),
        (status = 409, description = "Ambiguous skill", body = crate::api::openapi::ApiErrorBody),
        (status = 502, description = "Worker failed", body = crate::api::openapi::ApiErrorBody),
        (status = 504, description = "Delegation timed out", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub async fn delegate<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Json(req): Json<DelegateRequest>,
) -> Result<Json<A2aDelegateResult>, (StatusCode, Json<A2aErrorResponse>)> {
    let timeout_secs = req.timeout_secs.unwrap_or(DEFAULT_DELEGATE_TIMEOUT_SECS);

    // Delegation initiated from the REST API: empty chain, synthetic calling agent.
    let rest_caller = apollia_core::AgentId::from("__rest_api__");
    delegate_inner(DelegateInner {
        registry: &state.registry_handle,
        router: &state.router_handle,
        event_bus: &state.event_sender,
        skill_id: &req.skill_id,
        input_payload: req.input,
        timeout_secs,
        parent_chain: &[],
        current_agent: &rest_caller,
        max_hops: DEFAULT_A2A_MAX_HOPS,
    })
    .await
    .map(Json)
    .map_err(a2a_err_response)
}

// ─────────────────────────────────────────────────────────────────────────────
// Response types, GET /api/v1/a2a/skills
// ─────────────────────────────────────────────────────────────────────────────

/// Response body for `GET /api/v1/a2a/skills`.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct A2aSkillsResponse {
    /// Flat list of all available A2A skills.
    pub skills: Vec<SkillListing>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Request types, POST /api/v1/a2a/invoke
// ─────────────────────────────────────────────────────────────────────────────

/// Request body for `POST /api/v1/a2a/invoke`.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct InvokeRequest {
    /// Target skill identifier (e.g. `"read-excel"`).
    pub skill_id: String,
    /// JSON payload passed to the Worker Agent as input.
    #[schema(value_type = Object)]
    pub input: serde_json::Value,
    /// Caller name (Director Agent), used for observability.
    #[serde(default)]
    pub caller: Option<String>,
    /// Invocation timeout in seconds (default: 120).
    #[serde(default)]
    pub timeout_secs: Option<u64>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Handlers, GET /api/v1/a2a/skills
// ─────────────────────────────────────────────────────────────────────────────

/// Handler for `GET /api/v1/a2a/skills`.
///
/// Returns the flat list of all available A2A skills via the [`A2AInvoker`].
/// Returns 503 if the invoker is not initialized.
#[utoipa::path(
    get,
    path = "/api/v1/a2a/skills",
    tag = "a2a",
    responses(
        (status = 200, description = "Available A2A skills", body = A2aSkillsResponse),
        (status = 503, description = "A2A invoker not initialized", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub async fn list_a2a_skills<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
) -> Result<Json<A2aSkillsResponse>, (StatusCode, Json<A2aErrorResponse>)> {
    let invoker = state.a2a_invoker.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(A2aErrorResponse {
                error: "A2A invoker not initialized".to_string(),
                skill_id: None,
                available_skills: None,
                conflicting_agents: None,
            }),
        )
    })?;

    let skills = invoker.list_skills().await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(A2aErrorResponse {
                error: e.to_string(),
                skill_id: None,
                available_skills: None,
                conflicting_agents: None,
            }),
        )
    })?;

    Ok(Json(A2aSkillsResponse { skills }))
}

/// Handler for `POST /api/v1/a2a/invoke`.
///
/// Invokes a Worker Agent by skill ID via the [`A2AInvoker`].
/// Returns 503 if the invoker is not initialized.
/// Returns 404 if the skill is not found, 503 if the agent is not active.
#[utoipa::path(
    post,
    path = "/api/v1/a2a/invoke",
    tag = "a2a",
    request_body = InvokeRequest,
    responses(
        (status = 200, description = "Invocation result", body = crate::a2a::invoker::A2AInvocationResult),
        (status = 404, description = "Skill not found", body = crate::api::openapi::ApiErrorBody),
        (status = 503, description = "A2A invoker not initialized or agent not active", body = crate::api::openapi::ApiErrorBody),
        (status = 504, description = "Invocation timed out", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub async fn invoke_by_skill<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Json(req): Json<InvokeRequest>,
) -> Result<Json<crate::a2a::invoker::A2AInvocationResult>, (StatusCode, Json<A2aErrorResponse>)> {
    let invoker = state.a2a_invoker.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(A2aErrorResponse {
                error: "A2A invoker not initialized".to_string(),
                skill_id: None,
                available_skills: None,
                conflicting_agents: None,
            }),
        )
    })?;

    let caller = req.caller.as_deref().unwrap_or("api");
    let timeout = req.timeout_secs.map(std::time::Duration::from_secs);

    invoker
        .invoke(crate::a2a::A2AInvokeRequest {
            skill_id: &req.skill_id,
            input: req.input,
            caller,
            a2a_depth: 0,
            timeout,
            chain_deadline: None,
        })
        .await
        .map(Json)
        .map_err(a2a_invoker_err_response)
}

// ─────────────────────────────────────────────────────────────────────────────
// Handler, GET /api/v1/tasks/{task_id}/sidechains
// ─────────────────────────────────────────────────────────────────────────────

/// Handler for `GET /api/v1/tasks/{task_id}/sidechains`.
///
/// Returns all A2A delegations recorded for the parent task.
/// Returns 404 if no delegation is found for this `task_id`.
/// Returns 503 if the sidechain logger is not initialized.
#[utoipa::path(
    get,
    path = "/api/v1/tasks/{id}/sidechains",
    tag = "a2a",
    params(("id" = String, Path, description = "Parent task id")),
    responses(
        (status = 200, description = "Delegation sidechains", body = [crate::a2a::SidechainRow]),
        (status = 404, description = "No sidechains for this task", body = crate::api::openapi::ApiErrorBody),
        (status = 503, description = "Sidechain logging not initialized", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub async fn get_task_sidechains<B: ExecutionBackend + Clone>(
    Path(task_id): Path<String>,
    State(state): State<AppState<B>>,
) -> Result<Json<Vec<crate::a2a::SidechainRow>>, (StatusCode, Json<A2aErrorResponse>)> {
    let invoker = state.a2a_invoker.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(A2aErrorResponse {
                error: "A2A invoker not initialized".to_string(),
                skill_id: None,
                available_skills: None,
                conflicting_agents: None,
            }),
        )
    })?;

    let logger = invoker.sidechain_logger().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(A2aErrorResponse {
                error: "sidechain logging not initialized".to_string(),
                skill_id: None,
                available_skills: None,
                conflicting_agents: None,
            }),
        )
    })?;

    let rows = logger.list_by_parent(&task_id).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(A2aErrorResponse {
                error: e.to_string(),
                skill_id: None,
                available_skills: None,
                conflicting_agents: None,
            }),
        )
    })?;

    if rows.is_empty() {
        return Err((
            StatusCode::NOT_FOUND,
            Json(A2aErrorResponse {
                error: format!("no sidechain delegations found for task '{task_id}'"),
                skill_id: None,
                available_skills: None,
                conflicting_agents: None,
            }),
        ));
    }

    Ok(Json(rows))
}

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Converts a [`ProcessState`] into a string label.
fn state_label(state: &ProcessState) -> String {
    match state {
        ProcessState::Initializing => "initializing",
        ProcessState::Active => "active",
        ProcessState::Degraded => "degraded",
        ProcessState::Stopping => "stopping",
        ProcessState::Stopped => "stopped",
    }
    .to_string()
}

/// Converts an [`A2aError`] into an axum HTTP response with the appropriate status.
fn a2a_err_response(err: A2aError) -> (StatusCode, Json<A2aErrorResponse>) {
    let status = match &err {
        A2aError::SkillNotFound { .. } => StatusCode::NOT_FOUND,
        A2aError::AmbiguousSkill { .. } => StatusCode::CONFLICT,
        A2aError::Timeout { .. } => StatusCode::GATEWAY_TIMEOUT,
        A2aError::WorkerFailed { .. } => StatusCode::BAD_GATEWAY,
        A2aError::Registry(_) | A2aError::RouterDead => StatusCode::INTERNAL_SERVER_ERROR,
        A2aError::CycleDetected { .. } | A2aError::MaxHopsExceeded { .. } => {
            StatusCode::UNPROCESSABLE_ENTITY
        }
    };
    (status, Json(A2aErrorResponse::from_error(&err)))
}

/// Converts an [`A2aError`] wrapping a registry error into an HTTP response.
fn registry_err_response(err: A2aError) -> (StatusCode, Json<A2aErrorResponse>) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(A2aErrorResponse::from_error(&err)),
    )
}

/// Converts an [`A2AError`] (high-level invoker) into an axum HTTP response.
fn a2a_invoker_err_response(err: A2AError) -> (StatusCode, Json<A2aErrorResponse>) {
    let status = match &err {
        A2AError::SkillNotFound { .. } => StatusCode::NOT_FOUND,
        A2AError::AgentNotActive { .. } => StatusCode::SERVICE_UNAVAILABLE,
        A2AError::Timeout { .. } => StatusCode::GATEWAY_TIMEOUT,
        A2AError::ExecutionFailed { .. } => StatusCode::BAD_GATEWAY,
        A2AError::RegistryError(_) => StatusCode::INTERNAL_SERVER_ERROR,
        A2AError::MaxDepthExceeded { .. }
        | A2AError::SelfInvocation { .. }
        | A2AError::ChainTimeoutExceeded { .. } => StatusCode::TOO_MANY_REQUESTS,
    };
    let skill_id = match &err {
        A2AError::SkillNotFound { skill_id, .. } => Some(skill_id.clone()),
        A2AError::Timeout { skill_id, .. } => Some(skill_id.clone()),
        _ => None,
    };
    (
        status,
        Json(A2aErrorResponse {
            error: err.to_string(),
            skill_id,
            available_skills: None,
            conflicting_agents: None,
        }),
    )
}
