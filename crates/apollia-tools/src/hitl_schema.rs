//! Versioned schema of `hitl.db`, shared by every store in the file.
//!
//! `hitl.db` holds the HITL task tables (`tasks`, `task_approvals`, owned by
//! [`crate::task_repository`]) and the notification delivery log
//! (`notification_logs`, written by the `apollia-notifications` engine and
//! read by the runtime's notification routes). `PRAGMA user_version` belongs
//! to the database file, not to a table, so every opener goes through this
//! one migration list: whichever opens first brings the whole file to the
//! current version, and each refuses a file written by a newer binary.

use rusqlite::Connection;

use apollia_core::schema::{add_column_if_missing, Migration};

/// Current schema version of `hitl.db`.
pub const HITL_SCHEMA_VERSION: u32 = 6;

/// Numbered migration steps of `hitl.db`.
///
/// Step `k` migrates the file from version `k` to `k + 1`; the list length
/// always equals [`HITL_SCHEMA_VERSION`].
pub const HITL_MIGRATIONS: &[Migration] = &[
    migrate_v1, migrate_v2, migrate_v3, migrate_v4, migrate_v5, migrate_v6,
];

/// v1: the pre-versioning lineage of the file, replayed idempotently.
///
/// Every `hitl.db` written before the versioned layer is at
/// `user_version = 0` whatever columns it carries, so this step must accept
/// a fresh file, the initial `005_hitl_tables` shape, and every intermediate
/// one (observability columns, HITL timing columns, notification log), and
/// bring each of them to the same state.
fn migrate_v1(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch(include_str!("../migrations/005_hitl_tables.sql"))?;

    // Observability columns added to `tasks` after 005 shipped.
    add_column_if_missing(conn, "ALTER TABLE tasks ADD COLUMN input_text TEXT")?;
    add_column_if_missing(
        conn,
        "ALTER TABLE tasks ADD COLUMN input_truncated INTEGER NOT NULL DEFAULT 0",
    )?;
    add_column_if_missing(conn, "ALTER TABLE tasks ADD COLUMN output_text TEXT")?;
    add_column_if_missing(
        conn,
        "ALTER TABLE tasks ADD COLUMN output_truncated INTEGER NOT NULL DEFAULT 0",
    )?;
    add_column_if_missing(conn, "ALTER TABLE tasks ADD COLUMN duration_ms INTEGER")?;
    add_column_if_missing(conn, "ALTER TABLE tasks ADD COLUMN transitions_json TEXT")?;
    add_column_if_missing(conn, "ALTER TABLE tasks ADD COLUMN run_id TEXT")?;

    // HITL timing columns added to `task_approvals` after 005 shipped.
    add_column_if_missing(
        conn,
        "ALTER TABLE task_approvals ADD COLUMN suspended_at TEXT",
    )?;
    add_column_if_missing(
        conn,
        "ALTER TABLE task_approvals ADD COLUMN wait_duration_ms INTEGER",
    )?;

    conn.execute_batch(
        "CREATE INDEX IF NOT EXISTS idx_task_approvals_pending
             ON task_approvals(task_id) WHERE approved IS NULL;

         CREATE TABLE IF NOT EXISTS notification_logs (
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

/// v2: typed pauses on the task path.
///
/// A pause used to persist a prompt and a context; a typed pause also carries
/// the payload it asks for, the answer it received, and the skill and agent
/// that paused, which is what `GET /api/v1/tasks?status=input_required` lists.
/// Every column is nullable, so a task paused before this version keeps
/// reading as a prompt-only pause.
fn migrate_v2(conn: &Connection) -> Result<(), rusqlite::Error> {
    add_column_if_missing(
        conn,
        "ALTER TABLE tasks ADD COLUMN input_required_payload TEXT",
    )?;
    add_column_if_missing(
        conn,
        "ALTER TABLE tasks ADD COLUMN input_response_answer TEXT",
    )?;
    add_column_if_missing(conn, "ALTER TABLE tasks ADD COLUMN skill_id TEXT")?;
    add_column_if_missing(
        conn,
        "ALTER TABLE task_approvals ADD COLUMN payload_json TEXT",
    )?;
    add_column_if_missing(
        conn,
        "ALTER TABLE task_approvals ADD COLUMN answer_json TEXT",
    )?;
    Ok(())
}

/// v3: the whole input of a task.
///
/// `input_text` is cut at the observability limit; a task run outside the
/// coordinators also keeps its input whole, as JSON, so it can be read back
/// or run again as it was given. Nullable: a task without it is one that
/// predates the column or ran through a coordinator.
fn migrate_v3(conn: &Connection) -> Result<(), rusqlite::Error> {
    add_column_if_missing(conn, "ALTER TABLE tasks ADD COLUMN input_json TEXT")?;
    Ok(())
}

/// v4: batches, and the items of each batch.
///
/// A batch runs one agent on every item of a source. `batches` holds what it
/// was launched with (the agent, the source descriptor, the accesses granted
/// for its duration, its concurrency and its failure threshold), its state
/// and its counters. `batch_items` holds each item: its rank in the source,
/// its origin reference, its input, its status, its output, the sources it
/// cited, and the task that ran it, which joins `tasks`. The items are
/// written whole at launch, so a batch resumes from its pending items without
/// reading its source again.
fn migrate_v4(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS batches (
            batch_id      TEXT    PRIMARY KEY,
            agent_name    TEXT    NOT NULL,
            source_json   TEXT    NOT NULL,
            grants_json   TEXT    NOT NULL DEFAULT '[]',
            concurrency   INTEGER NOT NULL DEFAULT 1,
            max_failures  INTEGER NOT NULL,
            state         TEXT    NOT NULL,
            total         INTEGER NOT NULL DEFAULT 0,
            completed     INTEGER NOT NULL DEFAULT 0,
            failed        INTEGER NOT NULL DEFAULT 0,
            canceled      INTEGER NOT NULL DEFAULT 0,
            created_at    TEXT    NOT NULL DEFAULT CURRENT_TIMESTAMP,
            updated_at    TEXT    NOT NULL DEFAULT CURRENT_TIMESTAMP,
            ended_at      TEXT
        );
        CREATE INDEX IF NOT EXISTS idx_batches_state ON batches(state);

        CREATE TABLE IF NOT EXISTS batch_items (
            batch_id      TEXT    NOT NULL REFERENCES batches(batch_id),
            rank          INTEGER NOT NULL,
            origin_ref    TEXT    NOT NULL,
            input_json    TEXT    NOT NULL,
            status        TEXT    NOT NULL DEFAULT 'pending',
            output_text   TEXT,
            sources_json  TEXT,
            task_id       TEXT,
            updated_at    TEXT    NOT NULL DEFAULT CURRENT_TIMESTAMP,
            PRIMARY KEY (batch_id, rank)
        );
        CREATE INDEX IF NOT EXISTS idx_batch_items_status ON batch_items(batch_id, status);
        CREATE INDEX IF NOT EXISTS idx_batch_items_task ON batch_items(task_id);",
    )
}

/// v5: workflow runs, and the steps of each run.
///
/// `workflow_runs` holds one run of a workflow: its identifier
/// (`<package>/<workflow>`), the run that called it and its nesting depth
/// when a step started it, its inputs, its status (`running`, `completed`,
/// `failed`, `canceled`), what it returned or why it failed, and when it
/// started and ended. `workflow_steps` holds each declared step of the run,
/// in declared order, written `pending` at launch: its status (`pending`,
/// `running`, `completed`, `failed`, `canceled`, `not_started`), when it
/// started and ended, its output and its error. Timestamps carry
/// milliseconds, since steps follow each other faster than a second.
fn migrate_v5(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS workflow_runs (
            run_id         TEXT    PRIMARY KEY,
            workflow_id    TEXT    NOT NULL,
            parent_run_id  TEXT,
            depth          INTEGER NOT NULL DEFAULT 1,
            inputs_json    TEXT    NOT NULL,
            status         TEXT    NOT NULL DEFAULT 'running',
            output_json    TEXT,
            error          TEXT,
            started_at     TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
            ended_at       TEXT
        );
        CREATE INDEX IF NOT EXISTS idx_workflow_runs_workflow
            ON workflow_runs(workflow_id, started_at);
        CREATE INDEX IF NOT EXISTS idx_workflow_runs_status ON workflow_runs(status);

        CREATE TABLE IF NOT EXISTS workflow_steps (
            run_id      TEXT    NOT NULL REFERENCES workflow_runs(run_id),
            rank        INTEGER NOT NULL,
            name        TEXT    NOT NULL,
            status      TEXT    NOT NULL DEFAULT 'pending',
            started_at  TEXT,
            ended_at    TEXT,
            output      TEXT,
            error       TEXT,
            PRIMARY KEY (run_id, rank)
        );",
    )
}

/// v6: the history of an agent, read by agent.
///
/// The tasks and the batches of an agent are filtered by its name, to show
/// its history and to remove it when the agent is uninstalled; neither column
/// had an index.
fn migrate_v6(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch(
        "CREATE INDEX IF NOT EXISTS idx_tasks_agent_name ON tasks(agent_name);
         CREATE INDEX IF NOT EXISTS idx_batches_agent_name ON batches(agent_name);",
    )
}

/// Opens the file's schema at the current version, or refuses it.
///
/// Shared entry point for every `hitl.db` opener.
///
/// # Errors
///
/// Returns [`apollia_core::schema::SchemaError`] when the migration fails or
/// the database was written by a newer binary.
pub fn open_hitl_schema(conn: &Connection) -> Result<(), apollia_core::schema::SchemaError> {
    apollia_core::schema::open_versioned(
        conn,
        apollia_core::paths::DataFile::Hitl.file_name(),
        HITL_SCHEMA_VERSION,
        HITL_MIGRATIONS,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use apollia_core::schema::SchemaError;

    /// Pre-versioning `hitl.db` schema, frozen as the `005_hitl_tables`
    /// shape a published binary wrote (`user_version = 0`, no observability
    /// columns, no timing columns, no notification log).
    const HITL_V0_SQL: &str = include_str!("../tests/fixtures/schemas/hitl_v0.sql");

    // GIVEN a database written by a pre-versioning binary, with a task row
    // WHEN opening it through the versioned layer
    // THEN the row survives, the missing columns and table appear and the version is stamped
    #[test]
    fn test_hitl_db_old_format_migrates_and_keeps_rows() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(HITL_V0_SQL).unwrap();
        conn.execute(
            "INSERT INTO tasks (task_id, agent_name, status) VALUES ('t-1', 'mailer', 'completed')",
            [],
        )
        .unwrap();

        open_hitl_schema(&conn).unwrap();

        let (task, run_id): (String, Option<String>) = conn
            .query_row("SELECT task_id, run_id FROM tasks", [], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })
            .unwrap();
        assert_eq!(task, "t-1");
        assert!(run_id.is_none());
        let logs: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='notification_logs'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(logs, 1);
        let version: i64 = conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(version, i64::from(HITL_SCHEMA_VERSION));
    }

    // GIVEN a database at version 3, with a task row
    // WHEN opening it through the versioned layer
    // THEN the batch tables appear, an item joins its task, and the version is stamped
    #[test]
    fn test_hitl_db_v3_gains_the_batch_tables() {
        let conn = Connection::open_in_memory().unwrap();
        apollia_core::schema::open_versioned(&conn, "hitl.db", 3, &HITL_MIGRATIONS[..3]).unwrap();
        conn.execute(
            "INSERT INTO tasks (task_id, agent_name, status) VALUES ('t-1', 'clerk', 'completed')",
            [],
        )
        .unwrap();

        open_hitl_schema(&conn).unwrap();

        conn.execute(
            "INSERT INTO batches (batch_id, agent_name, source_json, max_failures, state) \
             VALUES ('b-1', 'clerk', '{}', 5, 'running')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO batch_items (batch_id, rank, origin_ref, input_json, task_id) \
             VALUES ('b-1', 0, 'row-2', '{}', 't-1')",
            [],
        )
        .unwrap();
        let (status, task_status): (String, String) = conn
            .query_row(
                "SELECT i.status, t.status FROM batch_items i JOIN tasks t ON t.task_id = i.task_id",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(status, "pending");
        assert_eq!(task_status, "completed");
        let version: i64 = conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(version, i64::from(HITL_SCHEMA_VERSION));
    }

    // GIVEN a database at version 4, with a batch row
    // WHEN opening it through the versioned layer
    // THEN the workflow tables appear, a step joins its run, and the version is stamped
    #[test]
    fn test_hitl_db_v4_gains_the_workflow_tables() {
        let conn = Connection::open_in_memory().unwrap();
        apollia_core::schema::open_versioned(&conn, "hitl.db", 4, &HITL_MIGRATIONS[..4]).unwrap();
        conn.execute(
            "INSERT INTO batches (batch_id, agent_name, source_json, max_failures, state) \
             VALUES ('b-1', 'clerk', '{}', 5, 'running')",
            [],
        )
        .unwrap();

        open_hitl_schema(&conn).unwrap();

        conn.execute(
            "INSERT INTO workflow_runs (run_id, workflow_id, inputs_json) \
             VALUES ('r-1', 'invoices/close-month', '{}')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO workflow_steps (run_id, rank, name) VALUES ('r-1', 0, 'extract')",
            [],
        )
        .unwrap();
        let (run_status, step_status, batches): (String, String, i64) = conn
            .query_row(
                "SELECT r.status, s.status, (SELECT COUNT(*) FROM batches) \
                 FROM workflow_steps s JOIN workflow_runs r ON r.run_id = s.run_id",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(run_status, "running");
        assert_eq!(step_status, "pending");
        assert_eq!(batches, 1);
        let version: i64 = conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(version, i64::from(HITL_SCHEMA_VERSION));
    }

    // GIVEN a database at version 5, with a task row
    // WHEN opening it through the versioned layer
    // THEN the tasks and the batches are indexed by agent, and the row survives
    #[test]
    fn test_hitl_db_v5_gains_the_agent_indexes() {
        let conn = Connection::open_in_memory().unwrap();
        apollia_core::schema::open_versioned(&conn, "hitl.db", 5, &HITL_MIGRATIONS[..5]).unwrap();
        conn.execute(
            "INSERT INTO tasks (task_id, agent_name, status) VALUES ('t-1', 'clerk', 'completed')",
            [],
        )
        .unwrap();

        open_hitl_schema(&conn).unwrap();

        let indexes: Vec<String> = conn
            .prepare(
                "SELECT name FROM sqlite_master WHERE type = 'index' \
                 AND name IN ('idx_tasks_agent_name', 'idx_batches_agent_name') ORDER BY name",
            )
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(
            indexes,
            vec!["idx_batches_agent_name", "idx_tasks_agent_name"]
        );
        let plan: String = conn
            .query_row(
                "EXPLAIN QUERY PLAN SELECT task_id FROM tasks WHERE agent_name = 'clerk'",
                [],
                |row| row.get(3),
            )
            .unwrap();
        assert!(plan.contains("idx_tasks_agent_name"), "{plan}");
        let tasks: i64 = conn
            .query_row("SELECT COUNT(*) FROM tasks", [], |row| row.get(0))
            .unwrap();
        assert_eq!(tasks, 1);
        let version: i64 = conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(version, 6);
    }

    // GIVEN a database stamped by a newer binary
    // WHEN opening it through the versioned layer
    // THEN the open is refused
    #[test]
    fn test_hitl_db_newer_version_is_refused() {
        let conn = Connection::open_in_memory().unwrap();
        conn.pragma_update(None, "user_version", HITL_SCHEMA_VERSION + 1)
            .unwrap();

        let err = open_hitl_schema(&conn).unwrap_err();

        assert!(matches!(err, SchemaError::NewerThanBinary { .. }));
    }
}
