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
//! deposited policy. The user's keys are rows of the table. The policy's are
//! not stored: the policy names its trusted publishers by key fingerprint, and
//! [`TrustedKeyRepository::with_policy_publishers`] lays that list over the
//! table, read only. A key the policy names cannot be removed, and while the
//! list is not empty it is the only one trusted: a key the user imported and
//! the policy does not name is listed, but no longer trusted.

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
    /// The public key, 64 lowercase hexadecimal characters; empty for a key
    /// the policy names by its fingerprint and the user never imported.
    pub public_key: String,
    /// Name the user gave the publisher; empty for a key only the policy
    /// names, which gives it none.
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
    /// The key is trusted by the organisation policy; only the policy removes
    /// it.
    #[error("key {fingerprint} is trusted by the organisation policy and cannot be removed")]
    HeldByPolicy {
        /// Fingerprint of the key.
        fingerprint: String,
    },
}

/// SQLite repository of the trusted publisher keys.
pub struct TrustedKeyRepository {
    conn: Mutex<Connection>,
    /// Fingerprints of the publishers the organisation policy trusts, lowercase.
    policy_publishers: Vec<String>,
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
            policy_publishers: Vec::new(),
        })
    }

    /// Lays the trusted publishers of the organisation policy, by key
    /// fingerprint, over the keyring.
    ///
    /// Each one reads as a key of origin [`TrustedKeyOrigin::Policy`] that
    /// [`remove`](Self::remove) refuses. A non-empty list is the whole of the
    /// trust: [`get`](Self::get) answers only for its fingerprints. An empty
    /// list changes nothing.
    #[must_use]
    pub fn with_policy_publishers(mut self, fingerprints: &[String]) -> Self {
        self.policy_publishers = fingerprints
            .iter()
            .map(|f| f.to_ascii_lowercase())
            .collect();
        self
    }

    /// Whether the organisation policy names the publishers trusted, so that
    /// a key outside its list is not.
    pub fn restricted_to_policy(&self) -> bool {
        !self.policy_publishers.is_empty()
    }

    fn held_by_policy(&self, fingerprint: &str) -> bool {
        self.policy_publishers.iter().any(|f| f == fingerprint)
    }

    /// `key` as the policy shows it: of origin policy when the policy names it.
    fn with_policy_origin(&self, mut key: TrustedKey) -> TrustedKey {
        if self.held_by_policy(&key.fingerprint) {
            key.origin = TrustedKeyOrigin::Policy;
        }
        key
    }

    /// A key the policy names and the table does not hold.
    fn policy_only(fingerprint: &str) -> TrustedKey {
        TrustedKey {
            fingerprint: fingerprint.to_owned(),
            public_key: String::new(),
            label: String::new(),
            origin: TrustedKeyOrigin::Policy,
            added_at: String::new(),
        }
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
        let stored: Vec<TrustedKey> = rows.collect::<Result<_, _>>()?;
        let mut keys: Vec<TrustedKey> = self
            .policy_publishers
            .iter()
            .filter(|f| !stored.iter().any(|key| &key.fingerprint == *f))
            .map(|f| Self::policy_only(f))
            .collect();
        keys.extend(stored.into_iter().map(|key| self.with_policy_origin(key)));
        keys.sort_by(|a, b| (&a.label, &a.fingerprint).cmp(&(&b.label, &b.fingerprint)));
        Ok(keys)
    }

    /// The trusted key of fingerprint `fingerprint`, if it is trusted.
    ///
    /// While the policy names trusted publishers, only their keys are: a key
    /// the user imported and the policy does not name answers `None`.
    ///
    /// # Errors
    ///
    /// [`TrustedKeyError::Sqlite`].
    pub fn get(&self, fingerprint: &str) -> Result<Option<TrustedKey>, TrustedKeyError> {
        let fingerprint = fingerprint.to_ascii_lowercase();
        if self.restricted_to_policy() && !self.held_by_policy(&fingerprint) {
            return Ok(None);
        }
        let conn = self.conn.lock().unwrap_or_else(|p| p.into_inner());
        let stored = get_in(&conn, &fingerprint)?;
        Ok(match stored {
            Some(key) => Some(self.with_policy_origin(key)),
            None if self.held_by_policy(&fingerprint) => Some(Self::policy_only(&fingerprint)),
            None => None,
        })
    }

    /// Removes the key of fingerprint `fingerprint`; `false` when there was
    /// none.
    ///
    /// # Errors
    ///
    /// [`TrustedKeyError::HeldByPolicy`] when the policy names the key, which
    /// stays; [`TrustedKeyError::Sqlite`].
    pub fn remove(&self, fingerprint: &str) -> Result<bool, TrustedKeyError> {
        let fingerprint = fingerprint.to_ascii_lowercase();
        if self.held_by_policy(&fingerprint) {
            return Err(TrustedKeyError::HeldByPolicy { fingerprint });
        }
        let conn = self.conn.lock().unwrap_or_else(|p| p.into_inner());
        let removed = conn.execute(
            "DELETE FROM trusted_keys WHERE fingerprint = ?1",
            params![fingerprint],
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
    fn test_policy_publishers_are_listed_read_only_and_are_the_only_trust() {
        // GIVEN a keyring where the user imported two keys, one of which the
        //   policy also names, and a policy naming a third key never imported
        let (_dir, repo) = repo();
        let named = SigningKeyPair::from_seed(&[4u8; 32]).public_key();
        let unnamed = SigningKeyPair::from_seed(&[5u8; 32]).public_key();
        let only_policy = SigningKeyPair::from_seed(&[6u8; 32]).public_key();
        repo.import(&named.to_hex(), "Acme", TrustedKeyOrigin::User)
            .unwrap();
        repo.import(&unnamed.to_hex(), "Other", TrustedKeyOrigin::User)
            .unwrap();
        let repo = repo.with_policy_publishers(&[
            named.fingerprint().to_ascii_uppercase(),
            only_policy.fingerprint(),
        ]);

        // WHEN the keyring is listed, each key looked up, and the policy's
        //   keys removed
        let listed = repo.list().unwrap();
        let [named_trust, unnamed_trust, policy_trust] =
            [&named, &unnamed, &only_policy].map(|k| repo.get(&k.fingerprint()).unwrap());
        let removals = [&named, &only_policy].map(|k| repo.remove(&k.fingerprint()));

        // THEN the three keys are listed, the policy's with origin policy; only
        //   the policy's are trusted; neither of them can be removed
        assert!(repo.restricted_to_policy());
        assert_eq!(listed.len(), 3);
        let origin_of = |key: &PublicKey| {
            listed
                .iter()
                .find(|k| k.fingerprint == key.fingerprint())
                .map(|k| k.origin)
        };
        assert_eq!(origin_of(&named), Some(TrustedKeyOrigin::Policy));
        assert_eq!(origin_of(&only_policy), Some(TrustedKeyOrigin::Policy));
        assert_eq!(origin_of(&unnamed), Some(TrustedKeyOrigin::User));
        assert_eq!(named_trust.map(|k| k.label), Some("Acme".to_owned()));
        assert_eq!(unnamed_trust, None);
        assert_eq!(
            policy_trust.map(|k| k.origin),
            Some(TrustedKeyOrigin::Policy)
        );
        for removal in removals {
            assert!(matches!(removal, Err(TrustedKeyError::HeldByPolicy { .. })));
        }
        assert_eq!(repo.list().unwrap().len(), 3);
    }

    #[test]
    fn test_an_empty_policy_list_leaves_the_keyring_as_the_user_made_it() {
        // GIVEN a key the user imported, and a policy naming no publisher
        let (_dir, repo) = repo();
        let key = SigningKeyPair::from_seed(&[7u8; 32]).public_key();
        repo.import(&key.to_hex(), "Acme", TrustedKeyOrigin::User)
            .unwrap();
        let repo = repo.with_policy_publishers(&[]);

        // WHEN the key is looked up, then removed
        let trusted = repo.get(&key.fingerprint()).unwrap();
        let removed = repo.remove(&key.fingerprint()).unwrap();

        // THEN it was trusted as the user's, and the user removed it
        assert!(!repo.restricted_to_policy());
        assert_eq!(trusted.map(|k| k.origin), Some(TrustedKeyOrigin::User));
        assert!(removed);
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
