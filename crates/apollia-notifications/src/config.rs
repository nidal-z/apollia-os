use crate::{
    channels::{terminal::TerminalChannel, DesktopChannel, WebhookChannel},
    engine::NotificationChannel,
    WebhookChannelConfig,
};
use serde::Deserialize;

fn default_inactivity_timeout_secs() -> u64 {
    30
}

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
    /// Durée d'inactivité en secondes avant déclenchement d'une notification (défaut : 30).
    #[serde(default = "default_inactivity_timeout_secs")]
    pub inactivity_timeout_secs: u64,
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
    /// Secret HMAC-SHA256 pour signer les payloads sortants (uniquement pour `webhook`).
    ///
    /// Si absent, le webhook est envoyé sans header de signature.
    pub signing_secret: Option<String>,
    /// Sévérité minimale pour ce canal (uniquement pour le canal `terminal`).
    ///
    /// Les notifications dont la sévérité est inférieure à ce seuil sont silencieusement
    /// ignorées. Défaut : `Info` (toutes les notifications sont transmises).
    pub min_severity: Option<Severity>,
}

/// Type de canal de notification.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChannelKind {
    /// Notification native OS via `notify-rust`.
    Desktop,
    /// Requête HTTP POST vers une URL configurée.
    Webhook,
    /// Server-Sent Events via le dashboard local.
    Sse,
    /// Notification dans le terminal via séquences OSC (iTerm2, GNOME/VTE, ou bell).
    Terminal,
}

/// Niveau de sévérité d'une notification, du moins critique au plus critique.
///
/// L'ordre des variantes définit l'ordre naturel (Ord) utilisé pour le filtrage :
/// `Debug < Info < Warning < Error < Critical`.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, Deserialize,
)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    /// Diagnostic — destiné aux développeurs, non affiché aux utilisateurs en production.
    Debug = 0,
    /// Information — événement non bloquant.
    #[default]
    Info = 1,
    /// Avertissement — intervention recommandée.
    Warning = 2,
    /// Erreur — intervention requise.
    Error = 3,
    /// Critique — défaillance grave, intervention immédiate.
    Critical = 4,
}

impl Severity {
    /// Retourne la représentation textuelle de la sévérité.
    pub fn as_str(&self) -> &'static str {
        match self {
            Severity::Debug => "debug",
            Severity::Info => "info",
            Severity::Warning => "warning",
            Severity::Error => "error",
            Severity::Critical => "critical",
        }
    }
}

impl ChannelConfig {
    /// Retourne la sévérité minimale par défaut selon le type de canal.
    ///
    /// Ces valeurs sont appliquées quand `min_severity` est absent de la configuration :
    /// - `Desktop` → [`Severity::Error`] (bureau ne reçoit que les alertes graves)
    /// - `Webhook` → [`Severity::Info`] (webhook reçoit tout)
    /// - `Terminal` → [`Severity::Warning`] (terminal filtre les informations de bas niveau)
    /// - `Sse` → [`Severity::Info`] (SSE géré par le dashboard, pas de filtrage strict)
    pub fn default_min_severity(kind: &ChannelKind) -> Severity {
        match kind {
            ChannelKind::Desktop => Severity::Error,
            ChannelKind::Webhook => Severity::Info,
            ChannelKind::Terminal => Severity::Warning,
            ChannelKind::Sse => Severity::Info,
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

/// Erreur retournée par [`build_channels`] quand la configuration est invalide.
#[derive(Debug, thiserror::Error)]
pub enum NotifConfigError {
    /// Champ `url` absent pour un canal de type `webhook`.
    #[error("url manquante pour le canal webhook '{id}'")]
    MissingWebhookUrl {
        /// Identifiant du canal mal configuré.
        id: String,
    },
}

/// Instancie les canaux actifs à partir d'une liste de [`ChannelConfig`].
///
/// Itère sur `configs` en ordre de déclaration :
/// - `enabled = false` → canal ignoré silencieusement
/// - `type = "desktop"` → [`DesktopChannel`] ajouté
/// - `type = "webhook"` → [`WebhookChannel`] ajouté (erreur si `url` absent)
/// - `type = "terminal"` → [`TerminalChannel`] ajouté (détection automatique de l'émulateur)
/// - `type = "sse"` → ignoré (géré directement par le dashboard)
///
/// Retourne une erreur si un canal `webhook` actif n'a pas de `url`.
pub fn build_channels(
    configs: &[ChannelConfig],
) -> Result<Vec<Box<dyn NotificationChannel>>, NotifConfigError> {
    let mut channels: Vec<Box<dyn NotificationChannel>> = Vec::new();
    for cfg in configs {
        if !cfg.enabled {
            continue;
        }
        let min_severity = cfg
            .min_severity
            .unwrap_or_else(|| ChannelConfig::default_min_severity(&cfg.kind));

        match cfg.kind {
            ChannelKind::Desktop => {
                channels.push(Box::new(DesktopChannel::new(
                    cfg.id.clone(),
                    cfg.enabled,
                    cfg.events.clone(),
                    min_severity,
                )));
            }
            ChannelKind::Webhook => {
                let url = cfg
                    .url
                    .clone()
                    .ok_or_else(|| NotifConfigError::MissingWebhookUrl { id: cfg.id.clone() })?;
                channels.push(Box::new(WebhookChannel::new(WebhookChannelConfig {
                    id: cfg.id.clone(),
                    url,
                    enabled: cfg.enabled,
                    events: cfg.events.clone(),
                    signing_secret: cfg.signing_secret.clone(),
                    min_severity,
                })));
            }
            ChannelKind::Terminal => {
                channels.push(Box::new(TerminalChannel::detect(
                    cfg.id.clone(),
                    cfg.enabled,
                    cfg.events.clone(),
                    min_severity,
                )));
            }
            ChannelKind::Sse => {
                // Le canal SSE est géré directement par le dashboard.
            }
        }
    }
    Ok(channels)
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
    fn test_severity_ordering() {
        // GIVEN les 5 niveaux de sévérité
        // WHEN comparés
        // THEN Debug < Info < Warning < Error < Critical
        assert!(Severity::Debug < Severity::Info);
        assert!(Severity::Info < Severity::Warning);
        assert!(Severity::Warning < Severity::Error);
        assert!(Severity::Error < Severity::Critical);
    }

    #[test]
    fn test_severity_default_is_info() {
        // GIVEN / WHEN / THEN
        assert_eq!(Severity::default(), Severity::Info);
    }

    #[test]
    fn test_severity_as_str() {
        assert_eq!(Severity::Debug.as_str(), "debug");
        assert_eq!(Severity::Info.as_str(), "info");
        assert_eq!(Severity::Warning.as_str(), "warning");
        assert_eq!(Severity::Error.as_str(), "error");
        assert_eq!(Severity::Critical.as_str(), "critical");
    }

    #[test]
    fn test_default_min_severity_per_channel() {
        // GIVEN les 3 types de canaux configurables
        // WHEN on appelle default_min_severity()
        // THEN Desktop → Error, Webhook → Info, Terminal → Warning
        assert_eq!(
            ChannelConfig::default_min_severity(&ChannelKind::Desktop),
            Severity::Error
        );
        assert_eq!(
            ChannelConfig::default_min_severity(&ChannelKind::Webhook),
            Severity::Info
        );
        assert_eq!(
            ChannelConfig::default_min_severity(&ChannelKind::Terminal),
            Severity::Warning
        );
    }

    // build_channels retourne un DesktopChannel quand desktop enabled=true
    #[test]
    fn test_ac1_build_channels_desktop_enabled() {
        // GIVEN config avec desktop enabled=true
        let configs = vec![ChannelConfig {
            id: "desktop".into(),
            kind: ChannelKind::Desktop,
            enabled: true,
            events: None,
            url: None,
            signing_secret: None,
            min_severity: None,
        }];

        // WHEN
        let result = build_channels(&configs);

        // THEN 1 canal retourné
        let channels = result.expect("build_channels ne doit pas échouer");
        assert_eq!(channels.len(), 1);
        assert_eq!(channels[0].id(), "desktop");
    }

    // build_channels ignore les canaux avec enabled=false
    #[test]
    fn test_ac3_build_channels_disabled_skipped() {
        // GIVEN config avec webhook enabled=false
        let configs = vec![ChannelConfig {
            id: "slack".into(),
            kind: ChannelKind::Webhook,
            enabled: false,
            events: None,
            url: Some("https://hooks.slack.com/test".into()),
            signing_secret: None,
            min_severity: None,
        }];

        // WHEN
        let result = build_channels(&configs);

        // THEN 0 canaux retournés (canal désactivé ignoré silencieusement)
        let channels = result.expect("build_channels ne doit pas échouer");
        assert!(channels.is_empty());
    }

    // build_channels retourne une erreur si webhook n'a pas d'url
    #[test]
    fn test_ac5_build_channels_webhook_no_url_returns_error() {
        // GIVEN config type="webhook" sans champ url
        let configs = vec![ChannelConfig {
            id: "monitoring".into(),
            kind: ChannelKind::Webhook,
            enabled: true,
            events: None,
            url: None,
            signing_secret: None,
            min_severity: None,
        }];

        // WHEN
        let result = build_channels(&configs);

        // THEN Err(NotifConfigError::MissingWebhookUrl) avec l'id du canal
        let err = match result {
            Err(e) => e,
            Ok(_) => panic!("build_channels doit retourner une erreur pour webhook sans url"),
        };
        assert!(
            matches!(&err, NotifConfigError::MissingWebhookUrl { id } if id == "monitoring"),
            "attendu MissingWebhookUrl {{ id: monitoring }}, obtenu: {err:?}"
        );
        assert!(err.to_string().contains("monitoring"));
    }
}
