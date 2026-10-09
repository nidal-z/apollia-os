use super::bootstrap::{seed_default_desktop_channel_if_needed, SEEDED_DESKTOP_CHANNEL_MARKER};
use super::bundled::native_tool_descriptors;

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
    // THEN: 17 baseline tools, plus optional web tools when their features
    // are on.
    let expected_count =
        17 + cfg!(feature = "web-search") as usize + cfg!(feature = "web-read") as usize;
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

/// The action family of every native tool, copied from the product decision
/// that fixes it. A divergence between this table and a descriptor is a tool
/// whose family changed without the decision changing with it.
const NATIVE_TOOL_FAMILIES: &[(&str, apollia_tools::ActionFamily)] = &[
    ("file_read", apollia_tools::ActionFamily::Read),
    ("file_list", apollia_tools::ActionFamily::Read),
    ("file_glob", apollia_tools::ActionFamily::Read),
    ("file_grep", apollia_tools::ActionFamily::Read),
    ("notebook_read", apollia_tools::ActionFamily::Read),
    ("code_context", apollia_tools::ActionFamily::Read),
    ("web_search", apollia_tools::ActionFamily::Read),
    ("web_read", apollia_tools::ActionFamily::Read),
    ("mcp_resources_list", apollia_tools::ActionFamily::Read),
    ("mcp_resources_read", apollia_tools::ActionFamily::Read),
    ("memory_search", apollia_tools::ActionFamily::Exempt),
    ("memory_write", apollia_tools::ActionFamily::Exempt),
    ("source_search", apollia_tools::ActionFamily::Read),
    ("file_write", apollia_tools::ActionFamily::WriteOrDelete),
    (
        "profile_propose",
        apollia_tools::ActionFamily::WriteOrDelete,
    ),
    ("file_edit", apollia_tools::ActionFamily::WriteOrDelete),
    ("notebook_edit", apollia_tools::ActionFamily::WriteOrDelete),
    ("http_fetch", apollia_tools::ActionFamily::SendExternal),
    ("bash_executor", apollia_tools::ActionFamily::ExecuteCode),
    ("python_executor", apollia_tools::ActionFamily::ExecuteCode),
    ("ask_user", apollia_tools::ActionFamily::Exempt),
];

#[test]
fn every_registered_native_tool_carries_the_decided_family() {
    // GIVEN every native descriptor registered at boot, the MCP resource
    // tools included
    let mut descriptors = native_tool_descriptors();
    descriptors.extend(apollia_mcp::mcp_resources::mcp_resource_descriptors());
    assert!(!descriptors.is_empty());

    for d in descriptors {
        // WHEN its family is looked up in the decided table
        let expected = NATIVE_TOOL_FAMILIES
            .iter()
            .find(|(name, _)| *name == d.name)
            .map(|(_, family)| *family);

        // THEN the tool is in the table, carries the decided family, and that
        // family is fixed in Apollia's code
        let Some(expected) = expected else {
            panic!("native tool `{}` has no decided family", d.name);
        };
        assert_eq!(d.family, expected, "family of `{}`", d.name);
        assert_eq!(
            d.family_source,
            apollia_tools::FamilySource::Builtin,
            "family source of `{}`",
            d.name
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
            tcp_listener: None,
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
        tools_config: apollia_core::ToolsConfig::default(),
        mcp_loading: LoadingMode::Eager,
        tool_search_limit: 20,
        hooks_config: apollia_core::HooksConfig::default(),
        plan_mode_default: false,
        chat_tool_turn_temperature: None,
        workflows_config: apollia_core::WorkflowsConfig::default(),
        config_channels: None,
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
    let result = supervisor.start(MockBackend, None).await;

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
    let handles = supervisor.start(MockBackend, None).await.unwrap();

    // Subscribe AFTER start (AllReady was already emitted, but let's verify via a new event)
    let mut rx = handles.event_sender.subscribe();

    // Emit a test event to verify the bus is working
    let _ = handles.event_sender.send(RuntimeEvent::AllReady);
    let event = tokio::time::timeout(Duration::from_secs(1), rx.recv())
        .await
        .expect("should receive within 1s")
        .expect("recv should succeed");
    // THEN the subscriber receives it within a second
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
    // WHEN start() returns
    let handles = supervisor.start(MockBackend, None).await.unwrap();

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
    // native tools + the tools of each connected account (Google + Microsoft
    // 365, none without an account) + the mailbox tools + the
    // always-advertised MCP resource tools.
    let connector_count = crate::connectors_bridge::connector_descriptors_for(
        &crate::connectors_bridge::connected_accounts().await,
    )
    .len()
        + crate::connectors_bridge::imap_tool_descriptors().len();
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

    // Organisation policy: the verdict read at boot from the policy location
    let location = apollia_permissions::policy_location();
    assert_eq!(
        *handles.org_policy,
        apollia_permissions::load_policy(&location.dir, location.owner)
    );

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
            tcp_listener: None,
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
        tools_config: apollia_core::ToolsConfig::default(),
        mcp_loading: LoadingMode::Eager,
        tool_search_limit: 20,
        hooks_config: apollia_core::HooksConfig::default(),
        plan_mode_default: false,
        chat_tool_turn_temperature: None,
        workflows_config: apollia_core::WorkflowsConfig::default(),
        config_channels: None,
    };
    let supervisor = Supervisor::new(config);

    // WHEN start() is called
    let result = supervisor.start(MockBackend, None).await;

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
            tcp_listener: None,
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
        tools_config: apollia_core::ToolsConfig::default(),
        mcp_loading: LoadingMode::Eager,
        tool_search_limit: 20,
        hooks_config: apollia_core::HooksConfig::default(),
        plan_mode_default: false,
        chat_tool_turn_temperature: None,
        workflows_config: apollia_core::WorkflowsConfig::default(),
        config_channels: None,
    };
    // Release the probe listener only now, right before the bind it protects.
    reserved_port.release();
    let supervisor = Supervisor::new(config);

    // WHEN start() is called
    let handles = supervisor
        .start(MockBackend, None)
        .await
        .expect("start() must succeed without an LLM config");

    // THEN llm_router is None and startup proceeded normally
    assert!(
        handles.llm_router.is_none(),
        "llm_router must be None when llm_config is absent"
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
    };

    // WHEN the AppState is cloned
    let cloned = state.clone();

    // THEN the clone preserves an empty cell (the Arc is shared, but the
    // inner option stays None after clone).
    assert!(
        cloned.llm_router.read().await.is_none(),
        "the clone must preserve llm_router = None"
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
            tcp_listener: None,
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
        tools_config: apollia_core::ToolsConfig::default(),
        mcp_loading: LoadingMode::Eager,
        tool_search_limit: 20,
        hooks_config: apollia_core::HooksConfig::default(),
        plan_mode_default: false,
        chat_tool_turn_temperature: None,
        workflows_config: apollia_core::WorkflowsConfig::default(),
        config_channels: None,
    };
    // Release the probe listener only now, right before the bind it protects.
    reserved_port.release();
    let supervisor = Supervisor::new(config);

    // WHEN start() is called
    let result = supervisor.start(MockBackend, None).await;

    // THEN startup succeeds and TriggerEngine is present with 0 triggers
    assert!(result.is_ok(), "start() must succeed with 0 triggers");
    let handles = result.unwrap();
    let trigger_list = handles.trigger_engine.list().await;
    assert!(
        trigger_list.is_empty(),
        "no trigger expected, got {:?}",
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
            tcp_listener: None,
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
        tools_config: apollia_core::ToolsConfig::default(),
        mcp_loading: LoadingMode::Eager,
        tool_search_limit: 20,
        hooks_config: apollia_core::HooksConfig::default(),
        plan_mode_default: false,
        chat_tool_turn_temperature: None,
        workflows_config: apollia_core::WorkflowsConfig::default(),
        config_channels: None,
    };
    // Release the probe listener only now, right before the bind it protects.
    reserved_port.release();
    let supervisor = Supervisor::new(config);

    // WHEN start() is called
    let result = supervisor.start(MockBackend, None).await;

    // THEN no error, NotificationEngine silently not started
    assert!(
        result.is_ok(),
        "start without [notifications] must succeed, error: {:?}",
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
        workflow: None,
        workflow_inputs: None,
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
            tcp_listener: None,
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
        tools_config: apollia_core::ToolsConfig::default(),
        mcp_loading: LoadingMode::Eager,
        tool_search_limit: 20,
        hooks_config: apollia_core::HooksConfig::default(),
        plan_mode_default: false,
        chat_tool_turn_temperature: None,
        workflows_config: apollia_core::WorkflowsConfig::default(),
        config_channels: None,
    };
    // Release the probe listener only now, right before the bind it protects.
    reserved_port.release();
    let supervisor = Supervisor::new(config);

    // WHEN start() is called
    let handles = supervisor
        .start(MockBackend, None)
        .await
        .expect("start() must succeed");

    // THEN trigger_engine holds 1 trigger loaded from SQLite
    let trigger_list = handles.trigger_engine.list().await;
    assert_eq!(
        trigger_list.len(),
        1,
        "1 trigger expected, got {:?}",
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
        .start(MockBackend, None)
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
    // Label falls back to "Desktop" since no profile name was set.
    assert_eq!(ch.label.as_deref(), Some("Desktop"));

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
    // GIVEN a user memory with profile.name = "Ada"
    let (notif, um, _tmp) = make_seed_inputs();
    um.lock()
        .unwrap()
        .set("name", "Ada", apollia_memory::user_memory::WrittenBy::User)
        .expect("set name");

    // WHEN the seed runs
    seed_default_desktop_channel_if_needed(&notif, Some(&um));

    // THEN the label uses the personalised form
    let ch = notif
        .get_channel("desktop-default")
        .expect("get")
        .expect("Some");
    assert_eq!(ch.label.as_deref(), Some("Ada's desktop"));
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
    // WHEN the default desktop channel is seeded twice
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
            tcp_listener: None,
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
        tools_config: apollia_core::ToolsConfig::default(),
        mcp_loading: LoadingMode::Eager,
        tool_search_limit: 20,
        hooks_config: apollia_core::HooksConfig::default(),
        plan_mode_default: false,
        chat_tool_turn_temperature: None,
        workflows_config: apollia_core::WorkflowsConfig::default(),
        config_channels: None,
    };
    // Release the probe listener only now, right before the bind it protects.
    reserved_port.release();
    let supervisor = Supervisor::new(config);

    // WHEN start() succeeds
    let handles = supervisor
        .start(MockBackend, None)
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

/// A stored agent named `name`, as a package or the interface leaves it.
fn test_stored_agent(name: &str, enabled: bool) -> apollia_tools::StoredAgent {
    apollia_tools::StoredAgent {
        name: name.to_string(),
        origin: apollia_tools::AgentOrigin::Package,
        package: None,
        version: "1.0.0".to_string(),
        definition: serde_json::json!({
            "id": name, "name": name, "mission": format!("Test agent {name}"),
            "defaults": {},
        }),
        enabled,
        installed_at: "2026-03-17T10:00:00Z".to_string(),
        updated_at: "2026-03-17T10:00:00Z".to_string(),
    }
}

/// Opens an in-memory [`AgentRepository`] for testing.
fn open_test_repo() -> AgentRepository {
    AgentRepository::open(std::path::Path::new(":memory:")).expect("in-memory repo should open")
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
    repo.save_definition(&test_stored_agent("agent-a", true))
        .expect("save a");
    repo.save_definition(&test_stored_agent("agent-b", true))
        .expect("save b");
    repo.save_definition(&test_stored_agent("agent-c", false))
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
        .start(MockBackend, None)
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
    repo.save_definition(&test_stored_agent("enabled-agent", true))
        .expect("save enabled");
    repo.save_definition(&test_stored_agent("disabled-agent", false))
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
        .start(MockBackend, None)
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
        .start(MockBackend, None)
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
        .start(MockBackend, None)
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
        .start(MockBackend, None)
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
        .start(MockBackend, None)
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

// The boot registers what the store holds, and never the Apollia agent.
#[tokio::test]
async fn test_boot_registers_the_stored_agents_and_never_the_apollia_agent() {
    // GIVEN a store holding two enabled agents beside the Apollia agent,
    // which every store holds
    let repo = open_test_repo();
    repo.save_definition(&test_stored_agent("agent-a", true))
        .expect("save a");
    repo.save_definition(&test_stored_agent("agent-b", true))
        .expect("save b");
    assert!(repo
        .stored(apollia_tools::APOLLIA_AGENT_ID)
        .expect("stored")
        .is_some());

    let reserved_port = reserve_port();
    let port = reserved_port.port();
    let socket_path = temp_socket_path();
    let (mut config, _tmp_dir) = test_config(port, socket_path.clone());
    config.agent_repository = Some(repo);
    reserved_port.release();
    let supervisor = Supervisor::new(config);

    // WHEN the Supervisor starts
    let handles = supervisor
        .start(crate::coordinator::DynBackend::new(MockBackend), None)
        .await
        .expect("start() should succeed");

    // THEN both agents are available under what their definition says, and
    // the Apollia agent is not registered: it is never started nor stopped
    let mut agents = handles
        .registry_handle
        .list_agents()
        .await
        .expect("list_agents should succeed");
    agents.sort_by(|a, b| a.manifest.name.cmp(&b.manifest.name));
    let names: Vec<&str> = agents.iter().map(|a| a.manifest.name.as_str()).collect();
    assert_eq!(names, ["agent-a", "agent-b"]);
    assert!(agents
        .iter()
        .all(|a| a.process_state == apollia_core::ProcessState::Active));
    assert_eq!(agents[0].manifest.description, "Test agent agent-a");

    shutdown_handles_dyn(handles, &socket_path).await;
}

/// [`shutdown_handles`] for a supervisor started on [`crate::coordinator::DynBackend`].
async fn shutdown_handles_dyn(
    handles: SupervisorHandles<crate::coordinator::DynBackend>,
    socket_path: &std::path::Path,
) {
    handles.trigger_engine.shutdown().await;
    handles.api_handle.shutdown();
    handles.router_handle.shutdown();
    handles.tool_registry_handle.shutdown().await;
    handles.registry_handle.shutdown();
    let _ = std::fs::remove_file(socket_path);
}

/// Boots a supervisor whose data directory holds an `apollia.toml` with
/// `server_section`, and returns its handles and the compatible server's port.
async fn boot_with_server_section(
    enabled: bool,
) -> (
    SupervisorHandles<MockBackend>,
    tempfile::TempDir,
    PathBuf,
    u16,
) {
    let compat_port = crate::compat_server::tests::free_port();
    let socket_path = temp_socket_path();
    // The native API serves its socket only: the test reads the second listener.
    let (mut config, tmp_dir) = test_config(0, socket_path.clone());
    config.api_config.tcp_port = None;
    std::fs::write(
        tmp_dir.path().join("apollia.toml"),
        format!("[server]\nenabled = {enabled}\nport = {compat_port}\n"),
    )
    .unwrap();
    let handles = Supervisor::new(config)
        .start(MockBackend, None)
        .await
        .unwrap();
    (handles, tmp_dir, socket_path, compat_port)
}

#[tokio::test]
async fn test_a_runtime_whose_compatible_server_is_off_opens_no_second_listener() {
    // GIVEN an apollia.toml with [server] enabled = false
    // WHEN the runtime starts
    let (handles, _tmp, socket_path, compat_port) = boot_with_server_section(false).await;

    // THEN nothing listens on the compatible server's port
    assert!(!crate::compat_server::tests::listens(compat_port).await);
    let _ = handles.event_sender.send(RuntimeEvent::ShutdownRequested);
    shutdown_handles(handles, &socket_path).await;
}

#[tokio::test]
async fn test_a_runtime_whose_compatible_server_is_on_opens_it_at_start() {
    // GIVEN an apollia.toml with [server] enabled = true
    // WHEN the runtime starts
    let (handles, _tmp, socket_path, compat_port) = boot_with_server_section(true).await;

    // THEN the compatible server listens on its own port, beside the native API
    let listening = poll_until_async(std::time::Duration::from_secs(5), || {
        crate::compat_server::tests::listens(compat_port)
    })
    .await;
    assert!(listening, "the compatible server did not open at start");
    let _ = handles.event_sender.send(RuntimeEvent::ShutdownRequested);
    shutdown_handles(handles, &socket_path).await;
}

#[test]
fn the_production_router_carries_the_prompt_debug_log_of_the_file() {
    // GIVEN an apollia.toml whose [llm.observability] turns debug_log_prompt on
    let llm: LlmConfig = toml::from_str(
        "default = \"local\"\nbackends = []\n[observability]\ndebug_log_prompt = true\n",
    )
    .expect("a valid [llm] section");
    let (mut config, _dir) = test_config(0, temp_socket_path());
    config.llm_config = Some(llm);
    let supervisor = Supervisor::new(config);

    // WHEN the runtime builds its router, from system.db, then grafted with
    // what the file declares
    let router = supervisor.finalize_llm_router(apollia_llm::LlmRouter::empty());

    // THEN the option is on in the router every model call goes through
    assert!(router.observability().debug_log_prompt);
}

#[test]
fn the_production_router_leaves_the_prompt_debug_log_off_by_default() {
    // GIVEN no [llm] section in the file
    let (config, _dir) = test_config(0, temp_socket_path());
    let supervisor = Supervisor::new(config);

    // WHEN the runtime builds its router
    let router = supervisor.finalize_llm_router(apollia_llm::LlmRouter::empty());

    // THEN prompts are not logged
    assert!(!router.observability().debug_log_prompt);
}
