//! `apollia-triggers`: core types and trigger engine for Apollia OS.
//!
//! This crate provides:
//! - Core types ([`types`]): `TriggerDefinition`, `TriggerEvent`, `InputTemplate`, etc.
//! - The central actor ([`engine`]): `TriggerEngine` + `TriggerEngineHandle` + `TaskSubmitter`.
//! - The sources ([`sources`]): `CronTrigger`, `IntervalTrigger`, `OneshotTrigger` + stubs.
//! - Persistence ([`persistence`]): `TriggerPersistence`, `trigger_history`, `trigger_state`.
//! - The definition repository ([`definition_repository`]): SQLite CRUD for definitions.
//! - Business validation ([`validation`]): validation rules for trigger definitions.
//! - File watcher configuration ([`config`]): `FileWatchConfig` + default exclusion patterns.

/// Default depth of the bounded FIFO queue per agent.
///
/// Used when `OnBusyPolicy::Queue` is built without an explicit value, for
/// example during TOML parsing or when reading from SQLite.
/// Overridable via `apollia.toml`: `[triggers] queue_max_depth = N`.
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
