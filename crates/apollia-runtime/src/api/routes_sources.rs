//! The registry of sources: what an agent may consult once it is checked.
//!
//! - `GET    /api/v1/sources`      , every source, oldest first
//! - `POST   /api/v1/sources`      , add a source
//! - `DELETE /api/v1/sources/{id}` , remove a source
//! - `POST   /api/v1/sources/preview`, what a batch on a workbook or a CSV
//!   file would read ([`preview`])
//!
//! The registry lives in `system.db` and is owned by its actor; these routes
//! talk to it through its handle, and answer 503 when the runtime holds none.
//! An agent's `sources` list names the identifiers these routes return, or
//! `profile` and `memory`.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use apollia_sources::{
    IndexState, NewSource, Source, SourceKind, SourceRegistryError, SourceRegistryHandle,
    SourcesError,
};

use crate::api::routes_llm::servers::{api_err, ApiError};
use crate::api::server::AppState;
use crate::coordinator::ExecutionBackend;

/// Build the router of the registry of sources.
pub fn sources_router<B: ExecutionBackend + Clone>() -> Router<AppState<B>> {
    Router::new()
        .route(
            "/api/v1/sources",
            get(list_sources::<B>).post(create_source::<B>),
        )
        .route("/api/v1/sources/:id", delete(delete_source::<B>))
        .route("/api/v1/sources/preview", post(preview::preview_source))
}

pub mod preview;

/// What a source is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum SourceKindDto {
    /// A folder whose documents are indexed.
    Folder,
    /// One file.
    File,
    /// A mailbox.
    Mail,
    /// The user profile.
    Profile,
    /// The agent's memory.
    Memory,
    /// A code folder, offered in developer mode.
    CodeFolder,
}

impl From<SourceKind> for SourceKindDto {
    fn from(kind: SourceKind) -> Self {
        match kind {
            SourceKind::Folder => SourceKindDto::Folder,
            SourceKind::File => SourceKindDto::File,
            SourceKind::Mail => SourceKindDto::Mail,
            SourceKind::Profile => SourceKindDto::Profile,
            SourceKind::Memory => SourceKindDto::Memory,
            SourceKind::CodeFolder => SourceKindDto::CodeFolder,
        }
    }
}

/// Where the index of a source stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum IndexStateDto {
    /// Not indexed.
    Absent,
    /// Being indexed.
    Indexing,
    /// Indexed, keywords and meaning.
    Ready,
    /// Indexing failed.
    Error,
    /// Indexed for keywords only.
    KeywordsOnly,
}

impl From<IndexState> for IndexStateDto {
    fn from(state: IndexState) -> Self {
        match state {
            IndexState::Absent => IndexStateDto::Absent,
            IndexState::Indexing => IndexStateDto::Indexing,
            IndexState::Ready => IndexStateDto::Ready,
            IndexState::Error => IndexStateDto::Error,
            IndexState::KeywordsOnly => IndexStateDto::KeywordsOnly,
        }
    }
}

/// A source of the registry.
#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SourceResponse {
    /// Identifier, the form an agent's `sources` list names it by.
    pub id: String,
    /// What the source is.
    pub kind: SourceKindDto,
    /// The name the user sees.
    pub label: String,
    /// The group the source belongs to, if any.
    pub group: Option<String>,
    /// What the source points at: a folder connection, a file path, a mail
    /// connection, a memory space.
    pub reference: Option<String>,
    /// Where its index stands.
    pub index_state: IndexStateDto,
    /// When it was added, RFC 3339 in UTC.
    pub added_at: String,
}

impl From<Source> for SourceResponse {
    fn from(source: Source) -> Self {
        Self {
            id: source.id,
            kind: source.kind.into(),
            label: source.label,
            group: source.group,
            reference: source.reference,
            index_state: source.index_state.into(),
            added_at: source.added_at,
        }
    }
}

/// Body of `POST /api/v1/sources`.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateSourceBody {
    /// `folder`, `file`, `mail`, `profile`, `memory` or `code_folder`.
    pub kind: String,
    /// The name the user sees.
    pub label: String,
    /// A group the source belongs to.
    #[serde(default)]
    pub group: Option<String>,
    /// What the source points at; required for `folder`, `file` and `mail`.
    #[serde(default)]
    pub reference: Option<String>,
}

/// The registry handle of the runtime, or 503.
pub(crate) fn registry<B: ExecutionBackend + Clone>(
    state: &AppState<B>,
) -> Result<SourceRegistryHandle, ApiError> {
    state.source_registry.clone().ok_or_else(|| {
        api_err(
            StatusCode::SERVICE_UNAVAILABLE,
            "source registry not available",
        )
    })
}

/// The status of a registry refusal or failure.
pub(crate) fn sources_error(err: SourcesError) -> ApiError {
    let status = match &err {
        SourcesError::Registry(
            SourceRegistryError::UnknownKind(_)
            | SourceRegistryError::EmptyLabel
            | SourceRegistryError::MissingReference(_)
            | SourceRegistryError::UnknownSource(_),
        ) => StatusCode::UNPROCESSABLE_ENTITY,
        SourcesError::Registry(SourceRegistryError::NotFound(_)) => StatusCode::NOT_FOUND,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    };
    api_err(status, err)
}

/// `GET /api/v1/sources`: every source, oldest first.
#[utoipa::path(
    get,
    path = "/api/v1/sources",
    tag = "sources",
    responses(
        (status = 200, description = "The sources of the registry", body = Vec<SourceResponse>),
        (status = 503, description = "Source registry not available", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub async fn list_sources<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
) -> Result<Json<Vec<SourceResponse>>, ApiError> {
    let sources = registry(&state)?.list().await.map_err(sources_error)?;
    Ok(Json(sources.into_iter().map(Into::into).collect()))
}

/// `POST /api/v1/sources`: add a source.
#[utoipa::path(
    post,
    path = "/api/v1/sources",
    tag = "sources",
    request_body = CreateSourceBody,
    responses(
        (status = 201, description = "Source added", body = SourceResponse),
        (status = 422, description = "Unknown kind, blank label, or a folder, file or mailbox without a reference", body = crate::api::openapi::ApiErrorBody),
        (status = 503, description = "Source registry not available", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub async fn create_source<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Json(body): Json<CreateSourceBody>,
) -> Result<(StatusCode, Json<SourceResponse>), ApiError> {
    let registry = registry(&state)?;
    let kind =
        SourceKind::parse(&body.kind).map_err(|e| api_err(StatusCode::UNPROCESSABLE_ENTITY, e))?;
    let created = registry
        .create(NewSource {
            kind,
            label: body.label,
            group: body.group,
            reference: body.reference,
        })
        .await
        .map_err(sources_error)?;
    Ok((StatusCode::CREATED, Json(created.into())))
}

/// `DELETE /api/v1/sources/{id}`: remove a source.
#[utoipa::path(
    delete,
    path = "/api/v1/sources/{id}",
    tag = "sources",
    params(("id" = String, Path, description = "Source identifier")),
    responses(
        (status = 200, description = "Source removed; the source as it was", body = SourceResponse),
        (status = 404, description = "Source not found", body = crate::api::openapi::ApiErrorBody),
        (status = 503, description = "Source registry not available", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub async fn delete_source<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path(id): Path<String>,
) -> Result<Json<SourceResponse>, ApiError> {
    let removed = registry(&state)?.delete(&id).await.map_err(sources_error)?;
    Ok(Json(removed.into()))
}

#[cfg(test)]
mod tests;
