//! The agents of the machine, `GET /api/v1/agents/installed`.
//!
//! `GET /api/v1/agents` lists what the registry holds; an agent stored but
//! disabled is absent from it. This view is the one an operator reads: the
//! rows of `agents.db`, each enriched with its runtime state when it is
//! registered, followed by the agents registered without being stored. The
//! desktop builds the same view from its own handles when the runtime runs in
//! its process, and reads it here when it attached to a daemon, so both go
//! through [`merge_agent_views`].
//!
//! The Apollia agent, the built-in chat, is not part of this view: it is
//! never started, stopped nor uninstalled. An agent arrives with a `.apo`
//! package (`/api/v1/packages/`) or is created in the interface
//! (`/api/v1/agent-definitions`).

use std::path::PathBuf;

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};

use apollia_core::AgentManifest;
use apollia_tools::{AgentRepository, StoredAgent};

use super::{state_to_string, ErrorResponse};
use crate::api::server::AppState;
use crate::coordinator::ExecutionBackend;
use crate::registry::AgentEntry;

/// Skill declared by an agent in its manifest.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct AgentSkillView {
    /// Unique skill identifier (e.g. `"read-excel"`).
    pub id: String,
    /// Human-readable skill name.
    pub name: String,
    /// Short skill description.
    pub description: String,
}

/// One agent of the machine: stored, registered, or both.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct InstalledAgentView {
    /// Runtime identifier, `None` when stored but not registered.
    pub id: Option<String>,
    /// Unique agent name.
    pub name: String,
    /// Semver version.
    pub version: String,
    /// Whether the agent is made available at boot.
    pub enabled: bool,
    /// Runtime state (`"active"`, `"degraded"`, `"stopped"`...), `None` when
    /// not started.
    pub runtime_status: Option<String>,
    /// Install timestamp (RFC 3339), `None` for an agent registered without
    /// being stored.
    pub installed_at: Option<String>,
    /// What the agent is for.
    pub description: Option<String>,
    /// Free-form tags for routing and discovery.
    pub tags: Vec<String>,
    /// Execution mode (`"auto"`, `"direct"`, `"orchestrated"`).
    pub execution_mode: Option<String>,
    /// Whether the agent takes part in agent-to-agent calls.
    pub supports_a2a: bool,
    /// Declared skills.
    pub skills: Vec<AgentSkillView>,
    /// Semantic role: `"worker"`, `"assistant"`, `"system"`, or `None`.
    pub agent_type: Option<String>,
    /// Prompt examples of typical uses.
    pub examples: Vec<String>,
    /// What the agent does not do.
    pub limitations: Vec<String>,
    /// Configuration needed before the first use.
    pub setup_notes: Option<String>,
    /// Name of the Python class of the agent.
    pub agent_class: Option<String>,
    /// Primary memory namespace, as the manifest declares it.
    pub memory_namespace: Option<String>,
    /// Shared memory namespaces the agent reads.
    pub shared_memory_namespaces: Vec<String>,
}

fn view(
    manifest: &AgentManifest,
    entry: Option<&AgentEntry>,
    stored: Option<&StoredAgent>,
) -> InstalledAgentView {
    InstalledAgentView {
        id: entry.map(|e| e.id.to_string()),
        name: stored.map_or_else(|| manifest.name.clone(), |a| a.name.clone()),
        version: stored.map_or_else(|| manifest.version.clone(), |a| a.version.clone()),
        enabled: stored.is_none_or(|a| a.enabled),
        runtime_status: entry.map(|e| state_to_string(&e.process_state)),
        installed_at: stored.map(|a| a.installed_at.clone()),
        description: (!manifest.description.is_empty()).then(|| manifest.description.clone()),
        tags: manifest.tags.clone(),
        execution_mode: Some(manifest.execution_mode.clone()),
        supports_a2a: manifest.supports_a2a,
        skills: manifest
            .skills
            .iter()
            .map(|s| AgentSkillView {
                id: s.id.clone(),
                name: s.name.clone(),
                description: s.description.clone(),
            })
            .collect(),
        agent_type: manifest.agent_type.clone(),
        examples: manifest.examples.clone(),
        limitations: manifest.limitations.clone(),
        setup_notes: manifest.setup_notes.clone(),
        agent_class: manifest.agent_class.clone(),
        memory_namespace: manifest.memory_namespace.clone(),
        shared_memory_namespaces: manifest.shared_memory_namespaces.clone(),
    }
}

/// The stored agents, each with its runtime state, then the agents
/// registered without being stored.
///
/// A registered agent shows what the registry holds; a stored agent that is
/// not registered shows what its definition says.
pub fn merge_agent_views(
    stored: &[StoredAgent],
    runtime: &[AgentEntry],
) -> Vec<InstalledAgentView> {
    let mut items: Vec<InstalledAgentView> = stored
        .iter()
        .map(|agent| {
            let entry = runtime.iter().find(|e| e.manifest.name == agent.name);
            match entry {
                Some(entry) => view(&entry.manifest, Some(entry), Some(agent)),
                None => view(&apollia_tools::manifest_of(agent), None, Some(agent)),
            }
        })
        .collect();
    items.extend(
        runtime
            .iter()
            .filter(|e| !stored.iter().any(|a| a.name == e.manifest.name))
            .map(|e| view(&e.manifest, Some(e), None)),
    );
    items
}

fn failure(status: StatusCode, error: impl ToString) -> (StatusCode, Json<ErrorResponse>) {
    (
        status,
        Json(ErrorResponse {
            error: error.to_string(),
        }),
    )
}

/// Runs `work` on the repository of `<data_dir>/agents.db`, off the async
/// threads.
async fn with_repository<T: Send + 'static>(
    data_dir: PathBuf,
    work: impl FnOnce(&AgentRepository) -> Result<T, String> + Send + 'static,
) -> Result<T, (StatusCode, Json<ErrorResponse>)> {
    tokio::task::spawn_blocking(move || {
        let repo = AgentRepository::open(&apollia_core::paths::DataFile::Agents.path(&data_dir))
            .map_err(|e| format!("cannot open agents.db: {e}"))?;
        work(&repo)
    })
    .await
    .map_err(|e| failure(StatusCode::INTERNAL_SERVER_ERROR, e))?
    .map_err(|e| failure(StatusCode::INTERNAL_SERVER_ERROR, e))
}

/// Handler for `GET /api/v1/agents/installed`.
#[utoipa::path(
    get,
    path = "/api/v1/agents/installed",
    tag = "agents",
    responses(
        (status = 200, description = "Stored agents with their runtime state, then agents registered without being stored", body = Vec<InstalledAgentView>),
        (status = 500, description = "agents.db or the registry cannot be read", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub async fn list_installed_agents<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
) -> Result<Json<Vec<InstalledAgentView>>, (StatusCode, Json<ErrorResponse>)> {
    let installed = if apollia_core::paths::DataFile::Agents
        .path(&state.data_dir)
        .exists()
    {
        with_repository(state.data_dir.clone(), |repo| {
            repo.list_stored()
                .map(|agents| {
                    agents
                        .into_iter()
                        .filter(|agent| !apollia_tools::is_built_in_agent(&agent.name))
                        .collect()
                })
                .map_err(|e| e.to_string())
        })
        .await?
    } else {
        Vec::new()
    };
    let runtime = state
        .registry_handle
        .list_agents()
        .await
        .map_err(|e| failure(StatusCode::INTERNAL_SERVER_ERROR, e))?;
    Ok(Json(merge_agent_views(&installed, &runtime)))
}

#[cfg(test)]
mod tests;
