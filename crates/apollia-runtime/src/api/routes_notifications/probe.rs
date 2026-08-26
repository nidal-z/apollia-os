//! The test-dispatch endpoint, `POST /api/v1/notifications/test`.
//!
//! Builds every enabled channel from the live repository, sends one
//! `test.ping` notification through each, and records the outcome in the
//! notification log like a real dispatch would.

use std::collections::HashMap;
use std::time::Instant;

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;

use apollia_notifications::{
    build_channels, config::NotificationConfig, engine::Notification, NotificationLogRow, Severity,
};

use crate::api::routes_notifications::{channel_kind_by_id, channel_kind_str, ChannelTestResult};
use crate::api::server::AppState;
use crate::coordinator::ExecutionBackend;

/// `POST /api/v1/notifications/test`, send a test notification.
///
/// For each channel enabled in the config:
/// - Instantiates the channel via [`build_channels`]
/// - Sends a test [`Notification`] with the `"test.ping"` event
/// - Measures latency and collects the status (`"ok"`, `"error"`, `"disabled"`)
///
/// Source of truth: the channel list is read from the SQLite repository, not
/// from the `state.notification_config` snapshot (which is frozen at boot and
/// does not reflect CRUD performed via the API). Falling back to the snapshot
/// when the repo is unavailable preserves the legacy behavior for
/// `apollia.toml`-only config.
#[utoipa::path(
    post,
    path = "/api/v1/notifications/test",
    tag = "notifications",
    responses(
        (status = 200, description = "Per-channel test results"),
        (status = 500, description = "Failed to build channels", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub async fn test_channels<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
) -> (StatusCode, Json<serde_json::Value>) {
    let config = match resolve_live_config(&state) {
        Some(cfg) => cfg,
        None => return (StatusCode::OK, Json(serde_json::json!({ "results": [] }))),
    };

    let mut results: Vec<ChannelTestResult> = Vec::new();

    for ch in &config.channels {
        if !ch.enabled {
            results.push(ChannelTestResult {
                channel_id: ch.id.clone(),
                kind: channel_kind_str(&ch.kind),
                status: "disabled".to_string(),
                error: None,
                latency_ms: None,
            });
        }
    }

    let channels = match build_channels(&config.channels) {
        Ok(c) => c,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": e.to_string() })),
            );
        }
    };

    let test_notif = make_test_notification();

    // Per-channel send results, kept alongside the typed `results` so we can
    // persist a `notification_logs` row mirroring what the engine writes when
    // it dispatches real events.
    let mut channel_results: HashMap<String, Option<String>> = HashMap::new();

    for channel in &channels {
        let start = Instant::now();
        let outcome = channel.send(&test_notif).await;
        let latency_ms = start.elapsed().as_millis() as u64;
        let kind = channel_kind_by_id(channel.id(), &config);

        let (status, error) = match outcome {
            Ok(()) => ("ok".to_string(), None),
            Err(e) => ("error".to_string(), Some(e.to_string())),
        };
        channel_results.insert(channel.id().to_string(), error.clone());

        results.push(ChannelTestResult {
            channel_id: channel.id().to_string(),
            kind,
            status,
            error,
            latency_ms: Some(latency_ms),
        });
    }

    // Persist a log row so the "Notifications envoyées" tab shows test fires
    // (the engine's own logging path is bypassed by direct channel sends).
    if !channel_results.is_empty() {
        write_test_log(&state, &test_notif, &channel_results);
    }

    (
        StatusCode::OK,
        Json(serde_json::json!({ "results": results })),
    )
}

/// Persists a `notification_logs` row for a test-channel dispatch.
///
/// Mirrors the shape that the engine writes when it processes a real
/// `RuntimeEvent`. Silently no-ops if `notification_repo` is unset
/// (e.g. in unit tests) or if the write fails, the test endpoint must
/// not error out on best-effort logging.
pub(super) fn write_test_log<B: ExecutionBackend + Clone>(
    state: &AppState<B>,
    notif: &Notification,
    channel_results: &HashMap<String, Option<String>>,
) {
    let Some(repo) = state.notification_repo.as_ref() else {
        return;
    };

    let channels_json: serde_json::Map<String, serde_json::Value> = channel_results
        .iter()
        .map(|(id, err)| {
            let status = match err {
                None => serde_json::Value::String("ok".into()),
                Some(msg) => serde_json::Value::String(msg.clone()),
            };
            (id.clone(), status)
        })
        .collect();
    let channels_str = serde_json::to_string(&channels_json).unwrap_or_else(|_| "{}".into());

    let global_error: Option<String> = channel_results
        .values()
        .find_map(|e| e.as_deref().map(str::to_string));

    let row = NotificationLogRow {
        id: uuid::Uuid::new_v4().to_string(),
        event_name: notif.event.clone(),
        task_id: notif.task_id.clone(),
        agent_id: notif.agent.clone(),
        sent_at: notif.timestamp.to_rfc3339(),
        channels: channels_str,
        error: global_error,
    };

    let guard = match repo.lock() {
        Ok(g) => g,
        Err(e) => e.into_inner(),
    };
    if let Err(e) = guard.write_log(&row) {
        tracing::warn!(error = %e, "notification.test_log.write.failed");
    }
}

/// Resolves the *live* notification config.
///
/// Reads from the SQLite repository when available (so CRUD-created channels
/// are visible without an engine reload-and-snapshot cycle), and falls back
/// to the boot-time `state.notification_config` otherwise. Returns `None`
/// when neither source has anything (e.g. tests with both fields unset).
pub(super) fn resolve_live_config<B: ExecutionBackend + Clone>(
    state: &AppState<B>,
) -> Option<NotificationConfig> {
    if let Some(repo) = &state.notification_repo {
        let guard = repo.lock().unwrap_or_else(|e| e.into_inner());
        let rows = guard.list_channels().unwrap_or_default();
        let events = guard.get_global_events().unwrap_or_default();
        if rows.is_empty() && events.is_empty() {
            // Empty repo, fall back so tests that only set `notification_config`
            // (no repo) keep working.
            return state.notification_config.clone();
        }
        let inactivity = state
            .notification_config
            .as_ref()
            .map(|c| c.inactivity_timeout_secs)
            .unwrap_or(30);
        return Some(NotificationConfig {
            events,
            channels: rows.iter().map(|row| row.to_channel_config()).collect(),
            inactivity_timeout_secs: inactivity,
        });
    }
    state.notification_config.clone()
}
/// Build a test [`Notification`] for the `"test.ping"` event.
pub(super) fn make_test_notification() -> Notification {
    Notification {
        event: "test.ping".to_string(),
        timestamp: chrono::Utc::now(),
        task_id: None,
        agent: None,
        message: "Notification de test Apollia OS".to_string(),
        metadata: HashMap::new(),
        severity: Severity::Info,
    }
}
