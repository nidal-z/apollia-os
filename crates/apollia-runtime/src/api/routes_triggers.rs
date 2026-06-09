//! REST routes for trigger management.
//!
//! Exposes CRUD operations on triggers through the runtime REST API:
//! - `GET    /api/v1/triggers`             , list all triggers
//! - `GET    /api/v1/triggers/:id`         , full definition plus runtime status
//! - `POST   /api/v1/triggers`             , create a trigger
//! - `PUT    /api/v1/triggers/:id`         , update a trigger
//! - `DELETE /api/v1/triggers/:id`         , delete a trigger
//! - `POST   /api/v1/triggers/:id/fire`    , immediate firing
//! - `POST   /api/v1/triggers/:id/enable`  , enable
//! - `POST   /api/v1/triggers/:id/disable` , disable
//! - `GET    /api/v1/triggers/:id/logs`    , SQLite history
//! - `POST   /api/v1/triggers/reload`      , hot reload from SQLite
//!
//! **Shared return codes:**
//! - `503`, `TriggerEngine` or repository unavailable.
//! - `404`, unknown trigger.
//! - `409`, identifier already exists.
//! - `422`, validation error.
//! - `200` / `201`, success.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};

use apollia_triggers::{
    DefinitionRepositoryError, OnBusy, TriggerDefinitionRepository, TriggerDefinitionRow,
    TriggerEngineError, TriggerHistoryEntry,
};

use crate::api::server::AppState;
use crate::coordinator::ExecutionBackend;

// ─── Request types ───────────────────────────────────────────────────────

/// Request body for `POST /api/v1/triggers`, trigger creation.
#[derive(Debug, Deserialize)]
pub struct CreateTriggerRequest {
    /// Unique trigger identifier.
    pub id: String,
    /// Target agent.
    pub agent: Option<String>,
    /// Whether the trigger is active (default: `true`).
    pub enabled: Option<bool>,
    /// Policy when the agent is busy (default: `"queue"`).
    pub on_busy: Option<String>,
    /// Trigger source configuration.
    pub source: TriggerSourceInput,
    /// Input message template.
    pub input_template: Option<String>,
}

/// Trigger source description used in create/update requests.
#[derive(Debug, Deserialize)]
pub struct TriggerSourceInput {
    /// Source type: `"cron"`, `"interval"`, `"oneshot"`, `"file_watch"`, `"webhook"`.
    pub r#type: String,
    /// Source-specific configuration (fields flattened via `serde(flatten)`).
    #[serde(flatten)]
    pub config: serde_json::Value,
}

/// Request body for `PUT /api/v1/triggers/:id`, trigger update.
#[derive(Debug, Deserialize)]
pub struct UpdateTriggerRequest {
    /// Target agent.
    pub agent: Option<String>,
    /// Whether the trigger is active.
    pub enabled: Option<bool>,
    /// Policy when the agent is busy.
    pub on_busy: Option<String>,
    /// Trigger source configuration.
    pub source: TriggerSourceInput,
    /// Input message template.
    pub input_template: Option<String>,
}

// ─── Response types ──────────────────────────────────────────────────────

/// Response for CRUD operations returning a full definition.
#[derive(Debug, Serialize)]
pub struct TriggerDefinitionResponse {
    /// Unique trigger identifier.
    pub id: String,
    /// Target agent.
    pub agent: Option<String>,
    /// Whether the trigger is active.
    pub enabled: bool,
    /// Policy when the agent is busy.
    pub on_busy: String,
    /// Source type.
    pub source_type: String,
    /// Source JSON configuration.
    pub source_config: serde_json::Value,
    /// Input message template.
    pub input_template: Option<String>,
    /// Creation timestamp (ISO 8601).
    pub created_at: String,
    /// Last-modified timestamp (ISO 8601).
    pub updated_at: String,
}

/// Response for `DELETE /api/v1/triggers/:id`.
#[derive(Debug, Serialize)]
pub struct DeleteResponse {
    /// Identifier of the deleted trigger.
    pub deleted: String,
}

/// Success response body for reload.
#[derive(Serialize)]
pub struct ReloadResponse {
    /// Number of active triggers after reload.
    pub reloaded: usize,
}

/// Error response body.
#[derive(Debug, Serialize, Deserialize)]
pub struct ErrorResponse {
    /// Error description.
    pub error: String,
}

/// Response for `GET /api/v1/triggers/:id`, detailed status (legacy).
#[derive(Serialize)]
pub struct TriggerDetailResponse {
    /// Trigger identifier.
    pub id: String,
    /// Target agent name.
    pub agent: String,
    /// Source type.
    pub source_kind: String,
    /// Source configuration detail (e.g. cron expression, interval).
    pub source_detail: String,
    /// Policy when the agent is busy.
    pub on_busy: String,
    /// Whether the trigger is active.
    pub enabled: bool,
    /// Number of successful fires.
    pub fire_count: u64,
    /// Number of skips.
    pub skip_count: u64,
    /// Last fire timestamp (RFC3339) or `null`.
    pub last_fired: Option<String>,
}

/// Response for `POST /api/v1/triggers/:id/fire`.
#[derive(Serialize)]
pub struct FireResponse {
    /// Identifier of the submitted task.
    pub task_id: String,
}

/// Response for enable/disable.
#[derive(Serialize)]
pub struct OkResponse {
    /// Confirmation flag.
    pub ok: bool,
}

/// Response for `GET /api/v1/triggers/:id/logs`.
#[derive(Serialize)]
pub struct LogsResponse {
    /// History entries.
    pub entries: Vec<TriggerHistoryEntry>,
}

/// Query parameters for `GET /api/v1/triggers/:id/logs`.
#[derive(Debug, Deserialize)]
pub struct LogsQuery {
    /// Maximum number of entries to return (default: 20).
    #[serde(default = "default_last")]
    pub last: usize,
}

fn default_last() -> usize {
    20
}

// ─── Helpers ─────────────────────────────────────────────────────────────

/// Converts a [`TriggerDefinitionRow`] into a [`TriggerDefinitionResponse`].
fn row_to_response(row: TriggerDefinitionRow) -> TriggerDefinitionResponse {
    let on_busy = match row.on_busy {
        OnBusy::Queue => "queue".to_string(),
        OnBusy::Drop => "drop".to_string(),
    };
    TriggerDefinitionResponse {
        id: row.id,
        agent: row.agent,
        enabled: row.enabled,
        on_busy,
        source_type: row.source_type,
        source_config: row.source_config,
        input_template: row.input_template,
        created_at: row.created_at,
        updated_at: row.updated_at,
    }
}

/// Parses the `on_busy` field from the request (default: `"queue"`).
fn parse_on_busy(value: Option<&str>) -> Result<OnBusy, (StatusCode, Json<ErrorResponse>)> {
    match value {
        None | Some("queue") => Ok(OnBusy::Queue),
        Some("drop") => Ok(OnBusy::Drop),
        Some(other) => Err((
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(ErrorResponse {
                error: format!("validation error: invalid on_busy value: {other}"),
            }),
        )),
    }
}

/// Maps a [`DefinitionRepositoryError`] to a `(StatusCode, Json<ErrorResponse>)` tuple.
fn map_repo_error(err: DefinitionRepositoryError) -> (StatusCode, Json<ErrorResponse>) {
    let (status, msg) = match &err {
        DefinitionRepositoryError::NotFound(_) => (StatusCode::NOT_FOUND, err.to_string()),
        DefinitionRepositoryError::DuplicateId(_) => (StatusCode::CONFLICT, err.to_string()),
        DefinitionRepositoryError::ValidationError(_) => {
            (StatusCode::UNPROCESSABLE_ENTITY, err.to_string())
        }
        DefinitionRepositoryError::Database(_) => {
            (StatusCode::INTERNAL_SERVER_ERROR, err.to_string())
        }
    };
    (status, Json(ErrorResponse { error: msg }))
}

/// Extracts the repository from state, returns 503 if absent.
fn require_repo<B: ExecutionBackend + Clone>(
    state: &AppState<B>,
) -> Result<
    &std::sync::Arc<std::sync::Mutex<TriggerDefinitionRepository>>,
    (StatusCode, Json<ErrorResponse>),
> {
    state.trigger_def_repo.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse {
                error: "trigger definition repository not available".into(),
            }),
        )
    })
}

/// Reloads the `TriggerEngine` from the repository definitions (fire-and-forget).
///
/// Reads all definitions, converts them to rich types, and sends them to the engine.
/// Conversion errors are silently ignored: the trigger stays as-is in the DB but is
/// not loaded at runtime.
async fn reload_engine_from_repo<B: ExecutionBackend + Clone>(state: &AppState<B>) {
    let engine = match &state.trigger_engine {
        Some(e) => e.clone(),
        None => return,
    };
    let repo = match &state.trigger_def_repo {
        Some(r) => r.clone(),
        None => return,
    };

    let definitions = {
        let guard = match repo.lock() {
            Ok(g) => g,
            Err(_) => return,
        };
        match guard.list() {
            Ok(rows) => rows,
            Err(_) => return,
        }
    };

    let rich_defs: Vec<_> = definitions
        .into_iter()
        .filter_map(|row| apollia_triggers::TriggerDefinition::try_from(row).ok())
        .collect();

    engine.reload(rich_defs).await;
}

/// Returns the source type string for use in the `TriggerStatus` fallback.
fn source_detail_kind(source: &apollia_triggers::TriggerSourceConfig) -> String {
    source_kind_and_detail(source).0
}

/// Returns `(kind, detail)` from a [`TriggerSourceConfig`].
fn source_kind_and_detail(source: &apollia_triggers::TriggerSourceConfig) -> (String, String) {
    use apollia_triggers::TriggerSourceConfig;
    match source {
        TriggerSourceConfig::Cron { schedule } => ("cron".into(), schedule.clone()),
        TriggerSourceConfig::Interval { every } => ("interval".into(), every.clone()),
        TriggerSourceConfig::Oneshot { fire_at } => ("oneshot".into(), fire_at.to_rfc3339()),
        TriggerSourceConfig::FileWatch { path, events, .. } => {
            let evts: Vec<_> = events.iter().map(|e| e.to_string()).collect();
            (
                "file_watch".into(),
                format!("{} [{}]", path.display(), evts.join(",")),
            )
        }
        TriggerSourceConfig::Webhook { .. } => ("webhook".into(), String::new()),
    }
}

// ─── CRUD Handlers ───────────────────────────────────────────────────────

/// `POST /api/v1/triggers`, create a new trigger definition.
///
/// Validates the definition, inserts it into `triggers.db`, reloads the engine,
/// and returns `201` with the full definition (including `created_at`).
pub async fn create_trigger<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Json(body): Json<CreateTriggerRequest>,
) -> Result<(StatusCode, Json<TriggerDefinitionResponse>), (StatusCode, Json<ErrorResponse>)> {
    let repo = require_repo(&state)?;

    let on_busy = parse_on_busy(body.on_busy.as_deref())?;

    let row = TriggerDefinitionRow {
        id: body.id.clone(),
        agent: body.agent,
        enabled: body.enabled.unwrap_or(true),
        on_busy,
        source_type: body.source.r#type,
        source_config: body.source.config,
        input_template: body.input_template,
        created_at: String::new(),
        updated_at: String::new(),
    };

    let created = {
        let guard = repo.lock().map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "repository lock poisoned".into(),
                }),
            )
        })?;

        guard.insert(&row).map_err(map_repo_error)?;

        guard
            .get(&body.id)
            .map_err(map_repo_error)?
            .ok_or_else(|| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        error: "trigger inserted but not found".into(),
                    }),
                )
            })?
    };

    reload_engine_from_repo(&state).await;

    Ok((StatusCode::CREATED, Json(row_to_response(created))))
}

/// `PUT /api/v1/triggers/:id`, update an existing trigger definition.
///
/// Validates the new definition, updates it in `triggers.db`,
/// reloads the engine, and returns `200` with the updated definition.
pub async fn update_trigger<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path(id): Path<String>,
    Json(body): Json<UpdateTriggerRequest>,
) -> Result<Json<TriggerDefinitionResponse>, (StatusCode, Json<ErrorResponse>)> {
    let repo = require_repo(&state)?;

    let on_busy = parse_on_busy(body.on_busy.as_deref())?;

    let row = TriggerDefinitionRow {
        id: id.clone(),
        agent: body.agent,
        enabled: body.enabled.unwrap_or(true),
        on_busy,
        source_type: body.source.r#type,
        source_config: body.source.config,
        input_template: body.input_template,
        created_at: String::new(),
        updated_at: String::new(),
    };

    let updated = {
        let guard = repo.lock().map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "repository lock poisoned".into(),
                }),
            )
        })?;

        guard.update(&id, &row).map_err(map_repo_error)?;

        guard.get(&id).map_err(map_repo_error)?.ok_or_else(|| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "trigger updated but not found".into(),
                }),
            )
        })?
    };

    reload_engine_from_repo(&state).await;

    Ok(Json(row_to_response(updated)))
}

/// `DELETE /api/v1/triggers/:id`, delete a trigger definition.
///
/// Removes it from `triggers.db`, reloads the engine, and returns `200`.
pub async fn delete_trigger<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path(id): Path<String>,
) -> Result<Json<DeleteResponse>, (StatusCode, Json<ErrorResponse>)> {
    let repo = require_repo(&state)?;

    {
        let guard = repo.lock().map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "repository lock poisoned".into(),
                }),
            )
        })?;
        guard.delete(&id).map_err(map_repo_error)?;
    }

    reload_engine_from_repo(&state).await;

    Ok(Json(DeleteResponse { deleted: id }))
}

/// `GET /api/v1/triggers/:id`, full definition plus runtime status.
pub async fn get_trigger_by_id<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path(id): Path<String>,
) -> Result<Json<TriggerDefinitionResponse>, (StatusCode, Json<ErrorResponse>)> {
    let repo = require_repo(&state)?;

    let row = {
        let guard = repo.lock().map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "repository lock poisoned".into(),
                }),
            )
        })?;
        guard.get(&id).map_err(map_repo_error)?
    };

    let row = row.ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: format!("trigger not found: {id}"),
            }),
        )
    })?;

    Ok(Json(row_to_response(row)))
}

// ─── Trigger Action Handlers ──────────────────────────────────────────────

/// `GET /api/v1/triggers`, list all triggers with their status.
pub async fn list_triggers<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
) -> impl IntoResponse {
    let engine = match &state.trigger_engine {
        Some(e) => e.clone(),
        None => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({"error": "TriggerEngine not available"})),
            )
                .into_response();
        }
    };

    let statuses = engine.list().await;
    Json(serde_json::json!({ "triggers": statuses })).into_response()
}

/// `GET /api/v1/triggers/:id`, detailed status of a trigger (legacy, via TriggerEngine).
pub async fn get_trigger<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let engine = match &state.trigger_engine {
        Some(e) => e.clone(),
        None => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(ErrorResponse {
                    error: "TriggerEngine not available".into(),
                }),
            )
                .into_response();
        }
    };

    let def = match engine.get_definition(&id).await {
        Some(d) => d,
        None => {
            return (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: format!("trigger '{id}' not found"),
                }),
            )
                .into_response();
        }
    };

    let status = engine
        .list()
        .await
        .into_iter()
        .find(|s| s.id == id)
        .unwrap_or_else(|| apollia_triggers::TriggerStatus {
            id: def.id.clone(),
            agent: def.agent.clone(),
            source_kind: source_detail_kind(&def.source),
            source_config: source_kind_and_detail(&def.source).1,
            enabled: def.enabled,
            fire_count: 0,
            skip_count: 0,
            last_fired: None,
        });

    let (source_kind, source_detail) = source_kind_and_detail(&def.source);
    let on_busy = match def.on_busy {
        apollia_triggers::OnBusyPolicy::Queue { .. } => "queue",
        apollia_triggers::OnBusyPolicy::Skip => "skip",
        apollia_triggers::OnBusyPolicy::Block => "block",
    };

    let detail = TriggerDetailResponse {
        id: status.id,
        agent: status.agent,
        source_kind,
        source_detail,
        on_busy: on_busy.to_string(),
        enabled: status.enabled,
        fire_count: status.fire_count,
        skip_count: status.skip_count,
        last_fired: status.last_fired.map(|dt| dt.to_rfc3339()),
    };

    Json(detail).into_response()
}

/// `POST /api/v1/triggers/:id/fire`, immediate firing.
pub async fn fire_trigger<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let engine = match &state.trigger_engine {
        Some(e) => e.clone(),
        None => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(ErrorResponse {
                    error: "TriggerEngine not available".into(),
                }),
            )
                .into_response();
        }
    };

    match engine.fire_now(&id).await {
        Ok(task_id) => Json(FireResponse {
            task_id: task_id.to_string(),
        })
        .into_response(),
        Err(TriggerEngineError::NotFound { .. }) => (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: format!("trigger '{id}' not found"),
            }),
        )
            .into_response(),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: e.to_string(),
            }),
        )
            .into_response(),
    }
}

/// `POST /api/v1/triggers/:id/enable`, enable a disabled trigger.
pub async fn enable_trigger<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let engine = match &state.trigger_engine {
        Some(e) => e.clone(),
        None => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(ErrorResponse {
                    error: "TriggerEngine not available".into(),
                }),
            )
                .into_response();
        }
    };

    match engine.enable(&id).await {
        Ok(()) => Json(OkResponse { ok: true }).into_response(),
        Err(TriggerEngineError::NotFound { .. }) => (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: format!("trigger '{id}' not found"),
            }),
        )
            .into_response(),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: e.to_string(),
            }),
        )
            .into_response(),
    }
}

/// `POST /api/v1/triggers/:id/disable`, disable an active trigger.
pub async fn disable_trigger<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let engine = match &state.trigger_engine {
        Some(e) => e.clone(),
        None => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(ErrorResponse {
                    error: "TriggerEngine not available".into(),
                }),
            )
                .into_response();
        }
    };

    match engine.disable(&id).await {
        Ok(()) => Json(OkResponse { ok: true }).into_response(),
        Err(TriggerEngineError::NotFound { .. }) => (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: format!("trigger '{id}' not found"),
            }),
        )
            .into_response(),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: e.to_string(),
            }),
        )
            .into_response(),
    }
}

/// `GET /api/v1/triggers/:id/logs`, firing history from SQLite.
///
/// The `?last=N` query parameter controls the number of entries (default: 20).
pub async fn get_trigger_logs<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path(id): Path<String>,
    Query(params): Query<LogsQuery>,
) -> impl IntoResponse {
    let engine = match &state.trigger_engine {
        Some(e) => e.clone(),
        None => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(ErrorResponse {
                    error: "TriggerEngine not available".into(),
                }),
            )
                .into_response();
        }
    };

    let entries = engine.query_history(&id, params.last).await;
    Json(LogsResponse { entries }).into_response()
}

// ─── Reload Handler ───────────────────────────────────────────────────────

/// Axum handler for `POST /api/v1/triggers/reload`.
///
/// Re-reads definitions from the SQLite repository (no longer TOML),
/// converts them to rich types, and reloads the `TriggerEngine`.
/// Emits `TriggersReloaded` on the EventBus via the engine.
pub async fn reload_triggers<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
) -> impl IntoResponse {
    let engine = match &state.trigger_engine {
        Some(e) => e.clone(),
        None => return StatusCode::SERVICE_UNAVAILABLE.into_response(),
    };

    let repo = match &state.trigger_def_repo {
        Some(r) => r.clone(),
        None => return StatusCode::SERVICE_UNAVAILABLE.into_response(),
    };

    let rows = {
        let guard = match repo.lock() {
            Ok(g) => g,
            Err(_) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        error: "repository lock poisoned".into(),
                    }),
                )
                    .into_response();
            }
        };
        match guard.list() {
            Ok(rows) => rows,
            Err(e) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        error: format!("failed to list triggers: {e}"),
                    }),
                )
                    .into_response();
            }
        }
    };

    let definitions: Vec<_> = rows
        .into_iter()
        .filter_map(|row| apollia_triggers::TriggerDefinition::try_from(row).ok())
        .collect();

    let count = definitions.iter().filter(|d| d.enabled).count();

    engine.reload(definitions).await;

    Json(ReloadResponse { reloaded: count }).into_response()
}

// ─── Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::server::AppState;
    use crate::coordinator::ExecutionBackend;
    use crate::eventbus::EventBus;
    use crate::registry::AgentRegistry;
    use crate::router::TaskRouterHandle;
    use apollia_core::{AIPInput, AIPResult, AIPTask, TaskId, TaskStatus};
    use apollia_triggers::{TaskSubmitter, TriggerDefinitionRepository, TriggerEngineHandle};
    use axum::body::Body;
    use axum::http::Request;
    use axum::routing::{delete, get, post, put};
    use axum::Router;
    use http_body_util::BodyExt;
    use std::future::Future;
    use std::path::PathBuf;
    use std::pin::Pin;
    use std::sync::Arc;
    use tower::ServiceExt;

    #[derive(Clone)]
    struct MockBackend;

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

    /// Minimal mock of `TaskSubmitter` for route tests.
    struct MockSubmitter;

    impl TaskSubmitter for MockSubmitter {
        fn submit<'a>(
            &'a self,
            _agent: &'a str,
            _input: AIPInput,
        ) -> Pin<Box<dyn Future<Output = Result<TaskId, String>> + Send + 'a>> {
            Box::pin(async { Ok(TaskId::new_v4()) })
        }

        fn pending_count<'a>(
            &'a self,
            _agent: &'a str,
        ) -> Pin<Box<dyn Future<Output = usize> + Send + 'a>> {
            Box::pin(async { 0 })
        }
    }

    /// Builds a minimal `AppState` for CRUD tests.
    async fn make_state_with_repo(trigger_db_path: &std::path::Path) -> AppState<MockBackend> {
        let (event_tx, _) = EventBus::new();
        let registry = AgentRegistry::spawn(event_tx.clone());
        let router: TaskRouterHandle<MockBackend> =
            TaskRouterHandle::spawn(registry.clone(), event_tx.clone(), 64);

        let trigger_engine = TriggerEngineHandle::spawn(
            vec![],
            MockSubmitter,
            event_tx.clone(),
            None,
            apollia_core::ObservabilityConfig::default(),
        )
        .await;

        let repo = TriggerDefinitionRepository::open(trigger_db_path).expect("open test repo");

        AppState {
            router_handle: router,
            registry_handle: registry,
            event_sender: event_tx,
            agent_loader: Arc::new(crate::api::routes_agents::StubAgentLoader),
            backend: MockBackend,
            llm_router: crate::api::server::empty_shared_llm_router(),
            trigger_engine: Some(trigger_engine),
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
            trigger_def_repo: Some(Arc::new(std::sync::Mutex::new(repo))),
            notification_repo: None,
            notification_engine_handle: None,
            chat_manager: None,
            plan_cache: None,
            mailbox_handle: None,
            user_memory: None,
            stt_engine: None,
            stt_repository: None,
            mcp_handle: None,
            mcp_server_repo: None,
            llm_backend_repo: None,
            stt_config_repo: None,
            a2a_invoker: None,
            resilience_layer: None,
            runner_proxy: None,
        }
    }

    /// Builds a router with all CRUD routes.
    fn make_crud_router(state: AppState<MockBackend>) -> Router {
        Router::new()
            .route(
                "/api/v1/triggers",
                get(list_triggers::<MockBackend>).post(create_trigger::<MockBackend>),
            )
            .route(
                "/api/v1/triggers/reload",
                post(reload_triggers::<MockBackend>),
            )
            .route(
                "/api/v1/triggers/:id",
                get(get_trigger_by_id::<MockBackend>)
                    .put(update_trigger::<MockBackend>)
                    .delete(delete_trigger::<MockBackend>),
            )
            .with_state(state)
    }

    /// Creates a JSON body for a valid cron trigger.
    fn cron_trigger_body(id: &str, agent: &str, schedule: &str) -> String {
        serde_json::json!({
            "id": id,
            "agent": agent,
            "source": {
                "type": "cron",
                "schedule": schedule
            }
        })
        .to_string()
    }

    /// Reads the response body as a `serde_json::Value`.
    async fn read_body(resp: axum::http::Response<Body>) -> serde_json::Value {
        let bytes = resp
            .into_body()
            .collect()
            .await
            .expect("read body")
            .to_bytes();
        serde_json::from_slice(&bytes).expect("parse JSON")
    }

    // ── POST /api/v1/triggers → 201 ─────────────────────────────────────

    #[tokio::test]
    async fn test_create_trigger_201() {
        // GIVEN an empty repository
        let dir = tempfile::TempDir::new().expect("tempdir");
        let state = make_state_with_repo(&dir.path().join("triggers.db")).await;
        let router = make_crud_router(state);

        // WHEN POST /api/v1/triggers with a valid cron trigger
        let body = cron_trigger_body("rapport-hebdo", "rapport-agent", "0 0 8 * * MON *");
        let req = Request::builder()
            .method("POST")
            .uri("/api/v1/triggers")
            .header("content-type", "application/json")
            .body(Body::from(body))
            .expect("build request");
        let resp = router.oneshot(req).await.expect("oneshot");

        // THEN 201 with the full definition
        assert_eq!(resp.status(), StatusCode::CREATED);
        let json = read_body(resp).await;
        assert_eq!(json["id"], "rapport-hebdo");
        assert_eq!(json["agent"], "rapport-agent");
        assert_eq!(json["source_type"], "cron");
        assert!(json["created_at"].as_str().is_some_and(|s| !s.is_empty()));
    }

    // ── PUT /api/v1/triggers/:id → 200 ──────────────────────────────────

    #[tokio::test]
    async fn test_update_trigger_200() {
        // GIVEN an existing "rapport-hebdo" trigger
        let dir = tempfile::TempDir::new().expect("tempdir");
        let state = make_state_with_repo(&dir.path().join("triggers.db")).await;
        let router = make_crud_router(state);

        let create_body = cron_trigger_body("rapport-hebdo", "rapport-agent", "0 0 8 * * MON *");
        let req = Request::builder()
            .method("POST")
            .uri("/api/v1/triggers")
            .header("content-type", "application/json")
            .body(Body::from(create_body))
            .expect("build request");
        let resp = router.clone().oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::CREATED);

        // WHEN PUT /api/v1/triggers/rapport-hebdo with a new schedule
        let update_body = serde_json::json!({
            "agent": "rapport-agent",
            "source": {
                "type": "cron",
                "schedule": "0 0 9 * * MON *"
            }
        })
        .to_string();

        let req = Request::builder()
            .method("PUT")
            .uri("/api/v1/triggers/rapport-hebdo")
            .header("content-type", "application/json")
            .body(Body::from(update_body))
            .expect("build request");
        let resp = router.oneshot(req).await.expect("oneshot");

        // THEN 200 with the updated definition
        assert_eq!(resp.status(), StatusCode::OK);
        let json = read_body(resp).await;
        assert_eq!(json["id"], "rapport-hebdo");
        assert_eq!(
            json["source_config"]["schedule"].as_str(),
            Some("0 0 9 * * MON *")
        );
    }

    // ── DELETE /api/v1/triggers/:id → 200 ────────────────────────────────

    #[tokio::test]
    async fn test_delete_trigger_200() {
        // GIVEN an existing "rapport-hebdo" trigger
        let dir = tempfile::TempDir::new().expect("tempdir");
        let state = make_state_with_repo(&dir.path().join("triggers.db")).await;
        let router = make_crud_router(state);

        let create_body = cron_trigger_body("rapport-hebdo", "rapport-agent", "0 0 8 * * MON *");
        let req = Request::builder()
            .method("POST")
            .uri("/api/v1/triggers")
            .header("content-type", "application/json")
            .body(Body::from(create_body))
            .expect("build request");
        let resp = router.clone().oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::CREATED);

        // WHEN DELETE /api/v1/triggers/rapport-hebdo
        let req = Request::builder()
            .method("DELETE")
            .uri("/api/v1/triggers/rapport-hebdo")
            .body(Body::empty())
            .expect("build request");
        let resp = router.oneshot(req).await.expect("oneshot");

        // THEN 200 with {"deleted": "rapport-hebdo"}
        assert_eq!(resp.status(), StatusCode::OK);
        let json = read_body(resp).await;
        assert_eq!(json["deleted"], "rapport-hebdo");
    }

    // ── GET /api/v1/triggers/:id → 200 ──────────────────────────────────

    #[tokio::test]
    async fn test_get_trigger_200() {
        // GIVEN an existing "rapport-hebdo" trigger
        let dir = tempfile::TempDir::new().expect("tempdir");
        let state = make_state_with_repo(&dir.path().join("triggers.db")).await;
        let router = make_crud_router(state);

        let create_body = cron_trigger_body("rapport-hebdo", "rapport-agent", "0 0 8 * * MON *");
        let req = Request::builder()
            .method("POST")
            .uri("/api/v1/triggers")
            .header("content-type", "application/json")
            .body(Body::from(create_body))
            .expect("build request");
        let resp = router.clone().oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::CREATED);

        // WHEN GET /api/v1/triggers/rapport-hebdo
        let req = Request::builder()
            .method("GET")
            .uri("/api/v1/triggers/rapport-hebdo")
            .body(Body::empty())
            .expect("build request");
        let resp = router.oneshot(req).await.expect("oneshot");

        // THEN 200 with the full definition
        assert_eq!(resp.status(), StatusCode::OK);
        let json = read_body(resp).await;
        assert_eq!(json["id"], "rapport-hebdo");
        assert_eq!(json["agent"], "rapport-agent");
        assert_eq!(json["source_type"], "cron");
        assert_eq!(json["enabled"], true);
        assert_eq!(json["on_busy"], "queue");
    }

    // ── Invalid cron → 422 ──────────────────────────────────────────────

    #[tokio::test]
    async fn test_create_invalid_cron_422() {
        // GIVEN an empty repository
        let dir = tempfile::TempDir::new().expect("tempdir");
        let state = make_state_with_repo(&dir.path().join("triggers.db")).await;
        let router = make_crud_router(state);

        // WHEN POST with an invalid cron
        let body = cron_trigger_body("bad-cron", "agent", "not-a-cron");
        let req = Request::builder()
            .method("POST")
            .uri("/api/v1/triggers")
            .header("content-type", "application/json")
            .body(Body::from(body))
            .expect("build request");
        let resp = router.oneshot(req).await.expect("oneshot");

        // THEN 422 with a validation message
        assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let json = read_body(resp).await;
        let error = json["error"].as_str().expect("error field");
        assert!(
            error.contains("invalid cron expression"),
            "expected 'invalid cron expression' in: {error}"
        );
    }

    // ── Duplicate ID → 409 ──────────────────────────────────────────────

    #[tokio::test]
    async fn test_create_duplicate_409() {
        // GIVEN an existing "rapport-hebdo" trigger
        let dir = tempfile::TempDir::new().expect("tempdir");
        let state = make_state_with_repo(&dir.path().join("triggers.db")).await;
        let router = make_crud_router(state);

        let body = cron_trigger_body("rapport-hebdo", "agent", "0 0 8 * * MON *");
        let req = Request::builder()
            .method("POST")
            .uri("/api/v1/triggers")
            .header("content-type", "application/json")
            .body(Body::from(body.clone()))
            .expect("build request");
        let resp = router.clone().oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::CREATED);

        // WHEN POST with the same ID
        let req = Request::builder()
            .method("POST")
            .uri("/api/v1/triggers")
            .header("content-type", "application/json")
            .body(Body::from(body))
            .expect("build request");
        let resp = router.oneshot(req).await.expect("oneshot");

        // THEN 409 with message "duplicate trigger id"
        assert_eq!(resp.status(), StatusCode::CONFLICT);
        let json = read_body(resp).await;
        let error = json["error"].as_str().expect("error field");
        assert!(
            error.contains("duplicate trigger id"),
            "expected 'duplicate trigger id' in: {error}"
        );
    }

    // ── Nonexistent ID → 404 ────────────────────────────────────────────

    #[tokio::test]
    async fn test_update_not_found_404() {
        // GIVEN an empty repository
        let dir = tempfile::TempDir::new().expect("tempdir");
        let state = make_state_with_repo(&dir.path().join("triggers.db")).await;
        let router = make_crud_router(state);

        // WHEN PUT /api/v1/triggers/inconnu
        let update_body = serde_json::json!({
            "agent": "agent",
            "source": {
                "type": "cron",
                "schedule": "0 0 8 * * MON *"
            }
        })
        .to_string();

        let req = Request::builder()
            .method("PUT")
            .uri("/api/v1/triggers/inconnu")
            .header("content-type", "application/json")
            .body(Body::from(update_body))
            .expect("build request");
        let resp = router.oneshot(req).await.expect("oneshot");

        // THEN 404 with message "trigger not found"
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
        let json = read_body(resp).await;
        let error = json["error"].as_str().expect("error field");
        assert!(
            error.contains("trigger not found"),
            "expected 'trigger not found' in: {error}"
        );
    }

    // ── Reload from SQLite ───────────────────────────────────────────────

    #[tokio::test]
    async fn test_reload_from_sqlite() {
        // GIVEN a trigger in the DB
        let dir = tempfile::TempDir::new().expect("tempdir");
        let state = make_state_with_repo(&dir.path().join("triggers.db")).await;
        let router = make_crud_router(state);

        let body = cron_trigger_body("reload-test", "agent", "0 0 8 * * MON *");
        let req = Request::builder()
            .method("POST")
            .uri("/api/v1/triggers")
            .header("content-type", "application/json")
            .body(Body::from(body))
            .expect("build request");
        let resp = router.clone().oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::CREATED);

        // WHEN POST /api/v1/triggers/reload
        let req = Request::builder()
            .method("POST")
            .uri("/api/v1/triggers/reload")
            .body(Body::empty())
            .expect("build request");
        let resp = router.oneshot(req).await.expect("oneshot");

        // THEN 200 with reloaded >= 1
        assert_eq!(resp.status(), StatusCode::OK);
        let json = read_body(resp).await;
        assert!(
            json["reloaded"].as_u64().is_some_and(|n| n >= 1),
            "expected reloaded >= 1, got: {json}"
        );
    }

    // ── 503 when TriggerEngine is absent ────────────────────────────────

    #[tokio::test]
    async fn test_reload_503_when_no_trigger_engine() {
        // GIVEN state without TriggerEngine or repo
        let (event_tx, _) = EventBus::new();
        let registry = AgentRegistry::spawn(event_tx.clone());
        let router_handle: TaskRouterHandle<MockBackend> =
            TaskRouterHandle::spawn(registry.clone(), event_tx.clone(), 64);

        let state = AppState {
            router_handle,
            registry_handle: registry,
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
            mcp_handle: None,
            mcp_server_repo: None,
            llm_backend_repo: None,
            stt_config_repo: None,
            a2a_invoker: None,
            resilience_layer: None,
            runner_proxy: None,
        };

        let router = Router::new()
            .route(
                "/api/v1/triggers/reload",
                post(reload_triggers::<MockBackend>),
            )
            .with_state(state);

        // WHEN POST /api/v1/triggers/reload
        let req = Request::builder()
            .method("POST")
            .uri("/api/v1/triggers/reload")
            .body(Body::empty())
            .expect("build request");
        let resp = router.oneshot(req).await.expect("oneshot");

        // THEN 503
        assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
    }
}
