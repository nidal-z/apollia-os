//! Inference servers, `/api/v1/llm/servers`.
//!
//! A server is an address that speaks the OpenAI protocol, with an optional
//! key. Before it is saved it can be tested (`POST .../servers/test`, which
//! lists its models and writes nothing); once saved, its models are listed by
//! name and each one routes as `<server>/<model>` with no backend row of its
//! own. The key goes to the secret store and is never answered back.

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use apollia_auth::SecretStore;
use apollia_core::inference_server::{
    route_name, validate_address, validate_server_name, INFERENCE_SERVER_SECRET_SERVICE,
};
use apollia_core::{InferenceServer, LlmBackendRepository};
use apollia_llm::server_models::{list_server_models, ServerModelsError};

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
}

impl From<InferenceServer> for ServerResponse {
    fn from(s: InferenceServer) -> Self {
        Self {
            name: s.name,
            base_url: s.base_url,
            has_key: s.has_key,
            api_key_env: s.api_key_env,
            enabled: s.enabled,
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
}

/// Body of `GET /api/v1/llm/servers/{name}/models`.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ServerModelsResponse {
    /// Server name.
    pub server: String,
    /// Models the server offers, in its order.
    pub models: Vec<ServerModel>,
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
/// saved again without a key loses the one it had.
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
    let had_key = repo
        .find_server(&req.name)
        .map_err(map_backend_error)?
        .is_some_and(|s| s.has_key);
    let server = InferenceServer {
        name: req.name.clone(),
        base_url: req.base_url.trim().trim_end_matches('/').to_string(),
        has_key: key.is_some(),
        api_key_env: None,
        enabled: req.enabled,
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
        Ok(guard) => server_endpoints(&guard, || (state.secret_store)()),
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
        (status = 422, description = "The address is not an HTTP URL", body = ServerProbeErrorResponse),
        (status = 502, description = "Unreachable, key refused, or not an OpenAI-compatible server; `kind` says which", body = ServerProbeErrorResponse),
    )
)]
pub async fn test_llm_server(
    Json(req): Json<TestServerRequest>,
) -> Result<Json<TestServerResponse>, (StatusCode, Json<ServerProbeErrorResponse>)> {
    validate_address(&req.base_url)
        .map_err(map_backend_error)
        .map_err(as_probe)?;
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
        (status = 422, description = "Invalid name or address", body = crate::api::openapi::ApiErrorBody),
        (status = 503, description = "Repository or secret store not available", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub async fn create_llm_server<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Json(req): Json<CreateServerRequest>,
) -> Result<(StatusCode, Json<ServerResponse>), ApiError> {
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
    let ids = list_server_models(&server.base_url, key.as_deref())
        .await
        .map_err(probe_error)?;
    Ok(Json(ServerModelsResponse {
        models: ids
            .into_iter()
            .map(|id| ServerModel {
                route: route_name(&server.name, &id),
                id,
            })
            .collect(),
        server: server.name,
    }))
}

/// The server routes, merged into [`super::llm_routes`].
pub fn server_routes<B: ExecutionBackend + Clone>() -> Router<AppState<B>> {
    Router::new()
        .route(
            "/api/v1/llm/servers",
            get(list_llm_servers::<B>).post(create_llm_server::<B>),
        )
        .route("/api/v1/llm/servers/test", post(test_llm_server))
        .route("/api/v1/llm/servers/:name", delete(delete_llm_server::<B>))
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
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        FakeServer { url, received }
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
                { "id": "qwen3-32b", "route": "corp/qwen3-32b" },
                { "id": "llama-3.3-70b", "route": "corp/llama-3.3-70b" }
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
}
