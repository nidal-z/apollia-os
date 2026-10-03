//! `DELETE /api/v1/llm/installed-models?path=...`: removes a model installed
//! under the models directory of the runtime.
//!
//! Serves [`crate::installed_models::delete_installed`] to any client of the
//! daemon, the attached desktop first, which refuses what the desktop running
//! the runtime in its own process refuses, with the same words.

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::api::openapi::ApiErrorBody;
use crate::api::server::AppState;
use crate::coordinator::ExecutionBackend;
use crate::installed_models::{delete_installed, InstalledModelError};

/// Query for `DELETE /api/v1/llm/installed-models`.
#[derive(Debug, Deserialize)]
pub struct InstalledModelQuery {
    /// Absolute path of the installed model file.
    pub path: String,
}

/// Answer of `DELETE /api/v1/llm/installed-models`.
#[derive(Debug, Deserialize, Serialize, utoipa::ToSchema)]
pub struct DeletedInstalledModel {
    /// The path removed, as it was asked for.
    pub deleted: String,
}

fn refusal(error: InstalledModelError) -> (StatusCode, Json<ApiErrorBody>) {
    let status = match error {
        InstalledModelError::NotFound(_) | InstalledModelError::NoModelsDir(_) => {
            StatusCode::NOT_FOUND
        }
        InstalledModelError::OutsideModelsDir => StatusCode::FORBIDDEN,
        InstalledModelError::InUse { .. } => StatusCode::CONFLICT,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    };
    (
        status,
        Json(ApiErrorBody {
            error: error.to_string(),
        }),
    )
}

/// `DELETE /api/v1/llm/installed-models?path=...`
#[utoipa::path(
    delete,
    path = "/api/v1/llm/installed-models",
    tag = "model_hub",
    params(
        ("path" = String, Query, description = "Absolute path of the installed model file"),
    ),
    responses(
        (status = 200, description = "The file and its index entry are gone", body = DeletedInstalledModel),
        (status = 403, description = "The path resolves outside the models directory", body = ApiErrorBody),
        (status = 404, description = "No such file, or no models directory", body = ApiErrorBody),
        (status = 409, description = "An LLM backend or the dictation engine uses the file", body = ApiErrorBody),
        (status = 500, description = "The file could not be removed", body = ApiErrorBody),
    )
)]
pub async fn delete_installed_model<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Query(query): Query<InstalledModelQuery>,
) -> Result<Json<DeletedInstalledModel>, (StatusCode, Json<ApiErrorBody>)> {
    let data_dir = state.data_dir.clone();
    let path = query.path;
    let asked = path.clone();
    let outcome = tokio::task::spawn_blocking(move || delete_installed(&data_dir, &path))
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiErrorBody {
                    error: format!("the removal did not complete: {e}"),
                }),
            )
        })?;
    outcome.map_err(refusal)?;
    tracing::event!(
        tracing::Level::INFO,
        path = %asked,
        "model_hub.installed.deleted"
    );
    Ok(Json(DeletedInstalledModel { deleted: asked }))
}

#[cfg(test)]
mod tests;
