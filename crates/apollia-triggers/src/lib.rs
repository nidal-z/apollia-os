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

/// Profondeur par défaut de la file d'attente bornée FIFO par agent.
///
/// Utilisée quand `OnBusyPolicy::Queue` est construit sans valeur explicite —
/// par exemple lors du parsing TOML ou de la lecture depuis SQLite.
/// Surchargeable via `apollia.toml` : `[triggers] queue_max_depth = N`.
pub const DEFAULT_QUEUE_MAX_DEPTH: usize = 10;

pub mod config;
pub mod definition_repository;
pub mod engine;
pub mod handlers;
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
    TriggerHistoryEntry, TriggerPersistence, TriggerPersistenceError, TriggerStateRow, TriggerStats,
};
pub use toml_config::{parse_triggers_from_toml_str, TriggerTomlError};
pub use types::{
    parse_interval, FileEventKind, InputTemplate, OnBusyPolicy, TriggerDefinition,
    TriggerDefinitionError, TriggerEvent, TriggerId, TriggerPayload, TriggerSourceConfig,
};
pub use validation::{validate_trigger, validate_trigger_source};
