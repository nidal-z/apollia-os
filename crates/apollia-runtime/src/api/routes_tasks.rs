//! REST routes for task management — POST/GET/DELETE/resume `/api/v1/tasks`.
//!
//! These routes are the core API surface for submitting, querying, canceling,
//! and resuming agent tasks. They delegate to [`TaskRouterHandle`] for dispatch
//! and status tracking, and to [`TaskRepository`] for HITL persistence.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::api::server::AppState;
use crate::coordinator::ExecutionBackend;
use crate::router::SubmitError;

use apollia_core::{AIPInput, AIPPart, DataPart, InputResponseData, RuntimeEvent, TaskId};

/// Request body for `POST /api/v1/tasks`.
#[derive(Debug, Deserialize)]
pub struct SubmitTaskRequest {
    /// Identifier of the target agent.
    pub agent_id: String,
    /// Free-form JSON input for the task.
    pub input: serde_json::Value,
}

/// Response body for task operations.
#[derive(Debug, Serialize)]
pub struct TaskResponse {
    /// Unique task identifier (UUID v4).
    pub task_id: String,
    /// Current task status.
    pub status: String,
    /// Task result payload (present when completed).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    /// Error message (present when failed).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Standard error response body.
#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    /// Human-readable error description.
    pub error: String,
}

/// Convert a [`SubmitError`] into an HTTP status code and error response.
fn submit_error_to_response(err: SubmitError) -> (StatusCode, Json<ErrorResponse>) {
    let (status, message) = match &err {
        SubmitError::AgentNotFound(_) => (StatusCode::NOT_FOUND, err.to_string()),
        SubmitError::AgentNotReady(_) => (StatusCode::SERVICE_UNAVAILABLE, err.to_string()),
        SubmitError::AgentUnavailable(_) => (StatusCode::SERVICE_UNAVAILABLE, err.to_string()),
        SubmitError::ConcurrencyLimit(_) => (StatusCode::SERVICE_UNAVAILABLE, err.to_string()),
        SubmitError::NoCoordinator(_) => (StatusCode::SERVICE_UNAVAILABLE, err.to_string()),
        SubmitError::ActorDead => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
    };
    (status, Json(ErrorResponse { error: message }))
}

/// Convert free-form JSON input into an [`AIPInput`] with a single `DataPart`.
fn json_to_aip_input(value: serde_json::Value) -> AIPInput {
    AIPInput {
        parts: vec![AIPPart::Data(DataPart { data: value })],
    }
}

/// Handler for `POST /api/v1/tasks`.
///
/// Submits a new task to the specified agent via the TaskRouter.
/// Returns 202 Accepted with the generated task_id on success.
pub async fn submit_task<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Json(req): Json<SubmitTaskRequest>,
) -> Result<(StatusCode, Json<TaskResponse>), (StatusCode, Json<ErrorResponse>)> {
    let input = json_to_aip_input(req.input);

    let task_id = state
        .router_handle
        .submit(&req.agent_id, input)
        .await
        .map_err(submit_error_to_response)?;

    Ok((
        StatusCode::ACCEPTED,
        Json(TaskResponse {
            task_id: task_id.to_string(),
            status: "submitted".into(),
            result: None,
            error: None,
        }),
    ))
}

/// Handler for `GET /api/v1/tasks/{id}`.
///
/// Returns the current status of a task. Returns 404 if the task does not exist.
pub async fn get_task<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path(task_id): Path<String>,
) -> Result<Json<TaskResponse>, (StatusCode, Json<ErrorResponse>)> {
    let status = state
        .router_handle
        .get_status(&task_id)
        .await
        .map_err(submit_error_to_response)?;

    match status {
        Some(s) => Ok(Json(TaskResponse {
            task_id,
            status: serde_json::to_value(&s)
                .ok()
                .and_then(|v| v.as_str().map(String::from))
                .unwrap_or_else(|| format!("{s:?}")),
            result: None,
            error: None,
        })),
        None => Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: format!("task not found: {task_id}"),
            }),
        )),
    }
}

/// Handler for `DELETE /api/v1/tasks/{id}`.
///
/// Cancels a running task. Returns 404 if the task does not exist.
pub async fn cancel_task<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path(task_id): Path<String>,
) -> Result<Json<TaskResponse>, (StatusCode, Json<ErrorResponse>)> {
    let status = state
        .router_handle
        .cancel(&task_id)
        .await
        .map_err(submit_error_to_response)?;

    match status {
        Some(s) => Ok(Json(TaskResponse {
            task_id,
            status: serde_json::to_value(&s)
                .ok()
                .and_then(|v| v.as_str().map(String::from))
                .unwrap_or_else(|| format!("{s:?}")),
            result: None,
            error: None,
        })),
        None => Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: format!("task not found: {task_id}"),
            }),
        )),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// ResumeHandler — POST /api/v1/tasks/{id}/resume
// ─────────────────────────────────────────────────────────────────────────────

/// Body de la requête `POST /api/v1/tasks/{id}/resume`.
///
/// L'opérateur transmet sa décision (`approved`) et une raison optionnelle.
/// Le champ `approved` est obligatoire — son absence provoque HTTP 422.
#[derive(Debug, Deserialize)]
pub struct ResumeRequest {
    /// `true` pour approuver, `false` pour rejeter.
    pub approved: bool,
    /// Raison de la décision — optionnelle, surtout utile en cas de rejet.
    pub reason: Option<String>,
}

/// Réponse de la route `POST /api/v1/tasks/{id}/resume`.
///
/// Retournée avec HTTP 200 quand la reprise est enregistrée avec succès.
#[derive(Debug, Serialize)]
pub struct ResumeResponse {
    /// Identifiant de la tâche reprise.
    pub task_id: String,
    /// Décision de l'opérateur.
    pub approved: bool,
    /// Nouveau statut de la tâche (`"working"` après approbation ou rejet).
    pub status: String,
}

/// Handler pour `POST /api/v1/tasks/{id}/resume`.
///
/// Valide que la tâche est en status `input_required`, persiste la décision
/// humaine dans SQLite, émet `RuntimeEvent::TaskResumed` sur l'EventBus,
/// et reconstruit l'`AIPTask` enrichi pour la relance ORIA (STORY-096).
///
/// ## Codes HTTP
/// - `200 OK` — reprise enregistrée avec succès
/// - `404 Not Found` — tâche inconnue du système HITL
/// - `409 Conflict` — tâche connue mais pas en status `input_required`
/// - `503 Service Unavailable` — HITL non configuré (`task_repository` absent)
/// - `500 Internal Server Error` — erreur SQLite ou interne
pub async fn resume_task<B: ExecutionBackend + Clone>(
    Path(task_id): Path<String>,
    State(state): State<AppState<B>>,
    Json(body): Json<ResumeRequest>,
) -> Result<Json<ResumeResponse>, (StatusCode, Json<ErrorResponse>)> {
    // ── Vérifier que le TaskRepository est disponible ────────────────────────
    let repo = match state.task_repository.as_ref() {
        Some(r) => r,
        None => {
            return Err((
                StatusCode::SERVICE_UNAVAILABLE,
                Json(ErrorResponse {
                    error: "HITL not configured — task_repository absent".into(),
                }),
            ));
        }
    };

    // ── AC-4 / AC-3 : vérifier le statut via TaskRepository ─────────────────
    let db_status = repo.get_task_status(&task_id).await.map_err(|e| {
        tracing::error!(task_id = %task_id, error = %e, "get_task_status failed");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("database error: {e}"),
            }),
        )
    })?;

    match db_status.as_deref() {
        // AC-4 — tâche absente de la table tasks → 404
        None => {
            return Err((
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: format!("task not found: {task_id}"),
                }),
            ));
        }
        // AC-3 — tâche présente mais pas en input_required → 409
        Some(status) if status != "input_required" => {
            return Err((
                StatusCode::CONFLICT,
                Json(ErrorResponse {
                    error: format!(
                        "task '{task_id}' is not in input_required status (current: {status})"
                    ),
                }),
            ));
        }
        _ => {}
    }

    // ── Construire la réponse humaine avec horodatage ISO 8601 ───────────────
    let responded_at = chrono::Utc::now().to_rfc3339();
    let input_response = InputResponseData {
        approved: body.approved,
        reason: body.reason.clone(),
        context: serde_json::Value::Object(serde_json::Map::new()),
        responded_at,
    };

    // ── AC-2 : durabilité avant notification — DB write avant EventBus ───────
    repo.save_input_response(&task_id, &input_response)
        .await
        .map_err(|e| {
            tracing::error!(task_id = %task_id, error = %e, "save_input_response failed");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("failed to persist response: {e}"),
                }),
            )
        })?;

    // ── Émettre TaskResumed sur l'EventBus ──────────────────────────────────
    let _ = state.event_sender.send(RuntimeEvent::TaskResumed {
        task_id: TaskId::from(task_id.as_str()),
        approved: body.approved,
    });

    // ── Reconstruire l'AIPTask enrichi et résoudre le oneshot ORIA (STORY-096) ──
    match repo.rebuild_for_resume(&task_id).await {
        Ok(enriched_task) => {
            // Résoudre le oneshot PendingApprovals pour débloquer execute_direct()
            if let Some(pending) = state.pending_approvals.as_ref() {
                match pending.resolve(
                    &task_id,
                    enriched_task.input_response.clone().unwrap_or(
                        apollia_core::InputResponseData {
                            approved: body.approved,
                            reason: body.reason.clone(),
                            context: serde_json::Value::Null,
                            responded_at: chrono::Utc::now().to_rfc3339(),
                        },
                    ),
                ) {
                    Ok(()) => {
                        tracing::info!(
                            task_id = %task_id,
                            approved = body.approved,
                            "PendingApprovals resolved — ORIA execute_direct unblocked"
                        );
                    }
                    Err(e) => {
                        // Not a fatal error — the task may have been cleaned up or
                        // PendingApprovals was not registered (e.g. non-ORIA execution).
                        tracing::warn!(
                            task_id = %task_id,
                            error = %e,
                            "PendingApprovals.resolve failed — task may not be suspended via ORIA"
                        );
                    }
                }
            } else {
                tracing::warn!(
                    task_id = %task_id,
                    "PendingApprovals not configured in AppState — ORIA will not be unblocked"
                );
            }
        }
        Err(e) => {
            tracing::error!(task_id = %task_id, error = %e, "rebuild_for_resume failed");
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("failed to rebuild task for resume: {e}"),
                }),
            ));
        }
    }

    Ok(Json(ResumeResponse {
        task_id,
        approved: body.approved,
        status: "working".into(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::coordinator::ExecutionBackend;
    use crate::eventbus::EventBus;
    use crate::registry::AgentRegistry;
    use crate::router::TaskRouterHandle;
    use apollia_core::{
        AIPResult, AgentManifest, InputResponseData, ProcessState, RuntimeEvent, TaskStatus,
    };
    use std::future::Future;
    use std::pin::Pin;
    use tokio::sync::broadcast;

    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use axum::routing::{get, post};
    use axum::Router;
    use tower::ServiceExt;

    use crate::coordinator::ExecutionCoordinator;

    /// Minimal ExecutionBackend for testing (completes instantly).
    #[derive(Clone)]
    struct MockBackend;

    impl ExecutionBackend for MockBackend {
        fn execute(
            &self,
            task: apollia_core::AIPTask,
        ) -> Pin<Box<dyn Future<Output = Result<AIPResult, String>> + Send>> {
            Box::pin(async move {
                Ok(AIPResult {
                    task_id: task.task_id,
                    status: TaskStatus::Completed,
                    output: Vec::new(),
                    error: None,
                    artifacts: Vec::new(),
                    input_required_data: None,
                })
            })
        }
    }

    /// Backend that blocks forever — used for cancel tests to avoid race with TaskCompleted.
    #[derive(Clone)]
    struct NeverMockBackend;

    impl ExecutionBackend for NeverMockBackend {
        fn execute(
            &self,
            _task: apollia_core::AIPTask,
        ) -> Pin<Box<dyn Future<Output = Result<AIPResult, String>> + Send>> {
            Box::pin(async {
                std::future::pending::<()>().await;
                unreachable!()
            })
        }
    }

    fn test_manifest(name: &str) -> AgentManifest {
        AgentManifest {
            name: name.to_string(),
            version: "0.1.0".to_string(),
            description: String::new(),
            tools_required: vec![],
            tools_optional: vec![],
            supports_streaming: false,
            supports_a2a: false,
            memory_namespace: None,
            shared_memory_namespaces: vec![],
            max_concurrent_tasks: 1,
            step_budget: None,
            network_allowlist: None,
            dangerous_tools_allowed: false,
            tags: vec![],
            skills: vec![],
            execution_mode: "auto".to_string(),
            system_prompt: None,
            tools_requiring_approval: vec![],
        }
    }

    fn test_router() -> Router {
        let (event_tx, _) = EventBus::new();
        let registry_handle = AgentRegistry::spawn(event_tx.clone());
        let router_handle: TaskRouterHandle<MockBackend> =
            TaskRouterHandle::spawn(registry_handle.clone(), event_tx.clone(), 64);
        let state = AppState {
            router_handle,
            registry_handle,
            event_sender: event_tx,
            agent_loader: std::sync::Arc::new(crate::api::routes_agents::StubAgentLoader),
            backend: MockBackend,
            llm_router: None,
            trigger_engine: None,
            config_path: None,
            task_repository: None,
            pending_approvals: None,
        };
        Router::new()
            .route("/api/v1/tasks", post(submit_task::<MockBackend>))
            .route(
                "/api/v1/tasks/:id",
                get(get_task::<MockBackend>).delete(cancel_task::<MockBackend>),
            )
            .with_state(state)
    }

    /// Build a test environment with an active agent and coordinator registered.
    async fn test_router_with_agent() -> (Router, String) {
        let (event_tx, _) = broadcast::channel::<RuntimeEvent>(64);
        let registry_handle = AgentRegistry::spawn(event_tx.clone());
        let router_handle: TaskRouterHandle<MockBackend> =
            TaskRouterHandle::spawn(registry_handle.clone(), event_tx.clone(), 64);

        // Register agent and set Active
        let agent_id = registry_handle
            .register(test_manifest("test-agent"))
            .await
            .expect("register failed");
        registry_handle
            .update_state(agent_id.as_str(), ProcessState::Active)
            .await
            .expect("activate failed");

        // Register coordinator
        let coordinator =
            ExecutionCoordinator::new(agent_id.clone(), 1, event_tx.clone(), MockBackend);
        router_handle
            .register_coordinator(agent_id.clone(), coordinator)
            .await
            .expect("register coordinator failed");

        let state = AppState {
            router_handle,
            registry_handle,
            event_sender: event_tx,
            agent_loader: std::sync::Arc::new(crate::api::routes_agents::StubAgentLoader),
            backend: MockBackend,
            llm_router: None,
            trigger_engine: None,
            config_path: None,
            task_repository: None,
            pending_approvals: None,
        };
        let router = Router::new()
            .route("/api/v1/tasks", post(submit_task::<MockBackend>))
            .route(
                "/api/v1/tasks/:id",
                get(get_task::<MockBackend>).delete(cancel_task::<MockBackend>),
            )
            .with_state(state);

        (router, agent_id.to_string())
    }

    /// Build a test environment with a never-completing backend — for cancel tests.
    async fn test_router_with_blocking_agent() -> (Router, String) {
        let (event_tx, _) = broadcast::channel::<RuntimeEvent>(64);
        let registry_handle = AgentRegistry::spawn(event_tx.clone());
        let router_handle: TaskRouterHandle<NeverMockBackend> =
            TaskRouterHandle::spawn(registry_handle.clone(), event_tx.clone(), 64);

        let agent_id = registry_handle
            .register(test_manifest("blocking-agent"))
            .await
            .expect("register failed");
        registry_handle
            .update_state(agent_id.as_str(), ProcessState::Active)
            .await
            .expect("activate failed");

        let coordinator =
            ExecutionCoordinator::new(agent_id.clone(), 1, event_tx.clone(), NeverMockBackend);
        router_handle
            .register_coordinator(agent_id.clone(), coordinator)
            .await
            .expect("register coordinator failed");

        let state = AppState {
            router_handle,
            registry_handle,
            event_sender: event_tx,
            agent_loader: std::sync::Arc::new(crate::api::routes_agents::StubAgentLoader),
            backend: NeverMockBackend,
            llm_router: None,
            trigger_engine: None,
            config_path: None,
            task_repository: None,
            pending_approvals: None,
        };
        let router = Router::new()
            .route("/api/v1/tasks", post(submit_task::<NeverMockBackend>))
            .route(
                "/api/v1/tasks/:id",
                get(get_task::<NeverMockBackend>).delete(cancel_task::<NeverMockBackend>),
            )
            .with_state(state);

        (router, agent_id.to_string())
    }

    async fn body_json(resp: axum::response::Response) -> serde_json::Value {
        let bytes = axum::body::to_bytes(resp.into_body(), 4096)
            .await
            .expect("read body");
        serde_json::from_slice(&bytes).expect("parse json")
    }

    #[tokio::test]
    async fn test_submit_task_returns_202() {
        // GIVEN un agent actif avec coordinateur
        let (router, agent_id) = test_router_with_agent().await;

        // WHEN POST /api/v1/tasks avec agent_id valide
        let body = serde_json::json!({
            "agent_id": agent_id,
            "input": {"prompt": "Bonjour"}
        });
        let req = Request::builder()
            .method("POST")
            .uri("/api/v1/tasks")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).expect("serialize")))
            .expect("build request");
        let resp = router.oneshot(req).await.expect("request failed");

        // THEN 202 Accepted avec task_id et status "submitted"
        assert_eq!(resp.status(), StatusCode::ACCEPTED);
        let json = body_json(resp).await;
        assert_eq!(json["status"], "submitted");
        assert!(json["task_id"].is_string());
        assert!(!json["task_id"].as_str().expect("task_id str").is_empty());
    }

    #[tokio::test]
    async fn test_submit_task_unknown_agent_returns_404() {
        // GIVEN aucun agent "ghost-agent" enregistre
        let router = test_router();

        // WHEN POST /api/v1/tasks avec agent_id "ghost-agent"
        let body = serde_json::json!({
            "agent_id": "ghost-agent",
            "input": {"prompt": "Hello"}
        });
        let req = Request::builder()
            .method("POST")
            .uri("/api/v1/tasks")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).expect("serialize")))
            .expect("build request");
        let resp = router.oneshot(req).await.expect("request failed");

        // THEN 404 avec erreur
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
        let json = body_json(resp).await;
        assert!(json["error"]
            .as_str()
            .expect("error str")
            .contains("not found"));
    }

    #[tokio::test]
    async fn test_submit_task_invalid_body_returns_400() {
        // GIVEN un body JSON invalide (champ manquant)
        let router = test_router();

        let req = Request::builder()
            .method("POST")
            .uri("/api/v1/tasks")
            .header("content-type", "application/json")
            .body(Body::from(r#"{"bad": "data"}"#))
            .expect("build request");
        let resp = router.oneshot(req).await.expect("request failed");

        // THEN 422 (axum returns 422 for deserialization errors)
        assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[tokio::test]
    async fn test_get_task_status_returns_200() {
        // GIVEN une tache soumise
        let (router, agent_id) = test_router_with_agent().await;

        // Submit a task first
        let body = serde_json::json!({
            "agent_id": agent_id,
            "input": {"prompt": "Test"}
        });
        let req = Request::builder()
            .method("POST")
            .uri("/api/v1/tasks")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).expect("serialize")))
            .expect("build request");
        let resp = router.clone().oneshot(req).await.expect("request failed");
        assert_eq!(resp.status(), StatusCode::ACCEPTED);
        let submit_json = body_json(resp).await;
        let task_id = submit_json["task_id"].as_str().expect("task_id");

        // WHEN GET /api/v1/tasks/{id}
        let req = Request::builder()
            .uri(format!("/api/v1/tasks/{task_id}"))
            .body(Body::empty())
            .expect("build request");
        let resp = router.oneshot(req).await.expect("request failed");

        // THEN 200 avec le statut
        assert_eq!(resp.status(), StatusCode::OK);
        let json = body_json(resp).await;
        assert_eq!(json["task_id"], task_id);
        assert!(json["status"].is_string());
    }

    #[tokio::test]
    async fn test_get_task_not_found_returns_404() {
        // GIVEN aucune tache "unknown-task"
        let router = test_router();

        // WHEN GET /api/v1/tasks/unknown-task
        let req = Request::builder()
            .uri("/api/v1/tasks/unknown-task")
            .body(Body::empty())
            .expect("build request");
        let resp = router.oneshot(req).await.expect("request failed");

        // THEN 404
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
        let json = body_json(resp).await;
        assert!(json["error"]
            .as_str()
            .expect("error str")
            .contains("task not found"));
    }

    #[tokio::test]
    async fn test_cancel_task_returns_200() {
        // GIVEN une tache soumise sur un backend bloquant (jamais termine)
        // Utilise NeverMockBackend pour eviter la race condition entre TaskCompleted et Cancel
        let (router, agent_id) = test_router_with_blocking_agent().await;

        // Submit a task first
        let body = serde_json::json!({
            "agent_id": agent_id,
            "input": {"prompt": "Cancel me"}
        });
        let req = Request::builder()
            .method("POST")
            .uri("/api/v1/tasks")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).expect("serialize")))
            .expect("build request");
        let resp = router.clone().oneshot(req).await.expect("request failed");
        assert_eq!(resp.status(), StatusCode::ACCEPTED);
        let submit_json = body_json(resp).await;
        let task_id = submit_json["task_id"].as_str().expect("task_id");

        // WHEN DELETE /api/v1/tasks/{id}
        let req = Request::builder()
            .method("DELETE")
            .uri(format!("/api/v1/tasks/{task_id}"))
            .body(Body::empty())
            .expect("build request");
        let resp = router.oneshot(req).await.expect("request failed");

        // THEN 200 avec status "canceled"
        assert_eq!(resp.status(), StatusCode::OK);
        let json = body_json(resp).await;
        assert_eq!(json["task_id"], task_id);
        assert_eq!(json["status"], "canceled");
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Tests ResumeHandler — POST /api/v1/tasks/{id}/resume
    // ─────────────────────────────────────────────────────────────────────────

    /// Ouvre un `TaskRepository` sur un fichier temporaire unique.
    async fn open_test_repo() -> apollia_tools::TaskRepository {
        let path = std::env::temp_dir().join(format!("apollia_resume_{}.db", uuid::Uuid::new_v4()));
        apollia_tools::TaskRepository::open(&path)
            .await
            .expect("TaskRepository::open failed")
    }

    /// Construit un Router avec la route resume et un `TaskRepository` actif.
    async fn resume_router_with_repo(repo: apollia_tools::TaskRepository) -> axum::Router {
        let (event_tx, _) = EventBus::new();
        let registry_handle = AgentRegistry::spawn(event_tx.clone());
        let router_handle: TaskRouterHandle<MockBackend> =
            TaskRouterHandle::spawn(registry_handle.clone(), event_tx.clone(), 64);
        let state = AppState {
            router_handle,
            registry_handle,
            event_sender: event_tx,
            agent_loader: std::sync::Arc::new(crate::api::routes_agents::StubAgentLoader),
            backend: MockBackend,
            llm_router: None,
            trigger_engine: None,
            config_path: None,
            task_repository: Some(std::sync::Arc::new(repo)),
            pending_approvals: None,
        };
        Router::new()
            .route("/api/v1/tasks/:id/resume", post(resume_task::<MockBackend>))
            .with_state(state)
    }

    // AC-1 — Approbation valide → 200 OK + TaskResumed émis sur EventBus

    #[tokio::test]
    async fn test_ac1_resume_approve_returns_200() {
        // GIVEN une tâche en status input_required dans le HITL DB
        let repo = open_test_repo().await;
        let task_id = "t-0042";
        repo.save_input_required(task_id, None, "Confirmer ?", &serde_json::json!({}))
            .await
            .expect("save_input_required failed");

        let router = resume_router_with_repo(repo).await;

        // WHEN POST /api/v1/tasks/t-0042/resume { "approved": true }
        let body = serde_json::json!({ "approved": true });
        let req = Request::builder()
            .method("POST")
            .uri(format!("/api/v1/tasks/{task_id}/resume"))
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).expect("serialize")))
            .expect("build request");
        let resp = router.oneshot(req).await.expect("request failed");

        // THEN 200 avec approved=true et status="working"
        assert_eq!(resp.status(), StatusCode::OK);
        let json = body_json(resp).await;
        assert_eq!(json["task_id"], task_id);
        assert_eq!(json["approved"], true);
        assert_eq!(json["status"], "working");
    }

    // AC-2 — Rejet valide avec raison → 200 OK

    #[tokio::test]
    async fn test_ac2_resume_reject_with_reason() {
        // GIVEN une tâche en status input_required
        let repo = open_test_repo().await;
        let task_id = "t-0043";
        repo.save_input_required(task_id, None, "Budget OK ?", &serde_json::json!({}))
            .await
            .expect("save_input_required failed");

        let router = resume_router_with_repo(repo).await;

        // WHEN POST /resume { "approved": false, "reason": "Budget insuffisant" }
        let body = serde_json::json!({
            "approved": false,
            "reason": "Budget insuffisant"
        });
        let req = Request::builder()
            .method("POST")
            .uri(format!("/api/v1/tasks/{task_id}/resume"))
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).expect("serialize")))
            .expect("build request");
        let resp = router.oneshot(req).await.expect("request failed");

        // THEN 200 avec approved=false et status="working"
        assert_eq!(resp.status(), StatusCode::OK);
        let json = body_json(resp).await;
        assert_eq!(json["approved"], false);
        assert_eq!(json["status"], "working");
    }

    // AC-3 — Tâche pas en input_required → 409 CONFLICT

    #[tokio::test]
    async fn test_ac3_resume_not_input_required_returns_409() {
        // GIVEN une tâche en status working (input_required → save_input_response → working)
        let repo = open_test_repo().await;
        let task_id = "t-0044";
        repo.save_input_required(task_id, None, "Prompt", &serde_json::json!({}))
            .await
            .expect("save_input_required failed");
        // Transition vers working via save_input_response
        let resp_data = apollia_core::InputResponseData {
            approved: true,
            reason: None,
            context: serde_json::json!({}),
            responded_at: "2026-03-09T10:00:00Z".into(),
        };
        repo.save_input_response(task_id, &resp_data)
            .await
            .expect("save_input_response failed");

        let router = resume_router_with_repo(repo).await;

        // WHEN POST /resume { "approved": true } sur tâche déjà en working
        let body = serde_json::json!({ "approved": true });
        let req = Request::builder()
            .method("POST")
            .uri(format!("/api/v1/tasks/{task_id}/resume"))
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).expect("serialize")))
            .expect("build request");
        let resp = router.oneshot(req).await.expect("request failed");

        // THEN 409 CONFLICT
        assert_eq!(resp.status(), StatusCode::CONFLICT);
        let json = body_json(resp).await;
        assert!(
            json["error"]
                .as_str()
                .unwrap_or("")
                .contains("not in input_required"),
            "error should mention not in input_required, got: {}",
            json["error"]
        );
    }

    // AC-4 — Tâche inexistante → 404 NOT FOUND

    #[tokio::test]
    async fn test_ac4_resume_task_not_found_returns_404() {
        // GIVEN un TaskRepository vide (aucune tâche)
        let repo = open_test_repo().await;
        let router = resume_router_with_repo(repo).await;

        // WHEN POST /resume sur un task_id inexistant
        let body = serde_json::json!({ "approved": true });
        let req = Request::builder()
            .method("POST")
            .uri("/api/v1/tasks/t-9999/resume")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).expect("serialize")))
            .expect("build request");
        let resp = router.oneshot(req).await.expect("request failed");

        // THEN 404 NOT FOUND
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
        let json = body_json(resp).await;
        assert!(
            json["error"].as_str().unwrap_or("").contains("t-9999"),
            "error should mention task_id, got: {}",
            json["error"]
        );
    }
}
