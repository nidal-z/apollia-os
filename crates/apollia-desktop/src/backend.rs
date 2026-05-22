//! Production execution backend for the Apollia Desktop app.
//!
//! Mirrors the production setup used by `apollia-os start` (apollia-cli).
//! See `apollia-cli/src/commands/start.rs` for the CLI counterpart.

use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;

use apollia_aip::bridge::AIPBridge;
use apollia_aip::context::{
    effective_memory_namespace, DispatcherExecutor, RuntimeContext, ToolProxy,
};
use apollia_aip::memory::MemoryInterface;
use apollia_core::{
    AIPError, AIPResult, AIPTask, AgentManifest, PendingApprovals, TaskStatus, ToolsConfig,
};
use apollia_llm::{
    CompletionModel, CompletionRequest, CompletionResponse, LlmError, LlmRouter,
    ObservabilityConfig, StepBudgetView, ToolCallHelper, ToolInvoker,
};
use apollia_memory::manager::MemoryManager;
use apollia_memory::user_memory::UserMemoryRepository;
use apollia_oria::budget::StepBudget;
use apollia_oria::engine::{AgentRunner, ORIAEngine};
use apollia_runtime::a2a::{
    make_delegate_fn, A2AInvoker, A2AToolsProvider, A2aDelegateFn, DEFAULT_A2A_MAX_HOPS,
};
use apollia_runtime::api::routes_agents::{AgentBackendFactory, AgentLoader};
use apollia_runtime::coordinator::{DynBackend, ExecutionBackend};
use apollia_runtime::eventbus::EventBusSender;
use apollia_runtime::mailbox::AgentMailboxHandle;
use apollia_runtime::registry::AgentRegistryHandle;
use apollia_runtime::router::TaskRouterHandle;
use apollia_tools::tools::ask_user::PendingUserInputs;
use apollia_tools::{
    build_dispatcher_with, load_governance_snapshot, AuditTrailHandle, NativeDispatcherConfig,
    TaskRepository, ToolCredentialStore, ToolRegistryHandle,
};
use futures::stream;
use pyo3::prelude::*;

// ─── Agent loader ─────────────────────────────────────────────────────────────

/// Compute the per-agent venv's site-packages directories, given the agent's
/// installed `.py` path. Convention : the agent name matches the `.py` file
/// stem (e.g. `chart-worker.py` → agent `chart-worker` → venv at
/// `~/.apollia/venvs/chart-worker/venv/`).
fn venv_site_packages_for_agent_path(agent_py_path: &Path) -> Vec<PathBuf> {
    let agent_name = match agent_py_path.file_stem().and_then(|s| s.to_str()) {
        Some(s) => s,
        None => return Vec::new(),
    };
    venv_site_packages_for_agent_name(agent_name)
}

/// Same as [`venv_site_packages_for_agent_path`] but takes the agent name
/// directly. Use this when the manifest is already available.
fn venv_site_packages_for_agent_name(agent_name: &str) -> Vec<PathBuf> {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    let base = PathBuf::from(home).join(".apollia").join("venvs");
    apollia_tools::tools::python_executor::agent_venv_site_packages(&base, agent_name)
}

/// Real agent loader using AIPLoader + validate_agent (ADR-019).
///
/// Injects the agent's per-package venv into `sys.path` so top-level imports
/// of pip-installed packages (e.g. `matplotlib`, `openpyxl`) resolve at
/// runtime. The venv path is derived from the agent name (= `.py` file stem).
pub struct AIPAgentLoader;

impl AgentLoader for AIPAgentLoader {
    fn load_and_validate(&self, path: &Path) -> Result<AgentManifest, String> {
        let extras = venv_site_packages_for_agent_path(path);
        let module = apollia_aip::loader::load_agent_module_with_sys_paths(path, &extras)
            .map_err(|e| e.to_string())?;
        let validated =
            apollia_aip::validator::validate_agent(&module).map_err(|e| e.to_string())?;
        Ok(validated.manifest)
    }
}

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
            "tool '{name}' invocation via LLM loop not wired — use ctx.tools directly"
        ))
    }
}

// ─── Sandbox root helper ──────────────────────────────────────────────────────

/// Return the filesystem sandbox root used for file-oriented native tools.
///
/// Keeps the previous `$HOME` behaviour so workspaces located under the user's
/// home directory remain reachable by `file_read`, `file_write`, etc.
fn sandbox_root_for_agent() -> PathBuf {
    std::env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| std::env::temp_dir())
}

// ─── Fallback backend ─────────────────────────────────────────────────────────

#[derive(Clone)]
struct NoopBackend;

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

struct AIPProductionBackend {
    bridge: Arc<AIPBridge>,
    agent_id: String,
    allowed_tools: Vec<String>,
    llm_router: Option<Arc<LlmRouter>>,
    event_bus: EventBusSender,
    pending_approvals: Option<Arc<PendingApprovals>>,
    task_repository: Option<Arc<TaskRepository>>,
    tool_registry: Option<ToolRegistryHandle>,
    audit_trail: Option<AuditTrailHandle>,
    memory_namespace: Option<String>,
    memory_base_dir: PathBuf,
    /// Manifest opt-in for `ctx.memory.remember_user()` writes into `__user__`.
    user_memory_write: bool,
    /// MCP client manager handle so the BridgeRunner can construct one
    /// `McpToolExecutor` per active MCP tool and inject it into the agent's
    /// dispatcher.
    mcp_handle: Option<apollia_mcp::manager::McpClientManagerHandle>,
    /// `manifest.supports_a2a` — propagated into the `RuntimeContext` so Python
    /// agents can introspect their own A2A eligibility.
    supports_a2a: bool,
    /// Type-erased A2A delegate (Director → Worker). `None` when the runtime
    /// registry/router were not available at factory construction time.
    a2a_delegate: Option<A2aDelegateFn>,
    /// A2A orchestrator exposed both as virtual `a2a:*` tools (via `ToolProxy.with_a2a`
    /// and `allowed_tools` augmentation) and through `ctx.a2a_invoke()` for Python agents.
    a2a_invoker: Option<Arc<A2AInvoker>>,
    /// Inter-agent mailbox — exposed as `ctx.mailbox` so agents can send/receive
    /// async messages without going through A2A skill delegation.
    mailbox: Option<AgentMailboxHandle>,
    /// Operator-supplied tools configuration (`[tools]` section of apollia.toml).
    /// Drives `web_search`, `web_read`, `http_allowlist`, and statically-disabled tools.
    tools_config: ToolsConfig,
    /// Datasources YAML déclarées au manifest (ADR-103, LOT 5).
    datasources_declared: Vec<String>,
    /// Templates Jinja2 déclarés au manifest (ADR-103, LOT 5).
    templates_declared: Vec<String>,
    /// Répertoire racine de l'agent — utilisé pour résoudre
    /// `datasources/<name>.yaml` et `templates/<name>.j2` (ADR-103, LOT 5).
    agent_dir: Option<PathBuf>,
    /// Secrets déclarés au manifest (`manifest.secrets`) — allowlist
    /// `ctx.secrets.get()` (ADR-104, LOT 6).
    secrets_declared: Vec<String>,
}

impl Clone for AIPProductionBackend {
    fn clone(&self) -> Self {
        Self {
            bridge: Arc::clone(&self.bridge),
            agent_id: self.agent_id.clone(),
            allowed_tools: self.allowed_tools.clone(),
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
            supports_a2a: self.supports_a2a,
            a2a_delegate: self.a2a_delegate.clone(),
            a2a_invoker: self.a2a_invoker.clone(),
            mailbox: self.mailbox.clone(),
            tools_config: self.tools_config.clone(),
            datasources_declared: self.datasources_declared.clone(),
            templates_declared: self.templates_declared.clone(),
            agent_dir: self.agent_dir.clone(),
            secrets_declared: self.secrets_declared.clone(),
        }
    }
}

struct BridgeRunner {
    bridge: Arc<AIPBridge>,
    llm_router: Option<Arc<LlmRouter>>,
    event_bus: EventBusSender,
    agent_id: String,
    allowed_tools: Vec<String>,
    tool_registry: Option<ToolRegistryHandle>,
    audit_trail: Option<AuditTrailHandle>,
    memory_namespace: Option<String>,
    memory_base_dir: PathBuf,
    /// Manifest opt-in for `ctx.memory.remember_user()` writes into `__user__`.
    user_memory_write: bool,
    /// `ask_user` pending-input registry. `Some` when invoked by the chat
    /// manager, `None` for standalone task-mode runs (HITL relies on AIP
    /// `input_required` instead).
    pending_user_inputs: Option<PendingUserInputs>,
    /// Handle vers le MCP client manager — utilisé pour construire un
    /// `McpToolExecutor` par tool MCP enregistré et l'injecter dans le
    /// `ToolDispatcher` de l'agent. `None` quand MCP n'est pas configuré.
    mcp_handle: Option<apollia_mcp::manager::McpClientManagerHandle>,
    /// `manifest.supports_a2a` — exposed in `RuntimeContext`.
    supports_a2a: bool,
    /// Type-erased A2A delegate. `None` when registry/router were unavailable.
    a2a_delegate: Option<A2aDelegateFn>,
    /// Shared A2A orchestrator. When `Some`, `allowed_tools` is augmented with
    /// virtual `a2a:*` descriptors and the `ToolProxy` gets `.with_a2a(...)`.
    a2a_invoker: Option<Arc<A2AInvoker>>,
    /// Inter-agent mailbox handle — exposed as `ctx.mailbox`.
    mailbox: Option<AgentMailboxHandle>,
    /// Per-task user context (typically `{"profile": [(k, v), ...]}`). Populated
    /// only by the chat-agent runner so chat-mode Python agents see the same
    /// profile data Chat Libre receives. `None` in task-mode (triggers/API).
    user_context: Option<std::collections::HashMap<String, Vec<(String, String)>>>,
    /// Operator-supplied tools configuration (`[tools]` apollia.toml).
    tools_config: ToolsConfig,
    /// Datasources YAML déclarées au manifest (ADR-103, LOT 5).
    datasources_declared: Vec<String>,
    /// Templates Jinja2 déclarés au manifest (ADR-103, LOT 5).
    templates_declared: Vec<String>,
    /// Répertoire racine de l'agent — résolution `datasources/` et
    /// `templates/` (ADR-103, LOT 5).
    agent_dir: Option<PathBuf>,
    /// Secrets déclarés au manifest — allowlist `ctx.secrets.get()`
    /// (ADR-104, LOT 6).
    secrets_declared: Vec<String>,
}

impl AgentRunner for BridgeRunner {
    fn call_run(
        &self,
        task: AIPTask,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<AIPResult, String>> + Send + '_>> {
        let bridge = Arc::clone(&self.bridge);
        let llm_router = self.llm_router.clone();
        let event_bus = self.event_bus.clone();
        let agent_id = self.agent_id.clone();
        let mut allowed_tools = self.allowed_tools.clone();
        let tool_registry = self.tool_registry.clone();
        let audit_trail = self.audit_trail.clone();
        let memory_namespace = self.memory_namespace.clone();
        let memory_base_dir = self.memory_base_dir.clone();
        let user_memory_write = self.user_memory_write;
        let pending_user_inputs = self.pending_user_inputs.clone();
        let mcp_handle = self.mcp_handle.clone();
        let supports_a2a = self.supports_a2a;
        let a2a_delegate = self.a2a_delegate.clone();
        let a2a_invoker = self.a2a_invoker.clone();
        let mailbox = self.mailbox.clone();
        let user_context = self.user_context.clone();
        let tools_config = self.tools_config.clone();
        let datasources_declared = self.datasources_declared.clone();
        let templates_declared = self.templates_declared.clone();
        let agent_dir = self.agent_dir.clone();
        let secrets_declared = self.secrets_declared.clone();

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
                tracing::warn!(error = %e, "governance snapshot unavailable — defaulting to all tools enabled");
                Default::default()
            });

            // Augment allowed_tools with live A2A virtual skills (`a2a:*`).
            // Without this, even when ToolProxy gets `.with_a2a(...)`, the
            // ToolRegistry filter would reject the call as "unknown tool".
            if let Some(ref invoker) = a2a_invoker {
                let descriptors = A2AToolsProvider::new(Arc::clone(invoker))
                    .build_tool_descriptors()
                    .await;
                for desc in descriptors {
                    if !allowed_tools.iter().any(|n| n == &desc.name) {
                        allowed_tools.push(desc.name);
                    }
                }
            }

            // Merge operator-disabled (apollia.toml `[tools].disabled`) with
            // governance.db runtime-disabled — either source disables the tool.
            let disabled_tools = merge_disabled(&tools_config.disabled, snapshot.disabled_tools);
            // Build one McpToolExecutor per registered MCP tool so the agent's
            // ToolDispatcher can route `mcp:<server>/<tool>` invocations
            // through the MCP client manager. Without this, the registry
            // surfaces the tool to the agent's prompt but the dispatcher
            // returns UnknownTool at call time.
            let mut extra_executors: Vec<Box<dyn apollia_tools::executor::ToolExecutor>> =
                if let Some(handle) = &mcp_handle {
                    let mut execs: Vec<Box<dyn apollia_tools::executor::ToolExecutor>> = Vec::new();
                    for status in handle.status().await {
                        if !status.connected {
                            continue;
                        }
                        let Some(detail) = handle.server_detail(&status.name).await else {
                            continue;
                        };
                        for tool in detail.tools {
                            execs.push(Box::new(apollia_mcp::executor::McpToolExecutor::new(
                                handle.clone(),
                                status.name.clone(),
                                tool.local_name.clone(),
                            )));
                        }
                    }
                    execs
                } else {
                    Vec::new()
                };

            // Append connector executors (Google Workspace today; Microsoft
            // wires the same way once its executor module lands). When the
            // AuthManager hasn't initialised yet (no OAuth flow run, fresh
            // install), `build_google_executors` returns an empty Vec and
            // tool calls surface the "no Google account connected" error
            // from the executors rather than `UnknownTool` — better UX.
            let google_executors = crate::connectors_bridge::build_google_executors().await;
            extra_executors.extend(google_executors);

            let dispatcher = Arc::new(build_dispatcher_with(
                &NativeDispatcherConfig {
                    sandbox_root: sandbox_root_for_agent(),
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
                    governance_db_path: Some(
                        governance_base.join(apollia_tools::GOVERNANCE_DB_FILENAME),
                    ),
                },
                extra_executors,
            ));

            let tool_proxy = match (tool_registry.as_ref(), audit_trail.as_ref()) {
                (Some(registry), Some(audit)) => {
                    let proxy = ToolProxy::new(
                        registry.clone(),
                        audit.clone(),
                        Arc::new(DispatcherExecutor::new(dispatcher)),
                        allowed_tools,
                        agent_id.clone(),
                        task.task_id.clone(),
                    )
                    // ADR-088 — instrumentation tool_call_* sur l'EventBus.
                    .with_event_bus(event_bus.clone());
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
                        "ToolProxy not available — tool registry or audit trail missing"
                    );
                    None
                }
            };

            let memory_interface = memory_namespace.as_deref().and_then(|ns| {
                let eff_ns = effective_memory_namespace(ns, task.project_id.as_deref());
                let manager = MemoryManager::new(&memory_base_dir, Some(eff_ns.clone()), vec![]);
                let iface = MemoryInterface::new(manager, eff_ns, agent_id.clone())?;
                iface.announce_shared_namespaces(&event_bus);
                Some(iface)
            });

            // Chat mode wiring: when the task carries a message_id, the
            // runtime is invoked from the chat session manager — configure
            // the context so `ctx.emit_token()` routes tokens back to the
            // right SSE session.
            let chat_target = task
                .message_id
                .clone()
                .map(|mid| (task.context_id.clone(), mid));

            // ADR-087 — Build a dedicated MemoryManager for the __user__
            // namespace so we can expose ctx.profile alongside ctx.memory.
            let profile_interface = {
                let user_manager =
                    MemoryManager::new(&memory_base_dir, Some("__user__".to_string()), vec![]);
                let is_onboarding = agent_id == "onboarding-agent";
                Some(apollia_aip::profile::ProfileInterface::new(
                    user_manager,
                    agent_id.clone(),
                    user_memory_write,
                    is_onboarding,
                ))
            };

            let ctx: PyObject = Python::with_gil(|py| {
                let mut ctx = RuntimeContext::new_with_llm(
                    llm_router,
                    Arc::new(StepBudgetView::unlimited()),
                    tool_helper,
                    Arc::new(ObservabilityConfig::default()),
                    event_bus,
                    agent_id.clone().into(),
                    tool_proxy,
                    memory_interface,
                    mailbox,
                    agent_id,
                    supports_a2a,
                    user_context,
                    a2a_delegate,
                    a2a_invoker,
                    user_memory_write,
                );
                if let Some(profile) = profile_interface {
                    ctx = ctx.with_profile(profile);
                }
                if let Some((session_id, message_id)) = chat_target {
                    ctx = ctx.with_chat_target(session_id, message_id);
                }
                // ADR-103 (LOT 5) — datasources YAML + templates Jinja2.
                ctx = ctx
                    .with_datasources(datasources_declared, agent_dir.as_deref())
                    .with_templates(templates_declared, agent_dir.as_deref());
                // ADR-104 (LOT 6) — ctx.secrets read-only gated par le manifest.
                // Le store est ouvert ici (et non capturé dans le BridgeRunner)
                // pour éviter de tenir un Mutex à travers les awaits et pour
                // ramasser proprement les changements de secrets effectués
                // pendant la session.
                let secret_store = open_secret_store(&default_data_dir());
                ctx = ctx.with_secrets(apollia_aip::secrets::SecretsInterface::new(
                    secret_store,
                    secrets_declared,
                ));
                // ADR-088 — relier le contexte à la task pour que ctx.log()
                // étiquette correctement les RuntimeEvent::AgentLog persistés.
                ctx = ctx.with_task_id(task.task_id.clone());
                Py::new(py, ctx)
                    .map(|p| p.into_any())
                    .expect("RuntimeContext PyObject construction failed")
            });

            bridge.call_run(&task, ctx).await.map_err(|e| e.to_string())
        })
    }
}

impl ExecutionBackend for AIPProductionBackend {
    fn execute(
        &self,
        task: AIPTask,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<AIPResult, String>> + Send>> {
        let runner = BridgeRunner {
            bridge: Arc::clone(&self.bridge),
            llm_router: self.llm_router.clone(),
            event_bus: self.event_bus.clone(),
            agent_id: self.agent_id.clone(),
            allowed_tools: self.allowed_tools.clone(),
            tool_registry: self.tool_registry.clone(),
            audit_trail: self.audit_trail.clone(),
            memory_namespace: self.memory_namespace.clone(),
            memory_base_dir: self.memory_base_dir.clone(),
            user_memory_write: self.user_memory_write,
            // Task-mode backend: no chat UI to answer `ask_user` prompts.
            pending_user_inputs: None,
            mcp_handle: self.mcp_handle.clone(),
            supports_a2a: self.supports_a2a,
            a2a_delegate: self.a2a_delegate.clone(),
            a2a_invoker: self.a2a_invoker.clone(),
            mailbox: self.mailbox.clone(),
            // Task mode = trigger-fired/manual/API tasks. No user is "present"
            // in a chat the way Chat Libre has one, so no profile injection
            // here. Chat Agent mode populates this through ChatAgentRunner.
            user_context: None,
            tools_config: self.tools_config.clone(),
            datasources_declared: self.datasources_declared.clone(),
            templates_declared: self.templates_declared.clone(),
            agent_dir: self.agent_dir.clone(),
            secrets_declared: self.secrets_declared.clone(),
        };

        let mut engine = ORIAEngine::new().with_event_bus(self.event_bus.clone());
        if let Some(pending) = self.pending_approvals.clone() {
            engine = engine.with_pending_approvals(pending);
        }
        if let Some(repo) = self.task_repository.clone() {
            engine = engine.with_task_repository(repo);
        }

        let budget = Arc::new(StepBudget::unlimited());
        Box::pin(async move {
            engine
                .execute_direct(task, &runner, budget)
                .await
                .map_err(|e| e.to_string())
        })
    }
}

// ─── Factory ──────────────────────────────────────────────────────────────────

/// Creates a real `AIPProductionBackend` per agent at `agent start` time.
///
/// OnceLocks are populated by `main()` immediately after `init_embedded()` returns,
/// before any HTTP request can arrive.
pub struct ProductionBackendFactory {
    pub event_bus: Arc<std::sync::OnceLock<EventBusSender>>,
    pub llm_router: Arc<std::sync::RwLock<Option<Arc<LlmRouter>>>>,
    pub tool_registry: Arc<std::sync::OnceLock<ToolRegistryHandle>>,
    pub audit_trail: Arc<std::sync::OnceLock<AuditTrailHandle>>,
    pub pending_approvals: Arc<std::sync::OnceLock<Arc<PendingApprovals>>>,
    pub task_repository: Arc<std::sync::OnceLock<Arc<TaskRepository>>>,
    pub mcp_handle: Arc<std::sync::OnceLock<apollia_mcp::manager::McpClientManagerHandle>>,
    /// Agent registry handle — required to build the A2A invoker so
    /// trigger-fired agents and manual fires can call `ctx.a2a_invoke(...)`
    /// on parity with Chat Libre.
    pub agent_registry: Arc<std::sync::OnceLock<AgentRegistryHandle>>,
    /// Task router handle — required to build the A2A delegate.
    pub task_router: Arc<std::sync::OnceLock<TaskRouterHandle<DynBackend>>>,
    /// Inter-agent mailbox handle — exposed as `ctx.mailbox`.
    pub mailbox_handle: Arc<std::sync::OnceLock<AgentMailboxHandle>>,
    /// Operator tools configuration (`[tools]` of apollia.toml).
    pub tools_config: Arc<std::sync::OnceLock<ToolsConfig>>,
}

impl AgentBackendFactory for ProductionBackendFactory {
    fn create_for_agent(&self, agent_path: &Path, manifest: &AgentManifest) -> DynBackend {
        let agent_id = manifest.name.clone();

        let event_bus = match self.event_bus.get() {
            Some(bus) => bus.clone(),
            None => {
                tracing::error!(
                    agent = %agent_id,
                    "event bus not initialized — factory called before supervisor started"
                );
                return DynBackend::new(NoopBackend);
            }
        };
        let llm_router = self
            .llm_router
            .read()
            .expect("llm_router_lock poisoned")
            .clone();
        let tool_registry = self.tool_registry.get().cloned();
        let audit_trail = self.audit_trail.get().cloned();
        let pending_approvals = self.pending_approvals.get().cloned();
        let task_repository = self.task_repository.get().cloned();
        let mcp_handle = self.mcp_handle.get().cloned();
        let mailbox = self.mailbox_handle.get().cloned();
        let tools_config = self
            .tools_config
            .get()
            .cloned()
            .unwrap_or_else(ToolsConfig::default);

        // Build A2A delegate + invoker when registry+router are available.
        // Same shape as `apollia-cli/src/commands/start.rs:716-742`.
        let (a2a_delegate, a2a_invoker) = match (
            self.agent_registry.get().cloned(),
            self.task_router.get().cloned(),
        ) {
            (Some(registry), Some(router)) => {
                let delegate = make_delegate_fn(
                    registry.clone(),
                    router.clone(),
                    event_bus.clone(),
                    DEFAULT_A2A_MAX_HOPS,
                );
                let invoker = Arc::new(A2AInvoker::new(
                    registry,
                    router,
                    event_bus.clone(),
                    apollia_core::A2AConfig::default(),
                ));
                (Some(delegate), Some(invoker))
            }
            _ => {
                tracing::warn!(
                    agent = %agent_id,
                    "A2A delegate/invoker not available — registry or router not yet initialized"
                );
                (None, None)
            }
        };

        let result: Result<AIPProductionBackend, String> = (|| {
            // Inject the per-agent venv site-packages into sys.path so top-level
            // imports of pip-installed packages resolve correctly at agent start.
            let extras = venv_site_packages_for_agent_name(&manifest.name);
            let module = apollia_aip::loader::load_agent_module_with_sys_paths(agent_path, &extras)
                .map_err(|e| e.to_string())?;
            let validated =
                apollia_aip::validator::validate_agent(&module).map_err(|e| e.to_string())?;
            let allowed_tools = validated.manifest.tools_required.clone();
            let memory_namespace = validated.manifest.memory_namespace.clone();
            let user_memory_write = validated.manifest.user_memory_write;
            let supports_a2a = validated.manifest.supports_a2a;
            // ADR-103 (LOT 5) — capture les déclarations + le dossier de
            // l'agent pour brancher ctx.datasources / ctx.templates.
            let datasources_declared = validated.manifest.datasources.clone();
            let templates_declared = validated.manifest.templates.clone();
            let agent_dir = agent_path.parent().map(Path::to_path_buf);
            // ADR-104 (LOT 6) — capture la liste secrets déclarés.
            let secrets_declared = validated.manifest.secrets.clone();
            let bridge = Arc::new(AIPBridge::new(validated).map_err(|e| e.to_string())?);
            Ok(AIPProductionBackend {
                bridge,
                agent_id: agent_id.clone(),
                allowed_tools,
                llm_router,
                event_bus,
                tool_registry,
                audit_trail,
                memory_namespace,
                memory_base_dir: default_memory_dir(),
                pending_approvals,
                task_repository,
                user_memory_write,
                mcp_handle,
                supports_a2a,
                a2a_delegate,
                a2a_invoker,
                mailbox,
                tools_config,
                datasources_declared,
                templates_declared,
                agent_dir,
                secrets_declared,
            })
        })();

        match result {
            Ok(backend) => DynBackend::new(backend),
            Err(e) => {
                tracing::error!(
                    agent = %agent_id,
                    path = %agent_path.display(),
                    error = %e,
                    "failed to load agent Python module — falling back to NoopBackend"
                );
                DynBackend::new(NoopBackend)
            }
        }
    }
}

fn default_memory_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    PathBuf::from(home).join(".apollia").join("memory")
}

/// Base de données apollia (`~/.apollia/`) — utilisée pour ouvrir le
/// [`ToolCredentialStore`] partagé pour `ctx.secrets` (ADR-104, LOT 6).
fn default_data_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    PathBuf::from(home).join(".apollia")
}

/// Ouvre le [`ToolCredentialStore`] partagé pour `ctx.secrets` (ADR-104, LOT 6).
///
/// Retourne `None` si la base de gouvernance n'existe pas encore ou si
/// l'ouverture échoue — l'agent obtient alors `None` à chaque
/// `ctx.secrets.get(key)` (sémantique non-fatale, ADR-104).
fn open_secret_store(data_dir: &Path) -> Option<Arc<std::sync::Mutex<ToolCredentialStore>>> {
    let db_path = data_dir.join(apollia_tools::GOVERNANCE_DB_FILENAME);
    if !db_path.exists() {
        return None;
    }
    let keyfile_path = data_dir.join(".keyfile");
    match ToolCredentialStore::new(&db_path, &keyfile_path) {
        Ok(store) => Some(Arc::new(std::sync::Mutex::new(store))),
        Err(e) => {
            tracing::warn!(
                target: "apollia.aip.secrets",
                error = %e,
                "failed to open ToolCredentialStore for ctx.secrets — agent will see None for all keys"
            );
            None
        }
    }
}

// ─── Chat Agent Runner ───────────────────────────────────────────────────────

/// Production [`ChatAgentRunner`] for the desktop app.
///
/// Resolves the agent name to an `install_path` via [`AgentRepository`],
/// loads the Python module, creates a full `RuntimeContext`, and calls
/// `AIPBridge.call_run`. Same execution path as `AIPProductionBackend`
/// but parameterized by agent name instead of pre-loaded bridge.
pub struct ProductionChatAgentRunner {
    pub agent_repo: Arc<std::sync::Mutex<apollia_tools::AgentRepository>>,
    pub event_bus: Arc<std::sync::OnceLock<EventBusSender>>,
    pub llm_router: Arc<std::sync::RwLock<Option<Arc<LlmRouter>>>>,
    pub tool_registry: Arc<std::sync::OnceLock<ToolRegistryHandle>>,
    pub audit_trail: Arc<std::sync::OnceLock<AuditTrailHandle>>,
    pub pending_approvals: Arc<std::sync::OnceLock<Arc<PendingApprovals>>>,
    pub task_repository: Arc<std::sync::OnceLock<Arc<TaskRepository>>>,
    /// Chat manager's `ask_user` pending-input registry — populated after
    /// `init_embedded()` returns so the native tool dispatcher can route
    /// `ask_user` invocations through the chat HITL loop.
    pub pending_user_inputs: Arc<std::sync::OnceLock<PendingUserInputs>>,
    /// MCP client manager handle, populated after init_embedded() so the
    /// BridgeRunner can inject McpToolExecutor instances into the
    /// dispatcher at run time.
    pub mcp_handle: Arc<std::sync::OnceLock<apollia_mcp::manager::McpClientManagerHandle>>,
    /// Agent registry handle — required to build the A2A invoker so chat-agent
    /// Python agents can call `ctx.a2a_invoke(...)` like task-mode agents.
    pub agent_registry: Arc<std::sync::OnceLock<AgentRegistryHandle>>,
    /// Task router handle — required to build the A2A delegate.
    pub task_router: Arc<std::sync::OnceLock<TaskRouterHandle<DynBackend>>>,
    /// Inter-agent mailbox handle — exposed as `ctx.mailbox`.
    pub mailbox_handle: Arc<std::sync::OnceLock<AgentMailboxHandle>>,
    /// Global user memory repository — used to build `ctx.user_context`.
    pub user_memory: Arc<std::sync::OnceLock<Arc<std::sync::Mutex<UserMemoryRepository>>>>,
    /// Operator tools configuration (`[tools]` apollia.toml).
    pub tools_config: Arc<std::sync::OnceLock<ToolsConfig>>,
}

#[async_trait::async_trait]
impl apollia_runtime::chat::ChatAgentRunner for ProductionChatAgentRunner {
    async fn run_agent(
        &self,
        agent_name: &str,
        task: apollia_core::AIPTask,
    ) -> Result<apollia_core::AIPResult, String> {
        // 1. Resolve agent name → install_path
        let install_path = {
            let repo = self
                .agent_repo
                .lock()
                .map_err(|e| format!("agent repo mutex poisoned: {e}"))?;
            let agent = repo
                .get(agent_name)
                .map_err(|e| format!("agent repo error: {e}"))?
                .ok_or_else(|| format!("agent not found in repository: {agent_name}"))?;
            agent.install_path.clone()
        };

        // 2. Load Python module + create bridge.
        // Inject the per-agent venv site-packages so top-level imports of
        // pip-installed packages resolve correctly.
        let extras = venv_site_packages_for_agent_name(agent_name);
        let module = apollia_aip::loader::load_agent_module_with_sys_paths(&install_path, &extras)
            .map_err(|e| e.to_string())?;
        let validated =
            apollia_aip::validator::validate_agent(&module).map_err(|e| e.to_string())?;
        // Merge required + optional tools: the manifest contract says agents
        // may call any tool from either list. Excluding optional tools would
        // hide `ask_user` and similar HITL helpers from Python agents.
        let allowed_tools: Vec<String> = validated
            .manifest
            .tools_required
            .iter()
            .chain(validated.manifest.tools_optional.iter())
            .cloned()
            .collect();
        let memory_namespace = validated.manifest.memory_namespace.clone();
        let user_memory_write = validated.manifest.user_memory_write;
        let supports_a2a = validated.manifest.supports_a2a;
        // ADR-103 (LOT 5) — capture datasources/templates + dossier de l'agent.
        let datasources_declared = validated.manifest.datasources.clone();
        let templates_declared = validated.manifest.templates.clone();
        let agent_dir = install_path.parent().map(Path::to_path_buf);
        // ADR-104 (LOT 6) — capture la liste des secrets déclarés.
        let secrets_declared = validated.manifest.secrets.clone();
        let bridge = Arc::new(AIPBridge::new(validated).map_err(|e| e.to_string())?);

        // 3. Resolve OnceLock handles
        let event_bus = self
            .event_bus
            .get()
            .cloned()
            .ok_or("event bus not initialized")?;
        let llm_router = self
            .llm_router
            .read()
            .expect("llm_router_lock poisoned")
            .clone();
        let tool_registry = self.tool_registry.get().cloned();
        let audit_trail = self.audit_trail.get().cloned();
        let mailbox = self.mailbox_handle.get().cloned();
        let tools_config = self
            .tools_config
            .get()
            .cloned()
            .unwrap_or_else(ToolsConfig::default);

        // Build A2A delegate + invoker when registry+router are available —
        // gives chat-agent Python agents the same A2A capabilities as task-mode
        // and Chat Libre.
        let (a2a_delegate, a2a_invoker) = match (
            self.agent_registry.get().cloned(),
            self.task_router.get().cloned(),
        ) {
            (Some(registry), Some(router)) => {
                let delegate = make_delegate_fn(
                    registry.clone(),
                    router.clone(),
                    event_bus.clone(),
                    DEFAULT_A2A_MAX_HOPS,
                );
                let invoker = Arc::new(A2AInvoker::new(
                    registry,
                    router,
                    event_bus.clone(),
                    apollia_core::A2AConfig::default(),
                ));
                (Some(delegate), Some(invoker))
            }
            _ => {
                tracing::warn!(
                    agent = %agent_name,
                    "A2A delegate/invoker not available for chat-agent runner — registry or router not yet initialized"
                );
                (None, None)
            }
        };

        // Build user_context from the global user memory store (chat mode only).
        let user_context = self.user_memory.get().and_then(|repo_mutex| {
            let repo = repo_mutex.lock().ok()?;
            build_user_context_from_repo(&repo)
        });

        // 4. Build RuntimeContext and call the agent
        let runner = BridgeRunner {
            bridge,
            llm_router,
            event_bus: event_bus.clone(),
            agent_id: agent_name.to_string(),
            allowed_tools,
            tool_registry,
            audit_trail,
            memory_namespace,
            memory_base_dir: default_memory_dir(),
            user_memory_write,
            pending_user_inputs: self.pending_user_inputs.get().cloned(),
            mcp_handle: self.mcp_handle.get().cloned(),
            supports_a2a,
            a2a_delegate,
            a2a_invoker,
            mailbox,
            user_context,
            tools_config,
            datasources_declared,
            templates_declared,
            agent_dir,
            secrets_declared,
        };

        let mut engine = ORIAEngine::new().with_event_bus(event_bus);
        if let Some(pending) = self.pending_approvals.get().cloned() {
            engine = engine.with_pending_approvals(pending);
        }
        if let Some(repo) = self.task_repository.get().cloned() {
            engine = engine.with_task_repository(repo);
        }

        let budget = Arc::new(StepBudget::unlimited());
        engine
            .execute_direct(task, &runner, budget)
            .await
            .map_err(|e| e.to_string())
    }
}

/// Merges `[tools].disabled` from apollia.toml with the runtime-disabled list
/// from `governance.db`. Either source disables a tool; duplicates removed.
fn merge_disabled(static_disabled: &[String], mut runtime_disabled: Vec<String>) -> Vec<String> {
    for name in static_disabled {
        if !runtime_disabled.iter().any(|n| n == name) {
            runtime_disabled.push(name.clone());
        }
    }
    runtime_disabled
}

/// Flattens a [`UserMemoryRepository`] into the `{"profile": [(k, v), ...]}`
/// shape consumed by `RuntimeContext.user_context`. Returns `None` for an empty
/// repo so chat-mode agents see no profile rather than an empty object.
fn build_user_context_from_repo(
    repo: &UserMemoryRepository,
) -> Option<std::collections::HashMap<String, Vec<(String, String)>>> {
    let entries = repo.list_all().unwrap_or_default();
    if entries.is_empty() {
        return None;
    }
    let mut map = std::collections::HashMap::new();
    map.insert(
        "profile".to_string(),
        entries.into_iter().map(|e| (e.key, e.value)).collect(),
    );
    Some(map)
}
