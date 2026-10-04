//! Uninstalling a package, or an agent installed on its own.
//!
//! - `POST /api/v1/packages/{name}/uninstall`         , a package and everything it installed
//! - `POST /api/v1/agents/installed/{name}/uninstall` , an agent installed on its own
//!
//! The body says whether the memory and the history of the agents are erased
//! or kept. Both routes answer `503` without an audit journal: an
//! uninstallation is never left unrecorded. Beyond the stores, the agents
//! leave the registry, the trigger engine drops their triggers, and the bus
//! announces each removed agent.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;

use apollia_core::events::RuntimeEvent;
use apollia_tools::{AgentRepository, PackageRepository};

use super::{agents_db, api_err, internal, ApiError, PackageErrorBody, StateMcp, API_ACTOR};
use crate::api::server::AppState;
use crate::coordinator::ExecutionBackend;
use crate::governance::NoPolicyGovernance;
use crate::packages::{self, UninstallError, UninstallOptions, UninstallReport, UninstallTargets};

/// Body of both uninstall routes: what the user chose to erase.
#[derive(Debug, Default, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UninstallRequest {
    /// Erase the memory of the agents instead of keeping it.
    #[serde(default)]
    pub clear_memory: bool,
    /// Erase the history (tasks, batches, workflow runs) instead of keeping
    /// it.
    #[serde(default)]
    pub clear_history: bool,
}

impl From<&UninstallRequest> for UninstallOptions {
    fn from(request: &UninstallRequest) -> Self {
        Self {
            clear_memory: request.clear_memory,
            clear_history: request.clear_history,
        }
    }
}

/// What is uninstalled.
enum Subject<'a> {
    Package(&'a str),
    Agent(&'a str),
}

/// `POST /api/v1/packages/{name}/uninstall`: uninstall a package.
///
/// Always removed: its agents with their overrides, model and triggers, its
/// workflows and tools, its environments and files, and the MCP servers it
/// brought. The connections it required stay. The memory and the history of
/// its agents and workflows are erased only when the body asks.
#[utoipa::path(
    post,
    path = "/api/v1/packages/{name}/uninstall",
    tag = "packages",
    params(("name" = String, Path, description = "Package name")),
    request_body = UninstallRequest,
    responses(
        (status = 200, description = "Uninstalled", body = UninstallReport),
        (status = 403, description = "The package carries Apollia", body = PackageErrorBody),
        (status = 404, description = "No such package", body = PackageErrorBody),
        (status = 500, description = "Store or journal failure", body = PackageErrorBody),
        (status = 503, description = "No audit journal", body = PackageErrorBody),
    )
)]
pub(crate) async fn uninstall_package<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path(name): Path<String>,
    Json(request): Json<UninstallRequest>,
) -> Result<Json<UninstallReport>, ApiError> {
    run(&state, Subject::Package(&name), &request).await
}

/// `POST /api/v1/agents/installed/{name}/uninstall`: uninstall an agent
/// installed on its own.
///
/// Removes the agent, its overrides, its model, its triggers, its
/// environment and its files. Its memory and history are erased only when
/// the body asks. Apollia cannot be uninstalled; an agent of an installed
/// package goes with its package.
#[utoipa::path(
    post,
    path = "/api/v1/agents/installed/{name}/uninstall",
    tag = "packages",
    params(("name" = String, Path, description = "Agent name")),
    request_body = UninstallRequest,
    responses(
        (status = 200, description = "Uninstalled", body = UninstallReport),
        (status = 403, description = "Apollia cannot be uninstalled", body = PackageErrorBody),
        (status = 404, description = "No such agent", body = PackageErrorBody),
        (status = 409, description = "The agent belongs to an installed package", body = PackageErrorBody),
        (status = 500, description = "Store or journal failure", body = PackageErrorBody),
        (status = 503, description = "No audit journal", body = PackageErrorBody),
    )
)]
pub(crate) async fn uninstall_agent<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path(name): Path<String>,
    Json(request): Json<UninstallRequest>,
) -> Result<Json<UninstallReport>, ApiError> {
    run(&state, Subject::Agent(&name), &request).await
}

/// Uninstalls the package `name` on the one path every caller takes, as
/// `request` chooses. The route of the packages installed from a folder
/// reaches it too.
pub(crate) async fn package<B: ExecutionBackend + Clone>(
    state: &AppState<B>,
    name: &str,
    request: &UninstallRequest,
) -> Result<Json<UninstallReport>, ApiError> {
    run(state, Subject::Package(name), request).await
}

async fn run<B: ExecutionBackend + Clone>(
    state: &AppState<B>,
    subject: Subject<'_>,
    request: &UninstallRequest,
) -> Result<Json<UninstallReport>, ApiError> {
    let journal = state.audit_journal.clone().ok_or_else(|| {
        api_err(
            StatusCode::SERVICE_UNAVAILABLE,
            "journal_unavailable",
            "audit journal not available: nothing is uninstalled without its record",
        )
    })?;
    let db = agents_db(state);
    let packages = PackageRepository::open(&db).map_err(internal)?;
    let agents = AgentRepository::open(&db).map_err(internal)?;
    let governance = NoPolicyGovernance::new(journal);
    let mcp = StateMcp { state };
    let targets = UninstallTargets {
        packages: &packages,
        agents: &agents,
        governance: &governance,
        mcp: state.mcp_handle.as_ref().map(|_| &mcp),
        triggers: state.trigger_def_repo.as_deref(),
        models: state.llm_backend_repo.as_deref(),
        data_dir: &state.data_dir,
    };
    let options = UninstallOptions::from(request);
    let report = match subject {
        Subject::Package(name) => {
            packages::uninstall_package(name, options, &targets, API_ACTOR).await
        }
        Subject::Agent(name) => packages::uninstall_agent(name, options, &targets, API_ACTOR).await,
    }
    .map_err(uninstall_error)?;

    for agent in &report.agents {
        if let Ok(Some(agent_id)) = state.registry_handle.find_by_name(agent).await {
            let _ = state.router_handle.forget_coordinator(&agent_id).await;
            let _ = state.registry_handle.unregister(agent_id.as_str()).await;
        }
        let _ = state.event_sender.send(RuntimeEvent::AgentUninstalled {
            name: agent.clone(),
        });
    }
    if !report.triggers_removed.is_empty() {
        crate::api::routes_triggers::reload_engine_from_repo(state).await;
    }
    Ok(Json(report))
}

fn uninstall_error(error: UninstallError) -> ApiError {
    let message = error.to_string();
    match error {
        UninstallError::NotFound(_) => api_err(StatusCode::NOT_FOUND, "not_found", message),
        UninstallError::BuiltIn(_) => api_err(StatusCode::FORBIDDEN, "built_in", message),
        UninstallError::OwnedByPackage { .. } => {
            api_err(StatusCode::CONFLICT, "owned_by_package", message)
        }
        _ => internal(message),
    }
}

#[cfg(test)]
mod tests;
