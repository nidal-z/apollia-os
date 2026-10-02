//! The keyring of trusted publisher keys, in `agents.db`.
//!
//! A signed package carries the public key that signed it; the signature says
//! the package was not altered since, not who the author is. The user says
//! that by importing the author's public key here, under a label of their
//! choosing. A publisher whose key is in the keyring is shown as trusted, by
//! that label; any other signed package shows the fingerprint of its key and
//! the name it declares, which nothing verifies.
//!
//! Each key records who put it there ([`TrustedKeyOrigin`]): the user, or the
//! deposited policy. Only the user's keys are written by this crate today.

use std::path::Path;
use std::sync::Mutex;

use apollia_signing::PublicKey;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

/// Who put a key in the keyring.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrustedKeyOrigin {
    /// Imported by the user.
    User,
    /// Set by the deposited policy.
    Policy,
}

impl TrustedKeyOrigin {
    /// Stored form.
    pub fn as_str(self) -> &'static str {
        match self {
            TrustedKeyOrigin::User => "user",
            TrustedKeyOrigin::Policy => "policy",
        }
    }

    fn parse(raw: &str) -> Self {
        match raw {
            "policy" => TrustedKeyOrigin::Policy,
            _ => TrustedKeyOrigin::User,
        }
    }
}

/// One trusted publisher key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrustedKey {
    /// SHA-256 fingerprint of the key, 64 lowercase hexadecimal characters.
    pub fingerprint: String,
    /// The public key, 64 lowercase hexadecimal characters.
    pub public_key: String,
    /// Name the user gave the publisher.
    pub label: String,
    /// Who put the key here.
    pub origin: TrustedKeyOrigin,
    /// When the key was added.
    pub added_at: String,
}

/// Errors of the keyring.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum TrustedKeyError {
    /// SQLite failure.
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    /// The database schema could not be brought to the supported version.
    #[error(transparent)]
    Schema(#[from] apollia_core::schema::SchemaError),
    /// The text is not an Ed25519 public key.
    #[error("not a publisher public key: {0}")]
    InvalidKey(#[from] apollia_signing::SigningError),
    /// The label is empty.
    #[error("a trusted key needs a non-empty label")]
    EmptyLabel,
}

/// SQLite repository of the trusted publisher keys.
pub struct TrustedKeyRepository {
    conn: Mutex<Connection>,
}

impl TrustedKeyRepository {
    /// Opens (or creates) `agents.db` and brings it to the current schema.
    ///
    /// # Errors
    ///
    /// [`TrustedKeyError::Sqlite`] or [`TrustedKeyError::Schema`].
    pub fn open(path: &Path) -> Result<Self, TrustedKeyError> {
        let conn = Connection::open(path)?;
        conn.execute_batch("PRAGMA journal_mode=WAL;")?;
        crate::agents_db::open_agents_schema(&conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// Adds the key written as `public_key_hex` under `label`, and returns it.
    ///
    /// Importing a key already in the keyring replaces its label and origin.
    ///
    /// # Errors
    ///
    /// [`TrustedKeyError::InvalidKey`] when the text is not a public key,
    /// [`TrustedKeyError::EmptyLabel`], or [`TrustedKeyError::Sqlite`].
    pub fn import(
        &self,
        public_key_hex: &str,
        label: &str,
        origin: TrustedKeyOrigin,
    ) -> Result<TrustedKey, TrustedKeyError> {
        let key = PublicKey::from_hex(public_key_hex)?;
        let label = label.trim();
        if label.is_empty() {
            return Err(TrustedKeyError::EmptyLabel);
        }
        let fingerprint = key.fingerprint();
        let conn = self.conn.lock().unwrap_or_else(|p| p.into_inner());
        conn.execute(
            "INSERT INTO trusted_keys (fingerprint, public_key, label, origin) \
             VALUES (?1, ?2, ?3, ?4) \
             ON CONFLICT(fingerprint) DO UPDATE SET \
                 label = excluded.label, origin = excluded.origin",
            params![&fingerprint, key.to_hex(), label, origin.as_str()],
        )?;
        let stored = get_in(&conn, &fingerprint)?;
        stored.ok_or(TrustedKeyError::Sqlite(
            rusqlite::Error::QueryReturnedNoRows,
        ))
    }

    /// Every trusted key, by label then fingerprint.
    ///
    /// # Errors
    ///
    /// [`TrustedKeyError::Sqlite`].
    pub fn list(&self) -> Result<Vec<TrustedKey>, TrustedKeyError> {
        let conn = self.conn.lock().unwrap_or_else(|p| p.into_inner());
        let mut stmt = conn.prepare(
            "SELECT fingerprint, public_key, label, origin, added_at FROM trusted_keys \
             ORDER BY label, fingerprint",
        )?;
        let rows = stmt.query_map([], read_row)?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// The trusted key of fingerprint `fingerprint`, if there is one.
    ///
    /// # Errors
    ///
    /// [`TrustedKeyError::Sqlite`].
    pub fn get(&self, fingerprint: &str) -> Result<Option<TrustedKey>, TrustedKeyError> {
        let conn = self.conn.lock().unwrap_or_else(|p| p.into_inner());
        Ok(get_in(&conn, &fingerprint.to_ascii_lowercase())?)
    }

    /// Removes the key of fingerprint `fingerprint`; `false` when there was
    /// none.
    ///
    /// # Errors
    ///
    /// [`TrustedKeyError::Sqlite`].
    pub fn remove(&self, fingerprint: &str) -> Result<bool, TrustedKeyError> {
        let conn = self.conn.lock().unwrap_or_else(|p| p.into_inner());
        let removed = conn.execute(
            "DELETE FROM trusted_keys WHERE fingerprint = ?1",
            params![fingerprint.to_ascii_lowercase()],
        )?;
        Ok(removed > 0)
    }
}

fn get_in(conn: &Connection, fingerprint: &str) -> Result<Option<TrustedKey>, rusqlite::Error> {
    conn.query_row(
        "SELECT fingerprint, public_key, label, origin, added_at FROM trusted_keys \
         WHERE fingerprint = ?1",
        params![fingerprint],
        read_row,
    )
    .optional()
}

fn read_row(row: &rusqlite::Row<'_>) -> Result<TrustedKey, rusqlite::Error> {
    Ok(TrustedKey {
        fingerprint: row.get(0)?,
        public_key: row.get(1)?,
        label: row.get(2)?,
        origin: TrustedKeyOrigin::parse(&row.get::<_, String>(3)?),
        added_at: row.get(4)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use apollia_signing::SigningKeyPair;
    use tempfile::TempDir;

    fn repo() -> (TempDir, TrustedKeyRepository) {
        let dir = TempDir::new().unwrap();
        let repo = TrustedKeyRepository::open(&dir.path().join("agents.db")).unwrap();
        (dir, repo)
    }

    #[test]
    fn test_import_list_remove_a_publisher_key() {
        // GIVEN an empty keyring and a publisher key written as hex
        let (_dir, repo) = repo();
        let key = SigningKeyPair::from_seed(&[1u8; 32]).public_key();

        // WHEN the key is imported, listed, then removed
        let imported = repo
            .import(
                &format!("{}\n", key.to_hex()),
                " Acme ",
                TrustedKeyOrigin::User,
            )
            .unwrap();
        let listed = repo.list().unwrap();
        let removed = repo.remove(&imported.fingerprint).unwrap();
        let removed_again = repo.remove(&imported.fingerprint).unwrap();

        // THEN the key is stored under its fingerprint and trimmed label, and
        //   removing it twice removes it once
        assert_eq!(imported.fingerprint, key.fingerprint());
        assert_eq!(imported.public_key, key.to_hex());
        assert_eq!(imported.label, "Acme");
        assert_eq!(imported.origin, TrustedKeyOrigin::User);
        assert_eq!(listed, vec![imported]);
        assert!(removed);
        assert!(!removed_again);
        assert!(repo.list().unwrap().is_empty());
    }

    #[test]
    fn test_reimport_replaces_the_label() {
        // GIVEN a key imported under one label
        let (_dir, repo) = repo();
        let key = SigningKeyPair::from_seed(&[2u8; 32]).public_key();
        repo.import(&key.to_hex(), "Old", TrustedKeyOrigin::User)
            .unwrap();

        // WHEN it is imported again under another
        repo.import(&key.to_hex(), "New", TrustedKeyOrigin::User)
            .unwrap();

        // THEN one key remains, with the new label
        let keys = repo.list().unwrap();
        assert_eq!(keys.len(), 1);
        assert_eq!(keys[0].label, "New");
        assert_eq!(
            repo.get(&key.fingerprint()).unwrap().map(|k| k.label),
            Some("New".to_owned())
        );
    }

    #[test]
    fn test_import_refuses_a_non_key_and_an_empty_label() {
        // GIVEN a text that is no key, and a key with a blank label
        let (_dir, repo) = repo();
        let key = SigningKeyPair::from_seed(&[3u8; 32]).public_key();

        // WHEN each is imported
        let not_a_key = repo.import("not a key", "Acme", TrustedKeyOrigin::User);
        let blank = repo.import(&key.to_hex(), "  ", TrustedKeyOrigin::User);

        // THEN both are refused and nothing is stored
        assert!(matches!(not_a_key, Err(TrustedKeyError::InvalidKey(_))));
        assert!(matches!(blank, Err(TrustedKeyError::EmptyLabel)));
        assert!(repo.list().unwrap().is_empty());
    }
}
