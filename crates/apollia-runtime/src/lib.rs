#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]
//! Apollia OS, Runtime Core.
//!
//! Responsible for the orchestration layer of the runtime:
//! - `EventBus`, broadcast channel for all runtime events.
//! - `AgentRegistry`, Tokio actor tracking `ProcessState` per agent.
//! - `TaskRouter`, Tokio actor dispatching tasks to available agents.
//! - `ExecutionCoordinator`, per-agent lifecycle coordinator.
//! - `APIServer`, axum HTTP server on Unix socket + TCP 7771.
//! - `Supervisor`, ordered startup + watchdog.

pub mod a2a;
pub mod agents;
pub mod analyzers;
pub mod api;
pub mod audit_journal;
pub mod chat;
pub mod commands;
pub mod connectors_bridge;
pub mod coordinator;
pub mod embedded;
pub mod eventbus;
pub mod hooks;
pub mod llama_server;
pub mod llama_server_backend;
pub mod llm_timings;
pub mod mailbox;
pub mod observability;
pub mod perf_trace;
pub mod plan_approval;
pub mod projects;
pub mod registry;
pub mod replay;
pub mod router;
pub mod runner_supervisor;
pub mod session;
pub mod session_metrics;
pub mod session_replay;
pub mod shutdown;
pub mod stt;
pub mod supervisor;
pub mod timeout_watcher;

#[cfg(test)]
mod test_support;

pub use api::{APIServer, APIServerConfig, APIServerError, APIServerHandle, AppState};
pub use coordinator::{CoordinatorError, ExecutionBackend, ExecutionCoordinator};
pub use eventbus::{EventBus, EventBusReceiver, EventBusSender};
pub use hooks::{HookHandlerSummary, HookRegistry};
pub use plan_approval::{ApprovalError, PlanApprovalHandle};
pub use registry::{AgentEntry, AgentRegistry, AgentRegistryError, AgentRegistryHandle};
pub use router::{SubmitError, TaskRouterHandle};
pub use shutdown::{wait_for_shutdown_signal, ShutdownConfig, ShutdownController, ShutdownError};
pub use supervisor::{Supervisor, SupervisorConfig, SupervisorError, SupervisorHandles};
pub use timeout_watcher::{TimeoutWatcher, TimeoutWatcherConfig, TimeoutWatcherError};

pub use embedded::{init_embedded, worker_runtime, EmbeddedConfig, EmbeddedError, RuntimeHandle};

// Re-export from apollia-tools for convenience
pub use apollia_tools::ToolRegistryHandle;

// Agent-to-agent messaging
pub use mailbox::{AgentMailboxHandle, AgentMessage, MailboxError};

// A2A routing
pub use a2a::{
    make_delegate_fn, resolve_skill, A2AAgentCard, A2AError, A2AInvocationResult, A2AInvokeRequest,
    A2AInvoker, A2ASkillInfo, A2AToolsProvider, A2aDelegateFn, A2aDelegateResult, A2aError,
    A2aErrorResponse, SkillListing,
};

// Chat subsystem
pub use chat::ChatSessionManagerHandle;

pub use session::SessionConfig;
pub use session_metrics::{SessionMetricsActor, SessionMetricsStore};

pub use stt::{SttEngineError, SttEngineHandle, SttStatus, TranscriptSource};

// Custom slash commands
pub use commands::{CommandRegistry, CustomCommand};

// Community agent registry
pub use agents::registry_remote::{
    check_git_available, find_agent_file, git_clone, install_agent, install_from_dir,
    parse_install_source, read_manifest_file, update_registry, AgentInstallSource, RegistryEntry,
    RemoteInstallError, TempInstallDir,
};
