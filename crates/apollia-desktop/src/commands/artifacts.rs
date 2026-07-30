//! Tauri IPC commands for session artifacts.
//!
//! Artifacts are snapshots of agent tool outputs (file contents, code blocks,
//! specs, diagrams, bash logs) captured per chat session. They live in a
//! standalone SQLite DB at `~/.apollia/artifacts.db`, independent of the
//! chat manager actor; write/read paths are synchronous here and do not
//! interact with the runtime event bus.

use std::path::PathBuf;
use std::sync::Mutex;

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

/// Artifact persisted in `~/.apollia/artifacts.db`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Artifact {
    /// UUID v4 identifier.
    pub id: String,
    /// Owning chat session identifier.
    pub session_id: String,
    /// Source message identifier (nullable, manual creation).
    pub source_message_id: Option<String>,
    /// Kind bucket: `"code"`, `"file"`, `"spec"`, `"bash_output"`, `"other"`.
    pub kind: String,
    /// Detected or declared language (e.g. `"rust"`, `"markdown"`).
    pub language: Option<String>,
    /// Source tool name (e.g. `"file_write"`, `"bash_executor"`).
    pub source_tool: Option<String>,
    /// Human-readable title (path, heading, short description).
    pub title: String,
    /// Full textual content.
    pub content: String,
    /// ISO-8601 creation timestamp.
    pub created_at: String,
    /// ISO-8601 last-update timestamp.
    pub updated_at: String,
}

/// Input for [`save_artifact`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SaveArtifactRequest {
    /// Owning session.
    pub session_id: String,
    /// Source message identifier, set when detected from a tool call.
    pub source_message_id: Option<String>,
    /// Classification bucket.
    pub kind: String,
    /// Detected/declared language.
    pub language: Option<String>,
    /// Source tool (if emitted by a tool call).
    pub source_tool: Option<String>,
    /// Human-readable title.
    pub title: String,
    /// Full artifact content.
    pub content: String,
}

/// Process-wide connection guarded by a mutex (single-writer SQLite).
static ARTIFACT_DB: Mutex<Option<Connection>> = Mutex::new(None);

fn db_path() -> Result<PathBuf, String> {
    let home = apollia_core::paths::home_string_or_err()
        .map_err(|_| "cannot determine home directory: $HOME not set".to_string())?;
    Ok(PathBuf::from(home).join(".apollia").join("artifacts.db"))
}

fn with_conn<R>(f: impl FnOnce(&Connection) -> Result<R, String>) -> Result<R, String> {
    let mut guard = ARTIFACT_DB
        .lock()
        .map_err(|e| format!("artifacts DB mutex poisoned: {e}"))?;

    if guard.is_none() {
        let path = db_path()?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("failed to create artifacts dir: {e}"))?;
        }
        let conn =
            Connection::open(&path).map_err(|e| format!("failed to open artifacts.db: {e}"))?;
        conn.execute_batch("PRAGMA journal_mode = WAL;")
            .map_err(|e| format!("WAL pragma failed: {e}"))?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS chat_artifacts (
                id                 TEXT PRIMARY KEY,
                session_id         TEXT NOT NULL,
                source_message_id  TEXT,
                kind               TEXT NOT NULL,
                language           TEXT,
                source_tool        TEXT,
                title              TEXT NOT NULL,
                content            TEXT NOT NULL,
                created_at         TEXT NOT NULL,
                updated_at         TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_chat_artifacts_session
                ON chat_artifacts(session_id, created_at DESC);
            CREATE INDEX IF NOT EXISTS idx_chat_artifacts_source_message
                ON chat_artifacts(source_message_id);",
        )
        .map_err(|e| format!("artifacts schema failed: {e}"))?;
        *guard = Some(conn);
    }

    f(guard.as_ref().expect("just initialized"))
}

fn row_to_artifact(row: &rusqlite::Row<'_>) -> rusqlite::Result<Artifact> {
    Ok(Artifact {
        id: row.get(0)?,
        session_id: row.get(1)?,
        source_message_id: row.get(2)?,
        kind: row.get(3)?,
        language: row.get(4)?,
        source_tool: row.get(5)?,
        title: row.get(6)?,
        content: row.get(7)?,
        created_at: row.get(8)?,
        updated_at: row.get(9)?,
    })
}

const SELECT_COLS: &str = "id, session_id, source_message_id, kind, language, \
                           source_tool, title, content, created_at, updated_at";

/// Persist a new artifact. Returns the stored row.
#[tauri::command]
pub async fn save_artifact(request: SaveArtifactRequest) -> Result<Artifact, String> {
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();

    with_conn(|conn| {
        // Idempotency: when a source_message_id is provided, return the existing
        // row instead of inserting a duplicate. Auto-detection in the frontend
        // may fire twice for the same tool message (re-render, store replay).
        if let Some(msg_id) = request.source_message_id.as_deref() {
            let mut stmt = conn
                .prepare(&format!(
                    "SELECT {SELECT_COLS} FROM chat_artifacts \
                     WHERE session_id = ?1 AND source_message_id = ?2 LIMIT 1"
                ))
                .map_err(|e| format!("save_artifact prepare dedup: {e}"))?;
            let existing = stmt
                .query_row(params![&request.session_id, msg_id], row_to_artifact)
                .ok();
            if let Some(existing) = existing {
                return Ok(existing);
            }
        }

        conn.execute(
            "INSERT INTO chat_artifacts
                (id, session_id, source_message_id, kind, language, source_tool, title, content, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?9)",
            params![
                id,
                request.session_id,
                request.source_message_id,
                request.kind,
                request.language,
                request.source_tool,
                request.title,
                request.content,
                now,
            ],
        )
        .map_err(|e| format!("save_artifact insert: {e}"))?;

        Ok(Artifact {
            id: id.clone(),
            session_id: request.session_id.clone(),
            source_message_id: request.source_message_id.clone(),
            kind: request.kind.clone(),
            language: request.language.clone(),
            source_tool: request.source_tool.clone(),
            title: request.title.clone(),
            content: request.content.clone(),
            created_at: now.clone(),
            updated_at: now,
        })
    })
}

/// List artifacts for a session, most recent first.
#[tauri::command]
pub async fn list_artifacts(session_id: String) -> Result<Vec<Artifact>, String> {
    with_conn(|conn| {
        let mut stmt = conn
            .prepare(&format!(
                "SELECT {SELECT_COLS} FROM chat_artifacts \
                 WHERE session_id = ?1 ORDER BY created_at DESC"
            ))
            .map_err(|e| format!("list_artifacts prepare: {e}"))?;

        let rows = stmt
            .query_map(params![session_id], row_to_artifact)
            .map_err(|e| format!("list_artifacts query: {e}"))?;

        let mut out = Vec::new();
        for r in rows {
            out.push(r.map_err(|e| format!("list_artifacts row: {e}"))?);
        }
        Ok(out)
    })
}

/// Load a single artifact by id.
#[tauri::command]
pub async fn get_artifact(id: String) -> Result<Option<Artifact>, String> {
    with_conn(|conn| {
        let mut stmt = conn
            .prepare(&format!(
                "SELECT {SELECT_COLS} FROM chat_artifacts WHERE id = ?1"
            ))
            .map_err(|e| format!("get_artifact prepare: {e}"))?;

        match stmt.query_row(params![id], row_to_artifact) {
            Ok(a) => Ok(Some(a)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(format!("get_artifact query: {e}")),
        }
    })
}

/// Update the content and/or title of an artifact.
#[tauri::command]
pub async fn update_artifact(
    id: String,
    title: Option<String>,
    content: Option<String>,
) -> Result<Artifact, String> {
    let now = chrono::Utc::now().to_rfc3339();
    with_conn(|conn| {
        let mut parts: Vec<&str> = Vec::new();
        let mut values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
        if let Some(t) = title.as_deref() {
            parts.push("title = ?");
            values.push(Box::new(t.to_string()));
        }
        if let Some(c) = content.as_deref() {
            parts.push("content = ?");
            values.push(Box::new(c.to_string()));
        }
        parts.push("updated_at = ?");
        values.push(Box::new(now.clone()));

        let sql = format!(
            "UPDATE chat_artifacts SET {} WHERE id = ?",
            parts.join(", ")
        );
        values.push(Box::new(id.clone()));

        let sql_params: Vec<&dyn rusqlite::types::ToSql> =
            values.iter().map(|v| v.as_ref()).collect();
        let updated = conn
            .execute(&sql, sql_params.as_slice())
            .map_err(|e| format!("update_artifact: {e}"))?;
        if updated == 0 {
            return Err(format!("artifact not found: {id}"));
        }

        let mut stmt = conn
            .prepare(&format!(
                "SELECT {SELECT_COLS} FROM chat_artifacts WHERE id = ?1"
            ))
            .map_err(|e| format!("update_artifact reload prepare: {e}"))?;
        stmt.query_row(params![id], row_to_artifact)
            .map_err(|e| format!("update_artifact reload: {e}"))
    })
}

/// Delete an artifact by id. Idempotent: returns `Ok(())` even when absent.
#[tauri::command]
pub async fn delete_artifact(id: String) -> Result<(), String> {
    with_conn(|conn| {
        conn.execute("DELETE FROM chat_artifacts WHERE id = ?1", params![id])
            .map_err(|e| format!("delete_artifact: {e}"))?;
        Ok(())
    })
}
