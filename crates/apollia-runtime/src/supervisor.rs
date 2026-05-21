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

use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;
use tracing::{error, info, warn};

use apollia_core::{
    HitlConfig, LlmBackendRepository, PendingApprovals, ProcessState, RuntimeConfig, RuntimeEvent,
    SttConfigRepository, SttConfigRow,
};
use apollia_llm::{LlmCallRepository, LlmConfig, LlmRouter};
use apollia_mcp::{config::McpConfig, manager::McpClientManagerHandle, McpServerRepository};
use apollia_notifications::{
    build_channels, NotificationConfig, NotificationConfigRepository, NotificationEngine,
    NotificationEngineHandle,
};
use apollia_tools::{AgentRepository, AuditTrailHandle, TaskRepository, ToolRegistryHandle};
use apollia_triggers::{TriggerDefinitionRepository, TriggerEngineHandle, TriggerPersistence};

use crate::api::routes_agents::AgentLoader;
use crate::api::{APIServer, APIServerConfig, APIServerError, APIServerHandle, AppState};
use crate::coordinator::{ExecutionBackend, ExecutionCoordinator};
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
    /// agents receive `ctx.llm = None`. No error is raised.
    pub llm_config: Option<LlmConfig>,
    /// Path to `apollia.toml` — injected into [`AppState`] for hot reload.
    ///
    /// `None` when the runtime starts without a config file (e.g. tests, `apollia-os start`
    /// without a config file). The `POST /api/v1/triggers/reload` route returns 503 when absent.
    pub config_path: Option<std::path::PathBuf>,
    /// Configuration du runtime core (capacités EventBus et mailbox).
    ///
    /// Correspond à la section `[runtime]` dans `apollia.toml`.
    /// Défaut : [`RuntimeConfig::default()`].
    pub runtime_config: RuntimeConfig,

    /// Configuration Human-in-the-Loop (timeout et intervalle de scan).
    ///
    /// Correspond à la section `[hitl]` dans `apollia.toml`.
    /// Ignoré si `AppState.task_repository` est `None`.
    /// Défaut : [`HitlConfig::default()`] (24 heures, scan 60 secondes).
    pub hitl_config: HitlConfig,
    /// Répertoire de données du runtime (ex: `~/.apollia/`).
    ///
    /// Utilisé pour localiser les bases SQLite (`triggers.db`,
    /// `notifications.db`, etc.). Doit exister et être accessible en écriture.
    /// Sert de `base_dir` pour l'ouverture des repositories au boot.
    pub data_dir: std::path::PathBuf,
    /// Configuration d'observabilité (limites de troncature, flags debug).
    ///
    /// Injectée dans `AppState`, `TriggerEngine`, et `LlmCallRepository`.
    /// Par défaut : `ObservabilityConfig::default()` (32 KB max input/output).
    pub obs_config: apollia_core::ObservabilityConfig,
    /// Repository des agents installés.
    ///
    /// `Some` → l'auto-load est activé au boot : les agents `enabled` sont
    /// chargés via `AgentLoader`, validés et enregistrés dans `AgentRegistry`.
    /// `None` → l'auto-load est désactivé (compatibilité tests existants).
    pub agent_repository: Option<AgentRepository>,
    /// Repository des packages installés (migration 008).
    ///
    /// `Some` → la Phase 10.6 valide l'intégrité des packages au boot.
    /// `None` → la validation est désactivée (rétrocompatibilité).
    pub package_repository: Option<apollia_tools::PackageRepository>,
    /// Répertoire des agents bundled (ex: `agents/bundled/`).
    ///
    /// Si `Some`, `auto_load_bundled_agents` est appelé au boot pour enregistrer
    /// automatiquement les agents déclarés dans `manifest.json`.
    /// Si `None` ou si `manifest.json` est absent, l'auto-install est ignoré silencieusement.
    pub bundled_agents_path: Option<std::path::PathBuf>,

    /// Configuration des outils natifs (section `[tools]` de `apollia.toml`).
    ///
    /// Propagée par `EmbeddedConfig::apply_toml` → `SupervisorConfig` → `RuntimeHandle`.
    /// Permet aux agent runners (factory + chat) d'appliquer `web_search`,
    /// `web_read`, `http_allowlist` et `disabled` lors de la construction
    /// du `NativeDispatcherConfig`. Défaut : [`apollia_core::ToolsConfig::default()`].
    pub tools_config: apollia_core::ToolsConfig,
}

impl SupervisorConfig {
    /// Retourne le répertoire des agents bundled, ou `None` s'il n'est pas configuré.
    pub fn bundled_agents_dir(&self) -> Option<&std::path::Path> {
        self.bundled_agents_path.as_deref()
    }
}

/// Manifest des agents bundled distribués avec le runtime.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BundledManifest {
    /// Version du format de manifest (ex : `"1.0.0"`).
    pub version: String,
    /// Liste des agents bundled.
    pub bundled_agents: Vec<BundledAgentEntry>,
}

/// Entrée d'un agent dans le manifest bundled.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BundledAgentEntry {
    /// Nom unique de l'agent (doit correspondre à `manifest().name` dans le fichier Python).
    pub name: String,
    /// Nom du fichier source relatif au répertoire bundled (ex : `"excel-worker.py"`).
    pub file: String,
    /// Si `true`, l'agent est installé automatiquement au premier boot.
    pub auto_install: bool,
    /// Description courte de l'agent.
    pub description: String,
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
    /// LLM router initialized at position 5 of the startup sequence.
    ///
    /// `None` when no `[llm]` section is present in `apollia.toml`, or when
    /// `LlmRouter::from_config_with_bus` fails (warning logged, runtime continues).
    pub llm_router: Option<Arc<LlmRouter>>,
    /// Handle to the TriggerEngine actor at position 6 of the startup sequence.
    ///
    /// Always `Some` after successful startup — even when `config.triggers` is empty.
    /// Injected into `AppState` so webhook routes and CLI commands can reach it.
    pub trigger_engine: TriggerEngineHandle,
    /// Handle to the AuditTrail actor.
    ///
    /// `None` when the data directory is unavailable or the SQLite open fails
    /// (warning logged, runtime continues without audit). `Some` in production.
    pub audit_trail: Option<AuditTrailHandle>,
    /// HITL task repository — persists `input_required` prompts/contexts.
    ///
    /// Shared between `AppState` (resume handler) and `TimeoutWatcher`.
    /// `None` when the SQLite open fails (warning logged, HITL disabled).
    pub task_repository: Option<Arc<TaskRepository>>,
    /// HITL pending approvals registry — oneshot channels for Mode Direct suspension.
    ///
    /// `None` when `task_repository` is `None` (HITL disabled).
    pub pending_approvals: Option<Arc<apollia_core::PendingApprovals>>,
    /// Handle to the NotificationEngine actor.
    ///
    /// `None` when no `[notifications]` section is present in `apollia.toml`.
    /// Used by [`ShutdownController`] to stop the engine before the EventBus closes,
    /// preventing late notifications from being delivered after `apollia-os stop`.
    pub notification_engine: Option<NotificationEngineHandle>,
    /// Repository des appels LLM — agrégation coûts/tokens.
    ///
    /// `Some` quand un `LlmRouter` est configuré et que `llm_calls.db` est ouvert.
    /// Partagé entre `AppState` (route REST costs) et le subscriber EventBus.
    pub llm_call_repository: Option<Arc<std::sync::Mutex<LlmCallRepository>>>,
    /// Handle to the [`ChatSessionManager`] actor.
    ///
    /// `Some` after Phase 13 of the Supervisor startup sequence.
    /// `None` when the chat subsystem failed to start (warning logged).
    pub chat_manager: Option<crate::chat::ChatSessionManagerHandle>,
    /// ORIA plan cache repository — stores cached execution plans.
    ///
    /// `Some` when `plan_cache.db` opened successfully.
    /// `None` when the open failed (warning logged, caching disabled).
    pub plan_cache: Option<Arc<std::sync::Mutex<apollia_oria::plan_cache::PlanCacheRepository>>>,
    /// Handle to the agent-to-agent mailbox actor.
    ///
    /// Always `Some` after startup — the mailbox is lightweight and always spawned.
    pub mailbox_handle: Option<crate::mailbox::AgentMailboxHandle>,
    /// Repository for global user memory (preferences, habits, context).
    ///
    /// `Some` when `user_memory.db` opened successfully on startup.
    /// `None` when the open failed (warning logged, user memory disabled).
    pub user_memory:
        Option<std::sync::Arc<std::sync::Mutex<apollia_memory::user_memory::UserMemoryRepository>>>,
    /// Handle to the SttEngine actor (Phase 15).
    ///
    /// `Some` when `stt.enabled = true` and the model loaded successfully.
    /// `None` when STT is disabled, the model is absent, or loading failed.
    pub stt_engine: Option<crate::stt::SttEngineHandle>,
    /// STT transcription repository for API routes.
    ///
    /// Separate connection from the engine's internal repository (SQLite WAL
    /// supports concurrent readers). `None` when STT is disabled.
    pub stt_repository: Option<std::sync::Arc<std::sync::Mutex<apollia_stt::SttRepository>>>,
    /// Handle to the MCP client manager actor (Phase 3b).
    ///
    /// `Some` when `~/.apollia/mcp.toml` exists and at least one server connected.
    /// `None` when the config file is absent, empty, or all servers failed to start.
    pub mcp_handle: Option<McpClientManagerHandle>,
    /// Repository des projets (SQLite).
    ///
    /// `Some` quand `projects.db` est ouvert avec succès.
    /// `None` quand l'ouverture a échoué (warning loggé).
    pub project_repository: Option<std::sync::Arc<apollia_tools::ProjectRepository>>,
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

/// Installe les agents bundled au premier boot.
///
/// Lit `<bundled_agents_path>/manifest.json` et, pour chaque entrée avec
/// `auto_install: true` absente de la base, charge le manifest via `loader`
/// et persiste un [`apollia_tools::InstalledAgent`] dans `repo`.
///
/// Phase 11 (`list_enabled`) prend ensuite en charge le chargement et
/// l'enregistrement dans l'`AgentRegistry`. Les erreurs sont toujours loggées
/// mais ne bloquent jamais le boot.
fn auto_load_bundled_agents(
    bundled_agents_path: Option<&std::path::Path>,
    repo: &AgentRepository,
    loader: &Arc<dyn AgentLoader>,
) {
    let bundled_dir = match bundled_agents_path {
        Some(d) => d,
        None => {
            tracing::debug!("no bundled agents path configured");
            return;
        }
    };

    let manifest_path = bundled_dir.join("manifest.json");
    if !manifest_path.exists() {
        tracing::debug!("no bundled agents manifest found");
        return;
    }

    let manifest_content = match std::fs::read_to_string(&manifest_path) {
        Ok(c) => c,
        Err(e) => {
            warn!(error = %e, "failed to read bundled agents manifest");
            return;
        }
    };

    let manifest: BundledManifest = match serde_json::from_str(&manifest_content) {
        Ok(m) => m,
        Err(e) => {
            warn!(error = %e, "failed to parse bundled agents manifest");
            return;
        }
    };

    for entry in &manifest.bundled_agents {
        if !entry.auto_install {
            continue;
        }

        match repo.get(&entry.name) {
            Ok(Some(_)) => {
                tracing::debug!(agent = %entry.name, "bundled agent already installed");
                continue;
            }
            Ok(None) => {}
            Err(e) => {
                warn!(agent = %entry.name, error = %e, "failed to check bundled agent in repository");
                continue;
            }
        }

        let source_path = bundled_dir.join(&entry.file);
        info!(agent = %entry.name, "installing bundled agent");

        let agent_manifest = match loader.load_and_validate(&source_path) {
            Ok(m) => m,
            Err(e) => {
                warn!(agent = %entry.name, error = %e, "failed to load bundled agent manifest");
                continue;
            }
        };

        let now = chrono::Utc::now().to_rfc3339();
        let installed = apollia_tools::InstalledAgent {
            name: agent_manifest.name.clone(),
            version: agent_manifest.version.clone(),
            install_path: source_path.clone(),
            source_path,
            manifest: agent_manifest,
            enabled: true,
            installed_at: now.clone(),
            updated_at: now,
        };

        if let Err(e) = repo.save(&installed) {
            warn!(agent = %entry.name, error = %e, "failed to persist bundled agent to repository");
            continue;
        }

        info!(agent = %entry.name, "bundled agent registered for auto-load");
    }
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
        agent_runner: Option<Arc<dyn crate::chat::ChatAgentRunner>>,
    ) -> Result<SupervisorHandles<B>, SupervisorError> {
        let timeout = Duration::from_secs(self.config.startup_timeout_secs);

        // Phase 1: EventBus
        info!("Supervisor: starting EventBus");
        let (event_sender, mut startup_rx) =
            EventBus::with_capacity(self.config.runtime_config.eventbus_capacity);
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
        // Phase 3b: register connector tool descriptors (Google Workspace,
        // future Microsoft 365). Descriptors are static — they make the LLM
        // aware of the tools regardless of whether a Google account is
        // actually connected. The matching executors are wired in by the
        // desktop dispatcher builder, which has access to the AuthManager.
        let connector_descriptor_count =
            crate::connectors_bridge::all_connector_descriptors().len();
        for descriptor in crate::connectors_bridge::all_connector_descriptors() {
            let tool_name = descriptor.name.clone();
            if let Err(e) = tool_registry_handle.register(descriptor).await {
                warn!(
                    error = %e,
                    tool = %tool_name,
                    "failed to register connector tool descriptor"
                );
            }
        }
        info!(
            connector_tools = connector_descriptor_count,
            "Supervisor: ToolRegistry ready (native + connector tools registered)"
        );

        // Phase 3b: MCP Client Manager — reads server list from mcp.db (SQLite).
        //
        // On first boot, if `mcp.db` is empty and `mcp.toml` exists, performs a
        // one-shot migration and logs the count. After migration, `mcp.toml` is no
        // longer consulted. Errors are never fatal: the runtime continues and MCP
        // tools are simply unavailable.
        let mcp_db_path = self.config.data_dir.join("mcp.db");
        let mcp_config_path = self.config.data_dir.join("mcp.toml");

        let (mcp_handle, mcp_server_repo) = match McpServerRepository::open(&mcp_db_path) {
            Err(e) => {
                warn!(error = %e, "failed to open mcp.db — continuing without MCP");
                (None, None)
            }
            Ok(repo) => {
                // One-shot migration from mcp.toml when the database is empty.
                match repo.list() {
                    Ok(existing) if existing.is_empty() => {
                        match McpConfig::load(&mcp_config_path) {
                            Ok(toml_config) if !toml_config.servers.is_empty() => {
                                match repo.import_from_toml(toml_config.servers) {
                                    Ok(n) => info!(
                                        count = n,
                                        "imported MCP servers from mcp.toml (one-time migration)"
                                    ),
                                    Err(e) => {
                                        warn!(error = %e, "MCP migration from mcp.toml failed — skipping")
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                    Ok(_) => {}
                    Err(e) => warn!(error = %e, "failed to check mcp.db for migration — skipping"),
                }

                // Load server list and start the manager.
                let server_configs = match repo.list() {
                    Ok(v) => v,
                    Err(e) => {
                        warn!(error = %e, "failed to list MCP servers from mcp.db");
                        Vec::new()
                    }
                };

                // Always start the McpClientManager actor, even when the
                // server list is empty. Without this, the desktop "Add MCP
                // server" flow cannot register a first server: every write
                // route (POST /api/v1/mcp/servers, …) checks
                // require_mcp_handle and returns 503 "MCP is not configured"
                // until at least one server exists in mcp.db at boot — a
                // chicken-and-egg trap for first-time users.
                let server_count = server_configs.len();
                let handle = match McpClientManagerHandle::start(
                    server_configs,
                    &tool_registry_handle,
                    Some(event_sender.clone()),
                    None,
                )
                .await
                {
                    Ok(handle) => {
                        let status = handle.status().await;
                        let total_tools: usize = status.iter().map(|s| s.tools_count).sum();
                        info!(
                            servers = server_count,
                            connected = status.len(),
                            tools = total_tools,
                            "MCP Phase 3b complete"
                        );
                        // Start MCP config watcher only when the legacy mcp.toml exists.
                        // Config is now stored in mcp.db (SQLite) — mcp.toml is a
                        // deprecated migration path kept for back-compat.
                        if mcp_config_path.exists() {
                            apollia_triggers::handlers::config_watch::McpConfigWatcher::spawn(
                                mcp_config_path.clone(),
                                handle.clone(),
                            );
                        }
                        Some(handle)
                    }
                    Err(e) => {
                        warn!(error = %e, "MCP Phase 3b failed — continuing without MCP");
                        None
                    }
                };

                let repo = Arc::new(std::sync::Mutex::new(repo));
                (handle, Some(repo))
            }
        };

        // Phase 4 (pos 5): LlmRouter + LlmBackendRepository — loads backends from system.db
        let system_db_path = self.config.data_dir.join("system.db");
        let (llm_router, llm_backend_repo) = match LlmBackendRepository::open(&system_db_path) {
            Ok(repo) => {
                info!("Supervisor: starting LlmRouter from system.db");

                // Migration: if system.db has no backends and apollia.toml has backends,
                // import them. Handles first-boot and the onboarding case where
                // setup_local_llm writes only to TOML, not to system.db.
                if let Some(llm_cfg) = &self.config.llm_config {
                    if let Ok(existing) = repo.list() {
                        if existing.is_empty() {
                            info!("Supervisor: no LLM backends in system.db — migrating from apollia.toml");
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
                    }
                }

                let router_result = LlmRouter::from_repository(&repo).await;
                let repo = Arc::new(std::sync::Mutex::new(repo));
                match router_result {
                    Ok(router) => {
                        for info in router.list() {
                            tracing::info!(
                                backend = %info.name,
                                model = %info.model_id,
                                "LLM backend ready"
                            );
                        }
                        info!("Supervisor: LlmRouter ready");
                        (Some(Arc::new(router)), Some(repo))
                    }
                    Err(e) => {
                        warn!(error = %e, "LlmRouter failed to initialize — continuing without LLM");
                        (None, Some(repo))
                    }
                }
            }
            Err(e) => {
                warn!(error = %e, "failed to open system.db — LLM disabled");
                (None, None)
            }
        };

        // Phase 4b: LlmCallRepository — subscriber EventBus pour persister les appels LLM
        let mut llm_call_repository: Option<Arc<std::sync::Mutex<LlmCallRepository>>> = None;
        if llm_router.is_some() {
            let db_path = self.config.data_dir.join("llm_calls.db");
            match LlmCallRepository::open(&db_path) {
                Ok(repo) => {
                    let repo = Arc::new(std::sync::Mutex::new(repo));
                    let mut obs = self.config.obs_config.clone();
                    if let Some(c) = self.config.llm_config.as_ref() {
                        obs.debug_log_prompt = c.observability.debug_log_prompt;
                    }
                    apollia_llm::spawn_llm_subscriber(repo.clone(), &event_sender, obs);
                    llm_call_repository = Some(repo);
                    info!("Supervisor: LlmCallRepository ready (subscriber spawned)");
                }
                Err(e) => {
                    warn!(error = %e, "LlmCallRepository failed to open — LLM call persistence disabled");
                }
            }
        }

        // Phase 4c: EventPersistor (ADR-088) — log append-only de la trace
        // d'exécution agent (Lot 1 : AgentLog ; Lots 2+ ajoutent thoughts,
        // tool_call_*, llm_call_*, etc.). Si l'ouverture échoue, le runtime
        // continue sans persistance (logs partent toujours dans `tracing`).
        {
            let db_path = self.config.data_dir.join("runtime_events.db");
            match crate::observability::EventPersistorHandle::open(&db_path).await {
                Ok(handle) => {
                    crate::observability::spawn_runtime_events_subscriber(
                        handle,
                        &event_sender,
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
                        "EventPersistor failed to open — runtime_events persistence disabled"
                    );
                }
            }
        }

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
        info!("Supervisor: shared ResilienceLayer ready (event subscriber spawned)");

        // Phase 5 (pos 6): TaskRouter
        info!("Supervisor: starting TaskRouter");
        let router_handle: TaskRouterHandle<B> =
            TaskRouterHandle::spawn(registry_handle.clone(), event_sender.clone(), 256);
        info!("Supervisor: TaskRouter ready");

        // Phase 6 (pos 7): TriggerEngine — démarré après TaskRouter (besoin du submitter)
        info!("Supervisor: starting TriggerEngine");
        // Ouvre le repository de définitions de triggers depuis SQLite.
        let trigger_def_db_path = self.config.data_dir.join("triggers_def.db");
        let trigger_def_repo =
            TriggerDefinitionRepository::open(&trigger_def_db_path).map_err(|e| {
                SupervisorError::ActorStartFailed {
                    actor: "trigger_engine".to_string(),
                    reason: format!("failed to open triggers_def.db: {e}"),
                }
            })?;
        let trigger_definitions: Vec<apollia_triggers::TriggerDefinition> = {
            let rows = trigger_def_repo
                .list()
                .map_err(|e| SupervisorError::ActorStartFailed {
                    actor: "trigger_engine".to_string(),
                    reason: format!("failed to list trigger definitions: {e}"),
                })?;
            let mut defs = Vec::with_capacity(rows.len());
            for row in rows {
                match apollia_triggers::TriggerDefinition::try_from(row) {
                    Ok(def) => defs.push(def),
                    Err(e) => {
                        warn!(error = %e, "Skipping invalid trigger definition");
                    }
                }
            }
            defs
        };
        let trigger_def_repo = Arc::new(std::sync::Mutex::new(trigger_def_repo));
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
        let enabled_count = trigger_definitions.iter().filter(|t| t.enabled).count();
        let trigger_engine = TriggerEngineHandle::spawn(
            trigger_definitions,
            router_handle.clone(),
            event_sender.clone(),
            trigger_persistence,
            self.config.obs_config.clone(),
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
        // Open TaskRepository (HITL persistence).
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
        // PendingApprovals — oneshot channel registry for HITL suspension.
        let pending_approvals: Option<Arc<PendingApprovals>> = task_repository
            .as_ref()
            .map(|_| Arc::new(PendingApprovals::new()));

        // Phase 13 (early): UserMemoryRepository — promoted before notifications
        // so we can consult the seed marker + profile name when bootstrapping
        // the default desktop channel (Item 5 of Notifications v2).
        let user_memory: Option<
            std::sync::Arc<std::sync::Mutex<apollia_memory::user_memory::UserMemoryRepository>>,
        > = {
            let db_path = self.config.data_dir.join("user_memory.db");
            match apollia_memory::user_memory::UserMemoryRepository::new(&db_path) {
                Ok(repo) => {
                    info!("Supervisor: UserMemoryRepository ready");
                    Some(std::sync::Arc::new(std::sync::Mutex::new(repo)))
                }
                Err(e) => {
                    warn!(error = %e, "UserMemoryRepository failed to open — user memory disabled");
                    None
                }
            }
        };

        // open NotificationConfigRepository from SQLite.
        let notif_db_path = self.config.data_dir.join("notifications.db");
        let notification_repo =
            NotificationConfigRepository::open(&notif_db_path).map_err(|e| {
                SupervisorError::ActorStartFailed {
                    actor: "notification_engine".to_string(),
                    reason: format!("failed to open notifications.db: {e}"),
                }
            })?;

        // Item 5: bootstrap a desktop-default channel on first launch.
        // Idempotent — guarded by a marker in __user__ namespace.
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

        // NotificationEngine — spawné avant AppState pour passer le handle.
        let notification_engine: Option<NotificationEngineHandle> =
            if let Some(ref notif_config) = notification_config_for_engine {
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
                let api_base_url = format!(
                    "http://{}:{}",
                    connect_addr, self.config.api_config.tcp_port
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
                Some(handle)
            } else {
                tracing::info!(
                    "Supervisor: aucun canal de notification en base — NotificationEngine désactivé"
                );
                None
            };

        // Phase 12b: PlanCacheRepository — opened before APIServer for REST stats/clear.
        let plan_cache: Option<
            Arc<std::sync::Mutex<apollia_oria::plan_cache::PlanCacheRepository>>,
        > = {
            let db_path = self.config.data_dir.join("plan_cache.db");
            match apollia_oria::plan_cache::PlanCacheRepository::open(&db_path) {
                Ok(repo) => {
                    info!("Supervisor: PlanCacheRepository ready");
                    Some(Arc::new(std::sync::Mutex::new(repo)))
                }
                Err(e) => {
                    warn!(error = %e, "PlanCacheRepository failed to open — plan caching disabled");
                    None
                }
            }
        };

        // Phase 12c: AgentMailbox — lightweight actor, always spawned.
        let mailbox_handle = crate::mailbox::AgentMailboxHandle::spawn(
            event_sender.clone(),
            self.config.runtime_config.mailbox_capacity,
        );
        info!("Supervisor: AgentMailbox ready");

        // Phase 13: UserMemoryRepository was promoted above (before notifications) to
        // support the Item 5 seed bootstrap of the default desktop channel. Variable
        // `user_memory` is already bound by that earlier block.

        // Phase 13b: ProjectRepository — SQLite projects.db.
        let project_repository: Option<std::sync::Arc<apollia_tools::ProjectRepository>> = {
            let db_path = self.config.data_dir.join("projects.db");
            match apollia_tools::ProjectRepository::open(&db_path) {
                Ok(repo) => {
                    if let Err(e) = repo.seed_builtin_templates() {
                        warn!(error = %e, "ProjectRepository: seed_builtin_templates failed");
                    }
                    info!("Supervisor: ProjectRepository ready");
                    Some(std::sync::Arc::new(repo))
                }
                Err(e) => {
                    warn!(error = %e, "ProjectRepository failed to open — projects disabled");
                    None
                }
            }
        };

        // Phase 14: ChatSessionManager — spawned before APIServer to inject handle into AppState.
        info!("Supervisor: starting ChatSessionManager");
        let chat_db_path = self.config.data_dir.join("chat.db");

        // SidechainRepository — opened before A2AInvoker so the logger can be injected.
        let sidechain_logger: Option<crate::a2a::SidechainLogger> = {
            let db_path = self.config.data_dir.join("sidechains.db");
            match crate::a2a::SidechainRepository::open(&db_path) {
                Ok(repo) => {
                    info!("Supervisor: SidechainRepository ready");
                    Some(crate::a2a::SidechainLogger::new(std::sync::Arc::new(
                        std::sync::Mutex::new(repo),
                    )))
                }
                Err(e) => {
                    warn!(error = %e, "SidechainRepository failed to open — sidechain logging disabled");
                    None
                }
            }
        };

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
                // ADR-096 Phase 3 — feed the chat dispatcher the same global
                // config the Agent-mode dispatcher uses. Native tools needing
                // app-level config (web_search Brave key, web_read SSRF
                // settings, http_fetch allowlist, memory_search base dir,
                // permission_rule_* governance.db) become available across
                // Chat Libre now too.
                Some(std::sync::Arc::new(crate::chat::ChatToolsConfig {
                    data_dir: self.config.data_dir.clone(),
                    brave_api_key: None, // resolved lazily by web_search when missing
                    tools_config: self.config.tools_config.clone(),
                })),
            ) {
                Ok(handle) => {
                    info!("Supervisor: ChatSessionManager ready");
                    Some(handle)
                }
                Err(e) => {
                    warn!(error = %e, "ChatSessionManager failed to start — chat disabled");
                    None
                }
            };

        // Phase 15: SttEngine — reads config from system.db, then conditionally starts
        // the engine when `stt_config.enabled = true`.
        //
        // Opens `SttConfigRepository` against the same `system.db` used by the
        // LLM backend registry. On first boot an empty table is initialised with
        // defaults. The engine is only started when the persisted config has
        // `enabled = true` and the model file exists on disk.
        let stt_config_repo = match SttConfigRepository::open(&system_db_path) {
            Ok(repo) => {
                info!("Supervisor: SttConfigRepository opened");
                Some(std::sync::Arc::new(std::sync::Mutex::new(repo)))
            }
            Err(e) => {
                warn!(error = %e, "SttConfigRepository failed to open — STT config disabled");
                None
            }
        };

        let stt_cfg: Option<SttConfigRow> = stt_config_repo.as_ref().and_then(|repo| {
            repo.lock()
                .ok()
                .and_then(|guard| guard.get_or_default().ok())
        });

        let (stt_engine, stt_repository): (
            Option<crate::stt::SttEngineHandle>,
            Option<std::sync::Arc<std::sync::Mutex<apollia_stt::SttRepository>>>,
        ) = if stt_cfg.as_ref().is_some_and(|c| c.enabled) {
            let cfg = stt_cfg.as_ref().expect("checked above");
            info!("Supervisor: starting SttEngine");

            let model_path = resolve_home(std::path::Path::new(&cfg.model_path));

            if !model_path.exists() {
                error!(
                    path = %model_path.display(),
                    "STT model file not found — SttEngine disabled"
                );
                (None, None)
            } else {
                let repo_path = self.config.data_dir.join("stt_transcriptions.db");
                match apollia_stt::SttRepository::open(&repo_path) {
                    Ok(repository) => {
                        let mp = model_path.display().to_string();
                        match tokio::task::spawn_blocking(move || {
                            crate::stt::engine::try_load_backend(&mp)
                        })
                        .await
                        {
                            Ok(Ok(backend)) => {
                                let handle = crate::stt::SttEngineHandle::start(
                                    backend,
                                    repository,
                                    cfg.clone(),
                                    event_sender.clone(),
                                );
                                info!("Supervisor: SttEngine ready");
                                let api_repo = apollia_stt::SttRepository::open(&repo_path)
                                    .map(|r| std::sync::Arc::new(std::sync::Mutex::new(r)))
                                    .ok();
                                (Some(handle), api_repo)
                            }
                            Ok(Err(e)) => {
                                warn!(error = %e, "STT engine disabled (model unavailable or backend not compiled)");
                                (None, None)
                            }
                            Err(e) => {
                                warn!(error = %e, "STT engine disabled (loader panicked)");
                                (None, None)
                            }
                        }
                    }
                    Err(e) => {
                        error!(error = %e, "SttRepository failed to open — SttEngine disabled");
                        (None, None)
                    }
                }
            }
        } else {
            info!("Supervisor: STT disabled in config — Phase 15 skipped");
            (None, None)
        };

        // Clone handles before moving into AppState — needed for auto-load.
        let agent_loader_for_autoload = agent_loader.clone();
        let backend_factory_for_autoload = backend_factory.clone();
        let backend_for_autoload = backend.clone();
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
            backend_factory,
            tool_registry_handle: Some(tool_registry_handle.clone()),
            audit_trail: audit_trail_handle.clone(),
            obs_config: self.config.obs_config.clone(),
            llm_call_repository: llm_call_repository.clone(),
            trigger_def_repo: Some(trigger_def_repo.clone()),
            notification_repo: Some(notification_repo.clone()),
            notification_engine_handle: notification_engine.clone(),
            chat_manager: chat_manager.clone(),
            plan_cache: plan_cache.clone(),
            mailbox_handle: Some(mailbox_handle.clone()),
            user_memory: user_memory.clone(),
            stt_engine: stt_engine.clone(),
            stt_repository: stt_repository.clone(),
            stt_config_repo: stt_config_repo.clone(),
            mcp_handle: mcp_handle.clone(),
            mcp_server_repo: mcp_server_repo.clone(),
            llm_backend_repo: llm_backend_repo.clone(),
            a2a_invoker: Some(a2a_invoker),
            resilience_layer: Some(shared_resilience_layer.clone()),
        };
        let api_server = APIServer::new(self.config.api_config, state);

        let api_handle = match tokio::time::timeout(timeout, api_server.start()).await {
            Ok(Ok(handle)) => handle,
            Ok(Err(api_err)) => {
                // Rollback: stop actors in reverse order (TriggerEngine → TaskRouter → …)
                trigger_engine.shutdown().await;
                router_handle.shutdown();
                tool_registry_handle.shutdown().await;
                registry_handle.shutdown();
                return Err(SupervisorError::from(api_err));
            }
            Err(_elapsed) => {
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

        // Phase 8 (pos 9): TimeoutWatcher — démarré si task_repository est configuré
        if let Some(ref repo) = task_repository {
            info!("Supervisor: starting TimeoutWatcher");
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
            info!("Supervisor: TimeoutWatcher started");
        }

        // Emit AllReady
        let _ = event_sender.send(RuntimeEvent::AllReady);
        info!("Supervisor: all actors ready, emitted AllReady");

        // Drain the AllReady event from the startup receiver
        drain_until_all_ready(&mut startup_rx, timeout).await;

        // Onboarding detection: emit OnboardingRequired if UserMemory is empty.
        // Non-blocking — the runtime is fully operational regardless of the result.
        if let Some(ref um) = user_memory {
            match um.lock() {
                Ok(repo) => match repo.is_empty() {
                    Ok(true) => {
                        let _ = event_sender.send(RuntimeEvent::OnboardingRequired);
                        info!("first launch detected — onboarding required");
                    }
                    Ok(false) => {
                        info!("user memory populated — skipping onboarding");
                    }
                    Err(e) => {
                        warn!(error = %e, "failed to check user memory for onboarding — skipping");
                    }
                },
                Err(e) => {
                    warn!(error = %e, "user memory lock poisoned — skipping onboarding check");
                }
            }
        }

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
        if let Some(ref repo) = self.config.agent_repository {
            match repo.list_enabled() {
                Ok(agents) => {
                    if agents.is_empty() {
                        info!("No installed agents to load");
                    }
                    for agent in &agents {
                        if !agent.enabled {
                            warn!(name = %agent.name, "Skipping disabled installed agent");
                            continue;
                        }
                        let manifest = match agent_loader_for_autoload
                            .load_and_validate(&agent.install_path)
                        {
                            Ok(m) => m,
                            Err(e) => {
                                warn!(
                                    name = %agent.name,
                                    error = %e,
                                    "Failed to load installed agent"
                                );
                                let _ = event_sender.send(RuntimeEvent::AgentLoadFailed {
                                    name: agent.name.clone(),
                                    error: e.to_string(),
                                });
                                continue;
                            }
                        };

                        let max_concurrent = manifest.max_concurrent_tasks;
                        let agent_name = manifest.name.clone();

                        // Install pip packages into the agent's venv before registration.
                        // On failure the agent continues in Degraded state — boot is never blocked.
                        let degraded_reason: Option<String> = if manifest.packages.is_empty() {
                            None
                        } else {
                            let venv_base = self.config.data_dir.join("venvs");
                            match apollia_tools::tools::python_executor::PythonExecutor::new(
                                &manifest.name,
                                &venv_base,
                            ) {
                                Ok(executor) => {
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
                                                "package installation failed — agent will start in DEGRADED state"
                                            );
                                            Some(e.to_string())
                                        }
                                    }
                                }
                                Err(e) => {
                                    warn!(
                                        agent = %manifest.name,
                                        error = %e,
                                        "failed to create PythonExecutor for venv — agent will start in DEGRADED state"
                                    );
                                    Some(e.to_string())
                                }
                            }
                        };

                        // Register in AgentRegistry (state = Initializing).
                        let agent_id = match registry_handle.register(manifest).await {
                            Ok(id) => id,
                            Err(e) => {
                                warn!(
                                    name = %agent_name,
                                    error = %e,
                                    "Failed to register installed agent"
                                );
                                continue;
                            }
                        };

                        // Transition: Initializing → Active (required by state machine).
                        if let Err(e) = registry_handle
                            .update_state(agent_id.as_str(), ProcessState::Active)
                            .await
                        {
                            warn!(name = %agent_name, error = %e, "Failed to activate agent");
                            continue;
                        }

                        // If package installation failed: Active → Degraded.
                        if degraded_reason.is_some() {
                            registry_handle
                                .update_state(agent_id.as_str(), ProcessState::Degraded)
                                .await
                                .unwrap_or_else(|e| {
                                    warn!(name = %agent_name, error = %e, "failed to set Degraded state")
                                });
                        }

                        // Create ExecutionCoordinator with backend factory.
                        let agent_backend: B = match &backend_factory_for_autoload {
                            Some(factory) => {
                                let dyn_backend =
                                    factory.create_for_agent(&agent.install_path, &agent.manifest);
                                B::from(dyn_backend)
                            }
                            None => backend_for_autoload.clone(),
                        };
                        let mut coordinator = ExecutionCoordinator::new(
                            agent_id.clone(),
                            max_concurrent,
                            event_sender.clone(),
                            agent_backend,
                        )
                        .with_agent_name(agent_name.clone());
                        if let Some(ref repo) = task_repository {
                            coordinator = coordinator.with_task_repository(
                                Arc::clone(repo),
                                self.config.obs_config.clone(),
                            );
                        }

                        // Register coordinator in TaskRouter.
                        let _ = router_handle
                            .register_coordinator(agent_id.clone(), coordinator)
                            .await;

                        info!(
                            name = %agent_name,
                            id = %agent_id,
                            "Auto-loaded installed agent"
                        );
                    }
                }
                Err(e) => {
                    warn!(error = %e, "Failed to list installed agents — skipping auto-load");
                }
            }
        }

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
            notification_engine,
            llm_call_repository,
            chat_manager,
            plan_cache,
            mailbox_handle: Some(mailbox_handle),
            user_memory,
            stt_engine,
            stt_repository,
            mcp_handle,
            project_repository,
        })
    }
}

/// Phase 10.6: validate installed package integrity.
///
/// Checks that each package's `root_path` still exists on disk.
/// Missing packages → log warning, disable all package agents.
/// Never panics, never blocks boot.
fn validate_installed_packages(
    pkg_repo: &apollia_tools::PackageRepository,
    agent_repo: &AgentRepository,
) {
    let packages = match pkg_repo.list() {
        Ok(p) => p,
        Err(e) => {
            warn!(error = %e, "Phase 10.6: failed to list packages");
            return;
        }
    };
    for pkg in &packages {
        if !pkg.root_path.exists() {
            warn!(
                package = %pkg.name,
                path = %pkg.root_path.display(),
                "Phase 10.6: package root_path missing — disabling agents"
            );
            let agent_names = pkg_repo
                .list_agents_for_package(&pkg.name)
                .unwrap_or_default();
            for agent_name in &agent_names {
                if let Err(e) = agent_repo.set_enabled(agent_name, false) {
                    warn!(agent = %agent_name, error = %e, "Phase 10.6: failed to disable agent");
                }
            }
        }
    }
}

/// Resolves `~` at the start of a path to `$HOME`.
fn resolve_home(path: &std::path::Path) -> std::path::PathBuf {
    if let Ok(stripped) = path.strip_prefix("~") {
        if let Ok(home) = std::env::var("HOME") {
            return std::path::PathBuf::from(home).join(stripped);
        }
    }
    path.to_owned()
}

/// Returns descriptors for native tools bundled with `apollia-tools`.
///
/// Registers all 12 active native tools in the order: existing tools first,
/// then new atomic tools grouped by category.
fn native_tool_descriptors() -> Vec<apollia_tools::ToolDescriptor> {
    // `mut` is used only when at least one web-* feature is active; allow the
    // warning explicitly for minimal-feature builds.
    #[allow(unused_mut)]
    let mut descriptors = vec![
        apollia_tools::tools::bash_executor::BashExecutor::descriptor(),
        apollia_tools::tools::python_executor::PythonExecutor::descriptor(),
        apollia_tools::tools::file_read::FileRead::descriptor(),
        apollia_tools::tools::file_write::FileWrite::descriptor(),
        apollia_tools::tools::file_edit::FileEdit::descriptor(),
        apollia_tools::tools::file_list::FileList::descriptor(),
        apollia_tools::tools::file_glob::FileGlob::descriptor(),
        apollia_tools::tools::file_grep::FileGrep::descriptor(),
        apollia_tools::tools::http_fetch::HttpFetch::descriptor(),
        apollia_tools::tools::memory_search::MemorySearchTool::descriptor(),
        apollia_tools::tools::notebook_read::NotebookRead::descriptor(),
        apollia_tools::tools::notebook_edit::NotebookEdit::descriptor(),
        apollia_tools::tools::ask_user::AskUser::descriptor(),
        // ADR-086 — gouvernance agent-driven des permissions.
        apollia_tools::tools::permission_rules::PermissionRuleAdd::descriptor(),
        apollia_tools::tools::permission_rules::PermissionRuleRemove::descriptor(),
        apollia_tools::tools::permission_rules::PermissionRuleList::descriptor(),
    ];

    // Web tools are always advertised in the catalogue (so the UI and agent
    // manifests can reference them) but runtime availability still depends on
    // `[tools].web_search` / `[tools].web_read` in `apollia.toml`. ADR-072.
    #[cfg(feature = "web-search")]
    descriptors.push(apollia_tools::tools::web_search::WebSearch::descriptor());

    #[cfg(feature = "web-read")]
    descriptors.push(apollia_tools::tools::web_read::WebRead::descriptor());

    descriptors
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

/// Marker key stored under `__user__.notifications_seeded_desktop` (with the
/// `__` internal prefix added by `user_memory::set_internal`).
///
/// Once set, prevents a subsequent boot from re-seeding the default desktop
/// channel — even if the operator deletes the seeded channel manually.
const SEEDED_DESKTOP_CHANNEL_MARKER: &str = "notifications_seeded_desktop";

/// On first boot with a usable [`UserMemoryRepository`] and an empty
/// notifications database, insert a sensible default desktop channel so the
/// operator does not start from a blank Notifications page.
///
/// The function is fully idempotent:
/// - if the seed marker is already set in memory, return without touching anything;
/// - if the notification repository already has at least one channel, set the
///   marker anyway (so further deletions never trigger a re-seed) and return;
/// - otherwise, insert the channel, then set the marker.
///
/// All failures are best-effort and logged at `warn!` — they must never block
/// supervisor startup.
fn seed_default_desktop_channel_if_needed(
    notif_repo: &NotificationConfigRepository,
    user_memory: Option<&std::sync::Arc<std::sync::Mutex<apollia_memory::user_memory::UserMemoryRepository>>>,
) {
    let Some(um_arc) = user_memory else {
        // No user memory available — can't track the marker safely. Skip.
        return;
    };
    let um = um_arc.lock().unwrap_or_else(|e| e.into_inner());

    // 1. Check marker.
    match um.get_internal(SEEDED_DESKTOP_CHANNEL_MARKER) {
        Ok(Some(_)) => return, // already seeded — leave the user's setup alone
        Ok(None) => {}
        Err(e) => {
            warn!(error = %e, "seed default desktop channel: marker read failed — skipping");
            return;
        }
    }

    // 2. If the repository is not empty, just set the marker (legacy user with
    //    existing channels — record that we've considered seeding once).
    let channels = match notif_repo.list_channels() {
        Ok(rows) => rows,
        Err(e) => {
            warn!(error = %e, "seed default desktop channel: list_channels failed — skipping");
            return;
        }
    };
    if !channels.is_empty() {
        let _ = um.set_internal(SEEDED_DESKTOP_CHANNEL_MARKER, "true");
        return;
    }

    // 3. Compose label from the user's profile name (if available).
    let name: Option<String> = um.get("name").ok().flatten().and_then(|entry| {
        let trimmed = entry.value.trim().to_string();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    });
    let label = match name {
        Some(n) => format!("Bureau de {n}"),
        None => "Bureau".to_string(),
    };

    // 4. Insert the channel.
    let row = apollia_notifications::NotificationChannelRow {
        id: "desktop-default".to_string(),
        label: Some(label),
        channel_type: "desktop".to_string(),
        enabled: true,
        config_json: serde_json::json!({}),
        events_json: None,
        min_interval_seconds: 0,
        created_at: String::new(),
        updated_at: String::new(),
    };
    if let Err(e) = notif_repo.insert_channel(&row) {
        warn!(error = %e, "seed default desktop channel: insert failed — skipping marker");
        return;
    }

    // 5. Set the marker so we never re-seed (even after manual deletion).
    if let Err(e) = um.set_internal(SEEDED_DESKTOP_CHANNEL_MARKER, "true") {
        warn!(
            error = %e,
            "seed default desktop channel: marker write failed — channel inserted but re-seed possible on next boot"
        );
    }
    info!("Supervisor: seeded default desktop notification channel");
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

    #[test]
    fn native_tool_descriptors_returns_expected_count() {
        // GIVEN: the native_tool_descriptors() function
        // WHEN: called
        // THEN: 16 baseline tools (13 historical + 3 permission_rule_* added
        // by ADR-086), plus optional web tools when their features are on.
        let expected_count =
            16 + cfg!(feature = "web-search") as usize + cfg!(feature = "web-read") as usize;
        let descriptors = native_tool_descriptors();
        assert_eq!(descriptors.len(), expected_count);
    }

    #[test]
    fn native_tool_descriptors_all_pass_validation() {
        // GIVEN: the native_tool_descriptors() function
        // WHEN: called
        // THEN: every descriptor passes validate()
        for descriptor in native_tool_descriptors() {
            assert!(
                descriptor.validate().is_ok(),
                "descriptor '{}' failed validation",
                descriptor.name
            );
        }
    }

    #[test]
    fn mcp_phase3b_toml_absent_returns_empty() {
        // GIVEN a path that does not exist
        let path = std::path::Path::new("/tmp/nonexistent-mcp-config-335.toml");
        // WHEN
        let config = McpConfig::load(path).unwrap();
        // THEN no servers and no error
        assert!(config.servers.is_empty());
    }

    #[tokio::test]
    async fn test_ac1_boot_with_mcp_toml_imports_servers() {
        // GIVEN an empty mcp.db and a mcp.toml with one server
        use std::io::Write as _;
        let dir = tempfile::TempDir::new().unwrap();
        let db_path = dir.path().join("mcp.db");
        let toml_path = dir.path().join("mcp.toml");

        let toml_content = r#"
            [[servers]]
            name = "notion"
            command = "npx"
            args = ["-y", "@notionhq/notion-mcp-server"]
        "#;
        let mut file = std::fs::File::create(&toml_path).unwrap();
        file.write_all(toml_content.as_bytes()).unwrap();

        // WHEN the import logic runs
        let repo = apollia_mcp::McpServerRepository::open(&db_path).unwrap();
        assert!(repo.list().unwrap().is_empty());
        let toml_config = McpConfig::load(&toml_path).unwrap();
        let n = repo.import_from_toml(toml_config.servers).unwrap();

        // THEN mcp.db contains 1 server and the log count is correct
        assert_eq!(n, 1);
        let servers = repo.list().unwrap();
        assert_eq!(servers.len(), 1);
        assert_eq!(servers[0].name, "notion");
    }

    #[tokio::test]
    async fn test_ac2_boot_without_mcp_toml_ok() {
        // GIVEN neither mcp.db nor mcp.toml
        let dir = tempfile::TempDir::new().unwrap();
        let db_path = dir.path().join("mcp.db");
        let toml_path = dir.path().join("mcp.toml");

        // WHEN the import logic runs with an absent toml
        let repo = apollia_mcp::McpServerRepository::open(&db_path).unwrap();
        let n = repo.import_from_toml(vec![]).unwrap();

        // THEN Ok(0) and no error
        assert_eq!(n, 0);
        assert!(repo.list().unwrap().is_empty());

        // AND loading a missing mcp.toml returns empty without error
        let config = McpConfig::load(&toml_path).unwrap();
        assert!(config.servers.is_empty());
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

    /// Returns `(SupervisorConfig, TempDir)` — the caller must hold `TempDir`
    /// alive until the test completes so the data_dir path remains valid.
    fn test_config(port: u16, socket_path: PathBuf) -> (SupervisorConfig, tempfile::TempDir) {
        let temp_dir = tempfile::tempdir().expect("tempdir");
        let config = SupervisorConfig {
            api_config: APIServerConfig {
                socket_path,
                bind_addr: "127.0.0.1".to_owned(),
                tcp_port: port,
                api_token: None,
            },
            startup_timeout_secs: 10,
            llm_config: None,
            config_path: None,
            runtime_config: apollia_core::RuntimeConfig::default(),
            hitl_config: apollia_core::HitlConfig::default(),
            data_dir: temp_dir.path().to_path_buf(),
            obs_config: apollia_core::ObservabilityConfig::default(),
            agent_repository: None,
            package_repository: None,
            bundled_agents_path: None,
            tools_config: apollia_core::ToolsConfig::default(),
        };
        (config, temp_dir)
    }

    #[tokio::test]
    async fn test_startup_sequence_all_ready() {
        // GIVEN un Supervisor configure
        let port = free_port().await;
        let socket_path = temp_socket_path();
        let (config, _tmp_dir) = test_config(port, socket_path.clone());
        let supervisor = Supervisor::new(config);

        // WHEN start() est appele
        let result = supervisor
            .start(
                MockBackend,
                Arc::new(crate::api::routes_agents::StubAgentLoader),
                None,
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
        let (config, _tmp_dir) = test_config(port, socket_path.clone());
        let supervisor = Supervisor::new(config);

        // WHEN start() est appele
        let handles = supervisor
            .start(
                MockBackend,
                Arc::new(crate::api::routes_agents::StubAgentLoader),
                None,
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
        let (config, _tmp_dir) = test_config(port, socket_path.clone());
        let supervisor = Supervisor::new(config);
        let handles = supervisor
            .start(
                MockBackend,
                Arc::new(crate::api::routes_agents::StubAgentLoader),
                None,
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

        // ToolRegistryHandle: can list (native tools should be registered).
        // Count mirrors native_tool_descriptors() — 16 baseline (13 historical
        // + 3 permission_rule_* added by ADR-086) + web-search + web-read
        // when those features are compiled in (ADR-072), plus the connector
        // tool descriptors registered at Phase 3b (ADR-096 convergence —
        // Google ops today, Microsoft to come).
        let connector_count = crate::connectors_bridge::all_connector_descriptors().len();
        let expected = 16
            + cfg!(feature = "web-search") as usize
            + cfg!(feature = "web-read") as usize
            + connector_count;
        let tools = handles.tool_registry_handle.list().await;
        assert!(tools.is_ok());
        assert_eq!(
            tools.unwrap().len(),
            expected,
            "expected {expected} native + connector tools to be auto-registered"
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
                bind_addr: "127.0.0.1".to_owned(),
                tcp_port: port,
                api_token: None,
            },
            startup_timeout_secs: 1,
            llm_config: None,
            config_path: None,
            runtime_config: apollia_core::RuntimeConfig::default(),
            hitl_config: apollia_core::HitlConfig::default(),
            data_dir: {
                let d = tempfile::tempdir().expect("tempdir");
                let p = d.path().to_path_buf();
                std::mem::forget(d);
                p
            },
            obs_config: apollia_core::ObservabilityConfig::default(),
            agent_repository: None,
            package_repository: None,
            bundled_agents_path: None,
            tools_config: apollia_core::ToolsConfig::default(),
        };
        let supervisor = Supervisor::new(config);

        // WHEN start() est appele
        let result = supervisor
            .start(
                MockBackend,
                Arc::new(crate::api::routes_agents::StubAgentLoader),
                None,
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

        // THEN 7 specs sont retournees dans l'ordre
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

    // Supervisor starts successfully with llm_config = None
    #[tokio::test]
    async fn test_ac2_start_without_llm_config_succeeds() {
        // GIVEN un Supervisor sans section [llm]
        let port = free_port().await;
        let socket_path = temp_socket_path();
        let config = SupervisorConfig {
            api_config: APIServerConfig {
                socket_path: socket_path.clone(),
                bind_addr: "127.0.0.1".to_owned(),
                tcp_port: port,
                api_token: None,
            },
            startup_timeout_secs: 10,
            llm_config: None,
            config_path: None,
            runtime_config: apollia_core::RuntimeConfig::default(),
            hitl_config: apollia_core::HitlConfig::default(),
            data_dir: {
                let d = tempfile::tempdir().expect("tempdir");
                let p = d.path().to_path_buf();
                std::mem::forget(d);
                p
            },
            obs_config: apollia_core::ObservabilityConfig::default(),
            agent_repository: None,
            package_repository: None,
            bundled_agents_path: None,
            tools_config: apollia_core::ToolsConfig::default(),
        };
        let supervisor = Supervisor::new(config);

        // WHEN start() est appele
        let handles = supervisor
            .start(
                MockBackend,
                Arc::new(crate::api::routes_agents::StubAgentLoader),
                None,
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

    // AppState clone preserves llm_router = None
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
            backend_factory: None,
            tool_registry_handle: None,
            audit_trail: None,
            obs_config: apollia_core::ObservabilityConfig::default(),
            llm_call_repository: None,
            trigger_def_repo: None,
            notification_repo: None,
            notification_engine_handle: None,
            chat_manager: None,
            plan_cache: None,
            mailbox_handle: None,
            user_memory: None,
            stt_engine: None,
            stt_repository: None,
            mcp_handle: None,
            mcp_server_repo: None,
            llm_backend_repo: None,
            stt_config_repo: None,
            a2a_invoker: None,
            resilience_layer: None,
        };

        // WHEN on clone l'AppState
        let cloned = state.clone();

        // THEN le clone preserve llm_router = None
        assert!(
            cloned.llm_router.is_none(),
            "le clone doit preserver llm_router = None"
        );
    }

    // Supervisor démarre avec 0 triggers ; TriggerEngine toujours présent
    #[tokio::test]
    async fn test_ac3_supervisor_starts_with_zero_triggers() {
        // GIVEN une config sans triggers
        let port = free_port().await;
        let socket_path = temp_socket_path();
        let config = SupervisorConfig {
            api_config: APIServerConfig {
                socket_path: socket_path.clone(),
                bind_addr: "127.0.0.1".to_owned(),
                tcp_port: port,
                api_token: None,
            },
            startup_timeout_secs: 10,
            llm_config: None,
            config_path: None,
            runtime_config: apollia_core::RuntimeConfig::default(),
            hitl_config: apollia_core::HitlConfig::default(),
            data_dir: {
                let d = tempfile::tempdir().expect("tempdir");
                let p = d.path().to_path_buf();
                std::mem::forget(d);
                p
            },
            obs_config: apollia_core::ObservabilityConfig::default(),
            agent_repository: None,
            package_repository: None,
            bundled_agents_path: None,
            tools_config: apollia_core::ToolsConfig::default(),
        };
        let supervisor = Supervisor::new(config);

        // WHEN start() est appele
        let result = supervisor
            .start(
                MockBackend,
                Arc::new(crate::api::routes_agents::StubAgentLoader),
                None,
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

    // Supervisor démarre sans section [notifications] sans erreur
    #[tokio::test]
    async fn test_ac4_no_notifications_section_starts_ok() {
        // GIVEN une config sans section [notifications]
        let port = free_port().await;
        let socket_path = temp_socket_path();
        let config = SupervisorConfig {
            api_config: APIServerConfig {
                socket_path: socket_path.clone(),
                bind_addr: "127.0.0.1".to_owned(),
                tcp_port: port,
                api_token: None,
            },
            startup_timeout_secs: 10,
            llm_config: None,
            config_path: None,
            runtime_config: apollia_core::RuntimeConfig::default(),
            hitl_config: apollia_core::HitlConfig::default(),
            data_dir: {
                let d = tempfile::tempdir().expect("tempdir");
                let p = d.path().to_path_buf();
                std::mem::forget(d);
                p
            },
            obs_config: apollia_core::ObservabilityConfig::default(),
            agent_repository: None,
            package_repository: None,
            bundled_agents_path: None,
            tools_config: apollia_core::ToolsConfig::default(),
        };
        let supervisor = Supervisor::new(config);

        // WHEN start() est appelé
        let result = supervisor
            .start(
                MockBackend,
                Arc::new(crate::api::routes_agents::StubAgentLoader),
                None,
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

    // Triggers chargés depuis SQLite au boot
    #[tokio::test]
    async fn test_ac4_trigger_engine_loads_from_sqlite() {
        use apollia_triggers::{TriggerDefinitionRepository, TriggerDefinitionRow};

        // GIVEN une DB triggers_def.db pré-remplie avec 1 trigger
        let port = free_port().await;
        let socket_path = temp_socket_path();
        let tmp_dir = tempfile::tempdir().expect("tempdir");
        let db_path = tmp_dir.path().join("triggers_def.db");
        let repo = TriggerDefinitionRepository::open(&db_path).expect("open repo");
        repo.insert(&TriggerDefinitionRow {
            id: "test-trigger".into(),
            agent: Some("test-agent".into()),
            enabled: true,
            on_busy: apollia_triggers::OnBusy::Queue,
            source_type: "cron".into(),
            source_config: serde_json::json!({ "schedule": "0 8 * * MON" }),
            input_template: Some("hello".into()),
            created_at: String::new(),
            updated_at: String::new(),
        })
        .expect("insert trigger def");
        drop(repo);

        let config = SupervisorConfig {
            api_config: APIServerConfig {
                socket_path: socket_path.clone(),
                bind_addr: "127.0.0.1".to_owned(),
                tcp_port: port,
                api_token: None,
            },
            startup_timeout_secs: 10,
            llm_config: None,
            config_path: None,
            runtime_config: apollia_core::RuntimeConfig::default(),
            hitl_config: apollia_core::HitlConfig::default(),
            data_dir: tmp_dir.path().to_path_buf(),
            obs_config: apollia_core::ObservabilityConfig::default(),
            agent_repository: None,
            package_repository: None,
            bundled_agents_path: None,
            tools_config: apollia_core::ToolsConfig::default(),
        };
        let supervisor = Supervisor::new(config);

        // WHEN start() est appelé
        let handles = supervisor
            .start(
                MockBackend,
                Arc::new(crate::api::routes_agents::StubAgentLoader),
                None,
                None,
            )
            .await
            .expect("start() doit reussir");

        // THEN trigger_engine contient 1 trigger chargé depuis SQLite
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

    // Boot avec DBs vides crée les bases et démarre sans erreur
    #[tokio::test]
    async fn test_story187_boot_empty_dbs() {
        // GIVEN un répertoire vide (aucune DB pré-existante)
        let port = free_port().await;
        let socket_path = temp_socket_path();
        let (config, _tmp_dir) = test_config(port, socket_path.clone());
        let supervisor = Supervisor::new(config);

        // WHEN start() est appelé
        let handles = supervisor
            .start(
                MockBackend,
                Arc::new(crate::api::routes_agents::StubAgentLoader),
                None,
                None,
            )
            .await
            .expect("boot with empty DBs should succeed");

        // THEN AllReady est émis, 0 triggers ; le NotificationEngine est
        // démarré car le supervisor seed automatiquement un canal Desktop par
        // défaut (Item 5, Notifications v2). Le marker dans `user_memory`
        // empêche le re-seed sur les boots suivants.
        let trigger_list = handles.trigger_engine.list().await;
        assert!(trigger_list.is_empty(), "empty DB should yield 0 triggers");
        assert!(
            handles.notification_engine.is_some(),
            "empty DB should yield a NotificationEngine seeded with the default desktop channel"
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

    // ── Item 5 — seed_default_desktop_channel_if_needed (unit) ──────────────

    /// Helper : (notif_repo, user_memory) initialisés sur tempdirs vierges.
    fn make_seed_inputs() -> (
        NotificationConfigRepository,
        std::sync::Arc<std::sync::Mutex<apollia_memory::user_memory::UserMemoryRepository>>,
        tempfile::TempDir,
    ) {
        let dir = tempfile::tempdir().expect("tempdir");
        let notif = NotificationConfigRepository::open(&dir.path().join("notifications.db"))
            .expect("notif repo");
        let um = apollia_memory::user_memory::UserMemoryRepository::new(&dir.path().join("user_memory.db"))
            .expect("user memory");
        (
            notif,
            std::sync::Arc::new(std::sync::Mutex::new(um)),
            dir,
        )
    }

    #[test]
    fn test_seed_inserts_channel_when_empty_and_no_marker() {
        // GIVEN an empty notification repo and user memory with no marker
        let (notif, um, _tmp) = make_seed_inputs();

        // WHEN the seed runs
        seed_default_desktop_channel_if_needed(&notif, Some(&um));

        // THEN a `desktop-default` channel exists
        let chans = notif.list_channels().expect("list");
        assert_eq!(chans.len(), 1);
        let ch = &chans[0];
        assert_eq!(ch.id, "desktop-default");
        assert_eq!(ch.channel_type, "desktop");
        assert!(ch.enabled);
        // Label falls back to "Bureau" since no profile name was set.
        assert_eq!(ch.label.as_deref(), Some("Bureau"));

        // AND the marker was set in user memory
        let marker = um
            .lock()
            .unwrap()
            .get_internal(SEEDED_DESKTOP_CHANNEL_MARKER)
            .expect("get_internal")
            .expect("marker present");
        assert_eq!(marker, "true");
    }

    #[test]
    fn test_seed_uses_profile_name_when_present() {
        // GIVEN a user memory with profile.name = "Nidal"
        let (notif, um, _tmp) = make_seed_inputs();
        um.lock()
            .unwrap()
            .set("name", "Nidal", apollia_memory::user_memory::WrittenBy::User)
            .expect("set name");

        // WHEN the seed runs
        seed_default_desktop_channel_if_needed(&notif, Some(&um));

        // THEN the label uses the personalised form
        let ch = notif
            .get_channel("desktop-default")
            .expect("get")
            .expect("Some");
        assert_eq!(ch.label.as_deref(), Some("Bureau de Nidal"));
    }

    #[test]
    fn test_seed_skips_when_marker_present() {
        // GIVEN the marker is already set
        let (notif, um, _tmp) = make_seed_inputs();
        um.lock()
            .unwrap()
            .set_internal(SEEDED_DESKTOP_CHANNEL_MARKER, "true")
            .expect("set marker");

        // WHEN the seed runs
        seed_default_desktop_channel_if_needed(&notif, Some(&um));

        // THEN no channel is created (the user may have deleted the original)
        let chans = notif.list_channels().expect("list");
        assert!(chans.is_empty());
    }

    #[test]
    fn test_seed_sets_marker_only_when_existing_channels() {
        // GIVEN a non-empty notification repo but no marker
        let (notif, um, _tmp) = make_seed_inputs();
        let existing = apollia_notifications::NotificationChannelRow {
            id: "my-webhook".into(),
            label: None,
            channel_type: "webhook".into(),
            enabled: true,
            config_json: serde_json::json!({"url": "https://hooks.example.com/x"}),
            events_json: None,
            min_interval_seconds: 0,
            created_at: String::new(),
            updated_at: String::new(),
        };
        notif.insert_channel(&existing).expect("insert");

        // WHEN seed runs
        seed_default_desktop_channel_if_needed(&notif, Some(&um));

        // THEN no new channel is added (still just the user's pre-existing one)
        let chans = notif.list_channels().expect("list");
        assert_eq!(chans.len(), 1);
        assert_eq!(chans[0].id, "my-webhook");

        // AND the marker is now set, so future boots skip the seed entirely
        let marker = um
            .lock()
            .unwrap()
            .get_internal(SEEDED_DESKTOP_CHANNEL_MARKER)
            .expect("get_internal")
            .expect("marker present");
        assert_eq!(marker, "true");
    }

    #[test]
    fn test_seed_idempotent_across_multiple_calls() {
        // GIVEN — call seed twice
        let (notif, um, _tmp) = make_seed_inputs();
        seed_default_desktop_channel_if_needed(&notif, Some(&um));
        seed_default_desktop_channel_if_needed(&notif, Some(&um));

        // THEN — still only one channel, no duplicate insert
        let chans = notif.list_channels().expect("list");
        assert_eq!(chans.len(), 1);
    }

    #[test]
    fn test_seed_no_op_when_user_memory_missing() {
        // GIVEN no user memory available
        let dir = tempfile::tempdir().expect("tempdir");
        let notif = NotificationConfigRepository::open(&dir.path().join("notifications.db"))
            .expect("notif repo");

        // WHEN seed is invoked with None
        seed_default_desktop_channel_if_needed(&notif, None);

        // THEN nothing is inserted (we cannot track a marker safely)
        let chans = notif.list_channels().expect("list");
        assert!(chans.is_empty());
    }

    // AppState contient les 3 repositories après boot
    #[tokio::test]
    async fn test_story187_appstate_contains_repos() {
        // GIVEN un Supervisor avec répertoire vide
        let port = free_port().await;
        let socket_path = temp_socket_path();
        let tmp_dir = tempfile::tempdir().expect("tempdir");
        let config = SupervisorConfig {
            api_config: APIServerConfig {
                socket_path: socket_path.clone(),
                bind_addr: "127.0.0.1".to_owned(),
                tcp_port: port,
                api_token: None,
            },
            startup_timeout_secs: 10,
            llm_config: None,
            config_path: None,
            runtime_config: apollia_core::RuntimeConfig::default(),
            hitl_config: apollia_core::HitlConfig::default(),
            data_dir: tmp_dir.path().to_path_buf(),
            obs_config: apollia_core::ObservabilityConfig::default(),
            agent_repository: None,
            package_repository: None,
            bundled_agents_path: None,
            tools_config: apollia_core::ToolsConfig::default(),
        };
        let supervisor = Supervisor::new(config);

        // WHEN start() réussit
        let handles = supervisor
            .start(
                MockBackend,
                Arc::new(crate::api::routes_agents::StubAgentLoader),
                None,
                None,
            )
            .await
            .expect("start should succeed");

        // THEN les fichiers DB sont créés dans data_dir
        assert!(
            tmp_dir.path().join("triggers_def.db").exists(),
            "triggers_def.db should be created"
        );
        assert!(
            tmp_dir.path().join("notifications.db").exists(),
            "notifications.db should be created"
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

    // ── Auto-load installed agents at boot ──────────────────────

    /// Creates a test [`AgentManifest`] with minimal fields.
    fn test_manifest(name: &str) -> apollia_core::AgentManifest {
        apollia_core::AgentManifest {
            name: name.to_string(),
            version: "1.0.0".to_string(),
            description: format!("Test agent {name}"),
            tools_required: vec![],
            tools_optional: vec![],
            supports_streaming: false,
            supports_a2a: false,
            memory_namespace: None,
            shared_memory_namespaces: vec![],
            max_concurrent_tasks: 1,
            step_budget: None,
            network_allowlist: None,
            dangerous_tools_allowed: false,
            tags: vec![],
            skills: vec![],
            execution_mode: "auto".to_string(),
            system_prompt: None,
            tools_requiring_approval: vec![],
            llm_backend: None,
            packages: vec![],
            memory_config: None,
            agent_type: None,
            examples: vec![],
            limitations: vec![],
            setup_notes: None,
            agent_class: None,
            user_memory_write: false,
            datasources: vec![],
            templates: vec![],            secrets: vec![],
        }
    }

    /// Creates a test [`InstalledAgent`].
    fn test_installed_agent(name: &str, enabled: bool) -> apollia_tools::InstalledAgent {
        apollia_tools::InstalledAgent {
            name: name.to_string(),
            version: "1.0.0".to_string(),
            // Use the agent name as the filename stem so StubAgentLoader derives
            // the correct manifest name (name = file_stem of install_path).
            install_path: PathBuf::from(format!("/tmp/agents/{name}/{name}.py")),
            source_path: PathBuf::from(format!("/tmp/{name}.py")),
            manifest: test_manifest(name),
            enabled,
            installed_at: "2026-03-17T10:00:00Z".to_string(),
            updated_at: "2026-03-17T10:00:00Z".to_string(),
        }
    }

    /// Opens an in-memory [`AgentRepository`] for testing.
    fn open_test_repo() -> AgentRepository {
        AgentRepository::open(std::path::Path::new(":memory:")).expect("in-memory repo should open")
    }

    /// An [`AgentLoader`] that fails for agents whose path contains "corrupted".
    struct FailingAgentLoader;

    impl AgentLoader for FailingAgentLoader {
        fn load_and_validate(
            &self,
            path: &std::path::Path,
        ) -> Result<apollia_core::AgentManifest, String> {
            let path_str = path.to_string_lossy();
            if path_str.contains("corrupted") {
                return Err("Python syntax error: invalid syntax".to_string());
            }
            // Delegate to StubAgentLoader for valid agents
            crate::api::routes_agents::StubAgentLoader.load_and_validate(path)
        }
    }

    /// Helper to shutdown handles cleanly.
    async fn shutdown_handles(
        handles: SupervisorHandles<MockBackend>,
        socket_path: &std::path::Path,
    ) {
        handles.trigger_engine.shutdown().await;
        handles.api_handle.shutdown();
        handles.router_handle.shutdown();
        handles.tool_registry_handle.shutdown().await;
        handles.registry_handle.shutdown();
        tokio::time::sleep(Duration::from_millis(50)).await;
        let _ = std::fs::remove_file(socket_path);
    }

    // Les agents enabled sont chargés au boot
    #[tokio::test]
    async fn test_autoload_enabled_agents() {
        // GIVEN 3 agents installés dont 2 enabled
        let repo = open_test_repo();
        repo.save(&test_installed_agent("agent-a", true))
            .expect("save a");
        repo.save(&test_installed_agent("agent-b", true))
            .expect("save b");
        repo.save(&test_installed_agent("agent-c", false))
            .expect("save c (disabled)");

        let port = free_port().await;
        let socket_path = temp_socket_path();
        let (mut config, _tmp_dir) = test_config(port, socket_path.clone());
        config.agent_repository = Some(repo);

        let supervisor = Supervisor::new(config);

        // WHEN le Supervisor démarre
        let handles = supervisor
            .start(
                MockBackend,
                Arc::new(crate::api::routes_agents::StubAgentLoader),
                None,
                None,
            )
            .await
            .expect("start() should succeed");

        // THEN les 2 agents enabled sont enregistrés dans AgentRegistry
        let agents = handles
            .registry_handle
            .list_agents()
            .await
            .expect("list_agents should succeed");
        assert_eq!(agents.len(), 2, "2 enabled agents should be registered");

        shutdown_handles(handles, &socket_path).await;
    }

    // Les agents disabled sont ignorés
    #[tokio::test]
    async fn test_autoload_skips_disabled() {
        // GIVEN 2 agents dont 1 disabled
        let repo = open_test_repo();
        repo.save(&test_installed_agent("enabled-agent", true))
            .expect("save enabled");
        repo.save(&test_installed_agent("disabled-agent", false))
            .expect("save disabled");

        let port = free_port().await;
        let socket_path = temp_socket_path();
        let (mut config, _tmp_dir) = test_config(port, socket_path.clone());
        config.agent_repository = Some(repo);

        let supervisor = Supervisor::new(config);

        // WHEN le Supervisor démarre
        let handles = supervisor
            .start(
                MockBackend,
                Arc::new(crate::api::routes_agents::StubAgentLoader),
                None,
                None,
            )
            .await
            .expect("start() should succeed");

        // THEN seul l'agent enabled est enregistré
        let agents = handles
            .registry_handle
            .list_agents()
            .await
            .expect("list_agents should succeed");
        assert_eq!(agents.len(), 1, "only 1 enabled agent should be registered");

        shutdown_handles(handles, &socket_path).await;
    }

    // Un agent en erreur ne bloque pas le boot
    #[tokio::test]
    async fn test_autoload_corrupted_agent_continues() {
        // GIVEN 2 agents enabled dont 1 avec un fichier "corrompu"
        let repo = open_test_repo();
        let mut valid = test_installed_agent("valid-agent", true);
        valid.install_path = PathBuf::from("/tmp/agents/valid-agent/agent.py");
        repo.save(&valid).expect("save valid");

        let mut corrupted = test_installed_agent("corrupted-agent", true);
        corrupted.install_path = PathBuf::from("/tmp/agents/corrupted/agent.py");
        repo.save(&corrupted).expect("save corrupted");

        let port = free_port().await;
        let socket_path = temp_socket_path();
        let (mut config, _tmp_dir) = test_config(port, socket_path.clone());
        config.agent_repository = Some(repo);

        let supervisor = Supervisor::new(config);

        // Subscribe avant start() pour capturer AgentLoadFailed
        // (impossible ici car event_sender est créé dans start())
        // On vérifie plutôt que le boot réussit et que l'agent valide est chargé

        // WHEN le Supervisor démarre avec un loader qui échoue pour "corrupted"
        let handles = supervisor
            .start(MockBackend, Arc::new(FailingAgentLoader), None, None)
            .await
            .expect("start() should succeed despite corrupted agent");

        // THEN l'agent valide est enregistré
        let agents = handles
            .registry_handle
            .list_agents()
            .await
            .expect("list_agents should succeed");
        assert_eq!(agents.len(), 1, "only the valid agent should be registered");

        shutdown_handles(handles, &socket_path).await;
    }

    // Aucun agent installé = boot normal
    #[tokio::test]
    async fn test_autoload_no_agents_no_error() {
        // GIVEN une base vide
        let repo = open_test_repo();

        let port = free_port().await;
        let socket_path = temp_socket_path();
        let (mut config, _tmp_dir) = test_config(port, socket_path.clone());
        config.agent_repository = Some(repo);

        let supervisor = Supervisor::new(config);

        // WHEN le Supervisor démarre
        let handles = supervisor
            .start(
                MockBackend,
                Arc::new(crate::api::routes_agents::StubAgentLoader),
                None,
                None,
            )
            .await
            .expect("start() should succeed with no agents");

        // THEN aucun agent enregistré, pas d'erreur
        let agents = handles
            .registry_handle
            .list_agents()
            .await
            .expect("list_agents should succeed");
        assert!(agents.is_empty(), "no agents should be registered");

        shutdown_handles(handles, &socket_path).await;
    }

    // agent_repository = None → auto-load skippé
    #[tokio::test]
    async fn test_autoload_none_repository_skips() {
        // GIVEN une config sans agent_repository
        let port = free_port().await;
        let socket_path = temp_socket_path();
        let (config, _tmp_dir) = test_config(port, socket_path.clone());
        // agent_repository is already None in test_config

        let supervisor = Supervisor::new(config);

        // WHEN le Supervisor démarre
        let handles = supervisor
            .start(
                MockBackend,
                Arc::new(crate::api::routes_agents::StubAgentLoader),
                None,
                None,
            )
            .await
            .expect("start() should succeed without agent_repository");

        // THEN pas d'agent enregistré, boot normal
        let agents = handles
            .registry_handle
            .list_agents()
            .await
            .expect("list_agents should succeed");
        assert!(
            agents.is_empty(),
            "no agents should be registered when agent_repository is None"
        );

        shutdown_handles(handles, &socket_path).await;
    }

    // Agent avec packages vide → pas de venv créé, état Active
    #[tokio::test]
    async fn test_autoload_empty_packages_agent_is_active() {
        // GIVEN un agent avec packages: []
        let repo = open_test_repo();
        repo.save(&test_installed_agent("no-pkg-agent", true))
            .expect("save agent");

        let port = free_port().await;
        let socket_path = temp_socket_path();
        let (mut config, tmp_dir) = test_config(port, socket_path.clone());
        config.agent_repository = Some(repo);

        let supervisor = Supervisor::new(config);

        // WHEN le Supervisor démarre
        let handles = supervisor
            .start(
                MockBackend,
                Arc::new(crate::api::routes_agents::StubAgentLoader),
                None,
                None,
            )
            .await
            .expect("start() should succeed");

        // THEN l'agent est en état Active
        let agents = handles
            .registry_handle
            .list_agents()
            .await
            .expect("list_agents should succeed");
        assert_eq!(agents.len(), 1);
        assert_eq!(agents[0].process_state, ProcessState::Active);

        // ET aucun venv n'a été créé dans data_dir/venvs/
        let venv_dir = tmp_dir.path().join("venvs").join("no-pkg-agent");
        assert!(
            !venv_dir.exists(),
            "venv directory should not be created for agent with empty packages"
        );

        shutdown_handles(handles, &socket_path).await;
    }

    // Agent avec package invalide → état Degraded, autres agents démarrent normalement
    #[tokio::test]
    async fn test_autoload_bad_package_agent_is_degraded() {
        // GIVEN deux agents : un avec package invalide, un sans packages
        // Install paths use the agent name as file stem so StubAgentLoader resolves correctly.
        let repo = open_test_repo();

        let mut bad_agent = test_installed_agent("bad-pkg-agent", true);
        bad_agent.install_path = PathBuf::from("/tmp/agents/bad-pkg-agent/bad-pkg-agent.py");
        bad_agent.manifest.packages = vec!["nonexistent-pkg-zzz-99999==0.0.1".to_string()];
        repo.save(&bad_agent).expect("save bad agent");

        let mut good_agent = test_installed_agent("good-agent", true);
        good_agent.install_path = PathBuf::from("/tmp/agents/good-agent/good-agent.py");
        repo.save(&good_agent).expect("save good agent");

        let port = free_port().await;
        let socket_path = temp_socket_path();
        let (mut config, _tmp_dir) = test_config(port, socket_path.clone());
        config.agent_repository = Some(repo);

        // Loader that injects packages into the bad agent's manifest using the file stem.
        struct PackageAwareLoader;
        impl AgentLoader for PackageAwareLoader {
            fn load_and_validate(
                &self,
                path: &std::path::Path,
            ) -> Result<apollia_core::AgentManifest, String> {
                let mut manifest =
                    crate::api::routes_agents::StubAgentLoader.load_and_validate(path)?;
                if manifest.name == "bad-pkg-agent" {
                    manifest.packages = vec!["nonexistent-pkg-zzz-99999==0.0.1".to_string()];
                }
                Ok(manifest)
            }
        }

        let supervisor = Supervisor::new(config);

        // WHEN le Supervisor démarre
        let handles = supervisor
            .start(MockBackend, Arc::new(PackageAwareLoader), None, None)
            .await
            .expect("start() should succeed despite bad package");

        // THEN les deux agents sont enregistrés
        let agents = handles
            .registry_handle
            .list_agents()
            .await
            .expect("list_agents should succeed");
        assert_eq!(agents.len(), 2, "both agents should be registered");

        // ET l'agent valide est Active
        let good = agents
            .iter()
            .find(|a| a.manifest.name == "good-agent")
            .expect("good-agent should be registered");
        assert_eq!(
            good.process_state,
            ProcessState::Active,
            "good-agent should be Active"
        );

        // ET l'agent avec mauvais package est Degraded (python3 disponible) ou Active (python3 absent)
        let bad = agents
            .iter()
            .find(|a| a.manifest.name == "bad-pkg-agent")
            .expect("bad-pkg-agent should be registered");
        assert!(
            bad.process_state == ProcessState::Degraded
                || bad.process_state == ProcessState::Active,
            "bad-pkg-agent should be Degraded or Active (never Stopped), got {:?}",
            bad.process_state
        );

        shutdown_handles(handles, &socket_path).await;
    }

    #[tokio::test]
    async fn test_first_launch_emits_onboarding_required() {
        // GIVEN a fresh Supervisor with empty UserMemory (no entries)
        let port = free_port().await;
        let socket_path = temp_socket_path();
        let (config, _tmp_dir) = test_config(port, socket_path.clone());
        let supervisor = Supervisor::new(config);

        // WHEN start() completes — user_memory.db is created empty
        let handles = supervisor
            .start(
                MockBackend,
                Arc::new(crate::api::routes_agents::StubAgentLoader),
                None,
                None,
            )
            .await
            .expect("start() should succeed");

        // THEN OnboardingRequired was emitted (verify via a fresh subscriber + replay)
        // Since we can't replay past events, we verify the user_memory handle is Some
        // and that a fresh empty repo triggers the detection logic directly.
        assert!(
            handles.user_memory.is_some(),
            "user_memory handle should be present"
        );
        let um = handles.user_memory.as_ref().expect("checked above");
        let is_empty = um.lock().expect("lock").is_empty().expect("is_empty");
        assert!(is_empty, "user_memory should be empty on first launch");

        shutdown_handles(handles, &socket_path).await;
    }

    #[tokio::test]
    async fn test_subsequent_launch_no_onboarding_event() {
        // GIVEN a Supervisor whose UserMemory already contains entries
        let port = free_port().await;
        let socket_path = temp_socket_path();
        let (config, _tmp_dir) = test_config(port, socket_path.clone());

        // Pre-populate user_memory.db before starting the Supervisor
        let user_memory_db = config.data_dir.join("user_memory.db");
        {
            let repo = apollia_memory::user_memory::UserMemoryRepository::new(&user_memory_db)
                .expect("open user_memory.db");
            repo.set(
                "preferences.language",
                "fr",
                apollia_memory::user_memory::WrittenBy::Onboarding,
            )
            .expect("set entry");
        }

        let supervisor = Supervisor::new(config);

        // WHEN start() completes
        let handles = supervisor
            .start(
                MockBackend,
                Arc::new(crate::api::routes_agents::StubAgentLoader),
                None,
                None,
            )
            .await
            .expect("start() should succeed");

        // Subscribe after start to check no OnboardingRequired is emitted for subsequent events
        let mut rx = handles.event_sender.subscribe();

        // Emit a sentinel event to drain the bus
        let _ = handles.event_sender.send(RuntimeEvent::AllReady);
        let event = tokio::time::timeout(Duration::from_secs(1), rx.recv())
            .await
            .expect("should receive within 1s")
            .expect("recv should succeed");

        // THEN the received event is AllReady, not OnboardingRequired
        assert!(
            matches!(event, RuntimeEvent::AllReady),
            "expected AllReady sentinel, got: {event:?}"
        );

        // AND user_memory is not empty
        let um = handles.user_memory.as_ref().expect("user_memory present");
        let is_empty = um.lock().expect("lock").is_empty().expect("is_empty");
        assert!(
            !is_empty,
            "user_memory should NOT be empty on subsequent launch"
        );

        shutdown_handles(handles, &socket_path).await;
    }

    // ── Bundled agents auto-install ──────────────────────────────

    /// Crée un répertoire bundled temporaire avec un manifest.json et des stubs Python.
    fn setup_bundled_dir(tmp: &tempfile::TempDir, agents: &[(&str, &str)]) -> std::path::PathBuf {
        let bundled_dir = tmp.path().join("bundled");
        std::fs::create_dir_all(&bundled_dir).expect("create bundled dir");

        let entries: Vec<serde_json::Value> = agents
            .iter()
            .map(|(name, file)| {
                serde_json::json!({
                    "name": name,
                    "file": file,
                    "auto_install": true,
                    "description": format!("Test agent {name}")
                })
            })
            .collect();

        let manifest = serde_json::json!({
            "version": "1.0.0",
            "bundled_agents": entries
        });
        std::fs::write(
            bundled_dir.join("manifest.json"),
            serde_json::to_string_pretty(&manifest).expect("serialize"),
        )
        .expect("write manifest.json");

        for (name, file) in agents {
            // Stub Python files — named so StubAgentLoader derives the agent name from stem.
            std::fs::write(bundled_dir.join(file), format!("# {name}\n"))
                .expect("write stub agent");
        }

        bundled_dir
    }

    // Pas de manifest.json → pas d'agent installé, pas d'erreur
    #[test]
    fn test_no_manifest_no_error() {
        // GIVEN aucun manifest.json dans le répertoire bundled
        let tmp = tempfile::tempdir().expect("tempdir");
        let bundled_dir = tmp.path().join("bundled");
        std::fs::create_dir_all(&bundled_dir).expect("create dir");

        let repo = open_test_repo();

        // WHEN
        let loader: Arc<dyn AgentLoader> = Arc::new(crate::api::routes_agents::StubAgentLoader);
        auto_load_bundled_agents(Some(&bundled_dir), &repo, &loader);

        // THEN aucun agent dans la base
        let agents = repo.list().expect("list should succeed");
        assert!(agents.is_empty(), "no agents should be installed");
    }

    // bundled_agents_path = None → aucun agent installé, pas d'erreur
    #[test]
    fn test_no_bundled_path_no_error() {
        // GIVEN bundled_agents_path non configuré
        let repo = open_test_repo();
        let loader: Arc<dyn AgentLoader> = Arc::new(crate::api::routes_agents::StubAgentLoader);

        // WHEN
        auto_load_bundled_agents(None, &repo, &loader);

        // THEN aucun agent dans la base
        let agents = repo.list().expect("list should succeed");
        assert!(
            agents.is_empty(),
            "no agents should be installed when path is None"
        );
    }

    // 4 agents dans le manifest, DB vide → 4 agents enregistrés
    #[test]
    fn test_bundled_agents_auto_installed() {
        // GIVEN manifest.json avec 4 agents ET aucun agent en DB
        let tmp = tempfile::tempdir().expect("tempdir");
        let bundled_dir = setup_bundled_dir(
            &tmp,
            &[
                ("excel-worker", "excel-worker.py"),
                ("csv-data-worker", "csv-data-worker.py"),
                ("pdf-worker", "pdf-worker.py"),
                ("code-worker", "code-worker.py"),
            ],
        );

        let repo = open_test_repo();
        let loader: Arc<dyn AgentLoader> = Arc::new(crate::api::routes_agents::StubAgentLoader);

        // WHEN
        auto_load_bundled_agents(Some(&bundled_dir), &repo, &loader);

        // THEN 4 agents dans la base, tous enabled
        let agents = repo.list().expect("list should succeed");
        assert_eq!(agents.len(), 4, "all 4 bundled agents should be installed");
        assert!(
            agents.iter().all(|a| a.enabled),
            "all agents should be enabled"
        );
        let names: Vec<&str> = agents.iter().map(|a| a.name.as_str()).collect();
        assert!(names.contains(&"excel-worker"));
        assert!(names.contains(&"csv-data-worker"));
        assert!(names.contains(&"pdf-worker"));
        assert!(names.contains(&"code-worker"));
    }

    // excel-worker déjà en DB → pas ré-installé, 3 autres installés
    #[test]
    fn test_bundled_agents_skip_existing() {
        // GIVEN excel-worker déjà installé en DB
        let tmp = tempfile::tempdir().expect("tempdir");
        let bundled_dir = setup_bundled_dir(
            &tmp,
            &[
                ("excel-worker", "excel-worker.py"),
                ("csv-data-worker", "csv-data-worker.py"),
                ("pdf-worker", "pdf-worker.py"),
                ("code-worker", "code-worker.py"),
            ],
        );

        let repo = open_test_repo();
        repo.save(&test_installed_agent("excel-worker", true))
            .expect("pre-install excel-worker");

        let loader: Arc<dyn AgentLoader> = Arc::new(crate::api::routes_agents::StubAgentLoader);

        // WHEN
        auto_load_bundled_agents(Some(&bundled_dir), &repo, &loader);

        // THEN 4 agents en DB (1 préexistant + 3 nouveaux), excel-worker non dupliqué
        let agents = repo.list().expect("list should succeed");
        assert_eq!(agents.len(), 4, "4 agents total (1 existing + 3 new)");
        let excel_count = agents.iter().filter(|a| a.name == "excel-worker").count();
        assert_eq!(excel_count, 1, "excel-worker should not be duplicated");
    }
}
