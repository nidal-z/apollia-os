use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use apollia_auth::ConnectorProvider;
use apollia_core::{AIPResult, AIPTask, TaskStatus};
use apollia_mcp::{ConnectionOrigin, McpServerRepository, Provenance};
use apollia_tools::ToolRegistryHandle;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::routing::get;
use axum::Router;
use tempfile::TempDir;
use tower::ServiceExt;

use super::collect_connections;
use crate::api::server::AppState;
use crate::coordinator::{DynBackend, ExecutionBackend};
use crate::eventbus::EventBus;
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

fn stdio(name: &str) -> apollia_mcp::config::McpServerConfig {
    serde_json::from_value(serde_json::json!({ "name": name, "command": "npx" })).unwrap()
}

/// A router whose `GET /api/v1/connections` reads `state` and a fixed Google
/// account in place of the real account index.
async fn router(dir: &TempDir) -> Router {
    let repo = McpServerRepository::open(&dir.path().join("mcp.db")).unwrap();
    repo.save(&stdio("files")).unwrap();
    let mut time = stdio("time");
    time.origin = Some(ConnectionOrigin {
        provenance: Provenance::Selection,
        official: false,
        entry: None,
    });
    repo.save(&time).unwrap();

    let tool_registry = ToolRegistryHandle::start();
    let mut read = crate::connectors_bridge::google_tool_descriptors()[0].clone();
    read.name = "mcp:time/get_current_time".to_owned();
    read.family = apollia_tools::ActionFamily::Read;
    tool_registry.register(read).await.unwrap();

    let (event_tx, _) = EventBus::new();
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
        tool_registry_handle: Some(tool_registry),
        audit_trail: None,
        audit_journal: None,
        connection_governance: Default::default(),
        governance: None,
        inference_governance: Default::default(),
        obs_config: apollia_core::ObservabilityConfig::default(),
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
        mcp_handle: None,
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
    Router::new().route(
        "/api/v1/connections",
        get(move || {
            let state = state.clone();
            async move {
                let accounts = vec![(
                    ConnectorProvider::Google,
                    vec!["ada@example.com".to_owned()],
                )];
                collect_connections(&state, accounts).await.map(axum::Json)
            }
        }),
    )
}

#[tokio::test]
async fn test_get_connections_answers_every_store_in_one_list() {
    // GIVEN two MCP servers in mcp.db, one from the selection with a
    // registered read tool, and one Google account
    let dir = TempDir::new().unwrap();
    let app = router(&dir).await;

    // WHEN the list is requested
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/connections")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    // THEN three connections come back, the selection's read tool prechecked
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), 1 << 20)
        .await
        .unwrap();
    let list: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let ids: Vec<&str> = list
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["id"].as_str().unwrap())
        .collect();
    assert_eq!(
        ids,
        ["native:google:ada@example.com", "mcp:files", "mcp:time"]
    );
    assert_eq!(list[1]["provenance"], "custom");
    assert_eq!(list[1]["state"], "stopped");
    assert_eq!(list[2]["provenance"], "selection");
    assert_eq!(list[2]["tools"][0]["id"], "mcp:time/get_current_time");
    assert_eq!(list[2]["tools"][0]["prechecked"], true);
}
