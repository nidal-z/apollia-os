//! Agents at boot: the chat session manager and the agent-to-agent invoker,
//! the agent executor shared by batches, workflows and triggers, and the
//! loading of installed agents once the runtime is ready.

use crate::supervisor::*;

use super::data::Stores;
use super::governance::Governance;
use super::inference::Inference;
use super::Core;
use crate::supervisor::bootstrap::{agent_launcher, start_trigger_engine};
use crate::supervisor::bundled::{auto_load_bundled_agents, validate_installed_packages};
use crate::supervisor::persistence::open_sidechain_logger;

/// What runs agents outside the chat: batches, workflows and triggers.
pub(super) struct Executors {
    pub(super) batches: Option<crate::batch::BatchHandle>,
    pub(super) workflows: Option<crate::workflows::WorkflowHandle>,
    pub(super) trigger_engine: TriggerEngineHandle,
}

impl Supervisor {
    /// Phase 14: the ChatSessionManager, spawned before APIServer to inject
    /// its handle into AppState, with the A2AInvoker it shares.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn start_chat<B: ExecutionBackend + Clone + From<crate::coordinator::DynBackend>>(
        &self,
        core: &Core<B>,
        inference: &Inference,
        stores: &Stores,
        agent_loader: &Arc<dyn AgentLoader>,
        agent_runner: &Option<Arc<dyn crate::chat::ChatAgentRunner>>,
    ) -> (
        Option<crate::chat::ChatSessionManagerHandle>,
        Arc<crate::a2a::A2AInvoker>,
    ) {
        info!("supervisor.chat.starting");
        let chat_db_path = self
            .config
            .data_dir
            .join(apollia_core::paths::DataFile::Chat.file_name());

        // SidechainRepository, opened before A2AInvoker so the logger can be injected.
        let sidechain_logger = open_sidechain_logger(&self.config.data_dir);

        let a2a_invoker_builder = crate::a2a::A2AInvoker::new(
            core.registry_handle.clone(),
            core.router_handle.clone(),
            core.event_sender.clone(),
            apollia_core::A2AConfig::default(),
        );
        let a2a_invoker = std::sync::Arc::new(match sidechain_logger {
            Some(logger) => a2a_invoker_builder.with_sidechain_logger(logger),
            None => a2a_invoker_builder,
        });
        // Build the lifecycle hook executor once from the validated [hooks]
        // config and share it (read-only) with the chat loop. An empty config
        // yields an executor over an empty registry: zero overhead, no I/O.
        let hook_executor =
            std::sync::Arc::new(crate::hooks::HookExecutor::new(std::sync::Arc::new(
                crate::hooks::HookRegistry::from_config(&self.config.hooks_config),
            )));
        let chat_manager: Option<crate::chat::ChatSessionManagerHandle> =
            match crate::chat::ChatSessionManagerHandle::spawn(
                &chat_db_path,
                inference.llm_router.clone(),
                core.tool_registry_handle.clone(),
                agent_loader.clone(),
                agent_runner.clone(),
                core.event_sender.clone(),
                apollia_core::StepBudgetConfig::chat_default(),
                stores.user_memory.clone(),
                core.registry_handle.clone(),
                Some(a2a_invoker.clone()),
                core.mcp_handle.clone(),
                // Feed the chat dispatcher the same global config the Agent-mode
                // dispatcher uses. Native tools that need app-level config
                // (web_search Brave key, web_read SSRF settings, http_fetch
                // allowlist, memory_search base dir) become available in free
                // chat too.
                Some(std::sync::Arc::new(crate::chat::ChatToolsConfig {
                    data_dir: self.config.data_dir.clone(),
                    brave_api_key: None, // resolved lazily by web_search when missing
                    tools_config: self.config.tools_config.clone(),
                    tool_turn_temperature: self.config.chat_tool_turn_temperature,
                    source_registry: inference.source_registry.clone(),
                })),
                self.config.mcp_loading,
                self.config.tool_search_limit,
                Some(hook_executor.clone()),
                self.config.plan_mode_default,
            ) {
                Ok(handle) => {
                    info!("supervisor.chat.ready");
                    Some(handle)
                }
                Err(e) => {
                    warn!(error = %e, detail = "chat disabled", "supervisor.chat.failed");
                    None
                }
            };
        (chat_manager, a2a_invoker)
    }

    /// The agent executor and what it serves: batches, the workflow engine,
    /// the composer of the chat, then phase 6b, the TriggerEngine.
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn start_executors<
        B: ExecutionBackend + Clone + From<crate::coordinator::DynBackend>,
    >(
        &self,
        core: &Core<B>,
        chat_manager: Option<&crate::chat::ChatSessionManagerHandle>,
        stores: &Stores,
        governance: &Governance,
        trigger_definitions: Vec<apollia_triggers::TriggerDefinition>,
    ) -> Executors {
        // The batch engine runs an agent without code on every item of a
        // source; it needs the task store and the agent executor, and resumes
        // the batches left running as soon as it starts. It reads workbooks
        // (`spreadsheet`) and CSV files (`csv`); a launch on any other kind
        // answers that it is unknown.
        // Governance says which accesses of the agent are sensitive.
        // One agent executor path, shared by batches, workflow steps and
        // triggers; `None` without the executor or the task store.
        let launcher = agent_launcher(
            &core.router_handle,
            chat_manager,
            stores.task_repository.as_ref(),
            &core.event_sender,
            &self.config.obs_config,
        )
        .map(|launcher| crate::agents::task::AgentTaskLauncher {
            plan_cache: stores.plan_cache.clone(),
            ..launcher
        });
        let batches = match (&stores.task_repository, &launcher) {
            (Some(repo), Some(launcher)) => Some(crate::batch::BatchHandle::spawn(
                repo.clone(),
                Arc::new(batch_sources()),
                batch_runner(
                    launcher.clone(),
                    self.config.data_dir.clone(),
                    governance.audit_journal.clone(),
                ),
                core.event_sender.clone(),
            )),
            _ => None,
        };
        // The workflow engine records each run and its steps in the task
        // store; the agent of a step runs through the agent executor, as a
        // task of its own. The Python host is attached once the runtime is
        // up, by whoever embeds the interpreter.
        let mut step_agents: Option<Arc<dyn crate::workflows::AgentRunner>> = None;
        let workflows = stores.task_repository.as_ref().map(|repo| {
            let agents = launcher.clone().map(|launcher| {
                Arc::new(crate::workflows::LaunchedAgentRunner::new(
                    launcher,
                    self.config.data_dir.clone(),
                )) as Arc<dyn crate::workflows::AgentRunner>
            });
            step_agents = agents.clone();
            crate::workflows::WorkflowHandle::spawn(
                repo.clone(),
                core.event_sender.clone(),
                self.config.workflows_config.budget(),
                agents,
            )
        });
        // An agent calls the agents and workflows it was granted through the
        // same executor and the same engine a workflow step uses.
        if let (Some(chat), Some(agents), Some(engine)) = (chat_manager, step_agents, &workflows) {
            chat.set_composer(Arc::new(crate::composition::RuntimeComposer::new(
                agents,
                engine.clone(),
                self.config.data_dir.clone(),
            )))
            .await;
        }
        // Phase 6b: TriggerEngine; a trigger starts an agent or a workflow.
        let trigger_engine = start_trigger_engine(
            trigger_definitions,
            crate::trigger_targets::TriggerTargets {
                router: core.router_handle.clone(),
                agents: launcher,
                workflows: workflows.clone(),
                data_dir: self.config.data_dir.clone(),
            },
            &core.event_sender,
            &self.config,
        )
        .await;
        Executors {
            batches,
            workflows,
            trigger_engine,
        }
    }
}

/// Phases 10.5 to 11, once the runtime is ready: the bundled agents, the
/// integrity of the installed packages, then the installed agents.
///
/// Takes the config pieces rather than `&Supervisor`: building `APIServer`
/// has already moved `api_config` out of the config.
pub(super) async fn load_agents_after_ready<
    B: ExecutionBackend + Clone + From<crate::coordinator::DynBackend>,
>(
    bundled_agents_path: Option<&std::path::Path>,
    package_repository: Option<&apollia_tools::PackageRepository>,
    agent_loader: &Arc<dyn AgentLoader>,
    ctx: AutoLoadCtx<'_, B>,
) {
    // Phase 10.5: Auto-install bundled agents
    //
    // If bundled_agents_path is configured and manifest.json is present,
    // register each auto_install agent in the DB so Phase 11 picks them up.
    // Agents already in the DB are skipped. Errors are logged, never fatal.
    if let Some(repo) = ctx.agent_repository {
        auto_load_bundled_agents(bundled_agents_path, repo, agent_loader);
    }

    // Phase 10.6: Validate installed package integrity
    //
    // Lightweight check: for each installed package, verify root_path still
    // exists on disk. If missing → log warning, disable all package agents.
    // Never blocks boot. Phase 11 handles the actual loading.
    if let (Some(pkg_repo), Some(repo)) = (package_repository, ctx.agent_repository) {
        validate_installed_packages(pkg_repo, repo);
    }

    // Phase 11: Auto-load installed agents
    //
    // After AllReady, register every enabled agent from the manifest its
    // store holds, transition it to Active, and give the TaskRouter a
    // coordinator whose backend imports the agent on its first task.
    // Nothing is imported here. Errors are logged but never block the
    // boot (graceful degradation).
    Supervisor::auto_load_installed_agents(ctx).await;
}

/// The item runner of the batch engine, asking the default governance which
/// accesses are sensitive when the audit journal is open; without it, an
/// access is sensitive when its tool declares it so, the same rule.
fn batch_runner<B: crate::coordinator::ExecutionBackend>(
    launcher: crate::agents::task::AgentTaskLauncher<B>,
    data_dir: std::path::PathBuf,
    journal: Option<crate::audit_journal::AuditJournalHandle>,
) -> crate::batch::AgentItemRunner<B> {
    let runner = crate::batch::AgentItemRunner::new(launcher, data_dir);
    match journal {
        Some(journal) => runner.with_governance(Arc::new(
            crate::governance::NoPolicyGovernance::new(journal),
        )),
        None => runner,
    }
}

/// The batch source kinds this runtime reads.
fn batch_sources() -> apollia_core::batch_source::SourceRegistry {
    let mut registry = apollia_core::batch_source::SourceRegistry::new();
    if let Err(error) = apollia_sources::register_table_sources(&mut registry) {
        tracing::event!(
            tracing::Level::ERROR,
            error = %error,
            "batch.source_registration_failed"
        );
    }
    registry
}
