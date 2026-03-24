//! SemanticMemory — base de connaissances structuree de l'agent.
//!
//! Chaque entree est une paire cle/valeur avec un score de confiance (0.0..=1.0),
//! une source d'origine optionnelle, et un TTL optionnel (`expires_at`).
//! Les cles sont hierarchiques par convention (`client.dupont.budget_max`).
//! L'agent est seul maitre de ce qui est enregistre (Principe #6).

use crate::store::MemoryStore;

/// Default maximum number of semantic entries per namespace.
/// Beyond this limit, least-recently-updated entries are evicted.
pub(crate) const DEFAULT_MAX_ENTRIES_PER_NAMESPACE: u64 = 5_000;

/// Backend de memoire semantique — base de connaissances structuree.
///
/// Stocke des paires cle/valeur avec confiance et source.
/// Les cles sont hierarchiques par convention (`client.dupont.budget_max`).
/// L'upsert est natif : `remember()` avec une cle existante met a jour.
pub struct SemanticMemory<'a> {
    store: &'a MemoryStore,
}

/// Entree retournee par [`SemanticMemory::recall`].
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SemanticEntry {
    /// UUID v4 unique de l'entree.
    pub id: String,
    /// Namespace de l'entree.
    pub namespace: String,
    /// Cle hierarchique (ex: `client.dupont.budget_max`).
    pub key: String,
    /// Valeur JSON de la connaissance.
    pub value: serde_json::Value,
    /// Source ayant produit cette connaissance.
    pub source: Option<String>,
    /// Score de confiance (0.0..=1.0).
    pub confidence: f64,
    /// Date de creation ISO 8601.
    pub created_at: String,
    /// Date de derniere mise a jour ISO 8601.
    pub updated_at: String,
    /// Date d'expiration optionnelle ISO 8601.
    pub expires_at: Option<String>,
}

/// Erreurs du backend semantique.
#[derive(Debug, thiserror::Error)]
pub enum SemanticMemoryError {
    /// L'insertion ou mise a jour d'une connaissance a echoue.
    #[error("failed to store knowledge: {0}")]
    StoreFailed(String),

    /// La requete de recall a echoue.
    #[error("failed to recall knowledge: {0}")]
    RecallFailed(String),

    /// Le score de confiance est hors de l'intervalle [0.0, 1.0].
    #[error("invalid confidence score: {0} (must be 0.0..=1.0)")]
    InvalidConfidence(f64),

    /// La cle est vide.
    #[error("empty key")]
    EmptyKey,

    /// Erreur SQLite propagee depuis rusqlite.
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
}

impl<'a> SemanticMemory<'a> {
    /// Cree un backend semantique lie a un [`MemoryStore`].
    pub fn new(store: &'a MemoryStore) -> Self {
        Self { store }
    }

    /// Stocke ou met a jour une connaissance.
    ///
    /// Si la cle existe deja dans le namespace, la valeur est mise a jour (upsert).
    /// Retourne l'UUID de l'entree creee ou mise a jour.
    pub fn remember(
        &self,
        namespace: &str,
        key: &str,
        value: &serde_json::Value,
        confidence: f64,
        source: Option<&str>,
        expires_at: Option<&str>,
    ) -> Result<String, SemanticMemoryError> {
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

    /// Recupere toutes les connaissances d'un namespace, triees par cle.
    ///
    /// Retourne un vecteur vide si le namespace n'a aucune entree.
    pub fn recall_all(&self, namespace: &str) -> Result<Vec<SemanticEntry>, SemanticMemoryError> {
        let conn = self.store.conn();

        let mut stmt = conn
            .prepare(
                "SELECT id, namespace, key, value, source, confidence, created_at, updated_at, expires_at
                 FROM semantic_memories
                 WHERE namespace = ?1
                 ORDER BY key ASC",
            )
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

    /// Recupere une connaissance par cle. `None` si absente.
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

    /// Supprime une connaissance. Retourne `true` si supprimee, `false` si absente.
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

    /// Supprime les connaissances expirees. Retourne le nombre purge.
    ///
    /// Nettoie aussi les entrees FTS correspondantes.
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
}

/// Returns the current UTC time as an ISO 8601 string.
///
/// Delegates to SQLite's `strftime` for consistency with datetime comparisons
/// used in queries (e.g. `purge_expired`).
fn chrono_now_utc() -> String {
    let conn = rusqlite::Connection::open_in_memory()
        .expect("in-memory SQLite connection should always succeed");
    conn.query_row("SELECT strftime('%Y-%m-%dT%H:%M:%SZ', 'now')", [], |row| {
        row.get(0)
    })
    .expect("strftime should always succeed")
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
    fn test_ac1_remember_stores_entry_and_fts() {
        // GIVEN
        let (store, _) = setup();
        let sem = SemanticMemory::new(&store);
        // WHEN
        let id = sem
            .remember(
                "ns",
                "client.dupont.budget",
                &json!(15000),
                1.0,
                Some("crm-agent"),
                None,
            )
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
    fn test_ac2_recall_existing_key() {
        // GIVEN
        let (store, _) = setup();
        let sem = SemanticMemory::new(&store);
        sem.remember(
            "ns",
            "client.dupont.budget",
            &json!(15000),
            0.9,
            Some("crm"),
            None,
        )
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
    fn test_ac3_upsert_updates_value() {
        // GIVEN
        let (store, _) = setup();
        let sem = SemanticMemory::new(&store);
        sem.remember("ns", "key", &json!("old"), 1.0, None, None)
            .unwrap();
        // WHEN
        sem.remember("ns", "key", &json!("new"), 0.8, None, None)
            .unwrap();
        // THEN
        let entry = sem.recall("ns", "key").unwrap().unwrap();
        assert_eq!(entry.value, json!("new"));
        assert_eq!(entry.confidence, 0.8);
    }

    // bis — upsert updates FTS entry
    #[test]
    fn test_ac3_upsert_updates_fts() {
        // GIVEN
        let (store, _) = setup();
        let sem = SemanticMemory::new(&store);
        let id = sem
            .remember("ns", "key", &json!("old_value"), 1.0, None, None)
            .unwrap();
        // WHEN
        sem.remember("ns", "key", &json!("new_value"), 0.8, None, None)
            .unwrap();
        // THEN — only one FTS entry for this source_id
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
    fn test_ac4_forget_removes_entry() {
        // GIVEN
        let (store, _) = setup();
        let sem = SemanticMemory::new(&store);
        let id = sem
            .remember("ns", "old_key", &json!("val"), 1.0, None, None)
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
    fn test_ac5_recall_nonexistent_returns_none() {
        // GIVEN
        let (store, _) = setup();
        let sem = SemanticMemory::new(&store);
        // WHEN / THEN
        assert!(sem.recall("ns", "nope").unwrap().is_none());
    }

    // forget nonexistent returns false
    #[test]
    fn test_ac6_forget_nonexistent_returns_false() {
        // GIVEN
        let (store, _) = setup();
        let sem = SemanticMemory::new(&store);
        // WHEN / THEN
        assert!(!sem.forget("ns", "nope").unwrap());
    }

    // Validation — empty key rejected
    #[test]
    fn test_empty_key_rejected() {
        // GIVEN
        let (store, _) = setup();
        let sem = SemanticMemory::new(&store);
        // WHEN
        let result = sem.remember("ns", "", &json!("val"), 1.0, None, None);
        // THEN
        assert!(matches!(result, Err(SemanticMemoryError::EmptyKey)));
    }

    // Validation — invalid confidence rejected
    #[test]
    fn test_invalid_confidence_rejected() {
        // GIVEN
        let (store, _) = setup();
        let sem = SemanticMemory::new(&store);
        // WHEN — negative
        let result = sem.remember("ns", "k", &json!("v"), -0.1, None, None);
        // THEN
        assert!(matches!(
            result,
            Err(SemanticMemoryError::InvalidConfidence(_))
        ));
        // WHEN — too high
        let result = sem.remember("ns", "k", &json!("v"), 1.1, None, None);
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
            .remember(
                "ns",
                "old_key",
                &json!("old"),
                1.0,
                None,
                Some("2020-01-01T00:00:00Z"),
            )
            .unwrap();
        sem.remember("ns", "fresh_key", &json!("fresh"), 1.0, None, None)
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

    // DT-018 — enforce_limit evicts least-recently-updated entries and cleans FTS
    #[test]
    fn test_enforce_limit_evicts_oldest_updated() {
        // GIVEN — 5 entries with explicit updated_at timestamps
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

        // WHEN — enforce limit of 3
        let evicted = sem.enforce_limit("ns", 3).unwrap();

        // THEN — 2 oldest evicted, 3 remain
        assert_eq!(evicted, 2);
        let remaining = sem.recall_all("ns").unwrap();
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

    // DT-018 — enforce_limit is no-op when under limit
    #[test]
    fn test_enforce_limit_noop_under_limit() {
        // GIVEN — 2 entries
        let (store, _) = setup();
        let sem = SemanticMemory::new(&store);
        sem.remember("ns", "k1", &json!("v1"), 1.0, None, None)
            .unwrap();
        sem.remember("ns", "k2", &json!("v2"), 1.0, None, None)
            .unwrap();

        // WHEN — enforce limit of 10
        let evicted = sem.enforce_limit("ns", 10).unwrap();

        // THEN — nothing evicted
        assert_eq!(evicted, 0);
        assert_eq!(sem.recall_all("ns").unwrap().len(), 2);
    }

    // DT-018 — upsert does NOT trigger eviction (only new inserts do)
    #[test]
    fn test_upsert_does_not_trigger_eviction() {
        // GIVEN — 3 entries at the limit
        let (store, _) = setup();
        let sem = SemanticMemory::new(&store);
        sem.remember("ns", "k1", &json!("v1"), 1.0, None, None)
            .unwrap();
        sem.remember("ns", "k2", &json!("v2"), 1.0, None, None)
            .unwrap();
        sem.remember("ns", "k3", &json!("v3"), 1.0, None, None)
            .unwrap();

        // WHEN — upsert existing key (no new row)
        sem.remember("ns", "k1", &json!("updated"), 0.9, None, None)
            .unwrap();

        // THEN — still 3 entries (no eviction happened)
        assert_eq!(sem.recall_all("ns").unwrap().len(), 3);
    }

    // Namespace isolation
    #[test]
    fn test_namespace_isolation() {
        // GIVEN
        let (store, _) = setup();
        let sem = SemanticMemory::new(&store);
        sem.remember("ns-a", "key", &json!("val-a"), 1.0, None, None)
            .unwrap();
        sem.remember("ns-b", "key", &json!("val-b"), 1.0, None, None)
            .unwrap();
        // WHEN
        let a = sem.recall("ns-a", "key").unwrap().unwrap();
        let b = sem.recall("ns-b", "key").unwrap().unwrap();
        // THEN
        assert_eq!(a.value, json!("val-a"));
        assert_eq!(b.value, json!("val-b"));
    }
}
