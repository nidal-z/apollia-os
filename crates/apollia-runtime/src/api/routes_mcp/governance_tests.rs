//! The MCP server routes against connection governance: the admission before
//! an addition, and the journal of every addition, change and removal.

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use apollia_core::{AIPResult, AIPTask, TaskStatus};
use apollia_mcp::config::McpServerConfig;
use apollia_mcp::manager::McpClientManagerHandle;
use apollia_mcp::session::LoadingMode;
use apollia_mcp::McpServerRepository;
use apollia_tools::ToolRegistryHandle;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use tempfile::TempDir;

use super::{add_server, remove_server, restart_server, update_server_config};
use crate::api::server::AppState;
use crate::audit_journal::{AuditJournalHandle, JournalEntryKind};
use crate::coordinator::{DynBackend, ExecutionBackend};
use crate::eventbus::EventBus;
use crate::governance::{
    connection_run_id, AdmissionDecision, AdmissionRequest, ConnectionAdmission,
    ConnectionGovernance, PolicyReadError,
};
use crate::registry::AgentRegistry;
use crate::router::TaskRouterHandle;

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

/// Refuses every connection.
struct RefuseAll;

impl ConnectionAdmission for RefuseAll {
    fn read_decision(
        &self,
        _request: &AdmissionRequest,
    ) -> Result<AdmissionDecision, PolicyReadError> {
        Ok(AdmissionDecision::Refused {
            reason: "no new connection on this machine".into(),
        })
    }
}

/// Cannot be read.
struct Unreadable;

impl ConnectionAdmission for Unreadable {
    fn read_decision(
        &self,
        _request: &AdmissionRequest,
    ) -> Result<AdmissionDecision, PolicyReadError> {
        Err(PolicyReadError::Unreadable("policy.toml: invalid".into()))
    }
}

/// A server the mock MCP script answers for over stdio.
fn mock_server(name: &str) -> McpServerConfig {
    McpServerConfig {
        format_version: 1,
        name: name.to_string(),
        command: "python3".to_string(),
        args: vec![concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../apollia-mcp/tests/mock_mcp_server.py"
        )
        .to_string()],
        env: HashMap::new(),
        transport: "stdio".to_string(),
        url: None,
        init_timeout_secs: 10,
        call_timeout_secs: 10,
        max_response_bytes: 8 * 1024 * 1024,
        max_tools: 256,
        tags: vec![],
        tool_families: Default::default(),
        operator_families: Default::default(),
        origin: None,
    }
}

struct Harness {
    dir: TempDir,
    state: AppState<MockBackend>,
    manager: McpClientManagerHandle,
    journal: AuditJournalHandle,
}

/// An app state with a live MCP manager, `mcp.db` in a temporary directory,
/// and connection governance over `admission` and a journal.
async fn harness(admission: Arc<dyn ConnectionAdmission>) -> Harness {
    let dir = TempDir::new().unwrap();
    let repo = McpServerRepository::open(&dir.path().join("mcp.db")).unwrap();
    let journal = AuditJournalHandle::open(&dir.path().join("audit_journal.db"))
        .await
        .unwrap();
    let tool_registry = ToolRegistryHandle::start();
    let manager = McpClientManagerHandle::start(vec![], &tool_registry, None, LoadingMode::Eager)
        .await
        .unwrap();

    let (event_tx, _) = EventBus::new();
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
        tool_registry_handle: Some(tool_registry),
        audit_journal: Some(journal.clone()),
        connection_governance: ConnectionGovernance::new(admission, Some(journal.clone())),
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
        data_dir: dir.path().to_path_buf(),
        stt_engine: crate::api::server::empty_shared_stt_engine(),
        stt_repository: crate::api::server::empty_shared_stt_repository(),
        mcp_handle: Some(manager.clone()),
        mcp_server_repo: Some(Arc::new(std::sync::Mutex::new(repo))),
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
    Harness {
        dir,
        state,
        manager,
        journal,
    }
}

fn persisted(h: &Harness) -> Vec<McpServerConfig> {
    h.state
        .mcp_server_repo
        .as_ref()
        .unwrap()
        .lock()
        .unwrap()
        .list()
        .unwrap()
}

// A refused addition starts nothing and writes nothing: no session, no row in
// mcp.db, no mcp.toml, and nothing journaled as added.
#[tokio::test]
async fn test_refused_addition_writes_nothing() {
    // GIVEN a policy refusing every connection
    let h = harness(Arc::new(RefuseAll)).await;

    // WHEN a server is added through the route
    let (code, Json(body)) = add_server(State(h.state.clone()), Json(mock_server("calc")))
        .await
        .unwrap_err();

    // THEN it is forbidden with the policy's reason
    assert_eq!(code, StatusCode::FORBIDDEN);
    assert_eq!(body["error"], "no new connection on this machine");
    // AND no session started, mcp.db holds no server, no mcp.toml exists
    assert!(h.manager.status().await.is_empty());
    assert!(persisted(&h).is_empty());
    assert!(!h.dir.path().join("mcp.toml").exists());
    // AND nothing was journaled for the connection
    assert!(h
        .journal
        .query_run(&connection_run_id("mcp:calc"))
        .await
        .is_empty());
    h.manager.shutdown().await;
}

// A server whose npm package the policy denies is refused by the policy, and
// nothing is written.
#[tokio::test]
async fn test_denied_package_is_refused_by_the_policy_and_writes_nothing() {
    // GIVEN a deposited policy denying one npm package
    let admission = crate::governance::PolicyAdmission::new(
        crate::governance::policy_admission::valid_verdict_with_connections(
            "deny = [\"package:npm:denied-server\"]",
        ),
    );
    let h = harness(Arc::new(admission)).await;
    let mut config = mock_server("denied");
    config.command = "npx".into();
    config.args = vec!["-y".into(), "denied-server@1.0.0".into()];

    // WHEN the server running it is added through the route
    let (code, Json(body)) = add_server(State(h.state.clone()), Json(config))
        .await
        .unwrap_err();

    // THEN it is forbidden, the reason saying the policy refused it
    assert_eq!(code, StatusCode::FORBIDDEN);
    assert_eq!(body["error"], crate::governance::CONNECTION_DENIED_REASON);
    assert!(body["error"]
        .as_str()
        .unwrap()
        .starts_with(crate::governance::POLICY_REFUSAL_PREFIX));
    // AND nothing started, mcp.db holds no server, no mcp.toml exists
    assert!(h.manager.status().await.is_empty());
    assert!(persisted(&h).is_empty());
    assert!(!h.dir.path().join("mcp.toml").exists());
    h.manager.shutdown().await;
}

// A server the policy forbade at boot is not started again by a restart.
#[tokio::test]
async fn test_a_forbidden_server_is_not_restarted() {
    // GIVEN a policy allowing only Google accounts, under which the stored
    // server was forbidden at boot
    let admission = crate::governance::PolicyAdmission::new(
        crate::governance::policy_admission::valid_verdict_with_connections(
            "allow = [\"native:google\"]",
        ),
    );
    let forbidden = crate::governance::ForbiddenConnections::compute(
        &admission,
        &[mock_server("calc")],
        &[],
        &[],
    );
    let mut h = harness(Arc::new(admission)).await;
    h.state.connection_governance = h
        .state
        .connection_governance
        .clone()
        .with_forbidden(Arc::new(forbidden));
    h.state
        .mcp_server_repo
        .as_ref()
        .unwrap()
        .lock()
        .unwrap()
        .save(&mock_server("calc"))
        .unwrap();

    // WHEN it is restarted through the route
    let (code, Json(body)) = restart_server(State(h.state.clone()), Path("calc".to_string()))
        .await
        .unwrap_err();

    // THEN it is refused by the policy, no session starts, and it is kept
    assert_eq!(code, StatusCode::FORBIDDEN);
    assert_eq!(
        body["error"],
        crate::governance::FORBIDDEN_CONNECTION_REASON
    );
    assert!(h.manager.status().await.is_empty());
    assert_eq!(persisted(&h).len(), 1);
    h.manager.shutdown().await;
}

// A policy that cannot be read refuses, and writes nothing either.
#[tokio::test]
async fn test_unreadable_policy_refuses_the_addition() {
    // GIVEN a policy whose read fails
    let h = harness(Arc::new(Unreadable)).await;

    // WHEN a server is added through the route
    let (code, Json(body)) = add_server(State(h.state.clone()), Json(mock_server("calc")))
        .await
        .unwrap_err();

    // THEN it is forbidden, naming the read failure, and nothing is persisted
    assert_eq!(code, StatusCode::FORBIDDEN);
    assert!(body["error"].as_str().unwrap().contains("policy.toml"));
    assert!(persisted(&h).is_empty());
    assert!(h.manager.status().await.is_empty());
    h.manager.shutdown().await;
}

// Adding, changing then removing a server through the routes journals the
// three events on the connection's own run, in order.
#[tokio::test]
async fn test_add_modify_remove_are_journaled() {
    // GIVEN no policy deposited
    let h = harness(Arc::new(crate::governance::NoPolicyAdmission)).await;

    // WHEN a server is added, its configuration changed, then it is removed
    let (code, _) = add_server(State(h.state.clone()), Json(mock_server("calc")))
        .await
        .expect("add");
    assert_eq!(code, StatusCode::CREATED);
    let Json(modified) = update_server_config(
        State(h.state.clone()),
        Path("calc".to_string()),
        Json(serde_json::json!({ "call_timeout_secs": 20 })),
    )
    .await
    .expect("modify");
    assert_eq!(modified.name, "calc");
    let Json(removed) = remove_server(State(h.state.clone()), Path("calc".to_string()))
        .await
        .expect("remove");
    assert_eq!(removed["removed"], "calc");

    // THEN the connection run holds added, modified, removed
    let entries = h.journal.query_run(&connection_run_id("mcp:calc")).await;
    let kinds: Vec<JournalEntryKind> = entries.iter().map(|e| e.kind.clone()).collect();
    assert_eq!(
        kinds,
        vec![
            JournalEntryKind::ConnectionAdded,
            JournalEntryKind::ConnectionModified,
            JournalEntryKind::ConnectionRemoved,
        ]
    );
    // AND the addition says what was added: an MCP server, local, custom
    assert_eq!(entries[0].payload["kind"], "mcp");
    assert_eq!(entries[0].payload["remote"], false);
    assert_eq!(entries[0].payload["provenance"], "custom");
    assert_eq!(entries[0].payload["actor"], "api");
    // AND the chain verifies
    let report = h
        .journal
        .verify_chain(&connection_run_id("mcp:calc"))
        .await
        .expect("verify");
    assert!(report.ok);
    h.manager.shutdown().await;
}

// A server in pause is not started on request, by a restart or by a change
// of its configuration.
#[tokio::test]
async fn test_a_paused_server_is_not_started_on_request() {
    // GIVEN a stored server the user put in pause
    let h = harness(Arc::new(crate::governance::NoPolicyAdmission)).await;
    {
        let repo = h.state.mcp_server_repo.as_ref().unwrap().lock().unwrap();
        repo.save(&mock_server("calc")).unwrap();
        repo.set_enabled("calc", false).unwrap();
    }

    // WHEN it is restarted, then its configuration changed, through the routes
    let (restart, Json(body)) = restart_server(State(h.state.clone()), Path("calc".to_string()))
        .await
        .unwrap_err();
    let (update, _) = update_server_config(
        State(h.state.clone()),
        Path("calc".to_string()),
        Json(serde_json::json!({"tags": ["x"]})),
    )
    .await
    .unwrap_err();

    // THEN both are refused as paused, no session starts, and it is kept
    assert_eq!(restart, StatusCode::CONFLICT);
    assert_eq!(body["error"], super::PAUSED_SERVER_REASON);
    assert_eq!(update, StatusCode::CONFLICT);
    assert!(h.manager.status().await.is_empty());
    assert_eq!(persisted(&h).len(), 1);
    h.manager.shutdown().await;
}
