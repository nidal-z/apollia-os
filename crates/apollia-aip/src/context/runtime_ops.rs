//! The Rust-side construction and wiring of `RuntimeContext`.
//!
//! Split out of `context.rs`: the pyclass and its `#[pymethods]` block stay in
//! the parent, the methods the runtime calls to build and equip a context
//! live here.

use std::collections::HashMap;
use std::sync::Arc;

use apollia_core::events::{AgentId, EventBusSender, RuntimeEvent};
use apollia_llm::{LlmRouter, ObservabilityConfig, StepBudgetView, ToolCallHelper};
use apollia_runtime::a2a::A2AInvoker;
use apollia_runtime::mailbox::AgentMailboxHandle;

use crate::context::{RuntimeContext, ToolProxy, WorkspaceContextPy};
use crate::llm::LlmProxy;

impl RuntimeContext {
    /// Builds the context with optional LLM injection, optional ToolProxy, and
    /// optional MemoryInterface.
    ///
    /// If `llm_router` is `None` or holds a router with no backend, `ctx.llm`
    /// is `None` and `RuntimeEvent::AgentDegraded` is emitted fire-and-forget
    /// on `event_bus` (`send()` errors silently ignored).
    ///
    /// If `tool_proxy` is `Some`, `ctx.tools` exposes the tools allocated to
    /// the agent.
    ///
    /// If `memory_interface` is `Some`, `ctx.memory` exposes the SQLite memory
    /// isolated to the namespace declared in the manifest.
    ///
    /// The context never panics at construction: degradation is signaled, but
    /// the agent decides whether the absence of a capability is fatal.
    // REASON: test constructor mirroring the production bridge wiring, handle by handle.
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
        user_context: Option<HashMap<String, Vec<(String, String)>>>,
        a2a_invoker: Option<Arc<A2AInvoker>>,
        user_memory_writable: bool,
    ) -> Self {
        let bus_for_token = event_bus.clone();
        let step_budget_arc = Arc::clone(&budget_view);
        let agent_id_stored = agent_id.clone();
        let llm = llm_router.and_then(|router| {
            if router.list().is_empty() {
                // fire-and-forget: send() errors silently ignored.
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

        // Build the nested surfaces.
        // All are built systematically (at worst in no-op mode) so that
        // `ctx.a2a`, `ctx.events`, etc. are always accessible without
        // branching on the Python side. The gating values (datasources,
        // templates, secrets) default to empty; the bridge overrides them
        // after reading the manifest via the dedicated builders.
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
            run_id: None,
            supports_mailbox: false,
            mailbox_allowlist: None,
            mailbox_send_requires_approval: false,
            agent_name,
            user_context,
            a2a_invoker,
            user_memory_writable,
            workspace: None,
            event_bus: Some(bus_for_token),
            chat_session_id: None,
            chat_message_id: None,
            step_budget: Some(step_budget_arc),
            notify: None,
            stt: None,
            profile: None,
            agent_id: agent_id_stored,
            task_id: None,
            a2a_iface,
            events_iface,
            datasources_iface,
            templates_iface,
            secrets_iface,
            wall_clock_secs: None,
        }
    }
    /// Builds a minimal context for intra-crate unit tests.
    ///
    /// All optional fields are `None`. Must never be called in production code.
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
            run_id: None,
            supports_mailbox: false,
            mailbox_allowlist: None,
            mailbox_send_requires_approval: false,
            agent_name: "test-agent".to_string(),
            user_context: None,
            a2a_invoker: None,
            user_memory_writable: false,
            workspace: None,
            event_bus: None,
            chat_session_id: None,
            chat_message_id: None,
            step_budget: None,
            notify: None,
            stt: None,
            profile: None,
            agent_id: test_agent_id,
            task_id: None,
            a2a_iface,
            events_iface,
            datasources_iface,
            templates_iface,
            secrets_iface,
            wall_clock_secs: None,
        }
    }
    /// Injects a collected [`WorkspaceSnapshot`] into this `RuntimeContext`.
    ///
    /// Converts the snapshot into a [`WorkspaceContextPy`] accessible from
    /// Python via `ctx.workspace`. Must be called after [`new_with_llm`](RuntimeContext::new_with_llm).
    pub fn with_workspace_snapshot(
        mut self,
        snapshot: &apollia_workspace::WorkspaceSnapshot,
    ) -> Self {
        let workspace_py = WorkspaceContextPy::from_snapshot(snapshot);
        self.workspace = pyo3::Python::with_gil(|py| pyo3::Py::new(py, workspace_py).ok());
        self
    }
    /// Initializes `ctx.workspace` with an empty context (no sections).
    ///
    /// Guarantees that `ctx.workspace` is not `None` even when no provider is
    /// configured. The bridge can then patch sections via `set_section`.
    pub fn with_empty_workspace(mut self) -> Self {
        let workspace_py = WorkspaceContextPy::empty();
        self.workspace = pyo3::Python::with_gil(|py| pyo3::Py::new(py, workspace_py).ok());
        self
    }
    /// Attaches a notification interface to this context.
    ///
    /// Called after [`new_with_llm`](RuntimeContext::new_with_llm) when a
    /// `NotificationEngineHandle` is available. If not called, `ctx.notify` is
    /// `None` (silent no-op on the Python side).
    pub fn with_notify(mut self, notify: crate::notify::PyNotifyInterface) -> Self {
        self.notify = pyo3::Python::with_gil(|py| pyo3::Py::new(py, notify).ok());
        self
    }
    /// Attaches an STT interface to this context.
    ///
    /// Called after [`new_with_llm`](RuntimeContext::new_with_llm) when an STT
    /// backend is available. If not called, `ctx.stt` is `None`.
    pub fn with_stt(mut self, stt: crate::stt::PySttInterface) -> Self {
        self.stt = pyo3::Python::with_gil(|py| pyo3::Py::new(py, stt).ok());
        self
    }
    /// Attaches a user profile interface to this context.
    ///
    /// Called after [`new_with_llm`](RuntimeContext::new_with_llm) when a
    /// `MemoryManager` targeting `__user__` is available. If not called,
    /// `ctx.profile` is `None`.
    pub fn with_profile(mut self, profile: crate::profile::ProfileInterface) -> Self {
        self.profile = pyo3::Python::with_gil(|py| pyo3::Py::new(py, profile).ok());
        self
    }
    /// Propagates the total wall-clock budget (in seconds) from the manifest
    /// to feed `ctx.budget.wall_clock_remaining`.
    ///
    /// Called by the `BridgeRunner`/`AipBridge` after reading the manifest.
    /// If not called, `ctx.budget.wall_clock_remaining` stays `None` (no
    /// deadline imposed from the agent's point of view).
    pub fn with_wall_clock_secs(mut self, secs: u64) -> Self {
        self.wall_clock_secs = Some(secs);
        self
    }
    /// Configures the target of the `ChatToken` events emitted by `emit_token()`.
    ///
    /// Called by the `BridgeRunner` in chat mode to link the context to the
    /// current session + message. If not called (e.g. CLI task mode),
    /// `emit_token()` is a no-op.
    pub fn with_chat_target(
        mut self,
        session_id: impl Into<String>,
        message_id: impl Into<String>,
    ) -> Self {
        self.chat_session_id = Some(session_id.into());
        self.chat_message_id = Some(message_id.into());
        self
    }
    /// Binds this context to the current task id.
    ///
    /// Must be called before `bridge.call_run()` so that `ctx.log()` can tag
    /// its `RuntimeEvent::AgentLog` events with the right `task_id` and they
    /// are findable in the task trace.
    ///
    /// Side effect: also propagates `task_id` to the internal [`LlmProxy`] for
    /// emitting `LlmCallStarted`.
    pub fn with_task_id(mut self, task_id: impl Into<String>) -> Self {
        let task_id_str = task_id.into();
        // Propagate to the LlmProxy for observability.
        if let Some(llm) = self.llm.take() {
            self.llm = Some(llm.with_task_context(task_id_str.clone(), self.agent_id.to_string()));
        }
        // Rebuild EventsInterface with the task_id injected.
        // The pyclass is immutable, so we replace the Py<> in place.
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
    /// Binds this context to the run it executes within.
    ///
    /// Propagates `run_id` to the internal [`LlmProxy`] so `LlmCallStarted`
    /// events carry it. Tool events are tagged separately via the
    /// [`ToolProxyConfig::run_id`] set at proxy construction. `None` leaves the
    /// execution uncorrelated to any run.
    pub fn with_run_id(mut self, run_id: Option<apollia_core::events::RunId>) -> Self {
        self.run_id = run_id.clone();
        if let Some(llm) = self.llm.take() {
            self.llm = Some(llm.with_run_id(run_id));
        }
        self
    }
    /// Declares the mailbox capability and its optional recipient allowlist.
    ///
    /// Called by the runtime from the agent manifest (`supports_mailbox`,
    /// `mailbox_allowlist`). When not called, `ctx.mail` refuses every call.
    pub fn with_mailbox_capability(
        mut self,
        supports_mailbox: bool,
        allowlist: Option<Vec<String>>,
        send_requires_approval: bool,
    ) -> Self {
        self.supports_mailbox = supports_mailbox;
        self.mailbox_allowlist = allowlist;
        self.mailbox_send_requires_approval = send_requires_approval;
        self
    }
    // Builders for the nested surfaces.
    /// Wires the datasources interface onto the context.
    ///
    /// Builds a [`crate::datasources::DatasourcesInterface`] gating on
    /// `declared` (typically `manifest.datasources`). If `agent_dir` is
    /// `Some`, immediately loads the files
    /// `<agent_dir>/datasources/<name>.yaml` into the internal cache; this is
    /// the path taken in production. For tests, passing `None` leaves the
    /// cache empty (any read returns
    /// `FileNotFoundError("not found on disk")`).
    ///
    /// Must be called after [`new_with_llm`](RuntimeContext::new_with_llm).
    pub fn with_datasources(
        mut self,
        declared: Vec<String>,
        agent_dir: Option<&std::path::Path>,
    ) -> Self {
        let mut iface = crate::datasources::DatasourcesInterface::new(declared);
        if let Some(dir) = agent_dir {
            let _ = iface.load_from_dir(dir);
        }
        self.datasources_iface = pyo3::Python::with_gil(|py| pyo3::Py::new(py, iface).ok());
        self
    }
    /// Wires the Jinja2 templates interface onto the context.
    ///
    /// Builds a [`crate::templates::TemplatesInterface`] gating on `declared`
    /// (typically `manifest.templates`). If `agent_dir` is `Some`, immediately
    /// compiles the Jinja templates from
    /// `<agent_dir>/templates/<name>.{j2,jinja2,jinja}`. For tests, passing
    /// `None` leaves the minijinja environment empty.
    pub fn with_templates(
        mut self,
        declared: Vec<String>,
        agent_dir: Option<&std::path::Path>,
    ) -> Self {
        let mut iface = crate::templates::TemplatesInterface::new(declared);
        if let Some(dir) = agent_dir {
            let _ = iface.load_from_dir(dir);
        }
        self.templates_iface = pyo3::Python::with_gil(|py| pyo3::Py::new(py, iface).ok());
        self
    }
    /// Wires the secrets interface onto the context.
    pub fn with_secrets(mut self, sc: crate::secrets::SecretsInterface) -> Self {
        self.secrets_iface = pyo3::Python::with_gil(|py| pyo3::Py::new(py, sc).ok());
        self
    }
}
