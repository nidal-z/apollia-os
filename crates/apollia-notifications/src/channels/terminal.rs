use async_trait::async_trait;
use std::io::Write as _;

use crate::{
    config::{channel_accepts_event, NotificationConfig, Severity},
    engine::{NotifError, Notification, NotificationChannel},
};

/// Terminal emulator detected at channel construction.
#[derive(Debug, Clone, PartialEq)]
pub enum TerminalKind {
    /// iTerm2: notification via OSC 9 sequence.
    ITerm2,
    /// GNOME Terminal / VTE: notification via OSC 777 sequence.
    GnomeVte,
    /// Universal fallback: terminal bell (`\x07`).
    BellFallback,
}

/// Terminal notification channel: detects the emulator and sends the appropriate OSC sequence.
///
/// - **iTerm2**: `ESC ] 9 ; <message> BEL`
/// - **GNOME/VTE**: `ESC ] 777 ; notify ; Apollia ; <message> BEL`
/// - **Fallback**: `BEL` (`\x07`)
///
/// The sequences are written to `stderr` so they do not pollute standard output.
/// Notifications whose severity is below [`TerminalChannel::min_severity`] are
/// silently ignored.
pub struct TerminalChannel {
    id: String,
    enabled: bool,
    events: Option<Vec<String>>,
    /// Terminal emulator detected at construction.
    pub kind: TerminalKind,
    /// Severity threshold: less critical notifications are ignored.
    pub min_severity: Severity,
}

impl TerminalChannel {
    /// Builds a `TerminalChannel` by detecting the emulator from environment variables.
    ///
    /// Detection relies on `TERM_PROGRAM` and `VTE_VERSION`:
    /// - `TERM_PROGRAM=iTerm.app` -> [`TerminalKind::ITerm2`]
    /// - `VTE_VERSION` present or `TERM_PROGRAM=vte` -> [`TerminalKind::GnomeVte`]
    /// - Otherwise -> [`TerminalKind::BellFallback`]
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

    /// Builds the OSC sequence to write to `stderr` based on the emulator type.
    fn build_osc_sequence(&self, message: &str) -> String {
        match self.kind {
            TerminalKind::ITerm2 => format!("\x1b]9;{}\x07", message),
            TerminalKind::GnomeVte => format!("\x1b]777;notify;Apollia;{}\x07", message),
            TerminalKind::BellFallback => "\x07".to_string(),
        }
    }
}

/// Detects the terminal emulator type from environment variables.
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
    /// Returns the channel identifier as configured in `apollia.toml`.
    fn id(&self) -> &str {
        &self.id
    }

    /// Returns `true` if this channel is enabled and accepts the given event.
    fn accepts(&self, event: &str, config: &NotificationConfig) -> bool {
        channel_accepts_event(self.enabled, &self.events, event, &config.events)
    }

    /// Writes the OSC sequence to `stderr`.
    ///
    /// Returns `Ok(())` immediately, without any write, if the notification's
    /// severity is below [`TerminalChannel::min_severity`].
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

    /// Mutex to serialize the tests that manipulate global environment variables.
    ///
    /// The terminal emulator detection tests read `TERM_PROGRAM` and
    /// `VTE_VERSION`. Without serialization, concurrent tests can interfere with
    /// each other.
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
        // GIVEN TERM_PROGRAM unset or unknown
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
        // THEN Ok(()) with no write (severity too low)
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
        // THEN OSC 9 sequence
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
        // THEN OSC 777 sequence
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
