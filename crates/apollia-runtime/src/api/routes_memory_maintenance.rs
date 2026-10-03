//! Upkeep of the agents' memory over HTTP, one namespace at a time:
//! `POST /api/v1/memory/namespaces/{namespace}/clear` and `.../purge`,
//! `DELETE .../entries/{entry_id}`, `GET .../export` and `POST .../import`.
//!
//! These operations had no route: only the desktop running the runtime in its
//! own process could reach the memory files. The routes serve the functions of
//! [`crate::memory_maintenance`] to any client of the daemon, the attached
//! desktop first. An export travels as JSON: the file it is read from or
//! written to stays on the machine of the client.

use apollia_memory::export::MemoryExport;
use axum::extract::{DefaultBodyLimit, Path, State};
use axum::http::StatusCode;
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use crate::api::openapi::ApiErrorBody;
use crate::api::server::AppState;
use crate::coordinator::ExecutionBackend;
use crate::memory_maintenance::{self, memory_dir, MemoryMaintenanceError};

type ApiError = (StatusCode, Json<ApiErrorBody>);

/// Upper bound on an imported export, the size the attached desktop reads
/// from the daemon.
const MAX_IMPORT_BYTES: usize = 32 * 1024 * 1024;

/// Request body for `POST /api/v1/memory/namespaces/{namespace}/clear`.
#[derive(Debug, Default, Deserialize, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ClearMemoryBody {
    /// `episodic`, `semantic`, `procedural`; absent or any other value
    /// reaches every kind.
    #[serde(default)]
    pub memory_type: Option<String>,
}

/// Request body for `POST /api/v1/memory/namespaces/{namespace}/purge`.
#[derive(Debug, Deserialize, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PurgeMemoryBody {
    /// Entries older than this many days are removed.
    pub older_than_days: u32,
    /// `episodic`, `semantic`, `procedural`; absent or any other value
    /// reaches every kind.
    #[serde(default)]
    pub memory_type: Option<String>,
}

/// Answer of the clear and purge routes.
#[derive(Debug, Deserialize, Serialize, utoipa::ToSchema)]
pub struct MemoryRemoved {
    /// How many entries were removed.
    pub removed: u64,
}

/// Answer of `DELETE /api/v1/memory/namespaces/{namespace}/entries/{entry_id}`.
#[derive(Debug, Deserialize, Serialize, utoipa::ToSchema)]
pub struct MemoryEntryDeleted {
    /// Whether an entry had this identifier.
    pub deleted: bool,
}

/// Request body for `POST /api/v1/memory/namespaces/{namespace}/import`.
#[derive(Debug, Deserialize, Serialize, utoipa::ToSchema)]
pub struct ImportMemoryBody {
    /// `replace` empties the namespace first; any other value merges.
    pub mode: String,
    /// The export, as `GET .../export` answers it.
    #[schema(value_type = Object)]
    pub content: MemoryExport,
}

/// Answer of `POST /api/v1/memory/namespaces/{namespace}/import`.
#[derive(Debug, Deserialize, Serialize, utoipa::ToSchema)]
pub struct MemoryImported {
    /// How many entries went in.
    pub imported: usize,
}

/// Build the memory upkeep router.
pub fn memory_maintenance_router<B: ExecutionBackend + Clone>() -> Router<AppState<B>> {
    Router::new()
        .route(
            "/api/v1/memory/namespaces/:namespace/clear",
            post(clear_memory::<B>),
        )
        .route(
            "/api/v1/memory/namespaces/:namespace/purge",
            post(purge_memory::<B>),
        )
        .route(
            "/api/v1/memory/namespaces/:namespace/entries/:entry_id",
            delete(delete_memory_entry::<B>),
        )
        .route(
            "/api/v1/memory/namespaces/:namespace/export",
            get(export_memory_namespace::<B>),
        )
        .route(
            "/api/v1/memory/namespaces/:namespace/import",
            post(import_memory_namespace::<B>).layer(DefaultBodyLimit::max(MAX_IMPORT_BYTES)),
        )
}

fn refusal(error: MemoryMaintenanceError) -> ApiError {
    let status = match &error {
        MemoryMaintenanceError::NamespaceNotFound(_) => StatusCode::NOT_FOUND,
        e if e.is_invalid_request() => StatusCode::BAD_REQUEST,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    };
    (
        status,
        Json(ApiErrorBody {
            error: error.to_string(),
        }),
    )
}

/// `POST /api/v1/memory/namespaces/{namespace}/clear`: removes every entry of
/// one kind, or of all kinds.
#[utoipa::path(
    post,
    path = "/api/v1/memory/namespaces/{namespace}/clear",
    tag = "memory",
    params(("namespace" = String, Path, description = "Memory namespace")),
    request_body = ClearMemoryBody,
    responses(
        (status = 200, description = "How many entries were removed", body = MemoryRemoved),
        (status = 400, description = "Invalid namespace", body = ApiErrorBody),
    )
)]
pub(crate) async fn clear_memory<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path(namespace): Path<String>,
    Json(body): Json<ClearMemoryBody>,
) -> Result<Json<MemoryRemoved>, ApiError> {
    let removed =
        memory_maintenance::clear(memory_dir(&state.data_dir), namespace, body.memory_type)
            .await
            .map_err(refusal)?;
    Ok(Json(MemoryRemoved { removed }))
}

/// `POST /api/v1/memory/namespaces/{namespace}/purge`: removes the entries
/// older than a number of days.
#[utoipa::path(
    post,
    path = "/api/v1/memory/namespaces/{namespace}/purge",
    tag = "memory",
    params(("namespace" = String, Path, description = "Memory namespace")),
    request_body = PurgeMemoryBody,
    responses(
        (status = 200, description = "How many entries were removed", body = MemoryRemoved),
        (status = 400, description = "Invalid namespace", body = ApiErrorBody),
    )
)]
pub(crate) async fn purge_memory<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path(namespace): Path<String>,
    Json(body): Json<PurgeMemoryBody>,
) -> Result<Json<MemoryRemoved>, ApiError> {
    let removed = memory_maintenance::purge(
        memory_dir(&state.data_dir),
        namespace,
        body.older_than_days,
        body.memory_type,
    )
    .await
    .map_err(refusal)?;
    Ok(Json(MemoryRemoved { removed }))
}

/// `DELETE /api/v1/memory/namespaces/{namespace}/entries/{entry_id}`: removes
/// one entry, whatever its kind.
#[utoipa::path(
    delete,
    path = "/api/v1/memory/namespaces/{namespace}/entries/{entry_id}",
    tag = "memory",
    params(
        ("namespace" = String, Path, description = "Memory namespace"),
        ("entry_id" = String, Path, description = "Entry identifier"),
    ),
    responses(
        (status = 200, description = "Whether an entry had this identifier", body = MemoryEntryDeleted),
        (status = 400, description = "Invalid namespace", body = ApiErrorBody),
        (status = 404, description = "No file holds the namespace", body = ApiErrorBody),
    )
)]
pub(crate) async fn delete_memory_entry<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path((namespace, entry_id)): Path<(String, String)>,
) -> Result<Json<MemoryEntryDeleted>, ApiError> {
    let deleted =
        memory_maintenance::delete_entry(memory_dir(&state.data_dir), namespace, entry_id)
            .await
            .map_err(refusal)?;
    Ok(Json(MemoryEntryDeleted { deleted }))
}

/// `GET /api/v1/memory/namespaces/{namespace}/export`: every entry of the
/// namespace, in the export format.
#[utoipa::path(
    get,
    path = "/api/v1/memory/namespaces/{namespace}/export",
    tag = "memory",
    params(("namespace" = String, Path, description = "Memory namespace")),
    responses(
        (status = 200, description = "The export, format version 1", body = Object),
        (status = 400, description = "Invalid namespace", body = ApiErrorBody),
    )
)]
pub(crate) async fn export_memory_namespace<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path(namespace): Path<String>,
) -> Result<Json<MemoryExport>, ApiError> {
    memory_maintenance::export(memory_dir(&state.data_dir), namespace)
        .await
        .map(Json)
        .map_err(refusal)
}

/// `POST /api/v1/memory/namespaces/{namespace}/import`: writes an export into
/// the namespace.
#[utoipa::path(
    post,
    path = "/api/v1/memory/namespaces/{namespace}/import",
    tag = "memory",
    params(("namespace" = String, Path, description = "Memory namespace")),
    request_body = ImportMemoryBody,
    responses(
        (status = 200, description = "How many entries went in", body = MemoryImported),
        (status = 400, description = "Invalid namespace or unknown format version", body = ApiErrorBody),
    )
)]
pub(crate) async fn import_memory_namespace<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path(namespace): Path<String>,
    Json(body): Json<ImportMemoryBody>,
) -> Result<Json<MemoryImported>, ApiError> {
    let imported = memory_maintenance::import(
        memory_dir(&state.data_dir),
        namespace,
        body.content,
        &body.mode,
    )
    .await
    .map_err(refusal)?;
    Ok(Json(MemoryImported { imported }))
}

#[cfg(test)]
mod tests;
