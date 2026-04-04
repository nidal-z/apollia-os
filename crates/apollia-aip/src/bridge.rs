//! Bridge Tokio <-> asyncio for calling Python agent methods from Rust async.
//!
//! Uses `tokio::task::spawn_blocking` + `asyncio.run()` to execute Python
//! coroutines without blocking Tokio workers. See ADR-014 for rationale.
//!
//! The GIL is only held on the blocking thread pool, never on Tokio workers.
//!
//! ## Python AIP types
//!
//! `AIPResult` and `InputResponse` Python convenience classes are injected into
//! the agent's `run.__globals__` namespace before each `call_run()` invocation.
//! Agents may use them directly without any import statement:
//!
//! ```python
//! async def run(self, task, ctx):
//!     if task["is_resumed"]:
//!         ir = task["input_response"]
//!         if not ir.approved:
//!             return AIPResult.failed("REJECTED", ir.reason or "Refusé")
//!     return AIPResult.input_required("Confirmer ?", {"key": "val"})
//! ```

use std::collections::HashMap;

use apollia_core::{AIPResult, AIPTask};
use pyo3::prelude::*;
use pyo3::types::PyDict;

use crate::validator::ValidatedAgent;

// ─────────────────────────────────────────────
// Python AIP types — static definition
// ─────────────────────────────────────────────

/// Python source for the `AIPResult` and `InputResponse` helper classes.
///
/// Injected into the agent's `run.__globals__` namespace before each `call_run()`.
/// The factory methods return plain Python dicts that serialise cleanly with
/// `json.dumps()` and deserialise into the Rust [`AIPResult`] / [`InputResponseData`]
/// types via serde.
const AIP_TYPES_PY: &str = r#"
class InputResponse:
    """Réponse humaine reçue après une suspension input_required.

    Accessible via task["input_response"] dans run().
    Supporte l'accès par attribut (ir.approved) ET par clé (ir.get("approved")).
    """
    def __init__(self, data):
        self.approved     = data.get("approved", False)
        self.reason       = data.get("reason")
        self.context      = data.get("context", {})
        self.responded_at = data.get("responded_at", "")

    def get(self, key, default=None):
        return getattr(self, key, default)

    def __getitem__(self, key):
        return getattr(self, key)


class AIPResult:
    """Factory pour les variants de résultat AIP.

    Retourne des dicts JSON-sérialisables directement par json.dumps().
    Injecté automatiquement dans run.__globals__ par le bridge Rust.
    """

    @classmethod
    def input_required(cls, prompt, context):
        """Suspendre la tâche et demander une validation humaine.

        Le runtime persiste prompt et context dans SQLite,
        puis notifie l'utilisateur sur les canaux configurés.
        """
        return {
            "status": "input_required",
            "output": [],
            "input_required_data": {"prompt": prompt, "context": context},
        }

    @classmethod
    def completed(cls, text):
        """Résultat de succès avec texte de réponse."""
        return {
            "status": "completed",
            "output": [{"type": "text", "text": text}],
        }

    @classmethod
    def failed(cls, code, message):
        """Résultat d'échec avec code et message structurés."""
        return {
            "status": "failed",
            "output": [],
            "error": {"code": code, "message": message},
        }
"#;

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
///
/// Injects Python `AIPResult` and `InputResponse` convenience classes into
/// the agent's `run.__globals__` namespace before every `call_run()`.
pub struct AIPBridge {
    /// The Python agent object.
    agent: Py<PyAny>,
    /// Whether the agent has an `on_start` callback.
    has_on_start: bool,
    /// Whether the agent has an `on_stop` callback.
    has_on_stop: bool,
    /// Whether the agent has an `on_plan_complete` hook.
    has_on_plan_complete: bool,
    /// Python `AIPResult` class — injected into agent globals for convenience.
    aip_result_class: Py<PyAny>,
    /// Python `InputResponse` class — wraps the input_response dict for attribute access.
    input_response_class: Py<PyAny>,
}

impl AIPBridge {
    /// Creates a new bridge from a validated agent.
    ///
    /// Initialises the Python `AIPResult` and `InputResponse` helper classes
    /// that will be injected into the agent's globals on each
    /// `call_run()` invocation.
    ///
    /// # Errors
    ///
    /// Returns `AIPBridgeError::Internal` if the Python helper classes cannot
    /// be defined (should never happen with the bundled static source).
    pub fn new(validated: ValidatedAgent) -> Result<Self, AIPBridgeError> {
        let (aip_result_class, input_response_class) = Python::with_gil(|py| {
            let code_c = std::ffi::CString::new(AIP_TYPES_PY).map_err(|e| {
                AIPBridgeError::Internal(format!("AIP_TYPES_PY contains NUL byte: {e}"))
            })?;
            let module = pyo3::types::PyModule::from_code(
                py,
                &code_c,
                c"apollia_aip_types.py",
                c"apollia_aip_types",
            )
            .map_err(|e| {
                AIPBridgeError::Internal(format!("failed to define AIP Python types: {e}"))
            })?;

            let aip_result = module
                .getattr("AIPResult")
                .map_err(|e| AIPBridgeError::Internal(format!("AIPResult class not found: {e}")))?
                .unbind();

            let input_response = module
                .getattr("InputResponse")
                .map_err(|e| {
                    AIPBridgeError::Internal(format!("InputResponse class not found: {e}"))
                })?
                .unbind();

            Ok::<_, AIPBridgeError>((aip_result, input_response))
        })?;

        Ok(Self {
            agent: validated.object,
            has_on_start: validated.has_on_start,
            has_on_stop: validated.has_on_stop,
            has_on_plan_complete: validated.has_on_plan_complete,
            aip_result_class,
            input_response_class,
        })
    }

    /// Returns `true` if the agent exposes an `on_plan_complete()` hook.
    ///
    /// Detected at validation time via `hasattr` Python duck typing.
    pub fn has_on_plan_complete(&self) -> bool {
        self.has_on_plan_complete
    }

    /// Calls `agent.run(task, ctx)` asynchronously.
    ///
    /// Serializes `AIPTask` to a Python dict, injects the `AIPResult` and
    /// `InputResponse` helper classes into the agent's `run.__globals__`
    ///, calls the `run` coroutine via `asyncio.run()`, and
    /// deserializes the result into `AIPResult`.
    ///
    /// If `task.input_response` is present (resumed task), the raw dict is
    /// wrapped as an `InputResponse` object so agents can use attribute access
    /// (`ir.approved`, `ir.reason`, etc.).
    ///
    /// The GIL is only held on the blocking thread pool, not on Tokio workers.
    ///
    /// # Errors
    ///
    /// - `SerializationError` if `AIPTask` cannot be converted to a dict
    /// - `PythonException` if the Python code raises an exception
    /// - `DeserializationError` if the result cannot become `AIPResult`
    /// - `Internal` if Python class injection fails
    pub async fn call_run(
        &self,
        task: &AIPTask,
        ctx: PyObject,
    ) -> Result<AIPResult, AIPBridgeError> {
        let task_json = serde_json::to_string(task)
            .map_err(|e| AIPBridgeError::SerializationError(e.to_string()))?;
        let agent = Python::with_gil(|py| self.agent.clone_ref(py));
        let aip_result_class = Python::with_gil(|py| self.aip_result_class.clone_ref(py));
        let input_response_class = Python::with_gil(|py| self.input_response_class.clone_ref(py));

        let result_json = tokio::task::spawn_blocking(move || {
            Python::with_gil(|py| -> Result<String, AIPBridgeError> {
                // 1. Inject AIPResult and InputResponse into the agent's run method globals
                //    so the agent can use them without any import statement.
                let run_method = agent.bind(py).getattr("run").map_err(|e| {
                    AIPBridgeError::Internal(format!("agent has no run method: {e}"))
                })?;
                let run_globals = run_method.getattr("__globals__").map_err(|e| {
                    AIPBridgeError::Internal(format!("run has no __globals__: {e}"))
                })?;
                run_globals
                    .set_item("AIPResult", aip_result_class.bind(py))
                    .map_err(|e| {
                        AIPBridgeError::Internal(format!("inject AIPResult failed: {e}"))
                    })?;
                run_globals
                    .set_item("InputResponse", input_response_class.bind(py))
                    .map_err(|e| {
                        AIPBridgeError::Internal(format!("inject InputResponse failed: {e}"))
                    })?;

                // 2. Deserialise AIPTask into a Python dict.
                let task_dict = json_loads(py, &task_json)
                    .map_err(|e| AIPBridgeError::SerializationError(e.to_string()))?;

                // 3. If task["input_response"] is present, wrap it as an InputResponse
                //    object so the agent can use ir.approved, ir.reason, etc.
                if let Ok(dict) = task_dict.downcast::<pyo3::types::PyDict>() {
                    if let Ok(Some(ir_raw)) = dict.get_item("input_response") {
                        if !ir_raw.is_none() {
                            let ir_obj =
                                input_response_class
                                    .bind(py)
                                    .call1((ir_raw,))
                                    .map_err(|e| {
                                        AIPBridgeError::Internal(format!(
                                            "InputResponse wrap failed: {e}"
                                        ))
                                    })?;
                            dict.set_item("input_response", ir_obj).map_err(|e| {
                                AIPBridgeError::Internal(format!("set input_response failed: {e}"))
                            })?;
                        }
                    }
                }

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
                // Build PyDict from HashMap
                let py_dict = PyDict::new(py);
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

                // Hook must return a str
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
        .import("asyncio")
        .map_err(|e| AIPBridgeError::Internal(e.to_string()))?;

    let result = asyncio
        .call_method1("run", (coroutine,))
        .map_err(|e| AIPBridgeError::PythonException(format!("{e}")))?;

    Ok(result.into())
}

/// Parses a JSON string into a Python object via `json.loads()`.
fn json_loads<'py>(py: Python<'py>, json_str: &str) -> Result<Bound<'py, PyAny>, PyErr> {
    let json_mod = py.import("json")?;
    json_mod.call_method1("loads", (json_str,))
}

/// Converts a Python object to a JSON string via `json.dumps()`.
fn py_obj_to_json_string(py: Python<'_>, obj: &PyObject) -> Result<String, AIPBridgeError> {
    let json_mod = py
        .import("json")
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
    use apollia_core::{InputResponseData, TaskStatus};
    use pyo3::types::{PyDict, PyModule};

    // ─────────────────────────────────────────────
    // Test helpers
    // ─────────────────────────────────────────────

    /// Creates an `AIPBridge` from inline Python code (test helper).
    fn create_bridge(code: &str) -> AIPBridge {
        let agent = Python::with_gil(|py| {
            let code_c = std::ffi::CString::new(code).expect("code contains NUL byte");
            let module = PyModule::from_code(py, &code_c, c"test_bridge.py", c"test_bridge")
                .expect("failed to create test module");
            module.getattr("agent").expect("failed to get agent").into()
        });
        let validated = validate_agent(&agent).expect("agent validation failed");
        AIPBridge::new(validated).expect("AIPBridge init failed")
    }

    /// Creates a `ValidatedAgent` from inline Python code (for flag-inspection tests).
    fn create_validated(code: &str) -> crate::validator::ValidatedAgent {
        let agent = Python::with_gil(|py| {
            let code_c = std::ffi::CString::new(code).expect("code contains NUL byte");
            let module = PyModule::from_code(py, &code_c, c"test_bridge.py", c"test_bridge")
                .expect("failed to create test module");
            module.getattr("agent").expect("failed to get agent").into()
        });
        validate_agent(&agent).expect("agent validation failed")
    }

    /// Creates an empty Python dict for use as ctx.
    fn empty_ctx() -> PyObject {
        Python::with_gil(|py| {
            let dict = PyDict::new(py);
            dict.into()
        })
    }

    // ─────────────────────────────────────────────
    // Python agent fixtures
    // ─────────────────────────────────────────────

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

    // ─────────────────────────────────────────────
    // Existing bridge tests (unchanged behaviour)
    // ─────────────────────────────────────────────

    #[tokio::test]
    async fn test_call_run_success() {
        // GIVEN a bridge with a valid agent
        let bridge = create_bridge(VALID_AGENT_CODE);
        let task = AIPTask::default();
        let ctx = empty_ctx();

        // WHEN we call run()
        let result = bridge.call_run(&task, ctx).await;

        // THEN we get a valid AIPResult
        assert!(result.is_ok());
        let aip_result = result.expect("call_run should succeed");
        assert_eq!(aip_result.status, TaskStatus::Completed);
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
        let bridge = create_bridge(code);
        let ctx = empty_ctx();

        // WHEN we call run()
        let result = bridge.call_run(&AIPTask::default(), ctx).await;

        // THEN we get PythonException
        assert!(matches!(result, Err(AIPBridgeError::PythonException(_))));
    }

    #[tokio::test]
    async fn test_call_on_start_with_callback() {
        // GIVEN a bridge with an agent that has on_start
        let validated = create_validated(VALID_AGENT_CODE);
        assert!(validated.has_on_start);
        let bridge = AIPBridge::new(validated).expect("bridge init");
        let ctx = empty_ctx();

        // WHEN we call on_start()
        let result = bridge.call_on_start(ctx).await;

        // THEN the call succeeds
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_call_on_stop_with_callback() {
        // GIVEN a bridge with an agent that has on_stop
        let validated = create_validated(VALID_AGENT_CODE);
        assert!(validated.has_on_stop);
        let bridge = AIPBridge::new(validated).expect("bridge init");

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
        let validated = create_validated(code);
        assert!(!validated.has_on_start);
        let bridge = AIPBridge::new(validated).expect("bridge init");
        let ctx = empty_ctx();

        // WHEN we call on_start() on an agent without the callback
        let result = bridge.call_on_start(ctx).await;

        // THEN the call succeeds (no-op)
        assert!(result.is_ok());
    }

    #[test]
    fn test_has_on_plan_complete_true() {
        // GIVEN a validated agent with on_plan_complete()
        let bridge = create_bridge(AGENT_WITH_HOOK);
        // THEN has_on_plan_complete() returns true
        assert!(bridge.has_on_plan_complete());
    }

    #[test]
    fn test_has_on_plan_complete_false() {
        // GIVEN a validated agent without on_plan_complete()
        let bridge = create_bridge(VALID_AGENT_CODE);
        // THEN has_on_plan_complete() returns false
        assert!(!bridge.has_on_plan_complete());
    }

    #[tokio::test]
    async fn test_call_on_plan_complete_success() {
        // GIVEN a bridge with an agent that has on_plan_complete()
        //   AND step_results with 2 entries
        let bridge = create_bridge(AGENT_WITH_HOOK);
        let mut step_results = HashMap::new();
        step_results.insert("s1".to_string(), "output A".to_string());
        step_results.insert("s2".to_string(), "output B".to_string());
        let ctx = empty_ctx();

        // WHEN we call on_plan_complete()
        let result = bridge.call_on_plan_complete(step_results, ctx).await;

        // THEN we get AIPResult::Completed with the hook's string output
        assert!(result.is_ok());
        let aip_result = result.expect("call_on_plan_complete should succeed");
        assert_eq!(aip_result.status, TaskStatus::Completed);
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

    #[tokio::test]
    async fn test_call_on_plan_complete_python_exception() {
        // GIVEN a bridge with an agent whose on_plan_complete() raises
        let bridge = create_bridge(AGENT_HOOK_RAISES);
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
        let bridge = create_bridge(code);
        let ctx = empty_ctx();
        let task = AIPTask::default();

        // WHEN we call run()
        let result = bridge.call_run(&task, ctx).await;

        // THEN serialization/deserialization works
        assert!(result.is_ok());
    }

    // ─────────────────────────────────────────────
    // HITL contract tests
    // ─────────────────────────────────────────────

    // AIPResult.input_required() retourne le bon variant

    #[tokio::test]
    async fn test_ac1_aip_result_input_required_variant() {
        // GIVEN an agent that returns AIPResult.input_required(...)
        let code = r#"
class A:
    def manifest(self):
        return {
            "name": "hitl-agent", "version": "1.0.0",
            "description": "HITL test agent", "tools_required": [],
        }
    async def run(self, task, ctx):
        # AIPResult is injected by the bridge — no import needed
        return AIPResult.input_required("Confirmer ?", {"key": "val"})
agent = A()
"#;
        let bridge = create_bridge(code);
        let ctx = empty_ctx();

        // WHEN we call run()
        let result = bridge.call_run(&AIPTask::default(), ctx).await;

        // THEN the variant is InputRequired with correct prompt and context
        assert!(result.is_ok(), "call_run failed: {:?}", result.err());
        let aip_result = result.expect("call_run should succeed");
        assert_eq!(
            aip_result.status,
            TaskStatus::InputRequired,
            "expected InputRequired status"
        );
        let data = aip_result
            .input_required_data
            .expect("input_required_data must be present");
        assert_eq!(data.prompt, "Confirmer ?");
        assert_eq!(data.context, serde_json::json!({"key": "val"}));
    }

    // AIPTask enrichi à la reprise (is_resumed=true, input_response peuplé)

    #[tokio::test]
    async fn test_ac2_aip_task_is_resumed_true() {
        // GIVEN an AIPTask built with is_resumed=true and a populated InputResponse
        let task = AIPTask {
            task_id: "t-resume-001".into(),
            is_resumed: true,
            input_response: Some(InputResponseData {
                approved: true,
                reason: None,
                context: serde_json::json!({"devis": 42}),
                responded_at: "2026-03-09T10:00:00Z".into(),
            }),
            ..AIPTask::default()
        };

        // WHEN an agent reads task["is_resumed"] and task["input_response"]
        let code = r#"
class A:
    def manifest(self):
        return {
            "name": "resume-reader", "version": "1.0.0",
            "description": "Reads is_resumed and input_response", "tools_required": [],
        }
    async def run(self, task, ctx):
        ir = task["input_response"]
        return {
            "status": "completed",
            "output": [{"type": "text", "text": str(task["is_resumed"]) + "|" + str(ir.approved)}],
        }
agent = A()
"#;
        let bridge = create_bridge(code);
        let ctx = empty_ctx();

        // WHEN we call run() with a resumed task
        let result = bridge.call_run(&task, ctx).await;

        // THEN is_resumed=True and input_response.approved=True are visible in Python
        assert!(result.is_ok(), "call_run failed: {:?}", result.err());
        let aip_result = result.expect("should succeed");
        if let Some(apollia_core::AIPPart::Text(t)) = aip_result.output.first() {
            assert!(
                t.text.contains("True|True"),
                "expected 'True|True' in output, got: {}",
                t.text
            );
        } else {
            panic!("expected TextPart output");
        }
    }

    // AIPTask reprise après rejet (input_response.approved=False, reason peuplée)

    #[tokio::test]
    async fn test_ac3_aip_task_is_resumed_rejected() {
        // GIVEN an AIPTask built after a rejected approval
        let task = AIPTask {
            task_id: "t-reject-001".into(),
            is_resumed: true,
            input_response: Some(InputResponseData {
                approved: false,
                reason: Some("Remise à négocier d'abord".into()),
                context: serde_json::json!({}),
                responded_at: "2026-03-09T10:01:00Z".into(),
            }),
            ..AIPTask::default()
        };

        let code = r#"
class A:
    def manifest(self):
        return {
            "name": "reject-reader", "version": "1.0.0",
            "description": "Reads rejection response", "tools_required": [],
        }
    async def run(self, task, ctx):
        ir = task["input_response"]
        return {
            "status": "completed",
            "output": [{"type": "text", "text": str(ir.approved) + "|" + (ir.reason or "none")}],
        }
agent = A()
"#;
        let bridge = create_bridge(code);
        let ctx = empty_ctx();

        // WHEN we call run()
        let result = bridge.call_run(&task, ctx).await;

        // THEN approved=False and reason is transmitted correctly
        assert!(result.is_ok(), "call_run failed: {:?}", result.err());
        let aip_result = result.expect("should succeed");
        if let Some(apollia_core::AIPPart::Text(t)) = aip_result.output.first() {
            assert!(
                t.text.contains("False|Remise"),
                "expected 'False|Remise' in output, got: {}",
                t.text
            );
        } else {
            panic!("expected TextPart output");
        }
    }

    // Valeurs par défaut — is_resumed=False sur un premier appel

    #[test]
    fn test_ac4_aip_task_default_not_resumed() {
        // GIVEN an AIPTask created normally (first call, no resume)
        let task = AIPTask::default();

        // WHEN we inspect is_resumed and input_response
        // THEN is_resumed == false, input_response == None
        assert!(!task.is_resumed, "is_resumed must be false by default");
        assert!(
            task.input_response.is_none(),
            "input_response must be None by default"
        );
    }

    // InputResponseData serializable en JSON (roundtrip)

    #[test]
    fn test_ac5_input_response_json_roundtrip() {
        // GIVEN an InputResponseData with all fields populated
        let original = InputResponseData {
            approved: true,
            reason: None,
            context: serde_json::json!({"n": 42, "label": "test"}),
            responded_at: "2026-03-09T10:00:00Z".into(),
        };

        // WHEN we serialise then deserialise via serde_json
        let json = serde_json::to_string(&original).expect("serialise failed");
        let restored: InputResponseData = serde_json::from_str(&json).expect("deserialise failed");

        // THEN the roundtrip is lossless
        assert_eq!(restored.approved, original.approved);
        assert_eq!(restored.reason, original.reason);
        assert_eq!(restored.context, original.context);
        assert_eq!(restored.responded_at, original.responded_at);
    }
}
