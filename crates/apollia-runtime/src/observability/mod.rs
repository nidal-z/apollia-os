//! Event-sourced observability — `runtime_events` log (ADR-088).
//!
//! Cette brique transforme le bus broadcast `RuntimeEvent` en source de
//! vérité persistante de la trajectoire d'exécution d'un agent. Là où la
//! `Timeline API` agrégeait 5 sources SQLite hétérogènes, le persistor ici
//! écrit chaque événement significatif dans une table append-only unique
//! (`runtime_events.db`), indexée par `task_id`/`parent_event_id`/
//! `correlation_id`, qui devient la base de la nouvelle vue conversation
//! `ExecutionTrace` côté UI.
//!
//! Les variants existants (`AuditTrail`, `LlmCallRepository`, etc.) sont
//! conservés intacts pour leur usage métier (audit immuable, agrégations
//! coûts) — la duplication est volontaire et bornée à un seul nouveau log.
//!
//! Voir le plan complet dans `docs/adr/ADR-088-event-sourced-observability.md`.

pub mod persistor;
pub mod repository;
pub mod resilience_subscriber;

pub use persistor::{spawn_runtime_events_subscriber, EventPersistorError, EventPersistorHandle};
pub use repository::{RuntimeEventRecord, RuntimeEventsRepository};
pub use resilience_subscriber::spawn_resilience_subscriber;
