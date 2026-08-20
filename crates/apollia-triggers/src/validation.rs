//! Business validation of trigger definitions.
//!
//! This module centralizes the validation run before each write (insert/update)
//! in [`crate::definition_repository::TriggerDefinitionRepository`]. The
//! validation rules are fail-fast.

use std::str::FromStr;

use crate::definition_repository::{TriggerDefinitionError, TriggerDefinitionRow};

/// Source types recognized by the trigger system.
const VALID_SOURCE_TYPES: &[&str] = &["cron", "interval", "oneshot", "file_watch", "webhook"];

/// Minimum length of the HMAC-SHA256 secret for webhooks.
const MIN_WEBHOOK_SECRET_LENGTH: usize = 32;

/// Validates a [`TriggerDefinitionRow`] before insert or update.
///
/// Checks:
/// - Agent set (non-None and non-empty)
/// - Non-empty identifier
/// - Recognized `source_type`
/// - `source_config` valid for the given `source_type`
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

/// Validates the `source_config` JSON according to the `source_type`.
///
/// Rules per type:
/// - `cron`: parsable cron expression (5 or 6 fields, auto normalization)
/// - `interval`: `every` field in the format `"30m"`, `"1h"`, etc.
/// - `oneshot`: ISO 8601 `fire_at` field
/// - `file_watch`: non-empty `path` field
/// - `webhook`: `secret` field >= 32 characters
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

/// Validates a cron expression (5 or 6 fields, auto 5-to-6 normalization).
fn validate_cron(config: &serde_json::Value) -> Result<(), TriggerDefinitionError> {
    let schedule = config
        .get("schedule")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    accepted_cron_schedule(schedule).map(|_| ())
}

/// Returns the schedule in the form `cron::Schedule::from_str` accepts verbatim.
///
/// An already-parseable expression is returned unchanged; a 5-field expression
/// is normalized by prepending the seconds field. Rejecting instead of
/// returning is what [`validate_cron`] used to do with the normalized string,
/// and the runtime reader (`sources/cron.rs`) then failed on the stored
/// 5-field original: the validator accepted a value the reader refused.
///
/// This is the single 5-to-6 normalization of the tree. Both entry points run
/// it: the SQLite write path through [`normalized_source_config`], and the
/// `[[triggers]]` TOML parser through `toml_config::normalize_cron`. A second
/// copy would let the two doors accept different sets of expressions, and the
/// runtime reader only accepts the six-field form.
pub(crate) fn accepted_cron_schedule(schedule: &str) -> Result<String, TriggerDefinitionError> {
    if schedule.is_empty() {
        return Err(TriggerDefinitionError::ValidationError(
            "cron schedule is required".to_string(),
        ));
    }

    if cron::Schedule::from_str(schedule).is_ok() {
        return Ok(schedule.to_string());
    }

    // Normalize 5 to 6 fields (prepend the seconds field).
    let field_count = schedule.split_whitespace().count();
    if field_count == 5 {
        let normalized = format!("0 {schedule}");
        if cron::Schedule::from_str(&normalized).is_ok() {
            return Ok(normalized);
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

/// Returns `source_config` in the form the runtime readers accept verbatim.
///
/// The SQLite write path persists this value: a cron `schedule` is stored in
/// the form `cron::Schedule::from_str` accepts, so the reader in
/// `sources/cron.rs` never rejects what the validator accepted. Non-cron
/// sources are returned unchanged.
///
/// The SQLite read path runs it too, in
/// `definition_repository::row_to_definition`, because no migration rewrites
/// the rows a build older than that write path persisted. Those rows still
/// hold a 5-field expression, and the trigger they describe stays listed and
/// never fires until the read repairs it.
pub(crate) fn normalized_source_config(
    source_type: &str,
    config: &serde_json::Value,
) -> Result<serde_json::Value, TriggerDefinitionError> {
    if source_type != "cron" {
        return Ok(config.clone());
    }

    let schedule = config
        .get("schedule")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let accepted = accepted_cron_schedule(schedule)?;

    let mut normalized = config.clone();
    if let Some(obj) = normalized.as_object_mut() {
        obj.insert("schedule".to_string(), serde_json::Value::String(accepted));
    }
    Ok(normalized)
}

/// Validates a periodic interval (`every` field).
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

/// Validates a oneshot trigger (ISO 8601 `fire_at` field).
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

/// Validates a file_watch source (non-empty `path` field).
fn validate_file_watch(config: &serde_json::Value) -> Result<(), TriggerDefinitionError> {
    let path = config.get("path").and_then(|v| v.as_str()).unwrap_or("");

    if path.is_empty() {
        return Err(TriggerDefinitionError::ValidationError(
            "file_watch path is required".to_string(),
        ));
    }

    Ok(())
}

/// Validates a webhook (`secret` field >= 32 characters).
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

    /// Creates a valid definition for pure validation tests.
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
