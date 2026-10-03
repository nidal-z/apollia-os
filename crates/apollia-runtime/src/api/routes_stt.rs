//! REST routes for Speech-to-Text, `/api/v1/stt/*`.
//!
//! Provides 5 endpoints:
//! - `GET  /api/v1/stt/status`             , engine status
//! - `POST /api/v1/stt/transcribe`         , transcribe uploaded audio (multipart)
//! - `GET  /api/v1/stt/transcriptions`     , list transcription history
//! - `DELETE /api/v1/stt/transcriptions/:id`, delete a transcription
//! - `GET  /api/v1/stt/models`             , list available model files
//!
//! All routes return `503 Service Unavailable` when the STT engine is not
//! running (i.e. `stt_engine = None` in [`AppState`]).

use std::io::Cursor;

use axum::extract::{Multipart, Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};

use apollia_core::SttConfigRow;

use crate::api::server::AppState;
use crate::coordinator::{DynBackend, ExecutionBackend};
use crate::stt::TranscriptSource;

pub mod settings;

/// Error response body shared across STT routes.
#[derive(Debug, Serialize)]
pub struct SttErrorResponse {
    /// Human-readable error description.
    pub error: String,
}

/// Response body for `GET /api/v1/stt/status`.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct SttStatusResponse {
    /// Whether STT is enabled in configuration.
    pub enabled: bool,
    /// Whether a transcription has come back from the engine since it started.
    ///
    /// The runner sidecar loads the model on its first transcription, and the
    /// daemon only checks that the model file exists before starting: a reading
    /// that came back is the only proof it holds that the file is a model the
    /// engine can read. `false` therefore means "configured, not yet exercised"
    /// rather than "broken".
    pub model_loaded: bool,
    /// Filesystem path of the loaded model.
    pub model_path: String,
    /// Short model name (derived from filename without extension).
    pub model_name: String,
    /// Name of the active backend (e.g. `"whisper-cpp"`).
    pub backend_name: String,
    /// `true` when compiled with Apple Metal GPU acceleration.
    pub metal_enabled: bool,
    /// `true` when compiled with NVIDIA CUDA GPU acceleration.
    pub cuda_enabled: bool,
}

/// Query parameters for `GET /api/v1/stt/transcriptions`.
#[derive(Debug, Deserialize)]
pub struct ListTranscriptionsQuery {
    /// Maximum number of transcriptions to return (default: 50).
    pub limit: Option<u32>,
    /// Number of transcriptions to skip (default: 0).
    pub offset: Option<u32>,
}

/// Response body for `GET /api/v1/stt/transcriptions`.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct TranscriptionsListResponse {
    /// List of transcription rows.
    #[schema(value_type = Vec<Object>)]
    pub transcriptions: Vec<apollia_stt::TranscriptRow>,
}

/// Description of a model file on disk.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ModelInfo {
    /// Model filename (e.g. `"whisper-large-v3-fr-q5_0.bin"`).
    pub name: String,
    /// Full filesystem path.
    pub path: String,
    /// File size in bytes.
    pub size_bytes: u64,
}

/// Response body for `GET /api/v1/stt/models`.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ModelsListResponse {
    /// Available model files in `~/.apollia/models/`.
    pub models: Vec<ModelInfo>,
}

// ── Helpers ─────────────────────────────────────────────────────────

/// Type alias for the route Result.
type RouteResult<T> = Result<(StatusCode, Json<T>), (StatusCode, Json<SttErrorResponse>)>;

/// Returns a 503 error when the STT engine is not available.
fn stt_unavailable() -> (StatusCode, Json<SttErrorResponse>) {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        Json(SttErrorResponse {
            error: "STT engine not available - stt.enabled is false or model failed to load".into(),
        }),
    )
}

/// Snapshot the persisted STT configuration, dropping the (!Send) lock guard
/// before any await. Shared by the status and reload routes.
fn read_stt_config<B: ExecutionBackend + Clone + From<DynBackend>>(
    state: &AppState<B>,
) -> Result<SttConfigRow, (StatusCode, Json<SttErrorResponse>)> {
    let repo = state.stt_config_repo.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(SttErrorResponse {
                error: "STT config repository not available".into(),
            }),
        )
    })?;
    let guard = repo.lock().map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(SttErrorResponse {
                error: format!("repository lock error: {e}"),
            }),
        )
    })?;
    guard.get_or_default().map_err(|e| {
        tracing::error!(error = %e, "stt.config.read.failed");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(SttErrorResponse {
                error: format!("database error: {e}"),
            }),
        )
    })
}

// ── Handlers ────────────────────────────────────────────────────────

/// `GET /api/v1/stt/status`, return current STT engine status.
///
/// Returns `200 OK` with the status when the engine is running, and `200 OK`
/// with `model_loaded: false` read from the persisted configuration when it is
/// absent (disabled, model file missing, runner sidecar unavailable).
/// Returns `503 Service Unavailable` only when that configuration cannot be
/// read either.
#[utoipa::path(
    get,
    path = "/api/v1/stt/status",
    tag = "stt",
    responses(
        (status = 200, description = "STT engine status", body = SttStatusResponse),
        (status = 500, description = "STT engine actor stopped", body = crate::api::openapi::ApiErrorBody),
        (status = 503, description = "STT engine unavailable", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub async fn stt_status<B: ExecutionBackend + Clone + From<DynBackend>>(
    State(state): State<AppState<B>>,
) -> RouteResult<SttStatusResponse> {
    let Some(engine) = state.stt_engine.read().await.clone() else {
        // No engine: disabled in configuration, model file absent, or the
        // runner sidecar unavailable on this build. A status query still gets
        // a status, read from the persisted configuration, with
        // `model_loaded` false; the 503 stays for the case where even that
        // configuration cannot be read. Measured on a Linux build without a
        // runner, the previous 503 made `apollia-os stt status` answer an
        // error where the same fixture on macOS answered "not loaded".
        let cfg = read_stt_config(&state)?;
        let model_name = std::path::Path::new(&cfg.model_path)
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        return Ok((
            StatusCode::OK,
            Json(SttStatusResponse {
                enabled: cfg.enabled,
                model_loaded: false,
                model_path: cfg.model_path,
                model_name,
                backend_name: String::new(),
                metal_enabled: false,
                cuda_enabled: false,
            }),
        ));
    };

    let status = engine.status().await.ok_or_else(|| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(SttErrorResponse {
                error: "STT engine actor has stopped".into(),
            }),
        )
    })?;

    Ok((
        StatusCode::OK,
        Json(SttStatusResponse {
            enabled: status.enabled,
            model_loaded: status.model_loaded,
            model_path: status.model_path,
            model_name: status.model_name,
            backend_name: status.backend_name,
            metal_enabled: status.metal_enabled,
            cuda_enabled: status.cuda_enabled,
        }),
    ))
}

/// `POST /api/v1/stt/transcribe`, transcribe an uploaded audio file.
///
/// Accepts `multipart/form-data` with:
/// - `audio` (required): WAV audio file
/// - `language` (optional): language hint (ISO 639-1 code)
/// - `source` (optional): what the audio is, `api` (default), `file` or
///   `hotkey`, the tag the history shows; the desktop attached to this runtime
///   sends a file it read or a dictation it captured
///
/// Returns `200 OK` with the persisted transcript row (`TranscriptRow`).
/// Returns `400 Bad Request` on missing or invalid audio, or an unknown source.
/// Returns `503 Service Unavailable` when the engine is absent.
#[utoipa::path(
    post,
    path = "/api/v1/stt/transcribe",
    tag = "stt",
    request_body(content = inline(crate::api::openapi::TranscribeForm), content_type = "multipart/form-data", description = "Multipart form with an `audio` WAV field (required), an optional `language` hint and an optional `source` tag"),
    responses(
        (status = 200, description = "Persisted transcription row"),
        (status = 400, description = "Missing or invalid audio", body = crate::api::openapi::ApiErrorBody),
        (status = 500, description = "Transcription or persistence error", body = crate::api::openapi::ApiErrorBody),
        (status = 503, description = "STT engine unavailable", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub async fn transcribe_audio<B: ExecutionBackend + Clone + From<DynBackend>>(
    State(state): State<AppState<B>>,
    mut multipart: Multipart,
) -> RouteResult<apollia_stt::TranscriptRow> {
    let engine = state
        .stt_engine
        .read()
        .await
        .clone()
        .ok_or_else(stt_unavailable)?;
    let repo = state
        .stt_repository
        .read()
        .await
        .clone()
        .ok_or_else(stt_unavailable)?;

    let mut audio_data: Option<Vec<u8>> = None;
    let mut file_name: Option<String> = None;
    let mut language: Option<String> = None;
    let mut source_tag: Option<String> = None;

    while let Some(field) = multipart.next_field().await.map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(SttErrorResponse {
                error: format!("multipart read error: {e}"),
            }),
        )
    })? {
        let field_name = field.name().unwrap_or_default().to_owned();
        match field_name.as_str() {
            "audio" => {
                file_name = field.file_name().map(str::to_owned);
                let bytes = field.bytes().await.map_err(|e| {
                    (
                        StatusCode::BAD_REQUEST,
                        Json(SttErrorResponse {
                            error: format!("failed to read audio field: {e}"),
                        }),
                    )
                })?;
                audio_data = Some(bytes.to_vec());
            }
            "language" => {
                let text = field.text().await.map_err(|e| {
                    (
                        StatusCode::BAD_REQUEST,
                        Json(SttErrorResponse {
                            error: format!("failed to read language field: {e}"),
                        }),
                    )
                })?;
                if !text.is_empty() {
                    language = Some(text);
                }
            }
            "source" => {
                source_tag = Some(field.text().await.map_err(|e| {
                    (
                        StatusCode::BAD_REQUEST,
                        Json(SttErrorResponse {
                            error: format!("failed to read source field: {e}"),
                        }),
                    )
                })?);
            }
            _ => {}
        }
    }

    let source = transcript_source(source_tag.as_deref(), file_name)
        .map_err(|error| (StatusCode::BAD_REQUEST, Json(SttErrorResponse { error })))?;

    let raw_audio = audio_data.ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            Json(SttErrorResponse {
                error: "missing required 'audio' field in multipart form".into(),
            }),
        )
    })?;

    let (samples, sample_rate, channels) = decode_wav(&raw_audio).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(SttErrorResponse {
                error: format!("WAV decode error: {e}"),
            }),
        )
    })?;

    let audio = apollia_stt::to_whisper_format(&samples, sample_rate, channels).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(SttErrorResponse {
                error: format!("audio resampling error: {e}"),
            }),
        )
    })?;

    // The `language` field is honoured for this request only; when the caller
    // omits it, the persisted configuration hint applies. It used to be parsed
    // and then dropped, so a caller could set it and see no effect at all.
    let tag = source.as_str().to_owned();
    let transcribed = match language {
        Some(code) => {
            engine
                .transcribe_with_language(audio, 16000, source, Some(code))
                .await
        }
        None => engine.transcribe(audio, 16000, source).await,
    };
    let transcript = transcribed.map_err(|e| {
        tracing::error!(error = %e, "stt.transcription.failed");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(SttErrorResponse {
                error: format!("transcription failed: {e}"),
            }),
        )
    })?;

    let row = repo
        .lock()
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(SttErrorResponse {
                    error: format!("repository lock error: {e}"),
                }),
            )
        })?
        .insert(&tag, &transcript, None)
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(SttErrorResponse {
                    error: format!("failed to persist transcription: {e}"),
                }),
            )
        })?;

    let transcript_row = repo
        .lock()
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(SttErrorResponse {
                    error: format!("repository lock error: {e}"),
                }),
            )
        })?
        .get(&row)
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(SttErrorResponse {
                    error: format!("failed to retrieve transcription: {e}"),
                }),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(SttErrorResponse {
                    error: "transcription was persisted but could not be retrieved".into(),
                }),
            )
        })?;

    Ok((StatusCode::OK, Json(transcript_row)))
}

/// The source a transcription request names, `api` when it names none.
///
/// A `file` carries the name the upload gave its audio part.
fn transcript_source(
    tag: Option<&str>,
    file_name: Option<String>,
) -> Result<TranscriptSource, String> {
    match tag.unwrap_or("api") {
        "api" | "" => Ok(TranscriptSource::Api),
        "file" => Ok(TranscriptSource::File(file_name.unwrap_or_default())),
        "hotkey" => Ok(TranscriptSource::Hotkey),
        other => Err(format!(
            "unknown source '{other}': expected api, file or hotkey"
        )),
    }
}

/// `GET /api/v1/stt/transcriptions`, list transcription history.
///
/// Supports `?limit=N&offset=N` query parameters for pagination.
/// Returns `200 OK` with the list, even when empty.
/// Returns `503 Service Unavailable` when the STT subsystem is absent.
#[utoipa::path(
    get,
    path = "/api/v1/stt/transcriptions",
    tag = "stt",
    params(
        ("limit" = Option<u32>, Query, description = "Maximum number of transcriptions (default 50)"),
        ("offset" = Option<u32>, Query, description = "Number of transcriptions to skip (default 0)"),
    ),
    responses(
        (status = 200, description = "Transcription history", body = TranscriptionsListResponse),
        (status = 500, description = "Repository error", body = crate::api::openapi::ApiErrorBody),
        (status = 503, description = "STT subsystem unavailable", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub async fn list_transcriptions<B: ExecutionBackend + Clone + From<DynBackend>>(
    State(state): State<AppState<B>>,
    Query(params): Query<ListTranscriptionsQuery>,
) -> RouteResult<TranscriptionsListResponse> {
    let repo = state
        .stt_repository
        .read()
        .await
        .clone()
        .ok_or_else(stt_unavailable)?;

    let limit = params.limit.unwrap_or(50);
    let offset = params.offset.unwrap_or(0);

    let transcriptions = repo
        .lock()
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(SttErrorResponse {
                    error: format!("repository lock error: {e}"),
                }),
            )
        })?
        .list(limit, offset)
        .map_err(|e| {
            tracing::error!(error = %e, "stt.transcriptions.list.failed");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(SttErrorResponse {
                    error: format!("failed to list transcriptions: {e}"),
                }),
            )
        })?;

    Ok((
        StatusCode::OK,
        Json(TranscriptionsListResponse { transcriptions }),
    ))
}

/// `DELETE /api/v1/stt/transcriptions/:id`, delete a transcription by ID.
///
/// Returns `204 No Content` on success (even if the ID did not exist).
/// Returns `503 Service Unavailable` when the STT subsystem is absent.
#[utoipa::path(
    delete,
    path = "/api/v1/stt/transcriptions/{id}",
    tag = "stt",
    params(("id" = String, Path, description = "Transcription id")),
    responses(
        (status = 204, description = "Transcription deleted"),
        (status = 500, description = "Repository error", body = crate::api::openapi::ApiErrorBody),
        (status = 503, description = "STT subsystem unavailable", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub async fn delete_transcription<B: ExecutionBackend + Clone + From<DynBackend>>(
    State(state): State<AppState<B>>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<SttErrorResponse>)> {
    let repo = state
        .stt_repository
        .read()
        .await
        .clone()
        .ok_or_else(stt_unavailable)?;

    repo.lock()
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(SttErrorResponse {
                    error: format!("repository lock error: {e}"),
                }),
            )
        })?
        .delete(&id)
        .map_err(|e| {
            tracing::error!(error = %e, id = %id, "stt.transcription.delete.failed");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(SttErrorResponse {
                    error: format!("failed to delete transcription: {e}"),
                }),
            )
        })?;

    Ok(StatusCode::NO_CONTENT)
}

/// `GET /api/v1/stt/models`, list available model files.
///
/// Scans `~/.apollia/models/` for the extensions in
/// [`MODEL_EXTENSIONS`](crate::stt::MODEL_EXTENSIONS) and returns their name,
/// path, and size, the list the desktop shows in both modes. Returns an empty
/// list if the directory does not exist.
#[utoipa::path(
    get,
    path = "/api/v1/stt/models",
    tag = "stt",
    responses(
        (status = 200, description = "Available STT model files", body = ModelsListResponse),
        (status = 500, description = "Failed to read models directory", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub async fn list_models<B: ExecutionBackend + Clone + From<DynBackend>>(
    State(_state): State<AppState<B>>,
) -> RouteResult<ModelsListResponse> {
    // No home directory resolves to no models directory, hence the empty
    // list below, which is what the unresolved `~` used to produce.
    let models_dir = apollia_core::paths::data_dir().map(|d| d.join("models"));

    let mut models = Vec::new();

    if let Some(models_dir) = models_dir.filter(|d| d.is_dir()) {
        let entries = std::fs::read_dir(&models_dir).map_err(|e| {
            tracing::error!(error = %e, path = %models_dir.display(), "stt.models.read.failed");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(SttErrorResponse {
                    error: format!("failed to read models directory: {e}"),
                }),
            )
        })?;

        for entry in entries {
            let entry = match entry {
                Ok(e) => e,
                Err(_) => continue,
            };
            let path = entry.path();
            if crate::stt::is_model_file(&path) {
                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let size_bytes = entry.metadata().map(|m| m.len()).unwrap_or(0);
                models.push(ModelInfo {
                    name,
                    path: path.display().to_string(),
                    size_bytes,
                });
            }
        }
    }

    Ok((StatusCode::OK, Json(ModelsListResponse { models })))
}

// ── WAV decoding ────────────────────────────────────────────────────

/// Decode a WAV byte buffer into f32 samples, returning (samples, sample_rate, channels).
fn decode_wav(data: &[u8]) -> Result<(Vec<f32>, u32, u16), String> {
    let cursor = Cursor::new(data);
    let reader = hound::WavReader::new(cursor).map_err(|e| format!("invalid WAV: {e}"))?;
    let spec = reader.spec();
    let sample_rate = spec.sample_rate;
    let channels = spec.channels;

    let samples: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Int => {
            let max_val = (1u32 << (spec.bits_per_sample - 1)) as f32;
            reader
                .into_samples::<i32>()
                .map(|s| s.map(|v| v as f32 / max_val))
                .collect::<Result<Vec<f32>, _>>()
                .map_err(|e| format!("WAV sample read error: {e}"))?
        }
        hound::SampleFormat::Float => reader
            .into_samples::<f32>()
            .collect::<Result<Vec<f32>, _>>()
            .map_err(|e| format!("WAV sample read error: {e}"))?,
    };

    Ok((samples, sample_rate, channels))
}

#[cfg(test)]
mod tests {
    use super::settings::{get_stt_config, reload_stt_engine, update_stt_config};
    use super::*;
    use crate::api::server::AppState;
    use crate::coordinator::ExecutionBackend;
    use crate::eventbus::EventBus;
    use crate::registry::AgentRegistry;
    use crate::router::TaskRouterHandle;
    use apollia_core::SttConfigRepository;
    use apollia_core::{AIPResult, AIPTask, TaskStatus};
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use axum::routing::{delete, get, post};
    use axum::Router;
    use std::future::Future;
    use std::pin::Pin;
    use std::sync::Arc;
    use tower::ServiceExt;

    #[derive(Clone)]
    struct MockBackend;

    impl From<DynBackend> for MockBackend {
        fn from(_: DynBackend) -> Self {
            MockBackend
        }
    }

    impl ExecutionBackend for MockBackend {
        fn execute(
            &self,
            _task: AIPTask,
        ) -> Pin<Box<dyn Future<Output = Result<AIPResult, String>> + Send>> {
            Box::pin(async {
                Ok(AIPResult {
                    task_id: String::new(),
                    status: TaskStatus::Completed,
                    output: Vec::new(),
                    error: None,
                    artifacts: Vec::new(),
                    input_required_data: None,
                })
            })
        }
    }

    fn base_state() -> AppState<MockBackend> {
        let (event_tx, _) = EventBus::new();
        let registry_handle = AgentRegistry::spawn(event_tx.clone());
        let router_handle: TaskRouterHandle<MockBackend> =
            TaskRouterHandle::spawn(registry_handle.clone(), event_tx.clone(), 64);
        AppState {
            router_handle,
            registry_handle,
            event_sender: event_tx,
            agent_loader: Arc::new(crate::api::routes_agents::StubAgentLoader),
            backend: MockBackend,
            llm_router: crate::api::server::empty_shared_llm_router(),
            trigger_engine: None,
            config_path: None,
            task_repository: None,
            pending_approvals: None,
            plan_gates: None,
            notification_config: None,
            backend_factory: None,
            tool_registry_handle: None,
            audit_trail: None,
            audit_journal: None,
            connection_governance: Default::default(),
            obs_config: apollia_core::ObservabilityConfig::default(),
            llm_call_repository: None,
            trigger_def_repo: None,
            notification_repo: None,
            notification_engine_handle: None,
            chat_manager: None,
            plan_cache: None,
            mailbox_handle: None,
            user_memory: None,
            data_dir: std::path::PathBuf::new(),
            stt_engine: crate::api::server::empty_shared_stt_engine(),
            stt_repository: crate::api::server::empty_shared_stt_repository(),
            stt_config_repo: None,
            mcp_handle: None,
            mcp_server_repo: None,
            llm_backend_repo: None,
            a2a_invoker: None,
            resilience_layer: None,
            runner_proxy: None,
            llama_server_supervisor: None,
            mlx_engine: None,
            secret_store: crate::inference_servers::default_store_opener(),
            batches: None,
            workflows: None,
        }
    }

    fn router_without_stt() -> Router {
        let state = base_state();
        Router::new()
            .route("/api/v1/stt/status", get(stt_status::<MockBackend>))
            .route(
                "/api/v1/stt/transcribe",
                post(transcribe_audio::<MockBackend>),
            )
            .route(
                "/api/v1/stt/transcriptions",
                get(list_transcriptions::<MockBackend>),
            )
            .route(
                "/api/v1/stt/transcriptions/:id",
                delete(delete_transcription::<MockBackend>),
            )
            .route("/api/v1/stt/models", get(list_models::<MockBackend>))
            .with_state(state)
    }

    fn router_with_stt_repo() -> Router {
        let mut state = base_state();
        let dir =
            std::env::temp_dir().join(format!("apollia_stt_api_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        let repo = apollia_stt::SttRepository::open(&dir.join("stt.db")).expect("open test repo");
        state.stt_repository = crate::api::server::shared_stt_repository_from(Some(Arc::new(
            std::sync::Mutex::new(repo),
        )));

        Router::new()
            .route(
                "/api/v1/stt/transcriptions",
                get(list_transcriptions::<MockBackend>),
            )
            .route(
                "/api/v1/stt/transcriptions/:id",
                delete(delete_transcription::<MockBackend>),
            )
            .with_state(state)
    }

    // GIVEN no STT engine in AppState, and a config repository that holds a
    //       configuration enabling STT on a model path
    // WHEN GET /api/v1/stt/status
    // THEN 200 with that configuration and model_loaded false, the reading a
    //      Linux build without a runner sidecar used to answer with a 503
    #[tokio::test]
    async fn status_without_engine_but_with_config_reports_not_loaded() {
        let mut state = base_state();
        let dir =
            std::env::temp_dir().join(format!("apollia_stt_api_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        let repo = SttConfigRepository::open(&dir.join("stt_config.db")).expect("open config repo");
        let mut row = repo.get_or_default().expect("default row");
        row.enabled = true;
        row.model_path = "/models/ggml-base.bin".into();
        repo.upsert(&row).expect("persist row");
        state.stt_config_repo = Some(Arc::new(std::sync::Mutex::new(repo)));
        let router = Router::new()
            .route("/api/v1/stt/status", get(stt_status::<MockBackend>))
            .with_state(state);

        let req = Request::builder()
            .uri("/api/v1/stt/status")
            .body(Body::empty())
            .unwrap();
        let resp = router.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let body = axum::body::to_bytes(resp.into_body(), 4096).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["enabled"], serde_json::Value::Bool(true));
        assert_eq!(json["model_loaded"], serde_json::Value::Bool(false));
        assert_eq!(json["model_path"], "/models/ggml-base.bin");
        assert_eq!(json["model_name"], "ggml-base");
    }

    // GIVEN no STT engine in AppState, and no config repository either
    // WHEN GET /api/v1/stt/status
    // THEN 503 with descriptive error
    #[tokio::test]
    async fn status_without_engine_returns_503() {
        let router = router_without_stt();
        let req = Request::builder()
            .uri("/api/v1/stt/status")
            .body(Body::empty())
            .unwrap();
        let resp = router.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);

        let body = axum::body::to_bytes(resp.into_body(), 4096).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(json["error"].as_str().unwrap().contains("not available"));
    }

    // GIVEN no STT engine in AppState
    // WHEN POST /api/v1/stt/transcribe
    // THEN 503
    #[tokio::test]
    async fn transcribe_without_engine_returns_503() {
        let router = router_without_stt();
        let req = Request::builder()
            .method("POST")
            .uri("/api/v1/stt/transcribe")
            .header("content-type", "multipart/form-data; boundary=testboundary")
            .body(Body::from(
                "--testboundary\r\n\
                 Content-Disposition: form-data; name=\"audio\"; filename=\"test.wav\"\r\n\
                 Content-Type: audio/wav\r\n\r\n\
                 fake\r\n\
                 --testboundary--\r\n",
            ))
            .unwrap();
        let resp = router.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    // GIVEN no STT repository in AppState
    // WHEN GET /api/v1/stt/transcriptions
    // THEN 503
    #[tokio::test]
    async fn list_transcriptions_without_repo_returns_503() {
        let router = router_without_stt();
        let req = Request::builder()
            .uri("/api/v1/stt/transcriptions")
            .body(Body::empty())
            .unwrap();
        let resp = router.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    // GIVEN an empty STT repository
    // WHEN GET /api/v1/stt/transcriptions
    // THEN 200 with empty list
    #[tokio::test]
    async fn list_transcriptions_returns_empty_list() {
        let router = router_with_stt_repo();
        let req = Request::builder()
            .uri("/api/v1/stt/transcriptions")
            .body(Body::empty())
            .unwrap();
        let resp = router.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let body = axum::body::to_bytes(resp.into_body(), 4096).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let transcriptions = json["transcriptions"].as_array().unwrap();
        assert!(transcriptions.is_empty());
    }

    // GIVEN no STT repository
    // WHEN DELETE /api/v1/stt/transcriptions/:id
    // THEN 503
    #[tokio::test]
    async fn delete_transcription_without_repo_returns_503() {
        let router = router_without_stt();
        let req = Request::builder()
            .method("DELETE")
            .uri("/api/v1/stt/transcriptions/some-id")
            .body(Body::empty())
            .unwrap();
        let resp = router.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    // GIVEN a repository
    // WHEN DELETE /api/v1/stt/transcriptions/:nonexistent
    // THEN 204 (no-op delete)
    #[tokio::test]
    async fn delete_nonexistent_transcription_returns_204() {
        let router = router_with_stt_repo();
        let req = Request::builder()
            .method("DELETE")
            .uri("/api/v1/stt/transcriptions/nonexistent")
            .body(Body::empty())
            .unwrap();
        let resp = router.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NO_CONTENT);
    }

    // GIVEN any state
    // WHEN GET /api/v1/stt/models
    // THEN 200 (may be empty list if directory doesn't exist)
    #[tokio::test]
    async fn list_models_returns_ok() {
        let router = router_without_stt();
        let req = Request::builder()
            .uri("/api/v1/stt/models")
            .body(Body::empty())
            .unwrap();
        let resp = router.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let body = axum::body::to_bytes(resp.into_body(), 4096).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(json["models"].is_array());
    }

    /// A backend that answers every audio with the same sentence.
    struct EchoBackend;

    impl apollia_stt::SttBackend for EchoBackend {
        fn name(&self) -> &str {
            "echo"
        }

        fn transcribe(
            &self,
            _audio: &[f32],
            _sample_rate: u32,
            _language_hint: Option<&str>,
        ) -> Result<apollia_stt::TranscriptResult, apollia_stt::SttError> {
            Ok(apollia_stt::TranscriptResult {
                full_text: "bonjour".to_owned(),
                segments: Vec::new(),
                language: Some("fr".to_owned()),
                audio_duration_ms: 1000,
                processing_time_ms: 1,
            })
        }
    }

    /// A router whose engine and history are live, the engine echoing.
    fn router_with_engine() -> Router {
        let mut state = base_state();
        let dir =
            std::env::temp_dir().join(format!("apollia_stt_engine_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        let engine_repo = apollia_stt::SttRepository::open(&dir.join("stt.db")).expect("repo");
        let route_repo = apollia_stt::SttRepository::open(&dir.join("stt.db")).expect("repo");
        let engine = crate::stt::SttEngineHandle::start(
            Box::new(EchoBackend),
            engine_repo,
            SttConfigRow::default(),
            state.event_sender.clone(),
        );
        state.stt_engine = crate::api::server::shared_stt_engine_from(Some(engine));
        state.stt_repository = crate::api::server::shared_stt_repository_from(Some(Arc::new(
            std::sync::Mutex::new(route_repo),
        )));
        Router::new()
            .route(
                "/api/v1/stt/transcribe",
                post(transcribe_audio::<MockBackend>),
            )
            .with_state(state)
    }

    /// A multipart body carrying one second of silence and the given source.
    fn upload(source: &str) -> Request<Body> {
        let mut wav = std::io::Cursor::new(Vec::new());
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: 16_000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut writer = hound::WavWriter::new(&mut wav, spec).expect("writer");
        for _ in 0..16_000 {
            writer.write_sample(0i16).expect("sample");
        }
        writer.finalize().expect("finalize");
        let mut body = Vec::new();
        body.extend_from_slice(
            b"--b\r\nContent-Disposition: form-data; name=\"audio\"; filename=\"memo.wav\"\r\n\
              Content-Type: audio/wav\r\n\r\n",
        );
        body.extend_from_slice(wav.get_ref());
        body.extend_from_slice(
            format!(
                "\r\n--b\r\nContent-Disposition: form-data; name=\"source\"\r\n\r\n{source}\r\n--b--\r\n"
            )
            .as_bytes(),
        );
        Request::builder()
            .method("POST")
            .uri("/api/v1/stt/transcribe")
            .header("content-type", "multipart/form-data; boundary=b")
            .body(Body::from(body))
            .unwrap()
    }

    /// A router serving the settings routes over a configuration repository
    /// in a fresh directory, or over none.
    fn settings_router(with_repo: bool) -> Router {
        let mut state = base_state();
        if with_repo {
            let dir =
                std::env::temp_dir().join(format!("apollia_stt_api_test_{}", uuid::Uuid::new_v4()));
            std::fs::create_dir_all(&dir).expect("create temp dir");
            let repo = SttConfigRepository::open(&dir.join("system.db")).expect("open config repo");
            state.stt_config_repo = Some(Arc::new(std::sync::Mutex::new(repo)));
            state.data_dir = dir;
        }
        Router::new()
            .route(
                "/api/v1/stt/config",
                get(get_stt_config::<MockBackend>).put(update_stt_config::<MockBackend>),
            )
            .route("/api/v1/stt/reload", post(reload_stt_engine::<MockBackend>))
            .with_state(state)
    }

    async fn json_of(resp: axum::response::Response) -> serde_json::Value {
        let body = axum::body::to_bytes(resp.into_body(), 8192).await.unwrap();
        serde_json::from_slice(&body).unwrap()
    }

    fn put_config(body: serde_json::Value) -> Request<Body> {
        Request::builder()
            .method("PUT")
            .uri("/api/v1/stt/config")
            .header("content-type", "application/json")
            .body(Body::from(body.to_string()))
            .unwrap()
    }

    #[tokio::test]
    async fn an_uploaded_file_is_kept_in_the_history_as_a_file() {
        // GIVEN a live engine, and a WAV the desktop read from a file
        let router = router_with_engine();

        // WHEN it is uploaded with the source the desktop names
        let resp = router.oneshot(upload("file")).await.unwrap();

        // THEN the transcription is answered and kept under that source, the
        // tag the history shows, as the embedded command keeps it
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), 64 * 1024)
            .await
            .unwrap();
        let row: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(row["source"], "file");
        assert_eq!(row["full_text"], "bonjour");
    }

    #[tokio::test]
    async fn an_unknown_source_is_refused() {
        // GIVEN a live engine
        let router = router_with_engine();

        // WHEN an upload names a source the history does not know
        let resp = router.oneshot(upload("radio")).await.unwrap();

        // THEN it is refused before any transcription, with the reason
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        let body = axum::body::to_bytes(resp.into_body(), 4096).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(json["error"]
            .as_str()
            .unwrap()
            .contains("unknown source 'radio'"));
    }

    #[test]
    fn a_request_naming_no_source_is_an_api_one() {
        // GIVEN a request with no source field
        // WHEN its source is read
        let source = transcript_source(None, None).unwrap();

        // THEN it keeps the tag every caller had before the field existed
        assert_eq!(source.as_str(), "api");
    }

    #[tokio::test]
    async fn config_is_read_with_its_defaults_on_first_boot() {
        // GIVEN a configuration repository that holds no row yet
        let router = settings_router(true);

        // WHEN GET /api/v1/stt/config
        let resp = router
            .oneshot(
                Request::builder()
                    .uri("/api/v1/stt/config")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        // THEN 200 with the defaults, every field the desktop reads present
        assert_eq!(resp.status(), StatusCode::OK);
        let json = json_of(resp).await;
        assert_eq!(json["enabled"], false);
        for field in ["model_path", "hotkey", "clipboard_mode", "trigger_mode"] {
            assert!(json[field].is_string(), "{field} missing: {json}");
        }
    }

    #[tokio::test]
    async fn config_without_repository_is_refused_with_503() {
        // GIVEN no configuration repository
        let router = settings_router(false);

        // WHEN GET /api/v1/stt/config
        let resp = router
            .oneshot(
                Request::builder()
                    .uri("/api/v1/stt/config")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        // THEN 503 naming the missing repository
        assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
        let json = json_of(resp).await;
        assert!(json["error"].as_str().unwrap().contains("not available"));
    }

    #[tokio::test]
    async fn an_updated_configuration_is_persisted_and_returned() {
        // GIVEN a configuration repository
        let router = settings_router(true);

        // WHEN PUT /api/v1/stt/config with another model and a listed language
        let resp = router
            .clone()
            .oneshot(put_config(serde_json::json!({
                "enabled": false,
                "model_path": "/models/ggml-small.gguf",
                "language": "de",
                "input_device": "USB mic"
            })))
            .await
            .unwrap();

        // THEN 200 with the new values, and a later read returns them
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(json_of(resp).await["model_path"], "/models/ggml-small.gguf");
        let read = router
            .oneshot(
                Request::builder()
                    .uri("/api/v1/stt/config")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let json = json_of(read).await;
        assert_eq!(json["language"], "de");
        assert_eq!(json["input_device"], "USB mic");
    }

    #[tokio::test]
    async fn an_unlisted_language_is_refused_with_400_and_not_persisted() {
        // GIVEN a configuration repository
        let router = settings_router(true);

        // WHEN PUT /api/v1/stt/config with a free-text language
        let resp = router
            .clone()
            .oneshot(put_config(serde_json::json!({
                "model_path": "/models/ggml-small.bin",
                "language": "francais"
            })))
            .await
            .unwrap();

        // THEN 400 with the message the embedded desktop shows, and the
        // persisted configuration is untouched
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        let error = json_of(resp).await["error"].as_str().unwrap().to_owned();
        assert!(
            error.starts_with("unsupported dictation language 'francais'"),
            "{error}"
        );
        let read = router
            .oneshot(
                Request::builder()
                    .uri("/api/v1/stt/config")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_ne!(json_of(read).await["model_path"], "/models/ggml-small.bin");
    }

    #[tokio::test]
    async fn a_reload_with_dictation_disabled_answers_not_loaded() {
        // GIVEN a configuration repository whose default row disables dictation
        let router = settings_router(true);

        // WHEN POST /api/v1/stt/reload
        let resp = router
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/stt/reload")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        // THEN 200, no engine loaded
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(json_of(resp).await["loaded"], false);
    }

    #[tokio::test]
    async fn a_reload_without_repository_is_refused_with_503() {
        // GIVEN no configuration repository
        let router = settings_router(false);

        // WHEN POST /api/v1/stt/reload
        let resp = router
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/stt/reload")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        // THEN 503
        assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
    }
}
