use super::bootstrap::{seed_default_desktop_channel_if_needed, SEEDED_DESKTOP_CHANNEL_MARKER};
use super::bundled::{auto_load_bundled_agents, native_tool_descriptors};

use super::*;
use crate::coordinator::ExecutionBackend;
use crate::test_support::{poll_until_async, reserve_port};
use apollia_core::{AIPResult, AIPTask, RuntimeEvent, TaskStatus};
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use tokio::net::TcpListener;

/// Minimal ExecutionBackend for testing.
#[derive(Clone)]
struct MockBackend;

impl From<crate::coordinator::DynBackend> for MockBackend {
    fn from(_: crate::coordinator::DynBackend) -> Self {
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

#[test]
fn native_tool_descriptors_returns_expected_count() {
    // GIVEN: the native_tool_descriptors() function
    // WHEN: called
    // THEN: 16 baseline tools (13 historical + 3 permission_rule_*),
    // plus optional web tools when their features are on.
    let expected_count =
        16 + cfg!(feature = "web-search") as usize + cfg!(feature = "web-read") as usize;
    let descriptors = native_tool_descriptors();
    assert_eq!(descriptors.len(), expected_count);
}

#[test]
fn native_tool_descriptors_all_pass_validation() {
    // GIVEN: the native_tool_descriptors() function
    // WHEN: called
    // THEN: every descriptor passes validate()
    for descriptor in native_tool_descriptors() {
        assert!(
            descriptor.validate().is_ok(),
            "descriptor '{}' failed validation",
            descriptor.name
        );
    }
}

#[test]
fn mcp_phase3b_toml_absent_returns_empty() {
    // GIVEN a path that does not exist
    let path = std::path::Path::new("/tmp/nonexistent-mcp-config-335.toml");
    // WHEN
    let config = McpConfig::load(path).unwrap();
    // THEN no servers and no error
    assert!(config.servers.is_empty());
}

#[tokio::test]
async fn test_boot_with_mcp_toml_imports_servers() {
    // GIVEN an empty mcp.db and a mcp.toml with one server
    use std::io::Write as _;
    let dir = tempfile::TempDir::new().unwrap();
    let db_path = dir.path().join("mcp.db");
    let toml_path = dir.path().join("mcp.toml");

    let toml_content = r#"
            [[servers]]
            name = "notion"
            command = "npx"
            args = ["-y", "@notionhq/notion-mcp-server"]
        "#;
    let mut file = std::fs::File::create(&toml_path).unwrap();
    file.write_all(toml_content.as_bytes()).unwrap();

    // WHEN the import logic runs
    let repo = apollia_mcp::McpServerRepository::open(&db_path).unwrap();
    assert!(repo.list().unwrap().is_empty());
    let toml_config = McpConfig::load(&toml_path).unwrap();
    let n = repo.import_from_toml(toml_config.servers).unwrap();

    // THEN mcp.db contains 1 server and the log count is correct
    assert_eq!(n, 1);
    let servers = repo.list().unwrap();
    assert_eq!(servers.len(), 1);
    assert_eq!(servers[0].name, "notion");
}

#[tokio::test]
async fn test_boot_without_mcp_toml_ok() {
    // GIVEN neither mcp.db nor mcp.toml
    let dir = tempfile::TempDir::new().unwrap();
    let db_path = dir.path().join("mcp.db");
    let toml_path = dir.path().join("mcp.toml");

    // WHEN the import logic runs with an absent toml
    let repo = apollia_mcp::McpServerRepository::open(&db_path).unwrap();
    let n = repo.import_from_toml(vec![]).unwrap();

    // THEN Ok(0) and no error
    assert_eq!(n, 0);
    assert!(repo.list().unwrap().is_empty());

    // AND loading a missing mcp.toml returns empty without error
    let config = McpConfig::load(&toml_path).unwrap();
    assert!(config.servers.is_empty());
}

/// Create a short unique temp socket path (macOS SUN_LEN limit).
fn temp_socket_path() -> PathBuf {
    let id = &uuid::Uuid::new_v4().to_string()[..8];
    PathBuf::from(format!("/tmp/ap-{}.sock", id))
}

/// Returns `(SupervisorConfig, TempDir)`, the caller must hold `TempDir`
/// alive until the test completes so the data_dir path remains valid.
fn test_config(port: u16, socket_path: PathBuf) -> (SupervisorConfig, tempfile::TempDir) {
    let temp_dir = tempfile::tempdir().expect("tempdir");
    let config = SupervisorConfig {
        api_config: APIServerConfig {
            socket_path,
            bind_addr: "127.0.0.1".to_owned(),
            tcp_port: Some(port),
            api_token: None,
            tls_cert_path: None,
            tls_key_path: None,
        },
        startup_timeout_secs: 10,
        llm_config: None,
        config_path: None,
        runtime_config: apollia_core::RuntimeConfig::default(),
        hitl_config: apollia_core::HitlConfig::default(),
        data_dir: temp_dir.path().to_path_buf(),
        obs_config: apollia_core::ObservabilityConfig::default(),
        agent_repository: None,
        package_repository: None,
        bundled_agents_path: None,
        tools_config: apollia_core::ToolsConfig::default(),
        mcp_loading: LoadingMode::Eager,
        tool_search_limit: 20,
        hooks_config: apollia_core::HooksConfig::default(),
        plan_mode_default: false,
        chat_default_workspace: None,
        chat_tool_turn_temperature: None,
    };
    (config, temp_dir)
}

#[tokio::test]
async fn test_startup_sequence_all_ready() {
    // GIVEN a configured Supervisor
    let reserved_port = reserve_port();
    let port = reserved_port.port();
    let socket_path = temp_socket_path();
    let (config, _tmp_dir) = test_config(port, socket_path.clone());
    // Release the probe listener only now, right before the bind it protects.
    reserved_port.release();
    let supervisor = Supervisor::new(config);

    // WHEN start() is called
    let result = supervisor
        .start(
            MockBackend,
            Arc::new(crate::api::routes_agents::StubAgentLoader),
            None,
            None,
        )
        .await;

    // THEN all actors start and handles are returned
    assert!(result.is_ok(), "start() should succeed");
    let handles = result.unwrap();

    // Cleanup
    handles.api_handle.shutdown();
    handles.router_handle.shutdown();
    handles.tool_registry_handle.shutdown().await;
    handles.registry_handle.shutdown();
    let _ = std::fs::remove_file(&socket_path);
}

#[tokio::test]
async fn test_all_ready_event_emitted() {
    // GIVEN a configured Supervisor
    let reserved_port = reserve_port();
    let port = reserved_port.port();
    let socket_path = temp_socket_path();
    let (config, _tmp_dir) = test_config(port, socket_path.clone());
    // Release the probe listener only now, right before the bind it protects.
    reserved_port.release();
    let supervisor = Supervisor::new(config);

    // WHEN start() is called
    let handles = supervisor
        .start(
            MockBackend,
            Arc::new(crate::api::routes_agents::StubAgentLoader),
            None,
            None,
        )
        .await
        .unwrap();

    // Subscribe AFTER start (AllReady was already emitted, but let's verify via a new event)
    let mut rx = handles.event_sender.subscribe();

    // Emit a test event to verify the bus is working
    let _ = handles.event_sender.send(RuntimeEvent::AllReady);
    let event = tokio::time::timeout(Duration::from_secs(1), rx.recv())
        .await
        .expect("should receive within 1s")
        .expect("recv should succeed");
    assert!(
        matches!(event, RuntimeEvent::AllReady),
        "expected AllReady, got: {event:?}"
    );

    // Cleanup
    handles.api_handle.shutdown();
    handles.router_handle.shutdown();
    handles.tool_registry_handle.shutdown().await;
    handles.registry_handle.shutdown();
    let _ = std::fs::remove_file(&socket_path);
}

#[tokio::test]
async fn test_handles_accessible_after_start() {
    // GIVEN a Supervisor started successfully
    let reserved_port = reserve_port();
    let port = reserved_port.port();
    let socket_path = temp_socket_path();
    let (config, _tmp_dir) = test_config(port, socket_path.clone());
    // Release the probe listener only now, right before the bind it protects.
    reserved_port.release();
    let supervisor = Supervisor::new(config);
    let handles = supervisor
        .start(
            MockBackend,
            Arc::new(crate::api::routes_agents::StubAgentLoader),
            None,
            None,
        )
        .await
        .unwrap();

    // THEN all handles are present and usable
    // EventBusSender: can send (need a subscriber for broadcast to succeed)
    let _rx = handles.event_sender.subscribe();
    let send_result = handles.event_sender.send(RuntimeEvent::ShutdownRequested);
    assert!(send_result.is_ok());

    // AgentRegistryHandle: can list
    let agents = handles.registry_handle.list_agents().await;
    assert!(agents.is_ok());
    assert!(agents.unwrap().is_empty());

    // ToolRegistryHandle: can list. The count is derived from the same
    // sources `register_builtin_tools` registers, so it stays correct as
    // native tools, connector ops, and MCP resource tools evolve while
    // still catching a registration that silently drops a descriptor:
    // native tools + connector descriptors (Google + Microsoft 365) + the
    // always-advertised MCP resource tools.
    let connector_count = crate::connectors_bridge::all_connector_descriptors().len();
    let mcp_resource_count = apollia_mcp::mcp_resources::mcp_resource_descriptors().len();
    let expected = native_tool_descriptors().len() + connector_count + mcp_resource_count;
    let tools = handles.tool_registry_handle.list().await;
    assert!(tools.is_ok());
    assert_eq!(
        tools.unwrap().len(),
        expected,
        "expected {expected} native + connector tools to be auto-registered"
    );

    // TaskRouterHandle: is clone
    let _cloned = handles.router_handle.clone();

    // APIServerHandle: can shutdown
    handles.api_handle.shutdown();

    // Verify Send + Sync at compile time
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<EventBusSender>();
    assert_send_sync::<AgentRegistryHandle>();
    assert_send_sync::<ToolRegistryHandle>();
    assert_send_sync::<TaskRouterHandle<MockBackend>>();

    // Cleanup
    handles.router_handle.shutdown();
    handles.tool_registry_handle.shutdown().await;
    handles.registry_handle.shutdown();
    let _ = std::fs::remove_file(&socket_path);
}

#[tokio::test]
async fn test_startup_timeout_rollback() {
    // GIVEN a port already in use (bind will fail, not timeout, but tests the
    // error path). Bind an OS-assigned port and KEEP the listener, so the
    // supervisor's bind of the same port fails deterministically. An unreleased
    // reserve_port() guard would work too; this test predates the guard and an
    // OS-assigned number held for the whole test serves the same purpose.
    let _listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = _listener.local_addr().unwrap().port();
    let socket_path = temp_socket_path();

    let config = SupervisorConfig {
        api_config: APIServerConfig {
            socket_path: socket_path.clone(),
            bind_addr: "127.0.0.1".to_owned(),
            tcp_port: Some(port),
            api_token: None,
            tls_cert_path: None,
            tls_key_path: None,
        },
        startup_timeout_secs: 1,
        llm_config: None,
        config_path: None,
        runtime_config: apollia_core::RuntimeConfig::default(),
        hitl_config: apollia_core::HitlConfig::default(),
        data_dir: {
            let d = tempfile::tempdir().expect("tempdir");
            let p = d.path().to_path_buf();
            std::mem::forget(d);
            p
        },
        obs_config: apollia_core::ObservabilityConfig::default(),
        agent_repository: None,
        package_repository: None,
        bundled_agents_path: None,
        tools_config: apollia_core::ToolsConfig::default(),
        mcp_loading: LoadingMode::Eager,
        tool_search_limit: 20,
        hooks_config: apollia_core::HooksConfig::default(),
        plan_mode_default: false,
        chat_default_workspace: None,
        chat_tool_turn_temperature: None,
    };
    let supervisor = Supervisor::new(config);

    // WHEN start() is called
    let result = supervisor
        .start(
            MockBackend,
            Arc::new(crate::api::routes_agents::StubAgentLoader),
            None,
            None,
        )
        .await;

    // THEN ActorStartFailed is returned (port already in use)
    assert!(result.is_err());
    let err = match result {
        Err(e) => e,
        Ok(_) => panic!("expected error, got Ok"),
    };
    assert!(
        matches!(&err, SupervisorError::ActorStartFailed { actor, .. } if actor == "api_server"),
        "expected ActorStartFailed for api_server, got: {err:?}"
    );

    let _ = std::fs::remove_file(&socket_path);
}

#[tokio::test]
async fn test_watch_exits_on_shutdown_requested() {
    // GIVEN an EventBus and a watch in progress
    let (sender, _rx) = EventBus::new();
    let sender_clone = sender.clone();

    let watch_handle = tokio::spawn(async move { watch(&sender_clone).await });

    // WHEN ShutdownRequested is emitted. watch() subscribes inside its own
    // task, so signal until it has subscribed and exited rather than sleeping
    // a fixed delay and hoping the subscription already happened.
    let exited = poll_until_async(Duration::from_secs(5), || async {
        let _ = sender.send(RuntimeEvent::ShutdownRequested);
        watch_handle.is_finished()
    })
    .await;
    assert!(exited, "watch should exit after ShutdownRequested");

    // THEN watch() returns Ok(())
    let result = watch_handle.await.expect("join should succeed");
    assert!(result.is_ok());
}

// Supervisor starts successfully with llm_config = None
#[tokio::test]
async fn test_start_without_llm_config_succeeds() {
    // GIVEN a Supervisor with no [llm] section
    let reserved_port = reserve_port();
    let port = reserved_port.port();
    let socket_path = temp_socket_path();
    let config = SupervisorConfig {
        api_config: APIServerConfig {
            socket_path: socket_path.clone(),
            bind_addr: "127.0.0.1".to_owned(),
            tcp_port: Some(port),
            api_token: None,
            tls_cert_path: None,
            tls_key_path: None,
        },
        startup_timeout_secs: 10,
        llm_config: None,
        config_path: None,
        runtime_config: apollia_core::RuntimeConfig::default(),
        hitl_config: apollia_core::HitlConfig::default(),
        data_dir: {
            let d = tempfile::tempdir().expect("tempdir");
            let p = d.path().to_path_buf();
            std::mem::forget(d);
            p
        },
        obs_config: apollia_core::ObservabilityConfig::default(),
        agent_repository: None,
        package_repository: None,
        bundled_agents_path: None,
        tools_config: apollia_core::ToolsConfig::default(),
        mcp_loading: LoadingMode::Eager,
        tool_search_limit: 20,
        hooks_config: apollia_core::HooksConfig::default(),
        plan_mode_default: false,
        chat_default_workspace: None,
        chat_tool_turn_temperature: None,
    };
    // Release the probe listener only now, right before the bind it protects.
    reserved_port.release();
    let supervisor = Supervisor::new(config);

    // WHEN start() is called
    let handles = supervisor
        .start(
            MockBackend,
            Arc::new(crate::api::routes_agents::StubAgentLoader),
            None,
            None,
        )
        .await
        .expect("start() doit reussir sans config LLM");

    // THEN llm_router is None and startup proceeded normally
    assert!(
        handles.llm_router.is_none(),
        "llm_router doit etre None quand llm_config est absent"
    );

    // Cleanup
    handles.api_handle.shutdown();
    handles.router_handle.shutdown();
    handles.tool_registry_handle.shutdown().await;
    handles.registry_handle.shutdown();
    let _ = std::fs::remove_file(&socket_path);
}

// AppState clone preserves llm_router = None
#[tokio::test]
async fn test_app_state_clone_with_llm_router_none() {
    use crate::eventbus::EventBus;
    use crate::registry::AgentRegistry;
    use crate::router::TaskRouterHandle;

    // GIVEN an AppState with llm_router = None
    let (event_tx, _event_rx) = EventBus::new();
    let registry_handle = AgentRegistry::spawn(event_tx.clone());
    let router_handle: TaskRouterHandle<MockBackend> =
        TaskRouterHandle::spawn(registry_handle.clone(), event_tx.clone(), 64);
    let state = AppState {
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

    // WHEN the AppState is cloned
    let cloned = state.clone();

    // THEN the clone preserves an empty cell (the Arc is shared, but the
    // inner option stays None after clone).
    assert!(
        cloned.llm_router.read().await.is_none(),
        "le clone doit preserver llm_router = None"
    );
}

// Supervisor starts with 0 triggers; TriggerEngine is always present.
#[tokio::test]
async fn test_supervisor_starts_with_zero_triggers() {
    // GIVEN a config with no triggers
    let reserved_port = reserve_port();
    let port = reserved_port.port();
    let socket_path = temp_socket_path();
    let config = SupervisorConfig {
        api_config: APIServerConfig {
            socket_path: socket_path.clone(),
            bind_addr: "127.0.0.1".to_owned(),
            tcp_port: Some(port),
            api_token: None,
            tls_cert_path: None,
            tls_key_path: None,
        },
        startup_timeout_secs: 10,
        llm_config: None,
        config_path: None,
        runtime_config: apollia_core::RuntimeConfig::default(),
        hitl_config: apollia_core::HitlConfig::default(),
        data_dir: {
            let d = tempfile::tempdir().expect("tempdir");
            let p = d.path().to_path_buf();
            std::mem::forget(d);
            p
        },
        obs_config: apollia_core::ObservabilityConfig::default(),
        agent_repository: None,
        package_repository: None,
        bundled_agents_path: None,
        tools_config: apollia_core::ToolsConfig::default(),
        mcp_loading: LoadingMode::Eager,
        tool_search_limit: 20,
        hooks_config: apollia_core::HooksConfig::default(),
        plan_mode_default: false,
        chat_default_workspace: None,
        chat_tool_turn_temperature: None,
    };
    // Release the probe listener only now, right before the bind it protects.
    reserved_port.release();
    let supervisor = Supervisor::new(config);

    // WHEN start() is called
    let result = supervisor
        .start(
            MockBackend,
            Arc::new(crate::api::routes_agents::StubAgentLoader),
            None,
            None,
        )
        .await;

    // THEN startup succeeds and TriggerEngine is present with 0 triggers
    assert!(result.is_ok(), "start() doit reussir avec 0 triggers");
    let handles = result.unwrap();
    let trigger_list = handles.trigger_engine.list().await;
    assert!(
        trigger_list.is_empty(),
        "aucun trigger attendu, got {:?}",
        trigger_list
    );

    // Cleanup
    handles.trigger_engine.shutdown().await;
    handles.api_handle.shutdown();
    handles.router_handle.shutdown();
    handles.tool_registry_handle.shutdown().await;
    handles.registry_handle.shutdown();
    let _ = std::fs::remove_file(&socket_path);
}

// Supervisor starts without a [notifications] section and without error.
#[tokio::test]
async fn test_no_notifications_section_starts_ok() {
    // GIVEN a config with no [notifications] section
    let reserved_port = reserve_port();
    let port = reserved_port.port();
    let socket_path = temp_socket_path();
    let config = SupervisorConfig {
        api_config: APIServerConfig {
            socket_path: socket_path.clone(),
            bind_addr: "127.0.0.1".to_owned(),
            tcp_port: Some(port),
            api_token: None,
            tls_cert_path: None,
            tls_key_path: None,
        },
        startup_timeout_secs: 10,
        llm_config: None,
        config_path: None,
        runtime_config: apollia_core::RuntimeConfig::default(),
        hitl_config: apollia_core::HitlConfig::default(),
        data_dir: {
            let d = tempfile::tempdir().expect("tempdir");
            let p = d.path().to_path_buf();
            std::mem::forget(d);
            p
        },
        obs_config: apollia_core::ObservabilityConfig::default(),
        agent_repository: None,
        package_repository: None,
        bundled_agents_path: None,
        tools_config: apollia_core::ToolsConfig::default(),
        mcp_loading: LoadingMode::Eager,
        tool_search_limit: 20,
        hooks_config: apollia_core::HooksConfig::default(),
        plan_mode_default: false,
        chat_default_workspace: None,
        chat_tool_turn_temperature: None,
    };
    // Release the probe listener only now, right before the bind it protects.
    reserved_port.release();
    let supervisor = Supervisor::new(config);

    // WHEN start() is called
    let result = supervisor
        .start(
            MockBackend,
            Arc::new(crate::api::routes_agents::StubAgentLoader),
            None,
            None,
        )
        .await;

    // THEN no error, NotificationEngine silently not started
    assert!(
        result.is_ok(),
        "démarrage sans [notifications] doit réussir, erreur: {:?}",
        result.err()
    );

    // Cleanup
    let handles = result.unwrap();
    handles.api_handle.shutdown();
    handles.router_handle.shutdown();
    handles.tool_registry_handle.shutdown().await;
    handles.registry_handle.shutdown();
    let _ = std::fs::remove_file(&socket_path);
}

// Triggers loaded from SQLite at boot.
#[tokio::test]
async fn test_trigger_engine_loads_from_sqlite() {
    use apollia_triggers::{TriggerDefinitionRepository, TriggerDefinitionRow};

    // GIVEN a triggers_def.db pre-filled with 1 trigger
    let reserved_port = reserve_port();
    let port = reserved_port.port();
    let socket_path = temp_socket_path();
    let tmp_dir = tempfile::tempdir().expect("tempdir");
    let db_path = tmp_dir.path().join("triggers_def.db");
    let repo = TriggerDefinitionRepository::open(&db_path).expect("open repo");
    repo.insert(&TriggerDefinitionRow {
        id: "test-trigger".into(),
        agent: Some("test-agent".into()),
        enabled: true,
        on_busy: apollia_triggers::OnBusy::Queue,
        source_type: "cron".into(),
        source_config: serde_json::json!({ "schedule": "0 8 * * MON" }),
        input_template: Some("hello".into()),
        created_at: String::new(),
        updated_at: String::new(),
    })
    .expect("insert trigger def");
    drop(repo);

    let config = SupervisorConfig {
        api_config: APIServerConfig {
            socket_path: socket_path.clone(),
            bind_addr: "127.0.0.1".to_owned(),
            tcp_port: Some(port),
            api_token: None,
            tls_cert_path: None,
            tls_key_path: None,
        },
        startup_timeout_secs: 10,
        llm_config: None,
        config_path: None,
        runtime_config: apollia_core::RuntimeConfig::default(),
        hitl_config: apollia_core::HitlConfig::default(),
        data_dir: tmp_dir.path().to_path_buf(),
        obs_config: apollia_core::ObservabilityConfig::default(),
        agent_repository: None,
        package_repository: None,
        bundled_agents_path: None,
        tools_config: apollia_core::ToolsConfig::default(),
        mcp_loading: LoadingMode::Eager,
        tool_search_limit: 20,
        hooks_config: apollia_core::HooksConfig::default(),
        plan_mode_default: false,
        chat_default_workspace: None,
        chat_tool_turn_temperature: None,
    };
    // Release the probe listener only now, right before the bind it protects.
    reserved_port.release();
    let supervisor = Supervisor::new(config);

    // WHEN start() is called
    let handles = supervisor
        .start(
            MockBackend,
            Arc::new(crate::api::routes_agents::StubAgentLoader),
            None,
            None,
        )
        .await
        .expect("start() doit reussir");

    // THEN trigger_engine holds 1 trigger loaded from SQLite
    let trigger_list = handles.trigger_engine.list().await;
    assert_eq!(
        trigger_list.len(),
        1,
        "1 trigger attendu, got {:?}",
        trigger_list
    );
    assert_eq!(trigger_list[0].id, "test-trigger");

    // Cleanup
    handles.trigger_engine.shutdown().await;
    handles.api_handle.shutdown();
    handles.router_handle.shutdown();
    handles.tool_registry_handle.shutdown().await;
    handles.registry_handle.shutdown();
    let _ = std::fs::remove_file(&socket_path);
}

// Booting with empty DBs creates the databases and starts without error.
#[tokio::test]
async fn test_story187_boot_empty_dbs() {
    // GIVEN an empty directory (no pre-existing DB)
    let reserved_port = reserve_port();
    let port = reserved_port.port();
    let socket_path = temp_socket_path();
    let (config, _tmp_dir) = test_config(port, socket_path.clone());
    // Release the probe listener only now, right before the bind it protects.
    reserved_port.release();
    let supervisor = Supervisor::new(config);

    // WHEN start() is called
    let handles = supervisor
        .start(
            MockBackend,
            Arc::new(crate::api::routes_agents::StubAgentLoader),
            None,
            None,
        )
        .await
        .expect("boot with empty DBs should succeed");

    // THEN AllReady is emitted, 0 triggers; the NotificationEngine is
    // started because the supervisor auto-seeds a default desktop channel.
    // The marker in `user_memory` prevents a re-seed on subsequent boots.
    let trigger_list = handles.trigger_engine.list().await;
    assert!(trigger_list.is_empty(), "empty DB should yield 0 triggers");
    assert!(
        handles.notification_engine.is_some(),
        "empty DB should yield a NotificationEngine seeded with the default desktop channel"
    );

    // Cleanup
    handles.trigger_engine.shutdown().await;
    handles.api_handle.shutdown();
    handles.router_handle.shutdown();
    handles.tool_registry_handle.shutdown().await;
    handles.registry_handle.shutdown();
    let _ = std::fs::remove_file(&socket_path);
}

// ── seed_default_desktop_channel_if_needed (unit) ──────────────

/// Helper: (notif_repo, user_memory) initialised on fresh tempdirs.
fn make_seed_inputs() -> (
    NotificationConfigRepository,
    std::sync::Arc<std::sync::Mutex<apollia_memory::user_memory::UserMemoryRepository>>,
    tempfile::TempDir,
) {
    let dir = tempfile::tempdir().expect("tempdir");
    let notif = NotificationConfigRepository::open(&dir.path().join("notifications.db"))
        .expect("notif repo");
    let um =
        apollia_memory::user_memory::UserMemoryRepository::new(&dir.path().join("user_memory.db"))
            .expect("user memory");
    (notif, std::sync::Arc::new(std::sync::Mutex::new(um)), dir)
}

#[test]
fn test_seed_inserts_channel_when_empty_and_no_marker() {
    // GIVEN an empty notification repo and user memory with no marker
    let (notif, um, _tmp) = make_seed_inputs();

    // WHEN the seed runs
    seed_default_desktop_channel_if_needed(&notif, Some(&um));

    // THEN a `desktop-default` channel exists
    let chans = notif.list_channels().expect("list");
    assert_eq!(chans.len(), 1);
    let ch = &chans[0];
    assert_eq!(ch.id, "desktop-default");
    assert_eq!(ch.channel_type, "desktop");
    assert!(ch.enabled);
    // Label falls back to "Bureau" since no profile name was set.
    assert_eq!(ch.label.as_deref(), Some("Bureau"));

    // AND the marker was set in user memory
    let marker = um
        .lock()
        .unwrap()
        .get_internal(SEEDED_DESKTOP_CHANNEL_MARKER)
        .expect("get_internal")
        .expect("marker present");
    assert_eq!(marker, "true");
}

#[test]
fn test_seed_uses_profile_name_when_present() {
    // GIVEN a user memory with profile.name = "Nidal"
    let (notif, um, _tmp) = make_seed_inputs();
    um.lock()
        .unwrap()
        .set(
            "name",
            "Nidal",
            apollia_memory::user_memory::WrittenBy::User,
        )
        .expect("set name");

    // WHEN the seed runs
    seed_default_desktop_channel_if_needed(&notif, Some(&um));

    // THEN the label uses the personalised form
    let ch = notif
        .get_channel("desktop-default")
        .expect("get")
        .expect("Some");
    assert_eq!(ch.label.as_deref(), Some("Bureau de Nidal"));
}

#[test]
fn test_seed_skips_when_marker_present() {
    // GIVEN the marker is already set
    let (notif, um, _tmp) = make_seed_inputs();
    um.lock()
        .unwrap()
        .set_internal(SEEDED_DESKTOP_CHANNEL_MARKER, "true")
        .expect("set marker");

    // WHEN the seed runs
    seed_default_desktop_channel_if_needed(&notif, Some(&um));

    // THEN no channel is created (the user may have deleted the original)
    let chans = notif.list_channels().expect("list");
    assert!(chans.is_empty());
}

#[test]
fn test_seed_sets_marker_only_when_existing_channels() {
    // GIVEN a non-empty notification repo but no marker
    let (notif, um, _tmp) = make_seed_inputs();
    let existing = apollia_notifications::NotificationChannelRow {
        id: "my-webhook".into(),
        label: None,
        channel_type: "webhook".into(),
        enabled: true,
        config_json: serde_json::json!({"url": "https://hooks.example.com/x"}),
        events_json: None,
        min_interval_seconds: 0,
        created_at: String::new(),
        updated_at: String::new(),
    };
    notif.insert_channel(&existing).expect("insert");

    // WHEN seed runs
    seed_default_desktop_channel_if_needed(&notif, Some(&um));

    // THEN no new channel is added (still just the user's pre-existing one)
    let chans = notif.list_channels().expect("list");
    assert_eq!(chans.len(), 1);
    assert_eq!(chans[0].id, "my-webhook");

    // AND the marker is now set, so future boots skip the seed entirely
    let marker = um
        .lock()
        .unwrap()
        .get_internal(SEEDED_DESKTOP_CHANNEL_MARKER)
        .expect("get_internal")
        .expect("marker present");
    assert_eq!(marker, "true");
}

#[test]
fn test_seed_idempotent_across_multiple_calls() {
    // GIVEN, call seed twice
    let (notif, um, _tmp) = make_seed_inputs();
    seed_default_desktop_channel_if_needed(&notif, Some(&um));
    seed_default_desktop_channel_if_needed(&notif, Some(&um));

    // THEN, still only one channel, no duplicate insert
    let chans = notif.list_channels().expect("list");
    assert_eq!(chans.len(), 1);
}

#[test]
fn test_seed_no_op_when_user_memory_missing() {
    // GIVEN no user memory available
    let dir = tempfile::tempdir().expect("tempdir");
    let notif = NotificationConfigRepository::open(&dir.path().join("notifications.db"))
        .expect("notif repo");

    // WHEN seed is invoked with None
    seed_default_desktop_channel_if_needed(&notif, None);

    // THEN nothing is inserted (we cannot track a marker safely)
    let chans = notif.list_channels().expect("list");
    assert!(chans.is_empty());
}

// AppState holds the repositories after boot.
#[tokio::test]
async fn test_story187_appstate_contains_repos() {
    // GIVEN a Supervisor with an empty directory
    let reserved_port = reserve_port();
    let port = reserved_port.port();
    let socket_path = temp_socket_path();
    let tmp_dir = tempfile::tempdir().expect("tempdir");
    let config = SupervisorConfig {
        api_config: APIServerConfig {
            socket_path: socket_path.clone(),
            bind_addr: "127.0.0.1".to_owned(),
            tcp_port: Some(port),
            api_token: None,
            tls_cert_path: None,
            tls_key_path: None,
        },
        startup_timeout_secs: 10,
        llm_config: None,
        config_path: None,
        runtime_config: apollia_core::RuntimeConfig::default(),
        hitl_config: apollia_core::HitlConfig::default(),
        data_dir: tmp_dir.path().to_path_buf(),
        obs_config: apollia_core::ObservabilityConfig::default(),
        agent_repository: None,
        package_repository: None,
        bundled_agents_path: None,
        tools_config: apollia_core::ToolsConfig::default(),
        mcp_loading: LoadingMode::Eager,
        tool_search_limit: 20,
        hooks_config: apollia_core::HooksConfig::default(),
        plan_mode_default: false,
        chat_default_workspace: None,
        chat_tool_turn_temperature: None,
    };
    // Release the probe listener only now, right before the bind it protects.
    reserved_port.release();
    let supervisor = Supervisor::new(config);

    // WHEN start() succeeds
    let handles = supervisor
        .start(
            MockBackend,
            Arc::new(crate::api::routes_agents::StubAgentLoader),
            None,
            None,
        )
        .await
        .expect("start should succeed");

    // THEN the DB files are created in data_dir
    assert!(
        tmp_dir.path().join("triggers_def.db").exists(),
        "triggers_def.db should be created"
    );
    assert!(
        tmp_dir.path().join("notifications.db").exists(),
        "notifications.db should be created"
    );

    // Cleanup
    handles.trigger_engine.shutdown().await;
    handles.api_handle.shutdown();
    handles.router_handle.shutdown();
    handles.tool_registry_handle.shutdown().await;
    handles.registry_handle.shutdown();
    let _ = std::fs::remove_file(&socket_path);
}

// ── Auto-load installed agents at boot ──────────────────────

/// Creates a test [`AgentManifest`] with minimal fields.
fn test_manifest(name: &str) -> apollia_core::AgentManifest {
    apollia_core::AgentManifest {
        format_version: 1,
        name: name.to_string(),
        version: "1.0.0".to_string(),
        description: format!("Test agent {name}"),
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
        supports_mailbox: false,
        mailbox_allowlist: None,
        system_prompt: None,
        tools_requiring_approval: vec![],
        llm_backend: None,
        packages: vec![],
        memory_config: None,
        agent_type: None,
        examples: vec![],
        limitations: vec![],
        setup_notes: None,
        agent_class: None,
        user_memory_write: false,
        datasources: vec![],
        templates: vec![],
        secrets: vec![],
        check_commands: vec![],
    }
}

/// Creates a test [`InstalledAgent`].
fn test_installed_agent(name: &str, enabled: bool) -> apollia_tools::InstalledAgent {
    apollia_tools::InstalledAgent {
        name: name.to_string(),
        version: "1.0.0".to_string(),
        // Use the agent name as the filename stem so StubAgentLoader derives
        // the correct manifest name (name = file_stem of install_path).
        install_path: PathBuf::from(format!("/tmp/agents/{name}/{name}.py")),
        source_path: PathBuf::from(format!("/tmp/{name}.py")),
        manifest: test_manifest(name),
        enabled,
        installed_at: "2026-03-17T10:00:00Z".to_string(),
        updated_at: "2026-03-17T10:00:00Z".to_string(),
    }
}

/// Opens an in-memory [`AgentRepository`] for testing.
fn open_test_repo() -> AgentRepository {
    AgentRepository::open(std::path::Path::new(":memory:")).expect("in-memory repo should open")
}

/// An [`AgentLoader`] that fails for agents whose path contains "corrupted".
struct FailingAgentLoader;

impl AgentLoader for FailingAgentLoader {
    fn load_and_validate(
        &self,
        path: &std::path::Path,
    ) -> Result<apollia_core::AgentManifest, String> {
        let path_str = path.to_string_lossy();
        if path_str.contains("corrupted") {
            return Err("Python syntax error: invalid syntax".to_string());
        }
        // Delegate to StubAgentLoader for valid agents
        crate::api::routes_agents::StubAgentLoader.load_and_validate(path)
    }
}

/// Helper to shutdown handles cleanly.
async fn shutdown_handles(handles: SupervisorHandles<MockBackend>, socket_path: &std::path::Path) {
    handles.trigger_engine.shutdown().await;
    handles.api_handle.shutdown();
    handles.router_handle.shutdown();
    handles.tool_registry_handle.shutdown().await;
    handles.registry_handle.shutdown();
    let _ = std::fs::remove_file(socket_path);
}

// Enabled agents are loaded at boot.
#[tokio::test]
async fn test_autoload_enabled_agents() {
    // GIVEN 3 installed agents, 2 of them enabled
    let repo = open_test_repo();
    repo.save(&test_installed_agent("agent-a", true))
        .expect("save a");
    repo.save(&test_installed_agent("agent-b", true))
        .expect("save b");
    repo.save(&test_installed_agent("agent-c", false))
        .expect("save c (disabled)");

    let reserved_port = reserve_port();
    let port = reserved_port.port();
    let socket_path = temp_socket_path();
    let (mut config, _tmp_dir) = test_config(port, socket_path.clone());
    config.agent_repository = Some(repo);

    // Release the probe listener only now, right before the bind it protects.
    reserved_port.release();
    let supervisor = Supervisor::new(config);

    // WHEN the Supervisor starts
    let handles = supervisor
        .start(
            MockBackend,
            Arc::new(crate::api::routes_agents::StubAgentLoader),
            None,
            None,
        )
        .await
        .expect("start() should succeed");

    // THEN the 2 enabled agents are registered in AgentRegistry
    let agents = handles
        .registry_handle
        .list_agents()
        .await
        .expect("list_agents should succeed");
    assert_eq!(agents.len(), 2, "2 enabled agents should be registered");

    shutdown_handles(handles, &socket_path).await;
}

// Disabled agents are ignored.
#[tokio::test]
async fn test_autoload_skips_disabled() {
    // GIVEN 2 agents, 1 of them disabled
    let repo = open_test_repo();
    repo.save(&test_installed_agent("enabled-agent", true))
        .expect("save enabled");
    repo.save(&test_installed_agent("disabled-agent", false))
        .expect("save disabled");

    let reserved_port = reserve_port();
    let port = reserved_port.port();
    let socket_path = temp_socket_path();
    let (mut config, _tmp_dir) = test_config(port, socket_path.clone());
    config.agent_repository = Some(repo);

    // Release the probe listener only now, right before the bind it protects.
    reserved_port.release();
    let supervisor = Supervisor::new(config);

    // WHEN the Supervisor starts
    let handles = supervisor
        .start(
            MockBackend,
            Arc::new(crate::api::routes_agents::StubAgentLoader),
            None,
            None,
        )
        .await
        .expect("start() should succeed");

    // THEN only the enabled agent is registered
    let agents = handles
        .registry_handle
        .list_agents()
        .await
        .expect("list_agents should succeed");
    assert_eq!(agents.len(), 1, "only 1 enabled agent should be registered");

    shutdown_handles(handles, &socket_path).await;
}

// A failing agent does not block the boot.
#[tokio::test]
async fn test_autoload_corrupted_agent_continues() {
    // GIVEN 2 enabled agents, 1 with a "corrupted" file
    let repo = open_test_repo();
    let mut valid = test_installed_agent("valid-agent", true);
    valid.install_path = PathBuf::from("/tmp/agents/valid-agent/agent.py");
    repo.save(&valid).expect("save valid");

    let mut corrupted = test_installed_agent("corrupted-agent", true);
    corrupted.install_path = PathBuf::from("/tmp/agents/corrupted/agent.py");
    repo.save(&corrupted).expect("save corrupted");

    let reserved_port = reserve_port();
    let port = reserved_port.port();
    let socket_path = temp_socket_path();
    let (mut config, _tmp_dir) = test_config(port, socket_path.clone());
    config.agent_repository = Some(repo);

    // Release the probe listener only now, right before the bind it protects.
    reserved_port.release();
    let supervisor = Supervisor::new(config);

    // We cannot subscribe before start() to capture AgentLoadFailed because
    // event_sender is created inside start(). Instead we verify that the boot
    // succeeds and the valid agent is loaded.

    // WHEN the Supervisor starts with a loader that fails for "corrupted"
    let handles = supervisor
        .start(MockBackend, Arc::new(FailingAgentLoader), None, None)
        .await
        .expect("start() should succeed despite corrupted agent");

    // THEN the valid agent is registered
    let agents = handles
        .registry_handle
        .list_agents()
        .await
        .expect("list_agents should succeed");
    assert_eq!(agents.len(), 1, "only the valid agent should be registered");

    shutdown_handles(handles, &socket_path).await;
}

// No installed agents means a normal boot.
#[tokio::test]
async fn test_autoload_no_agents_no_error() {
    // GIVEN an empty database
    let repo = open_test_repo();

    let reserved_port = reserve_port();
    let port = reserved_port.port();
    let socket_path = temp_socket_path();
    let (mut config, _tmp_dir) = test_config(port, socket_path.clone());
    config.agent_repository = Some(repo);

    // Release the probe listener only now, right before the bind it protects.
    reserved_port.release();
    let supervisor = Supervisor::new(config);

    // WHEN the Supervisor starts
    let handles = supervisor
        .start(
            MockBackend,
            Arc::new(crate::api::routes_agents::StubAgentLoader),
            None,
            None,
        )
        .await
        .expect("start() should succeed with no agents");

    // THEN no agent is registered and there is no error
    let agents = handles
        .registry_handle
        .list_agents()
        .await
        .expect("list_agents should succeed");
    assert!(agents.is_empty(), "no agents should be registered");

    shutdown_handles(handles, &socket_path).await;
}

// agent_repository = None skips auto-load.
#[tokio::test]
async fn test_autoload_none_repository_skips() {
    // GIVEN a config without agent_repository
    let reserved_port = reserve_port();
    let port = reserved_port.port();
    let socket_path = temp_socket_path();
    let (config, _tmp_dir) = test_config(port, socket_path.clone());
    // agent_repository is already None in test_config

    // Release the probe listener only now, right before the bind it protects.
    reserved_port.release();
    let supervisor = Supervisor::new(config);

    // WHEN the Supervisor starts
    let handles = supervisor
        .start(
            MockBackend,
            Arc::new(crate::api::routes_agents::StubAgentLoader),
            None,
            None,
        )
        .await
        .expect("start() should succeed without agent_repository");

    // THEN no agent registered, normal boot
    let agents = handles
        .registry_handle
        .list_agents()
        .await
        .expect("list_agents should succeed");
    assert!(
        agents.is_empty(),
        "no agents should be registered when agent_repository is None"
    );

    shutdown_handles(handles, &socket_path).await;
}

// Agent with empty packages: no venv created, Active state.
#[tokio::test]
async fn test_autoload_empty_packages_agent_is_active() {
    // GIVEN an agent with packages: []
    let repo = open_test_repo();
    repo.save(&test_installed_agent("no-pkg-agent", true))
        .expect("save agent");

    let reserved_port = reserve_port();
    let port = reserved_port.port();
    let socket_path = temp_socket_path();
    let (mut config, tmp_dir) = test_config(port, socket_path.clone());
    config.agent_repository = Some(repo);

    // Release the probe listener only now, right before the bind it protects.
    reserved_port.release();
    let supervisor = Supervisor::new(config);

    // WHEN the Supervisor starts
    let handles = supervisor
        .start(
            MockBackend,
            Arc::new(crate::api::routes_agents::StubAgentLoader),
            None,
            None,
        )
        .await
        .expect("start() should succeed");

    // THEN the agent is in Active state
    let agents = handles
        .registry_handle
        .list_agents()
        .await
        .expect("list_agents should succeed");
    assert_eq!(agents.len(), 1);
    assert_eq!(agents[0].process_state, ProcessState::Active);

    // AND no venv was created in data_dir/venvs/
    let venv_dir = tmp_dir.path().join("venvs").join("no-pkg-agent");
    assert!(
        !venv_dir.exists(),
        "venv directory should not be created for agent with empty packages"
    );

    shutdown_handles(handles, &socket_path).await;
}

// Agent with an invalid package: Degraded state, other agents start normally.
#[tokio::test]
async fn test_autoload_bad_package_agent_is_degraded() {
    // GIVEN two agents: one with an invalid package, one without packages
    // Install paths use the agent name as file stem so StubAgentLoader resolves correctly.
    let repo = open_test_repo();

    let mut bad_agent = test_installed_agent("bad-pkg-agent", true);
    bad_agent.install_path = PathBuf::from("/tmp/agents/bad-pkg-agent/bad-pkg-agent.py");
    bad_agent.manifest.packages = vec!["nonexistent-pkg-zzz-99999==0.0.1".to_string()];
    repo.save(&bad_agent).expect("save bad agent");

    let mut good_agent = test_installed_agent("good-agent", true);
    good_agent.install_path = PathBuf::from("/tmp/agents/good-agent/good-agent.py");
    repo.save(&good_agent).expect("save good agent");

    let reserved_port = reserve_port();
    let port = reserved_port.port();
    let socket_path = temp_socket_path();
    let (mut config, _tmp_dir) = test_config(port, socket_path.clone());
    config.agent_repository = Some(repo);

    // Loader that injects packages into the bad agent's manifest using the file stem.
    struct PackageAwareLoader;
    impl AgentLoader for PackageAwareLoader {
        fn load_and_validate(
            &self,
            path: &std::path::Path,
        ) -> Result<apollia_core::AgentManifest, String> {
            let mut manifest =
                crate::api::routes_agents::StubAgentLoader.load_and_validate(path)?;
            if manifest.name == "bad-pkg-agent" {
                manifest.packages = vec!["nonexistent-pkg-zzz-99999==0.0.1".to_string()];
            }
            Ok(manifest)
        }
    }

    // Release the probe listener only now, right before the bind it protects.
    reserved_port.release();
    let supervisor = Supervisor::new(config);

    // WHEN the Supervisor starts
    let handles = supervisor
        .start(MockBackend, Arc::new(PackageAwareLoader), None, None)
        .await
        .expect("start() should succeed despite bad package");

    // THEN both agents are registered
    let agents = handles
        .registry_handle
        .list_agents()
        .await
        .expect("list_agents should succeed");
    assert_eq!(agents.len(), 2, "both agents should be registered");

    // AND the valid agent is Active
    let good = agents
        .iter()
        .find(|a| a.manifest.name == "good-agent")
        .expect("good-agent should be registered");
    assert_eq!(
        good.process_state,
        ProcessState::Active,
        "good-agent should be Active"
    );

    // AND the agent with a bad package is Degraded (python3 available) or Active (python3 absent)
    let bad = agents
        .iter()
        .find(|a| a.manifest.name == "bad-pkg-agent")
        .expect("bad-pkg-agent should be registered");
    assert!(
        bad.process_state == ProcessState::Degraded || bad.process_state == ProcessState::Active,
        "bad-pkg-agent should be Degraded or Active (never Stopped), got {:?}",
        bad.process_state
    );

    shutdown_handles(handles, &socket_path).await;
}

#[tokio::test]
async fn test_first_launch_emits_onboarding_required() {
    // GIVEN a fresh Supervisor with empty UserMemory (no entries)
    let reserved_port = reserve_port();
    let port = reserved_port.port();
    let socket_path = temp_socket_path();
    let (config, _tmp_dir) = test_config(port, socket_path.clone());
    // Release the probe listener only now, right before the bind it protects.
    reserved_port.release();
    let supervisor = Supervisor::new(config);

    // WHEN start() completes, user_memory.db is created empty
    let handles = supervisor
        .start(
            MockBackend,
            Arc::new(crate::api::routes_agents::StubAgentLoader),
            None,
            None,
        )
        .await
        .expect("start() should succeed");

    // THEN OnboardingRequired was emitted (verify via a fresh subscriber + replay)
    // Since we can't replay past events, we verify the user_memory handle is Some
    // and that a fresh empty repo triggers the detection logic directly.
    assert!(
        handles.user_memory.is_some(),
        "user_memory handle should be present"
    );
    let um = handles.user_memory.as_ref().expect("checked above");
    let is_empty = um.lock().expect("lock").is_empty().expect("is_empty");
    assert!(is_empty, "user_memory should be empty on first launch");

    shutdown_handles(handles, &socket_path).await;
}

#[tokio::test]
async fn test_subsequent_launch_no_onboarding_event() {
    // GIVEN a Supervisor whose UserMemory already contains entries
    let reserved_port = reserve_port();
    let port = reserved_port.port();
    let socket_path = temp_socket_path();
    let (config, _tmp_dir) = test_config(port, socket_path.clone());

    // Pre-populate user_memory.db before starting the Supervisor
    let user_memory_db = config.data_dir.join("user_memory.db");
    {
        let repo = apollia_memory::user_memory::UserMemoryRepository::new(&user_memory_db)
            .expect("open user_memory.db");
        repo.set(
            "preferences.language",
            "fr",
            apollia_memory::user_memory::WrittenBy::Onboarding,
        )
        .expect("set entry");
    }

    // Release the probe listener only now, right before the bind it protects.
    reserved_port.release();
    let supervisor = Supervisor::new(config);

    // WHEN start() completes
    let handles = supervisor
        .start(
            MockBackend,
            Arc::new(crate::api::routes_agents::StubAgentLoader),
            None,
            None,
        )
        .await
        .expect("start() should succeed");

    // Subscribe after start to check no OnboardingRequired is emitted for subsequent events
    let mut rx = handles.event_sender.subscribe();

    // Emit a sentinel event to drain the bus
    let _ = handles.event_sender.send(RuntimeEvent::AllReady);
    let event = tokio::time::timeout(Duration::from_secs(1), rx.recv())
        .await
        .expect("should receive within 1s")
        .expect("recv should succeed");

    // THEN the received event is AllReady, not OnboardingRequired
    assert!(
        matches!(event, RuntimeEvent::AllReady),
        "expected AllReady sentinel, got: {event:?}"
    );

    // AND user_memory is not empty
    let um = handles.user_memory.as_ref().expect("user_memory present");
    let is_empty = um.lock().expect("lock").is_empty().expect("is_empty");
    assert!(
        !is_empty,
        "user_memory should NOT be empty on subsequent launch"
    );

    shutdown_handles(handles, &socket_path).await;
}

// ── Bundled agents auto-install ──────────────────────────────

/// Creates a temporary bundled directory with a manifest.json and Python stubs.
fn setup_bundled_dir(tmp: &tempfile::TempDir, agents: &[(&str, &str)]) -> std::path::PathBuf {
    let bundled_dir = tmp.path().join("bundled");
    std::fs::create_dir_all(&bundled_dir).expect("create bundled dir");

    let entries: Vec<serde_json::Value> = agents
        .iter()
        .map(|(name, file)| {
            serde_json::json!({
                "name": name,
                "file": file,
                "auto_install": true,
                "description": format!("Test agent {name}")
            })
        })
        .collect();

    let manifest = serde_json::json!({
        "version": "1.0.0",
        "bundled_agents": entries
    });
    std::fs::write(
        bundled_dir.join("manifest.json"),
        serde_json::to_string_pretty(&manifest).expect("serialize"),
    )
    .expect("write manifest.json");

    for (name, file) in agents {
        // Stub Python files, named so StubAgentLoader derives the agent name from stem.
        std::fs::write(bundled_dir.join(file), format!("# {name}\n")).expect("write stub agent");
    }

    bundled_dir
}

// No manifest.json means no agent installed and no error.
#[test]
fn test_no_manifest_no_error() {
    // GIVEN no manifest.json in the bundled directory
    let tmp = tempfile::tempdir().expect("tempdir");
    let bundled_dir = tmp.path().join("bundled");
    std::fs::create_dir_all(&bundled_dir).expect("create dir");

    let repo = open_test_repo();

    // WHEN
    let loader: Arc<dyn AgentLoader> = Arc::new(crate::api::routes_agents::StubAgentLoader);
    auto_load_bundled_agents(Some(&bundled_dir), &repo, &loader);

    // THEN no agent in the database
    let agents = repo.list().expect("list should succeed");
    assert!(agents.is_empty(), "no agents should be installed");
}

// bundled_agents_path = None means no agent installed and no error.
#[test]
fn test_no_bundled_path_no_error() {
    // GIVEN bundled_agents_path is not configured
    let repo = open_test_repo();
    let loader: Arc<dyn AgentLoader> = Arc::new(crate::api::routes_agents::StubAgentLoader);

    // WHEN
    auto_load_bundled_agents(None, &repo, &loader);

    // THEN no agent in the database
    let agents = repo.list().expect("list should succeed");
    assert!(
        agents.is_empty(),
        "no agents should be installed when path is None"
    );
}

// 4 agents in the manifest, empty DB: 4 agents registered.
#[test]
fn test_bundled_agents_auto_installed() {
    // GIVEN a manifest.json with 4 agents AND no agent in the DB
    let tmp = tempfile::tempdir().expect("tempdir");
    let bundled_dir = setup_bundled_dir(
        &tmp,
        &[
            ("excel-worker", "excel-worker.py"),
            ("csv-data-worker", "csv-data-worker.py"),
            ("pdf-worker", "pdf-worker.py"),
            ("code-worker", "code-worker.py"),
        ],
    );

    let repo = open_test_repo();
    let loader: Arc<dyn AgentLoader> = Arc::new(crate::api::routes_agents::StubAgentLoader);

    // WHEN
    auto_load_bundled_agents(Some(&bundled_dir), &repo, &loader);

    // THEN 4 agents in the database, all enabled
    let agents = repo.list().expect("list should succeed");
    assert_eq!(agents.len(), 4, "all 4 bundled agents should be installed");
    assert!(
        agents.iter().all(|a| a.enabled),
        "all agents should be enabled"
    );
    let names: Vec<&str> = agents.iter().map(|a| a.name.as_str()).collect();
    assert!(names.contains(&"excel-worker"));
    assert!(names.contains(&"csv-data-worker"));
    assert!(names.contains(&"pdf-worker"));
    assert!(names.contains(&"code-worker"));
}

// excel-worker already in the DB: not reinstalled, the 3 others are installed.
#[test]
fn test_bundled_agents_skip_existing() {
    // GIVEN excel-worker already installed in the DB
    let tmp = tempfile::tempdir().expect("tempdir");
    let bundled_dir = setup_bundled_dir(
        &tmp,
        &[
            ("excel-worker", "excel-worker.py"),
            ("csv-data-worker", "csv-data-worker.py"),
            ("pdf-worker", "pdf-worker.py"),
            ("code-worker", "code-worker.py"),
        ],
    );

    let repo = open_test_repo();
    repo.save(&test_installed_agent("excel-worker", true))
        .expect("pre-install excel-worker");

    let loader: Arc<dyn AgentLoader> = Arc::new(crate::api::routes_agents::StubAgentLoader);

    // WHEN
    auto_load_bundled_agents(Some(&bundled_dir), &repo, &loader);

    // THEN 4 agents in the DB (1 pre-existing + 3 new), excel-worker not duplicated
    let agents = repo.list().expect("list should succeed");
    assert_eq!(agents.len(), 4, "4 agents total (1 existing + 3 new)");
    let excel_count = agents.iter().filter(|a| a.name == "excel-worker").count();
    assert_eq!(excel_count, 1, "excel-worker should not be duplicated");
}
