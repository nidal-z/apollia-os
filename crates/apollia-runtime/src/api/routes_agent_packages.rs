//! The installed packages of the machine, over HTTP.
//!
//! - `GET    /api/v1/agent-packages/installed`         , the installed packages
//! - `GET    /api/v1/agent-packages/installed/{name}`  , one package in full
//! - `DELETE /api/v1/agent-packages/installed/{name}`  , uninstall it
//!
//! The routes serve the functions of [`crate::agent_packages`] to any client
//! of the runtime, the desktop first, attached to the daemon or not. A
//! package is installed from a `.apo` archive, under `/api/v1/packages/`.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use crate::agent_packages::check_name;
use crate::agent_packages::{self, AgentPackageDetail, AgentPackageError, AgentPackageListItem};
use crate::api::openapi::ApiErrorBody;
use crate::api::routes_packages::uninstall::UninstallRequest;
use crate::api::server::AppState;
use crate::coordinator::{DynBackend, ExecutionBackend};

type ApiError = (StatusCode, Json<ApiErrorBody>);

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
            get(list_agent_packages::<B>),
        )
        .route(
            "/api/v1/agent-packages/installed/:name",
            get(get_agent_package::<B>).delete(uninstall_agent_package::<B>),
        )
}

fn refusal(error: AgentPackageError) -> ApiError {
    let status = match &error {
        AgentPackageError::NotFound(_) => StatusCode::NOT_FOUND,
        AgentPackageError::InvalidName(_) => StatusCode::BAD_REQUEST,
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
    refusal(AgentPackageError::Store(format!("spawn error: {error}")))
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
