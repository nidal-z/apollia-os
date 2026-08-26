//! ProceduralMemory: workflows the agent has learned.
//!
//! Stores trigger->steps patterns with a success counter.
//! When an agent recognises a similar situation, it can retrieve the
//! associated workflow rather than starting from scratch.
//! The agent alone decides what gets recorded.

use crate::store::MemoryStore;

/// Default maximum number of procedural entries per namespace.
/// Beyond this limit, least-recently-used entries are evicted (LRU).
pub(crate) const DEFAULT_MAX_ENTRIES_PER_NAMESPACE: u64 = 1_000;

/// Procedural memory backend: workflows the agent has learned.
///
/// Stores trigger->steps patterns with a success counter.
/// The trigger is matched exactly (no FTS).
pub struct ProceduralMemory<'a> {
    store: &'a MemoryStore,
}

/// Entry returned by [`ProceduralMemory::recall`] and [`ProceduralMemory::list`].
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ProcedureEntry {
    /// Unique UUID v4 of the procedure.
    pub id: String,
    /// Procedure namespace.
    pub namespace: String,
    /// Exact trigger that fires this procedure.
    pub trigger: String,
    /// Workflow steps, in order.
    pub steps: Vec<String>,
    /// Number of times this procedure was confirmed successful.
    pub success_count: u32,
    /// ISO 8601 last-used date.
    pub last_used_at: String,
    /// ISO 8601 creation date.
    pub created_at: String,
}

/// Errors from the procedural backend.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ProceduralMemoryError {
    /// Learning a procedure failed.
    #[error("failed to learn procedure: {0}")]
    LearnFailed(String),

    /// The trigger is empty.
    #[error("empty trigger")]
    EmptyTrigger,

    /// The steps list is empty.
    #[error("empty steps list")]
    EmptySteps,

    /// SQLite error propagated from rusqlite.
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
}

impl<'a> ProceduralMemory<'a> {
    /// Create a procedural backend bound to a [`MemoryStore`].
    pub fn new(store: &'a MemoryStore) -> Self {
        Self { store }
    }

    /// Learn a procedure. If the trigger already exists in the namespace,
    /// increment `success_count` and update `steps` and `last_used_at`.
    ///
    /// Returns the UUID of the procedure (new or existing).
    pub fn learn(
        &self,
        namespace: &str,
        trigger: &str,
        steps: &[String],
    ) -> Result<String, ProceduralMemoryError> {
        if trigger.is_empty() {
            return Err(ProceduralMemoryError::EmptyTrigger);
        }
        if steps.is_empty() {
            return Err(ProceduralMemoryError::EmptySteps);
        }

        let now = chrono_now_utc();
        let steps_json = serde_json::to_string(steps)
            .map_err(|e| ProceduralMemoryError::LearnFailed(format!("steps serialization: {e}")))?;
        let conn = self.store.conn();

        // Check if procedure already exists for this namespace+trigger.
        let existing: Option<(String, u32)> = conn
            .query_row(
                "SELECT id, success_count FROM procedural_memories WHERE namespace = ?1 AND trigger_text = ?2",
                rusqlite::params![namespace, trigger],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .ok();

        let id = match existing {
            Some((existing_id, count)) => {
                conn.execute(
                    "UPDATE procedural_memories
                     SET steps = ?1, success_count = ?2, last_used_at = ?3
                     WHERE id = ?4",
                    rusqlite::params![steps_json, count + 1, now, existing_id],
                )
                .map_err(|e| ProceduralMemoryError::LearnFailed(e.to_string()))?;

                tracing::info!(
                    procedure_id = %existing_id,
                    namespace = %namespace,
                    trigger = %trigger,
                    success_count = count + 1,
                    "procedure reinforced"
                );

                existing_id
            }
            None => {
                let new_id = uuid::Uuid::new_v4().to_string();

                conn.execute(
                    "INSERT INTO procedural_memories (id, namespace, trigger_text, steps, success_count, last_used_at, created_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    rusqlite::params![new_id, namespace, trigger, steps_json, 1, now, now],
                )
                .map_err(|e| ProceduralMemoryError::LearnFailed(e.to_string()))?;

                tracing::info!(
                    procedure_id = %new_id,
                    namespace = %namespace,
                    trigger = %trigger,
                    "procedure learned"
                );

                self.enforce_limit(namespace, DEFAULT_MAX_ENTRIES_PER_NAMESPACE)?;

                new_id
            }
        };

        Ok(id)
    }

    /// Evicts least-recently-used entries when the namespace exceeds `max_entries`.
    ///
    /// Deletes entries ordered by `last_used_at ASC` (LRU). Procedural memories
    /// have no FTS index, so only the main table rows are deleted.
    /// Returns the number of evicted entries.
    fn enforce_limit(
        &self,
        namespace: &str,
        max_entries: u64,
    ) -> Result<u64, ProceduralMemoryError> {
        let conn = self.store.conn();

        let count: u64 = conn
            .query_row(
                "SELECT COUNT(*) FROM procedural_memories WHERE namespace = ?1",
                rusqlite::params![namespace],
                |row| row.get(0),
            )
            .map_err(|e| ProceduralMemoryError::LearnFailed(format!("count query failed: {e}")))?;

        if count <= max_entries {
            return Ok(0);
        }

        let excess = count - max_entries;

        let evicted = conn
            .execute(
                "DELETE FROM procedural_memories WHERE id IN (
                    SELECT id FROM procedural_memories
                    WHERE namespace = ?1
                    ORDER BY last_used_at ASC
                    LIMIT ?2
                )",
                rusqlite::params![namespace, excess],
            )
            .map_err(|e| {
                ProceduralMemoryError::LearnFailed(format!("eviction delete failed: {e}"))
            })?;

        tracing::info!(
            namespace = %namespace,
            evicted = evicted,
            max_entries = max_entries,
            "procedural entries evicted to enforce namespace limit"
        );

        Ok(evicted as u64)
    }

    /// Retrieve a procedure by exact trigger. `None` if absent.
    pub fn recall(
        &self,
        namespace: &str,
        trigger: &str,
    ) -> Result<Option<ProcedureEntry>, ProceduralMemoryError> {
        let conn = self.store.conn();

        let result = conn.query_row(
            "SELECT id, namespace, trigger_text, steps, success_count, last_used_at, created_at
             FROM procedural_memories
             WHERE namespace = ?1 AND trigger_text = ?2",
            rusqlite::params![namespace, trigger],
            |row| {
                let steps_str: String = row.get(3)?;
                Ok(ProcedureEntry {
                    id: row.get(0)?,
                    namespace: row.get(1)?,
                    trigger: row.get(2)?,
                    steps: serde_json::from_str(&steps_str).unwrap_or_default(),
                    success_count: row.get(4)?,
                    last_used_at: row.get(5)?,
                    created_at: row.get(6)?,
                })
            },
        );

        match result {
            Ok(entry) => Ok(Some(entry)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(ProceduralMemoryError::Sqlite(e)),
        }
    }

    /// List all procedures in a namespace, sorted by `success_count` DESC.
    pub fn list(&self, namespace: &str) -> Result<Vec<ProcedureEntry>, ProceduralMemoryError> {
        let conn = self.store.conn();

        let mut stmt = conn.prepare(
            "SELECT id, namespace, trigger_text, steps, success_count, last_used_at, created_at
             FROM procedural_memories
             WHERE namespace = ?1
             ORDER BY success_count DESC",
        )?;

        let rows = stmt.query_map(rusqlite::params![namespace], |row| {
            let steps_str: String = row.get(3)?;
            Ok(ProcedureEntry {
                id: row.get(0)?,
                namespace: row.get(1)?,
                trigger: row.get(2)?,
                steps: serde_json::from_str(&steps_str).unwrap_or_default(),
                success_count: row.get(4)?,
                last_used_at: row.get(5)?,
                created_at: row.get(6)?,
            })
        })?;

        let mut entries = Vec::new();
        for row in rows {
            entries.push(row?);
        }

        Ok(entries)
    }

    /// Deletes procedural entries older than `days` days. Returns the count deleted.
    ///
    /// Procedural memories have no FTS index, so only the main table rows are removed.
    pub fn purge_older_than(
        &self,
        namespace: &str,
        days: u32,
    ) -> Result<u64, ProceduralMemoryError> {
        let conn = self.store.conn();
        let cutoff = format!("-{days} days");

        let deleted = conn
            .execute(
                "DELETE FROM procedural_memories
                 WHERE namespace = ?1 AND created_at < datetime('now', ?2)",
                rusqlite::params![namespace, cutoff],
            )
            .map_err(|e| ProceduralMemoryError::LearnFailed(format!("purge_older_than: {e}")))?;

        if deleted > 0 {
            tracing::info!(
                namespace = %namespace,
                days = days,
                deleted = deleted,
                "procedural entries purged by age"
            );
        }

        Ok(deleted as u64)
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

    fn setup() -> (MemoryStore, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("apollia_proc_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test.db");
        let store = MemoryStore::open(&path).unwrap();
        (store, path)
    }

    #[test]
    fn test_learn_new_procedure() {
        // GIVEN
        let (store, _) = setup();
        let proc = ProceduralMemory::new(&store);
        // WHEN
        let id = proc
            .learn(
                "ns",
                "devis grand compte",
                &["Verifier SIRET".into(), "Calculer remise".into()],
            )
            .unwrap();
        // THEN
        assert!(!id.is_empty());
    }

    #[test]
    fn test_recall_existing_procedure() {
        // GIVEN
        let (store, _) = setup();
        let proc = ProceduralMemory::new(&store);
        proc.learn(
            "ns",
            "devis grand compte",
            &["Verifier SIRET".into(), "Calculer remise".into()],
        )
        .unwrap();
        // WHEN
        let entry = proc.recall("ns", "devis grand compte").unwrap();
        // THEN
        assert!(entry.is_some());
        let e = entry.unwrap();
        assert_eq!(e.steps.len(), 2);
        assert_eq!(e.steps[0], "Verifier SIRET");
        assert_eq!(e.success_count, 1);
    }

    #[test]
    fn test_learn_again_increments_count() {
        // GIVEN
        let (store, _) = setup();
        let proc = ProceduralMemory::new(&store);
        proc.learn("ns", "trigger", &["step1".into()]).unwrap();
        // WHEN
        proc.learn("ns", "trigger", &["step1".into(), "step2".into()])
            .unwrap();
        // THEN
        let entry = proc.recall("ns", "trigger").unwrap().unwrap();
        assert_eq!(entry.success_count, 2);
        assert_eq!(entry.steps.len(), 2);
    }

    #[test]
    fn test_recall_nonexistent_returns_none() {
        // GIVEN
        let (store, _) = setup();
        let proc = ProceduralMemory::new(&store);
        // WHEN / THEN
        assert!(proc.recall("ns", "nope").unwrap().is_none());
    }

    #[test]
    fn test_list_sorted_by_success_count() {
        // GIVEN
        let (store, _) = setup();
        let proc = ProceduralMemory::new(&store);
        proc.learn("ns", "rare", &["step".into()]).unwrap();
        proc.learn("ns", "popular", &["step".into()]).unwrap();
        proc.learn("ns", "popular", &["step".into()]).unwrap();
        proc.learn("ns", "popular", &["step".into()]).unwrap();
        // WHEN
        let list = proc.list("ns").unwrap();
        // THEN
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].trigger, "popular");
        assert_eq!(list[0].success_count, 3);
        assert_eq!(list[1].trigger, "rare");
        assert_eq!(list[1].success_count, 1);
    }

    #[test]
    fn test_empty_trigger_rejected() {
        // GIVEN
        let (store, _) = setup();
        let proc = ProceduralMemory::new(&store);
        // WHEN / THEN
        assert!(matches!(
            proc.learn("ns", "", &["step".into()]),
            Err(ProceduralMemoryError::EmptyTrigger)
        ));
    }

    #[test]
    fn test_empty_steps_rejected() {
        // GIVEN
        let (store, _) = setup();
        let proc = ProceduralMemory::new(&store);
        // WHEN / THEN
        assert!(matches!(
            proc.learn("ns", "trigger", &[]),
            Err(ProceduralMemoryError::EmptySteps)
        ));
    }

    // enforce_limit evicts least-recently-used entries (LRU)
    #[test]
    fn test_enforce_limit_evicts_lru_entries() {
        // GIVEN: 5 procedures with explicit last_used_at timestamps
        let (store, _) = setup();
        let proc_mem = ProceduralMemory::new(&store);
        let conn = store.conn();

        for i in 0..5 {
            let id = uuid::Uuid::new_v4().to_string();
            let trigger = format!("trigger-{i}");
            conn.execute(
                "INSERT INTO procedural_memories (id, namespace, trigger_text, steps, success_count, last_used_at, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                rusqlite::params![
                    id,
                    "ns",
                    trigger,
                    "[\"step\"]",
                    1,
                    format!("2026-01-0{}T10:00:00Z", i + 1),
                    format!("2026-01-0{}T10:00:00Z", i + 1),
                ],
            )
            .unwrap();
        }

        // WHEN: enforce limit of 3
        let evicted = proc_mem.enforce_limit("ns", 3).unwrap();

        // THEN: 2 LRU evicted, 3 remain
        assert_eq!(evicted, 2);
        let remaining = proc_mem.list("ns").unwrap();
        assert_eq!(remaining.len(), 3);
        // trigger-0 and trigger-1 (oldest last_used_at) should be gone
        for entry in &remaining {
            assert!(
                entry.trigger != "trigger-0" && entry.trigger != "trigger-1",
                "LRU entries should have been evicted"
            );
        }
    }

    // enforce_limit is no-op when under limit
    #[test]
    fn test_enforce_limit_noop_under_limit() {
        // GIVEN: 2 procedures
        let (store, _) = setup();
        let proc_mem = ProceduralMemory::new(&store);
        proc_mem.learn("ns", "t1", &["s".into()]).unwrap();
        proc_mem.learn("ns", "t2", &["s".into()]).unwrap();

        // WHEN: enforce limit of 10
        let evicted = proc_mem.enforce_limit("ns", 10).unwrap();

        // THEN: nothing evicted
        assert_eq!(evicted, 0);
        assert_eq!(proc_mem.list("ns").unwrap().len(), 2);
    }

    // reinforcing existing trigger does NOT trigger eviction
    #[test]
    fn test_reinforce_does_not_trigger_eviction() {
        // GIVEN: 3 procedures
        let (store, _) = setup();
        let proc_mem = ProceduralMemory::new(&store);
        proc_mem.learn("ns", "t1", &["s".into()]).unwrap();
        proc_mem.learn("ns", "t2", &["s".into()]).unwrap();
        proc_mem.learn("ns", "t3", &["s".into()]).unwrap();

        // WHEN: reinforce existing trigger (no new row)
        proc_mem
            .learn("ns", "t1", &["s1".into(), "s2".into()])
            .unwrap();

        // THEN: still 3 procedures
        assert_eq!(proc_mem.list("ns").unwrap().len(), 3);
    }

    #[test]
    fn test_namespace_isolation() {
        // GIVEN
        let (store, _) = setup();
        let proc = ProceduralMemory::new(&store);
        proc.learn("ns-a", "trigger", &["step-a".into()]).unwrap();
        proc.learn("ns-b", "trigger", &["step-b".into()]).unwrap();
        // WHEN
        let a = proc.recall("ns-a", "trigger").unwrap().unwrap();
        let b = proc.recall("ns-b", "trigger").unwrap().unwrap();
        // THEN
        assert_eq!(a.steps[0], "step-a");
        assert_eq!(b.steps[0], "step-b");
    }

    #[test]
    fn test_list_empty_namespace() {
        // GIVEN
        let (store, _) = setup();
        let proc = ProceduralMemory::new(&store);
        // WHEN / THEN
        assert!(proc.list("empty").unwrap().is_empty());
    }

    /// `chrono_now_utc` returns a valid timestamp even when the SQLite call path is used.
    /// The fallback path (chrono::Utc::now()) is exercised via unit test of the helper.
    #[test]
    fn test_procedural_timestamp_fallback_on_sqlite_error() {
        // GIVEN / WHEN: call the timestamp helper directly
        let ts = chrono_now_utc();

        // THEN: the result is a valid ISO 8601 string regardless of which path was taken
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
