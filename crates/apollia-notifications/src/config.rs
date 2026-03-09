use serde::Deserialize;

/// Configuration globale du système de notifications.
///
/// Chargée depuis `apollia.toml` via la section `[notifications]`.
/// Contient la liste des événements activés globalement et la liste des canaux.
#[derive(Debug, Clone, Deserialize)]
pub struct NotificationConfig {
    /// Événements activés globalement (ex: `["task.input_required", "task.failed"]`).
    ///
    /// Utilisé comme liste de référence pour les canaux configurés avec `events = ["*"]`
    /// ou sans liste d'événements spécifique.
    pub events: Vec<String>,
    /// Canaux de notification configurés.
    pub channels: Vec<ChannelConfig>,
}

/// Configuration d'un canal de notification individuel.
///
/// Correspond à une entrée `[[notifications.channels]]` dans `apollia.toml`.
#[derive(Debug, Clone, Deserialize)]
pub struct ChannelConfig {
    /// Identifiant unique du canal (ex: `"desktop"`, `"slack"`).
    pub id: String,
    /// Type de canal.
    #[serde(rename = "type")]
    pub kind: ChannelKind,
    /// Si `false`, le canal est ignoré même s'il est configuré.
    pub enabled: bool,
    /// Liste des événements à recevoir sur ce canal.
    ///
    /// - `None` → utilise la liste globale (`NotificationConfig.events`)
    /// - `Some(["*"])` → accepte tous les événements de la liste globale
    /// - `Some(liste)` → sous-ensemble d'événements spécifiques à ce canal
    pub events: Option<Vec<String>>,
    /// URL du webhook (uniquement pour le canal `webhook`).
    pub url: Option<String>,
}

/// Type de canal de notification.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChannelKind {
    /// Notification native OS via `notify-rust` (STORY-100).
    Desktop,
    /// Requête HTTP POST vers une URL configurée (STORY-101).
    Webhook,
    /// Server-Sent Events via le dashboard local (Sprint 9, déjà disponible).
    Sse,
}

/// Sévérité d'une notification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    /// Information — événement non bloquant.
    Info,
    /// Avertissement — intervention recommandée.
    Warning,
    /// Erreur — intervention requise.
    Error,
}

impl Severity {
    /// Retourne la représentation textuelle de la sévérité.
    pub fn as_str(&self) -> &'static str {
        match self {
            Severity::Info => "info",
            Severity::Warning => "warning",
            Severity::Error => "error",
        }
    }
}

/// Détermine si un canal accepte un événement donné selon sa configuration.
///
/// Logique de filtrage :
/// - `enabled == false` → `false` (canal désactivé)
/// - `channel_events == None` → `true` si l'événement est dans `global_events`
/// - `channel_events == Some(["*"])` → `true` si l'événement est dans `global_events`
/// - `channel_events == Some(liste)` → `true` si l'événement est dans `liste`
pub fn channel_accepts_event(
    enabled: bool,
    channel_events: &Option<Vec<String>>,
    event_name: &str,
    global_events: &[String],
) -> bool {
    if !enabled {
        return false;
    }
    match channel_events {
        None => global_events.iter().any(|e| e == event_name),
        Some(list) if list.iter().any(|e| e == "*") => {
            global_events.iter().any(|e| e == event_name)
        }
        Some(list) => list.iter().any(|e| e == event_name),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_channel_accepts_event_disabled() {
        // GIVEN canal désactivé
        // WHEN
        let result = channel_accepts_event(
            false,
            &Some(vec!["task.input_required".into()]),
            "task.input_required",
            &["task.input_required".into()],
        );
        // THEN
        assert!(!result);
    }

    #[test]
    fn test_channel_accepts_event_global_list() {
        // GIVEN canal sans liste propre → liste globale
        // WHEN
        let result = channel_accepts_event(
            true,
            &None,
            "task.input_required",
            &["task.input_required".into(), "task.failed".into()],
        );
        // THEN
        assert!(result);
    }

    #[test]
    fn test_channel_accepts_event_global_list_rejects_unknown() {
        // GIVEN canal sans liste propre, événement absent de la liste globale
        // WHEN
        let result = channel_accepts_event(
            true,
            &None,
            "agent.degraded",
            &["task.input_required".into(), "task.failed".into()],
        );
        // THEN
        assert!(!result);
    }

    #[test]
    fn test_channel_accepts_event_wildcard() {
        // GIVEN canal avec events=["*"]
        // WHEN
        let result = channel_accepts_event(
            true,
            &Some(vec!["*".into()]),
            "task.failed",
            &["task.input_required".into(), "task.failed".into()],
        );
        // THEN
        assert!(result);
    }

    #[test]
    fn test_channel_accepts_event_subset() {
        // GIVEN canal avec events=["task.input_required"]
        // WHEN
        let accepted = channel_accepts_event(
            true,
            &Some(vec!["task.input_required".into()]),
            "task.input_required",
            &[],
        );
        let rejected = channel_accepts_event(
            true,
            &Some(vec!["task.input_required".into()]),
            "agent.degraded",
            &[],
        );
        // THEN
        assert!(accepted);
        assert!(!rejected);
    }

    #[test]
    fn test_severity_as_str() {
        assert_eq!(Severity::Info.as_str(), "info");
        assert_eq!(Severity::Warning.as_str(), "warning");
        assert_eq!(Severity::Error.as_str(), "error");
    }
}
