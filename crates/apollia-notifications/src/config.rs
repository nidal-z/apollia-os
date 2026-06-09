use crate::{
    channels::{terminal::TerminalChannel, DesktopChannel, WebhookChannel},
    engine::NotificationChannel,
    WebhookChannelConfig,
};
use serde::Deserialize;

fn default_inactivity_timeout_secs() -> u64 {
    30
}

/// Global configuration of the notification system.
///
/// Loaded from `apollia.toml` via the `[notifications]` section. Holds the list
/// of globally enabled events and the list of channels.
#[derive(Debug, Clone, Deserialize)]
pub struct NotificationConfig {
    /// Globally enabled events (e.g. `["task.input_required", "task.failed"]`).
    ///
    /// Used as the reference list for channels configured with `events = ["*"]`
    /// or without a specific event list.
    pub events: Vec<String>,
    /// Configured notification channels.
    pub channels: Vec<ChannelConfig>,
    /// Inactivity duration in seconds before triggering a notification (default: 30).
    #[serde(default = "default_inactivity_timeout_secs")]
    pub inactivity_timeout_secs: u64,
}

/// Configuration of an individual notification channel.
///
/// Maps to a `[[notifications.channels]]` entry in `apollia.toml`.
#[derive(Debug, Clone, Deserialize)]
pub struct ChannelConfig {
    /// Unique channel identifier (e.g. `"desktop"`, `"slack"`).
    pub id: String,
    /// Channel type.
    #[serde(rename = "type")]
    pub kind: ChannelKind,
    /// If `false`, the channel is ignored even if configured.
    pub enabled: bool,
    /// List of events to receive on this channel.
    ///
    /// - `None` -> uses the global list (`NotificationConfig.events`)
    /// - `Some(["*"])` -> accepts all events from the global list
    /// - `Some(list)` -> a subset of events specific to this channel
    pub events: Option<Vec<String>>,
    /// Webhook URL (only for the `webhook` channel).
    pub url: Option<String>,
    /// HMAC-SHA256 secret to sign outgoing payloads (only for `webhook`).
    ///
    /// If absent, the webhook is sent without a signature header.
    pub signing_secret: Option<String>,
    /// Minimum severity for this channel (only for the `terminal` channel).
    ///
    /// Notifications whose severity is below this threshold are silently
    /// ignored. Default: `Info` (all notifications are forwarded).
    pub min_severity: Option<Severity>,
    /// Minimum interval between two notifications for the same
    /// `(channel, event)` pair, in seconds. `0` = no throttling.
    ///
    /// Applied by [`crate::engine::NotificationEngine`] before dispatch.
    /// Dropped notifications are counted and summarized in a recap emitted at
    /// the end of the window.
    #[serde(default)]
    pub min_interval_seconds: u32,
}

/// Notification channel type.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChannelKind {
    /// Native OS notification via `notify-rust`.
    Desktop,
    /// HTTP POST request to a configured URL.
    Webhook,
    /// Terminal notification via OSC sequences (iTerm2, GNOME/VTE, or bell).
    Terminal,
}

/// Severity level of a notification, from least to most critical.
///
/// The variant order defines the natural ordering (Ord) used for filtering:
/// `Debug < Info < Warning < Error < Critical`.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, Deserialize,
)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    /// Diagnostic: for developers, not shown to users in production.
    Debug = 0,
    /// Information: a non-blocking event.
    #[default]
    Info = 1,
    /// Warning: intervention recommended.
    Warning = 2,
    /// Error: intervention required.
    Error = 3,
    /// Critical: serious failure, immediate intervention.
    Critical = 4,
}

impl Severity {
    /// Returns the textual representation of the severity.
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
    /// Returns the default minimum severity per channel type.
    ///
    /// These values apply when `min_severity` is absent from the configuration:
    /// - `Desktop` -> [`Severity::Error`] (desktop only receives serious alerts)
    /// - `Webhook` -> [`Severity::Info`] (webhook receives everything)
    /// - `Terminal` -> [`Severity::Warning`] (terminal filters out low-level info)
    pub fn default_min_severity(kind: &ChannelKind) -> Severity {
        match kind {
            ChannelKind::Desktop => Severity::Error,
            ChannelKind::Webhook => Severity::Info,
            ChannelKind::Terminal => Severity::Warning,
        }
    }
}

/// Determines whether a channel accepts a given event based on its configuration.
///
/// Filtering logic:
/// - `enabled == false` -> `false` (disabled channel)
/// - `channel_events == None` -> `true` if the event is in `global_events`
/// - `channel_events == Some(["*"])` -> `true` if the event is in `global_events`
/// - `channel_events == Some(list)` -> `true` if the event is in `list`
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

/// Error returned by [`build_channels`] when the configuration is invalid.
#[derive(Debug, thiserror::Error)]
pub enum NotifConfigError {
    /// Missing `url` field for a `webhook` channel.
    #[error("url manquante pour le canal webhook '{id}'")]
    MissingWebhookUrl {
        /// Identifier of the misconfigured channel.
        id: String,
    },
}

/// Instantiates the active channels from a list of [`ChannelConfig`].
///
/// Iterates over `configs` in declaration order:
/// - `enabled = false` -> channel silently ignored
/// - `type = "desktop"` -> [`DesktopChannel`] added
/// - `type = "webhook"` -> [`WebhookChannel`] added (error if `url` absent)
/// - `type = "terminal"` -> [`TerminalChannel`] added (automatic emulator detection)
///
/// Returns an error if an active `webhook` channel has no `url`.
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
        }
    }
    Ok(channels)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_channel_accepts_event_disabled() {
        // GIVEN a disabled channel
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
        // GIVEN a channel without its own list, using the global list
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
        // GIVEN a channel without its own list, event absent from the global list
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
        // GIVEN a channel with events=["*"]
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
        // GIVEN a channel with events=["task.input_required"]
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
        // GIVEN the 5 severity levels
        // WHEN compared
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
        // GIVEN the 3 configurable channel types
        // WHEN default_min_severity() is called
        // THEN Desktop -> Error, Webhook -> Info, Terminal -> Warning
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

    // build_channels returns a DesktopChannel when desktop enabled=true
    #[test]
    fn test_build_channels_desktop_enabled() {
        // GIVEN config with desktop enabled=true
        let configs = vec![ChannelConfig {
            id: "desktop".into(),
            kind: ChannelKind::Desktop,
            enabled: true,
            events: None,
            url: None,
            signing_secret: None,
            min_severity: None,
            min_interval_seconds: 0,
        }];

        // WHEN
        let result = build_channels(&configs);

        // THEN 1 canal retourné
        let channels = result.expect("build_channels ne doit pas échouer");
        assert_eq!(channels.len(), 1);
        assert_eq!(channels[0].id(), "desktop");
    }

    // build_channels ignores channels with enabled=false
    #[test]
    fn test_build_channels_disabled_skipped() {
        // GIVEN config with webhook enabled=false
        let configs = vec![ChannelConfig {
            id: "slack".into(),
            kind: ChannelKind::Webhook,
            enabled: false,
            events: None,
            url: Some("https://hooks.slack.com/test".into()),
            signing_secret: None,
            min_severity: None,
            min_interval_seconds: 0,
        }];

        // WHEN
        let result = build_channels(&configs);

        // THEN 0 channels returned (disabled channel silently ignored)
        let channels = result.expect("build_channels ne doit pas échouer");
        assert!(channels.is_empty());
    }

    // build_channels returns an error if webhook has no url
    #[test]
    fn test_build_channels_webhook_no_url_returns_error() {
        // GIVEN config type="webhook" without a url field
        let configs = vec![ChannelConfig {
            id: "monitoring".into(),
            kind: ChannelKind::Webhook,
            enabled: true,
            events: None,
            url: None,
            signing_secret: None,
            min_severity: None,
            min_interval_seconds: 0,
        }];

        // WHEN
        let result = build_channels(&configs);

        // THEN Err(NotifConfigError::MissingWebhookUrl) with the channel id
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
