//! The profiles over HTTP, each named by its identifier or by `default`:
//! `GET` and `POST /api/v1/profiles`; `GET`, `PATCH` and `DELETE
//! /api/v1/profiles/{id}`; `POST /api/v1/profiles/{id}/default` and
//! `/reset`; `PUT` and `DELETE /api/v1/profiles/{id}/facts/{key}`.
//!
//! These routes serve the functions of [`crate::profile`] that manage several
//! profiles, the attached desktop first. `/api/v1/profile` keeps serving the
//! key-value view of the default profile.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{get, post, put};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use crate::api::openapi::ApiErrorBody;
use crate::api::routes_profile::{DeletedProfileEntry, ProfileReset, SetProfileEntryBody};
use crate::api::server::AppState;
use crate::coordinator::ExecutionBackend;
use crate::profile::{
    self, ProfileDetailView, ProfileEntryView, ProfileError, ProfileStore, ProfileSummaryView,
    ProfileUpdate,
};

type ApiError = (StatusCode, Json<ApiErrorBody>);

/// Request body for `POST /api/v1/profiles`.
#[derive(Debug, Deserialize, Serialize, utoipa::ToSchema)]
pub struct CreateProfileBody {
    /// Display name of the new profile.
    pub name: String,
}

/// Answer of `DELETE /api/v1/profiles/{id}`.
#[derive(Debug, Deserialize, Serialize, utoipa::ToSchema)]
pub struct DeletedProfile {
    /// The identifier removed, as the request named it.
    pub deleted: String,
}

/// Build the router of the profiles.
pub fn profiles_router<B: ExecutionBackend + Clone>() -> Router<AppState<B>> {
    Router::new()
        .route(
            "/api/v1/profiles",
            get(list_profiles::<B>).post(create_profile::<B>),
        )
        .route(
            "/api/v1/profiles/:id",
            get(get_profile_by_id::<B>)
                .patch(update_profile::<B>)
                .delete(delete_profile::<B>),
        )
        .route(
            "/api/v1/profiles/:id/default",
            post(set_default_profile::<B>),
        )
        .route("/api/v1/profiles/:id/reset", post(reset_profile::<B>))
        .route(
            "/api/v1/profiles/:id/facts/:key",
            put(set_profile_fact::<B>).delete(delete_profile_fact::<B>),
        )
}

fn refusal(error: ProfileError) -> ApiError {
    let status = match error {
        ProfileError::Unavailable => StatusCode::SERVICE_UNAVAILABLE,
        ProfileError::EmptyKey | ProfileError::InvalidKey(_) => StatusCode::BAD_REQUEST,
        ProfileError::NotFound(_) | ProfileError::ProfileNotFound(_) => StatusCode::NOT_FOUND,
        ProfileError::IsDefault(_) => StatusCode::CONFLICT,
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

/// `GET /api/v1/profiles`: every profile, the default first.
#[utoipa::path(
    get,
    path = "/api/v1/profiles",
    tag = "profile",
    responses(
        (status = 200, description = "The profiles", body = [ProfileSummaryView]),
        (status = 503, description = "The profile store did not open", body = ApiErrorBody),
    )
)]
pub(crate) async fn list_profiles<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
) -> Result<Json<Vec<ProfileSummaryView>>, ApiError> {
    profile::list_profiles(repository(&state)?)
        .await
        .map(Json)
        .map_err(refusal)
}

/// `POST /api/v1/profiles`: creates a profile with empty fields.
#[utoipa::path(
    post,
    path = "/api/v1/profiles",
    tag = "profile",
    request_body = CreateProfileBody,
    responses(
        (status = 200, description = "The profile created", body = ProfileSummaryView),
        (status = 400, description = "Empty name", body = ApiErrorBody),
        (status = 503, description = "The profile store did not open", body = ApiErrorBody),
    )
)]
pub(crate) async fn create_profile<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Json(body): Json<CreateProfileBody>,
) -> Result<Json<ProfileSummaryView>, ApiError> {
    profile::create_profile(repository(&state)?, body.name)
        .await
        .map(Json)
        .map_err(refusal)
}

/// `GET /api/v1/profiles/{id}`: one profile, its fields and its facts.
#[utoipa::path(
    get,
    path = "/api/v1/profiles/{id}",
    tag = "profile",
    params(("id" = String, Path, description = "Profile identifier, or `default`")),
    responses(
        (status = 200, description = "The profile in full", body = ProfileDetailView),
        (status = 404, description = "No profile has this identifier", body = ApiErrorBody),
        (status = 503, description = "The profile store did not open", body = ApiErrorBody),
    )
)]
pub(crate) async fn get_profile_by_id<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path(id): Path<String>,
) -> Result<Json<ProfileDetailView>, ApiError> {
    profile::read_profile_by_id(repository(&state)?, id)
        .await
        .map(Json)
        .map_err(refusal)
}

/// `PATCH /api/v1/profiles/{id}`: renames a profile and writes its fields.
#[utoipa::path(
    patch,
    path = "/api/v1/profiles/{id}",
    tag = "profile",
    params(("id" = String, Path, description = "Profile identifier, or `default`")),
    request_body = ProfileUpdate,
    responses(
        (status = 200, description = "The profile in full", body = ProfileDetailView),
        (status = 400, description = "Empty name or key outside the schema", body = ApiErrorBody),
        (status = 404, description = "No profile has this identifier", body = ApiErrorBody),
        (status = 503, description = "The profile store did not open", body = ApiErrorBody),
    )
)]
pub(crate) async fn update_profile<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path(id): Path<String>,
    Json(body): Json<ProfileUpdate>,
) -> Result<Json<ProfileDetailView>, ApiError> {
    profile::update_profile(repository(&state)?, id, body)
        .await
        .map(Json)
        .map_err(refusal)
}

/// `DELETE /api/v1/profiles/{id}`: deletes a profile and its facts.
#[utoipa::path(
    delete,
    path = "/api/v1/profiles/{id}",
    tag = "profile",
    params(("id" = String, Path, description = "Profile identifier")),
    responses(
        (status = 200, description = "The profile is gone", body = DeletedProfile),
        (status = 404, description = "No profile has this identifier", body = ApiErrorBody),
        (status = 409, description = "The default profile cannot be deleted", body = ApiErrorBody),
        (status = 503, description = "The profile store did not open", body = ApiErrorBody),
    )
)]
pub(crate) async fn delete_profile<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path(id): Path<String>,
) -> Result<Json<DeletedProfile>, ApiError> {
    profile::delete_profile(repository(&state)?, id.clone())
        .await
        .map_err(refusal)?;
    Ok(Json(DeletedProfile { deleted: id }))
}

/// `POST /api/v1/profiles/{id}/default`: makes a profile the default.
#[utoipa::path(
    post,
    path = "/api/v1/profiles/{id}/default",
    tag = "profile",
    params(("id" = String, Path, description = "Profile identifier")),
    responses(
        (status = 200, description = "The new default profile", body = ProfileSummaryView),
        (status = 404, description = "No profile has this identifier", body = ApiErrorBody),
        (status = 503, description = "The profile store did not open", body = ApiErrorBody),
    )
)]
pub(crate) async fn set_default_profile<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path(id): Path<String>,
) -> Result<Json<ProfileSummaryView>, ApiError> {
    profile::set_default_profile(repository(&state)?, id)
        .await
        .map(Json)
        .map_err(refusal)
}

/// `POST /api/v1/profiles/{id}/reset`: empties a profile and deletes its
/// facts.
#[utoipa::path(
    post,
    path = "/api/v1/profiles/{id}/reset",
    tag = "profile",
    params(("id" = String, Path, description = "Profile identifier, or `default`")),
    responses(
        (status = 200, description = "How many entries were removed", body = ProfileReset),
        (status = 404, description = "No profile has this identifier", body = ApiErrorBody),
        (status = 503, description = "The profile store did not open", body = ApiErrorBody),
    )
)]
pub(crate) async fn reset_profile<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path(id): Path<String>,
) -> Result<Json<ProfileReset>, ApiError> {
    let removed = profile::reset_profile_by_id(repository(&state)?, id)
        .await
        .map_err(refusal)?;
    Ok(Json(ProfileReset { removed }))
}

/// `PUT /api/v1/profiles/{id}/facts/{key}`: retains a fact in a profile, as
/// written by the user.
#[utoipa::path(
    put,
    path = "/api/v1/profiles/{id}/facts/{key}",
    tag = "profile",
    params(
        ("id" = String, Path, description = "Profile identifier, or `default`"),
        ("key" = String, Path, description = "Key of the fact"),
    ),
    request_body = SetProfileEntryBody,
    responses(
        (status = 200, description = "The fact as stored", body = ProfileEntryView),
        (status = 400, description = "Empty, reserved or schema key", body = ApiErrorBody),
        (status = 404, description = "No profile has this identifier", body = ApiErrorBody),
        (status = 503, description = "The profile store did not open", body = ApiErrorBody),
    )
)]
pub(crate) async fn set_profile_fact<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path((id, key)): Path<(String, String)>,
    Json(body): Json<SetProfileEntryBody>,
) -> Result<Json<ProfileEntryView>, ApiError> {
    profile::set_profile_fact(repository(&state)?, id, key, body.value)
        .await
        .map(Json)
        .map_err(refusal)
}

/// `DELETE /api/v1/profiles/{id}/facts/{key}`: forgets a fact of a profile.
#[utoipa::path(
    delete,
    path = "/api/v1/profiles/{id}/facts/{key}",
    tag = "profile",
    params(
        ("id" = String, Path, description = "Profile identifier, or `default`"),
        ("key" = String, Path, description = "Key of the fact"),
    ),
    responses(
        (status = 200, description = "The fact is gone", body = DeletedProfileEntry),
        (status = 404, description = "No such profile or fact", body = ApiErrorBody),
        (status = 503, description = "The profile store did not open", body = ApiErrorBody),
    )
)]
pub(crate) async fn delete_profile_fact<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path((id, key)): Path<(String, String)>,
) -> Result<Json<DeletedProfileEntry>, ApiError> {
    profile::delete_profile_fact(repository(&state)?, id, key.clone())
        .await
        .map_err(refusal)?;
    Ok(Json(DeletedProfileEntry { deleted: key }))
}

#[cfg(test)]
mod tests;
