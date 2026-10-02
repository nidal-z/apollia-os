//! A bare application state for the route tests of the attached desktop: the
//! registry and the router live, every optional subsystem absent, the data
//! directory chosen by the test.

use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;

use apollia_core::{AIPResult, AIPTask, AgentManifest, TaskStatus};

use crate::api::routes_agents::AgentLoader;
use crate::api::server::AppState;
use crate::coordinator::{DynBackend, ExecutionBackend};
use crate::eventbus::EventBus;
use crate::registry::AgentRegistry;
use crate::router::TaskRouterHandle;

/// A backend that completes every task at once.
#[derive(Clone)]
pub(crate) struct MockBackend;

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

/// A loader that names the agent after the file stem, or refuses with `refusal`.
pub(crate) struct StemLoader {
    pub(crate) refusal: Option<String>,
}

impl AgentLoader for StemLoader {
    fn load_and_validate(&self, path: &Path) -> Result<AgentManifest, String> {
        if let Some(refusal) = &self.refusal {
            return Err(refusal.clone());
        }
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("x");
        Ok(manifest(stem))
    }
}

/// A manifest with every optional part empty.
pub(crate) fn manifest(name: &str) -> AgentManifest {
    AgentManifest {
        format_version: 1,
        name: name.to_owned(),
        version: "1.0.0".to_owned(),
        description: format!("{name} description"),
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
        execution_mode: "auto".to_owned(),
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

/// The state, with `loader` and `data_dir`; the test sets any other field.
pub(crate) fn app_state(loader: Arc<dyn AgentLoader>, data_dir: PathBuf) -> AppState<MockBackend> {
    let (event_tx, _) = EventBus::new();
    let registry_handle = AgentRegistry::spawn(event_tx.clone());
    let router_handle: TaskRouterHandle<MockBackend> =
        TaskRouterHandle::spawn(registry_handle.clone(), event_tx.clone(), 64);
    AppState {
        router_handle,
        registry_handle,
        event_sender: event_tx,
        agent_loader: loader,
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
        connection_governance: Default::default(),
        obs_config: apollia_core::ObservabilityConfig::default(),
        llm_call_repository: None,
        trigger_def_repo: None,
        notification_repo: None,
        notification_engine_handle: None,
        chat_manager: None,
        plan_cache: None,
        mailbox_handle: None,
        user_memory: None,
        data_dir,
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
        secret_store: crate::inference_servers::default_store_opener(),
    }
}

/// Sends one request through the whole router and reads the JSON answer.
pub(crate) async fn call(
    state: AppState<MockBackend>,
    method: &str,
    uri: &str,
    body: Option<serde_json::Value>,
) -> (axum::http::StatusCode, serde_json::Value) {
    use tower::ServiceExt;
    let router = crate::api::server::APIServer::build_router_for_test(state);
    let mut request = axum::http::Request::builder().method(method).uri(uri);
    let body = match body {
        Some(json) => {
            request = request.header("content-type", "application/json");
            axum::body::Body::from(json.to_string())
        }
        None => axum::body::Body::empty(),
    };
    let response = router
        .oneshot(request.body(body).expect("request"))
        .await
        .expect("router answers");
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 1 << 20)
        .await
        .expect("body");
    let json = serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);
    (status, json)
}
