//! The runner the built-in chat uses when a message is addressed to an agent:
//! same execution path as the task backend, parameterised by agent name rather
//! than by an already loaded bridge.

use std::path::Path;
use std::sync::Arc;

use apollia_aip::bridge::AIPBridge;
use apollia_core::{PendingApprovals, ToolsConfig};
use apollia_llm::LlmRouter;
use apollia_memory::user_memory::UserMemoryRepository;
use apollia_oria::engine::ORIAEngine;
use apollia_runtime::a2a::A2AInvoker;
use apollia_runtime::coordinator::DynBackend;
use apollia_runtime::eventbus::EventBusSender;
use apollia_runtime::mailbox::AgentMailboxHandle;
use apollia_runtime::registry::AgentRegistryHandle;
use apollia_runtime::router::TaskRouterHandle;
use apollia_tools::tools::ask_user::PendingUserInputs;
use apollia_tools::{AuditTrailHandle, TaskRepository, ToolRegistryHandle};

use super::factory::direct_path_budget;
use super::runner::{llm_router_snapshot, BridgeRunner};
use super::venv_site_packages_for_agent_name;
use super::{build_user_context_from_repo, default_memory_dir};

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
    /// Chat manager's `ask_user` pending-input registry, populated after
    /// `init_embedded()` returns so the native tool dispatcher can route
    /// `ask_user` invocations through the chat HITL loop.
    pub pending_user_inputs: Arc<std::sync::OnceLock<PendingUserInputs>>,
    /// MCP client manager handle, populated after init_embedded() so the
    /// BridgeRunner can inject McpToolExecutor instances into the
    /// dispatcher at run time.
    pub mcp_handle: Arc<std::sync::OnceLock<apollia_mcp::manager::McpClientManagerHandle>>,
    /// Agent registry handle, required to build the A2A invoker so chat-agent
    /// Python agents can call `ctx.a2a_invoke(...)` like task-mode agents.
    pub agent_registry: Arc<std::sync::OnceLock<AgentRegistryHandle>>,
    /// Task router handle, required to build the A2A delegate.
    pub task_router: Arc<std::sync::OnceLock<TaskRouterHandle<DynBackend>>>,
    /// Inter-agent mailbox handle, exposed as `ctx.mailbox`.
    pub mailbox_handle: Arc<std::sync::OnceLock<AgentMailboxHandle>>,
    /// Global user memory repository, used to build `ctx.user_context`.
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
        // Capture datasources/templates and the agent directory.
        let datasources_declared = validated.manifest.datasources.clone();
        let templates_declared = validated.manifest.templates.clone();
        let agent_dir = install_path.parent().map(Path::to_path_buf);
        // Capture the list of declared secrets.
        let secrets_declared = validated.manifest.secrets.clone();
        // Capture the manifest before the bridge consumes `validated`.
        let chat_manifest = validated.manifest.clone();
        // Agent budget capped to the runtime ceiling on the direct path
        // (principle #7), captured before `chat_manifest` moves into the runner.
        let agent_step_budget = chat_manifest.step_budget.clone().unwrap_or_default();
        let bridge = Arc::new(AIPBridge::new(validated).map_err(|e| e.to_string())?);

        // 3. Resolve OnceLock handles
        let event_bus = self
            .event_bus
            .get()
            .cloned()
            .ok_or("event bus not initialized")?;
        let llm_router = llm_router_snapshot(&self.llm_router);
        let tool_registry = self.tool_registry.get().cloned();
        let audit_trail = self.audit_trail.get().cloned();
        let mailbox = self.mailbox_handle.get().cloned();
        let tools_config = self
            .tools_config
            .get()
            .cloned()
            .unwrap_or_else(ToolsConfig::default);

        // Build the A2A invoker when registry+router are available.
        // Gives chat-agent Python agents the same A2A capabilities as task-mode
        // and Chat Libre.
        let a2a_invoker = match (
            self.agent_registry.get().cloned(),
            self.task_router.get().cloned(),
        ) {
            (Some(registry), Some(router)) => {
                let invoker = Arc::new(A2AInvoker::new(
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
                    reason = "registry or router not initialised yet",
                    "chat.a2a.invoker.unavailable"
                );
                None
            }
        };

        // Build user_context from the global user memory store (chat mode only).
        let user_context = self.user_memory.get().and_then(|repo_mutex| {
            let repo = repo_mutex.lock().ok()?;
            build_user_context_from_repo(&repo)
        });

        // 4. Build RuntimeContext and call the agent. The Direct-path budget is
        // shared into the runner's ctx and the engine supervisor.
        let direct_budget = Arc::new(direct_path_budget(&agent_step_budget));
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
            a2a_invoker,
            mailbox,
            user_context,
            tools_config,
            datasources_declared,
            templates_declared,
            agent_dir,
            secrets_declared,
            manifest: chat_manifest,
            budget: Arc::clone(&direct_budget),
        };

        let mut engine = ORIAEngine::new().with_event_bus(event_bus);
        if let Some(pending) = self.pending_approvals.get().cloned() {
            engine = engine.with_pending_approvals(pending);
        }
        if let Some(repo) = self.task_repository.get().cloned() {
            engine = engine.with_task_repository(repo);
        }

        engine
            .execute_direct(task, &runner, direct_budget)
            .await
            .map_err(|e| e.to_string())
    }
}
