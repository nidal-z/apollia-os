//! Tokens of the compatible server, persisted in `system.db`.
//!
//! A token is what another application of the machine presents to the
//! compatible server. Each one is named after the application that holds it,
//! so it can be revoked without touching the others. The value is shown once,
//! when it is created, and never stored: the table keeps its SHA-256 (hex) and
//! its last four characters. A hash is not a secret, so the table needs no
//! secret store.
//!
//! This store does not draw, hash or compare values: the runtime does, and
//! hands it the hash. It never sees a token value.

use std::path::Path;

use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;

/// Longest token name, in characters.
pub const MAX_SERVER_TOKEN_NAME_CHARS: usize = 64;

/// Errors of the token store.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ServerTokenError {
    /// Underlying SQLite error.
    #[error("database error: {0}")]
    Db(#[from] rusqlite::Error),
    /// The database schema could not be brought to the supported version.
    #[error(transparent)]
    Schema(#[from] crate::schema::SchemaError),
    /// The name is empty, too long, or made of digits only (a name made of
    /// digits would read as an identifier).
    #[error("invalid token name: {reason}")]
    InvalidName {
        /// What the name breaks.
        reason: &'static str,
    },
    /// A token in use already carries this name.
    #[error("a token named '{name}' already exists")]
    NameTaken {
        /// The name asked for.
        name: String,
    },
    /// No token matches the identifier or the name.
    #[error("no token matches '{selector}'")]
    NotFound {
        /// The identifier or name looked for.
        selector: String,
    },
}

/// One token as the table keeps it: never its value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ServerTokenRow {
    /// Identifier, stable for the life of the row.
    pub id: i64,
    /// Name given at creation.
    pub name: String,
    /// The last four characters of the value.
    pub last_four: String,
    /// When it was created, RFC 3339.
    pub created_at: String,
    /// When it last opened the server, RFC 3339; `None` if never.
    pub last_used_at: Option<String>,
    /// When it was revoked, RFC 3339; `None` while it is in use.
    pub revoked_at: Option<String>,
}

/// Which token a revocation names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenSelector {
    /// By identifier.
    Id(i64),
    /// By name, the token in use that carries it.
    Name(String),
}

impl TokenSelector {
    /// Reads a path segment: digits name an identifier, anything else a name.
    ///
    /// A name made of digits only is refused at creation, so the two never
    /// collide.
    pub fn parse(raw: &str) -> Self {
        match raw.parse::<i64>() {
            Ok(id) if raw.bytes().all(|b| b.is_ascii_digit()) => Self::Id(id),
            _ => Self::Name(raw.to_owned()),
        }
    }

    fn describe(&self) -> String {
        match self {
            Self::Id(id) => id.to_string(),
            Self::Name(name) => name.clone(),
        }
    }
}

/// Checks a token name and returns it trimmed.
///
/// # Errors
///
/// [`ServerTokenError::InvalidName`] when the name is empty once trimmed,
/// longer than [`MAX_SERVER_TOKEN_NAME_CHARS`], holds a control character, or
/// is made of digits only.
pub fn validate_token_name(name: &str) -> Result<&str, ServerTokenError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(ServerTokenError::InvalidName {
            reason: "the name is empty",
        });
    }
    if name.chars().count() > MAX_SERVER_TOKEN_NAME_CHARS {
        return Err(ServerTokenError::InvalidName {
            reason: "the name is longer than 64 characters",
        });
    }
    if name.chars().any(char::is_control) {
        return Err(ServerTokenError::InvalidName {
            reason: "the name holds a control character",
        });
    }
    if name.bytes().all(|b| b.is_ascii_digit()) {
        return Err(ServerTokenError::InvalidName {
            reason: "the name is made of digits only, which reads as an identifier",
        });
    }
    Ok(name)
}

const COLUMNS: &str = "id, name, last_four, created_at, last_used_at, revoked_at";

fn row_of(row: &rusqlite::Row<'_>) -> rusqlite::Result<ServerTokenRow> {
    Ok(ServerTokenRow {
        id: row.get(0)?,
        name: row.get(1)?,
        last_four: row.get(2)?,
        created_at: row.get(3)?,
        last_used_at: row.get(4)?,
        revoked_at: row.get(5)?,
    })
}

/// The token table of `system.db`.
///
/// **Thread safety:** `rusqlite::Connection` is `Send` but not `Sync`; one
/// actor owns the repository.
pub struct ServerTokenRepository {
    conn: Connection,
}

impl ServerTokenRepository {
    /// Opens (or creates) `system.db` at `path` and brings it to the current
    /// schema.
    ///
    /// # Errors
    ///
    /// [`ServerTokenError::Db`] when the file cannot be opened,
    /// [`ServerTokenError::Schema`] when the migration fails or the file was
    /// written by a newer binary.
    pub fn open(path: &Path) -> Result<Self, ServerTokenError> {
        let conn = Connection::open(path)?;
        crate::schema::open_versioned(
            &conn,
            crate::paths::DataFile::System.file_name(),
            crate::system_db::SYSTEM_DB_SCHEMA_VERSION,
            crate::system_db::SYSTEM_DB_MIGRATIONS,
        )?;
        Ok(Self { conn })
    }

    /// Records a new token from its name, the hex SHA-256 of its value and
    /// the last four characters of its value.
    ///
    /// # Errors
    ///
    /// [`ServerTokenError::InvalidName`] for a name [`validate_token_name`]
    /// refuses, [`ServerTokenError::NameTaken`] when a token in use carries
    /// it, [`ServerTokenError::Db`] otherwise.
    pub fn insert(
        &self,
        name: &str,
        hash_hex: &str,
        last_four: &str,
    ) -> Result<ServerTokenRow, ServerTokenError> {
        let name = validate_token_name(name)?;
        let taken: Option<i64> = self
            .conn
            .query_row(
                "SELECT id FROM server_tokens WHERE name = ?1 AND revoked_at IS NULL",
                params![name],
                |row| row.get(0),
            )
            .optional()?;
        if taken.is_some() {
            return Err(ServerTokenError::NameTaken {
                name: name.to_owned(),
            });
        }
        self.conn.execute(
            "INSERT INTO server_tokens (name, hash, last_four) VALUES (?1, ?2, ?3)",
            params![name, hash_hex, last_four],
        )?;
        let id = self.conn.last_insert_rowid();
        self.get(id)
    }

    fn get(&self, id: i64) -> Result<ServerTokenRow, ServerTokenError> {
        self.conn
            .query_row(
                &format!("SELECT {COLUMNS} FROM server_tokens WHERE id = ?1"),
                params![id],
                row_of,
            )
            .optional()?
            .ok_or_else(|| ServerTokenError::NotFound {
                selector: id.to_string(),
            })
    }

    /// Every token, in use and revoked, oldest first.
    ///
    /// # Errors
    ///
    /// [`ServerTokenError::Db`] on any SQLite failure.
    pub fn list(&self) -> Result<Vec<ServerTokenRow>, ServerTokenError> {
        let mut stmt = self
            .conn
            .prepare(&format!("SELECT {COLUMNS} FROM server_tokens ORDER BY id"))?;
        let rows = stmt.query_map([], row_of)?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Revokes the token `selector` names and returns it. Revoking a revoked
    /// token by identifier changes nothing; a name only names a token in use.
    ///
    /// # Errors
    ///
    /// [`ServerTokenError::NotFound`] when nothing matches,
    /// [`ServerTokenError::Db`] otherwise.
    pub fn revoke(&self, selector: &TokenSelector) -> Result<ServerTokenRow, ServerTokenError> {
        let id: Option<i64> = match selector {
            TokenSelector::Id(id) => self
                .conn
                .query_row(
                    "SELECT id FROM server_tokens WHERE id = ?1",
                    params![id],
                    |row| row.get(0),
                )
                .optional()?,
            TokenSelector::Name(name) => self
                .conn
                .query_row(
                    "SELECT id FROM server_tokens WHERE name = ?1 AND revoked_at IS NULL",
                    params![name.trim()],
                    |row| row.get(0),
                )
                .optional()?,
        };
        let id = id.ok_or_else(|| ServerTokenError::NotFound {
            selector: selector.describe(),
        })?;
        self.conn.execute(
            "UPDATE server_tokens
             SET revoked_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
             WHERE id = ?1 AND revoked_at IS NULL",
            params![id],
        )?;
        self.get(id)
    }

    /// The identifier and hash of every token in use.
    ///
    /// # Errors
    ///
    /// [`ServerTokenError::Db`] on any SQLite failure.
    pub fn active_hashes(&self) -> Result<Vec<(i64, String)>, ServerTokenError> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, hash FROM server_tokens WHERE revoked_at IS NULL")?;
        let rows = stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// How many tokens are in use.
    ///
    /// # Errors
    ///
    /// [`ServerTokenError::Db`] on any SQLite failure.
    pub fn active_count(&self) -> Result<u64, ServerTokenError> {
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM server_tokens WHERE revoked_at IS NULL",
            [],
            |row| row.get(0),
        )?;
        Ok(u64::try_from(count).unwrap_or(0))
    }

    /// Stamps the last use of the token `id` with the current time.
    ///
    /// # Errors
    ///
    /// [`ServerTokenError::Db`] on any SQLite failure.
    pub fn touch(&self, id: i64) -> Result<(), ServerTokenError> {
        self.conn.execute(
            "UPDATE server_tokens
             SET last_used_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
             WHERE id = ?1",
            params![id],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests;
