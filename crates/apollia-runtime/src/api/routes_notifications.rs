//! REST routes for notifications.
//!
//! Exposes the notification management endpoints:
//! - `GET    /api/v1/notifications/channels`        , list channels (from SQLite)
//! - `POST   /api/v1/notifications/channels`        , create a channel
//! - `PUT    /api/v1/notifications/channels/:id`    , update a channel
//! - `DELETE /api/v1/notifications/channels/:id`    , delete a channel
//! - `GET    /api/v1/notifications/events`          , global events
//! - `PUT    /api/v1/notifications/events`          , replace global events
//! - `POST   /api/v1/notifications/channels/:id/test`, test a channel
//! - `POST   /api/v1/notifications/test`            , test all channels
//! - `GET    /api/v1/notifications/logs`            , history from notifications.db

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};

use apollia_notifications::{
    build_channels,
    config::{ChannelKind, NotificationConfig},
    engine::Notification,
    NotificationChannelRow, NotificationConfigError, NotificationLogRow, Severity,
};

use crate::api::server::AppState;
use crate::coordinator::ExecutionBackend;

// ─── Request types ──────────────────────────────────────────────────────────

/// Request body for `POST /api/v1/notifications/channels`.
#[derive(Debug, Deserialize)]
pub struct CreateChannelRequest {
    /// Unique channel identifier.
    pub id: String,
    /// Free-form display name. `None` falls back to `id` in the UI.
    #[serde(default)]
    pub label: Option<String>,
    /// Channel type: `"desktop"` or `"webhook"`.
    pub channel_type: String,
    /// Whether the channel is active (default: `true`).
    pub enabled: Option<bool>,
    /// Type-specific configuration (e.g. `{"url": "..."}` for webhook).
    pub config: serde_json::Value,
    /// Channel-specific event list. `null` uses the global events.
    pub events: Option<Vec<String>>,
    /// Minimum throttling interval, in seconds. Default: `0` (none).
    #[serde(default)]
    pub min_interval_seconds: u32,
}

/// Request body for `PUT /api/v1/notifications/channels/:id`.
///
/// The `label` field uses a double `Option`:
/// - absent from JSON: `None`, keep the existing label;
/// - `null`: `Some(None)`, clear the label;
/// - `"text"`: `Some(Some("text"))`, replace it.
#[derive(Debug, Deserialize)]
pub struct UpdateChannelRequest {
    /// New label. See the struct docs for the double-Option semantics.
    #[serde(default, deserialize_with = "deserialize_optional_field")]
    pub label: Option<Option<String>>,
    /// Channel type (optional, keeps the existing one if absent).
    pub channel_type: Option<String>,
    /// Whether the channel is active.
    pub enabled: Option<bool>,
    /// Type-specific configuration.
    pub config: Option<serde_json::Value>,
    /// Channel-specific event list.
    pub events: Option<Vec<String>>,
    /// New minimum throttling interval (s). Absent keeps the existing one.
    pub min_interval_seconds: Option<u32>,
}

/// Distinguishes `field: null` (`Some(None)`) from an absent field (`None`).
fn deserialize_optional_field<'de, T, D>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    T: serde::Deserialize<'de>,
    D: serde::Deserializer<'de>,
{
    Ok(Some(Option::<T>::deserialize(deserializer)?))
}

/// Request body for `PUT /api/v1/notifications/events`.
#[derive(Debug, Deserialize)]
pub struct SetEventsRequest {
    /// New list of global events.
    pub events: Vec<String>,
}

// ─── Response types ─────────────────────────────────────────────────────────

/// Full notification channel returned by the CRUD operations.
#[derive(Debug, Serialize)]
pub struct ChannelResponse {
    /// Unique channel identifier.
    pub id: String,
    /// Free-form display name. `null` falls back to `id` in the UI.
    pub label: Option<String>,
    /// Channel type.
    pub channel_type: String,
    /// `true` if the channel is enabled.
    pub enabled: bool,
    /// Type-specific configuration.
    pub config: serde_json::Value,
    /// Channel-specific events.
    pub events: Option<Vec<String>>,
    /// Minimum throttling interval, in seconds.
    pub min_interval_seconds: u32,
    /// Creation timestamp (ISO 8601).
    pub created_at: String,
    /// Last modification timestamp (ISO 8601).
    pub updated_at: String,
}

/// Response for `GET /api/v1/notifications/events`.
#[derive(Debug, Serialize)]
pub struct EventsResponse {
    /// List of global events.
    pub events: Vec<String>,
}

/// Public description of a channel returned by `GET /channels`.
#[derive(Debug, Serialize)]
pub struct ChannelInfo {
    /// Unique channel identifier (e.g. `"desktop"`, `"slack"`).
    pub channel_id: String,
    /// Display name (`None` falls back to `channel_id` in the UI).
    pub label: Option<String>,
    /// Channel type: `"desktop"`, `"webhook"`, or `"terminal"`.
    #[serde(rename = "type")]
    pub kind: String,
    /// `true` if the channel is enabled in the configuration.
    pub enabled: bool,
    /// List of events this channel accepts.
    pub events: Vec<String>,
    /// Minimum throttling interval, in seconds (`0` = none).
    pub min_interval_seconds: u32,
}

/// Result of testing an individual channel, returned by `POST /test`.
#[derive(Debug, Serialize, Deserialize)]
pub struct ChannelTestResult {
    /// Unique channel identifier.
    pub channel_id: String,
    /// Channel type.
    #[serde(rename = "type")]
    pub kind: String,
    /// Test status: `"ok"`, `"error"`, or `"disabled"`.
    pub status: String,
    /// Error message if `status == "error"`.
    pub error: Option<String>,
    /// Measured latency in milliseconds (`None` if the channel is disabled).
    pub latency_ms: Option<u64>,
}

/// Error response body.
#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    /// Error message.
    pub error: String,
}

/// Response for `DELETE /api/v1/notifications/channels/:id`.
#[derive(Debug, Serialize)]
pub struct DeleteResponse {
    /// Identifier of the deleted channel.
    pub deleted: String,
}

// ─── Query params ───────────────────────────────────────────────────────────

/// Query parameters for `GET /api/v1/notifications/logs`.
#[derive(Debug, Deserialize)]
pub struct LogsQuery {
    /// Maximum number of entries to return (default: 20, max: 1000).
    #[serde(default = "default_last")]
    pub last: usize,
}

fn default_last() -> usize {
    20
}

// ─── CRUD Handlers ──────────────────────────────────────────────────────────

/// `POST /api/v1/notifications/channels`, create a notification channel.
///
/// Validates the channel, inserts it into `notifications.db`, then reloads the
/// [`NotificationEngine`] via its handle.
pub async fn create_channel<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Json(body): Json<CreateChannelRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    let repo = match &state.notification_repo {
        Some(r) => Arc::clone(r),
        None => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({"error": "notification repository not available"})),
            );
        }
    };

    let row = NotificationChannelRow {
        id: body.id,
        label: body.label,
        channel_type: body.channel_type,
        enabled: body.enabled.unwrap_or(true),
        config_json: body.config,
        events_json: body.events,
        min_interval_seconds: body.min_interval_seconds,
        created_at: String::new(),
        updated_at: String::new(),
    };

    let created = {
        let guard = repo.lock().unwrap_or_else(|e| e.into_inner());
        if let Err(e) = guard.insert_channel(&row) {
            return map_notif_error(e);
        }
        match guard.get_channel(&row.id) {
            Ok(Some(ch)) => ch,
            Ok(None) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({"error": "channel inserted but not found"})),
                );
            }
            Err(e) => return map_notif_error(e),
        }
    };

    reload_notification_engine(&state, &repo).await;

    (
        StatusCode::CREATED,
        Json(serde_json::to_value(row_to_response(&created)).unwrap_or_default()),
    )
}

/// `PUT /api/v1/notifications/channels/:id`, update an existing channel.
///
/// Updates the channel in `notifications.db`, then reloads the
/// [`NotificationEngine`].
pub async fn update_channel<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path(id): Path<String>,
    Json(body): Json<UpdateChannelRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    let repo = match &state.notification_repo {
        Some(r) => Arc::clone(r),
        None => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({"error": "notification repository not available"})),
            );
        }
    };

    let updated = {
        let guard = repo.lock().unwrap_or_else(|e| e.into_inner());
        let existing = match guard.get_channel(&id) {
            Ok(Some(ch)) => ch,
            Ok(None) => {
                return (
                    StatusCode::NOT_FOUND,
                    Json(serde_json::json!({"error": format!("channel not found: {id}")})),
                );
            }
            Err(e) => return map_notif_error(e),
        };

        let merged = NotificationChannelRow {
            id: id.clone(),
            // Double-Option: absent keeps it; Some(None) clears it; Some(Some(s)) replaces it.
            label: match body.label {
                Some(value) => value,
                None => existing.label,
            },
            channel_type: body.channel_type.unwrap_or(existing.channel_type),
            enabled: body.enabled.unwrap_or(existing.enabled),
            config_json: body.config.unwrap_or(existing.config_json),
            events_json: body.events.or(existing.events_json),
            min_interval_seconds: body
                .min_interval_seconds
                .unwrap_or(existing.min_interval_seconds),
            created_at: existing.created_at,
            updated_at: existing.updated_at,
        };

        if let Err(e) = guard.update_channel(&id, &merged) {
            return map_notif_error(e);
        }
        match guard.get_channel(&id) {
            Ok(Some(ch)) => ch,
            Ok(None) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({"error": "channel updated but not found"})),
                );
            }
            Err(e) => return map_notif_error(e),
        }
    };

    reload_notification_engine(&state, &repo).await;

    (
        StatusCode::OK,
        Json(serde_json::to_value(row_to_response(&updated)).unwrap_or_default()),
    )
}

/// `DELETE /api/v1/notifications/channels/:id`, delete a channel.
///
/// Deletes the channel from `notifications.db`, then reloads the
/// [`NotificationEngine`].
pub async fn delete_channel<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path(id): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    let repo = match &state.notification_repo {
        Some(r) => Arc::clone(r),
        None => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({"error": "notification repository not available"})),
            );
        }
    };

    {
        let guard = repo.lock().unwrap_or_else(|e| e.into_inner());
        if let Err(e) = guard.delete_channel(&id) {
            return map_notif_error(e);
        }
    }

    reload_notification_engine(&state, &repo).await;

    (
        StatusCode::OK,
        Json(serde_json::to_value(DeleteResponse { deleted: id }).unwrap_or_default()),
    )
}

/// `GET /api/v1/notifications/events`, list global events.
pub async fn get_events<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
) -> (StatusCode, Json<serde_json::Value>) {
    let repo = match &state.notification_repo {
        Some(r) => Arc::clone(r),
        None => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({"error": "notification repository not available"})),
            );
        }
    };

    let events = {
        let guard = repo.lock().unwrap_or_else(|e| e.into_inner());
        match guard.get_global_events() {
            Ok(ev) => ev,
            Err(e) => return map_notif_error(e),
        }
    };

    (
        StatusCode::OK,
        Json(serde_json::to_value(EventsResponse { events }).unwrap_or_default()),
    )
}

/// `PUT /api/v1/notifications/events`, replace the global events.
///
/// Validates each event against [`KNOWN_EVENTS`](apollia_notifications::KNOWN_EVENTS),
/// then replaces the list in `notifications.db` and reloads the
/// [`NotificationEngine`].
pub async fn set_events<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Json(body): Json<SetEventsRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    let repo = match &state.notification_repo {
        Some(r) => Arc::clone(r),
        None => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({"error": "notification repository not available"})),
            );
        }
    };

    {
        let guard = repo.lock().unwrap_or_else(|e| e.into_inner());
        if let Err(e) = guard.set_global_events(&body.events) {
            return map_notif_error(e);
        }
    }

    reload_notification_engine(&state, &repo).await;

    let events = body.events;
    (
        StatusCode::OK,
        Json(serde_json::to_value(EventsResponse { events }).unwrap_or_default()),
    )
}

// ─── Existing Handlers ──────────────────────────────────────────────────────

/// `GET /api/v1/notifications/channels`, list configured channels.
///
/// Reads from the SQLite repository. Falls back to `notification_config`
/// if the repo is not available.
pub async fn list_channels<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
) -> Json<serde_json::Value> {
    // Prefer SQLite repo
    if let Some(ref repo) = state.notification_repo {
        let guard = repo.lock().unwrap_or_else(|e| e.into_inner());
        let channels: Vec<ChannelResponse> = match guard.list_channels() {
            Ok(rows) => rows.iter().map(row_to_response).collect(),
            Err(_) => vec![],
        };
        return Json(serde_json::json!({ "channels": channels }));
    }

    // Fallback: in-memory config (backward compat)
    let Some(config) = &state.notification_config else {
        return Json(serde_json::json!({ "channels": [] }));
    };

    let channels: Vec<ChannelInfo> = config
        .channels
        .iter()
        .map(|ch| ChannelInfo {
            channel_id: ch.id.clone(),
            label: None,
            kind: channel_kind_str(&ch.kind),
            enabled: ch.enabled,
            events: ch.events.clone().unwrap_or_else(|| config.events.clone()),
            min_interval_seconds: ch.min_interval_seconds,
        })
        .collect();

    Json(serde_json::json!({ "channels": channels }))
}

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
fn write_test_log<B: ExecutionBackend + Clone>(
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
        tracing::warn!(error = %e, "notification_logs : impossible d'écrire le log de test");
    }
}

/// Resolves the *live* notification config.
///
/// Reads from the SQLite repository when available (so CRUD-created channels
/// are visible without an engine reload-and-snapshot cycle), and falls back
/// to the boot-time `state.notification_config` otherwise. Returns `None`
/// when neither source has anything (e.g. tests with both fields unset).
fn resolve_live_config<B: ExecutionBackend + Clone>(
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

/// `GET /api/v1/notifications/logs?last=N`, notification history.
///
/// Reads from `notifications.db` via the repository.
/// Falls back to `hitl.db` if the repo is not available (backward compat).
pub async fn notification_logs<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Query(params): Query<LogsQuery>,
) -> (StatusCode, Json<serde_json::Value>) {
    let last = params.last.min(1000);

    // Prefer notifications.db repo
    if let Some(ref repo) = state.notification_repo {
        let guard = repo.lock().unwrap_or_else(|e| e.into_inner());
        match guard.query_logs(last) {
            Ok(logs) => {
                let entries: Vec<serde_json::Value> = logs
                    .iter()
                    .map(|log| {
                        let channels = serde_json::from_str(&log.channels)
                            .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));
                        serde_json::json!({
                            "id": log.id,
                            "event_name": log.event_name,
                            "task_id": log.task_id,
                            "agent_id": log.agent_id,
                            "sent_at": log.sent_at,
                            "channels": channels,
                            "error": log.error,
                        })
                    })
                    .collect();
                return (
                    StatusCode::OK,
                    Json(serde_json::json!({ "entries": entries })),
                );
            }
            Err(e) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({ "error": e.to_string() })),
                );
            }
        }
    }

    // Fallback: hitl.db (backward compat)
    let db_path = resolve_notif_db_path(&state);
    let entries_result =
        tokio::task::spawn_blocking(move || query_notification_logs(&db_path, last)).await;

    match entries_result {
        Ok(Ok(entries)) => (
            StatusCode::OK,
            Json(serde_json::json!({ "entries": entries })),
        ),
        Ok(Err(msg)) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": msg })),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": format!("spawn_blocking failed: {e}") })),
        ),
    }
}

// ─── Helpers ────────────────────────────────────────────────────────────────

/// Converts a [`NotificationChannelRow`] into a [`ChannelResponse`].
fn row_to_response(row: &NotificationChannelRow) -> ChannelResponse {
    ChannelResponse {
        id: row.id.clone(),
        label: row.label.clone(),
        channel_type: row.channel_type.clone(),
        enabled: row.enabled,
        config: row.config_json.clone(),
        events: row.events_json.clone(),
        min_interval_seconds: row.min_interval_seconds,
        created_at: row.created_at.clone(),
        updated_at: row.updated_at.clone(),
    }
}

/// Maps a [`NotificationConfigError`] to a `(StatusCode, JSON)` pair.
fn map_notif_error(err: NotificationConfigError) -> (StatusCode, Json<serde_json::Value>) {
    let (status, msg) = match &err {
        NotificationConfigError::NotFound(_) => (StatusCode::NOT_FOUND, err.to_string()),
        NotificationConfigError::DuplicateId(_) => (StatusCode::CONFLICT, err.to_string()),
        NotificationConfigError::ValidationError(_) => {
            (StatusCode::UNPROCESSABLE_ENTITY, err.to_string())
        }
        NotificationConfigError::Database(_) => {
            (StatusCode::INTERNAL_SERVER_ERROR, err.to_string())
        }
    };
    (status, Json(serde_json::json!({ "error": msg })))
}

/// Reloads the [`NotificationEngine`] from the repository data.
///
/// Reads the channels and global events from the repo, builds the new
/// [`NotificationConfig`], instantiates the concrete channels via
/// [`build_channels`], then sends everything to the engine for hot-reload.
async fn reload_notification_engine<B: ExecutionBackend + Clone>(
    state: &AppState<B>,
    repo: &Arc<std::sync::Mutex<apollia_notifications::NotificationConfigRepository>>,
) {
    let engine_handle = match &state.notification_engine_handle {
        Some(h) => h.clone(),
        None => return,
    };

    let (channel_rows, global_events) = {
        let guard = repo.lock().unwrap_or_else(|e| e.into_inner());
        let rows = guard.list_channels().unwrap_or_default();
        let events = guard.get_global_events().unwrap_or_default();
        (rows, events)
    };

    let channel_configs: Vec<apollia_notifications::ChannelConfig> = channel_rows
        .iter()
        .map(|row| row.to_channel_config())
        .collect();

    let config = NotificationConfig {
        events: global_events,
        channels: channel_configs,
        inactivity_timeout_secs: 30,
    };

    let channels = match build_channels(&config.channels) {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!(error = %e, "notification reload: failed to build channels");
            return;
        }
    };

    engine_handle.reload(config, channels).await;
}

/// Build a test [`Notification`] for the `"test.ping"` event.
fn make_test_notification() -> Notification {
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

/// Return the string representation of a [`ChannelKind`].
fn channel_kind_str(kind: &ChannelKind) -> String {
    match kind {
        ChannelKind::Desktop => "desktop".to_string(),
        ChannelKind::Webhook => "webhook".to_string(),
        ChannelKind::Terminal => "terminal".to_string(),
    }
}

/// Return the `kind` string for the channel identified by `id` in `config`.
///
/// Falls back to `"unknown"` if the ID is not found.
fn channel_kind_by_id(id: &str, config: &NotificationConfig) -> String {
    config
        .channels
        .iter()
        .find(|ch| ch.id == id)
        .map(|ch| channel_kind_str(&ch.kind))
        .unwrap_or_else(|| "unknown".to_string())
}

/// Resolve the path of the notification log database.
///
/// Uses `~/.apollia/hitl.db` by default.
fn resolve_notif_db_path<B: ExecutionBackend + Clone>(state: &AppState<B>) -> std::path::PathBuf {
    state
        .task_repository
        .as_ref()
        .and_then(|_| std::env::var("HOME").ok())
        .map(|home| std::path::PathBuf::from(format!("{home}/.apollia/hitl.db")))
        .or_else(|| {
            std::env::var("HOME")
                .ok()
                .map(|home| std::path::PathBuf::from(format!("{home}/.apollia/hitl.db")))
        })
        .unwrap_or_else(|| std::path::PathBuf::from("/tmp/apollia-notif.db"))
}

/// Open `db_path`, create `notification_logs` if needed, and return the last `N` entries.
fn query_notification_logs(
    db_path: &std::path::Path,
    last: usize,
) -> Result<Vec<serde_json::Value>, String> {
    let conn = rusqlite::Connection::open(db_path)
        .map_err(|e| format!("impossible d'ouvrir la base : {e}"))?;

    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS notification_logs (
            id          TEXT    PRIMARY KEY,
            event_name  TEXT    NOT NULL,
            task_id     TEXT,
            agent_id    TEXT,
            sent_at     TEXT    NOT NULL DEFAULT (datetime('now')),
            channels    TEXT    NOT NULL DEFAULT '{}',
            error       TEXT
        );
        CREATE INDEX IF NOT EXISTS idx_notif_logs_sent_at ON notification_logs(sent_at);",
    )
    .map_err(|e| format!("migration notification_logs échouée : {e}"))?;

    let mut stmt = conn
        .prepare(
            "SELECT id, event_name, task_id, agent_id, sent_at, channels, error
               FROM notification_logs
              ORDER BY sent_at DESC
              LIMIT ?1",
        )
        .map_err(|e| format!("prepare échoué : {e}"))?;

    let rows = stmt
        .query_map(rusqlite::params![last as i64], |row| {
            let channels_raw: String = row.get(5)?;
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, String>(4)?,
                channels_raw,
                row.get::<_, Option<String>>(6)?,
            ))
        })
        .map_err(|e| format!("query échouée : {e}"))?;

    let mut entries = Vec::new();
    for row_result in rows {
        let (id, event_name, task_id, agent_id, sent_at, channels_raw, error) =
            row_result.map_err(|e| format!("lecture ligne échouée : {e}"))?;

        let channels = serde_json::from_str(&channels_raw)
            .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));

        entries.push(serde_json::json!({
            "id": id,
            "event_name": event_name,
            "task_id": task_id,
            "agent_id": agent_id,
            "sent_at": sent_at,
            "channels": channels,
            "error": error,
        }));
    }

    Ok(entries)
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use apollia_notifications::config::{ChannelConfig, ChannelKind, NotificationConfig};
    use apollia_notifications::NotificationConfigRepository;

    use axum::body::Body;
    use axum::http::Request;
    use axum::Router;
    use tower::ServiceExt;

    use crate::coordinator::{DynBackend, ExecutionBackend};
    use crate::eventbus::EventBus;
    use crate::registry::AgentRegistry;
    use crate::router::TaskRouterHandle;
    use apollia_core::{AIPResult, AIPTask, TaskStatus};
    use std::future::Future;
    use std::pin::Pin;

    #[derive(Clone)]
    struct MockBackend;

    impl From<DynBackend> for MockBackend {
        fn from(_: DynBackend) -> Self {
            MockBackend
        }
    }

    impl ExecutionBackend for MockBackend {
        fn execute(
            &self,
            _task: AIPTask,
        ) -> Pin<Box<dyn Future<Output = Result<AIPResult, String>> + Send>> {
            Box::pin(async {
                Ok(AIPResult {
                    task_id: String::new(),
                    status: TaskStatus::Completed,
                    output: Vec::new(),
                    error: None,
                    artifacts: Vec::new(),
                    input_required_data: None,
                })
            })
        }
    }

    fn make_state_with_repo(db_path: &std::path::Path) -> AppState<MockBackend> {
        let (event_tx, _) = EventBus::new();
        let registry = AgentRegistry::spawn(event_tx.clone());
        let router_handle: TaskRouterHandle<MockBackend> =
            TaskRouterHandle::spawn(registry.clone(), event_tx.clone(), 64);
        let repo = NotificationConfigRepository::open(db_path).expect("open notifications.db");
        AppState {
            router_handle,
            registry_handle: registry,
            event_sender: event_tx,
            agent_loader: Arc::new(crate::api::routes_agents::StubAgentLoader),
            backend: MockBackend,
            llm_router: crate::api::server::empty_shared_llm_router(),
            trigger_engine: None,
            config_path: None,
            task_repository: None,
            pending_approvals: None,
            plan_gates: None,
            notification_config: None,
            backend_factory: None,
            tool_registry_handle: None,
            audit_trail: None,
            obs_config: apollia_core::ObservabilityConfig::default(),
            llm_call_repository: None,
            trigger_def_repo: None,
            notification_repo: Some(Arc::new(std::sync::Mutex::new(repo))),
            notification_engine_handle: None,
            chat_manager: None,
            plan_cache: None,
            mailbox_handle: None,
            user_memory: None,
            stt_engine: None,
            stt_repository: None,
            mcp_handle: None,
            mcp_server_repo: None,
            llm_backend_repo: None,
            stt_config_repo: None,
            a2a_invoker: None,
            resilience_layer: None,
            runner_proxy: None,
        }
    }

    fn make_crud_router(state: AppState<MockBackend>) -> Router {
        Router::new()
            .route(
                "/api/v1/notifications/channels",
                axum::routing::get(list_channels::<MockBackend>)
                    .post(create_channel::<MockBackend>),
            )
            .route(
                "/api/v1/notifications/channels/:id",
                axum::routing::put(update_channel::<MockBackend>)
                    .delete(delete_channel::<MockBackend>),
            )
            .route(
                "/api/v1/notifications/events",
                axum::routing::get(get_events::<MockBackend>).put(set_events::<MockBackend>),
            )
            .route(
                "/api/v1/notifications/logs",
                axum::routing::get(notification_logs::<MockBackend>),
            )
            .with_state(state)
    }

    async fn read_body(resp: axum::response::Response) -> serde_json::Value {
        let body = axum::body::to_bytes(resp.into_body(), 65536)
            .await
            .expect("read body");
        serde_json::from_slice(&body).expect("parse JSON")
    }

    // ── POST /api/v1/notifications/channels -> 201 ──────────────────────────

    #[tokio::test]
    async fn test_create_channel_201() {
        // GIVEN an empty repository
        let dir = tempfile::TempDir::new().expect("tempdir");
        let state = make_state_with_repo(&dir.path().join("notifications.db"));
        let router = make_crud_router(state);

        // WHEN POST with a valid webhook channel
        let body = serde_json::json!({
            "id": "slack-ops",
            "channel_type": "webhook",
            "config": {"url": "https://hooks.slack.com/test"}
        });
        let req = Request::builder()
            .method("POST")
            .uri("/api/v1/notifications/channels")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).expect("json")))
            .expect("build request");
        let resp = router.oneshot(req).await.expect("oneshot");

        // THEN 201 with the full channel
        assert_eq!(resp.status(), StatusCode::CREATED);
        let json = read_body(resp).await;
        assert_eq!(json["id"], "slack-ops");
        assert_eq!(json["channel_type"], "webhook");
        assert_eq!(json["enabled"], true);
        assert!(!json["created_at"].as_str().unwrap_or("").is_empty());
    }

    // ── PUT /api/v1/notifications/channels/:id -> 200 ───────────────────────

    #[tokio::test]
    async fn test_update_channel_200() {
        // GIVEN an existing channel
        let dir = tempfile::TempDir::new().expect("tempdir");
        let state = make_state_with_repo(&dir.path().join("notifications.db"));
        let router = make_crud_router(state);

        let create_body = serde_json::json!({
            "id": "slack-ops",
            "channel_type": "webhook",
            "config": {"url": "https://hooks.slack.com/old"}
        });
        let req = Request::builder()
            .method("POST")
            .uri("/api/v1/notifications/channels")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&create_body).expect("json")))
            .expect("build request");
        let resp = router.clone().oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::CREATED);

        // WHEN PUT with a new URL
        let update_body = serde_json::json!({
            "config": {"url": "https://hooks.slack.com/new"}
        });
        let req = Request::builder()
            .method("PUT")
            .uri("/api/v1/notifications/channels/slack-ops")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&update_body).expect("json")))
            .expect("build request");
        let resp = router.oneshot(req).await.expect("oneshot");

        // THEN 200 with the new URL
        assert_eq!(resp.status(), StatusCode::OK);
        let json = read_body(resp).await;
        assert_eq!(json["id"], "slack-ops");
        assert_eq!(json["config"]["url"], "https://hooks.slack.com/new");
    }

    // ── DELETE /api/v1/notifications/channels/:id -> 200 ────────────────────

    #[tokio::test]
    async fn test_delete_channel_200() {
        // GIVEN an existing channel
        let dir = tempfile::TempDir::new().expect("tempdir");
        let state = make_state_with_repo(&dir.path().join("notifications.db"));
        let router = make_crud_router(state);

        let body = serde_json::json!({
            "id": "slack-ops",
            "channel_type": "webhook",
            "config": {"url": "https://hooks.slack.com/test"}
        });
        let req = Request::builder()
            .method("POST")
            .uri("/api/v1/notifications/channels")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).expect("json")))
            .expect("build request");
        let resp = router.clone().oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::CREATED);

        // WHEN DELETE
        let req = Request::builder()
            .method("DELETE")
            .uri("/api/v1/notifications/channels/slack-ops")
            .body(Body::empty())
            .expect("build request");
        let resp = router.clone().oneshot(req).await.expect("oneshot");

        // THEN 200
        assert_eq!(resp.status(), StatusCode::OK);
        let json = read_body(resp).await;
        assert_eq!(json["deleted"], "slack-ops");

        // AND the channel no longer exists
        let req = Request::builder()
            .method("GET")
            .uri("/api/v1/notifications/channels")
            .body(Body::empty())
            .expect("build request");
        let resp = router.oneshot(req).await.expect("oneshot");
        let json = read_body(resp).await;
        assert_eq!(json["channels"].as_array().map(|a| a.len()), Some(0));
    }

    // ── GET /api/v1/notifications/events ────────────────────────────────────

    #[tokio::test]
    async fn test_get_events() {
        // GIVEN configured global events
        let dir = tempfile::TempDir::new().expect("tempdir");
        let state = make_state_with_repo(&dir.path().join("notifications.db"));

        // Insert events via repo
        {
            let guard = state
                .notification_repo
                .as_ref()
                .expect("repo")
                .lock()
                .expect("lock");
            guard
                .set_global_events(&["task.completed".into(), "task.failed".into()])
                .expect("set events");
        }

        let router = make_crud_router(state);

        // WHEN GET /events
        let req = Request::builder()
            .method("GET")
            .uri("/api/v1/notifications/events")
            .body(Body::empty())
            .expect("build request");
        let resp = router.oneshot(req).await.expect("oneshot");

        // THEN 200 with the events
        assert_eq!(resp.status(), StatusCode::OK);
        let json = read_body(resp).await;
        let events = json["events"].as_array().expect("events array");
        assert_eq!(events.len(), 2);
        assert!(events.contains(&serde_json::json!("task.completed")));
        assert!(events.contains(&serde_json::json!("task.failed")));
    }

    // ── PUT /api/v1/notifications/events -> 200 ─────────────────────────────

    #[tokio::test]
    async fn test_set_events() {
        // GIVEN an empty repository
        let dir = tempfile::TempDir::new().expect("tempdir");
        let state = make_state_with_repo(&dir.path().join("notifications.db"));
        let router = make_crud_router(state);

        // WHEN PUT with new events
        let body = serde_json::json!({
            "events": ["task.completed", "pipeline.failed"]
        });
        let req = Request::builder()
            .method("PUT")
            .uri("/api/v1/notifications/events")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).expect("json")))
            .expect("build request");
        let resp = router.oneshot(req).await.expect("oneshot");

        // THEN 200 with the updated events
        assert_eq!(resp.status(), StatusCode::OK);
        let json = read_body(resp).await;
        let events = json["events"].as_array().expect("events array");
        assert_eq!(events.len(), 2);
    }

    // ── Webhook validation without URL -> 422 ───────────────────────────────

    #[tokio::test]
    async fn test_validation_webhook_no_url_422() {
        // GIVEN
        let dir = tempfile::TempDir::new().expect("tempdir");
        let state = make_state_with_repo(&dir.path().join("notifications.db"));
        let router = make_crud_router(state);

        // WHEN POST webhook without url
        let body = serde_json::json!({
            "id": "bad-webhook",
            "channel_type": "webhook",
            "config": {}
        });
        let req = Request::builder()
            .method("POST")
            .uri("/api/v1/notifications/channels")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).expect("json")))
            .expect("build request");
        let resp = router.oneshot(req).await.expect("oneshot");

        // THEN 422 with a validation message
        assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let json = read_body(resp).await;
        let error = json["error"].as_str().expect("error string");
        assert!(
            error.contains("url"),
            "expected error about url, got: {error}"
        );
    }

    // ── Unknown event validation -> 422 ─────────────────────────────────────

    #[tokio::test]
    async fn test_validation_unknown_event_422() {
        // GIVEN
        let dir = tempfile::TempDir::new().expect("tempdir");
        let state = make_state_with_repo(&dir.path().join("notifications.db"));
        let router = make_crud_router(state);

        // WHEN PUT events with an unknown event
        let body = serde_json::json!({
            "events": ["bad.event"]
        });
        let req = Request::builder()
            .method("PUT")
            .uri("/api/v1/notifications/events")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).expect("json")))
            .expect("build request");
        let resp = router.oneshot(req).await.expect("oneshot");

        // THEN 422 with a validation message
        assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let json = read_body(resp).await;
        let error = json["error"].as_str().expect("error string");
        assert!(
            error.contains("bad.event"),
            "expected error about bad.event, got: {error}"
        );
    }

    // ── resolve_live_config (regression: test endpoint stale snapshot) ──────

    #[tokio::test]
    async fn test_resolve_live_config_reads_repo_first() {
        // GIVEN a repo holding a channel created later, and an empty snapshot
        let dir = tempfile::TempDir::new().expect("tempdir");
        let repo =
            NotificationConfigRepository::open(&dir.path().join("notifications.db")).expect("open");
        repo.insert_channel(&apollia_notifications::NotificationChannelRow {
            id: "test-discord".into(),
            label: Some("Discord test".into()),
            channel_type: "webhook".into(),
            enabled: true,
            config_json: serde_json::json!({"url": "https://discord.com/api/webhooks/x/y"}),
            events_json: None,
            min_interval_seconds: 0,
            created_at: String::new(),
            updated_at: String::new(),
        })
        .expect("insert");

        let mut state = make_state_with_repo(&dir.path().join("notifications.db"));
        // Override the repo with the one we just seeded
        state.notification_repo = Some(Arc::new(std::sync::Mutex::new(
            NotificationConfigRepository::open(&dir.path().join("notifications.db")).expect("open"),
        )));
        // notification_config (snapshot) deliberately absent: this reproduces
        // the case where the channel is known only to the repo.
        state.notification_config = None;

        // WHEN
        let resolved = resolve_live_config(&state).expect("config resolved");

        // THEN the repo channel is present
        assert_eq!(resolved.channels.len(), 1);
        assert_eq!(resolved.channels[0].id, "test-discord");
        assert!(matches!(resolved.channels[0].kind, ChannelKind::Webhook));
    }

    #[tokio::test]
    async fn test_resolve_live_config_falls_back_to_snapshot_when_repo_empty() {
        // GIVEN an empty repo but a non-empty legacy snapshot
        let dir = tempfile::TempDir::new().expect("tempdir");
        let mut state = make_state_with_repo(&dir.path().join("notifications.db"));
        state.notification_config = Some(NotificationConfig {
            events: vec![],
            channels: vec![ChannelConfig {
                id: "legacy".into(),
                kind: ChannelKind::Desktop,
                enabled: true,
                events: None,
                url: None,
                signing_secret: None,
                min_severity: None,
                min_interval_seconds: 0,
            }],
            inactivity_timeout_secs: 30,
        });

        // WHEN
        let resolved = resolve_live_config(&state).expect("config resolved");

        // THEN we fall back to the snapshot
        assert_eq!(resolved.channels.len(), 1);
        assert_eq!(resolved.channels[0].id, "legacy");
    }

    // ── Type tests ──────────────────────────────────────────────────────────

    #[test]
    fn test_channel_test_result_json_structure_ok() {
        // GIVEN
        let result = ChannelTestResult {
            channel_id: "desktop".to_string(),
            kind: "desktop".to_string(),
            status: "ok".to_string(),
            error: None,
            latency_ms: Some(12),
        };

        // WHEN
        let json = serde_json::to_value(&result).expect("sérialisation");

        // THEN
        assert_eq!(json["channel_id"], "desktop");
        assert_eq!(json["type"], "desktop");
        assert_eq!(json["status"], "ok");
        assert!(json["error"].is_null());
        assert_eq!(json["latency_ms"], 12);
    }

    #[test]
    fn test_channel_kind_str_all_variants() {
        assert_eq!(channel_kind_str(&ChannelKind::Desktop), "desktop");
        assert_eq!(channel_kind_str(&ChannelKind::Webhook), "webhook");
        assert_eq!(channel_kind_str(&ChannelKind::Terminal), "terminal");
    }

    #[test]
    fn test_channel_kind_by_id_found() {
        let config = NotificationConfig {
            events: vec![],
            channels: vec![ChannelConfig {
                id: "mon-desktop".to_string(),
                kind: ChannelKind::Desktop,
                enabled: true,
                events: None,
                url: None,
                signing_secret: None,
                min_severity: None,
                min_interval_seconds: 0,
            }],
            inactivity_timeout_secs: 30,
        };
        assert_eq!(channel_kind_by_id("mon-desktop", &config), "desktop");
    }

    #[test]
    fn test_channel_kind_by_id_not_found_returns_unknown() {
        let config = NotificationConfig {
            events: vec![],
            channels: vec![],
            inactivity_timeout_secs: 30,
        };
        assert_eq!(channel_kind_by_id("inconnu", &config), "unknown");
    }

    #[test]
    fn test_logs_lazy_table_creation_returns_empty() {
        let dir = tempfile::tempdir().expect("tempdir");
        let db_path = dir.path().join("test.db");
        let result = query_notification_logs(&db_path, 20);
        let entries = result.expect("query_notification_logs");
        assert!(entries.is_empty());
    }
}
