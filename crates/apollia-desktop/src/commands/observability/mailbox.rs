//! The inter-agent mailbox as the observability view reads it: the rows of
//! `mailbox.db`, newest first, read directly rather than through the API.

use serde::Serialize;

// ---------------------------------------------------------------------------
// Mailbox Messages
// ---------------------------------------------------------------------------

/// One inter-agent mailbox message, projected for the observability view.
#[derive(Debug, Serialize)]
pub struct MailboxMessageRow {
    /// Stable message identifier (primary key).
    pub message_id: String,
    /// Sending agent name (or `host:<id>` for a host injection).
    pub from_agent: String,
    /// Receiving agent name.
    pub to_agent: String,
    /// Raw JSON payload as stored, rendered verbatim in the UI.
    pub payload: String,
    /// Send timestamp (RFC 3339).
    pub sent_at: String,
    /// Delivery state: `pending` or `in_flight`.
    pub state: String,
}

/// Errors raised while reading the durable mailbox store.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum MailboxQueryError {
    /// The blocking read task failed to join.
    #[error("mailbox read task failed: {0}")]
    Join(String),
    /// A SQLite operation against `mailbox.db` failed.
    #[error("mailbox read failed")]
    Sqlite(#[from] rusqlite::Error),
}

/// Lists the most recent inter-agent mailbox messages across every recipient.
///
/// Reads the durable `mailbox.db` store written by the runtime mailbox actor,
/// returning rows newest first (descending `seq`). A missing store, or one whose
/// table has not been created yet, yields an empty list rather than an error:
/// the mailbox is simply empty until the first message is sent.
#[tauri::command]
pub async fn list_mailbox_messages(limit: Option<u32>) -> Result<Vec<MailboxMessageRow>, String> {
    // Resolve data_dir the same way the desktop bootstrapper does (main.rs).
    let data_dir = {
        let home = apollia_core::paths::home_dir_or_temp()
            .display()
            .to_string();
        apollia_core::paths::data_dir_under(home)
    };
    read_mailbox_messages(data_dir, limit.unwrap_or(100))
        .await
        .map_err(|e| e.to_string())
}

/// Inner reader, decoupled from the HOME lookup so it stays unit-testable with a
/// caller-provided data directory.
async fn read_mailbox_messages(
    data_dir: std::path::PathBuf,
    limit: u32,
) -> Result<Vec<MailboxMessageRow>, MailboxQueryError> {
    tokio::task::spawn_blocking(move || query_mailbox_db(&data_dir, limit))
        .await
        .map_err(|e| MailboxQueryError::Join(e.to_string()))?
}

/// Runs the mailbox SELECT on a blocking thread. Returns an empty list when the
/// store file or its `mailbox_messages` table is absent.
fn query_mailbox_db(
    data_dir: &std::path::Path,
    limit: u32,
) -> Result<Vec<MailboxMessageRow>, MailboxQueryError> {
    let path = data_dir.join(apollia_core::paths::DataFile::Mailbox.file_name());
    if !path.exists() {
        return Ok(Vec::new());
    }
    let conn = rusqlite::Connection::open(&path)?;
    let table_present = conn
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'mailbox_messages'",
            [],
            |_| Ok(()),
        )
        .is_ok();
    if !table_present {
        return Ok(Vec::new());
    }
    let mut stmt = conn.prepare(
        "SELECT message_id, from_agent, to_agent, payload, sent_at, state
         FROM mailbox_messages
         ORDER BY seq DESC
         LIMIT ?1",
    )?;
    let rows = stmt.query_map([limit], |row| {
        Ok(MailboxMessageRow {
            message_id: row.get(0)?,
            from_agent: row.get(1)?,
            to_agent: row.get(2)?,
            payload: row.get(3)?,
            sent_at: row.get(4)?,
            state: row.get(5)?,
        })
    })?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mailbox_message_row_serializes() {
        // GIVEN a mailbox row
        let row = MailboxMessageRow {
            message_id: "m-1".to_string(),
            from_agent: "apollia-guide".to_string(),
            to_agent: "seed-classifier".to_string(),
            payload: r#"{"kind":"ping"}"#.to_string(),
            sent_at: "2026-07-01T09:00:00Z".to_string(),
            state: "pending".to_string(),
        };

        // WHEN serialized to JSON
        let json = serde_json::to_value(&row).expect("serialize");

        // THEN every field is present under its snake_case name
        assert_eq!(json["message_id"], "m-1");
        assert_eq!(json["from_agent"], "apollia-guide");
        assert_eq!(json["to_agent"], "seed-classifier");
        assert_eq!(json["payload"], r#"{"kind":"ping"}"#);
        assert_eq!(json["state"], "pending");
    }

    #[test]
    fn test_query_mailbox_db_missing_file_is_empty() {
        // GIVEN a data dir without a mailbox.db
        let dir = tempfile::tempdir().expect("tempdir");

        // WHEN querying the mailbox store
        let rows = query_mailbox_db(dir.path(), 100).expect("query");

        // THEN the result is empty, not an error
        assert!(rows.is_empty());
    }

    #[test]
    fn test_query_mailbox_db_orders_by_seq_desc() {
        // GIVEN a mailbox.db seeded with two messages of increasing seq
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir
            .path()
            .join(apollia_core::paths::DataFile::Mailbox.file_name());
        let conn = rusqlite::Connection::open(&path).expect("open");
        conn.execute_batch(
            "CREATE TABLE mailbox_messages (
                message_id       TEXT PRIMARY KEY,
                to_agent         TEXT    NOT NULL,
                from_agent       TEXT    NOT NULL,
                payload          TEXT    NOT NULL,
                sent_at          TEXT    NOT NULL,
                created_unix     INTEGER NOT NULL,
                state            TEXT    NOT NULL,
                lease_until_unix INTEGER,
                lease_owner      TEXT,
                seq              INTEGER NOT NULL
            );",
        )
        .expect("schema");
        conn.execute(
            "INSERT INTO mailbox_messages VALUES \
             ('m-1','b','a','{}','2026-07-01T09:00:00Z',1,'pending',NULL,NULL,1)",
            [],
        )
        .expect("insert 1");
        conn.execute(
            "INSERT INTO mailbox_messages VALUES \
             ('m-2','a','b','{}','2026-07-01T09:01:00Z',2,'pending',NULL,NULL,2)",
            [],
        )
        .expect("insert 2");

        // WHEN querying the mailbox store
        let rows = query_mailbox_db(dir.path(), 100).expect("query");

        // THEN the newest (highest seq) message comes first
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].message_id, "m-2");
        assert_eq!(rows[1].message_id, "m-1");
    }
}
