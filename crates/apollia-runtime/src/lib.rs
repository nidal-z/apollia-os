//! Apollia OS — Runtime Core.
//!
//! Responsible for the orchestration layer of the runtime:
//! - `EventBus` — broadcast channel for all runtime events.
//! - `AgentRegistry` — Tokio actor tracking `ProcessState` per agent.
//! - `TaskRouter` — Tokio actor dispatching tasks to available agents.
//! - `ExecutionCoordinator` — per-agent lifecycle coordinator.
//! - `APIServer` — axum HTTP server on Unix socket + TCP 7771.
//! - `Supervisor` — ordered startup + watchdog.

pub mod a2a;
pub mod agents;
pub mod api;
pub mod chat;
pub mod commands;
pub mod coordinator;
pub mod embedded;
pub mod eventbus;
pub mod mailbox;
pub mod registry;
pub mod router;
pub mod session;
pub mod shutdown;
pub mod stt;
pub mod supervisor;
pub mod timeout_watcher;
pub mod workspace;

pub use api::{APIServer, APIServerConfig, APIServerError, APIServerHandle, AppState};
pub use coordinator::{CoordinatorError, ExecutionBackend, ExecutionCoordinator};
pub use eventbus::{EventBus, EventBusReceiver, EventBusSender};
pub use registry::{
    AgentEntry, AgentRegistry, AgentRegistryError, AgentRegistryHandle,
};
pub use router::{SubmitError, TaskRouterHandle};
pub use shutdown::{wait_for_shutdown_signal, ShutdownConfig, ShutdownController, ShutdownError};
pub use supervisor::{
    ChildSpec, RestartPolicy, RestartTracker, Supervisor, SupervisorConfig, SupervisorError,
    SupervisorHandles,
};
pub use timeout_watcher::{TimeoutWatcher, TimeoutWatcherConfig, TimeoutWatcherError};

// Embedded runtime.
pub use embedded::{init_embedded, EmbeddedConfig, EmbeddedError, RuntimeHandle};

// Re-export from apollia-tools for convenience
pub use apollia_tools::ToolRegistryHandle;

// Agent-to-agent messaging
pub use mailbox::{AgentMailboxHandle, AgentMessage, MailboxError};

// A2A routing
pub use a2a::{
    make_delegate_fn, resolve_skill, A2AAgentCard, A2AError, A2AInvocationResult, A2AInvoker,
    A2ASkillInfo, A2AToolsProvider, A2aDelegateFn, A2aDelegateResult, A2aError, A2aErrorResponse,
    SkillListing,
};

// Chat subsystem
pub use chat::ChatSessionManagerHandle;

// Session config
pub use session::SessionConfig;

// STT engine
pub use stt::{SttEngineError, SttEngineHandle, SttStatus, TranscriptSource};

// Custom slash commands
pub use commands::{CommandRegistry, CustomCommand};

// Community agent registry
pub use agents::registry_remote::{
    check_git_available, find_agent_file, git_clone, install_agent, install_from_dir,
    parse_install_source, read_manifest_file, update_registry, AgentInstallSource, RegistryEntry,
    RemoteInstallError, TempInstallDir,
};
