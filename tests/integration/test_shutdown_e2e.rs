//! Integration tests — graceful shutdown with active tasks.
//!
//! Tests the full shutdown sequence: drain in-progress tasks → stop agents →
//! stop actors in reverse order. Uses a mock backend to avoid Python dependency.

use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::time::Duration;

use apollia_core::{
    AIPInput, AIPResult, AIPTask, AgentManifest, ProcessState, RuntimeEvent, TaskStatus,
};
use apollia_runtime::api::routes_agents::StubAgentLoader;
use apollia_runtime::{
    api::{APIServer, APIServerConfig, AppState},
    coordinator::{DynBackend, ExecutionBackend, ExecutionCoordinator},
    eventbus::EventBus,
    registry::AgentRegistry,
    router::TaskRouterHandle,
    shutdown::{ShutdownConfig, ShutdownController},
};

// --- Mock backends ---

/// Backend that completes tasks after a configurable delay.
#[derive(Clone)]
struct MockBackend {
    delay: Duration,
}

impl MockBackend {
    fn slow(delay: Duration) -> Self {
        Self { delay }
    }
}

impl From<DynBackend> for MockBackend {
    fn from(_: DynBackend) -> Self {
        MockBackend {
            delay: Duration::ZERO,
        }
    }
}

impl ExecutionBackend for MockBackend {
    fn execute(
        &self,
        task: AIPTask,
    ) -> Pin<Box<dyn Future<Output = Result<AIPResult, String>> + Send>> {
        let delay = self.delay;
        Box::pin(async move {
            if !delay.is_zero() {
                tokio::time::sleep(delay).await;
            }
            Ok(AIPResult {
                task_id: task.task_id,
                status: TaskStatus::Completed,
                output: vec![],
                error: None,
                artifacts: vec![],
                input_required_data: None,
            })
        })
    }
}

/// Backend that never completes (blocks forever, for drain timeout tests).
#[derive(Clone)]
struct NeverBackend;

impl From<DynBackend> for NeverBackend {
    fn from(_: DynBackend) -> Self {
        NeverBackend
    }
}

impl ExecutionBackend for NeverBackend {
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

// --- Test helpers ---

fn test_manifest(name: &str) -> AgentManifest {
    AgentManifest {
        name: name.to_string(),
        version: "1.0.0".to_string(),
        description: String::new(),
        tools_required: vec![],
        tools_optional: vec![],
        supports_streaming: false,
        supports_a2a: false,
        memory_namespace: None,
        shared_memory_namespaces: vec![],
        max_concurrent_tasks: 2,
        step_budget: None,
        network_allowlist: None,
        dangerous_tools_allowed: false,
        tags: vec![],
        skills: vec![],
        execution_mode: "auto".to_string(),
        system_prompt: None,
        tools_requiring_approval: vec![],
        llm_backend: None,
        packages: vec![],
        memory_config: None,
    }
}

fn temp_socket_path() -> PathBuf {
    let id = &uuid::Uuid::new_v4().to_string()[..8];
    PathBuf::from(format!("/tmp/ap-e2e-{}.sock", id))
}

async fn free_port() -> u16 {
    let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    l.local_addr().unwrap().port()
}

// Task drains successfully before shutdown completes
#[tokio::test]
async fn test_shutdown_drains_active_tasks() {
    // GIVEN a full runtime with a slow backend (completes in 300ms)
    let (event_sender, _rx) = EventBus::new();
    let registry = AgentRegistry::spawn(event_sender.clone());
    let router: TaskRouterHandle<MockBackend> =
        TaskRouterHandle::spawn(registry.clone(), event_sender.clone(), 256);

    // Register an active agent
    let agent_id = registry
        .register(test_manifest("drain-agent"))
        .await
        .unwrap();
    registry
        .update_state(agent_id.as_str(), ProcessState::Active)
        .await
        .unwrap();

    // Register coordinator with slow backend
    let coordinator = ExecutionCoordinator::new(
        agent_id.clone(),
        2,
        event_sender.clone(),
        MockBackend::slow(Duration::from_millis(300)),
    );
    router
        .register_coordinator(agent_id.clone(), coordinator)
        .await
        .unwrap();

    // Submit a task (will take 300ms to complete)
    let _task_id = router
        .submit(agent_id.as_str(), AIPInput::default())
        .await
        .unwrap();

    // Set up minimal API server for ShutdownController
    let socket_path = temp_socket_path();
    let port = free_port().await;
    let state = AppState {
        router_handle: router.clone(),
        registry_handle: registry.clone(),
        event_sender: event_sender.clone(),
        agent_loader: std::sync::Arc::new(StubAgentLoader),
        backend: MockBackend::slow(Duration::from_millis(300)),
        llm_router: None,
        trigger_engine: None,
        config_path: None,
        task_repository: None,
        pending_approvals: None,
        notification_config: None,
        pipeline_engine: None,
        backend_factory: None,
        tool_registry_handle: None,
        audit_trail: None,
        obs_config: apollia_core::ObservabilityConfig::default(),
        llm_call_repository: None,
        trigger_def_repo: None,
        pipeline_def_repo: None,
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
    };
    let api = APIServer::new(
        APIServerConfig {
            socket_path: socket_path.clone(),
            tcp_port: port,
            bind_addr: "127.0.0.1".to_string(),
            api_token: None,
        },
        state,
    );
    let api_handle = api.start().await.unwrap();
    tokio::time::sleep(Duration::from_millis(20)).await;

    // WHEN ShutdownController drains with 5s timeout
    let controller = ShutdownController::new(
        ShutdownConfig {
            drain_timeout_secs: 5,
        },
        event_sender.clone(),
        api_handle,
        router,
        registry,
        None,
        None,
    );

    let start = tokio::time::Instant::now();
    let result = controller.shutdown().await;
    let elapsed = start.elapsed();

    // THEN drain returns Ok (task completed within timeout)
    assert!(result.is_ok(), "drain should succeed: {result:?}");
    // AND shutdown was not instant (waited for the task)
    assert!(
        elapsed >= Duration::from_millis(200),
        "should have waited for task: {elapsed:?}"
    );
    // AND shutdown completed within reasonable time
    assert!(
        elapsed < Duration::from_secs(4),
        "shutdown too slow: {elapsed:?}"
    );

    let _ = std::fs::remove_file(&socket_path);
}

// Agent transitions to STOPPED after shutdown
#[tokio::test]
async fn test_shutdown_stops_all_agents() {
    // GIVEN an active agent registered in the runtime
    let (event_sender, _rx) = EventBus::new();
    let registry = AgentRegistry::spawn(event_sender.clone());
    let router: TaskRouterHandle<MockBackend> =
        TaskRouterHandle::spawn(registry.clone(), event_sender.clone(), 256);

    let agent_id = registry
        .register(test_manifest("stop-agent"))
        .await
        .unwrap();
    registry
        .update_state(agent_id.as_str(), ProcessState::Active)
        .await
        .unwrap();

    let socket_path = temp_socket_path();
    let port = free_port().await;
    let state = AppState {
        router_handle: router.clone(),
        registry_handle: registry.clone(),
        event_sender: event_sender.clone(),
        agent_loader: std::sync::Arc::new(StubAgentLoader),
        backend: MockBackend::slow(Duration::ZERO),
        llm_router: None,
        trigger_engine: None,
        config_path: None,
        task_repository: None,
        pending_approvals: None,
        notification_config: None,
        pipeline_engine: None,
        backend_factory: None,
        tool_registry_handle: None,
        audit_trail: None,
        obs_config: apollia_core::ObservabilityConfig::default(),
        llm_call_repository: None,
        trigger_def_repo: None,
        pipeline_def_repo: None,
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
    };
    let api = APIServer::new(
        APIServerConfig {
            socket_path: socket_path.clone(),
            tcp_port: port,
            bind_addr: "127.0.0.1".to_string(),
            api_token: None,
        },
        state,
    );
    let api_handle = api.start().await.unwrap();
    tokio::time::sleep(Duration::from_millis(20)).await;

    let mut event_rx = event_sender.subscribe();

    // WHEN shutdown is triggered
    let controller = ShutdownController::new(
        ShutdownConfig {
            drain_timeout_secs: 5,
        },
        event_sender.clone(),
        api_handle,
        router,
        registry,
        None,
        None,
    );
    let _ = controller.shutdown().await;

    // THEN AgentStopped event is emitted for the agent
    let mut found_stopped = false;
    loop {
        match event_rx.try_recv() {
            Ok(RuntimeEvent::AgentStopped(id)) if id == agent_id => {
                found_stopped = true;
            }
            Ok(_) => {}
            Err(_) => break,
        }
    }
    assert!(found_stopped, "AgentStopped event should have been emitted");

    let _ = std::fs::remove_file(&socket_path);
}

// Supervisor stops all actors in reverse order (structural test)
#[tokio::test]
async fn test_shutdown_broadcasts_requested_event() {
    // GIVEN a runtime with an event subscriber
    let (event_sender, _rx) = EventBus::new();
    let registry = AgentRegistry::spawn(event_sender.clone());
    let router: TaskRouterHandle<MockBackend> =
        TaskRouterHandle::spawn(registry.clone(), event_sender.clone(), 256);

    let socket_path = temp_socket_path();
    let port = free_port().await;
    let state = AppState {
        router_handle: router.clone(),
        registry_handle: registry.clone(),
        event_sender: event_sender.clone(),
        agent_loader: std::sync::Arc::new(StubAgentLoader),
        backend: MockBackend::slow(Duration::ZERO),
        llm_router: None,
        trigger_engine: None,
        config_path: None,
        task_repository: None,
        pending_approvals: None,
        notification_config: None,
        pipeline_engine: None,
        backend_factory: None,
        tool_registry_handle: None,
        audit_trail: None,
        obs_config: apollia_core::ObservabilityConfig::default(),
        llm_call_repository: None,
        trigger_def_repo: None,
        pipeline_def_repo: None,
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
    };
    let api = APIServer::new(
        APIServerConfig {
            socket_path: socket_path.clone(),
            tcp_port: port,
            bind_addr: "127.0.0.1".to_string(),
            api_token: None,
        },
        state,
    );
    let api_handle = api.start().await.unwrap();
    tokio::time::sleep(Duration::from_millis(20)).await;

    let mut event_rx = event_sender.subscribe();

    // WHEN ShutdownController::shutdown() is called
    let controller = ShutdownController::new(
        ShutdownConfig {
            drain_timeout_secs: 1,
        },
        event_sender.clone(),
        api_handle,
        router,
        registry,
        None,
        None,
    );
    let _ = controller.shutdown().await;

    // THEN ShutdownRequested is broadcast on the EventBus
    let mut found_shutdown = false;
    loop {
        match event_rx.try_recv() {
            Ok(RuntimeEvent::ShutdownRequested) => found_shutdown = true,
            Ok(_) => {}
            Err(_) => break,
        }
    }
    assert!(found_shutdown, "ShutdownRequested should be broadcast");

    let _ = std::fs::remove_file(&socket_path);
}

// Drain timeout results in DrainTimeout error, but shutdown still completes
#[tokio::test]
async fn test_shutdown_drain_timeout_force_cancels() {
    use apollia_runtime::shutdown::ShutdownError;

    // GIVEN a never-completing backend
    let (event_sender, _rx) = EventBus::new();
    let registry = AgentRegistry::spawn(event_sender.clone());
    let router: TaskRouterHandle<NeverBackend> =
        TaskRouterHandle::spawn(registry.clone(), event_sender.clone(), 256);

    let agent_id = registry
        .register(test_manifest("never-agent"))
        .await
        .unwrap();
    registry
        .update_state(agent_id.as_str(), ProcessState::Active)
        .await
        .unwrap();

    let coordinator =
        ExecutionCoordinator::new(agent_id.clone(), 2, event_sender.clone(), NeverBackend);
    router
        .register_coordinator(agent_id.clone(), coordinator)
        .await
        .unwrap();

    // Submit a task that will never finish
    let _task_id = router
        .submit(agent_id.as_str(), AIPInput::default())
        .await
        .unwrap();

    let socket_path = temp_socket_path();
    let port = free_port().await;
    let state = AppState {
        router_handle: router.clone(),
        registry_handle: registry.clone(),
        event_sender: event_sender.clone(),
        agent_loader: std::sync::Arc::new(StubAgentLoader),
        backend: NeverBackend,
        llm_router: None,
        trigger_engine: None,
        config_path: None,
        task_repository: None,
        pending_approvals: None,
        notification_config: None,
        pipeline_engine: None,
        backend_factory: None,
        tool_registry_handle: None,
        audit_trail: None,
        obs_config: apollia_core::ObservabilityConfig::default(),
        llm_call_repository: None,
        trigger_def_repo: None,
        pipeline_def_repo: None,
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
    };
    let api = APIServer::new(
        APIServerConfig {
            socket_path: socket_path.clone(),
            tcp_port: port,
            bind_addr: "127.0.0.1".to_string(),
            api_token: None,
        },
        state,
    );
    let api_handle = api.start().await.unwrap();
    tokio::time::sleep(Duration::from_millis(20)).await;

    // WHEN drain timeout is 1s (short for tests)
    let controller = ShutdownController::new(
        ShutdownConfig {
            drain_timeout_secs: 1,
        },
        event_sender,
        api_handle,
        router,
        registry,
        None,
        None,
    );

    let result = controller.shutdown().await;

    // THEN DrainTimeout is returned (task still running after timeout)
    assert!(
        matches!(
            result,
            Err(ShutdownError::DrainTimeout {
                count: 1,
                timeout_secs: 1
            })
        ),
        "expected DrainTimeout with count=1, got: {result:?}"
    );

    let _ = std::fs::remove_file(&socket_path);
}
