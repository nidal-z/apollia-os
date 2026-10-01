//! Integration test: cancelling a task stops the Python agent it is running.
//!
//! The agent awaits for an hour. Cancelling the task through the router must
//! cancel the asyncio task running it at its next await point, so the whole
//! execution ends within two seconds, and the agent's own `except
//! CancelledError` branch proves the coroutine was stopped rather than left
//! running on its blocking thread.
//!
//! Requires the Apollia SDK importable by the interpreter PYO3_PYTHON points
//! to. Gated behind the `python-tests` feature:
//!
//!   PYO3_PYTHON="$(command -v python3)" \
//!   cargo test -p apollia-e2e-tests --features python-tests --test test_agent_cancel

use std::io::Write as _;
use std::pin::Pin;
use std::sync::Arc;
use std::time::{Duration, Instant};

use apollia_aip::{bridge::AIPBridge, loader::load_agent_module, validator::validate_agent};
use apollia_core::{AIPInput, AIPResult, AIPTask, AgentManifest, ProcessState, TaskStatus};
use apollia_runtime::{
    coordinator::{CancellationToken, ExecutionBackend, ExecutionCoordinator},
    eventbus::EventBus,
    registry::AgentRegistry,
    router::TaskRouterHandle,
};
use pyo3::prelude::*;

/// An agent whose only handler waits an hour, and writes down how it ended.
const SLEEPING_AGENT: &str = r#"
import asyncio
import pathlib

from apollia import agent, on_message

MARKER = pathlib.Path(__MARKER__)


@agent(name="sleeper", version="0.1.0", description="Waits for an hour.")
class Sleeper:
    @on_message
    async def handle(self, message, history, ctx):
        MARKER.write_text("started")
        try:
            await asyncio.sleep(3600)
        except asyncio.CancelledError:
            MARKER.write_text("cancelled")
            raise
        MARKER.write_text("finished")
        return "done"
"#;

/// A `ctx` with the one attribute the dispatch path reads.
const MOCK_CTX_CODE: &std::ffi::CStr = c"
class _Logger:
    def info(self, *a, **k): pass
    def debug(self, *a, **k): pass
    def warning(self, *a, **k): pass
    def error(self, *a, **k): pass

class MockCtx:
    agent_name = 'sleeper'
    def __init__(self):
        self.logger = _Logger()
    def log(self, level, message):
        pass

ctx_instance = MockCtx()
";

fn build_mock_ctx(py: Python<'_>) -> PyObject {
    let ns = pyo3::types::PyDict::new(py);
    py.run(MOCK_CTX_CODE, Some(&ns), Some(&ns))
        .expect("mock ctx code runs");
    ns.get_item("ctx_instance")
        .expect("get_item does not raise")
        .expect("ctx_instance is defined")
        .unbind()
}

/// Runs the agent through the bridge and honours the coordinator's token, the
/// way the production backends do.
struct BridgeBackend {
    bridge: Arc<AIPBridge>,
}

impl ExecutionBackend for BridgeBackend {
    fn execute(
        &self,
        task: AIPTask,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<AIPResult, String>> + Send>> {
        self.execute_cancellable(task, CancellationToken::new())
    }

    fn execute_cancellable(
        &self,
        task: AIPTask,
        cancel: CancellationToken,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<AIPResult, String>> + Send>> {
        let bridge = Arc::clone(&self.bridge);
        Box::pin(async move {
            let ctx: PyObject = Python::with_gil(build_mock_ctx);
            bridge
                .call_run_until(&task, ctx, cancel.cancelled())
                .await
                .map_err(|e| e.to_string())
        })
    }
}

fn manifest(name: &str) -> AgentManifest {
    AgentManifest {
        format_version: 1,
        name: name.to_string(),
        version: "0.1.0".to_string(),
        description: String::new(),
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
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cancelling_a_task_cancels_the_python_coroutine_it_runs() {
    // GIVEN an agent that awaits for an hour, loaded through the bridge
    let dir = tempfile::tempdir().expect("temp dir");
    let marker = dir.path().join("marker.txt");
    let source = SLEEPING_AGENT.replace("__MARKER__", &format!("{:?}", marker.to_string_lossy()));
    let agent_path = dir.path().join("sleeper.py");
    std::fs::File::create(&agent_path)
        .and_then(|mut f| f.write_all(source.as_bytes()))
        .expect("write the agent");
    let agent_obj = load_agent_module(&agent_path).expect("the agent loads");
    let validated = validate_agent(&agent_obj).expect("the agent validates");
    let bridge = Arc::new(AIPBridge::new(validated).expect("bridge"));

    // AND a runtime stack running it as a task
    let (event_sender, _rx) = EventBus::new();
    let registry = AgentRegistry::spawn(event_sender.clone());
    let router: TaskRouterHandle<BridgeBackend> =
        TaskRouterHandle::spawn(registry.clone(), event_sender.clone(), 64);
    let agent_id = registry
        .register(manifest("sleeper"))
        .await
        .expect("register");
    registry
        .update_state(agent_id.as_str(), ProcessState::Active)
        .await
        .expect("activate");
    let coordinator = ExecutionCoordinator::new(
        agent_id.clone(),
        1,
        event_sender.clone(),
        BridgeBackend { bridge },
    );
    router
        .register_coordinator(agent_id.clone(), coordinator)
        .await
        .expect("register coordinator");
    let task_id = router
        .submit(agent_id.as_str(), AIPInput::default())
        .await
        .expect("submit");

    // AND the agent is inside its hour-long await
    let deadline = Instant::now() + Duration::from_secs(20);
    while std::fs::read_to_string(&marker).ok().as_deref() != Some("started") {
        assert!(Instant::now() < deadline, "the agent never started");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    // WHEN the task is cancelled
    let asked = Instant::now();
    let status = router.cancel(task_id.as_str()).await.expect("cancel");
    let took = asked.elapsed();

    // THEN the execution has ended within two seconds, as Canceled
    assert_eq!(status, Some(TaskStatus::Canceled));
    assert!(took < Duration::from_secs(2), "took {took:?}");
    // AND the coroutine itself received the cancellation and stopped
    assert_eq!(
        std::fs::read_to_string(&marker).ok().as_deref(),
        Some("cancelled"),
        "the Python coroutine must be cancelled, not left running"
    );

    router.shutdown();
    registry.shutdown();
}
