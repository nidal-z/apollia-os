//! Bridge Tokio <-> asyncio for calling Python agent methods from Rust async.
//!
//! Uses `tokio::task::spawn_blocking` + `asyncio.run()` to execute Python
//! coroutines without blocking Tokio workers. See ADR-014 for rationale.
//!
//! The GIL is only held on the blocking thread pool, never on Tokio workers.

use std::collections::HashMap;

use apollia_core::{AIPResult, AIPTask};
use pyo3::prelude::*;
use pyo3::types::PyDict;

use crate::validator::ValidatedAgent;

/// Errors that can occur when calling a Python agent via the bridge.
#[derive(Debug, thiserror::Error)]
pub enum AIPBridgeError {
    /// A Python exception was raised during the async call.
    #[error("Python exception: {0}")]
    PythonException(String),

    /// Failed to serialize `AIPTask` to a Python dict.
    #[error("failed to serialize task to Python: {0}")]
    SerializationError(String),

    /// Failed to deserialize the Python result into `AIPResult`.
    #[error("failed to deserialize result from Python: {0}")]
    DeserializationError(String),

    /// Internal bridge error.
    #[error("bridge error: {0}")]
    Internal(String),
}

/// Bridge between the Tokio runtime and a Python asyncio agent.
///
/// Wraps a validated Python agent and provides async Rust methods
/// to call `run()`, `on_start()`, `on_stop()`, and `on_plan_complete()`.
///
/// Uses `tokio::task::spawn_blocking` to move Python execution off
/// Tokio worker threads (ADR-014).
pub struct AIPBridge {
    /// The Python agent object.
    agent: Py<PyAny>,
    /// Whether the agent has an `on_start` callback.
    has_on_start: bool,
    /// Whether the agent has an `on_stop` callback.
    has_on_stop: bool,
    /// Whether the agent has an `on_plan_complete` hook (STORY-086).
    has_on_plan_complete: bool,
}

impl AIPBridge {
    /// Creates a new bridge from a validated agent.
    pub fn new(validated: ValidatedAgent) -> Self {
        Self {
            agent: validated.object,
            has_on_start: validated.has_on_start,
            has_on_stop: validated.has_on_stop,
            has_on_plan_complete: validated.has_on_plan_complete,
        }
    }

    /// Returns `true` if the agent exposes an `on_plan_complete()` hook.
    ///
    /// Detected at validation time via `hasattr` Python duck typing (AC-1, AC-2).
    pub fn has_on_plan_complete(&self) -> bool {
        self.has_on_plan_complete
    }

    /// Calls `agent.run(task, ctx)` asynchronously.
    ///
    /// Serializes `AIPTask` to a Python dict, calls the `run` coroutine
    /// via `asyncio.run()`, and deserializes the result into `AIPResult`.
    ///
    /// The GIL is only held on the blocking thread pool, not on Tokio workers.
    ///
    /// # Errors
    ///
    /// - `SerializationError` if `AIPTask` cannot be converted to a dict
    /// - `PythonException` if the Python code raises an exception
    /// - `DeserializationError` if the result cannot become `AIPResult`
    pub async fn call_run(
        &self,
        task: &AIPTask,
        ctx: PyObject,
    ) -> Result<AIPResult, AIPBridgeError> {
        let task_json = serde_json::to_string(task)
            .map_err(|e| AIPBridgeError::SerializationError(e.to_string()))?;
        let agent = Python::with_gil(|py| self.agent.clone_ref(py));

        let result_json = tokio::task::spawn_blocking(move || {
            Python::with_gil(|py| -> Result<String, AIPBridgeError> {
                let task_dict = json_loads(py, &task_json)
                    .map_err(|e| AIPBridgeError::SerializationError(e.to_string()))?;

                let coroutine = agent
                    .bind(py)
                    .call_method1("run", (task_dict, ctx))
                    .map_err(|e| AIPBridgeError::PythonException(format!("{e}")))?;

                let result = run_coroutine(py, &coroutine)?;

                py_obj_to_json_string(py, &result)
            })
        })
        .await
        .map_err(|e| AIPBridgeError::Internal(format!("spawn_blocking failed: {e}")))??;

        serde_json::from_str(&result_json)
            .map_err(|e| AIPBridgeError::DeserializationError(e.to_string()))
    }

    /// Calls `agent.on_start(ctx)` if the callback exists.
    ///
    /// Does nothing if `has_on_start` is `false`.
    ///
    /// # Errors
    ///
    /// - `PythonException` if the callback raises an exception
    pub async fn call_on_start(&self, ctx: PyObject) -> Result<(), AIPBridgeError> {
        if !self.has_on_start {
            return Ok(());
        }

        let agent = Python::with_gil(|py| self.agent.clone_ref(py));

        tokio::task::spawn_blocking(move || {
            Python::with_gil(|py| -> Result<(), AIPBridgeError> {
                let coroutine = agent
                    .bind(py)
                    .call_method1("on_start", (ctx,))
                    .map_err(|e| AIPBridgeError::PythonException(format!("{e}")))?;

                run_coroutine(py, &coroutine)?;
                Ok(())
            })
        })
        .await
        .map_err(|e| AIPBridgeError::Internal(format!("spawn_blocking failed: {e}")))?
    }

    /// Calls `agent.on_plan_complete(step_results, ctx)` asynchronously.
    ///
    /// Converts `step_results: HashMap<String, String>` into a Python `dict[str, str]`
    /// via [`PyDict`], then calls the Python coroutine via `asyncio.run()` (ADR-014).
    /// The hook must return a `str`; the return value is wrapped in [`AIPResult::completed`].
    ///
    /// The GIL is only held on the blocking thread pool, not on Tokio workers.
    ///
    /// # Errors
    ///
    /// - `PythonException` if the Python method raises an exception
    /// - `DeserializationError` if the return value is not a `str`
    /// - `Internal` if `spawn_blocking` fails
    pub async fn call_on_plan_complete(
        &self,
        step_results: HashMap<String, String>,
        ctx: PyObject,
    ) -> Result<AIPResult, AIPBridgeError> {
        let agent = Python::with_gil(|py| self.agent.clone_ref(py));

        tokio::task::spawn_blocking(move || {
            Python::with_gil(|py| -> Result<AIPResult, AIPBridgeError> {
                // Build PyDict from HashMap (AC-3 — correct step_results injection)
                let py_dict = PyDict::new_bound(py);
                for (k, v) in &step_results {
                    py_dict
                        .set_item(k, v)
                        .map_err(|e| AIPBridgeError::PythonException(e.to_string()))?;
                }

                let coroutine = agent
                    .bind(py)
                    .call_method1("on_plan_complete", (py_dict, ctx))
                    .map_err(|e| AIPBridgeError::PythonException(format!("{e}")))?;

                let result = run_coroutine(py, &coroutine)?;

                // Hook must return a str (AC-3)
                result
                    .bind(py)
                    .extract::<String>()
                    .map(|s| AIPResult::completed(&s))
                    .map_err(|e| {
                        AIPBridgeError::DeserializationError(format!(
                            "on_plan_complete must return str, got: {e}"
                        ))
                    })
            })
        })
        .await
        .map_err(|e| AIPBridgeError::Internal(format!("spawn_blocking failed: {e}")))?
    }

    /// Calls `agent.on_stop()` if the callback exists.
    ///
    /// Does nothing if `has_on_stop` is `false`.
    ///
    /// # Errors
    ///
    /// - `PythonException` if the callback raises an exception
    pub async fn call_on_stop(&self) -> Result<(), AIPBridgeError> {
        if !self.has_on_stop {
            return Ok(());
        }

        let agent = Python::with_gil(|py| self.agent.clone_ref(py));

        tokio::task::spawn_blocking(move || {
            Python::with_gil(|py| -> Result<(), AIPBridgeError> {
                let coroutine = agent
                    .bind(py)
                    .call_method0("on_stop")
                    .map_err(|e| AIPBridgeError::PythonException(format!("{e}")))?;

                run_coroutine(py, &coroutine)?;
                Ok(())
            })
        })
        .await
        .map_err(|e| AIPBridgeError::Internal(format!("spawn_blocking failed: {e}")))?
    }
}

/// Runs a Python coroutine to completion via `asyncio.run()`.
fn run_coroutine<'py>(
    py: Python<'py>,
    coroutine: &Bound<'py, PyAny>,
) -> Result<PyObject, AIPBridgeError> {
    let asyncio = py
        .import_bound("asyncio")
        .map_err(|e| AIPBridgeError::Internal(e.to_string()))?;

    let result = asyncio
        .call_method1("run", (coroutine,))
        .map_err(|e| AIPBridgeError::PythonException(format!("{e}")))?;

    Ok(result.into())
}

/// Parses a JSON string into a Python object via `json.loads()`.
fn json_loads<'py>(py: Python<'py>, json_str: &str) -> Result<Bound<'py, PyAny>, PyErr> {
    let json_mod = py.import_bound("json")?;
    json_mod.call_method1("loads", (json_str,))
}

/// Converts a Python object to a JSON string via `json.dumps()`.
fn py_obj_to_json_string(py: Python<'_>, obj: &PyObject) -> Result<String, AIPBridgeError> {
    let json_mod = py
        .import_bound("json")
        .map_err(|e| AIPBridgeError::Internal(e.to_string()))?;

    json_mod
        .call_method1("dumps", (obj.bind(py),))
        .map_err(|e| AIPBridgeError::DeserializationError(e.to_string()))?
        .extract()
        .map_err(|e| AIPBridgeError::DeserializationError(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::validator::validate_agent;
    use pyo3::types::{PyDict, PyModule};

    /// Helper: create a ValidatedAgent from inline Python code.
    fn create_validated_agent(code: &str) -> ValidatedAgent {
        let agent = Python::with_gil(|py| {
            let module = PyModule::from_code_bound(py, code, "test_bridge.py", "test_bridge")
                .expect("failed to create test module");
            module.getattr("agent").expect("failed to get agent").into()
        });
        validate_agent(&agent).expect("agent validation failed")
    }

    /// Helper: create an empty Python dict as ctx.
    fn empty_ctx() -> PyObject {
        Python::with_gil(|py| {
            let dict = PyDict::new_bound(py);
            dict.into()
        })
    }

    const AGENT_WITH_HOOK: &str = r#"
class AgentAvecHook:
    def manifest(self):
        return {
            "name": "hook-agent",
            "version": "1.0.0",
            "description": "Agent with on_plan_complete",
            "tools_required": [],
        }

    async def run(self, task, ctx):
        return {"status": "completed", "output": [{"type": "text", "text": "ok"}]}

    async def on_plan_complete(self, step_results, ctx):
        return "résultat: " + str(len(step_results)) + " steps"

agent = AgentAvecHook()
"#;

    const AGENT_HOOK_RAISES: &str = r#"
class AgentHookRaises:
    def manifest(self):
        return {
            "name": "error-hook-agent",
            "version": "1.0.0",
            "description": "Agent whose on_plan_complete raises",
            "tools_required": [],
        }

    async def run(self, task, ctx):
        return {"status": "completed", "output": []}

    async def on_plan_complete(self, step_results, ctx):
        raise ValueError("hook exploded")

agent = AgentHookRaises()
"#;

    const VALID_AGENT_CODE: &str = r#"
import asyncio

class TestAgent:
    def manifest(self):
        return {
            "name": "bridge-test",
            "version": "1.0.0",
            "description": "Bridge test agent",
            "tools_required": [],
        }

    async def run(self, task, ctx):
        return {
            "status": "completed",
            "output": [{"type": "text", "text": "hello from python"}],
        }

    async def on_start(self, ctx):
        pass

    async def on_stop(self):
        pass

agent = TestAgent()
"#;

    #[tokio::test]
    async fn test_call_run_success() {
        // GIVEN a bridge with a valid agent
        let validated = create_validated_agent(VALID_AGENT_CODE);
        let bridge = AIPBridge::new(validated);
        let task = AIPTask::default();
        let ctx = empty_ctx();

        // WHEN we call run()
        let result = bridge.call_run(&task, ctx).await;

        // THEN we get a valid AIPResult
        assert!(result.is_ok());
        let aip_result = result.expect("call_run should succeed");
        assert_eq!(aip_result.status, apollia_core::TaskStatus::Completed);
    }

    #[tokio::test]
    async fn test_call_run_python_exception() {
        // GIVEN an agent whose run() raises an exception
        let code = r#"
class A:
    def manifest(self):
        return {
            "name": "err-agent", "version": "1.0.0",
            "description": "error agent", "tools_required": [],
        }
    async def run(self, task, ctx):
        raise ValueError("test error from python")
agent = A()
"#;
        let validated = create_validated_agent(code);
        let bridge = AIPBridge::new(validated);
        let ctx = empty_ctx();

        // WHEN we call run()
        let result = bridge.call_run(&AIPTask::default(), ctx).await;

        // THEN we get PythonException
        assert!(matches!(result, Err(AIPBridgeError::PythonException(_))));
    }

    #[tokio::test]
    async fn test_call_on_start_with_callback() {
        // GIVEN a bridge with an agent that has on_start
        let validated = create_validated_agent(VALID_AGENT_CODE);
        assert!(validated.has_on_start);
        let bridge = AIPBridge::new(validated);
        let ctx = empty_ctx();

        // WHEN we call on_start()
        let result = bridge.call_on_start(ctx).await;

        // THEN the call succeeds
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_call_on_stop_with_callback() {
        // GIVEN a bridge with an agent that has on_stop
        let validated = create_validated_agent(VALID_AGENT_CODE);
        assert!(validated.has_on_stop);
        let bridge = AIPBridge::new(validated);

        // WHEN we call on_stop()
        let result = bridge.call_on_stop().await;

        // THEN the call succeeds
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_call_on_start_without_callback() {
        // GIVEN a bridge with an agent WITHOUT on_start
        let code = r#"
class A:
    def manifest(self):
        return {
            "name": "no-cb", "version": "1.0.0",
            "description": "no callbacks", "tools_required": [],
        }
    async def run(self, task, ctx):
        return {"status": "completed", "output": []}
agent = A()
"#;
        let validated = create_validated_agent(code);
        assert!(!validated.has_on_start);
        let bridge = AIPBridge::new(validated);
        let ctx = empty_ctx();

        // WHEN we call on_start() on an agent without the callback
        let result = bridge.call_on_start(ctx).await;

        // THEN the call succeeds (no-op)
        assert!(result.is_ok());
    }

    // AC-1 — has_on_plan_complete() returns true when hook is present

    #[test]
    fn test_has_on_plan_complete_true() {
        // GIVEN a validated agent with on_plan_complete()
        let validated = create_validated_agent(AGENT_WITH_HOOK);

        // WHEN we create the bridge
        let bridge = AIPBridge::new(validated);

        // THEN has_on_plan_complete() returns true
        assert!(bridge.has_on_plan_complete());
    }

    // AC-2 — has_on_plan_complete() returns false when hook is absent

    #[test]
    fn test_has_on_plan_complete_false() {
        // GIVEN a validated agent without on_plan_complete()
        let validated = create_validated_agent(VALID_AGENT_CODE);

        // WHEN we create the bridge
        let bridge = AIPBridge::new(validated);

        // THEN has_on_plan_complete() returns false
        assert!(!bridge.has_on_plan_complete());
    }

    // AC-3 — call_on_plan_complete() calls hook with step_results dict

    #[tokio::test]
    async fn test_call_on_plan_complete_success() {
        // GIVEN a bridge with an agent that has on_plan_complete()
        //   AND step_results with 2 entries
        let validated = create_validated_agent(AGENT_WITH_HOOK);
        let bridge = AIPBridge::new(validated);
        let mut step_results = HashMap::new();
        step_results.insert("s1".to_string(), "output A".to_string());
        step_results.insert("s2".to_string(), "output B".to_string());
        let ctx = empty_ctx();

        // WHEN we call on_plan_complete()
        let result = bridge.call_on_plan_complete(step_results, ctx).await;

        // THEN we get AIPResult::Completed with the hook's string output
        assert!(result.is_ok());
        let aip_result = result.expect("call_on_plan_complete should succeed");
        assert_eq!(aip_result.status, apollia_core::TaskStatus::Completed);
        // The hook returns "résultat: 2 steps"
        if let Some(apollia_core::AIPPart::Text(t)) = aip_result.output.first() {
            assert!(
                t.text.contains("2 steps"),
                "expected '2 steps' in: {}",
                t.text
            );
        } else {
            panic!("expected TextPart output");
        }
    }

    // AC-4 — call_on_plan_complete() propagates Python exceptions

    #[tokio::test]
    async fn test_call_on_plan_complete_python_exception() {
        // GIVEN a bridge with an agent whose on_plan_complete() raises
        let validated = create_validated_agent(AGENT_HOOK_RAISES);
        let bridge = AIPBridge::new(validated);
        let ctx = empty_ctx();

        // WHEN we call on_plan_complete()
        let result = bridge.call_on_plan_complete(HashMap::new(), ctx).await;

        // THEN we get PythonException (no panic)
        assert!(
            matches!(result, Err(AIPBridgeError::PythonException(_))),
            "expected PythonException, got: {result:?}"
        );
    }

    #[tokio::test]
    async fn test_task_serialization_roundtrip() {
        // GIVEN an agent that echoes back the task_id
        let code = r#"
class A:
    def manifest(self):
        return {
            "name": "echo", "version": "1.0.0",
            "description": "echo agent", "tools_required": [],
        }
    async def run(self, task, ctx):
        return {
            "status": "completed",
            "output": [{"type": "text", "text": task.get("task_id", "no-id")}],
        }
agent = A()
"#;
        let validated = create_validated_agent(code);
        let bridge = AIPBridge::new(validated);
        let ctx = empty_ctx();
        let task = AIPTask::default();

        // WHEN we call run()
        let result = bridge.call_run(&task, ctx).await;

        // THEN serialization/deserialization works
        assert!(result.is_ok());
    }
}
