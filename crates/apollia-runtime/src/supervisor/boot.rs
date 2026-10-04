use super::*;

use apollia_core::events::resilient;

use super::bootstrap::{drain_until_all_ready, emit_onboarding_if_needed};
use super::bundled::register_builtin_tools;

mod agents;
mod config;
mod data;
mod governance;
mod inference;
mod servers;

/// The actors and stores every later phase reads, gathered once the
/// TaskRouter is up.
struct Core<B: ExecutionBackend> {
    event_sender: EventBusSender,
    org_policy: Arc<apollia_permissions::PolicyVerdict>,
    registry_handle: AgentRegistryHandle,
    tool_registry_handle: ToolRegistryHandle,
    mcp_handle: Option<McpClientManagerHandle>,
    mcp_server_repo: Option<Arc<std::sync::Mutex<McpServerRepository>>>,
    router_handle: TaskRouterHandle<B>,
    trigger_def_repo: Arc<std::sync::Mutex<TriggerDefinitionRepository>>,
}

impl Supervisor {
    /// Start all runtime actors in order and return their handles.
    ///
    /// Sequence: EventBus → AgentRegistry → ToolRegistry → TaskRouter → APIServer.
    /// Each step must complete within `startup_timeout_secs`.
    /// On failure, previously started actors are stopped in reverse order.
    ///
    /// The ToolRegistry is spawned and the three native tools (BashExecutor,
    /// PythonExecutor, FileIo) are registered automatically.
    pub async fn start<B: ExecutionBackend + Clone + From<crate::coordinator::DynBackend>>(
        mut self,
        backend: B,
        agent_loader: Arc<dyn AgentLoader>,
        backend_factory: Option<Arc<dyn crate::api::routes_agents::AgentBackendFactory>>,
        agent_runner: Option<Arc<dyn crate::chat::ChatAgentRunner>>,
    ) -> Result<SupervisorHandles<B>, SupervisorError> {
        let timeout = Duration::from_secs(self.config.startup_timeout_secs);

        // Phase 1: EventBus
        info!("supervisor.eventbus.starting");
        let (event_sender, startup_rx) =
            EventBus::with_capacity(self.config.runtime_config.eventbus_capacity);
        let mut startup_rx = resilient(startup_rx, "supervisor.startup");
        info!("supervisor.eventbus.ready");

        // Phase 1b: organisation policy, read once here and held for the life
        // of the runtime. A present but invalid policy is reported, not fatal:
        // what it shuts down is decided where the policy is applied.
        let org_policy = Arc::new(governance::read_org_policy());

        // Phase 1c: the configuration in force, its live sections, and the
        // reloader that follows the file.
        let (config_feed, config_reload) = self.start_config_reload(&event_sender, &org_policy);

        // Phase 2: AgentRegistry
        info!("supervisor.registry.starting");
        let registry_handle = AgentRegistry::spawn(event_sender.clone());
        info!("supervisor.registry.ready");

        // Phase 3: ToolRegistry + native tool registration
        info!("supervisor.tool_registry.starting");
        let tool_registry_handle = ToolRegistryHandle::start();

        // Phase 3a: the existing connections the policy now refuses (see
        // `ForbiddenConnections`): not started, their tools never registered.
        let connection_admission = Arc::new(crate::governance::PolicyAdmission::new(Arc::clone(
            &org_policy,
        )));
        let mcp_store = self.open_mcp_store();
        let native_accounts = crate::connectors_bridge::connected_accounts().await;
        let forbidden_connections = Arc::new(self.forbidden_connections(
            connection_admission.as_ref(),
            mcp_store.as_ref(),
            &native_accounts,
        ));
        register_builtin_tools(
            &tool_registry_handle,
            &forbidden_connections.allowed_accounts(&native_accounts),
            forbidden_connections.offers_mailbox_tools(),
        )
        .await;

        // Phase 3b: MCP Client Manager, reads server list from mcp.db (SQLite).
        //
        // On first boot, if `mcp.db` is empty and `mcp.toml` exists, performs a
        // one-shot migration and logs the count. After migration, `mcp.toml` is no
        // longer consulted. Errors are never fatal: the runtime continues and MCP
        // tools are simply unavailable.
        let (mcp_handle, mcp_server_repo) = self
            .start_mcp(
                &tool_registry_handle,
                &event_sender,
                mcp_store,
                &forbidden_connections,
            )
            .await;

        // The runner sidecar, the local engines, then phases 4 and 4b: the
        // LlmRouter and its call log.
        let inference = self.start_inference(&event_sender, &org_policy).await;

        // Phases 4c and 4d: the execution trace and the resilience layer.
        let shared_resilience_layer = self
            .spawn_trace_observers(&event_sender, &config_feed)
            .await;

        // Phase 5 (pos 6): TaskRouter
        info!("supervisor.router.starting");
        let router_handle: TaskRouterHandle<B> =
            TaskRouterHandle::spawn(registry_handle.clone(), event_sender.clone(), 256);
        info!("supervisor.router.ready");

        // Phase 6 (pos 7): the trigger definitions, read after TaskRouter.
        let triggers = self.open_trigger_definitions()?;
        let core = Core {
            event_sender,
            org_policy,
            registry_handle,
            tool_registry_handle,
            mcp_handle,
            mcp_server_repo,
            router_handle,
            trigger_def_repo: Arc::new(std::sync::Mutex::new(triggers.repository)),
        };

        // Phases 8 to 8d: audit trail and journal, upgrades, connections.
        let governance = self
            .open_governance(
                &core.tool_registry_handle,
                &core.event_sender,
                &core.org_policy,
                governance::Connections {
                    admission: connection_admission,
                    forbidden: forbidden_connections,
                },
            )
            .await;

        // Phase 8e: the folders the former settings named, added once as
        // folder connections through the admission and the journal.
        crate::connections::folder_import::import_legacy_folders(
            self.config.config_path.as_deref(),
            &self.config.data_dir,
            &governance.connection,
        );

        // Phases 9 to 13b: the stores the API server is built over.
        let stores = self.open_stores(&core.event_sender).await?;

        // Phase 14: ChatSessionManager.
        let (chat_manager, a2a_invoker) = self.start_chat(
            &core,
            &inference,
            &stores,
            &governance,
            &agent_loader,
            &agent_runner,
        );
        if let Some(chat) = chat_manager.as_ref() {
            chat.follow_config(config_feed.clone()).await;
        }

        // Phase 15: SttEngine.
        let stt = self.start_stt(&inference, &core.event_sender).await;

        // Clone handles before moving into AppState, needed for auto-load.
        let agent_loader_for_autoload = agent_loader.clone();
        let backend_factory_for_autoload = backend_factory.clone();
        let backend_for_autoload = backend.clone();

        // The agent executor, batches, workflows, then phase 6b: TriggerEngine.
        let mut executors = self
            .start_executors(
                &core,
                chat_manager.as_ref(),
                &stores,
                &governance,
                triggers.definitions,
            )
            .await;
        let compat_server = self.spawn_compat_server(&inference, &core.event_sender);
        let batches = executors.batches.take();
        let mut state = self.app_state(
            &core,
            &inference,
            &governance,
            &stores,
            &stt,
            &executors,
            &chat_manager,
            servers::StateOwned {
                agent_loader,
                backend,
                backend_factory,
                batches,
                a2a_invoker,
                resilience: shared_resilience_layer,
                compat_server,
            },
        );
        state.config_reload = config_reload;
        let api_server = APIServer::new(self.config.api_config, state);
        let api_handle = servers::serve_api(
            api_server,
            timeout,
            self.config.startup_timeout_secs,
            &core,
            &executors.trigger_engine,
        )
        .await?;
        info!("supervisor.api.ready");

        servers::spawn_timeout_watcher(
            &self.config.hitl_config,
            &config_feed.hitl,
            stores.task_repository.as_ref(),
            &core.event_sender,
        );

        // Emit AllReady
        let _ = core.event_sender.send(RuntimeEvent::AllReady);
        info!("supervisor.all_ready");

        // Drain the AllReady event from the startup receiver
        drain_until_all_ready(&mut startup_rx, timeout).await;

        // Onboarding detection: emit OnboardingRequired if UserMemory is empty.
        // Non-blocking, the runtime is fully operational regardless of the result.
        emit_onboarding_if_needed(stores.user_memory.as_ref(), &core.event_sender);

        // Phases 10.5 to 11: bundled agents, package integrity, installed agents.
        agents::load_agents_after_ready(
            self.config.bundled_agents_path.as_deref(),
            self.config.package_repository.as_ref(),
            &agent_loader_for_autoload,
            AutoLoadCtx {
                backend_factory: &backend_factory_for_autoload,
                base_backend: &backend_for_autoload,
                registry_handle: &core.registry_handle,
                router_handle: &core.router_handle,
                event_sender: &core.event_sender,
                task_repository: stores.task_repository.as_ref(),
                agent_repository: self.config.agent_repository.as_ref(),
                data_dir: &self.config.data_dir,
                obs_config: &config_feed.observability,
                idle_unload: Duration::from_secs(self.config.runtime_config.agent_idle_unload_secs),
            },
        )
        .await;

        Ok(SupervisorHandles {
            event_sender: core.event_sender,
            registry_handle: core.registry_handle,
            tool_registry_handle: core.tool_registry_handle,
            router_handle: core.router_handle,
            api_handle,
            llm_router: inference.llm_router,
            trigger_engine: executors.trigger_engine,
            audit_trail: governance.audit_trail,
            connection_governance: governance.connection,
            governance: governance.handle,
            audit_journal: governance.audit_journal,
            task_repository: stores.task_repository,
            pending_approvals: stores.pending_approvals,
            plan_gates: stores.plan_gates,
            notification_engine: stores.notification_engine,
            llm_call_repository: inference.llm_call_repository,
            chat_manager,
            workflows: executors.workflows,
            plan_cache: stores.plan_cache,
            mailbox_handle: Some(stores.mailbox_handle),
            user_memory: stores.user_memory,
            stt_engine: stt.engine,
            stt_repository: stt.repository,
            mcp_handle: core.mcp_handle,
            runner_supervisor: inference.runner_supervisor,
            llama_server_supervisor: inference.llama_server_supervisor,
            mlx_engine: inference.mlx_engine,
            org_policy: core.org_policy,
            config_feed,
        })
    }
}
