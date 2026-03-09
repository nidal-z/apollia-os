//! `apollia-triggers` — Types fondamentaux et moteur de déclenchement pour Apollia OS.
//!
//! Ce crate fournit :
//! - Les types de base ([`types`]) : `TriggerDefinition`, `TriggerEvent`, `InputTemplate`, etc.
//! - L'acteur central ([`engine`]) : `TriggerEngine` + `TriggerEngineHandle` + `TaskSubmitter`.
//! - Les sources ([`sources`]) : `CronTrigger`, `IntervalTrigger`, `OneshotTrigger` + stubs.
//! - La persistance ([`persistence`]) : `TriggerPersistence`, `trigger_history`, `trigger_state`.

pub mod engine;
pub mod persistence;
pub mod sources;
pub mod types;

pub use engine::{TaskSubmitter, TriggerEngineError, TriggerEngineHandle, TriggerStatus};
pub use persistence::{
    TriggerHistoryEntry, TriggerPersistence, TriggerPersistenceError, TriggerStateRow,
};
pub use types::{
    parse_interval, FileEventKind, InputTemplate, OnBusyPolicy, TriggerDefinition,
    TriggerDefinitionError, TriggerEvent, TriggerId, TriggerPayload, TriggerSourceConfig,
};
