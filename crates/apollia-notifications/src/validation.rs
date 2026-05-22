//! Validation des canaux de notification et des noms d'événements.
//!
//! Ce module centralise la liste des événements connus ([`KNOWN_EVENTS`])
//! et fournit des fonctions de validation appelées par le
//! [`NotificationConfigRepository`](crate::repository::NotificationConfigRepository)
//! avant toute écriture en base.

use crate::repository::NotificationChannelRow;

/// Liste exhaustive des noms d'événements reconnus par le système de notification.
///
/// Tout événement qui n'apparaît pas dans cette liste est rejeté par
/// [`validate_events`] avec une erreur [`NotificationConfigError::ValidationError`].
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

/// Erreur retournée par les opérations du [`NotificationConfigRepository`](crate::repository::NotificationConfigRepository).
#[derive(Debug, thiserror::Error)]
pub enum NotificationConfigError {
    /// Le canal demandé n'existe pas en base.
    #[error("channel not found: {0}")]
    NotFound(String),
    /// Un canal avec cet identifiant existe déjà.
    #[error("duplicate channel id: {0}")]
    DuplicateId(String),
    /// Donnée invalide (webhook sans URL, événement inconnu, etc.).
    #[error("validation error: {0}")]
    ValidationError(String),
    /// Erreur SQLite sous-jacente.
    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),
}

/// Longueur maximale du `label` d'un canal, en caractères Unicode (`char`).
pub const MAX_LABEL_LEN: usize = 80;

/// Borne supérieure du throttle d'un canal — 24 heures.
///
/// Au-delà, on bascule en territoire de digest planifié, qui sort du périmètre
/// de cette feature.
pub const MAX_MIN_INTERVAL_SECONDS: u32 = 86_400;

/// Valide un canal de notification avant insertion ou mise à jour.
///
/// Règles :
/// - Un canal de type `"webhook"` doit avoir une clé `"url"` non vide dans `config_json`.
/// - Si `label` est `Some`, il doit être non vide après trim et ≤ [`MAX_LABEL_LEN`] caractères.
/// - `min_interval_seconds` ≤ [`MAX_MIN_INTERVAL_SECONDS`].
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

/// Valide une valeur de `min_interval_seconds`.
///
/// `0` est accepté (pas de throttling). Au-delà de [`MAX_MIN_INTERVAL_SECONDS`],
/// la valeur est refusée pour éviter des fenêtres absurdes côté UI.
pub fn validate_min_interval(seconds: u32) -> Result<(), NotificationConfigError> {
    if seconds > MAX_MIN_INTERVAL_SECONDS {
        return Err(NotificationConfigError::ValidationError(format!(
            "min_interval_seconds too large: {seconds} (max {MAX_MIN_INTERVAL_SECONDS})"
        )));
    }
    Ok(())
}

/// Valide un label libre.
///
/// - `None` est accepté (le canal retombera sur son `id` côté UI).
/// - `Some("")` ou `Some("   ")` (whitespace seul) est refusé.
/// - Plus de [`MAX_LABEL_LEN`] caractères Unicode est refusé.
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
        let ch = make_webhook_channel(serde_json::json!({"url": "https://example.com/hook"}));
        assert!(validate_channel(&ch).is_ok());
    }

    #[test]
    fn test_validate_channel_webhook_no_url_rejects() {
        let ch = make_webhook_channel(serde_json::json!({}));
        let err = validate_channel(&ch).unwrap_err();
        assert!(
            matches!(&err, NotificationConfigError::ValidationError(msg) if msg.contains("url"))
        );
    }

    #[test]
    fn test_validate_channel_webhook_empty_url_rejects() {
        let ch = make_webhook_channel(serde_json::json!({"url": ""}));
        let err = validate_channel(&ch).unwrap_err();
        assert!(matches!(&err, NotificationConfigError::ValidationError(_)));
    }

    #[test]
    fn test_validate_channel_desktop_no_url_ok() {
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
        assert!(validate_channel(&ch).is_ok());
    }

    #[test]
    fn test_validate_min_interval_zero_ok() {
        assert!(validate_min_interval(0).is_ok());
    }

    #[test]
    fn test_validate_min_interval_reasonable_ok() {
        for v in [1u32, 60, 300, 3600, 7200, MAX_MIN_INTERVAL_SECONDS] {
            assert!(validate_min_interval(v).is_ok(), "expected {v} to be ok");
        }
    }

    #[test]
    fn test_validate_min_interval_over_cap_rejects() {
        let err = validate_min_interval(MAX_MIN_INTERVAL_SECONDS + 1).unwrap_err();
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
        // GIVEN un label libre avec espaces et accents
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
        let just_under: String = "🚀".repeat(80);
        assert!(validate_label(Some(&just_under)).is_ok());

        // GIVEN 81 emoji
        let over: String = "🚀".repeat(81);
        assert!(validate_label(Some(&over)).is_err());
    }

    #[test]
    fn test_validate_events_all_known_ok() {
        let events = vec!["task.completed".into(), "task.failed".into()];
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
        let events = vec!["task.completed".into(), "unknown.event".into()];
        let err = validate_events(&events).unwrap_err();
        assert!(
            matches!(&err, NotificationConfigError::ValidationError(msg) if msg.contains("unknown.event"))
        );
    }
}
