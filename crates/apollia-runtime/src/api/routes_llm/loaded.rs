//! The local models resident in memory, and unloading one of them.
//!
//! - `GET  /api/v1/llm/loaded-models`        , list the resident models
//! - `POST /api/v1/llm/loaded-models/unload` , stop one, by model path
//! - `POST /api/v1/llm/loaded-models`        , load one by hand, see [`super::load`]
//!
//! Both read every local engine: `llama-server` for GGUF models and `vllm-mlx`
//! for MLX ones, each model naming the engine that serves it. Without any
//! engine, the list answers empty with `engineAvailable: false` and an unload
//! answers 503, so a client can tell "nothing loaded" from "no local engine".

use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use crate::api::openapi::ApiErrorBody;
use crate::api::server::AppState;
use crate::coordinator::ExecutionBackend;
use crate::engine_store::Engine;
use crate::llama_server::{LoadedModel, ModelKind, UnloadError};
use crate::local_engine::LocalEngine;
use crate::residency::{Engines, LoadOrigin};

/// One resident model.
#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LoadedModelDto {
    /// The engine serving the model, chosen by its format: `llama-server` for
    /// GGUF, `vllm-mlx` for MLX.
    pub engine: Engine,
    /// What the model serves: `chat`, or `embeddings` for the instance that
    /// serves search by meaning, which counts neither against `maxLoaded` nor
    /// against the memory the guardrail weighs.
    pub kind: ModelKind,
    /// Display name, the model file name without its extension.
    pub model: String,
    /// Path the model was loaded from; the key an unload takes.
    pub model_path: String,
    /// Loopback port of the process serving it.
    pub port: u16,
    /// Context window in tokens.
    pub n_ctx: u32,
    /// Layers placed on the GPU.
    pub n_gpu_layers: i32,
    /// Requests the process may serve in parallel; absent when the engine
    /// default applies.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub n_parallel: Option<u32>,
    /// When the model finished loading, RFC 3339.
    pub loaded_at: String,
    /// When a request last used the model, RFC 3339.
    pub last_used_at: String,
    /// Requests being answered right now. An unload is refused above zero.
    pub active_requests: usize,
    /// Who loaded the model: `app`, or `server` when the compatible server
    /// loaded it for a request of another program. Fixed at load.
    pub origin: LoadOrigin,
}

impl From<LoadedModel> for LoadedModelDto {
    fn from(m: LoadedModel) -> Self {
        Self {
            engine: m.engine,
            kind: m.kind,
            model: m.model,
            model_path: m.model_path,
            port: m.port,
            n_ctx: m.n_ctx,
            n_gpu_layers: m.n_gpu_layers,
            n_parallel: m.n_parallel,
            loaded_at: m.loaded_at.to_rfc3339(),
            last_used_at: m.last_used_at.to_rfc3339(),
            active_requests: m.active_requests,
            origin: LoadOrigin::App,
        }
    }
}

/// Response body for `GET /api/v1/llm/loaded-models`.
#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LoadedModelsResponse {
    /// Whether a local engine exists on this host. `false` means nothing can
    /// ever be loaded, not merely that nothing is.
    pub engine_available: bool,
    /// The explicit ceiling on resident models (`APOLLIA_LLAMA_MAX_LOADED`);
    /// `null` when the load guardrail alone decides how many stay loaded, and
    /// without an engine.
    pub max_loaded: Option<usize>,
    /// The resident models, most recently used first.
    pub models: Vec<LoadedModelDto>,
}

/// Request body for `POST /api/v1/llm/loaded-models/unload`.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UnloadModelRequest {
    /// Path of the model to unload, as the list reports it.
    pub model_path: String,
}

/// Response body for `POST /api/v1/llm/loaded-models/unload`.
#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UnloadModelResponse {
    /// The model that was unloaded.
    pub model_path: String,
    /// Processes stopped: one, or more when the model ran with several
    /// context windows.
    pub stopped: usize,
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

/// Handler for `GET /api/v1/llm/loaded-models`.
#[utoipa::path(
    get,
    path = "/api/v1/llm/loaded-models",
    tag = "llm",
    responses(
        (status = 200, description = "Resident local models", body = LoadedModelsResponse),
    )
)]
pub async fn list_loaded_models<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
) -> Json<LoadedModelsResponse> {
    let llama = state.llama_server_supervisor.as_deref();
    let mlx = state.mlx_engine.as_deref();
    if llama.is_none() && mlx.is_none() {
        return Json(LoadedModelsResponse {
            engine_available: false,
            max_loaded: None,
            models: Vec::new(),
        });
    }
    let mut loaded = Vec::new();
    // A ceiling holds only when every engine present sets one: an engine
    // without one admits whatever the load guardrail admits.
    let mut max_loaded = Some(0_usize);
    if let Some(engine) = llama {
        loaded.extend(LocalEngine::loaded_models(engine).await);
        max_loaded = max_loaded
            .zip(LocalEngine::max_loaded(engine))
            .map(|(a, b)| a + b);
    }
    if let Some(engine) = mlx {
        loaded.extend(LocalEngine::loaded_models(engine).await);
        max_loaded = max_loaded
            .zip(LocalEngine::max_loaded(engine))
            .map(|(a, b)| a + b);
    }
    loaded.sort_by_key(|m| std::cmp::Reverse(m.last_used_at));
    let residency = Engines { llama, mlx }.residency();
    let models = loaded
        .into_iter()
        .map(|m| {
            let origin = residency
                .as_ref()
                .map_or(LoadOrigin::App, |r| r.origin_of(&m));
            LoadedModelDto {
                origin,
                ..LoadedModelDto::from(m)
            }
        })
        .collect();
    Json(LoadedModelsResponse {
        engine_available: true,
        max_loaded,
        models,
    })
}

/// Unload `model_path` from whichever engine serves it, and announce it as
/// asked by the user. Returns the processes stopped.
async fn unload_from_engines(
    llama: Option<&crate::llama_server::LlamaServerSupervisor>,
    mlx: Option<&crate::mlx_engine::MlxEngineSupervisor>,
    model_path: &str,
) -> Result<usize, UnloadError> {
    let mut outcome = Err(UnloadError::NotLoaded {
        model_path: model_path.to_owned(),
    });
    let mut engine = Engine::LlamaServer;
    if let Some(llama) = llama {
        outcome = LocalEngine::unload_model(llama, model_path).await;
    }
    if let (Err(UnloadError::NotLoaded { .. }), Some(mlx)) = (&outcome, mlx) {
        engine = Engine::VllmMlx;
        outcome = LocalEngine::unload_model(mlx, model_path).await;
    }
    if outcome.is_ok() {
        if let Some(residency) = (crate::residency::Engines { llama, mlx }).residency() {
            residency.announce_unloaded(
                engine.as_str(),
                model_path,
                apollia_core::ModelUnloadReason::Requested,
            );
        }
    }
    outcome
}

/// Handler for `POST /api/v1/llm/loaded-models/unload`.
///
/// Stops the model and frees its memory. Refused with 409 while it is
/// answering a request, so an unload never cuts an answer short.
#[utoipa::path(
    post,
    path = "/api/v1/llm/loaded-models/unload",
    tag = "llm",
    request_body = UnloadModelRequest,
    responses(
        (status = 200, description = "Model unloaded", body = UnloadModelResponse),
        (status = 400, description = "Empty model path", body = ApiErrorBody),
        (status = 404, description = "Model not loaded", body = ApiErrorBody),
        (status = 409, description = "Model in use, a response is in progress", body = ApiErrorBody),
        (status = 503, description = "No local engine on this host", body = ApiErrorBody),
    )
)]
pub async fn unload_loaded_model<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Json(body): Json<UnloadModelRequest>,
) -> Result<Json<UnloadModelResponse>, ApiError> {
    if body.model_path.trim().is_empty() {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "modelPath must not be empty",
        ));
    }
    let llama = state.llama_server_supervisor.as_deref();
    let mlx = state.mlx_engine.as_deref();
    if llama.is_none() && mlx.is_none() {
        return Err(api_err(
            StatusCode::SERVICE_UNAVAILABLE,
            "no local engine on this host",
        ));
    }
    match unload_from_engines(llama, mlx, &body.model_path).await {
        Ok(stopped) => Ok(Json(UnloadModelResponse {
            model_path: body.model_path,
            stopped,
        })),
        Err(e @ UnloadError::NotLoaded { .. }) => Err(api_err(StatusCode::NOT_FOUND, e)),
        Err(e @ UnloadError::InUse { .. }) => Err(api_err(StatusCode::CONFLICT, e)),
    }
}

/// Sub-router for the resident-model routes, merged into the LLM routes.
pub fn loaded_model_routes<B: ExecutionBackend + Clone>() -> Router<AppState<B>> {
    Router::new()
        .route(
            "/api/v1/llm/loaded-models",
            get(list_loaded_models::<B>).post(super::load::load_model::<B>),
        )
        .route(
            "/api/v1/llm/loaded-models/unload",
            post(unload_loaded_model::<B>),
        )
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use axum::body::Body;
    use axum::http::{Method, Request};
    use tower::ServiceExt;

    use super::super::tests::{test_app_state_no_llm, MockBackend};
    use super::*;
    use crate::llama_server::LlamaServerSupervisor;

    fn router_over(supervisor: Option<Arc<LlamaServerSupervisor>>) -> Router {
        let mut state = test_app_state_no_llm();
        state.llama_server_supervisor = supervisor;
        crate::api::routes_llm::llm_routes::<MockBackend>().with_state(state)
    }

    async fn send(router: Router, method: Method, path: &str, body: &str) -> (StatusCode, Vec<u8>) {
        let request = Request::builder()
            .method(method)
            .uri(path)
            .header("content-type", "application/json")
            .body(Body::from(body.to_owned()))
            .expect("build request");
        let response = router.oneshot(request).await.expect("router response");
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024)
            .await
            .expect("read body");
        (status, bytes.to_vec())
    }

    #[tokio::test]
    async fn test_unload_over_http_stops_the_first_model_and_lists_the_second() {
        // GIVEN two stand-in models resident behind the LLM routes
        let sup =
            LlamaServerSupervisor::with_stand_in_models(&["/models/a.gguf", "/models/b.gguf"])
                .await;
        let router = router_over(Some(Arc::clone(&sup)));

        // WHEN the first is unloaded over HTTP
        let (status, body) = send(
            router.clone(),
            Method::POST,
            "/api/v1/llm/loaded-models/unload",
            r#"{"modelPath":"/models/a.gguf"}"#,
        )
        .await;

        // THEN the route answers it stopped, and the list holds the second only
        assert_eq!(status, StatusCode::OK, "{}", String::from_utf8_lossy(&body));
        let unloaded: UnloadModelResponse = serde_json::from_slice(&body).expect("json");
        assert_eq!(unloaded.model_path, "/models/a.gguf");
        assert_eq!(unloaded.stopped, 1);
        let (status, body) = send(router, Method::GET, "/api/v1/llm/loaded-models", "").await;
        assert_eq!(status, StatusCode::OK);
        let listed: serde_json::Value = serde_json::from_slice(&body).expect("json");
        assert_eq!(listed["engineAvailable"], true);
        assert_eq!(listed["maxLoaded"], 2);
        let models = listed["models"].as_array().expect("models array");
        assert_eq!(models.len(), 1);
        assert_eq!(models[0]["modelPath"], "/models/b.gguf");
        assert_eq!(models[0]["model"], "b");
        for key in [
            "port",
            "nCtx",
            "nGpuLayers",
            "nParallel",
            "loadedAt",
            "lastUsedAt",
            "activeRequests",
        ] {
            assert!(models[0].get(key).is_some(), "missing {key}");
        }
    }

    #[tokio::test]
    async fn test_unload_over_http_of_a_model_in_use_answers_409() {
        // GIVEN a resident model answering one request
        let sup = LlamaServerSupervisor::with_stand_in_models(&["/models/a.gguf"]).await;
        let _lease = sup.lease_for_test("/models/a.gguf").await.expect("live");
        let router = router_over(Some(Arc::clone(&sup)));

        // WHEN it is asked to unload over HTTP
        let (status, body) = send(
            router,
            Method::POST,
            "/api/v1/llm/loaded-models/unload",
            r#"{"modelPath":"/models/a.gguf"}"#,
        )
        .await;

        // THEN the route refuses with 409 and says the model is in use
        assert_eq!(status, StatusCode::CONFLICT);
        let err: serde_json::Value = serde_json::from_slice(&body).expect("json");
        let message = err["error"].as_str().expect("error string");
        assert!(message.contains("is in use"), "{message}");
        assert_eq!(sup.loaded_models().await.len(), 1);
    }

    #[tokio::test]
    async fn test_unload_over_http_answers_404_400_and_503_for_what_it_cannot_do() {
        // GIVEN an engine with nothing resident, and a host with no engine
        let empty = router_over(Some(LlamaServerSupervisor::with_stand_in_models(&[]).await));
        let absent = router_over(None);
        let path = "/api/v1/llm/loaded-models/unload";
        let ask = r#"{"modelPath":"/models/a.gguf"}"#;

        // WHEN each is asked to unload
        let (not_loaded, _) = send(empty.clone(), Method::POST, path, ask).await;
        let (blank, _) = send(empty, Method::POST, path, r#"{"modelPath":" "}"#).await;
        let (no_engine, _) = send(absent.clone(), Method::POST, path, ask).await;
        let (_, list) = send(absent, Method::GET, "/api/v1/llm/loaded-models", "").await;

        // THEN each refusal carries its own status, and the list without an
        // engine says so rather than looking merely empty
        assert_eq!(not_loaded, StatusCode::NOT_FOUND);
        assert_eq!(blank, StatusCode::BAD_REQUEST);
        assert_eq!(no_engine, StatusCode::SERVICE_UNAVAILABLE);
        let listed: serde_json::Value = serde_json::from_slice(&list).expect("json");
        assert_eq!(listed["engineAvailable"], false);
        assert_eq!(listed["models"].as_array().map(Vec::len), Some(0));
    }

    #[tokio::test]
    async fn test_the_list_and_the_unload_cover_the_mlx_models_with_their_engine() {
        // GIVEN a GGUF model resident on llama-server and an MLX model
        // resident on vllm-mlx, behind the LLM routes
        let llama = LlamaServerSupervisor::with_stand_in_models(&["/models/a.gguf"]).await;
        let mlx =
            crate::mlx_engine::MlxEngineSupervisor::with_stand_in_models(&["/models/qwen-mlx"])
                .await;
        let mut state = test_app_state_no_llm();
        state.llama_server_supervisor = Some(llama);
        state.mlx_engine = Some(Arc::clone(&mlx));
        let router = crate::api::routes_llm::llm_routes::<MockBackend>().with_state(state);

        // WHEN the resident models are listed, then the MLX one is unloaded
        let (_, listed) = send(router.clone(), Method::GET, "/api/v1/llm/loaded-models", "").await;
        let (status, _) = send(
            router.clone(),
            Method::POST,
            "/api/v1/llm/loaded-models/unload",
            r#"{"modelPath":"/models/qwen-mlx"}"#,
        )
        .await;
        let (_, after) = send(router, Method::GET, "/api/v1/llm/loaded-models", "").await;

        // THEN both are listed, each naming its engine, no fixed ceiling holds
        // since the MLX engine sets none, and the MLX model unloads like a GGUF
        // one
        let listed: serde_json::Value = serde_json::from_slice(&listed).expect("json");
        let models = listed["models"].as_array().expect("models");
        let engine_of = |path: &str| {
            models
                .iter()
                .find(|m| m["modelPath"] == path)
                .map(|m| m["engine"].clone())
        };
        assert_eq!(models.len(), 2);
        assert_eq!(
            engine_of("/models/a.gguf"),
            Some(serde_json::json!("llama-server"))
        );
        assert_eq!(
            engine_of("/models/qwen-mlx"),
            Some(serde_json::json!("vllm-mlx"))
        );
        assert_eq!(listed["maxLoaded"], serde_json::Value::Null);
        assert_eq!(status, StatusCode::OK);
        assert!(mlx.loaded_models().await.is_empty());
        let after: serde_json::Value = serde_json::from_slice(&after).expect("json");
        assert_eq!(after["models"].as_array().map(Vec::len), Some(1));
        assert_eq!(after["models"][0]["engine"], "llama-server");
    }
}
