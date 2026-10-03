//! The Drive folder of each connected Google account over HTTP:
//! `GET /api/v1/connectors/drive-folders`, and `PUT` and `DELETE
//! /api/v1/connectors/drive-folders/{account_id}`.
//!
//! Only the desktop running the runtime in its own process could list or
//! change these folders. The routes serve the functions of
//! [`crate::drive_folders`] to any client of the daemon, the attached desktop
//! first, over the same preferences file the Drive executors read.

use axum::extract::Path;
use axum::http::StatusCode;
use axum::routing::{get, put};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use crate::api::openapi::ApiErrorBody;
use crate::api::server::AppState;
use crate::coordinator::ExecutionBackend;
use crate::drive_folders::{self, DriveFolderError, DriveFolderStatus};

type ApiError = (StatusCode, Json<ApiErrorBody>);

/// Request body for `PUT /api/v1/connectors/drive-folders/{account_id}`.
#[derive(Debug, Deserialize, Serialize, utoipa::ToSchema)]
pub struct SetDriveFolderBody {
    /// Folder path under My Drive, `Documents/Apollia` for instance. Empty
    /// means the root of My Drive.
    pub folder_path: String,
}

/// Build the Drive folder router.
pub fn drive_folders_router<B: ExecutionBackend + Clone>() -> Router<AppState<B>> {
    Router::new()
        .route("/api/v1/connectors/drive-folders", get(list_drive_folders))
        .route(
            "/api/v1/connectors/drive-folders/:account_id",
            put(set_drive_folder).delete(reset_drive_folder),
        )
}

fn refusal(error: DriveFolderError) -> ApiError {
    let status = match error {
        DriveFolderError::InvalidAccount | DriveFolderError::InvalidFolder => {
            StatusCode::BAD_REQUEST
        }
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    };
    (
        status,
        Json(ApiErrorBody {
            error: error.to_string(),
        }),
    )
}

/// `GET /api/v1/connectors/drive-folders`: the Drive folder of every
/// connected Google account.
#[utoipa::path(
    get,
    path = "/api/v1/connectors/drive-folders",
    tag = "connectors",
    responses(
        (status = 200, description = "The folder of each Google account", body = [DriveFolderStatus]),
        (status = 500, description = "The accounts or the preferences could not be read", body = ApiErrorBody),
    )
)]
pub(crate) async fn list_drive_folders() -> Result<Json<Vec<DriveFolderStatus>>, ApiError> {
    let manager = crate::connectors_bridge::get_auth()
        .await
        .map_err(|e| refusal(DriveFolderError::Accounts(e)))?;
    let prefs = drive_folders::prefs_path().map_err(refusal)?;
    let folders = drive_folders::list(&manager, &prefs).await;
    folders.map(Json).map_err(refusal)
}

/// `PUT /api/v1/connectors/drive-folders/{account_id}`: record the Drive
/// folder of one Google account.
#[utoipa::path(
    put,
    path = "/api/v1/connectors/drive-folders/{account_id}",
    tag = "connectors",
    params(("account_id" = String, Path, description = "Google account identifier")),
    request_body = SetDriveFolderBody,
    responses(
        (status = 200, description = "The folder as recorded", body = DriveFolderStatus),
        (status = 400, description = "Empty or oversized account or folder", body = ApiErrorBody),
        (status = 500, description = "The preferences could not be written", body = ApiErrorBody),
    )
)]
pub(crate) async fn set_drive_folder(
    Path(account_id): Path<String>,
    Json(body): Json<SetDriveFolderBody>,
) -> Result<Json<DriveFolderStatus>, ApiError> {
    let prefs = drive_folders::prefs_path().map_err(refusal)?;
    let folder = drive_folders::set(&prefs, &account_id, &body.folder_path);
    folder.map(Json).map_err(refusal)
}

/// `DELETE /api/v1/connectors/drive-folders/{account_id}`: forget the Drive
/// folder of one Google account, so the default applies again.
#[utoipa::path(
    delete,
    path = "/api/v1/connectors/drive-folders/{account_id}",
    tag = "connectors",
    params(("account_id" = String, Path, description = "Google account identifier")),
    responses(
        (status = 200, description = "The folder as recorded, the default again", body = DriveFolderStatus),
        (status = 400, description = "Empty or oversized account", body = ApiErrorBody),
        (status = 500, description = "The preferences could not be written", body = ApiErrorBody),
    )
)]
pub(crate) async fn reset_drive_folder(
    Path(account_id): Path<String>,
) -> Result<Json<DriveFolderStatus>, ApiError> {
    let prefs = drive_folders::prefs_path().map_err(refusal)?;
    let folder = drive_folders::reset(&prefs, &account_id);
    folder.map(Json).map_err(refusal)
}

#[cfg(test)]
mod tests;
