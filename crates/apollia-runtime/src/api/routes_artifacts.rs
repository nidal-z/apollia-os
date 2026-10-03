//! The artifacts of a conversation over HTTP:
//! `GET /api/v1/sessions/{id}/artifacts`, `POST /api/v1/artifacts`, and
//! `PATCH` and `DELETE /api/v1/artifacts/{id}`.
//!
//! The artifacts had no route: only the desktop running the runtime in its
//! own process could read or change them. These routes serve the functions of
//! [`crate::artifacts`] to any client of the daemon, the attached desktop
//! first, against `artifacts.db` of the daemon's data directory.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{get, patch, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use crate::api::openapi::ApiErrorBody;
use crate::api::server::AppState;
use crate::artifacts::{
    self, Artifact, ArtifactError, ArtifactStore, SaveArtifactRequest, UpdateArtifactRequest,
};
use crate::coordinator::ExecutionBackend;

type ApiError = (StatusCode, Json<ApiErrorBody>);

/// Answer of `DELETE /api/v1/artifacts/{id}`.
#[derive(Debug, Deserialize, Serialize, utoipa::ToSchema)]
pub struct DeletedArtifact {
    /// The identifier removed.
    pub deleted: String,
}

/// Build the artifacts router.
pub fn artifacts_router<B: ExecutionBackend + Clone>() -> Router<AppState<B>> {
    Router::new()
        .route("/api/v1/sessions/:id/artifacts", get(list_artifacts::<B>))
        .route("/api/v1/artifacts", post(save_artifact::<B>))
        .route(
            "/api/v1/artifacts/:id",
            patch(update_artifact::<B>).delete(delete_artifact::<B>),
        )
}

fn refusal(error: ArtifactError) -> ApiError {
    let status = match error {
        ArtifactError::NotFound(_) => StatusCode::NOT_FOUND,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    };
    (
        status,
        Json(ApiErrorBody {
            error: error.to_string(),
        }),
    )
}

fn store<B: ExecutionBackend + Clone>(state: &AppState<B>) -> ArtifactStore {
    ArtifactStore::under(&state.data_dir)
}

/// `GET /api/v1/sessions/{id}/artifacts`: the artifacts of a session, most
/// recent first.
#[utoipa::path(
    get,
    path = "/api/v1/sessions/{id}/artifacts",
    tag = "artifacts",
    params(("id" = String, Path, description = "Chat session identifier")),
    responses(
        (status = 200, description = "The artifacts of the session", body = Vec<Artifact>),
        (status = 500, description = "The artifacts store could not be read", body = ApiErrorBody),
    )
)]
pub(crate) async fn list_artifacts<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path(session_id): Path<String>,
) -> Result<Json<Vec<Artifact>>, ApiError> {
    artifacts::list(&store(&state), session_id)
        .await
        .map(Json)
        .map_err(refusal)
}

/// `POST /api/v1/artifacts`: stores a new artifact, or returns the one its
/// source message already has.
#[utoipa::path(
    post,
    path = "/api/v1/artifacts",
    tag = "artifacts",
    request_body = SaveArtifactRequest,
    responses(
        (status = 200, description = "The artifact as stored", body = Artifact),
        (status = 500, description = "The artifacts store could not be written", body = ApiErrorBody),
    )
)]
pub(crate) async fn save_artifact<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Json(request): Json<SaveArtifactRequest>,
) -> Result<Json<Artifact>, ApiError> {
    artifacts::save(&store(&state), request)
        .await
        .map(Json)
        .map_err(refusal)
}

/// `PATCH /api/v1/artifacts/{id}`: replaces the title and/or the content.
#[utoipa::path(
    patch,
    path = "/api/v1/artifacts/{id}",
    tag = "artifacts",
    params(("id" = String, Path, description = "Artifact identifier")),
    request_body = UpdateArtifactRequest,
    responses(
        (status = 200, description = "The artifact as changed", body = Artifact),
        (status = 404, description = "No artifact has this identifier", body = ApiErrorBody),
        (status = 500, description = "The artifacts store could not be written", body = ApiErrorBody),
    )
)]
pub(crate) async fn update_artifact<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path(id): Path<String>,
    Json(change): Json<UpdateArtifactRequest>,
) -> Result<Json<Artifact>, ApiError> {
    artifacts::update(&store(&state), id, change)
        .await
        .map(Json)
        .map_err(refusal)
}

/// `DELETE /api/v1/artifacts/{id}`: removes an artifact; one already gone
/// answers the same.
#[utoipa::path(
    delete,
    path = "/api/v1/artifacts/{id}",
    tag = "artifacts",
    params(("id" = String, Path, description = "Artifact identifier")),
    responses(
        (status = 200, description = "The artifact is gone", body = DeletedArtifact),
        (status = 500, description = "The artifacts store could not be written", body = ApiErrorBody),
    )
)]
pub(crate) async fn delete_artifact<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path(id): Path<String>,
) -> Result<Json<DeletedArtifact>, ApiError> {
    artifacts::delete(&store(&state), id.clone())
        .await
        .map_err(refusal)?;
    Ok(Json(DeletedArtifact { deleted: id }))
}

#[cfg(test)]
mod tests;
