//! `apollia-triggers` — Types fondamentaux et moteur de déclenchement pour Apollia OS.
//!
//! Ce crate fournit les types de base utilisés par le `TriggerEngine` et toutes les
//! sources de déclenchement (cron, interval, file watch, webhook). Aucune logique
//! d'exécution n'est présente ici — uniquement les types, les erreurs, et
//! [`types::InputTemplate::render`].

pub mod types;

pub use types::{
    parse_interval, FileEventKind, InputTemplate, OnBusyPolicy, TriggerDefinition,
    TriggerDefinitionError, TriggerEvent, TriggerId, TriggerPayload, TriggerSourceConfig,
};
