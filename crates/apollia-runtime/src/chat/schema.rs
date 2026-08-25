//! Versioned schema of the chat database.
//!
//! One database file backs three actors: the session repository
//! (`chat_sessions`, `chat_messages`, `chat_tool_authorizations`,
//! `chat_approval_log`, the FTS index), the todo actor (`session_todos`),
//! and the plan actor (`session_plans`, `session_plan_steps`,
//! `session_plan_mutations`). `PRAGMA user_version` is a property of the
//! file, not of a table family, so the whole file owns a single ordered
//! migration list, kept here and applied through
//! [`apollia_core::schema::open_versioned`] by every opener.
//!
//! Databases written before the versioned layer carry `user_version = 0`
//! whatever their real shape, so every step is idempotent: `IF NOT EXISTS`
//! DDL, [`add_column_if_missing`] for additive columns, and a witness-column
//! probe for the one destructive table recreate (v10).

use apollia_core::schema::{add_column_if_missing, open_versioned, Migration, SchemaError};
use rusqlite::Connection;

/// Current schema version of the chat database.
///
/// History: v1 base tables, v2 `llm_backend`, v3 `summary`, v4 FTS index,
/// v5 `title`, v6 forking, v7 project linkage, v8 message `metadata`,
/// v9 approval log, v10 `chat_sessions` recreate (mode CHECK gains
/// `companion`), v11 approval `reason`, v12 plan-mode columns,
/// v13 todo table, v14 plan tables.
pub(crate) const SCHEMA_VERSION: u32 = 14;

/// Base tables, applied on first open.
const BASE_SQL: &str = include_str!("../../migrations/001_chat_tables.sql");

/// SQL creating the todo table (v13); step 13 of the chat database.
const TODO_SQL: &str = "\
CREATE TABLE IF NOT EXISTS session_todos (
    id          TEXT NOT NULL,
    session_id  TEXT NOT NULL,
    content     TEXT NOT NULL,
    status      TEXT NOT NULL CHECK(status IN ('pending','in_progress','completed')),
    depends_on  TEXT NOT NULL DEFAULT '[]',
    created_at  TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at  TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (id, session_id)
);
CREATE INDEX IF NOT EXISTS idx_session_todos_session ON session_todos(session_id);";

/// SQL creating the three plan tables (v14); step 14 of the chat database.
const PLAN_SQL: &str = "\
CREATE TABLE IF NOT EXISTS session_plans (
    session_id  TEXT PRIMARY KEY,
    plan_id     TEXT NOT NULL,
    revision    INTEGER NOT NULL DEFAULT 0,
    status      TEXT NOT NULL,
    summary     TEXT,
    updated_at  TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE TABLE IF NOT EXISTS session_plan_steps (
    session_id  TEXT NOT NULL,
    step_id     TEXT NOT NULL,
    ordinal     INTEGER NOT NULL,
    payload     TEXT NOT NULL,
    PRIMARY KEY (session_id, step_id)
);
CREATE TABLE IF NOT EXISTS session_plan_mutations (
    session_id  TEXT NOT NULL,
    seq         INTEGER PRIMARY KEY AUTOINCREMENT,
    payload     TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_session_plan_mut ON session_plan_mutations(session_id);";

/// The ordered migration list; step `k` brings the file to version `k + 1`.
const MIGRATIONS: [Migration; SCHEMA_VERSION as usize] = [
    migrate_v1,
    migrate_v2,
    migrate_v3,
    migrate_v4,
    migrate_v5,
    migrate_v6,
    migrate_v7,
    migrate_v8,
    migrate_v9,
    migrate_v10,
    migrate_v11,
    migrate_v12,
    migrate_v13,
    migrate_v14,
];

/// Bring `conn` to the current chat schema version, or refuse it.
///
/// # Errors
///
/// Any [`SchemaError`], notably the refusal of a database written by a newer
/// binary.
pub(crate) fn migrate(conn: &Connection) -> Result<(), SchemaError> {
    open_versioned(
        conn,
        apollia_core::paths::DataFile::Chat.file_name(),
        SCHEMA_VERSION,
        &MIGRATIONS,
    )
}

fn migrate_v1(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch(BASE_SQL)
}

fn migrate_v2(conn: &Connection) -> Result<(), rusqlite::Error> {
    // No-op on a fresh database: the base SQL already carries the column.
    add_column_if_missing(
        conn,
        "ALTER TABLE chat_sessions ADD COLUMN llm_backend TEXT",
    )
}

fn migrate_v3(conn: &Connection) -> Result<(), rusqlite::Error> {
    add_column_if_missing(conn, "ALTER TABLE chat_sessions ADD COLUMN summary TEXT")
}

fn migrate_v4(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch(
        "CREATE VIRTUAL TABLE IF NOT EXISTS chat_sessions_fts USING fts5(
            session_id UNINDEXED,
            created_at UNINDEXED,
            summary
        );",
    )
}

fn migrate_v5(conn: &Connection) -> Result<(), rusqlite::Error> {
    add_column_if_missing(conn, "ALTER TABLE chat_sessions ADD COLUMN title TEXT")
}

fn migrate_v6(conn: &Connection) -> Result<(), rusqlite::Error> {
    add_column_if_missing(
        conn,
        "ALTER TABLE chat_sessions ADD COLUMN parent_session_id TEXT REFERENCES chat_sessions(id)",
    )?;
    add_column_if_missing(
        conn,
        "ALTER TABLE chat_sessions ADD COLUMN fork_depth INTEGER NOT NULL DEFAULT 0",
    )?;
    conn.execute_batch(
        "CREATE INDEX IF NOT EXISTS idx_sessions_parent ON chat_sessions(parent_session_id)",
    )
}

fn migrate_v7(conn: &Connection) -> Result<(), rusqlite::Error> {
    add_column_if_missing(conn, "ALTER TABLE chat_sessions ADD COLUMN project_id TEXT")?;
    conn.execute_batch(
        "CREATE INDEX IF NOT EXISTS idx_chat_sessions_project ON chat_sessions(project_id)",
    )
}

fn migrate_v8(conn: &Connection) -> Result<(), rusqlite::Error> {
    add_column_if_missing(conn, "ALTER TABLE chat_messages ADD COLUMN metadata TEXT")
}

fn migrate_v9(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS chat_approval_log (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            session_id  TEXT NOT NULL,
            message_id  TEXT NOT NULL,
            tool_name   TEXT NOT NULL,
            decision    TEXT NOT NULL CHECK (decision IN ('accept', 'refuse', 'always_accept')),
            resolved_at TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_chat_approval_log_resolved
            ON chat_approval_log(resolved_at DESC);",
    )
}

/// v10: add `companion` to the mode CHECK constraint.
///
/// SQLite cannot drop a CHECK constraint, so the table is recreated. The
/// recreate drops every column added after it, so it must run exactly once:
/// the `plan_mode` column (added by v12) is the witness that it already ran
/// on a pre-versioning database (`user_version = 0` whatever the shape).
/// Once the file is stamped past 10, the step never replays.
fn migrate_v10(conn: &Connection) -> Result<(), rusqlite::Error> {
    if column_exists(conn, "chat_sessions", "plan_mode")? {
        return Ok(());
    }
    conn.execute_batch(
        "PRAGMA foreign_keys = OFF;
        BEGIN;
        CREATE TABLE IF NOT EXISTS chat_sessions_new (
            id              TEXT PRIMARY KEY,
            mode            TEXT NOT NULL CHECK (mode IN ('libre', 'agent', 'companion')),
            agent_name      TEXT,
            system_prompt   TEXT NOT NULL DEFAULT '',
            status          TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'processing', 'closed')),
            available_tools TEXT NOT NULL DEFAULT '[]',
            created_at      TEXT NOT NULL,
            closed_at       TEXT,
            llm_backend     TEXT,
            summary         TEXT,
            title           TEXT,
            parent_session_id TEXT REFERENCES chat_sessions_new(id),
            fork_depth      INTEGER NOT NULL DEFAULT 0,
            project_id      TEXT
        );
        INSERT OR IGNORE INTO chat_sessions_new
            SELECT id, mode, agent_name, system_prompt, status, available_tools,
                   created_at, closed_at, llm_backend, summary, title,
                   parent_session_id, fork_depth, project_id
            FROM chat_sessions;
        DROP TABLE chat_sessions;
        ALTER TABLE chat_sessions_new RENAME TO chat_sessions;
        CREATE INDEX IF NOT EXISTS idx_chat_sessions_status ON chat_sessions(status);
        CREATE INDEX IF NOT EXISTS idx_sessions_parent ON chat_sessions(parent_session_id);
        CREATE INDEX IF NOT EXISTS idx_chat_sessions_project ON chat_sessions(project_id);
        COMMIT;
        PRAGMA foreign_keys = ON;",
    )
}

fn migrate_v11(conn: &Connection) -> Result<(), rusqlite::Error> {
    add_column_if_missing(conn, "ALTER TABLE chat_approval_log ADD COLUMN reason TEXT")
}

fn migrate_v12(conn: &Connection) -> Result<(), rusqlite::Error> {
    add_column_if_missing(
        conn,
        "ALTER TABLE chat_sessions ADD COLUMN plan_mode INTEGER NOT NULL DEFAULT 0",
    )?;
    add_column_if_missing(
        conn,
        "ALTER TABLE chat_sessions ADD COLUMN plan_phase TEXT NOT NULL DEFAULT 'done'",
    )
}

fn migrate_v13(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch(TODO_SQL)
}

fn migrate_v14(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch(PLAN_SQL)
}

/// Returns `true` when `table` already declares a column named `column`.
fn column_exists(conn: &Connection, table: &str, column: &str) -> Result<bool, rusqlite::Error> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})"))?;
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        let name: String = row.get(1)?;
        if name == column {
            return Ok(true);
        }
    }
    Ok(false)
}
