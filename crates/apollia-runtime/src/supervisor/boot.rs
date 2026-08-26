use super::*;

use apollia_core::events::resilient;

use super::bootstrap::{
    drain_until_all_ready, emit_onboarding_if_needed, flatten_api_start, load_trigger_definitions,
    rollback_startup_actors, seed_default_desktop_channel_if_needed, spawn_runner_supervisor,
};
use super::bundled::{
    auto_load_bundled_agents, register_builtin_tools, validate_installed_packages,
};
use super::persistence::{
    open_audit_journal, open_audit_trail, open_plan_cache, open_project_repository,
    open_sidechain_logger, open_task_repository, open_trigger_persistence, open_user_memory,
};

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
        self,
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

        // Phase 2: AgentRegistry
        info!("supervisor.registry.starting");
        let registry_handle = AgentRegistry::spawn(event_sender.clone());
        info!("supervisor.registry.ready");

        // Phase 3: ToolRegistry + native tool registration
        info!("supervisor.tool_registry.starting");
        let tool_registry_handle = ToolRegistryHandle::start();
        register_builtin_tools(&tool_registry_handle).await;

        // Phase 3b: MCP Client Manager, reads server list from mcp.db (SQLite).
        //
        // On first boot, if `mcp.db` is empty and `mcp.toml` exists, performs a
        // one-shot migration and logs the count. After migration, `mcp.toml` is no
        // longer consulted. Errors are never fatal: the runtime continues and MCP
        // tools are simply unavailable.
        let (mcp_handle, mcp_server_repo) =
            self.start_mcp(&tool_registry_handle, &event_sender).await;

        // Spawn the sidecar runner for the local LLM and STT backends. Cloud
        // backends are still served directly by the daemon via `LlmRouter`.
        //
        // Wrapped in an `Arc` and kept inside `SupervisorHandles` (then
        // `RuntimeHandle`): the supervisor owns the child runner process with
        // `kill_on_drop(true)`. If it were dropped at the end of boot, the
        // runner would be killed and any later inference call would fail with
        // a connection-refused error.
        let runner_supervisor = spawn_runner_supervisor().await.map(Arc::new);

        // Supervise the runner: a fatal condition (e.g. a GGML_ASSERT abort in
        // llama.cpp on an unsupported model architecture) kills the child, after
        // which the cached handle would point at a dead port forever. The
        // supervision task detects the exit and respawns a fresh runner so the
        // system recovers once a supported model is selected.
        if let Some(supervisor) = &runner_supervisor {
            Arc::clone(supervisor).spawn_supervision();
        }

        // Embedded llama-server is the local LLM engine (the runner now serves
        // STT only). Created lazily: it locates the binary but does not launch a
        // process until a model is requested, so a fresh install with no default
        // model still yields a supervisor the router factory can capture. `None`
        // when the binary is absent, leaving local inference unconfigured.
        let llama_server_supervisor = crate::llama_server::LlamaServerSupervisor::new(
            crate::llama_server::LlamaServerConfig::default(),
        )
        .ok();
        if let Some(supervisor) = &llama_server_supervisor {
            Arc::clone(supervisor).spawn_supervision();
        }

        // Phase 4 (pos 5): LlmRouter + LlmBackendRepository, loads backends from system.db
        let system_db_path = self
            .config
            .data_dir
            .join(apollia_core::paths::DataFile::System.file_name());
        let (llm_router, llm_backend_repo) = self
            .start_llm_router(&system_db_path, &llama_server_supervisor)
            .await;

        // Phase 4b: LlmCallRepository, an EventBus subscriber that persists LLM calls.
        let llm_call_repository = if llm_router.is_some() {
            self.spawn_llm_call_repository(&event_sender)
        } else {
            None
        };

        // Phase 4c: EventPersistor, an append-only log of the agent execution
        // trace (AgentLog, plus thoughts, tool_call_*, llm_call_*, etc.). If the
        // open fails, the runtime continues without persistence (logs still go
        // to `tracing`).
        self.spawn_event_persistor(&event_sender).await;

        // Phase 4d: shared ResilienceLayer + event subscriber.
        //
        // Per-task ORIA engines keep their own short-lived breakers, but the
        // operator-facing `/api/v1/resilience/*` surface needs a stable
        // snapshot. Hydrating it from the runtime event bus (`ToolCallCompleted`
        // / `ToolCallDenied`) keeps the engines decoupled from a global mutex
        // while still surfacing accurate per-tool circuit state.
        let shared_resilience_layer = std::sync::Arc::new(std::sync::Mutex::new(
            apollia_oria::ResilienceLayer::default(),
        ));
        crate::observability::spawn_resilience_subscriber(
            shared_resilience_layer.clone(),
            &event_sender,
        );
        info!("supervisor.resilience.ready");

        // Phase 5 (pos 6): TaskRouter
        info!("supervisor.router.starting");
        let router_handle: TaskRouterHandle<B> =
            TaskRouterHandle::spawn(registry_handle.clone(), event_sender.clone(), 256);
        info!("supervisor.router.ready");

        // Phase 6 (pos 7): TriggerEngine, started after TaskRouter (needs the submitter).
        info!("supervisor.triggers.starting");
        // Open the trigger definition repository from SQLite.
        let trigger_def_db_path = self
            .config
            .data_dir
            .join(apollia_core::paths::DataFile::TriggersDef.file_name());
        let trigger_def_repo =
            TriggerDefinitionRepository::open(&trigger_def_db_path).map_err(|e| {
                SupervisorError::ActorStartFailed {
                    actor: "trigger_engine".to_string(),
                    reason: format!("failed to open triggers_def.db: {e}"),
                }
            })?;
        let trigger_definitions = load_trigger_definitions(&trigger_def_repo)?;
        let trigger_def_repo = Arc::new(std::sync::Mutex::new(trigger_def_repo));
        // Open the SQLite trigger persistence (history of fires/skips).
        let trigger_persistence = open_trigger_persistence(&self.config.data_dir);
        let enabled_count = trigger_definitions.iter().filter(|t| t.enabled).count();
        let trigger_engine = TriggerEngineHandle::spawn(
            trigger_definitions,
            router_handle.clone(),
            event_sender.clone(),
            trigger_persistence,
            self.config.obs_config.clone(),
        )
        .await;
        tracing::info!(active = enabled_count, "trigger.engine.started");
        let _ = event_sender.send(RuntimeEvent::TriggersReloaded {
            count: enabled_count,
        });

        // Phase 8 (pos 9): AuditTrail, opened before APIServer so it's injectable into AppState.
        info!("supervisor.audit_trail.opening");
        let audit_trail_handle = open_audit_trail(&self.config.data_dir).await;

        // Phase 8b: hash-chained, signed AuditJournal + its EventBus subscriber.
        info!("supervisor.audit_journal.opening");
        let audit_journal_handle = open_audit_journal(&self.config.data_dir).await;
        if let Some(journal) = &audit_journal_handle {
            AuditJournalSubscriber::spawn(journal.clone(), event_sender.subscribe());
            info!("audit.journal.subscriber.started");
        } else {
            warn!(detail = "subscriber not started", "audit.journal.disabled");
        }

        // Phase 9 (pos 10): APIServer
        info!("supervisor.api.starting");
        // Open TaskRepository (HITL persistence).
        // Shared between AppState (resume handler) and TimeoutWatcher.
        let task_repository = open_task_repository(&self.config.data_dir).await;
        // PendingApprovals, oneshot channel registry for HITL suspension.
        let pending_approvals: Option<Arc<PendingApprovals>> = task_repository
            .as_ref()
            .map(|_| Arc::new(PendingApprovals::new()));
        // PendingPlanGates, oneshot registry shared between the plan-decision
        // route and the per-task ORIAEngine for plan-mode approval.
        let plan_gates: Option<Arc<apollia_oria::PendingPlanGates>> =
            Some(apollia_oria::PendingPlanGates::new());

        // Phase 13 (early): UserMemoryRepository, promoted before notifications
        // so we can consult the seed marker + profile name when bootstrapping
        // the default desktop channel.
        let user_memory = open_user_memory(&self.config.data_dir);

        // open NotificationConfigRepository from SQLite.
        let notif_db_path = self
            .config
            .data_dir
            .join(apollia_core::paths::DataFile::Notifications.file_name());
        let notification_repo =
            NotificationConfigRepository::open(&notif_db_path).map_err(|e| {
                SupervisorError::ActorStartFailed {
                    actor: "notification_engine".to_string(),
                    reason: format!("failed to open notifications.db: {e}"),
                }
            })?;

        // Bootstrap a desktop-default channel on first launch.
        // Idempotent, guarded by a marker in the __user__ namespace.
        seed_default_desktop_channel_if_needed(&notification_repo, user_memory.as_ref());

        // Read channels and global events from SQLite to build NotificationConfig.
        let notif_channel_rows =
            notification_repo
                .list_channels()
                .map_err(|e| SupervisorError::ActorStartFailed {
                    actor: "notification_engine".to_string(),
                    reason: format!("failed to list notification channels: {e}"),
                })?;
        let notif_global_events = notification_repo.get_global_events().map_err(|e| {
            SupervisorError::ActorStartFailed {
                actor: "notification_engine".to_string(),
                reason: format!("failed to get global events: {e}"),
            }
        })?;
        let notif_channel_configs: Vec<apollia_notifications::ChannelConfig> = notif_channel_rows
            .iter()
            .map(|row| row.to_channel_config())
            .collect();
        let notification_config_from_db =
            if notif_channel_configs.is_empty() && notif_global_events.is_empty() {
                None
            } else {
                Some(NotificationConfig {
                    events: notif_global_events,
                    channels: notif_channel_configs,
                    inactivity_timeout_secs: 30,
                })
            };
        let notification_config_for_state = notification_config_from_db.clone();
        let notification_config_for_engine = notification_config_from_db.clone();
        let notification_repo = Arc::new(std::sync::Mutex::new(notification_repo));

        // NotificationEngine, spawned before AppState so its handle can be passed in.
        let notification_engine =
            self.spawn_notification_engine(notification_config_for_engine.as_ref(), &event_sender)?;

        // Phase 12b: PlanCacheRepository, opened before APIServer for REST stats/clear.
        let plan_cache = open_plan_cache(&self.config.data_dir);

        // Phase 12c: AgentMailbox, durable SQLite-backed actor, always spawned.
        let rc = &self.config.runtime_config;
        let mailbox_config = crate::mailbox::MailboxConfig {
            capacity: rc.mailbox_capacity,
            visibility_timeout_secs: rc.mailbox_visibility_timeout_secs,
            message_ttl_secs: rc.mailbox_message_ttl_secs,
            send_quota_per_run: rc.mailbox_send_quota_per_run,
            max_payload_bytes: rc.mailbox_max_payload_bytes,
            audit_full_payload: rc.mailbox_audit_full_payload,
        };
        let mailbox_handle = crate::mailbox::AgentMailboxHandle::spawn(
            Some(
                self.config
                    .data_dir
                    .join(apollia_core::paths::DataFile::Mailbox.file_name()),
            ),
            event_sender.clone(),
            mailbox_config,
        )
        .await;
        info!("supervisor.mailbox.ready");

        // Phase 13: UserMemoryRepository was promoted above (before notifications)
        // to support the seed bootstrap of the default desktop channel. Variable
        // `user_memory` is already bound by that earlier block.

        // Phase 13b: ProjectRepository, SQLite projects.db.
        let project_repository = open_project_repository(&self.config.data_dir);

        // Phase 14: ChatSessionManager, spawned before APIServer to inject handle into AppState.
        info!("supervisor.chat.starting");
        let chat_db_path = self
            .config
            .data_dir
            .join(apollia_core::paths::DataFile::Chat.file_name());

        // SidechainRepository, opened before A2AInvoker so the logger can be injected.
        let sidechain_logger = open_sidechain_logger(&self.config.data_dir);

        let a2a_invoker_builder = crate::a2a::A2AInvoker::new(
            registry_handle.clone(),
            router_handle.clone(),
            event_sender.clone(),
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
                llm_router.clone(),
                tool_registry_handle.clone(),
                agent_loader.clone(),
                agent_runner.clone(),
                event_sender.clone(),
                apollia_core::StepBudgetConfig::chat_default(),
                user_memory.clone(),
                registry_handle.clone(),
                Some(a2a_invoker.clone()),
                project_repository.as_ref().map(|repo| {
                    std::sync::Arc::new(crate::chat::DefaultProjectContextProvider::new(
                        repo.clone(),
                    ))
                        as std::sync::Arc<dyn crate::chat::ProjectContextProvider>
                }),
                project_repository.clone(),
                mcp_handle.clone(),
                // Feed the chat dispatcher the same global config the Agent-mode
                // dispatcher uses. Native tools that need app-level config
                // (web_search Brave key, web_read SSRF settings, http_fetch
                // allowlist, memory_search base dir, permission_rule_*
                // governance.db) become available in free chat too.
                Some(std::sync::Arc::new(crate::chat::ChatToolsConfig {
                    data_dir: self.config.data_dir.clone(),
                    brave_api_key: None, // resolved lazily by web_search when missing
                    tools_config: self.config.tools_config.clone(),
                    default_workspace: self
                        .config
                        .chat_default_workspace
                        .clone()
                        .map(std::path::PathBuf::from),
                    tool_turn_temperature: self.config.chat_tool_turn_temperature,
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

        // Phase 15: SttEngine, reads config from system.db, then conditionally starts
        // the engine when `stt_config.enabled = true`.
        //
        // Opens `SttConfigRepository` against the same `system.db` used by the
        // LLM backend registry. On first boot an empty table is initialised with
        // defaults. The engine is only started when the persisted config has
        // `enabled = true` and the model file exists on disk.
        let stt_config_repo = match SttConfigRepository::open(&system_db_path) {
            Ok(repo) => {
                info!("supervisor.stt_config.ready");
                Some(std::sync::Arc::new(std::sync::Mutex::new(repo)))
            }
            Err(e) => {
                warn!(
                    error = %e,
                    detail = "STT configuration disabled",
                    "supervisor.stt_config.failed"
                );
                None
            }
        };

        let stt_cfg: Option<SttConfigRow> = stt_config_repo.as_ref().and_then(|repo| {
            repo.lock()
                .ok()
                .and_then(|guard| guard.get_or_default().ok())
        });

        let (stt_engine, stt_repository) = self
            .start_stt_engine(stt_cfg.as_ref(), &runner_supervisor, &event_sender)
            .await;
        // Wrap the boot snapshot in shared, swappable cells so the STT reload
        // route (and the desktop reload command) can bring a model online
        // mid-session without restarting the daemon. AppState and the embedded
        // runtime handle share the same cells, so a swap is seen by every reader.
        let shared_stt_engine = crate::api::server::shared_stt_engine_from(stt_engine);
        let shared_stt_repository = crate::api::server::shared_stt_repository_from(stt_repository);

        // Clone handles before moving into AppState, needed for auto-load.
        let agent_loader_for_autoload = agent_loader.clone();
        let backend_factory_for_autoload = backend_factory.clone();
        let backend_for_autoload = backend.clone();
        // Wrap the boot snapshot in a shared, swappable cell so the reload
        // route can replace the active router without restarting the daemon.
        // Other consumers (chat manager, embedded runtime) keep their own
        // direct handle and stay reachable via the supervisor handle struct.
        let shared_llm_router = crate::api::server::shared_llm_router_from(llm_router.clone());
        let state = AppState {
            router_handle: router_handle.clone(),
            registry_handle: registry_handle.clone(),
            event_sender: event_sender.clone(),
            agent_loader,
            backend,
            llm_router: shared_llm_router.clone(),
            trigger_engine: Some(trigger_engine.clone()),
            config_path: self.config.config_path.clone(),
            task_repository: task_repository.clone(),
            pending_approvals: pending_approvals.clone(),
            plan_gates: plan_gates.clone(),
            notification_config: notification_config_for_state,
            backend_factory,
            tool_registry_handle: Some(tool_registry_handle.clone()),
            audit_trail: audit_trail_handle.clone(),
            audit_journal: audit_journal_handle.clone(),
            obs_config: self.config.obs_config.clone(),
            llm_call_repository: llm_call_repository.clone(),
            trigger_def_repo: Some(trigger_def_repo.clone()),
            notification_repo: Some(notification_repo.clone()),
            notification_engine_handle: notification_engine.clone(),
            chat_manager: chat_manager.clone(),
            plan_cache: plan_cache.clone(),
            mailbox_handle: Some(mailbox_handle.clone()),
            user_memory: user_memory.clone(),
            stt_engine: shared_stt_engine.clone(),
            stt_repository: shared_stt_repository.clone(),
            data_dir: self.config.data_dir.clone(),
            stt_config_repo: stt_config_repo.clone(),
            mcp_handle: mcp_handle.clone(),
            mcp_server_repo: mcp_server_repo.clone(),
            llm_backend_repo: llm_backend_repo.clone(),
            a2a_invoker: Some(a2a_invoker),
            resilience_layer: Some(shared_resilience_layer.clone()),
            runner_proxy: runner_supervisor.as_ref().map(|s| s.proxy()),
            llama_server_supervisor: llama_server_supervisor.clone(),
        };
        let api_server = APIServer::new(self.config.api_config, state);

        let api_start = tokio::time::timeout(timeout, api_server.start()).await;
        let api_handle = match flatten_api_start(api_start, self.config.startup_timeout_secs) {
            Ok(handle) => handle,
            Err(e) => {
                // Rollback: stop actors in reverse order (TriggerEngine → … ).
                rollback_startup_actors(
                    &trigger_engine,
                    &router_handle,
                    &tool_registry_handle,
                    &registry_handle,
                )
                .await;
                return Err(e);
            }
        };
        info!("supervisor.api.ready");

        // Phase 8 (pos 9): TimeoutWatcher, started when task_repository is configured.
        if let Some(ref repo) = task_repository {
            info!("supervisor.timeout_watcher.starting");
            let watcher = TimeoutWatcher::new(
                TimeoutWatcherConfig {
                    input_required_timeout: self
                        .config
                        .hitl_config
                        .timeout_hours
                        .map(|h| Duration::from_secs(h * 3600)),
                    scan_interval: Duration::from_secs(self.config.hitl_config.scan_interval_secs),
                },
                Arc::clone(repo),
                event_sender.clone(),
            );
            tokio::spawn(watcher.run());
            info!("supervisor.timeout_watcher.started");
        }

        // Emit AllReady
        let _ = event_sender.send(RuntimeEvent::AllReady);
        info!("supervisor.all_ready");

        // Drain the AllReady event from the startup receiver
        drain_until_all_ready(&mut startup_rx, timeout).await;

        // Onboarding detection: emit OnboardingRequired if UserMemory is empty.
        // Non-blocking, the runtime is fully operational regardless of the result.
        emit_onboarding_if_needed(user_memory.as_ref(), &event_sender);

        // Phase 10.5: Auto-install bundled agents
        //
        // If bundled_agents_path is configured and manifest.json is present,
        // register each auto_install agent in the DB so Phase 11 picks them up.
        // Agents already in the DB are skipped. Errors are logged, never fatal.
        if let Some(ref repo) = self.config.agent_repository {
            auto_load_bundled_agents(
                self.config.bundled_agents_path.as_deref(),
                repo,
                &agent_loader_for_autoload,
            );
        }

        // Phase 10.6: Validate installed package integrity
        //
        // Lightweight check: for each installed package, verify root_path still
        // exists on disk. If missing → log warning, disable all package agents.
        // Never blocks boot. Phase 11 handles the actual loading.
        if let (Some(ref pkg_repo), Some(ref repo)) = (
            &self.config.package_repository,
            &self.config.agent_repository,
        ) {
            validate_installed_packages(pkg_repo, repo);
        }

        // Phase 11: Auto-load installed agents
        //
        // After AllReady, load all enabled agents from the repository,
        // validate via AgentLoader, register in AgentRegistry, transition to
        // Active, create an ExecutionCoordinator, and register in TaskRouter.
        // This mirrors the full start_agent flow from routes_agents.rs.
        // Errors are logged but never block the boot (graceful degradation).
        Self::auto_load_installed_agents(AutoLoadCtx {
            agent_loader: &agent_loader_for_autoload,
            backend_factory: &backend_factory_for_autoload,
            base_backend: &backend_for_autoload,
            registry_handle: &registry_handle,
            router_handle: &router_handle,
            event_sender: &event_sender,
            task_repository: task_repository.as_ref(),
            agent_repository: self.config.agent_repository.as_ref(),
            data_dir: &self.config.data_dir,
            obs_config: &self.config.obs_config,
        })
        .await;

        Ok(SupervisorHandles {
            event_sender,
            registry_handle,
            tool_registry_handle,
            router_handle,
            api_handle,
            llm_router,
            trigger_engine,
            audit_trail: audit_trail_handle,
            task_repository: task_repository.clone(),
            pending_approvals: pending_approvals.clone(),
            plan_gates: plan_gates.clone(),
            notification_engine,
            llm_call_repository,
            chat_manager,
            plan_cache,
            mailbox_handle: Some(mailbox_handle),
            user_memory,
            stt_engine: shared_stt_engine,
            stt_repository: shared_stt_repository,
            mcp_handle,
            project_repository,
            runner_supervisor,
            llama_server_supervisor,
        })
    }
}
