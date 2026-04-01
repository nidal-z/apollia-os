//! `apollia-os start` — start the runtime in foreground.
//!
//! Uses the Supervisor for ordered startup (EventBus → AgentRegistry → TaskRouter
//! → APIServer) with timeout and rollback on failure. Shutdown is handled by the
//! ShutdownController with graceful drain.

use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use std::time::Instant;

use apollia_aip::bridge::AIPBridge;
use apollia_aip::context::{RuntimeContext, ToolExecutor, ToolProxy};
use apollia_aip::memory::MemoryInterface;
use apollia_core::{AIPResult, AIPTask, AgentManifest, PendingApprovals, RuntimeEvent, TaskStatus};
use apollia_llm::{
    CompletionModel, CompletionRequest, CompletionResponse, LlmError, LlmRouter,
    ObservabilityConfig, StepBudgetView, ToolCallHelper, ToolInvoker,
};
use apollia_memory::manager::MemoryManager;
use apollia_oria::budget::StepBudget;
use apollia_oria::engine::{AgentRunner, ORIAEngine};
use apollia_runtime::a2a::make_delegate_fn;
use apollia_runtime::api::routes_agents::{AgentBackendFactory, AgentLoader};
use apollia_runtime::api::APIServerConfig;
use apollia_runtime::coordinator::{DynBackend, ExecutionBackend};
use apollia_runtime::eventbus::EventBusSender;
use apollia_runtime::registry::AgentRegistryHandle;
use apollia_runtime::router::TaskRouterHandle;
use apollia_runtime::shutdown::{ShutdownConfig, ShutdownController};
use apollia_runtime::supervisor::{Supervisor, SupervisorConfig};
use apollia_tools::{AuditTrailHandle, TaskRepository, ToolRegistryHandle};
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
    #[error("runtime already running on {address} — use `apollia-os stop` first")]
    AlreadyRunning { address: String },
}

/// Real agent loader using AIPLoader + validate_agent (ADR-019).
///
/// Loads a Python module via PyO3, validates AIP duck typing, and returns
/// the deserialized [`AgentManifest`].
struct AIPAgentLoader;

impl AgentLoader for AIPAgentLoader {
    fn load_and_validate(&self, path: &Path) -> Result<AgentManifest, String> {
        let module = apollia_aip::loader::load_agent_module(path).map_err(|e| e.to_string())?;
        let validated =
            apollia_aip::validator::validate_agent(&module).map_err(|e| e.to_string())?;
        Ok(validated.manifest)
    }
}

// ─────────────────────────────────────────────────────────────
// AIPChatAgentRunner — concrete ChatAgentRunner for Chat Agent mode.
// ─────────────────────────────────────────────────────────────

/// Concrete [`ChatAgentRunner`] implementation using PyO3 + AIPBridge.
///
/// Loads the Python agent from `data_dir/agents/<name>/`, validates AIP duck
/// typing, builds a `RuntimeContext` with tools/memory/LLM, and calls `run()`.
///
/// Uses the same `OnceLock` pattern as [`ProductionBackendFactory`] to access
/// runtime handles created inside `supervisor.start()`.
struct AIPChatAgentRunner {
    /// EventBus sender — populated after supervisor.start().
    event_bus: Arc<std::sync::OnceLock<EventBusSender>>,
    /// LLM router — populated after supervisor.start().
    llm_router: Arc<std::sync::OnceLock<Option<Arc<LlmRouter>>>>,
    /// Tool registry — populated after supervisor.start().
    tool_registry: Arc<std::sync::OnceLock<ToolRegistryHandle>>,
    /// Audit trail — populated after supervisor.start().
    audit_trail: Arc<std::sync::OnceLock<AuditTrailHandle>>,
    /// Global user memory repository — populated after supervisor.start().
    user_memory: Arc<
        std::sync::OnceLock<
            Option<Arc<std::sync::Mutex<apollia_memory::user_memory::UserMemoryRepository>>>,
        >,
    >,
    /// Base data directory (e.g. `~/.apollia/`).
    data_dir: PathBuf,
}

#[async_trait::async_trait]
impl apollia_runtime::chat::ChatAgentRunner for AIPChatAgentRunner {
    async fn run_agent(&self, agent_name: &str, task: AIPTask) -> Result<AIPResult, String> {
        let agent_path = self.data_dir.join("agents").join(agent_name);

        // Load and validate agent via PyO3
        let module =
            apollia_aip::loader::load_agent_module(&agent_path).map_err(|e| e.to_string())?;
        let validated =
            apollia_aip::validator::validate_agent(&module).map_err(|e| e.to_string())?;
        let manifest = validated.manifest.clone();
        let bridge = Arc::new(AIPBridge::new(validated).map_err(|e| e.to_string())?);

        // Get runtime handles from OnceLocks
        let event_bus = self
            .event_bus
            .get()
            .cloned()
            .ok_or("event bus not initialized — chat agent called before runtime ready")?;
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

        let tool_proxy: Option<ToolProxy> = match (tool_registry.as_ref(), audit_trail.as_ref()) {
            (Some(registry), Some(audit)) => Some(ToolProxy::new(
                registry.clone(),
                audit.clone(),
                Arc::new(NativeToolExecutor::new()),
                allowed_tools,
                agent_name.to_string(),
                task.task_id.clone(),
            )),
            _ => None,
        };

        let memory_base_dir = self.data_dir.join("memory");
        let memory_interface: Option<MemoryInterface> =
            manifest.memory_namespace.as_deref().and_then(|ns| {
                let manager = MemoryManager::new(&memory_base_dir, Some(ns.to_string()), vec![]);
                MemoryInterface::new(manager, ns.to_string(), agent_name.to_string(), false, None)
            });

        let supports_a2a = manifest.supports_a2a;

        // Build user_context from UserMemoryRepository (chat mode only).
        let user_context = self
            .user_memory
            .get()
            .and_then(|opt| opt.as_ref())
            .and_then(|repo_mutex| {
                let repo = repo_mutex.lock().ok()?;
                build_user_context_from_repo(&repo)
            });

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
                None, // mailbox — not available in chat runner context
                agent_name.to_string(),
                supports_a2a,
                user_context,
                None,  // a2a_delegate — chat runner does not participate in A2A delegation
                None,  // a2a_invoker — not available in chat mode
                false, // user_memory_read_only — direct start, not A2A
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
/// Recalls up to 50 entries per category and returns a `HashMap` with keys
/// `"preferences"`, `"habits"`, `"context"`, each mapping to a list of `(key, value)` pairs.
/// Returns `None` if all categories are empty.
fn build_user_context_from_repo(
    repo: &apollia_memory::user_memory::UserMemoryRepository,
) -> Option<std::collections::HashMap<String, Vec<(String, String)>>> {
    use apollia_memory::user_memory::UserMemoryCategory;

    const MAX_ENTRIES_PER_CATEGORY: usize = 50;

    let categories = [
        ("preferences", UserMemoryCategory::Preferences),
        ("habits", UserMemoryCategory::Habits),
        ("context", UserMemoryCategory::Context),
    ];

    let mut map = std::collections::HashMap::new();
    let mut total = 0usize;

    for (label, cat) in &categories {
        let entries = repo
            .recall(*cat, MAX_ENTRIES_PER_CATEGORY)
            .unwrap_or_default();
        total += entries.len();
        map.insert(
            (*label).to_string(),
            entries.into_iter().map(|e| (e.key, e.value)).collect(),
        );
    }

    if total == 0 {
        None
    } else {
        Some(map)
    }
}

/// Fallback backend — only used when agent loading fails at start time.
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
            "tool '{name}' invocation via LLM loop not wired — use ctx.tools directly"
        ))
    }
}

// ─────────────────────────────────────────────────────────────
// ─────────────────────────────────────────────────────────────
// NativeToolExecutor — bridges sync ToolExecutor trait to async native tools
// ─────────────────────────────────────────────────────────────

/// Production `ToolExecutor` — dispatches tool calls to native Apollia tools.
///
/// Uses `block_in_place` to bridge the synchronous `ToolExecutor::execute` trait
/// to the async tool implementations (`BashExecutor`, `FileIo`).
///
/// On macOS dev mode, `FileIo` uses `HOME` as its sandbox root — consistent with
/// `BashExecutor` dev mode bypass (ADR-012). All paths under HOME are reachable.
struct NativeToolExecutor {
    home_dir: PathBuf,
}

impl NativeToolExecutor {
    fn new() -> Self {
        let home_dir = std::env::var("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| std::env::temp_dir());
        Self { home_dir }
    }
}

impl ToolExecutor for NativeToolExecutor {
    fn execute(
        &self,
        tool_name: &str,
        input: serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        let tool = tool_name.to_string();
        let home = self.home_dir.clone();

        tokio::task::block_in_place(move || {
            tokio::runtime::Handle::current().block_on(async move {
                match tool.as_str() {
                    "bash_executor" => native_exec_bash(input).await,
                    "file_read" => native_exec_file_read(input, &home).await,
                    "file_write" => native_exec_file_write(input, &home).await,
                    "file_list" => native_exec_file_list(input, &home).await,
                    other => Err(format!("tool not found: {other}")),
                }
            })
        })
    }
}

/// Execute `bash_executor` from a raw JSON input dict.
async fn native_exec_bash(input: serde_json::Value) -> Result<serde_json::Value, String> {
    use apollia_tools::tools::bash_executor::{BashExecutor, BashInput};

    let command = input
        .get("command")
        .and_then(|v| v.as_str())
        .ok_or("bash_executor: missing 'command' field")?
        .to_string();
    let timeout_secs = input
        .get("timeout_seconds")
        .or_else(|| input.get("timeout_secs"))
        .and_then(|v| v.as_u64())
        .unwrap_or(30);

    let bash_input = BashInput {
        command,
        timeout_secs,
        working_dir: None,
    };
    let result = BashExecutor::new()
        .run(bash_input)
        .await
        .map_err(|e| e.to_string())?;

    Ok(serde_json::json!({
        "stdout": result.stdout,
        "stderr": result.stderr,
        "exit_code": result.exit_code,
        "duration_ms": result.duration_ms,
    }))
}

/// Execute `file_read` from a raw JSON input dict.
async fn native_exec_file_read(
    input: serde_json::Value,
    home_dir: &Path,
) -> Result<serde_json::Value, String> {
    use apollia_tools::tools::file_read::{FileRead, FileReadInput};

    let tool = FileRead::new(home_dir.to_path_buf()).map_err(|e| e.to_string())?;
    let file_input: FileReadInput =
        serde_json::from_value(input).map_err(|e| format!("file_read: invalid arguments: {e}"))?;
    let output = tool.run(file_input).await.map_err(|e| e.to_string())?;
    serde_json::to_value(&output).map_err(|e| e.to_string())
}

/// Execute `file_write` from a raw JSON input dict.
async fn native_exec_file_write(
    input: serde_json::Value,
    home_dir: &Path,
) -> Result<serde_json::Value, String> {
    use apollia_tools::tools::file_write::{FileWrite, FileWriteInput};

    let tool = FileWrite::new(home_dir.to_path_buf()).map_err(|e| e.to_string())?;
    let file_input: FileWriteInput =
        serde_json::from_value(input).map_err(|e| format!("file_write: invalid arguments: {e}"))?;
    tool.run(file_input).await.map_err(|e| e.to_string())?;
    Ok(serde_json::json!({ "written": true }))
}

/// Execute `file_list` from a raw JSON input dict.
async fn native_exec_file_list(
    input: serde_json::Value,
    home_dir: &Path,
) -> Result<serde_json::Value, String> {
    use apollia_tools::tools::file_list::{FileList, FileListInput};

    let tool = FileList::new(home_dir.to_path_buf()).map_err(|e| e.to_string())?;
    let file_input: FileListInput =
        serde_json::from_value(input).map_err(|e| format!("file_list: invalid arguments: {e}"))?;
    let output = tool.run(file_input).await.map_err(|e| e.to_string())?;
    serde_json::to_value(&output).map_err(|e| e.to_string())
}

// Real per-agent execution backend (AIPBridge + RuntimeContext)
// ─────────────────────────────────────────────────────────────

/// Per-agent backend that calls Python via `AIPBridge`.
///
/// Created once per agent at start time by `ProductionBackendFactory`.
/// All fields are `Arc`-wrapped — cloning is cheap.
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
    /// Namespace mémoire déclaré dans le manifest (ex: "apollia-reviewer").
    memory_namespace: Option<String>,
    /// Répertoire racine des fichiers mémoire (ex: `~/.apollia/memory/`).
    memory_base_dir: PathBuf,
    /// Indique si l'agent supporte le protocole A2A (depuis le manifest).
    supports_a2a: bool,
    /// Fonction de délégation A2A type-erasée — `None` si non disponible.
    a2a_delegate: Option<apollia_runtime::a2a::A2aDelegateFn>,
    /// Orchestrateur A2A de haut niveau — `None` si registry ou router non initialisés.
    a2a_invoker: Option<Arc<apollia_runtime::a2a::A2AInvoker>>,
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
            supports_a2a: self.supports_a2a,
            pending_approvals: self.pending_approvals.clone(),
            task_repository: self.task_repository.clone(),
            a2a_delegate: self.a2a_delegate.clone(),
            a2a_invoker: self.a2a_invoker.clone(),
        }
    }
}

// ─────────────────────────────────────────────────────────────
// BridgeRunner — implements AgentRunner for ORIAEngine::execute_direct
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
    allowed_tools: Vec<String>,
    tool_registry: Option<ToolRegistryHandle>,
    audit_trail: Option<AuditTrailHandle>,
    memory_namespace: Option<String>,
    memory_base_dir: PathBuf,
    /// Whether the agent declared A2A support in its manifest.
    supports_a2a: bool,
    /// Type-erased delegation function — `None` if not available at runner level.
    a2a_delegate: Option<apollia_runtime::a2a::A2aDelegateFn>,
    /// High-level A2A invoker — `None` if not available.
    a2a_invoker: Option<Arc<apollia_runtime::a2a::A2AInvoker>>,
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
        let supports_a2a = self.supports_a2a;
        let a2a_delegate = self.a2a_delegate.clone();
        let a2a_invoker = self.a2a_invoker.clone();

        Box::pin(async move {
            let router_for_helper = llm_router
                .clone()
                .unwrap_or_else(|| Arc::new(LlmRouter::empty()));
            let tool_helper = Arc::new(ToolCallHelper::new(
                Arc::new(RouterModel(router_for_helper)),
                Arc::new(NoopToolInvoker),
            ));

            let tool_proxy: Option<ToolProxy> = match (tool_registry.as_ref(), audit_trail.as_ref())
            {
                (Some(registry), Some(audit)) => Some(ToolProxy::new(
                    registry.clone(),
                    audit.clone(),
                    Arc::new(NativeToolExecutor::new()),
                    allowed_tools,
                    agent_id.clone(),
                    task.task_id.clone(),
                )),
                _ => {
                    tracing::warn!(
                        agent = %agent_id,
                        "ToolProxy not available — tool registry or audit trail missing; \
                         agent will use its own fallback for tool calls"
                    );
                    None
                }
            };

            let memory_interface: Option<MemoryInterface> =
                memory_namespace.as_deref().and_then(|ns| {
                    let manager =
                        MemoryManager::new(&memory_base_dir, Some(ns.to_string()), vec![]);
                    MemoryInterface::new(manager, ns.to_string(), agent_id.clone(), false, None)
                });

            let ctx: PyObject = Python::with_gil(|py| {
                let ctx = RuntimeContext::new_with_llm(
                    llm_router,
                    Arc::new(StepBudgetView::unlimited()),
                    tool_helper,
                    Arc::new(ObservabilityConfig::default()),
                    event_bus,
                    agent_id.clone().into(),
                    tool_proxy,
                    memory_interface,
                    None, // mailbox — not wired in task mode
                    agent_id,
                    supports_a2a,
                    None, // user_context — task mode, not chat
                    a2a_delegate,
                    a2a_invoker,
                    false, // user_memory_read_only — direct start, not A2A
                );
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
            supports_a2a: self.supports_a2a,
            a2a_delegate: self.a2a_delegate.clone(),
            a2a_invoker: self.a2a_invoker.clone(),
        };

        // Build a per-task ORIAEngine wired with HITL components (execute_direct).
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

// ─────────────────────────────────────────────────────────────
// Factory — creates one AIPProductionBackend per agent at `agent start`
// ─────────────────────────────────────────────────────────────

/// Creates a real `AIPProductionBackend` per agent (ADR-019 extension).
///
/// Called once from `POST /api/v1/agents` — loads Python, validates AIP duck typing,
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
    task_repository: Arc<std::sync::OnceLock<Arc<TaskRepository>>>,
    /// Agent registry handle — populated after supervisor.start().
    registry: Arc<std::sync::OnceLock<AgentRegistryHandle>>,
    /// Task router handle — populated after supervisor.start().
    router: Arc<std::sync::OnceLock<TaskRouterHandle<DynBackend>>>,
}

impl AgentBackendFactory for ProductionBackendFactory {
    fn create_for_agent(&self, agent_path: &Path, manifest: &AgentManifest) -> DynBackend {
        let agent_id = manifest.name.clone();

        // Retrieve the lazily-initialized event bus and LLM router.
        let event_bus = match self.event_bus.get() {
            Some(bus) => bus.clone(),
            None => {
                tracing::error!(
                    agent = %agent_id,
                    "event bus not initialized — factory called before supervisor.start() returned"
                );
                return DynBackend::new(NoopBackend);
            }
        };
        let llm_router = self.llm_router.get().cloned().flatten();
        let tool_registry = self.tool_registry.get().cloned();
        let audit_trail = self.audit_trail.get().cloned();
        let pending_approvals = self.pending_approvals.get().cloned();
        let task_repository = self.task_repository.get().cloned();

        // Build A2A delegate and invoker if registry + router are available.
        let (a2a_delegate, a2a_invoker) = match (
            self.registry.get().cloned(),
            self.router.get().cloned(),
        ) {
            (Some(registry), Some(router)) => {
                let delegate =
                    make_delegate_fn(registry.clone(), router.clone(), event_bus.clone());
                let invoker = Arc::new(apollia_runtime::a2a::A2AInvoker::new(
                    registry,
                    router,
                    event_bus.clone(),
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
            let module =
                apollia_aip::loader::load_agent_module(agent_path).map_err(|e| e.to_string())?;
            let validated =
                apollia_aip::validator::validate_agent(&module).map_err(|e| e.to_string())?;
            let allowed_tools = validated.manifest.tools_required.clone();
            let memory_namespace = validated.manifest.memory_namespace.clone();
            let supports_a2a = validated.manifest.supports_a2a;
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
                supports_a2a,
                a2a_delegate,
                a2a_invoker,
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

/// Returns the default memory directory (`~/.apollia/memory/`).
///
/// Matches the path convention used by `apollia-os memory inspect` and `MemoryManager`.
fn default_memory_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    PathBuf::from(home).join(".apollia").join("memory")
}

/// Resolves `~` to `$HOME` in a path string.
fn expand_tilde_str(s: &str) -> PathBuf {
    if s.starts_with("~/") {
        let home = std::env::var("HOME").unwrap_or_default();
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
fn find_config_file() -> Option<PathBuf> {
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

/// Returns `true` if a Unix socket file exists at `path`.
fn socket_is_in_use(path: &std::path::Path) -> bool {
    path.exists()
}

/// Bootstrap and run the runtime in foreground.
///
/// Uses the Supervisor for ordered startup with timeout and rollback.
/// Blocks until Ctrl+C, SIGTERM, or `POST /api/v1/shutdown` is received.
/// Graceful shutdown drains in-progress tasks (30s default).
pub async fn run(socket: Option<PathBuf>, port: Option<u16>) -> Result<(), StartError> {
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
    if socket_is_in_use(&socket_path) {
        return Err(StartError::AlreadyRunning {
            address: socket_path.display().to_string(),
        });
    }

    let home = std::env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| std::env::temp_dir());

    // Load apollia.toml if found (LLM only — agents, triggers, pipelines,
    // notifications, and stt are all loaded from SQLite by the Supervisor).
    let (llm_config, config_path) = match find_config_file() {
        Some(path) => {
            tracing::info!(config = %path.display(), "loading config");
            let cfg = crate::config::parse_apollia_toml(&path).map_err(|e| StartError::Config {
                path: path.clone(),
                reason: e.to_string(),
            })?;
            (cfg.llm, Some(path))
        }
        None => {
            tracing::info!("no apollia.toml found — starting with defaults");
            (None, None)
        }
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
                tracing::warn!(error = %e, "AgentRepository failed to open — auto-load disabled");
                None
            }
        }
    };

    // Start all actors via Supervisor (ordered, with timeout + rollback)
    let config = SupervisorConfig {
        api_config: APIServerConfig {
            socket_path: socket_path.clone(),
            tcp_port,
        },
        startup_timeout_secs: 10,
        llm_config,
        config_path,
        input_required_timeout_hours: 24,
        data_dir,
        obs_config: apollia_core::ObservabilityConfig::default(),
        agent_repository,
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
    };
    let supervisor = Supervisor::new(config);
    let agent_loader: Arc<dyn AgentLoader> = Arc::new(AIPAgentLoader);

    // The ProductionBackendFactory needs the EventBusSender, which is created
    // inside supervisor.start(). We use a shared OnceLock so the factory can be
    // constructed before start() returns, then initialized lazily before first use.
    //
    // Safety: create_for_agent() is called only from POST /api/v1/agents, which
    // happens after the runtime is fully up — well after start() returns and the
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

    let factory: Arc<dyn AgentBackendFactory> = Arc::new(ProductionBackendFactory {
        event_bus: event_bus_lock.clone(),
        llm_router: llm_router_lock.clone(),
        tool_registry: tool_registry_lock.clone(),
        audit_trail: audit_trail_lock.clone(),
        pending_approvals: pending_approvals_lock.clone(),
        task_repository: task_repository_lock.clone(),
        registry: registry_lock.clone(),
        router: router_lock.clone(),
    });

    // Concrete ChatAgentRunner for Chat Agent mode.
    let chat_agent_runner: Option<Arc<dyn apollia_runtime::chat::ChatAgentRunner>> =
        Some(Arc::new(AIPChatAgentRunner {
            event_bus: event_bus_lock.clone(),
            llm_router: llm_router_lock.clone(),
            tool_registry: tool_registry_lock.clone(),
            audit_trail: audit_trail_lock.clone(),
            user_memory: user_memory_lock.clone(),
            data_dir: data_dir_for_chat,
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
    if let Some(audit) = handles.audit_trail.clone() {
        let _ = audit_trail_lock.set(audit);
    }
    if let Some(pa) = handles.pending_approvals.clone() {
        let _ = pending_approvals_lock.set(pa);
    }
    if let Some(repo) = handles.task_repository.clone() {
        let _ = task_repository_lock.set(repo);
    }
    let _ = user_memory_lock.set(handles.user_memory.clone());

    let elapsed = start.elapsed();
    println!("  * EventBus            ready");
    println!("  * AgentRegistry       ready");
    println!("  * ToolRegistry        ready (3 native tools)");
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
    tokio::select! {
        signal = apollia_runtime::shutdown::wait_for_shutdown_signal() => {
            println!();
            println!("  {signal} received, draining tasks...");
        }
        _ = wait_for_shutdown_event(&mut shutdown_rx) => {
            println!("  Shutdown requested via API, draining tasks...");
        }
    }

    // Graceful shutdown via ShutdownController (drain + ordered teardown)
    let tool_registry_handle = handles.tool_registry_handle;
    let shutdown = ShutdownController::new(
        ShutdownConfig::default(),
        handles.event_sender,
        handles.api_handle,
        handles.router_handle,
        handles.registry_handle,
        handles.notification_engine,
        handles.mcp_handle,
    );

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

    Ok(())
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
}
