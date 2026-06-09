//! Core types for the Apollia OS trigger system.
//!
//! This module defines the stable contract used by `TriggerEngine` and every
//! trigger source. No actor logic here: only the types, validation errors, and
//! template substitution.

use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

/// Unique identifier for a trigger (non-empty string).
pub type TriggerId = String;

/// Full trigger definition as parsed from `apollia.toml`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TriggerDefinition {
    /// Unique trigger identifier (non-empty).
    pub id: TriggerId,
    /// Target agent name.
    pub agent: String,
    /// Whether the trigger is active.
    pub enabled: bool,
    /// Behavior when the target agent is busy.
    pub on_busy: OnBusyPolicy,
    /// Trigger source configuration.
    pub source: TriggerSourceConfig,
    /// Template for the input message sent to the agent.
    pub input_template: InputTemplate,
}

impl TriggerDefinition {
    /// Validates that the trigger identifier is non-empty.
    ///
    /// Returns [`TriggerDefinitionError::EmptyId`] if `id` is an empty string.
    pub fn validate_id(id: &str) -> Result<(), TriggerDefinitionError> {
        if id.is_empty() {
            Err(TriggerDefinitionError::EmptyId)
        } else {
            Ok(())
        }
    }

    /// Validates that the agent name is non-empty.
    ///
    /// Returns [`TriggerDefinitionError::EmptyAgent`] if `agent` is an empty string.
    pub fn validate_agent(agent: &str) -> Result<(), TriggerDefinitionError> {
        if agent.is_empty() {
            Err(TriggerDefinitionError::EmptyAgent)
        } else {
            Ok(())
        }
    }
}

/// Behavior when the target agent is already busy at fire time.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OnBusyPolicy {
    /// Drop the trigger.
    Skip,
    /// Bounded FIFO queue per agent.
    ///
    /// When the agent is busy, the trigger is queued up to `max_depth`
    /// elements. Beyond that, the trigger is dropped and
    /// `RuntimeEvent::TriggerQueueFull` is emitted.
    Queue {
        /// Maximum number of queued elements per agent.
        max_depth: usize,
    },
    /// Block until the agent is available (not implemented).
    Block,
}

/// Trigger source configuration.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TriggerSourceConfig {
    /// Standard cron expression (e.g. `"0 8 * * MON"`).
    Cron {
        /// Cron expression (e.g. `"0 8 * * MON"`).
        schedule: String,
    },
    /// Periodic interval (e.g. `"30m"`, `"1h"`, `"6h"`, `"1d"`).
    Interval {
        /// Duration as a string (e.g. `"30m"`).
        every: String,
    },
    /// Single fire at a precise date/time.
    Oneshot {
        /// Timestamp of the single fire.
        fire_at: DateTime<Utc>,
    },
    /// Watch a path (file or directory) for filesystem events.
    FileWatch {
        /// Watched path (specific file or directory).
        path: PathBuf,
        /// Event kinds that trigger a fire.
        events: Vec<FileEventKind>,
        /// Recursive watch of subdirectories (default: `false`).
        ///
        /// Only relevant when `path` points to a directory:
        /// - `false`: only the direct contents of the directory are watched.
        /// - `true`: all subdirectories (at any depth) are watched too.
        ///
        /// Ignored when `path` points to a specific file.
        #[serde(default)]
        recursive: bool,
        /// Follow symbolic links (default: `false`).
        ///
        /// When `false`, events whose path is a symlink are filtered out before
        /// propagation. Detected via `fs::symlink_metadata`.
        #[serde(default)]
        follow_symlinks: bool,
        /// Path segments and file patterns to exclude from events.
        ///
        /// Default: `[".git", "node_modules", "__pycache__", ".apollia"]`.
        /// Use `"*.ext"` for extensions, `"name"` or `"name/"` for segments.
        #[serde(default = "crate::config::default_exclude_patterns")]
        exclude_patterns: Vec<String>,
    },
    /// HTTP webhook with HMAC-SHA256 verification.
    Webhook {
        /// Shared secret used to verify the HMAC-SHA256 signature.
        secret: String,
    },
}

/// File event kind filtered by `FileWatchTrigger`.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileEventKind {
    /// File creation.
    Create,
    /// File modification.
    Modify,
    /// File deletion.
    Delete,
    /// Any event kind.
    Any,
}

impl std::fmt::Display for FileEventKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FileEventKind::Create => write!(f, "create"),
            FileEventKind::Modify => write!(f, "modify"),
            FileEventKind::Delete => write!(f, "delete"),
            FileEventKind::Any => write!(f, "any"),
        }
    }
}

/// Raw payload attached to a [`TriggerEvent`].
#[derive(Debug, Clone, serde::Serialize)]
pub enum TriggerPayload {
    /// Payload of a time-based trigger (cron, interval, oneshot).
    Timer {
        /// Scheduled fire time.
        scheduled_at: DateTime<Utc>,
        /// Actual fire time.
        fired_at: DateTime<Utc>,
    },
    /// Payload of a filesystem event.
    File {
        /// Full path of the affected file.
        path: PathBuf,
        /// File name only (without parent directory).
        filename: String,
        /// Size in bytes at the time of the event.
        size_bytes: u64,
        /// File event kind.
        event_kind: FileEventKind,
    },
    /// Payload of an incoming webhook request.
    Webhook {
        /// Raw HTTP request body.
        body: String,
        /// HTTP request headers.
        headers: HashMap<String, String>,
    },
}

/// Event produced by a trigger source and consumed by `TriggerEngine`.
#[derive(Debug, Clone)]
pub struct TriggerEvent {
    /// Identifier of the trigger that produced this event.
    pub trigger_id: TriggerId,
    /// Target agent name.
    pub agent: String,
    /// Raw event payload.
    pub payload: TriggerPayload,
    /// Actual fire time.
    pub fired_at: DateTime<Utc>,
}

/// Input message template with `{{variables}}` substitution.
///
/// Variables available per payload kind:
/// - `Timer`   : `{{scheduled_at}}`, `{{fired_at}}`
/// - `File`    : `{{path}}`, `{{filename}}`, `{{size_bytes}}`, `{{event_kind}}`
/// - `Webhook` : `{{body}}`, `{{header.<name>}}`
///
/// Variables absent from the payload are replaced by an empty string (never panics).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct InputTemplate(pub String);

impl InputTemplate {
    /// Substitutes the `{{variables}}` in the template with values from the payload.
    ///
    /// Unknown variables (absent from the map built from the payload) are
    /// replaced by an empty string. This method can never panic.
    pub fn render(&self, payload: &TriggerPayload) -> String {
        let vars = build_payload_vars(payload);
        let mut result = self.0.clone();
        for (key, value) in &vars {
            result = result.replace(&format!("{{{{{}}}}}", key), value);
        }
        replace_unknown_vars(result)
    }
}

/// Builds the `variable -> value` map from a [`TriggerPayload`].
fn build_payload_vars(payload: &TriggerPayload) -> HashMap<String, String> {
    let mut map = HashMap::new();
    match payload {
        TriggerPayload::Timer {
            scheduled_at,
            fired_at,
        } => {
            map.insert("scheduled_at".into(), scheduled_at.to_rfc3339());
            map.insert("fired_at".into(), fired_at.to_rfc3339());
        }
        TriggerPayload::File {
            path,
            filename,
            size_bytes,
            event_kind,
        } => {
            map.insert("path".into(), path.to_string_lossy().into_owned());
            map.insert("filename".into(), filename.clone());
            map.insert("size_bytes".into(), size_bytes.to_string());
            map.insert("event_kind".into(), event_kind.to_string());
        }
        TriggerPayload::Webhook { body, headers } => {
            map.insert("body".into(), body.clone());
            for (name, value) in headers {
                map.insert(format!("header.{}", name), value.clone());
            }
        }
    }
    map
}

/// Replaces the remaining `{{...}}` patterns (unknown variables) with an empty string.
///
/// Uses a sequential scan to preserve UTF-8 characters.
fn replace_unknown_vars(mut s: String) -> String {
    while let Some(start) = s.find("{{") {
        match s[start..].find("}}") {
            Some(end_rel) => {
                let end = start + end_rel + 2;
                s.replace_range(start..end, "");
            }
            None => break,
        }
    }
    s
}

/// Parses an interval string in the format `"30m"`, `"1h"`, `"6h"` or `"1d"`.
///
/// Returns [`TriggerDefinitionError::InvalidInterval`] when the format is invalid.
/// Recognized units are: `m` (minutes), `h` (hours), `d` (days).
pub fn parse_interval(value: &str) -> Result<Duration, TriggerDefinitionError> {
    // Handle "ms" (milliseconds) before single-char suffixes to avoid ambiguity
    if let Some(s) = value.strip_suffix("ms") {
        let n: u64 = s
            .parse()
            .map_err(|_| TriggerDefinitionError::InvalidInterval {
                value: value.to_string(),
            })?;
        return Ok(Duration::from_millis(n));
    }

    let (num_str, multiplier) = if let Some(s) = value.strip_suffix('m') {
        (s, 60u64)
    } else if let Some(s) = value.strip_suffix('h') {
        (s, 3_600u64)
    } else if let Some(s) = value.strip_suffix('d') {
        (s, 86_400u64)
    } else {
        return Err(TriggerDefinitionError::InvalidInterval {
            value: value.to_string(),
        });
    };

    let n: u64 = num_str
        .parse()
        .map_err(|_| TriggerDefinitionError::InvalidInterval {
            value: value.to_string(),
        })?;

    Ok(Duration::from_secs(n * multiplier))
}

/// Validation errors for a [`TriggerDefinition`].
#[derive(thiserror::Error, Debug)]
pub enum TriggerDefinitionError {
    /// The trigger identifier is empty.
    #[error("trigger id cannot be empty")]
    EmptyId,

    /// The target agent name is empty.
    #[error("agent name cannot be empty")]
    EmptyAgent,

    /// The cron expression is invalid.
    #[error("invalid cron schedule '{schedule}': {reason}")]
    InvalidCronSchedule {
        /// Invalid cron expression.
        schedule: String,
        /// Reason it is invalid.
        reason: String,
    },

    /// The interval does not match the expected format.
    #[error("invalid interval '{value}': expected format '30m', '1h', '6h' or '1d'")]
    InvalidInterval {
        /// Received value.
        value: String,
    },

    /// The webhook secret is empty.
    #[error("webhook secret cannot be empty")]
    EmptyWebhookSecret,

    /// The FileWatch path is empty.
    #[error("file_watch path is empty")]
    EmptyFileWatchPath,
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn test_render_timer_variables() {
        // GIVEN
        let template = InputTemplate("Rapport du {{scheduled_at}} - généré à {{fired_at}}".into());
        let scheduled = Utc.with_ymd_and_hms(2026, 3, 9, 8, 0, 0).unwrap();
        let fired = Utc.with_ymd_and_hms(2026, 3, 9, 8, 0, 1).unwrap();
        let payload = TriggerPayload::Timer {
            scheduled_at: scheduled,
            fired_at: fired,
        };
        // WHEN
        let result = template.render(&payload);
        // THEN
        assert!(result.contains("2026-03-09"), "résultat: {result}");
        assert!(
            !result.contains("{{"),
            "des variables non substituées restent: {result}"
        );
    }

    #[test]
    fn test_render_file_variables() {
        // GIVEN
        let template =
            InputTemplate("Nouvelle facture : {{filename}} ({{size_bytes}} octets)".into());
        let payload = TriggerPayload::File {
            path: "/inbox/facture-001.pdf".into(),
            filename: "facture-001.pdf".into(),
            size_bytes: 42100,
            event_kind: FileEventKind::Create,
        };
        // WHEN
        let result = template.render(&payload);
        // THEN
        assert_eq!(result, "Nouvelle facture : facture-001.pdf (42100 octets)");
    }

    #[test]
    fn test_unknown_variable_replaced_by_empty() {
        // GIVEN
        let template = InputTemplate("{{unknown}} texte".into());
        let payload = TriggerPayload::Timer {
            scheduled_at: Utc::now(),
            fired_at: Utc::now(),
        };
        // WHEN
        let result = template.render(&payload);
        // THEN
        assert_eq!(result, " texte");
        assert!(
            !result.contains("{{"),
            "des patterns {{}} restent: {result}"
        );
    }

    #[test]
    fn test_empty_id_returns_error() {
        // GIVEN / WHEN
        let result = TriggerDefinition::validate_id("");
        // THEN
        assert!(matches!(result, Err(TriggerDefinitionError::EmptyId)));
    }

    #[test]
    fn test_trigger_definition_json_roundtrip() {
        // GIVEN
        let def = TriggerDefinition {
            id: "rapport-hebdo".into(),
            agent: "rapport-agent".into(),
            enabled: true,
            on_busy: OnBusyPolicy::Queue { max_depth: 10 },
            source: TriggerSourceConfig::Cron {
                schedule: "0 8 * * MON".into(),
            },
            input_template: InputTemplate("Rapport du {{scheduled_at}}".into()),
        };
        // WHEN
        let json = serde_json::to_string(&def).expect("sérialisation JSON");
        let back: TriggerDefinition = serde_json::from_str(&json).expect("désérialisation JSON");
        // THEN
        assert_eq!(back.id, "rapport-hebdo");
        assert_eq!(back.agent, "rapport-agent");
        assert!(back.enabled);
        assert_eq!(back.on_busy, OnBusyPolicy::Queue { max_depth: 10 });
    }

    #[test]
    fn test_render_webhook_body_variable() {
        // GIVEN
        let template = InputTemplate("{{body}}".into());
        let payload = TriggerPayload::Webhook {
            body: r#"{"event":"sale"}"#.into(),
            headers: HashMap::new(),
        };
        // WHEN
        let result = template.render(&payload);
        // THEN
        assert_eq!(result, r#"{"event":"sale"}"#);
    }

    #[test]
    fn test_render_webhook_header_variable() {
        // GIVEN
        let template = InputTemplate("Content-Type: {{header.content-type}}".into());
        let mut headers = HashMap::new();
        headers.insert("content-type".into(), "application/json".into());
        let payload = TriggerPayload::Webhook {
            body: "{}".into(),
            headers,
        };
        // WHEN
        let result = template.render(&payload);
        // THEN
        assert_eq!(result, "Content-Type: application/json");
    }

    #[test]
    fn test_parse_interval_valid_formats() {
        // GIVEN / WHEN / THEN
        assert_eq!(parse_interval("30m").unwrap(), Duration::from_secs(1_800));
        assert_eq!(parse_interval("1h").unwrap(), Duration::from_secs(3_600));
        assert_eq!(parse_interval("6h").unwrap(), Duration::from_secs(21_600));
        assert_eq!(parse_interval("1d").unwrap(), Duration::from_secs(86_400));
    }

    #[test]
    fn test_parse_interval_invalid_returns_error() {
        // GIVEN / WHEN
        let result = parse_interval("invalid");
        // THEN
        assert!(matches!(
            result,
            Err(TriggerDefinitionError::InvalidInterval { .. })
        ));
    }

    #[test]
    fn test_validate_agent_empty_returns_error() {
        // GIVEN / WHEN
        let result = TriggerDefinition::validate_agent("");
        // THEN
        assert!(matches!(result, Err(TriggerDefinitionError::EmptyAgent)));
    }
}
