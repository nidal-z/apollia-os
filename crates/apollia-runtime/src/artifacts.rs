//! The artifacts of a conversation, as every surface reads and writes them.
//!
//! Artifacts are snapshots of agent tool outputs (file contents, code blocks,
//! specs, diagrams, bash logs) captured per chat session. They live in
//! `artifacts.db` of the data directory. The desktop running the runtime in
//! its own process calls these functions directly; a client of the daemon
//! reaches the same functions through `/api/v1/artifacts`. Both therefore
//! answer the same rows and refuse with the same words.
//!
//! Each operation opens the file for itself, off the async threads: the store
//! holds a path, not a connection, so no lock is shared between callers and
//! SQLite in WAL mode orders the writers.

use std::path::{Path, PathBuf};
use std::time::Duration;

use apollia_core::paths::DataFile;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

/// How long a writer waits for another one before giving up.
const BUSY_TIMEOUT: Duration = Duration::from_secs(5);

/// Current schema version of `artifacts.db`.
const ARTIFACTS_SCHEMA_VERSION: u32 = 1;

/// Numbered migration steps of `artifacts.db`.
///
/// Step `k` migrates the file from version `k` to `k + 1`; the list length
/// always equals [`ARTIFACTS_SCHEMA_VERSION`].
const ARTIFACTS_MIGRATIONS: &[apollia_core::schema::Migration] = &[migrate_v1];

const SELECT_COLS: &str = "id, session_id, source_message_id, kind, language, \
                           source_tool, title, content, created_at, updated_at";

/// One artifact, as the chat draws it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct Artifact {
    /// UUID v4 identifier.
    pub id: String,
    /// Owning chat session identifier.
    pub session_id: String,
    /// Source message identifier (absent for a manual creation).
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
    /// ISO 8601 creation timestamp.
    pub created_at: String,
    /// ISO 8601 last-update timestamp.
    pub updated_at: String,
}

/// A new artifact, as the chat captures it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct SaveArtifactRequest {
    /// Owning session.
    pub session_id: String,
    /// Source message identifier, set when detected from a tool call.
    pub source_message_id: Option<String>,
    /// Classification bucket.
    pub kind: String,
    /// Detected or declared language.
    pub language: Option<String>,
    /// Source tool (if emitted by a tool call).
    pub source_tool: Option<String>,
    /// Human-readable title.
    pub title: String,
    /// Full artifact content.
    pub content: String,
}

/// The fields of an artifact a change may replace; an absent one is kept.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct UpdateArtifactRequest {
    /// New title.
    pub title: Option<String>,
    /// New content.
    pub content: Option<String>,
}

/// Why an artifact operation did not complete. The display text is what the
/// interface shows, the same whichever process ran the operation.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ArtifactError {
    /// No artifact has this identifier.
    #[error("artifact not found: {0}")]
    NotFound(String),
    /// The data directory could not be created.
    #[error("failed to create artifacts dir: {0}")]
    Directory(#[source] std::io::Error),
    /// `artifacts.db` could not be opened.
    #[error("failed to open artifacts.db: {0}")]
    Open(#[source] rusqlite::Error),
    /// The schema of `artifacts.db` could not be brought to this version.
    #[error("artifacts schema failed: {0}")]
    Schema(#[source] apollia_core::schema::SchemaError),
    /// A statement failed.
    #[error("{operation}: {source}")]
    Storage {
        /// The operation, as the message names it.
        operation: &'static str,
        /// The SQLite failure.
        #[source]
        source: rusqlite::Error,
    },
    /// The blocking task did not complete.
    #[error("spawn_blocking failed: {0}")]
    Join(#[source] tokio::task::JoinError),
}

fn storage(operation: &'static str) -> impl FnOnce(rusqlite::Error) -> ArtifactError {
    move |source| ArtifactError::Storage { operation, source }
}

/// Where the artifacts live: `artifacts.db` of one data directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactStore {
    path: PathBuf,
}

impl ArtifactStore {
    /// The store of the data directory `data_dir` (`~/.apollia`).
    pub fn under(data_dir: &Path) -> Self {
        Self {
            path: DataFile::Artifacts.path(data_dir),
        }
    }

    /// Runs `operation` on a connection to the file, off the async threads.
    async fn run<T, F>(&self, operation: F) -> Result<T, ArtifactError>
    where
        T: Send + 'static,
        F: FnOnce(&Connection) -> Result<T, ArtifactError> + Send + 'static,
    {
        let path = self.path.clone();
        tokio::task::spawn_blocking(move || operation(&open(&path)?))
            .await
            .map_err(ArtifactError::Join)?
    }
}

/// v1: the pre-versioning schema of the file, replayed idempotently.
fn migrate_v1(conn: &Connection) -> Result<(), rusqlite::Error> {
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
}

fn open(path: &Path) -> Result<Connection, ArtifactError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(ArtifactError::Directory)?;
    }
    let conn = Connection::open(path).map_err(ArtifactError::Open)?;
    conn.busy_timeout(BUSY_TIMEOUT)
        .map_err(storage("busy timeout"))?;
    conn.execute_batch("PRAGMA journal_mode = WAL;")
        .map_err(storage("WAL pragma failed"))?;
    apollia_core::schema::open_versioned(
        &conn,
        DataFile::Artifacts.file_name(),
        ARTIFACTS_SCHEMA_VERSION,
        ARTIFACTS_MIGRATIONS,
    )
    .map_err(ArtifactError::Schema)?;
    Ok(conn)
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

/// Stores a new artifact and returns it. When `source_message_id` is given
/// and that message already has an artifact in the session, the stored one
/// comes back instead: the chat may detect the same tool output twice.
///
/// # Errors
///
/// [`ArtifactError`] when the file cannot be opened or written.
pub async fn save(
    store: &ArtifactStore,
    request: SaveArtifactRequest,
) -> Result<Artifact, ArtifactError> {
    store.run(move |conn| save_on(conn, request)).await
}

fn save_on(conn: &Connection, request: SaveArtifactRequest) -> Result<Artifact, ArtifactError> {
    if let Some(msg_id) = request.source_message_id.as_deref() {
        let mut stmt = conn
            .prepare(&format!(
                "SELECT {SELECT_COLS} FROM chat_artifacts \
                 WHERE session_id = ?1 AND source_message_id = ?2 LIMIT 1"
            ))
            .map_err(storage("save_artifact prepare dedup"))?;
        if let Ok(existing) = stmt.query_row(params![&request.session_id, msg_id], row_to_artifact)
        {
            return Ok(existing);
        }
    }

    let now = chrono::Utc::now().to_rfc3339();
    let artifact = Artifact {
        id: uuid::Uuid::new_v4().to_string(),
        session_id: request.session_id,
        source_message_id: request.source_message_id,
        kind: request.kind,
        language: request.language,
        source_tool: request.source_tool,
        title: request.title,
        content: request.content,
        created_at: now.clone(),
        updated_at: now,
    };
    conn.execute(
        "INSERT INTO chat_artifacts
            (id, session_id, source_message_id, kind, language, source_tool, title, content, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?9)",
        params![
            artifact.id,
            artifact.session_id,
            artifact.source_message_id,
            artifact.kind,
            artifact.language,
            artifact.source_tool,
            artifact.title,
            artifact.content,
            artifact.created_at,
        ],
    )
    .map_err(storage("save_artifact insert"))?;
    Ok(artifact)
}

/// The artifacts of a session, most recent first.
///
/// # Errors
///
/// [`ArtifactError`] when the file cannot be opened or read.
pub async fn list(
    store: &ArtifactStore,
    session_id: String,
) -> Result<Vec<Artifact>, ArtifactError> {
    store
        .run(move |conn| {
            let mut stmt = conn
                .prepare(&format!(
                    "SELECT {SELECT_COLS} FROM chat_artifacts \
                     WHERE session_id = ?1 ORDER BY created_at DESC"
                ))
                .map_err(storage("list_artifacts prepare"))?;
            let rows = stmt
                .query_map(params![session_id], row_to_artifact)
                .map_err(storage("list_artifacts query"))?;
            rows.collect::<Result<Vec<_>, _>>()
                .map_err(storage("list_artifacts row"))
        })
        .await
}

/// Replaces the title and/or the content of an artifact and returns it.
///
/// # Errors
///
/// [`ArtifactError::NotFound`] when no artifact has this identifier.
pub async fn update(
    store: &ArtifactStore,
    id: String,
    change: UpdateArtifactRequest,
) -> Result<Artifact, ArtifactError> {
    store.run(move |conn| update_on(conn, id, change)).await
}

fn update_on(
    conn: &Connection,
    id: String,
    change: UpdateArtifactRequest,
) -> Result<Artifact, ArtifactError> {
    let updated = conn
        .execute(
            "UPDATE chat_artifacts
                SET title = COALESCE(?1, title),
                    content = COALESCE(?2, content),
                    updated_at = ?3
              WHERE id = ?4",
            params![
                change.title,
                change.content,
                chrono::Utc::now().to_rfc3339(),
                id
            ],
        )
        .map_err(storage("update_artifact"))?;
    if updated == 0 {
        return Err(ArtifactError::NotFound(id));
    }
    let mut stmt = conn
        .prepare(&format!(
            "SELECT {SELECT_COLS} FROM chat_artifacts WHERE id = ?1"
        ))
        .map_err(storage("update_artifact reload prepare"))?;
    stmt.query_row(params![id], row_to_artifact)
        .map_err(storage("update_artifact reload"))
}

/// Deletes an artifact. Deleting one already gone is not an error.
///
/// # Errors
///
/// [`ArtifactError`] when the file cannot be opened or written.
pub async fn delete(store: &ArtifactStore, id: String) -> Result<(), ArtifactError> {
    store
        .run(move |conn| {
            conn.execute("DELETE FROM chat_artifacts WHERE id = ?1", params![id])
                .map_err(storage("delete_artifact"))?;
            Ok(())
        })
        .await
}
