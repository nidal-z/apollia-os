//! TOML parsing of the `[[triggers]]` section from the contents of `apollia.toml`.
//!
//! Used by `apollia-cli` when installing an agent package, to read the
//! `[[triggers]]` declared by the package before they are inserted in SQLite.
//!
//! Validation is fail-fast: any configuration error is detected before the
//! definitions are handed to the [`TriggerEngine`].

use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::types::{
    parse_interval, FileEventKind, InputTemplate, OnBusyPolicy, TriggerDefinition,
    TriggerDefinitionError, TriggerSourceConfig,
};

// --- Error -------------------------------------------------------------------

/// Errors returned by [`parse_triggers_from_toml_str`].
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
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
///
/// Delegates to [`crate::validation::accepted_cron_schedule`], which the SQLite
/// write path also runs on every insert and update. This wrapper exists only to
/// attach the trigger identifier the TOML error carries and the canonical
/// function does not know about.
fn normalize_cron(id: &str, schedule: &str) -> Result<String, TriggerTomlError> {
    crate::validation::accepted_cron_schedule(schedule).map_err(|e| {
        TriggerTomlError::InvalidTrigger {
            id: id.to_string(),
            reason: e.to_string(),
        }
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
    use std::str::FromStr;

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
        // WHEN it is parsed
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

    // --- The two doors to the cron normalization -------------------------

    /// Normalizes a cron expression through the `apollia agent install` door:
    /// the `[[triggers]]` TOML parser of this module.
    fn schedule_through_toml(id: &str, schedule: &str) -> Result<String, String> {
        let toml = format!(
            "[[triggers]]\n\
             id = \"{id}\"\n\
             agent = \"rapport-agent\"\n\
             enabled = true\n\
             on_busy = \"queue\"\n\
             input_template = \"Rapport\"\n\
             [triggers.source]\n\
             type = \"cron\"\n\
             schedule = \"{schedule}\"\n"
        );
        let defs = parse_triggers_from_toml_str(&toml).map_err(|e| e.to_string())?;
        let def = defs
            .into_iter()
            .next()
            .ok_or_else(|| "no trigger parsed".to_string())?;
        match def.source {
            TriggerSourceConfig::Cron { schedule } => Ok(schedule),
            other => Err(format!("not a cron source: {other:?}")),
        }
    }

    /// Normalizes a cron expression through the SQLite door: the write path
    /// every surface that persists a trigger goes through.
    fn schedule_through_repository(id: &str, schedule: &str) -> Result<String, String> {
        let dir = tempfile::TempDir::new().map_err(|e| e.to_string())?;
        let repo = crate::definition_repository::TriggerDefinitionRepository::open(
            &dir.path().join("triggers.db"),
        )
        .map_err(|e| e.to_string())?;
        let def = crate::definition_repository::TriggerDefinitionRow {
            id: id.to_string(),
            agent: Some("rapport-agent".to_string()),
            enabled: true,
            on_busy: crate::definition_repository::OnBusy::Queue,
            source_type: "cron".to_string(),
            source_config: serde_json::json!({ "schedule": schedule }),
            input_template: Some("Rapport".to_string()),
            created_at: String::new(),
            updated_at: String::new(),
        };
        repo.insert(&def).map_err(|e| e.to_string())?;
        let stored = repo
            .get(id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "trigger not stored".to_string())?;
        stored
            .source_config
            .get("schedule")
            .and_then(|v| v.as_str())
            .map(str::to_string)
            .ok_or_else(|| "stored schedule missing".to_string())
    }

    #[test]
    fn test_both_cron_doors_agree_on_verdict_form_and_reason() {
        // GIVEN expressions covering the 5-to-6 normalization, the forms
        // already accepted verbatim, and the rejections
        let cases = [
            "0 8 * * MON",
            "*/5 * * * *",
            "30 2 1 * *",
            "0 0 8 * * MON",
            "0 0 8 * * MON *",
            "",
            "not a cron",
            "99 99 99 99 99",
            "0 8 * *",
        ];

        for (index, schedule) in cases.iter().enumerate() {
            let id = format!("cross-{index}");

            // WHEN the same expression goes through both doors
            let via_toml = schedule_through_toml(&id, schedule);
            let via_repository = schedule_through_repository(&id, schedule);

            // THEN the verdict is the same
            assert_eq!(
                via_toml.is_ok(),
                via_repository.is_ok(),
                "verdicts diverge on {schedule:?}: toml={via_toml:?}, repository={via_repository:?}"
            );

            // THEN on acceptance the normalized form is the same, and the
            // runtime reader of `sources/cron.rs` accepts it verbatim
            if let (Ok(from_toml), Ok(from_repository)) = (&via_toml, &via_repository) {
                assert_eq!(
                    from_toml, from_repository,
                    "normalized forms diverge on {schedule:?}"
                );
                assert!(
                    cron::Schedule::from_str(from_toml).is_ok(),
                    "the runtime reader would refuse {from_toml:?}"
                );
            }

            // THEN on rejection the reason reported is the same one, the TOML
            // door only prefixing the trigger identifier
            if let (Err(from_toml), Err(from_repository)) = (&via_toml, &via_repository) {
                assert!(
                    from_toml.contains(from_repository.as_str()),
                    "reasons diverge on {schedule:?}: toml={from_toml:?}, repository={from_repository:?}"
                );
            }
        }
    }
}
