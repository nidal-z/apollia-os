use async_trait::async_trait;
#[cfg(not(target_os = "macos"))]
use notify_rust::Notification as OsNotif;

use crate::{
    config::{channel_accepts_event, NotificationConfig, Severity},
    engine::{NotifError, Notification, NotificationChannel},
};

/// Canal de notification desktop OS (macOS NSUserNotification, Linux D-Bus/libnotify).
///
/// Envoie des notifications natives via `notify-rust` v4. Pour les événements
/// `task.input_required` sur Linux (XDG), trois actions inline sont proposées :
/// - **✔ Approuver** → `POST /api/v1/tasks/{id}/resume { approved: true }`
/// - **✗ Rejeter**   → `POST /api/v1/tasks/{id}/resume { approved: false }`
/// - **Inspecter**   → ouvre le dashboard dans le navigateur par défaut
///
/// Pour tous les autres événements, une notification simple est affichée ;
/// un clic ouvre le dashboard local (Linux uniquement — `wait_for_action`).
///
/// La méthode [`send`] retourne `Ok(())` immédiatement : l'attente de l'action
/// utilisateur s'exécute dans un [`tokio::task::spawn_blocking`] non-bloquant.
///
/// En CI headless (Linux sans `DISPLAY` ni `DBUS_SESSION_BUS_ADDRESS`), les
/// notifications sont ignorées silencieusement pour éviter tout panic.
pub struct DesktopChannel {
    id: String,
    enabled: bool,
    events: Option<Vec<String>>,
}

impl DesktopChannel {
    /// Crée un canal desktop avec l'identifiant et la configuration donnés.
    pub fn new(id: impl Into<String>, enabled: bool, events: Option<Vec<String>>) -> Self {
        Self {
            id: id.into(),
            enabled,
            events,
        }
    }
}

impl Default for DesktopChannel {
    fn default() -> Self {
        Self::new("desktop", true, None)
    }
}

#[async_trait]
impl NotificationChannel for DesktopChannel {
    /// Retourne l'identifiant du canal tel que configuré dans `apollia.toml`.
    fn id(&self) -> &str {
        &self.id
    }

    /// Retourne `true` si ce canal est activé et accepte l'événement donné.
    fn accepts(&self, event: &str, config: &NotificationConfig) -> bool {
        channel_accepts_event(self.enabled, &self.events, event, &config.events)
    }

    /// Affiche une notification OS native.
    ///
    /// Pour `task.input_required` : ajoute les actions Approuver / Rejeter / Inspecter
    /// (Linux/XDG uniquement via `wait_for_action`).
    /// Pour les autres événements : notification simple, clic → dashboard.
    ///
    /// Retourne `Ok(())` immédiatement. L'attente d'une action utilisateur tourne
    /// dans un `spawn_blocking` en arrière-plan. Les erreurs OS sont loggées en
    /// `warn!` sans propagation.
    async fn send(&self, notif: &Notification) -> Result<(), NotifError> {
        // Dégradation gracieuse en CI headless (Linux sans display)
        #[cfg(target_os = "linux")]
        if std::env::var("DISPLAY").is_err() && std::env::var("DBUS_SESSION_BUS_ADDRESS").is_err() {
            tracing::debug!(
                event = %notif.event,
                "DesktopChannel : pas de display/D-Bus — notification ignorée (CI headless)"
            );
            return Ok(());
        }

        let agent_name = notif.agent.as_deref().unwrap_or("runtime").to_string();
        let summary = format!("Apollia OS — {agent_name}");
        let body = notif.message.clone();
        let is_hitl = notif.event == "task.input_required";
        let resume_url = notif
            .metadata
            .get("resume_url")
            .cloned()
            .unwrap_or_default();
        let inspect_url = notif
            .metadata
            .get("inspect_url")
            .cloned()
            .unwrap_or_default();
        let dashboard_url = notif
            .metadata
            .get("dashboard_url")
            .cloned()
            .unwrap_or_default();
        let severity = notif.severity;

        tokio::task::spawn_blocking(move || {
            show_os_notification(
                summary,
                body,
                severity,
                is_hitl,
                resume_url,
                inspect_url,
                dashboard_url,
            );
        });

        Ok(())
    }
}

/// Affiche la notification OS et gère les actions (appelé dans `spawn_blocking`).
///
/// Séparé de `send()` pour être testable et pour isoler le code bloquant.
fn show_os_notification(
    summary: String,
    body: String,
    severity: Severity,
    is_hitl: bool,
    resume_url: String,
    inspect_url: String,
    dashboard_url: String,
) {
    #[cfg(all(unix, not(target_os = "macos")))]
    show_os_notification_xdg(
        summary,
        body,
        severity,
        is_hitl,
        resume_url,
        inspect_url,
        dashboard_url,
    );

    // macOS : osascript est la seule API fiable pour les binaires CLI sans bundle ID.
    // mac-notification-sys (défaut notify-rust) nécessite un bundle identifier
    // qui n'existe pas pour un binaire compilé directement.
    #[cfg(target_os = "macos")]
    show_os_notification_macos(summary, body);

    // Windows et autres plateformes non-Unix : fallback notify-rust simple.
    #[cfg(not(any(all(unix, not(target_os = "macos")), target_os = "macos")))]
    show_os_notification_fallback(summary, body);

    // Paramètres non utilisés sur macOS/Windows — silence les warnings.
    #[cfg(not(all(unix, not(target_os = "macos"))))]
    {
        let _ = (severity, is_hitl, resume_url, inspect_url, dashboard_url);
    }
}

/// Implémentation XDG (Linux) avec urgency, actions inline et `wait_for_action`.
#[cfg(all(unix, not(target_os = "macos")))]
fn show_os_notification_xdg(
    summary: String,
    body: String,
    severity: Severity,
    is_hitl: bool,
    resume_url: String,
    inspect_url: String,
    dashboard_url: String,
) {
    let urgency = severity_to_urgency(severity);

    if is_hitl {
        let mut os_notif = OsNotif::new();
        os_notif
            .summary(&summary)
            .body(&body)
            .urgency(urgency)
            .icon("dialog-warning")
            .action("approve", "✔ Approuver")
            .action("reject", "✗ Rejeter")
            .action("inspect", "Inspecter");

        let handle = match os_notif.show() {
            Ok(h) => h,
            Err(err) => {
                tracing::warn!(
                    error = %err,
                    "DesktopChannel : affichage de notification OS impossible"
                );
                return;
            }
        };

        handle.wait_for_action(|action| match action {
            "approve" => {
                let payload = serde_json::json!({ "approved": true });
                if let Err(err) = ureq::post(&resume_url).send_json(payload) {
                    tracing::warn!(
                        error = %err,
                        resume_url = %resume_url,
                        "DesktopChannel : POST resume (approve) échoué"
                    );
                }
            }
            "reject" => {
                let payload = serde_json::json!({
                    "approved": false,
                    "reason": "Refusé depuis la notification"
                });
                if let Err(err) = ureq::post(&resume_url).send_json(payload) {
                    tracing::warn!(
                        error = %err,
                        resume_url = %resume_url,
                        "DesktopChannel : POST resume (reject) échoué"
                    );
                }
            }
            "inspect" => {
                if let Err(err) = open::that(&inspect_url) {
                    tracing::warn!(
                        error = %err,
                        inspect_url = %inspect_url,
                        "DesktopChannel : ouverture du browser impossible"
                    );
                }
            }
            _ => {}
        });
    } else {
        let mut os_notif = OsNotif::new();
        os_notif
            .summary(&summary)
            .body(&body)
            .urgency(urgency)
            .icon("dialog-information");

        let handle = match os_notif.show() {
            Ok(h) => h,
            Err(err) => {
                tracing::warn!(
                    error = %err,
                    "DesktopChannel : affichage de notification OS impossible"
                );
                return;
            }
        };

        handle.wait_for_action(|action| {
            if action != "__closed" {
                if let Err(err) = open::that(&dashboard_url) {
                    tracing::warn!(
                        error = %err,
                        "DesktopChannel : ouverture du dashboard impossible"
                    );
                }
            }
        });
    }
}

/// Implémentation macOS via `osascript` — fiable pour les binaires CLI sans bundle ID.
///
/// `mac-notification-sys` (utilisé par `notify-rust` 4 par défaut sur macOS) exige un
/// bundle identifier CFBundleIdentifier présent dans Info.plist. Les binaires compilés
/// directement ne l'ont pas, ce qui provoque un échec silencieux. `osascript` contourne
/// cette contrainte et fonctionne depuis n'importe quel processus Terminal.
#[cfg(target_os = "macos")]
fn show_os_notification_macos(summary: String, body: String) {
    // Échappe les guillemets internes pour l'AppleScript.
    let body_esc = body.replace('\\', "\\\\").replace('"', "\\\"");
    let summary_esc = summary.replace('\\', "\\\\").replace('"', "\\\"");
    let script = format!("display notification \"{body_esc}\" with title \"{summary_esc}\"");

    match std::process::Command::new("osascript")
        .arg("-e")
        .arg(&script)
        .output()
    {
        Ok(out) if out.status.success() => {}
        Ok(out) => {
            let stderr = String::from_utf8_lossy(&out.stderr);
            tracing::warn!(
                stderr = %stderr,
                "DesktopChannel : osascript retourné non-zéro"
            );
        }
        Err(err) => {
            tracing::warn!(
                error = %err,
                "DesktopChannel : impossible de lancer osascript"
            );
        }
    }
}

/// Implémentation de repli pour Windows et autres plateformes non-Unix.
#[cfg(not(any(all(unix, not(target_os = "macos")), target_os = "macos")))]
fn show_os_notification_fallback(summary: String, body: String) {
    let mut os_notif = OsNotif::new();
    os_notif.summary(&summary).body(&body);

    if let Err(err) = os_notif.show() {
        tracing::warn!(
            error = %err,
            "DesktopChannel : affichage de notification OS impossible"
        );
    }
}

/// Convertit une [`Severity`] en niveau d'urgence `notify-rust` (Linux/XDG uniquement).
#[cfg(all(unix, not(target_os = "macos")))]
fn severity_to_urgency(s: Severity) -> notify_rust::Urgency {
    use notify_rust::Urgency;
    match s {
        Severity::Error => Urgency::Critical,
        Severity::Warning => Urgency::Normal,
        Severity::Info => Urgency::Low,
    }
}

/// Version cross-platform de `severity_to_urgency` pour les tests.
///
/// Retourne une représentation textuelle de la sévérité utilisable sur toutes les plateformes.
pub fn severity_as_urgency_str(s: Severity) -> &'static str {
    match s {
        Severity::Error => "critical",
        Severity::Warning => "normal",
        Severity::Info => "low",
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    fn make_notif(
        event: &str,
        task_id: Option<&str>,
        metadata: HashMap<String, String>,
    ) -> Notification {
        Notification {
            event: event.into(),
            timestamp: chrono::Utc::now(),
            task_id: task_id.map(String::from),
            agent: Some("test-agent".into()),
            message: "Message de test".into(),
            metadata,
            severity: Severity::Warning,
        }
    }

    #[test]
    fn test_severity_to_urgency_mapping() {
        // GIVEN / WHEN / THEN — vérification du mapping textuel cross-platform
        assert_eq!(severity_as_urgency_str(Severity::Error), "critical");
        assert_eq!(severity_as_urgency_str(Severity::Warning), "normal");
        assert_eq!(severity_as_urgency_str(Severity::Info), "low");
    }

    #[test]
    fn test_desktop_channel_id() {
        // GIVEN
        let channel = DesktopChannel::default();
        // WHEN / THEN
        assert_eq!(channel.id(), "desktop");
    }

    #[test]
    fn test_desktop_channel_accepts_global_events() {
        // GIVEN canal activé sans liste propre → délègue à la liste globale
        let channel = DesktopChannel::new("desktop", true, None);
        let config = NotificationConfig {
            events: vec!["task.input_required".into(), "task.failed".into()],
            channels: vec![],
            inactivity_timeout_secs: 30,
        };
        // WHEN / THEN — événements de la liste globale acceptés
        assert!(channel.accepts("task.input_required", &config));
        assert!(channel.accepts("task.failed", &config));
        // Événement absent de la liste globale → refusé
        assert!(!channel.accepts("task.completed", &config));
    }

    #[test]
    fn test_desktop_channel_accepts_disabled() {
        // GIVEN canal désactivé
        let channel = DesktopChannel::new("desktop", false, None);
        let config = NotificationConfig {
            events: vec!["task.input_required".into()],
            channels: vec![],
            inactivity_timeout_secs: 30,
        };
        // WHEN / THEN — aucun événement accepté si canal désactivé
        assert!(!channel.accepts("task.input_required", &config));
    }

    #[test]
    fn test_desktop_channel_accepts_per_channel_events() {
        // GIVEN canal avec liste propre restreinte à un sous-ensemble
        let channel =
            DesktopChannel::new("desktop", true, Some(vec!["task.input_required".into()]));
        let config = NotificationConfig {
            events: vec!["task.input_required".into(), "task.failed".into()],
            channels: vec![],
            inactivity_timeout_secs: 30,
        };
        // WHEN / THEN
        assert!(channel.accepts("task.input_required", &config));
        assert!(!channel.accepts("task.failed", &config));
    }

    #[tokio::test]
    async fn test_ac6_send_is_nonblocking() {
        // GIVEN un canal desktop et une notification task.failed
        let channel = DesktopChannel::default();
        let notif = make_notif("task.failed", Some("t-test"), HashMap::new());

        // WHEN — send() appelé depuis un contexte async
        let result = channel.send(&notif).await;

        // THEN — retourne immédiatement Ok(()) sans bloquer
        // Sur CI headless Linux : return anticipé avant spawn_blocking
        // Sur macOS : spawn_blocking lancé en arrière-plan, send() retourne
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_ac6_send_hitl_is_nonblocking() {
        // GIVEN une notification task.input_required (HITL)
        let channel = DesktopChannel::default();
        let mut metadata = HashMap::new();
        metadata.insert(
            "resume_url".into(),
            "http://localhost:7771/api/v1/tasks/t-001/resume".into(),
        );
        metadata.insert(
            "inspect_url".into(),
            "http://localhost:7771/dashboard#tasks/t-001".into(),
        );
        let notif = make_notif("task.input_required", Some("t-001"), metadata);

        // WHEN
        let result = channel.send(&notif).await;

        // THEN — retourne Ok(()) immédiatement même pour les notifications HITL
        assert!(result.is_ok());
    }
}
