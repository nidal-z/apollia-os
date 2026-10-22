//! What a connection is, as its user reads it, and the last state each one
//! was found in, persisted in `system.db`.
//!
//! A connection reads in one of four states: it works, it needs repair (with
//! the reason and the one action that repairs it), it is paused, or the
//! organisation blocked it. Until something has found it in one of them, it
//! reads as not checked yet, which says neither that it works nor that it is
//! broken. The state is found by the runtime, never entered
//! by anyone, and each record carries the moment it was found. The last
//! record of each connection is kept in the table `connection_states`
//! (created by `system_db::migrate_v13`), so a list opened after a restart
//! shows what was last known, dated, before anything has run again.

use std::path::Path;

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Errors from [`ConnectionStateStore`].
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum ConnectionStateError {
    /// Underlying SQLite error.
    #[error("database error: {0}")]
    Db(#[from] rusqlite::Error),

    /// The database schema could not be brought to the supported version.
    #[error(transparent)]
    Schema(#[from] crate::schema::SchemaError),

    /// A stored row holds a value this binary does not read.
    #[error("stored state of '{id}' is unreadable: {cause}")]
    Unreadable {
        /// Identifier of the connection.
        id: String,
        /// What could not be read.
        cause: String,
    },
}

/// The state of a connection, as its user reads it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionStatus {
    /// Its last check found it working.
    Working,
    /// Something must be done before it works; the reason says what.
    NeedsRepair,
    /// Its user paused it.
    Paused,
    /// The organisation policy forbids it.
    Blocked,
    /// Nothing has found it in any state yet: no check, no use, no failure.
    /// The action is to check it.
    NotChecked,
}

/// Why a connection is not working, as a key the interface translates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionReasonKey {
    /// No check has proven it works yet; the action is to check it.
    NotChecked,
    /// Its account must sign in again.
    SignInAgain,
    /// Its server refused the identifier or the password; the action is to
    /// enter the password again.
    PasswordRefused,
    /// Its folder is no longer where it was added; the action is to point it
    /// at the folder again.
    FolderNotFound,
    /// Its service cannot be reached.
    Unreachable,
    /// It is reached, but its operations fail.
    Failing,
    /// The organisation policy forbids it.
    ForbiddenByPolicy,
    /// The provider's organisation requires an administrator to approve the
    /// application first; the address of that approval is in
    /// [`ConnectionReason::address`].
    AdminApprovalRequired,
}

/// The reason a connection is in its state: a key, and the technical detail
/// shown in developer mode.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConnectionReason {
    /// What is wrong, as a key the interface translates.
    pub key: ConnectionReasonKey,
    /// Technical detail, already redacted.
    #[serde(default)]
    pub detail: Option<String>,
    /// The address the repairing action opens, shown to be copied: for an
    /// administrator approval, the approval page to send the administrator.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub address: Option<String>,
}

impl ConnectionReason {
    /// A reason with its key and an optional detail.
    #[must_use]
    pub fn new(key: ConnectionReasonKey, detail: Option<String>) -> Self {
        Self {
            key,
            detail,
            address: None,
        }
    }

    /// The same reason, with the address its repairing action opens.
    #[must_use]
    pub fn with_address(mut self, address: impl Into<String>) -> Self {
        self.address = Some(address.into());
        self
    }
}

/// The state one connection was found in, and when.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConnectionStateRecord {
    /// Identifier of the connection (`mcp:<server>`, `imap:<address>`, ...).
    pub id: String,
    /// The state found.
    pub state: ConnectionStatus,
    /// Why, when it is not working.
    #[serde(default)]
    pub reason: Option<ConnectionReason>,
    /// When it was found, RFC 3339 in UTC.
    pub checked_at: String,
}

impl ConnectionStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Working => "working",
            Self::NeedsRepair => "needs_repair",
            Self::Paused => "paused",
            Self::Blocked => "blocked",
            Self::NotChecked => "not_checked",
        }
    }

    fn parse(text: &str) -> Option<Self> {
        [
            Self::Working,
            Self::NeedsRepair,
            Self::Paused,
            Self::Blocked,
            Self::NotChecked,
        ]
        .into_iter()
        .find(|s| s.as_str() == text)
    }
}

/// The last state of every connection, in `system.db`.
pub struct ConnectionStateStore {
    conn: Connection,
}

impl ConnectionStateStore {
    /// Open (or create) `system.db` at `path` and bring it to the current
    /// schema version.
    ///
    /// # Errors
    /// [`ConnectionStateError::Db`] if the file cannot be opened, and
    /// [`ConnectionStateError::Schema`] if the migration fails or the file
    /// was written by a newer binary.
    pub fn open(path: &Path) -> Result<Self, ConnectionStateError> {
        let conn = Connection::open(path)?;
        crate::schema::open_versioned(
            &conn,
            crate::paths::DataFile::System.file_name(),
            crate::system_db::SYSTEM_DB_SCHEMA_VERSION,
            crate::system_db::SYSTEM_DB_MIGRATIONS,
        )?;
        Ok(Self { conn })
    }

    /// The last state of connection `id`, if one was ever recorded.
    ///
    /// # Errors
    /// [`ConnectionStateError::Db`] on any SQLite failure, and
    /// [`ConnectionStateError::Unreadable`] when the row holds a value this
    /// binary does not read.
    pub fn get(&self, id: &str) -> Result<Option<ConnectionStateRecord>, ConnectionStateError> {
        let row = self
            .conn
            .query_row(
                "SELECT id, state, reason, checked_at FROM connection_states WHERE id = ?1",
                params![id],
                raw_row,
            )
            .optional()?;
        row.map(record_of).transpose()
    }

    /// The last state of every connection that has one, by identifier.
    ///
    /// A row this binary does not read is left out, so the others still
    /// show.
    ///
    /// # Errors
    /// [`ConnectionStateError::Db`] on any SQLite failure.
    pub fn list(&self) -> Result<Vec<ConnectionStateRecord>, ConnectionStateError> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, state, reason, checked_at FROM connection_states ORDER BY id")?;
        let rows = stmt.query_map([], raw_row)?;
        let mut out = Vec::new();
        for row in rows {
            match record_of(row?) {
                Ok(record) => out.push(record),
                Err(e) => tracing::event!(
                    tracing::Level::WARN,
                    error = %e,
                    "connection_states.row.unreadable"
                ),
            }
        }
        Ok(out)
    }

    /// Keep `record` as the last state of its connection, replacing the one
    /// before.
    ///
    /// # Errors
    /// [`ConnectionStateError::Db`] on any SQLite failure, and
    /// [`ConnectionStateError::Unreadable`] when the reason cannot be
    /// written as JSON.
    pub fn put(&self, record: &ConnectionStateRecord) -> Result<(), ConnectionStateError> {
        let reason = record
            .reason
            .as_ref()
            .map(serde_json::to_string)
            .transpose()
            .map_err(|e| ConnectionStateError::Unreadable {
                id: record.id.clone(),
                cause: e.to_string(),
            })?;
        self.conn.execute(
            "INSERT INTO connection_states (id, state, reason, checked_at)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(id) DO UPDATE SET
                state = excluded.state,
                reason = excluded.reason,
                checked_at = excluded.checked_at",
            params![record.id, record.state.as_str(), reason, record.checked_at],
        )?;
        Ok(())
    }

    /// Forget the last state of connection `id`, once the connection is
    /// removed. Answers whether a state was kept for it.
    ///
    /// # Errors
    /// [`ConnectionStateError::Db`] on any SQLite failure.
    pub fn delete(&self, id: &str) -> Result<bool, ConnectionStateError> {
        let removed = self
            .conn
            .execute("DELETE FROM connection_states WHERE id = ?1", params![id])?;
        Ok(removed > 0)
    }
}

type RawRow = (String, String, Option<String>, String);

fn raw_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<RawRow> {
    Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
}

fn record_of(
    (id, state, reason, checked_at): RawRow,
) -> Result<ConnectionStateRecord, ConnectionStateError> {
    let unreadable = |cause: String| ConnectionStateError::Unreadable {
        id: id.clone(),
        cause,
    };
    let state =
        ConnectionStatus::parse(&state).ok_or_else(|| unreadable(format!("state '{state}'")))?;
    let reason = reason
        .as_deref()
        .map(serde_json::from_str::<ConnectionReason>)
        .transpose()
        .map_err(|e| unreadable(e.to_string()))?;
    Ok(ConnectionStateRecord {
        id,
        state,
        reason,
        checked_at,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::DataFile;

    fn store(dir: &tempfile::TempDir) -> ConnectionStateStore {
        ConnectionStateStore::open(&dir.path().join(DataFile::System.file_name())).unwrap()
    }

    fn record(
        id: &str,
        state: ConnectionStatus,
        reason: Option<ConnectionReason>,
    ) -> ConnectionStateRecord {
        ConnectionStateRecord {
            id: id.to_owned(),
            state,
            reason,
            checked_at: "2026-10-21T09:00:00Z".to_owned(),
        }
    }

    #[test]
    fn test_a_recorded_state_is_read_back_after_the_file_is_reopened() {
        // GIVEN a state recorded for one connection, with its reason
        let dir = tempfile::tempdir().unwrap();
        let reason = ConnectionReason::new(
            ConnectionReasonKey::SignInAgain,
            Some("unauthorized".to_owned()),
        );
        store(&dir)
            .put(&record(
                "mcp:notion",
                ConnectionStatus::NeedsRepair,
                Some(reason.clone()),
            ))
            .unwrap();

        // WHEN the file is opened again, as after a restart
        let reopened = store(&dir);

        // THEN the state, its reason and its date read back as written
        let got = reopened.get("mcp:notion").unwrap().unwrap();
        assert_eq!(got.state, ConnectionStatus::NeedsRepair);
        assert_eq!(got.reason, Some(reason));
        assert_eq!(got.checked_at, "2026-10-21T09:00:00Z");
        assert_eq!(reopened.list().unwrap().len(), 1);
    }

    #[test]
    fn test_an_approval_address_is_kept_and_a_reason_without_one_still_reads() {
        // GIVEN a connection blocked until an administrator approves it, and a
        // reason row written before reasons carried an address
        let dir = tempfile::tempdir().unwrap();
        let store = store(&dir);
        let reason = ConnectionReason::new(
            ConnectionReasonKey::AdminApprovalRequired,
            Some("AADSTS90094".to_owned()),
        )
        .with_address("https://login.microsoftonline.com/organizations/adminconsent?client_id=c");
        store
            .put(&record(
                "native:microsoft",
                ConnectionStatus::Blocked,
                Some(reason.clone()),
            ))
            .unwrap();
        store
            .conn
            .execute(
                "INSERT INTO connection_states (id, state, reason, checked_at)
                 VALUES ('mcp:old', 'needs_repair', '{\"key\":\"failing\",\"detail\":null}',
                         '2026-10-21T09:00:00Z')",
                [],
            )
            .unwrap();

        // WHEN both are read back
        let blocked = store.get("native:microsoft").unwrap().unwrap();
        let old = store.get("mcp:old").unwrap().unwrap();

        // THEN the key and the address survive, and the older row reads with
        // no address
        assert_eq!(blocked.reason, Some(reason));
        assert_eq!(
            serde_json::to_value(&blocked.reason).unwrap()["key"],
            "admin_approval_required"
        );
        assert_eq!(old.reason.unwrap().address, None);
    }

    #[test]
    fn test_a_deleted_state_is_gone_and_a_second_delete_says_so() {
        // GIVEN states kept for two connections
        let dir = tempfile::tempdir().unwrap();
        let store = store(&dir);
        store
            .put(&record("mcp:notion", ConnectionStatus::Working, None))
            .unwrap();
        store
            .put(&record("mcp:other", ConnectionStatus::Working, None))
            .unwrap();

        // WHEN the state of one is deleted, twice
        let first = store.delete("mcp:notion").unwrap();
        let second = store.delete("mcp:notion").unwrap();

        // THEN it is gone, the other is kept, and only the first delete
        //      found a row
        assert!(first);
        assert!(!second);
        assert!(store.get("mcp:notion").unwrap().is_none());
        assert_eq!(store.list().unwrap().len(), 1);
    }

    #[test]
    fn test_a_new_state_replaces_the_previous_one() {
        // GIVEN a connection recorded as needing repair
        let dir = tempfile::tempdir().unwrap();
        let store = store(&dir);
        store
            .put(&record(
                "imap:ada@example.com",
                ConnectionStatus::NeedsRepair,
                Some(ConnectionReason::new(
                    ConnectionReasonKey::Unreachable,
                    None,
                )),
            ))
            .unwrap();

        // WHEN it is then found working
        let mut working = record("imap:ada@example.com", ConnectionStatus::Working, None);
        working.checked_at = "2026-10-21T10:00:00Z".to_owned();
        store.put(&working).unwrap();

        // THEN one row is left, the last one
        assert_eq!(store.list().unwrap(), vec![working]);
        assert!(store.get("mcp:other").unwrap().is_none());
    }

    #[test]
    fn test_an_unreadable_row_is_left_out_of_the_list() {
        // GIVEN one readable row and one whose state this binary does not know
        let dir = tempfile::tempdir().unwrap();
        let store = store(&dir);
        store
            .put(&record("folder:a", ConnectionStatus::Working, None))
            .unwrap();
        store
            .conn
            .execute(
                "INSERT INTO connection_states (id, state, checked_at)
                 VALUES ('folder:b', 'later', '2026-10-21T09:00:00Z')",
                [],
            )
            .unwrap();

        // WHEN the states are listed and the unreadable one is read alone
        let listed = store.list().unwrap();
        let alone = store.get("folder:b");

        // THEN the list keeps the readable row, and the read alone says why
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, "folder:a");
        assert!(matches!(
            alone,
            Err(ConnectionStateError::Unreadable { .. })
        ));
    }

    #[test]
    fn test_the_wire_names_are_the_stored_names() {
        // GIVEN every state
        let all = [
            ConnectionStatus::Working,
            ConnectionStatus::NeedsRepair,
            ConnectionStatus::Paused,
            ConnectionStatus::Blocked,
            ConnectionStatus::NotChecked,
        ];
        for status in all {
            // WHEN it is serialized and written as text
            let wire = serde_json::to_value(status).unwrap();
            // THEN both say the same, and the text reads back
            assert_eq!(wire.as_str(), Some(status.as_str()));
            assert_eq!(ConnectionStatus::parse(status.as_str()), Some(status));
        }
    }
}
