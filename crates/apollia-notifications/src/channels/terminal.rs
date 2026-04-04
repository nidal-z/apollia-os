use async_trait::async_trait;
use std::io::Write as _;

use crate::{
    config::{channel_accepts_event, NotificationConfig, Severity},
    engine::{NotifError, Notification, NotificationChannel},
};

/// Émulateur de terminal détecté à la construction du canal.
#[derive(Debug, Clone, PartialEq)]
pub enum TerminalKind {
    /// iTerm2 — notification via séquence OSC 9.
    ITerm2,
    /// GNOME Terminal / VTE — notification via séquence OSC 777.
    GnomeVte,
    /// Fallback universel — sonnerie terminale (`\x07`).
    BellFallback,
}

/// Canal de notification terminal — détecte l'émulateur et envoie la séquence OSC appropriée.
///
/// - **iTerm2** : `ESC ] 9 ; <message> BEL`
/// - **GNOME/VTE** : `ESC ] 777 ; notify ; Apollia ; <message> BEL`
/// - **Fallback** : `BEL` (`\x07`)
///
/// Les séquences sont écrites sur `stderr` pour ne pas polluer la sortie standard.
/// Les notifications dont la sévérité est inférieure à [`TerminalChannel::min_severity`]
/// sont ignorées silencieusement.
pub struct TerminalChannel {
    id: String,
    enabled: bool,
    events: Option<Vec<String>>,
    /// Émulateur terminal détecté à la construction.
    pub kind: TerminalKind,
    /// Seuil de sévérité : les notifications moins critiques sont ignorées.
    pub min_severity: Severity,
}

impl TerminalChannel {
    /// Construit un `TerminalChannel` en détectant l'émulateur depuis les variables d'environnement.
    ///
    /// La détection se base sur `TERM_PROGRAM` et `VTE_VERSION` :
    /// - `TERM_PROGRAM=iTerm.app` → [`TerminalKind::ITerm2`]
    /// - `VTE_VERSION` présent ou `TERM_PROGRAM=vte` → [`TerminalKind::GnomeVte`]
    /// - Sinon → [`TerminalKind::BellFallback`]
    pub fn detect(
        id: impl Into<String>,
        enabled: bool,
        events: Option<Vec<String>>,
        min_severity: Severity,
    ) -> Self {
        Self {
            id: id.into(),
            enabled,
            events,
            kind: detect_terminal_kind(),
            min_severity,
        }
    }

    /// Construit la séquence OSC à écrire sur `stderr` selon le type d'émulateur.
    fn build_osc_sequence(&self, message: &str) -> String {
        match self.kind {
            TerminalKind::ITerm2 => format!("\x1b]9;{}\x07", message),
            TerminalKind::GnomeVte => format!("\x1b]777;notify;Apollia;{}\x07", message),
            TerminalKind::BellFallback => "\x07".to_string(),
        }
    }
}

/// Détecte le type d'émulateur terminal depuis les variables d'environnement.
fn detect_terminal_kind() -> TerminalKind {
    match std::env::var("TERM_PROGRAM").as_deref() {
        Ok("iTerm.app") => return TerminalKind::ITerm2,
        Ok("vte") => return TerminalKind::GnomeVte,
        _ => {}
    }
    if std::env::var("VTE_VERSION").is_ok() {
        return TerminalKind::GnomeVte;
    }
    TerminalKind::BellFallback
}

#[async_trait]
impl NotificationChannel for TerminalChannel {
    /// Retourne l'identifiant du canal tel que configuré dans `apollia.toml`.
    fn id(&self) -> &str {
        &self.id
    }

    /// Retourne `true` si ce canal est activé et accepte l'événement donné.
    fn accepts(&self, event: &str, config: &NotificationConfig) -> bool {
        channel_accepts_event(self.enabled, &self.events, event, &config.events)
    }

    /// Écrit la séquence OSC sur `stderr`.
    ///
    /// Retourne `Ok(())` immédiatement si la sévérité de la notification est inférieure
    /// à [`TerminalChannel::min_severity`], sans aucune écriture.
    async fn send(&self, notif: &Notification) -> Result<(), NotifError> {
        if notif.severity < self.min_severity {
            return Ok(());
        }

        let seq = self.build_osc_sequence(&notif.message);
        let stderr = std::io::stderr();
        let mut handle = stderr.lock();
        handle
            .write_all(seq.as_bytes())
            .map_err(|e| NotifError::Internal(e.to_string()))?;
        handle
            .flush()
            .map_err(|e| NotifError::Internal(e.to_string()))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Mutex;

    /// Mutex pour sérialiser les tests qui manipulent des variables d'environnement globales.
    ///
    /// Les tests de détection d'émulateur terminal lisent `TERM_PROGRAM` et `VTE_VERSION`.
    /// Sans sérialisation, les tests concurrents peuvent s'interférer mutuellement.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn make_notif(severity: Severity) -> Notification {
        Notification {
            event: "agent.inactivity".into(),
            timestamp: chrono::Utc::now(),
            task_id: None,
            agent: None,
            message: "Test notification".into(),
            metadata: HashMap::new(),
            severity,
        }
    }

    #[test]
    fn terminal_channel_detects_iterm2() {
        // GIVEN TERM_PROGRAM="iTerm.app"
        // WHEN detect()
        // THEN kind = ITerm2
        let _guard = ENV_LOCK.lock().expect("env lock");
        std::env::set_var("TERM_PROGRAM", "iTerm.app");
        std::env::remove_var("VTE_VERSION");
        let ch = TerminalChannel::detect("terminal", true, None, Severity::Info);
        std::env::remove_var("TERM_PROGRAM");

        assert_eq!(ch.kind, TerminalKind::ITerm2);
    }

    #[test]
    fn terminal_channel_detects_gnome_vte_via_term_program() {
        // GIVEN TERM_PROGRAM="vte"
        // WHEN detect()
        // THEN kind = GnomeVte
        let _guard = ENV_LOCK.lock().expect("env lock");
        std::env::set_var("TERM_PROGRAM", "vte");
        std::env::remove_var("VTE_VERSION");
        let ch = TerminalChannel::detect("terminal", true, None, Severity::Info);
        std::env::remove_var("TERM_PROGRAM");

        assert_eq!(ch.kind, TerminalKind::GnomeVte);
    }

    #[test]
    fn terminal_channel_bell_fallback_on_unknown() {
        // GIVEN TERM_PROGRAM non défini ou inconnu
        // WHEN detect()
        // THEN kind = BellFallback
        let _guard = ENV_LOCK.lock().expect("env lock");
        std::env::remove_var("TERM_PROGRAM");
        std::env::remove_var("VTE_VERSION");
        let ch = TerminalChannel::detect("terminal", true, None, Severity::Info);

        assert_eq!(ch.kind, TerminalKind::BellFallback);
    }

    #[tokio::test]
    async fn terminal_channel_filters_by_severity() {
        // GIVEN TerminalChannel { min_severity: Warning }
        // WHEN send(Notification { severity: Info })
        // THEN Ok(()) sans écriture (sévérité insuffisante)
        let ch = TerminalChannel {
            id: "terminal".into(),
            enabled: true,
            events: None,
            kind: TerminalKind::BellFallback,
            min_severity: Severity::Warning,
        };
        let notif = make_notif(Severity::Info);

        let result = ch.send(&notif).await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn terminal_channel_sends_when_severity_meets_threshold() {
        // GIVEN TerminalChannel { min_severity: Info }
        // WHEN send(Notification { severity: Warning })
        // THEN Ok(())
        let ch = TerminalChannel {
            id: "terminal".into(),
            enabled: true,
            events: None,
            kind: TerminalKind::BellFallback,
            min_severity: Severity::Info,
        };
        let notif = make_notif(Severity::Warning);

        let result = ch.send(&notif).await;

        assert!(result.is_ok());
    }

    #[test]
    fn terminal_channel_osc_iterm2_format() {
        // GIVEN ITerm2 kind
        // WHEN build_osc_sequence("hello")
        // THEN séquence OSC 9
        let ch = TerminalChannel {
            id: "terminal".into(),
            enabled: true,
            events: None,
            kind: TerminalKind::ITerm2,
            min_severity: Severity::Info,
        };
        let seq = ch.build_osc_sequence("hello");
        assert_eq!(seq, "\x1b]9;hello\x07");
    }

    #[test]
    fn terminal_channel_osc_gnome_format() {
        // GIVEN GnomeVte kind
        // WHEN build_osc_sequence("hello")
        // THEN séquence OSC 777
        let ch = TerminalChannel {
            id: "terminal".into(),
            enabled: true,
            events: None,
            kind: TerminalKind::GnomeVte,
            min_severity: Severity::Info,
        };
        let seq = ch.build_osc_sequence("hello");
        assert_eq!(seq, "\x1b]777;notify;Apollia;hello\x07");
    }

    #[test]
    fn terminal_channel_osc_bell_fallback_format() {
        // GIVEN BellFallback kind
        // WHEN build_osc_sequence(anything)
        // THEN bell character
        let ch = TerminalChannel {
            id: "terminal".into(),
            enabled: true,
            events: None,
            kind: TerminalKind::BellFallback,
            min_severity: Severity::Info,
        };
        let seq = ch.build_osc_sequence("ignored");
        assert_eq!(seq, "\x07");
    }

    #[test]
    fn severity_ordering() {
        // GIVEN severity variants
        // WHEN compared
        // THEN Info < Warning < Error
        assert!(Severity::Info < Severity::Warning);
        assert!(Severity::Warning < Severity::Error);
        assert!(Severity::Info < Severity::Error);
    }
}
