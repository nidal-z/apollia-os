//! Bridge Tokio <-> asyncio for calling Python agent methods from Rust async.
//!
//! Uses `tokio::task::spawn_blocking` + `asyncio.run()` to execute Python
//! coroutines without blocking Tokio workers.
//!
//! The GIL is only held on the blocking thread pool, never on Tokio workers.
//!
//! ## Dispatch contract
//!
//! Every Apollia agent **must** be built with the decorator SDK
//! (`@agent`, `@skill`, `@on_message`, `@orchestrated`). The decorator
//! attaches `__apollia_dispatch__(task, ctx) -> dict` to the class, and the
//! bridge invokes that hook directly. There is no fallback to a raw
//! `run(task, ctx)` method: agents missing the dispatch attribute are
//! considered invalid at the bridge boundary and surface a
//! [`AIPBridgeError::Internal`] error.

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

    /// The Python `run()` coroutine exceeded the configured wall-clock budget.
    ///
    /// The underlying `spawn_blocking` task is detached when this fires:
    /// the Tokio runtime cannot preempt synchronous Python code holding the GIL.
    #[error("agent run() exceeded wall-clock limit of {secs}s")]
    WallClockTimeout { secs: u64 },
}

/// Default wall-clock budget applied to `call_run()` when none is configured.
const DEFAULT_WALL_CLOCK_SECS: u64 = 300;

/// Bridge between the Tokio runtime and a Python asyncio agent.
///
/// Wraps a validated Python agent and provides async Rust methods
/// to call the decorator-installed dispatch hook and `on_plan_complete()`.
///
/// Uses `tokio::task::spawn_blocking` to move Python execution off
/// Tokio worker threads.
pub struct AIPBridge {
    /// The Python agent object.
    agent: Py<PyAny>,
    /// Whether the agent has an `on_plan_complete` hook.
    has_on_plan_complete: bool,
    /// Working directory for `APOLLIA.md` discovery; `None` if not configured.
    ///
    /// When set, `call_run()` fetches `APOLLIA.md` asynchronously and injects its content
    /// into `ctx.workspace.apollia_md` before invoking the agent dispatch.
    cwd: Option<std::path::PathBuf>,
    /// Wall-clock budget enforced around `agent.run()`; falls back to
    /// [`DEFAULT_WALL_CLOCK_SECS`] when `None`.
    wall_clock_secs: Option<u64>,
}

impl AIPBridge {
    /// Creates a new bridge from a validated agent.
    ///
    /// # Errors
    ///
    /// Returns `AIPBridgeError::Internal` if the validated agent is missing
    /// `__apollia_dispatch__`; the bridge does not support legacy agents.
    pub fn new(validated: ValidatedAgent) -> Result<Self, AIPBridgeError> {
        Python::with_gil(|py| {
            if !validated
                .object
                .bind(py)
                .hasattr("__apollia_dispatch__")
                .map_err(|e| AIPBridgeError::Internal(format!("hasattr failed: {e}")))?
            {
                return Err(AIPBridgeError::Internal(
                    "agent is missing __apollia_dispatch__ - apply the @agent decorator"
                        .to_string(),
                ));
            }
            Ok(())
        })?;

        Ok(Self {
            agent: validated.object,
            has_on_plan_complete: validated.has_on_plan_complete,
            cwd: None,
            wall_clock_secs: None,
        })
    }

    /// Overrides the wall-clock budget enforced around `agent.run()`.
    ///
    /// Defaults to [`DEFAULT_WALL_CLOCK_SECS`] (5 minutes) when not set.
    /// Exceeding the budget yields [`AIPBridgeError::WallClockTimeout`].
    pub fn with_wall_clock_secs(mut self, secs: u64) -> Self {
        self.wall_clock_secs = Some(secs);
        self
    }

    /// Configures the working directory for `APOLLIA.md` discovery.
    ///
    /// When set, `call_run()` looks up `APOLLIA.md` from `cwd` upward and injects
    /// its content into `ctx.workspace.apollia_md` before calling the Python agent.
    /// Fails silently if `APOLLIA.md` is not found; the agent call continues unchanged.
    pub fn with_cwd(mut self, cwd: std::path::PathBuf) -> Self {
        self.cwd = Some(cwd);
        self
    }

    /// Returns `true` if the agent exposes an `on_plan_complete()` hook.
    ///
    /// Detected at validation time via `hasattr` Python duck typing.
    pub fn has_on_plan_complete(&self) -> bool {
        self.has_on_plan_complete
    }

    /// Calls `agent.__apollia_dispatch__(task, ctx)` asynchronously.
    ///
    /// The dispatch hook is installed by the `@agent` decorator and routes
    /// the incoming task to the right `@skill` / `@on_message` /
    /// `@orchestrated` handler before building the AIPResult dict.
    ///
    /// The GIL is only held on the blocking thread pool, not on Tokio workers.
    ///
    /// # Errors
    ///
    /// - `SerializationError` if `AIPTask` cannot be converted to a dict
    /// - `PythonException` if the Python code raises an exception
    /// - `DeserializationError` if the result cannot become `AIPResult`
    /// - `Internal` if the agent has no `__apollia_dispatch__` hook
    /// - `WallClockTimeout` if `agent.run()` exceeds the configured wall-clock budget
    ///   (defaults to [`DEFAULT_WALL_CLOCK_SECS`])
    pub async fn call_run(
        &self,
        task: &AIPTask,
        ctx: PyObject,
    ) -> Result<AIPResult, AIPBridgeError> {
        let task_json = serde_json::to_string(task)
            .map_err(|e| AIPBridgeError::SerializationError(e.to_string()))?;
        let agent = Python::with_gil(|py| self.agent.clone_ref(py));

        // Collect full workspace snapshot asynchronously (before spawn_blocking) if cwd is configured.
        let workspace_snapshot: Option<apollia_workspace::WorkspaceSnapshot> =
            if let Some(ref cwd) = self.cwd {
                let runtime = apollia_workspace::ProjectRuntime::default_project();
                Some(runtime.collect(cwd).await)
            } else {
                None
            };

        let wall_clock_secs = self.wall_clock_secs.unwrap_or(DEFAULT_WALL_CLOCK_SECS);
        let wall_clock = std::time::Duration::from_secs(wall_clock_secs);

        let blocking = tokio::task::spawn_blocking(move || {
            Python::with_gil(|py| -> Result<String, AIPBridgeError> {
                // 1. Inject workspace snapshot, wall-clock budget and logger
                //    wiring into the RuntimeContext before dispatch.
                inject_runtime_context(py, &ctx, workspace_snapshot.as_ref(), wall_clock_secs);

                // 2. Deserialise AIPTask into a Python dict.
                let task_dict = json_loads(py, &task_json)
                    .map_err(|e| AIPBridgeError::SerializationError(e.to_string()))?;

                // 3. Dispatch routing: every agent must expose
                //    `__apollia_dispatch__`, installed by the @agent
                //    decorator. The hook routes the task to the matching
                //    @skill / @on_message / @orchestrated handler and
                //    builds the AIPResult dict.
                let agent_bound = agent.bind(py);
                let coroutine = agent_bound
                    .call_method1("__apollia_dispatch__", (task_dict, ctx))
                    .map_err(|e| AIPBridgeError::PythonException(format!("{e}")))?;

                let result = run_coroutine(py, &coroutine)?;

                py_obj_to_json_string(py, &result)
            })
        });

        let result_json = match tokio::time::timeout(wall_clock, blocking).await {
            Ok(Ok(inner)) => inner?,
            Ok(Err(join_err)) => {
                return Err(AIPBridgeError::Internal(format!(
                    "spawn_blocking failed: {join_err}"
                )));
            }
            Err(_elapsed) => {
                // The blocking task is detached: Python is still executing on the
                // blocking thread until it returns. We cannot safely call
                // `py.check_signals()` from here because the GIL is held by that
                // thread, so we'd deadlock. The agent process will reclaim the
                // thread once `asyncio.run()` returns naturally.
                return Err(AIPBridgeError::WallClockTimeout {
                    secs: wall_clock_secs,
                });
            }
        };

        serde_json::from_str(&result_json)
            .map_err(|e| AIPBridgeError::DeserializationError(e.to_string()))
    }

    /// Calls `agent.on_plan_complete(step_results, ctx)` asynchronously.
    ///
    /// Converts `step_results: HashMap<String, String>` into a Python `dict[str, str]`
    /// via [`PyDict`], then calls the Python coroutine via `asyncio.run()`.
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
}

/// Injects the workspace snapshot, wall-clock budget and Python logger wiring
/// into the `RuntimeContext` bound to `ctx`, prior to dispatch.
///
/// All steps fail silently: an agent that does not depend on `ctx.workspace`,
/// `ctx.budget` or `ctx.logger` keeps running unchanged.
fn inject_runtime_context(
    py: Python<'_>,
    ctx: &PyObject,
    workspace_snapshot: Option<&apollia_workspace::WorkspaceSnapshot>,
    wall_clock_secs: u64,
) {
    // 1a. Inject full workspace snapshot into ctx.workspace if available.
    if let Some(snapshot) = workspace_snapshot {
        if let Ok(ctx_bound) = ctx.bind(py).downcast::<crate::context::RuntimeContext>() {
            let ws_py = crate::context::WorkspaceContextPy::from_snapshot(snapshot);
            if let Ok(new_ws) = pyo3::Py::new(py, ws_py) {
                ctx_bound.borrow_mut().workspace = Some(new_ws);
            }
        }
    }

    // 1b. Propagate the resolved wall_clock_secs (manifest to bridge) into the
    // RuntimeContext so it feeds `ctx.budget.wall_clock_remaining`. The bridge
    // already holds the value as a `Duration`; we push it onto ctx so the
    // `budget` getter can read it.
    if let Ok(ctx_bound) = ctx.bind(py).downcast::<crate::context::RuntimeContext>() {
        let mut rc = ctx_bound.borrow_mut();
        if rc.wall_clock_secs.is_none() {
            rc.wall_clock_secs = Some(wall_clock_secs);
        }
    }

    // 1c. Wire the Python logger to pipe records into `ctx.log(level, msg)`
    // (which emits Rust tracing + bus). Configured once per task rather than
    // lazily on every `ctx.logger` access (more predictable, single handler).
    // Fails silently: an agent that does not depend on `ctx.logger` keeps
    // running.
    let _ = (|| -> PyResult<()> {
        let bridge_mod = py.import("apollia._internal.logger_bridge")?;
        let configure = bridge_mod.getattr("configure_agent_logger")?;
        // Read agent_name from ctx to name the logger.
        let agent_name: String = ctx
            .bind(py)
            .getattr("agent_name")?
            .extract()
            .unwrap_or_else(|_| "unknown".to_string());
        configure.call1((ctx.bind(py), agent_name))?;
        Ok(())
    })();
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
