//! Routes REST pour le routing A2A (Agent-to-Agent).
//!
//! Expose :
//! - `GET  /api/v1/a2a/agents`          — liste les agents A2A actifs avec leurs skills
//! - `POST /api/v1/a2a/delegate`         — délègue une tâche à un Worker Agent par skill ID
//! - `GET  /api/v1/a2a/skills`           — liste plate de tous les skills A2A disponibles
//! - `POST /api/v1/a2a/invoke`           — invocation haut niveau via [`A2AInvoker`]
//! - `GET  /api/v1/tasks/:id/sidechains` — arborescence des délégations A2A d'une tâche parent

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};

use apollia_core::ProcessState;

use crate::a2a::invoker::{A2AError, SkillListing};
use crate::a2a::{delegate_inner, A2aDelegateResult, A2aError, A2aErrorResponse};
use crate::api::server::AppState;
use crate::coordinator::ExecutionBackend;

/// Timeout par défaut des délégations A2A en secondes.
const DEFAULT_DELEGATE_TIMEOUT_SECS: u64 = 120;

// ─────────────────────────────────────────────────────────────────────────────
// Types de réponse — GET /api/v1/a2a/agents
// ─────────────────────────────────────────────────────────────────────────────

/// Skill déclaré par un agent A2A.
#[derive(Debug, Serialize)]
pub struct A2aSkillDto {
    /// Identifiant unique du skill.
    pub id: String,
    /// Nom humain du skill.
    pub name: String,
    /// Description de ce que fait le skill.
    pub description: String,
    /// Modes d'entrée supportés (ex: `["text", "data"]`).
    pub input_modes: Vec<String>,
    /// Modes de sortie supportés (ex: `["text", "file"]`).
    pub output_modes: Vec<String>,
}

/// Entrée dans la liste des agents A2A.
#[derive(Debug, Serialize)]
pub struct A2aAgentDto {
    /// Identifiant unique de l'agent (UUID v4).
    pub agent_id: String,
    /// Nom de l'agent tel que déclaré dans son manifest.
    pub name: String,
    /// Version semver de l'agent.
    pub version: String,
    /// État courant du processus (`active`, `degraded`, etc.).
    pub state: String,
    /// Skills déclarés par cet agent.
    pub skills: Vec<A2aSkillDto>,
}

/// Corps de réponse pour `GET /api/v1/a2a/agents`.
#[derive(Debug, Serialize)]
pub struct A2aAgentsResponse {
    /// Agents A2A actifs avec leurs skills.
    pub agents: Vec<A2aAgentDto>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Types de requête — POST /api/v1/a2a/delegate
// ─────────────────────────────────────────────────────────────────────────────

/// Corps de requête pour `POST /api/v1/a2a/delegate`.
#[derive(Debug, Deserialize)]
pub struct DelegateRequest {
    /// Identifiant du skill cible (ex: `"read-excel"`).
    pub skill_id: String,
    /// Payload JSON transmis au Worker Agent comme input.
    pub input: serde_json::Value,
    /// Timeout de la délégation en secondes (défaut: 120).
    #[serde(default)]
    pub timeout_secs: Option<u64>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Handlers
// ─────────────────────────────────────────────────────────────────────────────

/// Handler pour `GET /api/v1/a2a/agents`.
///
/// Retourne tous les agents avec `supports_a2a = true` en état actif ou dégradé,
/// avec leurs skills déclarés.
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

/// Handler pour `POST /api/v1/a2a/delegate`.
///
/// Résout le `skill_id` vers un Worker Agent actif, soumet la tâche, attend la
/// complétion (avec timeout), et retourne le résultat structuré.
pub async fn delegate<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Json(req): Json<DelegateRequest>,
) -> Result<Json<A2aDelegateResult>, (StatusCode, Json<A2aErrorResponse>)> {
    let timeout_secs = req.timeout_secs.unwrap_or(DEFAULT_DELEGATE_TIMEOUT_SECS);

    delegate_inner(
        &state.registry_handle,
        &state.router_handle,
        &state.event_sender,
        &req.skill_id,
        req.input,
        timeout_secs,
    )
    .await
    .map(Json)
    .map_err(a2a_err_response)
}

// ─────────────────────────────────────────────────────────────────────────────
// Types de réponse — GET /api/v1/a2a/skills
// ─────────────────────────────────────────────────────────────────────────────

/// Corps de réponse pour `GET /api/v1/a2a/skills`.
#[derive(Debug, Serialize)]
pub struct A2aSkillsResponse {
    /// Liste plate de tous les skills A2A disponibles.
    pub skills: Vec<SkillListing>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Types de requête — POST /api/v1/a2a/invoke
// ─────────────────────────────────────────────────────────────────────────────

/// Corps de requête pour `POST /api/v1/a2a/invoke`.
#[derive(Debug, Deserialize)]
pub struct InvokeRequest {
    /// Identifiant du skill cible (ex: `"read-excel"`).
    pub skill_id: String,
    /// Payload JSON transmis au Worker Agent comme input.
    pub input: serde_json::Value,
    /// Nom du caller (Director Agent) — utilisé pour l'observabilité.
    #[serde(default)]
    pub caller: Option<String>,
    /// Timeout de l'invocation en secondes (défaut: 120).
    #[serde(default)]
    pub timeout_secs: Option<u64>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Handlers — GET /api/v1/a2a/skills
// ─────────────────────────────────────────────────────────────────────────────

/// Handler pour `GET /api/v1/a2a/skills`.
///
/// Retourne la liste plate de tous les skills A2A disponibles via l'[`A2AInvoker`].
/// Retourne 503 si l'invoker n'est pas initialisé.
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

/// Handler pour `POST /api/v1/a2a/invoke`.
///
/// Invoque un Worker Agent par skill ID via l'[`A2AInvoker`].
/// Retourne 503 si l'invoker n'est pas initialisé.
/// Retourne 404 si le skill est introuvable, 503 si l'agent n'est pas actif.
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
        .invoke(&req.skill_id, req.input, caller, 0, timeout, None)
        .await
        .map(Json)
        .map_err(a2a_invoker_err_response)
}

// ─────────────────────────────────────────────────────────────────────────────
// Handler — GET /api/v1/tasks/{task_id}/sidechains
// ─────────────────────────────────────────────────────────────────────────────

/// Handler pour `GET /api/v1/tasks/{task_id}/sidechains`.
///
/// Retourne l'ensemble des délégations A2A enregistrées pour la tâche parente.
/// Retourne 404 si aucune délégation n'est trouvée pour ce `task_id`.
/// Retourne 503 si le sidechain logger n'est pas initialisé.
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

/// Convertit un [`ProcessState`] en label string.
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

/// Convertit une [`A2aError`] en réponse HTTP axum avec statut approprié.
fn a2a_err_response(err: A2aError) -> (StatusCode, Json<A2aErrorResponse>) {
    let status = match &err {
        A2aError::SkillNotFound { .. } => StatusCode::NOT_FOUND,
        A2aError::AmbiguousSkill { .. } => StatusCode::CONFLICT,
        A2aError::Timeout { .. } => StatusCode::GATEWAY_TIMEOUT,
        A2aError::WorkerFailed { .. } => StatusCode::BAD_GATEWAY,
        A2aError::Registry(_) | A2aError::RouterDead => StatusCode::INTERNAL_SERVER_ERROR,
    };
    (status, Json(A2aErrorResponse::from_error(&err)))
}

/// Convertit une [`A2aError`] encapsulant une erreur registry en réponse HTTP.
fn registry_err_response(err: A2aError) -> (StatusCode, Json<A2aErrorResponse>) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(A2aErrorResponse::from_error(&err)),
    )
}

/// Convertit une [`A2AError`] (invoker de haut niveau) en réponse HTTP axum.
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
