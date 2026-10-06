//! The index of one source: its documents, their chunks, and their vectors.
//!
//! Each source has a database of its own, `<data_dir>/sources/<id>.db`
//! ([`apollia_core::paths::source_index_path`]), so removing a source is
//! removing one file ([`remove_index`]). The file holds:
//!
//! - `documents`: one row per file, with its path, a fingerprint of its
//!   content and its modification date, which a later pass compares to know
//!   what changed;
//! - `chunks`: the passages of each document ([`crate::chunk`]), their
//!   [`Location`] as JSON, and, when an embeddings model computed one, their
//!   vector as a BLOB of little-endian `f32` with a norm of one
//!   ([`apollia_core::vector_search::encode_vector`]);
//! - `chunks_fts`: a full-text index of the chunks (FTS5, `unicode61`, accents
//!   folded), kept in step by triggers;
//! - `index_meta`: the embeddings model the vectors came from. Vectors of two
//!   models are not comparable, so the index refuses a second model rather
//!   than mixing them;
//! - `skipped_files`: the files of the source left out of the index, the ones
//!   over the size cap and the ones no reader could open, so the state of the
//!   source can name them;
//! - `unread_pages`: the pages of an indexed document that hold no text and
//!   were not read, the ones waiting for optical character recognition and
//!   the ones it cannot read, so the state of the source can name them too.
//!
//! Searching it is [`SourceIndex::search`], in the `search` submodule.

use std::path::{Path, PathBuf};

use apollia_core::schema::{open_versioned, Migration, SchemaError};
use apollia_core::vector_search::{encode_vector, normalize, VectorDecodeError};
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;

use crate::chunk::Chunk;

mod search;

pub use search::{
    query_vector, QueryEmbedder, QueryVector, SearchHit, SearchMode, SearchOutcome,
    CANDIDATES_PER_RANKING, DEFAULT_LIMIT, MAX_LIMIT,
};

/// Version of the index schema; equals the length of the migration list.
pub const INDEX_SCHEMA_VERSION: u32 = 3;

const INDEX_MIGRATIONS: &[Migration] = &[migrate_v1, migrate_v2, migrate_v3];

/// Name of the index store in schema errors.
const STORE_NAME: &str = "source index";

/// Errors of a source index.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum IndexError {
    /// Underlying SQLite error.
    #[error("index database error: {0}")]
    Db(#[from] rusqlite::Error),
    /// The file could not be brought to the supported schema.
    #[error(transparent)]
    Schema(#[from] SchemaError),
    /// A file or directory of the index could not be created or removed.
    #[error("cannot reach {path}: {source}")]
    Io {
        /// The path concerned.
        path: PathBuf,
        /// The cause.
        #[source]
        source: std::io::Error,
    },
    /// The identifier cannot name a file under the sources directory.
    #[error("'{0}' cannot name a source index")]
    InvalidSourceId(String),
    /// Vectors of another model than the one the index holds.
    #[error("the index holds vectors of {stored}, not of {given}; it must be rebuilt")]
    ModelChanged {
        /// Model of the stored vectors.
        stored: String,
        /// Model of the vectors given.
        given: String,
    },
    /// Another number of vectors than of chunks.
    #[error("{chunks} chunk(s) but {vectors} vector(s)")]
    VectorCount {
        /// Chunks given.
        chunks: usize,
        /// Vectors given.
        vectors: usize,
    },
    /// A vector of another dimension than the others of the index.
    #[error("a vector of {given} dimensions in an index of {stored}")]
    Dimensions {
        /// Dimensions of the stored vectors.
        stored: usize,
        /// Dimensions of the vector given.
        given: usize,
    },
    /// A vector with no direction, which cannot be normalised.
    #[error("the vector of chunk {0} is zero")]
    ZeroVector(usize),
    /// A stored location is not readable.
    #[error("unreadable chunk location: {0}")]
    Location(#[from] serde_json::Error),
    /// A stored vector is not readable.
    #[error(transparent)]
    Vector(#[from] VectorDecodeError),
}

/// What identifies the version of a document that was indexed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentRecord {
    /// Path of the file, as the source names it.
    pub path: String,
    /// A fingerprint of the content, compared to know whether it changed.
    pub fingerprint: String,
    /// Modification date of the file, as RFC 3339 text.
    pub modified_at: String,
}

/// Why a file of a source is left out of its index.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SkipReason {
    /// The file is over the size cap of the indexing.
    TooLarge,
    /// No reader could open it: damaged, protected by a password, or not the
    /// format its name says.
    Unreadable,
}

impl SkipReason {
    fn as_str(self) -> &'static str {
        match self {
            SkipReason::TooLarge => "too_large",
            SkipReason::Unreadable => "unreadable",
        }
    }

    fn parse(raw: &str) -> Self {
        if raw == "too_large" {
            SkipReason::TooLarge
        } else {
            SkipReason::Unreadable
        }
    }
}

/// A file of a source left out of its index, and why.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SkippedFile {
    /// Path of the file.
    pub path: String,
    /// Its size in bytes when it was left out.
    pub size_bytes: u64,
    /// Why it was left out.
    pub reason: SkipReason,
}

/// Why a page of a document holds no text in the index.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum UnreadReason {
    /// A scanned page or a picture, on a machine where optical character
    /// recognition is not available: it is read once it is.
    OcrUnavailable,
    /// A page without text that recognition cannot read: it is not a single
    /// picture, or the engine failed on it.
    NotRead,
}

impl UnreadReason {
    fn as_str(self) -> &'static str {
        match self {
            UnreadReason::OcrUnavailable => "ocr_unavailable",
            UnreadReason::NotRead => "not_read",
        }
    }

    fn parse(raw: &str) -> Self {
        if raw == "ocr_unavailable" {
            UnreadReason::OcrUnavailable
        } else {
            UnreadReason::NotRead
        }
    }
}

/// A page of an indexed document that was not read, and why.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct UnreadPage {
    /// Path of the document.
    pub path: String,
    /// Page number, from 1; a picture file is its page 1.
    pub page: u32,
    /// Why it was not read.
    pub reason: UnreadReason,
}

/// The vectors of a document's chunks, one per chunk, in chunk order.
#[derive(Debug, Clone, PartialEq)]
pub struct ChunkVectors {
    /// Identifier of the embeddings model that computed them.
    pub model: String,
    /// One vector per chunk.
    pub vectors: Vec<Vec<f32>>,
}

/// v1: documents, chunks with their vectors, the full-text index and its
/// triggers, and the model of the vectors. Every statement is idempotent.
fn migrate_v1(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS documents (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            path        TEXT NOT NULL UNIQUE CHECK (length(path) > 0),
            fingerprint TEXT NOT NULL,
            modified_at TEXT NOT NULL,
            indexed_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
        );

        CREATE TABLE IF NOT EXISTS chunks (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            document_id INTEGER NOT NULL REFERENCES documents (id) ON DELETE CASCADE,
            ordinal     INTEGER NOT NULL,
            text        TEXT NOT NULL,
            location    TEXT NOT NULL,
            vector      BLOB,
            UNIQUE (document_id, ordinal)
        );

        CREATE TABLE IF NOT EXISTS index_meta (
            id              INTEGER PRIMARY KEY CHECK (id = 1),
            embedding_model TEXT,
            dimensions      INTEGER
        );

        CREATE VIRTUAL TABLE IF NOT EXISTS chunks_fts USING fts5 (
            text,
            content = 'chunks',
            content_rowid = 'id',
            tokenize = 'unicode61 remove_diacritics 2'
        );

        CREATE TRIGGER IF NOT EXISTS chunks_fts_insert AFTER INSERT ON chunks BEGIN
            INSERT INTO chunks_fts (rowid, text) VALUES (new.id, new.text);
        END;

        CREATE TRIGGER IF NOT EXISTS chunks_fts_delete AFTER DELETE ON chunks BEGIN
            INSERT INTO chunks_fts (chunks_fts, rowid, text) VALUES ('delete', old.id, old.text);
        END;",
    )
}

/// v2: the files left out of the index, by path.
fn migrate_v2(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS skipped_files (
            path       TEXT PRIMARY KEY CHECK (length(path) > 0),
            size_bytes INTEGER NOT NULL,
            reason     TEXT NOT NULL CHECK (reason IN ('too_large', 'unreadable'))
        );",
    )
}

/// v3: the pages of an indexed document that were not read, gone with it.
fn migrate_v3(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS unread_pages (
            document_id INTEGER NOT NULL REFERENCES documents (id) ON DELETE CASCADE,
            page        INTEGER NOT NULL,
            reason      TEXT NOT NULL CHECK (reason IN ('ocr_unavailable', 'not_read')),
            PRIMARY KEY (document_id, page)
        );",
    )
}

/// The index of one source, open on its database file.
pub struct SourceIndex {
    conn: Connection,
    source_id: String,
}

impl std::fmt::Debug for SourceIndex {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SourceIndex")
            .field("source_id", &self.source_id)
            .finish_non_exhaustive()
    }
}

/// The index file of `source_id` under `data_dir`.
///
/// # Errors
/// [`IndexError::InvalidSourceId`] when the identifier cannot name a file.
pub fn index_path(data_dir: &Path, source_id: &str) -> Result<PathBuf, IndexError> {
    apollia_core::paths::source_index_path(data_dir, source_id)
        .ok_or_else(|| IndexError::InvalidSourceId(source_id.to_owned()))
}

/// Removes the index of `source_id` under `data_dir`, with the journal files
/// SQLite keeps beside it. Returns whether there was an index to remove.
///
/// # Errors
/// [`IndexError::InvalidSourceId`] for an identifier that cannot name a file,
/// [`IndexError::Io`] when a file exists and cannot be removed.
pub fn remove_index(data_dir: &Path, source_id: &str) -> Result<bool, IndexError> {
    let path = index_path(data_dir, source_id)?;
    let mut removed = false;
    for suffix in ["", "-wal", "-shm", "-journal"] {
        let mut name = path.clone().into_os_string();
        name.push(suffix);
        let file = PathBuf::from(name);
        match std::fs::remove_file(&file) {
            Ok(()) => removed |= suffix.is_empty(),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(source) => return Err(IndexError::Io { path: file, source }),
        }
    }
    if removed {
        tracing::event!(
            tracing::Level::INFO,
            source = %source_id,
            "sources.index.removed"
        );
    }
    Ok(removed)
}

impl SourceIndex {
    /// Opens, creating it when absent, the index of `source_id` under
    /// `data_dir`, and brings it to the current schema.
    ///
    /// # Errors
    /// [`IndexError::InvalidSourceId`], [`IndexError::Io`] when the sources
    /// directory cannot be created, and the errors of [`Self::open_at`].
    pub fn open(data_dir: &Path, source_id: &str) -> Result<Self, IndexError> {
        let path = index_path(data_dir, source_id)?;
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|source| IndexError::Io {
                path: dir.to_path_buf(),
                source,
            })?;
        }
        Self::open_at(&path, source_id)
    }

    /// Opens the index database at `path` for the source `source_id`.
    ///
    /// # Errors
    /// [`IndexError::Db`] when the file cannot be opened, and
    /// [`IndexError::Schema`] when it cannot be migrated or is newer than
    /// this binary.
    pub fn open_at(path: &Path, source_id: &str) -> Result<Self, IndexError> {
        let conn = Connection::open(path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        open_versioned(&conn, STORE_NAME, INDEX_SCHEMA_VERSION, INDEX_MIGRATIONS)?;
        Ok(Self {
            conn,
            source_id: source_id.to_owned(),
        })
    }

    /// The source this index belongs to.
    #[must_use]
    pub fn source_id(&self) -> &str {
        &self.source_id
    }

    /// The embeddings model of the stored vectors, `None` before any.
    ///
    /// # Errors
    /// [`IndexError::Db`] on any SQLite failure.
    pub fn embedding_model(&self) -> Result<Option<String>, IndexError> {
        Ok(self
            .conn
            .query_row(
                "SELECT embedding_model FROM index_meta WHERE id = 1",
                [],
                |row| row.get::<_, Option<String>>(0),
            )
            .optional()?
            .flatten())
    }

    /// The indexed version of the document at `path`, if any.
    ///
    /// # Errors
    /// [`IndexError::Db`] on any SQLite failure.
    pub fn document(&self, path: &str) -> Result<Option<DocumentRecord>, IndexError> {
        Ok(self
            .conn
            .query_row(
                "SELECT path, fingerprint, modified_at FROM documents WHERE path = ?1",
                params![path],
                |row| {
                    Ok(DocumentRecord {
                        path: row.get(0)?,
                        fingerprint: row.get(1)?,
                        modified_at: row.get(2)?,
                    })
                },
            )
            .optional()?)
    }

    /// Number of documents and of chunks in the index.
    ///
    /// # Errors
    /// [`IndexError::Db`] on any SQLite failure.
    pub fn counts(&self) -> Result<(u64, u64), IndexError> {
        Ok(self.conn.query_row(
            "SELECT (SELECT COUNT(*) FROM documents), (SELECT COUNT(*) FROM chunks)",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?)
    }

    /// Writes a document and its chunks, replacing any earlier version of it,
    /// in one transaction.
    ///
    /// `vectors`, when given, holds one vector per chunk; each is normalised
    /// before it is stored. Without it the chunks are found by their words
    /// alone.
    ///
    /// # Errors
    /// [`IndexError::VectorCount`], [`IndexError::ZeroVector`],
    /// [`IndexError::Dimensions`] or [`IndexError::ModelChanged`] for vectors
    /// that do not fit the chunks or the index, and [`IndexError::Db`] on any
    /// SQLite failure. Nothing is written on error.
    pub fn upsert_document(
        &mut self,
        document: &DocumentRecord,
        chunks: &[Chunk],
        vectors: Option<&ChunkVectors>,
    ) -> Result<(), IndexError> {
        let encoded = match vectors {
            Some(given) => Some(self.encode_vectors(chunks.len(), given)?),
            None => None,
        };
        let tx = self.conn.transaction()?;
        delete_document(&tx, &document.path)?;
        tx.execute(
            "INSERT INTO documents (path, fingerprint, modified_at) VALUES (?1, ?2, ?3)",
            params![document.path, document.fingerprint, document.modified_at],
        )?;
        let document_id = tx.last_insert_rowid();
        {
            let mut insert = tx.prepare(
                "INSERT INTO chunks (document_id, ordinal, text, location, vector)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
            )?;
            for (ordinal, chunk) in chunks.iter().enumerate() {
                let location = serde_json::to_string(&chunk.location)?;
                let vector = encoded.as_ref().and_then(|v| v.get(ordinal));
                insert.execute(params![
                    document_id,
                    i64::try_from(ordinal).unwrap_or(i64::MAX),
                    chunk.text,
                    location,
                    vector
                ])?;
            }
        }
        if let Some(given) = vectors {
            let dimensions = given.vectors.first().map_or(0, Vec::len);
            tx.execute(
                "INSERT INTO index_meta (id, embedding_model, dimensions) VALUES (1, ?1, ?2)
                 ON CONFLICT (id) DO UPDATE SET
                     embedding_model = excluded.embedding_model,
                     dimensions = excluded.dimensions",
                params![given.model, i64::try_from(dimensions).unwrap_or(i64::MAX)],
            )?;
        }
        tx.commit()?;
        tracing::event!(
            tracing::Level::DEBUG,
            source = %self.source_id,
            chunks = chunks.len(),
            vectors = vectors.is_some(),
            "sources.index.document.written"
        );
        Ok(())
    }

    /// Removes the document at `path` and its chunks. Returns whether it was
    /// indexed.
    ///
    /// # Errors
    /// [`IndexError::Db`] on any SQLite failure.
    pub fn remove_document(&mut self, path: &str) -> Result<bool, IndexError> {
        let tx = self.conn.transaction()?;
        let removed = delete_document(&tx, path)?;
        tx.commit()?;
        Ok(removed)
    }

    /// Every indexed document, by path.
    ///
    /// # Errors
    /// [`IndexError::Db`] on any SQLite failure.
    pub fn documents(&self) -> Result<Vec<DocumentRecord>, IndexError> {
        let mut stmt = self
            .conn
            .prepare("SELECT path, fingerprint, modified_at FROM documents ORDER BY path")?;
        let rows = stmt.query_map([], |row| {
            Ok(DocumentRecord {
                path: row.get(0)?,
                fingerprint: row.get(1)?,
                modified_at: row.get(2)?,
            })
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    /// Records a new modification date for the document at `path`, whose
    /// content did not change. Returns whether it was indexed.
    ///
    /// # Errors
    /// [`IndexError::Db`] on any SQLite failure.
    pub fn touch_document(&mut self, path: &str, modified_at: &str) -> Result<bool, IndexError> {
        let changed = self.conn.execute(
            "UPDATE documents SET modified_at = ?2 WHERE path = ?1",
            params![path, modified_at],
        )?;
        Ok(changed > 0)
    }

    /// Empties the index: every document, chunk and skipped file, and the
    /// model of the vectors, so that it can be rebuilt with another model.
    ///
    /// # Errors
    /// [`IndexError::Db`] on any SQLite failure.
    pub fn clear(&mut self) -> Result<(), IndexError> {
        let tx = self.conn.transaction()?;
        tx.execute("DELETE FROM chunks", [])?;
        tx.execute("DELETE FROM documents", [])?;
        tx.execute("DELETE FROM skipped_files", [])?;
        tx.execute("DELETE FROM index_meta", [])?;
        tx.commit()?;
        tracing::event!(
            tracing::Level::INFO,
            source = %self.source_id,
            "sources.index.cleared"
        );
        Ok(())
    }

    /// The files left out of the index, by path.
    ///
    /// # Errors
    /// [`IndexError::Db`] on any SQLite failure.
    pub fn skipped(&self) -> Result<Vec<SkippedFile>, IndexError> {
        let mut stmt = self
            .conn
            .prepare("SELECT path, size_bytes, reason FROM skipped_files ORDER BY path")?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (path, size, reason) = row?;
            out.push(SkippedFile {
                path,
                size_bytes: u64::try_from(size).unwrap_or(0),
                reason: SkipReason::parse(&reason),
            });
        }
        Ok(out)
    }

    /// Records `file` as left out of the index, replacing any earlier record
    /// of the same path.
    ///
    /// # Errors
    /// [`IndexError::Db`] on any SQLite failure.
    pub fn record_skipped(&mut self, file: &SkippedFile) -> Result<(), IndexError> {
        self.conn.execute(
            "INSERT INTO skipped_files (path, size_bytes, reason) VALUES (?1, ?2, ?3)
             ON CONFLICT (path) DO UPDATE SET
                 size_bytes = excluded.size_bytes,
                 reason = excluded.reason",
            params![
                file.path,
                i64::try_from(file.size_bytes).unwrap_or(i64::MAX),
                file.reason.as_str()
            ],
        )?;
        Ok(())
    }

    /// Forgets that the file at `path` was left out. Returns whether it was.
    ///
    /// # Errors
    /// [`IndexError::Db`] on any SQLite failure.
    pub fn forget_skipped(&mut self, path: &str) -> Result<bool, IndexError> {
        let changed = self
            .conn
            .execute("DELETE FROM skipped_files WHERE path = ?1", params![path])?;
        Ok(changed > 0)
    }

    /// Records the pages of the document at `path` that were not read, in
    /// place of any earlier record. They leave the index with the document.
    ///
    /// # Errors
    /// [`IndexError::Db`] on any SQLite failure.
    pub fn set_unread_pages(
        &mut self,
        path: &str,
        pages: &[(u32, UnreadReason)],
    ) -> Result<(), IndexError> {
        let tx = self.conn.transaction()?;
        tx.execute(
            "DELETE FROM unread_pages
             WHERE document_id IN (SELECT id FROM documents WHERE path = ?1)",
            params![path],
        )?;
        for (page, reason) in pages {
            tx.execute(
                "INSERT OR REPLACE INTO unread_pages (document_id, page, reason)
                 SELECT id, ?2, ?3 FROM documents WHERE path = ?1",
                params![path, page, reason.as_str()],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// The pages that were not read, by path then page.
    ///
    /// # Errors
    /// [`IndexError::Db`] on any SQLite failure.
    pub fn unread_pages(&self) -> Result<Vec<UnreadPage>, IndexError> {
        let mut stmt = self.conn.prepare(
            "SELECT d.path, u.page, u.reason FROM unread_pages u
             JOIN documents d ON d.id = u.document_id
             ORDER BY d.path, u.page",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(UnreadPage {
                path: row.get(0)?,
                page: row.get(1)?,
                reason: UnreadReason::parse(&row.get::<_, String>(2)?),
            })
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    /// Checks `given` against the chunks and the stored model, and encodes
    /// its vectors, normalised.
    fn encode_vectors(
        &self,
        chunks: usize,
        given: &ChunkVectors,
    ) -> Result<Vec<Vec<u8>>, IndexError> {
        if given.vectors.len() != chunks {
            return Err(IndexError::VectorCount {
                chunks,
                vectors: given.vectors.len(),
            });
        }
        let stored: Option<(Option<String>, Option<i64>)> = self
            .conn
            .query_row(
                "SELECT embedding_model, dimensions FROM index_meta WHERE id = 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        let (stored_model, stored_dimensions) = stored.unwrap_or((None, None));
        if let Some(stored) = stored_model.filter(|m| *m != given.model) {
            return Err(IndexError::ModelChanged {
                stored,
                given: given.model.clone(),
            });
        }
        let expected = stored_dimensions
            .and_then(|d| usize::try_from(d).ok())
            .or_else(|| given.vectors.first().map(Vec::len));
        let mut encoded = Vec::with_capacity(chunks);
        for (ordinal, vector) in given.vectors.iter().enumerate() {
            if let Some(stored) = expected.filter(|d| *d != vector.len()) {
                return Err(IndexError::Dimensions {
                    stored,
                    given: vector.len(),
                });
            }
            let mut unit = vector.clone();
            if !normalize(&mut unit) {
                return Err(IndexError::ZeroVector(ordinal));
            }
            encoded.push(encode_vector(&unit));
        }
        Ok(encoded)
    }
}

/// Deletes the chunks, then the row, of the document at `path`.
fn delete_document(conn: &Connection, path: &str) -> Result<bool, rusqlite::Error> {
    conn.execute(
        "DELETE FROM chunks WHERE document_id IN (SELECT id FROM documents WHERE path = ?1)",
        params![path],
    )?;
    let removed = conn.execute("DELETE FROM documents WHERE path = ?1", params![path])?;
    Ok(removed > 0)
}

#[cfg(test)]
mod tests;
