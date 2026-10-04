//! Per-tool credentials (for example `web_search/brave.api_key`): values in the
//! secret store, index in `governance.db`.
//!
//! The value of a credential lives in the configured
//! [`SecretStore`](apollia_auth::SecretStore), under the service
//! [`TOOL_CREDENTIALS_SERVICE`] and the account `<tool>/<key>`, like every
//! other secret of a connection. `governance.db` keeps only what can be shown
//! without a secret: which credentials exist and when they were created and
//! last used, because a secret store cannot list its entries.
//!
//! Opening the store finishes, once, the move of the values an older version
//! encrypted in `governance.db` with a key file (see [`legacy`]).

mod legacy;

use std::path::Path;

use apollia_auth::SecretStore;
use rusqlite::{params, Connection, OpenFlags, OptionalExtension};

use crate::tool_registry::ToolGovernanceError;

/// Secret-store service holding the values of the tool credentials.
pub const TOOL_CREDENTIALS_SERVICE: &str = "apollia-tool-credentials";

/// Secret-store account of the `(tool, key)` credential.
pub fn credential_account(tool: &str, key: &str) -> String {
    format!("{tool}/{key}")
}

/// An entry of [`ToolCredentialStore::list`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CredentialEntry {
    /// Name of the tool that owns the credential.
    pub tool_name: String,
    /// Logical key name (e.g. `brave.api_key`).
    pub key_name: String,
    /// Unix timestamp (seconds) of creation.
    pub created_at: i64,
    /// Unix timestamp (seconds) of the last effective use, if any.
    pub last_used_at: Option<i64>,
}

/// Per-tool credential store: the value in the secret store, the index in
/// `governance.db`.
pub struct ToolCredentialStore {
    conn: Connection,
    secrets: Box<dyn SecretStore>,
}

impl ToolCredentialStore {
    /// Opens the store on *db_path* with the secret store selected by
    /// `APOLLIA_TOKEN_STORAGE`, the one every other connection secret uses.
    ///
    /// # Errors
    ///
    /// [`ToolGovernanceError::Secrets`] when the secret store cannot be
    /// selected; otherwise as [`ToolCredentialStore::new`].
    pub fn open(db_path: &Path) -> Result<Self, ToolGovernanceError> {
        let secrets = apollia_auth::select_secret_store()?;
        Self::new(db_path, secrets)
    }

    /// Opens the store on *db_path* over `secrets`, brings the schema of
    /// `governance.db` to the current version, and moves into `secrets` the
    /// values an older version left encrypted in the file.
    ///
    /// # Errors
    ///
    /// - [`ToolGovernanceError::Database`] if SQLite fails.
    /// - [`ToolGovernanceError::Schema`] if the schema cannot be migrated.
    /// - [`ToolGovernanceError::Secrets`] if a moved value cannot be written;
    ///   the move is then left pending and replayed at the next open.
    pub fn new(db_path: &Path, secrets: Box<dyn SecretStore>) -> Result<Self, ToolGovernanceError> {
        let conn = Connection::open_with_flags(
            db_path,
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE,
        )?;
        conn.execute_batch("PRAGMA foreign_keys=ON;")?;
        apollia_permissions::open_governance_schema(&conn)?;
        legacy::move_pending_values(&conn, db_path, secrets.as_ref())?;
        Ok(Self { conn, secrets })
    }

    /// Stores (inserts or replaces) the `(tool, key)` credential: the value in
    /// the secret store, the entry in the index.
    ///
    /// # Errors
    ///
    /// - [`ToolGovernanceError::Secrets`] if the secret store refuses the value.
    /// - [`ToolGovernanceError::Database`] if the index write fails.
    pub fn set(&mut self, tool: &str, key: &str, value: &str) -> Result<(), ToolGovernanceError> {
        self.secrets.set(
            TOOL_CREDENTIALS_SERVICE,
            &credential_account(tool, key),
            value,
        )?;
        self.conn.execute(
            "INSERT INTO tool_credentials (tool_name, key_name, created_at)
             VALUES (?1, ?2, unixepoch())
             ON CONFLICT(tool_name, key_name) DO UPDATE SET
               created_at   = unixepoch(),
               last_used_at = NULL",
            params![tool, key],
        )?;
        Ok(())
    }

    /// Fetches the value of `(tool, key)`, or `None` when the index does not
    /// name it or the secret store holds no value for it.
    ///
    /// # Errors
    ///
    /// - [`ToolGovernanceError::Database`] if the index read fails.
    /// - [`ToolGovernanceError::Secrets`] if the secret store fails.
    pub fn get(&self, tool: &str, key: &str) -> Result<Option<String>, ToolGovernanceError> {
        let indexed = self
            .conn
            .query_row(
                "SELECT 1 FROM tool_credentials WHERE tool_name = ?1 AND key_name = ?2",
                params![tool, key],
                |_| Ok(()),
            )
            .optional()?;
        if indexed.is_none() {
            return Ok(None);
        }
        Ok(self
            .secrets
            .get(TOOL_CREDENTIALS_SERVICE, &credential_account(tool, key))?)
    }

    /// Deletes the `(tool, key)` credential. Returns `true` if the index named
    /// it.
    ///
    /// # Errors
    ///
    /// - [`ToolGovernanceError::Secrets`] if the secret store fails.
    /// - [`ToolGovernanceError::Database`] if SQLite fails.
    pub fn delete(&mut self, tool: &str, key: &str) -> Result<bool, ToolGovernanceError> {
        self.secrets
            .delete(TOOL_CREDENTIALS_SERVICE, &credential_account(tool, key))?;
        let n = self.conn.execute(
            "DELETE FROM tool_credentials WHERE tool_name = ?1 AND key_name = ?2",
            params![tool, key],
        )?;
        Ok(n > 0)
    }

    /// Lists credentials, filtered by tool if *tool* is `Some`.
    ///
    /// Reads the index only: no value is read, so a "credential present"
    /// state is shown without touching a secret.
    ///
    /// # Errors
    ///
    /// Returns [`ToolGovernanceError::Database`] if the query fails.
    pub fn list(&self, tool: Option<&str>) -> Result<Vec<CredentialEntry>, ToolGovernanceError> {
        index_entries(&self.conn, tool)
    }

    /// Updates `last_used_at` for the `(tool, key)` credential.
    ///
    /// No-op if the credential does not exist.
    ///
    /// # Errors
    ///
    /// Returns [`ToolGovernanceError::Database`] if the write fails.
    pub fn touch_last_used(&mut self, tool: &str, key: &str) -> Result<(), ToolGovernanceError> {
        self.conn.execute(
            "UPDATE tool_credentials SET last_used_at = unixepoch() \
             WHERE tool_name = ?1 AND key_name = ?2",
            params![tool, key],
        )?;
        Ok(())
    }
}

/// Lists the credentials the index of the `governance.db` at *db_path*
/// names, filtered by tool if *tool* is `Some`, without opening a secret
/// store: listing reads no value.
///
/// # Errors
///
/// [`ToolGovernanceError::Database`] or [`ToolGovernanceError::Schema`] when
/// the database cannot be opened or read.
pub fn list_credential_index(
    db_path: &Path,
    tool: Option<&str>,
) -> Result<Vec<CredentialEntry>, ToolGovernanceError> {
    let conn = Connection::open_with_flags(
        db_path,
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE,
    )?;
    apollia_permissions::open_governance_schema(&conn)?;
    index_entries(&conn, tool)
}

fn index_entries(
    conn: &Connection,
    tool: Option<&str>,
) -> Result<Vec<CredentialEntry>, ToolGovernanceError> {
    let mut entries = Vec::new();
    match tool {
        Some(t) => {
            let mut stmt = conn.prepare(
                "SELECT tool_name, key_name, created_at, last_used_at \
                 FROM tool_credentials WHERE tool_name = ?1 ORDER BY key_name",
            )?;
            let rows = stmt.query_map(params![t], map_credential_row)?;
            for row in rows {
                entries.push(row?);
            }
        }
        None => {
            let mut stmt = conn.prepare(
                "SELECT tool_name, key_name, created_at, last_used_at \
                 FROM tool_credentials ORDER BY tool_name, key_name",
            )?;
            let rows = stmt.query_map([], map_credential_row)?;
            for row in rows {
                entries.push(row?);
            }
        }
    }
    Ok(entries)
}

fn map_credential_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<CredentialEntry> {
    Ok(CredentialEntry {
        tool_name: row.get(0)?,
        key_name: row.get(1)?,
        created_at: row.get(2)?,
        last_used_at: row.get(3)?,
    })
}

#[cfg(test)]
pub(crate) mod test_support {
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    use apollia_auth::{AuthError, SecretStore};

    /// In-memory secret store shared by its clones, so a test can read what a
    /// store it handed over wrote.
    #[derive(Clone, Default)]
    pub(crate) struct MemorySecrets {
        pub(crate) entries: Arc<Mutex<HashMap<(String, String), String>>>,
    }

    impl MemorySecrets {
        pub(crate) fn value(&self, service: &str, user: &str) -> Option<String> {
            let entries = self.entries.lock().unwrap_or_else(|e| e.into_inner());
            entries.get(&(service.to_owned(), user.to_owned())).cloned()
        }
    }

    impl SecretStore for MemorySecrets {
        fn set(&self, service: &str, user: &str, value: &str) -> Result<(), AuthError> {
            let mut entries = self.entries.lock().unwrap_or_else(|e| e.into_inner());
            entries.insert((service.to_owned(), user.to_owned()), value.to_owned());
            Ok(())
        }

        fn get(&self, service: &str, user: &str) -> Result<Option<String>, AuthError> {
            Ok(self.value(service, user))
        }

        fn delete(&self, service: &str, user: &str) -> Result<(), AuthError> {
            let mut entries = self.entries.lock().unwrap_or_else(|e| e.into_inner());
            entries.remove(&(service.to_owned(), user.to_owned()));
            Ok(())
        }

        fn backend_id(&self) -> &'static str {
            "memory"
        }
    }
}

#[cfg(test)]
mod tests;
