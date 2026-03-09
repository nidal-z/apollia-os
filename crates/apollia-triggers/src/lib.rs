//! `apollia-triggers` — Types fondamentaux et moteur de déclenchement pour Apollia OS.
//!
//! Ce crate fournit :
//! - Les types de base ([`types`]) : `TriggerDefinition`, `TriggerEvent`, `InputTemplate`, etc.
//! - L'acteur central ([`engine`]) : `TriggerEngine` + `TriggerEngineHandle` + `TaskSubmitter`.

pub mod engine;
pub mod types;

pub use engine::{TaskSubmitter, TriggerEngineError, TriggerEngineHandle, TriggerStatus};
pub use types::{
    parse_interval, FileEventKind, InputTemplate, OnBusyPolicy, TriggerDefinition,
    TriggerDefinitionError, TriggerEvent, TriggerId, TriggerPayload, TriggerSourceConfig,
};
