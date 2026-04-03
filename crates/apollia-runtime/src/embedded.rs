//! Embedded runtime — starts the full Supervisor inside a dedicated thread.
//!
//! Designed for Tauri v2 integration: the desktop process calls
//! [`init_embedded()`] once at startup, receives a [`RuntimeHandle`] with all
//! actor handles, and passes it to `tauri::Builder::manage()`.
//!
//! The Tokio runtime lives in a separate OS thread so the Tauri main thread
//! (which drives the native event loop) is never blocked after init.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use apollia_core::{A2AConfig, HitlConfig, ObservabilityConfig, PendingApprovals, RuntimeConfig};
use apollia_llm::LlmRouter;
use apollia_notifications::NotificationEngineHandle;
use apollia_pipelines::PipelineEngineHandle;
use apollia_tools::{AgentRepository, AuditTrailHandle, TaskRepository};

use crate::api::routes_agents::{AgentBackendFactory, AgentLoader, StubAgentLoader};
use crate::api::{APIServerConfig, APIServerHandle};
use crate::coordinator::{DynBackend, ExecutionBackend};
use crate::eventbus::EventBusSender;
use crate::registry::AgentRegistryHandle;
use crate::router::TaskRouterHandle;
use crate::supervisor::{Supervisor, SupervisorConfig, SupervisorError};
use apollia_tools::ToolRegistryHandle;
use apollia_triggers::TriggerEngineHandle;

/// Default timeout (in seconds) to wait for the Supervisor to emit `AllReady`.
const DEFAULT_STARTUP_TIMEOUT_SECS: u64 = 30;

/// Handle vers le runtime embarqué, contenant tous les handles d'acteurs.
///
/// Passé à Tauri via `manage()` pour être accessible dans les commandes IPC.
/// Tous les champs sont `Clone + Send + Sync`. Les handles qui n'implémentent
/// pas `Clone` nativement sont wrappés dans `Arc`.
#[derive(Clone)]
pub struct RuntimeHandle {
    /// Sender pour publier des événements sur l'EventBus.
    pub event_sender: EventBusSender,
    /// Handle vers l'AgentRegistry.
    pub registry_handle: AgentRegistryHandle,
    /// Handle vers le ToolRegistry.
    pub tool_registry_handle: ToolRegistryHandle,
    /// Handle vers le TaskRouter.
    pub router_handle: TaskRouterHandle<DynBackend>,
    /// Handle vers l'APIServer (pour shutdown). Wrappé dans `Arc` car
    /// `APIServerHandle` n'implémente pas `Clone` (watch::Sender interne).
    pub api_handle: Arc<APIServerHandle>,
    /// Routeur LLM optionnel.
    pub llm_router: Option<Arc<LlmRouter>>,
    /// Handle vers le TriggerEngine.
    pub trigger_engine: TriggerEngineHandle,
    /// Handle vers le PipelineEngine optionnel.
    pub pipeline_engine: Option<PipelineEngineHandle>,
    /// Handle vers l'AuditTrail optionnel.
    pub audit_trail: Option<AuditTrailHandle>,
    /// Accès au TaskRepository pour lecture.
    pub task_repository: Option<Arc<TaskRepository>>,
    /// Approbations en attente.
    pub pending_approvals: Option<Arc<PendingApprovals>>,
    /// Handle vers le NotificationEngine optionnel. Wrappé dans `Arc` car
    /// `NotificationEngineHandle` n'implémente pas `Clone`.
    pub notification_engine: Option<Arc<NotificationEngineHandle>>,
    /// Repository des appels LLM — agrégation coûts/tokens.
    ///
    /// `Some` quand un `LlmRouter` est configuré et que `llm_calls.db` est ouvert.
    pub llm_call_repository: Option<Arc<std::sync::Mutex<apollia_llm::LlmCallRepository>>>,
    /// Handle to the [`ChatSessionManager`] actor.
    ///
    /// `Some` after the Supervisor startup sequence.
    /// `None` when the chat subsystem failed to start.
    pub chat_manager: Option<crate::chat::ChatSessionManagerHandle>,
    /// ORIA plan cache repository.
    ///
    /// `Some` when `plan_cache.db` opened successfully.
    pub plan_cache: Option<Arc<std::sync::Mutex<apollia_oria::plan_cache::PlanCacheRepository>>>,
    /// Handle to the agent-to-agent mailbox actor.
    ///
    /// `Some` after the mailbox is spawned during startup.
    pub mailbox_handle: Option<crate::mailbox::AgentMailboxHandle>,
    /// Repository for global user memory (preferences, habits, context).
    ///
    /// `Some` when `user_memory.db` opened successfully on startup.
    /// `None` when the open failed (warning logged, user memory disabled).
    pub user_memory:
        Option<Arc<std::sync::Mutex<apollia_memory::user_memory::UserMemoryRepository>>>,
    /// Handle to the SttEngine actor.
    ///
    /// `Some` when `stt.enabled = true` and the model loaded successfully.
    /// `None` when STT is disabled, the model is absent, or loading failed.
    pub stt_engine: Option<crate::stt::SttEngineHandle>,
    /// STT transcription repository for API routes.
    ///
    /// Separate connection for read operations. `None` when STT is disabled.
    pub stt_repository: Option<Arc<std::sync::Mutex<apollia_stt::SttRepository>>>,
    /// Port TCP de l'APIServer.
    pub api_port: u16,
}

/// Erreur retournée par [`init_embedded()`].
#[derive(Debug, thiserror::Error)]
pub enum EmbeddedError {
    /// Le Supervisor a échoué au démarrage.
    #[error("supervisor startup failed: {0}")]
    SupervisorFailed(#[from] SupervisorError),
    /// Timeout en attendant `AllReady`.
    #[error("runtime did not become ready within {0}s")]
    StartupTimeout(u64),
    /// Le thread runtime a paniqué.
    #[error("runtime thread panicked")]
    RuntimeThreadPanicked,
}

/// Configuration pour [`init_embedded()`].
///
/// Permet de surcharger le port TCP, le chemin du socket Unix, et les
/// timeouts. Les valeurs par défaut correspondent au comportement standard
/// de `apollia-os start`.
pub struct EmbeddedConfig {
    /// Port TCP pour l'APIServer (défaut : 7771).
    pub tcp_port: u16,
    /// Chemin du socket Unix (défaut : `/tmp/apollia.sock`).
    pub socket_path: PathBuf,
    /// Répertoire de données du runtime (défaut : `~/.apollia/`).
    pub data_dir: PathBuf,
    /// Timeout de démarrage du Supervisor en secondes (défaut : 30).
    pub startup_timeout_secs: u64,
    /// Configuration d'observabilité.
    pub obs_config: ObservabilityConfig,
    /// Agent loader pour charger les agents Python.
    pub agent_loader: Arc<dyn AgentLoader>,
    /// Backend factory pour créer les backends d'exécution par agent.
    pub backend_factory: Option<Arc<dyn AgentBackendFactory>>,
    /// Configuration LLM optionnelle parsée depuis `apollia.toml`.
    pub llm_config: Option<apollia_llm::LlmConfig>,
    /// Chemin du fichier `apollia.toml` chargé — requis pour le hot reload des triggers.
    pub config_path: Option<PathBuf>,
    /// Repository des agents installés — requis pour l'auto-load au boot.
    pub agent_repository: Option<AgentRepository>,
    /// Répertoire des agents bundled pour l'auto-install au premier boot.
    ///
    /// Si `None` ou si `manifest.json` est absent, l'auto-install est ignoré.
    pub bundled_agents_path: Option<PathBuf>,
    /// Chat Agent runner — enables Chat Agent mode in the ChatSessionManager.
    /// When `None`, Agent mode sessions will fail at message time.
    pub chat_agent_runner: Option<Arc<dyn crate::chat::ChatAgentRunner>>,

    /// Configuration du runtime core (EventBus, mailbox).
    ///
    /// Correspond à la section `[runtime]` dans `apollia.toml`.
    /// Peuplé par [`EmbeddedConfig::apply_toml`].
    pub runtime_config: RuntimeConfig,

    /// Configuration Human-in-the-Loop (timeout HITL, scan interval).
    ///
    /// Correspond à la section `[hitl]` dans `apollia.toml`.
    /// Peuplé par [`EmbeddedConfig::apply_toml`].
    pub hitl_config: HitlConfig,

    /// Configuration A2A (chain timeout).
    ///
    /// Correspond à la section `[a2a]` dans `apollia.toml`.
    /// Peuplé par [`EmbeddedConfig::apply_toml`].
    pub a2a_config: A2AConfig,

    /// Configuration du moteur de pipelines (timeout step).
    ///
    /// Correspond à la section `[pipelines]` dans `apollia.toml`.
    /// Peuplé par [`EmbeddedConfig::apply_toml`].
    pub pipelines_config: apollia_core::PipelinesConfig,
}

impl Default for EmbeddedConfig {
    fn default() -> Self {
        let home = std::env::var("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| std::env::temp_dir());
        Self {
            tcp_port: 7771,
            socket_path: PathBuf::from("/tmp/apollia.sock"),
            data_dir: home.join(".apollia"),
            startup_timeout_secs: DEFAULT_STARTUP_TIMEOUT_SECS,
            obs_config: ObservabilityConfig::default(),
            agent_loader: Arc::new(StubAgentLoader),
            backend_factory: None,
            llm_config: None,
            config_path: None,
            agent_repository: None,
            bundled_agents_path: None,
            chat_agent_runner: None,
            runtime_config: RuntimeConfig::default(),
            hitl_config: HitlConfig::default(),
            a2a_config: A2AConfig::default(),
            pipelines_config: apollia_core::PipelinesConfig::default(),
        }
    }
}

impl EmbeddedConfig {
    /// Applique toutes les sections parsables de `apollia.toml` à cette config.
    ///
    /// Parse `[llm]`, `[runtime]`, `[hitl]`, `[a2a]`, et `[api]` en une seule passe.
    /// Les erreurs de parsing sont ignorées silencieusement — le runtime démarre
    /// avec les valeurs par défaut si une section est absente ou invalide.
    pub fn apply_toml(mut self, content: &str) -> Self {
        #[derive(serde::Deserialize)]
        struct TomlSections {
            llm: Option<apollia_llm::LlmConfig>,
            runtime: Option<RuntimeConfig>,
            hitl: Option<HitlConfig>,
            a2a: Option<A2AConfig>,
            api: Option<apollia_core::ApiConfig>,
            pipelines: Option<apollia_core::PipelinesConfig>,
        }
        if let Ok(s) = toml::from_str::<TomlSections>(content) {
            self.llm_config = s.llm;
            if let Some(rc) = s.runtime {
                self.runtime_config = rc;
            }
            if let Some(hc) = s.hitl {
                self.hitl_config = hc;
            }
            if let Some(ac) = s.a2a {
                self.a2a_config = ac;
            }
            if let Some(api) = s.api {
                // Update socket_path from [api].unix_socket when explicitly configured.
                self.socket_path = api.unix_socket;
            }
            if let Some(pc) = s.pipelines {
                self.pipelines_config = pc;
            }
        }

        // triggers and notifications are now loaded from SQLite by the Supervisor.
        // TOML sections for these are ignored.

        self
    }
}

/// Démarre le runtime Apollia en mode embarqué.
///
/// Crée un thread dédié avec un Tokio runtime, démarre le Supervisor,
/// attend l'événement `AllReady`, puis retourne un [`RuntimeHandle`].
///
/// Le socket Unix reste actif pour permettre l'utilisation simultanée de la CLI.
///
/// # Errors
///
/// - [`EmbeddedError::SupervisorFailed`] si le Supervisor échoue au démarrage.
/// - [`EmbeddedError::StartupTimeout`] si `AllReady` n'est pas reçu dans le délai.
/// - [`EmbeddedError::RuntimeThreadPanicked`] si le thread runtime panique.
pub fn init_embedded(config: EmbeddedConfig) -> Result<RuntimeHandle, EmbeddedError> {
    let (result_tx, result_rx) = std::sync::mpsc::channel::<Result<RuntimeHandle, EmbeddedError>>();
    let startup_timeout_secs = config.startup_timeout_secs;

    std::thread::Builder::new()
        .name("apollia-runtime".to_string())
        .spawn(move || {
            let rt = match tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .thread_name("apollia-worker")
                .build()
            {
                Ok(rt) => rt,
                Err(e) => {
                    let _ = result_tx.send(Err(EmbeddedError::SupervisorFailed(
                        SupervisorError::ActorStartFailed {
                            actor: "tokio-runtime".to_string(),
                            reason: e.to_string(),
                        },
                    )));
                    return;
                }
            };

            rt.block_on(async move {
                let result = start_supervisor_and_wait(config).await;
                let _ = result_tx.send(result);

                // Keep the Tokio runtime alive so actors continue running.
                // The thread parks here indefinitely — shutdown is driven by
                // the ShutdownController via the EventBus.
                std::future::pending::<()>().await;
            });
        })
        .map_err(|_| EmbeddedError::RuntimeThreadPanicked)?;

    let timeout = Duration::from_secs(startup_timeout_secs);
    match result_rx.recv_timeout(timeout) {
        Ok(result) => result,
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
            Err(EmbeddedError::StartupTimeout(startup_timeout_secs))
        }
        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
            Err(EmbeddedError::RuntimeThreadPanicked)
        }
    }
}

/// Internal: starts the Supervisor and waits for `AllReady`.
async fn start_supervisor_and_wait(config: EmbeddedConfig) -> Result<RuntimeHandle, EmbeddedError> {
    let tcp_port = config.tcp_port;

    let supervisor_config = SupervisorConfig {
        api_config: APIServerConfig {
            socket_path: config.socket_path,
            bind_addr: "127.0.0.1".to_owned(),
            tcp_port,
            api_token: None,
        },
        startup_timeout_secs: config.startup_timeout_secs,
        llm_config: config.llm_config,
        config_path: config.config_path,
        runtime_config: config.runtime_config,
        hitl_config: config.hitl_config,
        data_dir: config.data_dir,
        obs_config: config.obs_config,
        agent_repository: config.agent_repository,
        bundled_agents_path: config.bundled_agents_path,
        pipelines_config: config.pipelines_config,
    };

    let supervisor = Supervisor::new(supervisor_config);

    let noop = DynBackend::new(NoopBackend);
    let handles = supervisor
        .start(
            noop,
            config.agent_loader,
            config.backend_factory,
            config.chat_agent_runner,
        )
        .await?;

    Ok(RuntimeHandle {
        event_sender: handles.event_sender,
        registry_handle: handles.registry_handle,
        tool_registry_handle: handles.tool_registry_handle,
        router_handle: handles.router_handle,
        api_handle: Arc::new(handles.api_handle),
        llm_router: handles.llm_router,
        trigger_engine: handles.trigger_engine,
        pipeline_engine: handles.pipeline_engine,
        audit_trail: handles.audit_trail,
        task_repository: handles.task_repository,
        pending_approvals: handles.pending_approvals,
        notification_engine: handles.notification_engine.map(Arc::new),
        llm_call_repository: handles.llm_call_repository,
        chat_manager: handles.chat_manager,
        plan_cache: handles.plan_cache,
        mailbox_handle: handles.mailbox_handle,
        user_memory: handles.user_memory,
        stt_engine: handles.stt_engine,
        stt_repository: handles.stt_repository,
        api_port: tcp_port,
    })
}

/// Fallback backend — returns a `Failed` result immediately.
///
/// Used as the default execution backend when no Python agent is configured.
#[derive(Clone)]
struct NoopBackend;

impl ExecutionBackend for NoopBackend {
    fn execute(
        &self,
        task: apollia_core::AIPTask,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<apollia_core::AIPResult, String>> + Send>,
    > {
        Box::pin(async move {
            Ok(apollia_core::AIPResult {
                task_id: task.task_id,
                status: apollia_core::TaskStatus::Failed,
                output: Vec::new(),
                error: Some(apollia_core::AIPError {
                    code: "NO_BACKEND".to_string(),
                    message: "no execution backend configured for this agent".to_string(),
                    details: None,
                }),
                artifacts: Vec::new(),
                input_required_data: None,
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_embedded_config_default_values() {
        // GIVEN the default EmbeddedConfig
        let config = EmbeddedConfig::default();

        // THEN reasonable defaults are set
        assert_eq!(config.tcp_port, 7771);
        assert_eq!(config.socket_path, PathBuf::from("/tmp/apollia.sock"));
        assert_eq!(config.startup_timeout_secs, DEFAULT_STARTUP_TIMEOUT_SECS);
    }

    #[test]
    fn test_embedded_error_display() {
        // GIVEN various EmbeddedError variants
        let timeout_err = EmbeddedError::StartupTimeout(30);
        let panic_err = EmbeddedError::RuntimeThreadPanicked;

        // THEN display messages are informative
        assert!(timeout_err.to_string().contains("30s"));
        assert!(panic_err.to_string().contains("panicked"));
    }

    #[test]
    fn test_runtime_handle_is_clone() {
        // Compile-time check: RuntimeHandle must be Clone.
        fn assert_clone<T: Clone>() {}
        assert_clone::<RuntimeHandle>();
    }

    #[test]
    fn test_embedded_error_from_supervisor_error() {
        // GIVEN a SupervisorError
        let sup_err = SupervisorError::ConfigError("test".to_string());

        // WHEN converted to EmbeddedError
        let embedded_err: EmbeddedError = sup_err.into();

        // THEN it wraps correctly
        assert!(embedded_err.to_string().contains("test"));
    }
}
