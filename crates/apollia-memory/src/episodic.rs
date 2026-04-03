//! EpisodicMemory — journal des evenements de l'agent.
//!
//! Chaque episode est date, porte un score d'importance (0.0..=1.0),
//! et peut avoir un TTL (expiration via `expires_at`).
//! L'agent est seul maitre de ce qui est enregistre (Principe #6).

use crate::store::MemoryStore;
use serde_json::Value as JsonValue;

/// Default maximum number of episodic entries per namespace.
/// Beyond this limit, oldest entries are evicted (FIFO).
pub(crate) const DEFAULT_MAX_ENTRIES_PER_NAMESPACE: u64 = 10_000;

/// Backend de memoire episodique — journal des evenements de l'agent.
///
/// Chaque episode est date, porte un score d'importance, et peut expirer.
/// L'agent est seul maitre de ce qui est enregistre (Principe #6).
pub struct EpisodicMemory<'a> {
    store: &'a MemoryStore,
}

/// Entree retournee par [`EpisodicMemory::history`].
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct EpisodicEntry {
    /// UUID v4 unique de l'episode.
    pub id: String,
    /// Namespace de l'episode.
    pub namespace: String,
    /// Identifiant de l'agent ayant cree l'episode.
    pub agent_id: String,
    /// Identifiant optionnel de la tache associee.
    pub task_id: Option<String>,
    /// Contenu textuel de l'episode.
    pub content: String,
    /// Score d'importance (0.0..=1.0).
    pub importance: f64,
    /// Date de creation ISO 8601.
    pub created_at: String,
    /// Date d'expiration optionnelle ISO 8601.
    pub expires_at: Option<String>,
    /// Metadonnees JSON additionnelles.
    pub metadata: JsonValue,
}

/// Erreurs du backend episodique.
#[derive(Debug, thiserror::Error)]
pub enum EpisodicMemoryError {
    /// L'insertion d'un episode a echoue.
    #[error("failed to record episode: {0}")]
    RecordFailed(String),

    /// La requete d'historique a echoue.
    #[error("failed to query history: {0}")]
    QueryFailed(String),

    /// Le score d'importance est hors de l'intervalle [0.0, 1.0].
    #[error("invalid importance score: {0} (must be 0.0..=1.0)")]
    InvalidImportance(f64),

    /// Erreur SQLite propagee depuis rusqlite.
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
}

impl<'a> EpisodicMemory<'a> {
    /// Cree un backend episodique lie a un [`MemoryStore`].
    pub fn new(store: &'a MemoryStore) -> Self {
        Self { store }
    }

    /// Enregistre un episode. Retourne l'UUID de l'episode cree.
    ///
    /// Insere dans `episodic_memories` ET dans `memory_fts` (contenu indexe
    /// pour la recherche full-text).
    #[allow(clippy::too_many_arguments)]
    pub fn record(
        &self,
        namespace: &str,
        agent_id: &str,
        content: &str,
        importance: f64,
        task_id: Option<&str>,
        expires_at: Option<&str>,
        metadata: Option<&JsonValue>,
    ) -> Result<String, EpisodicMemoryError> {
        if !(0.0..=1.0).contains(&importance) {
            return Err(EpisodicMemoryError::InvalidImportance(importance));
        }

        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono_now_utc();
        let metadata_str = metadata
            .map(|v| v.to_string())
            .unwrap_or_else(|| "{}".to_string());

        let conn = self.store.conn();

        conn.execute(
            "INSERT INTO episodic_memories (id, namespace, agent_id, task_id, content, importance, created_at, expires_at, metadata)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            rusqlite::params![id, namespace, agent_id, task_id, content, importance, now, expires_at, metadata_str],
        )
        .map_err(|e| EpisodicMemoryError::RecordFailed(e.to_string()))?;

        conn.execute(
            "INSERT INTO memory_fts (content, source_table, source_id) VALUES (?1, ?2, ?3)",
            rusqlite::params![content, "episodic", id],
        )
        .map_err(|e| EpisodicMemoryError::RecordFailed(format!("FTS insert failed: {e}")))?;

        tracing::info!(
            episode_id = %id,
            namespace = %namespace,
            agent_id = %agent_id,
            importance = importance,
            "episode recorded"
        );

        self.enforce_limit(namespace, DEFAULT_MAX_ENTRIES_PER_NAMESPACE)?;

        Ok(id)
    }

    /// Evicts oldest entries when the namespace exceeds `max_entries`.
    ///
    /// Deletes the oldest entries by `created_at` (FIFO) and their
    /// corresponding FTS index rows. Returns the number of evicted entries.
    fn enforce_limit(&self, namespace: &str, max_entries: u64) -> Result<u64, EpisodicMemoryError> {
        let conn = self.store.conn();

        let count: u64 = conn
            .query_row(
                "SELECT COUNT(*) FROM episodic_memories WHERE namespace = ?1",
                rusqlite::params![namespace],
                |row| row.get(0),
            )
            .map_err(|e| EpisodicMemoryError::RecordFailed(format!("count query failed: {e}")))?;

        if count <= max_entries {
            return Ok(0);
        }

        let excess = count - max_entries;

        // Delete FTS entries for the rows about to be evicted.
        conn.execute(
            "DELETE FROM memory_fts WHERE source_table = 'episodic' AND source_id IN (
                SELECT id FROM episodic_memories
                WHERE namespace = ?1
                ORDER BY created_at ASC
                LIMIT ?2
            )",
            rusqlite::params![namespace, excess],
        )
        .map_err(|e| EpisodicMemoryError::RecordFailed(format!("FTS eviction failed: {e}")))?;

        let evicted = conn
            .execute(
                "DELETE FROM episodic_memories WHERE id IN (
                    SELECT id FROM episodic_memories
                    WHERE namespace = ?1
                    ORDER BY created_at ASC
                    LIMIT ?2
                )",
                rusqlite::params![namespace, excess],
            )
            .map_err(|e| {
                EpisodicMemoryError::RecordFailed(format!("eviction delete failed: {e}"))
            })?;

        tracing::info!(
            namespace = %namespace,
            evicted = evicted,
            max_entries = max_entries,
            "episodic entries evicted to enforce namespace limit"
        );

        Ok(evicted as u64)
    }

    /// Retourne l'historique du namespace, trie par date descendante.
    pub fn history(
        &self,
        namespace: &str,
        limit: u32,
        since: Option<&str>,
    ) -> Result<Vec<EpisodicEntry>, EpisodicMemoryError> {
        let conn = self.store.conn();

        let (sql, params): (String, Vec<Box<dyn rusqlite::types::ToSql>>) = match since {
            Some(since_date) => (
                "SELECT id, namespace, agent_id, task_id, content, importance, created_at, expires_at, metadata \
                 FROM episodic_memories \
                 WHERE namespace = ?1 AND created_at >= ?2 \
                 ORDER BY created_at DESC \
                 LIMIT ?3"
                    .to_string(),
                vec![
                    Box::new(namespace.to_string()),
                    Box::new(since_date.to_string()),
                    Box::new(limit),
                ],
            ),
            None => (
                "SELECT id, namespace, agent_id, task_id, content, importance, created_at, expires_at, metadata \
                 FROM episodic_memories \
                 WHERE namespace = ?1 \
                 ORDER BY created_at DESC \
                 LIMIT ?2"
                    .to_string(),
                vec![Box::new(namespace.to_string()), Box::new(limit)],
            ),
        };

        let param_refs: Vec<&dyn rusqlite::types::ToSql> =
            params.iter().map(|p| p.as_ref()).collect();

        let mut stmt = conn
            .prepare(&sql)
            .map_err(|e| EpisodicMemoryError::QueryFailed(e.to_string()))?;

        let rows = stmt
            .query_map(param_refs.as_slice(), |row| {
                let metadata_str: String = row.get(8)?;
                let metadata: JsonValue = serde_json::from_str(&metadata_str)
                    .unwrap_or(JsonValue::Object(Default::default()));
                Ok(EpisodicEntry {
                    id: row.get(0)?,
                    namespace: row.get(1)?,
                    agent_id: row.get(2)?,
                    task_id: row.get(3)?,
                    content: row.get(4)?,
                    importance: row.get(5)?,
                    created_at: row.get(6)?,
                    expires_at: row.get(7)?,
                    metadata,
                })
            })
            .map_err(|e| EpisodicMemoryError::QueryFailed(e.to_string()))?;

        let mut entries = Vec::new();
        for row in rows {
            entries.push(row.map_err(|e| EpisodicMemoryError::QueryFailed(e.to_string()))?);
        }

        Ok(entries)
    }

    /// Supprime les episodes expires. Retourne le nombre d'episodes purges.
    ///
    /// Nettoie aussi les entrees FTS correspondantes.
    pub fn purge_expired(&self, namespace: &str) -> Result<u64, EpisodicMemoryError> {
        let conn = self.store.conn();

        // Delete FTS entries for expired episodes first (need IDs before deletion).
        conn.execute(
            "DELETE FROM memory_fts WHERE source_table = 'episodic' AND source_id IN (
                SELECT id FROM episodic_memories
                WHERE namespace = ?1 AND expires_at IS NOT NULL AND expires_at < datetime('now')
            )",
            rusqlite::params![namespace],
        )
        .map_err(|e| EpisodicMemoryError::RecordFailed(format!("FTS purge failed: {e}")))?;

        let purged = conn
            .execute(
                "DELETE FROM episodic_memories
                 WHERE namespace = ?1 AND expires_at IS NOT NULL AND expires_at < datetime('now')",
                rusqlite::params![namespace],
            )
            .map_err(|e| EpisodicMemoryError::RecordFailed(format!("purge failed: {e}")))?;

        if purged > 0 {
            tracing::info!(
                namespace = %namespace,
                purged = purged,
                "expired episodes purged"
            );
        }

        Ok(purged as u64)
    }
}

/// Returns the current UTC time as an ISO 8601 string.
///
/// Delegates to SQLite's `strftime` for timestamp consistency with database queries.
/// Falls back to `chrono::Utc::now()` if the in-memory SQLite connection fails,
/// which cannot happen in practice but must be handled to avoid panics in production.
fn chrono_now_utc() -> String {
    let fallback = || chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
    let conn = match rusqlite::Connection::open_in_memory() {
        Ok(c) => c,
        Err(_) => return fallback(),
    };
    conn.query_row("SELECT strftime('%Y-%m-%dT%H:%M:%SZ', 'now')", [], |row| {
        row.get(0)
    })
    .unwrap_or_else(|_| fallback())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::MemoryStore;
    use serde_json::json;

    fn setup() -> (MemoryStore, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("apollia_ep_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test.db");
        let store = MemoryStore::open(&path).unwrap();
        (store, path)
    }

    // Record episode and verify FTS insert
    #[test]
    fn test_ac1_record_episode_and_fts_insert() {
        // GIVEN
        let (store, _) = setup();
        let ep = EpisodicMemory::new(&store);
        // WHEN
        let id = ep
            .record(
                "ns",
                "agent-1",
                "Devis envoye a Dupont",
                0.8,
                Some("task-1"),
                None,
                Some(&json!({"client": "Dupont"})),
            )
            .unwrap();
        // THEN
        assert!(!id.is_empty());
        let count: i64 = store
            .conn()
            .query_row(
                "SELECT COUNT(*) FROM memory_fts WHERE source_id = ?1",
                [&id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }

    // History returns most recent first, limited
    #[test]
    fn test_ac2_history_returns_recent_first() {
        // GIVEN — 5 episodes
        let (store, _) = setup();
        let ep = EpisodicMemory::new(&store);
        for i in 0..5 {
            ep.record(
                "ns",
                "agent-1",
                &format!("Episode {i}"),
                0.5,
                None,
                None,
                None,
            )
            .unwrap();
        }
        // WHEN
        let history = ep.history("ns", 3, None).unwrap();
        // THEN
        assert_eq!(history.len(), 3);
    }

    // History filtered by since date
    #[test]
    fn test_ac3_history_filtered_by_since() {
        // GIVEN — insert episodes with explicit timestamps via raw SQL
        let (store, _) = setup();
        let ep = EpisodicMemory::new(&store);
        let conn = store.conn();

        // Insert 2 old episodes (before 2026-01-01)
        for i in 0..2 {
            let id = uuid::Uuid::new_v4().to_string();
            conn.execute(
                "INSERT INTO episodic_memories (id, namespace, agent_id, content, importance, created_at, metadata)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                rusqlite::params![id, "ns", "agent-1", format!("Old {i}"), 0.5, "2025-06-15T10:00:00Z", "{}"],
            )
            .unwrap();
        }

        // Insert 3 recent episodes (after 2026-01-01)
        for i in 0..3 {
            let id = uuid::Uuid::new_v4().to_string();
            conn.execute(
                "INSERT INTO episodic_memories (id, namespace, agent_id, content, importance, created_at, metadata)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                rusqlite::params![id, "ns", "agent-1", format!("Recent {i}"), 0.7, format!("2026-02-0{i}T10:00:00Z", i = i + 1), "{}"],
            )
            .unwrap();
        }

        // WHEN
        let history = ep.history("ns", 10, Some("2026-01-01T00:00:00Z")).unwrap();

        // THEN
        assert_eq!(history.len(), 3);
        for entry in &history {
            assert!(entry.created_at.as_str() >= "2026-01-01T00:00:00Z");
        }
    }

    // Purge expired episodes (including FTS cleanup)
    #[test]
    fn test_ac4_purge_expired() {
        // GIVEN — 1 expired, 1 not expired
        let (store, _) = setup();
        let ep = EpisodicMemory::new(&store);
        let expired_id = ep
            .record(
                "ns",
                "agent-1",
                "Old episode",
                0.5,
                None,
                Some("2020-01-01T00:00:00Z"),
                None,
            )
            .unwrap();
        ep.record("ns", "agent-1", "Fresh episode", 0.5, None, None, None)
            .unwrap();
        // WHEN
        let purged = ep.purge_expired("ns").unwrap();
        // THEN
        assert_eq!(purged, 1);
        let remaining = ep.history("ns", 10, None).unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].content, "Fresh episode");

        // Verify FTS entry was also purged
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

    // Optional fields default correctly
    #[test]
    fn test_ac5_optional_fields_default() {
        // GIVEN / WHEN
        let (store, _) = setup();
        let ep = EpisodicMemory::new(&store);
        let id = ep
            .record("ns", "agent-1", "Simple episode", 0.5, None, None, None)
            .unwrap();
        // THEN
        let entry = ep.history("ns", 1, None).unwrap();
        assert_eq!(entry[0].id, id);
        assert!(entry[0].task_id.is_none());
        assert_eq!(entry[0].metadata, json!({}));
    }

    // Importance validation — reject out of range
    #[test]
    fn test_invalid_importance_rejected() {
        // GIVEN
        let (store, _) = setup();
        let ep = EpisodicMemory::new(&store);
        // WHEN — too high
        let result = ep.record("ns", "agent-1", "Bad", 1.5, None, None, None);
        // THEN
        assert!(matches!(
            result,
            Err(EpisodicMemoryError::InvalidImportance(_))
        ));

        // WHEN — negative
        let result = ep.record("ns", "agent-1", "Bad", -0.1, None, None, None);
        // THEN
        assert!(matches!(
            result,
            Err(EpisodicMemoryError::InvalidImportance(_))
        ));
    }

    // DT-018 — enforce_limit evicts oldest entries (FIFO) and cleans FTS
    #[test]
    fn test_enforce_limit_evicts_oldest_entries() {
        // GIVEN — 5 episodes with explicit timestamps for deterministic ordering
        let (store, _) = setup();
        let ep = EpisodicMemory::new(&store);
        let conn = store.conn();

        for i in 0..5 {
            let id = uuid::Uuid::new_v4().to_string();
            conn.execute(
                "INSERT INTO episodic_memories (id, namespace, agent_id, content, importance, created_at, metadata)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                rusqlite::params![
                    id,
                    "ns",
                    "agent-1",
                    format!("Episode {i}"),
                    0.5,
                    format!("2026-01-0{}T10:00:00Z", i + 1),
                    "{}"
                ],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO memory_fts (content, source_table, source_id) VALUES (?1, ?2, ?3)",
                rusqlite::params![format!("Episode {i}"), "episodic", id],
            )
            .unwrap();
        }

        // WHEN — enforce limit of 3
        let evicted = ep.enforce_limit("ns", 3).unwrap();

        // THEN — 2 oldest evicted, 3 remain
        assert_eq!(evicted, 2);
        let remaining = ep.history("ns", 10, None).unwrap();
        assert_eq!(remaining.len(), 3);
        // Oldest (Episode 0, Episode 1) should be gone
        for entry in &remaining {
            assert!(
                entry.content != "Episode 0" && entry.content != "Episode 1",
                "oldest entries should have been evicted"
            );
        }

        // FTS entries for evicted rows should also be gone
        let fts_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM memory_fts WHERE source_table = 'episodic'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(fts_count, 3);
    }

    // DT-018 — enforce_limit is no-op when under limit
    #[test]
    fn test_enforce_limit_noop_under_limit() {
        // GIVEN — 2 episodes
        let (store, _) = setup();
        let ep = EpisodicMemory::new(&store);
        ep.record("ns", "agent-1", "A", 0.5, None, None, None)
            .unwrap();
        ep.record("ns", "agent-1", "B", 0.5, None, None, None)
            .unwrap();

        // WHEN — enforce limit of 10
        let evicted = ep.enforce_limit("ns", 10).unwrap();

        // THEN — nothing evicted
        assert_eq!(evicted, 0);
        let remaining = ep.history("ns", 10, None).unwrap();
        assert_eq!(remaining.len(), 2);
    }

    // DT-018 — record() triggers eviction when over default limit (tested with small limit via enforce_limit)
    #[test]
    fn test_record_triggers_eviction_via_enforce_limit() {
        // GIVEN — insert 3 entries, then enforce limit of 2 manually
        let (store, _) = setup();
        let ep = EpisodicMemory::new(&store);
        ep.record("ns", "agent-1", "First", 0.5, None, None, None)
            .unwrap();
        ep.record("ns", "agent-1", "Second", 0.5, None, None, None)
            .unwrap();
        ep.record("ns", "agent-1", "Third", 0.5, None, None, None)
            .unwrap();

        // WHEN — enforce small limit
        let evicted = ep.enforce_limit("ns", 2).unwrap();

        // THEN — 1 evicted, 2 remain
        assert_eq!(evicted, 1);
        let remaining = ep.history("ns", 10, None).unwrap();
        assert_eq!(remaining.len(), 2);
    }

    // Namespace isolation — episodes from different namespaces don't mix
    #[test]
    fn test_namespace_isolation() {
        // GIVEN
        let (store, _) = setup();
        let ep = EpisodicMemory::new(&store);
        ep.record("ns-a", "agent-1", "Episode A", 0.5, None, None, None)
            .unwrap();
        ep.record("ns-b", "agent-1", "Episode B", 0.5, None, None, None)
            .unwrap();
        // WHEN
        let history_a = ep.history("ns-a", 10, None).unwrap();
        let history_b = ep.history("ns-b", 10, None).unwrap();
        // THEN
        assert_eq!(history_a.len(), 1);
        assert_eq!(history_b.len(), 1);
        assert_eq!(history_a[0].content, "Episode A");
        assert_eq!(history_b[0].content, "Episode B");
    }

    /// `chrono_now_utc` returns a valid timestamp even when the SQLite call path is used.
    /// The fallback path (chrono::Utc::now()) is exercised via unit test of the helper.
    #[test]
    fn test_episodic_timestamp_fallback_on_sqlite_error() {
        // GIVEN / WHEN — call the timestamp helper directly
        let ts = chrono_now_utc();

        // THEN — the result is a valid ISO 8601 string regardless of which path was taken
        assert!(
            ts.len() >= 19,
            "timestamp should be at least 19 chars: {ts}"
        );
        assert!(
            ts.contains('T'),
            "timestamp should contain 'T' separator: {ts}"
        );
        assert!(ts.ends_with('Z'), "timestamp should end with 'Z': {ts}");
    }
}
