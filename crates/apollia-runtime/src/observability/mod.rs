//! Event-sourced observability, `runtime_events` log.
//!
//! This component turns the `RuntimeEvent` broadcast bus into a persistent
//! source of truth for an agent's execution trajectory. Where the
//! `Timeline API` aggregated 5 heterogeneous SQLite sources, the persistor here
//! writes each significant event into a single append-only table
//! (`runtime_events.db`), indexed by `task_id`/`parent_event_id`/
//! `correlation_id`, which becomes the basis for the new `ExecutionTrace`
//! conversation view on the UI.
//!
//! The existing variants (`AuditTrail`, `LlmCallRepository`, etc.) are kept
//! intact for their business use (immutable audit, cost aggregations); the
//! duplication is deliberate and limited to this one new log.

pub mod persistor;
pub mod repository;
pub mod resilience_subscriber;

pub use persistor::{spawn_runtime_events_subscriber, EventPersistorError, EventPersistorHandle};
pub use repository::{RuntimeEventRecord, RuntimeEventsRepository};
pub use resilience_subscriber::spawn_resilience_subscriber;
