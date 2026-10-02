//! The user profile over HTTP: `GET /api/v1/profile`,
//! `PUT` and `DELETE /api/v1/profile/entries/{key}`, and
//! `POST /api/v1/profile/reset`.
//!
//! The profile had no route: only the desktop running the runtime in its own
//! process could read or change it. These routes serve the functions of
//! [`crate::profile`] to any client of the daemon, the attached desktop first.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{get, post, put};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use crate::api::openapi::ApiErrorBody;
use crate::api::server::AppState;
use crate::coordinator::ExecutionBackend;
use crate::profile::{
    delete_entry, read_profile, reset_profile, write_entry, ProfileEntryView, ProfileError,
    ProfileStore, UserProfileView,
};

type ApiError = (StatusCode, Json<ApiErrorBody>);

/// Request body for `PUT /api/v1/profile/entries/{key}`.
#[derive(Debug, Deserialize, Serialize, utoipa::ToSchema)]
pub struct SetProfileEntryBody {
    /// Plain-text value.
    pub value: String,
}

/// Answer of `DELETE /api/v1/profile/entries/{key}`.
#[derive(Debug, Deserialize, Serialize, utoipa::ToSchema)]
pub struct DeletedProfileEntry {
    /// The key removed.
    pub deleted: String,
}

/// Answer of `POST /api/v1/profile/reset`.
#[derive(Debug, Deserialize, Serialize, utoipa::ToSchema)]
pub struct ProfileReset {
    /// How many entries were removed.
    pub removed: usize,
}

/// Build the profile router.
pub fn profile_router<B: ExecutionBackend + Clone>() -> Router<AppState<B>> {
    Router::new()
        .route("/api/v1/profile", get(get_profile::<B>))
        .route(
            "/api/v1/profile/entries/:key",
            put(set_profile_entry::<B>).delete(delete_profile_entry::<B>),
        )
        .route("/api/v1/profile/reset", post(reset_user_profile::<B>))
}

fn refusal(error: ProfileError) -> ApiError {
    let status = match error {
        ProfileError::Unavailable => StatusCode::SERVICE_UNAVAILABLE,
        ProfileError::EmptyKey => StatusCode::BAD_REQUEST,
        ProfileError::NotFound(_) => StatusCode::NOT_FOUND,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    };
    (
        status,
        Json(ApiErrorBody {
            error: error.to_string(),
        }),
    )
}

fn repository<B: ExecutionBackend + Clone>(
    state: &AppState<B>,
) -> Result<ProfileStore<'_>, ApiError> {
    state
        .user_memory
        .as_ref()
        .map(ProfileStore)
        .ok_or_else(|| refusal(ProfileError::Unavailable))
}

/// `GET /api/v1/profile`: every entry of the profile.
#[utoipa::path(
    get,
    path = "/api/v1/profile",
    tag = "profile",
    responses(
        (status = 200, description = "The profile", body = UserProfileView),
        (status = 503, description = "The profile store did not open", body = ApiErrorBody),
    )
)]
pub(crate) async fn get_profile<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
) -> Result<Json<UserProfileView>, ApiError> {
    read_profile(repository(&state)?)
        .await
        .map(Json)
        .map_err(refusal)
}

/// `PUT /api/v1/profile/entries/{key}`: creates or replaces one entry, as
/// written by the user.
#[utoipa::path(
    put,
    path = "/api/v1/profile/entries/{key}",
    tag = "profile",
    params(("key" = String, Path, description = "Flat storage key")),
    request_body = SetProfileEntryBody,
    responses(
        (status = 200, description = "The entry as stored", body = ProfileEntryView),
        (status = 400, description = "Empty or reserved key", body = ApiErrorBody),
        (status = 503, description = "The profile store did not open", body = ApiErrorBody),
    )
)]
pub(crate) async fn set_profile_entry<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path(key): Path<String>,
    Json(body): Json<SetProfileEntryBody>,
) -> Result<Json<ProfileEntryView>, ApiError> {
    write_entry(repository(&state)?, key, body.value)
        .await
        .map(Json)
        .map_err(refusal)
}

/// `DELETE /api/v1/profile/entries/{key}`: removes one entry.
#[utoipa::path(
    delete,
    path = "/api/v1/profile/entries/{key}",
    tag = "profile",
    params(("key" = String, Path, description = "Flat storage key")),
    responses(
        (status = 200, description = "The entry is gone", body = DeletedProfileEntry),
        (status = 404, description = "No entry has this key", body = ApiErrorBody),
        (status = 503, description = "The profile store did not open", body = ApiErrorBody),
    )
)]
pub(crate) async fn delete_profile_entry<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path(key): Path<String>,
) -> Result<Json<DeletedProfileEntry>, ApiError> {
    delete_entry(repository(&state)?, key.clone())
        .await
        .map_err(refusal)?;
    Ok(Json(DeletedProfileEntry { deleted: key }))
}

/// `POST /api/v1/profile/reset`: removes every entry the user sees, keeping
/// the onboarding markers.
#[utoipa::path(
    post,
    path = "/api/v1/profile/reset",
    tag = "profile",
    responses(
        (status = 200, description = "How many entries were removed", body = ProfileReset),
        (status = 503, description = "The profile store did not open", body = ApiErrorBody),
    )
)]
pub(crate) async fn reset_user_profile<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
) -> Result<Json<ProfileReset>, ApiError> {
    let removed = reset_profile(repository(&state)?).await.map_err(refusal)?;
    Ok(Json(ProfileReset { removed }))
}

#[cfg(test)]
mod tests;
