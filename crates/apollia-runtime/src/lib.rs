//! Apollia OS — Runtime Core.
//!
//! Responsible for the orchestration layer of the runtime:
//! - `EventBus` — broadcast channel for all runtime events (STORY-006)
//! - `AgentRegistry` — Tokio actor tracking `ProcessState` per agent (STORY-007)
//! - `TaskRouter` — Tokio actor dispatching tasks to available agents (STORY-032)
//! - `ExecutionCoordinator` — per-agent lifecycle coordinator (STORY-031)
//! - `APIServer` — axum HTTP server on Unix socket + TCP 7771 (STORY-033)
//! - `Supervisor` — ordered startup + watchdog (STORY-039)

pub mod coordinator;
pub mod eventbus;
pub mod registry;
pub mod router;

pub use coordinator::{CoordinatorError, ExecutionBackend, ExecutionCoordinator};
pub use eventbus::{EventBus, EventBusReceiver, EventBusSender};
pub use registry::{AgentEntry, AgentRegistry, AgentRegistryError, AgentRegistryHandle};
pub use router::{SubmitError, TaskRouterHandle};
