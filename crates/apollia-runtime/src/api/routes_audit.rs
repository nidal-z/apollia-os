//! REST routes of the audit journal.
//!
//! - `GET /api/v1/audit/journal?limit=N&offset=M`, a page of the chained journal,
//!   each entry with its summary, narrowed by agent, family, nature, result and period
//! - `GET /api/v1/audit/journal/:run_id`, the full journal of a run
//! - `GET /api/v1/audit/journal/:run_id/entries/:seq`, one entry with its detail
//! - `GET /api/v1/audit/verify` and `/verify/:run_id`, the verification of the
//!   chains and signatures
//! - `GET /api/v1/audit/anchor`, the exportable head anchor
//! - `POST /api/v1/audit/replay/:run_id`, the replay of a captured run
//!
//! Every route returns 503 when no journal is configured in `AppState` (unit
//! tests, or a runtime started without a data directory).

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};

use apollia_core::events::RunId;

use crate::api::server::AppState;
use crate::audit_journal::entry::{JournalEntry, JournalEntryKind};
use crate::audit_journal::{
    JournalAnchor, JournalCriteria, JournalEntryDetail, JournalFilter, JournalLine,
    VerifyChainReport, VerifyJournalReport,
};
use crate::coordinator::ExecutionBackend;
use crate::replay::{
    ReplayBundle, ReplayCaptureError, ReplayFailReason, ReplayHarness, ReplayReport,
};

// ---------------------------------------------------------------------------
// Query parameters
// ---------------------------------------------------------------------------

fn default_limit() -> u32 {
    20
}

/// Query parameters of `GET /api/v1/audit/journal`.
#[derive(Debug, Deserialize)]
pub struct JournalPageQuery {
    /// Maximum number of entries to return (default 20, capped at 500).
    #[serde(default = "default_limit")]
    pub limit: u32,
    /// How many entries to skip, newest first. Defaults to 0.
    #[serde(default)]
    pub offset: u32,
    /// Comma-separated agent names. When set, the page holds the entries that
    /// name one of those agents and the runs of their tasks.
    pub agents: Option<String>,
    /// Comma-separated action families (`read`, `write_or_delete`, ...).
    pub family: Option<String>,
    /// Comma-separated natures: `action`, `access_change`, `policy`, `system`.
    pub nature: Option<String>,
    /// Comma-separated results: `success`, `failure`, `refused`.
    pub result: Option<String>,
    /// Start of the period, included: a day (`YYYY-MM-DD`, UTC) or an RFC 3339
    /// instant.
    pub since: Option<String>,
    /// End of the period, excluded: a day or an RFC 3339 instant.
    pub until: Option<String>,
}

// ---------------------------------------------------------------------------
// Response types
// ---------------------------------------------------------------------------

/// Response body for error cases.
#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub error: String,
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

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

/// `GET /api/v1/audit/verify`, verify the whole journal across all runs.
///
/// Unlike the per-run route, this walks the global chain (detecting interior
/// deletion and whole-run deletion) and compares the terminal head to the
/// persisted anchor (detecting global-tail truncation). An empty journal is a
/// valid 200 `ok:true`, not a 404. Returns 503 when the journal is not
/// configured and 500 on an internal error.
#[utoipa::path(
    get,
    path = "/api/v1/audit/verify",
    tag = "audit",
    responses(
        (status = 200, description = "Whole-journal verification report", body = crate::audit_journal::verify::VerifyJournalReport),
        (status = 503, description = "Audit journal not configured", body = crate::api::openapi::ApiErrorBody),
        (status = 500, description = "Internal verification error", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub async fn verify_audit_journal<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
) -> Result<Json<VerifyJournalReport>, (StatusCode, Json<ErrorResponse>)> {
    let handle = state.audit_journal.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse {
                error: "audit journal not available".to_string(),
            }),
        )
    })?;

    let report = handle.verify_journal().await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: e.to_string(),
            }),
        )
    })?;

    Ok(Json(report))
}

/// `GET /api/v1/audit/anchor`, the exportable head anchor of the global chain.
///
/// Storing this off-machine is the only defense against truncation of the
/// global tail once the signing key can be compromised. Returns 404 when the
/// journal has no entries yet, 503 when the journal is not configured.
#[utoipa::path(
    get,
    path = "/api/v1/audit/anchor",
    tag = "audit",
    responses(
        (status = 200, description = "Exportable head anchor", body = crate::audit_journal::verify::JournalAnchor),
        (status = 404, description = "Journal has no entries", body = crate::api::openapi::ApiErrorBody),
        (status = 503, description = "Audit journal not configured", body = crate::api::openapi::ApiErrorBody),
        (status = 500, description = "Internal error", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub async fn get_audit_anchor<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
) -> Result<Json<JournalAnchor>, (StatusCode, Json<ErrorResponse>)> {
    let handle = state.audit_journal.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse {
                error: "audit journal not available".to_string(),
            }),
        )
    })?;

    let anchor = handle.journal_anchor().await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: e.to_string(),
            }),
        )
    })?;

    match anchor {
        Some(anchor) => Ok(Json(anchor)),
        None => Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: "not_found".to_string(),
            }),
        )),
    }
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

/// Response body for `GET /api/v1/audit/journal/:run_id/entries/:seq`.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct AuditJournalEntryDetailResponse {
    /// The chained entry, with its `summary` and, when it is about a tool
    /// call, a `tool_call` holding the full `arguments`, `output` and `status`.
    #[schema(value_type = Object)]
    pub entry: JournalEntryDetail,
}

/// Response body for `GET /api/v1/audit/journal`.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct AuditJournalPageResponse {
    /// Journal entries, newest global position first. Each one is the chained
    /// entry with a `summary` beside its fields: agent, nature, family, what
    /// happened, tool, target, outcome and date, without content.
    #[schema(value_type = Vec<Object>)]
    pub entries: Vec<JournalLine>,
    /// Number of entries in this page.
    pub count: usize,
}

/// `GET /api/v1/audit/journal?limit=N&offset=M`, a page of the chained journal
/// across every run, each entry with its summary.
///
/// This is the only read of the journal that does not need a run id up front:
/// `GET /api/v1/audit/journal/{run_id}` answers a run the caller already knows.
/// The filters are applied before paging, so `limit` and `offset` count
/// matching entries. 400 on a filter value that cannot be read, 503 when the journal is
/// not configured.
#[utoipa::path(
    get,
    path = "/api/v1/audit/journal",
    tag = "audit",
    params(
        ("limit" = Option<u32>, Query, description = "Maximum number of entries to return (default 20, capped at 500)"),
        ("offset" = Option<u32>, Query, description = "Number of matching entries to skip, newest first (default 0). Page through the journal by advancing it."),
        ("agents" = Option<String>, Query, description = "Comma-separated agent names. When set, only the entries that name one of those agents, and the runs of their tasks, are paged."),
        ("family" = Option<String>, Query, description = "Comma-separated action families: `read`, `read_personal`, `write_or_delete`, `send_external`, `execute_code`, `unclassified`, `exempt`."),
        ("nature" = Option<String>, Query, description = "Comma-separated natures: `action`, `access_change`, `policy`, `system`."),
        ("result" = Option<String>, Query, description = "Comma-separated results: `success`, `failure`, `refused`."),
        ("since" = Option<String>, Query, description = "Start of the period, included: a day `YYYY-MM-DD` read in UTC, or an RFC 3339 instant."),
        ("until" = Option<String>, Query, description = "End of the period, excluded: a day `YYYY-MM-DD` read in UTC, or an RFC 3339 instant."),
    ),
    responses(
        (status = 200, description = "A page of hash-chained journal entries, each with its summary", body = AuditJournalPageResponse),
        (status = 400, description = "A filter value that cannot be read", body = crate::api::openapi::ApiErrorBody),
        (status = 503, description = "Audit journal not configured, or `agents` given with no task repository to resolve them", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub async fn list_audit_journal<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Query(params): Query<JournalPageQuery>,
) -> Result<Json<AuditJournalPageResponse>, (StatusCode, Json<ErrorResponse>)> {
    let handle = state.audit_journal.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse {
                error: "audit journal not available".to_string(),
            }),
        )
    })?;

    // Same page ceiling as the trail: it bounds one request, not the reachable
    // history, which `offset` walks past.
    let limit = params.limit.min(500) as usize;
    let offset = params.offset as usize;
    let filter = resolve_filter(
        &state,
        JournalCriteria {
            agents: params.agents.as_deref(),
            families: params.family.as_deref(),
            natures: params.nature.as_deref(),
            outcomes: params.result.as_deref(),
            since: params.since.as_deref(),
            until: params.until.as_deref(),
        },
    )
    .await?;
    let entries = handle.query_lines(filter, limit, offset).await;
    let count = entries.len();

    Ok(Json(AuditJournalPageResponse { entries, count }))
}

/// The filter `criteria` describe, with the runs of the tasks of its agents
/// resolved. 400 on a value that cannot be read, 503 when agents are named and
/// no task repository can resolve them.
pub(crate) async fn resolve_filter<B: ExecutionBackend + Clone>(
    state: &AppState<B>,
    criteria: JournalCriteria<'_>,
) -> Result<JournalFilter, (StatusCode, Json<ErrorResponse>)> {
    let mut filter = JournalFilter::from_criteria(criteria).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: e.to_string(),
            }),
        )
    })?;
    if !filter.agents.is_empty() {
        let repo = state.task_repository.as_ref().ok_or_else(|| {
            (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(ErrorResponse {
                    error: "task repository not available to resolve agents".to_string(),
                }),
            )
        })?;
        filter.agent_runs = repo.run_ids_for_agents(&filter.agents).await.map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("failed to resolve the runs of the agents: {e}"),
                }),
            )
        })?;
    }
    Ok(filter)
}

/// `GET /api/v1/audit/journal/:run_id/entries/:seq`, one entry with its detail.
///
/// The page gives each entry a summary without content. This gives the entry
/// whole and, for a tool call, the full arguments and the full output of that
/// call, joined by the run and the call identifier from the entries the run
/// captured for replay. 404 when the run has no such entry, 503 when the
/// journal is not configured.
#[utoipa::path(
    get,
    path = "/api/v1/audit/journal/{run_id}/entries/{seq}",
    tag = "audit",
    params(
        ("run_id" = String, Path, description = "Run the entry belongs to"),
        ("seq" = u64, Path, description = "Sequence number of the entry within its run"),
    ),
    responses(
        (status = 200, description = "The entry, its summary, and the arguments and output of its tool call when it is about one", body = AuditJournalEntryDetailResponse),
        (status = 404, description = "The run has no such entry", body = crate::api::openapi::ApiErrorBody),
        (status = 503, description = "Audit journal not configured", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub async fn show_audit_entry<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path((run_id, seq)): Path<(String, u64)>,
) -> Result<Json<AuditJournalEntryDetailResponse>, (StatusCode, Json<ErrorResponse>)> {
    let handle = state.audit_journal.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse {
                error: "audit journal not available".to_string(),
            }),
        )
    })?;

    match handle.entry_detail(&run_id, seq).await {
        Some(entry) => Ok(Json(AuditJournalEntryDetailResponse { entry })),
        None => Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: "not_found".to_string(),
            }),
        )),
    }
}

/// `GET /api/v1/audit/journal/:run_id`, the full journal for a run.
///
/// Returns the hash-chained journal entries, including `llm_completion` (the
/// model's prompts/responses). 404 when the run has no entries, 503 when the journal is
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

/// `POST /api/v1/audit/replay/:run_id`, check that the captured trace of a
/// run is complete and consistent.
///
/// Resolves the run id (full or unambiguous prefix), then steps through the
/// trace the journal captured, offline: each model response is read back and
/// each tool call it asked for is matched with the tool output captured next.
/// No model is called, no tool runs and the agent is not run again. Returns a
/// JSON body with a `status` discriminant: `identical`, `diverged`, or `error`.
/// Status codes: 200 for identical and diverged (the verdict is in the body),
/// 404 for an unknown run, 400 for an ambiguous prefix, 422 for an incomplete
/// trace, 503 when the journal is not configured.
#[utoipa::path(
    post,
    path = "/api/v1/audit/replay/{run_id}",
    tag = "audit",
    params(("run_id" = String, Path, description = "Run id (full or unambiguous prefix) whose trace is checked")),
    responses(
        (status = 200, description = "Verdict on the captured trace (identical or diverged)"),
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
mod no_erase_tests;

#[cfg(test)]
mod tests {
    use super::{resolve_run_id, RunIdResolution};
    use crate::api::server::{APIServer, AppState};
    use crate::coordinator::{DynBackend, ExecutionBackend};
    use crate::eventbus::EventBus;
    use crate::registry::AgentRegistry;
    use crate::router::TaskRouterHandle;
    use apollia_core::events::RunId;
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

    fn test_app_state() -> AppState<MockBackend> {
        let (event_tx, _) = EventBus::new();
        let registry_handle = AgentRegistry::spawn(event_tx.clone());
        let router_handle: TaskRouterHandle<MockBackend> =
            TaskRouterHandle::spawn(registry_handle.clone(), event_tx.clone(), 64);
        AppState {
            router_handle,
            registry_handle,
            event_sender: event_tx,
            backend: MockBackend,
            llm_router: crate::api::server::empty_shared_llm_router(),
            trigger_engine: None,
            config_path: None,
            task_repository: None,
            pending_approvals: None,
            plan_gates: None,
            notification_config: None,
            tool_registry_handle: None,
            audit_journal: None,
            connection_governance: Default::default(),
            governance: None,
            inference_governance: Default::default(),
            obs_config: apollia_core::ObservabilityConfig::default(),
            config_reload: None,
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
            mlx_engine: None,
            secret_store: crate::inference_servers::default_store_opener(),
            batches: None,
            workflows: None,
            compat_server: None,
            source_registry: None,
        }
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
        let mut state = test_app_state();
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

    // GET /api/v1/audit/journal pages the chain across runs without a run id
    #[tokio::test]
    async fn test_list_audit_journal_spans_runs() {
        // GIVEN a journal holding entries from two different runs
        use crate::audit_journal::{JournalEntryDraft, JournalEntryKind};
        let journal = open_temp_journal().await;
        for run in ["run-1", "run-2"] {
            journal.append(JournalEntryDraft {
                run_id: run.to_string(),
                ts: "2026-01-01T00:00:00Z".to_string(),
                kind: JournalEntryKind::ToolCallStarted,
                payload: serde_json::json!({ "tool_name": "bash" }),
            });
        }
        let mut state = test_app_state();
        state.audit_journal = Some(journal);
        let router = APIServer::build_router_for_test(state);

        // WHEN GET /api/v1/audit/journal, naming no run
        let req = Request::builder()
            .uri("/api/v1/audit/journal")
            .body(Body::empty())
            .unwrap();
        let resp = router.oneshot(req).await.unwrap();

        // THEN 200 with both runs in one page, newest first
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), 8192).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["count"].as_u64().unwrap(), 2);
        assert_eq!(json["entries"][0]["run_id"].as_str().unwrap(), "run-2");
        assert_eq!(json["entries"][1]["run_id"].as_str().unwrap(), "run-1");
    }

    // GET /api/v1/audit/journal?agents= pages the runs of those agents only
    #[tokio::test]
    async fn test_list_audit_journal_filters_by_agents() {
        // GIVEN three runs journaled, each the run of a task of a different agent
        use crate::audit_journal::{JournalEntryDraft, JournalEntryKind};
        let journal = open_temp_journal().await;
        let db = std::env::temp_dir().join(format!("apollia_jf_{}.db", uuid::Uuid::new_v4()));
        let repo = apollia_tools::TaskRepository::open(&db).await.unwrap();
        for (task, agent, run) in [
            ("t-1", "flux-quotes", "run-1"),
            ("t-2", "flux-other-space", "run-2"),
            ("t-3", "flux-invoices", "run-3"),
        ] {
            repo.set_run_id(task, run).await.unwrap();
            repo.set_agent_name(task, agent).await.unwrap();
            journal.append(JournalEntryDraft {
                run_id: run.to_string(),
                ts: "2026-01-01T00:00:00Z".to_string(),
                kind: JournalEntryKind::ToolCallStarted,
                payload: serde_json::json!({ "tool_name": "bash" }),
            });
        }
        let mut state = test_app_state();
        state.audit_journal = Some(journal);
        state.task_repository = Some(Arc::new(repo));
        let router = APIServer::build_router_for_test(state);

        // WHEN the page is asked for two of the agents
        let req = Request::builder()
            .uri("/api/v1/audit/journal?agents=flux-quotes,flux-invoices")
            .body(Body::empty())
            .unwrap();
        let resp = router.oneshot(req).await.unwrap();

        // THEN only their runs are in it, newest first
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), 8192).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["count"].as_u64().unwrap(), 2);
        assert_eq!(json["entries"][0]["run_id"].as_str().unwrap(), "run-3");
        assert_eq!(json["entries"][1]["run_id"].as_str().unwrap(), "run-1");
    }

    // The paged journal route answers 503 when no journal is configured
    #[tokio::test]
    async fn test_list_audit_journal_returns_503_when_not_configured() {
        // GIVEN no audit journal in AppState
        let state = test_app_state();
        let router = APIServer::build_router_for_test(state);

        // WHEN GET /api/v1/audit/journal
        let req = Request::builder()
            .uri("/api/v1/audit/journal")
            .body(Body::empty())
            .unwrap();
        let resp = router.oneshot(req).await.unwrap();

        // THEN 503 Service Unavailable, not a 404 from the run-scoped route
        assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    // GET verify with an unknown run returns 404 not_found
    #[tokio::test]
    async fn test_verify_unknown_run_returns_404() {
        // GIVEN a journal with no entries for the requested run
        let journal = open_temp_journal().await;
        let mut state = test_app_state();
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
        let state = test_app_state();
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
        // GIVEN three run identifiers, two of them sharing an eight-character prefix
        let ids = vec![
            "abcd1234-aaaa".to_string(),
            "abcd1234-bbbb".to_string(),
            "ef567890-cccc".to_string(),
        ];

        // WHEN each of the five lookup shapes is resolved
        // THEN the exact identifier wins, an unambiguous prefix resolves, an ambiguous one lists its candidates, and a short or unknown prefix finds nothing
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
        let mut state = test_app_state();
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
        let mut state = test_app_state();
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

    /// A journal holding one write by `quotes` (asked, started, captured,
    /// ended), one failed fetch by `invoices`, a consent `quotes` refused, a
    /// secret set on a connection, and a policy applied.
    async fn journal_of_several_natures() -> crate::audit_journal::AuditJournalHandle {
        use crate::audit_journal::{JournalEntryDraft, JournalEntryKind as K};
        let journal = open_temp_journal().await;
        let drafts = [
            (
                "run-1",
                "2026-10-01T08:00:00Z",
                K::LlmCompletion,
                serde_json::json!({
                    "content": "", "model_id": "m",
                    "tool_calls": [{ "id": "c0", "name": "file_write",
                        "arguments": { "path": "/home/u/a.md", "content": "SECRET-ARGUMENT" } }],
                }),
            ),
            (
                "run-1",
                "2026-10-01T08:00:01Z",
                K::ToolCallStarted,
                serde_json::json!({
                    "tool_name": "file_write", "agent_id": "quotes", "tool_call_id": "c0",
                    "family": "write_or_delete", "target": "/home/u/a.md",
                }),
            ),
            (
                "run-1",
                "2026-10-01T08:00:02Z",
                K::ToolOutput,
                serde_json::json!({
                    "tool_call_id": "c0", "tool_name": "file_write",
                    "output": "SECRET-OUTPUT", "status": "success",
                }),
            ),
            (
                "run-1",
                "2026-10-01T08:00:03Z",
                K::ToolCallCompleted,
                serde_json::json!({
                    "tool_name": "file_write", "agent_id": "quotes", "tool_call_id": "c0",
                    "family": "write_or_delete", "target": "/home/u/a.md", "success": true,
                }),
            ),
            (
                "run-2",
                "2026-10-02T08:00:00Z",
                K::ToolCallCompleted,
                serde_json::json!({
                    "tool_name": "http_fetch", "agent_id": "invoices", "tool_call_id": "e9",
                    "family": "send_external", "target": "example.org", "success": false,
                }),
            ),
            (
                "governance:quotes",
                "2026-10-03T08:00:00Z",
                K::ConsentAnswered,
                serde_json::json!({
                    "event": "consent_answered", "agent_id": "quotes", "access_id": "gmail.send",
                    "family": "send_external", "answer": "refuse",
                }),
            ),
            (
                "connection:mcp:notes",
                "2026-10-04T08:00:00Z",
                K::ConnectionSecretSet,
                serde_json::json!({
                    "event": "secret_set", "connection_id": "mcp:notes", "key": "NOTES_TOKEN",
                    "actor": "api",
                }),
            ),
            (
                "governance:policy:organisation",
                "2026-10-05T08:00:00Z",
                K::PolicyApplied,
                serde_json::json!({
                    "event": "policy_applied", "key_fingerprint": "ab", "policy_digest": "d1",
                    "publisher": "Acme IT", "signed_at": "2026-10-01T00:00:00Z",
                }),
            ),
        ];
        for (run, ts, kind, payload) in drafts {
            journal.append(JournalEntryDraft {
                run_id: run.to_string(),
                ts: ts.to_string(),
                kind,
                payload,
            });
        }
        journal
    }

    /// GETs `uri` on a router over `journal`, with a task repository that
    /// knows no task. Returns the status and the JSON body.
    async fn get_journal(
        journal: crate::audit_journal::AuditJournalHandle,
        uri: &str,
    ) -> (StatusCode, serde_json::Value) {
        let db = std::env::temp_dir().join(format!("apollia_jq_{}.db", uuid::Uuid::new_v4()));
        let repo = apollia_tools::TaskRepository::open(&db).await.unwrap();
        let mut state = test_app_state();
        state.audit_journal = Some(journal);
        state.task_repository = Some(Arc::new(repo));
        let router = APIServer::build_router_for_test(state);
        let req = Request::builder().uri(uri).body(Body::empty()).unwrap();
        let resp = router.oneshot(req).await.unwrap();
        let status = resp.status();
        let body = axum::body::to_bytes(resp.into_body(), 65536).await.unwrap();
        (status, serde_json::from_slice(&body).unwrap())
    }

    /// The `kind` of each entry of a page, in order.
    fn kinds(page: &serde_json::Value) -> Vec<&str> {
        page["entries"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| e["kind"].as_str().unwrap())
            .collect()
    }

    // Every entry of a page carries its summary beside its chained fields.
    #[tokio::test]
    async fn test_list_audit_journal_gives_each_entry_its_summary() {
        // GIVEN a journal holding a finished write on a path
        let journal = journal_of_several_natures().await;

        // WHEN the page of the successful actions is read
        let (status, page) = get_journal(
            journal,
            "/api/v1/audit/journal?nature=action&result=success",
        )
        .await;

        // THEN the one entry comes chained, with a summary that says who did
        // what to what and how it ended
        assert_eq!(status, StatusCode::OK);
        assert_eq!(page["count"], 1);
        let entry = &page["entries"][0];
        assert_eq!(entry["run_id"], "run-1");
        assert_eq!(entry["seq"], 3);
        assert!(entry["hash"].as_str().is_some_and(|h| !h.is_empty()));
        assert_eq!(
            entry["summary"],
            serde_json::json!({
                "agent": "quotes", "nature": "action", "family": "write_or_delete",
                "what": "Tool call finished", "tool": "file_write", "target": "/home/u/a.md",
                "outcome": "success", "ts": "2026-10-01T08:00:03Z",
            })
        );
    }

    // Each filter narrows the page, and they add up.
    #[tokio::test]
    async fn test_list_audit_journal_filters_by_family_nature_result_and_period() {
        let cases: [(&str, &[&str]); 9] = [
            (
                "family=send_external",
                &["consent_answered", "tool_call_completed"],
            ),
            ("nature=access_change", &["connection_secret_set"]),
            ("nature=policy", &["policy_applied"]),
            (
                "nature=policy,access_change",
                &["policy_applied", "connection_secret_set"],
            ),
            ("result=refused", &["consent_answered"]),
            ("result=failure", &["tool_call_completed"]),
            (
                "since=2026-10-04",
                &["policy_applied", "connection_secret_set"],
            ),
            (
                "since=2026-10-02&until=2026-10-04",
                &["consent_answered", "tool_call_completed"],
            ),
            (
                "nature=action&family=write_or_delete&result=success&until=2026-10-02",
                &["tool_call_completed"],
            ),
        ];
        for (query, expected) in cases {
            // GIVEN a journal holding actions, an access change and a policy entry
            let journal = journal_of_several_natures().await;

            // WHEN the page is read under each filter
            let (status, page) =
                get_journal(journal, &format!("/api/v1/audit/journal?{query}")).await;

            // THEN it holds the matching entries only, newest first
            assert_eq!(status, StatusCode::OK, "{query}");
            assert_eq!(kinds(&page), expected, "{query}");
        }
    }

    // The agent filter keeps what names the agent, in whatever run it sits.
    #[tokio::test]
    async fn test_list_audit_journal_keeps_what_names_the_agent() {
        // GIVEN a journal where `quotes` acted in a run and answered a consent
        // recorded in its governance run
        let journal = journal_of_several_natures().await;

        // WHEN the page of that agent is read
        let (status, page) = get_journal(journal, "/api/v1/audit/journal?agents=quotes").await;

        // THEN its consent and its call are there, and nothing of `invoices`
        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            kinds(&page),
            [
                "consent_answered",
                "tool_call_completed",
                "tool_call_started"
            ]
        );
    }

    // The filters apply before paging: limit and offset count matching entries.
    #[tokio::test]
    async fn test_list_audit_journal_pages_the_matching_entries() {
        // GIVEN a journal with four action entries among others
        // WHEN the actions are read one per page
        let mut seen = Vec::new();
        for offset in 0..4 {
            let journal = journal_of_several_natures().await;
            let (_, page) = get_journal(
                journal,
                &format!("/api/v1/audit/journal?nature=action&limit=1&offset={offset}"),
            )
            .await;
            seen.extend(kinds(&page).into_iter().map(str::to_owned));
        }

        // THEN the pages hold the four actions in order and nothing else
        assert_eq!(
            seen,
            [
                "consent_answered",
                "tool_call_completed",
                "tool_call_completed",
                "tool_call_started"
            ]
        );
    }

    // A filter value nobody defined is a 400 that names it.
    #[tokio::test]
    async fn test_list_audit_journal_refuses_an_unreadable_filter() {
        for (query, named) in [
            ("nature=everything", "everything"),
            ("result=ok", "ok"),
            ("since=last-week", "last-week"),
        ] {
            // GIVEN a journal
            let journal = journal_of_several_natures().await;

            // WHEN the page is asked with a value that cannot be read
            let (status, body) =
                get_journal(journal, &format!("/api/v1/audit/journal?{query}")).await;

            // THEN 400, and the error names the value
            assert_eq!(status, StatusCode::BAD_REQUEST, "{query}");
            assert!(body["error"].as_str().unwrap().contains(named), "{body}");
        }
    }

    // No summary of a page holds what a call wrote or returned.
    #[tokio::test]
    async fn test_no_summary_of_a_page_holds_an_argument_or_an_output() {
        // GIVEN a journal whose run kept a model response and a tool output
        let journal = journal_of_several_natures().await;

        // WHEN the whole journal is read
        let (_, page) = get_journal(journal, "/api/v1/audit/journal?limit=100").await;

        // THEN every entry has a summary, and none says the argument or the output
        let entries = page["entries"].as_array().unwrap();
        assert_eq!(entries.len(), 8);
        for entry in entries {
            let said = entry["summary"].to_string();
            assert!(entry["summary"]["what"]
                .as_str()
                .is_some_and(|w| !w.is_empty()));
            assert!(!said.contains("SECRET"), "{said}");
        }
    }

    // The detail of a call gives its arguments and output, joined on demand.
    #[tokio::test]
    async fn test_show_audit_entry_joins_the_arguments_and_output_of_a_call() {
        // GIVEN a journal holding a finished write, at seq 3 of run-1
        let journal = journal_of_several_natures().await;

        // WHEN its detail is read
        let (status, body) = get_journal(journal, "/api/v1/audit/journal/run-1/entries/3").await;

        // THEN the entry comes with its summary and the full call
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["entry"]["kind"], "tool_call_completed");
        assert_eq!(body["entry"]["summary"]["target"], "/home/u/a.md");
        assert_eq!(
            body["entry"]["tool_call"],
            serde_json::json!({
                "tool_call_id": "c0",
                "arguments": { "path": "/home/u/a.md", "content": "SECRET-ARGUMENT" },
                "output": "SECRET-OUTPUT",
                "status": "success",
            })
        );
    }

    // The detail of an entry that is not a call has no call, and an entry the
    // run does not have is a 404.
    #[tokio::test]
    async fn test_show_audit_entry_without_a_call_and_unknown_entry() {
        // GIVEN a journal holding a secret set on a connection
        // WHEN its detail is read, then the detail of a missing entry
        let (status, body) = get_journal(
            journal_of_several_natures().await,
            "/api/v1/audit/journal/connection:mcp:notes/entries/0",
        )
        .await;
        let (missing, _) = get_journal(
            journal_of_several_natures().await,
            "/api/v1/audit/journal/run-1/entries/99",
        )
        .await;

        // THEN the first is the entry whole without a call, the second a 404
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["entry"]["payload"]["key"], "NOTES_TOKEN");
        assert!(body["entry"]["tool_call"].is_null());
        assert_eq!(missing, StatusCode::NOT_FOUND);
    }
}
