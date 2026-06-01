use std::time::Duration;

use async_trait::async_trait;
use hmac::{Hmac, Mac};
use reqwest::Client;
use sha2::Sha256;

use crate::{
    config::{channel_accepts_event, NotificationConfig, Severity},
    engine::{NotifError, Notification, NotificationChannel},
};

type HmacSha256 = Hmac<Sha256>;

/// Configuration of a webhook channel.
///
/// Maps to a `[[notifications.channels]]` entry of type `webhook` in
/// `apollia.toml`. The `url` field is mandatory (unlike the generic
/// [`crate::config::ChannelConfig`], which has it as `Option`).
#[derive(Debug, Clone, serde::Deserialize)]
pub struct WebhookChannelConfig {
    /// Unique channel identifier (e.g. `"slack"`, `"monitoring"`).
    pub id: String,
    /// Webhook endpoint URL (e.g. `"https://hooks.slack.com/services/..."`).
    pub url: String,
    /// If `false`, the channel is ignored even if present in the config.
    pub enabled: bool,
    /// Subset of events to receive on this channel.
    ///
    /// - `None` -> uses the global list (`[notifications].events`)
    /// - `Some(["*"])` -> all events from the global list
    /// - `Some(list)` -> an explicit subset of events
    pub events: Option<Vec<String>>,
    /// Shared secret to sign outgoing payloads with HMAC-SHA256.
    ///
    /// If `Some`, each request gets an `X-Apollia-Signature: sha256=<hex>` header.
    /// If `None`, the webhook is sent without a signature (backward compatible).
    pub signing_secret: Option<String>,
    /// Minimum severity accepted by this channel.
    ///
    /// Notifications whose severity is below this threshold are silently
    /// ignored. Default on deserialization: [`Severity::Info`] (all non-debug
    /// notifications are forwarded).
    #[serde(default)]
    pub min_severity: Severity,
}

/// HTTP POST notification channel: fixed Apollia JSON format.
///
/// Builds a [`reqwest::Client`] with a 5-second timeout at creation. Each call
/// to [`send`] performs a `POST` to the configured URL with the JSON payload
/// documented in the Apollia spec.
///
/// On a network error, timeout, or non-2xx HTTP response,
/// [`NotifError::WebhookFailed`] is returned. The caller
/// ([`crate::engine::NotificationEngine`]) logs the error at `warn!` and
/// continues without interrupting dispatch to the other channels.
pub struct WebhookChannel {
    config: WebhookChannelConfig,
    client: Client,
    ssrf_guard: bool,
}

impl WebhookChannel {
    /// Creates a webhook channel with a 5s timeout and User-Agent `apollia-os/<version>`.
    ///
    /// # Panics
    ///
    /// Panics if building the [`reqwest::Client`] fails, which cannot happen on
    /// the supported systems (no custom TLS or incompatible system proxy
    /// required).
    pub fn new(config: WebhookChannelConfig) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(5))
            .user_agent(format!("apollia-os/{}", env!("CARGO_PKG_VERSION")))
            .build()
            .expect("reqwest::Client build ne peut pas échouer avec la config par défaut");
        Self::with_client(config, client)
    }

    /// Creates a channel with a provided reqwest client.
    ///
    /// Lets tests inject a client with a short timeout or a mock server.
    pub(crate) fn with_client(config: WebhookChannelConfig, client: Client) -> Self {
        Self {
            config,
            client,
            ssrf_guard: true,
        }
    }

    /// Enables or disables the SSRF guard on this channel.
    ///
    /// Enabled by default. The opt-out exists only for in-process integration
    /// tests that must talk to a mock server on `127.0.0.1`. Every production
    /// call site must leave it at `true`.
    #[must_use]
    pub fn with_ssrf_guard(mut self, enabled: bool) -> Self {
        self.ssrf_guard = enabled;
        self
    }
}

/// Computes the HMAC-SHA256 signature of a body with the given secret.
///
/// Returns the string in `sha256=<hex>` format as expected in the
/// `X-Apollia-Signature` header. Follows the GitHub/Stripe convention.
///
/// HMAC accepts keys of any size, so the internal `expect` can never trigger in
/// practice.
pub fn compute_signature(secret: &str, body: &[u8]) -> String {
    let mut mac =
        HmacSha256::new_from_slice(secret.as_bytes()).expect("HMAC accepts keys of any size");
    mac.update(body);
    let result = mac.finalize();
    format!("sha256={}", hex::encode(result.into_bytes()))
}

/// Payload format to send to the webhook endpoint.
///
/// Detected automatically by hostname in [`detect_webhook_kind`].
/// Discord or Slack users get a native payload accepted by their platform;
/// every other endpoint receives the historical Apollia JSON format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WebhookKind {
    /// Apollia JSON format: fields `event`, `task_id`, `severity`, etc.
    /// Used for custom endpoints (Apollia-aware services).
    Apollia,
    /// Discord Incoming Webhook: payload `{ content, embeds, username }`.
    /// Detection: hostname contains `discord.com` or `discordapp.com`.
    Discord,
    /// Slack Incoming Webhook: payload `{ text, attachments }`.
    /// Detection: hostname contains `hooks.slack.com`.
    Slack,
}

/// Guesses the expected format from the URL hostname.
///
/// Tolerant of URL errors: falls back to [`WebhookKind::Apollia`] if parsing
/// fails. SSRF/URL validation is done separately upstream.
pub(crate) fn detect_webhook_kind(url: &str) -> WebhookKind {
    let Ok(parsed) = url::Url::parse(url) else {
        return WebhookKind::Apollia;
    };
    match parsed.host_str() {
        Some(host)
            if host.eq_ignore_ascii_case("discord.com")
                || host.eq_ignore_ascii_case("ptb.discord.com")
                || host.eq_ignore_ascii_case("canary.discord.com")
                || host.eq_ignore_ascii_case("discordapp.com") =>
        {
            WebhookKind::Discord
        }
        Some(host) if host.eq_ignore_ascii_case("hooks.slack.com") => WebhookKind::Slack,
        _ => WebhookKind::Apollia,
    }
}

/// Builds the payload according to the detected format.
///
/// Delegates to [`build_apollia_payload`], [`build_discord_payload`], or
/// [`build_slack_payload`].
pub(crate) fn build_payload(notif: &Notification) -> serde_json::Value {
    build_apollia_payload(notif)
}

/// Builds the Apollia JSON payload from a [`Notification`].
///
/// The format is fixed and documented:
/// `event`, `timestamp`, `runtime`, `version`, `task_id`, `agent`, `message`,
/// `metadata`, `severity`.
pub(crate) fn build_apollia_payload(notif: &Notification) -> serde_json::Value {
    serde_json::json!({
        "event":     notif.event,
        "timestamp": notif.timestamp.to_rfc3339(),
        "runtime":   "apollia-os",
        "version":   env!("CARGO_PKG_VERSION"),
        "task_id":   notif.task_id,
        "agent":     notif.agent,
        "message":   notif.message,
        "metadata":  notif.metadata,
        "severity":  notif.severity.as_str(),
    })
}

/// Builds a Discord payload with a rich embed.
///
/// Discord accepts `content` (plain text) and/or `embeds` (an array of embed
/// objects). We choose the embed to keep the metadata: per-severity color,
/// event / agent / task fields, and timestamp.
pub(crate) fn build_discord_payload(notif: &Notification) -> serde_json::Value {
    let mut fields: Vec<serde_json::Value> = Vec::new();
    fields.push(serde_json::json!({
        "name": "Événement",
        "value": format!("`{}`", notif.event),
        "inline": true,
    }));
    if let Some(ref agent) = notif.agent {
        fields.push(serde_json::json!({
            "name": "Agent",
            "value": agent,
            "inline": true,
        }));
    }
    if let Some(ref task) = notif.task_id {
        fields.push(serde_json::json!({
            "name": "Tâche",
            "value": format!("`{task}`"),
            "inline": true,
        }));
    }

    // Discord colors (decimal integer). See https://discord.com/developers/docs/resources/channel#embed-object
    let color: u32 = match notif.severity {
        Severity::Critical => 0xB91C1C, // red-700
        Severity::Error => 0xDC2626,    // red-600
        Severity::Warning => 0xD97706,  // amber-600
        Severity::Info => 0x2563EB,     // blue-600
        Severity::Debug => 0x6B7280,    // gray-500
    };

    let embed = serde_json::json!({
        "title": truncate(&notif.message, 256),
        "color": color,
        "fields": fields,
        "timestamp": notif.timestamp.to_rfc3339(),
        "footer": { "text": format!("Apollia OS · {}", notif.severity.as_str()) },
    });

    serde_json::json!({
        "username": "Apollia OS",
        "embeds": [embed],
    })
}

/// Builds a Slack Incoming Webhook payload.
///
/// Slack accepts `text` (light markdown) and `attachments` (legacy but widely
/// supported). We use attachments for the color and the context fields.
pub(crate) fn build_slack_payload(notif: &Notification) -> serde_json::Value {
    let color = match notif.severity {
        Severity::Critical | Severity::Error => "danger",
        Severity::Warning => "warning",
        Severity::Info | Severity::Debug => "good",
    };

    let mut fields: Vec<serde_json::Value> = vec![serde_json::json!({
        "title": "Événement",
        "value": notif.event,
        "short": true,
    })];
    if let Some(ref agent) = notif.agent {
        fields.push(serde_json::json!({
            "title": "Agent",
            "value": agent,
            "short": true,
        }));
    }
    if let Some(ref task) = notif.task_id {
        fields.push(serde_json::json!({
            "title": "Tâche",
            "value": task,
            "short": true,
        }));
    }

    serde_json::json!({
        "text": notif.message,
        "attachments": [{
            "color": color,
            "fields": fields,
            "footer": "Apollia OS",
            "ts": notif.timestamp.timestamp(),
        }],
    })
}

/// Truncates a string to `max` bytes, respecting char boundaries.
fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.to_owned();
    }
    let mut end = max;
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &s[..end.saturating_sub(1)])
}

#[async_trait]
impl NotificationChannel for WebhookChannel {
    /// Returns the channel identifier as configured in `apollia.toml`.
    fn id(&self) -> &str {
        &self.config.id
    }

    /// Returns `true` if this channel is enabled and accepts the given event.
    ///
    /// Delegates the filtering logic to [`channel_accepts_event`].
    fn accepts(&self, event: &str, config: &NotificationConfig) -> bool {
        channel_accepts_event(
            self.config.enabled,
            &self.config.events,
            event,
            &config.events,
        )
    }

    /// Sends the notification via HTTP POST to the configured URL.
    ///
    /// - **Payload**: fixed Apollia JSON format (see [`build_payload`])
    /// - **Headers**: `Content-Type: application/json`, `X-Apollia-Event: <event>`,
    ///   `User-Agent: apollia-os/<version>` (via the client),
    ///   `X-Apollia-Signature: sha256=<hex>` if `signing_secret` is configured
    /// - **Timeout**: 5s (configured on the client in [`new`])
    ///
    /// The signature is computed over the final serialized JSON body before sending.
    ///
    /// Returns [`NotifError::WebhookFailed`] for any network error or non-2xx
    /// HTTP response. The error is non-critical: the runtime logs it at `warn!`
    /// without interrupting dispatch.
    async fn send(&self, notif: &Notification) -> Result<(), NotifError> {
        if notif.severity < self.config.min_severity {
            return Ok(());
        }

        // SSRF guard: must precede the HTTP request. An operator may configure a
        // URL pointing to 127.0.0.1 or a cloud metadata endpoint; refuse before
        // any byte is emitted.
        if self.ssrf_guard {
            let parsed_url = url::Url::parse(&self.config.url)
                .map_err(|e| NotifError::InvalidUrl(e.to_string()))?;
            apollia_tools::ssrf::assert_public(&parsed_url)
                .map_err(|e| NotifError::Ssrf(e.to_string()))?;
        }

        // Auto-detect Discord / Slack from hostname so out-of-the-box usage
        // works without operator config. Custom endpoints (anything else) keep
        // receiving the Apollia native JSON schema.
        let kind = detect_webhook_kind(&self.config.url);
        let payload = match kind {
            WebhookKind::Discord => build_discord_payload(notif),
            WebhookKind::Slack => build_slack_payload(notif),
            WebhookKind::Apollia => build_apollia_payload(notif),
        };
        let body_bytes =
            serde_json::to_vec(&payload).map_err(|e| NotifError::WebhookFailed(e.to_string()))?;

        let mut builder = self
            .client
            .post(&self.config.url)
            .header("Content-Type", "application/json")
            .header("X-Apollia-Event", &notif.event);

        if let Some(secret) = &self.config.signing_secret {
            let signature = compute_signature(secret, &body_bytes);
            tracing::debug!(channel = %self.config.id, "webhook request signed with HMAC-SHA256");
            builder = builder.header("X-Apollia-Signature", signature);
        }

        let resp = builder
            .body(body_bytes)
            .send()
            .await
            .map_err(|e| NotifError::WebhookFailed(e.to_string()))?;

        if !resp.status().is_success() {
            let status = resp.status();
            // Best-effort body extract for diagnostics: many providers (Discord,
            // Slack) return a JSON error body that explains the rejection.
            let body_excerpt = match resp.text().await {
                Ok(text) if !text.is_empty() => {
                    let trimmed: String = text.chars().take(200).collect();
                    format!(" - {trimmed}")
                }
                _ => String::new(),
            };
            return Err(NotifError::WebhookFailed(format!(
                "HTTP {status}{body_excerpt}"
            )));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use chrono::Utc;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    use super::*;
    use crate::config::{NotificationConfig, Severity};

    fn make_config(events: Vec<&str>) -> NotificationConfig {
        NotificationConfig {
            events: events.into_iter().map(String::from).collect(),
            channels: vec![],
            inactivity_timeout_secs: 30,
        }
    }

    fn make_notif(event: &str, task_id: Option<&str>, severity: Severity) -> Notification {
        let mut metadata = HashMap::new();
        if event == "task.input_required" {
            metadata.insert(
                "resume_url".into(),
                "http://localhost:7771/api/v1/tasks/t-0042/resume".into(),
            );
            metadata.insert(
                "inspect_url".into(),
                "http://localhost:7771/dashboard#tasks/t-0042".into(),
            );
        }
        Notification {
            event: event.into(),
            timestamp: Utc::now(),
            task_id: task_id.map(String::from),
            agent: Some("devis-agent".into()),
            message: "Message de test".into(),
            metadata,
            severity,
        }
    }

    fn make_channel_url(url: &str) -> WebhookChannel {
        WebhookChannel::new(WebhookChannelConfig {
            id: "test-webhook".into(),
            url: url.into(),
            enabled: true,
            events: None,
            signing_secret: None,
            min_severity: Severity::Info,
        })
        .with_ssrf_guard(false)
    }

    fn make_channel_with_secret(url: &str, secret: &str) -> WebhookChannel {
        WebhookChannel::new(WebhookChannelConfig {
            id: "test-signed-webhook".into(),
            url: url.into(),
            enabled: true,
            events: None,
            signing_secret: Some(secret.into()),
            min_severity: Severity::Info,
        })
        .with_ssrf_guard(false)
    }

    // --- disabled channel ------------------------------------------------

    #[test]
    fn test_ac5_disabled_channel_accepts_false() {
        // GIVEN a webhook channel configured with enabled=false
        let channel = WebhookChannel::new(WebhookChannelConfig {
            id: "slack".into(),
            url: "http://test".into(),
            enabled: false,
            events: None,
            signing_secret: None,
            min_severity: Severity::Info,
        });
        let config = make_config(vec!["task.input_required"]);

        // WHEN / THEN accepts() returns false and send() is never called
        assert!(!channel.accepts("task.input_required", &config));
    }

    #[test]
    fn test_ac5_enabled_channel_accepts_matching_event() {
        // GIVEN an enabled channel without its own list, delegating to the global list
        let channel = make_channel_url("http://example.com");
        let config = make_config(vec!["task.input_required", "task.failed"]);

        // WHEN / THEN
        assert!(channel.accepts("task.input_required", &config));
        assert!(channel.accepts("task.failed", &config));
        // Event absent from the global list is rejected.
        assert!(!channel.accepts("agent.degraded", &config));
    }

    #[test]
    fn test_ac5_channel_with_per_channel_events_subset() {
        // GIVEN a channel with an explicit subset of events
        let channel = WebhookChannel::new(WebhookChannelConfig {
            id: "slack".into(),
            url: "http://example.com".into(),
            enabled: true,
            events: Some(vec!["task.input_required".into(), "task.failed".into()]),
            signing_secret: None,
            min_severity: Severity::Info,
        });
        let config = make_config(vec!["task.input_required", "task.failed", "agent.degraded"]);

        // WHEN / THEN agent.degraded rejected because absent from the channel list
        assert!(channel.accepts("task.input_required", &config));
        assert!(channel.accepts("task.failed", &config));
        assert!(!channel.accepts("agent.degraded", &config));
    }

    // --- severity filtering ----------------------------------------------

    #[tokio::test]
    async fn test_webhook_filters_below_min_severity() {
        // GIVEN a webhook channel with min_severity = Warning
        let channel = WebhookChannel::new(WebhookChannelConfig {
            id: "webhook-warn".into(),
            url: "http://127.0.0.1:1".into(),
            enabled: true,
            events: None,
            signing_secret: None,
            min_severity: Severity::Warning,
        });

        // WHEN an Info notification (< Warning) is dispatched
        let notif = make_notif("task.completed", None, Severity::Info);

        // THEN immediate Ok(()) with no network attempt
        let result = channel.send(&notif).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_webhook_default_min_severity_is_info() {
        // GIVEN WebhookChannelConfig with the default min_severity (serde default)
        let json = r#"{
            "id": "webhook",
            "url": "http://example.com",
            "enabled": true
        }"#;
        let cfg: WebhookChannelConfig = serde_json::from_str(json).expect("déserialisation");

        // WHEN / THEN min_severity = Info (Severity::default())
        assert_eq!(cfg.min_severity, Severity::Info);
    }

    // --- JSON payload structure ------------------------------------------

    #[test]
    fn test_ac1_payload_json_structure_task_input_required() {
        // GIVEN a task.input_required Notification
        let notif = make_notif("task.input_required", Some("t-0042"), Severity::Warning);

        // WHEN
        let payload = build_payload(&notif);

        // THEN all fields of the fixed Apollia JSON format are present
        assert_eq!(payload["event"], "task.input_required");
        assert_eq!(payload["runtime"], "apollia-os");
        assert_eq!(payload["task_id"], "t-0042");
        assert_eq!(payload["agent"], "devis-agent");
        assert_eq!(payload["severity"], "warning");
        assert!(
            payload["timestamp"].as_str().is_some(),
            "timestamp doit être une chaîne ISO8601"
        );
        assert!(
            payload["version"].as_str().is_some(),
            "version doit être présente"
        );
        // metadata contains the HITL URLs
        assert!(
            payload["metadata"]["resume_url"].as_str().is_some(),
            "resume_url absent des metadata"
        );
        assert!(
            payload["metadata"]["inspect_url"].as_str().is_some(),
            "inspect_url absent des metadata"
        );
    }

    #[test]
    fn test_ac1_payload_task_failed_severity_error() {
        // GIVEN a task.failed notification
        let notif = make_notif("task.failed", Some("t-001"), Severity::Error);

        // WHEN
        let payload = build_payload(&notif);

        // THEN
        assert_eq!(payload["event"], "task.failed");
        assert_eq!(payload["severity"], "error");
        assert_eq!(payload["agent"], "devis-agent");
    }

    #[test]
    fn test_ac1_payload_null_fields_when_no_task_id() {
        // GIVEN a notification without task_id (e.g. agent.degraded)
        let notif = Notification {
            event: "agent.degraded".into(),
            timestamp: Utc::now(),
            task_id: None,
            agent: Some("mon-agent".into()),
            message: "Agent dégradé".into(),
            metadata: HashMap::new(),
            severity: Severity::Warning,
        };

        // WHEN
        let payload = build_payload(&notif);

        // THEN task_id is null (JSON null), not absent
        assert!(payload["task_id"].is_null());
        assert_eq!(payload["event"], "agent.degraded");
    }

    // --- Format detection by hostname (Discord / Slack / Apollia) --------

    #[test]
    fn test_detect_webhook_kind_discord() {
        assert_eq!(
            detect_webhook_kind("https://discord.com/api/webhooks/123/abc"),
            WebhookKind::Discord
        );
        assert_eq!(
            detect_webhook_kind("https://canary.discord.com/api/webhooks/123/abc"),
            WebhookKind::Discord
        );
        assert_eq!(
            detect_webhook_kind("https://discordapp.com/api/webhooks/123/abc"),
            WebhookKind::Discord
        );
    }

    #[test]
    fn test_detect_webhook_kind_slack() {
        assert_eq!(
            detect_webhook_kind("https://hooks.slack.com/services/T000/B000/xxx"),
            WebhookKind::Slack
        );
    }

    #[test]
    fn test_detect_webhook_kind_custom_falls_back_to_apollia() {
        assert_eq!(
            detect_webhook_kind("https://my-service.example.com/notify"),
            WebhookKind::Apollia
        );
        assert_eq!(
            detect_webhook_kind("https://example.com/hooks"),
            WebhookKind::Apollia
        );
    }

    #[test]
    fn test_detect_webhook_kind_invalid_url_falls_back_to_apollia() {
        assert_eq!(detect_webhook_kind("not a url"), WebhookKind::Apollia);
    }

    // --- Discord payload format ------------------------------------------

    #[test]
    fn test_build_discord_payload_has_embed_and_username() {
        // GIVEN a task.failed notification
        let notif = make_notif("task.failed", Some("t-001"), Severity::Error);

        // WHEN
        let payload = build_discord_payload(&notif);

        // THEN username override + an embed with title, color, fields
        assert_eq!(payload["username"], "Apollia OS");
        let embeds = payload["embeds"].as_array().expect("embeds array");
        assert_eq!(embeds.len(), 1);
        let embed = &embeds[0];
        assert!(embed["title"].as_str().is_some());
        // error color = 0xDC2626 = 14_427_686
        assert_eq!(embed["color"], 14_427_686);
        assert!(embed["fields"].as_array().expect("fields").len() >= 2);
        assert!(embed["timestamp"].as_str().is_some());
    }

    #[test]
    fn test_build_discord_payload_severity_critical_uses_red() {
        let notif = make_notif("llm.cost_alert", None, Severity::Critical);
        let payload = build_discord_payload(&notif);
        // 0xB91C1C = 12_131_356
        assert_eq!(payload["embeds"][0]["color"], 12_131_356);
    }

    #[test]
    fn test_build_discord_payload_no_optional_fields_when_absent() {
        // GIVEN no task_id or agent
        let notif = Notification {
            event: "trigger.error".into(),
            timestamp: Utc::now(),
            task_id: None,
            agent: None,
            message: "Trigger fail".into(),
            metadata: HashMap::new(),
            severity: Severity::Warning,
        };

        // WHEN
        let payload = build_discord_payload(&notif);

        // THEN a single field (the event)
        let fields = payload["embeds"][0]["fields"].as_array().expect("fields");
        assert_eq!(fields.len(), 1);
        assert_eq!(fields[0]["name"], "Événement");
    }

    // --- Slack payload format --------------------------------------------

    #[test]
    fn test_build_slack_payload_has_text_and_attachment() {
        let notif = make_notif("task.completed", Some("t-001"), Severity::Info);
        let payload = build_slack_payload(&notif);

        assert!(payload["text"].as_str().is_some());
        let attachments = payload["attachments"].as_array().expect("attachments");
        assert_eq!(attachments.len(), 1);
        assert_eq!(attachments[0]["color"], "good");
        assert!(attachments[0]["fields"].as_array().expect("fields").len() >= 2);
    }

    #[test]
    fn test_build_slack_payload_severity_error_uses_danger() {
        let notif = make_notif("task.failed", None, Severity::Error);
        let payload = build_slack_payload(&notif);
        assert_eq!(payload["attachments"][0]["color"], "danger");
    }

    // --- timeout -> NotifError::WebhookFailed ----------------------------

    #[tokio::test]
    async fn test_ac3_webhook_timeout_returns_error() {
        // GIVEN a TCP server that accepts the connection but never responds
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind échoue");
        let addr = listener.local_addr().expect("local_addr");

        tokio::spawn(async move {
            // Accept and hold the connection open without ever responding.
            if let Ok((_stream, _)) = listener.accept().await {
                tokio::time::sleep(Duration::from_secs(30)).await;
            }
        });

        // Client with a short timeout so the test does not block.
        let config = WebhookChannelConfig {
            id: "timeout-test".into(),
            url: format!("http://127.0.0.1:{}", addr.port()),
            enabled: true,
            events: None,
            signing_secret: None,
            min_severity: Severity::Info,
        };
        let client = Client::builder()
            .timeout(Duration::from_millis(300))
            .build()
            .expect("client build");
        let channel = WebhookChannel::with_client(config, client).with_ssrf_guard(false);

        // WHEN
        let result = channel
            .send(&make_notif(
                "task.input_required",
                Some("t-0042"),
                Severity::Warning,
            ))
            .await;

        // THEN NotifError::WebhookFailed returned, no panic
        assert!(
            matches!(result, Err(NotifError::WebhookFailed(_))),
            "attendu Err(WebhookFailed), obtenu {:?}",
            result
        );
    }

    // --- HTTP 500 response -> NotifError::WebhookFailed ------------------

    #[tokio::test]
    async fn test_ac4_webhook_500_returns_error() {
        // GIVEN a minimal HTTP server that responds 500
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind échoue");
        let addr = listener.local_addr().expect("local_addr");

        tokio::spawn(async move {
            if let Ok((mut stream, _)) = listener.accept().await {
                // Consume the request (a partial read is enough).
                let mut buf = [0u8; 4096];
                let _ = stream.read(&mut buf).await;
                let response = b"HTTP/1.1 500 Internal Server Error\r\nContent-Length: 0\r\n\r\n";
                let _ = stream.write_all(response).await;
            }
        });

        let channel = WebhookChannel::new(WebhookChannelConfig {
            id: "500-test".into(),
            url: format!("http://127.0.0.1:{}", addr.port()),
            enabled: true,
            events: None,
            signing_secret: None,
            min_severity: Severity::Info,
        })
        .with_ssrf_guard(false);

        // WHEN
        let result = channel
            .send(&make_notif("task.failed", Some("t-001"), Severity::Error))
            .await;

        // THEN NotifError::WebhookFailed containing "500"
        match result {
            Err(NotifError::WebhookFailed(msg)) => {
                assert!(
                    msg.contains("500"),
                    "message doit contenir '500', obtenu: {msg}"
                );
            }
            other => panic!("attendu Err(WebhookFailed), obtenu {:?}", other),
        }
    }

    // --- correct headers sent --------------------------------------------

    #[tokio::test]
    async fn test_ac2_headers_sent_correctly() {
        // GIVEN an HTTP server that captures the raw request
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind échoue");
        let addr = listener.local_addr().expect("local_addr");

        let (captured_tx, captured_rx) = tokio::sync::oneshot::channel::<String>();

        tokio::spawn(async move {
            if let Ok((mut stream, _)) = listener.accept().await {
                let mut buf = [0u8; 8192];
                let n = stream.read(&mut buf).await.unwrap_or(0);
                let request = String::from_utf8_lossy(&buf[..n]).to_string();
                let _ = captured_tx.send(request);
                let response = b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n";
                let _ = stream.write_all(response).await;
            }
        });

        let channel = make_channel_url(&format!("http://127.0.0.1:{}", addr.port()));
        let notif = make_notif("task.failed", Some("t-001"), Severity::Error);

        // WHEN
        let _ = channel.send(&notif).await;

        // THEN headers X-Apollia-Event, Content-Type and User-Agent present
        let request = captured_rx
            .await
            .expect("requête non capturée par le serveur mock");

        // X-Apollia-Event must contain the event name.
        assert!(
            request
                .to_lowercase()
                .contains("x-apollia-event: task.failed"),
            "header X-Apollia-Event absent ou incorrect\n---\n{request}"
        );
        // Content-Type application/json (set by reqwest via .json())
        assert!(
            request
                .to_lowercase()
                .contains("content-type: application/json"),
            "header Content-Type absent\n---\n{request}"
        );
        // User-Agent contains the apollia-os prefix.
        assert!(
            request.to_lowercase().contains("apollia-os/"),
            "header User-Agent absent ou incorrect\n---\n{request}"
        );
    }

    // --- HMAC-SHA256: signature computation ------------------------------

    #[test]
    fn test_hmac_signature_matches_expected_value() {
        // GIVEN secret = "secret", body = b"payload"
        let signature = compute_signature("secret", b"payload");

        // WHEN recompute independently with raw HMAC
        let mut mac = HmacSha256::new_from_slice(b"secret").expect("valid key");
        mac.update(b"payload");
        let expected = format!("sha256={}", hex::encode(mac.finalize().into_bytes()));

        // THEN both computations produce the same result
        assert_eq!(
            signature, expected,
            "compute_signature doit correspondre au calcul HMAC direct"
        );
    }

    #[test]
    fn test_empty_body_produces_valid_signature() {
        // GIVEN secret = "secret", empty body
        let signature = compute_signature("secret", b"");

        // WHEN recompute with empty body
        let mut mac = HmacSha256::new_from_slice(b"secret").expect("valid key");
        mac.update(b"");
        let expected = format!("sha256={}", hex::encode(mac.finalize().into_bytes()));

        // THEN the signature is valid and matches (edge case: empty body)
        assert_eq!(signature, expected);
        assert!(signature.starts_with("sha256="));
        assert_eq!(signature.len(), "sha256=".len() + 64);
    }

    #[test]
    fn test_different_secrets_produce_different_signatures() {
        // GIVEN the same body, different secrets
        let body = b"same payload";
        let sig1 = compute_signature("secret_a", body);
        let sig2 = compute_signature("secret_b", body);

        // THEN distinct signatures
        assert_ne!(
            sig1, sig2,
            "des secrets différents doivent produire des signatures différentes"
        );
    }

    #[test]
    fn test_signature_format_is_sha256_prefix_hex() {
        // GIVEN an arbitrary secret and body
        let signature = compute_signature("whsec_test123", b"{\"event\":\"agent.started\"}");

        // THEN format "sha256=<64 hex chars>"
        assert!(
            signature.starts_with("sha256="),
            "la signature doit commencer par 'sha256='"
        );
        let hex_part = &signature["sha256=".len()..];
        assert_eq!(
            hex_part.len(),
            64,
            "la partie hex doit faire 64 caractères (HMAC-SHA256 = 32 octets)"
        );
        assert!(
            hex_part.chars().all(|c| c.is_ascii_hexdigit()),
            "la partie hex ne doit contenir que des caractères hexadécimaux"
        );
    }

    // --- X-Apollia-Signature header in HTTP requests ---------------------

    #[tokio::test]
    async fn test_webhook_with_secret_adds_signature_header() {
        // GIVEN an HTTP server that captures the raw request
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind échoue");
        let addr = listener.local_addr().expect("local_addr");

        let (tx, rx) = tokio::sync::oneshot::channel::<String>();

        tokio::spawn(async move {
            if let Ok((mut stream, _)) = listener.accept().await {
                let mut buf = [0u8; 8192];
                let n = stream.read(&mut buf).await.unwrap_or(0);
                let request = String::from_utf8_lossy(&buf[..n]).to_string();
                let _ = tx.send(request);
                let response = b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n";
                let _ = stream.write_all(response).await;
            }
        });

        let channel = make_channel_with_secret(
            &format!("http://127.0.0.1:{}", addr.port()),
            "whsec_test123",
        );
        let notif = make_notif("agent.started", None, Severity::Info);

        // WHEN
        let _ = channel.send(&notif).await;
        let request = rx.await.expect("requête non capturée");

        // THEN X-Apollia-Signature present with the correct format
        let request_lower = request.to_lowercase();
        assert!(
            request_lower.contains("x-apollia-signature: sha256="),
            "header X-Apollia-Signature absent ou malformé\n---\n{request}"
        );
    }

    #[tokio::test]
    async fn test_webhook_without_secret_no_signature_header() {
        // GIVEN an HTTP server that captures the raw request
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind échoue");
        let addr = listener.local_addr().expect("local_addr");

        let (tx, rx) = tokio::sync::oneshot::channel::<String>();

        tokio::spawn(async move {
            if let Ok((mut stream, _)) = listener.accept().await {
                let mut buf = [0u8; 8192];
                let n = stream.read(&mut buf).await.unwrap_or(0);
                let request = String::from_utf8_lossy(&buf[..n]).to_string();
                let _ = tx.send(request);
                let response = b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n";
                let _ = stream.write_all(response).await;
            }
        });

        // Channel without signing_secret.
        let channel = make_channel_url(&format!("http://127.0.0.1:{}", addr.port()));
        let notif = make_notif("agent.started", None, Severity::Info);

        // WHEN
        let _ = channel.send(&notif).await;
        let request = rx.await.expect("requête non capturée");

        // THEN X-Apollia-Signature absent
        assert!(
            !request.to_lowercase().contains("x-apollia-signature"),
            "X-Apollia-Signature ne doit pas être présent sans secret\n---\n{request}"
        );
    }

    // --- SSRF guard ------------------------------------------------------

    #[tokio::test]
    async fn test_webhook_channel_blocks_internal_url() {
        // GIVEN a webhook channel configured (by operator error) with a URL
        // pointing to the RFC1918 private network. No server is listening, but
        // the SSRF guard must refuse the send before any network I/O.
        let channel = WebhookChannel::new(WebhookChannelConfig {
            id: "exfil-test".into(),
            url: "http://10.0.0.1/exfil".into(),
            enabled: true,
            events: None,
            signing_secret: None,
            min_severity: Severity::Info,
        });
        let notif = make_notif("task.failed", Some("t-001"), Severity::Error);

        // WHEN
        let result = channel.send(&notif).await;

        // THEN NotifError::Ssrf, never an HTTP attempt
        assert!(
            matches!(result, Err(NotifError::Ssrf(_))),
            "attendu Err(Ssrf), obtenu {:?}",
            result
        );
    }

    #[tokio::test]
    async fn test_webhook_channel_blocks_metadata_endpoint() {
        // GIVEN a URL pointing to the cloud metadata endpoint (link-local).
        let channel = WebhookChannel::new(WebhookChannelConfig {
            id: "metadata-test".into(),
            url: "http://169.254.169.254/latest/meta-data/".into(),
            enabled: true,
            events: None,
            signing_secret: None,
            min_severity: Severity::Info,
        });
        let notif = make_notif("task.failed", Some("t-001"), Severity::Error);

        // WHEN
        let result = channel.send(&notif).await;

        // THEN
        assert!(
            matches!(result, Err(NotifError::Ssrf(_))),
            "attendu Err(Ssrf), obtenu {:?}",
            result
        );
    }
}
