//! The OAuth clients of the connectors over HTTP: `GET /api/v1/oauth/clients`,
//! `PUT /api/v1/oauth/clients/{provider}/client-id`, `.../client-secret` and
//! `.../api-key`, `POST /api/v1/oauth/clients/{provider}/test` and
//! `POST /api/v1/oauth/clients/{provider}/import`.
//!
//! The OAuth clients had no route: only the desktop running the runtime in its
//! own process could read or change them. These routes serve the functions of
//! [`crate::oauth_clients`] to any client of the daemon, the attached desktop
//! first, against `oauth-clients.toml` under the data directory.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{get, post, put};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use crate::api::openapi::ApiErrorBody;
use crate::api::server::AppState;
use crate::coordinator::ExecutionBackend;
use crate::oauth_clients::{
    file_in, import_client_json, list_client_ids, set_field, test_client, OauthClientField,
    OauthClientIdStatus, OauthClientTestResult, OauthClientsError,
};

type ApiError = (StatusCode, Json<ApiErrorBody>);

/// Request body of the `PUT` routes: the new value, empty to clear the
/// override.
#[derive(Debug, Deserialize, Serialize, utoipa::ToSchema)]
pub struct OauthClientValueBody {
    /// The value, trimmed before it is written. Empty clears the override.
    pub value: String,
}

/// Request body of `POST /api/v1/oauth/clients/{provider}/import`.
#[derive(Debug, Deserialize, Serialize, utoipa::ToSchema)]
pub struct ImportOauthClientBody {
    /// The text of the JSON file the provider's console exported, read by the
    /// window that picked it.
    pub client_json: String,
}

/// Answer of a write: the provider whose entry changed.
#[derive(Debug, Deserialize, Serialize, utoipa::ToSchema)]
pub struct OauthClientWritten {
    /// Provider id (`"google"` / `"microsoft"`).
    pub provider: String,
}

/// Build the OAuth client router.
pub fn oauth_clients_router<B: ExecutionBackend + Clone>() -> Router<AppState<B>> {
    Router::new()
        .route("/api/v1/oauth/clients", get(list_oauth_clients::<B>))
        .route(
            "/api/v1/oauth/clients/:provider/client-id",
            put(set_oauth_client_id::<B>),
        )
        .route(
            "/api/v1/oauth/clients/:provider/client-secret",
            put(set_oauth_client_secret::<B>),
        )
        .route(
            "/api/v1/oauth/clients/:provider/api-key",
            put(set_oauth_api_key::<B>),
        )
        .route(
            "/api/v1/oauth/clients/:provider/test",
            post(test_oauth_client::<B>),
        )
        .route(
            "/api/v1/oauth/clients/:provider/import",
            post(import_oauth_client::<B>),
        )
}

fn refusal(error: OauthClientsError) -> ApiError {
    let status = match error {
        OauthClientsError::UnknownProvider(_) => StatusCode::BAD_REQUEST,
        OauthClientsError::InvalidClientFile(_) => StatusCode::UNPROCESSABLE_ENTITY,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    };
    (
        status,
        Json(ApiErrorBody {
            error: error.to_string(),
        }),
    )
}

fn write<B: ExecutionBackend + Clone>(
    state: &AppState<B>,
    provider: String,
    field: OauthClientField,
    value: &str,
) -> Result<Json<OauthClientWritten>, ApiError> {
    set_field(&file_in(&state.data_dir), &provider, field, value).map_err(refusal)?;
    Ok(Json(OauthClientWritten { provider }))
}

/// `GET /api/v1/oauth/clients`: the OAuth client of every provider and where
/// each value came from, never a secret.
#[utoipa::path(
    get,
    path = "/api/v1/oauth/clients",
    tag = "connectors",
    responses(
        (status = 200, description = "One row per provider", body = [OauthClientIdStatus]),
    )
)]
pub(crate) async fn list_oauth_clients<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
) -> Json<Vec<OauthClientIdStatus>> {
    Json(list_client_ids(&file_in(&state.data_dir)))
}

/// `PUT /api/v1/oauth/clients/{provider}/client-id`: writes the client id
/// override, empty to clear it.
#[utoipa::path(
    put,
    path = "/api/v1/oauth/clients/{provider}/client-id",
    tag = "connectors",
    params(("provider" = String, Path, description = "Provider id: google or microsoft")),
    request_body = OauthClientValueBody,
    responses(
        (status = 200, description = "The override is written", body = OauthClientWritten),
        (status = 400, description = "Unknown provider", body = ApiErrorBody),
        (status = 500, description = "The file could not be written", body = ApiErrorBody),
    )
)]
pub(crate) async fn set_oauth_client_id<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path(provider): Path<String>,
    Json(body): Json<OauthClientValueBody>,
) -> Result<Json<OauthClientWritten>, ApiError> {
    write(&state, provider, OauthClientField::ClientId, &body.value)
}

/// `PUT /api/v1/oauth/clients/{provider}/client-secret`: writes the client
/// secret override, empty to clear it. The secret is never answered back.
#[utoipa::path(
    put,
    path = "/api/v1/oauth/clients/{provider}/client-secret",
    tag = "connectors",
    params(("provider" = String, Path, description = "Provider id: google or microsoft")),
    request_body = OauthClientValueBody,
    responses(
        (status = 200, description = "The override is written", body = OauthClientWritten),
        (status = 400, description = "Unknown provider", body = ApiErrorBody),
        (status = 500, description = "The file could not be written", body = ApiErrorBody),
    )
)]
pub(crate) async fn set_oauth_client_secret<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path(provider): Path<String>,
    Json(body): Json<OauthClientValueBody>,
) -> Result<Json<OauthClientWritten>, ApiError> {
    write(
        &state,
        provider,
        OauthClientField::ClientSecret,
        &body.value,
    )
}

/// `PUT /api/v1/oauth/clients/{provider}/api-key`: writes the public API key
/// Google Picker uses, empty to clear it.
#[utoipa::path(
    put,
    path = "/api/v1/oauth/clients/{provider}/api-key",
    tag = "connectors",
    params(("provider" = String, Path, description = "Provider id: google or microsoft")),
    request_body = OauthClientValueBody,
    responses(
        (status = 200, description = "The override is written", body = OauthClientWritten),
        (status = 400, description = "Unknown provider", body = ApiErrorBody),
        (status = 500, description = "The file could not be written", body = ApiErrorBody),
    )
)]
pub(crate) async fn set_oauth_api_key<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path(provider): Path<String>,
    Json(body): Json<OauthClientValueBody>,
) -> Result<Json<OauthClientWritten>, ApiError> {
    write(&state, provider, OauthClientField::ApiKey, &body.value)
}

/// `POST /api/v1/oauth/clients/{provider}/test`: checks the client without a
/// consent: present, well-formed, and its authorization server reachable.
#[utoipa::path(
    post,
    path = "/api/v1/oauth/clients/{provider}/test",
    tag = "connectors",
    params(("provider" = String, Path, description = "Provider id: google or microsoft")),
    responses(
        (status = 200, description = "The verdict, passed or the first failing check", body = OauthClientTestResult),
        (status = 400, description = "Unknown provider", body = ApiErrorBody),
    )
)]
pub(crate) async fn test_oauth_client<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path(provider): Path<String>,
) -> Result<Json<OauthClientTestResult>, ApiError> {
    test_client(&file_in(&state.data_dir), &provider)
        .await
        .map(Json)
        .map_err(refusal)
}

/// `POST /api/v1/oauth/clients/{provider}/import`: writes the client id and
/// secret of a Google console export.
#[utoipa::path(
    post,
    path = "/api/v1/oauth/clients/{provider}/import",
    tag = "connectors",
    params(("provider" = String, Path, description = "Provider id: google")),
    request_body = ImportOauthClientBody,
    responses(
        (status = 200, description = "Both halves are written", body = OauthClientWritten),
        (status = 400, description = "Unknown provider", body = ApiErrorBody),
        (status = 422, description = "Not a Google client export", body = ApiErrorBody),
        (status = 500, description = "The file could not be written", body = ApiErrorBody),
    )
)]
pub(crate) async fn import_oauth_client<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path(provider): Path<String>,
    Json(body): Json<ImportOauthClientBody>,
) -> Result<Json<OauthClientWritten>, ApiError> {
    import_client_json(&file_in(&state.data_dir), &provider, &body.client_json).map_err(refusal)?;
    Ok(Json(OauthClientWritten { provider }))
}

#[cfg(test)]
mod tests;
