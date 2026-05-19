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
use apollia_aip::context::{
    effective_memory_namespace, DispatcherExecutor, RuntimeContext, ToolProxy,
};
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
use apollia_runtime::A2AToolsProvider;
use apollia_tools::tools::ask_user::PendingUserInputs;
use apollia_tools::{
    build_native_dispatcher, load_governance_snapshot, AuditTrailHandle, NativeDispatcherConfig,
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
    #[error("runtime already running on {address} — use `apollia-os stop` first")]
    AlreadyRunning { address: String },
    /// API token could not be loaded or generated while `require_token = true`.
    #[error("failed to load or generate API token: {0}")]
    ApiToken(#[from] apollia_runtime::api::TokenFileError),
}

/// Real agent loader using AIPLoader + validate_agent (ADR-019).
///
/// Loads a Python module via PyO3, validates AIP duck typing, and returns
/// the deserialized [`AgentManifest`].
struct AIPAgentLoader;

impl AgentLoader for AIPAgentLoader {
    fn load_and_validate(&self, path: &Path) -> Result<AgentManifest, String> {
        let extras = venv_site_packages_for_path(path);
        let module = apollia_aip::loader::load_agent_module_with_sys_paths(path, &extras)
            .map_err(|e| e.to_string())?;
        let validated =
            apollia_aip::validator::validate_agent(&module).map_err(|e| e.to_string())?;
        Ok(validated.manifest)
    }
}

/// Compute the per-agent venv's site-packages from the agent's `.py` path.
/// Convention : agent name == `.py` file stem.
fn venv_site_packages_for_path(agent_py_path: &Path) -> Vec<PathBuf> {
    let agent_name = match agent_py_path.file_stem().and_then(|s| s.to_str()) {
        Some(s) => s,
        None => return Vec::new(),
    };
    venv_site_packages_for_name(agent_name)
}

/// Compute the per-agent venv's site-packages from the agent name.
fn venv_site_packages_for_name(agent_name: &str) -> Vec<PathBuf> {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    let base = PathBuf::from(home).join(".apollia").join("venvs");
    apollia_tools::tools::python_executor::agent_venv_site_packages(&base, agent_name)
}

/// Ouvre le [`ToolCredentialStore`] partagé pour `ctx.secrets` (ADR-104, LOT 6).
///
/// Retourne `None` si la base de gouvernance n'existe pas encore (premier
/// démarrage avant `apollia-os tools secret set ...`) ou si l'ouverture échoue.
/// L'agent obtiendra alors `None` à chaque `ctx.secrets.get(key)` — cohérent
/// avec la sémantique non-fatale d'ADR-104.
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
    /// Chat manager's `ask_user` pending-input registry — populated after
    /// `supervisor.start()` so the native dispatcher can wire
    /// `AskUserExecutor` to the chat HITL loop.
    pending_user_inputs: Arc<std::sync::OnceLock<PendingUserInputs>>,
    /// Agent registry — required to build A2A delegate + invoker so chat-agent
    /// Python agents get the same A2A capabilities as task-mode agents.
    agent_registry: Arc<std::sync::OnceLock<apollia_runtime::registry::AgentRegistryHandle>>,
    /// Task router — required to build the A2A delegate.
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
}

#[async_trait::async_trait]
impl apollia_runtime::chat::ChatAgentRunner for AIPChatAgentRunner {
    async fn run_agent(&self, agent_name: &str, task: AIPTask) -> Result<AIPResult, String> {
        let agent_path = self.data_dir.join("agents").join(agent_name);

        // Load and validate agent via PyO3. Inject per-agent venv site-packages
        // so top-level imports of pip-installed packages resolve correctly.
        let extras = venv_site_packages_for_name(agent_name);
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

        let memory_base_dir = self.data_dir.join("memory");
        let snapshot = load_governance_snapshot(&self.data_dir).unwrap_or_else(|e| {
            tracing::warn!(error = %e, "governance snapshot unavailable — defaulting to all tools enabled");
            Default::default()
        });
        let disabled_tools = merge_disabled(&self.tools_config.disabled, snapshot.disabled_tools);
        let dispatcher = Arc::new(build_native_dispatcher(&NativeDispatcherConfig {
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
            governance_db_path: Some(
                self.data_dir.join(apollia_tools::GOVERNANCE_DB_FILENAME),
            ),
        }));

        // Build A2A delegate + invoker so chat-agent Python agents can call
        // `ctx.a2a_invoke(...)` and `ctx.tools.invoke("a2a:<skill>")` on parity
        // with task-mode (triggers/API) — fixes the previous "not available in
        // chat mode" gap.
        let (a2a_delegate, a2a_invoker) =
            match (self.agent_registry.get().cloned(), self.task_router.get().cloned()) {
                (Some(registry), Some(router)) => {
                    let delegate = apollia_runtime::a2a::make_delegate_fn(
                        registry.clone(),
                        router.clone(),
                        event_bus.clone(),
                        apollia_runtime::a2a::DEFAULT_A2A_MAX_HOPS,
                    );
                    let invoker = Arc::new(apollia_runtime::a2a::A2AInvoker::new(
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
                let proxy = ToolProxy::new(
                    registry.clone(),
                    audit.clone(),
                    Arc::new(DispatcherExecutor::new(dispatcher)),
                    allowed_tools,
                    agent_name.to_string(),
                    task.task_id.clone(),
                )
                // ADR-088 — instrumentation tool_call_* (Lot 2).
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

        let profile_interface = {
            let user_manager =
                MemoryManager::new(&memory_base_dir, Some("__user__".to_string()), vec![]);
            apollia_aip::profile::ProfileInterface::new(
                user_manager,
                agent_name.to_string(),
                manifest.user_memory_write,
                agent_name == "onboarding-agent",
            )
        };

        // ADR-103 (LOT 5) — directory containing the agent .py, used to
        // resolve datasources/ and templates/ files relative to the agent.
        let agent_dir = agent_path.parent().map(Path::to_path_buf);
        let datasources_declared = manifest.datasources.clone();
        let templates_declared = manifest.templates.clone();
        // ADR-104 (LOT 6) — secrets allowlist + shared credential store.
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
                None, // mailbox — not available in chat runner context
                agent_name.to_string(),
                supports_a2a,
                user_context,
                a2a_delegate,
                a2a_invoker,
                manifest.user_memory_write, // user_memory_writable — manifest-controlled
            )
            .with_profile(profile_interface)
            // ADR-103 (LOT 5) — datasources YAML + templates Jinja2.
            .with_datasources(datasources_declared, agent_dir.as_deref())
            .with_templates(templates_declared, agent_dir.as_deref())
            // ADR-104 (LOT 6) — ctx.secrets read-only gated par le manifest.
            .with_secrets(apollia_aip::secrets::SecretsInterface::new(
                secret_store,
                secrets_declared,
            ))
            // ADR-088 — relier le contexte à la task pour que ctx.log()
            // étiquette les RuntimeEvent::AgentLog persistés.
            .with_task_id(task.task_id.clone());
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
// Filesystem sandbox root for native tools (ADR-012 dev mode).
// `FileIo` and friends sandbox all paths under this root — we keep
// `$HOME` for parity with the previous embedded `NativeToolExecutor`
// so workspaces located anywhere under the user's home remain usable.
// ─────────────────────────────────────────────────────────────

/// Return the sandbox root used for file-oriented native tools.
///
/// Centralised so every runner in this crate points at the same root.
fn sandbox_root_for_agent() -> PathBuf {
    std::env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| std::env::temp_dir())
}

/// Union of statically-disabled tools (from `apollia.toml`) with the runtime
/// disabled set (from `governance.db`). Either source disables a tool — the
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
    /// Manifest opt-in for `ctx.memory.remember_user()` writes into `__user__`.
    user_memory_write: bool,
    /// Fonction de délégation A2A type-erasée — `None` si non disponible.
    a2a_delegate: Option<apollia_runtime::a2a::A2aDelegateFn>,
    /// Orchestrateur A2A de haut niveau — `None` si registry ou router non initialisés.
    a2a_invoker: Option<Arc<apollia_runtime::a2a::A2AInvoker>>,
    /// Operator-supplied tools configuration loaded from `apollia.toml`.
    tools_config: apollia_core::ToolsConfig,
    /// Datasources déclarées au manifest (`manifest.datasources`). Vide quand
    /// l'agent n'en déclare pas (ADR-103, LOT 5).
    datasources_declared: Vec<String>,
    /// Templates Jinja2 déclarés au manifest (`manifest.templates`). Vide
    /// quand l'agent n'en déclare pas (ADR-103, LOT 5).
    templates_declared: Vec<String>,
    /// Répertoire racine de l'agent — utilisé pour résoudre
    /// `datasources/<name>.yaml` et `templates/<name>.j2` (ADR-103, LOT 5).
    agent_dir: Option<PathBuf>,
    /// Secrets déclarés au manifest (`manifest.secrets`). Allowlist stricte
    /// pour `ctx.secrets.get()` (ADR-104, LOT 6). Vide quand l'agent n'en
    /// déclare pas.
    secrets_declared: Vec<String>,
    /// Base de données apollia (`~/.apollia/` par défaut) — utilisée pour
    /// ouvrir le [`ToolCredentialStore`] partagé qui alimente `ctx.secrets`
    /// (ADR-104, LOT 6). `None` accepté en mode dégradé.
    secrets_data_dir: Option<PathBuf>,
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
            tools_config: self.tools_config.clone(),
            user_memory_write: self.user_memory_write,
            datasources_declared: self.datasources_declared.clone(),
            templates_declared: self.templates_declared.clone(),
            agent_dir: self.agent_dir.clone(),
            secrets_declared: self.secrets_declared.clone(),
            secrets_data_dir: self.secrets_data_dir.clone(),
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
    /// Operator-supplied tools configuration loaded from `apollia.toml`.
    tools_config: apollia_core::ToolsConfig,
    /// Manifest opt-in for `ctx.memory.remember_user()` writes into `__user__`.
    user_memory_write: bool,
    /// Datasources déclarées au manifest (ADR-103, LOT 5).
    datasources_declared: Vec<String>,
    /// Templates Jinja2 déclarés au manifest (ADR-103, LOT 5).
    templates_declared: Vec<String>,
    /// Répertoire racine de l'agent — résolution `datasources/` et
    /// `templates/` (ADR-103, LOT 5).
    agent_dir: Option<PathBuf>,
    /// Secrets déclarés au manifest (`manifest.secrets`) — allowlist
    /// `ctx.secrets.get()` (ADR-104, LOT 6).
    secrets_declared: Vec<String>,
    /// Base de données apollia (`~/.apollia/`) — ouverture lazy du
    /// [`ToolCredentialStore`] partagé pour `ctx.secrets` (ADR-104, LOT 6).
    secrets_data_dir: Option<PathBuf>,
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
        let tools_config = self.tools_config.clone();
        let user_memory_write = self.user_memory_write;
        let datasources_declared = self.datasources_declared.clone();
        let templates_declared = self.templates_declared.clone();
        let agent_dir = self.agent_dir.clone();
        let secrets_declared = self.secrets_declared.clone();
        let secrets_data_dir = self.secrets_data_dir.clone();

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
                tracing::warn!(error = %e, "governance snapshot unavailable — defaulting to all tools enabled");
                Default::default()
            });
            let disabled_tools = merge_disabled(&tools_config.disabled, snapshot.disabled_tools);
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
                pending_user_inputs: None,
                disabled_tools,
                brave_api_key: snapshot.brave_api_key,
                web_search_config: tools_config.web_search.clone(),
                web_read_config: tools_config.web_read.clone(),
                governance_db_path: Some(
                    governance_base.join(apollia_tools::GOVERNANCE_DB_FILENAME),
                ),
            }));

            let tool_proxy: Option<ToolProxy> = match (tool_registry.as_ref(), audit_trail.as_ref())
            {
                (Some(registry), Some(audit)) => {
                    let proxy = ToolProxy::new(
                        registry.clone(),
                        audit.clone(),
                        Arc::new(DispatcherExecutor::new(dispatcher)),
                        allowed_tools,
                        agent_id.clone(),
                        task.task_id.clone(),
                    )
                    // ADR-088 — instrumentation tool_call_* (Lot 2).
                    .with_event_bus(event_bus.clone());
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
                        "ToolProxy not available — tool registry or audit trail missing; \
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
                let user_manager =
                    MemoryManager::new(&memory_base_dir, Some("__user__".to_string()), vec![]);
                apollia_aip::profile::ProfileInterface::new(
                    user_manager,
                    agent_id.clone(),
                    user_memory_write,
                    agent_id == "onboarding-agent",
                )
            };

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
                    user_memory_write, // user_memory_writable — manifest-controlled
                )
                .with_profile(profile_interface)
                // ADR-103 (LOT 5) — datasources YAML + templates Jinja2.
                .with_datasources(datasources_declared, agent_dir.as_deref())
                .with_templates(templates_declared, agent_dir.as_deref())
                // ADR-104 (LOT 6) — ctx.secrets read-only gated par le manifest.
                .with_secrets(apollia_aip::secrets::SecretsInterface::new(
                    secrets_data_dir.as_deref().and_then(open_secret_store),
                    secrets_declared,
                ))
                // ADR-088 — task_id pour étiqueter ctx.log() côté trace.
                .with_task_id(task.task_id.clone());
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
            tools_config: self.tools_config.clone(),
            user_memory_write: self.user_memory_write,
            datasources_declared: self.datasources_declared.clone(),
            templates_declared: self.templates_declared.clone(),
            agent_dir: self.agent_dir.clone(),
            secrets_declared: self.secrets_declared.clone(),
            secrets_data_dir: self.secrets_data_dir.clone(),
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
    /// Operator-supplied tools configuration loaded from `apollia.toml`.
    tools_config: apollia_core::ToolsConfig,
    /// Base data directory (`~/.apollia/`) — utilisée pour ouvrir le
    /// [`ToolCredentialStore`] partagé à chaque exécution d'agent (ADR-104,
    /// LOT 6 — `ctx.secrets`).
    data_dir: PathBuf,
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
                let delegate = make_delegate_fn(
                    registry.clone(),
                    router.clone(),
                    event_bus.clone(),
                    apollia_runtime::a2a::DEFAULT_A2A_MAX_HOPS,
                );
                let invoker = Arc::new(apollia_runtime::a2a::A2AInvoker::new(
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
            // Inject per-agent venv site-packages so top-level imports resolve.
            let extras = venv_site_packages_for_name(&manifest.name);
            let module =
                apollia_aip::loader::load_agent_module_with_sys_paths(agent_path, &extras)
                    .map_err(|e| e.to_string())?;
            let validated =
                apollia_aip::validator::validate_agent(&module).map_err(|e| e.to_string())?;
            let allowed_tools = validated.manifest.tools_required.clone();
            let memory_namespace = validated.manifest.memory_namespace.clone();
            let supports_a2a = validated.manifest.supports_a2a;
            let user_memory_write = validated.manifest.user_memory_write;
            // ADR-103 (LOT 5) — capture datasources/templates declarations +
            // the agent's package directory so the BridgeRunner can build
            // ctx.datasources / ctx.templates on every call_run.
            let datasources_declared = validated.manifest.datasources.clone();
            let templates_declared = validated.manifest.templates.clone();
            let agent_dir = agent_path.parent().map(Path::to_path_buf);
            // ADR-104 (LOT 6) — capture la liste des secrets déclarés.
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
                supports_a2a,
                a2a_delegate,
                a2a_invoker,
                tools_config: self.tools_config.clone(),
                user_memory_write,
                datasources_declared,
                templates_declared,
                agent_dir,
                secrets_declared,
                secrets_data_dir: Some(self.data_dir.clone()),
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
    if socket_is_in_use(&socket_path) {
        return Err(StartError::AlreadyRunning {
            address: socket_path.display().to_string(),
        });
    }

    let home = std::env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| std::env::temp_dir());

    // Load apollia.toml if found — agents, triggers, pipelines, notifications, and stt
    // are loaded from SQLite by the Supervisor; only static sections are parsed here.
    let (
        llm_config,
        api_file_config,
        runtime_file_config,
        hitl_file_config,
        tools_file_config,
        config_path,
    ) = match find_config_file() {
        Some(path) => {
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
            (cfg.llm, cfg.api, cfg.runtime, cfg.hitl, cfg.tools, Some(path))
        }
        None => {
            tracing::info!("no apollia.toml found — starting with defaults");
            (None, None, None, None, None, None)
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

    // Open PackageRepository for Phase 10.6 integrity check.
    let package_repository: Option<apollia_tools::PackageRepository> = {
        let db_path = data_dir.join("agents.db");
        match apollia_tools::PackageRepository::open(&db_path) {
            Ok(repo) => Some(repo),
            Err(e) => {
                tracing::warn!(error = %e, "PackageRepository failed to open — Phase 10.6 disabled");
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
    let config = SupervisorConfig {
        api_config: APIServerConfig {
            socket_path: socket_path.clone(),
            bind_addr,
            tcp_port,
            api_token,
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
    let pending_user_inputs_lock: Arc<std::sync::OnceLock<PendingUserInputs>> =
        Arc::new(std::sync::OnceLock::new());

    let factory: Arc<dyn AgentBackendFactory> = Arc::new(ProductionBackendFactory {
        event_bus: event_bus_lock.clone(),
        llm_router: llm_router_lock.clone(),
        tool_registry: tool_registry_lock.clone(),
        audit_trail: audit_trail_lock.clone(),
        pending_approvals: pending_approvals_lock.clone(),
        task_repository: task_repository_lock.clone(),
        registry: registry_lock.clone(),
        router: router_lock.clone(),
        tools_config: tools_config.clone(),
        data_dir: data_dir_for_chat.clone(),
    });

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
    if let Some(chat) = handles.chat_manager.as_ref() {
        let _ = pending_user_inputs_lock.set(chat.pending_user_inputs());
    }

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

    Ok(interrupted)
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
