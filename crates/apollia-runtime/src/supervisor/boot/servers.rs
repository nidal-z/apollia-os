//! Servers at boot: the compatible server, the application state, the API
//! server with its rollback, and the HITL timeout watcher.

use crate::supervisor::*;

use super::agents::Executors;
use super::data::Stores;
use super::governance::Governance;
use super::inference::{Inference, Stt};
use super::Core;
use crate::supervisor::bootstrap::{flatten_api_start, rollback_startup_actors};

/// What `AppState` takes by value rather than sharing with the handles.
pub(super) struct StateOwned<B: ExecutionBackend + Clone> {
    pub(super) agent_loader: Arc<dyn AgentLoader>,
    pub(super) backend: B,
    pub(super) backend_factory: Option<Arc<dyn crate::api::routes_agents::AgentBackendFactory>>,
    pub(super) batches: Option<crate::batch::BatchHandle>,
    pub(super) a2a_invoker: Arc<crate::a2a::A2AInvoker>,
    pub(super) resilience: Arc<std::sync::Mutex<apollia_oria::ResilienceLayer>>,
    pub(super) compat_server: crate::compat_server::CompatServerHandle,
}

impl Supervisor {
    /// The compatible server, which reads and writes its switch in the file
    /// the runtime was started with, or in the data directory's apollia.toml,
    /// the file the desktop reads, when it was started without one.
    ///
    /// It serves `agents`, the exposed agents and workflows, as models, and
    /// writes each request it answers to `journal`.
    pub(super) fn spawn_compat_server(
        &self,
        inference: &Inference,
        event_sender: &EventBusSender,
        agents: crate::compat_server::ExposedAgents,
        journal: Option<AuditJournalHandle>,
    ) -> crate::compat_server::CompatServerHandle {
        let settings = self
            .config
            .config_path
            .clone()
            .unwrap_or_else(|| self.config.data_dir.join("apollia.toml"));
        crate::compat_server::CompatServerHandle::spawn(
            settings.clone(),
            crate::compat_server::ServerRecords {
                system_db: inference.system_db_path.clone(),
                journal,
            },
            crate::compat_server::LocalModels::new(
                inference.llama_server_supervisor.clone(),
                inference.mlx_engine.clone(),
                Some(crate::installed_models::models_dir(&self.config.data_dir)),
            )
            .with_files(settings, inference.system_db_path.clone()),
            agents,
            event_sender,
        )
    }

    /// The state every API route reads.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn app_state<B: ExecutionBackend + Clone>(
        &self,
        core: &Core<B>,
        inference: &Inference,
        governance: &Governance,
        stores: &Stores,
        stt: &Stt,
        executors: &Executors,
        chat_manager: &Option<crate::chat::ChatSessionManagerHandle>,
        owned: StateOwned<B>,
    ) -> AppState<B> {
        // Wrap the boot snapshot in a shared, swappable cell so the reload
        // route can replace the active router without restarting the daemon.
        // Other consumers (chat manager, embedded runtime) keep their own
        // direct handle and stay reachable via the supervisor handle struct.
        let shared_llm_router =
            crate::api::server::shared_llm_router_from(inference.llm_router.clone());
        AppState {
            router_handle: core.router_handle.clone(),
            registry_handle: core.registry_handle.clone(),
            event_sender: core.event_sender.clone(),
            agent_loader: owned.agent_loader,
            backend: owned.backend,
            llm_router: shared_llm_router.clone(),
            trigger_engine: Some(executors.trigger_engine.clone()),
            config_path: self.config.config_path.clone(),
            task_repository: stores.task_repository.clone(),
            pending_approvals: stores.pending_approvals.clone(),
            plan_gates: stores.plan_gates.clone(),
            notification_config: stores.notification_config.clone(),
            backend_factory: owned.backend_factory,
            tool_registry_handle: Some(core.tool_registry_handle.clone()),
            audit_trail: governance.audit_trail.clone(),
            audit_journal: governance.audit_journal.clone(),
            connection_governance: governance.connection.clone(),
            governance: Some(governance.handle.clone()),
            inference_governance: crate::governance::InferenceGovernance::new(Arc::clone(
                &core.org_policy,
            )),
            obs_config: self.config.obs_config.clone(),
            config_reload: None,
            llm_call_repository: inference.llm_call_repository.clone(),
            trigger_def_repo: Some(core.trigger_def_repo.clone()),
            notification_repo: Some(stores.notification_repo.clone()),
            notification_engine_handle: stores.notification_engine.clone(),
            chat_manager: chat_manager.clone(),
            plan_cache: stores.plan_cache.clone(),
            mailbox_handle: Some(stores.mailbox_handle.clone()),
            user_memory: stores.user_memory.clone(),
            stt_engine: stt.engine.clone(),
            stt_repository: stt.repository.clone(),
            data_dir: self.config.data_dir.clone(),
            stt_config_repo: stt.config_repo.clone(),
            mcp_handle: core.mcp_handle.clone(),
            mcp_server_repo: core.mcp_server_repo.clone(),
            llm_backend_repo: inference.llm_backend_repo.clone(),
            a2a_invoker: Some(owned.a2a_invoker),
            resilience_layer: Some(owned.resilience),
            runner_proxy: inference.runner_supervisor.as_ref().map(|s| s.proxy()),
            llama_server_supervisor: inference.llama_server_supervisor.clone(),
            mlx_engine: inference.mlx_engine.clone(),
            secret_store: crate::inference_servers::default_store_opener(),
            batches: owned.batches,
            workflows: executors.workflows.clone(),
            compat_server: Some(owned.compat_server),
            source_registry: inference.source_registry.clone(),
        }
    }
}

/// Starts the API server within the startup timeout; on failure, stops the
/// actors already started in reverse order (TriggerEngine → … ).
pub(super) async fn serve_api<B: ExecutionBackend + Clone>(
    api_server: APIServer,
    timeout: Duration,
    startup_timeout_secs: u64,
    core: &Core<B>,
    trigger_engine: &TriggerEngineHandle,
) -> Result<APIServerHandle, SupervisorError> {
    let api_start = tokio::time::timeout(timeout, api_server.start()).await;
    match flatten_api_start(api_start, startup_timeout_secs) {
        Ok(handle) => Ok(handle),
        Err(e) => {
            rollback_startup_actors(
                trigger_engine,
                &core.router_handle,
                &core.tool_registry_handle,
                &core.registry_handle,
            )
            .await;
            Err(e)
        }
    }
}

/// Phase 8 (pos 9): TimeoutWatcher, started when the task repository is
/// configured.
pub(super) fn spawn_timeout_watcher(
    hitl_config: &HitlConfig,
    live: &crate::config_reload::Live<HitlConfig>,
    task_repository: Option<&Arc<TaskRepository>>,
    event_sender: &EventBusSender,
) {
    if let Some(repo) = task_repository {
        info!("supervisor.timeout_watcher.starting");
        let watcher = TimeoutWatcher::new(
            TimeoutWatcherConfig::from(hitl_config),
            Arc::clone(repo),
            event_sender.clone(),
        )
        .following(live.clone());
        tokio::spawn(watcher.run());
        info!("supervisor.timeout_watcher.started");
    }
}
