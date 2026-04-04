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

/// Valide un canal de notification avant insertion ou mise à jour.
///
/// Règles :
/// - Un canal de type `"webhook"` doit avoir une clé `"url"` non vide dans `config_json`.
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
            channel_type: "webhook".into(),
            enabled: true,
            config_json: config,
            events_json: None,
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
            channel_type: "desktop".into(),
            enabled: true,
            config_json: serde_json::json!({}),
            events_json: None,
            created_at: String::new(),
            updated_at: String::new(),
        };
        assert!(validate_channel(&ch).is_ok());
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
