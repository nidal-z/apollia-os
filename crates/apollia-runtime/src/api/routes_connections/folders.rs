//! A folder of this machine added or removed as a connection.
//!
//! - `POST   /api/v1/connections/folders`      , add the folder at a path
//! - `PUT    /api/v1/connections/folders/{id}` , point a folder connection at
//!   another folder, its identifier kept
//! - `DELETE /api/v1/connections/folders/{id}` , remove a folder connection
//!
//! An addition goes through the connection admission first, then the folder
//! is kept in `system.db` by its canonical path and the addition is journaled
//! as `folder_added`. A removal deletes the row and is journaled as
//! `connection_removed`. A folder that moved or was renamed is chosen again
//! under the same connection: the new folder goes through the admission, the
//! row keeps its identifier, so the accesses of its agents and the sources made
//! on it stay, and the change is journaled as `connection_modified`. Each
//! answers the connection as the list shows it.
//!
//! Sources may be made on a folder connection (a folder whose documents are
//! indexed, a code folder). Removing the connection then asks for
//! confirmation: without `?confirm=true` the route removes nothing and
//! answers 409 with the sources that depend on it; with it, those sources and
//! their indexes go first, then the connection.

use std::path::Path;

use axum::extract::{Path as UrlPath, Query, State};
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};

use apollia_core::{FolderConnectionError, FolderConnectionStore};
use apollia_sources::{SourceKind, SourceRegistryHandle};

use crate::api::server::AppState;
use crate::connections::folders::folder_families;
use crate::connections::{folder_connection, Connection};
use crate::coordinator::ExecutionBackend;
use crate::governance::{
    AdmissionDecision, AdmissionRequest, ConnectionEvent, ConnectionGovernance,
};

/// Who the connection journal names for a folder added or removed here.
const ROUTE_ACTOR: &str = "api";

/// Body of `POST /api/v1/connections/folders`.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AddFolderBody {
    /// Absolute path of the folder, as the system picker answered it.
    pub path: String,
    /// Name shown to the user; the folder's own name when absent.
    #[serde(default)]
    pub label: Option<String>,
}

/// Body of `PUT /api/v1/connections/folders/{id}`.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RepointFolderBody {
    /// Absolute path of the folder, as the system picker answered it.
    pub path: String,
}

/// Query of `DELETE /api/v1/connections/folders/{id}`.
#[derive(Debug, Default, Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct RemoveFolderQuery {
    /// Remove the sources made on the folder, and their indexes, with it.
    /// Without it, a folder sources depend on is not removed.
    #[serde(default)]
    pub confirm: bool,
}

/// A source made on a folder connection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, utoipa::ToSchema)]
pub struct DependentSource {
    /// Source identifier.
    pub id: String,
    /// The name the user sees.
    pub label: String,
    /// `folder` or `code_folder`.
    pub kind: String,
}

/// A refused folder: the message and its stable code.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct FolderErrorBody {
    /// What went wrong, for a person. For `admission_refused`, the reason
    /// the admission gave, as is.
    pub error: String,
    /// Stable code: `not_a_folder`, `already_added`, `admission_refused`,
    /// `not_found`, `has_dependent_sources`, `sources`, `store`.
    pub code: String,
    /// For `has_dependent_sources`, the sources removing the folder would
    /// remove; absent otherwise.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dependent_sources: Vec<DependentSource>,
}

pub(crate) type FolderRouteError = (StatusCode, Json<FolderErrorBody>);

fn refused(status: StatusCode, code: &str, error: impl std::fmt::Display) -> FolderRouteError {
    (
        status,
        Json(FolderErrorBody {
            error: error.to_string(),
            code: code.to_owned(),
            dependent_sources: Vec::new(),
        }),
    )
}

fn store_err(err: &FolderConnectionError) -> FolderRouteError {
    match err {
        FolderConnectionError::NotAFolder { .. } => {
            refused(StatusCode::BAD_REQUEST, "not_a_folder", err)
        }
        FolderConnectionError::AlreadyAdded { .. } => {
            refused(StatusCode::CONFLICT, "already_added", err)
        }
        FolderConnectionError::NotFound(_) => refused(StatusCode::NOT_FOUND, "not_found", err),
        _ => refused(StatusCode::INTERNAL_SERVER_ERROR, "store", err),
    }
}

/// The folder store of the data directory `data_dir`.
pub(crate) fn store(data_dir: &Path) -> Result<FolderConnectionStore, FolderConnectionError> {
    FolderConnectionStore::open(&apollia_core::paths::DataFile::System.path(data_dir))
}

/// `POST /api/v1/connections/folders`: add a folder of this machine.
#[utoipa::path(
    post,
    path = "/api/v1/connections/folders",
    tag = "connections",
    request_body = AddFolderBody,
    responses(
        (status = 201, description = "Folder added as a connection", body = Connection),
        (status = 400, description = "The path is not a folder of this machine", body = FolderErrorBody),
        (status = 403, description = "Refused by the connection admission", body = FolderErrorBody),
        (status = 409, description = "The folder is already a connection", body = FolderErrorBody),
        (status = 500, description = "system.db could not be written", body = FolderErrorBody),
    )
)]
pub(crate) async fn add_folder_connection<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Json(body): Json<AddFolderBody>,
) -> Result<(StatusCode, Json<Connection>), FolderRouteError> {
    let store = store(&state.data_dir).map_err(|e| store_err(&e))?;
    let connection = add(&state.connection_governance, &store, &body)?;
    Ok((StatusCode::CREATED, Json(connection)))
}

/// Admit, store and journal the folder `body` names.
pub(crate) fn add(
    governance: &ConnectionGovernance,
    store: &FolderConnectionStore,
    body: &AddFolderBody,
) -> Result<Connection, FolderRouteError> {
    add_as(governance, store, body, ROUTE_ACTOR)
}

/// [`add`], the journal naming `actor` as who added the folder.
pub(crate) fn add_as(
    governance: &ConnectionGovernance,
    store: &FolderConnectionStore,
    body: &AddFolderBody,
    actor: &str,
) -> Result<Connection, FolderRouteError> {
    let canonical =
        FolderConnectionStore::canonical_folder(&body.path).map_err(|e| store_err(&e))?;
    if let AdmissionDecision::Refused { reason } =
        governance.admit(&AdmissionRequest::for_folder(&canonical, folder_families()))
    {
        return Err(refused(StatusCode::FORBIDDEN, "admission_refused", reason));
    }
    let folder = store
        .add(&canonical, body.label.as_deref())
        .map_err(|e| store_err(&e))?;
    let connection = folder_connection(&folder);
    governance.record_or_warn(&ConnectionEvent::FolderAdded {
        connection_id: connection.id.clone(),
        path: folder.path.clone(),
        actor: actor.to_owned(),
    });
    tracing::event!(
        tracing::Level::INFO,
        connection_id = %connection.id,
        "connections.folder.added"
    );
    Ok(connection)
}

/// `PUT /api/v1/connections/folders/{id}`: point a folder connection at
/// another folder, its identifier, label and accesses kept.
#[utoipa::path(
    put,
    path = "/api/v1/connections/folders/{id}",
    tag = "connections",
    params(
        ("id" = String, Path, description = "Identifier of the stored folder, the part of the connection identifier after `folder:`"),
    ),
    request_body = RepointFolderBody,
    responses(
        (status = 200, description = "The connection now points at the folder; the connection it is", body = Connection),
        (status = 400, description = "The path is not a folder of this machine", body = FolderErrorBody),
        (status = 403, description = "Refused by the connection admission", body = FolderErrorBody),
        (status = 404, description = "No folder connection has this identifier", body = FolderErrorBody),
        (status = 409, description = "Another connection already holds the folder", body = FolderErrorBody),
        (status = 500, description = "system.db could not be written", body = FolderErrorBody),
    )
)]
pub(crate) async fn repoint_folder_connection<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    UrlPath(id): UrlPath<String>,
    Json(body): Json<RepointFolderBody>,
) -> Result<Json<Connection>, FolderRouteError> {
    let store = store(&state.data_dir).map_err(|e| store_err(&e))?;
    repoint(&state.connection_governance, &store, &id, &body.path).map(Json)
}

/// Admit the folder at `path`, point the connection `id` at it and journal
/// the change.
pub(crate) fn repoint(
    governance: &ConnectionGovernance,
    store: &FolderConnectionStore,
    id: &str,
    path: &str,
) -> Result<Connection, FolderRouteError> {
    if store.get(id).map_err(|e| store_err(&e))?.is_none() {
        return Err(store_err(&FolderConnectionError::NotFound(id.to_owned())));
    }
    let canonical = FolderConnectionStore::canonical_folder(path).map_err(|e| store_err(&e))?;
    if let AdmissionDecision::Refused { reason } =
        governance.admit(&AdmissionRequest::for_folder(&canonical, folder_families()))
    {
        return Err(refused(StatusCode::FORBIDDEN, "admission_refused", reason));
    }
    let folder = store.repoint(id, &canonical).map_err(|e| store_err(&e))?;
    let connection = folder_connection(&folder);
    governance.record_or_warn(&ConnectionEvent::Modified {
        connection_id: connection.id.clone(),
        remote: false,
        actor: ROUTE_ACTOR.to_owned(),
    });
    tracing::event!(
        tracing::Level::INFO,
        connection_id = %connection.id,
        "connections.folder.repointed"
    );
    Ok(connection)
}

/// `DELETE /api/v1/connections/folders/{id}`: remove a folder connection.
#[utoipa::path(
    delete,
    path = "/api/v1/connections/folders/{id}",
    tag = "connections",
    params(
        ("id" = String, Path, description = "Identifier of the stored folder, the part of the connection identifier after `folder:`"),
        RemoveFolderQuery,
    ),
    responses(
        (status = 200, description = "Folder connection removed, with the sources made on it when confirmed; the connection it was", body = Connection),
        (status = 404, description = "No folder connection has this identifier", body = FolderErrorBody),
        (status = 409, description = "Sources depend on the folder and the removal was not confirmed; nothing was removed, `dependent_sources` lists them", body = FolderErrorBody),
        (status = 500, description = "system.db could not be written, or a dependent source could not be removed", body = FolderErrorBody),
    )
)]
pub(crate) async fn remove_folder_connection<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    UrlPath(id): UrlPath<String>,
    Query(query): Query<RemoveFolderQuery>,
) -> Result<Json<Connection>, FolderRouteError> {
    remove_with_sources(
        &state.connection_governance,
        &state.data_dir,
        state.source_registry.as_ref(),
        &id,
        query.confirm,
    )
    .await
    .map(Json)
}

/// The sources of `registry` made on the folder connection `id`.
pub(crate) async fn dependent_sources(
    registry: &SourceRegistryHandle,
    id: &str,
) -> Result<Vec<DependentSource>, FolderRouteError> {
    let sources = registry
        .list()
        .await
        .map_err(|e| refused(StatusCode::INTERNAL_SERVER_ERROR, "sources", e))?;
    Ok(sources
        .into_iter()
        .filter(|s| matches!(s.kind, SourceKind::Folder | SourceKind::CodeFolder))
        .filter(|s| {
            s.reference
                .as_deref()
                .map(|r| r.strip_prefix("folder:").unwrap_or(r))
                == Some(id)
        })
        .map(|s| DependentSource {
            id: s.id,
            label: s.label,
            kind: s.kind.as_str().to_owned(),
        })
        .collect())
}

/// [`remove`], after the sources made on the folder: without `confirm`, a
/// folder sources depend on is refused with their list and nothing is
/// removed; with it, each of them, and its index, goes first. The folder
/// store of `data_dir` is opened around the awaits, never held across them.
pub(crate) async fn remove_with_sources(
    governance: &ConnectionGovernance,
    data_dir: &Path,
    registry: Option<&SourceRegistryHandle>,
    id: &str,
    confirm: bool,
) -> Result<Connection, FolderRouteError> {
    let known = store(data_dir)
        .and_then(|store| store.get(id))
        .map_err(|e| store_err(&e))?;
    if known.is_none() {
        return Err(store_err(&FolderConnectionError::NotFound(id.to_owned())));
    }
    if let Some(registry) = registry {
        let dependents = dependent_sources(registry, id).await?;
        if !dependents.is_empty() && !confirm {
            let (status, Json(mut body)) = refused(
                StatusCode::CONFLICT,
                "has_dependent_sources",
                format!(
                    "{} source(s) depend on this folder; confirm to remove them with it",
                    dependents.len()
                ),
            );
            body.dependent_sources = dependents;
            return Err((status, Json(body)));
        }
        for source in &dependents {
            match registry.delete(&source.id).await {
                Ok(_) => tracing::event!(
                    tracing::Level::INFO,
                    source = %source.id,
                    connection_id = %id,
                    "connections.folder.source_removed"
                ),
                Err(apollia_sources::SourcesError::Registry(
                    apollia_sources::SourceRegistryError::NotFound(_),
                )) => {}
                Err(e) => return Err(refused(StatusCode::INTERNAL_SERVER_ERROR, "sources", e)),
            }
        }
    }
    let store = store(data_dir).map_err(|e| store_err(&e))?;
    remove(governance, &store, id)
}

/// Remove and journal the folder connection `id`.
pub(crate) fn remove(
    governance: &ConnectionGovernance,
    store: &FolderConnectionStore,
    id: &str,
) -> Result<Connection, FolderRouteError> {
    let folder = store.remove(id).map_err(|e| store_err(&e))?;
    let connection = folder_connection(&folder);
    governance.record_or_warn(&ConnectionEvent::Removed {
        connection_id: connection.id.clone(),
        actor: ROUTE_ACTOR.to_owned(),
    });
    tracing::event!(
        tracing::Level::INFO,
        connection_id = %connection.id,
        "connections.folder.removed"
    );
    Ok(connection)
}

#[cfg(test)]
pub(crate) mod tests;
