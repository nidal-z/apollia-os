//! TOML parsing of the `[[triggers]]` section from the contents of `apollia.toml`.
//!
//! Used by `apollia-runtime` for hot reload via `POST /api/v1/triggers/reload`
//! without depending on `apollia-cli` (which would create a circular dependency).
//!
//! Validation is fail-fast: any configuration error is detected before the
//! definitions are handed to the [`TriggerEngine`].

use std::path::{Path, PathBuf};
use std::str::FromStr;

use serde::Deserialize;

use crate::types::{
    parse_interval, FileEventKind, InputTemplate, OnBusyPolicy, TriggerDefinition,
    TriggerDefinitionError, TriggerSourceConfig,
};

// --- Error -------------------------------------------------------------------

/// Errors returned by [`parse_triggers_from_toml_str`].
#[derive(Debug, thiserror::Error)]
pub enum TriggerTomlError {
    /// The TOML content is malformed (invalid syntax, wrong type).
    #[error("invalid TOML: {0}")]
    Parse(#[from] toml::de::Error),

    /// A trigger has a semantically invalid configuration.
    #[error("invalid trigger '{id}': {reason}")]
    InvalidTrigger {
        /// Identifier of the offending trigger.
        id: String,
        /// Description of the validation error.
        reason: String,
    },
}

// --- Raw TOML types ----------------------------------------------------------

/// Root structure to deserialize only the `[[triggers]]` section.
#[derive(Debug, Deserialize)]
struct RawRoot {
    #[serde(default)]
    triggers: Vec<RawTrigger>,
}

/// Raw format of a trigger in TOML before semantic validation.
#[derive(Debug, Deserialize)]
struct RawTrigger {
    id: String,
    /// Target agent.
    #[serde(default)]
    agent: String,
    #[serde(default = "default_true")]
    enabled: bool,
    #[serde(default)]
    on_busy: String,
    #[serde(default)]
    input_template: String,
    source: RawTriggerSource,
}

/// Raw format of a trigger's `source` subsection.
#[derive(Debug, Deserialize)]
struct RawTriggerSource {
    #[serde(rename = "type")]
    kind: String,
    schedule: Option<String>,
    every: Option<String>,
    fire_at: Option<String>,
    path: Option<String>,
    events: Option<Vec<String>>,
    secret: Option<String>,
    /// Recursive watch of subdirectories for `file_watch` sources pointing to a
    /// directory (default: false).
    #[serde(default)]
    recursive: bool,
    /// Follow symlinks for `file_watch` sources (default: false).
    #[serde(default)]
    follow_symlinks: bool,
    /// Exclusion patterns for `file_watch` sources.
    /// Absent from the TOML means the defaults are applied.
    exclude_patterns: Option<Vec<String>>,
}

/// Returns `true`, the default value for the `enabled` field.
fn default_true() -> bool {
    true
}

// --- Public API --------------------------------------------------------------

/// Parses the `[[triggers]]` section from the TOML contents of an `apollia.toml`.
///
/// Semantically validates each enabled trigger (cron schedule, webhook secret, etc.).
/// Triggers with `enabled = false` are not semantically validated.
///
/// # Errors
///
/// - [`TriggerTomlError::Parse`]: malformed TOML.
/// - [`TriggerTomlError::InvalidTrigger`]: trigger with an invalid configuration.
pub fn parse_triggers_from_toml_str(
    toml_str: &str,
) -> Result<Vec<TriggerDefinition>, TriggerTomlError> {
    let raw: RawRoot = toml::from_str(toml_str)?;
    raw.triggers.iter().map(validate_trigger).collect()
}

// --- Validation --------------------------------------------------------------

/// Validates a raw trigger and converts it into a [`TriggerDefinition`].
fn validate_trigger(raw: &RawTrigger) -> Result<TriggerDefinition, TriggerTomlError> {
    if raw.id.is_empty() {
        return Err(TriggerTomlError::InvalidTrigger {
            id: raw.id.clone(),
            reason: TriggerDefinitionError::EmptyId.to_string(),
        });
    }
    if raw.agent.is_empty() {
        return Err(TriggerTomlError::InvalidTrigger {
            id: raw.id.clone(),
            reason: TriggerDefinitionError::EmptyAgent.to_string(),
        });
    }

    let on_busy = match raw.on_busy.as_str() {
        "skip" | "drop" => OnBusyPolicy::Skip,
        "block" => OnBusyPolicy::Block,
        _ => OnBusyPolicy::Queue {
            max_depth: crate::DEFAULT_QUEUE_MAX_DEPTH,
        },
    };

    let source = if raw.enabled {
        validate_trigger_source(&raw.id, &raw.source)?
    } else {
        parse_trigger_source_unchecked(&raw.source)
    };

    Ok(TriggerDefinition {
        id: raw.id.clone(),
        agent: raw.agent.clone(),
        enabled: raw.enabled,
        on_busy,
        source,
        input_template: InputTemplate(raw.input_template.clone()),
    })
}

/// Semantically validates the source of an enabled trigger.
fn validate_trigger_source(
    id: &str,
    raw: &RawTriggerSource,
) -> Result<TriggerSourceConfig, TriggerTomlError> {
    match raw.kind.as_str() {
        "cron" => {
            let schedule = raw.schedule.clone().unwrap_or_default();
            let normalized = normalize_cron(id, &schedule)?;
            Ok(TriggerSourceConfig::Cron {
                schedule: normalized,
            })
        }

        "interval" => {
            let every = raw.every.clone().unwrap_or_default();
            parse_interval(&every).map_err(|e| TriggerTomlError::InvalidTrigger {
                id: id.to_string(),
                reason: e.to_string(),
            })?;
            Ok(TriggerSourceConfig::Interval { every })
        }

        "oneshot" => {
            let fire_at_str = raw.fire_at.clone().unwrap_or_default();
            let fire_at = fire_at_str
                .parse::<chrono::DateTime<chrono::Utc>>()
                .map_err(|e| TriggerTomlError::InvalidTrigger {
                    id: id.to_string(),
                    reason: format!("invalid fire_at timestamp: {e}"),
                })?;
            Ok(TriggerSourceConfig::Oneshot { fire_at })
        }

        "file_watch" => {
            let path_str = raw.path.clone().unwrap_or_default();
            if path_str.is_empty() {
                return Err(TriggerTomlError::InvalidTrigger {
                    id: id.to_string(),
                    reason: TriggerDefinitionError::EmptyFileWatchPath.to_string(),
                });
            }
            let path = expand_tilde(Path::new(&path_str));
            let events = parse_file_event_kinds(raw.events.as_deref().unwrap_or(&[]));
            let exclude_patterns = raw
                .exclude_patterns
                .clone()
                .unwrap_or_else(crate::config::default_exclude_patterns);
            Ok(TriggerSourceConfig::FileWatch {
                path,
                events,
                recursive: raw.recursive,
                follow_symlinks: raw.follow_symlinks,
                exclude_patterns,
            })
        }

        "webhook" => {
            let secret = raw.secret.clone().unwrap_or_default();
            if secret.is_empty() {
                return Err(TriggerTomlError::InvalidTrigger {
                    id: id.to_string(),
                    reason: TriggerDefinitionError::EmptyWebhookSecret.to_string(),
                });
            }
            Ok(TriggerSourceConfig::Webhook { secret })
        }

        unknown => Err(TriggerTomlError::InvalidTrigger {
            id: id.to_string(),
            reason: format!("unknown source type '{unknown}'"),
        }),
    }
}

/// Validates and normalizes a cron expression (5 or 6 fields).
fn normalize_cron(id: &str, schedule: &str) -> Result<String, TriggerTomlError> {
    if cron::Schedule::from_str(schedule).is_ok() {
        return Ok(schedule.to_string());
    }
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
        .unwrap_or_else(|| "invalid cron expression".to_string());
    Err(TriggerTomlError::InvalidTrigger {
        id: id.to_string(),
        reason: TriggerDefinitionError::InvalidCronSchedule {
            schedule: schedule.to_string(),
            reason,
        }
        .to_string(),
    })
}

/// Converts file event names into [`FileEventKind`].
fn parse_file_event_kinds(raw: &[String]) -> Vec<FileEventKind> {
    if raw.is_empty() {
        return vec![FileEventKind::Create];
    }
    raw.iter()
        .map(|s| match s.as_str() {
            "create" => FileEventKind::Create,
            "modify" => FileEventKind::Modify,
            "delete" => FileEventKind::Delete,
            _ => FileEventKind::Any,
        })
        .collect()
}

/// Minimal source parsing without validation, for disabled triggers.
fn parse_trigger_source_unchecked(raw: &RawTriggerSource) -> TriggerSourceConfig {
    match raw.kind.as_str() {
        "cron" => TriggerSourceConfig::Cron {
            schedule: raw.schedule.clone().unwrap_or_default(),
        },
        "interval" => TriggerSourceConfig::Interval {
            every: raw.every.clone().unwrap_or_default(),
        },
        "oneshot" => {
            let fire_at = raw
                .fire_at
                .as_deref()
                .and_then(|s| s.parse::<chrono::DateTime<chrono::Utc>>().ok())
                .unwrap_or_else(chrono::Utc::now);
            TriggerSourceConfig::Oneshot { fire_at }
        }
        "file_watch" => {
            let path = expand_tilde(Path::new(raw.path.as_deref().unwrap_or("")));
            let events = parse_file_event_kinds(raw.events.as_deref().unwrap_or(&[]));
            let exclude_patterns = raw
                .exclude_patterns
                .clone()
                .unwrap_or_else(crate::config::default_exclude_patterns);
            TriggerSourceConfig::FileWatch {
                path,
                events,
                recursive: raw.recursive,
                follow_symlinks: raw.follow_symlinks,
                exclude_patterns,
            }
        }
        _ => TriggerSourceConfig::Webhook {
            secret: raw.secret.clone().unwrap_or_default(),
        },
    }
}

/// Resolves a leading `~` path component to `$HOME`.
fn expand_tilde(path: &Path) -> PathBuf {
    let s = path.to_string_lossy();
    if s.starts_with("~/") {
        let home = apollia_core::paths::home_string().unwrap_or_default();
        PathBuf::from(format!("{}{}", home, &s[1..]))
    } else if s == "~" {
        PathBuf::from(apollia_core::paths::home_string().unwrap_or_default())
    } else {
        path.to_path_buf()
    }
}

// --- Tests -------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_valid_cron_trigger() {
        // GIVEN a TOML with a valid cron trigger
        let toml = r#"
[[triggers]]
id             = "rapport-hebdo"
agent          = "rapport-agent"
enabled        = true
on_busy        = "queue"
input_template = "Rapport du {{scheduled_at}}"

[triggers.source]
type     = "cron"
schedule = "0 8 * * MON"
"#;
        // WHEN
        let result = parse_triggers_from_toml_str(toml);
        // THEN
        let defs = result.expect("doit parser sans erreur");
        assert_eq!(defs.len(), 1);
        assert_eq!(defs[0].id, "rapport-hebdo");
        assert!(defs[0].enabled);
    }

    #[test]
    fn test_parse_invalid_cron_returns_error() {
        // GIVEN a cron trigger with an invalid schedule
        let toml = r#"
[[triggers]]
id             = "bad-cron"
agent          = "agent"
enabled        = true
input_template = "test"

[triggers.source]
type     = "cron"
schedule = "not-valid"
"#;
        // WHEN
        let result = parse_triggers_from_toml_str(toml);
        // THEN error containing the trigger_id
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("bad-cron"),
            "message doit contenir 'bad-cron': {msg}"
        );
    }

    #[test]
    fn test_parse_empty_triggers_returns_empty_vec() {
        // GIVEN a TOML without a [[triggers]] section
        let result = parse_triggers_from_toml_str("[agents]\ndirectory = \"agents/\"");
        // THEN empty vec, no error
        let defs = result.expect("doit parser sans erreur");
        assert!(defs.is_empty());
    }

    #[test]
    fn test_parse_webhook_empty_secret_returns_error() {
        // GIVEN a webhook trigger with an empty secret
        let toml = r#"
[[triggers]]
id             = "crm-sync"
agent          = "crm-agent"
enabled        = true
input_template = "{{body}}"

[triggers.source]
type   = "webhook"
secret = ""
"#;
        // WHEN
        let result = parse_triggers_from_toml_str(toml);
        // THEN error
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("crm-sync"),
            "message doit contenir 'crm-sync': {msg}"
        );
    }
}
