//! Uninstalling and updating an installed agent, `DELETE` and
//! `PUT /api/v1/agents/installed/{name}`.
//!
//! Both read `agents.db`, the agent directory and the registry of the runtime,
//! so they live here and serve every client: the desktop reaches them through
//! the listener of the runtime in its process, or through the daemon's when it
//! attached to one.

use std::path::{Path, PathBuf};

use axum::extract::{Path as AxumPath, State};
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};

use apollia_core::ProcessState;
use apollia_tools::{AgentRepository, InstalledAgent};

use super::{copy_python_tree, failure, AgentLoader, ErrorResponse, InstallError};
use crate::api::routes_agents::{start_agent, StartAgentRequest};
use crate::api::server::AppState;
use crate::coordinator::{DynBackend, ExecutionBackend};
use crate::registry::AgentRegistryHandle;
use crate::router::TaskRouterHandle;

/// What happened to the live runtime instance while an agent's module was
/// replaced.
///
/// The embedded interpreter keeps the imported module in memory, so writing a
/// new `.py` over the installed one changes nothing for an agent that is
/// already running. The replacement therefore stops and restarts it, and this
/// value is what lets the window say which of the two versions is now serving
/// tasks instead of announcing a success it cannot vouch for.
pub mod restart_outcome {
    /// No live runtime entry: the new module loads at the next start.
    pub const NOT_RUNNING: &str = "not_running";
    /// Stopped and started again on the new module.
    pub const RESTARTED: &str = "restarted";
    /// The stop was refused: the previous module is still running.
    pub const STOP_FAILED: &str = "stop_failed";
    /// Stopped, but it did not come back up: nothing is running.
    pub const START_FAILED: &str = "start_failed";
}

/// Request body for `PUT /api/v1/agents/installed/{name}`.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct UpdateAgentRequest {
    /// Path of the new Python module on this machine.
    pub path: String,
}

/// Response to an agent update.
///
/// Distinct from the install answer because an update has an outcome an
/// install cannot have: a runtime instance that had to be cycled.
#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct UpdateAgentResponse {
    /// Unique name of the updated agent.
    pub name: String,
    /// Semver version read from the new module's manifest.
    pub version: String,
    /// Install path on disk.
    pub install_path: String,
    /// One of the [`restart_outcome`] constants.
    pub restart_outcome: String,
    /// Raw cause when the stop or the start failed, `None` otherwise.
    pub restart_error: Option<String>,
}

/// Response to an agent uninstall.
#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct AgentUninstalled {
    /// Name of the agent removed.
    pub name: String,
}

/// Why an uninstall or an update did not complete. The messages are the ones
/// the window shows.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum LifecycleError {
    /// No row of `agents.db` carries this name.
    #[error("agent '{0}' not found in installed agents")]
    NotInstalled(String),
    /// `agents.db` refused the read or the write.
    #[error("{0}")]
    Database(String),
    /// The new module is absent, unresolvable or refused by the loader.
    #[error(transparent)]
    Module(#[from] InstallError),
    /// The new module cannot be copied over the installed one.
    #[error("cannot copy {from} to {to}: {source}")]
    Copy {
        /// The new module.
        from: String,
        /// The installed module.
        to: String,
        /// The cause.
        #[source]
        source: std::io::Error,
    },
}

impl LifecycleError {
    fn status(&self) -> StatusCode {
        match self {
            Self::NotInstalled(_) => StatusCode::NOT_FOUND,
            Self::Module(InstallError::Write { .. }) => StatusCode::INTERNAL_SERVER_ERROR,
            Self::Module(_) => StatusCode::BAD_REQUEST,
            Self::Database(_) | Self::Copy { .. } => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

/// Removes the row of `name` from `agents.db` and its install directory, the
/// latter best-effort. Returns the row removed.
///
/// # Errors
///
/// [`LifecycleError::NotInstalled`] when no row carries the name,
/// [`LifecycleError::Database`] when `agents.db` refuses.
pub fn remove_installed(
    repo: &AgentRepository,
    name: &str,
) -> Result<InstalledAgent, LifecycleError> {
    let agent = repo
        .get(name)
        .map_err(|e| LifecycleError::Database(format!("database error: {e}")))?
        .ok_or_else(|| LifecycleError::NotInstalled(name.to_owned()))?;
    repo.delete(name)
        .map_err(|e| LifecycleError::Database(format!("failed to delete from database: {e}")))?;
    if let Some(parent) = agent.install_path.parent() {
        let _ = std::fs::remove_dir_all(parent);
    }
    Ok(agent)
}

/// Validates the module at `path`, copies it over the installed module of
/// `name` with its siblings, and records the new version. Returns the row
/// recorded.
///
/// # Errors
///
/// [`LifecycleError`] when the module is absent or refused, the agent is not
/// installed, or the copy or the record fails.
pub fn replace_installed(
    loader: &dyn AgentLoader,
    repo: &AgentRepository,
    name: &str,
    path: &str,
) -> Result<InstalledAgent, LifecycleError> {
    let source_path = PathBuf::from(path);
    if !source_path.exists() {
        return Err(InstallError::NotFound(path.to_owned()).into());
    }
    let canonical = source_path
        .canonicalize()
        .map_err(|source| InstallError::Resolve {
            path: path.to_owned(),
            source,
        })?;
    let existing = repo
        .get(name)
        .map_err(|e| LifecycleError::Database(format!("database error: {e}")))?
        .ok_or_else(|| LifecycleError::NotInstalled(name.to_owned()))?;

    // The Python cause is the actionable part and must survive the boundary.
    let manifest = loader.load_and_validate(&canonical).map_err(|cause| {
        tracing::event!(
            tracing::Level::WARN,
            agent = %name,
            path = %canonical.display(),
            cause = %cause,
            "agent.update.rejected"
        );
        InstallError::Rejected(cause)
    })?;

    std::fs::copy(&canonical, &existing.install_path).map_err(|source| LifecycleError::Copy {
        from: canonical.display().to_string(),
        to: existing.install_path.display().to_string(),
        source,
    })?;
    if let (Some(source_dir), Some(install_dir)) =
        (canonical.parent(), existing.install_path.parent())
    {
        copy_python_tree(source_dir, install_dir, &canonical);
    }

    let updated = InstalledAgent {
        name: existing.name,
        version: manifest.version.clone(),
        install_path: existing.install_path,
        source_path: canonical,
        manifest,
        enabled: existing.enabled,
        installed_at: existing.installed_at,
        updated_at: chrono::Utc::now().to_rfc3339(),
    };
    repo.save(&updated)
        .map_err(|e| LifecycleError::Database(format!("failed to update database: {e}")))?;
    Ok(updated)
}

/// True while the agent still answers tasks.
///
/// `Stopping` counts as not running: the transition is already under way and
/// the registry refuses a second stop, so the caller must not try to cycle it.
fn is_running(state: &ProcessState) -> bool {
    matches!(
        state,
        ProcessState::Initializing | ProcessState::Active | ProcessState::Degraded
    )
}

/// Takes the running instance of an agent out of service before an update.
///
/// The order is load-bearing: `Stopping` first so the window sees the
/// transition, then the coordinator leaves the router so no new task is
/// dispatched, then `Stopped`. Unregistering before the state change would let
/// a task be routed to a coordinator that is already gone.
async fn stop_for_update<B: ExecutionBackend + Clone>(
    registry: &AgentRegistryHandle,
    router: &TaskRouterHandle<B>,
    agent_id: &str,
) -> Result<(), String> {
    let entry = registry
        .get_agent(agent_id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("agent not found: {agent_id}"))?;
    if entry.process_state == ProcessState::Stopped {
        return Err(format!("agent already stopped: {agent_id}"));
    }
    if entry.process_state == ProcessState::Stopping {
        return Err(format!("agent already stopping: {agent_id}"));
    }
    let canonical_id = entry.id;
    registry
        .update_state(canonical_id.as_str(), ProcessState::Stopping)
        .await
        .map_err(|e| e.to_string())?;
    let _ = router.unregister_coordinator(&canonical_id).await;
    registry
        .update_state(canonical_id.as_str(), ProcessState::Stopped)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Cycles the runtime instance of `name` so it picks up the module just
/// written.
///
/// Returns the [`restart_outcome`] constant and the raw cause of a failure. A
/// registry lookup that itself fails is reported as "not running" rather than
/// as a failed restart: nothing was stopped, so the previous instance, if any,
/// is untouched, and the operator is told the new module applies at the next
/// start.
async fn restart_after_update<B: ExecutionBackend + Clone + From<DynBackend>>(
    state: &AppState<B>,
    name: &str,
    install_path: &str,
) -> (&'static str, Option<String>) {
    let live_id = match state.registry_handle.find_by_name(name).await {
        Ok(Some(id)) => id,
        Ok(None) => return (restart_outcome::NOT_RUNNING, None),
        Err(e) => {
            tracing::event!(tracing::Level::WARN, agent = %name, cause = %e, "agent.update.registry_unreachable");
            return (restart_outcome::NOT_RUNNING, None);
        }
    };
    let running = matches!(
        state.registry_handle.get_agent(live_id.as_str()).await,
        Ok(Some(ref entry)) if is_running(&entry.process_state)
    );
    if !running {
        return (restart_outcome::NOT_RUNNING, None);
    }
    if let Err(e) = stop_for_update(
        &state.registry_handle,
        &state.router_handle,
        live_id.as_str(),
    )
    .await
    {
        tracing::event!(tracing::Level::WARN, agent = %name, cause = %e, "agent.update.stop_failed");
        return (restart_outcome::STOP_FAILED, Some(e));
    }
    // `register` evicts the entry that carries the same manifest name, so the
    // start replaces the stopped one instead of leaving two rows behind.
    let request = StartAgentRequest {
        agent_path: install_path.to_owned(),
    };
    match start_agent(State(state.clone()), Json(request)).await {
        Ok((_, Json(started))) => {
            tracing::event!(tracing::Level::INFO, agent = %name, agent_id = %started.agent_id, "agent.update.restarted");
            (restart_outcome::RESTARTED, None)
        }
        Err((_, Json(refusal))) => {
            tracing::event!(tracing::Level::WARN, agent = %name, cause = %refusal.error, "agent.update.start_failed");
            (restart_outcome::START_FAILED, Some(refusal.error))
        }
    }
}

/// Runs `work` on the repository of `<data_dir>/agents.db`, off the async
/// threads, and turns a refusal into its HTTP answer.
async fn on_repository<T: Send + 'static>(
    data_dir: &Path,
    work: impl FnOnce(&AgentRepository) -> Result<T, LifecycleError> + Send + 'static,
) -> Result<T, (StatusCode, Json<ErrorResponse>)> {
    let db = apollia_core::paths::DataFile::Agents.path(data_dir);
    let outcome = tokio::task::spawn_blocking(move || {
        let repo = AgentRepository::open(&db)
            .map_err(|e| LifecycleError::Database(format!("cannot open agents.db: {e}")))?;
        work(&repo)
    })
    .await
    .map_err(|e| failure(StatusCode::INTERNAL_SERVER_ERROR, e))?;
    outcome.map_err(|e| failure(e.status(), e))
}

/// Handler for `DELETE /api/v1/agents/installed/{name}`.
///
/// Removes the row and the install directory, then the agent from the
/// registry, its coordinator dropped for good: a stopped agent would otherwise
/// be loaded again by the next task that names it. Emits
/// `RuntimeEvent::AgentUninstalled`.
#[utoipa::path(
    delete,
    path = "/api/v1/agents/installed/{name}",
    tag = "agents",
    params(("name" = String, Path, description = "Name of the installed agent")),
    responses(
        (status = 200, description = "Agent uninstalled", body = AgentUninstalled),
        (status = 404, description = "No installed agent of that name", body = crate::api::openapi::ApiErrorBody),
        (status = 500, description = "agents.db cannot be written", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub async fn uninstall_agent<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    AxumPath(name): AxumPath<String>,
) -> Result<Json<AgentUninstalled>, (StatusCode, Json<ErrorResponse>)> {
    let target = name.clone();
    on_repository(&state.data_dir, move |repo| remove_installed(repo, &target)).await?;
    if let Ok(Some(agent_id)) = state.registry_handle.find_by_name(&name).await {
        let _ = state.router_handle.forget_coordinator(&agent_id).await;
        let _ = state.registry_handle.unregister(agent_id.as_str()).await;
    }
    let _ = state
        .event_sender
        .send(apollia_core::events::RuntimeEvent::AgentUninstalled { name: name.clone() });
    Ok(Json(AgentUninstalled { name }))
}

/// Handler for `PUT /api/v1/agents/installed/{name}`.
///
/// Validates the new module, replaces the installed one, records the new
/// version, and cycles the runtime instance when there is one. The cycle is the
/// point of the route: the interpreter holds the module it imported at start
/// time, so without the stop and the start the window would report a new
/// version while the old one keeps answering. Emits
/// `RuntimeEvent::AgentInstalled` once the runtime has settled.
#[utoipa::path(
    put,
    path = "/api/v1/agents/installed/{name}",
    tag = "agents",
    params(("name" = String, Path, description = "Name of the installed agent")),
    request_body = UpdateAgentRequest,
    responses(
        (status = 200, description = "Agent updated, with what happened to its running instance", body = UpdateAgentResponse),
        (status = 400, description = "File absent or module refused", body = crate::api::openapi::ApiErrorBody),
        (status = 404, description = "No installed agent of that name", body = crate::api::openapi::ApiErrorBody),
        (status = 500, description = "The update cannot be written", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub async fn update_agent<B: ExecutionBackend + Clone + From<DynBackend>>(
    State(state): State<AppState<B>>,
    AxumPath(name): AxumPath<String>,
    Json(request): Json<UpdateAgentRequest>,
) -> Result<Json<UpdateAgentResponse>, (StatusCode, Json<ErrorResponse>)> {
    let loader = state.agent_loader.clone();
    let target = name.clone();
    let updated = on_repository(&state.data_dir, move |repo| {
        replace_installed(loader.as_ref(), repo, &target, &request.path)
    })
    .await?;

    let install_path = updated.install_path.to_string_lossy().to_string();
    let (outcome, restart_error) = restart_after_update(&state, &name, &install_path).await;
    let _ = state
        .event_sender
        .send(apollia_core::events::RuntimeEvent::AgentInstalled {
            name: updated.name.clone(),
            version: updated.version.clone(),
        });
    Ok(Json(UpdateAgentResponse {
        name: updated.name,
        version: updated.version,
        install_path,
        restart_outcome: outcome.to_owned(),
        restart_error,
    }))
}

#[cfg(test)]
mod tests;
