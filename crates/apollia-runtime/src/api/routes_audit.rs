//! REST routes for the audit trail.
//!
//! - `GET /api/v1/audit?limit=N`, last N tool invocations (default 20)
//! - `GET /api/v1/audit/stats`  , aggregate counts (total, unique tools, agents)
//!
//! Both routes return 503 when no `AuditTrailHandle` is configured in `AppState`
//! (e.g. unit tests or a runtime started without a data directory).

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};

use apollia_core::events::RunId;
use apollia_tools::ToolInvocationRecord;

use crate::api::server::AppState;
use crate::audit_journal::entry::{JournalEntry, JournalEntryKind};
use crate::audit_journal::VerifyChainReport;
use crate::coordinator::ExecutionBackend;
use crate::replay::{
    ReplayBundle, ReplayCaptureError, ReplayFailReason, ReplayHarness, ReplayReport,
};

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
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct AuditEventResponse {
    pub id: String,
    pub agent_id: String,
    pub task_id: String,
    /// Stable run identifier this invocation belongs to (the key `audit verify`
    /// uses). `null` for invocations recorded before run_id tracking (kept in the
    /// payload unconditionally so the schema is stable for automation).
    pub run_id: Option<String>,
    pub tool_name: String,
    pub input_hash: String,
    pub sandbox_profile: String,
    pub started_at: String,
    pub duration_ms: Option<u64>,
    pub exit_code: Option<i32>,
    pub success: bool,
    pub error_code: Option<String>,
    /// Arguments JSON complets de l'invocation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub args_json: Option<String>,
    /// Standard output of the tool, possibly truncated.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stdout: Option<String>,
    /// Error output of the tool, possibly truncated.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stderr: Option<String>,
}

impl From<ToolInvocationRecord> for AuditEventResponse {
    fn from(r: ToolInvocationRecord) -> Self {
        Self {
            id: r.id,
            agent_id: r.agent_id,
            task_id: r.task_id,
            run_id: r.run_id,
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
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct AuditListResponse {
    pub events: Vec<AuditEventResponse>,
    pub count: usize,
}

/// Response body for `GET /api/v1/audit/stats`.
#[derive(Debug, Serialize, utoipa::ToSchema)]
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

/// `GET /api/v1/audit?limit=N`, list the most recent tool invocations.
#[utoipa::path(
    get,
    path = "/api/v1/audit",
    tag = "audit",
    params(("limit" = Option<u32>, Query, description = "Maximum number of events to return (default 20, capped at 500)")),
    responses(
        (status = 200, description = "Recent tool invocations", body = AuditListResponse),
        (status = 503, description = "Audit trail not configured", body = crate::api::openapi::ApiErrorBody),
    )
)]
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

/// `GET /api/v1/audit/stats`, aggregate counts for the audit trail.
#[utoipa::path(
    get,
    path = "/api/v1/audit/stats",
    tag = "audit",
    responses(
        (status = 200, description = "Aggregate audit counts", body = AuditStatsResponse),
        (status = 503, description = "Audit trail not configured", body = crate::api::openapi::ApiErrorBody),
    )
)]
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

/// `GET /api/v1/audit/verify/:run_id`, verify a run's hash chain and signatures.
///
/// Returns 200 with the [`VerifyChainReport`] (whether or not the chain is
/// intact), 404 when the run has no entries, 503 when the journal is not
/// configured, and 500 on an internal error.
#[utoipa::path(
    get,
    path = "/api/v1/audit/verify/{run_id}",
    tag = "audit",
    params(("run_id" = String, Path, description = "Run id whose hash chain is verified")),
    responses(
        (status = 200, description = "Verification report", body = crate::audit_journal::verify::VerifyChainReport),
        (status = 404, description = "Run has no entries", body = crate::api::openapi::ApiErrorBody),
        (status = 503, description = "Audit journal not configured", body = crate::api::openapi::ApiErrorBody),
        (status = 500, description = "Internal verification error", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub async fn verify_audit_run<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path(run_id): Path<String>,
) -> Result<Json<VerifyChainReport>, (StatusCode, Json<ErrorResponse>)> {
    let handle = state.audit_journal.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse {
                error: "audit journal not available".to_string(),
            }),
        )
    })?;

    let report = handle.verify_chain(&run_id).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: e.to_string(),
            }),
        )
    })?;

    if report.entries_checked == 0 {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: "not_found".to_string(),
            }),
        ));
    }

    Ok(Json(report))
}

/// Response body for `GET /api/v1/audit/journal/:run_id`.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct AuditJournalResponse {
    /// The run whose journal entries these are.
    pub run_id: String,
    /// Ordered journal entries (tool calls AND LLM completions), so the model's
    /// captured reasoning is readable, not only verifiable/replayable.
    #[schema(value_type = Vec<Object>)]
    pub entries: Vec<JournalEntry>,
}

/// `GET /api/v1/audit/journal/:run_id`, the full journal for a run.
///
/// Unlike `GET /api/v1/audit` (the tool-only audit trail), this returns the
/// hash-chained journal entries including `llm_completion` (the model's
/// prompts/responses). 404 when the run has no entries, 503 when the journal is
/// not configured.
#[utoipa::path(
    get,
    path = "/api/v1/audit/journal/{run_id}",
    tag = "audit",
    params(("run_id" = String, Path, description = "Run id whose journal entries are returned")),
    responses(
        (status = 200, description = "Full hash-chained journal for the run", body = AuditJournalResponse),
        (status = 404, description = "Run has no entries", body = crate::api::openapi::ApiErrorBody),
        (status = 503, description = "Audit journal not configured", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub async fn show_audit_run<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path(run_id): Path<String>,
) -> Result<Json<AuditJournalResponse>, (StatusCode, Json<ErrorResponse>)> {
    let handle = state.audit_journal.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse {
                error: "audit journal not available".to_string(),
            }),
        )
    })?;

    let entries = handle.query_run(&run_id).await;
    if entries.is_empty() {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: "not_found".to_string(),
            }),
        ));
    }
    Ok(Json(AuditJournalResponse { run_id, entries }))
}

/// Minimum length of a run-id prefix accepted for resolution.
const MIN_RUN_ID_PREFIX: usize = 8;

/// Outcome of resolving a run-id argument (full id or prefix) against the journal.
enum RunIdResolution {
    /// Exactly one run matched.
    Found(String),
    /// No run matched.
    NotFound,
    /// More than one run matched the prefix.
    Ambiguous(Vec<String>),
}

/// Resolve a run-id argument to a single full run id.
///
/// An exact match wins. Otherwise the argument is treated as a prefix of at
/// least [`MIN_RUN_ID_PREFIX`] characters; zero matches is not found, several is
/// ambiguous.
fn resolve_run_id(ids: &[String], input: &str) -> RunIdResolution {
    if ids.iter().any(|id| id == input) {
        return RunIdResolution::Found(input.to_string());
    }
    if input.len() < MIN_RUN_ID_PREFIX {
        return RunIdResolution::NotFound;
    }
    let matches: Vec<String> = ids
        .iter()
        .filter(|id| id.starts_with(input))
        .cloned()
        .collect();
    match matches.len() {
        0 => RunIdResolution::NotFound,
        1 => RunIdResolution::Found(matches.into_iter().next().unwrap_or_default()),
        _ => RunIdResolution::Ambiguous(matches),
    }
}

/// `POST /api/v1/audit/replay/:run_id`, replay a captured run and report
/// determinism.
///
/// Resolves the run id (full or unambiguous prefix), runs the
/// [`ReplayHarness`], and returns a JSON body with a `status` discriminant:
/// `identical`, `diverged`, or `error`. Status codes: 200 for identical and
/// diverged (the determinism verdict is in the body), 404 for an unknown run,
/// 400 for an ambiguous prefix, 422 for an incomplete trace, 503 when the
/// journal is not configured.
#[utoipa::path(
    post,
    path = "/api/v1/audit/replay/{run_id}",
    tag = "audit",
    params(("run_id" = String, Path, description = "Run id (full or unambiguous prefix) to replay")),
    responses(
        (status = 200, description = "Replay determinism verdict (identical or diverged)"),
        (status = 400, description = "Ambiguous run id prefix", body = crate::api::openapi::ApiErrorBody),
        (status = 404, description = "Unknown run", body = crate::api::openapi::ApiErrorBody),
        (status = 422, description = "Incomplete trace", body = crate::api::openapi::ApiErrorBody),
        (status = 503, description = "Audit journal not configured", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub async fn post_replay_run<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path(run_id): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    let handle = match state.audit_journal.as_ref() {
        Some(handle) => handle,
        None => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({ "status": "error", "code": "journal_unavailable" })),
            );
        }
    };

    let resolved = match resolve_run_id(&handle.run_ids().await, &run_id) {
        RunIdResolution::Found(id) => id,
        RunIdResolution::NotFound => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({
                    "status": "error", "code": "run_not_found", "run_id": run_id,
                })),
            );
        }
        RunIdResolution::Ambiguous(candidates) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "status": "error", "code": "ambiguous_run_id",
                    "prefix": run_id, "candidates": candidates,
                })),
            );
        }
    };

    let entries = handle.query_run(&resolved).await;
    let steps = entries
        .iter()
        .filter(|e| e.kind == JournalEntryKind::LlmCompletion)
        .count();
    let run = RunId::from(resolved.clone());

    let bundle = match ReplayBundle::from_journal(&entries, &run) {
        Ok(bundle) => bundle,
        Err(ReplayCaptureError::NoCaptures { .. }) => {
            return incomplete_trace_response(&resolved, "LlmCompletion", 0);
        }
        Err(ReplayCaptureError::OrdinalGap { found, .. }) => {
            return incomplete_trace_response(&resolved, "LlmCompletion", found);
        }
        Err(ReplayCaptureError::StepExhausted { requested, .. }) => {
            return incomplete_trace_response(&resolved, "LlmCompletion", requested);
        }
    };

    let mut harness = ReplayHarness::from_bundle(&run, bundle);
    match harness.run().await {
        Ok(ReplayReport::Identical) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "status": "identical", "run_id": resolved, "steps": steps,
            })),
        ),
        Ok(ReplayReport::Diverged { divergences }) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "status": "diverged", "run_id": resolved, "divergences": divergences,
            })),
        ),
        Ok(ReplayReport::Failed { reason }) => {
            let (kind, step) = match reason {
                ReplayFailReason::IncompleteTrace { kind, step, .. } => (kind, step),
                ReplayFailReason::OrdinalGap { found, .. } => ("OrdinalGap".to_string(), found),
                ReplayFailReason::AgentError(_) => ("AgentError".to_string(), 0),
            };
            incomplete_trace_response(&resolved, &kind, step)
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "status": "error", "code": "replay_error",
                "run_id": resolved, "detail": e.to_string(),
            })),
        ),
    }
}

/// Build the 422 incomplete-trace response body.
fn incomplete_trace_response(
    run_id: &str,
    missing_kind: &str,
    step: u32,
) -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::UNPROCESSABLE_ENTITY,
        Json(serde_json::json!({
            "status": "error", "code": "incomplete_trace",
            "run_id": run_id, "missing_kind": missing_kind, "step": step,
        })),
    )
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::{resolve_run_id, RunIdResolution};
    use crate::api::server::{APIServer, AppState};
    use crate::coordinator::{DynBackend, ExecutionBackend};
    use crate::eventbus::EventBus;
    use crate::registry::AgentRegistry;
    use crate::router::TaskRouterHandle;
    use apollia_core::events::RunId;
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
            llm_router: crate::api::server::empty_shared_llm_router(),
            trigger_engine: None,
            config_path: None,
            task_repository: None,
            pending_approvals: None,
            plan_gates: None,
            notification_config: None,
            backend_factory: None,
            tool_registry_handle: None,
            audit_trail: audit,
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
        }
    }

    fn make_record(tool_name: &str, success: bool) -> ToolInvocationRecord {
        ToolInvocationRecord {
            id: uuid::Uuid::new_v4().to_string(),
            agent_id: "agent-test".to_string(),
            task_id: "task-001".to_string(),
            run_id: None,
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

    // GET /api/v1/audit returns events list
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

    // GET /api/v1/audit/stats returns aggregate counts
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

    // GET /api/v1/audit returns 503 when audit trail is not configured
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

    // limit is capped at 500
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

        // THEN 200 (capped, no panic, no error)
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), 4096).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["count"].as_u64().unwrap(), 0);
    }

    async fn open_temp_journal() -> crate::audit_journal::AuditJournalHandle {
        let db_path = std::env::temp_dir().join(format!(
            "apollia_journal_routes_{}.db",
            uuid::Uuid::new_v4()
        ));
        crate::audit_journal::AuditJournalHandle::open(&db_path)
            .await
            .expect("open journal for test")
    }

    // GET /api/v1/audit/verify/:run_id returns ok for an intact chain
    #[tokio::test]
    async fn test_verify_intact_chain_returns_ok() {
        // GIVEN a journal with two entries for run-1
        use crate::audit_journal::{JournalEntryDraft, JournalEntryKind};
        let journal = open_temp_journal().await;
        for i in 0..2 {
            journal.append(JournalEntryDraft {
                run_id: "run-1".to_string(),
                ts: "2026-01-01T00:00:00Z".to_string(),
                kind: JournalEntryKind::ToolCallStarted,
                payload: serde_json::json!({ "i": i }),
            });
        }
        tokio::time::sleep(std::time::Duration::from_millis(80)).await;
        let mut state = test_app_state_with_audit(None);
        state.audit_journal = Some(journal);
        let router = APIServer::build_router_for_test(state);

        // WHEN GET /api/v1/audit/verify/run-1
        let req = Request::builder()
            .uri("/api/v1/audit/verify/run-1")
            .body(Body::empty())
            .unwrap();
        let resp = router.oneshot(req).await.unwrap();

        // THEN 200 with ok=true and the two entries checked
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), 4096).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(json["ok"].as_bool().unwrap());
        assert_eq!(json["entries_checked"].as_u64().unwrap(), 2);
        assert!(json["first_broken_link"].is_null());
    }

    // GET verify with an unknown run returns 404 not_found
    #[tokio::test]
    async fn test_verify_unknown_run_returns_404() {
        // GIVEN a journal with no entries for the requested run
        let journal = open_temp_journal().await;
        let mut state = test_app_state_with_audit(None);
        state.audit_journal = Some(journal);
        let router = APIServer::build_router_for_test(state);

        // WHEN GET /api/v1/audit/verify/missing
        let req = Request::builder()
            .uri("/api/v1/audit/verify/missing")
            .body(Body::empty())
            .unwrap();
        let resp = router.oneshot(req).await.unwrap();

        // THEN 404 not_found
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
        let body = axum::body::to_bytes(resp.into_body(), 4096).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["error"].as_str().unwrap(), "not_found");
    }

    // GET verify returns 503 when the journal is not configured
    #[tokio::test]
    async fn test_verify_returns_503_when_not_configured() {
        // GIVEN no journal in AppState
        let state = test_app_state_with_audit(None);
        let router = APIServer::build_router_for_test(state);

        // WHEN GET /api/v1/audit/verify/run-1
        let req = Request::builder()
            .uri("/api/v1/audit/verify/run-1")
            .body(Body::empty())
            .unwrap();
        let resp = router.oneshot(req).await.unwrap();

        // THEN 503 Service Unavailable
        assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    #[test]
    fn test_resolve_run_id_exact_and_prefix() {
        let ids = vec![
            "abcd1234-aaaa".to_string(),
            "abcd1234-bbbb".to_string(),
            "ef567890-cccc".to_string(),
        ];

        // exact match wins
        assert!(matches!(
            resolve_run_id(&ids, "ef567890-cccc"),
            RunIdResolution::Found(id) if id == "ef567890-cccc"
        ));
        // an unambiguous >= 8 char prefix resolves
        assert!(matches!(
            resolve_run_id(&ids, "ef567890"),
            RunIdResolution::Found(id) if id == "ef567890-cccc"
        ));
        // an ambiguous prefix lists the candidates
        assert!(matches!(
            resolve_run_id(&ids, "abcd1234"),
            RunIdResolution::Ambiguous(c) if c.len() == 2
        ));
        // a too-short prefix is not found (guards against broad matches)
        assert!(matches!(
            resolve_run_id(&ids, "abcd"),
            RunIdResolution::NotFound
        ));
        // an unknown prefix is not found
        assert!(matches!(
            resolve_run_id(&ids, "zzzzzzzz"),
            RunIdResolution::NotFound
        ));
    }

    // POST replay of a complete run returns 200 identical.
    #[tokio::test]
    async fn test_replay_identical_returns_200() {
        use crate::audit_journal::{JournalEntryDraft, JournalEntryKind};
        use crate::replay::{LlmCompletionSnapshot, ToolOutputSnapshot};

        // GIVEN a run with one tool turn then a final turn, plus the tool output
        let journal = open_temp_journal().await;
        let run = "replay-run-001";
        let llm0 = LlmCompletionSnapshot {
            run_id: RunId::from(run.to_string()),
            step_ordinal: 0,
            backend_name: "local".into(),
            model_id: "m".into(),
            content: "calling".into(),
            tool_calls: vec![serde_json::json!({ "id": "c1", "name": "bash", "arguments": {} })],
            prompt_tokens: 0,
            completion_tokens: 0,
            cost_usd: None,
            stream_truncated: false,
        };
        let llm1 = LlmCompletionSnapshot {
            run_id: RunId::from(run.to_string()),
            step_ordinal: 1,
            backend_name: "local".into(),
            model_id: "m".into(),
            content: "done".into(),
            tool_calls: vec![],
            prompt_tokens: 0,
            completion_tokens: 0,
            cost_usd: None,
            stream_truncated: false,
        };
        let tool0 = ToolOutputSnapshot {
            run_id: RunId::from(run.to_string()),
            step_ordinal: 0,
            tool_call_id: "c1".into(),
            tool_name: "bash".into(),
            output: serde_json::json!("ok"),
            status: "success".into(),
        };
        for (kind, payload) in [
            (
                JournalEntryKind::LlmCompletion,
                serde_json::to_value(&llm0).unwrap(),
            ),
            (
                JournalEntryKind::LlmCompletion,
                serde_json::to_value(&llm1).unwrap(),
            ),
            (
                JournalEntryKind::ToolOutput,
                serde_json::to_value(&tool0).unwrap(),
            ),
        ] {
            journal.append(JournalEntryDraft {
                run_id: run.to_string(),
                ts: "2026-01-01T00:00:00Z".to_string(),
                kind,
                payload,
            });
        }
        tokio::time::sleep(std::time::Duration::from_millis(80)).await;
        let mut state = test_app_state_with_audit(None);
        state.audit_journal = Some(journal);
        let router = APIServer::build_router_for_test(state);

        // WHEN POST /api/v1/audit/replay/replay-run-001
        let req = Request::builder()
            .method("POST")
            .uri(format!("/api/v1/audit/replay/{run}"))
            .body(Body::empty())
            .unwrap();
        let resp = router.oneshot(req).await.unwrap();

        // THEN 200 with status identical and two steps
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), 8192).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["status"].as_str().unwrap(), "identical");
        assert_eq!(json["steps"].as_u64().unwrap(), 2);
    }

    // POST replay of an unknown run returns 404 run_not_found.
    #[tokio::test]
    async fn test_replay_unknown_run_returns_404() {
        // GIVEN an empty journal
        let journal = open_temp_journal().await;
        let mut state = test_app_state_with_audit(None);
        state.audit_journal = Some(journal);
        let router = APIServer::build_router_for_test(state);

        // WHEN POST /api/v1/audit/replay/missing-run
        let req = Request::builder()
            .method("POST")
            .uri("/api/v1/audit/replay/missing-run")
            .body(Body::empty())
            .unwrap();
        let resp = router.oneshot(req).await.unwrap();

        // THEN 404 with code run_not_found
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
        let body = axum::body::to_bytes(resp.into_body(), 4096).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["code"].as_str().unwrap(), "run_not_found");
    }
}
