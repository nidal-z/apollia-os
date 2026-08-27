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

use apollia_triggers::{TriggerDefinitionRow, TriggerEngineError};

use crate::api::server::AppState;
use crate::coordinator::ExecutionBackend;

mod dto;
mod support;

pub use dto::{
    CreateTriggerRequest, DeleteResponse, ErrorResponse, FireResponse, LogsQuery, LogsResponse,
    OkResponse, ReloadResponse, TriggerDefinitionResponse, TriggerDetailResponse,
    TriggerSourceInput, UpdateTriggerRequest,
};

use support::{
    map_repo_error, parse_on_busy, reload_engine_from_repo, require_repo, row_to_response,
    source_detail_kind, source_kind_and_detail,
};

// ─── CRUD Handlers ───────────────────────────────────────────────────────

/// `POST /api/v1/triggers`, create a new trigger definition.
///
/// Validates the definition, inserts it into `triggers.db`, reloads the engine,
/// and returns `201` with the full definition (including `created_at`).
#[utoipa::path(
    post,
    path = "/api/v1/triggers",
    tag = "triggers",
    request_body = CreateTriggerRequest,
    responses(
        (status = 201, description = "Trigger created", body = TriggerDefinitionResponse),
        (status = 409, description = "Trigger id already exists", body = crate::api::openapi::ApiErrorBody),
        (status = 422, description = "Validation error", body = crate::api::openapi::ApiErrorBody),
        (status = 500, description = "Repository error", body = crate::api::openapi::ApiErrorBody),
        (status = 503, description = "Trigger repository unavailable", body = crate::api::openapi::ApiErrorBody),
    )
)]
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
#[utoipa::path(
    put,
    path = "/api/v1/triggers/{id}",
    tag = "triggers",
    params(("id" = String, Path, description = "Trigger id")),
    request_body = UpdateTriggerRequest,
    responses(
        (status = 200, description = "Trigger updated", body = TriggerDefinitionResponse),
        (status = 404, description = "Trigger not found", body = crate::api::openapi::ApiErrorBody),
        (status = 422, description = "Validation error", body = crate::api::openapi::ApiErrorBody),
        (status = 500, description = "Repository error", body = crate::api::openapi::ApiErrorBody),
        (status = 503, description = "Trigger repository unavailable", body = crate::api::openapi::ApiErrorBody),
    )
)]
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
#[utoipa::path(
    delete,
    path = "/api/v1/triggers/{id}",
    tag = "triggers",
    params(("id" = String, Path, description = "Trigger id")),
    responses(
        (status = 200, description = "Trigger deleted", body = DeleteResponse),
        (status = 404, description = "Trigger not found", body = crate::api::openapi::ApiErrorBody),
        (status = 500, description = "Repository error", body = crate::api::openapi::ApiErrorBody),
        (status = 503, description = "Trigger repository unavailable", body = crate::api::openapi::ApiErrorBody),
    )
)]
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
#[utoipa::path(
    get,
    path = "/api/v1/triggers/{id}",
    tag = "triggers",
    params(("id" = String, Path, description = "Trigger id")),
    responses(
        (status = 200, description = "Trigger definition", body = TriggerDefinitionResponse),
        (status = 404, description = "Trigger not found", body = crate::api::openapi::ApiErrorBody),
        (status = 500, description = "Repository error", body = crate::api::openapi::ApiErrorBody),
        (status = 503, description = "Trigger repository unavailable", body = crate::api::openapi::ApiErrorBody),
    )
)]
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
#[utoipa::path(
    get,
    path = "/api/v1/triggers",
    tag = "triggers",
    responses(
        (status = 200, description = "Trigger list with runtime status"),
        (status = 503, description = "Trigger engine unavailable", body = crate::api::openapi::ApiErrorBody),
    )
)]
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
#[utoipa::path(
    post,
    path = "/api/v1/triggers/{id}/fire",
    tag = "triggers",
    params(("id" = String, Path, description = "Trigger id")),
    responses(
        (status = 200, description = "Trigger fired", body = FireResponse),
        (status = 400, description = "Trigger could not be fired", body = crate::api::openapi::ApiErrorBody),
        (status = 404, description = "Trigger not found", body = crate::api::openapi::ApiErrorBody),
        (status = 503, description = "Trigger engine unavailable", body = crate::api::openapi::ApiErrorBody),
    )
)]
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
#[utoipa::path(
    post,
    path = "/api/v1/triggers/{id}/enable",
    tag = "triggers",
    params(("id" = String, Path, description = "Trigger id")),
    responses(
        (status = 200, description = "Trigger enabled", body = OkResponse),
        (status = 400, description = "Trigger could not be enabled", body = crate::api::openapi::ApiErrorBody),
        (status = 404, description = "Trigger not found", body = crate::api::openapi::ApiErrorBody),
        (status = 503, description = "Trigger engine unavailable", body = crate::api::openapi::ApiErrorBody),
    )
)]
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
#[utoipa::path(
    post,
    path = "/api/v1/triggers/{id}/disable",
    tag = "triggers",
    params(("id" = String, Path, description = "Trigger id")),
    responses(
        (status = 200, description = "Trigger disabled", body = OkResponse),
        (status = 400, description = "Trigger could not be disabled", body = crate::api::openapi::ApiErrorBody),
        (status = 404, description = "Trigger not found", body = crate::api::openapi::ApiErrorBody),
        (status = 503, description = "Trigger engine unavailable", body = crate::api::openapi::ApiErrorBody),
    )
)]
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
#[utoipa::path(
    get,
    path = "/api/v1/triggers/{id}/logs",
    tag = "triggers",
    params(
        ("id" = String, Path, description = "Trigger id"),
        ("last" = Option<usize>, Query, description = "Maximum number of entries (default 20)"),
    ),
    responses(
        (status = 200, description = "Trigger firing history", body = LogsResponse),
        (status = 503, description = "Trigger engine unavailable", body = crate::api::openapi::ApiErrorBody),
    )
)]
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
#[utoipa::path(
    post,
    path = "/api/v1/triggers/reload",
    tag = "triggers",
    responses(
        (status = 200, description = "Triggers reloaded", body = ReloadResponse),
        (status = 500, description = "Repository error", body = crate::api::openapi::ApiErrorBody),
        (status = 503, description = "Trigger engine unavailable", body = crate::api::openapi::ApiErrorBody),
    )
)]
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
    use axum::routing::{get, post};
    use axum::Router;
    use http_body_util::BodyExt;
    use std::future::Future;
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
            data_dir: std::path::PathBuf::new(),
            stt_engine: crate::api::server::empty_shared_stt_engine(),
            stt_repository: crate::api::server::empty_shared_stt_repository(),
            mcp_handle: None,
            mcp_server_repo: None,
            llm_backend_repo: None,
            stt_config_repo: None,
            a2a_invoker: None,
            resilience_layer: None,
            runner_proxy: None,
            llama_server_supervisor: None,
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

    // ── POST with a 5-field scheduler preset → persisted normalized ─────

    #[tokio::test]
    async fn test_create_trigger_five_field_cron_persists_normalized_schedule() {
        // GIVEN an empty repository
        let dir = tempfile::TempDir::new().expect("tempdir");
        let state = make_state_with_repo(&dir.path().join("triggers.db")).await;
        let router = make_crud_router(state);

        // WHEN POST with the 5-field expression the desktop 15-minute preset emits
        let body = cron_trigger_body("bureau-15m", "rapport-agent", "*/15 * * * *");
        let req = Request::builder()
            .method("POST")
            .uri("/api/v1/triggers")
            .header("content-type", "application/json")
            .body(Body::from(body))
            .expect("build request");
        let resp = router.clone().oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::CREATED);

        // THEN the persisted row, re-read through GET, carries the 6-field form
        // (the apollia-triggers repository test proves Schedule::from_str
        // accepts exactly this form verbatim)
        let req = Request::builder()
            .method("GET")
            .uri("/api/v1/triggers/bureau-15m")
            .body(Body::empty())
            .expect("build request");
        let resp = router.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::OK);
        let json = read_body(resp).await;
        assert_eq!(
            json["source_config"]["schedule"].as_str(),
            Some("0 */15 * * * *")
        );
    }

    // ── PUT with a 5-field scheduler preset → persisted normalized ──────

    #[tokio::test]
    async fn test_update_trigger_five_field_cron_persists_normalized_schedule() {
        // GIVEN an existing trigger with a directly parseable schedule
        let dir = tempfile::TempDir::new().expect("tempdir");
        let state = make_state_with_repo(&dir.path().join("triggers.db")).await;
        let router = make_crud_router(state);

        let create_body = cron_trigger_body("bureau-daily", "rapport-agent", "0 0 8 * * MON *");
        let req = Request::builder()
            .method("POST")
            .uri("/api/v1/triggers")
            .header("content-type", "application/json")
            .body(Body::from(create_body))
            .expect("build request");
        let resp = router.clone().oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::CREATED);

        // WHEN PUT with the 5-field expression the desktop daily preset emits
        let update_body = serde_json::json!({
            "agent": "rapport-agent",
            "source": {
                "type": "cron",
                "schedule": "30 8 * * *"
            }
        })
        .to_string();
        let req = Request::builder()
            .method("PUT")
            .uri("/api/v1/triggers/bureau-daily")
            .header("content-type", "application/json")
            .body(Body::from(update_body))
            .expect("build request");
        let resp = router.oneshot(req).await.expect("oneshot");

        // THEN 200 and the persisted definition carries the 6-field form
        assert_eq!(resp.status(), StatusCode::OK);
        let json = read_body(resp).await;
        assert_eq!(
            json["source_config"]["schedule"].as_str(),
            Some("0 30 8 * * *")
        );
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

        // WHEN PUT /api/v1/triggers/no-such-trigger
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
            .uri("/api/v1/triggers/no-such-trigger")
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
            data_dir: std::path::PathBuf::new(),
            stt_engine: crate::api::server::empty_shared_stt_engine(),
            stt_repository: crate::api::server::empty_shared_stt_repository(),
            mcp_handle: None,
            mcp_server_repo: None,
            llm_backend_repo: None,
            stt_config_repo: None,
            a2a_invoker: None,
            resilience_layer: None,
            runner_proxy: None,
            llama_server_supervisor: None,
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
