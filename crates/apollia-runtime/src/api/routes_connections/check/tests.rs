use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use apollia_connectors::imap::{ConnectionProbe, ImapAccount, ImapError};
use apollia_core::{
    AIPResult, AIPTask, ConnectionReasonKey, ConnectionStateStore, ConnectionStatus, RuntimeEvent,
    TaskStatus,
};
use apollia_mcp::config::McpServerConfig;
use apollia_mcp::manager::McpClientManagerHandle;
use apollia_mcp::session::LoadingMode;
use apollia_mcp::McpServerRepository;
use apollia_tools::ToolRegistryHandle;
use axum::http::StatusCode;
use tempfile::TempDir;

use super::{check_connection_with, CheckConnectionBody, Reach};
use crate::api::server::AppState;
use crate::coordinator::{DynBackend, ExecutionBackend};
use crate::eventbus::EventBus;
use crate::governance::{
    AdmissionDecision, AdmissionRequest, ConnectionAdmission, ConnectionGovernance,
    ForbiddenConnections, PolicyReadError,
};
use crate::registry::AgentRegistry;
use crate::router::TaskRouterHandle;
use crate::EventBusReceiver;

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

/// No mailbox is checked by these tests.
struct NoMailbox;

impl ConnectionProbe for NoMailbox {
    async fn verify(&self, _account: &ImapAccount, _password: &str) -> Result<(), ImapError> {
        Err(ImapError::CredentialsRejected)
    }
}

/// A mailbox server that accepts every login, and the server of each check
/// sent to it.
#[derive(Default)]
struct RecordingMailServer {
    reached: std::sync::Mutex<Vec<String>>,
}

impl ConnectionProbe for RecordingMailServer {
    async fn verify(&self, account: &ImapAccount, _password: &str) -> Result<(), ImapError> {
        self.reached.lock().unwrap().push(account.host.clone());
        Ok(())
    }
}

/// An organisation policy that refuses every connection.
struct RefuseAll;

impl ConnectionAdmission for RefuseAll {
    fn read_decision(
        &self,
        _request: &AdmissionRequest,
    ) -> Result<AdmissionDecision, PolicyReadError> {
        Ok(AdmissionDecision::Refused {
            reason: "no connection on this machine".into(),
        })
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
    events: EventBusReceiver,
}

/// An app state over a temporary directory, with an MCP manager that runs no
/// session, and `servers` declared in `mcp.db`.
async fn harness(servers: &[McpServerConfig]) -> Harness {
    let dir = TempDir::new().unwrap();
    let repo = McpServerRepository::open(&dir.path().join("mcp.db")).unwrap();
    for server in servers {
        repo.save(server).unwrap();
    }
    let tool_registry = ToolRegistryHandle::start();
    let manager = McpClientManagerHandle::start(vec![], &tool_registry, None, LoadingMode::Eager)
        .await
        .unwrap();
    let (event_tx, _) = EventBus::new();
    let events = event_tx.subscribe();
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
        audit_journal: None,
        connection_governance: ConnectionGovernance::without_policy(None),
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
        events,
    }
}

async fn check(h: &Harness, id: &str) -> Result<crate::connections::Connection, StatusCode> {
    check_connection_with(
        &h.state,
        Vec::new(),
        &Reach::<NoMailbox> {
            auth: None,
            imap: None,
        },
        id,
        CheckConnectionBody::default(),
    )
    .await
    .map_err(|(status, _)| status)
}

/// The next `ConnectionStateChanged` the bus carries, for `id`.
async fn announced(events: &mut EventBusReceiver, id: &str) -> ConnectionStatus {
    loop {
        match events.recv().await.unwrap() {
            RuntimeEvent::ConnectionStateChanged {
                id: announced,
                state,
                ..
            } if announced == id => return state,
            _ => continue,
        }
    }
}

#[tokio::test]
async fn test_an_mcp_server_without_a_session_is_started_then_tested() {
    // GIVEN a server declared in mcp.db with no live session
    let mut h = harness(&[mock_server("mock")]).await;
    assert!(h.manager.status().await.is_empty());

    // WHEN it is checked
    let connection = check(&h, "mcp:mock").await.unwrap();

    // THEN its session runs, it works, dated, and that state is kept and
    // announced
    assert!(h.manager.status().await.iter().any(|s| s.name == "mock"));
    assert_eq!(connection.status, ConnectionStatus::Working);
    assert!(connection.reason.is_none());
    assert!(connection.checked_at.is_some());
    let store =
        ConnectionStateStore::open(&apollia_core::paths::DataFile::System.path(h.dir.path()))
            .unwrap();
    assert_eq!(
        store.get("mcp:mock").unwrap().unwrap().state,
        ConnectionStatus::Working
    );
    assert_eq!(
        announced(&mut h.events, "mcp:mock").await,
        ConnectionStatus::Working
    );
    h.manager.shutdown().await;
}

#[tokio::test]
async fn test_an_mcp_server_that_cannot_start_needs_repair() {
    // GIVEN a server whose command does not exist
    let mut broken = mock_server("broken");
    broken.command = "apollia-no-such-command".to_owned();
    broken.args.clear();
    let h = harness(&[broken]).await;

    // WHEN it is checked
    let connection = check(&h, "mcp:broken").await.unwrap();

    // THEN it needs repair, unreachable
    assert_eq!(connection.status, ConnectionStatus::NeedsRepair);
    assert_eq!(
        connection.reason.map(|r| r.key),
        Some(ConnectionReasonKey::Unreachable)
    );
    h.manager.shutdown().await;
}

#[tokio::test]
async fn test_a_moved_folder_needs_repair_and_reads_so_in_the_list() {
    // GIVEN a folder added as a connection, then moved away
    let mut h = harness(&[]).await;
    let folder = h.dir.path().join("reports");
    std::fs::create_dir(&folder).unwrap();
    let added = super::super::folders::store(h.dir.path())
        .unwrap()
        .add(folder.to_str().unwrap(), None)
        .unwrap();
    std::fs::remove_dir(&folder).unwrap();
    let id = crate::connections::folder_connection_id(&added.id);

    // WHEN it is checked
    let connection = check(&h, &id).await.unwrap();

    // THEN it needs repair, folder not found, and the list says the same
    assert_eq!(connection.status, ConnectionStatus::NeedsRepair);
    assert_eq!(
        connection.reason.as_ref().map(|r| r.key),
        Some(ConnectionReasonKey::FolderNotFound)
    );
    assert_eq!(
        connection.state,
        crate::connections::ConnectionState::Unavailable
    );
    assert_eq!(
        announced(&mut h.events, &id).await,
        ConnectionStatus::NeedsRepair
    );
    let listed = super::super::collect_connections(&h.state, Vec::new())
        .await
        .unwrap();
    let listed = listed.iter().find(|c| c.id == id).unwrap();
    assert_eq!(listed.reason, connection.reason);
    assert_eq!(listed.checked_at, connection.checked_at);
    h.manager.shutdown().await;
}

#[tokio::test]
async fn test_an_unknown_connection_is_not_found_and_nothing_is_kept() {
    // GIVEN no connection at all
    let h = harness(&[]).await;

    // WHEN an unknown identifier is checked
    let status = check(&h, "imap:nobody@example.org").await.unwrap_err();

    // THEN it is not found, and no state was kept
    assert_eq!(status, StatusCode::NOT_FOUND);
    let store =
        ConnectionStateStore::open(&apollia_core::paths::DataFile::System.path(h.dir.path()))
            .unwrap();
    assert!(store.list().unwrap().is_empty());
    h.manager.shutdown().await;
}

#[tokio::test]
async fn test_a_forbidden_connection_reads_blocked_and_is_not_reached() {
    // GIVEN a mailbox in the list, which the organisation policy forbade at
    // boot, and a mail server that records every check sent to it
    let mut h = harness(&[]).await;
    let mailbox = ImapAccount::from_input(&apollia_connectors::imap::ImapAccountInput {
        email: "ada@example.org".into(),
        host: Some("imap.example.org".into()),
        port: Some(993),
        username: None,
    })
    .unwrap();
    std::fs::write(
        h.dir.path().join(apollia_connectors::imap::IMAP_INDEX_FILE),
        serde_json::json!({ "accounts": [mailbox] }).to_string(),
    )
    .unwrap();
    let forbidden = ForbiddenConnections::compute(&RefuseAll, &[], &[], &[mailbox]);
    h.state.connection_governance =
        ConnectionGovernance::without_policy(None).with_forbidden(Arc::new(forbidden));
    let server = RecordingMailServer::default();

    // WHEN it is checked
    let connection = check_connection_with(
        &h.state,
        Vec::new(),
        &Reach {
            auth: None,
            imap: Some(&server),
        },
        "imap:ada@example.org",
        CheckConnectionBody::default(),
    )
    .await
    .map_err(|(status, _)| status)
    .unwrap();

    // THEN it reads blocked by the organisation, its server was never
    // reached, and the blocked state is kept and announced
    assert_eq!(connection.status, ConnectionStatus::Blocked);
    assert_eq!(
        connection.reason.map(|r| r.key),
        Some(ConnectionReasonKey::ForbiddenByPolicy)
    );
    assert!(server.reached.lock().unwrap().is_empty());
    assert_eq!(
        announced(&mut h.events, "imap:ada@example.org").await,
        ConnectionStatus::Blocked
    );
    h.manager.shutdown().await;
}
