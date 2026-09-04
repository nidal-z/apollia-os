//! The API operations no CLI leaf reaches, exercised through the real router.
//!
//! A sweep of the CLI traverses the API operations the CLI addresses, and only
//! those. Eleven operations of the surface sit outside that path: nothing in
//! `apollia-cli` builds their URL, so no CLI sweep, however exhaustive, can
//! observe them. This module is the instrument that covers them, and it drives
//! [`APIServer::build_router_for_test`] rather than a hand-rolled router, so a
//! path renamed or a method rebound in `build_router` fails here instead of
//! passing against a private copy of the route table.
//!
//! Two probes per operation, and they answer different questions.
//!
//! The census sends a method the route does not declare and expects `405`,
//! which axum returns only when the path matched. That proves the path is
//! mounted, and it costs nothing, so every one of the eleven takes it.
//!
//! The live probes send the declared method and assert what the handler
//! answers. Ten of the eleven take one. The eleventh, `get_task_timeline`,
//! is measured and reported instead of run: `routes_timeline::resolve_data_dir`
//! ignores `state.data_dir` and resolves the process home, so invoking it here
//! would read the developer's own `~/.apollia`. See [`LIVE_PROBE_EXCLUSION`].

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use apollia_core::{AIPResult, AIPTask, RuntimeEvent, TaskStatus};
use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use axum::Router;
use futures::StreamExt;
use tower::ServiceExt;

use crate::api::server::{APIServer, AppState};
use crate::coordinator::{DynBackend, ExecutionBackend};
use crate::eventbus::EventBus;
use crate::registry::AgentRegistry;
use crate::router::TaskRouterHandle;

/// One operation of the API that no CLI leaf addresses.
struct UnreachedOp {
    /// OpenAPI operation id, the name the coverage table counts.
    id: &'static str,
    /// Method `build_router` binds the path to.
    method: Method,
    /// Concrete path, path parameters already filled with a probe value.
    path: &'static str,
    /// Method the route does not declare, used by the census probe.
    absent_method: Method,
    /// `true` when a live probe elsewhere in this module invokes the handler.
    live_probed: bool,
}

/// Why `get_task_timeline` carries no live probe.
const LIVE_PROBE_EXCLUSION: &str = "get_task_timeline resolves the process home \
instead of state.data_dir, so a live probe would read the real ~/.apollia";

/// The eleven operations, in the order the coverage table lists them.
fn unreached_ops() -> Vec<UnreachedOp> {
    vec![
        UnreachedOp {
            id: "clear_plan_cache",
            method: Method::POST,
            path: "/api/v1/plan-cache/clear",
            absent_method: Method::GET,
            live_probed: true,
        },
        UnreachedOp {
            id: "get_plan_cache_stats",
            method: Method::GET,
            path: "/api/v1/plan-cache/stats",
            absent_method: Method::POST,
            live_probed: true,
        },
        UnreachedOp {
            id: "list_resources",
            method: Method::GET,
            path: "/api/v1/mcp/resources",
            absent_method: Method::DELETE,
            live_probed: true,
        },
        UnreachedOp {
            id: "list_a2a_agents",
            method: Method::GET,
            path: "/api/v1/a2a/agents",
            absent_method: Method::DELETE,
            live_probed: true,
        },
        UnreachedOp {
            id: "list_tools",
            method: Method::GET,
            path: "/api/v1/tools",
            absent_method: Method::DELETE,
            live_probed: true,
        },
        UnreachedOp {
            id: "set_server_approval",
            method: Method::PATCH,
            path: "/api/v1/mcp/servers/probe/approval",
            absent_method: Method::GET,
            live_probed: false,
        },
        UnreachedOp {
            id: "stream_mailbox",
            method: Method::GET,
            path: "/api/v1/mailbox/stream",
            absent_method: Method::POST,
            live_probed: true,
        },
        UnreachedOp {
            id: "delegate",
            method: Method::POST,
            path: "/api/v1/a2a/delegate",
            absent_method: Method::GET,
            live_probed: true,
        },
        UnreachedOp {
            id: "handle_webhook",
            method: Method::POST,
            path: "/webhooks/probe-trigger",
            absent_method: Method::GET,
            live_probed: true,
        },
        UnreachedOp {
            id: "reload_stt_engine",
            method: Method::POST,
            path: "/api/v1/stt/reload",
            absent_method: Method::GET,
            live_probed: true,
        },
        UnreachedOp {
            id: "get_llm_daily_costs",
            method: Method::GET,
            path: "/api/v1/llm/costs/daily",
            absent_method: Method::DELETE,
            live_probed: true,
        },
        UnreachedOp {
            id: "get_session_todo",
            method: Method::GET,
            path: "/api/v1/sessions/probe-session/todo",
            absent_method: Method::DELETE,
            live_probed: true,
        },
        UnreachedOp {
            id: "get_task_sidechains",
            method: Method::GET,
            path: "/api/v1/tasks/probe-task/sidechains",
            absent_method: Method::POST,
            live_probed: true,
        },
        UnreachedOp {
            id: "get_task_timeline",
            method: Method::GET,
            path: "/api/v1/tasks/probe-task/timeline",
            absent_method: Method::POST,
            live_probed: false,
        },
    ]
}

// ── Test fixtures ────────────────────────────────────────────────────────

/// Backend that completes every task without running anything.
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

/// An `AppState` with every optional dependency absent, rooted at `data_dir`.
///
/// `data_dir` is always a throwaway directory owned by the test: no path in
/// this module points at the real profile.
fn base_state(data_dir: &std::path::Path) -> AppState<MockBackend> {
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
        data_dir: data_dir.to_path_buf(),
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
    }
}

/// Send one request through the router and return status plus body bytes.
async fn call(router: Router, method: Method, path: &str, body: Body) -> (StatusCode, Vec<u8>) {
    let request = Request::builder()
        .method(method)
        .uri(path)
        .header("content-type", "application/json")
        .body(body)
        .expect("build request");
    let response = router.oneshot(request).await.expect("router response");
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("read body");
    (status, bytes.to_vec())
}

/// Send one request and return only its status, leaving the body unread.
///
/// The mailbox stream never ends, so a probe that collects bodies cannot be
/// pointed at it. This one can.
async fn status_of(router: Router, method: Method, path: &str, body: Body) -> StatusCode {
    let request = Request::builder()
        .method(method)
        .uri(path)
        .header("content-type", "application/json")
        .body(body)
        .expect("build request");
    router
        .oneshot(request)
        .await
        .expect("router response")
        .status()
}

/// Parse a JSON body, failing the test with the raw bytes when it is not JSON.
fn json_of(bytes: &[u8]) -> serde_json::Value {
    serde_json::from_slice(bytes).unwrap_or_else(|e| {
        panic!(
            "body is not JSON ({e}): {}",
            String::from_utf8_lossy(bytes).escape_debug()
        )
    })
}

// ── Census, the route table serves all eleven ────────────────────────────

#[tokio::test]
async fn test_every_operation_unreached_by_the_cli_is_mounted_on_the_router() {
    // GIVEN the eleven operations no CLI leaf addresses, and the router the
    // daemon actually serves
    let dir = tempfile::tempdir().expect("temp dir");
    let ops = unreached_ops();
    assert_eq!(
        ops.len(),
        14,
        "the census must carry the fourteen operations the coverage table lists"
    );

    // WHEN each is probed with a method its route does not declare
    let mut probed = Vec::new();
    for op in &ops {
        let router = APIServer::build_router_for_test(base_state(dir.path()));
        let status = status_of(router, op.absent_method.clone(), op.path, Body::empty()).await;
        probed.push((op.id, status));
    }

    // THEN axum answers 405 for every one of them, which it returns only when
    // the path matched: an unmounted path answers 404 instead
    let unmounted: Vec<&(&str, StatusCode)> = probed
        .iter()
        .filter(|(_, status)| *status != StatusCode::METHOD_NOT_ALLOWED)
        .collect();
    assert!(
        unmounted.is_empty(),
        "operations not mounted on the router: {unmounted:?}"
    );
    assert_eq!(
        probed.len(),
        ops.len(),
        "every operation must be probed, none skipped"
    );

    // AND the method each one declares is the one its route is bound to: sent
    // that way, the request reaches a handler instead of the 405 above
    let mut bound = 0_usize;
    for op in ops.iter().filter(|op| op.live_probed) {
        let router = APIServer::build_router_for_test(base_state(dir.path()));
        let body = if op.method == Method::POST {
            Body::from("{}")
        } else {
            Body::empty()
        };
        let status = status_of(router, op.method.clone(), op.path, body).await;
        assert_ne!(
            status,
            StatusCode::METHOD_NOT_ALLOWED,
            "{} is not bound to {}",
            op.id,
            op.method
        );
        assert_ne!(status, StatusCode::NOT_FOUND, "{} is unmounted", op.id);
        bound += 1;
    }
    assert_eq!(
        bound, 12,
        "the method binding of twelve operations must be probed"
    );
}

#[tokio::test]
async fn test_twelve_of_the_fourteen_operations_carry_a_live_probe() {
    // GIVEN the census table, which records which operations a live probe
    // invokes the handler of
    let ops = unreached_ops();

    // WHEN the live probes are counted
    let live = ops.iter().filter(|op| op.live_probed).count();
    let excluded: Vec<&str> = ops
        .iter()
        .filter(|op| !op.live_probed)
        .map(|op| op.id)
        .collect();

    // THEN ten operations are exercised handler-deep, and the single exclusion
    // is the one whose handler reads the process home rather than the state's
    // data directory
    assert_eq!(
        live, 12,
        "live probe count changed without the table saying so"
    );
    // Two exclusions, and they are not the same kind. `get_task_timeline`
    // cannot be live-probed here at all, for the reason the constant states.
    // `set_server_approval` mutates a server the census does not create, so it
    // carries a routing probe only until a probe builds that server: debt, and
    // named as such rather than counted as coverage.
    let mut excluded = excluded;
    excluded.sort_unstable();
    assert_eq!(excluded, vec!["get_task_timeline", "set_server_approval"]);
    assert!(LIVE_PROBE_EXCLUSION.contains("state.data_dir"));
}

// ── Live probes ──────────────────────────────────────────────────────────

#[tokio::test]
async fn test_the_two_listing_routes_the_cli_never_calls_answer_from_an_empty_state() {
    // GIVEN the router the daemon serves, over a state with no agent registered
    // and the mock tool backend
    let dir = tempfile::tempdir().expect("temp dir");
    let router = APIServer::build_router_for_test(base_state(dir.path()));

    // WHEN the two collection routes are read, the ones the CLI addresses only
    // in their single-item form
    let (agents_status, agents_body) = call(
        router.clone(),
        Method::GET,
        "/api/v1/a2a/agents",
        Body::empty(),
    )
    .await;
    let (tools_status, tools_body) =
        call(router, Method::GET, "/api/v1/tools", Body::empty()).await;

    // THEN each handler runs and answers its own contract, which a routing probe
    // cannot tell apart: the agent listing serves an empty collection, and the
    // tool listing refuses with 503 because this state carries no registry.
    // Pinning the refusal is the point: a handler that panicked on the missing
    // registry would answer 500 and the path would still have matched.
    let agents_text = String::from_utf8_lossy(&agents_body).to_string();
    let tools_text = String::from_utf8_lossy(&tools_body).to_string();
    assert_eq!(agents_status, StatusCode::OK, "body was: {agents_text}");
    assert_eq!(
        tools_status,
        StatusCode::SERVICE_UNAVAILABLE,
        "body was: {tools_text}"
    );
    serde_json::from_str::<serde_json::Value>(&agents_text).expect("agents body is json");
    let refusal: serde_json::Value = serde_json::from_str(&tools_text).expect("tools body is json");
    assert!(
        refusal["error"]
            .as_str()
            .is_some_and(|m| m.contains("tool registry")),
        "the refusal names what is missing: {tools_text}"
    );
}

#[tokio::test]
async fn test_plan_cache_routes_report_the_repository_they_are_given() {
    // GIVEN a plan cache holding one stored plan
    let dir = tempfile::tempdir().expect("temp dir");
    let db = dir
        .path()
        .join(apollia_core::paths::DataFile::PlanCache.file_name());
    let repo = apollia_oria::plan_cache::PlanCacheRepository::open(&db).expect("open plan cache");
    repo.store(
        "probe-key",
        &apollia_oria::plan::ExecutionPlan {
            plan_id: "plan-probe".to_string(),
            task_id: "task-probe".to_string(),
            steps: Vec::new(),
        },
        "probe-agent",
        "0.1.0",
    )
    .expect("store plan");
    let mut state = base_state(dir.path());
    state.plan_cache = Some(Arc::new(std::sync::Mutex::new(repo)));
    let router = APIServer::build_router_for_test(state);

    // WHEN the statistics are read, then the cache is cleared
    let (stats_status, stats_body) = call(
        router.clone(),
        Method::GET,
        "/api/v1/plan-cache/stats",
        Body::empty(),
    )
    .await;
    let (clear_status, clear_body) = call(
        router.clone(),
        Method::POST,
        "/api/v1/plan-cache/clear",
        Body::empty(),
    )
    .await;
    let (after_status, after_body) = call(
        router,
        Method::GET,
        "/api/v1/plan-cache/stats",
        Body::empty(),
    )
    .await;

    // THEN both routes report the real content of that repository, and the
    // second read observes the clear the first one performed
    assert_eq!(stats_status, StatusCode::OK);
    assert_eq!(json_of(&stats_body)["total_entries"], 1);
    assert_eq!(clear_status, StatusCode::OK);
    assert_eq!(json_of(&clear_body)["cleared_count"], 1);
    assert_eq!(after_status, StatusCode::OK);
    assert_eq!(json_of(&after_body)["total_entries"], 0);
}

#[tokio::test]
async fn test_plan_cache_routes_answer_503_without_a_repository() {
    // GIVEN a runtime started without a plan cache
    let dir = tempfile::tempdir().expect("temp dir");
    let router = APIServer::build_router_for_test(base_state(dir.path()));

    // WHEN both plan-cache routes are called
    let (stats_status, _) = call(
        router.clone(),
        Method::GET,
        "/api/v1/plan-cache/stats",
        Body::empty(),
    )
    .await;
    let (clear_status, _) = call(
        router,
        Method::POST,
        "/api/v1/plan-cache/clear",
        Body::empty(),
    )
    .await;

    // THEN both name the missing dependency instead of reporting an empty cache
    assert_eq!(stats_status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(clear_status, StatusCode::SERVICE_UNAVAILABLE);
}

#[tokio::test]
async fn test_mcp_resources_degrades_to_an_empty_array_without_a_manager() {
    // GIVEN a runtime with no MCP manager connected
    let dir = tempfile::tempdir().expect("temp dir");
    let router = APIServer::build_router_for_test(base_state(dir.path()));

    // WHEN the aggregated resource list is read
    let (status, body) = call(router, Method::GET, "/api/v1/mcp/resources", Body::empty()).await;

    // THEN the picker gets an empty list rather than an error, which is the
    // documented degrade contract for this route
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json_of(&body), serde_json::json!([]));
}

#[tokio::test]
async fn test_mailbox_stream_relays_mailbox_events_and_filters_the_rest() {
    // GIVEN an open mailbox stream, and a runtime event that is not mailbox
    // traffic published before the one that is
    let dir = tempfile::tempdir().expect("temp dir");
    let state = base_state(dir.path());
    let bus = state.event_sender.clone();
    let router = APIServer::build_router_for_test(state);
    let request = Request::builder()
        .method(Method::GET)
        .uri("/api/v1/mailbox/stream")
        .body(Body::empty())
        .expect("build request");
    let response = router.oneshot(request).await.expect("router response");
    assert_eq!(response.status(), StatusCode::OK);

    // WHEN a non-mailbox event and then a mailbox send are published
    let _ = bus.send(RuntimeEvent::ShutdownRequested);
    let _ = bus.send(RuntimeEvent::AgentMessageSent {
        from: "agent-a".to_string(),
        to: "agent-b".to_string(),
        message_id: "msg-probe".to_string(),
        run_id: None,
        payload_hash: "0".repeat(64),
        full_payload: None,
    });

    // THEN the first frame the stream emits is the mailbox one: the shutdown
    // event never reaches the wire
    let mut stream = response.into_body().into_data_stream();
    let frame = tokio::time::timeout(std::time::Duration::from_secs(5), stream.next())
        .await
        .expect("stream produced no frame within the deadline")
        .expect("stream ended before emitting a frame")
        .expect("frame error");
    let text = String::from_utf8_lossy(&frame).to_string();
    assert!(text.contains("\"event\":\"sent\""), "frame was: {text}");
    assert!(text.contains("msg-probe"), "frame was: {text}");
    assert!(
        !text.contains("ShutdownRequested"),
        "a non-mailbox event reached the mailbox stream: {text}"
    );
}

#[tokio::test]
async fn test_delegate_reports_an_unknown_skill_rather_than_waiting_for_a_worker() {
    // GIVEN a runtime whose registry holds no agent
    let dir = tempfile::tempdir().expect("temp dir");
    let router = APIServer::build_router_for_test(base_state(dir.path()));

    // WHEN a delegation is requested for a skill nobody exposes
    let (status, body) = call(
        router,
        Method::POST,
        "/api/v1/a2a/delegate",
        Body::from(r#"{"skill_id":"no-such-skill","input":{}}"#),
    )
    .await;

    // THEN the caller is told the skill is unknown, before any task is
    // submitted and without consuming the delegation timeout
    assert_eq!(status, StatusCode::NOT_FOUND);
    let json = json_of(&body);
    assert_eq!(json["skill_id"], "no-such-skill");
    assert!(json["error"].is_string());
}

#[tokio::test]
async fn test_webhook_route_answers_503_when_no_trigger_engine_runs() {
    // GIVEN a runtime started without a trigger engine
    let dir = tempfile::tempdir().expect("temp dir");
    let router = APIServer::build_router_for_test(base_state(dir.path()));

    // WHEN a signed webhook payload arrives
    let (status, _) = call(
        router,
        Method::POST,
        "/webhooks/probe-trigger",
        Body::from("{}"),
    )
    .await;

    // THEN the sender is told the receiver is unavailable, and no signature
    // check is attempted on a payload nothing could consume
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
}

#[tokio::test]
async fn test_stt_reload_reports_no_engine_when_the_persisted_config_is_disabled() {
    // GIVEN a system.db whose stt_config row holds the default, disabled state
    let dir = tempfile::tempdir().expect("temp dir");
    let db = dir
        .path()
        .join(apollia_core::paths::DataFile::System.file_name());
    let repo = apollia_core::stt_config::SttConfigRepository::open(&db).expect("open system.db");
    let mut state = base_state(dir.path());
    state.stt_config_repo = Some(Arc::new(std::sync::Mutex::new(repo)));
    let router = APIServer::build_router_for_test(state);

    // WHEN the engine is asked to reload
    let (status, body) = call(router, Method::POST, "/api/v1/stt/reload", Body::empty()).await;

    // THEN the route answers that no engine is loaded, rather than failing:
    // a reload against a disabled configuration is a defined outcome
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json_of(&body)["loaded"], false);
}

#[tokio::test]
async fn test_stt_reload_answers_503_without_a_config_repository() {
    // GIVEN a runtime with no STT configuration repository open
    let dir = tempfile::tempdir().expect("temp dir");
    let router = APIServer::build_router_for_test(base_state(dir.path()));

    // WHEN the engine is asked to reload
    let (status, _) = call(router, Method::POST, "/api/v1/stt/reload", Body::empty()).await;

    // THEN the caller is told the dependency is missing
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
}

#[tokio::test]
async fn test_daily_costs_aggregates_the_calls_the_repository_holds() {
    // GIVEN one LLM call recorded today
    let dir = tempfile::tempdir().expect("temp dir");
    let db = dir
        .path()
        .join(apollia_core::paths::DataFile::LlmCalls.file_name());
    let repo = apollia_llm::repository::LlmCallRepository::open(&db).expect("open llm_calls.db");
    repo.save(&apollia_llm::repository::LlmCallRecord {
        id: "call-probe".to_string(),
        task_id: None,
        step_id: None,
        backend: "probe-backend".to_string(),
        model: "probe-model".to_string(),
        prompt_tokens: Some(10),
        completion_tokens: Some(20),
        cost_usd: Some(0.25),
        latency_ms: Some(5),
        prompt_text: None,
        completion_text: None,
    })
    .expect("save call");
    let mut state = base_state(dir.path());
    state.llm_call_repository = Some(Arc::new(std::sync::Mutex::new(repo)));
    let router = APIServer::build_router_for_test(state);

    // WHEN the daily breakdown is read over the default window
    let (status, body) = call(
        router,
        Method::GET,
        "/api/v1/llm/costs/daily",
        Body::empty(),
    )
    .await;

    // THEN the call appears under its backend, with the cost that was saved
    assert_eq!(status, StatusCode::OK);
    let json = json_of(&body);
    assert_eq!(json["days"], 7);
    let entries = json["entries"].as_array().expect("entries array").clone();
    assert_eq!(entries.len(), 1, "entries were: {entries:?}");
    assert_eq!(entries[0]["backend"], "probe-backend");
    assert_eq!(entries[0]["cost_usd"], 0.25);
}

#[tokio::test]
async fn test_daily_costs_answers_503_without_a_call_repository() {
    // GIVEN a runtime with no LLM call repository open
    let dir = tempfile::tempdir().expect("temp dir");
    let router = APIServer::build_router_for_test(base_state(dir.path()));

    // WHEN the daily breakdown is read
    let (status, _) = call(
        router,
        Method::GET,
        "/api/v1/llm/costs/daily",
        Body::empty(),
    )
    .await;

    // THEN the chart is told the source is absent, not handed an empty series
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
}

#[tokio::test]
async fn test_session_todo_answers_503_without_the_chat_subsystem() {
    // GIVEN a runtime started without the chat session manager
    let dir = tempfile::tempdir().expect("temp dir");
    let router = APIServer::build_router_for_test(base_state(dir.path()));

    // WHEN the todo list of a session is read
    let (status, body) = call(
        router,
        Method::GET,
        "/api/v1/sessions/probe-session/todo",
        Body::empty(),
    )
    .await;

    // THEN the caller is told the subsystem is unavailable, which is distinct
    // from the 404 an unknown session gets
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert!(json_of(&body)["error"].is_string());
}

#[tokio::test]
async fn test_sidechains_answers_503_without_the_a2a_invoker() {
    // GIVEN a runtime whose A2A invoker was never initialized
    let dir = tempfile::tempdir().expect("temp dir");
    let router = APIServer::build_router_for_test(base_state(dir.path()));

    // WHEN the delegations of a task are read
    let (status, body) = call(
        router,
        Method::GET,
        "/api/v1/tasks/probe-task/sidechains",
        Body::empty(),
    )
    .await;

    // THEN the caller is told the invoker is missing, which is distinct from
    // the 404 a task with no delegation gets
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert!(json_of(&body)["error"].is_string());
}
