//! Validation of notification channels and event names.
//!
//! This module centralizes the list of known events ([`KNOWN_EVENTS`]) and
//! provides validation functions called by the
//! [`NotificationConfigRepository`](crate::repository::NotificationConfigRepository)
//! before any write to the database.

use crate::repository::NotificationChannelRow;

/// Exhaustive list of event names recognized by the notification system.
///
/// Any event not in this list is rejected by [`validate_events`] with a
/// [`NotificationConfigError::ValidationError`].
pub const KNOWN_EVENTS: &[&str] = &[
    "task.completed",
    "task.failed",
    "task.input_required",
    "agent.degraded",
    "agent.inactivity",
    "trigger.error",
    "pipeline.completed",
    "pipeline.failed",
    "pipeline.suspended",
    "llm.backend_down",
    "chat.approval_required",
    "chat.tool_failed",
    "chat.user_input_required",
];

/// Error returned by [`NotificationConfigRepository`](crate::repository::NotificationConfigRepository) operations.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum NotificationConfigError {
    /// The requested channel does not exist in the database.
    #[error("channel not found: {0}")]
    NotFound(String),
    /// A channel with this identifier already exists.
    #[error("duplicate channel id: {0}")]
    DuplicateId(String),
    /// Invalid data (webhook without URL, unknown event, etc.).
    #[error("validation error: {0}")]
    ValidationError(String),
    /// Underlying SQLite error.
    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),
    /// The database schema could not be brought to the supported version.
    #[error(transparent)]
    Schema(#[from] apollia_core::schema::SchemaError),
}

/// Maximum length of a channel `label`, in Unicode characters (`char`).
pub const MAX_LABEL_LEN: usize = 80;

/// Upper bound on a channel's throttle: 24 hours.
///
/// Beyond that, we move into scheduled-digest territory, which is out of scope
/// for this feature.
pub const MAX_MIN_INTERVAL_SECONDS: u32 = 86_400;

/// Validates a notification channel before insert or update.
///
/// Rules:
/// - A `"webhook"` channel must have a non-empty `"url"` key in `config_json`.
/// - If `label` is `Some`, it must be non-empty after trim and <= [`MAX_LABEL_LEN`] characters.
/// - `min_interval_seconds` <= [`MAX_MIN_INTERVAL_SECONDS`].
pub fn validate_channel(ch: &NotificationChannelRow) -> Result<(), NotificationConfigError> {
    if ch.channel_type == "webhook" {
        let has_url = ch
            .config_json
            .get("url")
            .and_then(|v| v.as_str())
            .is_some_and(|s| !s.is_empty());
        if !has_url {
            return Err(NotificationConfigError::ValidationError(
                "webhook channel requires 'url' in config".into(),
            ));
        }
    }
    validate_label(ch.label.as_deref())?;
    validate_min_interval(ch.min_interval_seconds)?;
    Ok(())
}

/// Validates a `min_interval_seconds` value.
///
/// `0` is accepted (no throttling). Beyond [`MAX_MIN_INTERVAL_SECONDS`], the
/// value is rejected to avoid absurd windows on the UI side.
pub fn validate_min_interval(seconds: u32) -> Result<(), NotificationConfigError> {
    if seconds > MAX_MIN_INTERVAL_SECONDS {
        return Err(NotificationConfigError::ValidationError(format!(
            "min_interval_seconds too large: {seconds} (max {MAX_MIN_INTERVAL_SECONDS})"
        )));
    }
    Ok(())
}

/// Validates a free-form label.
///
/// - `None` is accepted (the channel falls back to its `id` on the UI side).
/// - `Some("")` or `Some("   ")` (whitespace only) is rejected.
/// - More than [`MAX_LABEL_LEN`] Unicode characters is rejected.
pub fn validate_label(label: Option<&str>) -> Result<(), NotificationConfigError> {
    let Some(label) = label else {
        return Ok(());
    };
    if label.trim().is_empty() {
        return Err(NotificationConfigError::ValidationError(
            "label cannot be empty or whitespace-only".into(),
        ));
    }
    let char_count = label.chars().count();
    if char_count > MAX_LABEL_LEN {
        return Err(NotificationConfigError::ValidationError(format!(
            "label too long: {char_count} chars (max {MAX_LABEL_LEN})"
        )));
    }
    Ok(())
}

/// Valide une liste de noms d'événements contre [`KNOWN_EVENTS`].
///
/// Retourne une erreur dès le premier événement inconnu rencontré.
pub fn validate_events(events: &[String]) -> Result<(), NotificationConfigError> {
    for event in events {
        if !KNOWN_EVENTS.contains(&event.as_str()) {
            return Err(NotificationConfigError::ValidationError(format!(
                "unknown event: {event}"
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_webhook_channel(config: serde_json::Value) -> NotificationChannelRow {
        NotificationChannelRow {
            id: "test-webhook".into(),
            label: None,
            channel_type: "webhook".into(),
            enabled: true,
            config_json: config,
            events_json: None,
            min_interval_seconds: 0,
            created_at: String::new(),
            updated_at: String::new(),
        }
    }

    #[test]
    fn test_validate_channel_webhook_with_url_ok() {
        // GIVEN a webhook channel carrying a URL
        let ch = make_webhook_channel(serde_json::json!({"url": "https://example.com/hook"}));
        // WHEN the channel is validated
        // THEN it is accepted
        assert!(validate_channel(&ch).is_ok());
    }

    #[test]
    fn test_validate_channel_webhook_no_url_rejects() {
        // GIVEN a webhook channel with no URL at all
        let ch = make_webhook_channel(serde_json::json!({}));
        // WHEN the channel is validated
        let err = validate_channel(&ch).unwrap_err();
        // THEN it is refused, and the message names the missing field
        assert!(
            matches!(&err, NotificationConfigError::ValidationError(msg) if msg.contains("url"))
        );
    }

    #[test]
    fn test_validate_channel_webhook_empty_url_rejects() {
        // GIVEN a webhook channel whose URL is the empty string
        let ch = make_webhook_channel(serde_json::json!({"url": ""}));
        // WHEN the channel is validated
        let err = validate_channel(&ch).unwrap_err();
        // THEN it is refused: an empty URL is not a URL
        assert!(matches!(&err, NotificationConfigError::ValidationError(_)));
    }

    #[test]
    fn test_validate_channel_desktop_no_url_ok() {
        // GIVEN a desktop channel, which needs no URL
        let ch = NotificationChannelRow {
            id: "desktop".into(),
            label: None,
            channel_type: "desktop".into(),
            enabled: true,
            config_json: serde_json::json!({}),
            events_json: None,
            min_interval_seconds: 0,
            created_at: String::new(),
            updated_at: String::new(),
        };
        // WHEN the channel is validated
        // THEN it is accepted
        assert!(validate_channel(&ch).is_ok());
    }

    #[test]
    fn test_validate_min_interval_zero_ok() {
        // GIVEN a minimum interval of zero, meaning no throttling
        // WHEN it is validated
        // THEN it is accepted
        assert!(validate_min_interval(0).is_ok());
    }

    #[test]
    fn test_validate_min_interval_reasonable_ok() {
        // GIVEN intervals from one second up to the cap itself
        // WHEN each is validated
        for v in [1u32, 60, 300, 3600, 7200, MAX_MIN_INTERVAL_SECONDS] {
            // THEN every one of them is accepted, the cap included
            assert!(validate_min_interval(v).is_ok(), "expected {v} to be ok");
        }
    }

    #[test]
    fn test_validate_min_interval_over_cap_rejects() {
        // GIVEN an interval one second past the cap
        // WHEN it is validated
        let err = validate_min_interval(MAX_MIN_INTERVAL_SECONDS + 1).unwrap_err();
        // THEN it is refused, and the message says the value is too large
        assert!(
            matches!(&err, NotificationConfigError::ValidationError(m) if m.contains("too large"))
        );
    }

    #[test]
    fn test_validate_label_none_ok() {
        // GIVEN no label
        // WHEN / THEN validation passes (legacy channels without label)
        assert!(validate_label(None).is_ok());
    }

    #[test]
    fn test_validate_label_human_text_ok() {
        // GIVEN a free-form label with spaces and accents
        // WHEN / THEN validation passes
        assert!(validate_label(Some("Alertes Slack équipe")).is_ok());
    }

    #[test]
    fn test_validate_label_empty_rejects() {
        // GIVEN an empty or whitespace-only label
        // WHEN / THEN validation fails
        assert!(validate_label(Some("")).is_err());
        assert!(validate_label(Some("   ")).is_err());
    }

    #[test]
    fn test_validate_label_too_long_rejects() {
        // GIVEN a label of 81 chars
        let too_long: String = "a".repeat(81);
        // WHEN / THEN validation fails
        let err = validate_label(Some(&too_long)).unwrap_err();
        assert!(
            matches!(&err, NotificationConfigError::ValidationError(m) if m.contains("too long")),
            "expected too-long error, got: {err:?}"
        );
    }

    #[test]
    fn test_validate_label_unicode_chars_counted() {
        // GIVEN 80 emoji (1 char each in Unicode), should pass
        // WHEN each label is validated
        // THEN the eighty-character label passes and the eighty-first character fails: the limit counts characters, not bytes
        let just_under: String = "🚀".repeat(80);
        assert!(validate_label(Some(&just_under)).is_ok());

        // GIVEN 81 emoji
        let over: String = "🚀".repeat(81);
        assert!(validate_label(Some(&over)).is_err());
    }

    #[test]
    fn test_validate_events_all_known_ok() {
        // GIVEN two event names the runtime declares
        let events = vec!["task.completed".into(), "task.failed".into()];
        // WHEN the list is validated
        // THEN it is accepted
        assert!(validate_events(&events).is_ok());
    }

    #[test]
    fn test_validate_events_chat_approval_required_ok() {
        // GIVEN the new chat.approval_required event
        let events = vec!["chat.approval_required".into()];
        // WHEN / THEN validation passes
        assert!(validate_events(&events).is_ok());
    }

    #[test]
    fn test_validate_events_unknown_rejects() {
        // GIVEN a list mixing a declared event with one the runtime never emits
        let events = vec!["task.completed".into(), "unknown.event".into()];
        // WHEN the list is validated
        let err = validate_events(&events).unwrap_err();
        // THEN it is refused, and the message names the offending event
        assert!(
            matches!(&err, NotificationConfigError::ValidationError(msg) if msg.contains("unknown.event"))
        );
    }
}
