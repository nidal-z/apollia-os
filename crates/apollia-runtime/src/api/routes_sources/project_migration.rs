//! `GET /api/v1/sources/project-migration`: the report of the migration of
//! the former projects into sources, as the last run at boot wrote it.

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;

use crate::api::routes_llm::servers::{api_err, ApiError};
use crate::api::server::AppState;
use crate::coordinator::ExecutionBackend;
use crate::project_migration::{read_report, MigrationReport};

/// `GET /api/v1/sources/project-migration`: what the migration of the former
/// projects carried over, and what it listed instead.
#[utoipa::path(
    get,
    path = "/api/v1/sources/project-migration",
    tag = "sources",
    responses(
        (status = 200, description = "The report of the last run", body = MigrationReport),
        (status = 404, description = "No migration of projects has run", body = crate::api::openapi::ApiErrorBody),
        (status = 500, description = "The report cannot be read", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub async fn project_migration_report<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
) -> Result<Json<MigrationReport>, ApiError> {
    let data_dir = state.data_dir.clone();
    let read = tokio::task::spawn_blocking(move || read_report(&data_dir))
        .await
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, e))?;
    match read {
        Ok(Some(report)) => Ok(Json(report)),
        Ok(None) => Err(api_err(
            StatusCode::NOT_FOUND,
            "no migration of projects has run",
        )),
        Err(e) => Err(api_err(StatusCode::INTERNAL_SERVER_ERROR, e)),
    }
}
