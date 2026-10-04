//! The registry of sources, persisted in `system.db`.
//!
//! A source is what an agent may consult once it is checked for it: an indexed
//! folder, a file, a mailbox, the user profile, the agent's memory, or a code
//! folder. Each row names its kind, a label, an optional group, what it points
//! at (a folder connection, a file path, a mail connection, a memory space),
//! the state of its index and when it was added. The table is created by
//! `system_db::migrate_v8`.
//!
//! The `sources` list of an agent (package default, then the user's override)
//! holds identifiers of this registry, or one of the two kinds that point at
//! nothing: [`SourceKind::Profile`] and [`SourceKind::Memory`], written by
//! their token (`profile`, `memory`). [`SourceRegistryStore::check_entries`]
//! is the check a write of that list goes through.

use std::path::Path;

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Errors from [`SourceRegistryStore`].
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum SourceRegistryError {
    /// Underlying SQLite error.
    #[error("database error: {0}")]
    Db(#[from] rusqlite::Error),

    /// The database schema could not be brought to the supported version.
    #[error(transparent)]
    Schema(#[from] crate::schema::SchemaError),

    /// The kind given is not one of [`SourceKind::ALL`].
    #[error(
        "unknown source kind '{0}', expected one of folder, file, mail, profile, memory, code_folder"
    )]
    UnknownKind(String),

    /// A stored index state is not one of [`IndexState::ALL`].
    #[error("unknown index state '{0}'")]
    UnknownIndexState(String),

    /// The label is empty once trimmed.
    #[error("a source needs a label")]
    EmptyLabel,

    /// A folder, a file or a mailbox was given nothing to point at.
    #[error("a {0} source needs a reference")]
    MissingReference(SourceKind),

    /// No source has this identifier.
    #[error("source '{0}' not found")]
    NotFound(String),

    /// An entry of an agent's source list is neither a registry identifier
    /// nor `profile` nor `memory`.
    #[error("unknown source '{0}': expected a source identifier, profile or memory")]
    UnknownSource(String),
}

/// What a source is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    /// A folder whose documents are indexed.
    Folder,
    /// One file.
    File,
    /// A mailbox.
    Mail,
    /// The user profile.
    Profile,
    /// The agent's memory.
    Memory,
    /// A code folder, offered in developer mode.
    CodeFolder,
}

impl SourceKind {
    /// Every kind, in display order.
    pub const ALL: [SourceKind; 6] = [
        SourceKind::Folder,
        SourceKind::File,
        SourceKind::Mail,
        SourceKind::Profile,
        SourceKind::Memory,
        SourceKind::CodeFolder,
    ];

    /// Stored and wire form.
    pub fn as_str(self) -> &'static str {
        match self {
            SourceKind::Folder => "folder",
            SourceKind::File => "file",
            SourceKind::Mail => "mail",
            SourceKind::Profile => "profile",
            SourceKind::Memory => "memory",
            SourceKind::CodeFolder => "code_folder",
        }
    }

    /// Reads a kind.
    ///
    /// # Errors
    ///
    /// [`SourceRegistryError::UnknownKind`] for any other token.
    pub fn parse(raw: &str) -> Result<Self, SourceRegistryError> {
        Self::ALL
            .into_iter()
            .find(|kind| kind.as_str() == raw)
            .ok_or_else(|| SourceRegistryError::UnknownKind(raw.to_owned()))
    }

    /// Whether a source of this kind must name what it points at.
    pub fn requires_reference(self) -> bool {
        matches!(
            self,
            SourceKind::Folder | SourceKind::File | SourceKind::Mail
        )
    }

    /// Whether an agent's source list may name this kind by its token alone,
    /// without a registry identifier.
    pub fn listed_by_token(self) -> bool {
        matches!(self, SourceKind::Profile | SourceKind::Memory)
    }
}

impl std::fmt::Display for SourceKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Where the index of a source stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IndexState {
    /// Not indexed.
    Absent,
    /// Being indexed.
    Indexing,
    /// Indexed, keywords and meaning.
    Ready,
    /// Indexing failed.
    Error,
    /// Indexed for keywords only, no embeddings model being present.
    KeywordsOnly,
}

impl IndexState {
    /// Every state.
    pub const ALL: [IndexState; 5] = [
        IndexState::Absent,
        IndexState::Indexing,
        IndexState::Ready,
        IndexState::Error,
        IndexState::KeywordsOnly,
    ];

    /// Stored and wire form.
    pub fn as_str(self) -> &'static str {
        match self {
            IndexState::Absent => "absent",
            IndexState::Indexing => "indexing",
            IndexState::Ready => "ready",
            IndexState::Error => "error",
            IndexState::KeywordsOnly => "keywords_only",
        }
    }

    /// Reads a state.
    ///
    /// # Errors
    ///
    /// [`SourceRegistryError::UnknownIndexState`] for any other token.
    pub fn parse(raw: &str) -> Result<Self, SourceRegistryError> {
        Self::ALL
            .into_iter()
            .find(|state| state.as_str() == raw)
            .ok_or_else(|| SourceRegistryError::UnknownIndexState(raw.to_owned()))
    }
}

/// A source to add to the registry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewSource {
    /// What the source is.
    pub kind: SourceKind,
    /// The name the user sees.
    pub label: String,
    /// A group the source belongs to, such as the files of one project.
    pub group: Option<String>,
    /// What the source points at: a folder connection, a file path, a mail
    /// connection, a memory space. Required for a folder, a file and a mailbox.
    pub reference: Option<String>,
}

/// A source of the registry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Source {
    /// Identifier, the form an agent's source list names it by.
    pub id: String,
    /// What the source is.
    pub kind: SourceKind,
    /// The name the user sees.
    pub label: String,
    /// The group the source belongs to, if any.
    pub group: Option<String>,
    /// What the source points at, if anything.
    pub reference: Option<String>,
    /// Where its index stands.
    pub index_state: IndexState,
    /// When it was added, RFC 3339 in UTC.
    pub added_at: String,
}

/// The registry of sources of `system.db`.
pub struct SourceRegistryStore {
    conn: Connection,
}

/// A trimmed value, `None` when blank.
fn non_blank(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(str::to_owned)
}

impl SourceRegistryStore {
    /// Open (or create) `system.db` at `path` and bring it to the current
    /// schema version.
    ///
    /// # Errors
    /// [`SourceRegistryError::Db`] if the file cannot be opened, and
    /// [`SourceRegistryError::Schema`] if the migration fails or the file was
    /// written by a newer binary.
    pub fn open(path: &Path) -> Result<Self, SourceRegistryError> {
        let conn = Connection::open(path)?;
        crate::schema::open_versioned(
            &conn,
            crate::paths::DataFile::System.file_name(),
            crate::system_db::SYSTEM_DB_SCHEMA_VERSION,
            crate::system_db::SYSTEM_DB_MIGRATIONS,
        )?;
        Ok(Self { conn })
    }

    /// Adds `source` and returns it as stored, with a new identifier, the
    /// index state `absent` and the time of addition.
    ///
    /// Label, group and reference are stored trimmed, a blank group or
    /// reference as none.
    ///
    /// # Errors
    /// [`SourceRegistryError::EmptyLabel`] for a blank label,
    /// [`SourceRegistryError::MissingReference`] for a folder, a file or a
    /// mailbox without a reference, and [`SourceRegistryError::Db`] on any
    /// SQLite failure.
    pub fn create(&self, source: &NewSource) -> Result<Source, SourceRegistryError> {
        let label = source.label.trim();
        if label.is_empty() {
            return Err(SourceRegistryError::EmptyLabel);
        }
        let reference = non_blank(source.reference.as_deref());
        if source.kind.requires_reference() && reference.is_none() {
            return Err(SourceRegistryError::MissingReference(source.kind));
        }
        let group = non_blank(source.group.as_deref());
        let id = format!("src-{}", uuid::Uuid::new_v4().simple());
        self.conn.execute(
            "INSERT INTO sources (id, kind, label, group_name, reference)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![id, source.kind.as_str(), label, group, reference],
        )?;
        self.get(&id)?
            .ok_or_else(|| SourceRegistryError::NotFound(id.clone()))
    }

    /// Every source, oldest first.
    ///
    /// # Errors
    /// [`SourceRegistryError::Db`] on any SQLite failure, and
    /// [`SourceRegistryError::UnknownKind`] or
    /// [`SourceRegistryError::UnknownIndexState`] for a row the `CHECK`
    /// constraints should have refused.
    pub fn list(&self) -> Result<Vec<Source>, SourceRegistryError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, kind, label, group_name, reference, index_state, added_at
             FROM sources ORDER BY added_at, id",
        )?;
        let rows = stmt.query_map([], raw_row)?;
        rows.map(|row| row.map_err(SourceRegistryError::from).and_then(source_of))
            .collect()
    }

    /// The source `id`, `None` when there is none.
    ///
    /// # Errors
    /// As [`SourceRegistryStore::list`].
    pub fn get(&self, id: &str) -> Result<Option<Source>, SourceRegistryError> {
        self.conn
            .query_row(
                "SELECT id, kind, label, group_name, reference, index_state, added_at
                 FROM sources WHERE id = ?1",
                params![id],
                raw_row,
            )
            .optional()?
            .map(source_of)
            .transpose()
    }

    /// Removes the source `id` and returns it.
    ///
    /// # Errors
    /// [`SourceRegistryError::NotFound`] when there is none, and
    /// [`SourceRegistryError::Db`] on any SQLite failure.
    pub fn delete(&self, id: &str) -> Result<Source, SourceRegistryError> {
        let source = self
            .get(id)?
            .ok_or_else(|| SourceRegistryError::NotFound(id.to_owned()))?;
        self.conn
            .execute("DELETE FROM sources WHERE id = ?1", params![id])?;
        Ok(source)
    }

    /// Checks that every entry of an agent's source list names a source of
    /// the registry, or one of the kinds listed by their token (`profile`,
    /// `memory`).
    ///
    /// # Errors
    /// [`SourceRegistryError::UnknownSource`] for the first entry that does
    /// not, and [`SourceRegistryError::Db`] on any SQLite failure.
    pub fn check_entries(&self, entries: &[String]) -> Result<(), SourceRegistryError> {
        for entry in entries {
            let by_token = SourceKind::parse(entry).is_ok_and(SourceKind::listed_by_token);
            if !by_token && self.get(entry)?.is_none() {
                return Err(SourceRegistryError::UnknownSource(entry.clone()));
            }
        }
        Ok(())
    }
}

/// The columns of one row, before the kind and state are read.
type RawRow = (
    String,
    String,
    String,
    Option<String>,
    Option<String>,
    String,
    String,
);

fn raw_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<RawRow> {
    Ok((
        row.get(0)?,
        row.get(1)?,
        row.get(2)?,
        row.get(3)?,
        row.get(4)?,
        row.get(5)?,
        row.get(6)?,
    ))
}

fn source_of(raw: RawRow) -> Result<Source, SourceRegistryError> {
    let (id, kind, label, group, reference, index_state, added_at) = raw;
    Ok(Source {
        id,
        kind: SourceKind::parse(&kind)?,
        label,
        group,
        reference,
        index_state: IndexState::parse(&index_state)?,
        added_at,
    })
}

#[cfg(test)]
mod tests;
