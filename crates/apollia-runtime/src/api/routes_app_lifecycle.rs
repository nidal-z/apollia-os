//! Resets and erasures of the installation over HTTP:
//! `POST /api/v1/onboarding/complete`, `POST /api/v1/onboarding/reset`,
//! `POST /api/v1/memory/wipe`, `POST /api/v1/logs/clear` and
//! `POST /api/v1/factory-reset`.
//!
//! These operations had no route: only the desktop running the runtime in its
//! own process could reach the files they erase. The routes serve the
//! functions of [`crate::app_lifecycle`] on the daemon's data directory to any
//! client of the daemon, the attached desktop first.

use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::post;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use crate::api::openapi::ApiErrorBody;
use crate::api::server::AppState;
use crate::app_lifecycle::{self, LifecycleError};
use crate::coordinator::ExecutionBackend;
use crate::profile::ProfileStore;

type ApiError = (StatusCode, Json<ApiErrorBody>);

/// Answer of `POST /api/v1/onboarding/complete`.
#[derive(Debug, Deserialize, Serialize, utoipa::ToSchema)]
pub struct OnboardingMarked {
    /// Always `true`: the flag is written.
    pub onboarded: bool,
}

/// Answer of `POST /api/v1/onboarding/reset`.
#[derive(Debug, Deserialize, Serialize, utoipa::ToSchema)]
pub struct OnboardingReset {
    /// How many profile entries were removed.
    pub removed_profile_entries: usize,
}

/// Answer of `POST /api/v1/memory/wipe`.
#[derive(Debug, Deserialize, Serialize, utoipa::ToSchema)]
pub struct MemoryWiped {
    /// How many memory databases were removed.
    pub removed_databases: usize,
}

/// Answer of `POST /api/v1/logs/clear` and `POST /api/v1/factory-reset`.
#[derive(Debug, Deserialize, Serialize, utoipa::ToSchema)]
pub struct DirectoryRemoved {
    /// Whether the directory existed and was removed.
    pub removed: bool,
}

/// Build the lifecycle router.
pub fn app_lifecycle_router<B: ExecutionBackend + Clone>() -> Router<AppState<B>> {
    Router::new()
        .route("/api/v1/onboarding/complete", post(mark_onboarded::<B>))
        .route("/api/v1/onboarding/reset", post(reset_onboarding::<B>))
        .route("/api/v1/memory/wipe", post(clear_all_memories::<B>))
        .route("/api/v1/logs/clear", post(clear_logs::<B>))
        .route("/api/v1/factory-reset", post(factory_reset::<B>))
}

fn refusal(error: LifecycleError) -> ApiError {
    let status = match error {
        LifecycleError::ProfileUnavailable => StatusCode::SERVICE_UNAVAILABLE,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    };
    (
        status,
        Json(ApiErrorBody {
            error: error.to_string(),
        }),
    )
}

/// `POST /api/v1/onboarding/complete`: writes the flag that says the
/// onboarding is complete.
#[utoipa::path(
    post,
    path = "/api/v1/onboarding/complete",
    tag = "lifecycle",
    responses(
        (status = 200, description = "The flag is written", body = OnboardingMarked),
        (status = 500, description = "The flag cannot be written", body = ApiErrorBody),
    )
)]
pub(crate) async fn mark_onboarded<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
) -> Result<Json<OnboardingMarked>, ApiError> {
    app_lifecycle::mark_onboarded(&state.data_dir)
        .await
        .map_err(refusal)?;
    Ok(Json(OnboardingMarked { onboarded: true }))
}

/// `POST /api/v1/onboarding/reset`: removes the flag, the onboarding markers,
/// the visible profile and the onboarding agent's memory.
#[utoipa::path(
    post,
    path = "/api/v1/onboarding/reset",
    tag = "lifecycle",
    responses(
        (status = 200, description = "How many profile entries were removed", body = OnboardingReset),
        (status = 503, description = "The profile store did not open", body = ApiErrorBody),
    )
)]
pub(crate) async fn reset_onboarding<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
) -> Result<Json<OnboardingReset>, ApiError> {
    let removed_profile_entries = app_lifecycle::reset_onboarding(
        &state.data_dir,
        state.user_memory.as_ref().map(ProfileStore),
    )
    .await
    .map_err(refusal)?;
    Ok(Json(OnboardingReset {
        removed_profile_entries,
    }))
}

/// `POST /api/v1/memory/wipe`: removes every memory database of the daemon.
#[utoipa::path(
    post,
    path = "/api/v1/memory/wipe",
    tag = "lifecycle",
    responses(
        (status = 200, description = "How many databases were removed", body = MemoryWiped),
        (status = 500, description = "The memory directory cannot be listed", body = ApiErrorBody),
    )
)]
pub(crate) async fn clear_all_memories<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
) -> Result<Json<MemoryWiped>, ApiError> {
    let removed_databases = app_lifecycle::clear_all_memories(&state.data_dir)
        .await
        .map_err(refusal)?;
    Ok(Json(MemoryWiped { removed_databases }))
}

/// `POST /api/v1/logs/clear`: removes the daemon's logs directory.
#[utoipa::path(
    post,
    path = "/api/v1/logs/clear",
    tag = "lifecycle",
    responses(
        (status = 200, description = "Whether a logs directory was removed", body = DirectoryRemoved),
        (status = 500, description = "The directory cannot be removed", body = ApiErrorBody),
    )
)]
pub(crate) async fn clear_logs<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
) -> Result<Json<DirectoryRemoved>, ApiError> {
    let removed = app_lifecycle::clear_logs(&state.data_dir)
        .await
        .map_err(refusal)?;
    Ok(Json(DirectoryRemoved { removed }))
}

/// `POST /api/v1/factory-reset`: removes the daemon's whole data directory.
#[utoipa::path(
    post,
    path = "/api/v1/factory-reset",
    tag = "lifecycle",
    responses(
        (status = 200, description = "Whether the data directory was removed", body = DirectoryRemoved),
        (status = 500, description = "The directory cannot be removed", body = ApiErrorBody),
    )
)]
pub(crate) async fn factory_reset<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
) -> Result<Json<DirectoryRemoved>, ApiError> {
    let removed = app_lifecycle::factory_reset(&state.data_dir)
        .await
        .map_err(refusal)?;
    Ok(Json(DirectoryRemoved { removed }))
}

#[cfg(test)]
mod tests;
