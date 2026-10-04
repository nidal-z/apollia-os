//! The execution backend the desktop hands to the runtime: the ORIA engine
//! wired to the local router, the PyO3 bridge that runs one agent, and the two
//! inert stand-ins used before the real wiring is available.

use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;

use apollia_aip::bridge::AIPBridge;
use apollia_aip::context::{DispatcherExecutor, RuntimeContext, ToolProxy, ToolProxyConfig};
use apollia_aip::memory::MemoryInterface;
use apollia_core::{
    AIPError, AIPResult, AIPTask, AgentManifest, ORIAConfig, PendingApprovals, TaskStatus,
    ToolsConfig,
};
use apollia_llm::{
    CompletionModel, CompletionRequest, CompletionResponse, LlmError, LlmRouter,
    ObservabilityConfig, ToolCallHelper, ToolInvoker,
};
use apollia_memory::manager::MemoryManager;
use apollia_oria::budget::StepBudget;
use apollia_oria::engine::{AgentRunner, ORIAEngine};
use apollia_oria::plan_cache::PlanCacheRepository;
use apollia_oria::PendingPlanGates;
use apollia_runtime::a2a::A2AInvoker;
use apollia_runtime::coordinator::{CancellationToken, ExecutionBackend};
use apollia_runtime::eventbus::EventBusSender;
use apollia_runtime::mailbox::AgentMailboxHandle;
use apollia_tools::tools::ask_user::PendingUserInputs;
use apollia_tools::{
    build_dispatcher_with, load_governance_snapshot, AuditTrailHandle, NativeDispatcherConfig,
    TaskRepository, ToolRegistryHandle,
};
use futures::stream;
use pyo3::prelude::*;

use super::factory::{direct_path_budget, wire_engine_with_llm};
use super::{default_data_dir, merge_disabled, open_secret_store};

// ─── LLM wrappers ─────────────────────────────────────────────────────────────

struct RouterModel(Arc<LlmRouter>);

#[async_trait::async_trait]
impl CompletionModel for RouterModel {
    async fn complete(&self, req: CompletionRequest) -> Result<CompletionResponse, LlmError> {
        self.0
            .complete_with_observability(None, req, None, &ObservabilityConfig::default())
            .await
    }

    async fn stream(
        &self,
        _req: CompletionRequest,
    ) -> Result<
        Pin<Box<dyn futures::Stream<Item = Result<apollia_llm::StreamChunk, LlmError>> + Send>>,
        LlmError,
    > {
        let s: Pin<
            Box<dyn futures::Stream<Item = Result<apollia_llm::StreamChunk, LlmError>> + Send>,
        > = Box::pin(stream::empty());
        Ok(s)
    }

    fn is_available(&self) -> bool {
        !self.0.list().is_empty()
    }
    fn backend_name(&self) -> &str {
        "router"
    }
    fn model_id(&self) -> &str {
        "router"
    }
}

struct NoopToolInvoker;

#[async_trait::async_trait]
impl ToolInvoker for NoopToolInvoker {
    async fn invoke(&self, name: &str, _args: &serde_json::Value) -> Result<String, String> {
        Err(format!(
            "tool '{name}' invocation via LLM loop not wired - use ctx.tools directly"
        ))
    }
}

// ─── File tool roots ──────────────────────────────────────────────────────────

/// The access an agent's executor checks, and the folders its file tools
/// reach: the folders granted to it, read from the `system.db` of `data_dir`.
/// An agent granted no folder reaches none, and its file tools say so.
fn file_tool_access(
    allowed_tools: Vec<String>,
    data_dir: &Path,
) -> (Vec<String>, apollia_tools::FolderRoots) {
    apollia_tools::granted_folders(allowed_tools, data_dir)
}

// ─── Fallback backend ─────────────────────────────────────────────────────────

#[derive(Clone)]
pub(super) struct NoopBackend;

impl ExecutionBackend for NoopBackend {
    fn execute(
        &self,
        task: AIPTask,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<AIPResult, String>> + Send>> {
        Box::pin(async move {
            Ok(AIPResult {
                task_id: task.task_id,
                status: TaskStatus::Failed,
                output: Vec::new(),
                error: Some(AIPError {
                    code: "NO_BACKEND".to_string(),
                    message: "no execution backend configured for this agent".to_string(),
                    details: None,
                }),
                artifacts: Vec::new(),
                input_required_data: None,
            })
        })
    }
}

// ─── Per-agent production backend ─────────────────────────────────────────────

pub(super) struct AIPProductionBackend {
    pub(super) bridge: Arc<AIPBridge>,
    pub(super) agent_id: String,
    /// The access list of the agent, read when each run starts.
    pub(super) access: apollia_tools::GrantedAccess,
    pub(super) llm_router: Option<Arc<LlmRouter>>,
    pub(super) event_bus: EventBusSender,
    pub(super) pending_approvals: Option<Arc<PendingApprovals>>,
    pub(super) task_repository: Option<Arc<TaskRepository>>,
    pub(super) tool_registry: Option<ToolRegistryHandle>,
    pub(super) audit_trail: Option<AuditTrailHandle>,
    pub(super) memory_namespace: Option<String>,
    pub(super) memory_base_dir: PathBuf,
    /// Manifest opt-in for `ctx.memory.remember_user()` writes into `__user__`.
    pub(super) user_memory_write: bool,
    /// MCP client manager handle so the BridgeRunner can construct one
    /// `McpToolExecutor` per active MCP tool and inject it into the agent's
    /// dispatcher.
    pub(super) mcp_handle: Option<apollia_mcp::manager::McpClientManagerHandle>,
    /// A2A orchestrator exposed both as virtual `a2a:*` tools (via `ToolProxy.with_a2a`
    /// and `allowed_tools` augmentation) and through `ctx.a2a_invoke()` for Python agents.
    pub(super) a2a_invoker: Option<Arc<A2AInvoker>>,
    /// Inter-agent mailbox, exposed as `ctx.mailbox` so agents can send/receive
    /// async messages without going through A2A skill delegation.
    pub(super) mailbox: Option<AgentMailboxHandle>,
    /// Operator-supplied tools configuration (`[tools]` section of apollia.toml).
    /// Drives `web_search`, `web_read`, `http_allowlist`, and statically-disabled tools.
    pub(super) tools_config: ToolsConfig,
    /// Datasources YAML declared in the manifest.
    pub(super) datasources_declared: Vec<String>,
    /// Jinja2 templates declared in the manifest.
    pub(super) templates_declared: Vec<String>,
    /// Agent root directory, used to resolve
    /// `datasources/<name>.yaml` and `templates/<name>.j2`.
    pub(super) agent_dir: Option<PathBuf>,
    /// Secrets declared in the manifest (`manifest.secrets`): the allowlist for
    /// `ctx.secrets.get()`.
    pub(super) secrets_declared: Vec<String>,
    /// Shared plan-gate registry. Wired so the desktop UI can resolve a pending
    /// gate via `submit_plan_decision`. `None` when the runtime exposed none.
    pub(super) plan_gates: Option<Arc<PendingPlanGates>>,
    /// Shared plan cache, the same repository the supervisor opened at boot and
    /// exposes over REST. `None` when it failed to open, in which case the
    /// engine re-plans every run.
    pub(super) plan_cache: Option<Arc<std::sync::Mutex<PlanCacheRepository>>>,
    /// Agent manifest: drives execution-mode routing, the orchestrated step
    /// budget, and the `AIPAgent` contract for the ORIA planner path.
    pub(super) manifest: AgentManifest,
}

impl Clone for AIPProductionBackend {
    fn clone(&self) -> Self {
        Self {
            bridge: Arc::clone(&self.bridge),
            agent_id: self.agent_id.clone(),
            access: self.access.clone(),
            llm_router: self.llm_router.clone(),
            event_bus: self.event_bus.clone(),
            tool_registry: self.tool_registry.clone(),
            audit_trail: self.audit_trail.clone(),
            memory_namespace: self.memory_namespace.clone(),
            memory_base_dir: self.memory_base_dir.clone(),
            pending_approvals: self.pending_approvals.clone(),
            task_repository: self.task_repository.clone(),
            user_memory_write: self.user_memory_write,
            mcp_handle: self.mcp_handle.clone(),
            a2a_invoker: self.a2a_invoker.clone(),
            mailbox: self.mailbox.clone(),
            tools_config: self.tools_config.clone(),
            datasources_declared: self.datasources_declared.clone(),
            templates_declared: self.templates_declared.clone(),
            agent_dir: self.agent_dir.clone(),
            secrets_declared: self.secrets_declared.clone(),
            plan_gates: self.plan_gates.clone(),
            plan_cache: self.plan_cache.clone(),
            manifest: self.manifest.clone(),
        }
    }
}

pub(super) struct BridgeRunner {
    pub(super) bridge: Arc<AIPBridge>,
    pub(super) llm_router: Option<Arc<LlmRouter>>,
    pub(super) event_bus: EventBusSender,
    pub(super) agent_id: String,
    pub(super) allowed_tools: Vec<String>,
    pub(super) tool_registry: Option<ToolRegistryHandle>,
    pub(super) audit_trail: Option<AuditTrailHandle>,
    pub(super) memory_namespace: Option<String>,
    pub(super) memory_base_dir: PathBuf,
    /// Manifest opt-in for `ctx.memory.remember_user()` writes into `__user__`.
    pub(super) user_memory_write: bool,
    /// `ask_user` pending-input registry. `Some` when invoked by the chat
    /// manager, `None` for standalone task-mode runs (HITL relies on AIP
    /// `input_required` instead).
    pub(super) pending_user_inputs: Option<PendingUserInputs>,
    /// Handle to the MCP client manager, used to build one
    /// `McpToolExecutor` per registered MCP tool and inject it into the agent's
    /// `ToolDispatcher`. `None` when MCP is not configured.
    pub(super) mcp_handle: Option<apollia_mcp::manager::McpClientManagerHandle>,
    /// Shared A2A orchestrator. When `Some`, `allowed_tools` is augmented with
    /// virtual `a2a:*` descriptors and the `ToolProxy` gets `.with_a2a(...)`.
    pub(super) a2a_invoker: Option<Arc<A2AInvoker>>,
    /// Inter-agent mailbox handle, exposed as `ctx.mailbox`.
    pub(super) mailbox: Option<AgentMailboxHandle>,
    /// Operator-supplied tools configuration (`[tools]` apollia.toml).
    pub(super) tools_config: ToolsConfig,
    /// Datasources YAML declared in the manifest.
    pub(super) datasources_declared: Vec<String>,
    /// Jinja2 templates declared in the manifest.
    pub(super) templates_declared: Vec<String>,
    /// Agent root directory, used to resolve `datasources/` and
    /// `templates/`.
    pub(super) agent_dir: Option<PathBuf>,
    /// Secrets declared in the manifest: the allowlist for `ctx.secrets.get()`.
    pub(super) secrets_declared: Vec<String>,
    /// Agent manifest, exposed via the `AIPAgent` contract for the orchestrated
    /// planner path (manifest + plan-complete hook).
    pub(super) manifest: AgentManifest,
    /// Direct-path `StepBudget`, shared with `execute_direct`. A live view of it
    /// is wired into `ctx.tools` and `ctx.llm` so the Python agent's tool and
    /// LLM calls are counted and cut off (principle #7, non-bypassable).
    pub(super) budget: Arc<StepBudget>,
    /// Cancels this run. Fired by the coordinator when the user stops the
    /// task; the Python dispatch is then cancelled at its next await point.
    pub(super) cancel: CancellationToken,
}

impl AgentRunner for BridgeRunner {
    fn call_run(
        &self,
        task: AIPTask,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<AIPResult, String>> + Send + '_>> {
        let bridge = Arc::clone(&self.bridge);
        let cancel = self.cancel.clone();
        let llm_router = self.llm_router.clone();
        let event_bus = self.event_bus.clone();
        let agent_id = self.agent_id.clone();
        let allowed_tools = self.allowed_tools.clone();
        let tool_registry = self.tool_registry.clone();
        let audit_trail = self.audit_trail.clone();
        let memory_namespace = self.memory_namespace.clone();
        let memory_config = self.manifest.memory_config.clone();
        let memory_base_dir = self.memory_base_dir.clone();
        let user_memory_write = self.user_memory_write;
        let pending_user_inputs = self.pending_user_inputs.clone();
        let mcp_handle = self.mcp_handle.clone();
        // This run's approval state: the response the task was resumed with, if
        // any, gates the MCP calls that need a human.
        let mcp_task_approval =
            apollia_mcp::task_approval::TaskApproval::new(task.input_response.clone());
        let mcp_tools_requiring_approval = self.manifest.tools_requiring_approval.clone();
        let supports_mailbox = self.manifest.supports_mailbox;
        let mailbox_allowlist = self.manifest.mailbox_allowlist.clone();
        let mailbox_send_gated = self
            .manifest
            .tools_requiring_approval
            .iter()
            .any(|t| t == "mailbox:send");
        let a2a_invoker = self.a2a_invoker.clone();
        let mailbox = self.mailbox.clone();
        let tools_config = self.tools_config.clone();
        let datasources_declared = self.datasources_declared.clone();
        let templates_declared = self.templates_declared.clone();
        let agent_dir = self.agent_dir.clone();
        let secrets_declared = self.secrets_declared.clone();
        let declared_backend = self.manifest.llm_backend.clone();
        let package_request = self.manifest.model.as_ref().and_then(|m| m.request.clone());
        // Live view of the Direct-path budget, shared into ctx.tools and ctx.llm.
        let budget_view = Arc::new(self.budget.to_live_budget_view());

        Box::pin(async move {
            let router_for_helper = llm_router
                .clone()
                .unwrap_or_else(|| Arc::new(LlmRouter::empty()));
            let tool_helper = Arc::new(ToolCallHelper::new(
                Arc::new(RouterModel(router_for_helper)),
                Arc::new(NoopToolInvoker),
            ));

            let governance_base = memory_base_dir
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_else(|| memory_base_dir.clone());
            let snapshot = load_governance_snapshot(&governance_base).unwrap_or_else(|e| {
                tracing::warn!(
                    error = %e,
                    detail = "every tool is enabled",
                    "tools.governance.unavailable"
                );
                Default::default()
            });

            // An access to a connector tool without its account names the
            // single connected account, or nothing.
            let allowed_tools = apollia_runtime::connectors_bridge::resolve_connector_access(
                &agent_id,
                allowed_tools,
            )
            .await;
            // The folders granted to the agent are the roots of its file tools.
            let (allowed_tools, folders) = file_tool_access(allowed_tools, &governance_base);

            // Merge operator-disabled (apollia.toml `[tools].disabled`) with
            // governance.db runtime-disabled: either source disables the tool.
            let disabled_tools = merge_disabled(&tools_config.disabled, snapshot.disabled_tools);
            // Build one McpToolExecutor per registered MCP tool so the agent's
            // ToolDispatcher can route `mcp:<server>/<tool>` invocations
            // through the MCP client manager. Without this, the registry
            // surfaces the tool to the agent's prompt but the dispatcher
            // returns UnknownTool at call time.
            let mut extra_executors =
                build_mcp_executors(&mcp_handle, mcp_task_approval, mcp_tools_requiring_approval)
                    .await;

            // Append the SaaS connector executors from the runtime: one per
            // operation and per account connected when the run starts, each
            // bound to the token of its account.
            extra_executors
                .extend(apollia_runtime::connectors_bridge::build_connector_executors().await);

            // A code folder checked for the agent grants `code_context`,
            // bound to its folders; without one the tool is neither granted
            // nor dispatched.
            let code_context =
                apollia_runtime::agents::sources::code_context_for(&governance_base, &agent_id);
            let allowed_tools =
                apollia_tools::granted_with_code_folders(allowed_tools, code_context.is_some());
            if let Some(tool) = code_context {
                extra_executors.push(Box::new(tool));
            }

            let dispatcher = Arc::new(build_dispatcher_with(
                &NativeDispatcherConfig {
                    session_id: None,
                    sandbox_roots: folders.read,
                    write_roots: folders.write,
                    agent_id: agent_id.clone(),
                    venv_base_dir: memory_base_dir
                        .parent()
                        .map(|p| p.join("venvs"))
                        .unwrap_or_else(|| memory_base_dir.join("venvs")),
                    memory_namespace: memory_namespace.clone(),
                    memory_shared_namespaces: Vec::new(),
                    memory_base_dir: memory_base_dir.clone(),
                    http_allowlist: None,
                    pending_user_inputs,
                    disabled_tools,
                    brave_api_key: snapshot.brave_api_key,
                    web_search_config: tools_config.web_search.clone(),
                    web_read_config: tools_config.web_read.clone(),
                    python_interpreter: tools_config.python_interpreter.clone(),
                    max_output_chars: tools_config.max_output_chars,
                },
                extra_executors,
            ));

            let tool_proxy = match (tool_registry.as_ref(), audit_trail.as_ref()) {
                (Some(registry), Some(audit)) => {
                    let proxy = ToolProxy::new(ToolProxyConfig {
                        registry: registry.clone(),
                        audit: audit.clone(),
                        executor: Arc::new(DispatcherExecutor::new(dispatcher)),
                        allowed_tools,
                        agent_id: agent_id.clone(),
                        task_id: task.task_id.clone(),
                        run_id: task.run_id.clone(),
                    })
                    // tool_call_* instrumentation on the EventBus.
                    .with_event_bus(event_bus.clone())
                    // Direct-path budget: count and cap the agent's tool calls.
                    .with_budget(Arc::clone(&budget_view));
                    // Wire A2A so `ctx.tools.invoke("a2a:<skill>")` routes through
                    // the orchestrator. Without this, the registry would still
                    // surface `a2a:*` in allowed_tools but the dispatcher would
                    // return UnknownTool at call time.
                    let proxy = if let Some(ref invoker) = a2a_invoker {
                        proxy.with_a2a(Arc::clone(invoker), 0, None)
                    } else {
                        proxy
                    };
                    Some(proxy)
                }
                _ => {
                    tracing::warn!(
                        agent = %agent_id,
                        reason = "tool registry or audit trail missing",
                        "agent.tool_proxy.unavailable"
                    );
                    None
                }
            };

            let memory_interface = memory_namespace.as_deref().and_then(|ns| {
                let eff_ns = ns.to_owned();
                let mut manager =
                    MemoryManager::new(&memory_base_dir, Some(eff_ns.clone()), vec![]);
                if let Some(ref memory_config) = memory_config {
                    manager.start_auto_purge(memory_config, &eff_ns);
                }
                let iface = MemoryInterface::new(manager, eff_ns, agent_id.clone())?;
                iface.announce_shared_namespaces(&event_bus);
                Some(iface)
            });

            // Chat mode wiring: when the task carries a message_id, the
            // runtime is invoked from the chat session manager, so configure
            // the context so `ctx.emit_token()` routes tokens back to the
            // right SSE session.
            let chat_target = task
                .message_id
                .clone()
                .map(|mid| (task.context_id.clone(), mid));

            // Expose ctx.profile against the canonical user-profile database
            // (`~/.apollia/user_memory.db`), the same file the desktop
            // `get_profile` command and the CLI read, so writes land where
            // Settings > Profile looks.
            let profile_interface = {
                let user_memory_db =
                    apollia_core::paths::DataFile::UserMemory.path(&default_data_dir());
                let is_onboarding = agent_id == "onboarding-agent";
                Some(apollia_aip::profile::ProfileInterface::new(
                    user_memory_db,
                    agent_id.clone(),
                    user_memory_write,
                    is_onboarding,
                ))
            };

            let agent_id_for_model = agent_id.clone();
            let ctx: PyObject = Python::with_gil(|py| {
                let mut ctx = RuntimeContext::new_with_llm(
                    llm_router,
                    Arc::clone(&budget_view),
                    tool_helper,
                    Arc::new(ObservabilityConfig::default()),
                    event_bus,
                    agent_id.clone().into(),
                    tool_proxy,
                    memory_interface,
                    mailbox,
                    agent_id,
                    a2a_invoker,
                    user_memory_write,
                );
                if let Some(profile) = profile_interface {
                    ctx = ctx.with_profile(profile);
                }
                // The model this agent asks for, read again at each `ctx.llm` call.
                ctx = ctx.with_agent_model(
                    apollia_llm::AgentModelBinding::new(
                        agent_id_for_model,
                        declared_backend,
                        Some(apollia_core::paths::DataFile::System.path(&default_data_dir())),
                    )
                    .with_package_request(package_request),
                );
                if let Some((session_id, message_id)) = chat_target {
                    ctx = ctx.with_chat_target(session_id, message_id);
                }
                // Datasources YAML + Jinja2 templates.
                ctx = ctx
                    .with_datasources(datasources_declared, agent_dir.as_deref())
                    .with_templates(templates_declared, agent_dir.as_deref());
                // ctx.secrets is read-only and gated by the manifest. The store
                // is opened here (not captured in the BridgeRunner) to avoid
                // holding a Mutex across awaits and to pick up cleanly any
                // secret changes made during the session.
                let secret_store = open_secret_store(&default_data_dir());
                ctx = ctx.with_secrets(apollia_aip::secrets::SecretsInterface::new(
                    secret_store,
                    secrets_declared,
                ));
                // Bind the context to the task so ctx.log() correctly labels
                // the persisted RuntimeEvent::AgentLog entries.
                ctx = ctx.with_task_id(task.task_id.clone());
                ctx = ctx.with_run_id(task.run_id.clone());
                ctx = ctx.with_mailbox_capability(
                    supports_mailbox,
                    mailbox_allowlist,
                    mailbox_send_gated,
                );
                Py::new(py, ctx)
                    .map(|p| p.into_any())
                    .map_err(|e| format!("RuntimeContext PyObject construction failed: {e}"))
            })?;

            bridge
                .call_run_until(&task, ctx, cancel.cancelled())
                .await
                .map_err(|e| e.to_string())
        })
    }
}

/// The router held behind the shared lock, recovered when a panic poisoned it.
///
/// Poisoning records that a previous holder panicked; the value it protects is
/// an `Option<Arc<LlmRouter>>`, which no panic can leave half written.
/// Recovering it keeps one panic from leaving every later agent without a
/// router, which is what the `expect` on this lock did.
pub(crate) fn llm_router_snapshot(
    lock: &std::sync::RwLock<Option<Arc<LlmRouter>>>,
) -> Option<Arc<LlmRouter>> {
    lock.read()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone()
}

/// Build one `McpToolExecutor` per registered MCP tool so the agent's
/// `ToolDispatcher` can route `mcp:<server>/<tool>` invocations through the MCP
/// client manager. Returns an empty Vec when no MCP handle is wired.
///
/// Gated through the task's approval state: a call to a server declared
/// `requires_approval`, or to a tool the manifest lists as requiring approval,
/// pauses the task and runs once it is resumed with that call approved.
async fn build_mcp_executors(
    mcp_handle: &Option<apollia_mcp::manager::McpClientManagerHandle>,
    approval: apollia_mcp::task_approval::TaskApproval,
    tools_requiring_approval: Vec<String>,
) -> Vec<Box<dyn apollia_tools::executor::ToolExecutor>> {
    match mcp_handle {
        Some(handle) => {
            apollia_mcp::executor::build_task_tool_executors(
                handle,
                approval,
                tools_requiring_approval,
            )
            .await
        }
        None => Vec::new(),
    }
}

impl apollia_oria::engine::AIPAgent for BridgeRunner {
    fn manifest(&self) -> AgentManifest {
        self.manifest.clone()
    }
}

impl ExecutionBackend for AIPProductionBackend {
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
        // Agent budget capped to the runtime ceiling, shared into the runner's
        // ctx and the engine supervisor so the Direct path is bounded on the
        // step/tool_call dimensions, not wall-clock alone (principle #7).
        let agent_step_budget = self.manifest.step_budget.clone().unwrap_or_default();
        let direct_budget = Arc::new(direct_path_budget(&agent_step_budget));

        let runner = BridgeRunner {
            bridge: Arc::clone(&self.bridge),
            llm_router: self.llm_router.clone(),
            event_bus: self.event_bus.clone(),
            agent_id: self.agent_id.clone(),
            allowed_tools: self.access.current(),
            tool_registry: self.tool_registry.clone(),
            audit_trail: self.audit_trail.clone(),
            memory_namespace: self.memory_namespace.clone(),
            memory_base_dir: self.memory_base_dir.clone(),
            user_memory_write: self.user_memory_write,
            // Task-mode backend: no chat UI to answer `ask_user` prompts.
            pending_user_inputs: None,
            mcp_handle: self.mcp_handle.clone(),
            a2a_invoker: self.a2a_invoker.clone(),
            mailbox: self.mailbox.clone(),
            tools_config: self.tools_config.clone(),
            datasources_declared: self.datasources_declared.clone(),
            templates_declared: self.templates_declared.clone(),
            agent_dir: self.agent_dir.clone(),
            secrets_declared: self.secrets_declared.clone(),
            manifest: self.manifest.clone(),
            budget: Arc::clone(&direct_budget),
            cancel,
        };

        let mut engine = ORIAEngine::new().with_event_bus(self.event_bus.clone());
        engine = engine.with_agent_name(self.manifest.name.clone());
        if let Some(pending) = self.pending_approvals.clone() {
            engine = engine.with_pending_approvals(pending);
        }
        if let Some(repo) = self.task_repository.clone() {
            engine = engine.with_task_repository(repo);
        }

        // Plan-mode gate: the per-run override wins, otherwise the autonomy tier
        // drives the policy (default Assisted = active). The shared registry is
        // wired so the desktop UI can resolve the gate via submit_plan_decision.
        engine = engine.with_plan_gate_override(task.run_options.plan_gate);
        if let Some(gates) = self.plan_gates.clone() {
            engine = engine.with_pending_plan_gates(gates);
        }
        if let Some(cache) = self.plan_cache.clone() {
            engine = engine.with_shared_plan_cache(cache);
        }
        if let Some(tier) = task.run_options.autonomy_level {
            engine = engine.with_oria_config(ORIAConfig {
                autonomy_level: Some(tier),
                ..ORIAConfig::default()
            });
        }
        let step_budget_max = self
            .manifest
            .step_budget
            .as_ref()
            .map(|b| b.max_steps)
            .unwrap_or(20);
        engine = wire_engine_with_llm(
            engine,
            self.llm_router.clone(),
            &apollia_llm::AgentModelBinding::new(
                self.agent_id.clone(),
                self.manifest.llm_backend.clone(),
                Some(apollia_core::paths::DataFile::System.path(&default_data_dir())),
            ),
            Some(&self.event_bus),
            step_budget_max,
        );

        // Orchestrated agents flow through ORIA's planner + ActorLoop (where the
        // plan gate lives); everything else uses the direct dispatch path.
        let execution_mode = self.manifest.execution_mode.clone();
        Box::pin(async move {
            if execution_mode == "orchestrated" {
                Ok(engine.execute(task, &runner).await)
            } else {
                engine
                    .execute_direct(task, &runner, direct_budget)
                    .await
                    .map_err(|e| e.to_string())
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::file_tool_access;
    use apollia_core::paths::DataFile;
    use apollia_core::FolderConnectionStore;

    #[test]
    fn a_task_reaches_the_folders_granted_to_its_agent_and_no_other() {
        // GIVEN two stored folders, the agent granted the first to read only
        let data = tempfile::tempdir().expect("tempdir");
        let granted = tempfile::tempdir().expect("tempdir");
        let other = tempfile::tempdir().expect("tempdir");
        let store = FolderConnectionStore::open(&DataFile::System.path(data.path()))
            .expect("open system.db");
        let granted = store
            .add(&granted.path().to_string_lossy(), None)
            .expect("add");
        store
            .add(&other.path().to_string_lossy(), None)
            .expect("add");
        let access = vec![format!("conn:folder:{}/file_read", granted.id)];

        // WHEN the task path derives the roots of its file tools
        let (tools, roots) = file_tool_access(access, data.path());

        // THEN file_read is granted over that folder alone, and nothing writes
        assert_eq!(tools, vec!["file_read".to_owned()]);
        assert_eq!(roots.read, vec![std::path::PathBuf::from(&granted.path)]);
        assert!(roots.write.is_empty());
    }

    #[test]
    fn a_task_whose_agent_has_no_folder_reaches_none() {
        // GIVEN an agent granted file_read but no folder, the home folder
        // being no longer a default root
        let data = tempfile::tempdir().expect("tempdir");

        // WHEN the task path derives the roots of its file tools
        let (tools, roots) = file_tool_access(vec!["file_read".to_owned()], data.path());

        // THEN the tool stays granted and reaches nothing
        assert_eq!(tools, vec!["file_read".to_owned()]);
        assert!(roots.read.is_empty() && roots.write.is_empty());
    }
}
