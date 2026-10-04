//! An agent run of the daemon under a governance database that cannot be
//! read: a sensitive tool is refused, a disabled tool stays disabled, reading
//! continues. The direct path (`call_run`) and the orchestrated plan steps
//! build their tools through the same `BridgeRunner::build_tool_proxy`; a
//! chat-agent turn through `AIPChatAgentRunner::build_tool_proxy`.

use std::path::Path;
use std::sync::Arc;

use apollia_aip::bridge::AIPBridge;
use apollia_aip::validator::validate_agent;
use apollia_core::AIPTask;
use apollia_runtime::governance::{FailClosed, GovernanceHandle, GovernanceSources};
use pyo3::prelude::*;
use pyo3::types::PyModule;

use super::chat_runner::AIPChatAgentRunner;
use super::engine::{direct_path_budget, AIPProductionBackend};
use super::runner::BridgeRunner;

/// An agent that declares the three tools the test calls, on the folder
/// `__FOLDER__`; its dispatch is never reached, the tools are called through
/// the run's proxy.
const PROBE: &str = r#"
class Probe:
    __apollia_manifest__ = {
        "name": "fail-closed-probe",
        "version": "0.1.0",
        "description": "Declares one tool of each kind",
        "tools_required": [
            "conn:folder:__FOLDER__/file_write",
            "conn:folder:__FOLDER__/file_grep",
            "conn:folder:__FOLDER__/file_read",
        ],
    }

    async def __apollia_dispatch__(self, task, ctx):
        return {}

agent = Probe()
"#;

/// The governance actor over `data_dir`, started on a readable database that
/// disables `file_grep`, then left with a database it can no longer read.
async fn governance_on_unreadable_store(data_dir: &Path) -> GovernanceHandle {
    let db = data_dir.join(apollia_tools::GOVERNANCE_DB_FILENAME);
    apollia_tools::GovernanceDb::open(data_dir).unwrap();
    apollia_tools::NativeToolRegistry::new(&db)
        .unwrap()
        .set_enabled("file_grep", false)
        .unwrap();
    let governance = GovernanceHandle::spawn(GovernanceSources {
        governance_db: Some(db.clone()),
        config_path: None,
        policy: Arc::new(apollia_permissions::PolicyVerdict::Absent),
        journal: None,
        events: None,
        forbidden_connections: Default::default(),
    });
    for suffix in ["-wal", "-shm"] {
        let mut side = db.clone().into_os_string();
        side.push(suffix);
        let _ = std::fs::remove_file(std::path::PathBuf::from(side));
    }
    std::fs::write(&db, vec![0x5a_u8; 4096]).unwrap();
    governance.refresh().await;
    assert!(matches!(
        governance.state().fail_closed,
        Some(FailClosed::StoreUnreadable { .. })
    ));
    governance
}

/// The production backend of the probe agent, with its tools registered and
/// its folder `data_dir/work`, where `seen.txt` waits to be read.
async fn probe_backend(data_dir: &Path, governance: GovernanceHandle) -> AIPProductionBackend {
    let work = data_dir.join("work");
    std::fs::create_dir_all(&work).unwrap();
    std::fs::write(work.join("seen.txt"), "seen").unwrap();
    let folder = apollia_core::FolderConnectionStore::open(
        &apollia_core::paths::DataFile::System.path(data_dir),
    )
    .unwrap()
    .add(&work.to_string_lossy(), None)
    .unwrap();
    let py_agent: Py<PyAny> = Python::with_gil(|py| {
        let code = std::ffi::CString::new(PROBE.replace("__FOLDER__", &folder.id)).unwrap();
        PyModule::from_code(py, &code, c"fail_closed_probe.py", c"fail_closed_probe")
            .unwrap()
            .getattr("agent")
            .unwrap()
            .into()
    });
    let validated = validate_agent(&py_agent).unwrap();
    let manifest = validated.manifest.clone();
    let bridge = Arc::new(AIPBridge::new(validated).unwrap());

    let registry = apollia_tools::ToolRegistryHandle::start();
    for descriptor in apollia_tools::tools::native_descriptors() {
        registry.register(descriptor).await.unwrap();
    }
    let audit = apollia_tools::AuditTrailHandle::open(&data_dir.join("audit.db"))
        .await
        .unwrap();
    let (event_bus, _events) = apollia_runtime::eventbus::EventBus::new();

    AIPProductionBackend {
        bridge,
        agent_id: "fail-closed-probe".to_string(),
        access: apollia_tools::GrantedAccess::new(None, &manifest),
        manifest,
        llm_router: None,
        event_bus,
        pending_approvals: None,
        plan_gates: None,
        plan_cache: None,
        task_repository: None,
        tool_registry: Some(registry),
        audit_trail: Some(audit),
        memory_namespace: None,
        memory_base_dir: data_dir.join("memory"),
        user_memory_write: false,
        a2a_invoker: None,
        tools_config: apollia_core::ToolsConfig::default().into(),
        datasources_declared: vec![],
        templates_declared: vec![],
        agent_dir: None,
        secrets_declared: vec![],
        secrets_data_dir: Some(data_dir.to_path_buf()),
        mcp_handle: None,
        governance: Some(governance),
    }
}

// The proxy a run executes its tools through, on the direct path and for
// the orchestrated plan steps, answers to a governance that cannot read its
// database.
#[tokio::test(flavor = "multi_thread")]
async fn test_run_tools_refuse_sensitive_and_keep_disabled() {
    // GIVEN a governance.db that stopped being readable after disabling
    // `file_grep`, and the runner of the probe agent
    let dir = tempfile::tempdir().unwrap();
    let governance = governance_on_unreadable_store(dir.path()).await;
    let backend = probe_backend(dir.path(), governance).await;
    let runner = BridgeRunner {
        bridge: Arc::clone(&backend.bridge),
        llm_router: None,
        event_bus: backend.event_bus.clone(),
        agent_id: backend.agent_id.clone(),
        manifest: backend.manifest.clone(),
        allowed_tools: backend.access.current(),
        tool_registry: backend.tool_registry.clone(),
        audit_trail: backend.audit_trail.clone(),
        memory_namespace: None,
        memory_base_dir: backend.memory_base_dir.clone(),
        a2a_invoker: None,
        tools_config: backend.tools_config.clone(),
        user_memory_write: false,
        datasources_declared: vec![],
        templates_declared: vec![],
        agent_dir: None,
        secrets_declared: vec![],
        secrets_data_dir: backend.secrets_data_dir.clone(),
        mcp_handle: None,
        governance: backend.governance.clone(),
        budget: Arc::new(direct_path_budget(
            &apollia_core::StepBudgetConfig::default(),
        )),
        cancel: apollia_runtime::coordinator::CancellationToken::new(),
    };
    let task = AIPTask {
        task_id: "fail-closed-run".to_string(),
        ..AIPTask::default()
    };

    // WHEN its proxy calls a writing, a disabled and a reading tool
    let proxy = runner.build_tool_proxy(&task).await.unwrap();
    let write = proxy
        .invoke_native(
            "file_write",
            serde_json::json!({"path": "note.txt", "content": "x"}),
        )
        .await;
    let grep = proxy
        .invoke_native("file_grep", serde_json::json!({"pattern": "seen"}))
        .await;
    let read = proxy
        .invoke_native("file_read", serde_json::json!({"path": "seen.txt"}))
        .await;

    // THEN the write is refused as fail-closed, the disabled tool does not
    // run, and the read runs
    let refusal = write.unwrap_err().to_string();
    assert!(refusal.contains("governance fails closed"), "{refusal}");
    assert!(grep.is_err(), "{grep:?}");
    assert!(read.is_ok(), "{read:?}");
    assert!(!dir.path().join("work/note.txt").exists());
}

// The proxy of a chat-agent turn answers to the same governance.
#[tokio::test(flavor = "multi_thread")]
async fn test_chat_agent_tools_refuse_sensitive_and_keep_disabled() {
    // GIVEN a governance.db that stopped being readable after disabling
    // `file_grep`, and the chat-agent runner of the daemon
    let dir = tempfile::tempdir().unwrap();
    let governance = governance_on_unreadable_store(dir.path()).await;
    let backend = probe_backend(dir.path(), governance.clone()).await;
    let runner = AIPChatAgentRunner {
        event_bus: Arc::new({
            let lock = std::sync::OnceLock::new();
            let _ = lock.set(backend.event_bus.clone());
            lock
        }),
        llm_router: Arc::new(std::sync::OnceLock::new()),
        tool_registry: Arc::new({
            let lock = std::sync::OnceLock::new();
            let _ = lock.set(backend.tool_registry.clone().unwrap());
            lock
        }),
        audit_trail: Arc::new({
            let lock = std::sync::OnceLock::new();
            let _ = lock.set(backend.audit_trail.clone().unwrap());
            lock
        }),
        pending_user_inputs: Arc::new(std::sync::OnceLock::new()),
        agent_registry: Arc::new(std::sync::OnceLock::new()),
        task_router: Arc::new(std::sync::OnceLock::new()),
        data_dir: dir.path().to_path_buf(),
        tools_config: apollia_core::ToolsConfig::default().into(),
        mcp_handle: Arc::new(std::sync::OnceLock::new()),
        governance: Arc::new({
            let lock = std::sync::OnceLock::new();
            let _ = lock.set(governance);
            lock
        }),
    };
    let task = AIPTask {
        task_id: "fail-closed-chat".to_string(),
        ..AIPTask::default()
    };

    // WHEN the turn's proxy calls a writing, a disabled and a reading tool
    let proxy = runner
        .build_tool_proxy("fail-closed-probe", &backend.manifest, &task, None)
        .await
        .unwrap();
    let write = proxy
        .invoke_native(
            "file_write",
            serde_json::json!({"path": "note.txt", "content": "x"}),
        )
        .await;
    let grep = proxy
        .invoke_native("file_grep", serde_json::json!({"pattern": "seen"}))
        .await;
    let read = proxy
        .invoke_native("file_read", serde_json::json!({"path": "seen.txt"}))
        .await;

    // THEN the write is refused as fail-closed, the disabled tool does not
    // run, and the read runs
    let refusal = write.unwrap_err().to_string();
    assert!(refusal.contains("governance fails closed"), "{refusal}");
    assert!(grep.is_err(), "{grep:?}");
    assert!(read.is_ok(), "{read:?}");
    assert!(!dir.path().join("work/note.txt").exists());
}
