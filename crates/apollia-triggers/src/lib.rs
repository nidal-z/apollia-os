//! `apollia-triggers` — Types fondamentaux et moteur de déclenchement pour Apollia OS.
//!
//! Ce crate fournit :
//! - Les types de base ([`types`]) : `TriggerDefinition`, `TriggerEvent`, `InputTemplate`, etc.
//! - L'acteur central ([`engine`]) : `TriggerEngine` + `TriggerEngineHandle` + `TaskSubmitter`.
//! - Les sources ([`sources`]) : `CronTrigger`, `IntervalTrigger`, `OneshotTrigger` + stubs.
//! - La persistance ([`persistence`]) : `TriggerPersistence`, `trigger_history`, `trigger_state`.
//! - Le repository de définitions ([`definition_repository`]) : CRUD SQLite pour les définitions.
//! - La validation métier ([`validation`]) : règles de validation des définitions de triggers.
//! - La configuration du file watcher ([`config`]) : `FileWatchConfig` + patterns d'exclusion par défaut.

pub mod config;
pub mod definition_repository;
pub mod engine;
pub mod persistence;
pub mod sources;
pub mod toml_config;
pub mod types;
pub mod validation;

pub use config::FileWatchConfig;
pub use definition_repository::{
    OnBusy, TriggerDefinitionError as DefinitionRepositoryError, TriggerDefinitionRepository,
    TriggerDefinitionRow,
};
pub use engine::{TaskSubmitter, TriggerEngineError, TriggerEngineHandle, TriggerStatus};
pub use persistence::{
    TriggerHistoryEntry, TriggerPersistence, TriggerPersistenceError, TriggerStateRow,
};
pub use toml_config::{parse_triggers_from_toml_str, TriggerTomlError};
pub use types::{
    parse_interval, FileEventKind, InputTemplate, OnBusyPolicy, TriggerDefinition,
    TriggerDefinitionError, TriggerEvent, TriggerId, TriggerPayload, TriggerSourceConfig,
};
pub use validation::{validate_trigger, validate_trigger_source};
