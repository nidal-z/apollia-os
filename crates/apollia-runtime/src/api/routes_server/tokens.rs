//! The tokens of the compatible server, from the native API:
//! `GET` and `POST /api/v1/server/tokens`, `DELETE /api/v1/server/tokens/{token}`.
//!
//! Creating a token answers its value once; nothing answers it again. Listing
//! shows its name, its last four characters and its dates, never the value.

use apollia_core::server_token::{ServerTokenError, ServerTokenRow, TokenSelector};
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};

use super::{handle, ApiError};
use crate::api::openapi::ApiErrorBody;
use crate::api::server::AppState;
use crate::compat_server::ServerTokensError;
use crate::coordinator::ExecutionBackend;

/// Body of `POST /api/v1/server/tokens`.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateServerTokenRequest {
    /// Who holds the token, usually the application it is for. At most 64
    /// characters, not digits only, unique among the tokens in use.
    pub name: String,
}

/// A token as the list shows it: never its value.
#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ServerTokenView {
    /// Identifier, usable to revoke it.
    pub id: i64,
    /// Name given at creation.
    pub name: String,
    /// The last four characters of the value.
    pub last_four: String,
    /// When it was created, RFC 3339.
    pub created_at: String,
    /// When it last opened the server, RFC 3339; absent if never.
    pub last_used_at: Option<String>,
    /// Whether it was revoked.
    pub revoked: bool,
    /// When it was revoked, RFC 3339.
    pub revoked_at: Option<String>,
}

impl From<ServerTokenRow> for ServerTokenView {
    fn from(row: ServerTokenRow) -> Self {
        Self {
            id: row.id,
            name: row.name,
            last_four: row.last_four,
            created_at: row.created_at,
            last_used_at: row.last_used_at,
            revoked: row.revoked_at.is_some(),
            revoked_at: row.revoked_at,
        }
    }
}

/// Answer of `POST /api/v1/server/tokens`: the token and, this once, its value.
#[derive(Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreatedServerTokenView {
    /// The token as the list will show it.
    #[serde(flatten)]
    pub token: ServerTokenView,
    /// The value (`apo_` and 64 hexadecimal characters). Shown this once.
    pub value: String,
}

fn token_refusal(error: ServerTokensError) -> ApiError {
    let status = match &error {
        ServerTokensError::Store(ServerTokenError::InvalidName { .. }) => StatusCode::BAD_REQUEST,
        ServerTokensError::Store(ServerTokenError::NameTaken { .. }) => StatusCode::CONFLICT,
        ServerTokensError::Store(ServerTokenError::NotFound { .. }) => StatusCode::NOT_FOUND,
        ServerTokensError::ActorGone => StatusCode::SERVICE_UNAVAILABLE,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    };
    (
        status,
        Json(ApiErrorBody {
            error: error.to_string(),
        }),
    )
}

/// `GET /api/v1/server/tokens`: every token, in use and revoked, oldest first.
#[utoipa::path(
    get,
    path = "/api/v1/server/tokens",
    tag = "server",
    responses(
        (status = 200, description = "The tokens, without their values", body = [ServerTokenView]),
        (status = 500, description = "The token store failed", body = ApiErrorBody),
        (status = 503, description = "No compatible server in this runtime", body = ApiErrorBody),
    )
)]
pub(crate) async fn list_server_tokens<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
) -> Result<Json<Vec<ServerTokenView>>, ApiError> {
    let rows = handle(&state)?
        .tokens()
        .list()
        .await
        .map_err(token_refusal)?;
    Ok(Json(rows.into_iter().map(ServerTokenView::from).collect()))
}

/// `POST /api/v1/server/tokens`: creates a named token and answers its value,
/// the only time it is given.
#[utoipa::path(
    post,
    path = "/api/v1/server/tokens",
    tag = "server",
    request_body = CreateServerTokenRequest,
    responses(
        (status = 201, description = "The token and its value, shown once", body = CreatedServerTokenView),
        (status = 400, description = "The name is empty, too long or digits only", body = ApiErrorBody),
        (status = 409, description = "A token in use already carries the name", body = ApiErrorBody),
        (status = 500, description = "The token store failed", body = ApiErrorBody),
        (status = 503, description = "No compatible server in this runtime", body = ApiErrorBody),
    )
)]
pub(crate) async fn create_server_token<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Json(request): Json<CreateServerTokenRequest>,
) -> Result<(StatusCode, Json<CreatedServerTokenView>), ApiError> {
    let created = handle(&state)?
        .tokens()
        .create(request.name)
        .await
        .map_err(token_refusal)?;
    Ok((
        StatusCode::CREATED,
        Json(CreatedServerTokenView {
            token: created.row.into(),
            value: created.value,
        }),
    ))
}

/// `DELETE /api/v1/server/tokens/{token}`: revokes a token by identifier or
/// by name. The row stays listed, marked revoked.
#[utoipa::path(
    delete,
    path = "/api/v1/server/tokens/{token}",
    tag = "server",
    params(("token" = String, Path, description = "Identifier (digits) or name of the token")),
    responses(
        (status = 200, description = "The token, revoked", body = ServerTokenView),
        (status = 404, description = "No token matches", body = ApiErrorBody),
        (status = 500, description = "The token store failed", body = ApiErrorBody),
        (status = 503, description = "No compatible server in this runtime", body = ApiErrorBody),
    )
)]
pub(crate) async fn revoke_server_token<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path(token): Path<String>,
) -> Result<Json<ServerTokenView>, ApiError> {
    let row = handle(&state)?
        .tokens()
        .revoke(TokenSelector::parse(&token))
        .await
        .map_err(token_refusal)?;
    Ok(Json(row.into()))
}
