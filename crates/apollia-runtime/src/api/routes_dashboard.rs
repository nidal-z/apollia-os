//! Dashboard routes for Apollia OS runtime.
//!
//! Exposes four routes:
//! - `GET /dashboard`                         — serves the embedded HTML placeholder (AC-1).
//! - `GET /api/v1/dashboard/state`            — returns a full JSON snapshot of runtime state (AC-2).
//! - `GET /api/v1/dashboard/partials/:section` — returns HTML fragments for HTMX (AC-3/AC-4).
//! - `GET /api/v1/dashboard/stream`           — SSE bridge: RuntimeEvent → named HTMX events (STORY-076).
//!
//! Supported sections: `"agents"` | `"triggers"` | `"tasks"` | `"llm"` | `"tools"` | `"memory"`.
//! Unknown sections return HTTP 404 (AC-4).
//!
//! STORY-077 will replace the placeholder HTML with the full HTMX dashboard.

use std::convert::Infallible;

use axum::{
    extract::{Path, State},
    http::{header, StatusCode},
    response::{
        sse::{Event, KeepAlive, Sse},
        Html, IntoResponse,
    },
    Json,
};
use chrono::Utc;
use serde::Serialize;
use tokio_stream::wrappers::UnboundedReceiverStream;
use tokio_stream::StreamExt;

use apollia_core::{ProcessState, RuntimeEvent};
use apollia_llm::BackendInfo;
use apollia_triggers::TriggerStatus;

use crate::api::server::AppState;
use crate::coordinator::ExecutionBackend;

// ─── Static assets ────────────────────────────────────────────────────────

/// Full HTMX dashboard HTML served at `GET /dashboard`.
///
/// Embedded at compile time via `include_str!`. The `CARGO_MANIFEST_DIR`-based
/// path ensures correctness regardless of where `cargo build` is invoked
/// (AC-6 + Principle #2 — Zéro dépendance externe).
static DASHBOARD_HTML: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/assets/dashboard.html"
));

/// HTMX v1.9.x minified JavaScript, served at `GET /static/htmx.min.js`.
///
/// Embedded at compile time via `include_str!`. Browsers load HTMX from the
/// runtime itself — no CDN, no external network access required (Principle #1
/// and #2, AC-5).
static HTMX_JS: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/htmx.min.js"));

// ─── Response types ────────────────────────────────────────────────────────

/// Complete snapshot of the runtime state, returned by `GET /api/v1/dashboard/state`.
#[derive(Debug, Serialize)]
pub struct DashboardState {
    /// Summary of all registered agents.
    pub agents: Vec<AgentSummary>,
    /// Status of all configured triggers.
    pub triggers: Vec<TriggerStatus>,
    /// Most recent tasks (placeholder — populated by STORY-076).
    pub recent_tasks: Vec<TaskSummary>,
    /// LLM backends declared in `apollia.toml`.
    pub llm_backends: Vec<BackendInfo>,
    /// Circuit breaker state per tool (placeholder — populated when ORIA is wired in).
    pub tool_circuits: Vec<CircuitSummary>,
    /// Aggregate memory stats (placeholder).
    pub memory_stats: Option<serde_json::Value>,
    /// Seconds since the runtime started (placeholder — 0 until uptime tracking is added).
    pub uptime_secs: u64,
    /// RFC 3339 timestamp of this snapshot.
    pub timestamp: String,
}

/// Lightweight summary of an agent for the dashboard.
#[derive(Debug, Serialize)]
pub struct AgentSummary {
    /// Agent identifier (UUID).
    pub id: String,
    /// Human-readable status string: `"active"` | `"degraded"` | `"stopped"` | etc.
    pub status: String,
    /// Number of tasks currently active for this agent (placeholder — always 0).
    pub task_count: u32,
}

/// Lightweight summary of a task for the dashboard.
#[derive(Debug, Serialize)]
pub struct TaskSummary {
    /// Task identifier.
    pub id: String,
    /// Agent that owns the task.
    pub agent: String,
    /// Task status string.
    pub status: String,
    /// RFC 3339 start timestamp.
    pub started_at: String,
}

/// Circuit breaker state summary for a single tool.
#[derive(Debug, Serialize)]
pub struct CircuitSummary {
    /// Tool name.
    pub tool_name: String,
    /// Circuit state: `"closed"` | `"open"` | `"half_open"`.
    pub state: String,
}

/// A named SSE event destined for a specific dashboard channel.
///
/// Used by [`dashboard_event_to_sse`] to carry plan/step payloads before
/// they are converted to the axum [`Event`] type. Having a typed intermediate
/// makes the routing logic unit-testable without an HTTP layer.
#[derive(Debug)]
pub(crate) struct DashboardSseEvent {
    /// Dashboard channel name (e.g. `"tasks"`, `"agents"`).
    pub(crate) channel: String,
    /// JSON payload forwarded verbatim to the browser.
    pub(crate) data: serde_json::Value,
}

// ─── Helpers ───────────────────────────────────────────────────────────────

/// Map a [`ProcessState`] to a dashboard status string.
fn process_state_label(state: &ProcessState) -> &'static str {
    match state {
        ProcessState::Initializing => "initializing",
        ProcessState::Active => "active",
        ProcessState::Degraded => "degraded",
        ProcessState::Stopping => "stopping",
        ProcessState::Stopped => "stopped",
    }
}

// ─── Handlers ─────────────────────────────────────────────────────────────

/// Serve the embedded HTML dashboard at `GET /dashboard`.
///
/// Returns HTTP 200 with `Content-Type: text/html`. The body is the full HTMX
/// dashboard compiled into the binary via `include_str!` (AC-2, AC-6).
pub async fn get_dashboard() -> impl IntoResponse {
    Html(DASHBOARD_HTML)
}

/// Serve the embedded HTMX JavaScript at `GET /static/htmx.min.js`.
///
/// Returns HTTP 200 with `Content-Type: application/javascript`. The body is
/// HTMX v1.9.x minified, compiled into the binary — no CDN required (AC-1, AC-5).
pub async fn get_htmx_js() -> impl IntoResponse {
    (
        [(
            header::CONTENT_TYPE,
            "application/javascript; charset=utf-8",
        )],
        HTMX_JS,
    )
}

/// Return a full JSON snapshot of the runtime state at `GET /api/v1/dashboard/state`.
///
/// Aggregates state from all available handles in [`AppState`]:
/// - Agent registry → `agents`
/// - Trigger engine → `triggers` (empty array when no engine is configured, AC-5)
/// - LLM router     → `llm_backends`
///
/// Fields `recent_tasks`, `tool_circuits`, `memory_stats`, and `uptime_secs` are
/// placeholders populated by later stories (AC-2).
pub async fn get_dashboard_state<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
) -> impl IntoResponse {
    // Agents — ignore registry errors and return empty list gracefully.
    let agents: Vec<AgentSummary> = state
        .registry_handle
        .list_agents()
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|entry| AgentSummary {
            id: entry.id.to_string(),
            status: process_state_label(&entry.process_state).to_string(),
            task_count: 0,
        })
        .collect();

    // Triggers — always present, empty when TriggerEngine is not configured (AC-5).
    let triggers: Vec<TriggerStatus> = match &state.trigger_engine {
        Some(engine) => engine.list().await,
        None => vec![],
    };

    // LLM backends — empty when no LlmRouter is configured.
    let llm_backends: Vec<BackendInfo> = state
        .llm_router
        .as_ref()
        .map(|r| r.list())
        .unwrap_or_default();

    let snapshot = DashboardState {
        agents,
        triggers,
        recent_tasks: vec![],
        llm_backends,
        tool_circuits: vec![],
        memory_stats: None,
        uptime_secs: 0,
        timestamp: Utc::now().to_rfc3339(),
    };

    Json(snapshot)
}

/// Return an HTML fragment for a dashboard section at `GET /api/v1/dashboard/partials/:section`.
///
/// Supported sections: `"agents"` | `"triggers"` | `"tasks"` | `"llm"` | `"tools"` | `"memory"`.
/// Returns HTTP 404 for any unknown section name (AC-4).
pub async fn get_dashboard_partial<B: ExecutionBackend + Clone>(
    Path(section): Path<String>,
    State(state): State<AppState<B>>,
) -> impl IntoResponse {
    match section.as_str() {
        "agents" => render_agents_partial(&state).await.into_response(),
        "triggers" => render_triggers_partial(&state).await.into_response(),
        "tasks" => render_tasks_partial().into_response(),
        "llm" => render_llm_partial(&state).into_response(),
        "tools" => render_tools_partial().into_response(),
        "memory" => render_memory_partial().into_response(),
        _ => StatusCode::NOT_FOUND.into_response(),
    }
}

// ─── Partial renderers ────────────────────────────────────────────────────

/// Render the agents section as an HTML `<table>`.
async fn render_agents_partial<B: ExecutionBackend + Clone>(state: &AppState<B>) -> Html<String> {
    let agents = state
        .registry_handle
        .list_agents()
        .await
        .unwrap_or_default();

    let mut rows = String::new();
    for entry in &agents {
        rows.push_str(&format!(
            "<tr><td>{}</td><td>{}</td></tr>",
            entry.id,
            process_state_label(&entry.process_state),
        ));
    }

    Html(format!(
        "<table>\
           <thead><tr><th>ID</th><th>Status</th></tr></thead>\
           <tbody>{}</tbody>\
         </table>",
        rows
    ))
}

/// Render the triggers section as an HTML `<table>`.
async fn render_triggers_partial<B: ExecutionBackend + Clone>(state: &AppState<B>) -> Html<String> {
    let triggers: Vec<TriggerStatus> = match &state.trigger_engine {
        Some(engine) => engine.list().await,
        None => vec![],
    };

    let mut rows = String::new();
    for t in &triggers {
        let status = if t.enabled { "active" } else { "disabled" };
        rows.push_str(&format!(
            "<tr><td>{}</td><td>{}</td><td>{}</td></tr>",
            t.id, t.agent, status,
        ));
    }

    Html(format!(
        "<table>\
           <thead><tr><th>ID</th><th>Agent</th><th>Status</th></tr></thead>\
           <tbody>{}</tbody>\
         </table>",
        rows
    ))
}

/// Render the tasks section as an HTML `<table>` (placeholder — always empty).
fn render_tasks_partial() -> Html<&'static str> {
    Html(
        "<table>\
           <thead><tr><th>ID</th><th>Agent</th><th>Status</th></tr></thead>\
           <tbody></tbody>\
         </table>",
    )
}

/// Render the LLM backends section as an HTML `<table>`.
fn render_llm_partial<B: ExecutionBackend + Clone>(state: &AppState<B>) -> Html<String> {
    let backends = state
        .llm_router
        .as_ref()
        .map(|r| r.list())
        .unwrap_or_default();

    let mut rows = String::new();
    for b in &backends {
        let status = if b.available {
            "available"
        } else {
            "unavailable"
        };
        rows.push_str(&format!(
            "<tr><td>{}</td><td>{}</td><td>{}</td></tr>",
            b.name, b.model_id, status,
        ));
    }

    Html(format!(
        "<table>\
           <thead><tr><th>Name</th><th>Model</th><th>Status</th></tr></thead>\
           <tbody>{}</tbody>\
         </table>",
        rows
    ))
}

/// Render the tools circuit breaker section (placeholder — always empty).
fn render_tools_partial() -> Html<&'static str> {
    Html(
        "<table>\
           <thead><tr><th>Tool</th><th>Circuit</th></tr></thead>\
           <tbody></tbody>\
         </table>",
    )
}

/// Render the memory section (placeholder — always empty).
fn render_memory_partial() -> Html<&'static str> {
    Html(
        "<table>\
           <thead><tr><th>Namespace</th><th>Stats</th></tr></thead>\
           <tbody></tbody>\
         </table>",
    )
}

// ─── SSE Dashboard Stream ─────────────────────────────────────────────────

/// Map an orchestration-related [`RuntimeEvent`] to a [`DashboardSseEvent`] on
/// the `"tasks"` channel (AC-4).
///
/// Returns `None` for any event that is not a plan/step variant. The separate
/// [`runtime_event_to_sse_events`] function handles all other dashboard events
/// and delegates to this one for plan variants.
pub(crate) fn dashboard_event_to_sse(event: &RuntimeEvent) -> Option<DashboardSseEvent> {
    match event {
        // Plan generated by the Reasoner — signals the browser to show the step list.
        RuntimeEvent::PlanGenerated {
            task_id,
            agent_name,
            plan_id,
            step_count,
        } => Some(DashboardSseEvent {
            channel: "tasks".into(),
            data: serde_json::json!({
                "event_type": "plan_generated",
                "task_id":    task_id,
                "agent_name": agent_name,
                "plan_id":    plan_id,
                "step_count": step_count,
            }),
        }),

        // A step has started execution — the browser marks it as running.
        RuntimeEvent::StepStarted {
            task_id,
            plan_id: _,
            step_id,
            step_num,
            total,
            desc,
        } => Some(DashboardSseEvent {
            channel: "tasks".into(),
            data: serde_json::json!({
                "event_type": "step_started",
                "task_id":  task_id,
                "step_id":  step_id,
                "num":      step_num,
                "total":    total,
                "desc":     desc,
            }),
        }),

        // A step completed successfully — the browser marks it with a checkmark.
        RuntimeEvent::StepCompleted {
            task_id,
            plan_id: _,
            step_id,
            duration_ms,
        } => Some(DashboardSseEvent {
            channel: "tasks".into(),
            data: serde_json::json!({
                "event_type":  "step_completed",
                "task_id":     task_id,
                "step_id":     step_id,
                "duration_ms": duration_ms,
            }),
        }),

        // A step failed — the browser marks it with an error indicator.
        RuntimeEvent::StepFailed {
            task_id,
            plan_id: _,
            step_id,
            error,
            retryable,
        } => Some(DashboardSseEvent {
            channel: "tasks".into(),
            data: serde_json::json!({
                "event_type": "step_failed",
                "task_id":   task_id,
                "step_id":   step_id,
                "error":     error,
                "retryable": retryable,
            }),
        }),

        // A replanning was triggered — the browser shows the replanning banner.
        RuntimeEvent::PlanReplanning {
            task_id,
            plan_id: _,
            attempt,
            failed_step,
            reason,
        } => Some(DashboardSseEvent {
            channel: "tasks".into(),
            data: serde_json::json!({
                "event_type":  "replanning",
                "task_id":     task_id,
                "attempt":     attempt,
                "failed_step": failed_step,
                "reason":      reason,
            }),
        }),

        // All steps completed — the browser borders the plan in green.
        RuntimeEvent::PlanCompleted {
            task_id,
            plan_id: _,
            step_count,
            duration_ms,
        } => Some(DashboardSseEvent {
            channel: "tasks".into(),
            data: serde_json::json!({
                "event_type":  "plan_completed",
                "task_id":     task_id,
                "step_count":  step_count,
                "duration_ms": duration_ms,
            }),
        }),

        // The plan failed irrecoverably — the browser borders the plan in red.
        RuntimeEvent::PlanFailed {
            task_id,
            plan_id: _,
            reason,
        } => Some(DashboardSseEvent {
            channel: "tasks".into(),
            data: serde_json::json!({
                "event_type": "plan_failed",
                "task_id":    task_id,
                "reason":     reason,
            }),
        }),

        _ => None,
    }
}

/// Map a [`RuntimeEvent`] to 0, 1, or 2 named SSE [`Event`]s for the dashboard.
///
/// Returns `None` for events irrelevant to the dashboard (e.g. `AllReady`,
/// `ShutdownRequested`). Returns `Some(vec![...])` with 1 or 2 named events
/// otherwise. Each event carries `data: refresh` — HTMX uses the event name
/// to decide which partial to reload.
///
/// This function is pure and unit-tested independently of the HTTP layer.
pub(crate) fn runtime_event_to_sse_events(event: RuntimeEvent) -> Option<Vec<Event>> {
    match event {
        RuntimeEvent::AgentReady(_)
        | RuntimeEvent::AgentStopped(_)
        | RuntimeEvent::AgentDegraded { .. } => {
            Some(vec![Event::default().event("agents").data("refresh")])
        }
        RuntimeEvent::TaskCompleted { .. } | RuntimeEvent::TaskStarted { .. } => Some(vec![
            Event::default().event("tasks").data("refresh"),
            Event::default().event("agents").data("refresh"),
        ]),
        RuntimeEvent::TriggerFired { .. }
        | RuntimeEvent::TriggerSkipped { .. }
        | RuntimeEvent::TriggerError { .. }
        | RuntimeEvent::TriggersReloaded { .. } => {
            Some(vec![Event::default().event("triggers").data("refresh")])
        }
        RuntimeEvent::LlmCallCompleted { .. }
        | RuntimeEvent::LlmModelReady { .. }
        | RuntimeEvent::LlmModelFailed { .. } => {
            Some(vec![Event::default().event("llm").data("refresh")])
        }
        RuntimeEvent::ToolCircuitBroken { .. } | RuntimeEvent::ToolCircuitRestored { .. } => {
            Some(vec![Event::default().event("tools").data("refresh")])
        }
        // Plan/step events (STORY-090) — delegate to dashboard_event_to_sse for routing.
        // Each event carries a JSON payload (not "refresh") so the JavaScript handler
        // in the browser can update #current-plan without a full partial reload.
        ref plan_event @ (RuntimeEvent::PlanGenerated { .. }
        | RuntimeEvent::StepStarted { .. }
        | RuntimeEvent::StepCompleted { .. }
        | RuntimeEvent::StepFailed { .. }
        | RuntimeEvent::PlanReplanning { .. }
        | RuntimeEvent::PlanCompleted { .. }
        | RuntimeEvent::PlanFailed { .. }) => dashboard_event_to_sse(plan_event)
            .map(|e| vec![Event::default().event(e.channel).data(e.data.to_string())]),
        _ => None,
    }
}

/// SSE bridge from [`EventBus`] to browser dashboard via `GET /api/v1/dashboard/stream`.
///
/// Subscribes to the runtime EventBus, filters [`RuntimeEvent`]s relevant to
/// the dashboard, and pushes **named SSE events** so HTMX can selectively reload
/// only the affected section. A keep-alive comment (`: keep-alive`) is sent
/// every 30 s to prevent proxy timeouts (AC-5).
///
/// A Tokio task bridges the broadcast receiver to an unbounded channel.
/// When the client disconnects, `tx.is_closed()` becomes true, and the task
/// exits on the next received EventBus event — no subscriber leak (AC-6).
pub async fn dashboard_stream<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
) -> Sse<impl tokio_stream::Stream<Item = Result<Event, Infallible>>> {
    use tokio::sync::broadcast::error::RecvError;

    let mut bus_rx = state.event_sender.subscribe();
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<Event>();

    // Bridge task: map RuntimeEvents to SSE Events and forward through the channel.
    // Exits when the client disconnects (tx.is_closed()) or the EventBus closes.
    tokio::spawn(async move {
        loop {
            if tx.is_closed() {
                return;
            }
            match bus_rx.recv().await {
                Ok(event) => {
                    if let Some(events) = runtime_event_to_sse_events(event) {
                        for sse_event in events {
                            if tx.send(sse_event).is_err() {
                                return;
                            }
                        }
                    }
                }
                Err(RecvError::Lagged(_)) => {} // skip missed events silently
                Err(RecvError::Closed) => return,
            }
        }
    });

    let stream = UnboundedReceiverStream::new(rx).map(Ok::<Event, Infallible>);

    Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(std::time::Duration::from_secs(30))
            .text("keep-alive"),
    )
}

// ─── Tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::server::AppState;
    use crate::coordinator::ExecutionBackend;
    use crate::eventbus::EventBus;
    use crate::registry::AgentRegistry;
    use crate::router::TaskRouterHandle;
    use apollia_core::{AIPResult, AIPTask, TaskStatus};
    use apollia_triggers::{TaskSubmitter, TriggerEngineHandle};
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use axum::routing::get;
    use axum::Router;
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

    struct MockSubmitter;

    impl TaskSubmitter for MockSubmitter {
        fn submit<'a>(
            &'a self,
            _agent: &'a str,
            _input: apollia_core::AIPInput,
        ) -> Pin<Box<dyn Future<Output = Result<apollia_core::TaskId, String>> + Send + 'a>>
        {
            Box::pin(async { Ok(apollia_core::TaskId::new_v4()) })
        }

        fn pending_count<'a>(
            &'a self,
            _agent: &'a str,
        ) -> Pin<Box<dyn Future<Output = usize> + Send + 'a>> {
            Box::pin(async { 0 })
        }
    }

    async fn make_test_app_state() -> AppState<MockBackend> {
        let (event_tx, _) = EventBus::new();
        let registry = AgentRegistry::spawn(event_tx.clone());
        let router: TaskRouterHandle<MockBackend> =
            TaskRouterHandle::spawn(registry.clone(), event_tx.clone(), 64);
        AppState {
            router_handle: router,
            registry_handle: registry,
            event_sender: event_tx,
            agent_loader: Arc::new(crate::api::routes_agents::StubAgentLoader),
            backend: MockBackend,
            llm_router: None,
            trigger_engine: None,
            config_path: None,
        }
    }

    async fn make_test_app_state_with_engine() -> AppState<MockBackend> {
        let (event_tx, _) = EventBus::new();
        let registry = AgentRegistry::spawn(event_tx.clone());
        let router: TaskRouterHandle<MockBackend> =
            TaskRouterHandle::spawn(registry.clone(), event_tx.clone(), 64);
        let engine =
            TriggerEngineHandle::spawn(vec![], MockSubmitter, event_tx.clone(), None).await;
        AppState {
            router_handle: router,
            registry_handle: registry,
            event_sender: event_tx,
            agent_loader: Arc::new(crate::api::routes_agents::StubAgentLoader),
            backend: MockBackend,
            llm_router: None,
            trigger_engine: Some(engine),
            config_path: None,
        }
    }

    fn make_router(state: AppState<MockBackend>) -> Router {
        Router::new()
            .route("/dashboard", get(get_dashboard))
            .route(
                "/api/v1/dashboard/state",
                get(get_dashboard_state::<MockBackend>),
            )
            .route(
                "/api/v1/dashboard/partials/:section",
                get(get_dashboard_partial::<MockBackend>),
            )
            .with_state(state)
    }

    /// AC-1 — GET /dashboard returns HTTP 200 with Content-Type text/html and "Apollia OS".
    #[tokio::test]
    async fn test_ac1_get_dashboard_returns_html() {
        // GIVEN a started APIServer
        let state = make_test_app_state().await;
        let router = make_router(state);

        // WHEN GET /dashboard
        let req = Request::builder()
            .uri("/dashboard")
            .body(Body::empty())
            .unwrap();
        let resp = router.oneshot(req).await.unwrap();

        // THEN HTTP 200
        assert_eq!(resp.status(), StatusCode::OK);

        // AND Content-Type: text/html
        let ct = resp
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        assert!(ct.contains("text/html"), "expected text/html, got: {ct}");

        // AND body contains "Apollia OS"
        let body = axum::body::to_bytes(resp.into_body(), 8192).await.unwrap();
        let body_str = std::str::from_utf8(&body).unwrap();
        assert!(
            body_str.contains("Apollia OS"),
            "body should contain 'Apollia OS'"
        );
    }

    /// AC-2 — GET /api/v1/dashboard/state returns JSON with required keys.
    #[tokio::test]
    async fn test_ac2_dashboard_state_contains_required_keys() {
        // GIVEN AppState with mock handles
        let state = make_test_app_state().await;
        let router = make_router(state);

        // WHEN GET /api/v1/dashboard/state
        let req = Request::builder()
            .uri("/api/v1/dashboard/state")
            .body(Body::empty())
            .unwrap();
        let resp = router.oneshot(req).await.unwrap();

        // THEN HTTP 200
        assert_eq!(resp.status(), StatusCode::OK);

        // AND JSON contains the 6 required keys
        let body = axum::body::to_bytes(resp.into_body(), 65536).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(json["agents"].is_array(), "missing 'agents'");
        assert!(json["triggers"].is_array(), "missing 'triggers'");
        assert!(json["recent_tasks"].is_array(), "missing 'recent_tasks'");
        assert!(json["llm_backends"].is_array(), "missing 'llm_backends'");
        assert!(json["tool_circuits"].is_array(), "missing 'tool_circuits'");
        assert!(json.get("timestamp").is_some(), "missing 'timestamp'");
    }

    /// AC-3 — GET /api/v1/dashboard/partials/agents returns HTML.
    #[tokio::test]
    async fn test_ac3_agents_partial_returns_html() {
        // GIVEN AppState
        let state = make_test_app_state().await;
        let router = make_router(state);

        // WHEN GET /api/v1/dashboard/partials/agents
        let req = Request::builder()
            .uri("/api/v1/dashboard/partials/agents")
            .body(Body::empty())
            .unwrap();
        let resp = router.oneshot(req).await.unwrap();

        // THEN HTTP 200
        assert_eq!(resp.status(), StatusCode::OK);

        // AND Content-Type: text/html
        let ct = resp
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        assert!(ct.contains("text/html"), "expected text/html, got: {ct}");
    }

    /// AC-4 — GET /api/v1/dashboard/partials/unknown-section returns 404.
    #[tokio::test]
    async fn test_ac4_unknown_partial_returns_404() {
        // GIVEN AppState
        let state = make_test_app_state().await;
        let router = make_router(state);

        // WHEN GET /api/v1/dashboard/partials/unknown-section
        let req = Request::builder()
            .uri("/api/v1/dashboard/partials/unknown-section")
            .body(Body::empty())
            .unwrap();
        let resp = router.oneshot(req).await.unwrap();

        // THEN HTTP 404
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    /// AC-5 — GET /api/v1/dashboard/state without triggers returns "triggers": [].
    #[tokio::test]
    async fn test_ac5_dashboard_state_triggers_empty_when_no_engine() {
        // GIVEN runtime started without triggers configured
        let state = make_test_app_state().await;
        let router = make_router(state);

        // WHEN GET /api/v1/dashboard/state
        let req = Request::builder()
            .uri("/api/v1/dashboard/state")
            .body(Body::empty())
            .unwrap();
        let resp = router.oneshot(req).await.unwrap();

        // THEN "triggers": [] is present (not absent)
        let body = axum::body::to_bytes(resp.into_body(), 65536).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(
            json.get("triggers").is_some(),
            "key 'triggers' must be present"
        );
        assert_eq!(
            json["triggers"],
            serde_json::json!([]),
            "triggers must be []"
        );
    }

    /// All 6 sections return HTTP 200 (not 404).
    #[tokio::test]
    async fn test_all_sections_return_200() {
        // GIVEN AppState with TriggerEngine
        let state = make_test_app_state_with_engine().await;
        let router = make_router(state);

        let sections = ["agents", "triggers", "tasks", "llm", "tools", "memory"];
        for section in sections {
            let req = Request::builder()
                .uri(format!("/api/v1/dashboard/partials/{section}"))
                .body(Body::empty())
                .unwrap();
            let resp = router.clone().oneshot(req).await.unwrap();
            assert_eq!(
                resp.status(),
                StatusCode::OK,
                "section '{section}' should return 200"
            );
        }
    }

    // ─── SSE unit tests (STORY-076) ─────────────────────────────────────

    /// AC-1 — GET /api/v1/dashboard/stream returns HTTP 200 with text/event-stream.
    #[tokio::test]
    async fn test_sse_ac1_stream_returns_event_stream_content_type() {
        // GIVEN a started APIServer state
        let state = make_test_app_state().await;
        let router = Router::new()
            .route(
                "/api/v1/dashboard/stream",
                get(dashboard_stream::<MockBackend>),
            )
            .with_state(state);

        // WHEN GET /api/v1/dashboard/stream
        let req = Request::builder()
            .uri("/api/v1/dashboard/stream")
            .body(Body::empty())
            .unwrap();
        let resp = router.oneshot(req).await.unwrap();

        // THEN HTTP 200
        assert_eq!(resp.status(), StatusCode::OK);

        // AND Content-Type: text/event-stream
        let ct = resp
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        assert!(
            ct.contains("text/event-stream"),
            "expected text/event-stream, got: {ct}"
        );
    }

    /// AC-2 — AgentReady maps to one "agents" SSE event.
    #[test]
    fn test_sse_ac2_agent_ready_maps_to_agents_event() {
        // GIVEN AgentReady event
        let result = runtime_event_to_sse_events(RuntimeEvent::AgentReady("agent-1".into()));

        // THEN one event produced
        let events = result.expect("AgentReady should produce events");
        assert_eq!(events.len(), 1, "expected 1 event for AgentReady");
    }

    /// AC-2 — AgentStopped also maps to "agents" section.
    #[test]
    fn test_sse_ac2_agent_stopped_maps_to_agents_event() {
        // GIVEN AgentStopped event
        let result = runtime_event_to_sse_events(RuntimeEvent::AgentStopped("agent-1".into()));

        // THEN one event produced
        let events = result.expect("AgentStopped should produce events");
        assert_eq!(events.len(), 1, "expected 1 event for AgentStopped");
    }

    /// AC-3 — TriggerFired maps to one "triggers" SSE event.
    #[test]
    fn test_sse_ac3_trigger_fired_maps_to_triggers_event() {
        // GIVEN TriggerFired event
        let result = runtime_event_to_sse_events(RuntimeEvent::TriggerFired {
            trigger_id: "rapport-hebdo".into(),
            agent: "rapport-agent".into(),
            task_id: "t-001".into(),
        });

        // THEN one event produced
        let events = result.expect("TriggerFired should produce events");
        assert_eq!(events.len(), 1, "expected 1 event for TriggerFired");
    }

    /// AC-4 — TaskCompleted maps to two events: "tasks" and "agents".
    #[test]
    fn test_sse_ac4_task_completed_maps_to_two_events() {
        // GIVEN TaskCompleted event
        let result = runtime_event_to_sse_events(RuntimeEvent::TaskCompleted {
            task_id: "t-001".into(),
            agent_id: "agent-1".into(),
            success: true,
        });

        // THEN two events: tasks + agents
        let events = result.expect("TaskCompleted should produce events");
        assert_eq!(
            events.len(),
            2,
            "TaskCompleted should yield tasks + agents events"
        );
    }

    /// Unknown system events return None (not relevant to dashboard).
    #[test]
    fn test_sse_unknown_event_returns_none() {
        // GIVEN AllReady (irrelevant to dashboard)
        let result = runtime_event_to_sse_events(RuntimeEvent::AllReady);

        // THEN None
        assert!(
            result.is_none(),
            "AllReady should not produce dashboard events"
        );
    }

    /// LLM model ready maps to "llm" section.
    #[test]
    fn test_sse_llm_model_ready_maps_to_llm_event() {
        // GIVEN LlmModelReady event
        let result = runtime_event_to_sse_events(RuntimeEvent::LlmModelReady {
            backend: "anthropic".into(),
            model_id: "claude-3".into(),
        });

        // THEN one event produced
        let events = result.expect("LlmModelReady should produce events");
        assert_eq!(events.len(), 1, "expected 1 event for LlmModelReady");
    }

    /// Tool circuit broken maps to "tools" section.
    #[test]
    fn test_sse_tool_circuit_broken_maps_to_tools_event() {
        // GIVEN ToolCircuitBroken event
        let result = runtime_event_to_sse_events(RuntimeEvent::ToolCircuitBroken {
            tool_name: "bash_executor".into(),
        });

        // THEN one event produced
        let events = result.expect("ToolCircuitBroken should produce events");
        assert_eq!(events.len(), 1, "expected 1 event for ToolCircuitBroken");
    }

    // ─── STORY-077 tests ────────────────────────────────────────────────────

    /// AC-6 — DASHBOARD_HTML is non-empty and contains the expected title.
    #[test]
    fn test_ac6_dashboard_html_embedded_compiles() {
        // GIVEN the include_str! evaluated at compile time
        // THEN the constant is non-empty and contains the page title
        assert!(!DASHBOARD_HTML.is_empty());
        assert!(
            DASHBOARD_HTML.contains("Apollia OS"),
            "dashboard.html must contain 'Apollia OS'"
        );
    }

    /// AC-6 — HTMX_JS is non-empty and contains the HTMX library marker.
    #[test]
    fn test_ac6_htmx_js_embedded() {
        // GIVEN the include_str! evaluated at compile time
        // THEN the constant is non-empty and contains recognisable HTMX code
        assert!(!HTMX_JS.is_empty());
        assert!(HTMX_JS.contains("htmx"), "htmx.min.js must contain 'htmx'");
    }

    // ─── STORY-090 tests — plan events routed to "tasks" channel (AC-4) ────

    /// AC-4 — PlanGenerated routes to the "tasks" dashboard channel.
    #[test]
    fn test_ac4_plan_generated_route_vers_tasks() {
        // GIVEN a PlanGenerated event
        let event = RuntimeEvent::PlanGenerated {
            task_id: "t-001".into(),
            agent_name: "agent".into(),
            plan_id: "p-001".into(),
            step_count: 3,
        };

        // WHEN mapped through dashboard_event_to_sse
        let sse = dashboard_event_to_sse(&event);

        // THEN routed to "tasks" channel
        assert!(
            sse.is_some(),
            "PlanGenerated must produce a dashboard event"
        );
        assert_eq!(sse.unwrap().channel, "tasks");
    }

    /// AC-4 — StepStarted routes to the "tasks" dashboard channel.
    #[test]
    fn test_ac4_step_started_route_vers_tasks() {
        // GIVEN a StepStarted event
        let event = RuntimeEvent::StepStarted {
            task_id: "t-001".into(),
            plan_id: "p-001".into(),
            step_id: "s1".into(),
            step_num: 1,
            total: 4,
            desc: "Step".into(),
        };

        // WHEN mapped through dashboard_event_to_sse
        let sse = dashboard_event_to_sse(&event);

        // THEN routed to "tasks" channel (not "agents" or "triggers")
        assert_eq!(sse.unwrap().channel, "tasks");
    }

    /// AC-4 — PlanReplanning routes to the "tasks" dashboard channel.
    #[test]
    fn test_ac4_replanning_route_vers_tasks() {
        // GIVEN a PlanReplanning event
        let event = RuntimeEvent::PlanReplanning {
            task_id: "t-001".into(),
            plan_id: "p-001".into(),
            attempt: 1,
            failed_step: "s2".into(),
            reason: "timeout".into(),
        };

        // WHEN mapped through dashboard_event_to_sse
        let sse = dashboard_event_to_sse(&event);

        // THEN routed to "tasks" channel
        assert_eq!(sse.unwrap().channel, "tasks");
    }

    /// AC-4 — All 6 sections declare hx-trigger="load, sse:<section>".
    #[test]
    fn test_ac4_dashboard_html_has_hx_trigger_load() {
        // GIVEN the embedded dashboard HTML
        // THEN every section carries hx-trigger="load" so initial state loads
        assert!(
            DASHBOARD_HTML.contains("hx-trigger=\"load"),
            "dashboard.html must have at least one hx-trigger containing 'load'"
        );
        // AND all 6 SSE event names are present
        assert!(DASHBOARD_HTML.contains("sse:agents"), "missing sse:agents");
        assert!(
            DASHBOARD_HTML.contains("sse:triggers"),
            "missing sse:triggers"
        );
        assert!(DASHBOARD_HTML.contains("sse:tasks"), "missing sse:tasks");
        assert!(DASHBOARD_HTML.contains("sse:llm"), "missing sse:llm");
        assert!(DASHBOARD_HTML.contains("sse:tools"), "missing sse:tools");
        assert!(DASHBOARD_HTML.contains("sse:memory"), "missing sse:memory");
    }

    /// AC-1 — GET /static/htmx.min.js returns HTTP 200 with Content-Type application/javascript.
    #[tokio::test]
    async fn test_ac1_htmx_route_returns_javascript() {
        // GIVEN the get_htmx_js handler
        let response = get_htmx_js().await.into_response();

        // THEN HTTP 200
        assert_eq!(response.status(), axum::http::StatusCode::OK);

        // AND Content-Type contains "javascript"
        let ct = response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        assert!(
            ct.contains("javascript"),
            "expected Content-Type to contain 'javascript', got: {ct}"
        );
    }
}
