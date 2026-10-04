//! A desktop agent run under a governance database that cannot be read: a
//! sensitive tool is refused, a disabled tool stays disabled, reading
//! continues. Task runs and chat-agent runs build their tools through the
//! same `BridgeRunner::build_tool_proxy`.

use std::path::Path;
use std::sync::Arc;

use apollia_aip::validator::validate_agent;
use apollia_runtime::governance::{FailClosed, GovernanceHandle, GovernanceSources};
use pyo3::types::PyModule;

use super::*;

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

/// The runner of the probe agent, with its tools registered and its folder
/// `data_dir/work`, where `seen.txt` waits to be read.
async fn probe_runner(data_dir: &Path, governance: GovernanceHandle) -> BridgeRunner {
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

    BridgeRunner {
        bridge,
        llm_router: None,
        event_bus,
        agent_id: "fail-closed-probe".to_string(),
        allowed_tools: manifest.tools_required.clone(),
        tool_registry: Some(registry),
        audit_trail: Some(audit),
        memory_namespace: None,
        memory_base_dir: data_dir.join("memory"),
        user_memory_write: false,
        pending_user_inputs: None,
        mcp_handle: None,
        governance: Some(governance),
        a2a_invoker: None,
        mailbox: None,
        tools_config: apollia_core::ToolsConfig::default().into(),
        datasources_declared: vec![],
        templates_declared: vec![],
        agent_dir: None,
        secrets_declared: vec![],
        budget: Arc::new(direct_path_budget(
            &manifest.step_budget.clone().unwrap_or_default(),
        )),
        manifest,
        cancel: CancellationToken::new(),
    }
}

// The proxy a desktop run executes its tools through answers to a governance
// that cannot read its database.
#[tokio::test(flavor = "multi_thread")]
async fn test_run_tools_refuse_sensitive_and_keep_disabled() {
    // GIVEN a governance.db that stopped being readable after disabling
    // `file_grep`, and the runner of the probe agent
    let dir = tempfile::tempdir().unwrap();
    let governance = governance_on_unreadable_store(dir.path()).await;
    let runner = probe_runner(dir.path(), governance).await;
    let task = AIPTask {
        task_id: "fail-closed-run".to_string(),
        ..AIPTask::default()
    };
    let budget_view = Arc::new(runner.budget.to_live_budget_view());

    // WHEN its proxy calls a writing, a disabled and a reading tool
    let proxy = runner.build_tool_proxy(&task, &budget_view).await.unwrap();
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
