//! The installed agents of the machine, `GET` and `POST /api/v1/agents/installed`.
//!
//! `GET /api/v1/agents` lists what the registry holds; an agent installed but
//! not started is absent from it. This view is the one an operator reads: the
//! rows of `agents.db`, each enriched with its runtime state when it runs,
//! followed by the agents started without being installed. The desktop builds
//! the same view from its own handles when the runtime runs in its process, and
//! reads it here when it attached to a daemon, so both go through
//! [`merge_agent_views`].
//!
//! Installing validates the module through the [`AgentLoader`] of the runtime,
//! copies it under `<data_dir>/agents/<name>/`, and records it in `agents.db`.
//! Updating lives in [`lifecycle`].

use std::path::{Path, PathBuf};

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};

use apollia_core::AgentManifest;
use apollia_tools::{AgentRepository, InstalledAgent};

use super::{state_to_string, AgentLoader, ErrorResponse};
use crate::api::server::AppState;
use crate::coordinator::ExecutionBackend;
use crate::registry::AgentEntry;

/// Updating an installed agent.
pub mod lifecycle;

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

/// One agent of the machine: installed, running, or both.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct InstalledAgentView {
    /// Runtime identifier, `None` when installed but not started.
    pub id: Option<String>,
    /// Unique agent name.
    pub name: String,
    /// Semver version.
    pub version: String,
    /// Whether the agent loads at boot.
    pub enabled: bool,
    /// Runtime state (`"active"`, `"degraded"`, `"stopped"`...), `None` when
    /// not started.
    pub runtime_status: Option<String>,
    /// Install timestamp (RFC 3339), `None` for an agent started without
    /// being installed.
    pub installed_at: Option<String>,
    /// Description from the manifest.
    pub description: Option<String>,
    /// Free-form tags for routing and discovery.
    pub tags: Vec<String>,
    /// Tools the agent requires.
    pub tools_required: Vec<String>,
    /// Tools the agent uses when present.
    pub tools_optional: Vec<String>,
    /// Execution mode (`"auto"`, `"direct"`, `"orchestrated"`).
    pub execution_mode: Option<String>,
    /// Install path on disk, `None` for an agent started without being
    /// installed.
    pub install_path: Option<String>,
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

/// Request body for `POST /api/v1/agents/installed`.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct InstallAgentRequest {
    /// Path of the Python module on this machine.
    pub path: String,
}

/// Response body for an install.
#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct InstallAgentResponse {
    /// Unique name of the installed agent.
    pub name: String,
    /// Semver version of the agent.
    pub version: String,
    /// Install path on disk.
    pub install_path: String,
}

/// Why an install did not complete. The messages are the ones the window
/// shows, the loader's own cause included.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum InstallError {
    /// No file at the given path.
    #[error("file not found: {0}")]
    NotFound(String),
    /// The path cannot be resolved.
    #[error("cannot resolve path {path}: {source}")]
    Resolve {
        /// The path given.
        path: String,
        /// The cause.
        #[source]
        source: std::io::Error,
    },
    /// The loader refused the module; the text is its own cause.
    #[error("{0}")]
    Rejected(String),
    /// The install directory cannot be written.
    #[error("cannot write {path}: {source}")]
    Write {
        /// The path written.
        path: String,
        /// The cause.
        #[source]
        source: std::io::Error,
    },
}

fn view(
    manifest: &AgentManifest,
    entry: Option<&AgentEntry>,
    installed: Option<&InstalledAgent>,
) -> InstalledAgentView {
    InstalledAgentView {
        id: entry.map(|e| e.id.to_string()),
        name: installed.map_or_else(|| manifest.name.clone(), |a| a.name.clone()),
        version: installed.map_or_else(|| manifest.version.clone(), |a| a.version.clone()),
        enabled: installed.is_none_or(|a| a.enabled),
        runtime_status: entry.map(|e| state_to_string(&e.process_state)),
        installed_at: installed.map(|a| a.installed_at.clone()),
        description: (!manifest.description.is_empty()).then(|| manifest.description.clone()),
        tags: manifest.tags.clone(),
        tools_required: manifest.tools_required.clone(),
        tools_optional: manifest.tools_optional.clone(),
        execution_mode: Some(manifest.execution_mode.clone()),
        install_path: installed.map(|a| a.install_path.to_string_lossy().to_string()),
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

/// The installed agents, each with its runtime state, then the agents started
/// without being installed.
///
/// A loaded agent shows the manifest the runtime parsed, not the one stored at
/// install time: a package install stores a stub manifest, the runtime holds
/// the real one.
pub fn merge_agent_views(
    installed: &[InstalledAgent],
    runtime: &[AgentEntry],
) -> Vec<InstalledAgentView> {
    let mut items: Vec<InstalledAgentView> = installed
        .iter()
        .map(|agent| {
            let entry = runtime.iter().find(|e| e.manifest.name == agent.name);
            let manifest = entry.map_or(&agent.manifest, |e| &e.manifest);
            view(manifest, entry, Some(agent))
        })
        .collect();
    items.extend(
        runtime
            .iter()
            .filter(|e| !installed.iter().any(|a| a.name == e.manifest.name))
            .map(|e| view(&e.manifest, Some(e), None)),
    );
    items
}

/// Copies the `.py` files and Python sub-packages of `src_dir` into `dst_dir`,
/// skipping `exclude` (the entry file, already copied as `agent.py`).
pub fn copy_python_tree(src_dir: &Path, dst_dir: &Path, exclude: &Path) {
    let Ok(entries) = std::fs::read_dir(src_dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path == exclude {
            continue;
        }
        if path.is_file() && path.extension().and_then(|e| e.to_str()) == Some("py") {
            if let Some(name) = path.file_name() {
                let _ = std::fs::copy(&path, dst_dir.join(name));
            }
        } else if path.is_dir() && path.join("__init__.py").exists() {
            if let Some(dir_name) = path.file_name() {
                let sub_dst = dst_dir.join(dir_name);
                let _ = std::fs::create_dir_all(&sub_dst);
                copy_python_tree(&path, &sub_dst, exclude);
            }
        }
    }
}

/// Validates the module at `path` and copies it under
/// `<data_dir>/agents/<name>/agent.py`, its siblings beside it. The caller
/// records the returned agent and announces it.
///
/// # Errors
///
/// [`InstallError`] when the file is absent, refused by the loader, or cannot
/// be copied.
pub fn place_agent(
    loader: &dyn AgentLoader,
    data_dir: &Path,
    path: &str,
) -> Result<InstalledAgent, InstallError> {
    let source_path = PathBuf::from(path);
    if !source_path.exists() {
        return Err(InstallError::NotFound(path.to_owned()));
    }
    let canonical = source_path
        .canonicalize()
        .map_err(|source| InstallError::Resolve {
            path: path.to_owned(),
            source,
        })?;
    // The Python cause is the only thing that tells the operator what to
    // change; it is logged here and carried to the window verbatim.
    let manifest = loader.load_and_validate(&canonical).map_err(|cause| {
        tracing::event!(
            tracing::Level::WARN,
            path = %canonical.display(),
            cause = %cause,
            "agent.install.rejected"
        );
        InstallError::Rejected(cause)
    })?;

    let agents_dir = data_dir.join("agents").join(&manifest.name);
    let write_error = |path: &Path| {
        let path = path.display().to_string();
        move |source| InstallError::Write { path, source }
    };
    std::fs::create_dir_all(&agents_dir).map_err(write_error(&agents_dir))?;
    let install_path = agents_dir.join("agent.py");
    std::fs::copy(&canonical, &install_path).map_err(write_error(&install_path))?;
    if let Some(source_dir) = canonical.parent() {
        copy_python_tree(source_dir, &agents_dir, &canonical);
    }

    let now = chrono::Utc::now().to_rfc3339();
    Ok(InstalledAgent {
        name: manifest.name.clone(),
        version: manifest.version.clone(),
        install_path,
        source_path: canonical,
        manifest,
        enabled: true,
        installed_at: now.clone(),
        updated_at: now,
    })
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
        (status = 200, description = "Installed agents with their runtime state, then agents started without being installed", body = Vec<InstalledAgentView>),
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
            repo.list().map_err(|e| e.to_string())
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

/// Handler for `POST /api/v1/agents/installed`.
///
/// Emits `RuntimeEvent::AgentInstalled` once the agent is recorded.
#[utoipa::path(
    post,
    path = "/api/v1/agents/installed",
    tag = "agents",
    request_body = InstallAgentRequest,
    responses(
        (status = 201, description = "Agent installed", body = InstallAgentResponse),
        (status = 400, description = "File absent or module refused", body = crate::api::openapi::ApiErrorBody),
        (status = 500, description = "The install cannot be written", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub async fn install_agent<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Json(request): Json<InstallAgentRequest>,
) -> Result<(StatusCode, Json<InstallAgentResponse>), (StatusCode, Json<ErrorResponse>)> {
    let agent =
        place_agent(state.agent_loader.as_ref(), &state.data_dir, &request.path).map_err(|e| {
            match e {
                InstallError::Write { .. } => failure(StatusCode::INTERNAL_SERVER_ERROR, e),
                other => failure(StatusCode::BAD_REQUEST, other),
            }
        })?;
    let record = agent.clone();
    with_repository(state.data_dir.clone(), move |repo| {
        repo.save(&record)
            .map_err(|e| format!("failed to save to database: {e}"))
    })
    .await?;
    let _ = state
        .event_sender
        .send(apollia_core::events::RuntimeEvent::AgentInstalled {
            name: agent.name.clone(),
            version: agent.version.clone(),
        });
    Ok((
        StatusCode::CREATED,
        Json(InstallAgentResponse {
            name: agent.name,
            version: agent.version,
            install_path: agent.install_path.to_string_lossy().to_string(),
        }),
    ))
}

#[cfg(test)]
mod tests;
