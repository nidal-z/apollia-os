//! REST routes for the audit trail.
//!
//! - `GET /api/v1/audit?limit=N` — last N tool invocations (default 20)
//! - `GET /api/v1/audit/stats`   — aggregate counts (total, unique tools, agents)
//!
//! Both routes return 503 when no `AuditTrailHandle` is configured in `AppState`
//! (e.g. unit tests or a runtime started without a data directory).

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};

use apollia_tools::ToolInvocationRecord;

use crate::api::server::AppState;
use crate::coordinator::ExecutionBackend;

// ---------------------------------------------------------------------------
// Query parameters
// ---------------------------------------------------------------------------

/// Query parameters for `GET /api/v1/audit`.
#[derive(Debug, Deserialize)]
pub struct AuditListQuery {
    /// Maximum number of events to return (default 20, capped at 500).
    #[serde(default = "default_limit")]
    pub limit: u32,
}

fn default_limit() -> u32 {
    20
}

// ---------------------------------------------------------------------------
// Response types
// ---------------------------------------------------------------------------

/// A single audit event as returned by the API.
#[derive(Debug, Serialize)]
pub struct AuditEventResponse {
    pub id: String,
    pub agent_id: String,
    pub task_id: String,
    pub tool_name: String,
    pub input_hash: String,
    pub sandbox_profile: String,
    pub started_at: String,
    pub duration_ms: Option<u64>,
    pub exit_code: Option<i32>,
    pub success: bool,
    pub error_code: Option<String>,
    /// Arguments JSON complets de l'invocation (STORY-128).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub args_json: Option<String>,
    /// Sortie standard de l'outil, potentiellement tronquée (STORY-128).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stdout: Option<String>,
    /// Sortie d'erreur de l'outil, potentiellement tronquée (STORY-128).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stderr: Option<String>,
}

impl From<ToolInvocationRecord> for AuditEventResponse {
    fn from(r: ToolInvocationRecord) -> Self {
        Self {
            id: r.id,
            agent_id: r.agent_id,
            task_id: r.task_id,
            tool_name: r.tool_name,
            input_hash: r.input_hash,
            sandbox_profile: r.sandbox_profile,
            started_at: r.started_at,
            duration_ms: r.duration_ms,
            exit_code: r.exit_code,
            success: r.success,
            error_code: r.error_code,
            args_json: r.args_json,
            stdout: r.stdout,
            stderr: r.stderr,
        }
    }
}

/// Response body for `GET /api/v1/audit`.
#[derive(Debug, Serialize)]
pub struct AuditListResponse {
    pub events: Vec<AuditEventResponse>,
    pub count: usize,
}

/// Response body for `GET /api/v1/audit/stats`.
#[derive(Debug, Serialize)]
pub struct AuditStatsResponse {
    pub total_events: u64,
    pub unique_tools: u64,
    pub unique_agents: u64,
}

/// Response body for error cases.
#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub error: String,
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// `GET /api/v1/audit?limit=N` — list the most recent tool invocations.
pub async fn list_audit<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Query(params): Query<AuditListQuery>,
) -> Result<Json<AuditListResponse>, (StatusCode, Json<ErrorResponse>)> {
    let handle = state.audit_trail.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse {
                error: "audit trail not available".to_string(),
            }),
        )
    })?;

    // Cap limit to prevent unbounded queries.
    let limit = params.limit.min(500) as usize;
    let records = handle.query_last(limit).await;

    let events: Vec<AuditEventResponse> = records.into_iter().map(Into::into).collect();
    let count = events.len();

    Ok(Json(AuditListResponse { events, count }))
}

/// `GET /api/v1/audit/stats` — aggregate counts for the audit trail.
pub async fn get_audit_stats<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
) -> Result<Json<AuditStatsResponse>, (StatusCode, Json<ErrorResponse>)> {
    let handle = state.audit_trail.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse {
                error: "audit trail not available".to_string(),
            }),
        )
    })?;

    let stats = handle.stats().await;

    Ok(Json(AuditStatsResponse {
        total_events: stats.total_events,
        unique_tools: stats.unique_tools,
        unique_agents: stats.unique_agents,
    }))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use crate::api::server::{APIServer, AppState};
    use crate::coordinator::{DynBackend, ExecutionBackend};
    use crate::eventbus::EventBus;
    use crate::registry::AgentRegistry;
    use crate::router::TaskRouterHandle;
    use apollia_tools::{AuditTrailHandle, ToolInvocationRecord};
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use std::future::Future;
    use std::pin::Pin;
    use std::sync::Arc;
    use tower::ServiceExt;

    use apollia_core::{AIPResult, AIPTask, TaskStatus};

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

    async fn open_temp_audit() -> AuditTrailHandle {
        let db_path =
            std::env::temp_dir().join(format!("apollia_audit_routes_{}.db", uuid::Uuid::new_v4()));
        AuditTrailHandle::open(&db_path)
            .await
            .expect("failed to open audit trail for test")
    }

    fn test_app_state_with_audit(audit: Option<AuditTrailHandle>) -> AppState<MockBackend> {
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
            llm_router: None,
            trigger_engine: None,
            config_path: None,
            task_repository: None,
            pending_approvals: None,
            notification_config: None,
            pipeline_engine: None,
            backend_factory: None,
            tool_registry_handle: None,
            audit_trail: audit,
            obs_config: apollia_core::ObservabilityConfig::default(),
            llm_call_repository: None,
            trigger_def_repo: None,
            pipeline_def_repo: None,
            notification_repo: None,
            notification_engine_handle: None,
            chat_manager: None,
        }
    }

    fn make_record(tool_name: &str, success: bool) -> ToolInvocationRecord {
        ToolInvocationRecord {
            id: uuid::Uuid::new_v4().to_string(),
            agent_id: "agent-test".to_string(),
            task_id: "task-001".to_string(),
            tool_name: tool_name.to_string(),
            input_hash: "abc".to_string(),
            sandbox_profile: "none".to_string(),
            started_at: "2026-01-01T00:00:00Z".to_string(),
            duration_ms: Some(10),
            exit_code: Some(if success { 0 } else { 1 }),
            success,
            error_code: None,
            resources_used: None,
            args_json: None,
            stdout: None,
            stderr: None,
        }
    }

    // AC-1 — GET /api/v1/audit returns events list
    #[tokio::test]
    async fn test_list_audit_returns_events() {
        // GIVEN an audit trail with 2 records
        let audit = open_temp_audit().await;
        audit.record(make_record("bash_executor", true));
        audit.record(make_record("file_io", false));
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let state = test_app_state_with_audit(Some(audit));
        let router = APIServer::build_router_for_test(state);

        // WHEN GET /api/v1/audit
        let req = Request::builder()
            .uri("/api/v1/audit?limit=10")
            .body(Body::empty())
            .unwrap();
        let resp = router.oneshot(req).await.unwrap();

        // THEN 200 with events array
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), 65536).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["count"].as_u64().unwrap(), 2);
        assert!(json["events"].as_array().unwrap().len() == 2);
    }

    // AC-2 — GET /api/v1/audit/stats returns aggregate counts
    #[tokio::test]
    async fn test_audit_stats_returns_counts() {
        // GIVEN an audit trail with records from 2 different agents and 2 tools
        let audit = open_temp_audit().await;
        let mut r1 = make_record("bash_executor", true);
        r1.agent_id = "agent-alpha".to_string();
        let mut r2 = make_record("file_io", true);
        r2.agent_id = "agent-beta".to_string();
        audit.record(r1);
        audit.record(r2);
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let state = test_app_state_with_audit(Some(audit));
        let router = APIServer::build_router_for_test(state);

        // WHEN GET /api/v1/audit/stats
        let req = Request::builder()
            .uri("/api/v1/audit/stats")
            .body(Body::empty())
            .unwrap();
        let resp = router.oneshot(req).await.unwrap();

        // THEN 200 with correct counts
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), 4096).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["total_events"].as_u64().unwrap(), 2);
        assert_eq!(json["unique_tools"].as_u64().unwrap(), 2);
        assert_eq!(json["unique_agents"].as_u64().unwrap(), 2);
    }

    // AC-3 — GET /api/v1/audit returns 503 when audit trail is not configured
    #[tokio::test]
    async fn test_list_audit_returns_503_when_not_configured() {
        // GIVEN no audit trail in AppState
        let state = test_app_state_with_audit(None);
        let router = APIServer::build_router_for_test(state);

        // WHEN GET /api/v1/audit
        let req = Request::builder()
            .uri("/api/v1/audit")
            .body(Body::empty())
            .unwrap();
        let resp = router.oneshot(req).await.unwrap();

        // THEN 503 Service Unavailable
        assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    // AC-4 — limit is capped at 500
    #[tokio::test]
    async fn test_list_audit_limit_is_capped() {
        // GIVEN an empty audit trail
        let audit = open_temp_audit().await;
        let state = test_app_state_with_audit(Some(audit));
        let router = APIServer::build_router_for_test(state);

        // WHEN GET /api/v1/audit?limit=9999
        let req = Request::builder()
            .uri("/api/v1/audit?limit=9999")
            .body(Body::empty())
            .unwrap();
        let resp = router.oneshot(req).await.unwrap();

        // THEN 200 (capped — no panic, no error)
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), 4096).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["count"].as_u64().unwrap(), 0);
    }
}
