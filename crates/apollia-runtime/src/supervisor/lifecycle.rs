use super::*;

use super::bootstrap::{migrate_mcp_from_toml, start_mcp_manager};
use crate::on_demand::{BackendLoader, OnDemandBackend};

impl Supervisor {
    /// Create a new Supervisor with the given configuration.
    pub fn new(config: SupervisorConfig) -> Self {
        Self { config }
    }

    /// Phase 3b, first half: open `mcp.db`, run the one-shot `mcp.toml`
    /// migration when empty, and read the declared servers. `None` when the
    /// store cannot be opened: the runtime continues without MCP.
    pub(in crate::supervisor) fn open_mcp_store(
        &self,
    ) -> Option<(
        McpServerRepository,
        Vec<apollia_mcp::config::McpServerConfig>,
    )> {
        let mcp_db_path = self
            .config
            .data_dir
            .join(apollia_core::paths::DataFile::Mcp.file_name());
        let repo = match McpServerRepository::open(&mcp_db_path) {
            Ok(repo) => repo,
            Err(e) => {
                warn!(error = %e, detail = "continuing without MCP", "mcp.store.open.failed");
                return None;
            }
        };

        // One-shot migration from mcp.toml when the database is empty.
        migrate_mcp_from_toml(&repo, &self.config.data_dir.join("mcp.toml"));

        let server_configs = match repo.list() {
            Ok(v) => v,
            Err(e) => {
                warn!(error = %e, "mcp.servers.list.failed");
                Vec::new()
            }
        };
        Some((repo, server_configs))
    }

    /// Phase 3a: submit every existing connection (the servers of `mcp_store`,
    /// `native_accounts`, the mailboxes of the data directory) to `admission`,
    /// and keep the ones it refuses.
    pub(in crate::supervisor) fn forbidden_connections(
        &self,
        admission: &dyn crate::governance::ConnectionAdmission,
        mcp_store: Option<&(
            McpServerRepository,
            Vec<apollia_mcp::config::McpServerConfig>,
        )>,
        native_accounts: &[crate::connections::NativeAccounts],
    ) -> crate::governance::ForbiddenConnections {
        let mailboxes =
            apollia_connectors::imap::ImapAccountStore::accounts_in(&self.config.data_dir)
                .unwrap_or_else(|e| {
                    warn!(error = %e, "connections.imap.unreadable");
                    Vec::new()
                });
        crate::governance::ForbiddenConnections::compute(
            admission,
            mcp_store.map_or(&[][..], |(_, configs)| configs),
            native_accounts,
            &mailboxes,
        )
    }

    /// Phase 3b, second half: start the MCP client manager (always, even with
    /// no servers) on every declared server the policy did not forbid.
    pub(in crate::supervisor) async fn start_mcp(
        &self,
        tool_registry_handle: &ToolRegistryHandle,
        event_sender: &EventBusSender,
        store: Option<(
            McpServerRepository,
            Vec<apollia_mcp::config::McpServerConfig>,
        )>,
        forbidden: &crate::governance::ForbiddenConnections,
    ) -> (
        Option<McpClientManagerHandle>,
        Option<Arc<std::sync::Mutex<McpServerRepository>>>,
    ) {
        let Some((repo, server_configs)) = store else {
            return (None, None);
        };
        let server_configs = server_configs
            .into_iter()
            .filter(|config| !forbidden.contains_mcp_server(&config.name))
            .collect();

        let handle = start_mcp_manager(
            server_configs,
            tool_registry_handle,
            event_sender,
            &self.config.data_dir.join("mcp.toml"),
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
                reason = "no channel in the database",
                "notification.engine.disabled"
            );
            return Ok(None);
        };
        let channels = build_channels(&notif_config.channels)
            .map_err(|e| SupervisorError::NotificationConfig(e.to_string()))?;
        let active = notif_config.channels.iter().filter(|c| c.enabled).count();
        let notif_log_db_path = Some(
            self.config
                .data_dir
                .join(apollia_core::paths::DataFile::Hitl.file_name()),
        );
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
        tracing::info!(channels = active, "notification.engine.started");
        Ok(Some(handle))
    }

    /// Phase 4c: open `runtime_events.db` and spawn the runtime-events
    /// subscriber. Best-effort: failures are logged, never fatal.
    pub(in crate::supervisor) async fn spawn_event_persistor(&self, event_sender: &EventBusSender) {
        let db_path = self
            .config
            .data_dir
            .join(apollia_core::paths::DataFile::RuntimeEvents.file_name());
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
                    warn!(error = %e, "events.retention.purge.failed");
                }
                crate::observability::spawn_runtime_events_subscriber(
                    handle,
                    event_sender,
                    self.config.obs_config.clone(),
                );
                info!(
                    path = %db_path.display(),
                    "supervisor.event_persistor.ready"
                );
            }
            Err(e) => {
                warn!(
                    error = %e,
                    path = %db_path.display(),
                    detail = "runtime_events persistence disabled",
                    "supervisor.event_persistor.failed"
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
        let db_path = self
            .config
            .data_dir
            .join(apollia_core::paths::DataFile::LlmCalls.file_name());
        match LlmCallRepository::open(&db_path) {
            Ok(repo) => {
                let repo = Arc::new(std::sync::Mutex::new(repo));
                apollia_llm::spawn_llm_subscriber(repo.clone(), event_sender);
                info!("supervisor.llm_calls.ready");
                Some(repo)
            }
            Err(e) => {
                warn!(
                    error = %e,
                    detail = "LLM call persistence disabled",
                    "supervisor.llm_calls.failed"
                );
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
        mlx_engine: &Option<Arc<crate::mlx_engine::MlxEngineSupervisor>>,
    ) -> (
        Option<Arc<LlmRouter>>,
        Option<Arc<std::sync::Mutex<LlmBackendRepository>>>,
    ) {
        let repo = match LlmBackendRepository::open(system_db_path) {
            Ok(repo) => repo,
            Err(e) => {
                warn!(error = %e, detail = "LLM disabled", "supervisor.system_db.open.failed");
                return (None, None);
            }
        };
        info!("supervisor.llm_router.starting");

        // Migration: if system.db has no backends and apollia.toml has backends,
        // import them. Handles first-boot and the onboarding case where
        // setup_local_llm writes only to TOML, not to system.db.
        self.migrate_llm_backends_from_toml(&repo);

        // Declared inference servers: a key the schema migration left in clear
        // moves into the secret store, then every enabled server is resolved
        // so its models route as `<server>/<model>`.
        crate::inference_servers::adopt_pending_keys(
            &repo,
            crate::inference_servers::default_store,
        );
        // Then every key a backend row still holds in clear leaves
        // `config_json` for the store.
        crate::backend_keys::seal_stored_backend_keys(
            &repo,
            crate::inference_servers::default_store,
        );
        let server_endpoints = crate::inference_servers::server_endpoints(
            &repo,
            crate::inference_servers::default_store,
        );

        // Override instantiation of `LlamaCpp` backends so they route through
        // the local engine their model's format calls for. Cloud backends keep
        // their standard `instantiate_from_config` path. The factory is shared
        // with the reload paths (see `local_engine::local_engine_override`).
        let router_result = {
            let factory = crate::local_engine::local_engine_override(
                llama_server_supervisor.clone(),
                mlx_engine.clone(),
                Some(crate::installed_models::models_dir(&self.config.data_dir)),
            );
            match crate::backend_keys::routable_configs(
                &repo,
                crate::inference_servers::default_store,
            ) {
                Ok((configs, Some(default))) => {
                    LlmRouter::from_backend_configs_with_override(configs, default, factory).await
                }
                Ok((_, None)) => Err(apollia_llm::LlmError::BackendUnavailable {
                    backend: "default".to_string(),
                    reason:
                        "no default LLM backend in system.db - configure one with is_default=true"
                            .to_string(),
                }),
                Err(e) => Err(apollia_llm::LlmError::BackendUnavailable {
                    backend: apollia_core::paths::DataFile::System
                        .file_name()
                        .to_string(),
                    reason: e.to_string(),
                }),
            }
        };

        let repo = Arc::new(std::sync::Mutex::new(repo));
        match router_result {
            Ok(router) => {
                let router = self.finalize_llm_router(router.with_servers(server_endpoints));
                info!("supervisor.llm_router.ready");
                (Some(Arc::new(router)), Some(repo))
            }
            Err(e) => {
                warn!(
                    error = %e,
                    detail = "continuing without LLM",
                    "supervisor.llm_router.failed"
                );
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
        info!("llm.backends.migration.started");
        for db_cfg in llm_cfg.to_db_configs() {
            let backend_name = db_cfg.name.clone();
            let is_default = db_cfg.is_default;
            match repo.save(&db_cfg) {
                Ok(()) => info!(
                    backend = %backend_name,
                    is_default,
                    "llm.backend.migrated"
                ),
                Err(e) => warn!(
                    backend = %backend_name,
                    error = %e,
                    "llm.backend.migration.failed"
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
                    "llm.routing.propagated"
                );
            }
        }
        for info in router.list() {
            tracing::info!(
                backend = %info.name,
                model = %info.model_id,
                "llm.backend.ready"
            );
        }
        router
    }

    /// Phase 11: make every enabled installed agent available, without
    /// importing any of them.
    ///
    /// Each agent is registered from the manifest its store holds, and its
    /// coordinator gets an [`OnDemandBackend`]: the Python module is imported
    /// by the first task that reaches the agent, and released once the agent
    /// has gone `agent_idle_unload_secs` without a call. A disabled agent is
    /// not registered, so a task to it is refused. Errors are logged, never
    /// fatal.
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
                warn!(error = %e, detail = "skipping the auto-load", "agent.list.failed");
                return;
            }
        };
        if agents.is_empty() {
            info!("agent.autoload.empty");
        }
        for agent in &agents {
            Self::load_one_installed_agent(agent, &ctx).await;
        }
    }

    /// Register a single installed agent on demand. Skips disabled agents and
    /// logs (never propagates) any per-agent failure.
    async fn load_one_installed_agent<
        B: ExecutionBackend + Clone + From<crate::coordinator::DynBackend>,
    >(
        agent: &apollia_tools::InstalledAgent,
        ctx: &AutoLoadCtx<'_, B>,
    ) {
        if !agent.enabled {
            warn!(name = %agent.name, "agent.autoload.skipped");
            return;
        }
        let manifest = agent.manifest.clone();
        let max_concurrent = manifest.max_concurrent_tasks;
        let agent_name = manifest.name.clone();

        // Install pip packages into the agent's venv before registration.
        // On failure the agent continues in Degraded state, boot is never blocked.
        let degraded_reason = Self::setup_agent_venv(ctx.data_dir, &manifest).await;

        // Register in AgentRegistry (state = Initializing).
        let agent_id = match ctx.registry_handle.register(manifest).await {
            Ok(id) => id,
            Err(e) => {
                warn!(name = %agent_name, error = %e, "agent.register.failed");
                return;
            }
        };

        // Transition: Initializing → Active (required by state machine).
        if let Err(e) = ctx
            .registry_handle
            .update_state(agent_id.as_str(), ProcessState::Active)
            .await
        {
            warn!(name = %agent_name, error = %e, "agent.activate.failed");
            return;
        }

        // If package installation failed: Active → Degraded.
        if degraded_reason.is_some() {
            ctx.registry_handle
                .update_state(agent_id.as_str(), ProcessState::Degraded)
                .await
                .unwrap_or_else(
                    |e| warn!(name = %agent_name, error = %e, "agent.state.degraded.failed"),
                );
        }

        // The backend is built by the first task, not here.
        let agent_backend: B = match ctx.backend_factory {
            Some(factory) => {
                let factory = Arc::clone(factory);
                let install_path = agent.install_path.clone();
                let stored = agent.manifest.clone();
                let loader: BackendLoader =
                    Arc::new(move || factory.create_for_agent(&install_path, &stored));
                B::from(crate::coordinator::DynBackend::new(OnDemandBackend::spawn(
                    &agent_name,
                    loader,
                    ctx.idle_unload,
                )))
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

        info!(name = %agent_name, id = %agent_id, "agent.available");
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
                    detail = "the agent starts degraded", "agent.venv.create.failed"
                );
                return Some(e.to_string());
            }
        };
        match executor.setup_venv(&manifest.packages).await {
            Ok(()) => {
                info!(
                    agent = %manifest.name,
                    packages = ?manifest.packages,
                    "agent.packages.installed"
                );
                None
            }
            Err(e) => {
                warn!(
                    agent = %manifest.name,
                    error = %e,
                    detail = "the agent starts degraded", "agent.packages.install.failed"
                );
                Some(e.to_string())
            }
        }
    }
}
