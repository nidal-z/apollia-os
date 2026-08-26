//! The `ToolProxy` pyclass and the executor behind it.
//!
//! Split out of `context.rs`: the runtime context stays in the parent, the
//! proxy an agent reaches through `ctx.tools` and its dispatcher-backed
//! executor live here.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Instant;

use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;

use apollia_tools::{AuditTrailHandle, ToolDescriptor, ToolDispatcher, ToolRegistryHandle};

use crate::context::tool_invoke::{
    emit_a2a_completion_events, emit_tool_completion_events, execute_tool, extract_a2a_skill_id,
    invoke_a2a_tool, json_value_to_py, A2ACompletionEvent, A2AInvokeContext, ToolCallContext,
    ToolCompletionEvent,
};

/// Errors from tool invocation via the proxy.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ToolProxyError {
    /// The requested tool was not found in the registry.
    #[error("tool not found: '{0}'")]
    ToolNotFound(String),

    /// The agent is not allowed to use this tool.
    #[error("tool '{0}' not allowed for this agent")]
    ToolNotAllowed(String),

    /// The tool execution failed.
    #[error("tool execution failed: {0}")]
    ExecutionFailed(String),
}
/// Trait abstracting tool execution for testability.
///
/// Concrete implementations dispatch to native tools (FileIo, BashExecutor, etc.).
/// Tests use a mock executor.
pub trait ToolExecutor: Send + Sync {
    /// Executes the named tool with the given JSON input.
    fn execute(
        &self,
        tool_name: &str,
        input: serde_json::Value,
    ) -> Result<serde_json::Value, String>;
}
/// Sync adapter that exposes an `apollia_tools::ToolDispatcher` through the
/// [`ToolExecutor`] trait consumed by [`ToolProxy`].
///
/// Python agents use the sync [`ToolExecutor`] trait, while the dispatcher is
/// async. This adapter bridges the two by calling `block_in_place` +
/// `block_on`; we're always invoked from inside the Tokio runtime because
/// `ToolProxy::call` runs on a PyO3-async future.
///
/// Per-agent instance: the wrapped dispatcher holds per-agent state
/// (sandbox root, memory namespace, ask_user pending registry).
pub struct DispatcherExecutor {
    pub(crate) dispatcher: Arc<ToolDispatcher>,
}
impl DispatcherExecutor {
    /// Wrap *dispatcher* in a sync [`ToolExecutor`] facade.
    pub fn new(dispatcher: Arc<ToolDispatcher>) -> Self {
        Self { dispatcher }
    }
}
impl ToolExecutor for DispatcherExecutor {
    fn execute(
        &self,
        tool_name: &str,
        input: serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        let dispatcher = Arc::clone(&self.dispatcher);
        let name = tool_name.to_string();
        tokio::task::block_in_place(move || {
            tokio::runtime::Handle::current()
                .block_on(async move { dispatcher.dispatch(&name, input).await })
                .map_err(|e| e.to_string())
        })
    }
}
/// Converts a [`ToolDescriptor`] into a [`serde_json::Value`] for serialization to Python.
///
/// Pure function, testable without PyO3 or the GIL. Returns a JSON object
/// with keys: `name`, `version`, `description`, `input_schema`, `output_schema`, `tags`.
pub fn describe_inner(descriptor: &ToolDescriptor) -> serde_json::Value {
    serde_json::json!({
        "name": descriptor.name,
        "version": descriptor.version,
        "description": descriptor.description,
        "input_schema": descriptor.input_schema,
        "output_schema": descriptor.output_schema,
        "tags": descriptor.tags,
    })
}
/// Python-facing proxy exposing Rust tools to an agent.
///
/// Each agent receives its own `ToolProxy` instance configured with
/// the tools allowed by its manifest. All calls are permission-checked,
/// audited, and counted.
///
/// When configured via [`with_a2a`], tool names prefixed with `"a2a:"` are
/// routed to the [`A2AInvoker`] instead of the native [`ToolExecutor`].
///
/// [`with_a2a`]: ToolProxy::with_a2a
#[pyclass]
pub struct ToolProxy {
    pub(crate) registry: ToolRegistryHandle,
    pub(crate) audit: AuditTrailHandle,
    pub(crate) executor: Arc<dyn ToolExecutor>,
    pub(crate) allowed_tools: Vec<String>,
    pub(crate) agent_id: String,
    pub(crate) task_id: String,
    pub(crate) run_id: Option<apollia_core::events::RunId>,
    pub(crate) tool_calls: AtomicU32,
    /// A2A invoker for routing `"a2a:{skill_id}"` tool calls; `None` if not configured.
    pub(crate) a2a_invoker: Option<Arc<apollia_runtime::a2a::A2AInvoker>>,
    /// Current A2A recursion depth for the owning agent (0 = direct invocation).
    pub(crate) a2a_depth: u32,
    /// Cumulative deadline for the current A2A chain; `None` before the first invocation.
    pub(crate) chain_deadline: Option<Instant>,
    /// Event bus to emit `ToolCallStarted/Completed/Denied`,
    /// `A2AInvokeStarted/Completed`. `None` disables runtime observability
    /// without breaking dispatch.
    pub(crate) event_bus: Option<apollia_core::events::EventBusSender>,
    /// Live view of the runtime `StepBudget`, shared with the engine. Present
    /// only on the Direct path, where the agent's tool calls are counted and
    /// cut off here (principle #7). `None` in orchestrated mode, whose ActorLoop
    /// accounts the budget itself, so the counter is never charged twice.
    pub(crate) budget: Option<Arc<apollia_llm::StepBudgetView>>,
}
#[pymethods]
impl ToolProxy {
    /// Calls a tool by name with a Python dict as input.
    ///
    /// Returns a Python awaitable that resolves to a dict.
    /// Checks permissions, looks up the tool, executes it,
    /// records an audit entry, and increments the call counter.
    pub(crate) fn call<'py>(
        &self,
        py: Python<'py>,
        tool_name: String,
        input: PyObject,
    ) -> PyResult<Bound<'py, PyAny>> {
        // Convert Python dict -> JSON string -> serde_json::Value
        let json_mod = py
            .import("json")
            .map_err(|e| PyRuntimeError::new_err(format!("failed to import json: {e}")))?;
        let input_str: String = json_mod
            .call_method1("dumps", (input.bind(py),))
            .map_err(|e| PyRuntimeError::new_err(format!("json.dumps failed: {e}")))?
            .extract()
            .map_err(|e| PyRuntimeError::new_err(format!("extract failed: {e}")))?;
        let input_value: serde_json::Value = serde_json::from_str(&input_str)
            .map_err(|e| PyRuntimeError::new_err(format!("JSON parse failed: {e}")))?;

        // Direct-path budget enforcement: count this tool call against the shared
        // runtime budget and deny once the tool-call (or step) dimension is spent.
        if let Some(reason) = self.reject_if_budget_exhausted() {
            return Err(PyRuntimeError::new_err(reason));
        }

        self.tool_calls.fetch_add(1, Ordering::Relaxed);

        // Emit ToolCallStarted. The event_id is generated here to serve as the
        // parent of the future ToolCallCompleted/Denied. `event_id` is also
        // used as the A2A invoke identifier on the A2A chain side.
        let started_event_id = uuid::Uuid::now_v7().to_string();
        if let Some(bus) = self.event_bus.as_ref() {
            let _ = bus.send(apollia_core::events::RuntimeEvent::ToolCallStarted {
                event_id: started_event_id.clone(),
                task_id: self.task_id.clone().into(),
                agent_id: self.agent_id.clone().into(),
                tool_name: tool_name.clone(),
                args_json: Some(input_str.clone()),
                run_id: self.run_id.clone(),
            });
        }

        // A2A path: intercept before registry lookup.
        // Accept both `a2a:{skill_id}` (legacy, Anthropic-compatible) and
        // `a2a__{skill_id_with_dots_replaced_by_double_underscore}` (new,
        // OpenAI-compatible; see `A2AInterface::skill_as_tool`).
        if let Some(skill_id) = extract_a2a_skill_id(&tool_name) {
            if !self.allowed_tools.iter().any(|t| t == &tool_name) {
                // Emit ToolCallDenied before returning.
                if let Some(bus) = self.event_bus.as_ref() {
                    let _ = bus.send(apollia_core::events::RuntimeEvent::ToolCallDenied {
                        parent_event_id: started_event_id.clone(),
                        task_id: self.task_id.clone().into(),
                        agent_id: self.agent_id.clone().into(),
                        tool_name: tool_name.clone(),
                        reason: "not_in_manifest".to_string(),
                        detail: None,
                    });
                }
                return Err(PyRuntimeError::new_err(
                    ToolProxyError::ToolNotAllowed(tool_name).to_string(),
                ));
            }
            let invoker = self.a2a_invoker.clone().ok_or_else(|| {
                PyRuntimeError::new_err("A2A invoker not configured for this agent")
            })?;
            let caller = self.agent_id.clone();
            let a2a_depth = self.a2a_depth;
            let chain_deadline = self.chain_deadline;

            // Companion A2AInvokeStarted: `event_id` shared with
            // ToolCallStarted, fresh `correlation_id` to start the chain.
            let a2a_correlation_id = uuid::Uuid::now_v7().to_string();
            if let Some(bus) = self.event_bus.as_ref() {
                let _ = bus.send(apollia_core::events::RuntimeEvent::A2AInvokeStarted {
                    event_id: started_event_id.clone(),
                    correlation_id: a2a_correlation_id,
                    task_id: self.task_id.clone().into(),
                    caller_agent_id: caller.clone().into(),
                    skill_id: skill_id.clone(),
                    child_task_id: None, // not yet propagated.
                });
            }

            let bus_for_async = self.event_bus.clone();
            let task_id_for_async = self.task_id.clone();
            let agent_id_for_async = self.agent_id.clone();
            let run_id_for_async = self.run_id.clone();
            let parent_id = started_event_id.clone();
            let tool_name_for_async = tool_name.clone();
            let skill_id_for_async = skill_id.clone();

            return pyo3_async_runtimes::tokio::future_into_py(py, async move {
                let started_at = std::time::Instant::now();
                let result = invoke_a2a_tool(
                    &A2AInvokeContext {
                        invoker: invoker.as_ref(),
                        skill_id: &skill_id_for_async,
                        caller: &caller,
                        a2a_depth,
                        chain_deadline,
                    },
                    input_value,
                )
                .await;
                let duration_ms = started_at.elapsed().as_millis() as u64;

                emit_a2a_completion_events(
                    bus_for_async.as_ref(),
                    &result,
                    A2ACompletionEvent {
                        parent_id,
                        task_id: task_id_for_async,
                        agent_id: agent_id_for_async,
                        tool_name: tool_name_for_async,
                        skill_id: skill_id_for_async,
                        duration_ms,
                        run_id: run_id_for_async,
                    },
                );

                let result = result.map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
                json_value_to_py(&result)
            });
        }

        // Clone fields for the 'static async future
        let registry = self.registry.clone();
        let audit = self.audit.clone();
        let executor = Arc::clone(&self.executor);
        let allowed = self.allowed_tools.clone();
        let agent_id = self.agent_id.clone();
        let task_id = self.task_id.clone();
        let run_id = self.run_id.clone();
        let bus_for_async = self.event_bus.clone();
        let started_event_id_clone = started_event_id.clone();
        let tool_name_for_async = tool_name.clone();

        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let started_at = std::time::Instant::now();
            let result = execute_tool(
                &ToolCallContext {
                    registry: &registry,
                    audit: &audit,
                    executor: &executor,
                    allowed_tools: &allowed,
                    agent_id: &agent_id,
                    task_id: &task_id,
                    run_id: run_id.as_ref().map(|r| r.as_str()),
                },
                &tool_name,
                input_value,
            )
            .await;
            let duration_ms = started_at.elapsed().as_millis() as u64;

            // Emit ToolCallCompleted or ToolCallDenied depending on the
            // outcome. Pairs with the ToolCallStarted emitted before dispatch.
            emit_tool_completion_events(
                bus_for_async.as_ref(),
                &result,
                ToolCompletionEvent {
                    parent_id: started_event_id_clone,
                    task_id,
                    agent_id,
                    tool_name: tool_name_for_async,
                    duration_ms,
                    run_id,
                },
            );

            match result {
                Ok(value) => json_value_to_py(&value),
                Err(e) => Err(PyRuntimeError::new_err(e.to_string())),
            }
        })
    }

    /// Lists the tools available to this agent.
    pub(crate) fn list_tools(&self) -> Vec<String> {
        self.allowed_tools.clone()
    }

    /// Returns the number of tool calls made so far.
    pub(crate) fn tool_call_count(&self) -> u32 {
        self.tool_calls.load(Ordering::Relaxed)
    }

    /// Returns the JSON schema of a tool by name, or `None` if the tool is not registered.
    ///
    /// Returns a Python awaitable that resolves to a dict with keys
    /// `name`, `version`, `description`, `input_schema`, `output_schema`, `tags`,
    /// or `None` if the tool does not exist in the registry.
    pub(crate) fn describe<'py>(
        &self,
        py: Python<'py>,
        name: String,
    ) -> PyResult<Bound<'py, PyAny>> {
        let registry = self.registry.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let descriptor = registry.describe(&name).await;
            match descriptor {
                Some(desc) => {
                    let value = describe_inner(&desc);
                    let json_str = serde_json::to_string(&value).map_err(|e| {
                        PyRuntimeError::new_err(format!("serialization error: {e}"))
                    })?;
                    Python::with_gil(|py| {
                        let json_mod = py
                            .import("json")
                            .map_err(|e| PyRuntimeError::new_err(format!("import json: {e}")))?;
                        let py_obj: PyObject = json_mod
                            .call_method1("loads", (json_str,))
                            .map_err(|e| PyRuntimeError::new_err(format!("json.loads: {e}")))?
                            .unbind();
                        Ok(py_obj)
                    })
                }
                None => Ok(Python::with_gil(|py| py.None())),
            }
        })
    }
}
/// Construction parameters for [`ToolProxy::new`].
///
/// Groups the proxy's required dependencies so that callers build it from a
/// single value (see SonarQube rust:S107, too many function arguments).
pub struct ToolProxyConfig {
    /// Tool registry handle for descriptor lookup.
    pub registry: ToolRegistryHandle,
    /// Audit trail handle for recording invocations.
    pub audit: AuditTrailHandle,
    /// Concrete tool executor.
    pub executor: Arc<dyn ToolExecutor>,
    /// Tool names the owning agent is permitted to call.
    pub allowed_tools: Vec<String>,
    /// Identifier of the owning agent.
    pub agent_id: String,
    /// Identifier of the current task.
    pub task_id: String,
    /// Run this task belongs to, for audit correlation of tool events.
    pub run_id: Option<apollia_core::events::RunId>,
}

// The methods in this block are called from Python via PyO3 (not from Rust),
// so the Rust compiler wrongly considers them dead code.
#[allow(
    dead_code,
    reason = "PyO3-exposed surface, called from Python at runtime so the Rust compiler cannot see the call sites"
)]
impl ToolProxy {
    /// Creates a new ToolProxy for an agent.
    ///
    /// Called by the runtime when constructing a `RuntimeContext` for a task.
    pub fn new(config: ToolProxyConfig) -> Self {
        let ToolProxyConfig {
            registry,
            audit,
            executor,
            allowed_tools,
            agent_id,
            task_id,
            run_id,
        } = config;
        Self {
            registry,
            audit,
            executor,
            allowed_tools,
            agent_id,
            task_id,
            run_id,
            tool_calls: AtomicU32::new(0),
            a2a_invoker: None,
            a2a_depth: 0,
            chain_deadline: None,
            event_bus: None,
            budget: None,
        }
    }

    /// Wires the live `StepBudget` view enforced on the Direct path.
    ///
    /// Each `call` then charges one tool-call unit against the shared budget and
    /// is denied once `max_tool_calls` (or the step dimension) is reached. Left
    /// unset in orchestrated mode so the ActorLoop remains the sole accountant.
    pub fn with_budget(mut self, budget: Arc<apollia_llm::StepBudgetView>) -> Self {
        self.budget = Some(budget);
        self
    }

    /// Wires the EventBus to emit observability events
    /// (`ToolCallStarted/Completed/Denied`, `A2AInvokeStarted/Completed`).
    ///
    /// Without a bus, dispatch works identically but no runtime trace is
    /// produced; useful for isolated test contexts.
    pub fn with_event_bus(mut self, bus: apollia_core::events::EventBusSender) -> Self {
        self.event_bus = Some(bus);
        self
    }

    /// Configures A2A routing on this proxy, enabling `"a2a:{skill_id}"` tool calls.
    ///
    /// Must be called after [`new`] when the owning agent has `supports_a2a: true`
    /// and an [`A2AInvoker`] is available. Returns `self` for ergonomic chaining.
    ///
    /// - `invoker`: high-level A2A orchestrator.
    /// - `a2a_depth`: current recursion depth (0 for direct invocations).
    /// - `chain_deadline`: cumulative chain deadline propagated from the parent invocation.
    pub fn with_a2a(
        mut self,
        invoker: Arc<apollia_runtime::a2a::A2AInvoker>,
        a2a_depth: u32,
        chain_deadline: Option<Instant>,
    ) -> Self {
        self.a2a_invoker = Some(invoker);
        self.a2a_depth = a2a_depth;
        self.chain_deadline = chain_deadline;
        self
    }

    /// Invoke a tool by name from Rust, without PyO3, returning its JSON output.
    ///
    /// Runs the exact same governance as the Python-facing `call`: permission
    /// check, registry lookup, execution, audit recording, and tool-call
    /// counting. Used by the ORIA orchestrated `ActorLoop` to execute plan tool
    /// steps under the same authority as a direct agent.
    pub async fn invoke_native(
        &self,
        tool_name: &str,
        input: serde_json::Value,
    ) -> Result<serde_json::Value, ToolProxyError> {
        self.call_inner(tool_name, input).await
    }

    /// Return the JSON input schema of `tool_name`, when the registry knows it.
    ///
    /// Reads the tool descriptor from the registry and returns its
    /// `input_schema`. Used by the orchestrated `ActorLoop` to resolve a step's
    /// structured arguments against the target tool's schema.
    pub async fn tool_input_schema(&self, tool_name: &str) -> Option<serde_json::Value> {
        self.registry
            .describe(tool_name)
            .await
            .map(|descriptor| descriptor.input_schema)
    }

    /// Core tool execution logic, testable without PyO3.
    ///
    /// Performs permission check, registry lookup, execution, and audit recording.
    /// Increments the tool call counter.
    pub(crate) async fn call_inner(
        &self,
        tool_name: &str,
        input: serde_json::Value,
    ) -> Result<serde_json::Value, ToolProxyError> {
        // Budget enforcement for a Direct-path proxy carrying a live budget. In
        // orchestrated mode `budget` is None and the ActorLoop accounts instead,
        // so this never double-counts.
        if let Some(reason) = self.reject_if_budget_exhausted() {
            return Err(ToolProxyError::ExecutionFailed(reason));
        }

        self.tool_calls.fetch_add(1, Ordering::Relaxed);

        if let Some(skill_id) = extract_a2a_skill_id(tool_name) {
            if !self.allowed_tools.iter().any(|t| t == tool_name) {
                return Err(ToolProxyError::ToolNotAllowed(tool_name.to_string()));
            }
            let invoker = self.a2a_invoker.as_ref().ok_or_else(|| {
                ToolProxyError::ExecutionFailed(
                    "A2A invoker not configured for this agent".to_string(),
                )
            })?;
            return invoke_a2a_tool(
                &A2AInvokeContext {
                    invoker: invoker.as_ref(),
                    skill_id: &skill_id,
                    caller: &self.agent_id,
                    a2a_depth: self.a2a_depth,
                    chain_deadline: self.chain_deadline,
                },
                input,
            )
            .await;
        }

        execute_tool(
            &ToolCallContext {
                registry: &self.registry,
                audit: &self.audit,
                executor: &self.executor,
                allowed_tools: &self.allowed_tools,
                agent_id: &self.agent_id,
                task_id: &self.task_id,
                run_id: self.run_id.as_ref().map(|r| r.as_str()),
            },
            tool_name,
            input,
        )
        .await
    }

    /// Charge one tool-call unit against the live budget, if one is wired.
    ///
    /// Returns `Some(reason)` when the call must be denied because the tool-call
    /// or step dimension is already spent; otherwise increments the shared
    /// tool-call counter and returns `None`. No-op (returns `None`) when no
    /// budget is attached, which is the orchestrated case.
    pub(crate) fn reject_if_budget_exhausted(&self) -> Option<String> {
        let budget = self.budget.as_ref()?;
        if budget.tool_calls_remaining() == 0 {
            return Some("step budget exhausted: max_tool_calls reached".to_string());
        }
        if budget.is_exhausted() {
            return Some("step budget exhausted: max_steps reached".to_string());
        }
        budget.increment_tool_calls();
        None
    }
}
