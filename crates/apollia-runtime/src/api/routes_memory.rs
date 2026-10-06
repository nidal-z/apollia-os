//! The memory of the agents over HTTP, read side:
//! `GET /api/v1/memory/namespaces`, the entries of one and a search in one,
//! the entries recalled during one turn, and the statistics of one chat
//! conversation (`GET /api/v1/sessions/{id}/stats`).
//!
//! Only the desktop running the runtime in its own process could read them.
//! These routes serve the functions of [`crate::memory_view`] and
//! [`crate::conversation_stats`] to any client of the daemon, the attached
//! desktop first.

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use crate::api::openapi::ApiErrorBody;
use crate::api::server::AppState;
use crate::conversation_stats::{
    conversation_stats, ConversationStatsError, ConversationStatsView,
};
use crate::coordinator::ExecutionBackend;
use crate::memory_view::{
    injected_entries, list_namespaces, namespace_entries, search, InjectedMemoryEntryView,
    MemoryEntryView, MemorySearchResultView, MemoryViewError,
};

type ApiError = (StatusCode, Json<ApiErrorBody>);

/// Query of `GET /api/v1/memory/namespaces/{namespace}/search`.
#[derive(Debug, Deserialize, Serialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct MemorySearchParams {
    /// The words searched for.
    pub q: String,
}

/// Build the memory router, read side.
pub fn memory_router<B: ExecutionBackend + Clone>() -> Router<AppState<B>> {
    Router::new()
        .route(
            "/api/v1/memory/namespaces",
            get(list_memory_namespaces::<B>),
        )
        .route(
            "/api/v1/memory/namespaces/:namespace/entries",
            get(list_memory_entries::<B>),
        )
        .route(
            "/api/v1/memory/namespaces/:namespace/search",
            get(search_memory::<B>),
        )
        .route(
            "/api/v1/memory/injections/:turn_id",
            get(get_injected_memory_entries::<B>),
        )
        .route(
            "/api/v1/sessions/:id/stats",
            get(get_conversation_stats::<B>),
        )
}

fn refusal(error: impl std::fmt::Display, status: StatusCode) -> ApiError {
    (
        status,
        Json(ApiErrorBody {
            error: error.to_string(),
        }),
    )
}

fn memory_refusal(error: MemoryViewError) -> ApiError {
    let status = match error {
        MemoryViewError::InvalidNamespace(_) => StatusCode::BAD_REQUEST,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    };
    refusal(error, status)
}

fn stats_refusal(error: ConversationStatsError) -> ApiError {
    let status = match error {
        ConversationStatsError::Unavailable => StatusCode::SERVICE_UNAVAILABLE,
        ConversationStatsError::NotFound(_) => StatusCode::NOT_FOUND,
    };
    refusal(error, status)
}

fn memory_dir<B: ExecutionBackend + Clone>(state: &AppState<B>) -> std::path::PathBuf {
    state.data_dir.join("memory")
}

/// `GET /api/v1/memory/namespaces`: the memory namespaces, one per file.
#[utoipa::path(
    get,
    path = "/api/v1/memory/namespaces",
    tag = "memory",
    responses(
        (status = 200, description = "The namespaces, sorted", body = Vec<String>),
        (status = 500, description = "The memory directory could not be read", body = ApiErrorBody),
    )
)]
pub(crate) async fn list_memory_namespaces<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
) -> Result<Json<Vec<String>>, ApiError> {
    list_namespaces(memory_dir(&state))
        .await
        .map(Json)
        .map_err(memory_refusal)
}

/// `GET /api/v1/memory/namespaces/{namespace}/entries`: every entry of one
/// namespace, newest first.
#[utoipa::path(
    get,
    path = "/api/v1/memory/namespaces/{namespace}/entries",
    tag = "memory",
    params(("namespace" = String, Path, description = "Memory namespace")),
    responses(
        (status = 200, description = "The entries", body = Vec<MemoryEntryView>),
        (status = 400, description = "The namespace names a path", body = ApiErrorBody),
        (status = 500, description = "The namespace did not open", body = ApiErrorBody),
    )
)]
pub(crate) async fn list_memory_entries<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path(namespace): Path<String>,
) -> Result<Json<Vec<MemoryEntryView>>, ApiError> {
    namespace_entries(memory_dir(&state), namespace)
        .await
        .map(Json)
        .map_err(memory_refusal)
}

/// `GET /api/v1/memory/namespaces/{namespace}/search?q=`: full-text search in
/// one namespace, best score first.
#[utoipa::path(
    get,
    path = "/api/v1/memory/namespaces/{namespace}/search",
    tag = "memory",
    params(
        ("namespace" = String, Path, description = "Memory namespace"),
        MemorySearchParams,
    ),
    responses(
        (status = 200, description = "The results", body = Vec<MemorySearchResultView>),
        (status = 400, description = "The namespace names a path", body = ApiErrorBody),
        (status = 500, description = "The namespace did not open or the search failed", body = ApiErrorBody),
    )
)]
pub(crate) async fn search_memory<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path(namespace): Path<String>,
    Query(params): Query<MemorySearchParams>,
) -> Result<Json<Vec<MemorySearchResultView>>, ApiError> {
    search(memory_dir(&state), namespace, params.q)
        .await
        .map(Json)
        .map_err(memory_refusal)
}

/// `GET /api/v1/memory/injections/{turn_id}`: the entries an agent recalled
/// during one turn.
#[utoipa::path(
    get,
    path = "/api/v1/memory/injections/{turn_id}",
    tag = "memory",
    params(("turn_id" = String, Path, description = "Turn identifier")),
    responses(
        (status = 200, description = "The recalled entries, none for an unknown turn", body = Vec<InjectedMemoryEntryView>),
    )
)]
pub(crate) async fn get_injected_memory_entries<B: ExecutionBackend + Clone>(
    State(_state): State<AppState<B>>,
    Path(turn_id): Path<String>,
) -> Json<Vec<InjectedMemoryEntryView>> {
    Json(injected_entries(&turn_id))
}

/// `GET /api/v1/sessions/{id}/stats`: the statistics of one chat
/// conversation.
#[utoipa::path(
    get,
    path = "/api/v1/sessions/{id}/stats",
    tag = "memory",
    params(("id" = String, Path, description = "Chat session id")),
    responses(
        (status = 200, description = "The statistics", body = ConversationStatsView),
        (status = 404, description = "No session has this id", body = ApiErrorBody),
        (status = 503, description = "The chat subsystem did not start", body = ApiErrorBody),
    )
)]
pub(crate) async fn get_conversation_stats<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path(id): Path<String>,
) -> Result<Json<ConversationStatsView>, ApiError> {
    conversation_stats(
        state.chat_manager.as_ref(),
        state.user_memory.as_deref(),
        id,
    )
    .await
    .map(Json)
    .map_err(stats_refusal)
}

#[cfg(test)]
mod tests;
