//! End-to-end tests - degraded behaviour with no LLM backend.
//!
//! Checks that:
//! - the runtime starts correctly with `llm_router = None`;
//! - `AgentDegraded` can be emitted and received on the EventBus;
//! - `LlmRouter::empty()` has no backend available;
//! - an LLM initialisation error does not stop the runtime from starting.
//!
//! No Python dependency - CI friendly.

use apollia_e2e_tests::reserve_port;
use std::collections::HashMap;
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;

use apollia_core::{AIPResult, AIPTask, AgentId, RuntimeEvent};
use apollia_llm::{LlmConfig, LlmRouter, ObservabilityConfig};
use apollia_runtime::{
    api::{
        routes_agents::StubAgentLoader, server::empty_shared_llm_router, APIServer,
        APIServerConfig, AppState,
    },
    coordinator::{DynBackend, ExecutionBackend},
    eventbus::EventBus,
    registry::AgentRegistry,
    router::TaskRouterHandle,
};

// ─────────────────────────────────────────────
// Mock backend - blocks forever
// ─────────────────────────────────────────────

/// Backend that never finishes - used to exercise the runtime start-up.
#[derive(Clone)]
struct NeverMockBackend;

impl From<DynBackend> for NeverMockBackend {
    fn from(_: DynBackend) -> Self {
        NeverMockBackend
    }
}

impl ExecutionBackend for NeverMockBackend {
    fn execute(
        &self,
        _task: AIPTask,
    ) -> Pin<Box<dyn Future<Output = Result<AIPResult, String>> + Send>> {
        Box::pin(async {
            std::future::pending::<()>().await;
            unreachable!()
        })
    }
}

// ─────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────

fn temp_socket_path() -> PathBuf {
    let id = &uuid::Uuid::new_v4().to_string()[..8];
    PathBuf::from(format!("/tmp/ap-llm-deg-{id}.sock"))
}

// ─────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────

/// The runtime starts without error when `llm_router = None`.
#[tokio::test]
async fn test_runtime_starts_without_llm_router() {
    // GIVEN a runtime configured without an LLM
    let (event_sender, _rx) = EventBus::new();
    let registry = AgentRegistry::spawn(event_sender.clone());
    let router: TaskRouterHandle<NeverMockBackend> =
        TaskRouterHandle::spawn(registry.clone(), event_sender.clone(), 256);

    let socket_path = temp_socket_path();
    let reserved_port = reserve_port();
    let port = reserved_port.port();

    let state = AppState {
        router_handle: router,
        registry_handle: registry,
        event_sender,
        agent_loader: Arc::new(StubAgentLoader),
        plan_gates: None,
        audit_journal: None,
        backend: NeverMockBackend,
        llm_router: empty_shared_llm_router(), // no LLM configured
        trigger_engine: None,
        config_path: None,
        task_repository: None,
        pending_approvals: None,
        notification_config: None,
        backend_factory: None,
        tool_registry_handle: None,
        audit_trail: None,
        obs_config: apollia_core::ObservabilityConfig::default(),
        llm_call_repository: None,
        trigger_def_repo: None,
        notification_repo: None,
        notification_engine_handle: None,
        chat_manager: None,
        plan_cache: None,
        mailbox_handle: None,
        user_memory: None,
        stt_engine: apollia_runtime::api::server::empty_shared_stt_engine(),
        stt_repository: apollia_runtime::api::server::empty_shared_stt_repository(),
        data_dir: std::path::PathBuf::new(),
        mcp_handle: None,
        mcp_server_repo: None,
        llm_backend_repo: None,
        stt_config_repo: None,
        a2a_invoker: None,
        resilience_layer: None,
        runner_proxy: None,
        llama_server_supervisor: None,
    };

    // WHEN the APIServer starts
    let api = APIServer::new(
        APIServerConfig {
            socket_path: socket_path.clone(),
            tcp_port: Some(port),
            bind_addr: "127.0.0.1".to_string(),
            api_token: None,
            tls_cert_path: None,
            tls_key_path: None,
        },
        state,
    );
    // Release the probe listener only now, right before the bind it protects.
    reserved_port.release();
    let result = api.start().await;

    // THEN it starts without an error (no panic, no Err)
    assert!(
        result.is_ok(),
        "the APIServer must start without an LLM router"
    );

    let _ = std::fs::remove_file(&socket_path);
}

/// `AgentDegraded` can be emitted and received through the EventBus.
///
/// Exercises the event infrastructure: the runtime can observe and propagate
/// the degradation of an agent with no LLM backend.
#[tokio::test]
async fn test_agent_degraded_event_emitted_when_no_llm() {
    // GIVEN an EventBus with an active subscriber
    let (event_sender, _rx) = EventBus::new();
    let mut event_rx = event_sender.subscribe();
    let agent_id = AgentId::new_v4();

    // WHEN AgentDegraded is emitted (as RuntimeContext would without an LLM)
    let _ = event_sender.send(RuntimeEvent::AgentDegraded {
        agent_id: agent_id.clone(),
        reason: "no LLM backend available".into(),
    });

    // THEN the event is received with the right agent_id and reason
    let event = event_rx
        .try_recv()
        .expect("AgentDegraded must be present on the bus");

    assert!(
        matches!(
            event,
            RuntimeEvent::AgentDegraded {
                agent_id: ref id,
                ref reason,
            } if *id == agent_id && reason.contains("no LLM")
        ),
        "unexpected event: {event:?}"
    );
}

/// `LlmRouter::empty()` has no backend available.
///
/// Matches the behaviour of `ctx.llm = None` when the router is empty.
#[tokio::test]
async fn test_ctx_llm_is_none_when_no_backend() {
    // GIVEN an LlmRouter with no configured backend
    let router = LlmRouter::empty();

    // WHEN the available backends are queried
    let backends = router.list();
    let default_backend = router.get(None);

    // THEN there is no backend and get() returns None
    assert!(
        backends.is_empty(),
        "LlmRouter::empty() must have no backend"
    );
    assert!(
        default_backend.is_none(),
        "get(None) must return None for an empty router"
    );
}

/// the runtime keeps working after an LLM initialisation failure.
///
/// Exercises principle #4 (fail fast): the error is detected at start-up, but
/// the runtime can carry on with `llm_router = None`.
#[tokio::test]
async fn test_runtime_continues_after_llm_init_failure() {
    // GIVEN an LLM config whose default backend does not exist
    let config = LlmConfig {
        default: "backend-inexistant".to_owned(),
        backends: vec![],
        observability: ObservabilityConfig::default(),
        routing: None,
        pricing_overrides: HashMap::new(),
        cost_alert_threshold_usd: None,
        vertex: None,
        runner: apollia_core::config::LlmRunnerConfig::default(),
    };

    // WHEN LlmRouter::from_config() fails (the backend is absent)
    let llm_result = LlmRouter::from_config(&config).await;

    // THEN the error comes back cleanly (no panic)
    assert!(
        llm_result.is_err(),
        "from_config must fail for an unknown default backend"
    );

    // AND the runtime can still start without an LLM
    let (event_sender, _rx) = EventBus::new();
    let registry = AgentRegistry::spawn(event_sender.clone());
    let router: TaskRouterHandle<NeverMockBackend> =
        TaskRouterHandle::spawn(registry.clone(), event_sender.clone(), 256);

    let socket_path = temp_socket_path();
    let reserved_port = reserve_port();
    let port = reserved_port.port();

    let state = AppState {
        router_handle: router,
        registry_handle: registry,
        event_sender,
        agent_loader: Arc::new(StubAgentLoader),
        plan_gates: None,
        audit_journal: None,
        backend: NeverMockBackend,
        llm_router: empty_shared_llm_router(),
        trigger_engine: None,
        config_path: None,
        task_repository: None,
        pending_approvals: None,
        notification_config: None,
        backend_factory: None,
        tool_registry_handle: None,
        audit_trail: None,
        obs_config: apollia_core::ObservabilityConfig::default(),
        llm_call_repository: None,
        trigger_def_repo: None,
        notification_repo: None,
        notification_engine_handle: None,
        chat_manager: None,
        plan_cache: None,
        mailbox_handle: None,
        user_memory: None,
        stt_engine: apollia_runtime::api::server::empty_shared_stt_engine(),
        stt_repository: apollia_runtime::api::server::empty_shared_stt_repository(),
        data_dir: std::path::PathBuf::new(),
        mcp_handle: None,
        mcp_server_repo: None,
        llm_backend_repo: None,
        stt_config_repo: None,
        a2a_invoker: None,
        resilience_layer: None,
        runner_proxy: None,
        llama_server_supervisor: None,
    };

    let api = APIServer::new(
        APIServerConfig {
            socket_path: socket_path.clone(),
            tcp_port: Some(port),
            bind_addr: "127.0.0.1".to_string(),
            api_token: None,
            tls_cert_path: None,
            tls_key_path: None,
        },
        state,
    );
    // Release the probe listener only now, right before the bind it protects.
    reserved_port.release();
    let result = api.start().await;

    assert!(
        result.is_ok(),
        "the runtime must carry on without an LLM after an init failure"
    );

    let _ = std::fs::remove_file(&socket_path);
}
