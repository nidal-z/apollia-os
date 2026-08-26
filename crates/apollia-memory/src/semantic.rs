//! SemanticMemory: the agent's structured knowledge base.
//!
//! Each entry is a key/value pair with a confidence score (0.0..=1.0), an
//! optional origin source, and an optional TTL (`expires_at`).
//! Keys are hierarchical by convention (`client.dupont.budget_max`).
//! The agent alone decides what gets recorded.

use crate::store::MemoryStore;

/// Default maximum number of semantic entries per namespace.
/// Beyond this limit, least-recently-updated entries are evicted.
pub(crate) const DEFAULT_MAX_ENTRIES_PER_NAMESPACE: u64 = 5_000;

/// Semantic memory backend: a structured knowledge base.
///
/// Stores key/value pairs with confidence and source.
/// Keys are hierarchical by convention (`client.dupont.budget_max`).
/// Upsert is native: `remember()` with an existing key updates it.
pub struct SemanticMemory<'a> {
    store: &'a MemoryStore,
}

/// Parameters for a piece of knowledge stored by [`SemanticMemory::remember`].
pub struct RememberInput<'r> {
    /// Entry namespace.
    pub namespace: &'r str,
    /// Hierarchical key (e.g. `client.dupont.budget_max`).
    pub key: &'r str,
    /// JSON value of the knowledge.
    pub value: &'r serde_json::Value,
    /// Confidence score (0.0..=1.0).
    pub confidence: f64,
    /// Source that produced this knowledge.
    pub source: Option<&'r str>,
    /// Optional ISO 8601 expiry date.
    pub expires_at: Option<&'r str>,
}

/// Entry returned by [`SemanticMemory::recall`].
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SemanticEntry {
    /// Unique UUID v4 of the entry.
    pub id: String,
    /// Entry namespace.
    pub namespace: String,
    /// Hierarchical key (e.g. `client.dupont.budget_max`).
    pub key: String,
    /// JSON value of the knowledge.
    pub value: serde_json::Value,
    /// Source that produced this knowledge.
    pub source: Option<String>,
    /// Confidence score (0.0..=1.0).
    pub confidence: f64,
    /// ISO 8601 creation date.
    pub created_at: String,
    /// ISO 8601 last-updated date.
    pub updated_at: String,
    /// Optional ISO 8601 expiry date.
    pub expires_at: Option<String>,
}

/// Errors from the semantic backend.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum SemanticMemoryError {
    /// Inserting or updating a piece of knowledge failed.
    #[error("failed to store knowledge: {0}")]
    StoreFailed(String),

    /// The recall query failed.
    #[error("failed to recall knowledge: {0}")]
    RecallFailed(String),

    /// The confidence score is outside the [0.0, 1.0] range.
    #[error("invalid confidence score: {0} (must be 0.0..=1.0)")]
    InvalidConfidence(f64),

    /// The key is empty.
    #[error("empty key")]
    EmptyKey,

    /// SQLite error propagated from rusqlite.
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
}

impl<'a> SemanticMemory<'a> {
    /// Create a semantic backend bound to a [`MemoryStore`].
    pub fn new(store: &'a MemoryStore) -> Self {
        Self { store }
    }

    /// Store or update a piece of knowledge.
    ///
    /// If the key already exists in the namespace, the value is updated (upsert).
    /// Returns the UUID of the created or updated entry.
    pub fn remember(&self, input: RememberInput<'_>) -> Result<String, SemanticMemoryError> {
        let RememberInput {
            namespace,
            key,
            value,
            confidence,
            source,
            expires_at,
        } = input;
        if key.is_empty() {
            return Err(SemanticMemoryError::EmptyKey);
        }
        if !(0.0..=1.0).contains(&confidence) {
            return Err(SemanticMemoryError::InvalidConfidence(confidence));
        }

        let now = chrono_now_utc();
        let value_str = value.to_string();
        let conn = self.store.conn();

        // Check if key already exists to decide insert vs upsert and handle FTS.
        let existing_id: Option<String> = conn
            .query_row(
                "SELECT id FROM semantic_memories WHERE namespace = ?1 AND key = ?2",
                rusqlite::params![namespace, key],
                |row| row.get(0),
            )
            .ok();

        let id = match existing_id {
            Some(existing) => {
                // Upsert: update existing row.
                conn.execute(
                    "UPDATE semantic_memories
                     SET value = ?1, confidence = ?2, source = ?3, updated_at = ?4, expires_at = ?5
                     WHERE namespace = ?6 AND key = ?7",
                    rusqlite::params![
                        value_str, confidence, source, now, expires_at, namespace, key
                    ],
                )
                .map_err(|e| SemanticMemoryError::StoreFailed(e.to_string()))?;

                // Update FTS: delete old entry then re-insert.
                conn.execute(
                    "DELETE FROM memory_fts WHERE source_table = 'semantic' AND source_id = ?1",
                    rusqlite::params![existing],
                )
                .map_err(|e| SemanticMemoryError::StoreFailed(format!("FTS delete failed: {e}")))?;

                conn.execute(
                    "INSERT INTO memory_fts (content, source_table, source_id) VALUES (?1, ?2, ?3)",
                    rusqlite::params![format!("{key} {value_str}"), "semantic", existing],
                )
                .map_err(|e| {
                    SemanticMemoryError::StoreFailed(format!("FTS re-insert failed: {e}"))
                })?;

                tracing::info!(
                    entry_id = %existing,
                    namespace = %namespace,
                    key = %key,
                    "semantic knowledge updated"
                );

                existing
            }
            None => {
                // Insert new row.
                let new_id = uuid::Uuid::new_v4().to_string();

                conn.execute(
                    "INSERT INTO semantic_memories (id, namespace, key, value, source, confidence, created_at, updated_at, expires_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                    rusqlite::params![new_id, namespace, key, value_str, source, confidence, now, now, expires_at],
                )
                .map_err(|e| SemanticMemoryError::StoreFailed(e.to_string()))?;

                conn.execute(
                    "INSERT INTO memory_fts (content, source_table, source_id) VALUES (?1, ?2, ?3)",
                    rusqlite::params![format!("{key} {value_str}"), "semantic", new_id],
                )
                .map_err(|e| SemanticMemoryError::StoreFailed(format!("FTS insert failed: {e}")))?;

                tracing::info!(
                    entry_id = %new_id,
                    namespace = %namespace,
                    key = %key,
                    "semantic knowledge stored"
                );

                self.enforce_limit(namespace, DEFAULT_MAX_ENTRIES_PER_NAMESPACE)?;

                new_id
            }
        };

        Ok(id)
    }

    /// Evicts least-recently-updated entries when the namespace exceeds `max_entries`.
    ///
    /// Deletes entries ordered by `updated_at ASC` and their corresponding FTS
    /// index rows. Returns the number of evicted entries.
    fn enforce_limit(&self, namespace: &str, max_entries: u64) -> Result<u64, SemanticMemoryError> {
        let conn = self.store.conn();

        let count: u64 = conn
            .query_row(
                "SELECT COUNT(*) FROM semantic_memories WHERE namespace = ?1",
                rusqlite::params![namespace],
                |row| row.get(0),
            )
            .map_err(|e| SemanticMemoryError::StoreFailed(format!("count query failed: {e}")))?;

        if count <= max_entries {
            return Ok(0);
        }

        let excess = count - max_entries;

        // Delete FTS entries for the rows about to be evicted.
        conn.execute(
            "DELETE FROM memory_fts WHERE source_table = 'semantic' AND source_id IN (
                SELECT id FROM semantic_memories
                WHERE namespace = ?1
                ORDER BY updated_at ASC
                LIMIT ?2
            )",
            rusqlite::params![namespace, excess],
        )
        .map_err(|e| SemanticMemoryError::StoreFailed(format!("FTS eviction failed: {e}")))?;

        let evicted = conn
            .execute(
                "DELETE FROM semantic_memories WHERE id IN (
                    SELECT id FROM semantic_memories
                    WHERE namespace = ?1
                    ORDER BY updated_at ASC
                    LIMIT ?2
                )",
                rusqlite::params![namespace, excess],
            )
            .map_err(|e| {
                SemanticMemoryError::StoreFailed(format!("eviction delete failed: {e}"))
            })?;

        tracing::info!(
            namespace = %namespace,
            evicted = evicted,
            max_entries = max_entries,
            "semantic entries evicted to enforce namespace limit"
        );

        Ok(evicted as u64)
    }

    /// Retrieve all non-expired knowledge in a namespace, sorted by key.
    ///
    /// Returns an empty vector if the namespace has no entries.
    /// `limit` caps the number of results via SQL `LIMIT`.
    pub fn recall_all(
        &self,
        namespace: &str,
        limit: Option<u64>,
    ) -> Result<Vec<SemanticEntry>, SemanticMemoryError> {
        let conn = self.store.conn();

        let sql = match limit {
            Some(n) => format!(
                "SELECT id, namespace, key, value, source, confidence, created_at, updated_at, expires_at
                 FROM semantic_memories
                 WHERE namespace = ?1
                   AND (expires_at IS NULL OR expires_at >= datetime('now'))
                 ORDER BY key ASC
                 LIMIT {n}"
            ),
            None => "SELECT id, namespace, key, value, source, confidence, created_at, updated_at, expires_at
                 FROM semantic_memories
                 WHERE namespace = ?1
                   AND (expires_at IS NULL OR expires_at >= datetime('now'))
                 ORDER BY key ASC"
                .to_string(),
        };

        let mut stmt = conn
            .prepare(&sql)
            .map_err(|e| SemanticMemoryError::RecallFailed(e.to_string()))?;

        let rows = stmt
            .query_map(rusqlite::params![namespace], |row| {
                let value_str: String = row.get(3)?;
                Ok(SemanticEntry {
                    id: row.get(0)?,
                    namespace: row.get(1)?,
                    key: row.get(2)?,
                    value: serde_json::from_str(&value_str)
                        .unwrap_or(serde_json::Value::String(value_str)),
                    source: row.get(4)?,
                    confidence: row.get(5)?,
                    created_at: row.get(6)?,
                    updated_at: row.get(7)?,
                    expires_at: row.get(8)?,
                })
            })
            .map_err(|e| SemanticMemoryError::RecallFailed(e.to_string()))?;

        let mut entries = Vec::new();
        for row in rows {
            entries.push(row.map_err(|e| SemanticMemoryError::RecallFailed(e.to_string()))?);
        }

        Ok(entries)
    }

    /// Retrieve a piece of knowledge by key, filtering out expired entries.
    ///
    /// Returns `None` if the key does not exist or the entry has expired.
    pub fn recall_entry(
        &self,
        namespace: &str,
        key: &str,
    ) -> Result<Option<SemanticEntry>, SemanticMemoryError> {
        let conn = self.store.conn();

        let result = conn.query_row(
            "SELECT id, namespace, key, value, source, confidence, created_at, updated_at, expires_at
             FROM semantic_memories
             WHERE namespace = ?1 AND key = ?2
               AND (expires_at IS NULL OR expires_at >= datetime('now'))",
            rusqlite::params![namespace, key],
            |row| {
                let value_str: String = row.get(3)?;
                Ok(SemanticEntry {
                    id: row.get(0)?,
                    namespace: row.get(1)?,
                    key: row.get(2)?,
                    value: serde_json::from_str(&value_str)
                        .unwrap_or(serde_json::Value::String(value_str)),
                    source: row.get(4)?,
                    confidence: row.get(5)?,
                    created_at: row.get(6)?,
                    updated_at: row.get(7)?,
                    expires_at: row.get(8)?,
                })
            },
        );

        match result {
            Ok(entry) => Ok(Some(entry)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(SemanticMemoryError::RecallFailed(e.to_string())),
        }
    }

    /// Retrieve a piece of knowledge by key. `None` if absent.
    pub fn recall(
        &self,
        namespace: &str,
        key: &str,
    ) -> Result<Option<SemanticEntry>, SemanticMemoryError> {
        let conn = self.store.conn();

        let result = conn.query_row(
            "SELECT id, namespace, key, value, source, confidence, created_at, updated_at, expires_at
             FROM semantic_memories
             WHERE namespace = ?1 AND key = ?2",
            rusqlite::params![namespace, key],
            |row| {
                let value_str: String = row.get(3)?;
                Ok(SemanticEntry {
                    id: row.get(0)?,
                    namespace: row.get(1)?,
                    key: row.get(2)?,
                    value: serde_json::from_str(&value_str)
                        .unwrap_or(serde_json::Value::String(value_str)),
                    source: row.get(4)?,
                    confidence: row.get(5)?,
                    created_at: row.get(6)?,
                    updated_at: row.get(7)?,
                    expires_at: row.get(8)?,
                })
            },
        );

        match result {
            Ok(entry) => Ok(Some(entry)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(SemanticMemoryError::RecallFailed(e.to_string())),
        }
    }

    /// Delete a piece of knowledge. Returns `true` if deleted, `false` if absent.
    pub fn forget(&self, namespace: &str, key: &str) -> Result<bool, SemanticMemoryError> {
        let conn = self.store.conn();

        // Get the ID first for FTS cleanup.
        let existing_id: Option<String> = conn
            .query_row(
                "SELECT id FROM semantic_memories WHERE namespace = ?1 AND key = ?2",
                rusqlite::params![namespace, key],
                |row| row.get(0),
            )
            .ok();

        let Some(id) = existing_id else {
            return Ok(false);
        };

        // Delete FTS entry.
        conn.execute(
            "DELETE FROM memory_fts WHERE source_table = 'semantic' AND source_id = ?1",
            rusqlite::params![id],
        )
        .map_err(|e| SemanticMemoryError::StoreFailed(format!("FTS delete failed: {e}")))?;

        // Delete the semantic entry.
        let deleted = conn
            .execute(
                "DELETE FROM semantic_memories WHERE id = ?1",
                rusqlite::params![id],
            )
            .map_err(|e| SemanticMemoryError::StoreFailed(e.to_string()))?;

        if deleted > 0 {
            tracing::info!(
                entry_id = %id,
                namespace = %namespace,
                key = %key,
                "semantic knowledge forgotten"
            );
        }

        Ok(deleted > 0)
    }

    /// Delete expired knowledge. Returns the number purged.
    ///
    /// Also cleans up the corresponding FTS entries.
    pub fn purge_expired(&self, namespace: &str) -> Result<u64, SemanticMemoryError> {
        let conn = self.store.conn();

        // Delete FTS entries for expired semantic memories first.
        conn.execute(
            "DELETE FROM memory_fts WHERE source_table = 'semantic' AND source_id IN (
                SELECT id FROM semantic_memories
                WHERE namespace = ?1 AND expires_at IS NOT NULL AND expires_at < datetime('now')
            )",
            rusqlite::params![namespace],
        )
        .map_err(|e| SemanticMemoryError::StoreFailed(format!("FTS purge failed: {e}")))?;

        let purged = conn
            .execute(
                "DELETE FROM semantic_memories
                 WHERE namespace = ?1 AND expires_at IS NOT NULL AND expires_at < datetime('now')",
                rusqlite::params![namespace],
            )
            .map_err(|e| SemanticMemoryError::StoreFailed(format!("purge failed: {e}")))?;

        if purged > 0 {
            tracing::info!(
                namespace = %namespace,
                purged = purged,
                "expired semantic memories purged"
            );
        }

        Ok(purged as u64)
    }

    /// Deletes semantic entries older than `days` days. Returns the count deleted.
    ///
    /// Removes the corresponding FTS index entries before deleting the main rows.
    pub fn purge_older_than(&self, namespace: &str, days: u32) -> Result<u64, SemanticMemoryError> {
        let conn = self.store.conn();
        let cutoff = format!("-{days} days");

        conn.execute(
            "DELETE FROM memory_fts WHERE source_table = 'semantic' AND source_id IN (
                SELECT id FROM semantic_memories
                WHERE namespace = ?1 AND created_at < datetime('now', ?2)
            )",
            rusqlite::params![namespace, cutoff],
        )
        .map_err(|e| SemanticMemoryError::StoreFailed(format!("FTS purge failed: {e}")))?;

        let deleted = conn
            .execute(
                "DELETE FROM semantic_memories
                 WHERE namespace = ?1 AND created_at < datetime('now', ?2)",
                rusqlite::params![namespace, cutoff],
            )
            .map_err(|e| {
                SemanticMemoryError::StoreFailed(format!("purge_older_than failed: {e}"))
            })?;

        if deleted > 0 {
            tracing::info!(
                namespace = %namespace,
                days = days,
                deleted = deleted,
                "semantic entries purged by age"
            );
        }

        Ok(deleted as u64)
    }
}

/// Returns the current UTC time as an ISO 8601 string.
///
/// The format is the one SQLite's `strftime('%Y-%m-%dT%H:%M:%SZ', 'now')`
/// produces, so the value still compares as text against the columns
/// `purge_expired` and its kin filter on. It is formatted in process: opening
/// an in-memory database to read a clock cost two fallible calls on a path
/// that cannot report a failure.
fn chrono_now_utc() -> String {
    chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::MemoryStore;
    use serde_json::json;

    fn setup() -> (MemoryStore, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("apollia_sem_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test.db");
        let store = MemoryStore::open(&path).unwrap();
        (store, path)
    }

    // remember stores entry and FTS index
    #[test]
    fn test_remember_stores_entry_and_fts() {
        // GIVEN
        let (store, _) = setup();
        let sem = SemanticMemory::new(&store);
        // WHEN
        let id = sem
            .remember(RememberInput {
                namespace: "ns",
                key: "client.dupont.budget",
                value: &json!(15000),
                confidence: 1.0,
                source: Some("crm-agent"),
                expires_at: None,
            })
            .unwrap();
        // THEN
        assert!(!id.is_empty());
        let fts_count: i64 = store
            .conn()
            .query_row(
                "SELECT COUNT(*) FROM memory_fts WHERE source_id = ?1",
                [&id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(fts_count, 1);
    }

    // recall existing key
    #[test]
    fn test_recall_existing_key() {
        // GIVEN
        let (store, _) = setup();
        let sem = SemanticMemory::new(&store);
        sem.remember(RememberInput {
            namespace: "ns",
            key: "client.dupont.budget",
            value: &json!(15000),
            confidence: 0.9,
            source: Some("crm"),
            expires_at: None,
        })
        .unwrap();
        // WHEN
        let entry = sem.recall("ns", "client.dupont.budget").unwrap();
        // THEN
        assert!(entry.is_some());
        let e = entry.unwrap();
        assert_eq!(e.value, json!(15000));
        assert_eq!(e.confidence, 0.9);
    }

    // upsert updates value
    #[test]
    fn test_upsert_updates_value() {
        // GIVEN
        let (store, _) = setup();
        let sem = SemanticMemory::new(&store);
        sem.remember(RememberInput {
            namespace: "ns",
            key: "key",
            value: &json!("old"),
            confidence: 1.0,
            source: None,
            expires_at: None,
        })
        .unwrap();
        // WHEN
        sem.remember(RememberInput {
            namespace: "ns",
            key: "key",
            value: &json!("new"),
            confidence: 0.8,
            source: None,
            expires_at: None,
        })
        .unwrap();
        // THEN
        let entry = sem.recall("ns", "key").unwrap().unwrap();
        assert_eq!(entry.value, json!("new"));
        assert_eq!(entry.confidence, 0.8);
    }

    // upsert updates FTS entry
    #[test]
    fn test_upsert_updates_fts() {
        // GIVEN
        let (store, _) = setup();
        let sem = SemanticMemory::new(&store);
        let id = sem
            .remember(RememberInput {
                namespace: "ns",
                key: "key",
                value: &json!("old_value"),
                confidence: 1.0,
                source: None,
                expires_at: None,
            })
            .unwrap();
        // WHEN
        sem.remember(RememberInput {
            namespace: "ns",
            key: "key",
            value: &json!("new_value"),
            confidence: 0.8,
            source: None,
            expires_at: None,
        })
        .unwrap();
        // THEN: only one FTS entry for this source_id
        let fts_count: i64 = store
            .conn()
            .query_row(
                "SELECT COUNT(*) FROM memory_fts WHERE source_id = ?1",
                [&id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(fts_count, 1);
    }

    // forget removes entry and FTS
    #[test]
    fn test_forget_removes_entry() {
        // GIVEN
        let (store, _) = setup();
        let sem = SemanticMemory::new(&store);
        let id = sem
            .remember(RememberInput {
                namespace: "ns",
                key: "old_key",
                value: &json!("val"),
                confidence: 1.0,
                source: None,
                expires_at: None,
            })
            .unwrap();
        // WHEN
        let removed = sem.forget("ns", "old_key").unwrap();
        // THEN
        assert!(removed);
        assert!(sem.recall("ns", "old_key").unwrap().is_none());
        // FTS also cleaned
        let fts_count: i64 = store
            .conn()
            .query_row(
                "SELECT COUNT(*) FROM memory_fts WHERE source_id = ?1",
                [&id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(fts_count, 0);
    }

    // recall nonexistent returns None
    #[test]
    fn test_recall_nonexistent_returns_none() {
        // GIVEN
        let (store, _) = setup();
        let sem = SemanticMemory::new(&store);
        // WHEN / THEN
        assert!(sem.recall("ns", "nope").unwrap().is_none());
    }

    // forget nonexistent returns false
    #[test]
    fn test_forget_nonexistent_returns_false() {
        // GIVEN
        let (store, _) = setup();
        let sem = SemanticMemory::new(&store);
        // WHEN / THEN
        assert!(!sem.forget("ns", "nope").unwrap());
    }

    // Validation: empty key rejected
    #[test]
    fn test_empty_key_rejected() {
        // GIVEN
        let (store, _) = setup();
        let sem = SemanticMemory::new(&store);
        // WHEN
        let result = sem.remember(RememberInput {
            namespace: "ns",
            key: "",
            value: &json!("val"),
            confidence: 1.0,
            source: None,
            expires_at: None,
        });
        // THEN
        assert!(matches!(result, Err(SemanticMemoryError::EmptyKey)));
    }

    // Validation: invalid confidence rejected
    #[test]
    fn test_invalid_confidence_rejected() {
        // GIVEN
        let (store, _) = setup();
        let sem = SemanticMemory::new(&store);
        // WHEN: negative
        let result = sem.remember(RememberInput {
            namespace: "ns",
            key: "k",
            value: &json!("v"),
            confidence: -0.1,
            source: None,
            expires_at: None,
        });
        // THEN
        assert!(matches!(
            result,
            Err(SemanticMemoryError::InvalidConfidence(_))
        ));
        // WHEN: too high
        let result = sem.remember(RememberInput {
            namespace: "ns",
            key: "k",
            value: &json!("v"),
            confidence: 1.1,
            source: None,
            expires_at: None,
        });
        // THEN
        assert!(matches!(
            result,
            Err(SemanticMemoryError::InvalidConfidence(_))
        ));
    }

    // Purge expired removes only expired entries
    #[test]
    fn test_purge_expired() {
        // GIVEN
        let (store, _) = setup();
        let sem = SemanticMemory::new(&store);
        let expired_id = sem
            .remember(RememberInput {
                namespace: "ns",
                key: "old_key",
                value: &json!("old"),
                confidence: 1.0,
                source: None,
                expires_at: Some("2020-01-01T00:00:00Z"),
            })
            .unwrap();
        sem.remember(RememberInput {
            namespace: "ns",
            key: "fresh_key",
            value: &json!("fresh"),
            confidence: 1.0,
            source: None,
            expires_at: None,
        })
        .unwrap();
        // WHEN
        let purged = sem.purge_expired("ns").unwrap();
        // THEN
        assert_eq!(purged, 1);
        assert!(sem.recall("ns", "old_key").unwrap().is_none());
        assert!(sem.recall("ns", "fresh_key").unwrap().is_some());
        // FTS also cleaned for expired
        let fts_count: i64 = store
            .conn()
            .query_row(
                "SELECT COUNT(*) FROM memory_fts WHERE source_id = ?1",
                [&expired_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(fts_count, 0);
    }

    // enforce_limit evicts least-recently-updated entries and cleans FTS
    #[test]
    fn test_enforce_limit_evicts_oldest_updated() {
        // GIVEN: 5 entries with explicit updated_at timestamps
        let (store, _) = setup();
        let sem = SemanticMemory::new(&store);
        let conn = store.conn();

        for i in 0..5 {
            let id = uuid::Uuid::new_v4().to_string();
            let key = format!("key.{i}");
            let value = format!("\"val{i}\"");
            conn.execute(
                "INSERT INTO semantic_memories (id, namespace, key, value, confidence, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                rusqlite::params![
                    id,
                    "ns",
                    key,
                    value,
                    1.0,
                    format!("2026-01-0{}T10:00:00Z", i + 1),
                    format!("2026-01-0{}T10:00:00Z", i + 1),
                ],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO memory_fts (content, source_table, source_id) VALUES (?1, ?2, ?3)",
                rusqlite::params![format!("{key} {value}"), "semantic", id],
            )
            .unwrap();
        }

        // WHEN: enforce limit of 3
        let evicted = sem.enforce_limit("ns", 3).unwrap();

        // THEN: 2 oldest evicted, 3 remain
        assert_eq!(evicted, 2);
        let remaining = sem.recall_all("ns", None).unwrap();
        assert_eq!(remaining.len(), 3);
        // key.0 and key.1 (oldest updated_at) should be gone
        for entry in &remaining {
            assert!(
                entry.key != "key.0" && entry.key != "key.1",
                "oldest entries should have been evicted"
            );
        }

        // FTS entries for evicted rows should also be gone
        let fts_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM memory_fts WHERE source_table = 'semantic'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(fts_count, 3);
    }

    // enforce_limit is no-op when under limit
    #[test]
    fn test_enforce_limit_noop_under_limit() {
        // GIVEN: 2 entries
        let (store, _) = setup();
        let sem = SemanticMemory::new(&store);
        sem.remember(RememberInput {
            namespace: "ns",
            key: "k1",
            value: &json!("v1"),
            confidence: 1.0,
            source: None,
            expires_at: None,
        })
        .unwrap();
        sem.remember(RememberInput {
            namespace: "ns",
            key: "k2",
            value: &json!("v2"),
            confidence: 1.0,
            source: None,
            expires_at: None,
        })
        .unwrap();

        // WHEN: enforce limit of 10
        let evicted = sem.enforce_limit("ns", 10).unwrap();

        // THEN: nothing evicted
        assert_eq!(evicted, 0);
        assert_eq!(sem.recall_all("ns", None).unwrap().len(), 2);
    }

    // upsert does NOT trigger eviction (only new inserts do)
    #[test]
    fn test_upsert_does_not_trigger_eviction() {
        // GIVEN: 3 entries at the limit
        let (store, _) = setup();
        let sem = SemanticMemory::new(&store);
        sem.remember(RememberInput {
            namespace: "ns",
            key: "k1",
            value: &json!("v1"),
            confidence: 1.0,
            source: None,
            expires_at: None,
        })
        .unwrap();
        sem.remember(RememberInput {
            namespace: "ns",
            key: "k2",
            value: &json!("v2"),
            confidence: 1.0,
            source: None,
            expires_at: None,
        })
        .unwrap();
        sem.remember(RememberInput {
            namespace: "ns",
            key: "k3",
            value: &json!("v3"),
            confidence: 1.0,
            source: None,
            expires_at: None,
        })
        .unwrap();

        // WHEN: upsert existing key (no new row)
        sem.remember(RememberInput {
            namespace: "ns",
            key: "k1",
            value: &json!("updated"),
            confidence: 0.9,
            source: None,
            expires_at: None,
        })
        .unwrap();

        // THEN: still 3 entries (no eviction happened)
        assert_eq!(sem.recall_all("ns", None).unwrap().len(), 3);
    }

    // Namespace isolation
    #[test]
    fn test_namespace_isolation() {
        // GIVEN
        let (store, _) = setup();
        let sem = SemanticMemory::new(&store);
        sem.remember(RememberInput {
            namespace: "ns-a",
            key: "key",
            value: &json!("val-a"),
            confidence: 1.0,
            source: None,
            expires_at: None,
        })
        .unwrap();
        sem.remember(RememberInput {
            namespace: "ns-b",
            key: "key",
            value: &json!("val-b"),
            confidence: 1.0,
            source: None,
            expires_at: None,
        })
        .unwrap();
        // WHEN
        let a = sem.recall("ns-a", "key").unwrap().unwrap();
        let b = sem.recall("ns-b", "key").unwrap().unwrap();
        // THEN
        assert_eq!(a.value, json!("val-a"));
        assert_eq!(b.value, json!("val-b"));
    }

    // recall_entry returns metadata
    #[test]
    fn test_recall_entry_returns_metadata() {
        // GIVEN
        let (store, _) = setup();
        let sem = SemanticMemory::new(&store);
        sem.remember(RememberInput {
            namespace: "ns",
            key: "bootstrap.meta",
            value: &json!("data"),
            confidence: 0.95,
            source: Some("agent"),
            expires_at: None,
        })
        .unwrap();
        // WHEN
        let entry = sem.recall_entry("ns", "bootstrap.meta").unwrap();
        // THEN
        let e = entry.expect("should be Some");
        assert_eq!(e.key, "bootstrap.meta");
        assert_eq!(e.value, json!("data"));
        assert!((e.confidence - 0.95).abs() < f64::EPSILON);
        assert_eq!(e.source, Some("agent".to_string()));
        assert!(!e.updated_at.is_empty());
    }

    // recall_entry returns None for missing key
    #[test]
    fn test_recall_entry_returns_none_for_missing_key() {
        // GIVEN
        let (store, _) = setup();
        let sem = SemanticMemory::new(&store);
        // WHEN
        let entry = sem.recall_entry("ns", "nonexistent").unwrap();
        // THEN
        assert!(entry.is_none());
    }

    // recall_all lists namespace entries
    #[test]
    fn test_recall_all_lists_namespace_entries() {
        // GIVEN
        let (store, _) = setup();
        let sem = SemanticMemory::new(&store);
        sem.remember(RememberInput {
            namespace: "ns",
            key: "k1",
            value: &json!("v1"),
            confidence: 1.0,
            source: None,
            expires_at: None,
        })
        .unwrap();
        sem.remember(RememberInput {
            namespace: "ns",
            key: "k2",
            value: &json!("v2"),
            confidence: 1.0,
            source: None,
            expires_at: None,
        })
        .unwrap();
        sem.remember(RememberInput {
            namespace: "ns",
            key: "k3",
            value: &json!("v3"),
            confidence: 1.0,
            source: None,
            expires_at: None,
        })
        .unwrap();
        // WHEN
        let entries = sem.recall_all("ns", None).unwrap();
        // THEN
        assert_eq!(entries.len(), 3);
    }

    // recall_all respects limit
    #[test]
    fn test_recall_all_respects_limit() {
        // GIVEN
        let (store, _) = setup();
        let sem = SemanticMemory::new(&store);
        for i in 0..10 {
            sem.remember(RememberInput {
                namespace: "ns",
                key: &format!("key.{i}"),
                value: &json!(i),
                confidence: 1.0,
                source: None,
                expires_at: None,
            })
            .unwrap();
        }
        // WHEN
        let entries = sem.recall_all("ns", Some(5)).unwrap();
        // THEN
        assert_eq!(entries.len(), 5);
    }

    // recall_entry excludes expired entries
    #[test]
    fn test_recall_entry_excludes_expired() {
        // GIVEN
        let (store, _) = setup();
        let sem = SemanticMemory::new(&store);
        sem.remember(RememberInput {
            namespace: "ns",
            key: "expired_key",
            value: &json!("old"),
            confidence: 1.0,
            source: None,
            expires_at: Some("2020-01-01T00:00:00Z"),
        })
        .unwrap();
        // WHEN
        let entry = sem.recall_entry("ns", "expired_key").unwrap();
        // THEN
        assert!(entry.is_none());
    }

    // recall_all excludes expired entries
    #[test]
    fn test_recall_all_excludes_expired() {
        // GIVEN
        let (store, _) = setup();
        let sem = SemanticMemory::new(&store);
        sem.remember(RememberInput {
            namespace: "ns",
            key: "fresh",
            value: &json!("ok"),
            confidence: 1.0,
            source: None,
            expires_at: None,
        })
        .unwrap();
        sem.remember(RememberInput {
            namespace: "ns",
            key: "expired",
            value: &json!("old"),
            confidence: 1.0,
            source: None,
            expires_at: Some("2020-01-01T00:00:00Z"),
        })
        .unwrap();
        // WHEN
        let entries = sem.recall_all("ns", None).unwrap();
        // THEN
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].key, "fresh");
    }
}
