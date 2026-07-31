use super::*;

use super::bootstrap::{migrate_mcp_from_toml, start_mcp_manager};

impl Supervisor {
    /// Create a new Supervisor with the given configuration.
    pub fn new(config: SupervisorConfig) -> Self {
        Self { config }
    }

    /// Phase 3b: open `mcp.db`, run the one-shot `mcp.toml` migration when
    /// empty, and start the MCP client manager (always, even with no servers).
    pub(in crate::supervisor) async fn start_mcp(
        &self,
        tool_registry_handle: &ToolRegistryHandle,
        event_sender: &EventBusSender,
    ) -> (
        Option<McpClientManagerHandle>,
        Option<Arc<std::sync::Mutex<McpServerRepository>>>,
    ) {
        let mcp_db_path = self.config.data_dir.join("mcp.db");
        let mcp_config_path = self.config.data_dir.join("mcp.toml");

        let repo = match McpServerRepository::open(&mcp_db_path) {
            Ok(repo) => repo,
            Err(e) => {
                warn!(error = %e, "failed to open mcp.db - continuing without MCP");
                return (None, None);
            }
        };

        // One-shot migration from mcp.toml when the database is empty.
        migrate_mcp_from_toml(&repo, &mcp_config_path);

        // Load server list and start the manager.
        let server_configs = match repo.list() {
            Ok(v) => v,
            Err(e) => {
                warn!(error = %e, "failed to list MCP servers from mcp.db");
                Vec::new()
            }
        };

        let handle = start_mcp_manager(
            server_configs,
            tool_registry_handle,
            event_sender,
            &mcp_config_path,
            self.config.mcp_loading,
        )
        .await;

        let repo = Arc::new(std::sync::Mutex::new(repo));
        (handle, Some(repo))
    }

    /// Spawn the NotificationEngine from the persisted config, or skip it when
    /// no channels/events are configured.
    pub(in crate::supervisor) fn spawn_notification_engine(
        &self,
        notif_config: Option<&NotificationConfig>,
        event_sender: &EventBusSender,
    ) -> Result<Option<NotificationEngineHandle>, SupervisorError> {
        let Some(notif_config) = notif_config else {
            tracing::info!(
                "Supervisor: aucun canal de notification en base - NotificationEngine désactivé"
            );
            return Ok(None);
        };
        let channels = build_channels(&notif_config.channels)
            .map_err(|e| SupervisorError::NotificationConfig(e.to_string()))?;
        let active = notif_config.channels.iter().filter(|c| c.enabled).count();
        let notif_log_db_path = Some(self.config.data_dir.join("hitl.db"));
        // Use 127.0.0.1 as connect address even when bind_addr is 0.0.0.0
        // (wildcard bind address is not a valid remote address for connect).
        let connect_addr = if self.config.api_config.bind_addr == "0.0.0.0" {
            "127.0.0.1".to_string()
        } else {
            self.config.api_config.bind_addr.clone()
        };
        // When TCP is disabled (Unix-socket-only host), fall back to the nominal
        // port for the callback URL string; webhook callbacks are only reachable
        // when a TCP listener is actually bound.
        let api_base_url = format!(
            "http://{}:{}",
            connect_addr,
            self.config.api_config.tcp_port.unwrap_or(7771)
        );
        let engine = NotificationEngine::new(
            notif_config.clone(),
            channels,
            event_sender.clone(),
            api_base_url,
            notif_log_db_path,
        );
        let handle = engine.spawn();
        tracing::info!(channels = active, "NotificationEngine démarré");
        Ok(Some(handle))
    }

    /// Phase 4c: open `runtime_events.db` and spawn the runtime-events
    /// subscriber. Best-effort: failures are logged, never fatal.
    pub(in crate::supervisor) async fn spawn_event_persistor(&self, event_sender: &EventBusSender) {
        let db_path = self.config.data_dir.join("runtime_events.db");
        match crate::observability::EventPersistorHandle::open(&db_path).await {
            Ok(handle) => {
                // Retention is applied once at boot. The event log is the only
                // store purged on a timer: the audit journal is a hash chain and
                // must stay whole for `audit verify`.
                let now_unix = chrono::Utc::now().timestamp();
                if let Err(e) = handle
                    .purge_older_than(self.config.obs_config.retention_days, now_unix)
                    .await
                {
                    warn!(error = %e, "runtime_events retention purge failed");
                }
                crate::observability::spawn_runtime_events_subscriber(
                    handle,
                    event_sender,
                    self.config.obs_config.clone(),
                );
                info!(
                    path = %db_path.display(),
                    "Supervisor: EventPersistor ready (runtime_events subscriber spawned)"
                );
            }
            Err(e) => {
                warn!(
                    error = %e,
                    path = %db_path.display(),
                    "EventPersistor failed to open - runtime_events persistence disabled"
                );
            }
        }
    }

    /// Phase 4b: open `llm_calls.db` and spawn the EventBus subscriber that
    /// persists LLM calls. Returns `None` (logged) when the DB can't open.
    pub(in crate::supervisor) fn spawn_llm_call_repository(
        &self,
        event_sender: &EventBusSender,
    ) -> Option<Arc<std::sync::Mutex<LlmCallRepository>>> {
        let db_path = self.config.data_dir.join("llm_calls.db");
        match LlmCallRepository::open(&db_path) {
            Ok(repo) => {
                let repo = Arc::new(std::sync::Mutex::new(repo));
                apollia_llm::spawn_llm_subscriber(repo.clone(), event_sender);
                info!("Supervisor: LlmCallRepository ready (subscriber spawned)");
                Some(repo)
            }
            Err(e) => {
                warn!(error = %e, "LlmCallRepository failed to open - LLM call persistence disabled");
                None
            }
        }
    }

    /// Phase 15: start the SttEngine when the persisted config is enabled and
    /// the model file exists. STT always routes through the runner sidecar;
    /// without a runner it stays disabled. Returns the engine handle and a
    /// separate API-side repository handle (both `None` when disabled).
    #[allow(clippy::type_complexity)]
    pub(in crate::supervisor) async fn start_stt_engine(
        &self,
        stt_cfg: Option<&SttConfigRow>,
        runner_supervisor: &Option<Arc<crate::runner_supervisor::RunnerSupervisor>>,
        event_sender: &EventBusSender,
    ) -> (
        Option<crate::stt::SttEngineHandle>,
        Option<std::sync::Arc<std::sync::Mutex<apollia_stt::SttRepository>>>,
    ) {
        crate::stt::build_stt_engine(
            &self.config.data_dir,
            stt_cfg,
            runner_supervisor.as_ref().map(|s| s.proxy()),
            event_sender,
        )
        .await
    }

    /// Phase 4: open `system.db`, migrate TOML backends on first boot, and
    /// build the [`LlmRouter`] (LlamaCpp backends routed through the managed
    /// llama-server). Returns `(router, backend_repo)`, either may be `None`.
    #[allow(clippy::type_complexity)]
    pub(in crate::supervisor) async fn start_llm_router(
        &self,
        system_db_path: &std::path::Path,
        llama_server_supervisor: &Option<Arc<crate::llama_server::LlamaServerSupervisor>>,
    ) -> (
        Option<Arc<LlmRouter>>,
        Option<Arc<std::sync::Mutex<LlmBackendRepository>>>,
    ) {
        let repo = match LlmBackendRepository::open(system_db_path) {
            Ok(repo) => repo,
            Err(e) => {
                warn!(error = %e, "failed to open system.db - LLM disabled");
                return (None, None);
            }
        };
        info!("Supervisor: starting LlmRouter from system.db");

        // Migration: if system.db has no backends and apollia.toml has backends,
        // import them. Handles first-boot and the onboarding case where
        // setup_local_llm writes only to TOML, not to system.db.
        self.migrate_llm_backends_from_toml(&repo);

        // Override instantiation of `LlamaCpp` backends so they route through
        // the managed llama-server. Cloud backends keep their standard
        // `instantiate_from_config` path. The factory is shared with the reload
        // paths (see `llama_server_backend::llama_server_override`).
        let router_result = {
            let factory =
                crate::llama_server_backend::llama_server_override(llama_server_supervisor.clone());
            LlmRouter::from_repository_with_override(&repo, factory).await
        };

        let repo = Arc::new(std::sync::Mutex::new(repo));
        match router_result {
            Ok(router) => {
                let router = self.finalize_llm_router(router);
                info!("Supervisor: LlmRouter ready");
                (Some(Arc::new(router)), Some(repo))
            }
            Err(e) => {
                warn!(error = %e, "LlmRouter failed to initialize - continuing without LLM");
                (None, Some(repo))
            }
        }
    }

    /// Import LLM backends declared in `apollia.toml` into `system.db` when the
    /// database has no backends yet. No-op otherwise. Errors are logged.
    fn migrate_llm_backends_from_toml(&self, repo: &LlmBackendRepository) {
        let Some(llm_cfg) = &self.config.llm_config else {
            return;
        };
        let Ok(existing) = repo.list() else {
            return;
        };
        if !existing.is_empty() {
            return;
        }
        info!("Supervisor: no LLM backends in system.db - migrating from apollia.toml");
        for db_cfg in llm_cfg.to_db_configs() {
            let backend_name = db_cfg.name.clone();
            let is_default = db_cfg.is_default;
            match repo.save(&db_cfg) {
                Ok(()) => info!(
                    backend = %backend_name,
                    is_default,
                    "LLM backend migrated from TOML to system.db"
                ),
                Err(e) => warn!(
                    backend = %backend_name,
                    error = %e,
                    "failed to migrate LLM backend from TOML to system.db"
                ),
            }
        }
    }

    /// Propagate the `[llm.routing]` TOML section onto the freshly built router
    /// and log the ready backends. `system.db` does not store routing.
    fn finalize_llm_router(&self, mut router: LlmRouter) -> LlmRouter {
        if let Some(llm_cfg) = self.config.llm_config.as_ref() {
            if let Some(routing) = llm_cfg.routing.as_ref() {
                router = router.with_routing(routing.clone());
                tracing::info!(
                    precise = %routing.precise,
                    fast = %routing.fast,
                    "Supervisor: [llm.routing] propagated to LlmRouter"
                );
            }
        }
        for info in router.list() {
            tracing::info!(
                backend = %info.name,
                model = %info.model_id,
                "LLM backend ready"
            );
        }
        router
    }

    /// Phase 11: load every enabled installed agent, register it, install its
    /// venv packages, and wire an [`ExecutionCoordinator`] into the router.
    /// Errors are logged, never fatal.
    pub(in crate::supervisor) async fn auto_load_installed_agents<
        B: ExecutionBackend + Clone + From<crate::coordinator::DynBackend>,
    >(
        ctx: AutoLoadCtx<'_, B>,
    ) {
        let Some(repo) = ctx.agent_repository else {
            return;
        };
        let agents = match repo.list_enabled() {
            Ok(agents) => agents,
            Err(e) => {
                warn!(error = %e, "Failed to list installed agents - skipping auto-load");
                return;
            }
        };
        if agents.is_empty() {
            info!("No installed agents to load");
        }
        for agent in &agents {
            Self::load_one_installed_agent(agent, &ctx).await;
        }
    }

    /// Load and wire a single installed agent. Skips disabled agents and logs
    /// (never propagates) any per-agent failure.
    async fn load_one_installed_agent<
        B: ExecutionBackend + Clone + From<crate::coordinator::DynBackend>,
    >(
        agent: &apollia_tools::InstalledAgent,
        ctx: &AutoLoadCtx<'_, B>,
    ) {
        if !agent.enabled {
            warn!(name = %agent.name, "Skipping disabled installed agent");
            return;
        }
        let manifest = match ctx.agent_loader.load_and_validate(&agent.install_path) {
            Ok(m) => m,
            Err(e) => {
                warn!(name = %agent.name, error = %e, "Failed to load installed agent");
                let _ = ctx.event_sender.send(RuntimeEvent::AgentLoadFailed {
                    name: agent.name.clone(),
                    error: e.to_string(),
                });
                return;
            }
        };

        let max_concurrent = manifest.max_concurrent_tasks;
        let agent_name = manifest.name.clone();

        // Install pip packages into the agent's venv before registration.
        // On failure the agent continues in Degraded state, boot is never blocked.
        let degraded_reason = Self::setup_agent_venv(ctx.data_dir, &manifest).await;

        // Register in AgentRegistry (state = Initializing).
        let agent_id = match ctx.registry_handle.register(manifest).await {
            Ok(id) => id,
            Err(e) => {
                warn!(name = %agent_name, error = %e, "Failed to register installed agent");
                return;
            }
        };

        // Transition: Initializing → Active (required by state machine).
        if let Err(e) = ctx
            .registry_handle
            .update_state(agent_id.as_str(), ProcessState::Active)
            .await
        {
            warn!(name = %agent_name, error = %e, "Failed to activate agent");
            return;
        }

        // If package installation failed: Active → Degraded.
        if degraded_reason.is_some() {
            ctx.registry_handle
                .update_state(agent_id.as_str(), ProcessState::Degraded)
                .await
                .unwrap_or_else(
                    |e| warn!(name = %agent_name, error = %e, "failed to set Degraded state"),
                );
        }

        // Create ExecutionCoordinator with backend factory.
        let agent_backend: B = match ctx.backend_factory {
            Some(factory) => {
                let dyn_backend = factory.create_for_agent(&agent.install_path, &agent.manifest);
                B::from(dyn_backend)
            }
            None => ctx.base_backend.clone(),
        };
        let mut coordinator = ExecutionCoordinator::new(
            agent_id.clone(),
            max_concurrent,
            ctx.event_sender.clone(),
            agent_backend,
        )
        .with_agent_name(agent_name.clone());
        if let Some(repo) = ctx.task_repository {
            coordinator =
                coordinator.with_task_repository(Arc::clone(repo), ctx.obs_config.clone());
        }

        // Register coordinator in TaskRouter.
        let _ = ctx
            .router_handle
            .register_coordinator(agent_id.clone(), coordinator)
            .await;

        info!(name = %agent_name, id = %agent_id, "Auto-loaded installed agent");
    }

    /// Install an agent's pip packages into its venv. Returns `Some(reason)`
    /// when the agent should start in a Degraded state, `None` on success or
    /// when there are no packages to install.
    async fn setup_agent_venv(
        data_dir: &std::path::Path,
        manifest: &apollia_core::AgentManifest,
    ) -> Option<String> {
        if manifest.packages.is_empty() {
            return None;
        }
        let venv_base = data_dir.join("venvs");
        let executor = match apollia_tools::tools::python_executor::PythonExecutor::new(
            &manifest.name,
            &venv_base,
        ) {
            Ok(executor) => executor,
            Err(e) => {
                warn!(
                    agent = %manifest.name,
                    error = %e,
                    "failed to create PythonExecutor for venv - agent will start in DEGRADED state"
                );
                return Some(e.to_string());
            }
        };
        match executor.setup_venv(&manifest.packages).await {
            Ok(()) => {
                info!(
                    agent = %manifest.name,
                    packages = ?manifest.packages,
                    "agent packages installed"
                );
                None
            }
            Err(e) => {
                warn!(
                    agent = %manifest.name,
                    error = %e,
                    "package installation failed - agent will start in DEGRADED state"
                );
                Some(e.to_string())
            }
        }
    }
}
