//! Apollia OS — Runtime Core.
//!
//! Responsible for the orchestration layer of the runtime:
//! - `EventBus` — broadcast channel for all runtime events (STORY-006)
//! - `AgentRegistry` — Tokio actor tracking `ProcessState` per agent (STORY-007)
//! - `TaskRouter` — Tokio actor dispatching tasks to available agents (STORY-032)
//! - `ExecutionCoordinator` — per-agent lifecycle coordinator (STORY-031)
//! - `APIServer` — axum HTTP server on Unix socket + TCP 7771 (STORY-033)
//! - `Supervisor` — ordered startup + watchdog (STORY-039)

pub mod api;
pub mod chat;
pub mod coordinator;
pub mod embedded;
pub mod eventbus;
pub mod registry;
pub mod router;
pub mod shutdown;
pub mod supervisor;
pub mod timeout_watcher;

pub use api::{APIServer, APIServerConfig, APIServerError, APIServerHandle, AppState};
pub use coordinator::{CoordinatorError, ExecutionBackend, ExecutionCoordinator};
pub use eventbus::{EventBus, EventBusReceiver, EventBusSender};
pub use registry::{AgentEntry, AgentRegistry, AgentRegistryError, AgentRegistryHandle};
pub use router::{SubmitError, TaskRouterHandle};
pub use shutdown::{wait_for_shutdown_signal, ShutdownConfig, ShutdownController, ShutdownError};
pub use supervisor::{
    ChildSpec, RestartPolicy, RestartTracker, Supervisor, SupervisorConfig, SupervisorError,
    SupervisorHandles,
};
pub use timeout_watcher::{TimeoutWatcher, TimeoutWatcherConfig, TimeoutWatcherError};

// Embedded runtime (STORY-135)
pub use embedded::{init_embedded, EmbeddedConfig, EmbeddedError, RuntimeHandle};

// Re-export from apollia-tools for convenience
pub use apollia_tools::ToolRegistryHandle;

// Chat subsystem (Sprint 18 — STORY-199)
pub use chat::ChatSessionManagerHandle;
