//! End-to-end integration tests - distribution of the bundled agents.
//!
//! Validates the full distribution flow through SupervisorConfig::bundled_agents_path:
//! the bundled agents are installed automatically at the first boot, and the
//! install is idempotent (an agent already in the DB is not reinstalled).
//!
//! No Python required - uses StubAgentLoader and InstantBackend.

use apollia_e2e_tests::reserve_port;
use std::path::PathBuf;
use std::sync::Arc;

use apollia_core::AgentManifest;
use apollia_runtime::{
    api::{routes_agents::StubAgentLoader, APIServerConfig},
    coordinator::{DynBackend, ExecutionBackend},
    supervisor::{Supervisor, SupervisorConfig},
};
use apollia_tools::{AgentRepository, InstalledAgent};
use tempfile::TempDir;

// ─── Test backend ─────────────────────────────────────────────────────────────

/// Backend that completes instantly, without Python.
#[derive(Clone)]
struct InstantBackend;

impl From<DynBackend> for InstantBackend {
    fn from(_: DynBackend) -> Self {
        InstantBackend
    }
}

impl ExecutionBackend for InstantBackend {
    fn execute(
        &self,
        task: apollia_core::AIPTask,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<apollia_core::AIPResult, String>> + Send>,
    > {
        Box::pin(async move {
            Ok(apollia_core::AIPResult {
                task_id: task.task_id,
                status: apollia_core::TaskStatus::Completed,
                output: vec![],
                error: None,
                artifacts: vec![],
                input_required_data: None,
            })
        })
    }
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

/// Writes `bundled/manifest.json` with 4 agents into the temporary directory.
///
/// Returns the path of the `bundled/` directory it created.
fn write_bundled_manifest(tmp: &TempDir) -> PathBuf {
    let bundled_dir = tmp.path().join("bundled");
    std::fs::create_dir_all(&bundled_dir).expect("create bundled dir");

    let manifest = serde_json::json!({
        "version": "1.0.0",
        "bundled_agents": [
            { "name": "excel-worker",    "file": "excel-worker.py",    "auto_install": true, "description": "Excel" },
            { "name": "csv-data-worker", "file": "csv-data-worker.py", "auto_install": true, "description": "CSV"   },
            { "name": "pdf-worker",      "file": "pdf-worker.py",      "auto_install": true, "description": "PDF"   },
            { "name": "code-worker",     "file": "code-worker.py",     "auto_install": true, "description": "Code"  }
        ]
    });
    std::fs::write(
        bundled_dir.join("manifest.json"),
        serde_json::to_string_pretty(&manifest).expect("serialize manifest"),
    )
    .expect("write manifest.json");

    bundled_dir
}

/// Builds a minimal `InstalledAgent` with the given timestamp.
fn pre_installed_agent(name: &str, installed_at: &str) -> InstalledAgent {
    let manifest = AgentManifest {
        format_version: 1,
        name: name.to_string(),
        version: "1.0.0".to_string(),
        description: format!("Pre-installed {name}"),
        tools_required: vec![],
        tools_optional: vec![],
        supports_streaming: false,
        supports_a2a: false,
        supports_mailbox: false,
        mailbox_allowlist: None,
        memory_namespace: None,
        shared_memory_namespaces: vec![],
        max_concurrent_tasks: 1,
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
    };
    InstalledAgent {
        name: name.to_string(),
        version: "1.0.0".to_string(),
        install_path: PathBuf::from(format!("/tmp/{name}.py")),
        source_path: PathBuf::from(format!("/tmp/{name}.py")),
        manifest,
        enabled: true,
        installed_at: installed_at.to_string(),
        updated_at: installed_at.to_string(),
    }
}

/// Builds a `SupervisorConfig` pointing at the temporary directory.
fn supervisor_config(
    tmp: &TempDir,
    tcp_port: u16,
    repo: Option<AgentRepository>,
    bundled_agents_path: Option<PathBuf>,
) -> SupervisorConfig {
    let socket_id = &uuid::Uuid::new_v4().to_string()[..8];
    SupervisorConfig {
        api_config: APIServerConfig {
            socket_path: PathBuf::from(format!("/tmp/ap-dist-{socket_id}.sock")),
            tcp_port: Some(tcp_port),
            bind_addr: "127.0.0.1".to_string(),
            api_token: None,
            tls_cert_path: None,
            tls_key_path: None,
        },
        startup_timeout_secs: 10,
        llm_config: None,
        config_path: None,
        runtime_config: apollia_core::RuntimeConfig::default(),
        hitl_config: apollia_core::HitlConfig::default(),
        data_dir: tmp.path().to_path_buf(),
        obs_config: apollia_core::ObservabilityConfig::default(),
        agent_repository: repo,
        bundled_agents_path,
        package_repository: None,
        tools_config: apollia_core::ToolsConfig::default(),
        mcp_loading: apollia_mcp::session::LoadingMode::Eager,
        tool_search_limit: 20,
        hooks_config: apollia_core::HooksConfig::default(),
        plan_mode_default: false,
        chat_default_workspace: None,
        filesystem_trusted_paths: Vec::new(),
        chat_tool_turn_temperature: None,
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

/// At the first boot, the 4 bundled agents are installed then registered.
#[tokio::test]
async fn test_bundled_agents_auto_installed() {
    // GIVEN a manifest.json with 4 bundled agents and an empty DB
    let tmp = TempDir::new().expect("create temp dir");
    let db_path = tmp.path().join("agents.db");
    let bundled_dir = write_bundled_manifest(&tmp);

    let repo = AgentRepository::open(&db_path).expect("open repo");
    let reserved_port = reserve_port();
    let port = reserved_port.port();
    let config = supervisor_config(&tmp, port, Some(repo), Some(bundled_dir));
    let supervisor = Supervisor::new(config);

    // WHEN the Supervisor starts with bundled_agents_path configured
    // Release the probe listener only now, right before the bind it protects.
    reserved_port.release();
    let handles = supervisor
        .start::<InstantBackend>(InstantBackend, Arc::new(StubAgentLoader), None, None)
        .await
        .expect("supervisor start");

    // THEN 4 agents are registered in the AgentRegistry, in the Active state
    let agents = handles
        .registry_handle
        .list_agents()
        .await
        .expect("list agents");

    assert_eq!(agents.len(), 4, "the 4 bundled agents must be installed");

    let names: Vec<String> = agents.iter().map(|a| a.manifest.name.clone()).collect();
    for expected in &[
        "excel-worker",
        "csv-data-worker",
        "pdf-worker",
        "code-worker",
    ] {
        assert!(
            names.contains(&expected.to_string()),
            "the agent '{}' must be in the registry, found: {:?}",
            expected,
            names
        );
    }

    handles.api_handle.shutdown();
}

/// An agent already installed in the DB is not reinstalled on a second boot.
#[tokio::test]
async fn test_bundled_agents_skip_existing() {
    // GIVEN excel-worker already in the DB with a sentinel timestamp
    let tmp = TempDir::new().expect("create temp dir");
    let db_path = tmp.path().join("agents.db");
    let bundled_dir = write_bundled_manifest(&tmp);

    let sentinel = "2026-01-01T00:00:00Z";
    {
        let repo = AgentRepository::open(&db_path).expect("open repo for pre-install");
        repo.save(&pre_installed_agent("excel-worker", sentinel))
            .expect("pre-install excel-worker");
    }

    let repo2 = AgentRepository::open(&db_path).expect("reopen repo");
    let reserved_port = reserve_port();
    let port = reserved_port.port();
    let config = supervisor_config(&tmp, port, Some(repo2), Some(bundled_dir));
    let supervisor = Supervisor::new(config);

    // WHEN the Supervisor starts (excel-worker in the DB, the 3 others absent)
    // Release the probe listener only now, right before the bind it protects.
    reserved_port.release();
    let handles = supervisor
        .start::<InstantBackend>(InstantBackend, Arc::new(StubAgentLoader), None, None)
        .await
        .expect("supervisor start");

    // THEN the registry holds exactly 4 agents, with no duplicate
    let agents = handles
        .registry_handle
        .list_agents()
        .await
        .expect("list agents");

    assert_eq!(
        agents.len(),
        4,
        "4 agents in total - excel-worker not duplicated, found: {:?}",
        agents.iter().map(|a| &a.manifest.name).collect::<Vec<_>>()
    );

    let excel_count = agents
        .iter()
        .filter(|a| a.manifest.name == "excel-worker")
        .count();
    assert_eq!(excel_count, 1, "excel-worker must appear exactly once");

    // AND the install timestamp of excel-worker is unchanged
    let repo3 = AgentRepository::open(&db_path).expect("recheck repo");
    let excel = repo3
        .get("excel-worker")
        .expect("get excel-worker")
        .expect("excel-worker must exist in DB");
    assert_eq!(
        excel.installed_at, sentinel,
        "installed_at must not change when the agent was already in the DB"
    );

    handles.api_handle.shutdown();
}
