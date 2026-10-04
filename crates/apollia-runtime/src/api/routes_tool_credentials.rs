//! The credentials of the native tools over HTTP:
//! `GET /api/v1/tool-credentials`, `PUT` and `DELETE
//! /api/v1/tool-credentials/{tool_name}/{key_name}`, and
//! `POST /api/v1/tool-credential-tests/{tool_name}`.
//!
//! Only the desktop running the runtime in its own process could manage these
//! credentials. These routes serve the functions of
//! [`crate::tool_credentials`] to any client of the daemon, the attached
//! desktop first. A value goes in and never comes out. The test has its own
//! prefix so that no key name can shadow it.

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{get, post, put};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use crate::api::openapi::ApiErrorBody;
use crate::api::server::AppState;
use crate::coordinator::ExecutionBackend;
use crate::tool_credentials::{
    delete_credential, list_credentials, set_credential, test_credential, CredentialEntryView,
    CredentialError, CredentialTestView,
};

type ApiError = (StatusCode, Json<ApiErrorBody>);

/// Query of `GET /api/v1/tool-credentials`.
#[derive(Debug, Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct CredentialListQuery {
    /// Only the credentials of this tool.
    pub tool_name: Option<String>,
}

/// Request body of `PUT /api/v1/tool-credentials/{tool_name}/{key_name}`.
#[derive(Debug, Deserialize, Serialize, utoipa::ToSchema)]
pub struct SetCredentialBody {
    /// Cleartext value, encrypted before it is written.
    pub value: String,
}

/// Answer of `PUT /api/v1/tool-credentials/{tool_name}/{key_name}`.
#[derive(Debug, Deserialize, Serialize, utoipa::ToSchema)]
pub struct StoredCredential {
    /// Owning tool.
    pub tool_name: String,
    /// Key name stored.
    pub key_name: String,
}

/// Answer of `DELETE /api/v1/tool-credentials/{tool_name}/{key_name}`.
#[derive(Debug, Deserialize, Serialize, utoipa::ToSchema)]
pub struct DeletedCredential {
    /// Whether a value was there to remove.
    pub removed: bool,
}

/// Build the tool credential router.
pub fn tool_credentials_router<B: ExecutionBackend + Clone>() -> Router<AppState<B>> {
    Router::new()
        .route("/api/v1/tool-credentials", get(list_tool_credentials::<B>))
        .route(
            "/api/v1/tool-credentials/:tool_name/:key_name",
            put(set_tool_credential::<B>).delete(delete_tool_credential::<B>),
        )
        .route(
            "/api/v1/tool-credential-tests/:tool_name",
            post(test_tool_credential::<B>),
        )
}

fn refusal(error: CredentialError) -> ApiError {
    let status = match error {
        CredentialError::EmptyName | CredentialError::EmptyValue | CredentialError::NoTest(_) => {
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

/// `GET /api/v1/tool-credentials`: the stored credentials, values left out.
#[utoipa::path(
    get,
    path = "/api/v1/tool-credentials",
    tag = "tools",
    params(CredentialListQuery),
    responses(
        (status = 200, description = "The credentials, without their values", body = Vec<CredentialEntryView>),
        (status = 500, description = "The credential store failed", body = ApiErrorBody),
    )
)]
pub(crate) async fn list_tool_credentials<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Query(query): Query<CredentialListQuery>,
) -> Result<Json<Vec<CredentialEntryView>>, ApiError> {
    list_credentials(
        state.data_dir.clone(),
        state.secret_store.clone(),
        query.tool_name,
    )
    .await
    .map(Json)
    .map_err(refusal)
}

/// `PUT /api/v1/tool-credentials/{tool_name}/{key_name}`: stores or replaces
/// one credential, encrypted.
#[utoipa::path(
    put,
    path = "/api/v1/tool-credentials/{tool_name}/{key_name}",
    tag = "tools",
    params(
        ("tool_name" = String, Path, description = "Owning tool"),
        ("key_name" = String, Path, description = "Key name, e.g. brave.api_key"),
    ),
    request_body = SetCredentialBody,
    responses(
        (status = 200, description = "The credential is stored", body = StoredCredential),
        (status = 400, description = "Blank name or empty value", body = ApiErrorBody),
        (status = 500, description = "The credential store failed", body = ApiErrorBody),
    )
)]
pub(crate) async fn set_tool_credential<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path((tool_name, key_name)): Path<(String, String)>,
    Json(body): Json<SetCredentialBody>,
) -> Result<Json<StoredCredential>, ApiError> {
    set_credential(
        state.data_dir.clone(),
        state.secret_store.clone(),
        tool_name.clone(),
        key_name.clone(),
        body.value,
    )
    .await
    .map_err(refusal)?;
    Ok(Json(StoredCredential {
        tool_name,
        key_name,
    }))
}

/// `DELETE /api/v1/tool-credentials/{tool_name}/{key_name}`: removes one
/// credential. Removing one that is not there is not an error.
#[utoipa::path(
    delete,
    path = "/api/v1/tool-credentials/{tool_name}/{key_name}",
    tag = "tools",
    params(
        ("tool_name" = String, Path, description = "Owning tool"),
        ("key_name" = String, Path, description = "Key name"),
    ),
    responses(
        (status = 200, description = "Whether a value was removed", body = DeletedCredential),
        (status = 500, description = "The credential store failed", body = ApiErrorBody),
    )
)]
pub(crate) async fn delete_tool_credential<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path((tool_name, key_name)): Path<(String, String)>,
) -> Result<Json<DeletedCredential>, ApiError> {
    let removed = delete_credential(
        state.data_dir.clone(),
        state.secret_store.clone(),
        tool_name,
        key_name,
    )
    .await
    .map_err(refusal)?;
    Ok(Json(DeletedCredential { removed }))
}

/// `POST /api/v1/tool-credential-tests/{tool_name}`: validates the tool's
/// credential with a live call. A missing or rejected key is an answer with
/// `ok: false`, not an error.
#[utoipa::path(
    post,
    path = "/api/v1/tool-credential-tests/{tool_name}",
    tag = "tools",
    params(("tool_name" = String, Path, description = "Tool whose credential is tested")),
    responses(
        (status = 200, description = "The outcome of the live call", body = CredentialTestView),
        (status = 400, description = "The tool has no live test", body = ApiErrorBody),
        (status = 500, description = "The credential store failed", body = ApiErrorBody),
    )
)]
pub(crate) async fn test_tool_credential<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path(tool_name): Path<String>,
) -> Result<Json<CredentialTestView>, ApiError> {
    test_credential(
        state.data_dir.clone(),
        state.secret_store.clone(),
        tool_name,
    )
    .await
    .map(Json)
    .map_err(refusal)
}

#[cfg(test)]
mod tests;
