//! Supervisor, ordered startup, shutdown rollback, and watchdog for runtime actors.
//!
//! The Supervisor starts all runtime actors in a strict sequence:
//! `EventBus → AgentRegistry → ToolRegistry (+ native tools) → TaskRouter → APIServer`.
//! Each actor must emit `RuntimeEvent::Ready` (or equivalent) before the next one starts.
//! If any actor fails to start within the configured timeout, all previously started
//! actors are stopped in reverse order.
//!
//! After startup, the model is fail-fast then degrade: `watch()` listens for a
//! `ShutdownRequested` event and coordinates shutdown. There is no
//! actor restart-on-crash. A crashed actor leaves the runtime running in a degraded
//! state until an explicit shutdown. The inference sidecar is the exception: its own
//! `RunnerSupervisor` restarts that process (see `runner_supervisor`).

use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;
use tracing::{info, warn};

use apollia_core::{
    HitlConfig, LlmBackendRepository, PendingApprovals, ProcessState, RuntimeConfig, RuntimeEvent,
    SttConfigRepository, SttConfigRow,
};
use apollia_llm::{LlmCallRepository, LlmConfig, LlmRouter};
use apollia_mcp::{
    config::McpConfig, manager::McpClientManagerHandle, session::LoadingMode, McpServerRepository,
};
use apollia_notifications::{
    build_channels, NotificationConfig, NotificationConfigRepository, NotificationEngine,
    NotificationEngineHandle,
};
use apollia_tools::{AgentRepository, AuditTrailHandle, TaskRepository, ToolRegistryHandle};
use apollia_triggers::{TriggerDefinitionRepository, TriggerEngineHandle, TriggerPersistence};

use crate::api::routes_agents::AgentLoader;
use crate::api::{APIServer, APIServerConfig, APIServerError, APIServerHandle, AppState};
use crate::audit_journal::{AuditJournalHandle, AuditJournalSubscriber};
use crate::coordinator::{ExecutionBackend, ExecutionCoordinator};
use crate::eventbus::{EventBus, EventBusSender};
use crate::registry::{AgentRegistry, AgentRegistryHandle};
use crate::router::TaskRouterHandle;
use crate::timeout_watcher::{TimeoutWatcher, TimeoutWatcherConfig};

mod boot;
mod bootstrap;
mod bundled;
mod lifecycle;
mod persistence;

pub(crate) use persistence::resolve_home;

pub use bootstrap::watch;

#[cfg(test)]
mod tests;

pub struct SupervisorConfig {
    /// Configuration for the APIServer (TCP port + Unix socket path).
    pub api_config: APIServerConfig,
    /// Maximum time (in seconds) to wait for each actor to become ready.
    pub startup_timeout_secs: u64,
    /// Optional LLM configuration parsed from the `[llm]` section of `apollia.toml`.
    ///
    /// `None` disables the LLM layer entirely, the runtime starts normally and
    /// agents receive `ctx.llm = None`. No error is raised.
    pub llm_config: Option<LlmConfig>,
    /// Path to `apollia.toml`, injected into [`AppState`] for hot reload.
    ///
    /// `None` when the runtime starts without a config file (e.g. tests, `apollia-os start`
    /// without a config file). The `POST /api/v1/triggers/reload` route returns 503 when absent.
    pub config_path: Option<std::path::PathBuf>,
    /// Core runtime configuration (EventBus and mailbox capacities).
    ///
    /// Maps to the `[runtime]` section in `apollia.toml`.
    /// Default: [`RuntimeConfig::default()`].
    pub runtime_config: RuntimeConfig,

    /// Human-in-the-Loop configuration (timeout and scan interval).
    ///
    /// Maps to the `[hitl]` section in `apollia.toml`.
    /// Ignored when `AppState.task_repository` is `None`.
    /// Default: [`HitlConfig::default()`] (24 hours, 60-second scan).
    pub hitl_config: HitlConfig,
    /// Runtime data directory (e.g. `~/.apollia/`).
    ///
    /// Used to locate the SQLite databases (`triggers.db`, `notifications.db`,
    /// etc.). Must exist and be writable. Serves as `base_dir` when opening the
    /// repositories at boot.
    pub data_dir: std::path::PathBuf,
    /// Observability configuration (truncation limits, debug flags).
    ///
    /// Injected into `AppState`, `TriggerEngine`, and `LlmCallRepository`.
    /// Default: `ObservabilityConfig::default()` (32 KB max input/output).
    pub obs_config: apollia_core::ObservabilityConfig,
    /// Repository of installed agents.
    ///
    /// `Some` enables auto-load at boot: `enabled` agents are loaded via
    /// `AgentLoader`, validated, and registered in `AgentRegistry`.
    /// `None` disables auto-load (compatibility with existing tests).
    pub agent_repository: Option<AgentRepository>,
    /// Repository of installed packages.
    ///
    /// `Some` runs package integrity validation at boot.
    /// `None` disables that validation (backward compatibility).
    pub package_repository: Option<apollia_tools::PackageRepository>,
    /// Bundled agents directory (e.g. `agents/bundled/`).
    ///
    /// When `Some`, `auto_load_bundled_agents` runs at boot to register the
    /// agents declared in `manifest.json`. When `None`, or when `manifest.json`
    /// is absent, auto-install is silently skipped.
    pub bundled_agents_path: Option<std::path::PathBuf>,

    /// Native tools configuration (the `[tools]` section of `apollia.toml`).
    ///
    /// Propagated by `EmbeddedConfig::apply_toml` into `SupervisorConfig` then
    /// `RuntimeHandle`. Lets the agent runners (factory + chat) apply
    /// `web_search`, `web_read`, `http_allowlist`, and `disabled` when building
    /// the `NativeDispatcherConfig`. Default: [`apollia_core::ToolsConfig::default()`].
    pub tools_config: apollia_core::ToolsConfig,

    /// MCP tool loading strategy (the `[mcp] tool_loading` key of `apollia.toml`).
    ///
    /// Selects how every MCP session boots: [`LoadingMode::Eager`] loads and
    /// registers all schemas up front; [`LoadingMode::Deferred`] keeps only a
    /// lightweight index and exposes the synthetic `tool_search` tool instead.
    pub mcp_loading: LoadingMode,

    /// Maximum `limit` accepted by the synthetic `tool_search` tool (the
    /// `[mcp] tool_search_limit` key of `apollia.toml`). Default: 20.
    pub tool_search_limit: usize,

    /// Lifecycle hooks configuration (the `[hooks]` section of `apollia.toml`).
    ///
    /// Propagated by `EmbeddedConfig::apply_toml` into `SupervisorConfig`. Used
    /// at boot to build the [`crate::hooks::HookRegistry`] shared with the chat
    /// loop and exposed by the `GET /hooks` route.
    /// Default: [`apollia_core::HooksConfig::default()`] (no handlers).
    pub hooks_config: apollia_core::HooksConfig,

    /// Default plan-mode state inherited by every new chat session (the
    /// `[chat] plan_mode_default` key of `apollia.toml`).
    ///
    /// Read once at boot and applied at session creation by the
    /// [`crate::chat::ChatSessionManager`], so the runtime is the single source
    /// of truth for the default rather than any individual client. Default:
    /// `false`.
    pub plan_mode_default: bool,

    /// Default working directory for free-chat sessions (the
    /// `[chat] default_workspace` key of `apollia.toml`).
    ///
    /// When set to an existing directory, free-chat file tools anchor there and
    /// the agent is told its working directory. `None` falls back to
    /// `~/.apollia`. Read once at boot.
    pub chat_default_workspace: Option<String>,

    /// Temperature applied to a chat turn that advertises tools (the
    /// `[chat] tool_turn_temperature` key of `apollia.toml`).
    ///
    /// Lowering it whenever tools are exposed makes structured tool-call output
    /// more reliable on small local models. `None` resolves to the agent
    /// default. Read once at boot.
    pub chat_tool_turn_temperature: Option<f32>,
}

impl SupervisorConfig {
    /// Returns the bundled agents directory, or `None` when it is not configured.
    pub fn bundled_agents_dir(&self) -> Option<&std::path::Path> {
        self.bundled_agents_path.as_deref()
    }
}

/// Manifest of the bundled agents shipped with the runtime.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BundledManifest {
    /// Manifest format version (e.g. `"1.0.0"`).
    pub version: String,
    /// List of bundled agents.
    pub bundled_agents: Vec<BundledAgentEntry>,
}

/// Entry for a single agent in the bundled manifest.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BundledAgentEntry {
    /// Unique agent name (must match `manifest().name` in the Python file).
    pub name: String,
    /// Source filename relative to the bundled directory (e.g. `"excel-worker.py"`).
    pub file: String,
    /// When `true`, the agent is installed automatically on first boot.
    pub auto_install: bool,
    /// Short description of the agent.
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
    /// Always `Some` after successful startup, even when `config.triggers` is empty.
    /// Injected into `AppState` so webhook routes and CLI commands can reach it.
    pub trigger_engine: TriggerEngineHandle,
    /// Handle to the AuditTrail actor.
    ///
    /// `None` when the data directory is unavailable or the SQLite open fails
    /// (warning logged, runtime continues without audit). `Some` in production.
    pub audit_trail: Option<AuditTrailHandle>,
    /// HITL task repository, persists `input_required` prompts/contexts.
    ///
    /// Shared between `AppState` (resume handler) and `TimeoutWatcher`.
    /// `None` when the SQLite open fails (warning logged, HITL disabled).
    pub task_repository: Option<Arc<TaskRepository>>,
    /// HITL pending approvals registry, oneshot channels for Mode Direct suspension.
    ///
    /// `None` when `task_repository` is `None` (HITL disabled).
    pub pending_approvals: Option<Arc<apollia_core::PendingApprovals>>,
    /// Plan-gate registry, shared between the plan-decision route and the
    /// per-task ORIAEngine for plan-mode approval.
    pub plan_gates: Option<Arc<apollia_oria::PendingPlanGates>>,
    /// Handle to the NotificationEngine actor.
    ///
    /// `None` when no `[notifications]` section is present in `apollia.toml`.
    /// Used by [`ShutdownController`] to stop the engine before the EventBus closes,
    /// preventing late notifications from being delivered after `apollia-os stop`.
    pub notification_engine: Option<NotificationEngineHandle>,
    /// LLM call repository, aggregates cost and token usage.
    ///
    /// `Some` when an `LlmRouter` is configured and `llm_calls.db` is open.
    /// Shared between `AppState` (the REST costs route) and the EventBus subscriber.
    pub llm_call_repository: Option<Arc<std::sync::Mutex<LlmCallRepository>>>,
    /// Handle to the [`ChatSessionManager`] actor.
    ///
    /// `Some` after Phase 13 of the Supervisor startup sequence.
    /// `None` when the chat subsystem failed to start (warning logged).
    pub chat_manager: Option<crate::chat::ChatSessionManagerHandle>,
    /// ORIA plan cache repository, stores cached execution plans.
    ///
    /// `Some` when `plan_cache.db` opened successfully.
    /// `None` when the open failed (warning logged, caching disabled).
    pub plan_cache: Option<Arc<std::sync::Mutex<apollia_oria::plan_cache::PlanCacheRepository>>>,
    /// Handle to the agent-to-agent mailbox actor.
    ///
    /// Always `Some` after startup, the mailbox is lightweight and always spawned.
    pub mailbox_handle: Option<crate::mailbox::AgentMailboxHandle>,
    /// Repository for global user memory (preferences, habits, context).
    ///
    /// `Some` when `user_memory.db` opened successfully on startup.
    /// `None` when the open failed (warning logged, user memory disabled).
    pub user_memory:
        Option<std::sync::Arc<std::sync::Mutex<apollia_memory::user_memory::UserMemoryRepository>>>,
    /// Swappable handle to the SttEngine actor (Phase 15).
    ///
    /// Shares the same cell as [`AppState`](crate::api::AppState) so a
    /// mid-session reload is visible to both the axum routes and the embedded
    /// runtime handle. Holds `None` when STT is disabled, the model is absent,
    /// or loading failed.
    pub stt_engine: crate::api::server::SharedSttEngine,
    /// Swappable STT transcription repository for API routes.
    ///
    /// Separate connection from the engine's internal repository (SQLite WAL
    /// supports concurrent readers). Holds `None` when STT is disabled.
    pub stt_repository: crate::api::server::SharedSttRepository,
    /// Handle to the MCP client manager actor (Phase 3b).
    ///
    /// `Some` when `~/.apollia/mcp.toml` exists and at least one server connected.
    /// `None` when the config file is absent, empty, or all servers failed to start.
    pub mcp_handle: Option<McpClientManagerHandle>,
    /// Projects repository (SQLite).
    ///
    /// `Some` when `projects.db` opened successfully.
    /// `None` when the open failed (warning logged).
    pub project_repository: Option<std::sync::Arc<apollia_tools::ProjectRepository>>,
    /// Supervisor of the local sidecar runner.
    ///
    /// `Some` when a runner started successfully (GPU detected, binary present,
    /// handshake completed), `None` otherwise. Kept here to hold the runner
    /// process alive: the supervisor owns the child `Child` with
    /// `kill_on_drop(true)`. Propagated to `RuntimeHandle` on the embedded path.
    pub runner_supervisor: Option<Arc<crate::runner_supervisor::RunnerSupervisor>>,
    /// Managed embedded `llama-server` (local LLM engine), or `None` when the
    /// binary is absent. Whisper STT stays on `runner_supervisor`.
    pub llama_server_supervisor: Option<Arc<crate::llama_server::LlamaServerSupervisor>>,
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

/// The Apollia Supervisor, orchestrates actor lifecycle.
///
/// Created with [`Supervisor::new`], started with [`Supervisor::start`].
/// The Supervisor holds no business state, it only manages actor lifecycles.
pub struct Supervisor {
    config: SupervisorConfig,
}

/// Borrowed dependencies threaded through Phase 11 auto-load helpers.
///
/// The auto-load helpers borrow the config pieces they need through this ctx
/// rather than `&self`. Building `APIServer` partially moves `self.config`
/// (`api_config` is consumed by value), which would forbid a later `&self`
/// borrow; threading the needed config fields here keeps the helpers callable.
struct AutoLoadCtx<'a, B: ExecutionBackend> {
    agent_loader: &'a Arc<dyn AgentLoader>,
    backend_factory: &'a Option<Arc<dyn crate::api::routes_agents::AgentBackendFactory>>,
    base_backend: &'a B,
    registry_handle: &'a AgentRegistryHandle,
    router_handle: &'a TaskRouterHandle<B>,
    event_sender: &'a EventBusSender,
    task_repository: Option<&'a Arc<TaskRepository>>,
    /// Repository of installed agents (`None` disables auto-load).
    agent_repository: Option<&'a AgentRepository>,
    /// Data directory, used to locate per-agent venvs.
    data_dir: &'a std::path::Path,
    /// Observability config wired into each agent's coordinator.
    obs_config: &'a apollia_core::ObservabilityConfig,
}
