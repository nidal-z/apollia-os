//! Routes REST pour le routing A2A (Agent-to-Agent).
//!
//! Expose :
//! - `GET  /api/v1/a2a/agents`   — liste les agents A2A actifs avec leurs skills
//! - `POST /api/v1/a2a/delegate` — délègue une tâche à un Worker Agent par skill ID

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};

use apollia_core::ProcessState;

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
