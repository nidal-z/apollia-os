use async_trait::async_trait;
#[cfg(not(target_os = "macos"))]
use notify_rust::Notification as OsNotif;

use crate::{
    config::{channel_accepts_event, NotificationConfig, Severity},
    engine::{NotifError, Notification, NotificationChannel},
};

/// Desktop OS notification channel (macOS NSUserNotification, Linux D-Bus/libnotify).
///
/// Sends native notifications via `notify-rust` v4. For `task.input_required`
/// events on Linux (XDG), three inline actions are offered:
/// - **Approve** -> `POST /api/v1/tasks/{id}/resume { approved: true }`
/// - **Reject**  -> `POST /api/v1/tasks/{id}/resume { approved: false }`
/// - **Inspect** -> opens the dashboard in the default browser
///
/// For all other events, a plain notification is shown; a click opens the local
/// dashboard (Linux only, via `wait_for_action`).
///
/// The [`send`] method returns `Ok(())` immediately: waiting for the user action
/// runs in a non-blocking [`tokio::task::spawn_blocking`].
///
/// In headless CI (Linux without `DISPLAY` or `DBUS_SESSION_BUS_ADDRESS`),
/// notifications are silently ignored to avoid any panic.
pub struct DesktopChannel {
    id: String,
    enabled: bool,
    events: Option<Vec<String>>,
    /// Severity threshold: less critical notifications are silently ignored.
    min_severity: Severity,
}

impl DesktopChannel {
    /// Creates a desktop channel with the given identifier, configuration, and severity threshold.
    pub fn new(
        id: impl Into<String>,
        enabled: bool,
        events: Option<Vec<String>>,
        min_severity: Severity,
    ) -> Self {
        Self {
            id: id.into(),
            enabled,
            events,
            min_severity,
        }
    }
}

impl Default for DesktopChannel {
    fn default() -> Self {
        Self::new("desktop", true, None, Severity::Error)
    }
}

#[async_trait]
impl NotificationChannel for DesktopChannel {
    /// Returns the channel identifier as configured in `apollia.toml`.
    fn id(&self) -> &str {
        &self.id
    }

    /// Returns `true` if this channel is enabled and accepts the given event.
    fn accepts(&self, event: &str, config: &NotificationConfig) -> bool {
        channel_accepts_event(self.enabled, &self.events, event, &config.events)
    }

    /// Shows a native OS notification.
    ///
    /// For `task.input_required`: adds the Approve / Reject / Inspect actions
    /// (Linux/XDG only, via `wait_for_action`).
    /// For other events: a plain notification, click opens the dashboard.
    ///
    /// Returns `Ok(())` immediately. Waiting for a user action runs in a
    /// background `spawn_blocking`. OS errors are logged at `warn!` without
    /// propagation.
    async fn send(&self, notif: &Notification) -> Result<(), NotifError> {
        if notif.severity < self.min_severity {
            return Ok(());
        }

        // Graceful degradation in headless CI (Linux without a display).
        #[cfg(target_os = "linux")]
        if std::env::var("DISPLAY").is_err() && std::env::var("DBUS_SESSION_BUS_ADDRESS").is_err() {
            tracing::debug!(
                event = %notif.event,
                "DesktopChannel : pas de display/D-Bus - notification ignorée (CI headless)"
            );
            return Ok(());
        }

        let agent_name = notif.agent.as_deref().unwrap_or("runtime").to_string();
        let summary = format!("Apollia OS - {agent_name}");
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

        let params = OsNotifParams {
            summary,
            body,
            severity,
            is_hitl,
            resume_url,
            inspect_url,
            dashboard_url,
        };

        tokio::task::spawn_blocking(move || {
            show_os_notification(params);
        });

        Ok(())
    }
}

/// Owned parameters of an OS notification, grouped into a single bundle to cross
/// the `spawn_blocking` boundary and the platform-specific implementations.
struct OsNotifParams {
    summary: String,
    body: String,
    severity: Severity,
    is_hitl: bool,
    resume_url: String,
    inspect_url: String,
    dashboard_url: String,
}

/// Shows the OS notification and handles the actions (called inside `spawn_blocking`).
///
/// Separated from `send()` to be testable and to isolate the blocking code.
fn show_os_notification(params: OsNotifParams) {
    #[cfg(all(unix, not(target_os = "macos")))]
    show_os_notification_xdg(params);

    // macOS: osascript is the only reliable API for CLI binaries without a bundle ID.
    // mac-notification-sys (notify-rust's default) requires a bundle identifier,
    // which does not exist for a directly compiled binary.
    #[cfg(target_os = "macos")]
    show_os_notification_macos(params.summary, params.body);

    // Windows and other non-Unix platforms: plain notify-rust fallback.
    #[cfg(not(any(all(unix, not(target_os = "macos")), target_os = "macos")))]
    show_os_notification_fallback(params.summary, params.body);

    // Parameters unused on macOS/Windows: silence the warnings.
    #[cfg(not(all(unix, not(target_os = "macos"))))]
    {
        let _ = (
            params.severity,
            params.is_hitl,
            params.resume_url,
            params.inspect_url,
            params.dashboard_url,
        );
    }
}

/// XDG (Linux) implementation with urgency, inline actions, and `wait_for_action`.
#[cfg(all(unix, not(target_os = "macos")))]
fn show_os_notification_xdg(params: OsNotifParams) {
    let urgency = severity_to_urgency(params.severity);

    if params.is_hitl {
        show_os_notification_xdg_hitl(&params, urgency);
    } else {
        show_os_notification_xdg_plain(&params, urgency);
    }
}

/// Shows a notification `handle` or logs the failure, returning `None` when the
/// display failed.
#[cfg(all(unix, not(target_os = "macos")))]
fn show_or_warn(os_notif: &OsNotif) -> Option<notify_rust::NotificationHandle> {
    match os_notif.show() {
        Ok(h) => Some(h),
        Err(err) => {
            tracing::warn!(
                error = %err,
                "DesktopChannel : affichage de notification OS impossible"
            );
            None
        }
    }
}

/// HITL variant: approve / reject / inspect actions.
#[cfg(all(unix, not(target_os = "macos")))]
fn show_os_notification_xdg_hitl(params: &OsNotifParams, urgency: notify_rust::Urgency) {
    let mut os_notif = OsNotif::new();
    os_notif
        .summary(&params.summary)
        .body(&params.body)
        .urgency(urgency)
        .icon("dialog-warning")
        .action("approve", "✔ Approuver")
        .action("reject", "✗ Rejeter")
        .action("inspect", "Inspecter");

    let Some(handle) = show_or_warn(&os_notif) else {
        return;
    };

    let resume_url = params.resume_url.clone();
    let inspect_url = params.inspect_url.clone();
    handle.wait_for_action(|action| match action {
        "approve" => post_resume_decision(&resume_url, true),
        "reject" => post_resume_decision(&resume_url, false),
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
}

/// POSTs the HITL approval decision to `resume_url`.
#[cfg(all(unix, not(target_os = "macos")))]
fn post_resume_decision(resume_url: &str, approved: bool) {
    let payload = if approved {
        serde_json::json!({ "approved": true })
    } else {
        serde_json::json!({
            "approved": false,
            "reason": "Refusé depuis la notification"
        })
    };
    let label = if approved { "approve" } else { "reject" };
    if let Err(err) = ureq::post(resume_url).send_json(payload) {
        tracing::warn!(
            error = %err,
            resume_url = %resume_url,
            "DesktopChannel : POST resume ({label}) échoué"
        );
    }
}

/// Plain variant: a click opens the dashboard.
#[cfg(all(unix, not(target_os = "macos")))]
fn show_os_notification_xdg_plain(params: &OsNotifParams, urgency: notify_rust::Urgency) {
    let mut os_notif = OsNotif::new();
    os_notif
        .summary(&params.summary)
        .body(&params.body)
        .urgency(urgency)
        .icon("dialog-information");

    let Some(handle) = show_or_warn(&os_notif) else {
        return;
    };

    let dashboard_url = params.dashboard_url.clone();
    handle.wait_for_action(move |action| {
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

/// macOS implementation via `osascript`: reliable for CLI binaries without a bundle ID.
///
/// `mac-notification-sys` (used by `notify-rust` 4 by default on macOS) requires
/// a CFBundleIdentifier present in Info.plist. Directly compiled binaries do not
/// have one, which causes a silent failure. `osascript` works around this
/// constraint and works from any Terminal process.
#[cfg(target_os = "macos")]
fn show_os_notification_macos(summary: String, body: String) {
    // Escape inner quotes for the AppleScript.
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

/// Fallback implementation for Windows and other non-Unix platforms.
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

/// Converts a [`Severity`] into a `notify-rust` urgency level (Linux/XDG only).
#[cfg(all(unix, not(target_os = "macos")))]
fn severity_to_urgency(s: Severity) -> notify_rust::Urgency {
    use notify_rust::Urgency;
    match s {
        Severity::Critical | Severity::Error => Urgency::Critical,
        Severity::Warning => Urgency::Normal,
        Severity::Info | Severity::Debug => Urgency::Low,
    }
}

/// Cross-platform version of `severity_to_urgency` for tests.
///
/// Returns a textual representation of the severity usable on all platforms.
pub fn severity_as_urgency_str(s: Severity) -> &'static str {
    match s {
        Severity::Critical | Severity::Error => "critical",
        Severity::Warning => "normal",
        Severity::Info | Severity::Debug => "low",
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
        // GIVEN / WHEN / THEN check the cross-platform textual mapping
        assert_eq!(severity_as_urgency_str(Severity::Critical), "critical");
        assert_eq!(severity_as_urgency_str(Severity::Error), "critical");
        assert_eq!(severity_as_urgency_str(Severity::Warning), "normal");
        assert_eq!(severity_as_urgency_str(Severity::Info), "low");
        assert_eq!(severity_as_urgency_str(Severity::Debug), "low");
    }

    #[test]
    fn test_desktop_channel_id() {
        // GIVEN
        let channel = DesktopChannel::default();
        // WHEN / THEN
        assert_eq!(channel.id(), "desktop");
    }

    #[test]
    fn test_desktop_default_min_severity_is_error() {
        // GIVEN the default desktop channel
        // WHEN / THEN
        let channel = DesktopChannel::default();
        assert_eq!(channel.min_severity, Severity::Error);
    }

    #[test]
    fn test_desktop_channel_accepts_global_events() {
        // GIVEN an enabled channel without its own list, delegating to the global list
        let channel = DesktopChannel::new("desktop", true, None, Severity::Error);
        let config = NotificationConfig {
            events: vec!["task.input_required".into(), "task.failed".into()],
            channels: vec![],
            inactivity_timeout_secs: 30,
        };
        // WHEN / THEN events from the global list are accepted
        assert!(channel.accepts("task.input_required", &config));
        assert!(channel.accepts("task.failed", &config));
        // Event absent from the global list is rejected.
        assert!(!channel.accepts("task.completed", &config));
    }

    #[test]
    fn test_desktop_channel_accepts_disabled() {
        // GIVEN a disabled channel
        let channel = DesktopChannel::new("desktop", false, None, Severity::Error);
        let config = NotificationConfig {
            events: vec!["task.input_required".into()],
            channels: vec![],
            inactivity_timeout_secs: 30,
        };
        // WHEN / THEN no event accepted when the channel is disabled
        assert!(!channel.accepts("task.input_required", &config));
    }

    #[test]
    fn test_desktop_channel_accepts_per_channel_events() {
        // GIVEN a channel with its own list restricted to a subset
        let channel = DesktopChannel::new(
            "desktop",
            true,
            Some(vec!["task.input_required".into()]),
            Severity::Error,
        );
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
    async fn test_desktop_filters_info_when_min_severity_is_error() {
        // GIVEN a desktop channel with min_severity = Error
        let channel = DesktopChannel::new("desktop", true, None, Severity::Error);
        let notif = make_notif("task.completed", None, HashMap::new());
        // notif.severity = Severity::Warning (see make_notif) < Error, so filtered.

        // WHEN
        let notif_info = Notification {
            severity: Severity::Info,
            ..notif
        };
        let result = channel.send(&notif_info).await;

        // THEN immediate Ok(()), nothing sent
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_desktop_sends_when_severity_meets_threshold() {
        // GIVEN a desktop channel with min_severity = Error, an Error notification
        let channel = DesktopChannel::new("desktop", true, None, Severity::Error);
        let notif = Notification {
            event: "task.failed".into(),
            timestamp: chrono::Utc::now(),
            task_id: Some("t-001".into()),
            agent: Some("test-agent".into()),
            message: "Erreur".into(),
            metadata: HashMap::new(),
            severity: Severity::Error,
        };

        // WHEN
        let result = channel.send(&notif).await;

        // THEN Ok(()) (send launched, non-blocking return)
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_send_is_nonblocking() {
        // GIVEN a desktop channel with min_severity = Info and a task.failed notification
        let channel = DesktopChannel::new("desktop", true, None, Severity::Info);
        let notif = make_notif("task.failed", Some("t-test"), HashMap::new());

        // WHEN send() called from an async context
        let result = channel.send(&notif).await;

        // THEN returns Ok(()) immediately without blocking.
        // On headless Linux CI: early return before spawn_blocking.
        // On macOS: spawn_blocking launched in the background, send() returns.
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_send_hitl_is_nonblocking() {
        // GIVEN a task.input_required notification (HITL)
        let channel = DesktopChannel::new("desktop", true, None, Severity::Info);
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

        // THEN returns Ok(()) immediately even for HITL notifications
        assert!(result.is_ok());
    }
}
