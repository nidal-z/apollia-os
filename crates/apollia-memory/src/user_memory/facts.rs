//! The facts retained about the user, one set per profile.
//!
//! A fact is a free-form entry: a key outside the profile schema and its
//! value, written by an agent or by the user. Each profile owns its facts.
//! Those of the default profile live in the `__user__` namespace itself, so
//! every reader and writer that names no profile (the persona brief, the
//! Memory screen, `remember_user`) keeps meaning "the default profile". The
//! facts of any other profile live in `__user__@<profile id>`.
//!
//! Making another profile the default swaps the two sets in the same
//! transaction as the default mark: the outgoing default's facts are parked in
//! its own namespace, the incoming one's move into `__user__`. Deleting a
//! profile deletes its facts. The onboarding markers (`__`-prefixed keys) stay
//! in `__user__` whatever the default.

use rusqlite::Connection;

use super::{Profile, UserMemoryError, USER_NAMESPACE};

/// Separator between `__user__` and a profile identifier in the namespace of
/// a profile that is not the default.
const PROFILE_NAMESPACE_SEPARATOR: char = '@';

fn storage(e: rusqlite::Error) -> UserMemoryError {
    UserMemoryError::StorageError(e.to_string())
}

/// Namespace of the profile `id` while it is not the default.
pub(crate) fn parked_namespace(id: &str) -> String {
    format!("{USER_NAMESPACE}{PROFILE_NAMESPACE_SEPARATOR}{id}")
}

/// Namespace that holds the facts of `profile` now.
pub(crate) fn namespace_of(profile: &Profile) -> String {
    if profile.is_default {
        USER_NAMESPACE.to_owned()
    } else {
        parked_namespace(&profile.id)
    }
}

/// Deletes every fact of `namespace`, with its full-text rows, and returns
/// how many were removed. The `__`-prefixed markers are kept.
pub(crate) fn drop_facts(conn: &Connection, namespace: &str) -> Result<usize, UserMemoryError> {
    conn.execute(
        "DELETE FROM memory_fts WHERE source_table = 'semantic' AND source_id IN (
             SELECT id FROM semantic_memories
             WHERE namespace = ?1 AND substr(key, 1, 2) <> '__')",
        rusqlite::params![namespace],
    )
    .map_err(storage)?;
    conn.execute(
        "DELETE FROM semantic_memories WHERE namespace = ?1 AND substr(key, 1, 2) <> '__'",
        rusqlite::params![namespace],
    )
    .map_err(storage)
}

/// Moves the facts of the default profile `outgoing` to its own namespace and
/// those of `incoming` into `__user__`. Runs on the connection of the caller's
/// transaction; `outgoing` equal to `incoming` moves nothing.
pub(crate) fn swap_default(
    conn: &Connection,
    outgoing: Option<&str>,
    incoming: &str,
) -> Result<(), UserMemoryError> {
    if outgoing == Some(incoming) {
        return Ok(());
    }
    if let Some(outgoing) = outgoing {
        let parked = parked_namespace(outgoing);
        drop_facts(conn, &parked)?;
        conn.execute(
            "UPDATE semantic_memories SET namespace = ?1
             WHERE namespace = ?2 AND substr(key, 1, 2) <> '__'",
            rusqlite::params![parked, USER_NAMESPACE],
        )
        .map_err(storage)?;
    }
    conn.execute(
        "UPDATE semantic_memories SET namespace = ?1 WHERE namespace = ?2",
        rusqlite::params![USER_NAMESPACE, parked_namespace(incoming)],
    )
    .map_err(storage)?;
    Ok(())
}

impl super::UserMemoryRepository {
    /// Deletes a single entry of `profile` by key: a schema key empties its
    /// field, any other key deletes the fact.  Returns
    /// [`UserMemoryError::NotFound`] when the key held nothing.
    pub fn forget_in(&self, profile: &Profile, key: &str) -> Result<(), UserMemoryError> {
        Self::validate_external_key(key)?;
        if crate::profile_schema::is_canonical(key) {
            if profile.field(key).is_none() {
                return Err(UserMemoryError::NotFound(key.to_owned()));
            }
            return super::profiles::set_field(self.store.conn(), &profile.id, key, None);
        }
        let deleted = crate::semantic::SemanticMemory::new(&self.store)
            .forget(&namespace_of(profile), key)
            .map_err(|e| UserMemoryError::StorageError(e.to_string()))?;
        if deleted {
            Ok(())
        } else {
            Err(UserMemoryError::NotFound(key.to_owned()))
        }
    }

    /// Empties every field and the free text of `profile` and deletes its
    /// facts, in one transaction.  The onboarding markers are preserved.
    /// Returns the number of removed entries.
    pub fn reset_in(&self, profile: &Profile) -> Result<usize, UserMemoryError> {
        let conn = self.store.conn();
        let tx = conn.unchecked_transaction().map_err(storage)?;
        let mut count = 0usize;
        for field in crate::profile_schema::PROFILE_SCHEMA {
            if profile.field(field.key).is_some() {
                super::profiles::set_field(&tx, &profile.id, field.key, None)?;
                count += 1;
            }
        }
        count += drop_facts(&tx, &namespace_of(profile))?;
        tx.commit().map_err(storage)?;
        Ok(count)
    }
}
