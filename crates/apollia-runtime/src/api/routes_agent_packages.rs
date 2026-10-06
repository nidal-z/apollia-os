//! Agent packages installed from a folder, over HTTP.
//!
//! - `GET    /api/v1/agent-packages/installed`         , the installed packages
//! - `POST   /api/v1/agent-packages/installed`         , install a package folder
//! - `GET    /api/v1/agent-packages/installed/{name}`  , one package in full
//! - `DELETE /api/v1/agent-packages/installed/{name}`  , uninstall it
//! - `POST   /api/v1/agent-packages/preview`           , what a folder contains; nothing is written
//!
//! The routes serve the functions of [`crate::agent_packages`] to any client
//! of the runtime, the desktop first, attached to the daemon or not. The
//! folder is named by its path on this machine: the runtime and its callers
//! share the file system, as for the `.apo` archives of `/api/v1/packages/`.

use std::path::PathBuf;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use crate::agent_packages::check_name;
use crate::agent_packages::{
    self, AgentPackageDetail, AgentPackageError, AgentPackageInstallReport, AgentPackageListItem,
    AgentPackagePreview, AgentPackageTriggerConfig, InstallRequest,
};
use crate::api::openapi::ApiErrorBody;
use crate::api::routes_agents::{start_agent, StartAgentRequest};
use crate::api::routes_packages::uninstall::UninstallRequest;
use crate::api::server::AppState;
use crate::coordinator::{DynBackend, ExecutionBackend};

type ApiError = (StatusCode, Json<ApiErrorBody>);

/// Body of `POST /api/v1/agent-packages/preview`.
#[derive(Debug, Deserialize, Serialize, utoipa::ToSchema)]
pub struct AgentPackagePreviewBody {
    /// The package folder on this machine, the one holding `agent.toml`.
    pub path: String,
}

/// Body of `POST /api/v1/agent-packages/installed`.
#[derive(Debug, Deserialize, Serialize, utoipa::ToSchema)]
pub struct AgentPackageInstallBody {
    /// The package folder on this machine, the one holding `agent.toml`.
    pub path: String,
    /// What the user gave each trigger, a webhook secret for now.
    #[serde(default)]
    pub trigger_configs: Vec<AgentPackageTriggerConfig>,
    /// The user accepted the Python dependencies the package installs.
    #[serde(default)]
    pub deps_confirmed: bool,
}

/// Answer of `DELETE /api/v1/agent-packages/installed/{name}`.
#[derive(Debug, Deserialize, Serialize, utoipa::ToSchema)]
pub struct AgentPackageUninstalled {
    /// The package removed.
    pub uninstalled: String,
}

/// The agent package routes.
pub fn agent_package_routes<B: ExecutionBackend + Clone + From<DynBackend>>() -> Router<AppState<B>>
{
    Router::new()
        .route(
            "/api/v1/agent-packages/installed",
            get(list_agent_packages::<B>).post(install_agent_package::<B>),
        )
        .route(
            "/api/v1/agent-packages/installed/:name",
            get(get_agent_package::<B>).delete(uninstall_agent_package::<B>),
        )
        .route(
            "/api/v1/agent-packages/preview",
            post(preview_agent_package),
        )
}

fn refusal(error: AgentPackageError) -> ApiError {
    let status = match &error {
        AgentPackageError::NotFound(_) => StatusCode::NOT_FOUND,
        AgentPackageError::InvalidName(_) | AgentPackageError::Manifest(_) => {
            StatusCode::BAD_REQUEST
        }
        AgentPackageError::DepsConfirmationRequired { .. } => StatusCode::PRECONDITION_REQUIRED,
        AgentPackageError::Install(_) => StatusCode::UNPROCESSABLE_ENTITY,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    };
    (
        status,
        Json(ApiErrorBody {
            error: error.to_string(),
        }),
    )
}

fn joined(error: tokio::task::JoinError) -> ApiError {
    refusal(AgentPackageError::Install(format!("spawn error: {error}")))
}

/// `GET /api/v1/agent-packages/installed`: the installed packages, each with
/// its publisher, the state of its signature and how much it holds.
#[utoipa::path(
    get,
    path = "/api/v1/agent-packages/installed",
    tag = "packages",
    responses(
        (status = 200, description = "The installed packages", body = [AgentPackageListItem]),
        (status = 500, description = "agents.db cannot be read", body = ApiErrorBody),
    )
)]
pub(crate) async fn list_agent_packages<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
) -> Result<Json<Vec<AgentPackageListItem>>, ApiError> {
    let data_dir = state.data_dir.clone();
    let listed = tokio::task::spawn_blocking(move || agent_packages::list(&data_dir))
        .await
        .map_err(joined)?;
    listed.map(Json).map_err(refusal)
}

/// `GET /api/v1/agent-packages/installed/{name}`: one installed package, its
/// manifest as recorded, its publisher, what it installed by kind, and the
/// automations that would go with it.
#[utoipa::path(
    get,
    path = "/api/v1/agent-packages/installed/{name}",
    tag = "packages",
    params(("name" = String, Path, description = "Package name")),
    responses(
        (status = 200, description = "The package", body = AgentPackageDetail),
        (status = 400, description = "A name that is a path", body = ApiErrorBody),
        (status = 404, description = "No such package", body = ApiErrorBody),
        (status = 500, description = "agents.db cannot be read", body = ApiErrorBody),
    )
)]
pub(crate) async fn get_agent_package<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path(name): Path<String>,
) -> Result<Json<AgentPackageDetail>, ApiError> {
    let data_dir = state.data_dir.clone();
    let triggers = state.trigger_def_repo.clone();
    let detail = tokio::task::spawn_blocking(move || {
        let mut detail = agent_packages::detail(&data_dir, &name)?;
        // The trigger store may be closed or unreadable: the package is
        // still described, with no automation listed.
        let rows = triggers
            .as_deref()
            .and_then(|repo| {
                repo.lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .list()
                    .ok()
            })
            .unwrap_or_default();
        detail.automations = agent_packages::automations_of(
            &detail,
            rows.into_iter()
                .map(|row| (row.id, row.agent, row.workflow)),
        );
        Ok(detail)
    })
    .await
    .map_err(joined)?;
    detail.map(Json).map_err(refusal)
}

/// `POST /api/v1/agent-packages/preview`: what the package folder contains,
/// its agents, its triggers and its Python dependencies. Nothing is written;
/// a manifest that breaks an invariant answers a preview marked invalid.
#[utoipa::path(
    post,
    path = "/api/v1/agent-packages/preview",
    tag = "packages",
    request_body = AgentPackagePreviewBody,
    responses(
        (status = 200, description = "The preview", body = AgentPackagePreview),
        (status = 400, description = "agent.toml cannot be read or parsed", body = ApiErrorBody),
    )
)]
pub(crate) async fn preview_agent_package(
    Json(body): Json<AgentPackagePreviewBody>,
) -> Result<Json<AgentPackagePreview>, ApiError> {
    let root = PathBuf::from(body.path);
    let preview = tokio::task::spawn_blocking(move || agent_packages::preview(&root))
        .await
        .map_err(joined)?;
    preview.map(Json).map_err(refusal)
}

/// `POST /api/v1/agent-packages/installed`: install a package folder.
///
/// Python dependencies need `deps_confirmed`; without it the answer is a
/// `428` whose message is `DEPS_CONFIRMATION_REQUIRED:<count>:<list>`. The
/// agents installed take tasks at once, and the triggers written are loaded.
#[utoipa::path(
    post,
    path = "/api/v1/agent-packages/installed",
    tag = "packages",
    request_body = AgentPackageInstallBody,
    responses(
        (status = 201, description = "Installed", body = AgentPackageInstallReport),
        (status = 400, description = "Invalid manifest, or a name that is a path", body = ApiErrorBody),
        (status = 422, description = "The copy, a virtualenv or an agent import failed; nothing is left", body = ApiErrorBody),
        (status = 428, description = "The Python dependencies are not confirmed", body = ApiErrorBody),
        (status = 500, description = "agents.db refused a row", body = ApiErrorBody),
    )
)]
pub(crate) async fn install_agent_package<B: ExecutionBackend + Clone + From<DynBackend>>(
    State(state): State<AppState<B>>,
    Json(body): Json<AgentPackageInstallBody>,
) -> Result<(StatusCode, Json<AgentPackageInstallReport>), ApiError> {
    let request = InstallRequest {
        path: PathBuf::from(body.path),
        trigger_configs: body.trigger_configs,
        deps_confirmed: body.deps_confirmed,
    };
    let installed = agent_packages::install(state.agent_loader.clone(), &state.data_dir, request)
        .await
        .map_err(refusal)?;

    // The agents take tasks at once: nobody starts them. A failure leaves the
    // install in place, and the agent becomes available at the next start.
    for (name, entry) in &installed.agents {
        let request = StartAgentRequest {
            agent_path: entry.to_string_lossy().to_string(),
        };
        if let Err((_, body)) = start_agent(State(state.clone()), Json(request)).await {
            tracing::event!(
                tracing::Level::WARN,
                agent = %name,
                cause = %body.0.error,
                "agent.install.register_failed"
            );
        }
    }
    if installed.report.trigger_count > 0 {
        let _ = crate::api::routes_triggers::reload_triggers(State(state.clone())).await;
    }
    Ok((StatusCode::CREATED, Json(installed.report)))
}

/// `DELETE /api/v1/agent-packages/installed/{name}`: uninstall a package
/// on the path the command line and `POST /api/v1/packages/{name}/uninstall`
/// take: its agents with their overrides, model and triggers, its
/// environments and its folder, recorded in the audit journal. The memory
/// and the history of its agents are kept.
#[utoipa::path(
    delete,
    path = "/api/v1/agent-packages/installed/{name}",
    tag = "packages",
    params(("name" = String, Path, description = "Package name")),
    responses(
        (status = 200, description = "Uninstalled", body = AgentPackageUninstalled),
        (status = 400, description = "A name that is a path", body = ApiErrorBody),
        (status = 403, description = "The package is part of Apollia", body = ApiErrorBody),
        (status = 404, description = "No such package", body = ApiErrorBody),
        (status = 500, description = "Store or journal failure", body = ApiErrorBody),
        (status = 503, description = "No audit journal", body = ApiErrorBody),
    )
)]
pub(crate) async fn uninstall_agent_package<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path(name): Path<String>,
) -> Result<Json<AgentPackageUninstalled>, ApiError> {
    check_name(&name).map_err(refusal)?;
    let _report = crate::api::routes_packages::uninstall::package(
        &state,
        &name,
        &UninstallRequest::default(),
    )
    .await
    .map_err(|(status, Json(body))| (status, Json(ApiErrorBody { error: body.error })))?;
    Ok(Json(AgentPackageUninstalled { uninstalled: name }))
}

#[cfg(test)]
mod tests;
