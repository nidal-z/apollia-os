//! The `profiles` table of `user_memory.db`: several named profiles, one of
//! them the default, each made of five optional fields and a free text.
//!
//! The table is created by the versioned memory schema (version 3 of
//! [`crate::store::MemoryStore`]). The database itself guarantees a single
//! default: a partial unique index on `is_default` refuses a second row marked
//! default, so no caller can leave two defaults behind, whatever order it
//! writes in.

use rusqlite::{Connection, OptionalExtension};

use crate::profile_schema::FREE_TEXT_KEY;

use super::UserMemoryError;

/// Name given to the default profile created on first open, and to the one
/// the existing global profile is migrated into.
pub const DEFAULT_PROFILE_NAME: &str = "Mon profil";

/// One profile as stored.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Profile {
    /// Stable identifier (UUID v4).
    pub id: String,
    /// Display name of the profile (`Mon profil` for the migrated one).
    pub name: String,
    /// `true` for the one default profile.
    pub is_default: bool,
    /// Name of the person.
    pub person_name: Option<String>,
    /// Role.
    pub role: Option<String>,
    /// Function.
    pub function: Option<String>,
    /// Tools used.
    pub tools: Option<String>,
    /// Preferences.
    pub preferences: Option<String>,
    /// Free text.
    pub free_text: Option<String>,
    /// ISO 8601 creation timestamp.
    pub created_at: String,
    /// ISO 8601 last-update timestamp.
    pub updated_at: String,
}

impl Profile {
    /// Value of the field named by a schema key, `None` when the key names no
    /// field or the field is empty.
    pub fn field(&self, key: &str) -> Option<&str> {
        let value = match key {
            "name" => &self.person_name,
            "role" => &self.role,
            "function" => &self.function,
            "tools" => &self.tools,
            "preferences" => &self.preferences,
            FREE_TEXT_KEY => &self.free_text,
            _ => return None,
        };
        value.as_deref()
    }

    /// `true` when every field and the free text are empty.
    pub fn is_blank(&self) -> bool {
        crate::profile_schema::PROFILE_SCHEMA
            .iter()
            .all(|f| self.field(f.key).is_none())
    }
}

/// Column of the `profiles` table that stores the field named by `key`.
pub(crate) fn column_for(key: &str) -> Option<&'static str> {
    match key {
        "name" => Some("person_name"),
        "role" => Some("role"),
        "function" => Some("function"),
        "tools" => Some("tools"),
        "preferences" => Some("preferences"),
        FREE_TEXT_KEY => Some("free_text"),
        _ => None,
    }
}

pub(crate) fn now_utc() -> String {
    chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

fn storage(e: rusqlite::Error) -> UserMemoryError {
    UserMemoryError::StorageError(e.to_string())
}

/// Number of stored profiles.
pub(crate) fn count(conn: &Connection) -> Result<u64, UserMemoryError> {
    conn.query_row("SELECT COUNT(*) FROM profiles", [], |row| row.get(0))
        .map_err(storage)
}

const SELECT_COLUMNS: &str = "id, name, is_default, person_name, role, function, tools, \
                              preferences, free_text, created_at, updated_at";

fn from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Profile> {
    Ok(Profile {
        id: row.get(0)?,
        name: row.get(1)?,
        is_default: row.get::<_, i64>(2)? == 1,
        person_name: row.get(3)?,
        role: row.get(4)?,
        function: row.get(5)?,
        tools: row.get(6)?,
        preferences: row.get(7)?,
        free_text: row.get(8)?,
        created_at: row.get(9)?,
        updated_at: row.get(10)?,
    })
}

/// Inserts a new profile with empty fields and returns it.
pub(crate) fn insert(
    conn: &Connection,
    name: &str,
    is_default: bool,
) -> Result<Profile, UserMemoryError> {
    let id = uuid::Uuid::new_v4().to_string();
    let now = now_utc();
    conn.execute(
        "INSERT INTO profiles (id, name, is_default, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?4)",
        rusqlite::params![id, name, i64::from(is_default), now],
    )
    .map_err(storage)?;
    get(conn, &id)?.ok_or(UserMemoryError::NotFound(id))
}

/// Reads one profile by identifier.
pub(crate) fn get(conn: &Connection, id: &str) -> Result<Option<Profile>, UserMemoryError> {
    conn.query_row(
        &format!("SELECT {SELECT_COLUMNS} FROM profiles WHERE id = ?1"),
        rusqlite::params![id],
        from_row,
    )
    .optional()
    .map_err(storage)
}

/// Reads the default profile, if one exists.
pub(crate) fn default_profile(conn: &Connection) -> Result<Option<Profile>, UserMemoryError> {
    conn.query_row(
        &format!("SELECT {SELECT_COLUMNS} FROM profiles WHERE is_default = 1"),
        [],
        from_row,
    )
    .optional()
    .map_err(storage)
}

/// Lists every profile, the default first, then by creation date.
pub(crate) fn list(conn: &Connection) -> Result<Vec<Profile>, UserMemoryError> {
    let mut stmt = conn
        .prepare(&format!(
            "SELECT {SELECT_COLUMNS} FROM profiles ORDER BY is_default DESC, created_at, id"
        ))
        .map_err(storage)?;
    let rows = stmt.query_map([], from_row).map_err(storage)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(storage)
}

/// Marks `id` as the default profile and clears the mark on any other, in one
/// transaction.
pub(crate) fn set_default(conn: &Connection, id: &str) -> Result<(), UserMemoryError> {
    let tx = conn.unchecked_transaction().map_err(storage)?;
    let now = now_utc();
    tx.execute(
        "UPDATE profiles SET is_default = 0, updated_at = ?1 WHERE is_default = 1 AND id <> ?2",
        rusqlite::params![now, id],
    )
    .map_err(storage)?;
    let changed = tx
        .execute(
            "UPDATE profiles SET is_default = 1, updated_at = ?1 WHERE id = ?2",
            rusqlite::params![now, id],
        )
        .map_err(storage)?;
    if changed == 0 {
        return Err(UserMemoryError::NotFound(id.to_owned()));
    }
    tx.commit().map_err(storage)
}

/// Writes one field of a profile; `None` empties it.
pub(crate) fn set_field(
    conn: &Connection,
    id: &str,
    key: &str,
    value: Option<&str>,
) -> Result<(), UserMemoryError> {
    let column = column_for(key).ok_or_else(|| UserMemoryError::InvalidKey(key.to_owned()))?;
    let changed = conn
        .execute(
            &format!("UPDATE profiles SET {column} = ?1, updated_at = ?2 WHERE id = ?3"),
            rusqlite::params![value, now_utc(), id],
        )
        .map_err(storage)?;
    if changed == 0 {
        return Err(UserMemoryError::NotFound(id.to_owned()));
    }
    Ok(())
}
