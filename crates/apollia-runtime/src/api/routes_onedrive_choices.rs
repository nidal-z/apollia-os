//! The OneDrive folders and files a Microsoft account lets Apollia read, over
//! HTTP: `GET /api/v1/connectors/onedrive/{account_id}/choices`,
//! `GET /api/v1/connectors/onedrive/{account_id}/browse`, and `PUT` and
//! `DELETE /api/v1/connectors/onedrive/{account_id}/choices/{item_id}`.
//!
//! The routes serve the functions of [`crate::onedrive_choices`] to any client
//! of the daemon, the attached desktop first, over the same file the OneDrive
//! executors read.

use axum::extract::{Path, Query};
use axum::http::StatusCode;
use axum::routing::{get, put};
use axum::{Json, Router};
use serde::Deserialize;

use crate::api::openapi::ApiErrorBody;
use crate::api::server::AppState;
use crate::coordinator::ExecutionBackend;
use crate::onedrive_choices::{self, OneDriveChoiceError, OneDriveChoiceView, OneDriveEntry};

type ApiError = (StatusCode, Json<ApiErrorBody>);

/// Query of `GET /api/v1/connectors/onedrive/{account_id}/browse`.
#[derive(Debug, Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct BrowseQuery {
    /// Folder to list; the root of the drive when absent.
    pub folder_id: Option<String>,
}

/// Build the OneDrive choices router.
pub fn onedrive_choices_router<B: ExecutionBackend + Clone>() -> Router<AppState<B>> {
    Router::new()
        .route(
            "/api/v1/connectors/onedrive/:account_id/choices",
            get(list_onedrive_choices),
        )
        .route(
            "/api/v1/connectors/onedrive/:account_id/browse",
            get(browse_onedrive),
        )
        .route(
            "/api/v1/connectors/onedrive/:account_id/choices/:item_id",
            put(choose_onedrive_item).delete(forget_onedrive_item),
        )
}

fn refusal(error: OneDriveChoiceError) -> ApiError {
    let status = match error {
        OneDriveChoiceError::InvalidAccount
        | OneDriveChoiceError::InvalidItem
        | OneDriveChoiceError::NotChoosable(_) => StatusCode::BAD_REQUEST,
        OneDriveChoiceError::NotConnected(_) => StatusCode::CONFLICT,
        OneDriveChoiceError::Graph(_) => StatusCode::BAD_GATEWAY,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    };
    (
        status,
        Json(ApiErrorBody {
            error: error.to_string(),
        }),
    )
}

/// `GET /api/v1/connectors/onedrive/{account_id}/choices`: the folders and
/// files the account lets Apollia read.
#[utoipa::path(
    get,
    path = "/api/v1/connectors/onedrive/{account_id}/choices",
    tag = "connectors",
    params(("account_id" = String, Path, description = "Microsoft account identifier")),
    responses(
        (status = 200, description = "The chosen folders and files", body = [OneDriveChoiceView]),
        (status = 400, description = "Empty or oversized account", body = ApiErrorBody),
        (status = 500, description = "The choices could not be read", body = ApiErrorBody),
    )
)]
pub(crate) async fn list_onedrive_choices(
    Path(account_id): Path<String>,
) -> Result<Json<Vec<OneDriveChoiceView>>, ApiError> {
    let prefs = onedrive_choices::prefs_path().map_err(refusal)?;
    onedrive_choices::list(&prefs, &account_id)
        .map(Json)
        .map_err(refusal)
}

/// `GET /api/v1/connectors/onedrive/{account_id}/browse`: the children of a
/// folder of the drive, each marked chosen or not, for the user to choose from.
#[utoipa::path(
    get,
    path = "/api/v1/connectors/onedrive/{account_id}/browse",
    tag = "connectors",
    params(
        ("account_id" = String, Path, description = "Microsoft account identifier"),
        BrowseQuery,
    ),
    responses(
        (status = 200, description = "The children of the folder", body = [OneDriveEntry]),
        (status = 400, description = "Empty or oversized account or folder", body = ApiErrorBody),
        (status = 409, description = "The account is not connected", body = ApiErrorBody),
        (status = 502, description = "OneDrive answered with an error", body = ApiErrorBody),
        (status = 500, description = "The choices could not be read", body = ApiErrorBody),
    )
)]
pub(crate) async fn browse_onedrive(
    Path(account_id): Path<String>,
    Query(query): Query<BrowseQuery>,
) -> Result<Json<Vec<OneDriveEntry>>, ApiError> {
    let entries = onedrive_choices::browse_drive(&account_id, query.folder_id.as_deref());
    entries.await.map(Json).map_err(refusal)
}

/// `PUT /api/v1/connectors/onedrive/{account_id}/choices/{item_id}`: let
/// Apollia read the folder or file `item_id`, as OneDrive describes it.
#[utoipa::path(
    put,
    path = "/api/v1/connectors/onedrive/{account_id}/choices/{item_id}",
    tag = "connectors",
    params(
        ("account_id" = String, Path, description = "Microsoft account identifier"),
        ("item_id" = String, Path, description = "OneDrive item id"),
    ),
    responses(
        (status = 200, description = "The choices as recorded", body = [OneDriveChoiceView]),
        (status = 400, description = "Empty or oversized input, or the root of the drive", body = ApiErrorBody),
        (status = 409, description = "The account is not connected", body = ApiErrorBody),
        (status = 502, description = "OneDrive answered with an error", body = ApiErrorBody),
        (status = 500, description = "The choices could not be written", body = ApiErrorBody),
    )
)]
pub(crate) async fn choose_onedrive_item(
    Path((account_id, item_id)): Path<(String, String)>,
) -> Result<Json<Vec<OneDriveChoiceView>>, ApiError> {
    let choices = onedrive_choices::choose_item(&account_id, &item_id);
    choices.await.map(Json).map_err(refusal)
}

/// `DELETE /api/v1/connectors/onedrive/{account_id}/choices/{item_id}`: stop
/// letting Apollia read the folder or file `item_id`.
#[utoipa::path(
    delete,
    path = "/api/v1/connectors/onedrive/{account_id}/choices/{item_id}",
    tag = "connectors",
    params(
        ("account_id" = String, Path, description = "Microsoft account identifier"),
        ("item_id" = String, Path, description = "OneDrive item id"),
    ),
    responses(
        (status = 200, description = "The choices as recorded", body = [OneDriveChoiceView]),
        (status = 400, description = "Empty or oversized input", body = ApiErrorBody),
        (status = 500, description = "The choices could not be written", body = ApiErrorBody),
    )
)]
pub(crate) async fn forget_onedrive_item(
    Path((account_id, item_id)): Path<(String, String)>,
) -> Result<Json<Vec<OneDriveChoiceView>>, ApiError> {
    let prefs = onedrive_choices::prefs_path().map_err(refusal)?;
    onedrive_choices::forget(&prefs, &account_id, &item_id)
        .map(Json)
        .map_err(refusal)
}

#[cfg(test)]
mod tests;
