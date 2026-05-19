//! ToolProxy — Python-facing proxy for invoking Rust tools.
//!
//! Exposes a `#[pyclass]` that agents use via `ctx.tools.call(tool_name, input)`.
//! Handles permission checks, registry lookup, tool execution, audit logging,
//! and tool call counting.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Instant;

use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;

use apollia_tools::{
    compute_input_hash, AuditTrailHandle, ToolDescriptor, ToolDispatcher, ToolInvocationRecord,
    ToolRegistryHandle,
};

/// Errors from tool invocation via the proxy.
#[derive(Debug, thiserror::Error)]
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

/// Trait abstracting tool execution for testability (ADR-015).
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
/// `block_on` — we're always invoked from inside the Tokio runtime because
/// `ToolProxy::call` runs on a PyO3-async future.
///
/// Per-agent instance: the wrapped dispatcher holds per-agent state
/// (sandbox root, memory namespace, ask_user pending registry).
pub struct DispatcherExecutor {
    dispatcher: Arc<ToolDispatcher>,
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
/// Pure function — testable without PyO3 or the GIL. Returns a JSON object
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
    registry: ToolRegistryHandle,
    audit: AuditTrailHandle,
    executor: Arc<dyn ToolExecutor>,
    allowed_tools: Vec<String>,
    agent_id: String,
    task_id: String,
    tool_calls: AtomicU32,
    /// A2A invoker for routing `"a2a:{skill_id}"` tool calls — `None` if not configured.
    a2a_invoker: Option<Arc<apollia_runtime::a2a::A2AInvoker>>,
    /// Current A2A recursion depth for the owning agent (0 = direct invocation).
    a2a_depth: u32,
    /// Cumulative deadline for the current A2A chain — `None` before the first invocation.
    chain_deadline: Option<Instant>,
    /// Event bus pour émettre `ToolCallStarted/Completed/Denied`,
    /// `A2AInvokeStarted/Completed` (ADR-088, Lot 2). `None` désactive
    /// l'observabilité runtime sans casser le dispatch.
    event_bus: Option<apollia_core::events::EventBusSender>,
}

#[pymethods]
impl ToolProxy {
    /// Calls a tool by name with a Python dict as input.
    ///
    /// Returns a Python awaitable that resolves to a dict.
    /// Checks permissions, looks up the tool, executes it,
    /// records an audit entry, and increments the call counter.
    fn call<'py>(
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

        self.tool_calls.fetch_add(1, Ordering::Relaxed);

        // ADR-088 — Émission ToolCallStarted (Lot 2). L'event_id est généré
        // ici pour servir de parent au futur ToolCallCompleted/Denied.
        // `event_id` est aussi utilisé comme identifiant de l'A2A invoke
        // côté chaîne A2A.
        let started_event_id = uuid::Uuid::now_v7().to_string();
        if let Some(bus) = self.event_bus.as_ref() {
            let _ = bus.send(apollia_core::events::RuntimeEvent::ToolCallStarted {
                event_id: started_event_id.clone(),
                task_id: self.task_id.clone().into(),
                agent_id: self.agent_id.clone().into(),
                tool_name: tool_name.clone(),
                args_json: Some(input_str.clone()),
            });
        }

        // A2A path: intercept before registry lookup
        if let Some(skill_id_ref) = tool_name.strip_prefix("a2a:") {
            if !self.allowed_tools.iter().any(|t| t == &tool_name) {
                // Émettre ToolCallDenied avant de retourner.
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
            let skill_id = skill_id_ref.to_string();
            let invoker = self.a2a_invoker.clone().ok_or_else(|| {
                PyRuntimeError::new_err("A2A invoker not configured for this agent")
            })?;
            let caller = self.agent_id.clone();
            let a2a_depth = self.a2a_depth;
            let chain_deadline = self.chain_deadline;

            // Companion A2AInvokeStarted — `event_id` partagé avec
            // ToolCallStarted, `correlation_id` neuf pour démarrer la chaîne.
            let a2a_correlation_id = uuid::Uuid::now_v7().to_string();
            if let Some(bus) = self.event_bus.as_ref() {
                let _ = bus.send(apollia_core::events::RuntimeEvent::A2AInvokeStarted {
                    event_id: started_event_id.clone(),
                    correlation_id: a2a_correlation_id,
                    task_id: self.task_id.clone().into(),
                    caller_agent_id: caller.clone().into(),
                    skill_id: skill_id.clone(),
                    child_task_id: None, // Lot 2 : non encore propagé.
                });
            }

            let bus_for_async = self.event_bus.clone();
            let task_id_for_async = self.task_id.clone();
            let agent_id_for_async = self.agent_id.clone();
            let parent_id = started_event_id.clone();
            let tool_name_for_async = tool_name.clone();
            let skill_id_for_async = skill_id.clone();

            return pyo3_async_runtimes::tokio::future_into_py(py, async move {
                let started_at = std::time::Instant::now();
                let result = invoke_a2a_tool(
                    &invoker,
                    &skill_id_for_async,
                    input_value,
                    &caller,
                    a2a_depth,
                    chain_deadline,
                )
                .await;
                let duration_ms = started_at.elapsed().as_millis() as u64;

                match &result {
                    Ok(value) => {
                        if let Some(bus) = bus_for_async.as_ref() {
                            let output_str = serde_json::to_string(value).ok();
                            let summary = output_str.as_deref().map(|s| {
                                let mut out = s.chars().take(200).collect::<String>();
                                if s.chars().count() > 200 {
                                    out.push('…');
                                }
                                out
                            });
                            let _ = bus.send(
                                apollia_core::events::RuntimeEvent::ToolCallCompleted {
                                    parent_event_id: parent_id.clone(),
                                    task_id: task_id_for_async.clone().into(),
                                    agent_id: agent_id_for_async.clone().into(),
                                    tool_name: tool_name_for_async.clone(),
                                    output_json: output_str,
                                    exit_code: None,
                                    duration_ms,
                                    success: true,
                                },
                            );
                            let _ = bus.send(
                                apollia_core::events::RuntimeEvent::A2AInvokeCompleted {
                                    parent_event_id: parent_id.clone(),
                                    task_id: task_id_for_async.into(),
                                    skill_id: skill_id_for_async,
                                    success: true,
                                    output_summary: summary,
                                    duration_ms,
                                },
                            );
                        }
                    }
                    Err(e) => {
                        if let Some(bus) = bus_for_async.as_ref() {
                            let _ = bus.send(
                                apollia_core::events::RuntimeEvent::ToolCallCompleted {
                                    parent_event_id: parent_id.clone(),
                                    task_id: task_id_for_async.clone().into(),
                                    agent_id: agent_id_for_async.into(),
                                    tool_name: tool_name_for_async,
                                    output_json: Some(format!("{{\"error\":{:?}}}", e.to_string())),
                                    exit_code: None,
                                    duration_ms,
                                    success: false,
                                },
                            );
                            let _ = bus.send(
                                apollia_core::events::RuntimeEvent::A2AInvokeCompleted {
                                    parent_event_id: parent_id,
                                    task_id: task_id_for_async.into(),
                                    skill_id: skill_id_for_async,
                                    success: false,
                                    output_summary: Some(e.to_string()),
                                    duration_ms,
                                },
                            );
                        }
                    }
                }

                let result = result.map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
                let json_str = serde_json::to_string(&result)
                    .map_err(|e| PyRuntimeError::new_err(format!("result serialization: {e}")))?;
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
            });
        }

        // Clone fields for the 'static async future
        let registry = self.registry.clone();
        let audit = self.audit.clone();
        let executor = Arc::clone(&self.executor);
        let allowed = self.allowed_tools.clone();
        let agent_id = self.agent_id.clone();
        let task_id = self.task_id.clone();
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
                },
                &tool_name,
                input_value,
            )
            .await;
            let duration_ms = started_at.elapsed().as_millis() as u64;

            // ADR-088 — Émission ToolCallCompleted ou ToolCallDenied selon
            // l'issue. Sandwich avec le ToolCallStarted émis avant dispatch.
            if let Some(bus) = bus_for_async.as_ref() {
                match &result {
                    Ok(value) => {
                        let _ = bus.send(
                            apollia_core::events::RuntimeEvent::ToolCallCompleted {
                                parent_event_id: started_event_id_clone.clone(),
                                task_id: task_id.clone().into(),
                                agent_id: agent_id.clone().into(),
                                tool_name: tool_name_for_async.clone(),
                                output_json: serde_json::to_string(value).ok(),
                                exit_code: None,
                                duration_ms,
                                success: true,
                            },
                        );
                    }
                    Err(ToolProxyError::ToolNotAllowed(name)) => {
                        let _ = bus.send(
                            apollia_core::events::RuntimeEvent::ToolCallDenied {
                                parent_event_id: started_event_id_clone.clone(),
                                task_id: task_id.clone().into(),
                                agent_id: agent_id.clone().into(),
                                tool_name: name.clone(),
                                reason: "not_in_manifest".to_string(),
                                detail: None,
                            },
                        );
                    }
                    Err(e) => {
                        let _ = bus.send(
                            apollia_core::events::RuntimeEvent::ToolCallCompleted {
                                parent_event_id: started_event_id_clone.clone(),
                                task_id: task_id.clone().into(),
                                agent_id: agent_id.clone().into(),
                                tool_name: tool_name_for_async.clone(),
                                output_json: Some(format!(
                                    "{{\"error\":{:?}}}",
                                    e.to_string()
                                )),
                                exit_code: None,
                                duration_ms,
                                success: false,
                            },
                        );
                    }
                }
            }

            match result {
                Ok(value) => {
                    let json_str = serde_json::to_string(&value).map_err(|e| {
                        PyRuntimeError::new_err(format!("result serialization: {e}"))
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
                Err(e) => Err(PyRuntimeError::new_err(e.to_string())),
            }
        })
    }

    /// Lists the tools available to this agent.
    fn list_tools(&self) -> Vec<String> {
        self.allowed_tools.clone()
    }

    /// Returns the number of tool calls made so far.
    fn tool_call_count(&self) -> u32 {
        self.tool_calls.load(Ordering::Relaxed)
    }

    /// Returns the JSON schema of a tool by name, or `None` if the tool is not registered.
    ///
    /// Returns a Python awaitable that resolves to a dict with keys
    /// `name`, `version`, `description`, `input_schema`, `output_schema`, `tags`,
    /// or `None` if the tool does not exist in the registry.
    fn describe<'py>(&self, py: Python<'py>, name: String) -> PyResult<Bound<'py, PyAny>> {
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

// Les méthodes de ce bloc sont appelées depuis Python via PyO3 (pas depuis Rust),
// ce qui fait que le compilateur Rust les considère comme du code mort à tort.
#[allow(dead_code)]
impl ToolProxy {
    /// Creates a new ToolProxy for an agent.
    ///
    /// Called by the runtime when constructing a `RuntimeContext` for a task.
    pub fn new(
        registry: ToolRegistryHandle,
        audit: AuditTrailHandle,
        executor: Arc<dyn ToolExecutor>,
        allowed_tools: Vec<String>,
        agent_id: String,
        task_id: String,
    ) -> Self {
        Self {
            registry,
            audit,
            executor,
            allowed_tools,
            agent_id,
            task_id,
            tool_calls: AtomicU32::new(0),
            a2a_invoker: None,
            a2a_depth: 0,
            chain_deadline: None,
            event_bus: None,
        }
    }

    /// Branche l'EventBus pour émettre les événements d'observabilité Lot 2
    /// (`ToolCallStarted/Completed/Denied`, `A2AInvokeStarted/Completed`).
    ///
    /// Sans bus, le dispatch fonctionne identiquement mais aucun trace
    /// runtime n'est produit — utile pour les contextes de test isolés.
    pub fn with_event_bus(mut self, bus: apollia_core::events::EventBusSender) -> Self {
        self.event_bus = Some(bus);
        self
    }

    /// Configures A2A routing on this proxy, enabling `"a2a:{skill_id}"` tool calls.
    ///
    /// Must be called after [`new`] when the owning agent has `supports_a2a: true`
    /// and an [`A2AInvoker`] is available. Returns `self` for ergonomic chaining.
    ///
    /// - `invoker` — high-level A2A orchestrator.
    /// - `a2a_depth` — current recursion depth (0 for direct invocations).
    /// - `chain_deadline` — cumulative chain deadline propagated from the parent invocation.
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

    /// Core tool execution logic — testable without PyO3.
    ///
    /// Performs permission check, registry lookup, execution, and audit recording.
    /// Increments the tool call counter.
    pub(crate) async fn call_inner(
        &self,
        tool_name: &str,
        input: serde_json::Value,
    ) -> Result<serde_json::Value, ToolProxyError> {
        self.tool_calls.fetch_add(1, Ordering::Relaxed);

        if let Some(skill_id) = tool_name.strip_prefix("a2a:") {
            if !self.allowed_tools.iter().any(|t| t == tool_name) {
                return Err(ToolProxyError::ToolNotAllowed(tool_name.to_string()));
            }
            let invoker = self.a2a_invoker.as_ref().ok_or_else(|| {
                ToolProxyError::ExecutionFailed(
                    "A2A invoker not configured for this agent".to_string(),
                )
            })?;
            return invoke_a2a_tool(
                invoker,
                skill_id,
                input,
                &self.agent_id,
                self.a2a_depth,
                self.chain_deadline,
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
            },
            tool_name,
            input,
        )
        .await
    }
}

/// Routes an `"a2a:{skill_id}"` tool call to the [`A2AInvoker`].
///
/// Calls `invoker.invoke()` with `a2a_depth + 1` and formats the result as
/// `"[{skill_id} via {agent_name}]\n{output_text}"` where `output_text` is
/// the concatenation of all [`AIPPart::Text`] parts from the worker's output.
///
/// Returns `ToolProxyError::ExecutionFailed` if the invocation fails.
async fn invoke_a2a_tool(
    invoker: &apollia_runtime::a2a::A2AInvoker,
    skill_id: &str,
    input: serde_json::Value,
    caller: &str,
    a2a_depth: u32,
    chain_deadline: Option<Instant>,
) -> Result<serde_json::Value, ToolProxyError> {
    let result = invoker
        .invoke(
            skill_id,
            input,
            caller,
            a2a_depth.saturating_add(1),
            None,
            chain_deadline,
        )
        .await
        .map_err(|e| ToolProxyError::ExecutionFailed(e.to_string()))?;

    let output_text: String = result
        .result
        .output
        .iter()
        .filter_map(|part| match part {
            AIPPart::Text(t) => Some(t.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n");

    let formatted = format!("[{skill_id} via {}]\n{output_text}", result.agent_name);
    Ok(serde_json::json!({ "text": formatted }))
}

/// Grouped parameters for [`execute_tool`] to avoid too many function arguments.
struct ToolCallContext<'a> {
    registry: &'a ToolRegistryHandle,
    audit: &'a AuditTrailHandle,
    executor: &'a Arc<dyn ToolExecutor>,
    allowed_tools: &'a [String],
    agent_id: &'a str,
    task_id: &'a str,
}

/// Shared tool execution logic used by both the Python `call()` and Rust `call_inner()`.
async fn execute_tool(
    ctx: &ToolCallContext<'_>,
    tool_name: &str,
    input: serde_json::Value,
) -> Result<serde_json::Value, ToolProxyError> {
    let start = Instant::now();
    let input_hash = compute_input_hash(&input);
    let args_json = serde_json::to_string(&input).ok();

    // 1. Check permission BEFORE registry lookup (don't reveal tool existence)
    if !ctx.allowed_tools.iter().any(|t| t == tool_name) {
        let duration = start.elapsed();
        emit_audit_record(
            ctx,
            tool_name,
            &input_hash,
            "unknown",
            AuditOutcome {
                duration_ms: duration.as_millis() as u64,
                success: false,
                error_code: Some(ToolProxyError::ToolNotAllowed(tool_name.to_string()).to_string()),
                args_json,
                stdout: None,
                stderr: None,
            },
        );
        return Err(ToolProxyError::ToolNotAllowed(tool_name.to_string()));
    }

    // 2. Lookup in registry
    let descriptor = ctx
        .registry
        .get(tool_name)
        .await
        .map_err(|e| ToolProxyError::ExecutionFailed(e.to_string()))?
        .ok_or_else(|| ToolProxyError::ToolNotFound(tool_name.to_string()))?;

    let sandbox_profile = format!("{:?}", descriptor.sandbox_profile);

    // 3. Execute the tool
    let exec_result = ctx.executor.execute(tool_name, input);
    let duration = start.elapsed();

    let (success, error_code, stdout, stderr) = match &exec_result {
        Ok(val) => (true, None, serde_json::to_string(val).ok(), None),
        Err(e) => (false, Some(e.clone()), None, Some(e.clone())),
    };

    // 4. Record audit (always, success or failure)
    emit_audit_record(
        ctx,
        tool_name,
        &input_hash,
        &sandbox_profile,
        AuditOutcome {
            duration_ms: duration.as_millis() as u64,
            success,
            error_code,
            args_json,
            stdout,
            stderr,
        },
    );

    // 5. Return result
    exec_result.map_err(ToolProxyError::ExecutionFailed)
}

/// Outcome fields for an audit record, grouped to keep `emit_audit_record` under 7 params.
struct AuditOutcome {
    duration_ms: u64,
    success: bool,
    error_code: Option<String>,
    args_json: Option<String>,
    stdout: Option<String>,
    stderr: Option<String>,
}

/// Records a tool invocation in the audit trail (fire-and-forget).
fn emit_audit_record(
    ctx: &ToolCallContext<'_>,
    tool_name: &str,
    input_hash: &str,
    sandbox_profile: &str,
    outcome: AuditOutcome,
) {
    ctx.audit.record(ToolInvocationRecord {
        id: uuid::Uuid::new_v4().to_string(),
        agent_id: ctx.agent_id.to_string(),
        task_id: ctx.task_id.to_string(),
        tool_name: tool_name.to_string(),
        input_hash: input_hash.to_string(),
        sandbox_profile: sandbox_profile.to_string(),
        started_at: now_rfc3339(),
        duration_ms: Some(outcome.duration_ms),
        exit_code: None,
        success: outcome.success,
        error_code: outcome.error_code,
        resources_used: None,
        args_json: outcome.args_json,
        stdout: outcome.stdout,
        stderr: outcome.stderr,
    });
}

/// Returns the current UTC time as an RFC3339 string.
fn now_rfc3339() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let (year, month, day) = epoch_secs_to_ymd(secs);
    let day_secs = (secs % 86400) as u32;
    let h = day_secs / 3600;
    let m = (day_secs % 3600) / 60;
    let s = day_secs % 60;

    format!("{year:04}-{month:02}-{day:02}T{h:02}:{m:02}:{s:02}Z")
}

/// Converts epoch seconds to (year, month, day).
fn epoch_secs_to_ymd(secs: u64) -> (i32, u32, u32) {
    let mut days = (secs / 86400) as i64;
    let mut year = 1970i32;

    loop {
        let days_in_year: i64 = if is_leap(year) { 366 } else { 365 };
        if days < days_in_year {
            break;
        }
        days -= days_in_year;
        year += 1;
    }

    let month_lengths: [i64; 12] = if is_leap(year) {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };

    let mut month = 1u32;
    for &ml in &month_lengths {
        if days < ml {
            break;
        }
        days -= ml;
        month += 1;
    }

    (year, month, days as u32 + 1)
}

/// Returns `true` if the given year is a leap year.
fn is_leap(y: i32) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

// ─────────────────────────────────────────────
// WorkspaceContextPy
// ─────────────────────────────────────────────

/// Contexte workspace exposé à l'agent Python via `ctx.workspace`.
///
/// Agrège les sections produites par tous les [`WorkspaceProvider`]s actifs.
/// Accessible en lecture seule depuis Python via des méthodes `#[pymethods]`.
///
/// ## API Python
/// ```python
/// ctx.workspace.rules           # contenu des règles projet (APOLLIA.md)
/// ctx.workspace.apollia_md      # alias pour rules (compatibilité)
/// ctx.workspace.get("Git")      # contenu d'une section par titre
/// ctx.workspace.sections        # liste de dicts {"title": ..., "content": ...}
/// ```
#[pyclass]
pub struct WorkspaceContextPy {
    /// Sections aplaties de tous les providers : (titre, contenu).
    pub sections: Vec<(String, String)>,
}

impl WorkspaceContextPy {
    /// Construit depuis un [`WorkspaceSnapshot`] collecté par `ProjectRuntime`.
    pub fn from_snapshot(snapshot: &apollia_workspace::WorkspaceSnapshot) -> Self {
        let sections = snapshot
            .slices
            .iter()
            .flat_map(|s| &s.sections)
            .map(|s| (s.title.clone(), s.content.clone()))
            .collect();
        Self { sections }
    }

    /// Construit un contexte vide (aucune section).
    pub fn empty() -> Self {
        Self { sections: vec![] }
    }

    /// Injecte ou remplace le contenu d'une section par son titre.
    ///
    /// Utilisé par le bridge pour patcher les règles projet après collecte asynchrone.
    pub fn set_section(&mut self, title: &str, content: String) {
        if let Some(existing) = self.sections.iter_mut().find(|(t, _)| t == title) {
            existing.1 = content;
        } else {
            self.sections.push((title.to_owned(), content));
        }
    }

    /// Retourne le contenu d'une section par titre (lookup Rust-side).
    pub fn get_section_content(&self, title: &str) -> Option<&str> {
        self.sections
            .iter()
            .find(|(t, _)| t == title)
            .map(|(_, c)| c.as_str())
    }
}

#[pymethods]
impl WorkspaceContextPy {
    /// Contenu des règles projet (section "Règles du projet"), ou `None` si absent.
    #[getter]
    fn rules(&self) -> Option<&str> {
        self.get_section_content("Règles du projet")
    }

    /// Alias pour `rules` — compatibilité avec les agents existants.
    #[getter]
    fn apollia_md(&self) -> Option<&str> {
        self.rules()
    }

    /// Retourne le contenu d'une section par son titre, ou `None` si introuvable.
    ///
    /// ```python
    /// git_info = ctx.workspace.get("Git")
    /// jira_tickets = ctx.workspace.get("Jira")
    /// ```
    fn get(&self, title: &str) -> Option<&str> {
        self.get_section_content(title)
    }

    /// Toutes les sections sous forme de liste de dicts `{"title": ..., "content": ...}`.
    #[getter]
    fn sections<'py>(&self, py: pyo3::Python<'py>) -> Vec<pyo3::Bound<'py, pyo3::types::PyDict>> {
        use pyo3::types::PyDict;
        self.sections
            .iter()
            .map(|(title, content)| {
                let d = PyDict::new(py);
                let _ = d.set_item("title", title);
                let _ = d.set_item("content", content);
                d
            })
            .collect()
    }
}

// ─────────────────────────────────────────────
// RuntimeContext
// ─────────────────────────────────────────────

use std::collections::HashMap;

use apollia_core::events::{AgentId, EventBusSender, RuntimeEvent};
use apollia_core::AIPPart;
use apollia_llm::{LlmRouter, ObservabilityConfig, StepBudgetView, ToolCallHelper};
use apollia_runtime::a2a::{A2AInvoker, A2aDelegateFn, A2aDelegateResult};
use apollia_runtime::mailbox::{AgentMailboxHandle, AgentMessage, MailboxError};

use crate::llm::LlmProxy;

/// Vue lecture-seule du budget d'exécution exposée à l'agent Python via `ctx.step_budget`.
///
/// Snapshot instantané au moment de l'accès. Les trois dimensions reflètent
/// les compteurs live du [`StepBudgetView`] Rust au moment de l'appel.
#[pyclass(frozen, name = "StepBudgetView")]
pub struct PyStepBudgetView {
    steps_remaining: i64,
    tool_calls_remaining: i64,
    elapsed_seconds: f64,
}

#[pymethods]
impl PyStepBudgetView {
    /// Steps restants avant d'atteindre la limite (`max_steps`).
    #[getter]
    fn steps_remaining(&self) -> i64 {
        self.steps_remaining
    }

    /// Appels outils restants avant d'atteindre `max_tool_calls`.
    #[getter]
    fn tool_calls_remaining(&self) -> i64 {
        self.tool_calls_remaining
    }

    /// Secondes écoulées depuis le démarrage de la tâche.
    #[getter]
    fn elapsed_seconds(&self) -> f64 {
        self.elapsed_seconds
    }
}

/// Contexte d'exécution exposé à l'agent Python via `run(task, ctx)`.
///
/// Construit par le runtime pour chaque exécution d'agent. Expose les
/// capacités optionnelles du runtime :
/// - `ctx.tools` — [`ToolProxy`] si des outils sont alloués à l'agent, `None` sinon.
/// - `ctx.llm` — [`LlmProxy`] si au moins un backend LLM est disponible,
///   `None` sinon (Principe #6 — l'agent choisit si l'absence est fatale).
/// - `ctx.memory` — [`crate::memory::MemoryInterface`] isolé par namespace si
///   `memory_namespace` est déclaré dans le manifest, `None` sinon (Principe #6).
///
/// L'absence de LLM est signalée sur l'EventBus via `AgentDegraded`
/// dès la construction (Principe #4 — fail fast).
#[pyclass(name = "RuntimeContext")]
pub struct RuntimeContext {
    /// Proxy outils exposé à Python — `None` si aucun outil alloué.
    tools: Option<pyo3::Py<ToolProxy>>,
    /// Proxy LLM exposé à Python — `None` si aucun backend LLM disponible.
    pub llm: Option<LlmProxy>,
    /// Interface mémoire isolée par namespace — `None` si `memory_namespace` absent du manifest.
    memory: Option<pyo3::Py<crate::memory::MemoryInterface>>,
    /// Handle vers l'AgentMailbox — `None` si le runtime n'a pas démarré de mailbox.
    mailbox: Option<AgentMailboxHandle>,
    /// Nom de l'agent propriétaire de ce contexte.
    agent_name: String,
    /// Indique si l'agent supporte le protocole A2A (from manifest).
    supports_a2a: bool,
    /// Mémoire utilisateur injectée en mode chat — `None` en mode task.
    ///
    /// Structure : `{"preferences": [("key", "value"), ...], "habits": [...], "context": [...]}`.
    /// L'agent décide quoi en faire — ce n'est jamais déterministe.
    user_context: Option<HashMap<String, Vec<(String, String)>>>,
    /// Fonction de délégation A2A type-erasée — `None` si le routing A2A n'est pas disponible.
    ///
    /// Injectée depuis `make_delegate_fn(registry, router, event_bus)` en production.
    /// Permet à un Director Agent d'appeler `ctx.delegate(skill_id, payload)` sans
    /// connaître le backend d'exécution sous-jacent.
    a2a_delegate: Option<A2aDelegateFn>,
    /// Orchestrateur A2A de haut niveau — `None` si non disponible dans ce contexte.
    ///
    /// Expose `ctx.a2a_invoke`, `ctx.a2a_discover`, `ctx.a2a_list_skills` aux agents Python.
    a2a_invoker: Option<Arc<A2AInvoker>>,
    /// Si `true`, l'agent peut écrire dans la mémoire utilisateur globale
    /// (`__user__`) via `ctx.memory.remember_user()`. Lecture du namespace
    /// `__user__` est, elle, toujours accessible — gérée par le
    /// [`MemoryInterface`] dès qu'un `user_manager` est fourni.
    ///
    /// Vrai uniquement pour les agents dont le manifest déclare
    /// `user_memory_write = true` (ex. `onboarding-agent`).
    user_memory_writable: bool,
    /// Profondeur courante dans la chaîne A2A (0 = invocation directe, pas via A2A).
    ///
    /// Incrémenté de 1 à chaque niveau d'invocation inter-agents.
    /// Transmis à [`A2AInvoker::invoke`] pour appliquer le garde-fou de récursivité.
    a2a_depth: u32,
    /// Deadline cumulé de la chaîne A2A courante.
    ///
    /// `None` avant la première invocation A2A depuis cet agent.
    /// Initialisé par l'`A2AInvoker` à la première invocation et propagé
    /// dans les invocations suivantes pour limiter le temps total de la chaîne.
    chain_deadline: Option<Instant>,
    /// Contexte workspace collecté au démarrage de la tâche.
    ///
    /// Peuplé par [`with_workspace_snapshot`](RuntimeContext::with_workspace_snapshot) ou via le bridge
    /// lors de `call_run()`. Expose `ctx.workspace.rules`, `ctx.workspace.get("Git")`, etc.
    /// `None` si le runtime n'a pas collecté le contexte workspace pour cette tâche.
    pub workspace: Option<Py<WorkspaceContextPy>>,
    /// Event bus cloné au construction — permet à `emit_token()` de pousser
    /// des `RuntimeEvent::ChatToken` vers le frontend SSE. `None` lorsque le
    /// contexte est construit hors chaîne d'événements (tests unitaires
    /// intra-crate).
    event_bus: Option<EventBusSender>,
    /// Identifiant de session à taguer sur les événements `ChatToken`.
    ///
    /// Injecté depuis `task.context_id` par le `BridgeRunner` en mode chat.
    /// `None` en mode task — `emit_token()` est alors un no-op.
    chat_session_id: Option<String>,
    /// Identifiant du message courant à taguer sur les événements `ChatToken`.
    ///
    /// Injecté depuis `task.message_id` par le `BridgeRunner` en mode chat.
    /// `None` en mode task.
    chat_message_id: Option<String>,
    /// Vue partagée du budget de steps — `None` en dehors d'une exécution budgétée.
    step_budget: Option<Arc<StepBudgetView>>,
    /// Interface de notification exposée à l'agent Python via `ctx.notify`.
    /// `None` si aucun canal de notification n'est configuré (opt-in).
    notify: Option<pyo3::Py<crate::notify::PyNotifyInterface>>,
    /// Interface STT exposée à l'agent Python via `ctx.stt`.
    /// `None` si aucun backend STT n'est configuré.
    stt: Option<pyo3::Py<crate::stt::PySttInterface>>,
    /// Interface profil utilisateur exposée à l'agent Python via `ctx.profile`.
    ///
    /// `None` quand le runtime n'a pas initialisé de `__user__` MemoryManager
    /// (tests, contextes minimaux).  Lecture toujours autorisée ; écriture
    /// gated par `user_memory_writable` (ADR-087).
    profile: Option<pyo3::Py<crate::profile::ProfileInterface>>,
    /// ID de l'agent propriétaire de ce contexte (UUID stable).
    ///
    /// Propagé dans la chaîne A2A (`AIPTask::delegation_chain`) lorsque cet agent
    /// délègue via `ctx.delegate()` (ADR-D7).
    agent_id: AgentId,
    /// Chaîne de délégation A2A héritée de la tâche en cours d'exécution.
    ///
    /// Vide pour une tâche racine. Étendue à chaque délégation par
    /// [`crate::context::RuntimeContext::delegate`] avant transmission au
    /// runtime (`a2a::validate_chain`).
    delegation_chain: Vec<AgentId>,
    /// Identifiant de la tâche courante — injecté par le backend au démarrage
    /// d'un `call_run` via [`with_task_id`](RuntimeContext::with_task_id).
    ///
    /// Utilisé par `ctx.log()` pour étiqueter le `RuntimeEvent::AgentLog` qui
    /// part vers le persistor `runtime_events` (ADR-088). `None` quand le
    /// contexte est construit pour des tests qui ne simulent pas une tâche.
    task_id: Option<String>,

    // ── LOT 4 — Nouvelles surfaces nestées (ADR-101) ────────────────────
    /// Façade A2A consolidée — `ctx.a2a` (ADR-102).
    ///
    /// Partage le même `Arc<A2AInvoker>` que [`Self::a2a_invoker`]. Construit
    /// systématiquement (l'agent voit toujours `ctx.a2a`, mais les méthodes
    /// lèvent `RuntimeError` quand l'invoker n'est pas disponible).
    a2a_iface: Option<Py<crate::a2a::A2AInterface>>,

    /// Surface d'émission d'événements typés — `ctx.events` (ADR-105).
    ///
    /// Construit systématiquement (au pire en mode no-op silencieux quand le
    /// bus est absent).
    events_iface: Option<Py<crate::events::EventsInterface>>,

    /// Interface datasources YAML — `ctx.datasources` (ADR-103).
    ///
    /// Construit systématiquement à partir de `manifest.datasources`. Si la
    /// liste déclarée est vide, l'interface est quand même présente mais
    /// `get()` lève toujours `FileNotFoundError("not declared")`.
    datasources_iface: Option<Py<crate::datasources::DatasourcesInterface>>,

    /// Interface templates Jinja2 — `ctx.templates` (ADR-103).
    templates_iface: Option<Py<crate::templates::TemplatesInterface>>,

    /// Interface secrets read-only — `ctx.secrets` (ADR-104).
    secrets_iface: Option<Py<crate::secrets::SecretsInterface>>,
}

impl RuntimeContext {
    /// Construit le contexte avec injection LLM optionnelle et ToolProxy optionnel.
    ///
    /// Si `llm_router` est `None` ou contient un router sans backend,
    /// `ctx.llm` est `None` et `RuntimeEvent::AgentDegraded` est émis
    /// fire-and-forget sur `event_bus` (erreurs `send()` silencieusement ignorées).
    ///
    /// Si `tool_proxy` est `Some`, `ctx.tools` expose les outils alloués à l'agent.
    ///
    /// Le contexte ne panic jamais à la construction : la dégradation est
    /// signalée, mais l'agent décide lui-même si l'absence de LLM est fatale.
    /// Construit le contexte avec injection LLM optionnelle, ToolProxy optionnel,
    /// et MemoryInterface optionnelle.
    ///
    /// Si `llm_router` est `None` ou contient un router sans backend,
    /// `ctx.llm` est `None` et `RuntimeEvent::AgentDegraded` est émis
    /// fire-and-forget sur `event_bus` (erreurs `send()` silencieusement ignorées).
    ///
    /// Si `tool_proxy` est `Some`, `ctx.tools` expose les outils alloués à l'agent.
    ///
    /// Si `memory_interface` est `Some`, `ctx.memory` expose la mémoire SQLite
    /// isolée du namespace déclaré dans le manifest (Principe #6).
    ///
    /// Le contexte ne panic jamais à la construction : la dégradation est
    /// signalée, mais l'agent décide lui-même si l'absence d'une capacité est fatale.
    #[allow(clippy::too_many_arguments)]
    pub fn new_with_llm(
        llm_router: Option<Arc<LlmRouter>>,
        budget_view: Arc<StepBudgetView>,
        tool_helper: Arc<ToolCallHelper>,
        obs_config: Arc<ObservabilityConfig>,
        event_bus: EventBusSender,
        agent_id: AgentId,
        tool_proxy: Option<ToolProxy>,
        memory_interface: Option<crate::memory::MemoryInterface>,
        mailbox: Option<AgentMailboxHandle>,
        agent_name: String,
        supports_a2a: bool,
        user_context: Option<HashMap<String, Vec<(String, String)>>>,
        a2a_delegate: Option<A2aDelegateFn>,
        a2a_invoker: Option<Arc<A2AInvoker>>,
        user_memory_writable: bool,
    ) -> Self {
        let bus_for_token = event_bus.clone();
        let step_budget_arc = Arc::clone(&budget_view);
        let agent_id_stored = agent_id.clone();
        let llm = llm_router.and_then(|router| {
            if router.list().is_empty() {
                // fire-and-forget — erreurs send() silencieusement ignorées.
                let _ = event_bus.send(RuntimeEvent::AgentDegraded {
                    agent_id,
                    reason: "no LLM backend available".into(),
                });
                None
            } else {
                Some(LlmProxy::new(
                    router,
                    tool_helper,
                    budget_view,
                    obs_config,
                    Some(event_bus),
                ))
            }
        });

        // Wrap ToolProxy in a Py<> handle so it can be cheaply cloned in the getter.
        let tools =
            tool_proxy.and_then(|proxy| pyo3::Python::with_gil(|py| pyo3::Py::new(py, proxy).ok()));

        // Wrap MemoryInterface in a Py<> handle if provided.
        let memory = memory_interface
            .and_then(|mem| pyo3::Python::with_gil(|py| pyo3::Py::new(py, mem).ok()));

        // ── LOT 4 — Construction des nouvelles surfaces nestées ────────
        // Toutes sont construites systématiquement (au pire en mode no-op)
        // pour que `ctx.a2a`, `ctx.events`, etc. soient toujours accessibles
        // sans branchement côté Python. Les valeurs de gating (datasources,
        // templates, secrets) sont par défaut vides — le bridge les
        // surchargera après lecture du manifest (LOT 5/6) via les builders
        // dédiés.
        let bus_for_iface = bus_for_token.clone();
        let agent_id_for_iface = agent_id_stored.clone();
        let agent_name_for_iface = agent_name.clone();
        let a2a_invoker_for_iface = a2a_invoker.clone();

        let (a2a_iface, events_iface, datasources_iface, templates_iface, secrets_iface) =
            pyo3::Python::with_gil(|py| {
                let a2a = crate::a2a::A2AInterface::new(
                    a2a_invoker_for_iface,
                    agent_name_for_iface,
                    0,
                    None,
                );
                let events = crate::events::EventsInterface::new(
                    Some(bus_for_iface),
                    None,
                    agent_id_for_iface,
                    None,
                    None,
                );
                let ds = crate::datasources::DatasourcesInterface::new(Vec::new());
                let tp = crate::templates::TemplatesInterface::new(Vec::new());
                let sc = crate::secrets::SecretsInterface::new(None, Vec::new());
                (
                    pyo3::Py::new(py, a2a).ok(),
                    pyo3::Py::new(py, events).ok(),
                    pyo3::Py::new(py, ds).ok(),
                    pyo3::Py::new(py, tp).ok(),
                    pyo3::Py::new(py, sc).ok(),
                )
            });

        Self {
            llm,
            tools,
            memory,
            mailbox,
            agent_name,
            supports_a2a,
            user_context,
            a2a_delegate,
            a2a_invoker,
            user_memory_writable,
            a2a_depth: 0,
            chain_deadline: None,
            workspace: None,
            event_bus: Some(bus_for_token),
            chat_session_id: None,
            chat_message_id: None,
            step_budget: Some(step_budget_arc),
            notify: None,
            stt: None,
            profile: None,
            agent_id: agent_id_stored,
            delegation_chain: Vec::new(),
            task_id: None,
            a2a_iface,
            events_iface,
            datasources_iface,
            templates_iface,
            secrets_iface,
        }
    }

    /// Construit un contexte minimal pour les tests unitaires intra-crate.
    ///
    /// Tous les champs optionnels sont à `None`. Ne doit jamais être appelé
    /// dans le code de production.
    #[cfg(test)]
    pub(crate) fn for_test() -> Self {
        let test_agent_id = AgentId::new_v4();
        let (a2a_iface, events_iface, datasources_iface, templates_iface, secrets_iface) =
            pyo3::Python::with_gil(|py| {
                let a2a = crate::a2a::A2AInterface::new(None, "test-agent".to_string(), 0, None);
                let events = crate::events::EventsInterface::new(
                    None,
                    None,
                    test_agent_id.clone(),
                    None,
                    None,
                );
                let ds = crate::datasources::DatasourcesInterface::new(Vec::new());
                let tp = crate::templates::TemplatesInterface::new(Vec::new());
                let sc = crate::secrets::SecretsInterface::new(None, Vec::new());
                (
                    pyo3::Py::new(py, a2a).ok(),
                    pyo3::Py::new(py, events).ok(),
                    pyo3::Py::new(py, ds).ok(),
                    pyo3::Py::new(py, tp).ok(),
                    pyo3::Py::new(py, sc).ok(),
                )
            });
        Self {
            tools: None,
            llm: None,
            memory: None,
            mailbox: None,
            agent_name: "test-agent".to_string(),
            supports_a2a: false,
            user_context: None,
            a2a_delegate: None,
            a2a_invoker: None,
            user_memory_writable: false,
            a2a_depth: 0,
            chain_deadline: None,
            workspace: None,
            event_bus: None,
            chat_session_id: None,
            chat_message_id: None,
            step_budget: None,
            notify: None,
            stt: None,
            profile: None,
            agent_id: test_agent_id,
            delegation_chain: Vec::new(),
            task_id: None,
            a2a_iface,
            events_iface,
            datasources_iface,
            templates_iface,
            secrets_iface,
        }
    }

    /// Injecte un [`WorkspaceSnapshot`] collecté dans ce `RuntimeContext`.
    ///
    /// Convertit le snapshot en [`WorkspaceContextPy`] accessible depuis Python
    /// via `ctx.workspace`. Doit être appelé après [`new_with_llm`](RuntimeContext::new_with_llm).
    pub fn with_workspace_snapshot(
        mut self,
        snapshot: &apollia_workspace::WorkspaceSnapshot,
    ) -> Self {
        let workspace_py = WorkspaceContextPy::from_snapshot(snapshot);
        self.workspace = pyo3::Python::with_gil(|py| pyo3::Py::new(py, workspace_py).ok());
        self
    }

    /// Initialise `ctx.workspace` avec un contexte vide (aucune section).
    ///
    /// Garantit que `ctx.workspace` n'est pas `None` même quand aucun provider
    /// n'est configuré. Le bridge peut ensuite patcher des sections via `set_section`.
    pub fn with_empty_workspace(mut self) -> Self {
        let workspace_py = WorkspaceContextPy::empty();
        self.workspace = pyo3::Python::with_gil(|py| pyo3::Py::new(py, workspace_py).ok());
        self
    }

    /// Attache une interface de notification à ce contexte.
    ///
    /// Appelé après [`new_with_llm`](RuntimeContext::new_with_llm) quand un
    /// `NotificationEngineHandle` est disponible. Si non appelé, `ctx.notify`
    /// est `None` (no-op silencieux côté Python).
    pub fn with_notify(mut self, notify: crate::notify::PyNotifyInterface) -> Self {
        self.notify =
            pyo3::Python::with_gil(|py| pyo3::Py::new(py, notify).ok());
        self
    }

    /// Attache une interface STT à ce contexte.
    ///
    /// Appelé après [`new_with_llm`](RuntimeContext::new_with_llm) quand un
    /// backend STT est disponible. Si non appelé, `ctx.stt` est `None`.
    pub fn with_stt(mut self, stt: crate::stt::PySttInterface) -> Self {
        self.stt =
            pyo3::Python::with_gil(|py| pyo3::Py::new(py, stt).ok());
        self
    }

    /// Attache une interface profil utilisateur à ce contexte (ADR-087).
    ///
    /// Appelé après [`new_with_llm`](RuntimeContext::new_with_llm) quand un
    /// `MemoryManager` ciblant `__user__` est disponible. Si non appelé,
    /// `ctx.profile` est `None`.
    pub fn with_profile(mut self, profile: crate::profile::ProfileInterface) -> Self {
        self.profile =
            pyo3::Python::with_gil(|py| pyo3::Py::new(py, profile).ok());
        self
    }

    /// Configure la cible des événements `ChatToken` émis par `emit_token()`.
    ///
    /// Appelé par le `BridgeRunner` en mode chat pour relier le contexte à la
    /// session + message en cours. Si non appelé (ex. mode task CLI),
    /// `emit_token()` est un no-op.
    pub fn with_chat_target(
        mut self,
        session_id: impl Into<String>,
        message_id: impl Into<String>,
    ) -> Self {
        self.chat_session_id = Some(session_id.into());
        self.chat_message_id = Some(message_id.into());
        self
    }

    /// Lie ce contexte à l'identifiant de la tâche courante.
    ///
    /// Doit être appelé avant `bridge.call_run()` pour que `ctx.log()` puisse
    /// étiqueter ses événements `RuntimeEvent::AgentLog` avec le bon
    /// `task_id` et qu'ils soient retrouvables dans la trace de la tâche
    /// (ADR-088).
    ///
    /// Effet de bord : propage aussi le `task_id` au [`LlmProxy`] interne pour
    /// l'émission des `LlmCallStarted` (Lot 2).
    pub fn with_task_id(mut self, task_id: impl Into<String>) -> Self {
        let task_id_str = task_id.into();
        // Propager au LlmProxy pour observabilité Lot 2.
        if let Some(llm) = self.llm.take() {
            self.llm = Some(llm.with_task_context(
                task_id_str.clone(),
                self.agent_id.to_string(),
            ));
        }
        // LOT 4 — re-construit EventsInterface en y injectant le task_id.
        // Le pyclass est immuable, on remplace donc le Py<> en place.
        let task_id_for_events = apollia_core::events::TaskId::from(task_id_str.clone());
        let agent_id_for_events = self.agent_id.clone();
        let bus_for_events = self.event_bus.clone();
        let chat_session = self.chat_session_id.clone();
        let chat_message = self.chat_message_id.clone();
        self.events_iface = pyo3::Python::with_gil(|py| {
            let new_iface = crate::events::EventsInterface::new(
                bus_for_events,
                Some(task_id_for_events),
                agent_id_for_events,
                chat_session,
                chat_message,
            );
            pyo3::Py::new(py, new_iface).ok()
        });
        self.task_id = Some(task_id_str);
        self
    }

    // ── LOT 4/5 — Builders pour les nouvelles surfaces nestées ──────────
    /// Branche l'interface datasources sur le contexte (LOT 5 — ADR-103).
    ///
    /// Construit une [`crate::datasources::DatasourcesInterface`] gating sur
    /// `declared` (typiquement `manifest.datasources`). Si `agent_dir` est
    /// `Some`, charge immédiatement les fichiers
    /// `<agent_dir>/datasources/<name>.yaml` dans le cache interne — c'est ce
    /// chemin qui est emprunté par la production. Pour les tests, passer
    /// `None` laisse le cache vide (toute lecture renverra
    /// `FileNotFoundError("not found on disk")`).
    ///
    /// Doit être appelé après [`new_with_llm`](RuntimeContext::new_with_llm).
    pub fn with_datasources(
        mut self,
        declared: Vec<String>,
        agent_dir: Option<&std::path::Path>,
    ) -> Self {
        let mut iface = crate::datasources::DatasourcesInterface::new(declared);
        if let Some(dir) = agent_dir {
            let _ = iface.load_from_dir(dir);
        }
        self.datasources_iface =
            pyo3::Python::with_gil(|py| pyo3::Py::new(py, iface).ok());
        self
    }

    /// Branche l'interface templates Jinja2 sur le contexte (LOT 5 — ADR-103).
    ///
    /// Construit une [`crate::templates::TemplatesInterface`] gating sur
    /// `declared` (typiquement `manifest.templates`). Si `agent_dir` est
    /// `Some`, compile immédiatement les templates Jinja depuis
    /// `<agent_dir>/templates/<name>.{j2,jinja2,jinja}`. Pour les tests, passer
    /// `None` laisse l'environnement minijinja vide.
    pub fn with_templates(
        mut self,
        declared: Vec<String>,
        agent_dir: Option<&std::path::Path>,
    ) -> Self {
        let mut iface = crate::templates::TemplatesInterface::new(declared);
        if let Some(dir) = agent_dir {
            let _ = iface.load_from_dir(dir);
        }
        self.templates_iface =
            pyo3::Python::with_gil(|py| pyo3::Py::new(py, iface).ok());
        self
    }

    /// Branche l'interface secrets sur le contexte (LOT 4/6).
    pub fn with_secrets(mut self, sc: crate::secrets::SecretsInterface) -> Self {
        self.secrets_iface = pyo3::Python::with_gil(|py| pyo3::Py::new(py, sc).ok());
        self
    }
}

#[pymethods]
impl RuntimeContext {
    /// Proxy outils injecté — `None` si aucun outil alloué.
    ///
    /// Propriété Python `ctx.tools`. Retourne `None` Python (pas d'exception)
    /// si l'agent n'a pas de `tools_required` ou que la factory n'a pas fourni de proxy.
    #[getter]
    fn tools(&self, py: Python<'_>) -> PyObject {
        match &self.tools {
            Some(proxy) => proxy.clone_ref(py).into_any(),
            None => py.None(),
        }
    }

    /// Proxy LLM injecté — `None` si aucun backend LLM disponible.
    ///
    /// Propriété Python `ctx.llm`. Retourne `None` Python (pas d'exception)
    /// si le runtime a démarré sans backend LLM configuré ou disponible.
    #[getter]
    fn llm(&self, py: Python<'_>) -> PyObject {
        match &self.llm {
            Some(proxy) => Py::new(py, proxy.clone())
                .map(|p| p.into_any())
                .unwrap_or_else(|_| py.None()),
            None => py.None(),
        }
    }

    /// Interface mémoire isolée par namespace.
    ///
    /// Propriété Python `ctx.memory`. Retourne `None` Python si le manifest
    /// de l'agent ne déclare pas de `memory_namespace`.
    #[getter]
    fn memory(&self, py: Python<'_>) -> PyObject {
        match &self.memory {
            Some(mem) => mem.clone_ref(py).into_any(),
            None => py.None(),
        }
    }

    /// Contexte workspace collecté au démarrage de la tâche.
    ///
    /// Propriété Python `ctx.workspace`. Expose `ctx.workspace.git_branch`,
    /// `ctx.workspace.git_is_clean`, `ctx.workspace.apollia_md`, etc.
    /// Retourne `None` Python si le runtime n'a pas collecté le contexte workspace.
    #[getter]
    fn workspace(&self, py: Python<'_>) -> PyObject {
        match &self.workspace {
            Some(ws) => ws.clone_ref(py).into_any(),
            None => py.None(),
        }
    }

    /// Interface de notification exposée à l'agent Python via `ctx.notify`.
    ///
    /// Propriété Python `ctx.notify`. Retourne `None` Python si aucun canal
    /// de notification n'est configuré (canaux opt-in par conception).
    #[getter]
    fn notify(&self, py: Python<'_>) -> PyObject {
        match &self.notify {
            Some(n) => n.clone_ref(py).into_any(),
            None => py.None(),
        }
    }

    /// Interface STT exposée à l'agent Python via `ctx.stt`.
    ///
    /// Propriété Python `ctx.stt`. Retourne `None` Python si le STT n'est pas configuré.
    #[getter]
    fn stt(&self, py: Python<'_>) -> PyObject {
        match &self.stt {
            Some(s) => s.clone_ref(py).into_any(),
            None => py.None(),
        }
    }

    /// Interface profil utilisateur exposée à l'agent Python via `ctx.profile` (ADR-087).
    ///
    /// Propriété Python `ctx.profile`.  Retourne `None` Python quand aucun
    /// `__user__` manager n'a été initialisé (tests, contextes minimaux).
    #[getter]
    fn profile(&self, py: Python<'_>) -> PyObject {
        match &self.profile {
            Some(p) => p.clone_ref(py).into_any(),
            None => py.None(),
        }
    }

    // ── LOT 4 — Getters pour les nouvelles surfaces nestées (ADR-101) ──
    /// Façade A2A consolidée — `ctx.a2a` (ADR-102).
    ///
    /// Retourne toujours une `A2AInterface` (jamais `None`) ; les méthodes
    /// internes lèvent `RuntimeError("A2A invoker not available …")` si le
    /// runtime n'a pas d'invoker actif. Cette uniformité évite à l'agent de
    /// brancher sur la présence de `ctx.a2a`.
    #[getter]
    fn a2a(&self, py: Python<'_>) -> PyObject {
        match &self.a2a_iface {
            Some(p) => p.clone_ref(py).into_any(),
            None => py.None(),
        }
    }

    /// Émission d'événements typés — `ctx.events` (ADR-105).
    #[getter]
    fn events(&self, py: Python<'_>) -> PyObject {
        match &self.events_iface {
            Some(p) => p.clone_ref(py).into_any(),
            None => py.None(),
        }
    }

    /// Datasources YAML lecture-seule — `ctx.datasources` (ADR-103).
    #[getter]
    fn datasources(&self, py: Python<'_>) -> PyObject {
        match &self.datasources_iface {
            Some(p) => p.clone_ref(py).into_any(),
            None => py.None(),
        }
    }

    /// Templates Jinja2 lecture-seule — `ctx.templates` (ADR-103).
    #[getter]
    fn templates(&self, py: Python<'_>) -> PyObject {
        match &self.templates_iface {
            Some(p) => p.clone_ref(py).into_any(),
            None => py.None(),
        }
    }

    /// Secrets lecture-seule avec gating manifest — `ctx.secrets` (ADR-104).
    #[getter]
    fn secrets(&self, py: Python<'_>) -> PyObject {
        match &self.secrets_iface {
            Some(p) => p.clone_ref(py).into_any(),
            None => py.None(),
        }
    }

    /// Vue lecture-seule du budget d'exécution — `ctx.budget` (ADR-101).
    ///
    /// Successeur typé de `ctx.step_budget` (qui reste fonctionnel et
    /// `#[deprecated]` jusqu'à LOT 9). Snapshot frais à chaque accès.
    #[getter]
    fn budget(&self, py: Python<'_>) -> PyResult<PyObject> {
        match &self.step_budget {
            Some(view) => {
                let bv = crate::budget::BudgetView::new(
                    view.steps_remaining(),
                    view.tool_calls_remaining(),
                    view.elapsed_secs(),
                    None,
                );
                Ok(Py::new(py, bv)?.into_any())
            }
            None => Ok(py.None()),
        }
    }

    /// Logger structuré pré-configuré pour cet agent — `ctx.logger`
    /// (ADR-106).
    ///
    /// Retourne `logging.getLogger("apollia.agent.{agent_id}")`. La
    /// configuration des handlers (relais tracing Rust, niveau, format) est
    /// gérée par le bootstrap SDK Python en LOT 8. Ici on garantit
    /// seulement que l'agent reçoit toujours un Logger nommé.
    #[getter]
    fn logger<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let logger_name = format!("apollia.agent.{}", self.agent_id);
        let logging = py.import("logging")?;
        logging.call_method1("getLogger", (logger_name,))
    }

    /// Émet un token de streaming vers le frontend en mode chat.
    ///
    /// Pousse un `RuntimeEvent::ChatToken { session_id, message_id, token }`
    /// sur l'event bus. Le filtrage SSE par `session_id` dans
    /// `routes_chat.rs` se charge du reste.
    ///
    /// No-op silencieux en mode task (quand `chat_session_id` ou
    /// `chat_message_id` est `None`), pour que les agents réutilisent le
    /// même code dans les deux modes sans garde explicite.
    ///
    /// Retourne `None` Python. Lève `RuntimeError` si la sérialisation
    /// échoue (ne devrait jamais arriver pour un `String`).
    fn emit_token(&self, token: String) -> PyResult<()> {
        let (session_id, message_id, bus) = match (
            self.chat_session_id.as_ref(),
            self.chat_message_id.as_ref(),
            self.event_bus.as_ref(),
        ) {
            (Some(sid), Some(mid), Some(bus)) => (sid.clone(), mid.clone(), bus.clone()),
            _ => return Ok(()),
        };
        // fire-and-forget : si le bus est saturé on drop silencieusement
        // plutôt que de propager une erreur au LLM qui n'y peut rien.
        let _ = bus.send(RuntimeEvent::ChatToken {
            session_id,
            message_id,
            token,
        });
        Ok(())
    }

    /// Contexte utilisateur injecté en mode chat — `None` en mode task.
    ///
    /// Propriété Python `ctx.user_context`. Retourne un `dict[str, list[tuple[str, str]]]`
    /// avec les catégories `preferences`, `habits`, `context`, ou `None` Python
    /// si l'agent n'est pas en mode chat ou si la mémoire utilisateur est vide.
    #[getter]
    fn user_context(&self, py: Python<'_>) -> PyObject {
        match &self.user_context {
            Some(ctx) => ctx.clone().into_pyobject(py).unwrap().into_any().unbind(),
            None => py.None(),
        }
    }

    /// Envoie un message à un autre agent via la mailbox inter-agents.
    ///
    /// Retourne un Python awaitable. Lève `RuntimeError` si `supports_a2a`
    /// est `false` dans le manifest ou si la mailbox n'est pas disponible.
    fn send<'py>(
        &self,
        py: Python<'py>,
        agent_name: String,
        message: PyObject,
    ) -> PyResult<Bound<'py, PyAny>> {
        if !self.supports_a2a {
            return Err(PyRuntimeError::new_err(
                "A2A messaging requires supports_a2a: true in manifest",
            ));
        }
        let mailbox = self.mailbox.clone().ok_or_else(|| {
            PyRuntimeError::new_err("A2A mailbox not available in this runtime context")
        })?;

        // Convert Python dict → JSON
        let json_mod = py
            .import("json")
            .map_err(|e| PyRuntimeError::new_err(format!("failed to import json: {e}")))?;
        let json_str: String = json_mod
            .call_method1("dumps", (message.bind(py),))
            .map_err(|e| PyRuntimeError::new_err(format!("json.dumps failed: {e}")))?
            .extract()
            .map_err(|e| PyRuntimeError::new_err(format!("extract failed: {e}")))?;
        let payload: serde_json::Value = serde_json::from_str(&json_str)
            .map_err(|e| PyRuntimeError::new_err(format!("JSON parse failed: {e}")))?;

        let from = self.agent_name.clone();

        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            send_inner(&mailbox, &from, &agent_name, payload)
                .await
                .map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
            Ok(Python::with_gil(|py| py.None()))
        })
    }

    /// Reçoit le prochain message en attente dans la mailbox de cet agent.
    ///
    /// Retourne un Python awaitable qui résout en `dict | None`.
    /// `timeout_seconds` défaut à 5.0 si absent.
    #[pyo3(signature = (timeout_seconds=None))]
    fn receive<'py>(
        &self,
        py: Python<'py>,
        timeout_seconds: Option<f64>,
    ) -> PyResult<Bound<'py, PyAny>> {
        if !self.supports_a2a {
            return Err(PyRuntimeError::new_err(
                "A2A messaging requires supports_a2a: true in manifest",
            ));
        }
        let mailbox = self.mailbox.clone().ok_or_else(|| {
            PyRuntimeError::new_err("A2A mailbox not available in this runtime context")
        })?;

        let agent_name = self.agent_name.clone();
        let timeout = std::time::Duration::from_secs_f64(timeout_seconds.unwrap_or(5.0));

        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let message = receive_inner(&mailbox, &agent_name, timeout).await;
            match message {
                Some(msg) => {
                    let value = serde_json::json!({
                        "from": msg.from,
                        "payload": msg.payload,
                        "sent_at": msg.sent_at,
                    });
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

    /// Délègue une tâche à un Worker Agent identifié par son skill ID.
    ///
    /// Retourne un Python awaitable qui résout en `dict` avec les clés
    /// `task_id`, `agent_name`, `output`.
    /// Lève `RuntimeError` si `supports_a2a` est `false` ou si la délégation
    /// A2A n'est pas disponible dans ce contexte d'exécution.
    #[pyo3(signature = (skill_id, payload, timeout_secs=None))]
    fn delegate<'py>(
        &self,
        py: Python<'py>,
        skill_id: String,
        payload: PyObject,
        timeout_secs: Option<u64>,
    ) -> PyResult<Bound<'py, PyAny>> {
        if !self.supports_a2a {
            return Err(PyRuntimeError::new_err(
                "A2A delegation requires supports_a2a: true in manifest",
            ));
        }
        let delegate_fn = self.a2a_delegate.clone().ok_or_else(|| {
            PyRuntimeError::new_err("A2A delegation not available in this runtime context")
        })?;

        // Convert Python dict → JSON Value.
        let json_mod = py
            .import("json")
            .map_err(|e| PyRuntimeError::new_err(format!("failed to import json: {e}")))?;
        let json_str: String = json_mod
            .call_method1("dumps", (payload.bind(py),))
            .map_err(|e| PyRuntimeError::new_err(format!("json.dumps failed: {e}")))?
            .extract()
            .map_err(|e| PyRuntimeError::new_err(format!("extract failed: {e}")))?;
        let input_value: serde_json::Value = serde_json::from_str(&json_str)
            .map_err(|e| PyRuntimeError::new_err(format!("JSON parse failed: {e}")))?;

        let timeout = timeout_secs.unwrap_or(120);
        let parent_chain = self.delegation_chain.clone();
        let current_agent = self.agent_id.clone();

        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let result: A2aDelegateResult =
                (delegate_fn)(skill_id, input_value, timeout, parent_chain, current_agent)
                    .await
                    .map_err(|e| PyRuntimeError::new_err(e.to_string()))?;

            let json_str = serde_json::to_string(&result)
                .map_err(|e| PyRuntimeError::new_err(format!("serialization error: {e}")))?;

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
        })
    }

    /// Invoque un Worker Agent par skill ID via l'[`A2AInvoker`] de haut niveau.
    ///
    /// Retourne un Python awaitable qui résout en `dict` avec les clés
    /// `result`, `agent_name`, `skill_id`, `duration_ms` en cas de succès,
    /// ou un dict `AIPResult` d'échec si une erreur survient (jamais d'exception Python).
    #[pyo3(signature = (skill_id, input, timeout_secs=None))]
    fn a2a_invoke<'py>(
        &self,
        py: Python<'py>,
        skill_id: String,
        input: PyObject,
        timeout_secs: Option<u64>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let invoker = self.a2a_invoker.clone().ok_or_else(|| {
            PyRuntimeError::new_err("A2A invoker not available in this runtime context")
        })?;

        let json_mod = py
            .import("json")
            .map_err(|e| PyRuntimeError::new_err(format!("failed to import json: {e}")))?;
        let json_str: String = json_mod
            .call_method1("dumps", (input.bind(py),))
            .map_err(|e| PyRuntimeError::new_err(format!("json.dumps failed: {e}")))?
            .extract()
            .map_err(|e| PyRuntimeError::new_err(format!("extract failed: {e}")))?;
        let input_value: serde_json::Value = serde_json::from_str(&json_str)
            .map_err(|e| PyRuntimeError::new_err(format!("JSON parse failed: {e}")))?;

        let caller = self.agent_name.clone();
        let timeout = timeout_secs.map(std::time::Duration::from_secs);
        let a2a_depth = self.a2a_depth.saturating_add(1);
        let chain_deadline = self.chain_deadline;

        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let out_json = match invoker
                .invoke(
                    &skill_id,
                    input_value,
                    &caller,
                    a2a_depth,
                    timeout,
                    chain_deadline,
                )
                .await
            {
                Ok(r) => serde_json::to_string(&r)
                    .map_err(|e| PyRuntimeError::new_err(format!("serialization error: {e}")))?,
                Err(e) => {
                    let failed =
                        apollia_core::AIPResult::failed("a2a_invoke_error", &e.to_string());
                    serde_json::to_string(&failed).map_err(|err| {
                        PyRuntimeError::new_err(format!("serialization error: {err}"))
                    })?
                }
            };

            Python::with_gil(|py| {
                let json_mod = py
                    .import("json")
                    .map_err(|e| PyRuntimeError::new_err(format!("import json: {e}")))?;
                let py_obj: PyObject = json_mod
                    .call_method1("loads", (out_json,))
                    .map_err(|e| PyRuntimeError::new_err(format!("json.loads: {e}")))?
                    .unbind();
                Ok(py_obj)
            })
        })
    }

    /// Découvre l'agent qui expose `skill_id` et retourne sa carte de découverte.
    ///
    /// Retourne un Python awaitable qui résout en `dict | None`.
    /// `None` si aucun agent disponible ne déclare le skill.
    fn a2a_discover<'py>(&self, py: Python<'py>, skill_id: String) -> PyResult<Bound<'py, PyAny>> {
        let invoker = self.a2a_invoker.clone().ok_or_else(|| {
            PyRuntimeError::new_err("A2A invoker not available in this runtime context")
        })?;

        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let card_opt = invoker
                .discover(&skill_id)
                .await
                .map_err(|e| PyRuntimeError::new_err(e.to_string()))?;

            match card_opt {
                None => Ok(Python::with_gil(|py| py.None())),
                Some(card) => {
                    let json_str = serde_json::to_string(&card).map_err(|e| {
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
            }
        })
    }

    /// Indique si cet agent peut écrire dans la mémoire utilisateur globale
    /// (`__user__`) via `ctx.memory.remember_user()`.
    ///
    /// Propriété Python `ctx.user_memory_writable`. La lecture de
    /// `__user__` est toujours disponible via le fallback de
    /// `ctx.memory.recall()` — ce drapeau ne contrôle que les écritures.
    #[getter]
    fn user_memory_writable(&self) -> bool {
        self.user_memory_writable
    }

    /// Log un message via le système `tracing::` du runtime.
    ///
    /// Les messages sont émis avec le champ `agent` pour corrélation dans les traces
    /// structurées. Niveaux acceptés : `"debug"`, `"info"`, `"warn"`, `"error"`.
    /// Lève `ValueError` pour tout autre niveau.
    ///
    /// **Effet de bord (ADR-088, Lot 1).** En plus de `tracing::*`, émet un
    /// `RuntimeEvent::AgentLog` sur l'`EventBus` si le contexte connaît son
    /// `task_id` et a un bus configuré. Le `EventPersistor` côté
    /// `apollia-runtime` consomme ces événements et les persiste dans
    /// `runtime_events.db`, ce qui les rend disponibles à l'API
    /// `GET /api/v1/tasks/{id}/trace` et à la nouvelle vue `ExecutionTrace`.
    /// Le `tracing::*` continue d'écrire vers stderr en parallèle pour la
    /// compatibilité ops.
    #[pyo3(text_signature = "(self, level, message)")]
    fn log(&self, level: &str, message: &str) -> PyResult<()> {
        let agent = &self.agent_name;
        match level {
            "debug" => tracing::debug!(agent = %agent, "{}", message),
            "info" => tracing::info!(agent = %agent, "{}", message),
            "warn" => tracing::warn!(agent = %agent, "{}", message),
            "error" => tracing::error!(agent = %agent, "{}", message),
            other => {
                return Err(pyo3::exceptions::PyValueError::new_err(format!(
                    "Invalid log level '{other}'. Expected: debug, info, warn, error"
                )))
            }
        }

        // Persistance via EventBus → EventPersistor (ADR-088).
        // Conditionnel : un contexte sans task_id (tests) ou sans bus (CLI
        // dry-run) reste silencieux côté trace mais émet bien dans tracing.
        if let (Some(task_id), Some(bus)) = (self.task_id.as_ref(), self.event_bus.as_ref()) {
            // fire-and-forget : un bus saturé est tracé par broadcast,
            // l'erreur send() est silencieusement ignorée pour ne jamais
            // bloquer le thread d'agent.
            let _ = bus.send(RuntimeEvent::AgentLog {
                task_id: task_id.clone().into(),
                agent_id: self.agent_id.clone(),
                level: level.to_string(),
                message: message.to_string(),
                extra_fields_json: None,
            });
        }

        Ok(())
    }

    /// Émet un événement `Thought` sur l'EventBus (ADR-088, Lot 2).
    ///
    /// Appelé par le SDK ReAct (`react.py`) à chaque tour, après le parsing
    /// JSON de l'action. Le payload `text` contient la `thought` brute
    /// extraite du LLM. No-op silencieux si `task_id` ou `event_bus` absents
    /// (mode test / dry-run).
    #[pyo3(text_signature = "(self, text, step_num)")]
    fn emit_thought(&self, text: String, step_num: u32) -> PyResult<()> {
        if let (Some(task_id), Some(bus)) = (self.task_id.as_ref(), self.event_bus.as_ref()) {
            let _ = bus.send(RuntimeEvent::Thought {
                task_id: task_id.clone().into(),
                agent_id: self.agent_id.clone(),
                step_num,
                text,
            });
        }
        Ok(())
    }

    /// Émet un événement `Retry` (ADR-088, Lot 2).
    ///
    /// Appelé par le SDK ReAct quand un step est retenté (parse error,
    /// tool error, llm error). `cause` doit être l'une des chaînes
    /// normalisées : `"action_parse_error" | "tool_error" | "llm_error"
    /// | "other"`.
    #[pyo3(text_signature = "(self, step_num, cause, attempt)")]
    fn emit_retry(&self, step_num: u32, cause: String, attempt: u32) -> PyResult<()> {
        if let (Some(task_id), Some(bus)) = (self.task_id.as_ref(), self.event_bus.as_ref()) {
            let _ = bus.send(RuntimeEvent::Retry {
                task_id: task_id.clone().into(),
                agent_id: self.agent_id.clone(),
                step_num,
                cause,
                attempt,
            });
        }
        Ok(())
    }

    /// Émet un événement `ActionParseError` (ADR-088, Lot 2).
    ///
    /// Appelé par le SDK ReAct quand le LLM renvoie un JSON action invalide
    /// non-réparable. Le payload `raw_content` permet au builder de voir
    /// exactement ce que le LLM a produit pour ajuster son system prompt.
    #[pyo3(text_signature = "(self, step_num, raw_content, repair_attempted)")]
    fn emit_action_parse_error(
        &self,
        step_num: u32,
        raw_content: String,
        repair_attempted: bool,
    ) -> PyResult<()> {
        if let (Some(task_id), Some(bus)) = (self.task_id.as_ref(), self.event_bus.as_ref()) {
            let _ = bus.send(RuntimeEvent::ActionParseError {
                task_id: task_id.clone().into(),
                agent_id: self.agent_id.clone(),
                step_num,
                raw_content,
                repair_attempted,
            });
        }
        Ok(())
    }

    /// Budget d'exécution restant pour la tâche en cours (lecture seule).
    ///
    /// Retourne un [`StepBudgetView`] avec `steps_remaining`, `tool_calls_remaining`,
    /// et `elapsed_seconds`. Retourne `None` Python si le contexte n'est pas budgété.
    #[getter]
    fn step_budget(&self, py: Python<'_>) -> PyResult<PyObject> {
        match &self.step_budget {
            Some(view) => {
                let py_view = PyStepBudgetView {
                    steps_remaining: view.steps_remaining(),
                    tool_calls_remaining: view.tool_calls_remaining(),
                    elapsed_seconds: view.elapsed_secs(),
                };
                Ok(Py::new(py, py_view)?.into_any())
            }
            None => Ok(py.None()),
        }
    }

    /// Liste tous les skills A2A disponibles dans le runtime.
    ///
    /// Retourne un Python awaitable qui résout en `list[dict]`.
    /// Chaque dict a les clés `skill_id`, `agent_name`, `skill_name`, `description`.
    fn a2a_list_skills<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let invoker = self.a2a_invoker.clone().ok_or_else(|| {
            PyRuntimeError::new_err("A2A invoker not available in this runtime context")
        })?;

        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let skills = invoker
                .list_skills()
                .await
                .map_err(|e| PyRuntimeError::new_err(e.to_string()))?;

            let json_str = serde_json::to_string(&skills)
                .map_err(|e| PyRuntimeError::new_err(format!("serialization error: {e}")))?;

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
        })
    }
}

/// Calcule le namespace mémoire effectif pour un agent dans un contexte de session.
///
/// Si l'agent tourne dans un projet (`project_id` non vide), le namespace est préfixé
/// avec le `project_id` pour garantir l'isolation entre projets.
///
/// Convention : `"{project_id}:{manifest_namespace}"` | `"{manifest_namespace}"`
///
/// Les `shared_memory_namespaces` ne sont PAS préfixés — ils sont globaux.
pub fn effective_memory_namespace(manifest_namespace: &str, project_id: Option<&str>) -> String {
    match project_id {
        Some(pid) if !pid.is_empty() => format!("{pid}:{manifest_namespace}"),
        _ => manifest_namespace.to_owned(),
    }
}

/// Envoie un message d'un agent à un autre via la mailbox — testable sans PyO3.
///
/// Wrapper fin autour de [`AgentMailboxHandle::send`].
pub(crate) async fn send_inner(
    mailbox: &AgentMailboxHandle,
    from: &str,
    to: &str,
    payload: serde_json::Value,
) -> Result<(), MailboxError> {
    mailbox.send(from, to, payload).await
}

/// Reçoit le prochain message en attente pour `agent_name` — testable sans PyO3.
///
/// Retourne `None` si aucun message n'arrive avant l'expiration du timeout.
pub(crate) async fn receive_inner(
    mailbox: &AgentMailboxHandle,
    agent_name: &str,
    timeout: std::time::Duration,
) -> Option<AgentMessage> {
    mailbox.receive(agent_name, timeout).await
}

#[cfg(test)]
mod runtime_context_tests {
    use super::*;
    use apollia_core::events::RuntimeEvent;
    use apollia_llm::{ObservabilityConfig, StepBudgetView, ToolCallHelper, ToolInvoker};
    use std::pin::Pin;
    use std::sync::Arc;
    use tokio::sync::broadcast;

    use apollia_llm::{
        CompletionModel, CompletionRequest, CompletionResponse, FinishReason, LlmError, LlmRouter,
        StreamChunk, TokenUsage,
    };
    use futures::Stream;

    // ── Mocks pour la construction du ToolCallHelper (jamais réellement appelés) ──

    struct NoopModel;

    #[async_trait::async_trait]
    impl CompletionModel for NoopModel {
        async fn complete(&self, _req: CompletionRequest) -> Result<CompletionResponse, LlmError> {
            Ok(CompletionResponse {
                content: String::new(),
                tool_calls: vec![],
                usage: TokenUsage {
                    prompt_tokens: 0,
                    completion_tokens: 0,
                    cost_usd: None,
                    ..Default::default()
                },
                finish_reason: FinishReason::Stop,
                latency_ms: 0,
                ttft_ms: None,
            })
        }

        async fn stream(
            &self,
            _req: CompletionRequest,
        ) -> Result<Pin<Box<dyn Stream<Item = Result<StreamChunk, LlmError>> + Send>>, LlmError>
        {
            Ok(Box::pin(futures::stream::empty()))
        }

        fn is_available(&self) -> bool {
            true
        }
        fn backend_name(&self) -> &str {
            "noop"
        }
        fn model_id(&self) -> &str {
            "noop"
        }
    }

    struct NoopInvoker;

    #[async_trait::async_trait]
    impl ToolInvoker for NoopInvoker {
        async fn invoke(
            &self,
            _tool_name: &str,
            _arguments: &serde_json::Value,
        ) -> Result<String, String> {
            Ok(String::new())
        }
    }

    fn make_tool_helper() -> Arc<ToolCallHelper> {
        Arc::new(ToolCallHelper::new(
            Arc::new(NoopModel),
            Arc::new(NoopInvoker),
        ))
    }

    /// `ctx.llm` est `None` si le router n'a aucun backend.
    #[tokio::test]
    async fn test_ac2_ctx_llm_none_if_no_backends() {
        // GIVEN un LlmRouter vide (0 backends)
        let router = Arc::new(LlmRouter::empty());
        let (tx, _rx) = broadcast::channel::<RuntimeEvent>(16);
        // WHEN
        let ctx = RuntimeContext::new_with_llm(
            Some(router),
            Arc::new(StepBudgetView::unlimited()),
            make_tool_helper(),
            Arc::new(ObservabilityConfig::default()),
            tx,
            AgentId::new_v4(),
            None,          // tool_proxy
            None,          // memory_interface
            None,          // mailbox
            String::new(), // agent_name
            false,         // supports_a2a
            None,          // user_context
            None,          // a2a_delegate
            None,          // a2a_invoker
            false,         // user_memory_writable
        );
        // THEN
        assert!(ctx.llm.is_none());
    }

    /// `AgentDegraded` émis sur EventBus si aucun backend LLM.
    #[tokio::test]
    async fn test_ac3_agent_degraded_emitted_if_no_llm() {
        // GIVEN un router vide et un bus avec receiver
        let (tx, mut rx) = broadcast::channel::<RuntimeEvent>(16);
        let router = Arc::new(LlmRouter::empty());
        let agent_id = AgentId::new_v4();
        // WHEN
        let _ctx = RuntimeContext::new_with_llm(
            Some(router),
            Arc::new(StepBudgetView::unlimited()),
            make_tool_helper(),
            Arc::new(ObservabilityConfig::default()),
            tx,
            agent_id,
            None,          // tool_proxy
            None,          // memory_interface
            None,          // mailbox
            String::new(), // agent_name
            false,         // supports_a2a
            None,          // user_context
            None,          // a2a_delegate
            None,          // a2a_invoker
            false,         // user_memory_writable
        );
        // THEN un événement AgentDegraded est présent sur le bus
        let event = rx.try_recv().expect("un événement doit être présent");
        assert!(
            matches!(
                event,
                RuntimeEvent::AgentDegraded { ref reason, .. }
                    if reason.contains("no LLM backend")
            ),
            "événement inattendu : {event:?}"
        );
    }

    /// (variante) — `ctx.llm` est `None` si `llm_router` est `None`.
    #[tokio::test]
    async fn test_ac2_ctx_llm_none_if_router_option_is_none() {
        // GIVEN llm_router = None (Supervisor n'a pas pu initialiser le LLM)
        let (tx, _rx) = broadcast::channel::<RuntimeEvent>(16);
        // WHEN
        let ctx = RuntimeContext::new_with_llm(
            None,
            Arc::new(StepBudgetView::unlimited()),
            make_tool_helper(),
            Arc::new(ObservabilityConfig::default()),
            tx,
            AgentId::new_v4(),
            None,          // tool_proxy
            None,          // memory_interface
            None,          // mailbox
            String::new(), // agent_name
            false,         // supports_a2a
            None,          // user_context
            None,          // a2a_delegate
            None,          // a2a_invoker
            false,         // user_memory_writable
        );
        // THEN
        assert!(ctx.llm.is_none());
    }

    // FC01 — ctx.log with valid level emits tracing event (no panic, no error)
    #[tokio::test]
    async fn test_log_valid_levels_succeed() {
        // GIVEN a minimal RuntimeContext
        let ctx = RuntimeContext::for_test();

        // WHEN ctx.log is called with each valid level
        // THEN no error is returned
        assert!(ctx.log("debug", "debug message").is_ok());
        assert!(ctx.log("info", "info message").is_ok());
        assert!(ctx.log("warn", "warn message").is_ok());
        assert!(ctx.log("error", "error message").is_ok());
    }

    // FC01 — ctx.log with invalid level raises ValueError
    #[tokio::test]
    async fn test_log_invalid_level_raises_value_error() {
        // GIVEN a minimal RuntimeContext
        let ctx = RuntimeContext::for_test();

        // WHEN ctx.log is called with an unknown level
        let result = ctx.log("critical", "should fail");

        // THEN a PyValueError is returned
        assert!(result.is_err());
        pyo3::Python::with_gil(|py| {
            let err = result.unwrap_err();
            assert!(
                err.is_instance_of::<pyo3::exceptions::PyValueError>(py),
                "expected PyValueError, got: {err}"
            );
        });
    }

    // FC02 — ctx.step_budget returns None when no budget is configured
    #[tokio::test]
    async fn test_step_budget_none_when_not_configured() {
        // GIVEN a for_test context with no step_budget
        let ctx = RuntimeContext::for_test();

        // WHEN step_budget getter is called
        let result = pyo3::Python::with_gil(|py| ctx.step_budget(py));

        // THEN Ok(None Python) is returned
        assert!(result.is_ok());
        pyo3::Python::with_gil(|py| {
            assert!(result.unwrap().is_none(py));
        });
    }

    // FC02 — steps_remaining reflects step_count atomically
    #[test]
    fn test_steps_remaining_reflects_count() {
        // GIVEN a StepBudgetView with limit 5 and 3 steps consumed
        use std::sync::atomic::{AtomicU32, Ordering};
        let count = Arc::new(AtomicU32::new(3));
        let view = Arc::new(StepBudgetView::new(count.clone(), 5));

        // WHEN steps_remaining is called
        // THEN result is 5 - 3 = 2
        assert_eq!(view.steps_remaining(), 2);

        // WHEN budget is exhausted (count >= limit)
        count.store(5, Ordering::Relaxed);
        assert_eq!(view.steps_remaining(), 0);

        // AND does not go negative even when over-consumed
        count.store(7, Ordering::Relaxed);
        assert_eq!(view.steps_remaining(), 0);
    }

    /// LOT 5 (ADR-103) — end-to-end builder test.
    /// `with_datasources(declared, Some(dir))` charge réellement le YAML
    /// depuis disque et `ctx.datasources` expose la valeur à Python.
    #[test]
    fn test_with_datasources_loads_from_real_dir() {
        // GIVEN a temp agent_dir with datasources/items.yaml
        let tmp = tempfile::tempdir().expect("temp dir");
        let ds_dir = tmp.path().join("datasources");
        std::fs::create_dir_all(&ds_dir).expect("mkdir");
        std::fs::write(ds_dir.join("items.yaml"), "- one\n- two\n- three\n")
            .expect("write yaml");

        // WHEN we build a RuntimeContext via the builder
        let ctx = RuntimeContext::for_test()
            .with_datasources(vec!["items".to_string()], Some(tmp.path()));

        // THEN ctx.datasources exposes the parsed list to Python
        pyo3::Python::with_gil(|py| {
            let ds_obj = ctx.datasources(py);
            assert!(!ds_obj.is_none(py), "ctx.datasources should not be None");
            let bound = ds_obj.bind(py);
            let result = bound
                .call_method1("get", ("items",))
                .expect("get should succeed");
            let len: usize = result
                .call_method0("__len__")
                .expect("len")
                .extract()
                .expect("usize");
            assert_eq!(len, 3, "should have 3 entries");
        });
    }

    /// LOT 5 (ADR-103) — `with_templates(declared, Some(dir))` charge réellement
    /// les templates Jinja2 et le rendu fonctionne avec un dict Python.
    #[test]
    fn test_with_templates_renders_from_real_dir() {
        // GIVEN a temp agent_dir with templates/greeting.j2
        let tmp = tempfile::tempdir().expect("temp dir");
        let tpl_dir = tmp.path().join("templates");
        std::fs::create_dir_all(&tpl_dir).expect("mkdir");
        std::fs::write(tpl_dir.join("greeting.j2"), "Hello {{ who }}!")
            .expect("write template");

        // WHEN we build a RuntimeContext via the builder
        let ctx = RuntimeContext::for_test()
            .with_templates(vec!["greeting".to_string()], Some(tmp.path()));

        // THEN ctx.templates.render returns the rendered string
        pyo3::Python::with_gil(|py| {
            let tpl_obj = ctx.templates(py);
            assert!(!tpl_obj.is_none(py), "ctx.templates should not be None");
            let bound = tpl_obj.bind(py);
            let kwargs = pyo3::types::PyDict::new(py);
            kwargs.set_item("who", "world").expect("set who");
            let rendered: String = bound
                .call_method("render", ("greeting",), Some(&kwargs))
                .expect("render should succeed")
                .extract()
                .expect("string");
            assert_eq!(rendered, "Hello world!");
        });
    }

    /// LOT 5 (ADR-103) — `with_datasources(declared, None)` n'effectue aucun
    /// I/O ; toute lecture renvoie `FileNotFoundError("not found on disk")`.
    #[test]
    fn test_with_datasources_no_dir_keeps_empty_cache() {
        let ctx = RuntimeContext::for_test()
            .with_datasources(vec!["foo".to_string()], None);
        pyo3::Python::with_gil(|py| {
            let ds_obj = ctx.datasources(py);
            let bound = ds_obj.bind(py);
            let err = bound
                .call_method1("get", ("foo",))
                .expect_err("should raise FileNotFoundError when no dir provided");
            assert!(err.is_instance_of::<pyo3::exceptions::PyFileNotFoundError>(py));
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use apollia_core::SandboxProfile;
    use apollia_tools::{ToolDescriptor, ToolKind};

    /// Mock executor that returns a configurable result.
    struct MockExecutor {
        result: Result<serde_json::Value, String>,
    }

    impl ToolExecutor for MockExecutor {
        fn execute(
            &self,
            _tool_name: &str,
            _input: serde_json::Value,
        ) -> Result<serde_json::Value, String> {
            self.result.clone()
        }
    }

    fn file_io_descriptor() -> ToolDescriptor {
        ToolDescriptor {
            name: "file_io".to_string(),
            version: "1.0.0".to_string(),
            description: "File system operations".to_string(),
            kind: ToolKind::Native,
            input_schema: serde_json::json!({ "type": "object" }),
            output_schema: None,
            sandbox_profile: SandboxProfile::FileSystem,
            tags: vec![],
            dangerous: false,
            is_read_only: false,
            risk_score: 5,
            approval_risk_level: None,
            impact_description: None,
            reject_reason_required: false,
        }
    }

    fn bash_descriptor() -> ToolDescriptor {
        ToolDescriptor {
            name: "bash_executor".to_string(),
            version: "1.0.0".to_string(),
            description: "Execute shell commands".to_string(),
            kind: ToolKind::Native,
            input_schema: serde_json::json!({ "type": "object" }),
            output_schema: None,
            sandbox_profile: SandboxProfile::FileSystem,
            tags: vec![],
            dangerous: false,
            is_read_only: false,
            risk_score: 8,
            approval_risk_level: None,
            impact_description: None,
            reject_reason_required: false,
        }
    }

    async fn open_test_audit() -> AuditTrailHandle {
        let db_path =
            std::env::temp_dir().join(format!("apollia_proxy_test_{}.db", uuid::Uuid::new_v4()));
        AuditTrailHandle::open(&db_path)
            .await
            .expect("failed to open audit trail")
    }

    async fn make_proxy(
        allowed_tools: Vec<&str>,
        executor_result: Result<serde_json::Value, String>,
    ) -> (ToolProxy, ToolRegistryHandle, AuditTrailHandle) {
        let registry = ToolRegistryHandle::start();
        let audit = open_test_audit().await;
        let executor = Arc::new(MockExecutor {
            result: executor_result,
        });

        let proxy = ToolProxy::new(
            registry.clone(),
            audit.clone(),
            executor,
            allowed_tools.into_iter().map(String::from).collect(),
            "test-agent".to_string(),
            "task-001".to_string(),
        );

        (proxy, registry, audit)
    }

    // Nominal tool call
    #[tokio::test]
    async fn test_call_tool_nominal() {
        // GIVEN a ToolProxy with "file_io" allowed and registered
        let expected = serde_json::json!({"files": ["a.txt", "b.txt"]});
        let (proxy, registry, audit) = make_proxy(vec!["file_io"], Ok(expected.clone())).await;
        registry
            .register(file_io_descriptor())
            .await
            .expect("register failed");

        // WHEN we call file_io
        let input = serde_json::json!({"action": "list", "path": "."});
        let result = proxy.call_inner("file_io", input).await;

        // THEN the result is Ok with the executor's output
        assert!(result.is_ok());
        assert_eq!(result.expect("should succeed"), expected);

        registry.shutdown().await;
        audit.shutdown().await;
    }

    // Tool not found in registry
    #[tokio::test]
    async fn test_call_tool_not_found() {
        // GIVEN a ToolProxy with "inexistant" allowed but NOT in registry
        let (proxy, registry, audit) =
            make_proxy(vec!["inexistant"], Ok(serde_json::json!({}))).await;

        // WHEN we call "inexistant"
        let result = proxy.call_inner("inexistant", serde_json::json!({})).await;

        // THEN we get ToolNotFound
        assert!(
            matches!(result, Err(ToolProxyError::ToolNotFound(ref name)) if name == "inexistant")
        );

        registry.shutdown().await;
        audit.shutdown().await;
    }

    // Tool not allowed for this agent
    #[tokio::test]
    async fn test_call_tool_not_allowed() {
        // GIVEN a ToolProxy with allowed_tools = ["file_io"]
        //   and "bash_executor" registered in the registry
        let (proxy, registry, audit) = make_proxy(vec!["file_io"], Ok(serde_json::json!({}))).await;
        registry
            .register(bash_descriptor())
            .await
            .expect("register failed");

        // WHEN we call "bash_executor" (exists but not allowed)
        let result = proxy
            .call_inner("bash_executor", serde_json::json!({}))
            .await;

        // THEN we get ToolNotAllowed
        assert!(
            matches!(result, Err(ToolProxyError::ToolNotAllowed(ref name)) if name == "bash_executor")
        );

        registry.shutdown().await;
        audit.shutdown().await;
    }

    // Tool execution failed
    #[tokio::test]
    async fn test_call_tool_execution_failed() {
        // GIVEN a ToolProxy with an executor that fails
        let (proxy, registry, audit) =
            make_proxy(vec!["file_io"], Err("permission denied".to_string())).await;
        registry
            .register(file_io_descriptor())
            .await
            .expect("register failed");

        // WHEN we call file_io
        let result = proxy
            .call_inner("file_io", serde_json::json!({"action": "read"}))
            .await;

        // THEN we get ExecutionFailed with the error message
        assert!(
            matches!(result, Err(ToolProxyError::ExecutionFailed(ref msg)) if msg.contains("permission denied"))
        );

        registry.shutdown().await;
        audit.shutdown().await;
    }

    // Audit trail records each call
    #[tokio::test]
    async fn test_call_records_audit_trail() {
        // GIVEN a ToolProxy with a connected AuditTrailHandle
        let (proxy, registry, audit) =
            make_proxy(vec!["file_io"], Ok(serde_json::json!({"ok": true}))).await;
        registry
            .register(file_io_descriptor())
            .await
            .expect("register failed");

        // WHEN we call file_io
        let input = serde_json::json!({"action": "list", "path": "."});
        let input_hash = compute_input_hash(&input);
        let _ = proxy.call_inner("file_io", input).await;

        // Wait for fire-and-forget audit to be processed
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        // THEN an audit record is stored
        let records = audit.query_last(1).await;
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].tool_name, "file_io");
        assert_eq!(records[0].agent_id, "test-agent");
        assert_eq!(records[0].task_id, "task-001");
        assert_eq!(records[0].input_hash, input_hash);
        assert!(records[0].success);
        assert!(records[0].duration_ms.is_some());

        registry.shutdown().await;
        audit.shutdown().await;
    }

    // Tool call counter increments
    #[tokio::test]
    async fn test_tool_call_count_increments() {
        // GIVEN a ToolProxy with tool_calls = 0
        let (proxy, registry, audit) = make_proxy(vec!["file_io"], Ok(serde_json::json!({}))).await;
        registry
            .register(file_io_descriptor())
            .await
            .expect("register failed");

        assert_eq!(proxy.tool_call_count(), 0);

        // WHEN we call call_inner() 3 times
        for _ in 0..3 {
            let _ = proxy.call_inner("file_io", serde_json::json!({})).await;
        }

        // THEN tool_call_count() returns 3
        assert_eq!(proxy.tool_call_count(), 3);

        registry.shutdown().await;
        audit.shutdown().await;
    }

    /// describe_inner retourne un JSON Value complet pour un descripteur renseigné.
    #[test]
    fn test_describe_inner_returns_json_value() {
        // GIVEN un ToolDescriptor avec tous les champs renseignés
        let descriptor = ToolDescriptor {
            name: "bash_executor".to_string(),
            version: "1.0.0".to_string(),
            description: "Execute shell commands".to_string(),
            kind: ToolKind::Native,
            input_schema: serde_json::json!({ "type": "object", "properties": { "command": { "type": "string" } } }),
            output_schema: Some(
                serde_json::json!({ "type": "object", "properties": { "stdout": { "type": "string" } } }),
            ),
            sandbox_profile: SandboxProfile::FileSystem,
            tags: vec!["shell".to_string(), "execution".to_string()],
            dangerous: false,
            is_read_only: false,
            risk_score: 8,
            approval_risk_level: None,
            impact_description: None,
            reject_reason_required: false,
        };

        // WHEN on appelle describe_inner
        let value = describe_inner(&descriptor);

        // THEN le résultat contient name, version, description, input_schema, output_schema, tags
        assert_eq!(value["name"], "bash_executor");
        assert_eq!(value["version"], "1.0.0");
        assert_eq!(value["description"], "Execute shell commands");
        assert_eq!(value["input_schema"]["type"], "object");
        assert!(value["input_schema"]["properties"]["command"].is_object());
        assert_eq!(value["output_schema"]["type"], "object");
        let tags = value["tags"].as_array().expect("tags should be an array");
        assert_eq!(tags.len(), 2);
        assert_eq!(tags[0], "shell");
        assert_eq!(tags[1], "execution");
    }

    /// (côté Rust) : describe_inner sur un descripteur minimal (champs optionnels vides/None).
    #[test]
    fn test_describe_inner_minimal_descriptor() {
        // GIVEN un ToolDescriptor avec output_schema=None et tags vide
        let descriptor = ToolDescriptor {
            name: "minimal".to_string(),
            version: "0.1.0".to_string(),
            description: "A minimal tool".to_string(),
            kind: ToolKind::Native,
            input_schema: serde_json::json!({ "type": "object" }),
            output_schema: None,
            sandbox_profile: SandboxProfile::ReadOnly,
            tags: vec![],
            dangerous: false,
            is_read_only: true,
            risk_score: 0,
            approval_risk_level: None,
            impact_description: None,
            reject_reason_required: false,
        };

        // WHEN on appelle describe_inner
        let value = describe_inner(&descriptor);

        // THEN les champs optionnels sont null/vides
        assert_eq!(value["name"], "minimal");
        assert_eq!(value["version"], "0.1.0");
        assert!(value["output_schema"].is_null());
        let tags = value["tags"].as_array().expect("tags should be an array");
        assert!(tags.is_empty());
    }
}

#[cfg(test)]
mod a2a_tests {
    use super::*;
    use apollia_runtime::EventBus;
    use std::time::Duration;

    /// send_inner délivre un message, receive_inner le reçoit.
    #[tokio::test]
    async fn test_send_inner_delivers_message() {
        // GIVEN une mailbox active
        let (event_tx, _event_rx) = EventBus::new();
        let handle = AgentMailboxHandle::spawn(event_tx, 100);

        // WHEN agent-a envoie un message à agent-b
        let payload = serde_json::json!({"greeting": "hello"});
        send_inner(&handle, "agent-a", "agent-b", payload.clone())
            .await
            .expect("send should succeed");

        // THEN agent-b reçoit le message avec les bons champs
        let received = receive_inner(&handle, "agent-b", Duration::from_secs(1))
            .await
            .expect("should receive a message");
        assert_eq!(received.from, "agent-a");
        assert_eq!(received.payload, payload);
        assert!(!received.sent_at.is_empty());

        handle.shutdown().await;
    }

    /// receive_inner retourne None si aucun message en attente (timeout).
    #[tokio::test]
    async fn test_receive_inner_returns_none_on_timeout() {
        // GIVEN une mailbox active sans messages
        let (event_tx, _event_rx) = EventBus::new();
        let handle = AgentMailboxHandle::spawn(event_tx, 100);

        // WHEN on essaie de recevoir avec un timeout court
        let result = receive_inner(&handle, "agent-c", Duration::from_millis(50)).await;

        // THEN le résultat est None
        assert!(result.is_none());

        handle.shutdown().await;
    }

    /// le gate check rejette l'appel si supports_a2a est false.
    #[tokio::test]
    async fn test_gate_check_rejects_without_a2a() {
        // GIVEN un RuntimeContext avec supports_a2a = false
        let ctx = RuntimeContext::for_test();

        // THEN les vérifications internes échouent
        assert!(!ctx.supports_a2a);
        assert!(ctx.mailbox.is_none());
    }

    #[tokio::test]
    async fn test_user_context_some_in_chat_mode() {
        // GIVEN a RuntimeContext with user_context populated (chat mode)
        let mut uc = HashMap::new();
        uc.insert(
            "preferences".to_string(),
            vec![
                ("langue".to_string(), "francais".to_string()),
                ("format".to_string(), "markdown".to_string()),
            ],
        );
        uc.insert(
            "habits".to_string(),
            vec![("working_hours".to_string(), "9h-18h".to_string())],
        );
        uc.insert("context".to_string(), vec![]);

        let mut ctx = RuntimeContext::for_test();
        ctx.user_context = Some(uc);

        // THEN user_context is Some with expected categories
        assert!(ctx.user_context.is_some());
        let uc = ctx.user_context.as_ref().expect("should be Some");
        assert_eq!(uc.get("preferences").expect("preferences").len(), 2);
        assert_eq!(uc.get("habits").expect("habits").len(), 1);
        assert!(uc.get("context").expect("context").is_empty());
    }

    #[tokio::test]
    async fn test_user_context_none_in_task_mode() {
        // GIVEN a RuntimeContext with user_context = None (task mode)
        let ctx = RuntimeContext::for_test();

        // THEN user_context is None
        assert!(ctx.user_context.is_none());
    }
}

#[cfg(test)]
mod tool_proxy_a2a_tests {
    use super::*;
    use apollia_core::{A2AConfig, AgentManifest, AgentSkill, ProcessState};
    use apollia_runtime::a2a::A2aError as LowLevelA2aError;
    use apollia_runtime::registry::AgentRegistry;
    use apollia_runtime::{A2AInvoker, A2aDelegateFn, A2aDelegateResult, EventBus};
    use std::future::Future;
    use std::pin::Pin;

    struct AlwaysOkExecutor;

    impl ToolExecutor for AlwaysOkExecutor {
        fn execute(&self, _: &str, _: serde_json::Value) -> Result<serde_json::Value, String> {
            Ok(serde_json::json!({}))
        }
    }

    fn make_ok_delegate() -> A2aDelegateFn {
        Arc::new(
            |skill_id: String,
             _input: serde_json::Value,
             _timeout: u64,
             _chain: Vec<apollia_core::AgentId>,
             _caller: apollia_core::AgentId| {
                let fut: Pin<
                    Box<dyn Future<Output = Result<A2aDelegateResult, LowLevelA2aError>> + Send>,
                > = Box::pin(async move {
                    Ok(A2aDelegateResult {
                        task_id: "task-a2a".to_string(),
                        agent_name: "excel-worker".to_string(),
                        output: format!("processed {skill_id}"),
                    })
                });
                fut
            },
        )
    }

    fn make_excel_manifest() -> AgentManifest {
        AgentManifest {
            name: "excel-worker".to_string(),
            version: "0.1.0".to_string(),
            description: "Excel worker".to_string(),
            tools_required: vec![],
            tools_optional: vec![],
            supports_streaming: false,
            supports_a2a: true,
            memory_namespace: None,
            shared_memory_namespaces: vec![],
            max_concurrent_tasks: 1,
            step_budget: None,
            network_allowlist: None,
            dangerous_tools_allowed: false,
            tags: vec!["worker".to_string()],
            skills: vec![AgentSkill {
                id: "read-excel".to_string(),
                name: "read-excel".to_string(),
                description: "Reads Excel files".to_string(),
                input_modes: vec!["text".to_string()],
                output_modes: vec!["text".to_string()],
                input_schema: None,
            }],
            execution_mode: "direct".to_string(),
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
            templates: vec![],            secrets: vec![],
        }
    }

    async fn make_invoker_with_excel() -> Arc<A2AInvoker> {
        let (bus_tx, _) = EventBus::new();
        let registry = AgentRegistry::spawn(bus_tx.clone());
        let agent_id = registry
            .register(make_excel_manifest())
            .await
            .expect("register failed");
        registry
            .update_state(agent_id.as_str(), ProcessState::Active)
            .await
            .expect("update state");

        Arc::new(A2AInvoker::new_for_test(
            registry,
            make_ok_delegate(),
            bus_tx,
            A2AConfig::default(),
        ))
    }

    async fn make_base_proxy(allowed: Vec<&str>) -> (ToolProxy, apollia_tools::AuditTrailHandle) {
        let db_path =
            std::env::temp_dir().join(format!("apollia_a2a_test_{}.db", uuid::Uuid::new_v4()));
        let audit = apollia_tools::AuditTrailHandle::open(&db_path)
            .await
            .expect("failed to open audit");
        let registry = apollia_tools::ToolRegistryHandle::start();
        let executor = Arc::new(AlwaysOkExecutor);
        let proxy = ToolProxy::new(
            registry,
            audit.clone(),
            executor,
            allowed.into_iter().map(String::from).collect(),
            "director-agent".to_string(),
            "task-001".to_string(),
        );
        (proxy, audit)
    }

    /// A2A tool call is routed to the A2AInvoker and output is formatted correctly.
    #[tokio::test]
    async fn test_a2a_prefix_routes_to_invoker() {
        // GIVEN a ToolProxy with "a2a:read-excel" allowed and an A2AInvoker with excel-worker
        let invoker = make_invoker_with_excel().await;
        let (proxy, audit) = make_base_proxy(vec!["a2a:read-excel"]).await;
        let proxy = proxy.with_a2a(invoker, 0, None);

        // WHEN we call "a2a:read-excel"
        let result = proxy
            .call_inner(
                "a2a:read-excel",
                serde_json::json!({"text": "process file.xlsx"}),
            )
            .await;

        // THEN the result is Ok and contains "[read-excel via excel-worker]" header
        let val = result.expect("should succeed");
        let text = val["text"].as_str().expect("text field");
        assert!(
            text.contains("[read-excel via excel-worker]"),
            "expected formatted header in: {text}"
        );

        audit.shutdown().await;
    }

    /// Native tool calls are not affected by A2A routing.
    #[tokio::test]
    async fn test_native_tool_not_routed_through_a2a() {
        // GIVEN a ToolProxy with A2A configured but calling a native tool name
        let invoker = make_invoker_with_excel().await;
        let (proxy, audit) = make_base_proxy(vec!["a2a:read-excel"]).await;
        let proxy = proxy.with_a2a(invoker, 0, None);

        // WHEN we call "a2a:unknown-skill" (not in allowed_tools)
        let result = proxy
            .call_inner("a2a:unknown-skill", serde_json::json!({}))
            .await;

        // THEN we get ToolNotAllowed (permission check fires before routing)
        assert!(
            matches!(result, Err(ToolProxyError::ToolNotAllowed(ref n)) if n == "a2a:unknown-skill"),
            "expected ToolNotAllowed, got: {result:?}"
        );

        audit.shutdown().await;
    }

    /// A2A tool without configured invoker returns ExecutionFailed.
    #[tokio::test]
    async fn test_a2a_tool_without_invoker_returns_error() {
        // GIVEN a ToolProxy with "a2a:read-excel" allowed but NO a2a_invoker configured
        let (proxy, audit) = make_base_proxy(vec!["a2a:read-excel"]).await;

        // WHEN we call "a2a:read-excel"
        let result = proxy
            .call_inner("a2a:read-excel", serde_json::json!({"text": "test"}))
            .await;

        // THEN we get ExecutionFailed
        assert!(
            matches!(result, Err(ToolProxyError::ExecutionFailed(_))),
            "expected ExecutionFailed, got: {result:?}"
        );

        audit.shutdown().await;
    }
}

// ─────────────────────────────────────────────
// Tests — WorkspaceContextPy
// ─────────────────────────────────────────────

#[cfg(test)]
mod workspace_context_tests {
    use super::*;

    #[test]
    fn test_workspace_context_py_rules_getter() {
        // GIVEN a WorkspaceContextPy with a "Règles du projet" section
        let mut ws = WorkspaceContextPy::empty();
        ws.set_section("Règles du projet", "Reply in French".to_string());
        ws.set_section("Git", "branch: main".to_string());
        // THEN rules() and apollia_md() both return the rules content
        assert_eq!(ws.rules(), Some("Reply in French"));
        assert_eq!(ws.apollia_md(), Some("Reply in French"));
        // AND get() returns sections by title
        assert_eq!(ws.get("Git"), Some("branch: main"));
        assert!(ws.get("Unknown").is_none());
    }

    #[test]
    fn test_workspace_context_py_empty() {
        // GIVEN an empty WorkspaceContextPy
        let ws = WorkspaceContextPy::empty();
        // THEN all getters return None
        assert!(ws.rules().is_none());
        assert!(ws.apollia_md().is_none());
        assert!(ws.get("Git").is_none());
    }

    #[test]
    fn test_workspace_context_py_set_section_upserts() {
        // GIVEN a WorkspaceContextPy with a section
        let mut ws = WorkspaceContextPy::empty();
        ws.set_section("Git", "branch: dev".to_string());
        // WHEN the section is updated
        ws.set_section("Git", "branch: main".to_string());
        // THEN only one section exists with the updated value
        assert_eq!(ws.sections.len(), 1);
        assert_eq!(ws.get("Git"), Some("branch: main"));
    }

    #[test]
    fn test_runtime_context_with_empty_workspace() {
        // GIVEN a RuntimeContext enriched with an empty workspace
        let ctx = RuntimeContext::for_test().with_empty_workspace();
        // THEN workspace is Some but has no sections
        assert!(ctx.workspace.is_some());
        pyo3::Python::with_gil(|py| {
            let ws = ctx.workspace.as_ref().expect("workspace should be Some");
            let borrowed = ws.borrow(py);
            assert!(borrowed.rules().is_none());
        });
    }

    #[test]
    fn test_runtime_context_workspace_none_by_default() {
        // GIVEN a RuntimeContext built without workspace
        let ctx = RuntimeContext::for_test();
        // THEN workspace is None
        assert!(ctx.workspace.is_none());
    }

    // ── effective_memory_namespace ──────────────────────────────────────────────

    #[test]
    fn test_effective_namespace_with_project_id() {
        // GIVEN
        let manifest_ns = "dev-assistant";
        let project_id = Some("proj_abc123");
        // WHEN
        let ns = effective_memory_namespace(manifest_ns, project_id);
        // THEN
        assert_eq!(ns, "proj_abc123:dev-assistant");
    }

    #[test]
    fn test_effective_namespace_without_project_id() {
        // GIVEN
        let manifest_ns = "dev-assistant";
        let project_id: Option<&str> = None;
        // WHEN
        let ns = effective_memory_namespace(manifest_ns, project_id);
        // THEN
        assert_eq!(ns, "dev-assistant");
    }

    #[test]
    fn test_effective_namespace_empty_project_id_treated_as_none() {
        // GIVEN
        let manifest_ns = "test-agent";
        let project_id = Some("");
        // WHEN
        let ns = effective_memory_namespace(manifest_ns, project_id);
        // THEN
        assert_eq!(ns, "test-agent");
    }

    #[test]
    fn test_isolation_between_two_projects() {
        use apollia_memory::{manager::MemoryManager, semantic::SemanticMemory};
        use tempfile::TempDir;

        // GIVEN an agent that wrote "forbidden_deps" in proj_A
        let dir = TempDir::new().expect("temp dir");

        let ns_a = effective_memory_namespace("dev-assistant", Some("proj_A"));
        let ns_b = effective_memory_namespace("dev-assistant", Some("proj_B"));

        {
            let mut mgr = MemoryManager::new(dir.path(), Some(ns_a.clone()), vec![]);
            let store = mgr.store(&ns_a).expect("open store for proj_A");
            let sem = SemanticMemory::new(store);
            sem.remember(
                &ns_a,
                "forbidden_deps",
                &serde_json::json!("no_async_std"),
                1.0,
                None,
                None,
            )
            .expect("write to proj_A");
        }

        // WHEN reading from the same agent in proj_B
        let mut mgr_b = MemoryManager::new(dir.path(), Some(ns_b.clone()), vec![]);
        let store_b = mgr_b.store(&ns_b).expect("open store for proj_B");
        let sem_b = SemanticMemory::new(store_b);
        let found = sem_b
            .recall(&ns_b, "forbidden_deps")
            .expect("recall from proj_B");

        // THEN the key written in proj_A is invisible in proj_B
        assert!(found.is_none());
    }
}
