//! The settings of the dictation, `/api/v1/stt/config` and `/api/v1/stt/reload`:
//! the persisted configuration, read and written, and the engine rebuilt from it.

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use serde::Serialize;

use apollia_core::SttConfigRow;

use crate::api::routes_stt::{read_stt_config, RouteResult, SttErrorResponse};
use crate::api::server::AppState;
use crate::coordinator::{DynBackend, ExecutionBackend};

/// `GET /api/v1/stt/config`, return the persisted STT configuration.
///
/// Returns `200 OK` with the current [`SttConfigRow`] from `system.db`.
/// If the table is empty (first boot), the defaults are inserted and returned.
/// Returns `503 Service Unavailable` when the config repository is unavailable.
#[utoipa::path(
    get,
    path = "/api/v1/stt/config",
    tag = "stt",
    responses(
        (status = 200, description = "Persisted STT configuration"),
        (status = 500, description = "Database error", body = crate::api::openapi::ApiErrorBody),
        (status = 503, description = "STT config repository unavailable", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub async fn get_stt_config<B: ExecutionBackend + Clone + From<DynBackend>>(
    State(state): State<AppState<B>>,
) -> RouteResult<SttConfigRow> {
    let repo = state.stt_config_repo.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(SttErrorResponse {
                error: "STT config repository not available".into(),
            }),
        )
    })?;

    let config = repo
        .lock()
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(SttErrorResponse {
                    error: format!("repository lock error: {e}"),
                }),
            )
        })?
        .get_or_default()
        .map_err(|e| {
            tracing::error!(error = %e, "stt.config.read.failed");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(SttErrorResponse {
                    error: format!("database error: {e}"),
                }),
            )
        })?;

    Ok((StatusCode::OK, Json(config)))
}

/// `PUT /api/v1/stt/config`, update the persisted STT configuration.
///
/// Accepts a JSON body of [`SttConfigRow`] (fields with `#[serde(default)]` may
/// be omitted, missing fields receive their default values). Replaces the
/// singleton row in `system.db` via an upsert.
///
/// Returns `200 OK` with the updated configuration.
/// Returns `400 Bad Request` for a language hint outside
/// [`STT_LANGUAGES`](crate::stt::STT_LANGUAGES), with the message the desktop
/// shows in both modes; nothing is persisted.
/// Returns `503 Service Unavailable` when the config repository is unavailable.
#[utoipa::path(
    put,
    path = "/api/v1/stt/config",
    tag = "stt",
    request_body(content = Object, content_type = "application/json", description = "Updated STT configuration (SttConfigRow); fields with defaults may be omitted"),
    responses(
        (status = 200, description = "Updated STT configuration"),
        (status = 400, description = "Unsupported language hint", body = crate::api::openapi::ApiErrorBody),
        (status = 500, description = "Database error", body = crate::api::openapi::ApiErrorBody),
        (status = 503, description = "STT config repository unavailable", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub async fn update_stt_config<B: ExecutionBackend + Clone + From<DynBackend>>(
    State(state): State<AppState<B>>,
    Json(new_config): Json<SttConfigRow>,
) -> RouteResult<SttConfigRow> {
    crate::stt::validate_language(new_config.language.as_deref()).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(SttErrorResponse {
                error: e.to_string(),
            }),
        )
    })?;

    let repo = state.stt_config_repo.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(SttErrorResponse {
                error: "STT config repository not available".into(),
            }),
        )
    })?;

    let guard = repo.lock().map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(SttErrorResponse {
                error: format!("repository lock error: {e}"),
            }),
        )
    })?;

    guard.upsert(&new_config).map_err(|e| {
        tracing::error!(error = %e, "stt.config.persist.failed");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(SttErrorResponse {
                error: format!("database error: {e}"),
            }),
        )
    })?;

    let updated = guard.get_or_default().map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(SttErrorResponse {
                error: format!("database error: {e}"),
            }),
        )
    })?;

    tracing::info!(enabled = updated.enabled, "stt.config.updated");
    Ok((StatusCode::OK, Json(updated)))
}

/// Response body for `POST /api/v1/stt/reload`.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct SttReloadResponse {
    /// Whether an engine is loaded after the reload. `false` when STT is
    /// disabled, the model file is absent, or the runner is unavailable.
    pub loaded: bool,
}

/// `POST /api/v1/stt/reload`, rebuild the STT engine from persisted config.
///
/// Reads the current `stt_config` row from `system.db`, rebuilds the engine
/// actor via [`build_stt_engine`](crate::stt::build_stt_engine), swaps it into
/// the shared cell, and shuts the previous engine down. Lets a model enabled or
/// downloaded mid-session come online without restarting the daemon. Idempotent:
/// swaps in `None` when STT is disabled or the model is missing.
#[utoipa::path(
    post,
    path = "/api/v1/stt/reload",
    tag = "stt",
    responses(
        (status = 200, description = "STT engine reloaded", body = SttReloadResponse),
        (status = 500, description = "Database error", body = crate::api::openapi::ApiErrorBody),
        (status = 503, description = "STT config repository unavailable", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub async fn reload_stt_engine<B: ExecutionBackend + Clone + From<DynBackend>>(
    State(state): State<AppState<B>>,
) -> RouteResult<SttReloadResponse> {
    // Snapshot the persisted config, dropping the (!Send) lock guard before the
    // async rebuild.
    let cfg = read_stt_config(&state)?;

    let (engine, repository) = crate::stt::build_stt_engine(
        &state.data_dir,
        Some(&cfg),
        state.runner_proxy.clone(),
        &state.event_sender,
    )
    .await;
    let loaded = engine.is_some();

    // Swap the new engine/repository in, then shut the previous engine down
    // outside the write guard so no reader observes a half-swapped state.
    let previous = {
        let mut engine_cell = state.stt_engine.write().await;
        let mut repo_cell = state.stt_repository.write().await;
        let old = engine_cell.take();
        *engine_cell = engine;
        *repo_cell = repository;
        old
    };
    if let Some(old) = previous {
        old.shutdown().await;
    }

    tracing::info!(loaded, "stt.engine.reloaded");
    Ok((StatusCode::OK, Json(SttReloadResponse { loaded })))
}
