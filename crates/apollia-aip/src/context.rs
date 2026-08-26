//! ToolProxy: Python-facing proxy for invoking Rust tools.
//!
//! Exposes a `#[pyclass]` that agents use via `ctx.tools.call(tool_name, input)`.
//! Handles permission checks, registry lookup, tool execution, audit logging,
//! and tool call counting.

use std::sync::Arc;

use pyo3::prelude::*;

mod runtime_ops;
mod tool_invoke;
mod tool_proxy;
mod workspace;

pub use tool_proxy::{
    describe_inner, DispatcherExecutor, ToolExecutor, ToolProxy, ToolProxyConfig,
};
pub use workspace::WorkspaceContextPy;

use std::collections::HashMap;

use apollia_core::events::{AgentId, EventBusSender, RuntimeEvent};
use apollia_llm::StepBudgetView;
use apollia_runtime::a2a::A2AInvoker;
use apollia_runtime::mailbox::AgentMailboxHandle;
#[cfg(test)]
use apollia_runtime::mailbox::MailboxConfig;

use crate::llm::LlmProxy;

/// Read-only view of the execution budget exposed to the Python agent via `ctx.step_budget`.
///
/// Instant snapshot at access time. The three dimensions reflect the live
/// counters of the Rust [`StepBudgetView`] at call time.
#[pyclass(frozen, name = "StepBudgetView")]
pub struct PyStepBudgetView {
    steps_remaining: i64,
    tool_calls_remaining: i64,
    elapsed_seconds: f64,
}

#[pymethods]
impl PyStepBudgetView {
    /// Steps left before reaching the limit (`max_steps`).
    #[getter]
    fn steps_remaining(&self) -> i64 {
        self.steps_remaining
    }

    /// Tool calls left before reaching `max_tool_calls`.
    #[getter]
    fn tool_calls_remaining(&self) -> i64 {
        self.tool_calls_remaining
    }

    /// Seconds elapsed since the task started.
    #[getter]
    fn elapsed_seconds(&self) -> f64 {
        self.elapsed_seconds
    }
}

/// Execution context exposed to the Python agent via `run(task, ctx)`.
///
/// Built by the runtime for each agent run. Exposes the runtime's optional
/// capabilities:
/// - `ctx.tools`: [`ToolProxy`] if tools are allocated to the agent, `None` otherwise.
/// - `ctx.llm`: [`LlmProxy`] if at least one LLM backend is available,
///   `None` otherwise (the agent chooses whether the absence is fatal).
/// - `ctx.memory`: [`crate::memory::MemoryInterface`] isolated per namespace if
///   `memory_namespace` is declared in the manifest, `None` otherwise.
///
/// The absence of an LLM is signaled on the EventBus via `AgentDegraded`
/// at construction (fail fast).
#[pyclass(name = "RuntimeContext")]
pub struct RuntimeContext {
    /// Tools proxy exposed to Python; `None` if no tool is allocated.
    tools: Option<pyo3::Py<ToolProxy>>,
    /// LLM proxy exposed to Python; `None` if no LLM backend is available.
    pub llm: Option<LlmProxy>,
    /// Per-namespace isolated memory interface; `None` if `memory_namespace` is absent from the manifest.
    memory: Option<pyo3::Py<crate::memory::MemoryInterface>>,
    /// Handle to the AgentMailbox; `None` if the runtime did not start a mailbox.
    mailbox: Option<AgentMailboxHandle>,
    /// Current run of this context, propagated into mailbox events for audit.
    /// Set via [`RuntimeContext::with_run_id`]; `None` leaves sends uncorrelated.
    run_id: Option<apollia_core::events::RunId>,
    /// Whether the agent declared the mailbox capability (`ctx.mail`). When
    /// `false`, `ctx.mail` refuses every call (opt-in surface).
    supports_mailbox: bool,
    /// Optional recipient allowlist for `ctx.mail.send`. `None` means any
    /// registered agent (once `supports_mailbox` is true).
    mailbox_allowlist: Option<Vec<String>>,
    /// Whether `ctx.mail.send` is gated behind human approval (opt-in via the
    /// manifest `tools_requiring_approval` containing `mailbox:send`).
    mailbox_send_requires_approval: bool,
    /// Name of the agent owning this context.
    agent_name: String,
    /// User memory injected in chat mode; `None` in task mode.
    ///
    /// Structure: `{"preferences": [("key", "value"), ...], "habits": [...], "context": [...]}`.
    /// The agent decides what to do with it; this is never deterministic.
    user_context: Option<HashMap<String, Vec<(String, String)>>>,
    /// High-level A2A orchestrator; `None` if unavailable in this context.
    ///
    /// Exposes `ctx.a2a_invoke`, `ctx.a2a_discover`, `ctx.a2a_list_skills` to Python agents.
    a2a_invoker: Option<Arc<A2AInvoker>>,
    /// If `true`, the agent can write into the global user memory (`__user__`)
    /// via `ctx.memory.remember_user()`. Reading the `__user__` namespace is
    /// always allowed, handled by the [`MemoryInterface`] as soon as a
    /// `user_manager` is provided.
    ///
    /// True only for agents whose manifest declares `user_memory_write = true`
    /// (e.g. `onboarding-agent`).
    user_memory_writable: bool,
    /// Workspace context collected at task startup.
    ///
    /// Populated by [`with_workspace_snapshot`](RuntimeContext::with_workspace_snapshot) or via the bridge
    /// during `call_run()`. Exposes `ctx.workspace.rules`, `ctx.workspace.get("Git")`, etc.
    /// `None` if the runtime did not collect the workspace context for this task.
    pub workspace: Option<Py<WorkspaceContextPy>>,
    /// Event bus cloned at construction, letting `emit_token()` push
    /// `RuntimeEvent::ChatToken` to the SSE frontend. `None` when the context
    /// is built outside an event chain (intra-crate unit tests).
    event_bus: Option<EventBusSender>,
    /// Session id to tag on `ChatToken` events.
    ///
    /// Injected from `task.context_id` by the `BridgeRunner` in chat mode.
    /// `None` in task mode, where `emit_token()` is a no-op.
    chat_session_id: Option<String>,
    /// Current message id to tag on `ChatToken` events.
    ///
    /// Injected from `task.message_id` by the `BridgeRunner` in chat mode.
    /// `None` in task mode.
    chat_message_id: Option<String>,
    /// Shared step budget view; `None` outside a budgeted run.
    step_budget: Option<Arc<StepBudgetView>>,
    /// Notification interface exposed to the Python agent via `ctx.notify`.
    /// `None` if no notification channel is configured (opt-in).
    notify: Option<pyo3::Py<crate::notify::PyNotifyInterface>>,
    /// STT interface exposed to the Python agent via `ctx.stt`.
    /// `None` if no STT backend is configured.
    stt: Option<pyo3::Py<crate::stt::PySttInterface>>,
    /// User profile interface exposed to the Python agent via `ctx.profile`.
    ///
    /// `None` when the runtime has not initialized a `__user__` MemoryManager
    /// (tests, minimal contexts). Reading is always allowed; writing is gated
    /// by `user_memory_writable`.
    profile: Option<pyo3::Py<crate::profile::ProfileInterface>>,
    /// ID of the agent owning this context (stable UUID).
    agent_id: AgentId,
    /// Current task id, injected by the backend at the start of a `call_run`
    /// via [`with_task_id`](RuntimeContext::with_task_id).
    ///
    /// Used by `ctx.log()` to tag the `RuntimeEvent::AgentLog` that goes to the
    /// `runtime_events` persistor. `None` when the context is built for tests
    /// that do not simulate a task.
    task_id: Option<String>,

    // Nested surfaces
    /// Consolidated A2A facade: `ctx.a2a`.
    ///
    /// Shares the same `Arc<A2AInvoker>` as [`Self::a2a_invoker`]. Always built
    /// (the agent always sees `ctx.a2a`, but the methods raise `RuntimeError`
    /// when the invoker is unavailable).
    a2a_iface: Option<Py<crate::a2a::A2AInterface>>,

    /// Typed event-emission surface: `ctx.events`.
    ///
    /// Always built (at worst in silent no-op mode when the bus is absent).
    events_iface: Option<Py<crate::events::EventsInterface>>,

    /// YAML datasources interface: `ctx.datasources`.
    ///
    /// Always built from `manifest.datasources`. If the declared list is empty,
    /// the interface is still present but `get()` always raises
    /// `FileNotFoundError("not declared")`.
    datasources_iface: Option<Py<crate::datasources::DatasourcesInterface>>,

    /// Jinja2 templates interface: `ctx.templates`.
    templates_iface: Option<Py<crate::templates::TemplatesInterface>>,

    /// Read-only secrets interface: `ctx.secrets`.
    secrets_iface: Option<Py<crate::secrets::SecretsInterface>>,

    /// Total wall-clock budget (in seconds), propagated from the manifest
    /// (`budget.wall_clock_secs`) at `bridge.call_run()` time.
    ///
    /// When `Some(n)`, the `ctx.budget.wall_clock_remaining` getter returns
    /// `Some(max(0, n - elapsed))`. When `None` (test mode, CLI dry-run without
    /// a deadline), the getter returns `None` and the agent should infer that
    /// no time constraint is applied.
    pub(crate) wall_clock_secs: Option<u64>,
}

impl RuntimeContext {}

// Every getter below whose body can hand back `py.None()` carries a
// `ctx-attachment:` line in its doc comment, with two admissible verdicts.
//
//   `optional`, a production path really leaves the field unset, so the agent
//              can read `None` and has to branch on it.
//   `always`,   the branch exists but no production path reaches it, so the
//              agent never sees `None`.
//
// The verdict is written here because this file is the only place that knows
// it: the syntax of the accessor does not, and the Python protocol does not
// either. `scripts/check_ctx_contract.py` reads these lines, refuses an
// accessor that can return `None` without a verdict, and refuses an `optional`
// verdict on an accessor with no such branch. `docs/site/scripts/gen_sdk_ref.py`
// publishes the sentence "the bridge may leave this service unattached" on the
// `optional` ones only. Publishing it on the syntax alone put that sentence on
// seven pages the bridge documents as always attached.
#[pymethods]
impl RuntimeContext {
    /// Injected tools proxy; `None` if no tool is allocated.
    ///
    /// Python property `ctx.tools`. Returns Python `None` (no exception) if the
    /// agent has no `tools_required` or the factory did not provide a proxy.
    ///
    /// ctx-attachment: optional, the production constructor takes an `Option` and
    ///     leaves `tools` unset when the agent declares no tool.
    #[getter]
    fn tools(&self, py: Python<'_>) -> PyObject {
        match &self.tools {
            Some(proxy) => proxy.clone_ref(py).into_any(),
            None => py.None(),
        }
    }

    /// Injected LLM proxy; `None` if no LLM backend is available.
    ///
    /// Python property `ctx.llm`. Returns Python `None` (no exception) if the
    /// runtime started with no LLM backend configured or available.
    ///
    /// ctx-attachment: optional, the production constructor takes an `Option` and
    ///     leaves `llm` unset when no backend is available.
    #[getter]
    fn llm(&self, py: Python<'_>) -> PyObject {
        match &self.llm {
            Some(proxy) => Py::new(py, proxy.clone())
                .map(|p| p.into_any())
                .unwrap_or_else(|_| py.None()),
            None => py.None(),
        }
    }

    /// Per-namespace isolated memory interface.
    ///
    /// Python property `ctx.memory`. Returns Python `None` if the agent's
    /// manifest does not declare a `memory_namespace`.
    ///
    /// ctx-attachment: optional, the production constructor takes an `Option` and
    ///     leaves `memory` unset without a `memory_namespace`.
    #[getter]
    fn memory(&self, py: Python<'_>) -> PyObject {
        match &self.memory {
            Some(mem) => mem.clone_ref(py).into_any(),
            None => py.None(),
        }
    }

    /// Workspace context collected at task startup.
    ///
    /// Python property `ctx.workspace`. Exposes `ctx.workspace.git_branch`,
    /// `ctx.workspace.git_is_clean`, `ctx.workspace.apollia_md`, etc.
    /// Returns Python `None` if the runtime did not collect the workspace context.
    ///
    /// ctx-attachment: optional, the production constructor sets `workspace` to `None`;
    ///     only `with_workspace_snapshot` fills it.
    #[getter]
    fn workspace(&self, py: Python<'_>) -> PyObject {
        match &self.workspace {
            Some(ws) => ws.clone_ref(py).into_any(),
            None => py.None(),
        }
    }

    /// Notification interface exposed to the Python agent via `ctx.notify`.
    ///
    /// Python property `ctx.notify`. Returns Python `None` if no notification
    /// channel is configured (channels are opt-in by design).
    ///
    /// ctx-attachment: optional, the production constructor sets `notify` to `None`;
    ///     channels are opt-in.
    #[getter]
    fn notify(&self, py: Python<'_>) -> PyObject {
        match &self.notify {
            Some(n) => n.clone_ref(py).into_any(),
            None => py.None(),
        }
    }

    /// STT interface exposed to the Python agent via `ctx.stt`.
    ///
    /// Python property `ctx.stt`. Returns Python `None` if STT is not configured.
    ///
    /// ctx-attachment: optional, the production constructor sets `stt` to `None`; only
    ///     `with_stt` fills it.
    #[getter]
    fn stt(&self, py: Python<'_>) -> PyObject {
        match &self.stt {
            Some(s) => s.clone_ref(py).into_any(),
            None => py.None(),
        }
    }

    /// User profile interface exposed to the Python agent via `ctx.profile`.
    ///
    /// Python property `ctx.profile`. Returns Python `None` when no `__user__`
    /// manager has been initialized (tests, minimal contexts).
    ///
    /// ctx-attachment: optional, the production constructor sets `profile` to `None`;
    ///     only `with_profile` fills it.
    #[getter]
    fn profile(&self, py: Python<'_>) -> PyObject {
        match &self.profile {
            Some(p) => p.clone_ref(py).into_any(),
            None => py.None(),
        }
    }

    /// Logical agent name (used to name the logger
    /// `apollia.agent.<agent_name>` via the `logger_bridge`).
    ///
    /// Python property `ctx.agent_name`. Stable for the whole context lifetime.
    /// Always present (an empty string is impossible in practice).
    #[getter]
    fn agent_name(&self) -> String {
        self.agent_name.clone()
    }

    // Getters for the nested surfaces.
    /// Consolidated A2A facade: `ctx.a2a`.
    ///
    /// Always returns an `A2AInterface` (never `None`); the internal methods
    /// raise `RuntimeError("A2A invoker not available ...")` if the runtime has
    /// no active invoker. This uniformity spares the agent from branching on
    /// the presence of `ctx.a2a`.
    ///
    /// ctx-attachment: always, `a2a_iface` is built systematically by the production
    ///     constructor; the methods raise when the invoker is missing.
    #[getter]
    fn a2a(&self, py: Python<'_>) -> PyObject {
        match &self.a2a_iface {
            Some(p) => p.clone_ref(py).into_any(),
            None => py.None(),
        }
    }

    /// Asynchronous inter-agent messaging: `ctx.mail`.
    ///
    /// Built on each access so the caller's current `run_id` (set after
    /// construction) flows into emitted events for auditability. Always returns
    /// a `MailInterface`; its methods raise if the runtime has no mailbox.
    ///
    /// ctx-attachment: always, a fresh `MailInterface` is built on each access; the
    ///     `None` branch is taken only if `Py::new` fails.
    #[getter]
    fn mail(&self, py: Python<'_>) -> PyObject {
        match Py::new(
            py,
            crate::mail::MailInterface::new(
                self.mailbox.clone(),
                self.agent_name.clone(),
                self.run_id.clone(),
                self.supports_mailbox,
                self.mailbox_allowlist.clone(),
                self.mailbox_send_requires_approval,
                self.event_bus.clone(),
                self.a2a_invoker.clone(),
            ),
        ) {
            Ok(p) => p.into_any(),
            Err(_) => py.None(),
        }
    }

    /// Typed event emission: `ctx.events`.
    ///
    /// ctx-attachment: always, `events_iface` is built systematically, in silent no-op
    ///     mode when the bus is absent.
    #[getter]
    fn events(&self, py: Python<'_>) -> PyObject {
        match &self.events_iface {
            Some(p) => p.clone_ref(py).into_any(),
            None => py.None(),
        }
    }

    /// Read-only YAML datasources: `ctx.datasources`.
    ///
    /// ctx-attachment: always, `datasources_iface` is built systematically from
    ///     `manifest.datasources`.
    #[getter]
    fn datasources(&self, py: Python<'_>) -> PyObject {
        match &self.datasources_iface {
            Some(p) => p.clone_ref(py).into_any(),
            None => py.None(),
        }
    }

    /// Read-only Jinja2 templates: `ctx.templates`.
    ///
    /// ctx-attachment: always, `templates_iface` is built systematically, empty when
    ///     the manifest declares nothing.
    #[getter]
    fn templates(&self, py: Python<'_>) -> PyObject {
        match &self.templates_iface {
            Some(p) => p.clone_ref(py).into_any(),
            None => py.None(),
        }
    }

    /// Read-only secrets with manifest gating: `ctx.secrets`.
    ///
    /// ctx-attachment: always, `secrets_iface` is built systematically, empty when the
    ///     manifest declares nothing.
    #[getter]
    fn secrets(&self, py: Python<'_>) -> PyObject {
        match &self.secrets_iface {
            Some(p) => p.clone_ref(py).into_any(),
            None => py.None(),
        }
    }

    /// Read-only view of the execution budget: `ctx.budget`.
    ///
    /// Typed successor of `ctx.step_budget` (which stays functional and
    /// `#[deprecated]`). Fresh snapshot on each access.
    ///
    /// ctx-attachment: always, the production constructor sets `step_budget` to `Some`;
    ///     the two `None` sites are under `#[cfg(test)]`.
    #[getter]
    fn budget(&self, py: Python<'_>) -> PyResult<PyObject> {
        match &self.step_budget {
            Some(view) => {
                let elapsed = view.elapsed_secs();
                let wall_clock_remaining = self
                    .wall_clock_secs
                    .map(|secs| (secs as f64 - elapsed).max(0.0));
                let bv = crate::budget::BudgetView::new(
                    view.steps_remaining(),
                    view.tool_calls_remaining(),
                    elapsed,
                    wall_clock_remaining,
                );
                Ok(Py::new(py, bv)?.into_any())
            }
            None => Ok(py.None()),
        }
    }

    /// Structured logger preconfigured for this agent: `ctx.logger`.
    ///
    /// Returns `logging.getLogger("apollia.agent.{agent_id}")`. Handler
    /// configuration (Rust tracing relay, level, format) is handled by the
    /// Python SDK bootstrap. Here we only guarantee the agent always gets a
    /// named Logger.
    #[getter]
    fn logger<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let logger_name = format!("apollia.agent.{}", self.agent_id);
        let logging = py.import("logging")?;
        logging.call_method1("getLogger", (logger_name,))
    }

    /// User context injected in chat mode; `None` in task mode.
    ///
    /// Python property `ctx.user_context`. Returns a `dict[str, list[tuple[str, str]]]`
    /// with the categories `preferences`, `habits`, `context`, or Python `None`
    /// if the agent is not in chat mode or the user memory is empty.
    #[getter]
    fn user_context(&self, py: Python<'_>) -> PyResult<PyObject> {
        match &self.user_context {
            Some(ctx) => Ok(ctx.clone().into_pyobject(py)?.into_any().unbind()),
            None => Ok(py.None()),
        }
    }

    /// Whether this agent can write into the global user memory (`__user__`)
    /// via `ctx.memory.remember_user()`.
    ///
    /// Python property `ctx.user_memory_writable`. Reading `__user__` is always
    /// available via the `ctx.memory.recall()` fallback; this flag controls
    /// writes only.
    #[getter]
    fn user_memory_writable(&self) -> bool {
        self.user_memory_writable
    }

    /// Logs a message via the runtime's `tracing::` system.
    ///
    /// Messages are emitted with the `agent` field for correlation in
    /// structured traces. Accepted levels: `"debug"`, `"info"`, `"warn"`,
    /// `"error"`. Raises `ValueError` for any other level.
    ///
    /// **Side effect.** In addition to `tracing::*`, emits a
    /// `RuntimeEvent::AgentLog` on the `EventBus` if the context knows its
    /// `task_id` and has a configured bus. The `EventPersistor` in
    /// `apollia-runtime` consumes these events and persists them in
    /// `runtime_events.db`, making them available to the
    /// `GET /api/v1/tasks/{id}/trace` API and the `ExecutionTrace` view.
    /// `tracing::*` keeps writing to stderr in parallel for ops compatibility.
    #[pyo3(text_signature = "(self, level, message)")]
    fn log(&self, level: &str, message: &str) -> PyResult<()> {
        let agent = &self.agent_name;
        match level {
            "debug" => tracing::debug!(agent = %agent, message = %message, "agent.log"),
            "info" => tracing::info!(agent = %agent, message = %message, "agent.log"),
            "warn" => tracing::warn!(agent = %agent, message = %message, "agent.log"),
            "error" => tracing::error!(agent = %agent, message = %message, "agent.log"),
            other => {
                return Err(pyo3::exceptions::PyValueError::new_err(format!(
                    "Invalid log level '{other}'. Expected: debug, info, warn, error"
                )))
            }
        }

        // Persistence via EventBus to EventPersistor.
        // Conditional: a context without task_id (tests) or without a bus (CLI
        // dry-run) stays silent on the trace side but still emits to tracing.
        if let (Some(task_id), Some(bus)) = (self.task_id.as_ref(), self.event_bus.as_ref()) {
            // fire-and-forget: a saturated bus is traced by broadcast, the
            // send() error is silently ignored so the agent thread is never
            // blocked.
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

    /// Remaining execution budget for the current task (read-only).
    ///
    /// Returns a [`StepBudgetView`] with `steps_remaining`, `tool_calls_remaining`,
    /// and `elapsed_seconds`. Returns Python `None` if the context is not budgeted.
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
}

/// Computes the effective memory namespace for an agent in a session context.
///
/// If the agent runs inside a project (`project_id` not empty), the namespace
/// is prefixed with the `project_id` to guarantee isolation between projects.
///
/// Convention: `"{project_id}:{manifest_namespace}"` | `"{manifest_namespace}"`
///
/// The `shared_memory_namespaces` are NOT prefixed; they are global.
pub fn effective_memory_namespace(manifest_namespace: &str, project_id: Option<&str>) -> String {
    match project_id {
        Some(pid) if !pid.is_empty() => format!("{pid}:{manifest_namespace}"),
        _ => manifest_namespace.to_owned(),
    }
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

    // Mocks for building the ToolCallHelper (never actually called).

    struct NoopModel;

    #[async_trait::async_trait]
    impl CompletionModel for NoopModel {
        async fn complete(&self, _req: CompletionRequest) -> Result<CompletionResponse, LlmError> {
            Ok(CompletionResponse {
                engine_timings: None,
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

    /// `ctx.llm` is `None` if the router has no backend.
    #[tokio::test]
    async fn test_ctx_llm_none_if_no_backends() {
        // GIVEN an empty LlmRouter (0 backends)
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
            None,          // user_context
            None,          // a2a_invoker
            false,         // user_memory_writable
        );
        // THEN
        assert!(ctx.llm.is_none());
    }

    /// `AgentDegraded` emitted on EventBus if no LLM backend.
    #[tokio::test]
    async fn test_agent_degraded_emitted_if_no_llm() {
        // GIVEN an empty router and a bus with a receiver
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
            None,          // user_context
            None,          // a2a_invoker
            false,         // user_memory_writable
        );
        // THEN an AgentDegraded event is present on the bus
        let event = rx.try_recv().expect("one event must be present");
        assert!(
            matches!(
                event,
                RuntimeEvent::AgentDegraded { ref reason, .. }
                    if reason.contains("no LLM backend")
            ),
            "unexpected event: {event:?}"
        );
    }

    /// (variant) `ctx.llm` is `None` if `llm_router` is `None`.
    #[tokio::test]
    async fn test_ctx_llm_none_if_router_option_is_none() {
        // GIVEN llm_router = None (the Supervisor could not initialize the LLM)
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
            None,          // user_context
            None,          // a2a_invoker
            false,         // user_memory_writable
        );
        // THEN
        assert!(ctx.llm.is_none());
    }

    // ctx.log with valid level emits tracing event (no panic, no error)
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

    // ctx.log with invalid level raises ValueError
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

    // ctx.step_budget returns None when no budget is configured
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

    // steps_remaining reflects step_count atomically
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

    /// End-to-end builder test.
    /// `with_datasources(declared, Some(dir))` actually loads the YAML from
    /// disk and `ctx.datasources` exposes the value to Python.
    #[test]
    fn test_with_datasources_loads_from_real_dir() {
        // GIVEN a temp agent_dir with datasources/items.yaml
        let tmp = tempfile::tempdir().expect("temp dir");
        let ds_dir = tmp.path().join("datasources");
        std::fs::create_dir_all(&ds_dir).expect("mkdir");
        std::fs::write(ds_dir.join("items.yaml"), "- one\n- two\n- three\n").expect("write yaml");

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

    /// `with_templates(declared, Some(dir))` actually loads the Jinja2
    /// templates and rendering works with a Python dict.
    #[test]
    fn test_with_templates_renders_from_real_dir() {
        // GIVEN a temp agent_dir with templates/greeting.j2
        let tmp = tempfile::tempdir().expect("temp dir");
        let tpl_dir = tmp.path().join("templates");
        std::fs::create_dir_all(&tpl_dir).expect("mkdir");
        std::fs::write(tpl_dir.join("greeting.j2"), "Hello {{ who }}!").expect("write template");

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

    /// `with_datasources(declared, None)` performs no I/O; any read returns
    /// `FileNotFoundError("not found on disk")`.
    #[test]
    fn test_with_datasources_no_dir_keeps_empty_cache() {
        // GIVEN a context declaring a datasource with no directory behind it
        let ctx = RuntimeContext::for_test().with_datasources(vec!["foo".to_string()], None);
        pyo3::Python::with_gil(|py| {
            let ds_obj = ctx.datasources(py);
            let bound = ds_obj.bind(py);
            // WHEN the declared datasource is read from Python
            let err = bound
                .call_method1("get", ("foo",))
                .expect_err("should raise FileNotFoundError when no dir provided");
            // THEN the read raises FileNotFoundError instead of serving an empty value
            assert!(err.is_instance_of::<pyo3::exceptions::PyFileNotFoundError>(py));
        });
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicU32;

    use apollia_tools::{compute_input_hash, AuditTrailHandle, ToolRegistryHandle};

    use super::*;

    use crate::context::tool_proxy::ToolProxyError;
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

        let proxy = ToolProxy::new(ToolProxyConfig {
            registry: registry.clone(),
            audit: audit.clone(),
            executor,
            allowed_tools: allowed_tools.into_iter().map(String::from).collect(),
            agent_id: "test-agent".to_string(),
            task_id: "task-001".to_string(),
            run_id: None,
        });

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

    // Deferred MCP tool: allowlisted, no descriptor in the registry, yet the
    // call must reach the executor (the dispatcher is the real existence gate),
    // not fail with ToolNotFound.
    #[tokio::test]
    async fn test_deferred_mcp_tool_without_descriptor_reaches_executor() {
        // GIVEN an allowlisted `mcp:` tool with NO registered descriptor
        let expected = serde_json::json!({"content": "pong"});
        let (proxy, registry, audit) =
            make_proxy(vec!["mcp:demo/ping"], Ok(expected.clone())).await;

        // WHEN we call the deferred MCP tool
        let result = proxy
            .call_inner("mcp:demo/ping", serde_json::json!({}))
            .await;

        // THEN the executor runs and returns its output (no ToolNotFound)
        assert_eq!(
            result.expect("deferred mcp tool should reach executor"),
            expected
        );

        registry.shutdown().await;
        audit.shutdown().await;
    }

    // Full end-to-end proof of `ctx.tools.call("mcp:...")`: a ToolProxy wired
    // exactly as the runtime wires it (MCP executors -> ToolDispatcher ->
    // DispatcherExecutor -> ToolProxy) must actually EXECUTE an MCP tool against
    // a real stdio MCP server and return its output, with no REST workaround.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_mcp_tool_executes_end_to_end_via_tool_proxy() {
        use apollia_mcp::config::McpServerConfig;
        use apollia_mcp::executor::build_agent_tool_executors;
        use apollia_mcp::manager::McpClientManagerHandle;
        use apollia_mcp::session::LoadingMode;
        use apollia_tools::executor::ToolDispatcher;
        use std::collections::HashMap;

        // GIVEN a real stdio MCP server connected through the manager
        let registry = ToolRegistryHandle::start();
        let audit = open_test_audit().await;
        let mcp_config = McpServerConfig {
            format_version: 1,
            name: "calc".to_string(),
            command: "python3".to_string(),
            args: vec![concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../apollia-mcp/tests/mock_mcp_server.py"
            )
            .to_string()],
            env: HashMap::new(),
            transport: "stdio".to_string(),
            url: None,
            requires_approval: false,
            init_timeout_secs: 10,
            call_timeout_secs: 10,
            max_response_bytes: 8 * 1024 * 1024,
            max_tools: 256,
            tags: vec![],
        };
        let manager = McpClientManagerHandle::start(
            vec![mcp_config],
            &registry,
            None,
            None,
            LoadingMode::Eager,
        )
        .await
        .expect("mcp manager start failed");

        // AND a ToolProxy backed by the runtime's own MCP executor assembly
        let execs = build_agent_tool_executors(&manager).await;
        let dispatcher = Arc::new(ToolDispatcher::new(execs));
        let executor = Arc::new(DispatcherExecutor::new(dispatcher));
        let proxy = ToolProxy::new(ToolProxyConfig {
            registry: registry.clone(),
            audit: audit.clone(),
            executor,
            allowed_tools: vec!["mcp:calc/echo".to_string()],
            agent_id: "test-agent".to_string(),
            task_id: "task-mcp".to_string(),
            run_id: None,
        });

        // WHEN the agent calls the MCP tool the way `ctx.tools.call("mcp:...")` does
        let result = proxy
            .call_inner(
                "mcp:calc/echo",
                serde_json::json!({"message": "end to end"}),
            )
            .await;

        // THEN the tool truly executed and returned its output
        assert_eq!(
            result.expect("mcp tool must execute end to end via ToolProxy"),
            serde_json::json!({"content": "end to end"})
        );

        manager.shutdown().await;
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

    // Orchestrated ORIA path proof: `invoke_native` (the method the CLI's
    // OriaToolProxy adapter calls) executes a REAL tool through the dispatcher
    // and records it in the audit trail, using the exact `{"input": ...}`
    // payload shape the orchestrated ActorLoop sends for a tool step.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_invoke_native_executes_real_tool_and_audits() {
        use apollia_tools::executor::{ToolDispatcher, ToolExecutionError};
        use apollia_tools::ToolExecutor as NativeToolExecutor;

        // A real read-`input` tool exercised through the real dispatcher path
        // (DispatcherExecutor -> ToolDispatcher -> executor), same wiring as
        // production orchestrated execution.
        struct EchoInputExecutor;
        impl NativeToolExecutor for EchoInputExecutor {
            fn name(&self) -> &str {
                "echo"
            }
            fn execute(
                &self,
                input: serde_json::Value,
            ) -> std::pin::Pin<
                Box<
                    dyn std::future::Future<Output = Result<serde_json::Value, ToolExecutionError>>
                        + Send
                        + '_,
                >,
            > {
                Box::pin(async move {
                    let text = input
                        .get("input")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default()
                        .to_string();
                    Ok(serde_json::Value::String(format!("echo: {text}")))
                })
            }
        }

        // GIVEN a ToolProxy wired as the runtime wires it for orchestrated
        // execution (ToolDispatcher -> DispatcherExecutor -> ToolProxy) plus a
        // real registry descriptor and audit trail.
        let registry = ToolRegistryHandle::start();
        let audit = open_test_audit().await;
        let dispatcher = Arc::new(ToolDispatcher::new(vec![Box::new(EchoInputExecutor)]));
        let executor = Arc::new(DispatcherExecutor::new(dispatcher));
        let proxy = ToolProxy::new(ToolProxyConfig {
            registry: registry.clone(),
            audit: audit.clone(),
            executor,
            allowed_tools: vec!["echo".to_string()],
            agent_id: "orchestrated-agent".to_string(),
            task_id: "task-orch".to_string(),
            run_id: None,
        });
        let mut echo_desc = file_io_descriptor();
        echo_desc.name = "echo".to_string();
        registry.register(echo_desc).await.expect("register failed");

        // WHEN the ORIA adapter invokes the tool with the ActorLoop payload shape
        let result = proxy
            .invoke_native("echo", serde_json::json!({"input": "hello orchestrated"}))
            .await;

        // THEN the real tool executed and returned its output
        assert_eq!(
            result.expect("orchestrated tool must execute via invoke_native"),
            serde_json::Value::String("echo: hello orchestrated".to_string())
        );

        // AND the invocation was recorded in the audit trail (governance holds).
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        let records = audit.query_last(1).await;
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].tool_name, "echo");
        assert_eq!(records[0].agent_id, "orchestrated-agent");
        assert!(records[0].success);

        registry.shutdown().await;
        audit.shutdown().await;
    }

    // tool_input_schema returns the registered descriptor's input schema.
    #[tokio::test]
    async fn test_tool_input_schema_returns_registered_schema() {
        // GIVEN a ToolProxy and a registered descriptor carrying an input schema
        let (proxy, registry, audit) = make_proxy(vec!["file_io"], Ok(serde_json::json!({}))).await;
        let mut desc = file_io_descriptor();
        desc.input_schema = serde_json::json!({
            "type": "object",
            "properties": { "path": { "type": "string" } },
            "required": ["path"]
        });
        registry.register(desc).await.expect("register");

        // WHEN looking up the schema, and an unknown tool
        let schema = proxy.tool_input_schema("file_io").await;
        let missing = proxy.tool_input_schema("does_not_exist").await;

        // THEN the registered schema is returned, unknown tools yield None
        assert_eq!(
            schema,
            Some(serde_json::json!({
                "type": "object",
                "properties": { "path": { "type": "string" } },
                "required": ["path"]
            }))
        );
        assert_eq!(missing, None);

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

    // Direct-path budget enforcement on the tool chokepoint (C7-R1, principle #7).

    /// GIVEN a ToolProxy carrying a live budget view with max_tool_calls = 2
    /// WHEN the agent makes 3 tool calls
    /// THEN the first two run and the third is denied without invoking the tool,
    ///      and the shared budget records both tool calls
    #[tokio::test]
    async fn test_call_inner_enforces_tool_call_budget() {
        use apollia_llm::StepBudgetView;

        // GIVEN a proxy with a live budget of 2 tool calls (unlimited steps).
        let (proxy, registry, audit) = make_proxy(vec!["file_io"], Ok(serde_json::json!({}))).await;
        registry
            .register(file_io_descriptor())
            .await
            .expect("register failed");
        let steps = Arc::new(AtomicU32::new(0));
        let tool_calls = Arc::new(AtomicU32::new(0));
        let view = Arc::new(StepBudgetView::with_tool_tracking(
            Arc::clone(&steps),
            u32::MAX,
            Arc::clone(&tool_calls),
            2,
            std::time::Instant::now(),
        ));
        let proxy = proxy.with_budget(Arc::clone(&view));

        // WHEN three calls are made against a 2-call budget.
        let r1 = proxy.call_inner("file_io", serde_json::json!({})).await;
        let r2 = proxy.call_inner("file_io", serde_json::json!({})).await;
        let r3 = proxy.call_inner("file_io", serde_json::json!({})).await;

        // THEN the first two succeed and the third is denied on the budget.
        assert!(r1.is_ok(), "first call should run");
        assert!(r2.is_ok(), "second call should run");
        assert!(
            matches!(r3, Err(ToolProxyError::ExecutionFailed(ref m)) if m.contains("max_tool_calls")),
            "third call must be denied on the tool-call budget, got: {r3:?}"
        );
        // AND the shared counter reflects exactly the two allowed calls.
        assert_eq!(tool_calls.load(std::sync::atomic::Ordering::Relaxed), 2);

        registry.shutdown().await;
        audit.shutdown().await;
    }

    /// describe_inner returns a complete JSON Value for a fully populated descriptor.
    #[test]
    fn test_describe_inner_returns_json_value() {
        // GIVEN a ToolDescriptor with all fields populated
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

        // WHEN describe_inner is called
        let value = describe_inner(&descriptor);

        // THEN the result contains name, version, description, input_schema, output_schema, tags
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

    /// (Rust side) describe_inner on a minimal descriptor (optional fields empty/None).
    #[test]
    fn test_describe_inner_minimal_descriptor() {
        // GIVEN a ToolDescriptor with output_schema=None and empty tags
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

        // WHEN describe_inner is called
        let value = describe_inner(&descriptor);

        // THEN the optional fields are null/empty
        assert_eq!(value["name"], "minimal");
        assert_eq!(value["version"], "0.1.0");
        assert!(value["output_schema"].is_null());
        let tags = value["tags"].as_array().expect("tags should be an array");
        assert!(tags.is_empty());
    }

    // ctx.budget wall-clock plumbing
    /// `ctx.budget.wall_clock_remaining` stays `None` when the bridge did not
    /// propagate `wall_clock_secs` (test / dry-run mode).
    #[test]
    fn test_budget_wall_clock_none_by_default() {
        // GIVEN a ctx with a budget view but no wall_clock_secs configured
        use std::sync::atomic::AtomicU32;
        let view = Arc::new(StepBudgetView::new(Arc::new(AtomicU32::new(0)), 5));
        let mut ctx = RuntimeContext::for_test();
        ctx.step_budget = Some(view);

        // WHEN reading ctx.budget.wall_clock_remaining
        pyo3::Python::with_gil(|py| {
            let bv_obj = ctx.budget(py).expect("budget getter");
            let bound = bv_obj.bind(py);
            let wcr = bound
                .getattr("wall_clock_remaining")
                .expect("wall_clock_remaining attr");
            // THEN it is Python None.
            assert!(wcr.is_none(), "expected None, got: {wcr}");
        });
    }

    /// When `with_wall_clock_secs(n)` is called, `wall_clock_remaining`
    /// returns `max(0, n - elapsed)` instead of `None`.
    #[test]
    fn test_budget_wall_clock_remaining_reflects_manifest() {
        // GIVEN a ctx with budget view + wall_clock_secs=600 from manifest
        use std::sync::atomic::AtomicU32;
        let view = Arc::new(StepBudgetView::new(Arc::new(AtomicU32::new(0)), 5));
        let mut ctx = RuntimeContext::for_test().with_wall_clock_secs(600);
        ctx.step_budget = Some(view);

        // WHEN reading ctx.budget.wall_clock_remaining
        pyo3::Python::with_gil(|py| {
            let bv_obj = ctx.budget(py).expect("budget getter");
            let bound = bv_obj.bind(py);
            let wcr: f64 = bound
                .getattr("wall_clock_remaining")
                .expect("wall_clock_remaining attr")
                .extract()
                .expect("f64");
            // THEN it is close to 600 (we just constructed the view).
            assert!(
                wcr > 599.0 && wcr <= 600.0,
                "expected wall_clock_remaining near 600.0, got: {wcr}"
            );
        });
    }

    /// `ctx.agent_name` is exposed read-only to agents.
    #[test]
    fn test_agent_name_getter_exposed_to_python() {
        // GIVEN a for_test context (agent_name = "test-agent")
        let ctx = RuntimeContext::for_test();

        // WHEN reading agent_name via the Python getter
        // THEN it matches the underlying field.
        assert_eq!(ctx.agent_name(), "test-agent");
    }
}

#[cfg(test)]
mod a2a_tests {

    use super::*;

    use apollia_runtime::EventBus;
    use std::time::Duration;

    /// A durable send is received then acknowledged through the shared handle.
    #[tokio::test]
    async fn test_mailbox_handle_send_receive_ack() {
        // GIVEN an active durable mailbox
        let (event_tx, _event_rx) = EventBus::new();
        let handle = AgentMailboxHandle::spawn(None, event_tx, MailboxConfig::default()).await;

        // WHEN agent-a sends a message to agent-b, which receives and acks it
        let payload = serde_json::json!({"greeting": "hello"});
        let id = handle
            .send("agent-a", "agent-b", payload.clone(), None)
            .await
            .expect("send should succeed");
        let received = handle
            .receive("agent-b", None, Duration::from_secs(1))
            .await
            .expect("should receive a message");

        // THEN the message content matches and ack clears the inbox
        assert_eq!(received.message_id, id);
        assert_eq!(received.from, "agent-a");
        assert_eq!(received.payload, payload);
        handle.ack("agent-b", &id, None).await.expect("ack");
        assert_eq!(handle.pending_count("agent-b").await, 0);

        handle.shutdown().await;
    }

    /// Receive returns None when no message is pending.
    #[tokio::test]
    async fn test_mailbox_handle_receive_empty() {
        // GIVEN an active mailbox with no messages
        let (event_tx, _event_rx) = EventBus::new();
        let handle = AgentMailboxHandle::spawn(None, event_tx, MailboxConfig::default()).await;

        // WHEN we try to receive
        let result = handle
            .receive("agent-c", None, Duration::from_millis(50))
            .await;

        // THEN the result is None
        assert!(result.is_none());

        handle.shutdown().await;
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

        // WHEN the context is read back
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

        // WHEN user_context is read
        // THEN user_context is None
        assert!(ctx.user_context.is_none());
    }

    // GIVEN a RuntimeContext whose user_context is populated
    // WHEN the Python `ctx.user_context` getter is invoked
    // THEN it converts to a Python dict without unwrapping / panicking
    #[test]
    fn test_user_context_getter_returns_dict() {
        // GIVEN
        let mut uc = HashMap::new();
        uc.insert(
            "preferences".to_string(),
            vec![("langue".to_string(), "francais".to_string())],
        );
        let mut ctx = RuntimeContext::for_test();
        ctx.user_context = Some(uc);

        // WHEN / THEN
        Python::with_gil(|py| {
            let obj = ctx
                .user_context(py)
                .expect("getter must return Ok, not panic");
            let bound = obj.bind(py);
            assert!(!bound.is_none(), "populated user_context must not be None");
            assert!(
                bound.is_instance_of::<pyo3::types::PyDict>(),
                "user_context must convert to a Python dict"
            );
        });
    }

    // GIVEN a RuntimeContext with no user_context (task mode)
    // WHEN the Python getter is invoked
    // THEN it returns Python None, still via a Result (no unwrap)
    #[test]
    fn test_user_context_getter_none() {
        // GIVEN
        let ctx = RuntimeContext::for_test();

        // WHEN / THEN
        Python::with_gil(|py| {
            let obj = ctx.user_context(py).expect("getter must return Ok");
            assert!(obj.bind(py).is_none());
        });
    }
}

#[cfg(test)]
mod tool_proxy_a2a_tests {

    use super::*;

    use crate::context::tool_proxy::ToolProxyError;
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
            format_version: 1,
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
                examples: vec![],
            }],
            execution_mode: "direct".to_string(),
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
        let proxy = ToolProxy::new(ToolProxyConfig {
            registry,
            audit: audit.clone(),
            executor,
            allowed_tools: allowed.into_iter().map(String::from).collect(),
            agent_id: "director-agent".to_string(),
            task_id: "task-001".to_string(),
            run_id: None,
        });
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

    /// New `a2a__{name}` prefix is routed to the invoker exactly like the
    /// legacy `a2a:{name}` prefix, with `__` decoded back to `.` in the
    /// skill_id.
    #[tokio::test]
    async fn test_a2a_double_underscore_prefix_is_routed() {
        // GIVEN a proxy carrying an invoker and one tool named with the `a2a__` prefix
        let invoker = make_invoker_with_excel().await;
        let (proxy, audit) = make_base_proxy(vec!["a2a__read-excel"]).await;
        let proxy = proxy.with_a2a(invoker, 0, None);

        // WHEN that tool is called under its prefixed name
        let result = proxy
            .call_inner(
                "a2a__read-excel",
                serde_json::json!({"text": "process file.xlsx"}),
            )
            .await;

        // THEN the call routes to the invoker and the answer carries the worker header
        let val = result.expect("a2a__ prefix should route to invoker");
        let text = val["text"].as_str().expect("text field");
        assert!(
            text.contains("[read-excel via excel-worker]"),
            "expected formatted header in: {text}"
        );
        audit.shutdown().await;
    }
}

#[cfg(test)]
mod extract_a2a_skill_id_tests {

    use crate::context::tool_invoke::extract_a2a_skill_id;

    #[test]
    fn test_extract_a2a_legacy_colon_prefix() {
        // GIVEN a tool name carrying the legacy `a2a:` prefix
        // WHEN the skill id is extracted
        // THEN the prefix is stripped and the bare skill id comes back
        assert_eq!(
            extract_a2a_skill_id("a2a:read-excel"),
            Some("read-excel".to_string())
        );
    }

    #[test]
    fn test_extract_a2a_double_underscore_prefix_simple() {
        // GIVEN a tool name carrying the `a2a__` prefix over a single-word skill id
        // WHEN the skill id is extracted
        // THEN the prefix is stripped and the bare skill id comes back
        assert_eq!(
            extract_a2a_skill_id("a2a__summarize"),
            Some("summarize".to_string())
        );
    }

    #[test]
    fn test_extract_a2a_double_underscore_prefix_dotted() {
        // GIVEN a tool name whose `__` separators encode a dotted skill id
        // WHEN the skill id is extracted
        // THEN the dots are restored, so the invoker sees the canonical skill id
        assert_eq!(
            extract_a2a_skill_id("a2a__pdf__read_text"),
            Some("pdf.read_text".to_string())
        );
    }

    #[test]
    fn test_extract_a2a_no_prefix_returns_none() {
        // GIVEN two tool names that carry no `a2a` prefix, one of them a near miss
        // WHEN the skill id is extracted from each
        // THEN neither yields a skill id
        assert_eq!(extract_a2a_skill_id("bash"), None);
        assert_eq!(extract_a2a_skill_id("a2a_typo"), None);
    }
}

// Tests: WorkspaceContextPy

#[cfg(test)]
mod workspace_context_tests {

    use super::*;

    #[test]
    fn test_workspace_context_py_rules_getter() {
        // GIVEN a WorkspaceContextPy with a "Project rules" section
        let mut ws = WorkspaceContextPy::empty();
        ws.set_section("Project rules", "Reply in French".to_string());
        ws.set_section("Git", "branch: main".to_string());
        // WHEN its getters are called
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
        // WHEN its getters are called
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
        // WHEN the workspace is read back
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
        // WHEN the workspace field is read
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
        use apollia_memory::{
            manager::MemoryManager,
            semantic::{RememberInput, SemanticMemory},
        };
        use tempfile::TempDir;

        // GIVEN an agent that wrote "forbidden_deps" in proj_A
        let dir = TempDir::new().expect("temp dir");

        let ns_a = effective_memory_namespace("dev-assistant", Some("proj_A"));
        let ns_b = effective_memory_namespace("dev-assistant", Some("proj_B"));

        {
            let mut mgr = MemoryManager::new(dir.path(), Some(ns_a.clone()), vec![]);
            let store = mgr.store(&ns_a).expect("open store for proj_A");
            let sem = SemanticMemory::new(store);
            sem.remember(RememberInput {
                namespace: &ns_a,
                key: "forbidden_deps",
                value: &serde_json::json!("no_async_std"),
                confidence: 1.0,
                source: None,
                expires_at: None,
            })
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

/// Pure-helper suite run under Miri to check the FFI-adjacent Rust code for
/// undefined behavior (integer casts, string slicing, allocation).
///
/// Miri cannot execute the PyO3 boundary itself (`Python::with_gil` calls into
/// libpython, an unsupported foreign function), so these tests deliberately
/// touch only interpreter-free helpers. Run them with:
///   cargo +nightly miri test -p apollia-aip --lib miri_pure
/// A normal `cargo test` also runs them (they are fast and pure).
#[cfg(test)]
mod miri_pure {

    use crate::context::tool_invoke::{epoch_secs_to_ymd, extract_a2a_skill_id, is_leap};

    // GIVEN epoch seconds, WHEN converted, THEN the date arithmetic and its
    // `as i64` / `as u32` casts are free of undefined behavior and land on known
    // anchors.
    #[test]
    fn miri_pure_epoch_secs_to_ymd_anchors() {
        assert_eq!(epoch_secs_to_ymd(0), (1970, 1, 1));
        assert_eq!(epoch_secs_to_ymd(86_400), (1970, 1, 2));
        // 1970 is not a leap year: 365 days later is 1971-01-01.
        assert_eq!(epoch_secs_to_ymd(31_536_000), (1971, 1, 1));
        // 2024-01-01T00:00:00Z.
        assert_eq!(epoch_secs_to_ymd(1_704_067_200), (2024, 1, 1));
    }

    // GIVEN a sweep of timestamps, WHEN converted, THEN every result is a
    // structurally valid date (exercises the cast paths across many inputs).
    #[test]
    fn miri_pure_epoch_secs_to_ymd_sweep_is_valid() {
        // Bounded: the helper loops year by year, so keep inputs modest.
        for secs in (0u64..=200_000_000).step_by(7_000_000) {
            let (y, m, d) = epoch_secs_to_ymd(secs);
            assert!(y >= 1970);
            assert!((1..=12).contains(&m));
            assert!((1..=31).contains(&d));
        }
    }

    // GIVEN representative years, WHEN tested, THEN leap detection matches the
    // Gregorian rule.
    #[test]
    fn miri_pure_is_leap_matches_gregorian_rule() {
        assert!(is_leap(2000));
        assert!(is_leap(2024));
        assert!(!is_leap(1900));
        assert!(!is_leap(2023));
    }

    // GIVEN tool names, WHEN decoded, THEN prefix stripping and `__`->`.`
    // rewriting allocate correctly (string slicing under Miri).
    #[test]
    fn miri_pure_extract_a2a_skill_id_decodes_both_forms() {
        assert_eq!(
            extract_a2a_skill_id("a2a:read-excel").as_deref(),
            Some("read-excel")
        );
        assert_eq!(
            extract_a2a_skill_id("a2a__pdf__read_text").as_deref(),
            Some("pdf.read_text")
        );
        assert_eq!(extract_a2a_skill_id("bash"), None);
    }

    // GIVEN a namespace and an optional project id, WHEN composed, THEN the
    // prefixing rule holds and the formatting allocates cleanly.
    #[test]
    fn miri_pure_effective_memory_namespace_prefixes_project() {
        assert_eq!(
            super::effective_memory_namespace("mem", Some("proj")),
            "proj:mem"
        );
        assert_eq!(super::effective_memory_namespace("mem", None), "mem");
        assert_eq!(super::effective_memory_namespace("mem", Some("")), "mem");
    }
}
