//! The notification history endpoint, `GET /api/v1/notifications/logs`.
//!
//! Reads the repository when one is wired, and otherwise opens the HITL
//! database directly, applying its own migration list before querying.

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::Json;

use crate::api::routes_notifications::LogsQuery;
use crate::api::server::AppState;
use crate::coordinator::ExecutionBackend;

/// `GET /api/v1/notifications/logs?last=N`, notification history.
///
/// Reads from `notifications.db` via the repository.
/// Falls back to `hitl.db` if the repo is not available (backward compat).
#[utoipa::path(
    get,
    path = "/api/v1/notifications/logs",
    tag = "notifications",
    params(("last" = Option<usize>, Query, description = "Maximum number of entries (default 20, max 1000)")),
    responses(
        (status = 200, description = "Notification dispatch history"),
        (status = 500, description = "Repository error", body = crate::api::openapi::ApiErrorBody),
    )
)]
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
    let Some(db_path) = resolve_notif_db_path() else {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "error": "cannot resolve the home directory (USERPROFILE on Windows, HOME on Unix)"
            })),
        );
    };
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

/// Resolve the path of the notification log database: `hitl.db` under the
/// data directory.
///
/// `None` when the home directory cannot be resolved. That case used to fall
/// back to a database in the world-writable `/tmp`, outside the profile; user
/// state never belongs there, so the caller reports the error instead.
pub(super) fn resolve_notif_db_path() -> Option<std::path::PathBuf> {
    apollia_core::paths::data_dir().map(|d| apollia_core::paths::DataFile::Hitl.path(&d))
}

/// Current schema version of the HITL notification-log store (a single step).
///
/// The same table is written by the notification engine
/// (`apollia-notifications`); the two DDLs and this version number must stay
/// aligned, `hitl.db` carries one `user_version` for both.
pub(super) const HITL_SCHEMA_VERSION: u32 = 1;

/// The ordered migration list applied through
/// [`apollia_core::schema::open_versioned`].
const HITL_MIGRATIONS: [apollia_core::schema::Migration; HITL_SCHEMA_VERSION as usize] =
    [hitl_migrate_v1];

pub(super) fn hitl_migrate_v1(conn: &rusqlite::Connection) -> Result<(), rusqlite::Error> {
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
}

/// Open `db_path`, create `notification_logs` if needed, and return the last `N` entries.
pub(super) fn query_notification_logs(
    db_path: &std::path::Path,
    last: usize,
) -> Result<Vec<serde_json::Value>, String> {
    let conn = rusqlite::Connection::open(db_path)
        .map_err(|e| format!("cannot open the database: {e}"))?;

    apollia_core::schema::open_versioned(
        &conn,
        apollia_core::paths::DataFile::Hitl.file_name(),
        HITL_SCHEMA_VERSION,
        &HITL_MIGRATIONS,
    )
    .map_err(|e| format!("notification_logs migration failed: {e}"))?;

    let mut stmt = conn
        .prepare(
            "SELECT id, event_name, task_id, agent_id, sent_at, channels, error
               FROM notification_logs
              ORDER BY sent_at DESC
              LIMIT ?1",
        )
        .map_err(|e| format!("prepare failed: {e}"))?;

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
        .map_err(|e| format!("query failed: {e}"))?;

    let mut entries = Vec::new();
    for row_result in rows {
        let (id, event_name, task_id, agent_id, sent_at, channels_raw, error) =
            row_result.map_err(|e| format!("reading a row failed: {e}"))?;

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
