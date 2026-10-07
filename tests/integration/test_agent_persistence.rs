//! Integration tests - agent persistence E2E.
//!
//! Validates the complete cycle: store → boot → available again, using a real
//! `AgentRepository` (SQLite) and the `Supervisor` auto-load. An agent is a
//! definition: nothing is imported, so no Python is involved.
//!
//! stored agent → available at boot, under what its definition says
//! disabled agent not available at boot
//! uninstalled agent gone at the next boot
//! a newer definition is the one the next boot registers

use apollia_e2e_tests::reserve_port;
use std::path::PathBuf;

use apollia_runtime::coordinator::{DynBackend, ExecutionBackend};
use apollia_runtime::{
    api::APIServerConfig,
    supervisor::{Supervisor, SupervisorConfig, SupervisorHandles},
};
use apollia_tools::{AgentOrigin, AgentRepository, StoredAgent};
use tempfile::TempDir;

// ─────────────────────────────────────────────────────────────────────────────
// Mock backend - completes instantly
// ─────────────────────────────────────────────────────────────────────────────

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

// ─────────────────────────────────────────────────────────────────────────────
// Test helpers
// ─────────────────────────────────────────────────────────────────────────────

/// A stored agent named `name` at `version`, as a package leaves it.
fn stored_agent(name: &str, version: &str) -> StoredAgent {
    StoredAgent {
        name: name.to_string(),
        origin: AgentOrigin::Package,
        package: None,
        version: version.to_string(),
        definition: serde_json::json!({
            "id": name, "name": name, "mission": format!("Mission of {name} {version}"),
            "defaults": {},
        }),
        enabled: true,
        installed_at: "2026-03-17T10:00:00Z".to_string(),
        updated_at: "2026-03-17T10:00:00Z".to_string(),
    }
}

/// Build a `SupervisorConfig` pointing at the given temp directory.
fn test_supervisor_config(
    tmp_dir: &TempDir,
    tcp_port: u16,
    agent_repository: Option<AgentRepository>,
) -> SupervisorConfig {
    let socket_id = &uuid::Uuid::new_v4().to_string()[..8];
    SupervisorConfig {
        api_config: APIServerConfig {
            socket_path: PathBuf::from(format!("/tmp/ap-persist-{socket_id}.sock")),
            tcp_port: Some(tcp_port),
            tcp_listener: None,
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
        data_dir: tmp_dir.path().to_path_buf(),
        obs_config: apollia_core::ObservabilityConfig::default(),
        agent_repository,
        package_repository: None,
        tools_config: apollia_core::ToolsConfig::default(),
        mcp_loading: apollia_mcp::session::LoadingMode::Eager,
        tool_search_limit: 20,
        hooks_config: apollia_core::HooksConfig::default(),
        plan_mode_default: false,
        chat_tool_turn_temperature: None,
        workflows_config: apollia_core::WorkflowsConfig::default(),
        config_channels: None,
    }
}

/// Boots a supervisor over the `agents.db` of `tmp`.
async fn boot(tmp: &TempDir) -> SupervisorHandles<InstantBackend> {
    let repo = AgentRepository::open(&tmp.path().join("agents.db")).expect("reopen repo");
    let reserved_port = reserve_port();
    let config = test_supervisor_config(tmp, reserved_port.port(), Some(repo));
    let supervisor = Supervisor::new(config);
    // Release the probe listener only now, right before the bind it protects.
    reserved_port.release();
    supervisor
        .start::<InstantBackend>(InstantBackend, None)
        .await
        .expect("supervisor start")
}

/// The names of the registered agents, sorted.
async fn registered(handles: &SupervisorHandles<InstantBackend>) -> Vec<String> {
    let mut names: Vec<String> = handles
        .registry_handle
        .list_agents()
        .await
        .expect("list agents")
        .into_iter()
        .map(|entry| entry.manifest.name)
        .collect();
    names.sort();
    names
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

/// A stored agent is available again after a restart.
#[tokio::test]
async fn test_stored_agent_is_available_after_a_restart() {
    // GIVEN a store holding one agent, written then closed
    let tmp = TempDir::new().expect("create temp dir");
    let repo = AgentRepository::open(&tmp.path().join("agents.db")).expect("open repo");
    repo.save_definition(&stored_agent("reload-agent", "1.0.0"))
        .expect("store");
    drop(repo);

    // WHEN the Supervisor starts over the same store
    let handles = boot(&tmp).await;

    // THEN the agent is registered under what its definition says, and the
    // Apollia agent the store always holds is not
    let agents = handles
        .registry_handle
        .list_agents()
        .await
        .expect("list agents");
    assert_eq!(agents.len(), 1, "expected 1 registered agent");
    assert_eq!(agents[0].manifest.name, "reload-agent");
    assert_eq!(agents[0].manifest.version, "1.0.0");
    assert_eq!(
        agents[0].manifest.description,
        "Mission of reload-agent 1.0.0"
    );

    handles.api_handle.shutdown();
}

/// A disabled agent is not available at boot.
#[tokio::test]
async fn test_disabled_agent_not_available_at_boot() {
    // GIVEN two stored agents, one of them disabled
    let tmp = TempDir::new().expect("create temp dir");
    let repo = AgentRepository::open(&tmp.path().join("agents.db")).expect("open repo");
    repo.save_definition(&stored_agent("awake", "1.0.0"))
        .expect("store");
    repo.save_definition(&stored_agent("asleep", "1.0.0"))
        .expect("store");
    repo.set_enabled("asleep", false).expect("disable");
    drop(repo);

    // WHEN the Supervisor starts
    let handles = boot(&tmp).await;

    // THEN the enabled agent alone is registered
    assert_eq!(registered(&handles).await, ["awake"]);

    handles.api_handle.shutdown();
}

/// An uninstalled agent is gone at the next boot.
#[tokio::test]
async fn test_uninstalled_agent_is_gone_at_the_next_boot() {
    // GIVEN two stored agents, one of them deleted with its override
    let tmp = TempDir::new().expect("create temp dir");
    let repo = AgentRepository::open(&tmp.path().join("agents.db")).expect("open repo");
    repo.save_definition(&stored_agent("kept", "1.0.0"))
        .expect("store");
    repo.save_definition(&stored_agent("removed", "1.0.0"))
        .expect("store");
    repo.set_override(
        "removed",
        apollia_tools::OverrideField::Mission,
        &serde_json::json!("Mine."),
    )
    .expect("override");
    repo.delete("removed").expect("delete");
    assert!(repo.stored("removed").expect("stored").is_none());
    assert!(repo.overrides("removed").expect("overrides").is_empty());
    drop(repo);

    // WHEN the Supervisor starts
    let handles = boot(&tmp).await;

    // THEN only the agent that was kept is registered
    assert_eq!(registered(&handles).await, ["kept"]);

    handles.api_handle.shutdown();
}

/// A newer definition is the one the next boot registers.
#[tokio::test]
async fn test_a_newer_definition_is_the_one_registered_at_the_next_boot() {
    // GIVEN an agent stored at 1.0.0, then stored again at 2.0.0 as an
    // update of its package does
    let tmp = TempDir::new().expect("create temp dir");
    let repo = AgentRepository::open(&tmp.path().join("agents.db")).expect("open repo");
    repo.save_definition(&stored_agent("update-agent", "1.0.0"))
        .expect("store v1");
    repo.save_definition(&stored_agent("update-agent", "2.0.0"))
        .expect("store v2");
    drop(repo);

    // WHEN the Supervisor starts
    let handles = boot(&tmp).await;

    // THEN one agent is registered, at the newer version and mission
    let agents = handles
        .registry_handle
        .list_agents()
        .await
        .expect("list agents");
    assert_eq!(agents.len(), 1);
    assert_eq!(agents[0].manifest.version, "2.0.0");
    assert_eq!(
        agents[0].manifest.description,
        "Mission of update-agent 2.0.0"
    );

    handles.api_handle.shutdown();
}
