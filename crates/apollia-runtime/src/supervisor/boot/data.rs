//! Data at boot: the execution trace and its observers, the trigger
//! definitions, and the stores the API server and the agents share (tasks,
//! user memory, notifications, plan cache, mailbox, and the migration of the
//! former projects).

use crate::supervisor::*;

use crate::supervisor::bootstrap::{
    load_trigger_definitions, seed_default_desktop_channel_if_needed,
};
use crate::supervisor::persistence::{open_plan_cache, open_task_repository, open_user_memory};

/// The trigger definitions read at boot and the repository they come from.
pub(super) struct TriggerDefinitions {
    pub(super) definitions: Vec<apollia_triggers::TriggerDefinition>,
    pub(super) repository: TriggerDefinitionRepository,
}

/// The stores opened before the API server, in the order they are opened.
pub(super) struct Stores {
    pub(super) task_repository: Option<Arc<TaskRepository>>,
    pub(super) pending_approvals: Option<Arc<PendingApprovals>>,
    pub(super) plan_gates: Option<Arc<apollia_oria::PendingPlanGates>>,
    pub(super) user_memory:
        Option<Arc<std::sync::Mutex<apollia_memory::user_memory::UserMemoryRepository>>>,
    pub(super) notification_config: Option<NotificationConfig>,
    pub(super) notification_repo: Arc<std::sync::Mutex<NotificationConfigRepository>>,
    pub(super) notification_engine: Option<NotificationEngineHandle>,
    pub(super) plan_cache:
        Option<Arc<std::sync::Mutex<apollia_oria::plan_cache::PlanCacheRepository>>>,
    pub(super) mailbox_handle: crate::mailbox::AgentMailboxHandle,
}

impl Supervisor {
    /// Phases 4c and 4d: the event persistor and the shared resilience layer.
    pub(super) async fn spawn_trace_observers(
        &self,
        event_sender: &EventBusSender,
    ) -> Arc<std::sync::Mutex<apollia_oria::ResilienceLayer>> {
        // Phase 4c: EventPersistor, an append-only log of the agent execution
        // trace (AgentLog, plus thoughts, tool_call_*, llm_call_*, etc.). If the
        // open fails, the runtime continues without persistence (logs still go
        // to `tracing`).
        self.spawn_event_persistor(event_sender).await;

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
            event_sender,
        );
        info!("supervisor.resilience.ready");
        shared_resilience_layer
    }

    /// Phase 6 (pos 7), first half: the trigger definitions, read once the
    /// TaskRouter is up. The engine itself starts once the agent executor and
    /// the workflow engine exist: a trigger starts either.
    pub(super) fn open_trigger_definitions(&self) -> Result<TriggerDefinitions, SupervisorError> {
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
        let definitions = load_trigger_definitions(&trigger_def_repo)?;
        Ok(TriggerDefinitions {
            definitions,
            repository: trigger_def_repo,
        })
    }

    /// Phases 9 to 13b: the stores the API server is built over.
    pub(super) async fn open_stores(
        &self,
        event_sender: &EventBusSender,
    ) -> Result<Stores, SupervisorError> {
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
            self.spawn_notification_engine(notification_config_for_engine.as_ref(), event_sender)?;

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
        // to support the seed bootstrap of the default desktop channel.

        // Phase 13a: the former projects become sources, before the chat
        // manager loads the sessions it detaches.
        crate::project_migration::run_at_boot(&self.config.data_dir).await;

        Ok(Stores {
            task_repository,
            pending_approvals,
            plan_gates,
            user_memory,
            notification_config: notification_config_for_state,
            notification_repo,
            notification_engine,
            plan_cache,
            mailbox_handle,
        })
    }
}
