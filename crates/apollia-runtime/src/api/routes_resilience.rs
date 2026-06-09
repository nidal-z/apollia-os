//! Resilience routes, snapshot and manage per-tool circuit breakers.
//!
//! - `GET /api/v1/resilience/status`, list every registered breaker.
//! - `GET /api/v1/resilience/status/:tool`, single-tool snapshot.
//! - `POST /api/v1/resilience/reset/:tool`, force-close a breaker.
//!
//! The shared [`ResilienceLayer`](apollia_oria::ResilienceLayer) lives in
//! [`crate::api::server::AppState::resilience_layer`]. It is hydrated by
//! [`crate::observability::spawn_resilience_subscriber`] from the
//! `RuntimeEvent::ToolCallCompleted` / `ToolCallDenied` stream, so a fresh
//! runtime returns an empty snapshot until tools actually run, the same
//! semantics the per-task ORIA engines exhibit today.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Serialize;

use apollia_oria::CircuitBreakerSnapshot;

use crate::api::server::AppState;
use crate::coordinator::ExecutionBackend;

/// Build the resilience sub-router.
pub fn resilience_router<B: ExecutionBackend + Clone>() -> Router<AppState<B>> {
    Router::new()
        .route("/api/v1/resilience/status", get(list_breakers::<B>))
        .route("/api/v1/resilience/status/:tool", get(get_breaker::<B>))
        .route("/api/v1/resilience/reset/:tool", post(reset_breaker::<B>))
}

/// Envelope for `GET /api/v1/resilience/status`.
#[derive(Debug, Serialize)]
pub struct ResilienceStatusResponse {
    /// Snapshot for every tool the runtime has seen at least one event for.
    pub circuit_breakers: Vec<CircuitBreakerSnapshot>,
}

/// `GET /api/v1/resilience/status`, list all known circuit breakers.
pub async fn list_breakers<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
) -> Json<ResilienceStatusResponse> {
    let snapshot = match state.resilience_layer.as_ref() {
        Some(layer) => layer
            .lock()
            .map(|guard| guard.snapshot())
            .unwrap_or_default(),
        None => Vec::new(),
    };
    Json(ResilienceStatusResponse {
        circuit_breakers: snapshot,
    })
}

/// Standard 404 body when a tool is not registered with a breaker yet.
#[derive(Debug, Serialize)]
pub struct NotFoundResponse {
    /// Human-readable explanation.
    pub error: String,
}

/// `GET /api/v1/resilience/status/:tool`, single-tool snapshot.
pub async fn get_breaker<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path(tool): Path<String>,
) -> Result<Json<CircuitBreakerSnapshot>, (StatusCode, Json<NotFoundResponse>)> {
    let Some(layer) = state.resilience_layer.as_ref() else {
        return Err((
            StatusCode::NOT_FOUND,
            Json(NotFoundResponse {
                error: format!("tool '{tool}' has no recorded circuit breaker"),
            }),
        ));
    };
    let guard = layer.lock().map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(NotFoundResponse {
                error: "resilience layer mutex poisoned".to_string(),
            }),
        )
    })?;
    let snapshot = guard
        .snapshot()
        .into_iter()
        .find(|s| s.tool_name == tool)
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(NotFoundResponse {
                    error: format!("tool '{tool}' has no recorded circuit breaker"),
                }),
            )
        })?;
    Ok(Json(snapshot))
}

/// Response body for the reset route.
#[derive(Debug, Serialize)]
pub struct ResetResponse {
    pub tool: String,
    pub reset: bool,
}

/// `POST /api/v1/resilience/reset/:tool`, force-close a breaker.
pub async fn reset_breaker<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path(tool): Path<String>,
) -> Result<Json<ResetResponse>, (StatusCode, Json<NotFoundResponse>)> {
    let Some(layer) = state.resilience_layer.as_ref() else {
        return Err((
            StatusCode::NOT_FOUND,
            Json(NotFoundResponse {
                error: format!("tool '{tool}' has no recorded circuit breaker"),
            }),
        ));
    };
    let guard = layer.lock().map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(NotFoundResponse {
                error: "resilience layer mutex poisoned".to_string(),
            }),
        )
    })?;
    let found = guard.reset_breaker(&tool);
    if !found {
        return Err((
            StatusCode::NOT_FOUND,
            Json(NotFoundResponse {
                error: format!("tool '{tool}' has no recorded circuit breaker"),
            }),
        ));
    }
    Ok(Json(ResetResponse { tool, reset: true }))
}
