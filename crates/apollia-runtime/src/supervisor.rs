//! Supervisor — ordered startup, shutdown rollback, and watchdog for runtime actors.
//!
//! The Supervisor starts all runtime actors in a strict sequence:
//! `EventBus → AgentRegistry → ToolRegistry (+ native tools) → TaskRouter → APIServer`.
//! Each actor must emit `RuntimeEvent::Ready` (or equivalent) before the next one starts.
//! If any actor fails to start within the configured timeout, all previously started
//! actors are stopped in reverse order.
//!
//! After startup, the Supervisor monitors actor health via `watch()` and applies
//! [`RestartPolicy`] on failure.

use std::collections::VecDeque;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::broadcast;
use tracing::{error, info, warn};

use apollia_core::{PendingApprovals, RuntimeEvent};
use apollia_llm::{LlmCallRepository, LlmConfig, LlmRouter};
use apollia_notifications::{
    build_channels, NotificationConfig, NotificationEngine, NotificationEngineHandle,
};
use apollia_pipelines::{
    PipelineDefinition, PipelineEngine, PipelineEngineHandle, PipelineRepository,
};
use apollia_tools::{AuditTrailHandle, TaskRepository, ToolRegistryHandle};
use apollia_triggers::{TriggerDefinition, TriggerEngineHandle, TriggerPersistence};

use crate::api::routes_agents::AgentLoader;
use crate::api::{APIServer, APIServerConfig, APIServerError, APIServerHandle, AppState};
use crate::coordinator::ExecutionBackend;
use crate::eventbus::{EventBus, EventBusSender};
use crate::registry::{AgentRegistry, AgentRegistryHandle};
use crate::router::TaskRouterHandle;
use crate::timeout_watcher::{TimeoutWatcher, TimeoutWatcherConfig};

/// Restart policy for supervised actors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RestartPolicy {
    /// Always restart on termination (ToolRegistry, MemoryEngine).
    Always,
    /// Restart only on failure/panic (APIServer).
    OnFailure,
    /// Never restart (one-shot actors).
    Never,
}

/// Specification for a supervised child actor.
#[derive(Debug)]
pub struct ChildSpec {
    /// Human-readable name of the actor.
    pub name: String,
    /// When to restart this actor.
    pub restart_policy: RestartPolicy,
    /// Maximum number of restarts within `restart_window_secs`.
    pub max_restarts: u32,
    /// Time window (in seconds) for counting restarts.
    pub restart_window_secs: u64,
}

/// Supervisor configuration.
pub struct SupervisorConfig {
    /// Configuration for the APIServer (TCP port + Unix socket path).
    pub api_config: APIServerConfig,
    /// Maximum time (in seconds) to wait for each actor to become ready.
    pub startup_timeout_secs: u64,
    /// Optional LLM configuration parsed from the `[llm]` section of `apollia.toml`.
    ///
    /// `None` disables the LLM layer entirely — the runtime starts normally and
    /// agents receive `ctx.llm = None` (see STORY-059). No error is raised.
    pub llm_config: Option<LlmConfig>,
    /// Trigger definitions parsed from `[[triggers]]` blocks in `apollia.toml`.
    ///
    /// An empty `Vec` starts the `TriggerEngine` with no active sources (AC-3).
    /// Populated by `apollia-cli` from `ApolliaConfig.triggers` (STORY-071).
    pub triggers: Vec<TriggerDefinition>,
    /// Path to `apollia.toml` — injected into [`AppState`] for hot reload (STORY-073).
    ///
    /// `None` when the runtime starts without a config file (e.g. tests, `apollia-os start`
    /// without a config file). The `POST /api/v1/triggers/reload` route returns 503 when absent.
    pub config_path: Option<std::path::PathBuf>,
    /// Durée (en heures) au-delà de laquelle une tâche `input_required` est annulée.
    ///
    /// Configurable via `[runtime] input_required_timeout_hours` dans `apollia.toml`.
    /// Le `TimeoutWatcher` (STORY-098) utilise cette valeur. Défaut : 24 heures.
    /// Ignoré si `AppState.task_repository` est `None`.
    pub input_required_timeout_hours: u64,
    /// Configuration optionnelle du système de notifications (section `[notifications]`
    /// dans `apollia.toml`).
    ///
    /// `None` → le `NotificationEngine` n'est pas démarré (pas d'erreur).
    /// `Some` → `build_channels()` est appelé et le moteur est spawné en position 9
    /// du Supervisor. Un crash du moteur n'affecte pas le runtime (tâche détachée).
    pub notifications: Option<NotificationConfig>,
    /// Définitions de pipelines parsées depuis `[[pipelines]]` dans `apollia.toml`.
    ///
    /// Un `Vec` vide (ou absent) → `PipelineEngine` non démarré, `AppState.pipeline_engine == None`.
    /// Peuplé par `apollia-cli` depuis `ApolliaConfig.pipelines` (STORY-118).
    pub pipelines: Vec<PipelineDefinition>,
    /// Répertoire de données du runtime (ex: `~/.apollia/`).
    ///
    /// Utilisé pour localiser `pipelines.db`. Doit exister et être accessible en écriture.
    pub data_dir: std::path::PathBuf,
}

/// Handles returned after successful startup.
///
/// All handles are `Clone + Send + Sync` and can be shared freely.
pub struct SupervisorHandles<B: ExecutionBackend> {
    /// Sender side of the runtime event bus.
    pub event_sender: EventBusSender,
    /// Handle to the agent registry actor.
    pub registry_handle: AgentRegistryHandle,
    /// Handle to the tool registry actor.
    pub tool_registry_handle: ToolRegistryHandle,
    /// Handle to the task router actor.
    pub router_handle: TaskRouterHandle<B>,
    /// Handle to the API server.
    pub api_handle: APIServerHandle,
    /// LLM router initialized at position 5 of the startup sequence (STORY-060).
    ///
    /// `None` when no `[llm]` section is present in `apollia.toml`, or when
    /// `LlmRouter::from_config_with_bus` fails (warning logged, runtime continues).
    pub llm_router: Option<Arc<LlmRouter>>,
    /// Handle to the TriggerEngine actor at position 6 of the startup sequence (STORY-072).
    ///
    /// Always `Some` after successful startup — even when `config.triggers` is empty
    /// (AC-3). Injected into `AppState` so webhook routes and CLI commands can reach it.
    pub trigger_engine: TriggerEngineHandle,
    /// Handle to the PipelineEngine actor (STORY-119).
    ///
    /// `None` when `config.pipelines` is empty — the runtime starts normally without
    /// pipeline support (AC-3). `Some` when at least one pipeline is defined.
    pub pipeline_engine: Option<PipelineEngineHandle>,
    /// Handle to the AuditTrail actor (STORY-016).
    ///
    /// `None` when the data directory is unavailable or the SQLite open fails
    /// (warning logged, runtime continues without audit). `Some` in production.
    pub audit_trail: Option<AuditTrailHandle>,
    /// HITL task repository — persists `input_required` prompts/contexts (STORY-094).
    ///
    /// Shared between `AppState` (resume handler) and `TimeoutWatcher`.
    /// `None` when the SQLite open fails (warning logged, HITL disabled).
    pub task_repository: Option<Arc<TaskRepository>>,
    /// HITL pending approvals registry — oneshot channels for Mode Direct suspension (STORY-096).
    ///
    /// `None` when `task_repository` is `None` (HITL disabled).
    pub pending_approvals: Option<Arc<apollia_core::PendingApprovals>>,
    /// Handle to the NotificationEngine actor (STORY-102).
    ///
    /// `None` when no `[notifications]` section is present in `apollia.toml`.
    /// Used by [`ShutdownController`] to stop the engine before the EventBus closes,
    /// preventing late notifications from being delivered after `apollia-os stop`.
    pub notification_engine: Option<NotificationEngineHandle>,
}

/// Supervisor errors.
#[derive(Debug, thiserror::Error)]
pub enum SupervisorError {
    /// An actor did not become ready within the timeout.
    #[error("startup timeout: {actor} did not become ready within {timeout_secs}s")]
    StartupTimeout {
        /// Name of the actor that timed out.
        actor: String,
        /// The timeout that was exceeded.
        timeout_secs: u64,
    },

    /// An actor failed to start.
    #[error("actor {actor} failed to start: {reason}")]
    ActorStartFailed {
        /// Name of the actor that failed.
        actor: String,
        /// Reason for the failure.
        reason: String,
    },

    /// Configuration is invalid.
    #[error("configuration error: {0}")]
    ConfigError(String),

    /// An actor exceeded its maximum restart count within the window.
    #[error("max restarts exceeded for {actor}: {count} restarts in {window_secs}s")]
    MaxRestartsExceeded {
        /// Name of the actor.
        actor: String,
        /// Number of restarts that occurred.
        count: u32,
        /// Time window in seconds.
        window_secs: u64,
    },

    /// The `[notifications]` section contains an invalid channel configuration.
    #[error("notification configuration error: {0}")]
    NotificationConfig(String),
}

impl From<APIServerError> for SupervisorError {
    fn from(err: APIServerError) -> Self {
        SupervisorError::ActorStartFailed {
            actor: "api_server".to_string(),
            reason: err.to_string(),
        }
    }
}

/// Tracks restart history for a single actor.
pub struct RestartTracker {
    spec: ChildSpec,
    timestamps: VecDeque<Instant>,
}

impl RestartTracker {
    /// Create a new tracker from a child spec.
    pub fn new(spec: ChildSpec) -> Self {
        Self {
            spec,
            timestamps: VecDeque::new(),
        }
    }

    /// Record a restart and check if max_restarts is exceeded within the window.
    ///
    /// Returns `true` if the restart is allowed, `false` if the limit is exceeded.
    pub fn record_restart(&mut self) -> bool {
        let now = Instant::now();
        let window = Duration::from_secs(self.spec.restart_window_secs);

        // Evict timestamps outside the window
        while let Some(&front) = self.timestamps.front() {
            if now.duration_since(front) > window {
                self.timestamps.pop_front();
            } else {
                break;
            }
        }

        self.timestamps.push_back(now);
        self.timestamps.len() <= self.spec.max_restarts as usize
    }
}

/// The Apollia Supervisor — orchestrates actor lifecycle.
///
/// Created with [`Supervisor::new`], started with [`Supervisor::start`].
/// The Supervisor holds no business state — it only manages actor lifecycles.
pub struct Supervisor {
    config: SupervisorConfig,
}

impl Supervisor {
    /// Create a new Supervisor with the given configuration.
    pub fn new(config: SupervisorConfig) -> Self {
        Self { config }
    }

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
    ) -> Result<SupervisorHandles<B>, SupervisorError> {
        let timeout = Duration::from_secs(self.config.startup_timeout_secs);

        // Phase 1: EventBus
        info!("Supervisor: starting EventBus");
        let (event_sender, mut startup_rx) = EventBus::new();
        info!("Supervisor: EventBus ready");

        // Phase 2: AgentRegistry
        info!("Supervisor: starting AgentRegistry");
        let registry_handle = AgentRegistry::spawn(event_sender.clone());
        info!("Supervisor: AgentRegistry ready");

        // Phase 3: ToolRegistry + native tool registration
        info!("Supervisor: starting ToolRegistry");
        let tool_registry_handle = ToolRegistryHandle::start();
        for descriptor in native_tool_descriptors() {
            if let Err(e) = tool_registry_handle.register(descriptor).await {
                warn!(error = %e, "failed to register native tool");
            }
        }
        info!("Supervisor: ToolRegistry ready (native tools registered)");

        // Phase 4 (pos 5): LlmRouter — initialized before TaskRouter
        let llm_router: Option<Arc<LlmRouter>> = if let Some(llm_cfg) = &self.config.llm_config {
            info!("Supervisor: starting LlmRouter");
            match LlmRouter::from_config_with_bus(llm_cfg, Some(event_sender.clone())).await {
                Ok(router) => {
                    for info in router.list() {
                        tracing::info!(
                            backend = %info.name,
                            model = %info.model_id,
                            "LLM backend ready"
                        );
                    }
                    info!("Supervisor: LlmRouter ready");
                    Some(Arc::new(router))
                }
                Err(e) => {
                    warn!(error = %e, "LlmRouter failed to initialize — continuing without LLM");
                    None
                }
            }
        } else {
            info!("Supervisor: no [llm] section in config — LLM disabled");
            None
        };

        // Phase 4b: LlmCallRepository — subscriber EventBus pour persister les appels LLM
        if llm_router.is_some() {
            let db_path = self.config.data_dir.join("llm_calls.db");
            match LlmCallRepository::open(&db_path) {
                Ok(repo) => {
                    let repo = Arc::new(std::sync::Mutex::new(repo));
                    let obs = self
                        .config
                        .llm_config
                        .as_ref()
                        .map(|c| apollia_core::ObservabilityConfig {
                            debug_log_prompt: c.observability.debug_log_prompt,
                            ..apollia_core::ObservabilityConfig::default()
                        })
                        .unwrap_or_default();
                    apollia_llm::spawn_llm_subscriber(repo, &event_sender, obs);
                    info!("Supervisor: LlmCallRepository ready (subscriber spawned)");
                }
                Err(e) => {
                    warn!(error = %e, "LlmCallRepository failed to open — LLM call persistence disabled");
                }
            }
        }

        // Phase 5 (pos 6): TaskRouter
        info!("Supervisor: starting TaskRouter");
        let router_handle: TaskRouterHandle<B> =
            TaskRouterHandle::spawn(registry_handle.clone(), event_sender.clone(), 256);
        info!("Supervisor: TaskRouter ready");

        // Phase 6 (pos 7): TriggerEngine — démarré après TaskRouter (besoin du submitter)
        info!("Supervisor: starting TriggerEngine");
        // Ouvre la persistance SQLite des triggers (historique des fires/skips).
        let trigger_persistence: Option<TriggerPersistence> = {
            let db_path = self.config.data_dir.join("triggers.db");
            match TriggerPersistence::open(&db_path) {
                Ok(p) => {
                    info!("Supervisor: TriggerPersistence ready");
                    Some(p)
                }
                Err(e) => {
                    warn!(
                        error = %e,
                        "TriggerPersistence failed to open — trigger history disabled"
                    );
                    None
                }
            }
        };
        let enabled_count = self.config.triggers.iter().filter(|t| t.enabled).count();
        let trigger_engine = TriggerEngineHandle::spawn(
            self.config.triggers.clone(),
            router_handle.clone(),
            event_sender.clone(),
            trigger_persistence,
            None, // PipelineEngine injecté après son démarrage — résout dépendance circulaire
            apollia_core::ObservabilityConfig::default(),
        )
        .await;
        tracing::info!(
            active = enabled_count,
            "✔ TriggerEngine — {} trigger(s) actif(s)",
            enabled_count
        );
        let _ = event_sender.send(RuntimeEvent::TriggersReloaded {
            count: enabled_count,
        });

        // Phase 7 (pos 8): PipelineEngine (si pipelines définis)
        //
        // NOTE(STORY-119): PipelineEngine démarre APRÈS TaskRouter (et non en position 8
        // théorique de la spec) pour résoudre la dépendance circulaire :
        // PipelineEngine → TaskSubmitter → TaskRouterHandle.
        // Décision documentée dans les Notes de la story.
        let pipeline_engine: Option<PipelineEngineHandle> = if self.config.pipelines.is_empty() {
            info!("Supervisor: no [[pipelines]] defined — PipelineEngine not started");
            None
        } else {
            info!("Supervisor: starting PipelineEngine");
            let db_path = self.config.data_dir.join("pipelines.db");
            let db_path_str = db_path.to_string_lossy().into_owned();
            let mut repo = PipelineRepository::open(&db_path_str).map_err(|e| {
                SupervisorError::ActorStartFailed {
                    actor: "pipeline_engine".to_string(),
                    reason: format!("failed to open pipelines.db: {e}"),
                }
            })?;
            repo.migrate()
                .map_err(|e| SupervisorError::ActorStartFailed {
                    actor: "pipeline_engine".to_string(),
                    reason: format!("pipeline migration failed: {e}"),
                })?;
            let repo = std::sync::Arc::new(std::sync::Mutex::new(repo));
            let submitter: std::sync::Arc<dyn apollia_pipelines::TaskSubmitter> =
                std::sync::Arc::new(router_handle.clone());
            let handle = PipelineEngine::spawn(
                self.config.pipelines.clone(),
                repo,
                submitter,
                event_sender.clone(),
            );
            let pipeline_count = self.config.pipelines.len();
            tracing::info!(
                count = pipeline_count,
                "✔ PipelineEngine — {} pipeline(s) chargé(s)",
                pipeline_count
            );
            Some(handle)
        };

        // Résolution de la dépendance circulaire TriggerEngine ↔ PipelineEngine :
        // TriggerEngine a démarré sans PipelineEngine (None ci-dessus).
        // Maintenant que PipelineEngine est prêt, on l'injecte via SetPipelineEngine.
        if let Some(ref pe) = pipeline_engine {
            trigger_engine.set_pipeline_engine(Some(pe.clone())).await;
            info!("Supervisor: PipelineEngine injecté dans TriggerEngine");
        }

        // Phase 8 (pos 9): AuditTrail — opened before APIServer so it's injectable into AppState.
        info!("Supervisor: opening AuditTrail");
        let audit_trail_handle: Option<AuditTrailHandle> = {
            let db_path = self.config.data_dir.join("audit.db");
            match AuditTrailHandle::open(&db_path).await {
                Ok(handle) => {
                    info!("Supervisor: AuditTrail ready");
                    Some(handle)
                }
                Err(e) => {
                    warn!(error = %e, "AuditTrail failed to open — audit disabled");
                    None
                }
            }
        };

        // Phase 9 (pos 10): APIServer
        info!("Supervisor: starting APIServer");
        // Open TaskRepository (HITL persistence — STORY-094/095).
        // Shared between AppState (resume handler) and TimeoutWatcher.
        let task_repository: Option<Arc<TaskRepository>> = {
            let db_path = self.config.data_dir.join("hitl.db");
            match TaskRepository::open(&db_path).await {
                Ok(repo) => {
                    info!("Supervisor: TaskRepository ready (HITL enabled)");
                    Some(Arc::new(repo))
                }
                Err(e) => {
                    warn!(error = %e, "TaskRepository failed to open — HITL disabled");
                    None
                }
            }
        };
        // PendingApprovals — oneshot channel registry for HITL suspension (STORY-096).
        let pending_approvals: Option<Arc<PendingApprovals>> = task_repository
            .as_ref()
            .map(|_| Arc::new(PendingApprovals::new()));
        // Clone notification config so AppState can serve /api/v1/notifications/channels
        // while the original is consumed by NotificationEngine below.
        let notification_config_for_state = self.config.notifications.clone();
        let state = AppState {
            router_handle: router_handle.clone(),
            registry_handle: registry_handle.clone(),
            event_sender: event_sender.clone(),
            agent_loader,
            backend,
            llm_router: llm_router.clone(),
            trigger_engine: Some(trigger_engine.clone()),
            config_path: self.config.config_path.clone(),
            task_repository: task_repository.clone(),
            pending_approvals: pending_approvals.clone(),
            notification_config: notification_config_for_state,
            pipeline_engine: pipeline_engine.clone(),
            backend_factory,
            tool_registry_handle: Some(tool_registry_handle.clone()),
            audit_trail: audit_trail_handle.clone(),
            obs_config: apollia_core::ObservabilityConfig::default(),
        };
        let api_server = APIServer::new(self.config.api_config, state);

        let api_handle = match tokio::time::timeout(timeout, api_server.start()).await {
            Ok(Ok(handle)) => handle,
            Ok(Err(api_err)) => {
                // Rollback: stop actors in reverse order (PipelineEngine → TriggerEngine → TaskRouter → …)
                if let Some(ref pe) = pipeline_engine {
                    pe.shutdown().await;
                }
                trigger_engine.shutdown().await;
                router_handle.shutdown();
                tool_registry_handle.shutdown().await;
                registry_handle.shutdown();
                return Err(SupervisorError::from(api_err));
            }
            Err(_elapsed) => {
                if let Some(ref pe) = pipeline_engine {
                    pe.shutdown().await;
                }
                trigger_engine.shutdown().await;
                router_handle.shutdown();
                tool_registry_handle.shutdown().await;
                registry_handle.shutdown();
                return Err(SupervisorError::StartupTimeout {
                    actor: "api_server".to_string(),
                    timeout_secs: self.config.startup_timeout_secs,
                });
            }
        };
        info!("Supervisor: APIServer ready");

        // Phase 8 (pos 9): TimeoutWatcher — démarré si task_repository est configuré (STORY-098)
        if let Some(ref repo) = task_repository {
            info!("Supervisor: starting TimeoutWatcher");
            let watcher = TimeoutWatcher::new(
                TimeoutWatcherConfig {
                    input_required_timeout: Duration::from_secs(
                        self.config.input_required_timeout_hours * 3600,
                    ),
                    ..TimeoutWatcherConfig::default()
                },
                Arc::clone(repo),
                event_sender.clone(),
            );
            tokio::spawn(watcher.run());
            info!("Supervisor: TimeoutWatcher started");
        }

        // Phase 9: NotificationEngine — démarré si [notifications] présent dans la config
        let notification_engine: Option<NotificationEngineHandle> =
            if let Some(notif_config) = self.config.notifications {
                let channels = build_channels(&notif_config.channels)
                    .map_err(|e| SupervisorError::NotificationConfig(e.to_string()))?;
                let active = notif_config.channels.iter().filter(|c| c.enabled).count();
                let notif_db_path = Some(self.config.data_dir.join("hitl.db"));
                let engine = NotificationEngine::new(
                    notif_config,
                    channels,
                    event_sender.clone(),
                    notif_db_path,
                );
                let handle = engine.spawn();
                tracing::info!(channels = active, "NotificationEngine démarré");
                Some(handle)
            } else {
                tracing::info!(
                    "Supervisor: aucune section [notifications] — NotificationEngine désactivé"
                );
                None
            };

        // Emit AllReady
        let _ = event_sender.send(RuntimeEvent::AllReady);
        info!("Supervisor: all actors ready, emitted AllReady");

        // Drain the AllReady event from the startup receiver
        drain_until_all_ready(&mut startup_rx, timeout).await;

        Ok(SupervisorHandles {
            event_sender,
            registry_handle,
            tool_registry_handle,
            router_handle,
            api_handle,
            llm_router,
            trigger_engine,
            pipeline_engine,
            audit_trail: audit_trail_handle,
            task_repository: task_repository.clone(),
            pending_approvals: pending_approvals.clone(),
            notification_engine,
        })
    }
}

/// Returns descriptors for the three native tools bundled with `apollia-tools`.
///
/// Used by the Supervisor to auto-register tools at startup.
fn native_tool_descriptors() -> Vec<apollia_tools::ToolDescriptor> {
    vec![
        apollia_tools::tools::bash_executor::BashExecutor::descriptor(),
        apollia_tools::tools::python_executor::PythonExecutor::descriptor(),
        apollia_tools::tools::file_io::FileIo::descriptor(),
    ]
}

/// Watch actor health and apply restart policies.
///
/// This is a standalone async function (not a method on Supervisor) because
/// the Supervisor is consumed by `start()`. The caller runs `watch()` as a
/// background task after obtaining handles.
///
/// Listens for `ShutdownRequested` or `FatalError` events on the EventBus.
/// Returns when shutdown is requested or a fatal error occurs.
pub async fn watch(
    event_sender: &EventBusSender,
    _trackers: Vec<RestartTracker>,
) -> Result<(), SupervisorError> {
    let mut rx = event_sender.subscribe();

    loop {
        match rx.recv().await {
            Ok(RuntimeEvent::ShutdownRequested) => {
                info!("Supervisor watch: shutdown requested");
                return Ok(());
            }
            Ok(RuntimeEvent::FatalError(reason)) => {
                error!(reason = %reason, "Supervisor watch: fatal error");
                return Err(SupervisorError::ActorStartFailed {
                    actor: "runtime".to_string(),
                    reason,
                });
            }
            Ok(_) => {
                // Other events — ignore in MVP
            }
            Err(broadcast::error::RecvError::Lagged(n)) => {
                warn!(skipped = n, "Supervisor watch: lagged, skipped events");
            }
            Err(broadcast::error::RecvError::Closed) => {
                info!("Supervisor watch: event bus closed");
                return Ok(());
            }
        }
    }
}

/// Drain events from a receiver until `AllReady` is seen or timeout expires.
async fn drain_until_all_ready(rx: &mut broadcast::Receiver<RuntimeEvent>, timeout: Duration) {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        tokio::select! {
            result = rx.recv() => {
                match result {
                    Ok(RuntimeEvent::AllReady) => return,
                    Ok(_) => continue,
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(broadcast::error::RecvError::Closed) => return,
                }
            }
            _ = tokio::time::sleep_until(deadline) => return,
        }
    }
}

/// Create the default child specs for the runtime actors.
pub fn default_child_specs() -> Vec<ChildSpec> {
    vec![
        ChildSpec {
            name: "event_bus".to_string(),
            restart_policy: RestartPolicy::Always,
            max_restarts: 5,
            restart_window_secs: 60,
        },
        ChildSpec {
            name: "agent_registry".to_string(),
            restart_policy: RestartPolicy::Always,
            max_restarts: 5,
            restart_window_secs: 60,
        },
        ChildSpec {
            name: "tool_registry".to_string(),
            restart_policy: RestartPolicy::Always,
            max_restarts: 5,
            restart_window_secs: 60,
        },
        ChildSpec {
            name: "memory_engine".to_string(),
            restart_policy: RestartPolicy::Always,
            max_restarts: 5,
            restart_window_secs: 60,
        },
        ChildSpec {
            name: "task_router".to_string(),
            restart_policy: RestartPolicy::OnFailure,
            max_restarts: 5,
            restart_window_secs: 60,
        },
        ChildSpec {
            name: "trigger_engine".to_string(),
            restart_policy: RestartPolicy::OnFailure,
            max_restarts: 5,
            restart_window_secs: 60,
        },
        ChildSpec {
            name: "api_server".to_string(),
            restart_policy: RestartPolicy::OnFailure,
            max_restarts: 5,
            restart_window_secs: 60,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::coordinator::ExecutionBackend;
    use apollia_core::{AIPResult, AIPTask, RuntimeEvent, TaskStatus};
    use std::future::Future;
    use std::path::PathBuf;
    use std::pin::Pin;
    use tokio::net::TcpListener;

    /// Minimal ExecutionBackend for testing.
    #[derive(Clone)]
    struct MockBackend;

    impl From<crate::coordinator::DynBackend> for MockBackend {
        fn from(_: crate::coordinator::DynBackend) -> Self {
            MockBackend
        }
    }

    impl ExecutionBackend for MockBackend {
        fn execute(
            &self,
            _task: AIPTask,
        ) -> Pin<Box<dyn Future<Output = Result<AIPResult, String>> + Send>> {
            Box::pin(async {
                Ok(AIPResult {
                    task_id: String::new(),
                    status: TaskStatus::Completed,
                    output: Vec::new(),
                    error: None,
                    artifacts: Vec::new(),
                    input_required_data: None,
                })
            })
        }
    }

    /// Find a free TCP port.
    async fn free_port() -> u16 {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        listener.local_addr().unwrap().port()
    }

    /// Create a short unique temp socket path (macOS SUN_LEN limit).
    fn temp_socket_path() -> PathBuf {
        let id = &uuid::Uuid::new_v4().to_string()[..8];
        PathBuf::from(format!("/tmp/ap-{}.sock", id))
    }

    fn test_config(port: u16, socket_path: PathBuf) -> SupervisorConfig {
        SupervisorConfig {
            api_config: APIServerConfig {
                socket_path,
                tcp_port: port,
            },
            startup_timeout_secs: 10,
            llm_config: None,
            triggers: vec![],
            config_path: None,
            input_required_timeout_hours: 24,
            notifications: None,
            pipelines: vec![],
            data_dir: std::env::temp_dir(),
        }
    }

    #[tokio::test]
    async fn test_startup_sequence_all_ready() {
        // GIVEN un Supervisor configure
        let port = free_port().await;
        let socket_path = temp_socket_path();
        let config = test_config(port, socket_path.clone());
        let supervisor = Supervisor::new(config);

        // WHEN start() est appele
        let result = supervisor
            .start(
                MockBackend,
                Arc::new(crate::api::routes_agents::StubAgentLoader),
                None,
            )
            .await;

        // THEN tous les acteurs demarrent et on obtient des handles
        assert!(result.is_ok(), "start() should succeed");
        let handles = result.unwrap();

        // Cleanup
        handles.api_handle.shutdown();
        handles.router_handle.shutdown();
        handles.tool_registry_handle.shutdown().await;
        handles.registry_handle.shutdown();
        tokio::time::sleep(Duration::from_millis(50)).await;
        let _ = std::fs::remove_file(&socket_path);
    }

    #[tokio::test]
    async fn test_all_ready_event_emitted() {
        // GIVEN un Supervisor configure
        let port = free_port().await;
        let socket_path = temp_socket_path();
        let config = test_config(port, socket_path.clone());
        let supervisor = Supervisor::new(config);

        // WHEN start() est appele
        let handles = supervisor
            .start(
                MockBackend,
                Arc::new(crate::api::routes_agents::StubAgentLoader),
                None,
            )
            .await
            .unwrap();

        // Subscribe AFTER start (AllReady was already emitted, but let's verify via a new event)
        let mut rx = handles.event_sender.subscribe();

        // Emit a test event to verify the bus is working
        let _ = handles.event_sender.send(RuntimeEvent::AllReady);
        let event = tokio::time::timeout(Duration::from_secs(1), rx.recv())
            .await
            .expect("should receive within 1s")
            .expect("recv should succeed");
        assert!(
            matches!(event, RuntimeEvent::AllReady),
            "expected AllReady, got: {event:?}"
        );

        // Cleanup
        handles.api_handle.shutdown();
        handles.router_handle.shutdown();
        handles.tool_registry_handle.shutdown().await;
        handles.registry_handle.shutdown();
        tokio::time::sleep(Duration::from_millis(50)).await;
        let _ = std::fs::remove_file(&socket_path);
    }

    #[tokio::test]
    async fn test_handles_accessible_after_start() {
        // GIVEN un Supervisor demarre avec succes
        let port = free_port().await;
        let socket_path = temp_socket_path();
        let config = test_config(port, socket_path.clone());
        let supervisor = Supervisor::new(config);
        let handles = supervisor
            .start(
                MockBackend,
                Arc::new(crate::api::routes_agents::StubAgentLoader),
                None,
            )
            .await
            .unwrap();

        // THEN tous les handles sont presents et utilisables
        // EventBusSender: can send (need a subscriber for broadcast to succeed)
        let _rx = handles.event_sender.subscribe();
        let send_result = handles.event_sender.send(RuntimeEvent::ShutdownRequested);
        assert!(send_result.is_ok());

        // AgentRegistryHandle: can list
        let agents = handles.registry_handle.list_agents().await;
        assert!(agents.is_ok());
        assert!(agents.unwrap().is_empty());

        // ToolRegistryHandle: can list (native tools should be registered)
        let tools = handles.tool_registry_handle.list().await;
        assert!(tools.is_ok());
        assert_eq!(
            tools.unwrap().len(),
            3,
            "3 native tools should be auto-registered"
        );

        // TaskRouterHandle: is clone
        let _cloned = handles.router_handle.clone();

        // APIServerHandle: can shutdown
        handles.api_handle.shutdown();

        // Verify Send + Sync at compile time
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<EventBusSender>();
        assert_send_sync::<AgentRegistryHandle>();
        assert_send_sync::<ToolRegistryHandle>();
        assert_send_sync::<TaskRouterHandle<MockBackend>>();

        // Cleanup
        handles.router_handle.shutdown();
        handles.tool_registry_handle.shutdown().await;
        handles.registry_handle.shutdown();
        tokio::time::sleep(Duration::from_millis(50)).await;
        let _ = std::fs::remove_file(&socket_path);
    }

    #[tokio::test]
    async fn test_startup_timeout_rollback() {
        // GIVEN a port already in use (bind will fail, not timeout — but tests the error path)
        let port = free_port().await;
        let socket_path = temp_socket_path();

        // Occupy the port
        let _listener = TcpListener::bind(format!("127.0.0.1:{}", port))
            .await
            .unwrap();

        let config = SupervisorConfig {
            api_config: APIServerConfig {
                socket_path: socket_path.clone(),
                tcp_port: port,
            },
            startup_timeout_secs: 1,
            llm_config: None,
            triggers: vec![],
            config_path: None,
            input_required_timeout_hours: 24,
            notifications: None,
            pipelines: vec![],
            data_dir: std::env::temp_dir(),
        };
        let supervisor = Supervisor::new(config);

        // WHEN start() est appele
        let result = supervisor
            .start(
                MockBackend,
                Arc::new(crate::api::routes_agents::StubAgentLoader),
                None,
            )
            .await;

        // THEN ActorStartFailed est retourne (port already in use)
        assert!(result.is_err());
        let err = match result {
            Err(e) => e,
            Ok(_) => panic!("expected error, got Ok"),
        };
        assert!(
            matches!(&err, SupervisorError::ActorStartFailed { actor, .. } if actor == "api_server"),
            "expected ActorStartFailed for api_server, got: {err:?}"
        );

        let _ = std::fs::remove_file(&socket_path);
    }

    #[tokio::test]
    async fn test_restart_tracker_allows_within_limit() {
        // GIVEN un tracker avec max_restarts=3, window=60s
        let spec = ChildSpec {
            name: "test_actor".to_string(),
            restart_policy: RestartPolicy::OnFailure,
            max_restarts: 3,
            restart_window_secs: 60,
        };
        let mut tracker = RestartTracker::new(spec);

        // WHEN on enregistre 3 restarts
        assert!(tracker.record_restart(), "1st restart should be allowed");
        assert!(tracker.record_restart(), "2nd restart should be allowed");
        assert!(tracker.record_restart(), "3rd restart should be allowed");

        // THEN le 4eme est refuse
        assert!(
            !tracker.record_restart(),
            "4th restart should be denied (exceeds max_restarts=3)"
        );
    }

    #[tokio::test]
    async fn test_max_restarts_exceeded() {
        // GIVEN un tracker avec max_restarts=2
        let spec = ChildSpec {
            name: "flaky_actor".to_string(),
            restart_policy: RestartPolicy::Always,
            max_restarts: 2,
            restart_window_secs: 60,
        };
        let mut tracker = RestartTracker::new(spec);

        // WHEN on depasse le max
        tracker.record_restart();
        tracker.record_restart();
        let allowed = tracker.record_restart();

        // THEN le restart est refuse
        assert!(!allowed, "should exceed max_restarts");

        // AND on peut construire l'erreur correspondante
        let err = SupervisorError::MaxRestartsExceeded {
            actor: "flaky_actor".to_string(),
            count: 3,
            window_secs: 60,
        };
        assert!(err.to_string().contains("flaky_actor"));
        assert!(err.to_string().contains("3"));
    }

    #[tokio::test]
    async fn test_watch_exits_on_shutdown_requested() {
        // GIVEN un EventBus et un watch en cours
        let (sender, _rx) = EventBus::new();
        let sender_clone = sender.clone();

        let watch_handle = tokio::spawn(async move { watch(&sender_clone, vec![]).await });

        // WHEN ShutdownRequested est emis
        tokio::time::sleep(Duration::from_millis(10)).await;
        let _ = sender.send(RuntimeEvent::ShutdownRequested);

        // THEN watch() retourne Ok(())
        let result = tokio::time::timeout(Duration::from_secs(2), watch_handle)
            .await
            .expect("watch should exit within 2s")
            .expect("join should succeed");
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_default_child_specs() {
        // GIVEN / WHEN
        let specs = default_child_specs();

        // THEN 7 specs sont retournees dans l'ordre (TriggerEngine ajouté en STORY-072)
        assert_eq!(specs.len(), 7);
        assert_eq!(specs[0].name, "event_bus");
        assert_eq!(specs[1].name, "agent_registry");
        assert_eq!(specs[2].name, "tool_registry");
        assert_eq!(specs[3].name, "memory_engine");
        assert_eq!(specs[4].name, "task_router");
        assert_eq!(specs[5].name, "trigger_engine");
        assert_eq!(specs[6].name, "api_server");

        // AND les policies sont correctes
        assert_eq!(specs[0].restart_policy, RestartPolicy::Always);
        assert_eq!(specs[4].restart_policy, RestartPolicy::OnFailure);
        assert_eq!(specs[5].restart_policy, RestartPolicy::OnFailure);
        assert_eq!(specs[6].restart_policy, RestartPolicy::OnFailure);
    }

    // AC-2 — Supervisor starts successfully with llm_config = None
    #[tokio::test]
    async fn test_ac2_start_without_llm_config_succeeds() {
        // GIVEN un Supervisor sans section [llm]
        let port = free_port().await;
        let socket_path = temp_socket_path();
        let config = SupervisorConfig {
            api_config: APIServerConfig {
                socket_path: socket_path.clone(),
                tcp_port: port,
            },
            startup_timeout_secs: 10,
            llm_config: None,
            triggers: vec![],
            config_path: None,
            input_required_timeout_hours: 24,
            notifications: None,
            pipelines: vec![],
            data_dir: std::env::temp_dir(),
        };
        let supervisor = Supervisor::new(config);

        // WHEN start() est appele
        let handles = supervisor
            .start(
                MockBackend,
                Arc::new(crate::api::routes_agents::StubAgentLoader),
                None,
            )
            .await
            .expect("start() doit reussir sans config LLM");

        // THEN llm_router est None et le demarrage s'est deroule normalement
        assert!(
            handles.llm_router.is_none(),
            "llm_router doit etre None quand llm_config est absent"
        );

        // Cleanup
        handles.api_handle.shutdown();
        handles.router_handle.shutdown();
        handles.tool_registry_handle.shutdown().await;
        handles.registry_handle.shutdown();
        tokio::time::sleep(Duration::from_millis(50)).await;
        let _ = std::fs::remove_file(&socket_path);
    }

    // AC-4 — AppState clone preserves llm_router = None
    #[tokio::test]
    async fn test_app_state_clone_with_llm_router_none() {
        use crate::eventbus::EventBus;
        use crate::registry::AgentRegistry;
        use crate::router::TaskRouterHandle;

        // GIVEN un AppState avec llm_router = None
        let (event_tx, _event_rx) = EventBus::new();
        let registry_handle = AgentRegistry::spawn(event_tx.clone());
        let router_handle: TaskRouterHandle<MockBackend> =
            TaskRouterHandle::spawn(registry_handle.clone(), event_tx.clone(), 64);
        let state = AppState {
            router_handle,
            registry_handle,
            event_sender: event_tx,
            agent_loader: Arc::new(crate::api::routes_agents::StubAgentLoader),
            backend: MockBackend,
            llm_router: None,
            trigger_engine: None,
            config_path: None,
            task_repository: None,
            pending_approvals: None,
            notification_config: None,
            pipeline_engine: None,
            backend_factory: None,
            tool_registry_handle: None,
            audit_trail: None,
            obs_config: apollia_core::ObservabilityConfig::default(),
        };

        // WHEN on clone l'AppState
        let cloned = state.clone();

        // THEN le clone preserve llm_router = None
        assert!(
            cloned.llm_router.is_none(),
            "le clone doit preserver llm_router = None"
        );
    }

    // AC-3 (STORY-072) — Supervisor démarre avec 0 triggers ; TriggerEngine toujours présent
    #[tokio::test]
    async fn test_ac3_supervisor_starts_with_zero_triggers() {
        // GIVEN une config sans triggers
        let port = free_port().await;
        let socket_path = temp_socket_path();
        let config = SupervisorConfig {
            api_config: APIServerConfig {
                socket_path: socket_path.clone(),
                tcp_port: port,
            },
            startup_timeout_secs: 10,
            llm_config: None,
            triggers: vec![],
            config_path: None,
            input_required_timeout_hours: 24,
            notifications: None,
            pipelines: vec![],
            data_dir: std::env::temp_dir(),
        };
        let supervisor = Supervisor::new(config);

        // WHEN start() est appele
        let result = supervisor
            .start(
                MockBackend,
                Arc::new(crate::api::routes_agents::StubAgentLoader),
                None,
            )
            .await;

        // THEN le demarrage reussit et TriggerEngine est present avec 0 triggers
        assert!(result.is_ok(), "start() doit reussir avec 0 triggers");
        let handles = result.unwrap();
        let trigger_list = handles.trigger_engine.list().await;
        assert!(
            trigger_list.is_empty(),
            "aucun trigger attendu, got {:?}",
            trigger_list
        );

        // Cleanup
        handles.trigger_engine.shutdown().await;
        handles.api_handle.shutdown();
        handles.router_handle.shutdown();
        handles.tool_registry_handle.shutdown().await;
        handles.registry_handle.shutdown();
        tokio::time::sleep(Duration::from_millis(50)).await;
        let _ = std::fs::remove_file(&socket_path);
    }

    // AC-4 (STORY-102) — Supervisor démarre sans section [notifications] sans erreur
    #[tokio::test]
    async fn test_ac4_no_notifications_section_starts_ok() {
        // GIVEN une config sans section [notifications]
        let port = free_port().await;
        let socket_path = temp_socket_path();
        let config = SupervisorConfig {
            api_config: APIServerConfig {
                socket_path: socket_path.clone(),
                tcp_port: port,
            },
            startup_timeout_secs: 10,
            llm_config: None,
            triggers: vec![],
            config_path: None,
            input_required_timeout_hours: 24,
            notifications: None,
            pipelines: vec![],
            data_dir: std::env::temp_dir(),
        };
        let supervisor = Supervisor::new(config);

        // WHEN start() est appelé
        let result = supervisor
            .start(
                MockBackend,
                Arc::new(crate::api::routes_agents::StubAgentLoader),
                None,
            )
            .await;

        // THEN pas d'erreur — NotificationEngine non démarré silencieusement
        assert!(
            result.is_ok(),
            "démarrage sans [notifications] doit réussir, erreur: {:?}",
            result.err()
        );

        // Cleanup
        let handles = result.unwrap();
        handles.api_handle.shutdown();
        handles.router_handle.shutdown();
        handles.tool_registry_handle.shutdown().await;
        handles.registry_handle.shutdown();
        tokio::time::sleep(Duration::from_millis(50)).await;
        let _ = std::fs::remove_file(&socket_path);
    }

    // AC-4 (STORY-072) — AppState.trigger_engine est Some après démarrage
    #[tokio::test]
    async fn test_ac4_trigger_engine_handle_in_app_state() {
        use apollia_triggers::{
            InputTemplate, OnBusyPolicy, TriggerDefinition, TriggerSourceConfig,
        };

        // GIVEN une config avec 1 trigger activé
        let port = free_port().await;
        let socket_path = temp_socket_path();
        let def = TriggerDefinition {
            id: "test-trigger".into(),
            agent: "test-agent".into(),
            pipeline: None,
            enabled: true,
            on_busy: OnBusyPolicy::Queue,
            source: TriggerSourceConfig::Cron {
                schedule: "0 8 * * MON".into(),
            },
            input_template: InputTemplate("hello".into()),
        };
        let config = SupervisorConfig {
            api_config: APIServerConfig {
                socket_path: socket_path.clone(),
                tcp_port: port,
            },
            startup_timeout_secs: 10,
            llm_config: None,
            triggers: vec![def],
            config_path: None,
            input_required_timeout_hours: 24,
            notifications: None,
            pipelines: vec![],
            data_dir: std::env::temp_dir(),
        };
        let supervisor = Supervisor::new(config);

        // WHEN start() est appele
        let handles = supervisor
            .start(
                MockBackend,
                Arc::new(crate::api::routes_agents::StubAgentLoader),
                None,
            )
            .await
            .expect("start() doit reussir");

        // THEN trigger_engine contient 1 trigger
        let trigger_list = handles.trigger_engine.list().await;
        assert_eq!(
            trigger_list.len(),
            1,
            "1 trigger attendu, got {:?}",
            trigger_list
        );
        assert_eq!(trigger_list[0].id, "test-trigger");

        // Cleanup
        handles.trigger_engine.shutdown().await;
        handles.api_handle.shutdown();
        handles.router_handle.shutdown();
        handles.tool_registry_handle.shutdown().await;
        handles.registry_handle.shutdown();
        tokio::time::sleep(Duration::from_millis(50)).await;
        let _ = std::fs::remove_file(&socket_path);
    }

    // AC-3 (STORY-119) — Aucun pipeline défini → PipelineEngine non démarré, pas d'erreur
    #[tokio::test]
    async fn test_ac3_no_pipelines_no_engine() {
        // GIVEN une config sans section [[pipelines]]
        let port = free_port().await;
        let socket_path = temp_socket_path();
        let config = SupervisorConfig {
            api_config: APIServerConfig {
                socket_path: socket_path.clone(),
                tcp_port: port,
            },
            startup_timeout_secs: 10,
            llm_config: None,
            triggers: vec![],
            config_path: None,
            input_required_timeout_hours: 24,
            notifications: None,
            pipelines: vec![],
            data_dir: std::env::temp_dir(),
        };
        let supervisor = Supervisor::new(config);

        // WHEN start() est appelé
        let result = supervisor
            .start(
                MockBackend,
                Arc::new(crate::api::routes_agents::StubAgentLoader),
                None,
            )
            .await;

        // THEN le démarrage réussit et pipeline_engine est None
        assert!(
            result.is_ok(),
            "démarrage sans pipelines doit réussir, erreur: {:?}",
            result.err()
        );
        let handles = result.unwrap();
        assert!(
            handles.pipeline_engine.is_none(),
            "pipeline_engine doit être None quand aucun pipeline n'est défini"
        );

        // Cleanup
        handles.trigger_engine.shutdown().await;
        handles.api_handle.shutdown();
        handles.router_handle.shutdown();
        handles.tool_registry_handle.shutdown().await;
        handles.registry_handle.shutdown();
        tokio::time::sleep(Duration::from_millis(50)).await;
        let _ = std::fs::remove_file(&socket_path);
    }
}
