//! Audit trail: SQLite-persisted log of tool invocations.
//!
//! Architecture: a `tokio::task::spawn_blocking` actor with a bounded
//! `tokio::sync::mpsc` channel. The actor exclusively owns the (non-`Sync`)
//! `rusqlite::Connection`. The handle is clonable and exposes an async API via
//! `oneshot` for operations that need a reply.

use std::path::Path;

use sha2::{Digest, Sha256};
use thiserror::Error;

/// Record of a tool invocation for the audit trail.
#[derive(Debug, Clone)]
pub struct ToolInvocationRecord {
    /// Unique invocation identifier (UUID v4).
    pub id: String,
    /// Identifier of the agent that invoked the tool.
    pub agent_id: String,
    /// Identifier of the task the invocation belongs to.
    pub task_id: String,
    /// Stable run identifier the invocation belongs to (the key the audit
    /// journal is indexed by). `None` for invocations outside a run context.
    pub run_id: Option<String>,
    /// Name of the invoked tool (e.g. `bash_executor`, `file_io`).
    pub tool_name: String,
    /// SHA256 hex of the JSON-serialized parameters.
    pub input_hash: String,
    /// Sandbox profile used, serialized to a string.
    pub sandbox_profile: String,
    /// Invocation start timestamp in RFC3339 UTC format.
    pub started_at: String,
    /// Invocation duration in milliseconds.
    pub duration_ms: Option<u64>,
    /// Child process exit code.
    pub exit_code: Option<i32>,
    /// `true` if the invocation finished without error.
    pub success: bool,
    /// Human-readable error code when `success` is `false`.
    pub error_code: Option<String>,
    /// Resources consumed (raw JSON). `null` for now.
    pub resources_used: Option<serde_json::Value>,
    /// Full JSON arguments of the invocation.
    pub args_json: Option<String>,
    /// Standard output of the tool (possibly truncated).
    pub stdout: Option<String>,
    /// Error output of the tool (possibly truncated).
    pub stderr: Option<String>,
}

/// Aggregated statistics of the audit trail.
#[derive(Debug, Clone)]
pub struct AuditStats {
    /// Total number of recorded invocations.
    pub total_events: u64,
    /// Number of distinct tools invoked.
    pub unique_tools: u64,
    /// Number of distinct agents that invoked tools.
    pub unique_agents: u64,
}

/// Errors when opening or initializing the audit trail.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum AuditTrailError {
    /// Failed to open the SQLite file.
    #[error("failed to open SQLite database: {0}")]
    OpenFailed(String),
    /// Failed to create the schema (table or index).
    #[error("failed to initialize audit schema: {0}")]
    SchemaInitFailed(String),
    /// The database schema could not be brought to the supported version.
    #[error(transparent)]
    Schema(#[from] apollia_core::schema::SchemaError),
}

/// Computes the SHA256 of a serialized JSON object.
///
/// Returns the lowercase hexadecimal representation of the hash. Two calls with
/// the same JSON value always produce the same result.
pub fn compute_input_hash(params: &serde_json::Value) -> String {
    let serialized = serde_json::to_string(params).unwrap_or_default();
    let hash = Sha256::digest(serialized.as_bytes());
    hash.iter().map(|b| format!("{b:02x}")).collect()
}

// ---------------------------------------------------------------------------
// SQL schema
// ---------------------------------------------------------------------------

/// Current schema version of `audit.db`.
const AUDIT_SCHEMA_VERSION: u32 = 1;

/// Numbered migration steps of `audit.db`.
///
/// Step `k` migrates the file from version `k` to `k + 1`; the list length
/// always equals [`AUDIT_SCHEMA_VERSION`].
const AUDIT_MIGRATIONS: &[apollia_core::schema::Migration] = &[migrate_v1];

/// v1: the pre-versioning lineage of the file, replayed idempotently.
///
/// Every `audit.db` written before the versioned layer is at
/// `user_version = 0` whatever columns it carries, so this step must accept
/// a fresh file, the initial schema, and the shape with the observability
/// columns, and bring each of them to the same state.
fn migrate_v1(conn: &rusqlite::Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch(SCHEMA)?;
    for ddl in MIGRATION_OBSERVABILITY_COLUMNS {
        apollia_core::schema::add_column_if_missing(conn, ddl)?;
    }
    Ok(())
}

/// SQL schema of the `tool_invocations` table and its indexes.
const SCHEMA: &str = "
    CREATE TABLE IF NOT EXISTS tool_invocations (
        id              TEXT PRIMARY KEY,
        agent_id        TEXT NOT NULL,
        task_id         TEXT NOT NULL,
        tool_name       TEXT NOT NULL,
        input_hash      TEXT NOT NULL,
        sandbox_profile TEXT NOT NULL,
        started_at      TEXT NOT NULL,
        duration_ms     INTEGER,
        exit_code       INTEGER,
        success         INTEGER NOT NULL,
        error_code      TEXT,
        resources_used  TEXT
    );
    CREATE INDEX IF NOT EXISTS idx_tool_invocations_agent_id
        ON tool_invocations(agent_id);
    CREATE INDEX IF NOT EXISTS idx_tool_invocations_started_at
        ON tool_invocations(started_at);
    CREATE TRIGGER IF NOT EXISTS audit_no_update
        BEFORE UPDATE ON tool_invocations
        BEGIN SELECT RAISE(ABORT, 'audit trail is append-only'); END;
    CREATE TRIGGER IF NOT EXISTS audit_no_delete
        BEFORE DELETE ON tool_invocations
        BEGIN SELECT RAISE(ABORT, 'audit trail is append-only'); END;
";

/// Observability columns added by a later migration.
///
/// Each ALTER TABLE is run individually; the "duplicate column" error is
/// ignored to keep the migration idempotent on existing stores.
const MIGRATION_OBSERVABILITY_COLUMNS: &[&str] = &[
    "ALTER TABLE tool_invocations ADD COLUMN args_json TEXT",
    "ALTER TABLE tool_invocations ADD COLUMN stdout    TEXT",
    "ALTER TABLE tool_invocations ADD COLUMN stderr    TEXT",
    "ALTER TABLE tool_invocations ADD COLUMN run_id    TEXT",
];

// ---------------------------------------------------------------------------
// Internal messages
// ---------------------------------------------------------------------------

/// Messages sent to the AuditTrail actor.
enum AuditMessage {
    /// Inserts a record (fire-and-forget).
    Record(Box<ToolInvocationRecord>),
    /// Returns N invocations sorted by descending date, skipping `offset` of
    /// them. `offset` is what makes the trail exportable past one page.
    QueryLast {
        n: usize,
        offset: usize,
        reply: tokio::sync::oneshot::Sender<Vec<ToolInvocationRecord>>,
    },
    /// Returns the aggregated audit trail statistics.
    QueryStats {
        reply: tokio::sync::oneshot::Sender<AuditStats>,
    },
    /// Stops the actor cleanly after draining the queue.
    Shutdown,
}

// ---------------------------------------------------------------------------
// Internal actor
// ---------------------------------------------------------------------------

/// Internal actor, never exposed directly.
///
/// Owns the `rusqlite::Connection` and processes messages sequentially,
/// guaranteeing no contention on the database.
struct AuditTrail {
    conn: rusqlite::Connection,
    receiver: tokio::sync::mpsc::Receiver<AuditMessage>,
}

impl AuditTrail {
    /// Main actor loop.
    fn run(mut self) {
        while let Some(msg) = self.receiver.blocking_recv() {
            match msg {
                AuditMessage::Record(record) => {
                    if let Err(e) = Self::insert(&self.conn, &record) {
                        tracing::error!(
                            error = %e,
                            tool = %record.tool_name,
                            id   = %record.id,
                            "tool.audit.insert.failed"
                        );
                    }
                }
                AuditMessage::QueryLast { n, offset, reply } => {
                    let results = Self::query_last_n(&self.conn, n, offset).unwrap_or_default();
                    let _ = reply.send(results);
                }
                AuditMessage::QueryStats { reply } => {
                    let stats = Self::query_stats(&self.conn).unwrap_or(AuditStats {
                        total_events: 0,
                        unique_tools: 0,
                        unique_agents: 0,
                    });
                    let _ = reply.send(stats);
                }
                AuditMessage::Shutdown => break,
            }
        }
    }

    /// Inserts a record into `tool_invocations`.
    fn insert(conn: &rusqlite::Connection, r: &ToolInvocationRecord) -> rusqlite::Result<()> {
        let resources_json = r.resources_used.as_ref().map(|v| v.to_string());
        conn.execute(
            "INSERT INTO tool_invocations \
             (id, agent_id, task_id, tool_name, input_hash, sandbox_profile, \
              started_at, duration_ms, exit_code, success, error_code, resources_used, \
              args_json, stdout, stderr, run_id) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
            rusqlite::params![
                r.id,
                r.agent_id,
                r.task_id,
                r.tool_name,
                r.input_hash,
                r.sandbox_profile,
                r.started_at,
                r.duration_ms.map(|v| v as i64),
                r.exit_code,
                r.success as i32,
                r.error_code,
                resources_json,
                r.args_json,
                r.stdout,
                r.stderr,
                r.run_id,
            ],
        )?;
        Ok(())
    }

    /// Returns the aggregated audit trail statistics via an aggregate SQL query.
    fn query_stats(conn: &rusqlite::Connection) -> rusqlite::Result<AuditStats> {
        conn.query_row(
            "SELECT COUNT(*), COUNT(DISTINCT tool_name), COUNT(DISTINCT agent_id) \
             FROM tool_invocations",
            [],
            |row| {
                Ok(AuditStats {
                    total_events: row.get::<_, i64>(0)? as u64,
                    unique_tools: row.get::<_, i64>(1)? as u64,
                    unique_agents: row.get::<_, i64>(2)? as u64,
                })
            },
        )
    }

    /// Returns the last N invocations, ordered by descending `started_at`.
    fn query_last_n(
        conn: &rusqlite::Connection,
        n: usize,
        offset: usize,
    ) -> rusqlite::Result<Vec<ToolInvocationRecord>> {
        // `id DESC` breaks ties on `started_at`. Without it the order between two
        // invocations recorded in the same millisecond is whatever SQLite
        // happens to produce, which is stable within one query and not across
        // two. Paging over an unstable order silently repeats some rows and
        // drops others, and an audit export that quietly loses a row is worse
        // than one that refuses to run.
        let mut stmt = conn.prepare(
            "SELECT id, agent_id, task_id, tool_name, input_hash, sandbox_profile, \
             started_at, duration_ms, exit_code, success, error_code, resources_used, \
             args_json, stdout, stderr, run_id \
             FROM tool_invocations \
             ORDER BY started_at DESC, id DESC \
             LIMIT ?1 OFFSET ?2",
        )?;
        let rows = stmt.query_map(rusqlite::params![n as i64, offset as i64], |row| {
            let resources_str: Option<String> = row.get(11)?;
            let resources_used = resources_str.and_then(|s| serde_json::from_str(&s).ok());
            Ok(ToolInvocationRecord {
                id: row.get(0)?,
                agent_id: row.get(1)?,
                task_id: row.get(2)?,
                run_id: row.get(15)?,
                tool_name: row.get(3)?,
                input_hash: row.get(4)?,
                sandbox_profile: row.get(5)?,
                started_at: row.get(6)?,
                duration_ms: row.get::<_, Option<i64>>(7)?.map(|v| v as u64),
                exit_code: row.get(8)?,
                success: row.get::<_, i32>(9)? != 0,
                error_code: row.get(10)?,
                resources_used,
                args_json: row.get(12)?,
                stdout: row.get(13)?,
                stderr: row.get(14)?,
            })
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
    }
}

// ---------------------------------------------------------------------------
// Public handle
// ---------------------------------------------------------------------------

/// Capacity of the internal channel between the handle and the actor.
const CHANNEL_CAPACITY: usize = 1024;

/// Clonable handle to the [`AuditTrail`] actor.
///
/// Created via [`AuditTrailHandle::open`]. All methods are thread-safe; several
/// handles can coexist and emit messages to the same actor.
#[derive(Clone)]
pub struct AuditTrailHandle {
    sender: tokio::sync::mpsc::Sender<AuditMessage>,
}

impl AuditTrailHandle {
    /// Opens the SQLite store and starts the actor in the background.
    ///
    /// Creates the file if absent. Creates the `tool_invocations` table and its
    /// indexes if they do not exist (`CREATE … IF NOT EXISTS`). Enables WAL mode.
    pub async fn open(db_path: &Path) -> Result<Self, AuditTrailError> {
        let db_path = db_path.to_path_buf();
        let (sender, receiver) = tokio::sync::mpsc::channel::<AuditMessage>(CHANNEL_CAPACITY);

        // Signalling channel for initialization: the thread notifies the caller
        // as soon as the connection and schema are ready (or on error).
        let (init_tx, init_rx) = tokio::sync::oneshot::channel::<Result<(), AuditTrailError>>();

        tokio::task::spawn_blocking(move || {
            // Open the SQLite connection.
            let conn = match rusqlite::Connection::open(&db_path) {
                Ok(c) => c,
                Err(e) => {
                    let _ = init_tx.send(Err(AuditTrailError::OpenFailed(e.to_string())));
                    return;
                }
            };

            // WAL mode for read/write concurrency.
            if let Err(e) = conn.execute_batch("PRAGMA journal_mode=WAL;") {
                let _ = init_tx.send(Err(AuditTrailError::SchemaInitFailed(e.to_string())));
                return;
            }

            // Versioned migration: stamps `PRAGMA user_version` and refuses
            // a database written by a newer binary.
            if let Err(e) = apollia_core::schema::open_versioned(
                &conn,
                apollia_core::paths::DataFile::Audit.file_name(),
                AUDIT_SCHEMA_VERSION,
                AUDIT_MIGRATIONS,
            ) {
                let _ = init_tx.send(Err(AuditTrailError::Schema(e)));
                return;
            }

            // Signal successful initialization before entering the loop.
            let _ = init_tx.send(Ok(()));

            // Actor loop, runs until Shutdown is received.
            AuditTrail { conn, receiver }.run();
        });

        // Wait for the initialization result.
        init_rx
            .await
            .map_err(|_| AuditTrailError::OpenFailed("init channel disconnected".to_string()))??;

        Ok(Self { sender })
    }

    /// Records an invocation (fire-and-forget).
    ///
    /// Returns immediately without waiting for SQLite confirmation. If the
    /// channel is saturated, the record is dropped with a `warn!`.
    pub fn record(&self, record: ToolInvocationRecord) {
        match self.sender.try_send(AuditMessage::Record(Box::new(record))) {
            Ok(()) => {}
            Err(tokio::sync::mpsc::error::TrySendError::Full(_)) => {
                tracing::warn!(
                    reason = "the audit channel is full",
                    "tool.audit.record.dropped"
                );
            }
            Err(tokio::sync::mpsc::error::TrySendError::Closed(_)) => {
                tracing::warn!(
                    reason = "the audit actor is disconnected",
                    "tool.audit.record.dropped"
                );
            }
        }
    }

    /// Returns the last N invocations, sorted by descending date.
    pub async fn query_last(&self, n: usize) -> Vec<ToolInvocationRecord> {
        self.query_page(n, 0).await
    }

    /// Returns N invocations sorted by descending date, skipping `offset`.
    ///
    /// This is what an export needs. Without an offset the trail could only ever
    /// be read from its head, so anything older than one page was unreachable
    /// through the API at all, whatever the caller did.
    pub async fn query_page(&self, n: usize, offset: usize) -> Vec<ToolInvocationRecord> {
        let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
        if self
            .sender
            .send(AuditMessage::QueryLast {
                n,
                offset,
                reply: reply_tx,
            })
            .await
            .is_err()
        {
            return Vec::new();
        }
        reply_rx.await.unwrap_or_default()
    }

    /// Returns the aggregated statistics (total, distinct tools, distinct agents).
    pub async fn stats(&self) -> AuditStats {
        let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
        if self
            .sender
            .send(AuditMessage::QueryStats { reply: reply_tx })
            .await
            .is_err()
        {
            return AuditStats {
                total_events: 0,
                unique_tools: 0,
                unique_agents: 0,
            };
        }
        reply_rx.await.unwrap_or(AuditStats {
            total_events: 0,
            unique_tools: 0,
            unique_agents: 0,
        })
    }

    /// Sends the shutdown signal to the actor and waits for it to process all
    /// pending messages.
    ///
    /// After this call the handle is consumed. Any remaining cloned handles will
    /// try to send on a channel whose receiver is closed.
    pub async fn shutdown(self) {
        let _ = self.sender.send(AuditMessage::Shutdown).await;
        // Give the actor time to process queued messages and close the connection.
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    async fn open_test_audit() -> AuditTrailHandle {
        let db_path =
            std::env::temp_dir().join(format!("apollia_audit_test_{}.db", uuid::Uuid::new_v4()));
        AuditTrailHandle::open(&db_path)
            .await
            .expect("failed to open audit trail")
    }

    /// Pre-versioning `audit.db` schema, frozen as the oldest shape a
    /// published binary wrote (`user_version = 0`, no observability columns).
    const AUDIT_V0_SQL: &str = include_str!("../tests/fixtures/schemas/audit_v0.sql");

    // GIVEN a database written by a pre-versioning binary, with a row
    // WHEN opening it through the versioned layer
    // THEN the row survives, the missing columns appear and the version is stamped
    #[tokio::test]
    async fn test_audit_db_old_format_migrates_and_keeps_rows() {
        let db_path =
            std::env::temp_dir().join(format!("apollia_audit_test_{}.db", uuid::Uuid::new_v4()));
        {
            let conn = rusqlite::Connection::open(&db_path).unwrap();
            conn.execute_batch(AUDIT_V0_SQL).unwrap();
            conn.execute(
                "INSERT INTO tool_invocations
                     (id, agent_id, task_id, tool_name, input_hash, sandbox_profile,
                      started_at, success)
                 VALUES ('i-1', 'a', 't', 'bash_executor', 'h', 'fs',
                         '2026-01-01T00:00:00Z', 1)",
                [],
            )
            .unwrap();
        }

        let handle = AuditTrailHandle::open(&db_path).await.unwrap();

        let rows = handle.query_last(1).await;
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].tool_name, "bash_executor");
        assert!(rows[0].run_id.is_none());
        handle.shutdown().await;
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        let version: i64 = conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(version, i64::from(AUDIT_SCHEMA_VERSION));
    }

    // GIVEN a database stamped by a newer binary
    // WHEN opening it
    // THEN the open is refused
    #[tokio::test]
    async fn test_audit_db_newer_version_is_refused() {
        let db_path =
            std::env::temp_dir().join(format!("apollia_audit_test_{}.db", uuid::Uuid::new_v4()));
        {
            let conn = rusqlite::Connection::open(&db_path).unwrap();
            conn.pragma_update(None, "user_version", AUDIT_SCHEMA_VERSION + 1)
                .unwrap();
        }

        let err = AuditTrailHandle::open(&db_path)
            .await
            .map(|_| ())
            .unwrap_err();

        assert!(matches!(
            err,
            AuditTrailError::Schema(apollia_core::schema::SchemaError::NewerThanBinary { .. })
        ));
    }

    fn make_record(success: bool, error_code: Option<&str>) -> ToolInvocationRecord {
        ToolInvocationRecord {
            id: uuid::Uuid::new_v4().to_string(),
            agent_id: "test-agent".to_string(),
            task_id: "task-001".to_string(),
            run_id: None,
            tool_name: "bash_executor".to_string(),
            input_hash: "abc123".to_string(),
            sandbox_profile: "file_system".to_string(),
            started_at: "2026-01-01T00:00:00Z".to_string(),
            duration_ms: Some(42),
            exit_code: if success { Some(0) } else { Some(1) },
            success,
            error_code: error_code.map(|s| s.to_string()),
            resources_used: None,
            args_json: None,
            stdout: None,
            stderr: None,
        }
    }

    // Recording a successful invocation
    #[tokio::test]
    async fn test_record_successful_invocation() {
        // GIVEN
        let handle = open_test_audit().await;
        let record = make_record(true, None);
        let tool_name = record.tool_name.clone();
        // WHEN
        handle.record(record);
        // THEN the query, sent on the same channel, is served after the record
        let results = handle.query_last(1).await;
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].tool_name, tool_name);
        assert!(results[0].success);
        handle.shutdown().await;
    }

    // Recording a failed invocation
    #[tokio::test]
    async fn test_record_failed_invocation() {
        // GIVEN
        let handle = open_test_audit().await;
        let record = make_record(false, Some("Timeout"));
        // WHEN
        handle.record(record);
        // THEN the query, sent on the same channel, is served after the record
        let results = handle.query_last(1).await;
        assert_eq!(results.len(), 1);
        assert!(!results[0].success);
        assert_eq!(results[0].error_code.as_deref(), Some("Timeout"));
        handle.shutdown().await;
    }

    // Schema created automatically on a fresh store
    #[tokio::test]
    async fn test_schema_created_on_fresh_db() {
        // GIVEN: a non-existent store
        let db_path =
            std::env::temp_dir().join(format!("apollia_fresh_{}.db", uuid::Uuid::new_v4()));
        // WHEN
        let result = AuditTrailHandle::open(&db_path).await;
        // THEN
        assert!(result.is_ok());
        let handle = result.unwrap();
        handle.shutdown().await;
        tokio::fs::remove_file(&db_path).await.ok();
    }

    // record() does not block (fire-and-forget)
    #[tokio::test]
    async fn test_record_is_fire_and_forget() {
        // GIVEN
        let handle = open_test_audit().await;
        // WHEN: 10 invocations are recorded without waiting
        for i in 0..10 {
            let mut r = make_record(true, None);
            r.id = format!("id-{i}");
            r.started_at = format!("2026-01-01T00:00:{i:02}Z");
            handle.record(r);
        }
        // THEN: the method returned immediately, and the ten inserts are all
        // ahead of the query in the same channel
        let results = handle.query_last(10).await;
        assert_eq!(results.len(), 10);
        handle.shutdown().await;
    }

    // Same parameters produce the same input_hash
    #[test]
    fn test_same_params_same_input_hash() {
        // GIVEN
        let params = serde_json::json!({ "command": "echo hello", "timeout_secs": 30 });
        // WHEN
        let hash1 = compute_input_hash(&params);
        let hash2 = compute_input_hash(&params);
        // THEN
        assert_eq!(hash1, hash2);
    }

    // Different hashes for different parameters
    #[test]
    fn test_different_params_different_hash() {
        // GIVEN
        let params_a = serde_json::json!({ "command": "echo hello" });
        let params_b = serde_json::json!({ "command": "echo world" });
        // WHEN / THEN
        assert_ne!(compute_input_hash(&params_a), compute_input_hash(&params_b));
    }

    // Hash is a valid hexadecimal string (64 chars = SHA256)
    #[test]
    fn test_input_hash_is_valid_hex() {
        // GIVEN
        let params = serde_json::json!({ "x": 1 });
        // WHEN
        let hash = compute_input_hash(&params);
        // THEN
        assert_eq!(hash.len(), 64);
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
    }

    // JSON arguments are persisted
    #[tokio::test]
    async fn test_tool_invocation_args_persisted() {
        // GIVEN an AuditTrail with an in-memory DB
        let handle = open_test_audit().await;
        let mut record = make_record(true, None);
        record.args_json = Some(r#"{"path":"/tmp/test"}"#.to_string());
        // WHEN recording with args_json
        handle.record(record);
        // THEN SELECT args_json returns the value
        let results = handle.query_last(1).await;
        assert_eq!(results.len(), 1);
        assert_eq!(
            results[0].args_json.as_deref(),
            Some(r#"{"path":"/tmp/test"}"#)
        );
        handle.shutdown().await;
    }

    // Stdout truncated via truncate_with_marker
    #[tokio::test]
    async fn test_tool_invocation_stdout_truncated() {
        // GIVEN max_tool_output_bytes = 100
        use apollia_core::truncate_with_marker;
        let handle = open_test_audit().await;
        let big_stdout = "x".repeat(500);
        let (truncated_stdout, _) = truncate_with_marker(&big_stdout, 100);
        let mut record = make_record(true, None);
        record.stdout = Some(truncated_stdout);
        // WHEN recording with truncated stdout
        handle.record(record);
        // THEN the persisted value is truncated with a marker
        let results = handle.query_last(1).await;
        assert_eq!(results.len(), 1);
        let stored = results[0].stdout.as_ref().expect("stdout should be set");
        assert!(stored.contains("500 bytes total"));
        assert!(stored.len() < 500);
        handle.shutdown().await;
    }

    // Stderr is persisted
    #[tokio::test]
    async fn test_tool_invocation_stderr_persisted() {
        // GIVEN an AuditTrail
        let handle = open_test_audit().await;
        let mut record = make_record(false, Some("NotFound"));
        record.stderr = Some("command not found".to_string());
        // WHEN recording with stderr
        handle.record(record);
        // THEN SELECT stderr returns the value
        let results = handle.query_last(1).await;
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].stderr.as_deref(), Some("command not found"));
        handle.shutdown().await;
    }

    // Existing duration and exit_code preserved after migration
    #[tokio::test]
    async fn test_tool_invocation_duration_preserved() {
        // GIVEN an AuditTrail
        let handle = open_test_audit().await;
        let record = make_record(true, None);
        // WHEN recording with duration_ms = 42
        handle.record(record);
        // THEN duration_ms and exit_code are read back correctly
        let results = handle.query_last(1).await;
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].duration_ms, Some(42));
        assert_eq!(results[0].exit_code, Some(0));
        handle.shutdown().await;
    }

    // Audit trail append-only: UPDATE/DELETE refused by SQLite triggers
    #[tokio::test]
    async fn test_audit_trail_is_append_only() {
        // GIVEN a store with one recorded invocation
        let db_path =
            std::env::temp_dir().join(format!("apollia_append_only_{}.db", uuid::Uuid::new_v4()));
        let handle = AuditTrailHandle::open(&db_path).await.unwrap();
        handle.record(make_record(true, None));
        // A query on the same channel is the barrier: it is served after the
        // record, so the row is on disk before the store is closed
        assert_eq!(handle.query_last(1).await.len(), 1);
        handle.shutdown().await;

        // WHEN attempting a direct UPDATE then DELETE on the table
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        let update_err = conn
            .execute("UPDATE tool_invocations SET success = 0", [])
            .unwrap_err()
            .to_string();
        let delete_err = conn
            .execute("DELETE FROM tool_invocations", [])
            .unwrap_err()
            .to_string();

        // THEN both operations are refused with the expected message
        assert!(
            update_err.contains("audit trail is append-only"),
            "expected append-only abort on UPDATE, got: {update_err}"
        );
        assert!(
            delete_err.contains("audit trail is append-only"),
            "expected append-only abort on DELETE, got: {delete_err}"
        );

        drop(conn);
        tokio::fs::remove_file(&db_path).await.ok();
    }
}
