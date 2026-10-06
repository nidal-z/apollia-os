//! Inference servers, `/api/v1/llm/servers`.
//!
//! A server is an address that speaks the OpenAI protocol, with an optional
//! key. Before it is saved it can be tested (`POST .../servers/test`, which
//! lists its models and writes nothing); once saved, its models are listed by
//! name and each one routes as `<server>/<model>` with no backend row of its
//! own. The key goes to the secret store and is never answered back. A saved
//! server is changed field by field (`PATCH .../servers/{name}`): its address,
//! its key, whether it is enabled, how long it may stay silent, and the
//! fields added to every request sent to it.

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{get, patch, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use apollia_auth::SecretStore;
use apollia_core::inference_server::{
    route_name, validate_address, validate_extra_params, validate_server_name,
    validate_server_timeout, INFERENCE_SERVER_SECRET_SERVICE,
};
use apollia_core::{InferenceServer, LlmBackendRepository};
use apollia_llm::server_models::{
    list_server_model_entries, list_server_models, ServerModelsError,
};
use apollia_llm::server_window::{context_short, fill_windows, EXPECTED_CONTEXT_WINDOW};

use crate::api::routes_llm::backends::{map_backend_error, BackendErrorResponse};
use crate::api::server::AppState;
use crate::coordinator::ExecutionBackend;
use crate::inference_servers::{resolve_server_key, server_endpoints};

pub(crate) type ApiError = (StatusCode, Json<BackendErrorResponse>);

pub(crate) fn api_err(status: StatusCode, msg: impl std::fmt::Display) -> ApiError {
    (
        status,
        Json(BackendErrorResponse {
            error: msg.to_string(),
        }),
    )
}

/// Body of `POST /api/v1/llm/servers/test`.
#[derive(Deserialize, utoipa::ToSchema)]
pub struct TestServerRequest {
    /// Base address, with or without the trailing `/v1`.
    pub base_url: String,
    /// Key, when the server asks for one. Used for this call only.
    #[serde(default)]
    pub api_key: Option<String>,
}

/// Body of `POST /api/v1/llm/servers`.
#[derive(Deserialize, utoipa::ToSchema)]
pub struct CreateServerRequest {
    /// Unique name, pattern `^[a-z0-9_-]+$`.
    pub name: String,
    /// Base address, with or without the trailing `/v1`.
    pub base_url: String,
    /// Key, when the server asks for one. Written to the secret store.
    #[serde(default)]
    pub api_key: Option<String>,
    /// Whether the server's models are routable (default: `true`).
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}

/// Body of `PATCH /api/v1/llm/servers/{name}`. A field left out keeps what
/// the server has.
#[derive(Deserialize, utoipa::ToSchema)]
pub struct UpdateServerRequest {
    /// New base address, with or without the trailing `/v1`.
    #[serde(default)]
    pub base_url: Option<String>,
    /// New key, written to the secret store. `null` or an empty string
    /// removes the key the server had.
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<String>)]
    pub api_key: Option<Option<String>>,
    /// Whether the server's models are routable.
    #[serde(default)]
    pub enabled: Option<bool>,
    /// Seconds the server may stay silent before a request to it is given
    /// up, 60 to 3600. `null` returns to the default of the runtime.
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<u32>)]
    pub timeout_secs: Option<Option<u32>>,
    /// JSON object whose fields are added to every request sent to this
    /// server; a field the request already carries is left as it is. `model`,
    /// `messages`, `stream`, `stream_options`, `tools` and `tool_choice` are
    /// refused. `null` adds nothing.
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<Object>)]
    pub extra_params: Option<Option<serde_json::Map<String, serde_json::Value>>>,
}

/// Reads a present field, `null` included, as `Some`, so an absent field
/// (`None`) is told from one that empties the value (`Some(None)`).
fn present<'de, T, D>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    T: Deserialize<'de>,
    D: serde::Deserializer<'de>,
{
    Option::<T>::deserialize(deserializer).map(Some)
}

/// Hand-written so a logged request can never print the key.
macro_rules! redacted_debug {
    ($ty:ident, $($field:ident),*) => {
        impl std::fmt::Debug for $ty {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.debug_struct(stringify!($ty))
                    $(.field(stringify!($field), &self.$field))*
                    .field("api_key", &self.api_key.as_ref().map(|_| "<redacted>"))
                    .finish()
            }
        }
    };
}
redacted_debug!(TestServerRequest, base_url);
redacted_debug!(CreateServerRequest, name, base_url, enabled);
redacted_debug!(UpdateServerRequest, base_url, enabled, timeout_secs);

/// One declared server. The key is never part of it.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ServerResponse {
    /// Unique name, the first segment of `<server>/<model>`.
    pub name: String,
    /// Base address.
    pub base_url: String,
    /// Whether a key is held for this server.
    pub has_key: bool,
    /// Environment variable holding the key, for a migrated `${VAR}` key.
    pub api_key_env: Option<String>,
    /// Whether the server's models are routable.
    pub enabled: bool,
    /// Seconds the server may stay silent before a request to it is given
    /// up. Absent when the default of the runtime applies.
    pub timeout_secs: Option<u32>,
    /// Fields added to every request sent to this server. Absent when none.
    #[schema(value_type = Option<Object>)]
    pub extra_params: Option<serde_json::Map<String, serde_json::Value>>,
}

impl From<InferenceServer> for ServerResponse {
    fn from(s: InferenceServer) -> Self {
        Self {
            name: s.name,
            base_url: s.base_url,
            has_key: s.has_key,
            api_key_env: s.api_key_env,
            enabled: s.enabled,
            timeout_secs: s.timeout_secs,
            extra_params: s.extra_params,
        }
    }
}

/// Body of `GET /api/v1/llm/servers`.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ServersListResponse {
    /// Every declared server, sorted by name.
    pub servers: Vec<ServerResponse>,
}

/// Body of a successful `POST /api/v1/llm/servers/test`.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct TestServerResponse {
    /// Identifiers of the models the server offers, in its order.
    pub models: Vec<String>,
}

/// One model of a declared server.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ServerModel {
    /// Identifier the server gives the model.
    pub id: String,
    /// Name to address it by, `<server>/<model>`.
    pub route: String,
    /// Context window the server serves the model with, in tokens, read on
    /// the server (`/api/show` on Ollama, `max_model_len` on vLLM). Absent
    /// when the server exposes none.
    pub context_window: Option<u32>,
    /// Whether that window is shorter than the one an agent is expected to
    /// have. Nothing is changed on the server: its administrator decides.
    pub context_short: bool,
}

/// Body of `GET /api/v1/llm/servers/{name}/models`.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ServerModelsResponse {
    /// Server name.
    pub server: String,
    /// Models the server offers, in its order.
    pub models: Vec<ServerModel>,
    /// Window an agent is expected to have, in tokens: a model served with a
    /// shorter one is marked `context_short`.
    pub expected_context_window: u32,
}

/// Body of a failed server call: a readable message and a stable kind.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ServerProbeErrorResponse {
    /// Human-readable message.
    pub error: String,
    /// `unreachable`, `key_refused` or `not_openai_compatible`.
    pub kind: String,
}

/// Body of `DELETE /api/v1/llm/servers/{name}`.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct DeleteServerResponse {
    /// Name of the deleted server.
    pub deleted: String,
}

fn probe_error(err: ServerModelsError) -> (StatusCode, Json<ServerProbeErrorResponse>) {
    (
        StatusCode::BAD_GATEWAY,
        Json(ServerProbeErrorResponse {
            kind: err.kind().to_string(),
            error: err.to_string(),
        }),
    )
}

fn as_probe((status, Json(body)): ApiError) -> (StatusCode, Json<ServerProbeErrorResponse>) {
    (
        status,
        Json(ServerProbeErrorResponse {
            error: body.error,
            kind: "request".into(),
        }),
    )
}

pub(crate) fn with_repo<B, T>(
    state: &AppState<B>,
    f: impl FnOnce(&LlmBackendRepository) -> Result<T, ApiError>,
) -> Result<T, ApiError>
where
    B: ExecutionBackend + Clone,
{
    let repo = state.llm_backend_repo.as_ref().ok_or_else(|| {
        api_err(
            StatusCode::SERVICE_UNAVAILABLE,
            "LLM backend repository not available",
        )
    })?;
    let guard = repo.lock().map_err(|_| {
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "repository lock poisoned",
        )
    })?;
    f(&guard)
}

/// Save `req` as a server, its key going to the store `store` opens. A server
/// saved again without a key loses the one it had; it keeps its timeout and
/// its extra parameters, which this request does not carry.
pub(crate) fn save_server<F>(
    repo: &LlmBackendRepository,
    req: &CreateServerRequest,
    store: F,
) -> Result<InferenceServer, ApiError>
where
    F: FnOnce() -> Result<Box<dyn SecretStore>, String>,
{
    let key = req
        .api_key
        .as_deref()
        .map(str::trim)
        .filter(|k| !k.is_empty());
    let previous = repo.find_server(&req.name).map_err(map_backend_error)?;
    let had_key = previous.as_ref().is_some_and(|s| s.has_key);
    let server = InferenceServer {
        name: req.name.clone(),
        base_url: req.base_url.trim().trim_end_matches('/').to_string(),
        has_key: key.is_some(),
        api_key_env: None,
        enabled: req.enabled,
        timeout_secs: previous.as_ref().and_then(|s| s.timeout_secs),
        extra_params: previous.and_then(|s| s.extra_params),
    };
    validate_server_name(&server.name).map_err(map_backend_error)?;
    validate_address(&server.base_url).map_err(map_backend_error)?;
    if key.is_some() || had_key {
        let store = store().map_err(|e| {
            api_err(
                StatusCode::SERVICE_UNAVAILABLE,
                format!("secret store unavailable: {e}"),
            )
        })?;
        let written = match key {
            Some(key) => store.set(INFERENCE_SERVER_SECRET_SERVICE, &server.name, key),
            None => store.delete(INFERENCE_SERVER_SECRET_SERVICE, &server.name),
        };
        written.map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, e))?;
    }
    repo.save_server(&server).map_err(map_backend_error)?;
    Ok(server)
}

/// Apply `req` to the saved server `name`: a field it leaves out keeps what
/// the server has. A new key goes to the store `store` opens, a removed one
/// leaves it; the store is opened for nothing else.
pub(crate) fn update_server<F>(
    repo: &LlmBackendRepository,
    name: &str,
    req: UpdateServerRequest,
    store: F,
) -> Result<InferenceServer, ApiError>
where
    F: FnOnce() -> Result<Box<dyn SecretStore>, String>,
{
    let mut server = repo
        .find_server(name)
        .map_err(map_backend_error)?
        .ok_or_else(|| api_err(StatusCode::NOT_FOUND, format!("server '{name}' not found")))?;
    if let Some(base_url) = req.base_url {
        server.base_url = base_url.trim().trim_end_matches('/').to_string();
        validate_address(&server.base_url).map_err(map_backend_error)?;
    }
    if let Some(enabled) = req.enabled {
        server.enabled = enabled;
    }
    if let Some(timeout_secs) = req.timeout_secs {
        if let Some(secs) = timeout_secs {
            validate_server_timeout(secs).map_err(map_backend_error)?;
        }
        server.timeout_secs = timeout_secs;
    }
    if let Some(extra_params) = req.extra_params {
        let extra_params = extra_params.filter(|params| !params.is_empty());
        if let Some(params) = &extra_params {
            validate_extra_params(params).map_err(map_backend_error)?;
        }
        server.extra_params = extra_params;
    }
    if let Some(api_key) = req.api_key {
        let key = api_key
            .as_deref()
            .map(str::trim)
            .filter(|key| !key.is_empty());
        if key.is_some() || server.has_key {
            let store = store().map_err(|e| {
                api_err(
                    StatusCode::SERVICE_UNAVAILABLE,
                    format!("secret store unavailable: {e}"),
                )
            })?;
            let written = match key {
                Some(key) => store.set(INFERENCE_SERVER_SECRET_SERVICE, name, key),
                None => store.delete(INFERENCE_SERVER_SECRET_SERVICE, name),
            };
            written.map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, e))?;
        }
        server.has_key = key.is_some();
        server.api_key_env = None;
    }
    repo.save_server(&server).map_err(map_backend_error)?;
    Ok(server)
}

/// Delete server `name` and the key it had.
pub(crate) fn remove_server<F>(
    repo: &LlmBackendRepository,
    name: &str,
    store: F,
) -> Result<(), ApiError>
where
    F: FnOnce() -> Result<Box<dyn SecretStore>, String>,
{
    let server = repo
        .find_server(name)
        .map_err(map_backend_error)?
        .ok_or_else(|| api_err(StatusCode::NOT_FOUND, format!("server '{name}' not found")))?;
    if server.has_key {
        let store = store().map_err(|e| {
            api_err(
                StatusCode::SERVICE_UNAVAILABLE,
                format!("secret store unavailable: {e}"),
            )
        })?;
        store
            .delete(INFERENCE_SERVER_SECRET_SERVICE, name)
            .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, e))?;
    }
    repo.delete_server(name).map_err(map_backend_error)
}

/// Re-attach the declared servers to the live router, so a server saved or
/// deleted is routable or gone at once, without a full reload.
async fn refresh_router_servers<B: ExecutionBackend + Clone>(state: &AppState<B>) {
    let Some(repo) = state.llm_backend_repo.as_ref() else {
        return;
    };
    let endpoints = match repo.lock() {
        Ok(guard) => server_endpoints(
            &guard,
            || (state.secret_store)(),
            &state.inference_governance,
        ),
        Err(_) => return,
    };
    let refreshed = {
        let mut cell = state.llm_router.write().await;
        let Some(router) = cell.as_ref() else {
            return;
        };
        let refreshed = Arc::new(router.as_ref().clone().with_servers(endpoints));
        *cell = Some(refreshed.clone());
        refreshed
    };
    if let Some(chat) = state.chat_manager.as_ref() {
        chat.reload_llm(Some(refreshed)).await;
    }
}

/// `POST /api/v1/llm/servers/test`: list the models of a server that is not
/// saved, to check the address and the key before saving. Writes nothing.
#[utoipa::path(
    post,
    path = "/api/v1/llm/servers/test",
    tag = "llm",
    request_body = TestServerRequest,
    responses(
        (status = 200, description = "The server answered with its models", body = TestServerResponse),
        (status = 403, description = "The organisation policy refuses this server; nothing was sent to it, `kind` is `policy`", body = ServerProbeErrorResponse),
        (status = 422, description = "The address is not an HTTP URL", body = ServerProbeErrorResponse),
        (status = 502, description = "Unreachable, key refused, or not an OpenAI-compatible server; `kind` says which", body = ServerProbeErrorResponse),
    )
)]
pub async fn test_llm_server<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Json(req): Json<TestServerRequest>,
) -> Result<Json<TestServerResponse>, (StatusCode, Json<ServerProbeErrorResponse>)> {
    validate_address(&req.base_url)
        .map_err(map_backend_error)
        .map_err(as_probe)?;
    state
        .inference_governance
        .server_decision(&req.base_url)
        .map_err(|reason| {
            (
                StatusCode::FORBIDDEN,
                Json(ServerProbeErrorResponse {
                    error: reason,
                    kind: "policy".into(),
                }),
            )
        })?;
    let models = list_server_models(&req.base_url, req.api_key.as_deref())
        .await
        .map_err(probe_error)?;
    Ok(Json(TestServerResponse { models }))
}

/// `GET /api/v1/llm/servers`: every declared server, without its key.
#[utoipa::path(
    get,
    path = "/api/v1/llm/servers",
    tag = "llm",
    responses(
        (status = 200, description = "Declared servers", body = ServersListResponse),
        (status = 503, description = "Backend repository not available", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub async fn list_llm_servers<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
) -> Result<Json<ServersListResponse>, ApiError> {
    let servers = with_repo(&state, |repo| {
        repo.list_servers().map_err(map_backend_error)
    })?;
    Ok(Json(ServersListResponse {
        servers: servers.into_iter().map(ServerResponse::from).collect(),
    }))
}

/// `POST /api/v1/llm/servers`: declare (or replace) a server. The key goes to
/// the secret store; the server's models become routable at once.
#[utoipa::path(
    post,
    path = "/api/v1/llm/servers",
    tag = "llm",
    request_body = CreateServerRequest,
    responses(
        (status = 201, description = "Server saved", body = ServerResponse),
        (status = 403, description = "The organisation policy refuses this server; nothing was saved", body = crate::api::openapi::ApiErrorBody),
        (status = 422, description = "Invalid name or address", body = crate::api::openapi::ApiErrorBody),
        (status = 503, description = "Repository or secret store not available", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub async fn create_llm_server<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Json(req): Json<CreateServerRequest>,
) -> Result<(StatusCode, Json<ServerResponse>), ApiError> {
    if let Err(reason) = state.inference_governance.server_decision(&req.base_url) {
        tracing::event!(
            tracing::Level::WARN,
            server = %req.name,
            reason = %reason,
            "llm.server.refused_by_policy"
        );
        return Err(api_err(StatusCode::FORBIDDEN, reason));
    }
    let server = with_repo(&state, |repo| {
        save_server(repo, &req, || (state.secret_store)())
    })?;
    tracing::event!(
        tracing::Level::INFO,
        server = %server.name,
        has_key = server.has_key,
        "llm.server.saved"
    );
    refresh_router_servers(&state).await;
    Ok((StatusCode::CREATED, Json(server.into())))
}

/// `PATCH /api/v1/llm/servers/{name}`: change a saved server field by field.
/// The change applies to the next request sent to one of its models.
#[utoipa::path(
    patch,
    path = "/api/v1/llm/servers/{name}",
    tag = "llm",
    params(("name" = String, Path, description = "Server name")),
    request_body = UpdateServerRequest,
    responses(
        (status = 200, description = "Server updated", body = ServerResponse),
        (status = 403, description = "The organisation policy refuses the new address; nothing was changed", body = crate::api::openapi::ApiErrorBody),
        (status = 404, description = "Server not found", body = crate::api::openapi::ApiErrorBody),
        (status = 422, description = "Invalid address, timeout out of bounds, or a reserved field among the extra parameters", body = crate::api::openapi::ApiErrorBody),
        (status = 503, description = "Repository or secret store not available", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub async fn update_llm_server<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path(name): Path<String>,
    Json(req): Json<UpdateServerRequest>,
) -> Result<Json<ServerResponse>, ApiError> {
    if let Some(base_url) = req.base_url.as_deref() {
        if let Err(reason) = state.inference_governance.server_decision(base_url) {
            tracing::event!(
                tracing::Level::WARN,
                server = %name,
                reason = %reason,
                "llm.server.refused_by_policy"
            );
            return Err(api_err(StatusCode::FORBIDDEN, reason));
        }
    }
    let server = with_repo(&state, |repo| {
        update_server(repo, &name, req, || (state.secret_store)())
    })?;
    tracing::event!(
        tracing::Level::INFO,
        server = %server.name,
        enabled = server.enabled,
        has_key = server.has_key,
        "llm.server.updated"
    );
    refresh_router_servers(&state).await;
    Ok(Json(server.into()))
}

/// `DELETE /api/v1/llm/servers/{name}`: forget a server and its key.
#[utoipa::path(
    delete,
    path = "/api/v1/llm/servers/{name}",
    tag = "llm",
    params(("name" = String, Path, description = "Server name")),
    responses(
        (status = 200, description = "Server deleted", body = DeleteServerResponse),
        (status = 404, description = "Server not found", body = crate::api::openapi::ApiErrorBody),
        (status = 503, description = "Repository or secret store not available", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub async fn delete_llm_server<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path(name): Path<String>,
) -> Result<Json<DeleteServerResponse>, ApiError> {
    with_repo(&state, |repo| {
        remove_server(repo, &name, || (state.secret_store)())
    })?;
    tracing::event!(tracing::Level::INFO, server = %name, "llm.server.deleted");
    refresh_router_servers(&state).await;
    Ok(Json(DeleteServerResponse { deleted: name }))
}

/// `GET /api/v1/llm/servers/{name}/models`: the models of a declared server,
/// each with the name it routes by.
#[utoipa::path(
    get,
    path = "/api/v1/llm/servers/{name}/models",
    tag = "llm",
    params(("name" = String, Path, description = "Server name")),
    responses(
        (status = 200, description = "The server's models", body = ServerModelsResponse),
        (status = 404, description = "Server not found", body = ServerProbeErrorResponse),
        (status = 502, description = "Unreachable, key refused, or not an OpenAI-compatible server; `kind` says which", body = ServerProbeErrorResponse),
        (status = 503, description = "Repository or key not available", body = ServerProbeErrorResponse),
    )
)]
pub async fn list_llm_server_models<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path(name): Path<String>,
) -> Result<Json<ServerModelsResponse>, (StatusCode, Json<ServerProbeErrorResponse>)> {
    let server = with_repo(&state, |repo| {
        repo.find_server(&name)
            .map_err(map_backend_error)?
            .ok_or_else(|| api_err(StatusCode::NOT_FOUND, format!("server '{name}' not found")))
    })
    .map_err(as_probe)?;
    let key = resolve_server_key(&server, || (state.secret_store)())
        .map_err(|e| as_probe(api_err(StatusCode::SERVICE_UNAVAILABLE, e)))?;
    let mut listed = list_server_model_entries(&server.base_url, key.as_deref())
        .await
        .map_err(probe_error)?;
    fill_windows(&server.base_url, key.as_deref(), &mut listed).await;
    Ok(Json(ServerModelsResponse {
        models: listed
            .into_iter()
            .map(|m| ServerModel {
                route: route_name(&server.name, &m.id),
                context_short: m
                    .context_window
                    .is_some_and(|w| context_short(&server.name, &m.id, w)),
                context_window: m.context_window,
                id: m.id,
            })
            .collect(),
        server: server.name,
        expected_context_window: EXPECTED_CONTEXT_WINDOW,
    }))
}

/// The server routes, merged into [`super::llm_routes`].
pub fn server_routes<B: ExecutionBackend + Clone>() -> Router<AppState<B>> {
    Router::new()
        .route(
            "/api/v1/llm/servers",
            get(list_llm_servers::<B>).post(create_llm_server::<B>),
        )
        .route("/api/v1/llm/servers/test", post(test_llm_server::<B>))
        .route(
            "/api/v1/llm/servers/:name",
            patch(update_llm_server::<B>).delete(delete_llm_server::<B>),
        )
        .route(
            "/api/v1/llm/servers/:name/models",
            get(list_llm_server_models::<B>),
        )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::routes_llm::llm_routes;
    use crate::api::routes_llm::tests::{test_app_state_no_llm, MockBackend};
    use apollia_auth::AgeFileSecretStore;
    use axum::body::Body;
    use axum::http::Request;
    use std::sync::Mutex;
    use tower::ServiceExt;

    /// One request a fake server received: path, `Authorization`, JSON body.
    type Received = (String, Option<String>, serde_json::Value);

    /// A local OpenAI-compatible server that answers `/v1/models` with
    /// `models_status` and `models_body`, answers every chat completion, and
    /// records what it received.
    struct FakeServer {
        url: String,
        received: Arc<Mutex<Vec<Received>>>,
    }

    async fn fake_server(models_status: u16, models_body: serde_json::Value) -> FakeServer {
        let received: Arc<Mutex<Vec<Received>>> = Arc::default();
        let record = |received: Arc<Mutex<Vec<Received>>>| {
            move |path: &str, headers: &axum::http::HeaderMap, body: serde_json::Value| {
                let auth = headers
                    .get("authorization")
                    .and_then(|v| v.to_str().ok())
                    .map(str::to_string);
                received
                    .lock()
                    .unwrap()
                    .push((path.to_string(), auth, body));
            }
        };
        let on_models = record(received.clone());
        let on_chat = record(received.clone());
        let app = Router::new()
            .route(
                "/v1/models",
                get(move |headers: axum::http::HeaderMap| {
                    on_models("/v1/models", &headers, serde_json::Value::Null);
                    let status = StatusCode::from_u16(models_status).unwrap();
                    let body = models_body.clone();
                    async move { (status, Json(body)) }
                }),
            )
            .route(
                "/v1/chat/completions",
                post(
                    move |headers: axum::http::HeaderMap, Json(body): Json<serde_json::Value>| {
                        let model = body["model"].clone();
                        on_chat("/v1/chat/completions", &headers, body);
                        async move {
                            Json(serde_json::json!({
                                "id": "chatcmpl-1",
                                "object": "chat.completion",
                                "created": 1,
                                "model": model,
                                "choices": [{
                                    "index": 0,
                                    "message": { "role": "assistant", "content": "pong" },
                                    "finish_reason": "stop"
                                }],
                                "usage": { "prompt_tokens": 1, "completion_tokens": 1, "total_tokens": 2 }
                            }))
                        }
                    },
                ),
            );
        FakeServer {
            url: serve(app).await,
            received,
        }
    }

    /// Serve `app` on a loopback port the system picks, and return its address.
    async fn serve(app: Router) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        url
    }

    fn two_models() -> serde_json::Value {
        serde_json::json!({
            "object": "list",
            "data": [
                { "id": "qwen3-32b", "object": "model" },
                { "id": "llama-3.3-70b", "object": "model" }
            ]
        })
    }

    async fn call(
        state: AppState<MockBackend>,
        method: &str,
        uri: &str,
        body: Option<serde_json::Value>,
    ) -> (StatusCode, serde_json::Value) {
        let app = llm_routes::<MockBackend>().with_state(state);
        let req = Request::builder()
            .method(method)
            .uri(uri)
            .header("content-type", "application/json")
            .body(match body {
                Some(b) => Body::from(serde_json::to_vec(&b).unwrap()),
                None => Body::empty(),
            })
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        let status = resp.status();
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null),
        )
    }

    fn state_with_repo(dir: &tempfile::TempDir) -> AppState<MockBackend> {
        let repo = LlmBackendRepository::open(&dir.path().join("system.db")).unwrap();
        let mut state = test_app_state_no_llm();
        state.llm_backend_repo = Some(Arc::new(std::sync::Mutex::new(repo)));
        state
    }

    fn file_store(dir: &tempfile::TempDir) -> Box<dyn SecretStore> {
        Box::new(
            AgeFileSecretStore::new(dir.path().join("secrets"), "test-passphrase".into()).unwrap(),
        )
    }

    // GIVEN a local server listing two models, and a runtime with an empty repository
    // WHEN the server is tested before being saved
    // THEN 200 with the two identifiers, and no server was written
    #[tokio::test]
    async fn test_testing_a_server_lists_its_models_and_writes_nothing() {
        // GIVEN
        let server = fake_server(200, two_models()).await;
        let dir = tempfile::tempdir().unwrap();
        let state = state_with_repo(&dir);

        // WHEN
        let (status, body) = call(
            state.clone(),
            "POST",
            "/api/v1/llm/servers/test",
            Some(serde_json::json!({ "base_url": server.url })),
        )
        .await;

        // THEN
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(
            body["models"],
            serde_json::json!(["qwen3-32b", "llama-3.3-70b"])
        );
        let repo = state.llm_backend_repo.as_ref().unwrap().lock().unwrap();
        assert!(repo.list_servers().unwrap().is_empty());
    }

    // GIVEN an address where nothing can listen
    // WHEN it is tested
    // THEN 502 with kind `unreachable`
    #[tokio::test]
    async fn test_testing_an_address_where_nothing_listens_says_unreachable() {
        // GIVEN
        // Port 0 is never a destination: the connection fails on every
        // machine, with no port reserved and released for the occasion.
        let url = "http://127.0.0.1:0".to_string();

        // WHEN
        let (status, body) = call(
            test_app_state_no_llm(),
            "POST",
            "/api/v1/llm/servers/test",
            Some(serde_json::json!({ "base_url": url })),
        )
        .await;

        // THEN
        assert_eq!(status, StatusCode::BAD_GATEWAY);
        assert_eq!(body["kind"], "unreachable", "{body}");
    }

    // GIVEN a server answering 401, and one answering 403
    // WHEN each is tested with a key
    // THEN 502 with kind `key_refused`
    #[tokio::test]
    async fn test_testing_a_server_that_refuses_the_key_says_key_refused() {
        for code in [401u16, 403] {
            // GIVEN
            let server = fake_server(code, serde_json::json!({"error": "bad key"})).await;

            // WHEN
            let (status, body) = call(
                test_app_state_no_llm(),
                "POST",
                "/api/v1/llm/servers/test",
                Some(serde_json::json!({ "base_url": server.url, "api_key": "wrong" })),
            )
            .await;

            // THEN
            assert_eq!(status, StatusCode::BAD_GATEWAY);
            assert_eq!(body["kind"], "key_refused", "{body}");
        }
    }

    // GIVEN a server whose `/v1/models` answers something that is not a model list
    // WHEN it is tested
    // THEN 502 with kind `not_openai_compatible`
    #[tokio::test]
    async fn test_testing_a_foreign_server_says_not_openai_compatible() {
        // GIVEN
        let server = fake_server(200, serde_json::json!({"status": "ok"})).await;

        // WHEN
        let (status, body) = call(
            test_app_state_no_llm(),
            "POST",
            "/api/v1/llm/servers/test",
            Some(serde_json::json!({ "base_url": server.url })),
        )
        .await;

        // THEN
        assert_eq!(status, StatusCode::BAD_GATEWAY);
        assert_eq!(body["kind"], "not_openai_compatible", "{body}");
    }

    // GIVEN a local server, called once without a key and once with one
    // WHEN both tests run
    // THEN the first request carries no Authorization header, the second `Bearer <key>`
    #[tokio::test]
    async fn test_testing_sends_authorization_only_with_a_key() {
        // GIVEN
        let server = fake_server(200, two_models()).await;

        // WHEN
        for body in [
            serde_json::json!({ "base_url": server.url }),
            serde_json::json!({ "base_url": server.url, "api_key": "sk-corp" }),
        ] {
            let (status, _) = call(
                test_app_state_no_llm(),
                "POST",
                "/api/v1/llm/servers/test",
                Some(body),
            )
            .await;
            assert_eq!(status, StatusCode::OK);
        }

        // THEN
        let auth: Vec<Option<String>> = server
            .received
            .lock()
            .unwrap()
            .iter()
            .map(|r| r.1.clone())
            .collect();
        assert_eq!(auth, vec![None, Some("Bearer sk-corp".to_string())]);
    }

    // GIVEN a runtime with a live router and a local server with two models
    // WHEN the server is saved (no key), its models are listed, and a chat is
    //      addressed to `<server>/<model>`
    // THEN the server is one row, each model comes with its route, and the
    //      completion reaches the server with `model` equal to the identifier
    #[tokio::test]
    async fn test_a_saved_server_lists_its_models_and_each_one_routes() {
        // GIVEN
        let server = fake_server(200, two_models()).await;
        let dir = tempfile::tempdir().unwrap();
        let state = state_with_repo(&dir);
        *state.llm_router.write().await = Some(Arc::new(apollia_llm::LlmRouter::empty()));

        // WHEN
        let (created, _) = call(
            state.clone(),
            "POST",
            "/api/v1/llm/servers",
            Some(serde_json::json!({ "name": "corp", "base_url": server.url })),
        )
        .await;
        let (listed, models) = call(
            state.clone(),
            "GET",
            "/api/v1/llm/servers/corp/models",
            None,
        )
        .await;
        let (chatted, chat) = call(
            state.clone(),
            "POST",
            "/api/v1/llm/chat",
            Some(serde_json::json!({ "prompt": "bonjour", "backend": "corp/llama-3.3-70b" })),
        )
        .await;

        // THEN
        assert_eq!(created, StatusCode::CREATED);
        assert_eq!(listed, StatusCode::OK, "{models}");
        assert_eq!(
            models["models"],
            serde_json::json!([
                {
                    "id": "qwen3-32b",
                    "route": "corp/qwen3-32b",
                    "context_window": null,
                    "context_short": false
                },
                {
                    "id": "llama-3.3-70b",
                    "route": "corp/llama-3.3-70b",
                    "context_window": null,
                    "context_short": false
                }
            ])
        );
        assert_eq!(chatted, StatusCode::OK, "{chat}");
        let completions: Vec<serde_json::Value> = server
            .received
            .lock()
            .unwrap()
            .iter()
            .filter(|r| r.0 == "/v1/chat/completions")
            .map(|r| r.2["model"].clone())
            .collect();
        assert_eq!(completions, vec![serde_json::json!("llama-3.3-70b")]);
        let repo = state.llm_backend_repo.as_ref().unwrap().lock().unwrap();
        assert_eq!(repo.list_servers().unwrap().len(), 1);
        assert!(repo.list().unwrap().is_empty(), "no backend row per model");
    }

    // GIVEN a saved Ollama server serving one model at 8192 tokens (its
    //       `/api/show` parameters) and one at its trained 131072
    // WHEN its models are listed
    // THEN each model carries the window read on the server, and only the
    //      first is flagged short
    #[tokio::test]
    async fn test_the_model_list_carries_each_window_and_flags_a_short_one() {
        // GIVEN
        let app = Router::new()
            .route(
                "/v1/models",
                get(|| async {
                    Json(serde_json::json!({
                        "object": "list",
                        "data": [
                            { "id": "qwen3:8b", "object": "model", "owned_by": "library" },
                            { "id": "qwen3:32b", "object": "model", "owned_by": "library" }
                        ]
                    }))
                }),
            )
            .route(
                "/api/show",
                post(|Json(body): Json<serde_json::Value>| async move {
                    Json(if body["model"] == "qwen3:8b" {
                        serde_json::json!({
                            "parameters": "num_ctx 8192",
                            "model_info": { "qwen3.context_length": 40_960 }
                        })
                    } else {
                        serde_json::json!({ "model_info": { "qwen3.context_length": 131_072 } })
                    })
                }),
            );
        let url = format!("{}/v1", serve(app).await);
        let dir = tempfile::tempdir().unwrap();
        let state = state_with_repo(&dir);
        let (created, _) = call(
            state.clone(),
            "POST",
            "/api/v1/llm/servers",
            Some(serde_json::json!({ "name": "ollama", "base_url": url })),
        )
        .await;

        // WHEN
        let (listed, models) = call(state, "GET", "/api/v1/llm/servers/ollama/models", None).await;

        // THEN
        assert_eq!(created, StatusCode::CREATED);
        assert_eq!(listed, StatusCode::OK, "{models}");
        let windows: Vec<(serde_json::Value, serde_json::Value)> = models["models"]
            .as_array()
            .unwrap()
            .iter()
            .map(|m| (m["context_window"].clone(), m["context_short"].clone()))
            .collect();
        assert_eq!(
            windows,
            vec![
                (serde_json::json!(8192), serde_json::json!(true)),
                (serde_json::json!(131_072), serde_json::json!(false)),
            ]
        );
        assert_eq!(models["expected_context_window"], EXPECTED_CONTEXT_WINDOW);
    }

    // GIVEN a server saved with a key, through an encrypted-file secret store
    // WHEN it is saved, then removed
    // THEN the key sits in the store and nowhere in `system.db`, and removing
    //      the server removes the key
    #[test]
    fn test_a_server_key_goes_to_the_secret_store_never_to_the_database() {
        // GIVEN
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("system.db");
        let repo = LlmBackendRepository::open(&db).unwrap();
        let req = CreateServerRequest {
            name: "corp".into(),
            base_url: "http://10.0.0.5:8000/v1".into(),
            api_key: Some("sk-very-secret".into()),
            enabled: true,
        };

        // WHEN
        let saved = save_server(&repo, &req, || Ok(file_store(&dir))).unwrap();

        // THEN
        assert!(saved.has_key);
        assert_eq!(
            file_store(&dir)
                .get(INFERENCE_SERVER_SECRET_SERVICE, "corp")
                .unwrap()
                .as_deref(),
            Some("sk-very-secret")
        );
        drop(repo);
        let bytes = std::fs::read(&db).unwrap();
        let text = String::from_utf8_lossy(&bytes);
        assert!(
            !text.contains("sk-very-secret"),
            "the key must not be in system.db"
        );

        // WHEN
        let repo = LlmBackendRepository::open(&db).unwrap();
        remove_server(&repo, "corp", || Ok(file_store(&dir))).unwrap();

        // THEN
        assert!(file_store(&dir)
            .get(INFERENCE_SERVER_SECRET_SERVICE, "corp")
            .unwrap()
            .is_none());
        assert!(repo.find_server("corp").unwrap().is_none());
    }

    fn update(body: serde_json::Value) -> UpdateServerRequest {
        serde_json::from_value(body).unwrap()
    }

    // GIVEN a server saved with a key
    // WHEN its address alone is changed, then its key removed with `null`
    // THEN the first change keeps the key, the second removes it from the store
    #[test]
    fn test_updating_a_server_keeps_what_the_request_leaves_out() {
        // GIVEN
        let dir = tempfile::tempdir().unwrap();
        let repo = LlmBackendRepository::open(&dir.path().join("system.db")).unwrap();
        let req = CreateServerRequest {
            name: "corp".into(),
            base_url: "http://10.0.0.5:8000/v1".into(),
            api_key: Some("sk-very-secret".into()),
            enabled: true,
        };
        save_server(&repo, &req, || Ok(file_store(&dir))).unwrap();
        let stored = || {
            file_store(&dir)
                .get(INFERENCE_SERVER_SECRET_SERVICE, "corp")
                .unwrap()
        };

        // WHEN
        let moved = update_server(
            &repo,
            "corp",
            update(serde_json::json!({ "base_url": "http://10.0.0.6:8000/" })),
            || Err("the store must not be opened".into()),
        )
        .unwrap();
        let key_after_move = stored();
        let unkeyed = update_server(
            &repo,
            "corp",
            update(serde_json::json!({ "api_key": null })),
            || Ok(file_store(&dir)),
        )
        .unwrap();

        // THEN
        assert_eq!(moved.base_url, "http://10.0.0.6:8000");
        assert!(moved.has_key && moved.enabled);
        assert_eq!(key_after_move.as_deref(), Some("sk-very-secret"));
        assert!(!unkeyed.has_key);
        assert_eq!(unkeyed.base_url, "http://10.0.0.6:8000");
        assert!(stored().is_none());
    }

    // GIVEN a saved server
    // WHEN it is disabled and given a timeout and an extra parameter, then
    //      saved again by the declaring route, then both settings emptied
    // THEN the settings are answered and stored, survive the second save,
    //      and are gone once emptied
    #[tokio::test]
    async fn test_patching_a_server_disables_it_and_stores_its_settings() {
        // GIVEN
        let dir = tempfile::tempdir().unwrap();
        let state = state_with_repo(&dir);
        let declared = serde_json::json!({ "name": "corp", "base_url": "http://10.0.0.5:8000" });
        let (status, _) = call(
            state.clone(),
            "POST",
            "/api/v1/llm/servers",
            Some(declared.clone()),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED);

        // WHEN
        let (status, body) = call(
            state.clone(),
            "PATCH",
            "/api/v1/llm/servers/corp",
            Some(serde_json::json!({
                "enabled": false,
                "timeout_secs": 120,
                "extra_params": { "keep_alive": "30m" }
            })),
        )
        .await;
        let (_, saved_again) =
            call(state.clone(), "POST", "/api/v1/llm/servers", Some(declared)).await;
        let (_, emptied) = call(
            state.clone(),
            "PATCH",
            "/api/v1/llm/servers/corp",
            Some(serde_json::json!({ "timeout_secs": null, "extra_params": null })),
        )
        .await;

        // THEN
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(body["enabled"], false);
        assert_eq!(body["timeout_secs"], 120);
        assert_eq!(
            body["extra_params"],
            serde_json::json!({ "keep_alive": "30m" })
        );
        assert_eq!(saved_again["timeout_secs"], 120);
        assert_eq!(
            saved_again["extra_params"],
            serde_json::json!({ "keep_alive": "30m" })
        );
        assert!(emptied["timeout_secs"].is_null(), "{emptied}");
        assert!(emptied["extra_params"].is_null(), "{emptied}");
        let (_, listed) = call(state, "GET", "/api/v1/llm/servers", None).await;
        assert!(listed["servers"][0]["timeout_secs"].is_null());
    }

    // GIVEN a saved server
    // WHEN it is patched with a timeout out of bounds, with a reserved extra
    //      parameter, with an address that is not a URL, and under a name
    //      nothing was saved with
    // THEN 422 three times and 404 once, and the server is unchanged
    #[tokio::test]
    async fn test_patching_a_server_refuses_what_cannot_be_stored() {
        // GIVEN
        let dir = tempfile::tempdir().unwrap();
        let state = state_with_repo(&dir);
        call(
            state.clone(),
            "POST",
            "/api/v1/llm/servers",
            Some(serde_json::json!({ "name": "corp", "base_url": "http://10.0.0.5:8000" })),
        )
        .await;

        // WHEN
        let mut statuses = Vec::new();
        for body in [
            serde_json::json!({ "timeout_secs": 5 }),
            serde_json::json!({ "extra_params": { "messages": [] } }),
            serde_json::json!({ "base_url": "10.0.0.5:8000" }),
        ] {
            let (status, _) = call(
                state.clone(),
                "PATCH",
                "/api/v1/llm/servers/corp",
                Some(body),
            )
            .await;
            statuses.push(status);
        }
        let (unknown, _) = call(
            state.clone(),
            "PATCH",
            "/api/v1/llm/servers/absent",
            Some(serde_json::json!({ "enabled": false })),
        )
        .await;

        // THEN
        assert_eq!(statuses, vec![StatusCode::UNPROCESSABLE_ENTITY; 3]);
        assert_eq!(unknown, StatusCode::NOT_FOUND);
        let (_, listed) = call(state, "GET", "/api/v1/llm/servers", None).await;
        assert_eq!(listed["servers"][0]["base_url"], "http://10.0.0.5:8000");
        assert_eq!(listed["servers"][0]["enabled"], true);
        assert!(listed["servers"][0]["timeout_secs"].is_null());
    }

    // GIVEN a router reaching a local OpenAI-compatible server that records
    //       what it receives
    // WHEN the diagnostic ping is sent to one of its models
    // THEN the backend answers and the completion asked for one token only
    #[tokio::test]
    async fn test_ping_asks_for_a_single_token() {
        // GIVEN
        let server = fake_server(200, two_models()).await;
        let state = test_app_state_no_llm();
        *state.llm_router.write().await = Some(Arc::new(
            apollia_llm::LlmRouter::empty().with_servers(vec![apollia_llm::ServerEndpoint {
                name: "corp".into(),
                base_url: server.url.clone(),
                api_key: None,
                refusal: None,
                idle_timeout: None,
                extra_params: None,
            }]),
        ));
        let app_router = llm_routes::<MockBackend>().with_state(state);
        let req = Request::builder()
            .method("POST")
            .uri("/api/v1/llm/ping")
            .header("content-type", "application/json")
            .body(Body::from(r#"{"backend":"corp/qwen3-32b"}"#))
            .unwrap();

        // WHEN
        let resp = app_router.oneshot(req).await.unwrap();

        // THEN
        assert_eq!(resp.status(), StatusCode::OK);
        let received = server.received.lock().unwrap();
        let ping = received
            .iter()
            .find(|r| r.0 == "/v1/chat/completions")
            .expect("the ping reached the server");
        assert_eq!(ping.2["max_tokens"], serde_json::json!(1), "{}", ping.2);
    }

    fn governed(inference: &str) -> crate::governance::InferenceGovernance {
        crate::governance::InferenceGovernance::new(
            crate::governance::inference_policy::valid_verdict_with_inference(inference),
        )
    }

    // GIVEN a policy that allows one server of the organisation
    // WHEN a server at another address is added
    // THEN 403 naming the policy, and nothing is saved
    #[tokio::test]
    async fn test_adding_a_server_the_policy_does_not_list_is_refused() {
        // GIVEN
        let dir = tempfile::tempdir().unwrap();
        let mut state = state_with_repo(&dir);
        state.inference_governance =
            governed("allowed_servers = [\"http://gpu.corp.example:8000\"]");

        // WHEN
        let (status, body) = call(
            state.clone(),
            "POST",
            "/api/v1/llm/servers",
            Some(serde_json::json!({ "name": "other", "base_url": "http://10.0.0.9:8000" })),
        )
        .await;

        // THEN
        assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
        assert!(
            body["error"]
                .as_str()
                .is_some_and(|e| e.contains("organisation policy")),
            "{body}"
        );
        let (_, listed) = call(state, "GET", "/api/v1/llm/servers", None).await;
        assert_eq!(listed["servers"], serde_json::json!([]));
    }

    // GIVEN a policy refusing remote endpoints, listing no server
    // WHEN a server off this machine is added, then one on this machine
    // THEN the first is refused by the policy, the second is saved
    #[tokio::test]
    async fn test_remote_deny_refuses_a_server_off_this_machine() {
        // GIVEN
        let dir = tempfile::tempdir().unwrap();
        let mut state = state_with_repo(&dir);
        state.inference_governance = governed("remote_endpoints = \"deny\"");

        // WHEN
        let (remote, body) = call(
            state.clone(),
            "POST",
            "/api/v1/llm/servers",
            Some(serde_json::json!({ "name": "cloud", "base_url": "https://api.example.com/v1" })),
        )
        .await;
        let (local, _) = call(
            state,
            "POST",
            "/api/v1/llm/servers",
            Some(serde_json::json!({ "name": "ollama", "base_url": "http://127.0.0.1:11434" })),
        )
        .await;

        // THEN
        assert_eq!(remote, StatusCode::FORBIDDEN, "{body}");
        assert_eq!(
            body["error"],
            crate::governance::REMOTE_SERVER_DENIED_REASON,
            "{body}"
        );
        assert_eq!(local, StatusCode::CREATED);
    }

    // GIVEN a local server that counts what it receives, and a policy that
    //       does not list it
    // WHEN it is tested before being saved
    // THEN 403 with kind `policy`, and the server received nothing
    #[tokio::test]
    async fn test_testing_a_server_the_policy_refuses_sends_nothing() {
        // GIVEN
        let server = fake_server(200, two_models()).await;
        let mut state = test_app_state_no_llm();
        state.inference_governance =
            governed("allowed_servers = [\"http://gpu.corp.example:8000\"]");

        // WHEN
        let (status, body) = call(
            state,
            "POST",
            "/api/v1/llm/servers/test",
            Some(serde_json::json!({ "base_url": server.url })),
        )
        .await;

        // THEN
        assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
        assert_eq!(body["kind"], "policy");
        assert!(server.received.lock().unwrap().is_empty());
    }

    // GIVEN a server saved while no policy applied, that counts the requests
    //       it receives, then a policy that does not list it
    // WHEN the router is built under that policy and a request is sent to one
    //      of its models
    // THEN the request fails with the policy's reason and the server received
    //      zero requests
    #[tokio::test]
    async fn test_a_request_to_a_saved_server_now_forbidden_never_leaves() {
        // GIVEN
        let server = fake_server(200, two_models()).await;
        let dir = tempfile::tempdir().unwrap();
        let repo = LlmBackendRepository::open(&dir.path().join("system.db")).unwrap();
        let req = CreateServerRequest {
            name: "corp".into(),
            base_url: server.url.clone(),
            api_key: None,
            enabled: true,
        };
        save_server(&repo, &req, || Ok(file_store(&dir))).unwrap();
        let governance = governed("allowed_servers = [\"http://gpu.corp.example:8000\"]");

        // WHEN
        let endpoints = server_endpoints(&repo, || Ok(file_store(&dir)), &governance);
        let router = apollia_llm::LlmRouter::empty().with_servers(endpoints);
        let backend = router
            .get(Some("corp/qwen3-32b"))
            .expect("the route is known");
        let outcome = backend
            .complete(apollia_llm::CompletionRequest {
                messages: vec![apollia_llm::ChatMessage::user("bonjour")],
                ..Default::default()
            })
            .await;

        // THEN
        assert!(
            matches!(
                outcome,
                Err(apollia_llm::LlmError::BackendUnavailable { ref reason, .. })
                    if reason == crate::governance::SERVER_NOT_ALLOWED_REASON
            ),
            "{outcome:?}"
        );
        assert_eq!(server.received.lock().unwrap().len(), 0);
    }
}
