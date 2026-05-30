//! Tauri IPC commands for managing notifications.
//!
//! Each command delegates to the internal REST API (`/api/v1/notifications/*`)
//! via the `http_get_json` / `http_post_json` helpers. Data travels as raw JSON
//! (`serde_json::Value`) to avoid duplicating the Rust types already defined in
//! `apollia-notifications`.

use apollia_runtime::embedded::RuntimeHandle;
use serde::{Deserialize, Serialize};
use tauri::State;

use super::{http_delete_json, http_get_json, http_post_json, http_put_json};

/// Public description of a notification channel for the UI.
#[derive(Debug, Serialize)]
pub struct NotificationChannel {
    /// Unique channel identifier (e.g. `"desktop"`, `"slack"`).
    pub channel_id: String,
    /// Name displayed in the UI. `None` = the UI falls back to `channel_id`.
    pub label: Option<String>,
    /// Channel type: `"desktop"` or `"webhook"`.
    #[serde(rename = "type")]
    pub kind: String,
    /// `true` if the channel is enabled in the configuration.
    pub enabled: bool,
    /// List of events this channel accepts.
    pub events: Vec<String>,
    /// Minimum throttling interval, in seconds (`0` = none).
    pub min_interval_seconds: u32,
}

/// Result of testing an individual channel.
#[derive(Debug, Serialize)]
pub struct ChannelTestResult {
    /// Unique channel identifier.
    pub channel_id: String,
    /// Test status: `"ok"`, `"error"`, or `"disabled"`.
    pub status: String,
    /// Error message if `status == "error"`.
    pub error: Option<String>,
    /// Measured latency in milliseconds.
    pub latency_ms: Option<u64>,
}

/// Notification history entry for the UI.
#[derive(Debug, Serialize)]
pub struct NotificationLogEntry {
    /// Unique entry identifier.
    pub id: String,
    /// Name of the triggering event (e.g. `"task.completed"`).
    pub event_name: String,
    /// Identifier of the related task, if applicable.
    pub task_id: Option<String>,
    /// Send timestamp (ISO 8601).
    pub sent_at: String,
    /// Per-channel results: `{"channel_id": "ok" | "error"}`.
    pub channels: serde_json::Value,
    /// Global error if the notification could not be dispatched.
    pub error: Option<String>,
}

/// Lists all configured notification channels.
///
/// Delegates to `GET /api/v1/notifications/channels` on the internal REST API.
#[tauri::command]
pub async fn list_notification_channels(
    state: State<'_, RuntimeHandle>,
) -> Result<Vec<NotificationChannel>, String> {
    let json = http_get_json(state.api_port, "/api/v1/notifications/channels").await?;

    let channels = json
        .get("channels")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let result = channels
        .into_iter()
        .map(|ch| {
            // The REST API returns "id" (SQLite CRUD) or "channel_id" (legacy in-memory).
            let id = ch
                .get("id")
                .or_else(|| ch.get("channel_id"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            // Similarly, "channel_type" (SQLite) or "type" (legacy).
            let kind = ch
                .get("channel_type")
                .or_else(|| ch.get("type"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let label = ch.get("label").and_then(|v| v.as_str()).map(String::from);
            let min_interval_seconds = ch
                .get("min_interval_seconds")
                .and_then(|v| v.as_u64())
                .and_then(|n| u32::try_from(n).ok())
                .unwrap_or(0);
            NotificationChannel {
                channel_id: id,
                label,
                kind,
                enabled: ch.get("enabled").and_then(|v| v.as_bool()).unwrap_or(false),
                events: ch
                    .get("events")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|e| e.as_str().map(String::from))
                            .collect()
                    })
                    .unwrap_or_default(),
                min_interval_seconds,
            }
        })
        .collect();

    Ok(result)
}

/// Tests a specific notification channel.
///
/// Delegates to `POST /api/v1/notifications/test` and filters the result to
/// return only the requested channel.
#[tauri::command]
pub async fn test_notification_channel(
    state: State<'_, RuntimeHandle>,
    channel_id: String,
) -> Result<ChannelTestResult, String> {
    let body = serde_json::json!({});
    let json = http_post_json(state.api_port, "/api/v1/notifications/test", &body).await?;

    let results = json
        .get("results")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let found = results.into_iter().find(|r| {
        r.get("channel_id")
            .and_then(|v| v.as_str())
            .is_some_and(|id| id == channel_id)
    });

    match found {
        Some(r) => Ok(ChannelTestResult {
            channel_id: r
                .get("channel_id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            status: r
                .get("status")
                .and_then(|v| v.as_str())
                .unwrap_or("error")
                .to_string(),
            error: r.get("error").and_then(|v| v.as_str()).map(String::from),
            latency_ms: r.get("latency_ms").and_then(|v| v.as_u64()),
        }),
        None => Err(format!("channel '{channel_id}' not found in test results")),
    }
}

/// Fetches the notification history.
///
/// Delegates to `GET /api/v1/notifications/logs?last=N`.
#[tauri::command]
pub async fn get_notification_logs(
    state: State<'_, RuntimeHandle>,
    limit: Option<u32>,
) -> Result<Vec<NotificationLogEntry>, String> {
    let l = limit.unwrap_or(50);
    let path = format!("/api/v1/notifications/logs?last={l}");
    let json = http_get_json(state.api_port, &path).await?;

    let entries = json
        .get("entries")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let result = entries
        .into_iter()
        .map(|e| NotificationLogEntry {
            id: e
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            event_name: e
                .get("event_name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            task_id: e.get("task_id").and_then(|v| v.as_str()).map(String::from),
            sent_at: e
                .get("sent_at")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            channels: e
                .get("channels")
                .cloned()
                .unwrap_or(serde_json::Value::Object(serde_json::Map::new())),
            error: e.get("error").and_then(|v| v.as_str()).map(String::from),
        })
        .collect();

    Ok(result)
}

/// Lists informational runtime events (failures/degradations/LLM) extracted
/// from `notification_logs` over the last `days` days.
///
/// Filters Rust-side on `event_name` in `{ "task.failed", "agent.degraded",
/// "trigger.error", "llm.backend_down" }`. Successes (`task.completed`) are
/// deliberately excluded; they go to My assistants → Logs.
///
/// Delegates to `GET /api/v1/notifications/logs?last=500`, then filters.
#[tauri::command]
pub async fn list_runtime_activity(
    state: State<'_, RuntimeHandle>,
    days: Option<i64>,
) -> Result<Vec<NotificationLogEntry>, String> {
    const ACTIVITY_EVENTS: &[&str] = &[
        "task.failed",
        "agent.degraded",
        "trigger.error",
        "llm.backend_down",
    ];

    let window_days = days.unwrap_or(14).max(1);
    let cutoff = chrono::Utc::now() - chrono::Duration::days(window_days);

    let path = "/api/v1/notifications/logs?last=500";
    let json = http_get_json(state.api_port, path).await?;
    let entries = json
        .get("entries")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let result: Vec<NotificationLogEntry> = entries
        .into_iter()
        .filter_map(|e| {
            let event_name = e.get("event_name").and_then(|v| v.as_str()).unwrap_or("");
            if !ACTIVITY_EVENTS.contains(&event_name) {
                return None;
            }
            let sent_at_raw = e.get("sent_at").and_then(|v| v.as_str()).unwrap_or("");
            let in_window = chrono::DateTime::parse_from_rfc3339(sent_at_raw)
                .map(|dt| dt.with_timezone(&chrono::Utc) >= cutoff)
                .unwrap_or(true);
            if !in_window {
                return None;
            }
            Some(NotificationLogEntry {
                id: e
                    .get("id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                event_name: event_name.to_string(),
                task_id: e.get("task_id").and_then(|v| v.as_str()).map(String::from),
                sent_at: sent_at_raw.to_string(),
                channels: e
                    .get("channels")
                    .cloned()
                    .unwrap_or(serde_json::Value::Object(serde_json::Map::new())),
                error: e.get("error").and_then(|v| v.as_str()).map(String::from),
            })
        })
        .collect();

    Ok(result)
}

// ─── CRUD types & commands ──────────────────────────────────────────────────

/// Full definition of a notification channel returned by the CRUD operations.
#[derive(Debug, Serialize)]
pub struct NotificationChannelView {
    /// Unique channel identifier.
    pub id: String,
    /// Name displayed in the UI. `null` = falls back to `id` on the UI side.
    pub label: Option<String>,
    /// Channel type: `"desktop"` or `"webhook"`.
    pub channel_type: String,
    /// `true` if the channel is enabled.
    pub enabled: bool,
    /// Type-specific configuration.
    pub config: serde_json::Value,
    /// Channel-specific events. `null` = uses the global events.
    pub events: Option<Vec<String>>,
    /// Minimum throttling interval, in seconds (`0` = none).
    pub min_interval_seconds: u32,
    /// Creation timestamp (ISO 8601).
    pub created_at: String,
    /// Last-modification timestamp (ISO 8601).
    pub updated_at: String,
}

/// Request body for creating a channel via `create_notification_channel`.
#[derive(Debug, Serialize, Deserialize)]
pub struct CreateChannelRequest {
    /// Unique channel identifier.
    pub id: String,
    /// Name displayed in the UI (free-form, max 80 chars). `null` = no label.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// Channel type: `"desktop"` or `"webhook"`.
    pub channel_type: String,
    /// Whether the channel is active (default: `true`).
    pub enabled: Option<bool>,
    /// Type-specific configuration (e.g. `{"url": "..."}` for webhook).
    pub config: serde_json::Value,
    /// List of specific events. `null` = uses the global events.
    pub events: Option<Vec<String>>,
    /// Minimum throttling interval, in seconds. Default: `0` (none).
    #[serde(default, skip_serializing_if = "is_zero")]
    pub min_interval_seconds: u32,
}

fn is_zero(v: &u32) -> bool {
    *v == 0
}

/// Request body for updating a channel via `update_notification_channel`.
///
/// `label` uses an `Option<Option<String>>`:
/// - absent from JSON → keep;
/// - `null` → clear;
/// - `"text"` → replace.
#[derive(Debug, Serialize, Deserialize)]
pub struct UpdateChannelRequest {
    /// New label. See the struct doc for the semantics.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<Option<String>>,
    /// Channel type (optional, keeps the existing one if absent).
    pub channel_type: Option<String>,
    /// Whether the channel is active.
    pub enabled: Option<bool>,
    /// Type-specific configuration.
    pub config: Option<serde_json::Value>,
    /// List of specific events.
    pub events: Option<Vec<String>>,
    /// New minimum throttling interval (s). Absent = keep.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_interval_seconds: Option<u32>,
}

/// Parses an API response JSON into a `NotificationChannelView`.
fn parse_channel_view(json: &serde_json::Value) -> NotificationChannelView {
    NotificationChannelView {
        id: json
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        label: json.get("label").and_then(|v| v.as_str()).map(String::from),
        channel_type: json
            .get("channel_type")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        enabled: json
            .get("enabled")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        config: json
            .get("config")
            .cloned()
            .unwrap_or(serde_json::Value::Object(serde_json::Map::new())),
        events: json.get("events").and_then(|v| {
            v.as_array().map(|arr| {
                arr.iter()
                    .filter_map(|e| e.as_str().map(String::from))
                    .collect()
            })
        }),
        min_interval_seconds: json
            .get("min_interval_seconds")
            .and_then(|v| v.as_u64())
            .and_then(|n| u32::try_from(n).ok())
            .unwrap_or(0),
        created_at: json
            .get("created_at")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        updated_at: json
            .get("updated_at")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
    }
}

/// Creates a new notification channel.
///
/// Delegates to `POST /api/v1/notifications/channels`.
#[tauri::command]
pub async fn create_notification_channel(
    state: State<'_, RuntimeHandle>,
    channel: CreateChannelRequest,
) -> Result<NotificationChannelView, String> {
    let body =
        serde_json::to_value(&channel).map_err(|e| format!("failed to serialize request: {e}"))?;
    let json = http_post_json(state.api_port, "/api/v1/notifications/channels", &body).await?;
    Ok(parse_channel_view(&json))
}

/// Updates an existing notification channel.
///
/// Delegates to `PUT /api/v1/notifications/channels/:id`.
#[tauri::command]
pub async fn update_notification_channel(
    state: State<'_, RuntimeHandle>,
    id: String,
    channel: UpdateChannelRequest,
) -> Result<NotificationChannelView, String> {
    let body =
        serde_json::to_value(&channel).map_err(|e| format!("failed to serialize request: {e}"))?;
    let path = format!("/api/v1/notifications/channels/{id}");
    let json = http_put_json(state.api_port, &path, &body).await?;
    Ok(parse_channel_view(&json))
}

/// Deletes a notification channel.
///
/// Delegates to `DELETE /api/v1/notifications/channels/:id`.
#[tauri::command]
pub async fn delete_notification_channel(
    state: State<'_, RuntimeHandle>,
    id: String,
) -> Result<(), String> {
    let path = format!("/api/v1/notifications/channels/{id}");
    http_delete_json(state.api_port, &path).await?;
    Ok(())
}

/// Fetches the list of global notification events.
///
/// Delegates to `GET /api/v1/notifications/events`.
#[tauri::command]
pub async fn get_notification_events(
    state: State<'_, RuntimeHandle>,
) -> Result<Vec<String>, String> {
    let json = http_get_json(state.api_port, "/api/v1/notifications/events").await?;

    let events = json
        .get("events")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|e| e.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();

    Ok(events)
}

/// Replaces the list of global notification events.
///
/// Delegates to `PUT /api/v1/notifications/events`.
#[tauri::command]
pub async fn set_notification_events(
    state: State<'_, RuntimeHandle>,
    events: Vec<String>,
) -> Result<(), String> {
    let body = serde_json::json!({ "events": events });
    http_put_json(state.api_port, "/api/v1/notifications/events", &body).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_notification_channel_serializes() {
        // GIVEN a NotificationChannel
        let channel = NotificationChannel {
            channel_id: "desktop".to_string(),
            label: Some("Bureau".to_string()),
            kind: "desktop".to_string(),
            enabled: true,
            events: vec!["task.completed".to_string(), "pipeline.failed".to_string()],
            min_interval_seconds: 0,
        };

        // WHEN serialized to JSON
        let json = serde_json::to_value(&channel).expect("serialize");

        // THEN all fields are present with correct values
        assert_eq!(json["channel_id"], "desktop");
        assert_eq!(json["label"], "Bureau");
        assert_eq!(json["type"], "desktop");
        assert_eq!(json["enabled"], true);
        assert_eq!(json["events"][0], "task.completed");
        assert_eq!(json["events"][1], "pipeline.failed");
    }

    #[test]
    fn test_notification_channel_serializes_empty_events() {
        // GIVEN a NotificationChannel with no event filters and no label
        let channel = NotificationChannel {
            channel_id: "slack".to_string(),
            label: None,
            kind: "webhook".to_string(),
            enabled: false,
            events: vec![],
            min_interval_seconds: 0,
        };

        // WHEN serialized to JSON
        let json = serde_json::to_value(&channel).expect("serialize");

        // THEN events is an empty array
        assert_eq!(json["enabled"], false);
        assert!(json["events"].as_array().expect("array").is_empty());
    }

    #[test]
    fn test_channel_test_result_serializes_ok() {
        // GIVEN a successful ChannelTestResult
        let result = ChannelTestResult {
            channel_id: "desktop".to_string(),
            status: "ok".to_string(),
            error: None,
            latency_ms: Some(12),
        };

        // WHEN serialized to JSON
        let json = serde_json::to_value(&result).expect("serialize");

        // THEN status is ok and error is null
        assert_eq!(json["status"], "ok");
        assert!(json["error"].is_null());
        assert_eq!(json["latency_ms"], 12);
    }

    #[test]
    fn test_channel_test_result_serializes_error() {
        // GIVEN a failed ChannelTestResult
        let result = ChannelTestResult {
            channel_id: "slack".to_string(),
            status: "error".to_string(),
            error: Some("connection refused".to_string()),
            latency_ms: Some(5001),
        };

        // WHEN serialized to JSON
        let json = serde_json::to_value(&result).expect("serialize");

        // THEN status is error and error message is present
        assert_eq!(json["status"], "error");
        assert_eq!(json["error"], "connection refused");
    }

    #[test]
    fn test_notification_log_entry_serializes() {
        // GIVEN a NotificationLogEntry
        let entry = NotificationLogEntry {
            id: "abc123".to_string(),
            event_name: "task.completed".to_string(),
            task_id: Some("task-456".to_string()),
            sent_at: "2026-03-13T10:00:00Z".to_string(),
            channels: serde_json::json!({"desktop": "ok", "slack": "error"}),
            error: None,
        };

        // WHEN serialized to JSON
        let json = serde_json::to_value(&entry).expect("serialize");

        // THEN all fields are correct
        assert_eq!(json["event_name"], "task.completed");
        assert_eq!(json["task_id"], "task-456");
        assert_eq!(json["channels"]["desktop"], "ok");
        assert_eq!(json["channels"]["slack"], "error");
        assert!(json["error"].is_null());
    }

    #[test]
    fn test_notification_log_entry_serializes_with_error() {
        // GIVEN a NotificationLogEntry with a global error
        let entry = NotificationLogEntry {
            id: "def789".to_string(),
            event_name: "pipeline.failed".to_string(),
            task_id: None,
            sent_at: "2026-03-13T11:00:00Z".to_string(),
            channels: serde_json::json!({}),
            error: Some("all channels down".to_string()),
        };

        // WHEN serialized to JSON
        let json = serde_json::to_value(&entry).expect("serialize");

        // THEN task_id is null and error is present
        assert!(json["task_id"].is_null());
        assert_eq!(json["error"], "all channels down");
    }

    #[test]
    fn test_notification_channel_view_serializes() {
        // GIVEN a NotificationChannelView with specific events and a label
        let view = NotificationChannelView {
            id: "slack-ops".to_string(),
            label: Some("Slack — Équipe Ops".to_string()),
            channel_type: "webhook".to_string(),
            enabled: true,
            config: serde_json::json!({"url": "https://hooks.slack.com/xxx"}),
            events: Some(vec![
                "task.completed".to_string(),
                "pipeline.failed".to_string(),
            ]),
            min_interval_seconds: 0,
            created_at: "2026-03-20T10:00:00Z".to_string(),
            updated_at: "2026-03-20T10:00:00Z".to_string(),
        };

        // WHEN serialized to JSON
        let json = serde_json::to_value(&view).expect("serialize");

        // THEN all fields are present
        assert_eq!(json["id"], "slack-ops");
        assert_eq!(json["label"], "Slack — Équipe Ops");
        assert_eq!(json["channel_type"], "webhook");
        assert_eq!(json["enabled"], true);
        assert_eq!(json["config"]["url"], "https://hooks.slack.com/xxx");
        assert_eq!(json["events"][0], "task.completed");
        assert_eq!(json["created_at"], "2026-03-20T10:00:00Z");
    }

    #[test]
    fn test_notification_channel_view_serializes_no_events() {
        // GIVEN a NotificationChannelView using global events and no label
        let view = NotificationChannelView {
            id: "desktop".to_string(),
            label: None,
            channel_type: "desktop".to_string(),
            enabled: true,
            config: serde_json::json!({}),
            events: None,
            min_interval_seconds: 0,
            created_at: "2026-03-20T10:00:00Z".to_string(),
            updated_at: "2026-03-20T10:00:00Z".to_string(),
        };

        // WHEN serialized to JSON
        let json = serde_json::to_value(&view).expect("serialize");

        // THEN events is null
        assert!(json["events"].is_null());
    }

    #[test]
    fn test_parse_channel_view_from_api_response() {
        // GIVEN a JSON response matching API ChannelResponse
        let api_json = serde_json::json!({
            "id": "ch-test",
            "channel_type": "desktop",
            "enabled": true,
            "config": {},
            "events": ["task.completed"],
            "created_at": "2026-03-20T10:00:00Z",
            "updated_at": "2026-03-20T11:00:00Z"
        });

        // WHEN parsed
        let view = parse_channel_view(&api_json);

        // THEN all fields are correctly mapped
        assert_eq!(view.id, "ch-test");
        assert_eq!(view.channel_type, "desktop");
        assert!(view.enabled);
        assert_eq!(view.events.as_ref().map(|e| e.len()), Some(1));
    }

    #[test]
    fn test_create_channel_request_serializes() {
        // GIVEN a CreateChannelRequest with a free-form label
        let req = CreateChannelRequest {
            id: "new-channel".to_string(),
            label: Some("Alertes Slack équipe".to_string()),
            channel_type: "webhook".to_string(),
            enabled: Some(true),
            config: serde_json::json!({"url": "https://example.com/hook"}),
            events: None,
            min_interval_seconds: 0,
        };

        // WHEN serialized to JSON
        let json = serde_json::to_value(&req).expect("serialize");

        // THEN required fields are present, including label
        assert_eq!(json["id"], "new-channel");
        assert_eq!(json["label"], "Alertes Slack équipe");
        assert_eq!(json["channel_type"], "webhook");
        assert_eq!(json["config"]["url"], "https://example.com/hook");
    }

    #[test]
    fn test_create_channel_request_omits_label_when_none() {
        // GIVEN no label
        let req = CreateChannelRequest {
            id: "ch".to_string(),
            label: None,
            channel_type: "desktop".to_string(),
            enabled: None,
            config: serde_json::json!({}),
            events: None,
            min_interval_seconds: 0,
        };

        // WHEN serialized
        let json = serde_json::to_value(&req).expect("serialize");

        // THEN the label key is absent from the wire format (clean payload)
        assert!(json.get("label").is_none());
    }

    #[test]
    fn test_parse_channel_view_reads_label() {
        // GIVEN an API response containing a label
        let api_json = serde_json::json!({
            "id": "ch-test",
            "label": "Mon canal",
            "channel_type": "desktop",
            "enabled": true,
            "config": {},
            "events": null,
            "created_at": "2026-05-12T10:00:00Z",
            "updated_at": "2026-05-12T10:00:00Z"
        });

        // WHEN parsed
        let view = parse_channel_view(&api_json);

        // THEN the label is preserved
        assert_eq!(view.label.as_deref(), Some("Mon canal"));
    }

    #[test]
    fn test_parse_channel_view_label_absent() {
        // GIVEN an API response without a label field
        let api_json = serde_json::json!({
            "id": "legacy",
            "channel_type": "desktop",
            "enabled": true,
            "config": {},
            "events": null,
            "created_at": "2026-05-12T10:00:00Z",
            "updated_at": "2026-05-12T10:00:00Z"
        });

        // WHEN parsed
        let view = parse_channel_view(&api_json);

        // THEN label is None (no crash, just absent)
        assert_eq!(view.label, None);
    }
}
