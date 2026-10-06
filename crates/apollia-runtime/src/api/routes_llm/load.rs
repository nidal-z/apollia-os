//! Loading a local model by hand, the estimate shown before, and the last
//! model loaded.
//!
//! - `POST /api/v1/llm/loaded-models/estimate`    , what a load would take
//! - `POST /api/v1/llm/loaded-models`             , load a model by hand
//! - `GET  /api/v1/llm/loaded-models/last`        , the last model loaded by hand
//! - `POST /api/v1/llm/loaded-models/last/reload` , load it again
//!
//! The estimate and the load take the same body: a model of the model index,
//! a window, an offload and advanced settings, all optional. The settings
//! apply to this load alone, laid over the model's default preset; with
//! `remember`, the load writes them into that default preset, so the next
//! load the model makes on its own uses them too. A model of a declared
//! server is never loaded here: nothing of it runs on this machine.
//!
//! An MLX model has no setting to choose: its estimate is the one the MLX
//! engine is held to, and a load that names a setting is refused.

use std::path::{Path, PathBuf};

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use apollia_core::model_preset::{ModelSource, PresetDraft, RequestSettings};
use apollia_core::{LlmBackendError, LlmBackendRepository};
use apollia_llm::hardware::CompatibilityBadge;
use apollia_llm::model_index::{ModelCategory, ModelFormat, ModelIndex, INDEX_FILE_NAME};

use crate::api::routes_llm::backends::map_backend_error;
use crate::api::routes_llm::loaded::LoadedModelDto;
use crate::api::routes_llm::servers::{api_err, with_repo, ApiError};
use crate::api::server::AppState;
use crate::coordinator::ExecutionBackend;
use crate::engine_store::Engine;
use crate::llama_server::{LlamaServerError, LOAD_KEYS};
use crate::local_engine::guardrails::{
    self, GuardrailLevel, GuardrailPolicy, GuardrailRefusedBody, GuardrailVerdict, LoadEstimate,
};
use crate::local_engine::{engine_for_format, LoadConsent, LocalEngine, LocalEngineError};

/// Body of the estimate and of the load.
#[derive(Debug, Clone, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LoadModelRequest {
    /// The model, by the identifier the model index gives it.
    pub model_id: String,
    /// Context window in tokens; absent leaves the preset's, else the default.
    #[serde(default)]
    pub n_ctx: Option<u32>,
    /// Layers placed on the accelerator; absent leaves the preset's, else the
    /// planned count.
    #[serde(default)]
    pub n_gpu_layers: Option<i32>,
    /// Advanced load settings, in the keys a preset's load settings use
    /// (`n_threads`, `flash_attn`, `cache_type_k`...). Never a system prompt.
    #[serde(default)]
    #[schema(value_type = Object)]
    pub advanced: Map<String, Value>,
    /// Write these settings into the model's default preset (load only).
    #[serde(default)]
    pub remember: bool,
    /// Load even when the load guardrail would refuse (load only).
    #[serde(default)]
    pub confirmed: bool,
}

/// Body of `POST /api/v1/llm/loaded-models/last/reload`.
#[derive(Debug, Default, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ReloadLastRequest {
    /// Load even when the load guardrail would refuse.
    #[serde(default)]
    pub confirmed: bool,
}

/// How a model fits the machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum FitDto {
    /// Runs whole on the fastest memory of the machine.
    Fits,
    /// Runs split between the card and system memory, more slowly.
    MightFit,
    /// Exceeds what the machine can hold.
    TooLarge,
}

impl From<CompatibilityBadge> for FitDto {
    fn from(badge: CompatibilityBadge) -> Self {
        match badge {
            CompatibilityBadge::Fits => Self::Fits,
            CompatibilityBadge::MightFit => Self::MightFit,
            CompatibilityBadge::TooLarge => Self::TooLarge,
        }
    }
}

/// What the load guardrail makes of a load.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum GuardrailVerdictDto {
    /// The load goes ahead.
    Allowed,
    /// The load goes ahead, past what the machine holds comfortably.
    Warned,
    /// The load is refused unless confirmed.
    Refused,
}

impl From<GuardrailVerdict> for GuardrailVerdictDto {
    fn from(verdict: GuardrailVerdict) -> Self {
        match verdict {
            GuardrailVerdict::Allowed => Self::Allowed,
            GuardrailVerdict::Warned => Self::Warned,
            GuardrailVerdict::Refused => Self::Refused,
        }
    }
}

/// The load guardrail's view of a load.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct GuardrailCheckDto {
    /// The level in force, read from `apollia.toml` now.
    pub level: GuardrailLevel,
    /// What it makes of the estimate. `allowed` when there is no estimate.
    pub verdict: GuardrailVerdictDto,
    /// The limit it holds the load to, in gigabytes; absent without an
    /// estimate.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub budget_gb: Option<f64>,
}

/// The memory a load would take.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MemoryEstimateDto {
    /// On the accelerator, in gigabytes.
    pub gpu_gb: f64,
    /// Weights, cache and engine overhead, in gigabytes.
    pub total_gb: f64,
    /// What the machine can hold, in gigabytes.
    pub capacity_gb: f64,
    /// How the model fits the machine.
    pub fit: FitDto,
    /// Whether the cache was sized from the model's header rather than by a
    /// proportional fallback.
    pub kv_cache_measured: bool,
}

/// Response of `POST /api/v1/llm/loaded-models/estimate`.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LoadEstimateResponse {
    /// The model, as asked.
    pub model_id: String,
    /// The engine that would serve it.
    pub engine: Engine,
    /// The file or folder that would be loaded.
    pub model_path: String,
    /// Whether the load takes settings. `false` for an MLX model.
    pub configurable: bool,
    /// Context window the process would be launched with.
    pub n_ctx: u32,
    /// Layers it would place on the accelerator; absent for an MLX model.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub n_gpu_layers: Option<i32>,
    /// Layers the model has; absent when unknown.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_layers: Option<u32>,
    /// The largest window the model accepts, from its header; absent when
    /// the header does not say, and for an MLX model.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_ctx: Option<u32>,
    /// The quantisation the file name declares (`Q4_K_M`); absent when it
    /// declares none.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quantization: Option<String>,
    /// The memory it would take; absent when the model cannot be sized.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory: Option<MemoryEstimateDto>,
    /// What the load guardrail makes of it.
    pub guardrail: GuardrailCheckDto,
}

/// Response of a load by hand.
#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LoadModelResponse {
    /// The model, as asked.
    pub model_id: String,
    /// The instance now serving it.
    pub loaded: LoadedModelDto,
    /// The default preset the settings were written into, when `remember`
    /// was asked.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remembered_preset_id: Option<i64>,
}

/// Response of `GET /api/v1/llm/loaded-models/last`.
#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LastLoadedModelResponse {
    /// The model, by the identifier the model index gives it.
    pub model_id: String,
    /// The settings it was loaded with, in the keys a preset's load settings
    /// use.
    #[schema(value_type = Object)]
    pub load: Map<String, Value>,
    /// When it was loaded, RFC 3339.
    pub loaded_at: String,
}

/// A local model of the index, resolved to what the engines need.
struct Target {
    model_id: String,
    /// Name of its backend row: the key its presets and lazy loads use.
    backend: String,
    model_path: String,
    format: ModelFormat,
}

/// What a route of this module answers instead of its body.
pub enum Refusal {
    /// An error with its status and message.
    Api(ApiError),
    /// A load the guardrail refused: 409 with the estimate and the budget.
    Guardrail(Box<GuardrailRefusedBody>),
}

impl IntoResponse for Refusal {
    fn into_response(self) -> Response {
        match self {
            Self::Api(error) => error.into_response(),
            Self::Guardrail(body) => (StatusCode::CONFLICT, Json(*body)).into_response(),
        }
    }
}

fn refused(status: StatusCode, msg: impl std::fmt::Display) -> Refusal {
    Refusal::Api(api_err(status, msg))
}

/// `apollia.toml`, whose `[llm.guardrails]` the engines read: the file the
/// runtime was started with, else the one in its data directory, as at boot.
fn guardrails_file<B: ExecutionBackend + Clone>(state: &AppState<B>) -> PathBuf {
    state
        .config_path
        .clone()
        .unwrap_or_else(|| state.data_dir.join("apollia.toml"))
}

/// The settings a body names, in the keys a preset's load settings use.
///
/// # Errors
/// 422 for a key the engine does not know, or a window of zero.
fn requested_settings(req: &LoadModelRequest) -> Result<Map<String, Value>, Refusal> {
    if let Some(key) = req
        .advanced
        .keys()
        .find(|k| !LOAD_KEYS.contains(&k.as_str()))
    {
        return Err(refused(
            StatusCode::UNPROCESSABLE_ENTITY,
            format!("'{key}' is not a load setting"),
        ));
    }
    if req.n_ctx == Some(0) {
        return Err(refused(
            StatusCode::UNPROCESSABLE_ENTITY,
            "nCtx must be above zero",
        ));
    }
    let mut map = req.advanced.clone();
    if let Some(n_ctx) = req.n_ctx {
        map.insert("n_ctx".to_owned(), Value::from(n_ctx));
    }
    if let Some(layers) = req.n_gpu_layers {
        map.insert("n_gpu_layers".to_owned(), Value::from(layers));
    }
    Ok(map)
}

/// Resolve `model_id` against the model index, writing its backend row when
/// it has none yet.
///
/// # Errors
/// 404 for a model the index does not hold, 422 for one the chat cannot use.
fn resolve<B: ExecutionBackend + Clone>(
    state: &AppState<B>,
    model_id: &str,
) -> Result<Target, Refusal> {
    let models_dir = crate::installed_models::models_dir(&state.data_dir);
    let index = ModelIndex::load(&models_dir.join(INDEX_FILE_NAME)).unwrap_or_default();
    let Some(model) = index.get(model_id) else {
        return Err(refused(
            StatusCode::NOT_FOUND,
            format!("{model_id} is not a model of the model index"),
        ));
    };
    if model.effective_category() != ModelCategory::Language {
        return Err(refused(
            StatusCode::UNPROCESSABLE_ENTITY,
            format!("{model_id} is not a language model"),
        ));
    }
    let backend = with_repo(state, |repo| {
        crate::local_engine::local_row_for(repo, &models_dir, model_id).map_err(map_backend_error)
    })
    .map_err(Refusal::Api)?
    .ok_or_else(|| {
        refused(
            StatusCode::NOT_FOUND,
            format!("{model_id} has no local backend"),
        )
    })?;
    Ok(Target {
        model_id: model_id.to_owned(),
        backend,
        model_path: model.path.display().to_string(),
        format: model.format,
    })
}

fn no_engine() -> Refusal {
    refused(
        StatusCode::SERVICE_UNAVAILABLE,
        "no local engine on this host runs this model",
    )
}

/// What the guardrail in force makes of `estimate`.
fn check(policy: &GuardrailPolicy, estimate: Option<&LoadEstimate>) -> GuardrailCheckDto {
    let (verdict, budget_gb) = match estimate {
        Some(e) if policy.level != GuardrailLevel::Disabled => (
            guardrails::decide(policy, e),
            Some(guardrails::budget_gb(policy, e)),
        ),
        _ => (GuardrailVerdict::Allowed, None),
    };
    GuardrailCheckDto {
        level: policy.level,
        verdict: verdict.into(),
        budget_gb,
    }
}

async fn estimate_target<B: ExecutionBackend + Clone>(
    state: &AppState<B>,
    target: &Target,
    settings: &Map<String, Value>,
) -> Result<LoadEstimateResponse, Refusal> {
    let policy_file = guardrails_file(state);
    let engine = engine_for_format(target.format);
    match target.format {
        ModelFormat::Gguf => {
            let llama = state
                .llama_server_supervisor
                .as_ref()
                .ok_or_else(no_engine)?;
            let load = llama.explicit_settings(&target.backend, settings).await;
            let estimate = llama.estimate_explicit(&target.model_path, &load).await;
            let n_ctx = llama.window_for(&target.model_path, &load, None).await;
            let policy = tokio::task::spawn_blocking(move || {
                GuardrailPolicy::read(Some(policy_file.as_path()))
            })
            .await
            .unwrap_or_default();
            Ok(LoadEstimateResponse {
                model_id: target.model_id.clone(),
                engine,
                model_path: target.model_path.clone(),
                configurable: true,
                n_ctx: estimate.map_or(n_ctx, |e| e.n_ctx),
                n_gpu_layers: estimate.map(|e| e.n_gpu_layers),
                total_layers: estimate.map(|e| e.total_layers).filter(|n| *n > 0),
                max_ctx: estimate.and_then(|e| e.max_ctx),
                quantization: apollia_llm::recommend::gguf_file_quantization(&target.model_path),
                memory: estimate.map(|e| MemoryEstimateDto {
                    gpu_gb: e.gpu_gb,
                    total_gb: e.total_gb,
                    capacity_gb: e.guardrail.capacity_gb,
                    fit: e.guardrail.badge.into(),
                    kv_cache_measured: e.kv_cache_measured,
                }),
                guardrail: check(&policy, estimate.as_ref().map(|e| &e.guardrail)),
            })
        }
        ModelFormat::Mlx => {
            if !settings.is_empty() {
                return Err(refused(
                    StatusCode::UNPROCESSABLE_ENTITY,
                    "an MLX model takes no load setting",
                ));
            }
            let path = PathBuf::from(&target.model_path);
            let n_ctx = match &state.mlx_engine {
                Some(mlx) => mlx.window_for(&target.model_path, None).await,
                None => crate::mlx_engine::MlxEngineSupervisor::window(&target.model_path, None),
            };
            let (policy, estimate) = tokio::task::spawn_blocking(move || {
                (
                    GuardrailPolicy::read(Some(policy_file.as_path())),
                    mlx_estimate(&path, n_ctx),
                )
            })
            .await
            .unwrap_or_default();
            Ok(LoadEstimateResponse {
                model_id: target.model_id.clone(),
                engine,
                model_path: target.model_path.clone(),
                configurable: false,
                n_ctx,
                n_gpu_layers: None,
                total_layers: None,
                max_ctx: None,
                quantization: None,
                memory: estimate.map(|(gpu_gb, e)| MemoryEstimateDto {
                    gpu_gb,
                    total_gb: e.total_gb,
                    capacity_gb: e.capacity_gb,
                    fit: e.badge.into(),
                    kv_cache_measured: true,
                }),
                guardrail: check(&policy, estimate.as_ref().map(|(_, e)| e)),
            })
        }
    }
}

/// The estimate the MLX engine holds a load of `dir` at `n_ctx` to, with the
/// memory on the accelerator: all of it, on unified memory.
fn mlx_estimate(dir: &Path, n_ctx: u32) -> Option<(f64, LoadEstimate)> {
    let shape = apollia_llm::recommend::RuntimeShape {
        n_ctx,
        n_parallel: 1,
        ..apollia_llm::recommend::RuntimeShape::default()
    };
    let profile = apollia_llm::hardware::detect();
    let estimate = apollia_llm::recommend::estimate_mlx_memory(dir, &shape).ok()?;
    Some((estimate.total_gb, LoadEstimate::graded(&estimate, &profile)))
}

/// Write `settings` into the default preset of the local model `backend`,
/// creating that preset when the model has none, and return its identifier.
fn remember(
    repo: &LlmBackendRepository,
    backend: &str,
    settings: &Map<String, Value>,
) -> Result<i64, LlmBackendError> {
    let current = repo
        .list_presets(Some((ModelSource::Local, backend)))?
        .into_iter()
        .find(|p| p.is_model_default);
    if let Some(preset) = current {
        let mut load = preset.load;
        load.extend(settings.clone());
        let draft = PresetDraft {
            name: preset.name,
            load,
            request: preset.request,
            is_model_default: true,
        };
        return Ok(repo.update_preset(preset.id, &draft)?.id);
    }
    let mut last = None;
    for name in ["Default", "Default (loaded)", "Default (remembered)"] {
        let draft = PresetDraft {
            name: name.to_owned(),
            load: settings.clone(),
            request: RequestSettings::default(),
            is_model_default: true,
        };
        match repo.create_preset(ModelSource::Local, backend, &draft) {
            Ok(preset) => return Ok(preset.id),
            Err(e @ LlmBackendError::PresetConflict(_)) => last = Some(e),
            Err(e) => return Err(e),
        }
    }
    Err(last.unwrap_or_else(|| LlmBackendError::PresetConflict("Default".to_owned())))
}

/// Load `target` under `settings`, and answer the instance serving it.
async fn load_target<B: ExecutionBackend + Clone>(
    state: &AppState<B>,
    target: &Target,
    settings: &Map<String, Value>,
    consent: LoadConsent,
) -> Result<LoadedModelDto, Refusal> {
    let lease = match target.format {
        ModelFormat::Gguf => {
            let llama = state
                .llama_server_supervisor
                .as_ref()
                .ok_or_else(no_engine)?;
            let load = llama.explicit_settings(&target.backend, settings).await;
            llama
                .load_explicitly(&target.backend, target.model_path.clone(), load, consent)
                .await
                .map_err(|e| match e {
                    LlamaServerError::GuardrailRefused(refusal) => {
                        LocalEngineError::GuardrailRefused(refusal)
                    }
                    other => LocalEngineError::Launch {
                        engine: Engine::LlamaServer.as_str(),
                        reason: other.to_string(),
                    },
                })
        }
        ModelFormat::Mlx => {
            if !settings.is_empty() {
                return Err(refused(
                    StatusCode::UNPROCESSABLE_ENTITY,
                    "an MLX model takes no load setting",
                ));
            }
            if !crate::local_engine::is_apple_silicon() {
                return Err(refused(
                    StatusCode::UNPROCESSABLE_ENTITY,
                    LocalEngineError::MlxNeedsAppleSilicon,
                ));
            }
            let mlx = state.mlx_engine.as_ref().ok_or_else(|| {
                refused(
                    StatusCode::UNPROCESSABLE_ENTITY,
                    LocalEngineError::MlxEngineMissing,
                )
            })?;
            LocalEngine::ensure_model(
                mlx.as_ref(),
                &target.backend,
                target.model_path.clone(),
                None,
                consent,
            )
            .await
        }
    };
    let lease = lease.map_err(|e| match e {
        LocalEngineError::GuardrailRefused(refusal) => {
            Refusal::Guardrail(Box::new(GuardrailRefusedBody::from(&refusal)))
        }
        other => refused(StatusCode::SERVICE_UNAVAILABLE, other),
    })?;
    let mut resident = Vec::new();
    if let Some(engine) = state.llama_server_supervisor.as_deref() {
        resident.extend(LocalEngine::loaded_models(engine).await);
    }
    if let Some(engine) = state.mlx_engine.as_deref() {
        resident.extend(LocalEngine::loaded_models(engine).await);
    }
    drop(lease);
    resident
        .into_iter()
        .find(|m| m.model_path == target.model_path)
        .map(LoadedModelDto::from)
        .ok_or_else(|| {
            refused(
                StatusCode::SERVICE_UNAVAILABLE,
                format!("{} stopped right after it loaded", target.model_id),
            )
        })
}

/// Load `model_id` under `settings`, remember them when asked, and record the
/// load as the last one.
async fn load_and_record<B: ExecutionBackend + Clone>(
    state: &AppState<B>,
    model_id: &str,
    settings: Map<String, Value>,
    remember_settings: bool,
    consent: LoadConsent,
) -> Result<LoadModelResponse, Refusal> {
    let target = resolve(state, model_id)?;
    if remember_settings && target.format == ModelFormat::Mlx {
        return Err(refused(
            StatusCode::UNPROCESSABLE_ENTITY,
            "an MLX model takes no load setting to remember",
        ));
    }
    let loaded = load_target(state, &target, &settings, consent).await?;
    let remembered_preset_id = if remember_settings {
        let id = with_repo(state, |repo| {
            remember(repo, &target.backend, &settings).map_err(map_backend_error)
        })
        .map_err(Refusal::Api)?;
        tracing::event!(
            tracing::Level::INFO,
            backend = %target.backend,
            preset = id,
            "llm.model.settings_remembered"
        );
        Some(id)
    } else {
        None
    };
    let recorded = with_repo(state, |repo| {
        repo.set_last_loaded_model(&target.model_id, &settings)
            .map_err(map_backend_error)
    });
    if let Err((_, Json(body))) = recorded {
        tracing::event!(
            tracing::Level::WARN,
            model = %target.model_id,
            error = %body.error,
            "llm.model.last_loaded_unrecorded"
        );
    }
    Ok(LoadModelResponse {
        model_id: target.model_id,
        loaded,
        remembered_preset_id,
    })
}

/// Handler for `POST /api/v1/llm/loaded-models/estimate`.
#[utoipa::path(
    post,
    path = "/api/v1/llm/loaded-models/estimate",
    tag = "llm",
    request_body = LoadModelRequest,
    responses(
        (status = 200, description = "What the load would take", body = LoadEstimateResponse),
        (status = 404, description = "Not a model of the model index", body = crate::api::openapi::ApiErrorBody),
        (status = 422, description = "Not a language model, an unknown setting, or a setting for an MLX model", body = crate::api::openapi::ApiErrorBody),
        (status = 503, description = "No local engine runs this model, or no backend repository", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub async fn estimate_model_load<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Json(req): Json<LoadModelRequest>,
) -> Result<Json<LoadEstimateResponse>, Refusal> {
    let settings = requested_settings(&req)?;
    let target = resolve(&state, &req.model_id)?;
    estimate_target(&state, &target, &settings).await.map(Json)
}

/// Handler for `POST /api/v1/llm/loaded-models`.
///
/// Loads the model under the settings given, for this instance alone, over
/// its default preset; the instance then serves every request for the model.
/// A load the guardrail refuses is 409 with the estimate and the budget.
#[utoipa::path(
    post,
    path = "/api/v1/llm/loaded-models",
    tag = "llm",
    request_body = LoadModelRequest,
    responses(
        (status = 200, description = "Model loaded", body = LoadModelResponse),
        (status = 404, description = "Not a model of the model index", body = crate::api::openapi::ApiErrorBody),
        (status = 409, description = "The load guardrail refused the model; confirm the load to go ahead", body = crate::local_engine::guardrails::GuardrailRefusedBody),
        (status = 422, description = "Not a language model, an unknown setting, or a setting for an MLX model", body = crate::api::openapi::ApiErrorBody),
        (status = 503, description = "No local engine runs this model, or the engine did not start", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub async fn load_model<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Json(req): Json<LoadModelRequest>,
) -> Result<Json<LoadModelResponse>, Refusal> {
    let settings = requested_settings(&req)?;
    let consent = if req.confirmed {
        LoadConsent::Confirmed
    } else {
        LoadConsent::Unconfirmed
    };
    load_and_record(&state, &req.model_id, settings, req.remember, consent)
        .await
        .map(Json)
}

/// Handler for `GET /api/v1/llm/loaded-models/last`.
#[utoipa::path(
    get,
    path = "/api/v1/llm/loaded-models/last",
    tag = "llm",
    responses(
        (status = 200, description = "The last model loaded by hand", body = LastLoadedModelResponse),
        (status = 404, description = "No model was loaded by hand yet", body = crate::api::openapi::ApiErrorBody),
        (status = 503, description = "Backend repository not available", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub async fn get_last_loaded_model<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
) -> Result<Json<LastLoadedModelResponse>, Refusal> {
    let last = with_repo(&state, |repo| {
        repo.last_loaded_model().map_err(map_backend_error)
    })
    .map_err(Refusal::Api)?
    .ok_or_else(|| refused(StatusCode::NOT_FOUND, "no model was loaded by hand yet"))?;
    Ok(Json(LastLoadedModelResponse {
        model_id: last.model_id,
        load: last.load,
        loaded_at: last.loaded_at,
    }))
}

/// Handler for `POST /api/v1/llm/loaded-models/last/reload`.
///
/// Loads the last model loaded by hand again, under the settings it was
/// loaded with.
#[utoipa::path(
    post,
    path = "/api/v1/llm/loaded-models/last/reload",
    tag = "llm",
    request_body = ReloadLastRequest,
    responses(
        (status = 200, description = "Model loaded", body = LoadModelResponse),
        (status = 404, description = "No model was loaded by hand yet, or it left the model index", body = crate::api::openapi::ApiErrorBody),
        (status = 409, description = "The load guardrail refused the model; confirm the load to go ahead", body = crate::local_engine::guardrails::GuardrailRefusedBody),
        (status = 503, description = "No local engine runs this model, or the engine did not start", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub async fn reload_last_loaded_model<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Json(req): Json<ReloadLastRequest>,
) -> Result<Json<LoadModelResponse>, Refusal> {
    let last = with_repo(&state, |repo| {
        repo.last_loaded_model().map_err(map_backend_error)
    })
    .map_err(Refusal::Api)?
    .ok_or_else(|| refused(StatusCode::NOT_FOUND, "no model was loaded by hand yet"))?;
    let consent = if req.confirmed {
        LoadConsent::Confirmed
    } else {
        LoadConsent::Unconfirmed
    };
    load_and_record(&state, &last.model_id, last.load, false, consent)
        .await
        .map(Json)
}

/// Sub-router for loading by hand, merged into the LLM routes. The load
/// itself, `POST /api/v1/llm/loaded-models`, is registered beside the list in
/// [`super::loaded::loaded_model_routes`].
pub fn load_routes<B: ExecutionBackend + Clone>() -> Router<AppState<B>> {
    Router::new()
        .route(
            "/api/v1/llm/loaded-models/estimate",
            post(estimate_model_load::<B>),
        )
        .route(
            "/api/v1/llm/loaded-models/last",
            get(get_last_loaded_model::<B>),
        )
        .route(
            "/api/v1/llm/loaded-models/last/reload",
            post(reload_last_loaded_model::<B>),
        )
}

#[cfg(test)]
mod tests;
