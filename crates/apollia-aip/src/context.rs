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
    compute_input_hash, AuditTrailHandle, ToolDescriptor, ToolInvocationRecord, ToolRegistryHandle,
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
#[pyclass]
pub struct ToolProxy {
    registry: ToolRegistryHandle,
    audit: AuditTrailHandle,
    executor: Arc<dyn ToolExecutor>,
    allowed_tools: Vec<String>,
    agent_id: String,
    task_id: String,
    tool_calls: AtomicU32,
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
            .import_bound("json")
            .map_err(|e| PyRuntimeError::new_err(format!("failed to import json: {e}")))?;
        let input_str: String = json_mod
            .call_method1("dumps", (input.bind(py),))
            .map_err(|e| PyRuntimeError::new_err(format!("json.dumps failed: {e}")))?
            .extract()
            .map_err(|e| PyRuntimeError::new_err(format!("extract failed: {e}")))?;
        let input_value: serde_json::Value = serde_json::from_str(&input_str)
            .map_err(|e| PyRuntimeError::new_err(format!("JSON parse failed: {e}")))?;

        // Increment counter unconditionally
        self.tool_calls.fetch_add(1, Ordering::Relaxed);

        // Clone fields for the 'static async future
        let registry = self.registry.clone();
        let audit = self.audit.clone();
        let executor = Arc::clone(&self.executor);
        let allowed = self.allowed_tools.clone();
        let agent_id = self.agent_id.clone();
        let task_id = self.task_id.clone();

        pyo3_async_runtimes::tokio::future_into_py(py, async move {
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

            match result {
                Ok(value) => {
                    let json_str = serde_json::to_string(&value).map_err(|e| {
                        PyRuntimeError::new_err(format!("result serialization: {e}"))
                    })?;
                    Python::with_gil(|py| {
                        let json_mod = py
                            .import_bound("json")
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
                            .import_bound("json")
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
        }
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
// RuntimeContext
// ─────────────────────────────────────────────

use std::collections::HashMap;

use apollia_core::events::{AgentId, EventBusSender, RuntimeEvent};
use apollia_llm::{LlmRouter, ObservabilityConfig, StepBudgetView, ToolCallHelper};
use apollia_runtime::mailbox::{AgentMailboxHandle, AgentMessage, MailboxError};

use crate::llm::LlmProxy;

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
    ) -> Self {
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

        Self {
            llm,
            tools,
            memory,
            mailbox,
            agent_name,
            supports_a2a,
            user_context,
        }
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

    /// Contexte utilisateur injecté en mode chat — `None` en mode task.
    ///
    /// Propriété Python `ctx.user_context`. Retourne un `dict[str, list[tuple[str, str]]]`
    /// avec les catégories `preferences`, `habits`, `context`, ou `None` Python
    /// si l'agent n'est pas en mode chat ou si la mémoire utilisateur est vide.
    #[getter]
    fn user_context(&self, py: Python<'_>) -> PyObject {
        match &self.user_context {
            Some(ctx) => ctx.to_object(py),
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
            .import_bound("json")
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
                            .import_bound("json")
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
                },
                finish_reason: FinishReason::Stop,
                latency_ms: 0,
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
        );
        // THEN
        assert!(ctx.llm.is_none());
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
        let handle = AgentMailboxHandle::spawn(event_tx);

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
        let handle = AgentMailboxHandle::spawn(event_tx);

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
        let ctx = RuntimeContext {
            tools: None,
            llm: None,
            memory: None,
            mailbox: None,
            agent_name: "test-agent".to_string(),
            supports_a2a: false,
            user_context: None,
        };

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

        let ctx = RuntimeContext {
            tools: None,
            llm: None,
            memory: None,
            mailbox: None,
            agent_name: "chat-agent".to_string(),
            supports_a2a: false,
            user_context: Some(uc),
        };

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
        let ctx = RuntimeContext {
            tools: None,
            llm: None,
            memory: None,
            mailbox: None,
            agent_name: "task-agent".to_string(),
            supports_a2a: false,
            user_context: None,
        };

        // THEN user_context is None
        assert!(ctx.user_context.is_none());
    }
}
