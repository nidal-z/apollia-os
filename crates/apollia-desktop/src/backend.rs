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
use apollia_core::{AIPError, AIPResult, AIPTask, AgentManifest, PendingApprovals, TaskStatus};
use apollia_llm::{
    CompletionModel, CompletionRequest, CompletionResponse, LlmError, LlmRouter,
    ObservabilityConfig, StepBudgetView, ToolCallHelper, ToolInvoker,
};
use apollia_memory::manager::MemoryManager;
use apollia_oria::budget::StepBudget;
use apollia_oria::engine::{AgentRunner, ORIAEngine};
use apollia_runtime::api::routes_agents::{AgentBackendFactory, AgentLoader};
use apollia_runtime::coordinator::{DynBackend, ExecutionBackend};
use apollia_runtime::eventbus::EventBusSender;
use apollia_tools::tools::ask_user::PendingUserInputs;
use apollia_tools::{
    build_native_dispatcher, load_governance_snapshot, AuditTrailHandle, NativeDispatcherConfig,
    TaskRepository, ToolRegistryHandle,
};
use futures::stream;
use pyo3::prelude::*;

// ─── Agent loader ─────────────────────────────────────────────────────────────

/// Real agent loader using AIPLoader + validate_agent (ADR-019).
pub struct AIPAgentLoader;

impl AgentLoader for AIPAgentLoader {
    fn load_and_validate(&self, path: &Path) -> Result<AgentManifest, String> {
        let module = apollia_aip::loader::load_agent_module(path).map_err(|e| e.to_string())?;
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
        let user_memory_write = self.user_memory_write;
        let pending_user_inputs = self.pending_user_inputs.clone();

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
            let dispatcher = Arc::new(build_native_dispatcher(&NativeDispatcherConfig {
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
                disabled_tools: snapshot.disabled_tools,
                brave_api_key: snapshot.brave_api_key,
                web_search_config: apollia_core::WebSearchConfig::default(),
                web_read_config: apollia_core::WebReadConfig::default(),
                governance_db_path: Some(
                    governance_base.join(apollia_tools::GOVERNANCE_DB_FILENAME),
                ),
            }));

            let tool_proxy = match (tool_registry.as_ref(), audit_trail.as_ref()) {
                (Some(registry), Some(audit)) => Some(ToolProxy::new(
                    registry.clone(),
                    audit.clone(),
                    Arc::new(DispatcherExecutor::new(dispatcher)),
                    allowed_tools,
                    agent_id.clone(),
                    task.task_id.clone(),
                )),
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
                // Always wire the global `__user__` store so every agent can
                // recall keys written by the onboarding agent (read fallback).
                let user_manager = MemoryManager::new(
                    &memory_base_dir,
                    Some("__user__".to_string()),
                    vec![],
                );
                let iface = MemoryInterface::new(
                    manager,
                    eff_ns,
                    agent_id.clone(),
                    user_memory_write,
                    Some(user_manager),
                )?;
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
                    None, // mailbox — not wired in desktop task mode
                    agent_id,
                    false, // supports_a2a — desktop backend does not participate in A2A routing
                    None,  // user_context — task mode, not chat
                    None,  // a2a_delegate — not available in desktop backend
                    None,  // a2a_invoker — not available in desktop backend
                    user_memory_write, // user_memory_writable — manifest-controlled
                );
                if let Some((session_id, message_id)) = chat_target {
                    ctx = ctx.with_chat_target(session_id, message_id);
                }
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

        let result: Result<AIPProductionBackend, String> = (|| {
            let module =
                apollia_aip::loader::load_agent_module(agent_path).map_err(|e| e.to_string())?;
            let validated =
                apollia_aip::validator::validate_agent(&module).map_err(|e| e.to_string())?;
            let allowed_tools = validated.manifest.tools_required.clone();
            let memory_namespace = validated.manifest.memory_namespace.clone();
            let user_memory_write = validated.manifest.user_memory_write;
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

        // 2. Load Python module + create bridge
        let module =
            apollia_aip::loader::load_agent_module(&install_path).map_err(|e| e.to_string())?;
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
