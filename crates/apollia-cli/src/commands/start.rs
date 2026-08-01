//! `apollia-os start`: start the runtime in foreground.
//!
//! Uses the Supervisor for ordered startup (EventBus, AgentRegistry, TaskRouter,
//! APIServer) with timeout and rollback on failure. Shutdown is handled by the
//! ShutdownController with graceful drain.

use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use std::time::Instant;

use apollia_aip::bridge::AIPBridge;
use apollia_aip::context::{
    effective_memory_namespace, DispatcherExecutor, RuntimeContext, ToolProxy, ToolProxyConfig,
};
use apollia_aip::memory::MemoryInterface;
use apollia_core::{
    AIPResult, AIPTask, AgentManifest, PendingApprovals, RuntimeEvent, StepBudgetConfig, TaskStatus,
};
use apollia_llm::{
    CompletionModel, CompletionRequest, CompletionResponse, LlmError, LlmRouter,
    ObservabilityConfig, StepBudgetView, ToolCallHelper, ToolInvoker,
};
use apollia_memory::manager::MemoryManager;
use apollia_oria::budget::StepBudget;
use apollia_oria::engine::{AgentRunner, ORIAEngine};
use apollia_runtime::api::routes_agents::{AgentBackendFactory, AgentLoader};
use apollia_runtime::api::APIServerConfig;
use apollia_runtime::coordinator::{DynBackend, ExecutionBackend, ExecutionCoordinator};
use apollia_runtime::eventbus::EventBusSender;
use apollia_runtime::registry::AgentRegistryHandle;
use apollia_runtime::router::TaskRouterHandle;
use apollia_runtime::shutdown::{ShutdownConfig, ShutdownController, ShutdownControllerDeps};
use apollia_runtime::supervisor::{Supervisor, SupervisorConfig};
use apollia_runtime::A2AToolsProvider;
use apollia_tools::tools::ask_user::PendingUserInputs;
use apollia_tools::{
    build_dispatcher_with, load_governance_snapshot, AuditTrailHandle, NativeDispatcherConfig,
    TaskRepository, ToolCredentialStore, ToolRegistryHandle,
};
use futures::stream;
use pyo3::prelude::*;

use crate::client::{DEFAULT_SOCKET_PATH, DEFAULT_TCP_PORT};

/// Errors that can occur during runtime startup.
#[derive(Debug, thiserror::Error)]
pub enum StartError {
    /// Supervisor failed to start actors.
    #[error("failed to start runtime: {0}")]
    Supervisor(#[from] apollia_runtime::supervisor::SupervisorError),
    /// Config file found but invalid.
    #[error("invalid config file {path}: {reason}")]
    Config {
        path: std::path::PathBuf,
        reason: String,
    },
    /// A runtime is already listening on the requested port or socket.
    #[error("runtime already running on {address} - use `apollia-os stop` first")]
    AlreadyRunning { address: String },
    /// API token could not be loaded or generated while `require_token = true`.
    #[error("failed to load or generate API token: {0}")]
    ApiToken(#[from] apollia_runtime::api::TokenFileError),
}

/// Real agent loader using AIPLoader + validate_agent.
///
/// Loads a Python module via PyO3, validates AIP duck typing, and returns
/// the deserialized [`AgentManifest`].
struct AIPAgentLoader;

impl AgentLoader for AIPAgentLoader {
    fn load_and_validate(&self, path: &Path) -> Result<AgentManifest, String> {
        // Re-use the same sys.path assembly as `apollia-os agent
        // install / validate` so the runtime's POST /api/v1/agents path
        // resolves the SDK + the enclosing package root the same way
        // (otherwise installed agents that validated fine through the CLI
        // would 400 the moment the runtime tries to load them).
        let extras = crate::community::validation_sys_paths(path);
        let module = apollia_aip::loader::load_agent_module_with_sys_paths(path, &extras)
            .map_err(|e| e.to_string())?;
        let validated =
            apollia_aip::validator::validate_agent(&module).map_err(|e| e.to_string())?;
        Ok(validated.manifest)
    }
}

/// Open the shared [`ToolCredentialStore`] used for `ctx.secrets`.
///
/// Returns `None` when the governance database does not exist yet (first
/// start before `apollia-os tools secret set ...`) or when opening it fails.
/// The agent then gets `None` for every `ctx.secrets.get(key)`, consistent
/// with the non-fatal secrets semantics.
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
                "failed to open ToolCredentialStore for ctx.secrets - agent will see None for all keys"
            );
            None
        }
    }
}

// ─────────────────────────────────────────────────────────────
// AIPChatAgentRunner: concrete ChatAgentRunner for Chat Agent mode.
// ─────────────────────────────────────────────────────────────

/// Concrete [`ChatAgentRunner`] implementation using PyO3 + AIPBridge.
///
/// Loads the Python agent from `data_dir/agents/<name>/`, validates AIP duck
/// typing, builds a `RuntimeContext` with tools/memory/LLM, and calls `run()`.
///
/// Uses the same `OnceLock` pattern as [`ProductionBackendFactory`] to access
/// runtime handles created inside `supervisor.start()`.
struct AIPChatAgentRunner {
    /// EventBus sender, populated after supervisor.start().
    event_bus: Arc<std::sync::OnceLock<EventBusSender>>,
    /// LLM router, populated after supervisor.start().
    llm_router: Arc<std::sync::OnceLock<Option<Arc<LlmRouter>>>>,
    /// Tool registry, populated after supervisor.start().
    tool_registry: Arc<std::sync::OnceLock<ToolRegistryHandle>>,
    /// Audit trail, populated after supervisor.start().
    audit_trail: Arc<std::sync::OnceLock<AuditTrailHandle>>,
    /// Global user memory repository, populated after supervisor.start().
    user_memory: Arc<
        std::sync::OnceLock<
            Option<Arc<std::sync::Mutex<apollia_memory::user_memory::UserMemoryRepository>>>,
        >,
    >,
    /// Chat manager's `ask_user` pending-input registry, populated after
    /// `supervisor.start()` so the native dispatcher can wire
    /// `AskUserExecutor` to the chat HITL loop.
    pending_user_inputs: Arc<std::sync::OnceLock<PendingUserInputs>>,
    /// Agent registry, required to build the A2A delegate + invoker so chat-agent
    /// Python agents get the same A2A capabilities as task-mode agents.
    agent_registry: Arc<std::sync::OnceLock<apollia_runtime::registry::AgentRegistryHandle>>,
    /// Task router, required to build the A2A delegate.
    task_router: Arc<
        std::sync::OnceLock<
            apollia_runtime::router::TaskRouterHandle<apollia_runtime::coordinator::DynBackend>,
        >,
    >,
    /// Base data directory (e.g. `~/.apollia/`).
    data_dir: PathBuf,
    /// Operator-supplied tools configuration loaded from `apollia.toml`.
    /// Drives `disabled` tools and `web_search` / `web_read` parameters.
    tools_config: apollia_core::ToolsConfig,
    /// MCP client manager handle, populated after `supervisor.start()` so the
    /// chat-agent dispatcher can route `mcp:<server>/<tool>` invocations.
    mcp_handle: Arc<std::sync::OnceLock<Option<apollia_mcp::manager::McpClientManagerHandle>>>,
}

#[async_trait::async_trait]
impl apollia_runtime::chat::ChatAgentRunner for AIPChatAgentRunner {
    async fn run_agent(&self, agent_name: &str, task: AIPTask) -> Result<AIPResult, String> {
        let agent_path = self.data_dir.join("agents").join(agent_name);

        // Load and validate agent via PyO3. Re-use the shared community helper
        // so the chat-agent runner sees the same sys.path as `agent install`
        // (per-agent venv + enclosing package root + workspace SDK fallback).
        let extras = crate::community::validation_sys_paths(&agent_path);
        let module = apollia_aip::loader::load_agent_module_with_sys_paths(&agent_path, &extras)
            .map_err(|e| e.to_string())?;
        let validated =
            apollia_aip::validator::validate_agent(&module).map_err(|e| e.to_string())?;
        let manifest = validated.manifest.clone();
        let bridge = Arc::new(AIPBridge::new(validated).map_err(|e| e.to_string())?);

        // Get runtime handles from OnceLocks
        let event_bus = self
            .event_bus
            .get()
            .cloned()
            .ok_or("event bus not initialized - chat agent called before runtime ready")?;
        let llm_router = self.llm_router.get().cloned().flatten();
        let tool_registry = self.tool_registry.get().cloned();
        let audit_trail = self.audit_trail.get().cloned();

        // Build RuntimeContext components
        let router_for_helper = llm_router
            .clone()
            .unwrap_or_else(|| Arc::new(LlmRouter::empty()));
        let tool_helper = Arc::new(ToolCallHelper::new(
            Arc::new(RouterModel(router_for_helper)),
            Arc::new(NoopToolInvoker),
        ));

        let allowed_tools: Vec<String> = manifest
            .tools_required
            .iter()
            .chain(manifest.tools_optional.iter())
            .cloned()
            .collect();

        let memory_base_dir = self.data_dir.join("memory");
        let snapshot = load_governance_snapshot(&self.data_dir).unwrap_or_else(|e| {
            tracing::warn!(error = %e, "governance snapshot unavailable - defaulting to all tools enabled");
            Default::default()
        });
        let disabled_tools = merge_disabled(&self.tools_config.disabled, snapshot.disabled_tools);
        // Inject one MCP executor per registered tool so `ctx.tools.call("mcp:...")`
        // routes through the MCP client manager instead of returning UnknownTool.
        let mcp_handle = self.mcp_handle.get().cloned().flatten();
        let extra_executors = mcp_executors_for(&mcp_handle).await;
        let dispatcher = Arc::new(build_dispatcher_with(
            &NativeDispatcherConfig {
                sandbox_root: sandbox_root_for_agent(),
                agent_id: agent_name.to_string(),
                venv_base_dir: self.data_dir.join("venvs"),
                memory_namespace: manifest.memory_namespace.clone(),
                memory_shared_namespaces: Vec::new(),
                memory_base_dir: memory_base_dir.clone(),
                http_allowlist: None,
                pending_user_inputs: self.pending_user_inputs.get().cloned(),
                disabled_tools,
                brave_api_key: snapshot.brave_api_key,
                web_search_config: self.tools_config.web_search.clone(),
                web_read_config: self.tools_config.web_read.clone(),
                governance_db_path: Some(self.data_dir.join(apollia_tools::GOVERNANCE_DB_FILENAME)),
            },
            extra_executors,
        ));

        // Build A2A delegate + invoker so chat-agent Python agents can call
        // `ctx.a2a_invoke(...)` and `ctx.tools.invoke("a2a:<skill>")` on parity
        // with task-mode (triggers/API). Fixes the previous "not available in
        // chat mode" gap.
        let a2a_invoker = match (
            self.agent_registry.get().cloned(),
            self.task_router.get().cloned(),
        ) {
            (Some(registry), Some(router)) => {
                let invoker = Arc::new(apollia_runtime::a2a::A2AInvoker::new(
                    registry,
                    router,
                    event_bus.clone(),
                    apollia_core::A2AConfig::default(),
                ));
                Some(invoker)
            }
            _ => {
                tracing::warn!(
                    agent = %agent_name,
                    "A2A invoker not available for chat-agent runner - registry or router not yet initialized"
                );
                None
            }
        };

        // Augment allowed_tools with live A2A virtual skills so the ToolProxy
        // accepts `a2a:*` calls (the registry filter would otherwise reject them).
        let mut allowed_tools = allowed_tools;
        if let Some(ref invoker) = a2a_invoker {
            let descriptors = apollia_runtime::a2a::A2AToolsProvider::new(Arc::clone(invoker))
                .build_tool_descriptors()
                .await;
            for desc in descriptors {
                if !allowed_tools.iter().any(|n| n == &desc.name) {
                    allowed_tools.push(desc.name);
                }
            }
        }

        let tool_proxy: Option<ToolProxy> = match (tool_registry.as_ref(), audit_trail.as_ref()) {
            (Some(registry), Some(audit)) => {
                let proxy = ToolProxy::new(ToolProxyConfig {
                    registry: registry.clone(),
                    audit: audit.clone(),
                    executor: Arc::new(DispatcherExecutor::new(dispatcher)),
                    allowed_tools,
                    agent_id: agent_name.to_string(),
                    task_id: task.task_id.clone(),
                    run_id: task.run_id.clone(),
                })
                // tool_call_* instrumentation.
                .with_event_bus(event_bus.clone());
                let proxy = if let Some(ref invoker) = a2a_invoker {
                    proxy.with_a2a(Arc::clone(invoker), 0, None)
                } else {
                    proxy
                };
                Some(proxy)
            }
            _ => None,
        };
        let memory_interface: Option<MemoryInterface> =
            manifest.memory_namespace.as_deref().and_then(|ns| {
                let eff_ns = effective_memory_namespace(ns, task.project_id.as_deref());
                let manager = MemoryManager::new(&memory_base_dir, Some(eff_ns.clone()), vec![]);
                // Always attach the global __user__ store so every agent can
                let iface = MemoryInterface::new(manager, eff_ns, agent_name.to_string())?;
                iface.announce_shared_namespaces(&event_bus);
                Some(iface)
            });

        // Build user_context from UserMemoryRepository (chat mode only).
        let user_context = self
            .user_memory
            .get()
            .and_then(|opt| opt.as_ref())
            .and_then(|repo_mutex| {
                let repo = repo_mutex.lock().ok()?;
                build_user_context_from_repo(&repo)
            });

        let profile_interface = {
            let user_memory_db = self.data_dir.join("user_memory.db");
            apollia_aip::profile::ProfileInterface::new(
                user_memory_db,
                agent_name.to_string(),
                manifest.user_memory_write,
                agent_name == "onboarding-agent",
            )
        };

        // Directory containing the agent .py, used to resolve datasources/
        // and templates/ files relative to the agent.
        let agent_dir = agent_path.parent().map(Path::to_path_buf);
        let datasources_declared = manifest.datasources.clone();
        let templates_declared = manifest.templates.clone();
        // Secrets allowlist + shared credential store.
        let secrets_declared = manifest.secrets.clone();
        let secret_store = open_secret_store(&self.data_dir);

        let ctx: PyObject = Python::with_gil(|py| {
            let ctx = RuntimeContext::new_with_llm(
                llm_router,
                Arc::new(StepBudgetView::unlimited()),
                tool_helper,
                Arc::new(ObservabilityConfig::default()),
                event_bus,
                agent_name.to_string().into(),
                tool_proxy,
                memory_interface,
                None, // mailbox: not available in chat runner context
                agent_name.to_string(),
                user_context,
                a2a_invoker,
                manifest.user_memory_write, // user_memory_writable: manifest-controlled
            )
            .with_profile(profile_interface)
            // Datasources YAML + templates Jinja2.
            .with_datasources(datasources_declared, agent_dir.as_deref())
            .with_templates(templates_declared, agent_dir.as_deref())
            // ctx.secrets read-only, gated by the manifest.
            .with_secrets(apollia_aip::secrets::SecretsInterface::new(
                secret_store,
                secrets_declared,
            ))
            // Bind the context to the task so ctx.log() labels the persisted
            // RuntimeEvent::AgentLog entries.
            .with_task_id(task.task_id.clone())
            .with_run_id(task.run_id.clone())
            .with_mailbox_capability(
                manifest.supports_mailbox,
                manifest.mailbox_allowlist.clone(),
                manifest
                    .tools_requiring_approval
                    .iter()
                    .any(|t| t == "mailbox:send"),
            );
            Py::new(py, ctx)
                .map(|p| p.into_any())
                .expect("RuntimeContext PyObject construction failed")
        });

        bridge.call_run(&task, ctx).await.map_err(|e| e.to_string())
    }
}

/// Builds the `user_context` dict from a [`UserMemoryRepository`].
///
/// Returns a `HashMap` with a single `"profile"` key mapping to the flat
/// list of `(key, value)` pairs from [`UserMemoryRepository::list_all`].
/// Returns `None` if the profile is empty.
fn build_user_context_from_repo(
    repo: &apollia_memory::user_memory::UserMemoryRepository,
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

/// Build MCP tool executors for an agent dispatcher, or an empty `Vec` when no
/// MCP handle is wired.
///
/// Delegates to the canonical `apollia_mcp` assembly so the CLI standalone-agent
/// path stays in lockstep with the chat and desktop dispatchers. Without this,
/// the registry surfaces `mcp:<server>/<tool>` to the agent but the dispatcher
/// returns `UnknownTool` at call time.
async fn mcp_executors_for(
    mcp_handle: &Option<apollia_mcp::manager::McpClientManagerHandle>,
) -> Vec<Box<dyn apollia_tools::executor::ToolExecutor>> {
    match mcp_handle {
        Some(handle) => apollia_mcp::executor::build_agent_tool_executors(handle).await,
        None => Vec::new(),
    }
}

/// Fallback backend, only used when agent loading fails at start time.
///
/// Returns a `Failed` result immediately without calling Python.
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
                error: Some(apollia_core::AIPError {
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

// ─────────────────────────────────────────────────────────────
// Stub LLM types required by ToolCallHelper constructor.
// RouterModel delegates to the real LlmRouter; NoopToolInvoker returns errors.
// These stubs are only invoked when an agent uses the LLM ReAct loop.
// ─────────────────────────────────────────────────────────────

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

/// Adapts an apollia-aip [`ToolProxy`] to ORIA's `ToolProxyTrait`.
///
/// Lets the orchestrated `ActorLoop` execute real, governed tools (permission
/// engine + audit trail + A2A routing + tool-call counting) instead of hitting
/// the engine's `NoopToolProxy` fallback. Tool output is normalised to a string
/// (JSON-serialised when not already a string) to match the trait contract.
struct OriaToolProxy {
    proxy: ToolProxy,
}

#[async_trait::async_trait]
impl apollia_oria::actor::ToolProxyTrait for OriaToolProxy {
    async fn invoke(&self, tool_name: &str, input: &serde_json::Value) -> Result<String, String> {
        match self.proxy.invoke_native(tool_name, input.clone()).await {
            Ok(serde_json::Value::String(s)) => Ok(s),
            Ok(other) => serde_json::to_string(&other).map_err(|e| e.to_string()),
            Err(e) => Err(e.to_string()),
        }
    }

    // `is_tool_read_only` keeps the trait default (false): orchestrated tool
    // steps run sequentially, never wrongly batched. Correct, if not maximally
    // parallel; ORIA-level read-only classification is a follow-up.

    async fn tool_schema(&self, tool_name: &str) -> Option<serde_json::Value> {
        self.proxy.tool_input_schema(tool_name).await
    }
}

// ─────────────────────────────────────────────────────────────
// Filesystem sandbox root for native tools (dev mode).
// `FileIo` and friends sandbox all paths under this root: we keep
// `$HOME` for parity with the previous embedded `NativeToolExecutor`
// so workspaces located anywhere under the user's home remain usable.
// ─────────────────────────────────────────────────────────────

/// Return the sandbox root used for file-oriented native tools.
///
/// Centralised so every runner in this crate points at the same root.
fn sandbox_root_for_agent() -> PathBuf {
    apollia_core::paths::home_dir_or_temp()
}

/// Union of statically-disabled tools (from `apollia.toml`) with the runtime
/// disabled set (from `governance.db`). Either source disables a tool: the
/// dispatcher only registers tools absent from both lists.
fn merge_disabled(static_disabled: &[String], mut runtime_disabled: Vec<String>) -> Vec<String> {
    for name in static_disabled {
        if !runtime_disabled.iter().any(|n| n == name) {
            runtime_disabled.push(name.clone());
        }
    }
    runtime_disabled
}

// Real per-agent execution backend (AIPBridge + RuntimeContext)
// ─────────────────────────────────────────────────────────────

/// Wires an [`ORIAEngine`] with the LLM router and Reasoner needed for
/// orchestrated execution. Extracted from
/// [`AIPProductionBackend::execute`] so the regression where orchestrated
/// agents fell through to `[NO_LLM]` (because the Reasoner was never plugged
/// in) has a unit-test guard.
///
/// Behaviour:
/// - `llm_router == None` → engine returned unchanged, warning logged.
///   Orchestrated execution will fail with `NO_LLM` at runtime.
/// - `llm_router == Some(_)` and `route_precise` succeeds → engine
///   gets both `with_llm_router` and `with_reasoner`. Orchestrated
///   execution can plan.
/// - `llm_router == Some(_)` but no `precise` backend resolved →
///   engine gets `with_llm_router` only (step LLM calls work), no
///   Reasoner. Orchestrated execution still fails with `NO_LLM` but
///   any LLM step within `execute_direct` keeps working. Warning
///   logged.
fn wire_engine_with_llm(
    mut engine: ORIAEngine,
    llm_router: Option<Arc<LlmRouter>>,
    agent_id: &str,
    max_steps: u32,
) -> ORIAEngine {
    let Some(router_arc) = llm_router else {
        tracing::warn!(
            agent = %agent_id,
            "no llm router configured - orchestrated execution will fail \
             with NO_LLM if invoked"
        );
        return engine;
    };
    let owned_router: LlmRouter = match Arc::try_unwrap(router_arc) {
        Ok(owned) => owned,
        Err(shared) => (*shared).clone(),
    };
    match owned_router.route_precise() {
        Ok(model) => {
            engine = engine
                .with_llm_router(owned_router)
                .with_reasoner(model, max_steps);
        }
        Err(err) => {
            tracing::warn!(
                agent = %agent_id,
                error = %err,
                "no precise LLM backend resolved - orchestrated \
                 execution will fail with NO_LLM if invoked"
            );
            engine = engine.with_llm_router(owned_router);
        }
    }
    engine
}

/// Builds the bounded [`StepBudget`] for the direct execution path.
///
/// Caps the agent-declared budget to the runtime ceiling
/// ([`StepBudgetConfig::default`]: 30 steps / 60 tool calls / 600s wall clock)
/// so `apollia-os run` never executes under an unlimited budget. This restores
/// principle #7 on the direct path, which previously used
/// `StepBudget::unlimited()` (u32::MAX steps + 24h). Extracted for unit testing.
fn direct_path_budget(agent_budget: &StepBudgetConfig) -> StepBudget {
    StepBudget::from_capped(agent_budget, &StepBudgetConfig::default())
}

/// Per-agent backend that calls Python via `AIPBridge`.
///
/// Created once per agent at start time by `ProductionBackendFactory`.
/// All fields are `Arc`-wrapped, so cloning is cheap.
struct AIPProductionBackend {
    bridge: Arc<AIPBridge>,
    agent_id: String,
    /// Manifest snapshot captured at factory time. Used to drive the
    /// `engine.execute_direct` vs `engine.execute_orchestrated_plan`
    /// routing decision in [`AIPProductionBackend::execute`].
    manifest: AgentManifest,
    allowed_tools: Vec<String>,
    llm_router: Option<Arc<LlmRouter>>,
    event_bus: EventBusSender,
    pending_approvals: Option<Arc<PendingApprovals>>,
    plan_gates: Option<Arc<apollia_oria::PendingPlanGates>>,
    /// Shared plan cache, the repository the supervisor opened at boot and
    /// exposes over REST. `None` leaves the engine re-planning every run.
    plan_cache: Option<Arc<std::sync::Mutex<apollia_oria::plan_cache::PlanCacheRepository>>>,
    task_repository: Option<Arc<TaskRepository>>,
    tool_registry: Option<ToolRegistryHandle>,
    audit_trail: Option<AuditTrailHandle>,
    /// Memory namespace declared in the manifest (e.g. "apollia-reviewer").
    memory_namespace: Option<String>,
    /// Root directory of the memory files (e.g. `~/.apollia/memory/`).
    memory_base_dir: PathBuf,
    /// Manifest opt-in for `ctx.memory.remember_user()` writes into `__user__`.
    user_memory_write: bool,
    /// High-level A2A orchestrator, `None` when registry or router are not initialized.
    a2a_invoker: Option<Arc<apollia_runtime::a2a::A2AInvoker>>,
    /// Operator-supplied tools configuration loaded from `apollia.toml`.
    tools_config: apollia_core::ToolsConfig,
    /// Datasources declared in the manifest (`manifest.datasources`). Empty
    /// when the agent declares none.
    datasources_declared: Vec<String>,
    /// Jinja2 templates declared in the manifest (`manifest.templates`). Empty
    /// when the agent declares none.
    templates_declared: Vec<String>,
    /// Root directory of the agent, used to resolve
    /// `datasources/<name>.yaml` and `templates/<name>.j2`.
    agent_dir: Option<PathBuf>,
    /// Secrets declared in the manifest (`manifest.secrets`). Strict allowlist
    /// for `ctx.secrets.get()`. Empty when the agent declares none.
    secrets_declared: Vec<String>,
    /// Apollia data directory (`~/.apollia/` by default), used to open the
    /// shared [`ToolCredentialStore`] that backs `ctx.secrets`. `None` is
    /// accepted in degraded mode.
    secrets_data_dir: Option<PathBuf>,
    /// MCP client manager handle, `None` when no MCP server is configured. Lets
    /// the `BridgeRunner` route `mcp:<server>/<tool>` calls to the MCP manager.
    mcp_handle: Option<apollia_mcp::manager::McpClientManagerHandle>,
}

impl Clone for AIPProductionBackend {
    fn clone(&self) -> Self {
        Self {
            bridge: Arc::clone(&self.bridge),
            agent_id: self.agent_id.clone(),
            manifest: self.manifest.clone(),
            allowed_tools: self.allowed_tools.clone(),
            llm_router: self.llm_router.clone(),
            event_bus: self.event_bus.clone(),
            tool_registry: self.tool_registry.clone(),
            audit_trail: self.audit_trail.clone(),
            memory_namespace: self.memory_namespace.clone(),
            memory_base_dir: self.memory_base_dir.clone(),
            pending_approvals: self.pending_approvals.clone(),
            plan_gates: self.plan_gates.clone(),
            plan_cache: self.plan_cache.clone(),
            task_repository: self.task_repository.clone(),
            a2a_invoker: self.a2a_invoker.clone(),
            tools_config: self.tools_config.clone(),
            user_memory_write: self.user_memory_write,
            datasources_declared: self.datasources_declared.clone(),
            templates_declared: self.templates_declared.clone(),
            agent_dir: self.agent_dir.clone(),
            secrets_declared: self.secrets_declared.clone(),
            secrets_data_dir: self.secrets_data_dir.clone(),
            mcp_handle: self.mcp_handle.clone(),
        }
    }
}

// ─────────────────────────────────────────────────────────────
// BridgeRunner: implements AgentRunner for ORIAEngine::execute_direct
// ─────────────────────────────────────────────────────────────

/// Wraps `AIPBridge` + context-building components as an `AgentRunner`.
///
/// Used by `AIPProductionBackend.execute()` to route tasks through
/// `ORIAEngine::execute_direct()`, which adds HITL suspension support
/// without changing the Python contract.
struct BridgeRunner {
    bridge: Arc<AIPBridge>,
    llm_router: Option<Arc<LlmRouter>>,
    event_bus: EventBusSender,
    agent_id: String,
    /// Manifest snapshot, exposed via [`AIPAgent::manifest`] so ORIA
    /// can drive the orchestrated path.
    manifest: AgentManifest,
    allowed_tools: Vec<String>,
    tool_registry: Option<ToolRegistryHandle>,
    audit_trail: Option<AuditTrailHandle>,
    memory_namespace: Option<String>,
    memory_base_dir: PathBuf,
    /// High-level A2A invoker, `None` if not available.
    a2a_invoker: Option<Arc<apollia_runtime::a2a::A2AInvoker>>,
    /// Operator-supplied tools configuration loaded from `apollia.toml`.
    tools_config: apollia_core::ToolsConfig,
    /// Manifest opt-in for `ctx.memory.remember_user()` writes into `__user__`.
    user_memory_write: bool,
    /// Datasources declared in the manifest.
    datasources_declared: Vec<String>,
    /// Jinja2 templates declared in the manifest.
    templates_declared: Vec<String>,
    /// Root directory of the agent, used to resolve `datasources/` and
    /// `templates/`.
    agent_dir: Option<PathBuf>,
    /// Secrets declared in the manifest (`manifest.secrets`), the allowlist
    /// for `ctx.secrets.get()`.
    secrets_declared: Vec<String>,
    /// Apollia data directory (`~/.apollia/`), used for lazy opening of the
    /// shared [`ToolCredentialStore`] that backs `ctx.secrets`.
    secrets_data_dir: Option<PathBuf>,
    /// MCP client manager handle, `None` when no MCP server is configured. Used
    /// at `call_run` to build one executor per registered MCP tool.
    mcp_handle: Option<apollia_mcp::manager::McpClientManagerHandle>,
    /// Direct-path `StepBudget`, shared with `execute_direct`. A live view of it
    /// is wired into the agent's `ctx.tools` and `ctx.llm` so the Python agent's
    /// tool and LLM calls are counted and cut off (principle #7, non-bypassable).
    budget: Arc<StepBudget>,
}

impl BridgeRunner {
    /// Builds the governed [`ToolProxy`] used to execute orchestrated plan steps.
    ///
    /// Mirrors the proxy `call_run` builds for the direct/ctx path so the
    /// orchestrated `ActorLoop` runs tools under the same permission engine,
    /// audit trail, disabled-tool set, and A2A routing. Returns `None` in
    /// degraded mode (tool registry or audit trail unavailable), matching
    /// `call_run`, in which case orchestrated tool steps fall back to the
    /// engine's `NoopToolProxy`.
    async fn build_tool_proxy(&self, task: &AIPTask) -> Option<ToolProxy> {
        let (registry, audit) = match (self.tool_registry.as_ref(), self.audit_trail.as_ref()) {
            (Some(r), Some(a)) => (r.clone(), a.clone()),
            _ => {
                tracing::warn!(
                    agent = %self.agent_id,
                    "orchestrated ToolProxy unavailable - tool registry or audit trail missing; \
                     orchestrated tool steps will fail via NoopToolProxy"
                );
                return None;
            }
        };

        // Extend allowed_tools with virtual A2A skill names before creating the proxy.
        let mut allowed_tools = self.allowed_tools.clone();
        if let Some(ref invoker) = self.a2a_invoker {
            let a2a_descriptors = A2AToolsProvider::new(Arc::clone(invoker))
                .build_tool_descriptors()
                .await;
            for desc in a2a_descriptors {
                allowed_tools.push(desc.name);
            }
        }

        let governance_base = self
            .memory_base_dir
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| self.memory_base_dir.clone());
        let snapshot = load_governance_snapshot(&governance_base).unwrap_or_else(|e| {
            tracing::warn!(error = %e, "governance snapshot unavailable - defaulting to all tools enabled");
            Default::default()
        });
        let disabled_tools = merge_disabled(&self.tools_config.disabled, snapshot.disabled_tools);
        let extra_executors = mcp_executors_for(&self.mcp_handle).await;
        let dispatcher = Arc::new(build_dispatcher_with(
            &NativeDispatcherConfig {
                sandbox_root: sandbox_root_for_agent(),
                agent_id: self.agent_id.clone(),
                venv_base_dir: self
                    .memory_base_dir
                    .parent()
                    .map(|p| p.join("venvs"))
                    .unwrap_or_else(|| self.memory_base_dir.join("venvs")),
                memory_namespace: self.memory_namespace.clone(),
                memory_shared_namespaces: Vec::new(),
                memory_base_dir: self.memory_base_dir.clone(),
                http_allowlist: None,
                pending_user_inputs: None,
                disabled_tools,
                brave_api_key: snapshot.brave_api_key,
                web_search_config: self.tools_config.web_search.clone(),
                web_read_config: self.tools_config.web_read.clone(),
                governance_db_path: Some(
                    governance_base.join(apollia_tools::GOVERNANCE_DB_FILENAME),
                ),
            },
            extra_executors,
        ));

        let proxy = ToolProxy::new(ToolProxyConfig {
            registry,
            audit,
            executor: Arc::new(DispatcherExecutor::new(dispatcher)),
            allowed_tools,
            agent_id: self.agent_id.clone(),
            task_id: task.task_id.clone(),
            run_id: task.run_id.clone(),
        })
        .with_event_bus(self.event_bus.clone());
        let proxy = if let Some(invoker) = self.a2a_invoker.clone() {
            proxy.with_a2a(invoker, 0, None)
        } else {
            proxy
        };
        Some(proxy)
    }
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
        let allowed_tools = self.allowed_tools.clone();
        let tool_registry = self.tool_registry.clone();
        let audit_trail = self.audit_trail.clone();
        let memory_namespace = self.memory_namespace.clone();
        let memory_base_dir = self.memory_base_dir.clone();
        let a2a_invoker = self.a2a_invoker.clone();
        let tools_config = self.tools_config.clone();
        let user_memory_write = self.user_memory_write;
        let datasources_declared = self.datasources_declared.clone();
        let templates_declared = self.templates_declared.clone();
        let agent_dir = self.agent_dir.clone();
        let secrets_declared = self.secrets_declared.clone();
        let secrets_data_dir = self.secrets_data_dir.clone();
        let mcp_handle = self.mcp_handle.clone();
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

            // Extend allowed_tools with virtual A2A skill names before creating the proxy.
            let mut allowed_tools = allowed_tools;
            if let Some(ref invoker) = a2a_invoker {
                let a2a_descriptors = A2AToolsProvider::new(Arc::clone(invoker))
                    .build_tool_descriptors()
                    .await;
                for desc in a2a_descriptors {
                    allowed_tools.push(desc.name);
                }
            }

            let governance_base = memory_base_dir
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_else(|| memory_base_dir.clone());
            let snapshot = load_governance_snapshot(&governance_base).unwrap_or_else(|e| {
                tracing::warn!(error = %e, "governance snapshot unavailable - defaulting to all tools enabled");
                Default::default()
            });
            let disabled_tools = merge_disabled(&tools_config.disabled, snapshot.disabled_tools);
            // Inject one MCP executor per registered tool so `ctx.tools.call("mcp:...")`
            // routes through the MCP client manager instead of returning UnknownTool.
            let extra_executors = mcp_executors_for(&mcp_handle).await;
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
                    pending_user_inputs: None,
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

            let tool_proxy: Option<ToolProxy> = match (tool_registry.as_ref(), audit_trail.as_ref())
            {
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
                    // tool_call_* instrumentation.
                    .with_event_bus(event_bus.clone())
                    // Direct-path budget: count and cap the agent's tool calls.
                    .with_budget(Arc::clone(&budget_view));
                    let proxy = if let Some(invoker) = a2a_invoker.clone() {
                        proxy.with_a2a(invoker, 0, None)
                    } else {
                        proxy
                    };
                    Some(proxy)
                }
                _ => {
                    tracing::warn!(
                        agent = %agent_id,
                        "ToolProxy not available - tool registry or audit trail missing; \
                         agent will use its own fallback for tool calls"
                    );
                    None
                }
            };

            let memory_interface: Option<MemoryInterface> =
                memory_namespace.as_deref().and_then(|ns| {
                    let eff_ns = effective_memory_namespace(ns, task.project_id.as_deref());
                    let manager =
                        MemoryManager::new(&memory_base_dir, Some(eff_ns.clone()), vec![]);
                    let iface = MemoryInterface::new(manager, eff_ns, agent_id.clone())?;
                    iface.announce_shared_namespaces(&event_bus);
                    Some(iface)
                });

            let profile_interface = {
                let data_dir = secrets_data_dir.clone().unwrap_or_else(|| {
                    let home = apollia_core::paths::home_dir_or_temp()
                        .display()
                        .to_string();
                    PathBuf::from(home).join(".apollia")
                });
                let user_memory_db = data_dir.join("user_memory.db");
                apollia_aip::profile::ProfileInterface::new(
                    user_memory_db,
                    agent_id.clone(),
                    user_memory_write,
                    agent_id == "onboarding-agent",
                )
            };

            let ctx: PyObject = Python::with_gil(|py| {
                let ctx = RuntimeContext::new_with_llm(
                    llm_router,
                    Arc::clone(&budget_view),
                    tool_helper,
                    Arc::new(ObservabilityConfig::default()),
                    event_bus,
                    agent_id.clone().into(),
                    tool_proxy,
                    memory_interface,
                    None, // mailbox: not wired in task mode
                    agent_id,
                    None, // user_context: task mode, not chat
                    a2a_invoker,
                    user_memory_write, // user_memory_writable: manifest-controlled
                )
                .with_profile(profile_interface)
                // Datasources YAML + templates Jinja2.
                .with_datasources(datasources_declared, agent_dir.as_deref())
                .with_templates(templates_declared, agent_dir.as_deref())
                // ctx.secrets read-only, gated by the manifest.
                .with_secrets(apollia_aip::secrets::SecretsInterface::new(
                    secrets_data_dir.as_deref().and_then(open_secret_store),
                    secrets_declared,
                ))
                // task_id used to label ctx.log() in the trace.
                .with_task_id(task.task_id.clone())
                .with_run_id(task.run_id.clone());
                Py::new(py, ctx)
                    .map(|p| p.into_any())
                    .expect("RuntimeContext PyObject construction failed")
            });

            bridge.call_run(&task, ctx).await.map_err(|e| e.to_string())
        })
    }
}

impl apollia_oria::engine::AIPAgent for BridgeRunner {
    fn manifest(&self) -> AgentManifest {
        self.manifest.clone()
    }

    fn has_on_plan_complete(&self) -> bool {
        self.bridge.has_on_plan_complete()
    }

    fn call_on_plan_complete(
        &self,
        step_results: std::collections::HashMap<String, String>,
    ) -> Pin<Box<dyn std::future::Future<Output = AIPResult> + Send + '_>> {
        // ORIA always calls this hook with an already-built ctx through the
        // bridge. Here we build a minimal ctx: the orchestrated plan step
        // outputs are formatted by the agent on its own, no `ctx.tools`
        // need to be wired for the post-processing hook.
        let bridge = Arc::clone(&self.bridge);
        let agent_id = self.agent_id.clone();
        let event_bus = self.event_bus.clone();

        Box::pin(async move {
            let ctx: PyObject = Python::with_gil(|py| {
                let ctx = RuntimeContext::new_with_llm(
                    None,
                    Arc::new(StepBudgetView::unlimited()),
                    Arc::new(ToolCallHelper::new(
                        Arc::new(RouterModel(Arc::new(LlmRouter::empty()))),
                        Arc::new(NoopToolInvoker),
                    )),
                    Arc::new(ObservabilityConfig::default()),
                    event_bus,
                    agent_id.clone().into(),
                    None,
                    None,
                    None,
                    agent_id.clone(),
                    None,
                    None,
                    false,
                );
                Py::new(py, ctx)
                    .map(|p| p.into_any())
                    .expect("RuntimeContext PyObject construction failed")
            });

            match bridge.call_on_plan_complete(step_results, ctx).await {
                Ok(result) => result,
                Err(e) => AIPResult::failed("ON_PLAN_COMPLETE_FAILED", &e.to_string()),
            }
        })
    }
}

impl ExecutionBackend for AIPProductionBackend {
    fn execute(
        &self,
        task: AIPTask,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<AIPResult, String>> + Send>> {
        // Agent-declared budget capped to the runtime ceiling. The same
        // Arc<StepBudget> is shared into the runner (so the Direct-path ctx
        // counts the agent's tool/LLM calls) and into execute_direct (so the
        // engine supervises the same counters). Principle #7 holds on the Direct
        // path, not only wall-clock.
        let agent_step_budget = self.manifest.step_budget.clone().unwrap_or_default();
        let direct_budget = Arc::new(direct_path_budget(&agent_step_budget));

        let runner = BridgeRunner {
            bridge: Arc::clone(&self.bridge),
            llm_router: self.llm_router.clone(),
            event_bus: self.event_bus.clone(),
            agent_id: self.agent_id.clone(),
            manifest: self.manifest.clone(),
            allowed_tools: self.allowed_tools.clone(),
            tool_registry: self.tool_registry.clone(),
            audit_trail: self.audit_trail.clone(),
            memory_namespace: self.memory_namespace.clone(),
            memory_base_dir: self.memory_base_dir.clone(),
            a2a_invoker: self.a2a_invoker.clone(),
            tools_config: self.tools_config.clone(),
            user_memory_write: self.user_memory_write,
            datasources_declared: self.datasources_declared.clone(),
            templates_declared: self.templates_declared.clone(),
            agent_dir: self.agent_dir.clone(),
            secrets_declared: self.secrets_declared.clone(),
            secrets_data_dir: self.secrets_data_dir.clone(),
            mcp_handle: self.mcp_handle.clone(),
            budget: Arc::clone(&direct_budget),
        };

        // Build a per-task ORIAEngine wired with HITL components.
        let mut engine = ORIAEngine::new().with_event_bus(self.event_bus.clone());
        if let Some(pending) = self.pending_approvals.clone() {
            engine = engine.with_pending_approvals(pending);
        }
        if let Some(repo) = self.task_repository.clone() {
            engine = engine.with_task_repository(repo);
        }

        // Plan-mode: forward the per-run gate override. Wire the shared gate
        // registry only when the gate is explicitly requested (`--plan`), so
        // headless submissions (A2A, triggers) never pause for a decision.
        engine = engine.with_plan_gate_override(task.run_options.plan_gate);
        if task.run_options.plan_gate == Some(true) {
            if let Some(gates) = self.plan_gates.clone() {
                engine = engine.with_pending_plan_gates(gates);
            }
        }
        if let Some(cache) = self.plan_cache.clone() {
            engine = engine.with_shared_plan_cache(cache);
        }
        // CLI `--autonomy` override feeds the engine tier (drives the gate
        // policy when no explicit `--plan` override is set).
        if let Some(tier) = task.run_options.autonomy_level {
            engine = engine.with_oria_config(apollia_core::ORIAConfig {
                autonomy_level: Some(tier),
                ..apollia_core::ORIAConfig::default()
            });
        }

        let execution_mode = self.manifest.execution_mode.clone();
        let step_budget_max = self
            .manifest
            .step_budget
            .as_ref()
            .map(|b| b.max_steps)
            .unwrap_or(20);

        // Wire the Reasoner + LlmRouter so ORIA's orchestrated path can plan.
        // Extracted into `wire_engine_with_llm` for unit testing, see the
        // regression guard in the test module at the bottom of this file.
        engine = wire_engine_with_llm(
            engine,
            self.llm_router.clone(),
            &self.agent_id,
            step_budget_max,
        );

        Box::pin(async move {
            // Orchestrated agents (declared via `@orchestrated`) flow through
            // ORIA's planning + ActorLoop. Everything else uses the direct
            // path which invokes `__apollia_dispatch__` (skills, @on_message).
            if execution_mode == "orchestrated" {
                // Wire the governed ToolProxy so orchestrated plan steps execute
                // real tools (under permission + audit + resilience + budget)
                // instead of the engine's NoopToolProxy. Without it, an
                // orchestrated agent could only run LLM steps.
                let engine = match runner.build_tool_proxy(&task).await {
                    Some(proxy) => engine.with_tool_proxy(Arc::new(OriaToolProxy { proxy })),
                    None => engine,
                };
                Ok(engine.execute(task, &runner).await)
            } else {
                // Bound the direct path by the same budget shared into the
                // runner's ctx, so the engine supervisor and the agent's
                // tool/LLM chokepoints enforce one set of counters.
                engine
                    .execute_direct(task, &runner, direct_budget)
                    .await
                    .map_err(|e| e.to_string())
            }
        })
    }
}

// ─────────────────────────────────────────────────────────────
// Factory: creates one AIPProductionBackend per agent at `agent start`
// ─────────────────────────────────────────────────────────────

/// Creates a real `AIPProductionBackend` per agent.
///
/// Called once from `POST /api/v1/agents`: loads Python, validates AIP duck typing,
/// and bakes an `AIPBridge` into a backend registered with the `TaskRouter`.
///
/// Uses `OnceLock` for `event_bus` and `llm_router` because they are created
/// inside `supervisor.start()`, which runs after this factory is constructed.
/// Both locks are populated before the first HTTP request arrives.
struct ProductionBackendFactory {
    event_bus: Arc<std::sync::OnceLock<EventBusSender>>,
    llm_router: Arc<std::sync::OnceLock<Option<Arc<LlmRouter>>>>,
    tool_registry: Arc<std::sync::OnceLock<ToolRegistryHandle>>,
    audit_trail: Arc<std::sync::OnceLock<AuditTrailHandle>>,
    pending_approvals: Arc<std::sync::OnceLock<Arc<PendingApprovals>>>,
    plan_gates: Arc<std::sync::OnceLock<Arc<apollia_oria::PendingPlanGates>>>,
    #[allow(clippy::type_complexity)]
    plan_cache: Arc<
        std::sync::OnceLock<Arc<std::sync::Mutex<apollia_oria::plan_cache::PlanCacheRepository>>>,
    >,
    task_repository: Arc<std::sync::OnceLock<Arc<TaskRepository>>>,
    /// Agent registry handle, populated after supervisor.start().
    registry: Arc<std::sync::OnceLock<AgentRegistryHandle>>,
    /// Task router handle, populated after supervisor.start().
    router: Arc<std::sync::OnceLock<TaskRouterHandle<DynBackend>>>,
    /// Operator-supplied tools configuration loaded from `apollia.toml`.
    tools_config: apollia_core::ToolsConfig,
    /// Base data directory (`~/.apollia/`), used to open the shared
    /// [`ToolCredentialStore`] on each agent execution (`ctx.secrets`).
    data_dir: PathBuf,
    /// MCP client manager handle, populated after `supervisor.start()`. Threaded
    /// into each `AIPProductionBackend` so agent dispatchers can execute MCP tools.
    mcp_handle: Arc<std::sync::OnceLock<Option<apollia_mcp::manager::McpClientManagerHandle>>>,
}

impl AgentBackendFactory for ProductionBackendFactory {
    fn create_for_agent(&self, agent_path: &Path, manifest: &AgentManifest) -> DynBackend {
        let agent_id = manifest.name.clone();

        // Retrieve the lazily-initialized event bus and LLM router.
        //
        // The OnceLocks are populated by start.rs *after* the Supervisor has
        // run its Phase 11 auto-load (which calls this factory). When that
        // happens we return a placeholder NoopBackend and rely on
        // [`rewire_auto_loaded_agents`] to replace it immediately. The
        // diagnostic is therefore DEBUG (operator-relevant only when
        // troubleshooting the rewire path), not ERROR.
        let event_bus = match self.event_bus.get() {
            Some(bus) => bus.clone(),
            None => {
                tracing::debug!(
                    agent = %agent_id,
                    "factory invoked before runtime handles are populated - \
                     emitting placeholder NoopBackend, will be rewired post-start"
                );
                return DynBackend::new(NoopBackend);
            }
        };
        let llm_router = self.llm_router.get().cloned().flatten();
        let tool_registry = self.tool_registry.get().cloned();
        let audit_trail = self.audit_trail.get().cloned();
        let pending_approvals = self.pending_approvals.get().cloned();
        let plan_gates = self.plan_gates.get().cloned();
        let plan_cache = self.plan_cache.get().cloned();
        let task_repository = self.task_repository.get().cloned();
        let mcp_handle = self.mcp_handle.get().cloned().flatten();

        // Build the A2A invoker if registry + router are available.
        let a2a_invoker = match (self.registry.get().cloned(), self.router.get().cloned()) {
            (Some(registry), Some(router)) => {
                let invoker = Arc::new(apollia_runtime::a2a::A2AInvoker::new(
                    registry,
                    router,
                    event_bus.clone(),
                    apollia_core::A2AConfig::default(),
                ));
                Some(invoker)
            }
            _ => {
                tracing::warn!(
                    agent = %agent_id,
                    "A2A invoker not available - registry or router not yet initialized"
                );
                None
            }
        };

        let result: Result<AIPProductionBackend, String> = (|| {
            // Re-use the community helper so the ProductionBackendFactory
            // sees the same sys.path layering as `agent install / validate`.
            let extras = crate::community::validation_sys_paths(agent_path);
            let module = apollia_aip::loader::load_agent_module_with_sys_paths(agent_path, &extras)
                .map_err(|e| e.to_string())?;
            let validated =
                apollia_aip::validator::validate_agent(&module).map_err(|e| e.to_string())?;
            let allowed_tools = validated.manifest.tools_required.clone();
            let memory_namespace = validated.manifest.memory_namespace.clone();
            let user_memory_write = validated.manifest.user_memory_write;
            // Capture datasources/templates declarations + the agent's package
            // directory so the BridgeRunner can build ctx.datasources /
            // ctx.templates on every call_run.
            let datasources_declared = validated.manifest.datasources.clone();
            let templates_declared = validated.manifest.templates.clone();
            let agent_dir = agent_path.parent().map(Path::to_path_buf);
            // Capture the list of declared secrets.
            let secrets_declared = validated.manifest.secrets.clone();
            let bridge = Arc::new(AIPBridge::new(validated).map_err(|e| e.to_string())?);
            Ok(AIPProductionBackend {
                bridge,
                agent_id: agent_id.clone(),
                manifest: manifest.clone(),
                allowed_tools,
                llm_router,
                event_bus,
                tool_registry,
                audit_trail,
                memory_namespace,
                memory_base_dir: default_memory_dir(),
                pending_approvals,
                plan_gates,
                plan_cache,
                task_repository,
                a2a_invoker,
                tools_config: self.tools_config.clone(),
                user_memory_write,
                datasources_declared,
                templates_declared,
                agent_dir,
                secrets_declared,
                secrets_data_dir: Some(self.data_dir.clone()),
                mcp_handle,
            })
        })();

        match result {
            Ok(backend) => DynBackend::new(backend),
            Err(e) => {
                tracing::error!(
                    agent = %agent_id,
                    path = %agent_path.display(),
                    error = %e,
                    "failed to load agent Python module - falling back to NoopBackend"
                );
                DynBackend::new(NoopBackend)
            }
        }
    }
}

/// Returns the default memory directory (`~/.apollia/memory/`).
///
/// Matches the path convention used by `apollia-os memory inspect` and `MemoryManager`.
fn default_memory_dir() -> PathBuf {
    let home = apollia_core::paths::home_dir_or_temp()
        .display()
        .to_string();
    PathBuf::from(home).join(".apollia").join("memory")
}

/// Resolves `~` to `$HOME` in a path string.
fn expand_tilde_str(s: &str) -> PathBuf {
    if s.starts_with("~/") {
        let home = apollia_core::paths::home_string().unwrap_or_default();
        PathBuf::from(format!("{}{}", home, &s[1..]))
    } else {
        PathBuf::from(s)
    }
}

/// Finds `apollia.toml` by searching in order:
///   1. `./apollia.toml`      (current working directory)
///   2. `~/.config/apollia/apollia.toml`  (user config dir)
///
/// Returns `None` if neither exists.
pub(crate) fn find_config_file() -> Option<PathBuf> {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let local = cwd.join("apollia.toml");
    if local.exists() {
        return Some(local);
    }
    let user_cfg = expand_tilde_str("~/.config/apollia/apollia.toml");
    if user_cfg.exists() {
        return Some(user_cfg);
    }
    None
}

/// Returns `true` if a TCP listener is already bound on `port`.
async fn port_is_in_use(port: u16) -> bool {
    tokio::net::TcpStream::connect(("127.0.0.1", port))
        .await
        .is_ok()
}

/// Returns `true` only when a live runtime is currently listening on `path`.
///
/// A previous crash can leave a stale socket file behind. To distinguish it
/// from a real running daemon we attempt a short-timeout connect: success
/// means a process is bound, `ConnectionRefused` means the file is stale and
/// safe to remove on the caller's side.
#[cfg(unix)]
async fn socket_is_in_use(path: &std::path::Path) -> bool {
    if !path.exists() {
        return false;
    }
    match tokio::time::timeout(
        std::time::Duration::from_millis(200),
        tokio::net::UnixStream::connect(path),
    )
    .await
    {
        Ok(Ok(_)) => true,
        Ok(Err(_)) | Err(_) => false,
    }
}

/// On Windows there is no Unix socket, so we check whether the default TCP
/// port is already bound by another daemon.
#[cfg(windows)]
async fn socket_is_in_use(_path: &std::path::Path) -> bool {
    use crate::client::DEFAULT_TCP_PORT;
    match tokio::time::timeout(
        std::time::Duration::from_millis(200),
        tokio::net::TcpStream::connect(format!("127.0.0.1:{}", DEFAULT_TCP_PORT)),
    )
    .await
    {
        Ok(Ok(_)) => true,
        Ok(Err(_)) | Err(_) => false,
    }
}

/// Remove a stale Unix socket file left over from a previous crashed runtime.
///
/// Called only after [`socket_is_in_use`] returned `false` for an existing
/// path, so we know no live daemon is listening. Any error is logged but not
/// propagated: the subsequent bind will surface the real cause.
fn cleanup_stale_socket(path: &std::path::Path) {
    if !path.exists() {
        return;
    }
    match std::fs::remove_file(path) {
        Ok(()) => {
            tracing::info!(
                socket = %path.display(),
                "removed stale Unix socket file from a previous crashed runtime"
            );
        }
        Err(e) => {
            tracing::warn!(
                socket = %path.display(),
                error = %e,
                "failed to remove stale Unix socket file"
            );
        }
    }
}

/// Bootstrap and run the runtime in foreground.
///
/// Uses the Supervisor for ordered startup with timeout and rollback.
/// Blocks until Ctrl+C, SIGTERM, or `POST /api/v1/shutdown` is received.
/// Graceful shutdown drains in-progress tasks (30s default).
///
/// Returns `Ok(true)` when shutdown was triggered by SIGINT (Ctrl+C), so the
/// caller can exit with code 5 to distinguish voluntary interruption from
/// success (0) or error (1–4).
pub async fn run(socket: Option<PathBuf>, port: Option<u16>) -> Result<bool, StartError> {
    let start = Instant::now();
    let socket_path = socket.unwrap_or_else(|| PathBuf::from(DEFAULT_SOCKET_PATH));
    let tcp_port = port.unwrap_or(DEFAULT_TCP_PORT);

    // Detect an already-running runtime before attempting to bind, so the user
    // gets a clear message instead of a low-level "Address already in use" error.
    if port_is_in_use(tcp_port).await {
        return Err(StartError::AlreadyRunning {
            address: format!("localhost:{tcp_port}"),
        });
    }
    if socket_is_in_use(&socket_path).await {
        return Err(StartError::AlreadyRunning {
            address: socket_path.display().to_string(),
        });
    }
    // The file exists but nobody is listening: clean it up so the bind succeeds.
    cleanup_stale_socket(&socket_path);

    let home = apollia_core::paths::home_dir_or_temp();

    // Load apollia.toml if found. Agents, triggers, pipelines, notifications, and stt
    // are loaded from SQLite by the Supervisor; only static sections are parsed here.
    let (loaded_config, config_path) = load_start_config()?;
    let (
        llm_config,
        api_file_config,
        runtime_file_config,
        hitl_file_config,
        tools_file_config,
        mcp_file_config,
        hooks_file_config,
        chat_file_config,
    ) = match loaded_config {
        Some(cfg) => (
            cfg.llm,
            cfg.api,
            cfg.runtime,
            cfg.hitl,
            cfg.tools,
            cfg.mcp,
            cfg.hooks,
            cfg.chat,
        ),
        None => (None, None, None, None, None, None, None, None),
    };

    let llm_label = llm_config
        .as_ref()
        .map(|l| format!("backend \"{}\"", l.default))
        .unwrap_or_else(|| "disabled".to_string());

    // Open AgentRepository for auto-load at boot.
    let data_dir = home.join(".apollia");
    let data_dir_for_chat = data_dir.clone();
    let agent_repository: Option<apollia_tools::AgentRepository> = {
        let db_path = data_dir.join("agents.db");
        match apollia_tools::AgentRepository::open(&db_path) {
            Ok(repo) => {
                tracing::info!("AgentRepository opened for auto-load");
                Some(repo)
            }
            Err(e) => {
                tracing::warn!(error = %e, "AgentRepository failed to open - auto-load disabled");
                None
            }
        }
    };
    // Keep a separate handle so we can rebuild auto-loaded backends after the
    // Supervisor has finished and the factory OnceLocks are fully populated
    // (workaround for the Phase 11 init-order race; see post-rewire below).
    let agent_repository_for_rewire = agent_repository.clone();

    // Open PackageRepository for Phase 10.6 integrity check.
    let package_repository: Option<apollia_tools::PackageRepository> = {
        let db_path = data_dir.join("agents.db");
        match apollia_tools::PackageRepository::open(&db_path) {
            Ok(repo) => Some(repo),
            Err(e) => {
                tracing::warn!(error = %e, "PackageRepository failed to open - Phase 10.6 disabled");
                None
            }
        }
    };

    // Resolve [api] section (absent = all defaults).
    let api_cfg = api_file_config.unwrap_or_default();
    let bind_addr = api_cfg.bind.clone();

    // Load or generate the API token when require_token = true.
    // Principle #4 (Fail fast): if the token cannot be loaded or generated, refuse to start
    // rather than silently degrading to an unauthenticated API.
    let api_token: Option<String> = if api_cfg.require_token {
        let token = apollia_runtime::api::load_or_generate_token(&data_dir)
            .map_err(StartError::ApiToken)?;
        Some(token)
    } else {
        tracing::info!("API token auth disabled via require_token = false");
        None
    };

    // Start all actors via Supervisor (ordered, with timeout + rollback)
    let runtime_config = runtime_file_config.unwrap_or_default();
    let hitl_config = hitl_file_config.unwrap_or_default();
    let tools_config = tools_file_config.unwrap_or_default();
    let mcp_config = mcp_file_config.unwrap_or_default();
    let hooks_config = hooks_file_config.unwrap_or_default();
    let chat_config = chat_file_config.unwrap_or_default();
    let config = SupervisorConfig {
        api_config: APIServerConfig {
            socket_path: socket_path.clone(),
            bind_addr,
            tcp_port: Some(tcp_port),
            api_token,
            tls_cert_path: api_cfg.tls_cert.clone(),
            tls_key_path: api_cfg.tls_key.clone(),
        },
        startup_timeout_secs: 10,
        llm_config,
        config_path,
        runtime_config,
        hitl_config,
        data_dir,
        obs_config: apollia_core::ObservabilityConfig::default(),
        agent_repository,
        package_repository,
        bundled_agents_path: {
            // Look for agents/bundled/ adjacent to the binary, then in the current directory.
            let from_exe = std::env::current_exe()
                .ok()
                .and_then(|exe| exe.parent().map(|d| d.join("agents").join("bundled")))
                .filter(|p| p.exists());
            let from_cwd = std::env::current_dir()
                .ok()
                .map(|d| d.join("agents").join("bundled"))
                .filter(|p| p.exists());
            from_exe.or(from_cwd)
        },
        tools_config: tools_config.clone(),
        mcp_loading: apollia_mcp::session::LoadingMode::from(mcp_config.tool_loading),
        tool_search_limit: mcp_config.tool_search_limit,
        hooks_config,
        plan_mode_default: chat_config.plan_mode_default,
        chat_default_workspace: chat_config.default_workspace.clone(),
        chat_tool_turn_temperature: chat_config.tool_turn_temperature,
    };
    let supervisor = Supervisor::new(config);
    let agent_loader: Arc<dyn AgentLoader> = Arc::new(AIPAgentLoader);

    // The ProductionBackendFactory needs the EventBusSender, which is created
    // inside supervisor.start(). We use a shared OnceLock so the factory can be
    // constructed before start() returns, then initialized lazily before first use.
    //
    // Safety: create_for_agent() is called only from POST /api/v1/agents, which
    // happens after the runtime is fully up, well after start() returns and the
    // OnceLock is populated.
    let event_bus_lock: Arc<std::sync::OnceLock<EventBusSender>> =
        Arc::new(std::sync::OnceLock::new());
    let llm_router_lock: Arc<std::sync::OnceLock<Option<Arc<LlmRouter>>>> =
        Arc::new(std::sync::OnceLock::new());
    let tool_registry_lock: Arc<std::sync::OnceLock<ToolRegistryHandle>> =
        Arc::new(std::sync::OnceLock::new());
    let audit_trail_lock: Arc<std::sync::OnceLock<AuditTrailHandle>> =
        Arc::new(std::sync::OnceLock::new());
    let pending_approvals_lock: Arc<std::sync::OnceLock<Arc<PendingApprovals>>> =
        Arc::new(std::sync::OnceLock::new());
    #[allow(clippy::type_complexity)]
    let plan_cache_lock: Arc<
        std::sync::OnceLock<Arc<std::sync::Mutex<apollia_oria::plan_cache::PlanCacheRepository>>>,
    > = Arc::new(std::sync::OnceLock::new());
    let plan_gates_lock: Arc<std::sync::OnceLock<Arc<apollia_oria::PendingPlanGates>>> =
        Arc::new(std::sync::OnceLock::new());
    let task_repository_lock: Arc<std::sync::OnceLock<Arc<TaskRepository>>> =
        Arc::new(std::sync::OnceLock::new());
    let registry_lock: Arc<std::sync::OnceLock<AgentRegistryHandle>> =
        Arc::new(std::sync::OnceLock::new());
    let router_lock: Arc<std::sync::OnceLock<TaskRouterHandle<DynBackend>>> =
        Arc::new(std::sync::OnceLock::new());
    let user_memory_lock: Arc<
        std::sync::OnceLock<
            Option<Arc<std::sync::Mutex<apollia_memory::user_memory::UserMemoryRepository>>>,
        >,
    > = Arc::new(std::sync::OnceLock::new());
    let pending_user_inputs_lock: Arc<std::sync::OnceLock<PendingUserInputs>> =
        Arc::new(std::sync::OnceLock::new());
    let mcp_handle_lock: Arc<
        std::sync::OnceLock<Option<apollia_mcp::manager::McpClientManagerHandle>>,
    > = Arc::new(std::sync::OnceLock::new());

    let factory: Arc<dyn AgentBackendFactory> = Arc::new(ProductionBackendFactory {
        event_bus: event_bus_lock.clone(),
        llm_router: llm_router_lock.clone(),
        tool_registry: tool_registry_lock.clone(),
        audit_trail: audit_trail_lock.clone(),
        pending_approvals: pending_approvals_lock.clone(),
        plan_gates: plan_gates_lock.clone(),
        plan_cache: plan_cache_lock.clone(),
        task_repository: task_repository_lock.clone(),
        registry: registry_lock.clone(),
        router: router_lock.clone(),
        tools_config: tools_config.clone(),
        data_dir: data_dir_for_chat.clone(),
        mcp_handle: mcp_handle_lock.clone(),
    });
    // Keep a handle for the post-supervisor rewire pass.
    let factory_for_rewire = factory.clone();

    // Concrete ChatAgentRunner for Chat Agent mode.
    let chat_agent_runner: Option<Arc<dyn apollia_runtime::chat::ChatAgentRunner>> =
        Some(Arc::new(AIPChatAgentRunner {
            event_bus: event_bus_lock.clone(),
            llm_router: llm_router_lock.clone(),
            tool_registry: tool_registry_lock.clone(),
            audit_trail: audit_trail_lock.clone(),
            user_memory: user_memory_lock.clone(),
            pending_user_inputs: pending_user_inputs_lock.clone(),
            agent_registry: registry_lock.clone(),
            task_router: router_lock.clone(),
            data_dir: data_dir_for_chat,
            tools_config,
            mcp_handle: mcp_handle_lock.clone(),
        }));

    let handles = supervisor
        .start(
            DynBackend::new(NoopBackend),
            agent_loader,
            Some(factory),
            chat_agent_runner,
        )
        .await?;

    // Populate the OnceLocks now that the supervisor is running.
    let _ = event_bus_lock.set(handles.event_sender.clone());
    let _ = llm_router_lock.set(handles.llm_router.clone());
    let _ = tool_registry_lock.set(handles.tool_registry_handle.clone());
    let _ = registry_lock.set(handles.registry_handle.clone());
    let _ = router_lock.set(handles.router_handle.clone());
    set_lock_if_some(&audit_trail_lock, handles.audit_trail.clone());
    set_lock_if_some(&pending_approvals_lock, handles.pending_approvals.clone());
    set_lock_if_some(&plan_gates_lock, handles.plan_gates.clone());
    set_lock_if_some(&plan_cache_lock, handles.plan_cache.clone());
    set_lock_if_some(&task_repository_lock, handles.task_repository.clone());
    let _ = user_memory_lock.set(handles.user_memory.clone());
    // Cloned (not moved) so the ShutdownController still receives handles.mcp_handle.
    let _ = mcp_handle_lock.set(handles.mcp_handle.clone());
    set_lock_if_some(
        &pending_user_inputs_lock,
        handles
            .chat_manager
            .as_ref()
            .map(|c| c.pending_user_inputs()),
    );

    // Rewire auto-loaded agents now that the factory's OnceLocks are populated.
    //
    // The Supervisor's Phase 11 (auto-load) calls factory.create_for_agent
    // BEFORE we get a chance to populate the OnceLocks the factory closes
    // over. As a result every auto-loaded agent is initially registered with
    // a NoopBackend that fails every task. Iterate the same repository the
    // Supervisor used and reissue create_for_agent (now wired) + replace the
    // coordinator in the router. Idempotent: register_coordinator inserts
    // into a HashMap so the previous entry is dropped cleanly.
    if let Some(ref repo) = agent_repository_for_rewire {
        rewire_auto_loaded_agents(repo, &factory_for_rewire, &handles, &handles.event_sender).await;
    }

    let elapsed = start.elapsed();
    // Query the registry for an accurate count instead of hard-coding the old
    // "3 native tools" string, which under-reports the ~60 native + connector
    // + MCP tools the runtime actually registers at boot.
    let tool_count_display = match handles.tool_registry_handle.list().await {
        Ok(descriptors) => format!("{} tools", descriptors.len()),
        Err(_) => "tool count unavailable".to_string(),
    };
    println!("  * EventBus            ready");
    println!("  * AgentRegistry       ready");
    println!("  * ToolRegistry        ready ({tool_count_display})");
    println!("  * LlmRouter           {llm_label}");
    println!("  * TaskRouter          ready");
    println!("  * TriggerEngine       ready (loaded from SQLite)");
    println!("  * PipelineEngine      ready (loaded from SQLite)");
    println!(
        "  * APIServer           listening on {} + localhost:{}",
        socket_path.display(),
        tcp_port
    );
    println!("  * NotificationEngine  ready (loaded from SQLite)");
    println!("  -------------------------------------------------");
    println!("  * Runtime ready in {:.1}s", elapsed.as_secs_f64());
    println!();
    println!("  Press Ctrl+C or run `apollia-os stop` to shut down.");

    // Wait for shutdown signal (Ctrl+C, SIGTERM, or ShutdownRequested via API)
    let mut shutdown_rx = handles.event_sender.subscribe();
    let interrupted = tokio::select! {
        signal = apollia_runtime::shutdown::wait_for_shutdown_signal() => {
            println!();
            println!("  {signal} received, draining tasks...");
            signal == "SIGINT"
        }
        _ = wait_for_shutdown_event(&mut shutdown_rx) => {
            println!("  Shutdown requested via API, draining tasks...");
            false
        }
    };

    // Graceful shutdown via ShutdownController (drain + ordered teardown)
    let tool_registry_handle = handles.tool_registry_handle;
    let shutdown = ShutdownController::new(ShutdownControllerDeps {
        config: ShutdownConfig::default(),
        event_sender: handles.event_sender,
        api_handle: handles.api_handle,
        router_handle: handles.router_handle,
        registry_handle: handles.registry_handle,
        notification_engine: handles.notification_engine,
        mcp_handle: handles.mcp_handle,
    });

    match shutdown.shutdown().await {
        Ok(()) => println!("  * Runtime stopped."),
        Err(e) => eprintln!("  * Runtime stopped with warnings: {e}"),
    }

    // Stop the tool registry after the main shutdown sequence
    tool_registry_handle.shutdown().await;

    // Clean up socket file
    if socket_path.exists() {
        let _ = std::fs::remove_file(&socket_path);
    }

    Ok(interrupted)
}

/// Locates and parses `apollia.toml` if present, validating the `[tools]`,
/// `[mcp]`, and `[hooks]` sections. Returns the parsed config and its path, or
/// `(None, None)` when no config file is found (defaults are then used by the
/// caller).
fn load_start_config(
) -> Result<(Option<crate::config::ApolliaCConfig>, Option<PathBuf>), StartError> {
    let Some(path) = find_config_file() else {
        tracing::info!("no apollia.toml found - starting with defaults");
        return Ok((None, None));
    };
    tracing::info!(config = %path.display(), "loading config");
    let cfg = crate::config::parse_apollia_toml(&path).map_err(|e| StartError::Config {
        path: path.clone(),
        reason: e.to_string(),
    })?;
    if let Some(tools) = cfg.tools.as_ref() {
        tools.validate().map_err(|e| StartError::Config {
            path: path.clone(),
            reason: e.to_string(),
        })?;
    }
    if let Some(mcp) = cfg.mcp.as_ref() {
        mcp.validate().map_err(|e| StartError::Config {
            path: path.clone(),
            reason: e.to_string(),
        })?;
    }
    if let Some(hooks) = cfg.hooks.as_ref() {
        hooks.validate().map_err(|e| StartError::Config {
            path: path.clone(),
            reason: e.to_string(),
        })?;
    }
    Ok((Some(cfg), Some(path)))
}

/// Populates a shared `OnceLock` from an optional value, ignoring the result
/// (the lock is set at most once; a second attempt is a harmless no-op).
fn set_lock_if_some<T>(lock: &std::sync::OnceLock<T>, value: Option<T>) {
    if let Some(v) = value {
        let _ = lock.set(v);
    }
}

/// Rewire every auto-loaded enabled agent so its TaskRouter coordinator uses
/// a real `AIPProductionBackend` instead of the `NoopBackend` fallback that
/// Supervisor Phase 11 installs when the factory OnceLocks are still empty.
///
/// This compensates for the construction order: the factory is built before
/// the Supervisor runs, but the Supervisor populates the runtime handles
/// only as part of its startup. Calling `register_coordinator` here is
/// idempotent: the router replaces the existing entry in its `HashMap`.
async fn rewire_auto_loaded_agents(
    repo: &apollia_tools::AgentRepository,
    factory: &Arc<dyn AgentBackendFactory>,
    handles: &apollia_runtime::supervisor::SupervisorHandles<DynBackend>,
    event_sender: &EventBusSender,
) {
    let installed = match repo.list_enabled() {
        Ok(rows) => rows,
        Err(e) => {
            tracing::warn!(error = %e, "rewire: failed to list installed agents - skipping");
            return;
        }
    };
    let mut rewired = 0usize;
    for agent in installed {
        if !agent.enabled {
            continue;
        }
        let agent_id = match handles
            .registry_handle
            .find_by_name(&agent.manifest.name)
            .await
        {
            Ok(Some(id)) => id,
            Ok(None) => {
                tracing::debug!(
                    name = %agent.manifest.name,
                    "rewire: agent not in registry (Supervisor skipped it), nothing to rewire"
                );
                continue;
            }
            Err(e) => {
                tracing::warn!(
                    name = %agent.manifest.name,
                    error = %e,
                    "rewire: registry lookup failed"
                );
                continue;
            }
        };
        let dyn_backend = factory.create_for_agent(&agent.install_path, &agent.manifest);
        let mut coordinator = ExecutionCoordinator::new(
            agent_id.clone(),
            agent.manifest.max_concurrent_tasks,
            event_sender.clone(),
            dyn_backend,
        )
        .with_agent_name(agent.manifest.name.clone());
        if let Some(ref task_repo) = handles.task_repository {
            coordinator = coordinator.with_task_repository(
                Arc::clone(task_repo),
                apollia_core::ObservabilityConfig::default(),
            );
        }
        match handles
            .router_handle
            .register_coordinator(agent_id.clone(), coordinator)
            .await
        {
            Ok(()) => {
                rewired += 1;
                tracing::debug!(
                    agent = %agent.manifest.name,
                    "rewire: coordinator replaced with wired backend"
                );
            }
            Err(e) => {
                tracing::warn!(
                    agent = %agent.manifest.name,
                    error = %e,
                    "rewire: failed to replace coordinator"
                );
            }
        }
    }
    if rewired > 0 {
        tracing::info!(
            count = rewired,
            "auto-loaded agents rewired with fully-initialised backends"
        );
    }
}

/// Wait until a `RuntimeEvent::ShutdownRequested` event is received on the bus.
async fn wait_for_shutdown_event(rx: &mut tokio::sync::broadcast::Receiver<RuntimeEvent>) {
    loop {
        match rx.recv().await {
            Ok(RuntimeEvent::ShutdownRequested) => return,
            Ok(_) => continue,
            Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                tracing::warn!(lagged = n, "EventBus receiver lagged");
                continue;
            }
            Err(tokio::sync::broadcast::error::RecvError::Closed) => return,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use apollia_core::TaskStatus;
    use apollia_oria::engine::AIPAgent;

    #[test]
    fn test_noop_backend_is_clone() {
        let backend = NoopBackend;
        let _cloned = backend.clone();
    }

    #[test]
    fn test_start_error_display() {
        let err = StartError::Supervisor(
            apollia_runtime::supervisor::SupervisorError::ConfigError("bad config".to_string()),
        );
        assert!(err.to_string().contains("bad config"));
    }

    // ── Direct-path budget guardrail (principle #7) ─────────────────────

    /// GIVEN an agent manifest declaring no budget (the unwrap_or_default case)
    /// WHEN the direct-path StepBudget is built
    /// THEN it is bounded, never the previous unlimited() (u32::MAX + 24h)
    #[test]
    fn test_direct_path_budget_is_bounded() {
        // GIVEN
        let agent = StepBudgetConfig::default();

        // WHEN
        let budget = direct_path_budget(&agent);

        // THEN it is bounded on every dimension
        assert!(budget.max_steps < u32::MAX, "steps must be bounded");
        assert!(
            budget.max_tool_calls < u32::MAX,
            "tool calls must be bounded"
        );
        assert!(
            budget.wall_clock_limit < std::time::Duration::from_secs(86_400),
            "wall clock must be bounded well under the old 24h unlimited() value"
        );
    }

    /// GIVEN an agent that declares an oversized budget
    /// WHEN the direct-path StepBudget is built
    /// THEN the runtime ceiling wins on every dimension (agent cannot exceed it)
    #[test]
    fn test_direct_path_budget_caps_oversized_manifest() {
        // GIVEN an agent asking for far more than the runtime ceiling
        let agent = StepBudgetConfig {
            max_steps: 100_000,
            max_tool_calls: 100_000,
            wall_clock_secs: 999_999,
        };

        // WHEN
        let budget = direct_path_budget(&agent);

        // THEN the runtime ceiling (StepBudgetConfig::default) clamps it
        let ceiling = StepBudgetConfig::default();
        assert_eq!(budget.max_steps, ceiling.max_steps);
        assert_eq!(budget.max_tool_calls, ceiling.max_tool_calls);
        assert_eq!(
            budget.wall_clock_limit,
            std::time::Duration::from_secs(ceiling.wall_clock_secs)
        );
    }

    // ── Orchestrated-routing regression guards ──────────────────────────
    //
    // Two failure modes were reported by manual testing of orchestrated
    // agents on an earlier runtime:
    //
    //  1. `apollia-os run <orchestrated>` returning
    //     `[NO_HANDLER] agent has neither @skill nor @on_message handler`,
    //     caused by AIPProductionBackend always dispatching to
    //     `execute_direct` (which goes through __apollia_dispatch__) even
    //     for orchestrated manifests. Fix: branch on
    //     `manifest.execution_mode == "orchestrated"` and call
    //     `engine.execute` instead.
    //
    //  2. `apollia-os run <orchestrated>` returning
    //     `[NO_LLM] Orchestrated mode requires a configured LLM
    //     (use with_reasoner())`, caused by the same fix routing to
    //     `engine.execute` without wiring a Reasoner. Fix:
    //     `wire_engine_with_llm` chains `.with_llm_router(...).
    //     with_reasoner(model, ...)` whenever the LlmRouter exposes
    //     a `precise` backend.
    //
    // The tests below cover the second mode at the engine boundary; the
    // first mode is enforced by the explicit branch in
    // `AIPProductionBackend::execute` and surfaces here as NO_LLM
    // (engine-level error) rather than NO_HANDLER (SDK-level error) when no
    // Reasoner is wired.

    /// Without an LlmRouter, the engine returned by `wire_engine_with_llm`
    /// has no Reasoner. Sanity check on the fast path.
    #[test]
    fn wire_engine_without_router_leaves_no_reasoner() {
        let engine = ORIAEngine::new();
        assert!(!engine.has_reasoner());

        let wired = wire_engine_with_llm(engine, None, "agent-under-test", 20);
        assert!(
            !wired.has_reasoner(),
            "wire_engine_with_llm(None) must not synthesise a Reasoner"
        );
    }

    /// `LlmRouter::empty()` carries no `[llm.routing]` section, so
    /// `route_precise()` errors out. The router is still attached (so
    /// step LLM calls would surface a descriptive error rather than
    /// silently noop), but no Reasoner is configured. Orchestrated
    /// execution will fail with NO_LLM at engine.execute() time, which
    /// is exactly what the second failure mode reported.
    #[test]
    fn wire_engine_with_empty_router_attaches_router_but_no_reasoner() {
        let engine = ORIAEngine::new();
        let router = Arc::new(LlmRouter::empty());

        let wired = wire_engine_with_llm(engine, Some(router), "agent-under-test", 20);

        assert!(
            !wired.has_reasoner(),
            "an empty LlmRouter must not produce a Reasoner - \
             would yield NO_LLM at runtime"
        );
    }

    /// End-to-end behaviour: an orchestrated agent submitted to an
    /// engine with no Reasoner must surface NO_LLM (not NO_HANDLER).
    ///
    /// Before the fix, `AIPProductionBackend::execute` routed every task
    /// through `execute_direct`, which goes through the SDK
    /// `__apollia_dispatch__`. An orchestrated manifest fell into the
    /// `failed("NO_HANDLER", ...)` branch of `dispatch_task`. After the
    /// fix, orchestrated tasks hit `engine.execute` directly; missing
    /// LLM is the only remaining failure mode and it has a stable code.
    /// Full-stack guard: builds a real `AIPProductionBackend` (PyO3 bridge
    /// + manifest + engine wiring) for an agent whose Python manifest
    /// declares `execution_mode = "orchestrated"`, and submits a task.
    ///
    /// The forged Python agent's `__apollia_dispatch__` returns a unique
    /// sentinel code (`SDK_DISPATCH_REACHED`) when invoked. If
    /// `AIPProductionBackend::execute` regresses to routing orchestrated
    /// tasks through `execute_direct` (the original mode 1), the result
    /// will carry that sentinel. If the routing stays correct but the
    /// Reasoner is missing (mode 2 wiring), the result will be ORIA's
    /// `NO_LLM`. Anything else means a regression.
    ///
    /// This is the test that, had it existed pre-patch, would have
    /// surfaced both modes immediately.
    #[tokio::test]
    async fn aip_production_backend_routes_orchestrated_to_oria_full_stack() {
        use apollia_aip::validator::validate_agent;
        use apollia_runtime::eventbus::EventBusSender;
        use pyo3::types::PyModule;
        use std::ffi::CString;

        // 1. Forge a minimal Python agent class with the orchestrated
        //    manifest shape the validator + bridge expect. We don't
        //    import the real Apollia SDK from this Rust test (it would
        //    require a pip-installed environment). The forged class has
        //    the exact attributes the validator and bridge introspect:
        //    `__apollia_manifest__` (dict) and async
        //    `__apollia_dispatch__`. The dispatch returns a sentinel so
        //    we can tell whether we ended up there (= routing regression).
        let code = r#"
class A:
    __apollia_manifest__ = {
        "name": "regression-orch-full",
        "version": "0.1.0",
        "description": "BUG-004 full-stack regression guard",
        "execution_mode": "orchestrated",
        "system_prompt": "Plan a tiny task and answer.",
        "tools_required": [],
    }

    async def __apollia_dispatch__(self, task, ctx):
        # Reaching this means AIPProductionBackend routed the task to
        # execute_direct (which goes through __apollia_dispatch__) - i.e.
        # BUG-004 mode 1 has regressed.
        return {
            "task_id": task.get("task_id", ""),
            "status": "failed",
            "output": [],
            "error": {
                "code": "SDK_DISPATCH_REACHED",
                "message": "orchestrated task was routed through SDK dispatch",
                "details": None,
            },
            "artifacts": [],
            "input_required_data": None,
        }

agent = A()
"#;

        let py_agent: pyo3::Py<pyo3::PyAny> = Python::with_gil(|py| {
            let code_c = CString::new(code).expect("test code contains NUL byte");
            let module = PyModule::from_code(
                py,
                &code_c,
                c"bug004_full_stack_test.py",
                c"bug004_full_stack_test",
            )
            .expect("failed to create test module");
            module
                .getattr("agent")
                .expect("forged module must expose `agent`")
                .into()
        });

        // 2. Validate via apollia_aip: confirms the manifest shape is
        //    well-formed and produces a ValidatedAgent with `execution_mode`
        //    correctly set.
        let validated = validate_agent(&py_agent).expect("validation must succeed");
        assert_eq!(
            validated.manifest.execution_mode, "orchestrated",
            "forged manifest must surface execution_mode = orchestrated"
        );
        let manifest_snapshot = validated.manifest.clone();

        // 3. Build the bridge.
        let bridge = Arc::new(AIPBridge::new(validated).expect("bridge construction failed"));

        // 4. Assemble a minimal AIPProductionBackend. Most optional
        //    components (tool registry, audit trail, A2A invoker, llm
        //    router, ...) are intentionally None so we exercise the
        //    no-LLM path. The branching we care about (execution_mode
        //    routing in `execute()`) is independent of these.
        let (event_bus, _event_rx): (EventBusSender, _) =
            apollia_runtime::eventbus::EventBus::new();
        let tmp = std::env::temp_dir().join("apollia-bug004-test");
        let _ = std::fs::create_dir_all(&tmp);
        let backend = AIPProductionBackend {
            bridge,
            agent_id: "regression-orch-full".to_string(),
            manifest: manifest_snapshot,
            allowed_tools: vec![],
            llm_router: None,
            event_bus,
            pending_approvals: None,
            plan_gates: None,
            plan_cache: None,
            task_repository: None,
            tool_registry: None,
            audit_trail: None,
            memory_namespace: None,
            memory_base_dir: tmp.clone(),
            user_memory_write: false,
            a2a_invoker: None,
            tools_config: apollia_core::ToolsConfig::default(),
            datasources_declared: vec![],
            templates_declared: vec![],
            agent_dir: Some(tmp.clone()),
            secrets_declared: vec![],
            secrets_data_dir: None,
            mcp_handle: None,
        };

        let task = AIPTask {
            task_id: "task-bug004-full".to_string(),
            ..AIPTask::default()
        };

        // 5. Run the full path. Must not panic, must produce an
        //    AIPResult, must NOT carry our SDK_DISPATCH_REACHED sentinel.
        let result = backend
            .execute(task)
            .await
            .expect("AIPProductionBackend::execute must not return Err");

        // 6. Assertions: routing went to ORIA (engine.execute), the
        //    missing Reasoner surfaced cleanly as NO_LLM, and the SDK
        //    dispatch path was never touched.
        assert_eq!(
            result.status,
            TaskStatus::Failed,
            "without an LLM router, orchestrated agents must Failed-fast"
        );
        let err = result.error.expect("Failed status must carry an AIPError");
        assert_ne!(
            err.code, "SDK_DISPATCH_REACHED",
            "regression: orchestrated task was routed through SDK dispatch \
             instead of engine.execute (BUG-004 mode 1)"
        );
        assert_ne!(
            err.code, "NO_HANDLER",
            "regression: orchestrated task fell through to SDK NO_HANDLER \
             branch (BUG-004 mode 1)"
        );
        assert!(
            err.code.to_uppercase().contains("LLM") || err.message.to_lowercase().contains("llm"),
            "expected an LLM-related error code, got code={} message={}",
            err.code,
            err.message
        );
    }

    #[tokio::test]
    async fn orchestrated_without_reasoner_yields_no_llm_not_no_handler() {
        let engine = ORIAEngine::new();
        assert!(!engine.has_reasoner());

        let manifest = AgentManifest {
            name: "regression-orchestrated".to_string(),
            version: "0.1.0".to_string(),
            description: "BUG-004 regression guard".to_string(),
            tools_required: vec![],
            tools_optional: vec![],
            supports_streaming: false,
            supports_a2a: false,
            memory_namespace: None,
            shared_memory_namespaces: vec![],
            max_concurrent_tasks: 1,
            step_budget: None,
            network_allowlist: None,
            dangerous_tools_allowed: false,
            tags: vec![],
            skills: vec![],
            execution_mode: "orchestrated".to_string(),
            supports_mailbox: false,
            mailbox_allowlist: None,
            system_prompt: Some("You are a planning assistant.".to_string()),
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
        };

        struct MockOrchestratedAgent {
            manifest: AgentManifest,
        }
        impl AIPAgent for MockOrchestratedAgent {
            fn manifest(&self) -> AgentManifest {
                self.manifest.clone()
            }
        }
        let agent = MockOrchestratedAgent {
            manifest: manifest.clone(),
        };

        let task = AIPTask {
            task_id: "test-task-bug004".to_string(),
            ..AIPTask::default()
        };

        let result = engine.execute(task, &agent).await;

        assert_eq!(
            result.status,
            TaskStatus::Failed,
            "missing Reasoner must produce a Failed result"
        );
        let err = result.error.expect("Failed status must carry an AIPError");
        assert_ne!(
            err.code, "NO_HANDLER",
            "regression: orchestrated agents must not fall through to \
             SDK dispatch when the engine has no Reasoner"
        );
        assert!(
            err.message.to_lowercase().contains("llm") || err.code.to_uppercase().contains("LLM"),
            "expected an LLM-related error, got code={} message={}",
            err.code,
            err.message
        );
    }
}

#[cfg(test)]
mod orchestrated_step_args_master_proof {
    //! End-to-end proof for the orchestrated step-argument contract.
    //!
    //! Drives the orchestrated `ActorLoop` through the production `OriaToolProxy`
    //! over a real governed `ToolProxy`, executing a native tool that requires
    //! structured arguments, and asserts the file was written, the invocation was
    //! audited, and the tool-call budget was consumed.

    use std::pin::Pin;
    use std::sync::Arc;
    use std::time::Duration;

    use super::{OriaToolProxy, RouterModel};
    use apollia_aip::context::{DispatcherExecutor, ToolProxy, ToolProxyConfig};
    use apollia_core::plan::PlanStep;
    use apollia_core::{SandboxProfile, StepBudgetConfig, TaskStatus};
    use apollia_llm::{CompletionModel, LlmRouter};
    use apollia_oria::actor::{ActorLoop, StepDeps};
    use apollia_oria::budget::StepBudget;
    use apollia_oria::plan::ExecutionPlan;
    use apollia_oria::plan_repository::PlanRepository;
    use apollia_oria::reasoner::Reasoner;
    use apollia_oria::ResilienceLayer;
    use apollia_tools::{
        ToolDescriptor, ToolDispatcher, ToolExecutionError, ToolExecutor, ToolKind,
        ToolRegistryHandle,
    };
    use serde_json::Value;

    /// Native tool with a structured (path, content) input contract: writes the
    /// content to the path. Stands in for `file_write`/`bash`/`http` in the test.
    struct WriteNoteExecutor;

    impl ToolExecutor for WriteNoteExecutor {
        fn name(&self) -> &str {
            "write_note"
        }
        fn execute(
            &self,
            input: Value,
        ) -> Pin<Box<dyn std::future::Future<Output = Result<Value, ToolExecutionError>> + Send + '_>>
        {
            Box::pin(async move {
                let path = input.get("path").and_then(Value::as_str).ok_or_else(|| {
                    ToolExecutionError::InvalidInput {
                        message: "missing 'path'".to_string(),
                    }
                })?;
                let content = input
                    .get("content")
                    .and_then(Value::as_str)
                    .ok_or_else(|| ToolExecutionError::InvalidInput {
                        message: "missing 'content'".to_string(),
                    })?;
                std::fs::write(path, content).map_err(|e| ToolExecutionError::ExecutionFailed {
                    code: "io_error".to_string(),
                    message: e.to_string(),
                })?;
                Ok(Value::String(format!("wrote {} bytes", content.len())))
            })
        }
    }

    fn write_note_descriptor() -> ToolDescriptor {
        ToolDescriptor {
            name: "write_note".to_string(),
            version: "1.0.0".to_string(),
            description: "Write content to a file path".to_string(),
            kind: ToolKind::Native,
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string" },
                    "content": { "type": "string" }
                },
                "required": ["path", "content"]
            }),
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

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_orchestrated_native_tool_with_structured_args_executes_and_audits() {
        // GIVEN a real governed ToolProxy exposing a structured-args native tool
        let tmp = std::env::temp_dir().join(format!(
            "apollia_stepargs_{}_{}.txt",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        let _ = std::fs::remove_file(&tmp);

        let registry = ToolRegistryHandle::start();
        registry
            .register(write_note_descriptor())
            .await
            .expect("register descriptor");
        let audit_db = std::env::temp_dir().join(format!(
            "apollia_stepargs_audit_{}.db",
            uuid::Uuid::new_v4()
        ));
        let audit = apollia_tools::AuditTrailHandle::open(&audit_db)
            .await
            .expect("open audit trail");
        let dispatcher = Arc::new(ToolDispatcher::new(vec![Box::new(WriteNoteExecutor)]));
        let proxy = ToolProxy::new(ToolProxyConfig {
            registry: registry.clone(),
            audit: audit.clone(),
            executor: Arc::new(DispatcherExecutor::new(dispatcher)),
            allowed_tools: vec!["write_note".to_string()],
            agent_id: "orchestrated-agent".to_string(),
            task_id: "task-stepargs".to_string(),
            run_id: None,
        });
        let oria_proxy = OriaToolProxy { proxy };

        // AND a plan whose single tool step carries structured plan-time args
        // (path A output: the persisted plan is fully specified).
        let mut step = PlanStep::new("s1", "write the greeting to the note file");
        step.tool_hint = Some("write_note".to_string());
        step.args = Some(serde_json::json!({
            "path": tmp.to_str().expect("utf-8 path"),
            "content": "hello orchestrated"
        }));
        let plan = ExecutionPlan {
            plan_id: "p-stepargs".to_string(),
            task_id: "task-stepargs".to_string(),
            steps: vec![step],
        };

        let db = PlanRepository::new(":memory:").expect("in-memory plan db");
        db.insert_plan(&plan, "orchestrated-agent")
            .expect("insert_plan");
        db.insert_steps(&plan.plan_id, &plan.steps)
            .expect("insert_steps");

        let (bus_tx, _bus_rx) = tokio::sync::broadcast::channel(64);
        let manifest = serde_json::from_str(
            r#"{"name":"orch","version":"0.1.0","description":"t","tools_required":["write_note"]}"#,
        )
        .expect("minimal manifest");
        let mut actor = ActorLoop::new(plan, 0, db, bus_tx, manifest);

        let cap = StepBudgetConfig {
            max_steps: 10,
            max_tool_calls: 10,
            wall_clock_secs: 60,
        };
        let budget = StepBudget::from_capped(&cap, &cap);
        let router = LlmRouter::empty();
        let reasoner = Reasoner::new(
            Arc::new(RouterModel(Arc::new(LlmRouter::empty()))) as Arc<dyn CompletionModel>,
            10,
        );
        let resilience = ResilienceLayer::default();

        // WHEN the orchestrated ActorLoop executes the plan
        let result = actor
            .execute(
                StepDeps {
                    tool_proxy: &oria_proxy,
                    llm_router: &router,
                    budget: &budget,
                    reasoner: &reasoner,
                },
                &resilience,
            )
            .await;

        // THEN the plan completed and the native tool ran with the structured args
        assert_eq!(
            result.status,
            TaskStatus::Completed,
            "orchestrated run should complete: {:?}",
            result.error
        );
        let written = std::fs::read_to_string(&tmp).expect("note file must exist");
        assert_eq!(written, "hello orchestrated");

        // AND the invocation was recorded in the audit trail (governance holds)
        tokio::time::sleep(Duration::from_millis(150)).await;
        let records = audit.query_last(1).await;
        assert_eq!(records.len(), 1, "one audited invocation expected");
        assert_eq!(records[0].tool_name, "write_note");
        assert_eq!(records[0].agent_id, "orchestrated-agent");
        assert!(records[0].success);

        // AND the tool-call budget was consumed (guardrail applies)
        assert_eq!(budget.tool_calls_left(), 9);

        let _ = std::fs::remove_file(&tmp);
        registry.shutdown().await;
        audit.shutdown().await;
    }
}
