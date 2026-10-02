//! The API operations no CLI leaf reaches, exercised through the real router.
//!
//! A sweep of the CLI traverses the API operations the CLI addresses, and only
//! those. The operations of [`unreached_ops`] sit outside that path: nothing in
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
//! mounted, and it costs nothing, so every operation takes it.
//!
//! The live probes send the declared method and assert what the handler
//! answers. All but three take one. `get_task_timeline` is measured and
//! reported instead of run: `routes_timeline::resolve_data_dir` ignores
//! `state.data_dir` and resolves the process home, so invoking it here would
//! read the developer's own `~/.apollia` ([`LIVE_PROBE_EXCLUSION`]).
//! `list_connector_accounts` reads the OS keychain
//! ([`KEYCHAIN_PROBE_EXCLUSION`]). `set_server_approval` is named as debt in
//! the test that counts them.

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
        UnreachedOp {
            id: "list_registry_servers",
            method: Method::GET,
            path: "/api/v1/mcp/registry",
            absent_method: Method::DELETE,
            live_probed: true,
        },
        UnreachedOp {
            id: "get_registry_server",
            method: Method::GET,
            path: "/api/v1/mcp/registry/server?name=io.example.probe/server",
            absent_method: Method::DELETE,
            live_probed: true,
        },
        // The secret paths carry a key no placeholder can hold, so a probe that
        // reaches the handler is refused before any secret store is opened.
        UnreachedOp {
            id: "put_mcp_secret",
            method: Method::PUT,
            path: "/api/v1/mcp/servers/probe/secrets/not-a-variable",
            absent_method: Method::POST,
            live_probed: true,
        },
        UnreachedOp {
            id: "delete_mcp_secret",
            method: Method::DELETE,
            path: "/api/v1/mcp/servers/probe/secrets/not-a-variable",
            absent_method: Method::POST,
            live_probed: true,
        },
        UnreachedOp {
            id: "start_connector_consent",
            method: Method::POST,
            path: "/api/v1/connectors/oauth/start",
            absent_method: Method::GET,
            live_probed: true,
        },
        UnreachedOp {
            id: "complete_connector_consent",
            method: Method::POST,
            path: "/api/v1/connectors/oauth/complete",
            absent_method: Method::GET,
            live_probed: true,
        },
        UnreachedOp {
            id: "list_connector_accounts",
            method: Method::GET,
            path: "/api/v1/connectors/accounts",
            absent_method: Method::POST,
            live_probed: false,
        },
        UnreachedOp {
            id: "disconnect_connector_account",
            method: Method::DELETE,
            path: "/api/v1/connectors/accounts/not-a-provider/probe",
            absent_method: Method::GET,
            live_probed: true,
        },
        UnreachedOp {
            id: "list_loaded_models",
            method: Method::GET,
            path: "/api/v1/llm/loaded-models",
            absent_method: Method::POST,
            live_probed: true,
        },
        UnreachedOp {
            id: "unload_loaded_model",
            method: Method::POST,
            path: "/api/v1/llm/loaded-models/unload",
            absent_method: Method::GET,
            live_probed: true,
        },
        UnreachedOp {
            id: "stream_events",
            method: Method::GET,
            path: "/api/v1/events",
            absent_method: Method::POST,
            live_probed: true,
        },
        UnreachedOp {
            id: "pause_session",
            method: Method::POST,
            path: "/api/v1/sessions/probe-session/pause",
            absent_method: Method::GET,
            live_probed: true,
        },
    ]
}

/// Why `list_connector_accounts` carries no live probe.
const KEYCHAIN_PROBE_EXCLUSION: &str = "list_connector_accounts reads every provider's \
account index from the OS keychain, so a live probe would read the real one";

/// Seed `data_dir` with a fresh registry cache holding one server, so the
/// registry routes answer from it and never reach the network.
fn seed_registry_cache(data_dir: &std::path::Path) {
    let cache = serde_json::json!([{
        "server": {
            "name": "io.example.probe/server",
            "title": "Probe",
            "description": "A server the census seeds",
            "version": "1.0.0",
            "repository": null,
            "websiteUrl": null,
            "packages": null,
            "icons": null
        },
        "_meta": null
    }]);
    std::fs::write(data_dir.join("mcp-registry.json"), cache.to_string())
        .expect("seed registry cache");
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
        secret_store: crate::inference_servers::default_store_opener(),
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
    // GIVEN the operations no CLI leaf addresses, and the router the daemon
    // actually serves, over a data directory whose registry cache is fresh
    let dir = tempfile::tempdir().expect("temp dir");
    seed_registry_cache(dir.path());
    let ops = unreached_ops();
    assert_eq!(
        ops.len(),
        26,
        "the census must carry the twenty-six operations the coverage table lists"
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
        bound, 23,
        "the method binding of twenty-three operations must be probed"
    );
}

#[tokio::test]
async fn test_twenty_three_of_the_twenty_six_operations_carry_a_live_probe() {
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
        live, 23,
        "live probe count changed without the table saying so"
    );
    // Three exclusions, and they are not the same kind. `get_task_timeline`
    // and `list_connector_accounts` cannot be live-probed here at all, for the
    // reasons their constants state. `set_server_approval` mutates a server the
    // census does not create, so it carries a routing probe only until a probe
    // builds that server: debt, and named as such rather than counted as
    // coverage.
    let mut excluded = excluded;
    excluded.sort_unstable();
    assert_eq!(
        excluded,
        vec![
            "get_task_timeline",
            "list_connector_accounts",
            "set_server_approval"
        ]
    );
    assert!(LIVE_PROBE_EXCLUSION.contains("state.data_dir"));
    assert!(KEYCHAIN_PROBE_EXCLUSION.contains("keychain"));
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
async fn test_event_stream_relays_every_runtime_event_as_the_bus_carries_it() {
    // GIVEN an open event stream
    let dir = tempfile::tempdir().expect("temp dir");
    let state = base_state(dir.path());
    let bus = state.event_sender.clone();
    let router = APIServer::build_router_for_test(state);
    let request = Request::builder()
        .method(Method::GET)
        .uri("/api/v1/events")
        .body(Body::empty())
        .expect("build request");
    let response = router.oneshot(request).await.expect("router response");
    assert_eq!(response.status(), StatusCode::OK);

    // WHEN an event no narrower stream forwards is published
    let _ = bus.send(RuntimeEvent::ShutdownRequested);

    // THEN it reaches the wire as a `runtime` frame holding its serde JSON,
    // which a client deserializes back into the same event
    let mut stream = response.into_body().into_data_stream();
    let frame = tokio::time::timeout(std::time::Duration::from_secs(5), stream.next())
        .await
        .expect("stream produced no frame within the deadline")
        .expect("stream ended before emitting a frame")
        .expect("frame error");
    let text = String::from_utf8_lossy(&frame).to_string();
    assert!(text.contains("event: runtime"), "frame was: {text}");
    let data = text
        .lines()
        .find_map(|line| line.strip_prefix("data: "))
        .expect("the frame carries a data line");
    let event: RuntimeEvent = serde_json::from_str(data).expect("the data is a runtime event");
    assert!(matches!(event, RuntimeEvent::ShutdownRequested));
}

#[tokio::test]
async fn test_pause_answers_503_without_the_chat_subsystem() {
    // GIVEN a runtime started without the chat session manager
    let dir = tempfile::tempdir().expect("temp dir");
    let router = APIServer::build_router_for_test(base_state(dir.path()));

    // WHEN the turn of a session is paused
    let (status, body) = call(
        router,
        Method::POST,
        "/api/v1/sessions/probe-session/pause",
        Body::empty(),
    )
    .await;

    // THEN the caller is told the subsystem is unavailable
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert!(json_of(&body)["error"].is_string());
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

#[tokio::test]
async fn test_registry_routes_answer_from_the_cache_without_the_network() {
    // GIVEN a data directory whose registry cache is fresh and holds one server
    let dir = tempfile::tempdir().expect("temp dir");
    seed_registry_cache(dir.path());
    let router = APIServer::build_router_for_test(base_state(dir.path()));

    // WHEN the catalogue is listed, then that server and an unknown one are read
    let (list_status, list_body) = call(
        router.clone(),
        Method::GET,
        "/api/v1/mcp/registry",
        Body::empty(),
    )
    .await;
    let (one_status, one_body) = call(
        router.clone(),
        Method::GET,
        "/api/v1/mcp/registry/server?name=io.example.probe%2Fserver",
        Body::empty(),
    )
    .await;
    let (missing_status, _) = call(
        router,
        Method::GET,
        "/api/v1/mcp/registry/server?name=io.example.probe%2Fabsent",
        Body::empty(),
    )
    .await;

    // THEN the listing and the detail are the cached entry, and an unknown name is not found
    assert_eq!(list_status, StatusCode::OK);
    let list = json_of(&list_body);
    assert_eq!(list.as_array().map(Vec::len), Some(1));
    assert_eq!(list[0]["server"]["name"], "io.example.probe/server");
    assert_eq!(one_status, StatusCode::OK);
    assert_eq!(json_of(&one_body)["server"]["title"], "Probe");
    assert_eq!(missing_status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_secret_routes_refuse_a_key_no_placeholder_can_carry() {
    // GIVEN the router the daemon serves
    let dir = tempfile::tempdir().expect("temp dir");
    let router = APIServer::build_router_for_test(base_state(dir.path()));

    // WHEN a secret is put and deleted under a key with a dash in it
    let (put_status, put_body) = call(
        router.clone(),
        Method::PUT,
        "/api/v1/mcp/servers/probe/secrets/not-a-variable",
        Body::from(r#"{"value":"v"}"#),
    )
    .await;
    let (delete_status, _) = call(
        router,
        Method::DELETE,
        "/api/v1/mcp/servers/probe/secrets/not-a-variable",
        Body::empty(),
    )
    .await;

    // THEN both handlers refuse it before any secret store is opened, and the
    // refusal never echoes the value
    assert_eq!(put_status, StatusCode::BAD_REQUEST);
    assert_eq!(delete_status, StatusCode::BAD_REQUEST);
    let text = String::from_utf8_lossy(&put_body).to_string();
    assert!(
        text.contains("environment variable name"),
        "body was: {text}"
    );
}

#[tokio::test]
async fn test_connector_consent_routes_refuse_before_reaching_a_provider() {
    // GIVEN the router the daemon serves
    let dir = tempfile::tempdir().expect("temp dir");
    let router = APIServer::build_router_for_test(base_state(dir.path()));

    // WHEN a consent is started for an unknown provider, completed under a
    // state never issued, and an account of an unknown provider disconnected
    let (start_status, start_body) = call(
        router.clone(),
        Method::POST,
        "/api/v1/connectors/oauth/start",
        Body::from(r#"{"provider":"github","redirect_uri":"https://app.example.test/cb"}"#),
    )
    .await;
    let (complete_status, _) = call(
        router.clone(),
        Method::POST,
        "/api/v1/connectors/oauth/complete",
        Body::from(r#"{"state":"never-issued","code":"c"}"#),
    )
    .await;
    let (disconnect_status, _) = call(
        router,
        Method::DELETE,
        "/api/v1/connectors/accounts/not-a-provider/probe",
        Body::empty(),
    )
    .await;

    // THEN each handler answers its refusal, and none opened a browser, a
    // listener, a provider connection or the keychain
    assert_eq!(start_status, StatusCode::BAD_REQUEST);
    assert!(json_of(&start_body)["error"]
        .as_str()
        .is_some_and(|m| m.contains("github")));
    assert_eq!(complete_status, StatusCode::NOT_FOUND);
    assert_eq!(disconnect_status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_loaded_model_routes_tell_a_host_without_a_local_engine_apart() {
    // GIVEN a runtime started without the embedded llama-server
    let dir = tempfile::tempdir().expect("temp dir");

    // WHEN the resident models are listed and one is asked to unload
    let (list_status, list_body) = call(
        APIServer::build_router_for_test(base_state(dir.path())),
        Method::GET,
        "/api/v1/llm/loaded-models",
        Body::empty(),
    )
    .await;
    let (unload_status, unload_body) = call(
        APIServer::build_router_for_test(base_state(dir.path())),
        Method::POST,
        "/api/v1/llm/loaded-models/unload",
        Body::from(r#"{"modelPath":"/models/probe.gguf"}"#),
    )
    .await;

    // THEN the list answers empty and says no engine exists, and the unload is
    // refused as unavailable rather than as a model that is merely not loaded
    assert_eq!(list_status, StatusCode::OK);
    let listed = json_of(&list_body);
    assert_eq!(listed["engineAvailable"], false);
    assert_eq!(listed["models"].as_array().map(Vec::len), Some(0));
    assert_eq!(unload_status, StatusCode::SERVICE_UNAVAILABLE);
    assert!(json_of(&unload_body)["error"].is_string());
}
