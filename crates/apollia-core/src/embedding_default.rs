//! The default embeddings model, persisted in `system.db`.
//!
//! One setting: the identifier of the embeddings entry of the registry that
//! search by meaning loads when it needs a vector. Unset means search falls
//! back to keywords alone, never that a model is picked on the user's behalf.
//! The table is created by `system_db::migrate_v5`.

use std::path::Path;

use rusqlite::{params, Connection, OptionalExtension};
use thiserror::Error;

/// Errors from [`EmbeddingDefaultStore`].
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum EmbeddingDefaultError {
    /// Underlying SQLite error.
    #[error("database error: {0}")]
    Db(#[from] rusqlite::Error),

    /// The database schema could not be brought to the supported version.
    #[error(transparent)]
    Schema(#[from] crate::schema::SchemaError),

    /// The identifier given is empty once trimmed.
    #[error("the embeddings model identifier is empty")]
    EmptyId,
}

/// The default embeddings model of `system.db`.
pub struct EmbeddingDefaultStore {
    conn: Connection,
}

impl EmbeddingDefaultStore {
    /// Open (or create) `system.db` at `path` and bring it to the current
    /// schema version.
    ///
    /// # Errors
    /// [`EmbeddingDefaultError::Db`] if the file cannot be opened, and
    /// [`EmbeddingDefaultError::Schema`] if the migration fails or the file
    /// was written by a newer binary.
    pub fn open(path: &Path) -> Result<Self, EmbeddingDefaultError> {
        let conn = Connection::open(path)?;
        crate::schema::open_versioned(
            &conn,
            crate::paths::DataFile::System.file_name(),
            crate::system_db::SYSTEM_DB_SCHEMA_VERSION,
            crate::system_db::SYSTEM_DB_MIGRATIONS,
        )?;
        Ok(Self { conn })
    }

    /// The identifier of the default embeddings model, `None` when unset.
    ///
    /// # Errors
    /// [`EmbeddingDefaultError::Db`] on any SQLite failure.
    pub fn get(&self) -> Result<Option<String>, EmbeddingDefaultError> {
        Ok(self
            .conn
            .query_row(
                "SELECT model_id FROM embedding_default WHERE id = 1",
                [],
                |row| row.get(0),
            )
            .optional()?)
    }

    /// Make `model_id` the default embeddings model, replacing any previous one.
    ///
    /// The identifier is stored trimmed. Whether it names an entry of the
    /// registry is the caller's to check: this store keeps a setting, it does
    /// not read the registry.
    ///
    /// # Errors
    /// [`EmbeddingDefaultError::EmptyId`] for a blank identifier, and
    /// [`EmbeddingDefaultError::Db`] on any SQLite failure.
    pub fn set(&self, model_id: &str) -> Result<(), EmbeddingDefaultError> {
        let model_id = model_id.trim();
        if model_id.is_empty() {
            return Err(EmbeddingDefaultError::EmptyId);
        }
        self.conn.execute(
            "INSERT INTO embedding_default (id, model_id, updated_at)
             VALUES (1, ?1, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
             ON CONFLICT(id) DO UPDATE SET
                model_id   = excluded.model_id,
                updated_at = excluded.updated_at",
            params![model_id],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_a_fresh_file_has_no_default_embeddings_model() {
        // GIVEN a system.db that has just been created
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("system.db");

        // WHEN the default embeddings model is read
        let store = EmbeddingDefaultStore::open(&path).unwrap();

        // THEN there is none
        assert_eq!(store.get().unwrap(), None);
    }

    #[test]
    fn test_the_default_survives_a_reopen_and_the_last_write_wins() {
        // GIVEN a default set, then replaced
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("system.db");
        {
            let store = EmbeddingDefaultStore::open(&path).unwrap();
            store.set("multilingual-e5-small").unwrap();
            store.set("  qwen3-embedding-0.6b ").unwrap();
        }

        // WHEN the file is opened again
        let store = EmbeddingDefaultStore::open(&path).unwrap();

        // THEN the last identifier is the one read, trimmed
        assert_eq!(
            store.get().unwrap().as_deref(),
            Some("qwen3-embedding-0.6b")
        );
    }

    #[test]
    fn test_a_blank_identifier_is_refused_and_keeps_the_default() {
        // GIVEN a default set
        let dir = tempfile::TempDir::new().unwrap();
        let store = EmbeddingDefaultStore::open(&dir.path().join("system.db")).unwrap();
        store.set("multilingual-e5-small").unwrap();

        // WHEN a blank identifier is written
        let refused = store.set("   ");

        // THEN it is refused and the default is unchanged
        assert!(matches!(refused, Err(EmbeddingDefaultError::EmptyId)));
        assert_eq!(
            store.get().unwrap().as_deref(),
            Some("multilingual-e5-small")
        );
    }

    #[test]
    fn test_a_file_at_version_four_gains_the_table_and_keeps_its_rows() {
        // GIVEN a system.db written by a binary at schema version four, with a
        // backend row
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("system.db");
        {
            let conn = Connection::open(&path).unwrap();
            crate::system_db::migrate_v1(&conn).unwrap();
            crate::system_db::migrate_v2(&conn).unwrap();
            crate::system_db::migrate_v3(&conn).unwrap();
            crate::system_db::migrate_v4(&conn).unwrap();
            conn.pragma_update(None, "user_version", 4).unwrap();
            conn.execute(
                "INSERT INTO llm_backends (name, provider, model) VALUES ('local', 'llama-cpp', 'm')",
                [],
            )
            .unwrap();
        }

        // WHEN the store opens it
        let store = EmbeddingDefaultStore::open(&path).unwrap();

        // THEN the file is at the current version, the table answers, and the row is kept
        assert_eq!(store.get().unwrap(), None);
        let version: i64 = store
            .conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(
            version,
            i64::from(crate::system_db::SYSTEM_DB_SCHEMA_VERSION)
        );
        let backends: i64 = store
            .conn
            .query_row("SELECT count(*) FROM llm_backends", [], |row| row.get(0))
            .unwrap();
        assert_eq!(backends, 1);
    }
}
