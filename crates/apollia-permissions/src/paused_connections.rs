//! The connections the user put in pause, in `governance.db`.
//!
//! A connection in pause keeps everything it was given: its configuration,
//! its secrets, and the accesses of the agents that hold its tools. While it
//! is in pause none of its tools runs; resuming it takes it back as it was.
//! The store holds the identifiers alone, as the connection list names them
//! (`mcp:<server>`, `native:<provider>:<account>`, `imap:<address>`,
//! `folder:<id>`).

use std::collections::HashSet;
use std::path::Path;

use rusqlite::{params, Connection, OpenFlags};

use crate::error::PermissionError;

/// The table of paused connections in `governance.db`.
pub struct PausedConnectionStore {
    db: Connection,
}

impl PausedConnectionStore {
    /// Opens `db_path`, bringing its schema to the current version.
    ///
    /// # Errors
    ///
    /// [`PermissionError::Database`] when the file cannot be opened,
    /// [`PermissionError::Schema`] when it was written by a newer binary.
    pub fn new(db_path: &Path) -> Result<Self, PermissionError> {
        let db = Connection::open_with_flags(
            db_path,
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE,
        )?;
        db.execute_batch("PRAGMA journal_mode=WAL;")?;
        crate::governance_schema::open_governance_schema(&db)?;
        Ok(Self { db })
    }

    /// Puts `connection_id` in pause at Unix time `now`. Returns `true` when
    /// it was not in pause yet.
    ///
    /// # Errors
    ///
    /// [`PermissionError::InvalidRule`] for an empty identifier,
    /// [`PermissionError::Database`] when the write fails.
    pub fn pause(&self, connection_id: &str, now: i64) -> Result<bool, PermissionError> {
        if connection_id.trim().is_empty() {
            return Err(PermissionError::InvalidRule(
                "a paused connection needs an identifier".to_owned(),
            ));
        }
        let written = self.db.execute(
            "INSERT OR IGNORE INTO paused_connections (connection_id, paused_at) VALUES (?1, ?2)",
            params![connection_id, now],
        )?;
        Ok(written > 0)
    }

    /// Takes `connection_id` out of pause. Returns `true` when it was in
    /// pause.
    ///
    /// # Errors
    ///
    /// [`PermissionError::Database`] when the write fails.
    pub fn resume(&self, connection_id: &str) -> Result<bool, PermissionError> {
        let removed = self.db.execute(
            "DELETE FROM paused_connections WHERE connection_id = ?1",
            params![connection_id],
        )?;
        Ok(removed > 0)
    }

    /// Every connection in pause.
    ///
    /// # Errors
    ///
    /// [`PermissionError::Database`] when the read fails.
    pub fn list(&self) -> Result<HashSet<String>, PermissionError> {
        let mut stmt = self
            .db
            .prepare("SELECT connection_id FROM paused_connections")?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        rows.collect::<Result<HashSet<_>, _>>()
            .map_err(PermissionError::from)
    }
}

/// The connections in pause in `db_path`, read without writing: a file that
/// does not exist, or that has no table of paused connections yet, holds none.
///
/// # Errors
///
/// [`PermissionError::Database`] when an existing file cannot be read.
pub fn read_paused_connections(db_path: &Path) -> Result<HashSet<String>, PermissionError> {
    if !db_path.exists() {
        return Ok(HashSet::new());
    }
    let db = Connection::open_with_flags(db_path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let has_table: bool = db.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' \
         AND name = 'paused_connections')",
        [],
        |row| row.get(0),
    )?;
    if !has_table {
        return Ok(HashSet::new());
    }
    let mut stmt = db.prepare("SELECT connection_id FROM paused_connections")?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
    rows.collect::<Result<HashSet<_>, _>>()
        .map_err(PermissionError::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    // GIVEN a fresh governance database
    // WHEN a connection is paused twice, then resumed twice
    // THEN the first of each says it changed something, the second does not,
    // and the list follows
    #[test]
    fn test_pause_and_resume_are_idempotent_and_listed() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("governance.db");
        let store = PausedConnectionStore::new(&path).unwrap();

        assert!(store.pause("mcp:notion", 10).unwrap());
        assert!(!store.pause("mcp:notion", 11).unwrap());
        assert_eq!(
            store.list().unwrap(),
            HashSet::from(["mcp:notion".to_owned()])
        );
        assert_eq!(
            read_paused_connections(&path).unwrap(),
            HashSet::from(["mcp:notion".to_owned()])
        );

        assert!(store.resume("mcp:notion").unwrap());
        assert!(!store.resume("mcp:notion").unwrap());
        assert!(store.list().unwrap().is_empty());
    }

    // GIVEN no governance database, then one opened before the table existed
    // WHEN the paused connections are read without writing
    // THEN none is answered
    #[test]
    fn test_reading_a_file_without_the_table_answers_none() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("governance.db");
        assert!(read_paused_connections(&path).unwrap().is_empty());

        let conn = Connection::open(&path).unwrap();
        conn.execute_batch("CREATE TABLE other (x INTEGER);")
            .unwrap();
        drop(conn);

        assert!(read_paused_connections(&path).unwrap().is_empty());
    }

    // GIVEN a governance database
    // WHEN an empty identifier is paused
    // THEN it is refused and nothing is written
    #[test]
    fn test_an_empty_identifier_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let store = PausedConnectionStore::new(&dir.path().join("governance.db")).unwrap();

        assert!(store.pause("  ", 1).is_err());
        assert!(store.list().unwrap().is_empty());
    }
}
