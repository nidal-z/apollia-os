//! Integration tests — full chain with hello_agent.py.
//!
//! Tests the complete path: AIPLoader → AIPBridge → TaskRouter → Coordinator.
//! Requires Python environment. Gated behind the `python-tests` feature.
//!
//! Run with:
//!   PYO3_PYTHON=/opt/homebrew/bin/python3.13 \
//!   cargo test -p apollia-e2e-tests --features python-tests -- --nocapture

use std::io::Write as _;
use std::path::Path;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use apollia_aip::{bridge::AIPBridge, loader::load_agent_module, validator::validate_agent};
use apollia_core::{
    AIPInput, AIPResult, AIPTask, AgentManifest, ProcessState, RuntimeEvent, TaskStatus,
};
use apollia_runtime::{
    coordinator::{ExecutionBackend, ExecutionCoordinator},
    eventbus::EventBus,
    registry::AgentRegistry,
    router::TaskRouterHandle,
};
use pyo3::prelude::*;

// --- AIPBridge backend ---

/// Wraps an `AIPBridge` as an `ExecutionBackend` so it can be used
/// with `ExecutionCoordinator` and `TaskRouter` in integration tests.
struct AIPBridgeBackend {
    bridge: Arc<AIPBridge>,
}

impl ExecutionBackend for AIPBridgeBackend {
    fn execute(
        &self,
        task: AIPTask,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<AIPResult, String>> + Send>> {
        let bridge = Arc::clone(&self.bridge);
        Box::pin(async move {
            // Build a minimal context (empty dict — hello_agent doesn't use tools/memory)
            let ctx: PyObject = Python::with_gil(|py| pyo3::types::PyDict::new_bound(py).into());
            bridge.call_run(&task, ctx).await.map_err(|e| e.to_string())
        })
    }
}

// --- Test helpers ---

fn test_manifest_for(name: &str) -> AgentManifest {
    AgentManifest {
        name: name.to_string(),
        version: "1.0.0".to_string(),
        description: String::new(),
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
        execution_mode: "auto".to_string(),
        system_prompt: None,
        tools_requiring_approval: vec![],
    }
}

/// Returns the absolute path to agents/hello_agent.py.
/// CARGO_MANIFEST_DIR is the `tests/` directory; agents/ is one level up.
fn hello_agent_path() -> std::path::PathBuf {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = manifest_dir
        .parent()
        .expect("tests/ must be inside workspace root");
    workspace_root.join("agents").join("hello_agent.py")
}

// full chain: AIPLoader → AIPBridge → TaskRouter → Coordinator → TaskCompleted
#[tokio::test]
async fn test_hello_agent_full_chain() {
    let agent_path = hello_agent_path();
    assert!(
        agent_path.exists(),
        "hello_agent.py must exist at: {}",
        agent_path.display()
    );

    // GIVEN hello_agent.py loaded and validated
    let agent_obj = load_agent_module(&agent_path).expect("load_agent_module should succeed");
    let validated =
        validate_agent(&agent_obj).expect("hello_agent.py should pass duck-typing validation");
    let bridge = Arc::new(AIPBridge::new(validated).expect("AIPBridge init failed"));

    // AND a full runtime stack (EventBus, AgentRegistry, TaskRouter)
    let (event_sender, _rx) = EventBus::new();
    let registry = AgentRegistry::spawn(event_sender.clone());
    let router: TaskRouterHandle<AIPBridgeBackend> =
        TaskRouterHandle::spawn(registry.clone(), event_sender.clone(), 256);

    // AND the agent registered and marked Active
    let agent_id = registry
        .register(test_manifest_for("hello-agent"))
        .await
        .expect("register should succeed");
    registry
        .update_state(agent_id.as_str(), ProcessState::Active)
        .await
        .expect("update_state should succeed");

    // AND a coordinator with the AIPBridge backend
    let backend = AIPBridgeBackend { bridge };
    let coordinator = ExecutionCoordinator::new(agent_id.clone(), 1, event_sender.clone(), backend);
    router
        .register_coordinator(agent_id.clone(), coordinator)
        .await
        .expect("register_coordinator should succeed");

    // AND a subscriber on the EventBus
    let mut event_rx = event_sender.subscribe();

    // WHEN a task is submitted via TaskRouterHandle
    let task_id = router
        .submit(agent_id.as_str(), AIPInput::default())
        .await
        .expect("submit should succeed");

    // THEN TaskCompleted event is received with success=true
    let completed = tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            match event_rx.recv().await {
                Ok(RuntimeEvent::TaskCompleted {
                    task_id: received_id,
                    success,
                    ..
                }) if received_id == task_id => {
                    return success;
                }
                Ok(_) => continue,
                Err(e) => panic!("EventBus recv error: {e}"),
            }
        }
    })
    .await
    .expect("TaskCompleted should be received within 15s");

    assert!(
        completed,
        "Task should have completed successfully via hello_agent.py"
    );

    // Cleanup
    router.shutdown();
    registry.shutdown();
}

// agent without manifest() fails validation before registration
#[tokio::test]
async fn test_invalid_agent_fails_at_load() {
    // GIVEN a Python file with run() but no manifest()
    let invalid_code = concat!(
        "class BadAgent:\n",
        "    async def run(self, task, ctx):\n",
        "        return {'status': 'completed', 'output': []}\n",
        "agent = BadAgent()\n"
    );

    let mut tmp = tempfile::Builder::new()
        .suffix(".py")
        .tempfile()
        .expect("failed to create temp file");
    tmp.write_all(invalid_code.as_bytes())
        .expect("failed to write temp file");

    // WHEN the agent is loaded and validated
    let agent_obj =
        load_agent_module(tmp.path()).expect("load should succeed (file is syntactically valid)");
    let result = validate_agent(&agent_obj);

    // THEN AIPValidationError is returned (manifest() missing)
    assert!(
        result.is_err(),
        "validate_agent should fail for an agent without manifest()"
    );
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("manifest") || err_msg.contains("method"),
        "error should mention missing manifest: {err_msg}"
    );
}

// complementary — agent with manifest() but non-async run() fails validation
#[tokio::test]
async fn test_sync_run_fails_validation() {
    // GIVEN a Python file where run() is not async
    let invalid_code = concat!(
        "class SyncAgent:\n",
        "    def manifest(self):\n",
        "        return {'name': 'sync-agent', 'version': '1.0.0', 'description': '', 'tools_required': []}\n",
        "    def run(self, task, ctx):  # sync, not async\n",
        "        return {'status': 'completed', 'output': []}\n",
        "agent = SyncAgent()\n"
    );

    let mut tmp = tempfile::Builder::new()
        .suffix(".py")
        .tempfile()
        .expect("failed to create temp file");
    tmp.write_all(invalid_code.as_bytes())
        .expect("failed to write temp file");

    let agent_obj = load_agent_module(tmp.path()).expect("load should succeed");
    let result = validate_agent(&agent_obj);

    // THEN validation fails (run() is not a coroutine function)
    assert!(
        result.is_err(),
        "validate_agent should fail for non-async run(): {result:?}"
    );
}

// complementary — loading a Python file with no 'agent' variable fails
#[tokio::test]
async fn test_missing_agent_variable_fails_load() {
    // GIVEN a valid Python file without a top-level `agent` variable
    let code = "class Unregistered:\n    pass\n";

    let mut tmp = tempfile::Builder::new()
        .suffix(".py")
        .tempfile()
        .expect("failed to create temp file");
    tmp.write_all(code.as_bytes())
        .expect("failed to write temp file");

    // WHEN load_agent_module is called
    let result = load_agent_module(tmp.path());

    // THEN AIPLoaderError::NoAgentFound is returned
    assert!(
        matches!(
            result,
            Err(apollia_aip::loader::AIPLoaderError::NoAgentFound(_))
        ),
        "expected NoAgentFound, got: {result:?}"
    );
}
