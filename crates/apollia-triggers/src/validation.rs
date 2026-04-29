//! Validation métier des définitions de triggers.
//!
//! Ce module centralise la validation exécutée avant chaque écriture
//! (insert/update) dans le [`crate::definition_repository::TriggerDefinitionRepository`].
//! Les règles de validation sont fail-fast (Principe #4 d'Apollia OS).

use std::str::FromStr;

use crate::definition_repository::{TriggerDefinitionError, TriggerDefinitionRow};

/// Types de sources reconnus par le système de triggers.
const VALID_SOURCE_TYPES: &[&str] = &["cron", "interval", "oneshot", "file_watch", "webhook"];

/// Longueur minimale du secret HMAC-SHA256 pour les webhooks.
const MIN_WEBHOOK_SECRET_LENGTH: usize = 32;

/// Valide une [`TriggerDefinitionRow`] avant insert ou update.
///
/// Vérifie :
/// - Agent défini (non-None et non-vide)
/// - Identifiant non vide
/// - `source_type` reconnu
/// - `source_config` valide pour le `source_type` donné
pub fn validate_trigger(def: &TriggerDefinitionRow) -> Result<(), TriggerDefinitionError> {
    match def.agent.as_deref() {
        None | Some("") => {
            return Err(TriggerDefinitionError::ValidationError(
                "agent must be set".to_string(),
            ));
        }
        Some(_) => {}
    }

    if def.id.is_empty() {
        return Err(TriggerDefinitionError::ValidationError(
            "trigger id cannot be empty".to_string(),
        ));
    }

    if !VALID_SOURCE_TYPES.contains(&def.source_type.as_str()) {
        return Err(TriggerDefinitionError::ValidationError(format!(
            "unknown source type: {}",
            def.source_type
        )));
    }

    validate_trigger_source(&def.source_type, &def.source_config)
}

/// Valide la `source_config` JSON selon le `source_type`.
///
/// Règles par type :
/// - `cron` : expression cron parsable (5 ou 6 champs, normalisation auto)
/// - `interval` : champ `every` au format `"30m"`, `"1h"`, etc.
/// - `oneshot` : champ `fire_at` ISO 8601
/// - `file_watch` : champ `path` non vide
/// - `webhook` : champ `secret` >= 32 caractères
pub fn validate_trigger_source(
    source_type: &str,
    config: &serde_json::Value,
) -> Result<(), TriggerDefinitionError> {
    match source_type {
        "cron" => validate_cron(config),
        "interval" => validate_interval(config),
        "oneshot" => validate_oneshot(config),
        "file_watch" => validate_file_watch(config),
        "webhook" => validate_webhook(config),
        unknown => Err(TriggerDefinitionError::ValidationError(format!(
            "unknown source type: {unknown}"
        ))),
    }
}

/// Valide une expression cron (5 ou 6 champs, normalisation 5→6 auto).
fn validate_cron(config: &serde_json::Value) -> Result<(), TriggerDefinitionError> {
    let schedule = config
        .get("schedule")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    if schedule.is_empty() {
        return Err(TriggerDefinitionError::ValidationError(
            "cron schedule is required".to_string(),
        ));
    }

    if cron::Schedule::from_str(schedule).is_ok() {
        return Ok(());
    }

    // Normalisation 5→6 champs (ajout du champ secondes)
    let field_count = schedule.split_whitespace().count();
    if field_count == 5 {
        let normalized = format!("0 {schedule}");
        if cron::Schedule::from_str(&normalized).is_ok() {
            return Ok(());
        }
    }

    let reason = cron::Schedule::from_str(schedule)
        .err()
        .map(|e| e.to_string())
        .unwrap_or_else(|| "invalid expression".to_string());

    Err(TriggerDefinitionError::ValidationError(format!(
        "invalid cron expression: {reason}"
    )))
}

/// Valide un intervalle périodique (champ `every`).
fn validate_interval(config: &serde_json::Value) -> Result<(), TriggerDefinitionError> {
    let every = config.get("every").and_then(|v| v.as_str()).unwrap_or("");

    if every.is_empty() {
        return Err(TriggerDefinitionError::ValidationError(
            "interval 'every' field is required".to_string(),
        ));
    }

    crate::types::parse_interval(every)
        .map_err(|e| TriggerDefinitionError::ValidationError(e.to_string()))?;

    Ok(())
}

/// Valide un déclenchement oneshot (champ `fire_at` ISO 8601).
fn validate_oneshot(config: &serde_json::Value) -> Result<(), TriggerDefinitionError> {
    let fire_at = config.get("fire_at").and_then(|v| v.as_str()).unwrap_or("");

    if fire_at.is_empty() {
        return Err(TriggerDefinitionError::ValidationError(
            "oneshot 'fire_at' timestamp is required".to_string(),
        ));
    }

    fire_at
        .parse::<chrono::DateTime<chrono::Utc>>()
        .map_err(|e| {
            TriggerDefinitionError::ValidationError(format!("invalid fire_at timestamp: {e}"))
        })?;

    Ok(())
}

/// Valide une source file_watch (champ `path` non vide).
fn validate_file_watch(config: &serde_json::Value) -> Result<(), TriggerDefinitionError> {
    let path = config.get("path").and_then(|v| v.as_str()).unwrap_or("");

    if path.is_empty() {
        return Err(TriggerDefinitionError::ValidationError(
            "file_watch path is required".to_string(),
        ));
    }

    Ok(())
}

/// Valide un webhook (champ `secret` >= 32 caractères).
fn validate_webhook(config: &serde_json::Value) -> Result<(), TriggerDefinitionError> {
    let secret = config.get("secret").and_then(|v| v.as_str()).unwrap_or("");

    if secret.len() < MIN_WEBHOOK_SECRET_LENGTH {
        return Err(TriggerDefinitionError::ValidationError(
            "webhook secret must be at least 32 characters".to_string(),
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::definition_repository::OnBusy;

    /// Crée une définition valide pour les tests de validation pure.
    fn make_valid_def() -> TriggerDefinitionRow {
        TriggerDefinitionRow {
            id: "test".to_string(),
            agent: Some("agent".to_string()),
            enabled: true,
            on_busy: OnBusy::Queue,
            source_type: "cron".to_string(),
            source_config: serde_json::json!({ "schedule": "0 0 8 * * MON *" }),
            input_template: None,
            created_at: String::new(),
            updated_at: String::new(),
        }
    }

    #[test]
    fn test_validate_valid_definition() {
        let def = make_valid_def();
        assert!(validate_trigger(&def).is_ok());
    }

    #[test]
    fn test_validate_empty_id() {
        let mut def = make_valid_def();
        def.id = String::new();
        let result = validate_trigger(&def);
        assert!(matches!(
            result,
            Err(TriggerDefinitionError::ValidationError(ref msg)) if msg.contains("id cannot be empty")
        ));
    }

    #[test]
    fn test_validate_unknown_source_type() {
        let mut def = make_valid_def();
        def.source_type = "unknown".to_string();
        let result = validate_trigger(&def);
        assert!(matches!(
            result,
            Err(TriggerDefinitionError::ValidationError(ref msg)) if msg.contains("unknown source type")
        ));
    }

    #[test]
    fn test_validate_cron_5_fields_normalized() {
        // 5-field cron "0 8 * * MON" → normalized to "0 0 8 * * MON" (6-field)
        let result =
            validate_trigger_source("cron", &serde_json::json!({ "schedule": "0 8 * * MON" }));
        assert!(
            result.is_ok(),
            "5-field cron should be auto-normalized: {result:?}"
        );
    }

    #[test]
    fn test_validate_interval_valid() {
        let result = validate_trigger_source("interval", &serde_json::json!({ "every": "30m" }));
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_interval_invalid() {
        let result = validate_trigger_source("interval", &serde_json::json!({ "every": "bad" }));
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_file_watch_empty_path() {
        let result = validate_trigger_source("file_watch", &serde_json::json!({ "path": "" }));
        assert!(matches!(
            result,
            Err(TriggerDefinitionError::ValidationError(ref msg)) if msg.contains("path is required")
        ));
    }

    #[test]
    fn test_validate_oneshot_invalid_timestamp() {
        let result =
            validate_trigger_source("oneshot", &serde_json::json!({ "fire_at": "not-a-date" }));
        assert!(matches!(
            result,
            Err(TriggerDefinitionError::ValidationError(ref msg)) if msg.contains("invalid fire_at")
        ));
    }
}
