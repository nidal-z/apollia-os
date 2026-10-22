//! The folders of this machine added as connections, persisted in `system.db`.
//!
//! A folder connection is a folder the user chose with the system picker. It
//! is kept by its canonical path (symbolic links resolved, no `..`), a label
//! and the moment it was added. The table is created by
//! `system_db::migrate_v9`.

use std::path::Path;

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Errors from [`FolderConnectionStore`].
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum FolderConnectionError {
    /// Underlying SQLite error.
    #[error("database error: {0}")]
    Db(#[from] rusqlite::Error),

    /// The database schema could not be brought to the supported version.
    #[error(transparent)]
    Schema(#[from] crate::schema::SchemaError),

    /// The path does not name an existing folder.
    #[error("'{path}' is not a folder of this machine")]
    NotAFolder {
        /// The path as given.
        path: String,
    },

    /// The folder is already a connection.
    #[error("the folder '{path}' is already a connection")]
    AlreadyAdded {
        /// Canonical path of the folder.
        path: String,
    },

    /// No folder connection has this identifier.
    #[error("folder connection '{0}' not found")]
    NotFound(String),
}

/// One folder added as a connection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderConnection {
    /// Identifier of the row.
    pub id: String,
    /// Canonical absolute path of the folder.
    pub path: String,
    /// Name shown to the user: the folder's own name unless one was given.
    pub label: String,
    /// When it was added, RFC 3339 in UTC.
    pub added_at: String,
}

/// The folder connections of `system.db`.
pub struct FolderConnectionStore {
    conn: Connection,
}

impl FolderConnectionStore {
    /// Open (or create) `system.db` at `path` and bring it to the current
    /// schema version.
    ///
    /// # Errors
    /// [`FolderConnectionError::Db`] if the file cannot be opened, and
    /// [`FolderConnectionError::Schema`] if the migration fails or the file
    /// was written by a newer binary.
    pub fn open(path: &Path) -> Result<Self, FolderConnectionError> {
        let conn = Connection::open(path)?;
        crate::schema::open_versioned(
            &conn,
            crate::paths::DataFile::System.file_name(),
            crate::system_db::SYSTEM_DB_SCHEMA_VERSION,
            crate::system_db::SYSTEM_DB_MIGRATIONS,
        )?;
        Ok(Self { conn })
    }

    /// Every folder connection, oldest first.
    ///
    /// # Errors
    /// [`FolderConnectionError::Db`] on any SQLite failure.
    pub fn list(&self) -> Result<Vec<FolderConnection>, FolderConnectionError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, path, label, added_at FROM folder_connections
             ORDER BY added_at, id",
        )?;
        let rows = stmt.query_map([], row_to_folder)?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    /// The folder connection `id`, if any.
    ///
    /// # Errors
    /// [`FolderConnectionError::Db`] on any SQLite failure.
    pub fn get(&self, id: &str) -> Result<Option<FolderConnection>, FolderConnectionError> {
        Ok(self
            .conn
            .query_row(
                "SELECT id, path, label, added_at FROM folder_connections WHERE id = ?1",
                params![id],
                row_to_folder,
            )
            .optional()?)
    }

    /// The canonical form of `path`, when it names an existing folder.
    ///
    /// # Errors
    /// [`FolderConnectionError::NotAFolder`] when the path does not resolve
    /// to a folder.
    pub fn canonical_folder(path: &str) -> Result<String, FolderConnectionError> {
        let not_a_folder = || FolderConnectionError::NotAFolder {
            path: path.to_owned(),
        };
        if path.trim().is_empty() {
            return Err(not_a_folder());
        }
        let canonical = std::fs::canonicalize(path).map_err(|_| not_a_folder())?;
        if !canonical.is_dir() {
            return Err(not_a_folder());
        }
        canonical
            .to_str()
            .map(str::to_owned)
            .ok_or_else(not_a_folder)
    }

    /// Add the folder at `path` as a connection, labelled `label` or, when
    /// none is given, by the folder's own name.
    ///
    /// # Errors
    /// [`FolderConnectionError::NotAFolder`] when `path` is not an existing
    /// folder, [`FolderConnectionError::AlreadyAdded`] when its canonical path
    /// is already a connection, and [`FolderConnectionError::Db`] on any
    /// SQLite failure.
    pub fn add(
        &self,
        path: &str,
        label: Option<&str>,
    ) -> Result<FolderConnection, FolderConnectionError> {
        let canonical = Self::canonical_folder(path)?;
        let exists: Option<String> = self
            .conn
            .query_row(
                "SELECT id FROM folder_connections WHERE path = ?1",
                params![canonical],
                |row| row.get(0),
            )
            .optional()?;
        if exists.is_some() {
            return Err(FolderConnectionError::AlreadyAdded { path: canonical });
        }
        let label = label
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .map_or_else(|| folder_name(&canonical), str::to_owned);
        let id = uuid::Uuid::new_v4().to_string();
        self.conn.execute(
            "INSERT INTO folder_connections (id, path, label) VALUES (?1, ?2, ?3)",
            params![id, canonical, label],
        )?;
        self.get(&id)?.ok_or(FolderConnectionError::NotFound(id))
    }

    /// Point the folder connection `id` at the folder `path`, its identifier,
    /// label and date kept, so what refers to the connection by its
    /// identifier (the accesses of its agents, the sources made on it) follows
    /// it to the new folder.
    ///
    /// # Errors
    /// [`FolderConnectionError::NotFound`] when no row has this identifier,
    /// [`FolderConnectionError::NotAFolder`] when `path` is not an existing
    /// folder, [`FolderConnectionError::AlreadyAdded`] when another connection
    /// already holds its canonical path, and [`FolderConnectionError::Db`] on
    /// any SQLite failure.
    pub fn repoint(&self, id: &str, path: &str) -> Result<FolderConnection, FolderConnectionError> {
        if self.get(id)?.is_none() {
            return Err(FolderConnectionError::NotFound(id.to_owned()));
        }
        let canonical = Self::canonical_folder(path)?;
        let holder: Option<String> = self
            .conn
            .query_row(
                "SELECT id FROM folder_connections WHERE path = ?1",
                params![canonical],
                |row| row.get(0),
            )
            .optional()?;
        if holder.as_deref().is_some_and(|other| other != id) {
            return Err(FolderConnectionError::AlreadyAdded { path: canonical });
        }
        self.conn.execute(
            "UPDATE folder_connections SET path = ?1 WHERE id = ?2",
            params![canonical, id],
        )?;
        self.get(id)?
            .ok_or_else(|| FolderConnectionError::NotFound(id.to_owned()))
    }

    /// Remove the folder connection `id` and answer the row it was.
    ///
    /// # Errors
    /// [`FolderConnectionError::NotFound`] when no row has this identifier,
    /// and [`FolderConnectionError::Db`] on any SQLite failure.
    pub fn remove(&self, id: &str) -> Result<FolderConnection, FolderConnectionError> {
        let folder = self
            .get(id)?
            .ok_or_else(|| FolderConnectionError::NotFound(id.to_owned()))?;
        self.conn
            .execute("DELETE FROM folder_connections WHERE id = ?1", params![id])?;
        Ok(folder)
    }

    /// Whether the one-time import `key` already ran against this file.
    ///
    /// # Errors
    /// [`FolderConnectionError::Db`] on any SQLite failure.
    pub fn imported(&self, key: &str) -> Result<bool, FolderConnectionError> {
        Ok(self
            .conn
            .query_row(
                "SELECT 1 FROM settings_imports WHERE key = ?1",
                params![key],
                |_| Ok(()),
            )
            .optional()?
            .is_some())
    }

    /// Record that the one-time import `key` ran, so it never runs again.
    ///
    /// # Errors
    /// [`FolderConnectionError::Db`] on any SQLite failure.
    pub fn mark_imported(&self, key: &str) -> Result<(), FolderConnectionError> {
        self.conn.execute(
            "INSERT OR IGNORE INTO settings_imports (key) VALUES (?1)",
            params![key],
        )?;
        Ok(())
    }
}

/// The last component of `path`, or the path itself for a root.
fn folder_name(path: &str) -> String {
    Path::new(path)
        .file_name()
        .and_then(|n| n.to_str())
        .filter(|n| !n.is_empty())
        .map_or_else(|| path.to_owned(), str::to_owned)
}

fn row_to_folder(row: &rusqlite::Row<'_>) -> rusqlite::Result<FolderConnection> {
    Ok(FolderConnection {
        id: row.get(0)?,
        path: row.get(1)?,
        label: row.get(2)?,
        added_at: row.get(3)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::DataFile;

    fn store(dir: &tempfile::TempDir) -> FolderConnectionStore {
        FolderConnectionStore::open(&dir.path().join(DataFile::System.file_name())).unwrap()
    }

    #[test]
    fn test_a_folder_is_added_by_its_canonical_path_and_its_name() {
        // GIVEN a fresh system.db and a folder reached through a `..` detour
        let db = tempfile::TempDir::new().unwrap();
        let home = tempfile::TempDir::new().unwrap();
        let clients = home.path().join("Clients");
        std::fs::create_dir(&clients).unwrap();
        std::fs::create_dir(home.path().join("Other")).unwrap();
        let detour = home.path().join("Other").join("..").join("Clients");
        let store = store(&db);

        // WHEN the folder is added without a label
        let added = store.add(detour.to_str().unwrap(), None).unwrap();

        // THEN the row holds the canonical path, the folder's own name and a
        // date, and the list answers it
        let canonical = std::fs::canonicalize(&clients).unwrap();
        assert_eq!(added.path, canonical.to_str().unwrap());
        assert_eq!(added.label, "Clients");
        assert!(!added.id.is_empty());
        assert!(added.added_at.ends_with('Z'));
        assert_eq!(store.list().unwrap(), vec![added]);
    }

    #[test]
    fn test_a_given_label_is_kept() {
        // GIVEN a folder
        let db = tempfile::TempDir::new().unwrap();
        let folder = tempfile::TempDir::new().unwrap();
        let store = store(&db);

        // WHEN it is added with a label
        let added = store
            .add(folder.path().to_str().unwrap(), Some("  Contracts "))
            .unwrap();

        // THEN the label is the one given, trimmed
        assert_eq!(added.label, "Contracts");
    }

    #[test]
    fn test_the_same_folder_is_not_added_twice() {
        // GIVEN a folder already added
        let db = tempfile::TempDir::new().unwrap();
        let folder = tempfile::TempDir::new().unwrap();
        let store = store(&db);
        let path = folder.path().to_str().unwrap();
        store.add(path, None).unwrap();

        // WHEN it is added a second time
        let again = store.add(path, None);

        // THEN it is refused and the table still holds one row
        assert!(matches!(
            again,
            Err(FolderConnectionError::AlreadyAdded { .. })
        ));
        assert_eq!(store.list().unwrap().len(), 1);
    }

    #[test]
    fn test_a_file_or_a_missing_path_is_not_a_folder() {
        // GIVEN a file and a path that does not exist
        let db = tempfile::TempDir::new().unwrap();
        let dir = tempfile::TempDir::new().unwrap();
        let file = dir.path().join("notes.txt");
        std::fs::write(&file, "x").unwrap();
        let missing = dir.path().join("missing");
        let store = store(&db);

        // WHEN each is added, and an empty path too
        let results = [
            store.add(file.to_str().unwrap(), None),
            store.add(missing.to_str().unwrap(), None),
            store.add("  ", None),
        ];

        // THEN each is refused and nothing is written
        for result in results {
            assert!(matches!(
                result,
                Err(FolderConnectionError::NotAFolder { .. })
            ));
        }
        assert!(store.list().unwrap().is_empty());
    }

    #[test]
    fn test_removing_a_folder_connection_removes_its_row() {
        // GIVEN two folders added
        let db = tempfile::TempDir::new().unwrap();
        let a = tempfile::TempDir::new().unwrap();
        let b = tempfile::TempDir::new().unwrap();
        let store = store(&db);
        let first = store.add(a.path().to_str().unwrap(), None).unwrap();
        let second = store.add(b.path().to_str().unwrap(), None).unwrap();

        // WHEN the first is removed, then removed again
        let removed = store.remove(&first.id).unwrap();
        let again = store.remove(&first.id);

        // THEN the row it was is answered, only the second is left, and a
        // second removal finds nothing
        assert_eq!(removed, first);
        assert_eq!(store.list().unwrap(), vec![second]);
        assert!(matches!(again, Err(FolderConnectionError::NotFound(_))));
    }

    #[test]
    fn test_a_folder_connection_is_pointed_at_another_folder_under_the_same_identifier() {
        // GIVEN a folder connection with a label, and another folder
        let db = tempfile::TempDir::new().unwrap();
        let old = tempfile::TempDir::new().unwrap();
        let new = tempfile::TempDir::new().unwrap();
        let store = store(&db);
        let added = store
            .add(old.path().to_str().unwrap(), Some("Contracts"))
            .unwrap();

        // WHEN it is pointed at the other folder
        let moved = store
            .repoint(&added.id, new.path().to_str().unwrap())
            .unwrap();

        // THEN the row keeps its identifier, label and date, holds the new
        // canonical path, and stays the only row
        let canonical = std::fs::canonicalize(new.path()).unwrap();
        assert_eq!(moved.id, added.id);
        assert_eq!(moved.label, "Contracts");
        assert_eq!(moved.added_at, added.added_at);
        assert_eq!(moved.path, canonical.to_str().unwrap());
        assert_eq!(store.list().unwrap(), vec![moved]);
    }

    #[test]
    fn test_a_folder_connection_is_not_pointed_at_a_folder_another_one_holds() {
        // GIVEN two folder connections, and a path that is no folder
        let db = tempfile::TempDir::new().unwrap();
        let a = tempfile::TempDir::new().unwrap();
        let b = tempfile::TempDir::new().unwrap();
        let store = store(&db);
        let first = store.add(a.path().to_str().unwrap(), None).unwrap();
        let second = store.add(b.path().to_str().unwrap(), None).unwrap();
        let missing = a.path().join("missing");

        // WHEN the first is pointed at the folder of the second, at a missing
        // path, at its own folder, and an unknown identifier is pointed anywhere
        let taken = store.repoint(&first.id, b.path().to_str().unwrap());
        let not_a_folder = store.repoint(&first.id, missing.to_str().unwrap());
        let same = store.repoint(&first.id, a.path().to_str().unwrap());
        let unknown = store.repoint("nope", b.path().to_str().unwrap());

        // THEN the first two and the last are refused, its own folder is
        // accepted, and both rows are unchanged
        assert!(matches!(
            taken,
            Err(FolderConnectionError::AlreadyAdded { .. })
        ));
        assert!(matches!(
            not_a_folder,
            Err(FolderConnectionError::NotAFolder { .. })
        ));
        assert_eq!(same.unwrap(), first);
        assert!(matches!(unknown, Err(FolderConnectionError::NotFound(_))));
        let mut rows = store.list().unwrap();
        let mut expected = vec![first, second];
        rows.sort_by(|x, y| x.id.cmp(&y.id));
        expected.sort_by(|x, y| x.id.cmp(&y.id));
        assert_eq!(rows, expected);
    }

    #[test]
    fn test_the_table_is_created_by_the_versioned_schema() {
        // GIVEN a fresh system.db opened through the store
        let db = tempfile::TempDir::new().unwrap();
        drop(store(&db));

        // WHEN the file is read back
        let conn = Connection::open(db.path().join(DataFile::System.file_name())).unwrap();
        let version: i64 = conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        let columns: Vec<String> = conn
            .prepare("SELECT name FROM pragma_table_info('folder_connections')")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();

        // THEN it carries the current version and the four columns
        assert_eq!(
            version,
            i64::from(crate::system_db::SYSTEM_DB_SCHEMA_VERSION)
        );
        assert_eq!(columns, ["id", "path", "label", "added_at"]);
    }

    #[test]
    fn test_a_one_time_import_is_remembered() {
        // GIVEN a fresh system.db
        let db = tempfile::TempDir::new().unwrap();
        let store = store(&db);
        let before = store.imported("legacy_folders").unwrap();

        // WHEN the import is recorded twice, then the file is opened again
        store.mark_imported("legacy_folders").unwrap();
        store.mark_imported("legacy_folders").unwrap();
        let reopened = super::FolderConnectionStore::open(
            &db.path().join(crate::paths::DataFile::System.file_name()),
        )
        .unwrap();

        // THEN it was not done before, and is done after, on any connection
        assert!(!before);
        assert!(reopened.imported("legacy_folders").unwrap());
        assert!(!reopened.imported("another").unwrap());
    }
}
