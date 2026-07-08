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
    /// Whether the model is loaded and ready for inference.
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

/// Resolve `~` in paths to the user's home directory.
fn resolve_home(path: &std::path::Path) -> std::path::PathBuf {
    if let Ok(stripped) = path.strip_prefix("~") {
        if let Ok(home) = std::env::var("HOME") {
            return std::path::PathBuf::from(home).join(stripped);
        }
    }
    path.to_owned()
}

// ── Handlers ────────────────────────────────────────────────────────

/// `GET /api/v1/stt/status`, return current STT engine status.
///
/// Returns `200 OK` with the status when the engine is running.
/// Returns `503 Service Unavailable` when the engine is absent.
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
    let engine = state.stt_engine.as_ref().ok_or_else(stt_unavailable)?;

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
///
/// Returns `200 OK` with the persisted transcript row (`TranscriptRow`).
/// Returns `400 Bad Request` on missing or invalid audio.
/// Returns `503 Service Unavailable` when the engine is absent.
#[utoipa::path(
    post,
    path = "/api/v1/stt/transcribe",
    tag = "stt",
    request_body(content_type = "multipart/form-data", description = "Multipart form with an `audio` WAV field (required) and an optional `language` hint"),
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
    let engine = state.stt_engine.as_ref().ok_or_else(stt_unavailable)?;
    let repo = state.stt_repository.as_ref().ok_or_else(stt_unavailable)?;

    let mut audio_data: Option<Vec<u8>> = None;
    let mut language: Option<String> = None;

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
            _ => {}
        }
    }

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

    let _ = language;
    let transcript = engine
        .transcribe(audio, 16000, TranscriptSource::Api)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "STT transcription failed");
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
        .insert("api", &transcript, None)
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
    let repo = state.stt_repository.as_ref().ok_or_else(stt_unavailable)?;

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
            tracing::error!(error = %e, "failed to list transcriptions");
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
    let repo = state.stt_repository.as_ref().ok_or_else(stt_unavailable)?;

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
            tracing::error!(error = %e, id = %id, "failed to delete transcription");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(SttErrorResponse {
                    error: format!("failed to delete transcription: {e}"),
                }),
            )
        })?;

    Ok(StatusCode::NO_CONTENT)
}

/// `GET /api/v1/stt/models`, list available `.bin` model files.
///
/// Scans `~/.apollia/models/` for `.bin` files and returns their name,
/// path, and size. Returns an empty list if the directory does not exist.
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
    let models_dir = resolve_home(std::path::Path::new("~/.apollia/models"));

    let mut models = Vec::new();

    if models_dir.is_dir() {
        let entries = std::fs::read_dir(&models_dir).map_err(|e| {
            tracing::error!(error = %e, path = %models_dir.display(), "failed to read models directory");
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
            if path.extension().and_then(|e| e.to_str()) == Some("bin") {
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

/// `GET /api/v1/stt/config`, return the persisted STT configuration.
///
/// Returns `200 OK` with the current [`SttConfigRow`] from `system.db`.
/// If the table is empty (first boot), the defaults are inserted and returned.
/// Returns `503 Service Unavailable` when the config repository is unavailable.
#[utoipa::path(
    get,
    path = "/api/v1/stt/config",
    tag = "stt",
    responses(
        (status = 200, description = "Persisted STT configuration"),
        (status = 500, description = "Database error", body = crate::api::openapi::ApiErrorBody),
        (status = 503, description = "STT config repository unavailable", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub async fn get_stt_config<B: ExecutionBackend + Clone + From<DynBackend>>(
    State(state): State<AppState<B>>,
) -> RouteResult<SttConfigRow> {
    let repo = state.stt_config_repo.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(SttErrorResponse {
                error: "STT config repository not available".into(),
            }),
        )
    })?;

    let config = repo
        .lock()
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(SttErrorResponse {
                    error: format!("repository lock error: {e}"),
                }),
            )
        })?
        .get_or_default()
        .map_err(|e| {
            tracing::error!(error = %e, "failed to read STT config from database");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(SttErrorResponse {
                    error: format!("database error: {e}"),
                }),
            )
        })?;

    Ok((StatusCode::OK, Json(config)))
}

/// `PUT /api/v1/stt/config`, update the persisted STT configuration.
///
/// Accepts a JSON body of [`SttConfigRow`] (fields with `#[serde(default)]` may
/// be omitted, missing fields receive their default values). Replaces the
/// singleton row in `system.db` via an upsert.
///
/// Returns `200 OK` with the updated configuration.
/// Returns `503 Service Unavailable` when the config repository is unavailable.
#[utoipa::path(
    put,
    path = "/api/v1/stt/config",
    tag = "stt",
    request_body(content_type = "application/json", description = "Updated STT configuration (SttConfigRow); fields with defaults may be omitted"),
    responses(
        (status = 200, description = "Updated STT configuration"),
        (status = 500, description = "Database error", body = crate::api::openapi::ApiErrorBody),
        (status = 503, description = "STT config repository unavailable", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub async fn update_stt_config<B: ExecutionBackend + Clone + From<DynBackend>>(
    State(state): State<AppState<B>>,
    Json(new_config): Json<SttConfigRow>,
) -> RouteResult<SttConfigRow> {
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

    guard.upsert(&new_config).map_err(|e| {
        tracing::error!(error = %e, "failed to persist STT config");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(SttErrorResponse {
                error: format!("database error: {e}"),
            }),
        )
    })?;

    let updated = guard.get_or_default().map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(SttErrorResponse {
                error: format!("database error: {e}"),
            }),
        )
    })?;

    tracing::info!(enabled = updated.enabled, "STT configuration updated");
    Ok((StatusCode::OK, Json(updated)))
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
    use super::*;
    use crate::api::server::AppState;
    use crate::coordinator::ExecutionBackend;
    use crate::eventbus::EventBus;
    use crate::registry::AgentRegistry;
    use crate::router::TaskRouterHandle;
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
            obs_config: apollia_core::ObservabilityConfig::default(),
            llm_call_repository: None,
            trigger_def_repo: None,
            notification_repo: None,
            notification_engine_handle: None,
            chat_manager: None,
            plan_cache: None,
            mailbox_handle: None,
            user_memory: None,
            stt_engine: None,
            stt_repository: None,
            stt_config_repo: None,
            mcp_handle: None,
            mcp_server_repo: None,
            llm_backend_repo: None,
            a2a_invoker: None,
            resilience_layer: None,
            runner_proxy: None,
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
        state.stt_repository = Some(Arc::new(std::sync::Mutex::new(repo)));

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

    // GIVEN no STT engine in AppState
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
}
