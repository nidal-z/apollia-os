//! A folder of this machine added or removed as a connection.
//!
//! - `POST   /api/v1/connections/folders`      , add the folder at a path
//! - `DELETE /api/v1/connections/folders/{id}` , remove a folder connection
//!
//! An addition goes through the connection admission first, then the folder
//! is kept in `system.db` by its canonical path and the addition is journaled
//! as `folder_added`. A removal deletes the row and is journaled as
//! `connection_removed`. Both answer the connection as the list shows it.

use std::path::Path;

use axum::extract::{Path as UrlPath, State};
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};

use apollia_core::{FolderConnectionError, FolderConnectionStore};

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

/// A refused folder: the message and its stable code.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct FolderErrorBody {
    /// What went wrong, for a person. For `admission_refused`, the reason
    /// the admission gave, as is.
    pub error: String,
    /// Stable code: `not_a_folder`, `already_added`, `admission_refused`,
    /// `not_found`, `store`.
    pub code: String,
}

type FolderRouteError = (StatusCode, Json<FolderErrorBody>);

fn refused(status: StatusCode, code: &str, error: impl std::fmt::Display) -> FolderRouteError {
    (
        status,
        Json(FolderErrorBody {
            error: error.to_string(),
            code: code.to_owned(),
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
        actor: ROUTE_ACTOR.to_owned(),
    });
    tracing::event!(
        tracing::Level::INFO,
        connection_id = %connection.id,
        "connections.folder.added"
    );
    Ok(connection)
}

/// `DELETE /api/v1/connections/folders/{id}`: remove a folder connection.
#[utoipa::path(
    delete,
    path = "/api/v1/connections/folders/{id}",
    tag = "connections",
    params(("id" = String, Path, description = "Identifier of the stored folder, the part of the connection identifier after `folder:`")),
    responses(
        (status = 200, description = "Folder connection removed; the connection it was", body = Connection),
        (status = 404, description = "No folder connection has this identifier", body = FolderErrorBody),
        (status = 500, description = "system.db could not be written", body = FolderErrorBody),
    )
)]
pub(crate) async fn remove_folder_connection<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    UrlPath(id): UrlPath<String>,
) -> Result<Json<Connection>, FolderRouteError> {
    let store = store(&state.data_dir).map_err(|e| store_err(&e))?;
    remove(&state.connection_governance, &store, &id).map(Json)
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
mod tests;
