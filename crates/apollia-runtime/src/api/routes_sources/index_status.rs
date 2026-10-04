//! `GET /api/v1/sources/{id}/index`: where the index of a source stands.
//!
//! The state the registry records, the documents and passages the index
//! holds, and the files it left out: those over the size cap of the indexing
//! and those no reader could open, each with its size and the reason.

use axum::extract::{Path, State};
use axum::Json;
use serde::Serialize;

use apollia_sources::index::{SkipReason, SkippedFile};
use apollia_sources::IndexStatus;

use super::{registry, sources_error, IndexStateDto};
use crate::api::routes_llm::servers::ApiError;
use crate::api::server::AppState;
use crate::coordinator::ExecutionBackend;

/// Why a file was left out of an index.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum SkipReasonDto {
    /// Over the size cap of the indexing (50 MiB).
    TooLarge,
    /// No reader could open it: damaged, protected by a password, or not the
    /// format its name says.
    Unreadable,
}

impl From<SkipReason> for SkipReasonDto {
    fn from(reason: SkipReason) -> Self {
        match reason {
            SkipReason::TooLarge => SkipReasonDto::TooLarge,
            SkipReason::Unreadable => SkipReasonDto::Unreadable,
        }
    }
}

/// A file left out of an index.
#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SkippedFileResponse {
    /// Path of the file on this machine.
    pub path: String,
    /// Its size in bytes when it was left out.
    pub size_bytes: u64,
    /// Why it was left out.
    pub reason: SkipReasonDto,
}

impl From<SkippedFile> for SkippedFileResponse {
    fn from(file: SkippedFile) -> Self {
        Self {
            path: file.path,
            size_bytes: file.size_bytes,
            reason: file.reason.into(),
        }
    }
}

/// Where the index of a source stands.
#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SourceIndexResponse {
    /// Source identifier.
    pub id: String,
    /// The state of its index.
    pub index_state: IndexStateDto,
    /// Documents the index holds.
    pub documents: u64,
    /// Passages the index holds.
    pub chunks: u64,
    /// Files left out of the index.
    pub skipped: Vec<SkippedFileResponse>,
}

impl From<IndexStatus> for SourceIndexResponse {
    fn from(status: IndexStatus) -> Self {
        Self {
            id: status.source,
            index_state: status.state.into(),
            documents: status.documents,
            chunks: status.chunks,
            skipped: status.skipped.into_iter().map(Into::into).collect(),
        }
    }
}

/// `GET /api/v1/sources/{id}/index`: where the index of a source stands.
#[utoipa::path(
    get,
    path = "/api/v1/sources/{id}/index",
    tag = "sources",
    params(("id" = String, Path, description = "Source identifier")),
    responses(
        (status = 200, description = "The state of the source's index, its counts and the files it left out", body = SourceIndexResponse),
        (status = 404, description = "Source not found", body = crate::api::openapi::ApiErrorBody),
        (status = 503, description = "Source registry not available", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub async fn source_index<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path(id): Path<String>,
) -> Result<Json<SourceIndexResponse>, ApiError> {
    let status = registry(&state)?
        .index_status(&id)
        .await
        .map_err(sources_error)?;
    Ok(Json(status.into()))
}
