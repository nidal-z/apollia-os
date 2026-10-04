//! What keeps a local model loaded: the lease of an open conversation, and
//! the user's pin.
//!
//! - `POST /api/v1/llm/leases`         , a conversation on screen designates a model
//! - `POST /api/v1/llm/leases/release` , that conversation left the screen
//! - `GET  /api/v1/llm/pins`           , the pinned models
//! - `PUT  /api/v1/llm/pins`           , pin or unpin a model
//!
//! Releasing the last lease on a model unloads it at once unless it is pinned
//! or answering a request. A lease names the client that opened it, the
//! `client` an interface passes to `GET /api/v1/events`: when that stream ends,
//! the client's leases are released. See [`crate::residency`].

use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use apollia_core::ModelUnloadReason;

use crate::api::openapi::ApiErrorBody;
use crate::api::server::AppState;
use crate::coordinator::ExecutionBackend;
use crate::residency::{Engines, Residency, ResidencyError};

/// Request body for `POST /api/v1/llm/leases`.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct OpenLeaseRequest {
    /// The interface holding the conversation, the `client` it passes to
    /// `GET /api/v1/events`.
    pub client_id: String,
    /// The conversation on screen.
    pub conversation_id: String,
    /// Path of the model the conversation designates, as the resident list
    /// reports it.
    pub model_path: String,
}

/// Response body for `POST /api/v1/llm/leases`.
#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LeaseResponse {
    /// The model the lease holds.
    pub model_path: String,
    /// Open conversations designating it, this one included.
    pub leases: usize,
}

/// Request body for `POST /api/v1/llm/leases/release`.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseLeaseRequest {
    /// The interface that opened the lease.
    pub client_id: String,
    /// The conversation that left the screen.
    pub conversation_id: String,
}

/// Response body for `POST /api/v1/llm/leases/release`.
#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseLeaseResponse {
    /// The model no open conversation designates any more; absent when the
    /// lease did not exist or another conversation still holds the model.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub released: Option<String>,
    /// Whether that model was unloaded. It stays loaded when pinned or
    /// answering a request.
    pub unloaded: bool,
}

/// Response body for `GET /api/v1/llm/pins`.
#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PinsResponse {
    /// Paths of the pinned models, in path order.
    pub pins: Vec<String>,
}

/// Request and response body for `PUT /api/v1/llm/pins`.
#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ModelPin {
    /// Path of the model, as the resident list reports it.
    pub model_path: String,
    /// `true` keeps it loaded until it is unloaded by hand.
    pub pinned: bool,
}

type ApiError = (StatusCode, Json<ApiErrorBody>);

fn api_err(code: StatusCode, msg: impl std::fmt::Display) -> ApiError {
    (
        code,
        Json(ApiErrorBody {
            error: msg.to_string(),
        }),
    )
}

/// The engines of `state` and the residency they share; 503 without an engine.
fn residency_of<B: ExecutionBackend + Clone>(
    state: &AppState<B>,
) -> Result<(Engines<'_>, std::sync::Arc<Residency>), ApiError> {
    let engines = Engines {
        llama: state.llama_server_supervisor.as_deref(),
        mlx: state.mlx_engine.as_deref(),
    };
    let residency = engines.residency().ok_or_else(|| {
        api_err(
            StatusCode::SERVICE_UNAVAILABLE,
            "no local engine on this host",
        )
    })?;
    Ok((engines, residency))
}

/// 400 when one of `fields` is blank.
fn require(fields: &[(&str, &str)]) -> Result<(), ApiError> {
    match fields.iter().find(|(_, value)| value.trim().is_empty()) {
        Some((name, _)) => Err(api_err(
            StatusCode::BAD_REQUEST,
            format!("{name} must not be empty"),
        )),
        None => Ok(()),
    }
}

/// Handler for `POST /api/v1/llm/leases`.
///
/// A conversation moved to another model releases its lease on the previous
/// one, which is unloaded when nothing else holds it.
#[utoipa::path(
    post,
    path = "/api/v1/llm/leases",
    tag = "llm",
    request_body = OpenLeaseRequest,
    responses(
        (status = 200, description = "Lease open", body = LeaseResponse),
        (status = 400, description = "A blank field", body = ApiErrorBody),
        (status = 503, description = "No local engine on this host", body = ApiErrorBody),
    )
)]
pub async fn open_model_lease<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Json(body): Json<OpenLeaseRequest>,
) -> Result<Json<LeaseResponse>, ApiError> {
    require(&[
        ("clientId", &body.client_id),
        ("conversationId", &body.conversation_id),
        ("modelPath", &body.model_path),
    ])?;
    let (engines, residency) = residency_of(&state)?;
    let previous = residency.open_lease(&body.client_id, &body.conversation_id, &body.model_path);
    if let Some(previous) = previous {
        engines
            .unload_released(&residency, &[previous], ModelUnloadReason::LastConversation)
            .await;
    }
    Ok(Json(LeaseResponse {
        leases: residency.leases_on(&body.model_path),
        model_path: body.model_path,
    }))
}

/// Handler for `POST /api/v1/llm/leases/release`.
///
/// Releasing the last lease on a model unloads it at once, unless it is
/// pinned or answering a request.
#[utoipa::path(
    post,
    path = "/api/v1/llm/leases/release",
    tag = "llm",
    request_body = ReleaseLeaseRequest,
    responses(
        (status = 200, description = "Lease released, or none was open", body = ReleaseLeaseResponse),
        (status = 400, description = "A blank field", body = ApiErrorBody),
        (status = 503, description = "No local engine on this host", body = ApiErrorBody),
    )
)]
pub async fn release_model_lease<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Json(body): Json<ReleaseLeaseRequest>,
) -> Result<Json<ReleaseLeaseResponse>, ApiError> {
    require(&[
        ("clientId", &body.client_id),
        ("conversationId", &body.conversation_id),
    ])?;
    let (engines, residency) = residency_of(&state)?;
    let released = residency.close_lease(&body.client_id, &body.conversation_id);
    let mut unloaded = false;
    if let Some(model) = &released {
        engines
            .unload_released(
                &residency,
                std::slice::from_ref(model),
                ModelUnloadReason::LastConversation,
            )
            .await;
        unloaded = !engines.serves(model).await;
    }
    Ok(Json(ReleaseLeaseResponse { released, unloaded }))
}

/// Handler for `GET /api/v1/llm/pins`.
#[utoipa::path(
    get,
    path = "/api/v1/llm/pins",
    tag = "llm",
    responses(
        (status = 200, description = "The pinned models", body = PinsResponse),
        (status = 503, description = "No local engine on this host", body = ApiErrorBody),
    )
)]
pub async fn list_model_pins<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
) -> Result<Json<PinsResponse>, ApiError> {
    let (_, residency) = residency_of(&state)?;
    Ok(Json(PinsResponse {
        pins: residency.pins(),
    }))
}

/// Handler for `PUT /api/v1/llm/pins`.
///
/// A pinned model is never unloaded on its own. Unpinning a model that nothing
/// else holds leaves it to the idle delay; it is not unloaded at once.
#[utoipa::path(
    put,
    path = "/api/v1/llm/pins",
    tag = "llm",
    request_body = ModelPin,
    responses(
        (status = 200, description = "Pin saved", body = ModelPin),
        (status = 400, description = "Empty model path", body = ApiErrorBody),
        (status = 500, description = "The pin could not be saved", body = ApiErrorBody),
        (status = 503, description = "No local engine on this host", body = ApiErrorBody),
    )
)]
pub async fn set_model_pin<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Json(body): Json<ModelPin>,
) -> Result<Json<ModelPin>, ApiError> {
    require(&[("modelPath", &body.model_path)])?;
    let (_, residency) = residency_of(&state)?;
    let model_path = body.model_path.trim().to_owned();
    let pinned = body.pinned;
    let saved = {
        let residency = std::sync::Arc::clone(&residency);
        let model_path = model_path.clone();
        tokio::task::spawn_blocking(move || residency.set_pinned(&model_path, pinned)).await
    };
    match saved {
        Ok(Ok(())) => Ok(Json(ModelPin { model_path, pinned })),
        Ok(Err(e @ ResidencyError::EmptyPath)) => Err(api_err(StatusCode::BAD_REQUEST, e)),
        Ok(Err(e)) => Err(api_err(StatusCode::INTERNAL_SERVER_ERROR, e)),
        Err(e) => Err(api_err(StatusCode::INTERNAL_SERVER_ERROR, e)),
    }
}

/// Sub-router for the lease and pin routes, merged into the LLM routes.
pub fn residency_routes<B: ExecutionBackend + Clone>() -> Router<AppState<B>> {
    Router::new()
        .route("/api/v1/llm/leases", post(open_model_lease::<B>))
        .route("/api/v1/llm/leases/release", post(release_model_lease::<B>))
        .route(
            "/api/v1/llm/pins",
            get(list_model_pins::<B>).put(set_model_pin::<B>),
        )
}

#[cfg(test)]
mod tests;
