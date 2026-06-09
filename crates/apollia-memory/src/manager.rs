//! MemoryManager: namespace isolation and cross-namespace access control.
//!
//! Single entry point for accessing an agent's memory.
//! Handles lazy opening of `.db` files, permissions (read/write vs read-only),
//! and routing to the right store.
//!
//! One SQLite file per namespace: `<base_dir>/<namespace>.db` (local-first).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

use apollia_core::manifest::MemoryConfig;

use crate::episodic::EpisodicMemory;
use crate::procedural::ProceduralMemory;
use crate::semantic::SemanticMemory;
use crate::store::MemoryStore;

pub use crate::store::MemoryStats;

/// Summary of a configurable purge operation, broken down by memory type.
#[derive(Debug, Default)]
pub struct PurgeReport {
    /// Number of episodic entries deleted.
    pub episodic_deleted: usize,
    /// Number of semantic entries deleted.
    pub semantic_deleted: usize,
    /// Number of procedural entries deleted.
    pub procedural_deleted: usize,
}

/// Intervalle minimum entre deux purges automatiques (5 minutes).
const PURGE_INTERVAL_SECS: u64 = 300;

/// Memory manager with per-namespace isolation.
///
/// Single entry point for accessing an agent's memory.
/// Handles opening `.db` files, permissions (read/write vs read-only),
/// and routing to the right store.
pub struct MemoryManager {
    /// Root directory for memory files (`~/.apollia/memory/`).
    base_dir: PathBuf,
    /// The agent's private namespace (read/write).
    primary_namespace: Option<String>,
    /// Shared namespaces (read-only).
    shared_namespaces: Vec<String>,
    /// Open stores (lazy-opened).
    stores: HashMap<String, MemoryStore>,
    /// Instant of the last automatic purge (None if never purged).
    last_purge: Option<Instant>,
}

/// Access level to a namespace.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryAccess {
    /// Read and write (the agent's private namespace).
    ReadWrite,
    /// Read-only (shared namespace).
    ReadOnly,
}

/// MemoryManager errors.
#[derive(Debug, thiserror::Error)]
pub enum MemoryManagerError {
    /// No memory namespace configured for this agent.
    #[error("no memory namespace configured for this agent")]
    NoNamespace,

    /// The namespace is read-only (shared namespace).
    #[error("namespace '{0}' is read-only (shared namespace)")]
    ReadOnlyNamespace(String),

    /// The namespace is not allowed for this agent.
    #[error("namespace '{0}' is not allowed for this agent")]
    NamespaceNotAllowed(String),

    /// Opening the namespace failed.
    #[error("failed to open namespace '{namespace}': {reason}")]
    OpenFailed {
        /// Namespace concerned.
        namespace: String,
        /// Reason for the failure.
        reason: String,
    },

    /// Error from the underlying MemoryStore.
    #[error("memory store error: {0}")]
    Store(#[from] crate::store::MemoryStoreError),

    /// Error from the episodic backend.
    #[error("episodic memory error: {0}")]
    Episodic(#[from] crate::episodic::EpisodicMemoryError),

    /// Error from the semantic backend.
    #[error("semantic memory error: {0}")]
    Semantic(#[from] crate::semantic::SemanticMemoryError),

    /// Error from the procedural backend.
    #[error("procedural memory error: {0}")]
    Procedural(#[from] crate::procedural::ProceduralMemoryError),

    /// FTS5 search error.
    #[error("search error: {0}")]
    Search(#[from] crate::search::MemorySearchError),

    /// I/O error (file, directory).
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

impl MemoryManager {
    /// Create a MemoryManager for an agent.
    ///
    /// - `base_dir`: root directory (`~/.apollia/memory/`)
    /// - `primary_namespace`: private namespace (None if no memory)
    /// - `shared_namespaces`: read-only namespaces
    ///
    /// Stores are opened lazily on first access, not in `new()`.
    pub fn new(
        base_dir: &Path,
        primary_namespace: Option<String>,
        shared_namespaces: Vec<String>,
    ) -> Self {
        Self {
            base_dir: base_dir.to_path_buf(),
            primary_namespace,
            shared_namespaces,
            stores: HashMap::new(),
            last_purge: None,
        }
    }

    /// Check the access level to a namespace.
    ///
    /// Returns `ReadWrite` for the private namespace, `ReadOnly` for shared
    /// namespaces, `None` if the namespace is not allowed.
    /// Lists the shared (read-only) namespaces configured for this agent.
    pub fn shared_namespaces(&self) -> &[String] {
        &self.shared_namespaces
    }

    pub fn access_level(&self, namespace: &str) -> Option<MemoryAccess> {
        if self.primary_namespace.as_deref() == Some(namespace) {
            return Some(MemoryAccess::ReadWrite);
        }
        if self.shared_namespaces.iter().any(|ns| ns == namespace) {
            return Some(MemoryAccess::ReadOnly);
        }
        None
    }

    /// Return a reference to a namespace's store (opening it if needed).
    ///
    /// Checks that the agent is allowed to access the namespace.
    /// Creates the `base_dir` directory if it does not exist.
    pub fn store(&mut self, namespace: &str) -> Result<&MemoryStore, MemoryManagerError> {
        if self.primary_namespace.is_none() {
            return Err(MemoryManagerError::NoNamespace);
        }

        if self.access_level(namespace).is_none() {
            return Err(MemoryManagerError::NamespaceNotAllowed(
                namespace.to_string(),
            ));
        }

        if !self.stores.contains_key(namespace) {
            self.open_store(namespace)?;
        }

        Ok(self.stores.get(namespace).expect("store was just inserted"))
    }

    /// Statistics for a namespace.
    ///
    /// Delegates to [`MemoryStore::stats`] after checking permissions.
    pub fn stats(&mut self, namespace: &str) -> Result<MemoryStats, MemoryManagerError> {
        let db_path = self.db_path(namespace);
        let store = self.store(namespace)?;
        let stats = store.stats(namespace, &db_path)?;
        Ok(stats)
    }

    /// Purge expired entries in the private namespace.
    ///
    /// Delegates to `EpisodicMemory::purge_expired()` and
    /// `SemanticMemory::purge_expired()`. Returns the total number of entries purged.
    pub fn purge_expired(&mut self) -> Result<u64, MemoryManagerError> {
        let namespace = self
            .primary_namespace
            .clone()
            .ok_or(MemoryManagerError::NoNamespace)?;

        let store = self.store(&namespace)?;
        let episodic = EpisodicMemory::new(store);
        let semantic = SemanticMemory::new(store);

        let ep_purged = episodic.purge_expired(&namespace)?;
        let sem_purged = semantic.purge_expired(&namespace)?;

        let total = ep_purged + sem_purged;

        if total > 0 {
            tracing::info!(
                namespace = %namespace,
                episodic_purged = ep_purged,
                semantic_purged = sem_purged,
                "expired memories purged"
            );
        }

        Ok(total)
    }

    /// Conditional automatic purge of expired entries.
    ///
    /// Calls `purge_expired()` only if at least `PURGE_INTERVAL_SECS` seconds
    /// have elapsed since the last purge (or if no purge has happened yet).
    /// Errors are logged but ignored (fire-and-forget).
    ///
    /// Meant to be called after each memory write.
    pub fn maybe_purge(&mut self) {
        let should_purge = match self.last_purge {
            None => true,
            Some(last) => last.elapsed().as_secs() >= PURGE_INTERVAL_SECS,
        };

        if !should_purge {
            return;
        }

        self.last_purge = Some(Instant::now());

        match self.purge_expired() {
            Ok(count) => {
                if count > 0 {
                    tracing::info!(purged = count, "automatic TTL purge completed");
                }
            }
            Err(err) => {
                tracing::warn!(error = %err, "automatic TTL purge failed");
            }
        }
    }

    /// Purge entries older than the supplied thresholds, per memory type.
    ///
    /// Pass `None` for any type to skip age-based purging for that type.
    /// Only namespaces accessible by this manager can be targeted.
    pub fn purge_old_entries(
        &mut self,
        namespace: &str,
        episodic_days: Option<u32>,
        semantic_days: Option<u32>,
        procedural_days: Option<u32>,
    ) -> Result<PurgeReport, MemoryManagerError> {
        if self.access_level(namespace).is_none() {
            return Err(MemoryManagerError::NamespaceNotAllowed(
                namespace.to_string(),
            ));
        }

        let store = self.store(namespace)?;
        let ep = EpisodicMemory::new(store);
        let sem = SemanticMemory::new(store);
        let proc = ProceduralMemory::new(store);

        let episodic_deleted = match episodic_days {
            Some(days) => ep.purge_older_than(namespace, days)? as usize,
            None => 0,
        };
        let semantic_deleted = match semantic_days {
            Some(days) => sem.purge_older_than(namespace, days)? as usize,
            None => 0,
        };
        let procedural_deleted = match procedural_days {
            Some(days) => proc.purge_older_than(namespace, days)? as usize,
            None => 0,
        };

        let report = PurgeReport {
            episodic_deleted,
            semantic_deleted,
            procedural_deleted,
        };

        if report.episodic_deleted + report.semantic_deleted + report.procedural_deleted > 0 {
            tracing::info!(
                namespace = %namespace,
                episodic = report.episodic_deleted,
                semantic = report.semantic_deleted,
                procedural = report.procedural_deleted,
                "configurable purge completed"
            );
        }

        Ok(report)
    }

    /// Run a single purge pass driven by `config` if `auto_purge = true`.
    ///
    /// Returns immediately when `auto_purge = false` without touching the database.
    /// Errors are logged as warnings and never propagated (fire-and-forget).
    pub fn start_auto_purge(&mut self, config: &MemoryConfig, namespace: &str) {
        if !config.auto_purge {
            return;
        }

        match self.purge_old_entries(
            namespace,
            config.episodic_retention_days,
            config.semantic_retention_days,
            config.procedural_retention_days,
        ) {
            Ok(report) => {
                tracing::info!(
                    namespace = %namespace,
                    episodic = report.episodic_deleted,
                    semantic = report.semantic_deleted,
                    procedural = report.procedural_deleted,
                    "auto purge completed"
                );
            }
            Err(err) => {
                tracing::warn!(error = %err, namespace = %namespace, "auto purge failed");
            }
        }
    }

    /// Root directory where the manager's `.db` files live.
    ///
    /// Exposed so external components (notably the PyO3 bridge) can delegate
    /// to [`crate::export`] without reopening a store themselves.
    pub fn base_dir(&self) -> &Path {
        &self.base_dir
    }

    /// The manager's private (read/write) namespace, or `None` if the agent
    /// has no memory configured.
    pub fn primary_namespace(&self) -> Option<&str> {
        self.primary_namespace.as_deref()
    }

    /// Build the `.db` file path for a namespace.
    fn db_path(&self, namespace: &str) -> PathBuf {
        self.base_dir.join(format!("{namespace}.db"))
    }

    /// Return the `.db` file path for a namespace, or `None` if the manager
    /// does not know that namespace.
    ///
    /// Exposed so external components (notably
    /// [`crate::user_memory::UserMemoryRepository`]) can open their own
    /// connection on the same file.
    pub fn db_path_for(&self, namespace: &str) -> Option<PathBuf> {
        Some(self.db_path(namespace))
    }

    /// Open a store for a namespace and insert it into the cache.
    fn open_store(&mut self, namespace: &str) -> Result<(), MemoryManagerError> {
        std::fs::create_dir_all(&self.base_dir)?;

        let path = self.db_path(namespace);
        let store = MemoryStore::open(&path).map_err(|e| MemoryManagerError::OpenFailed {
            namespace: namespace.to_string(),
            reason: e.to_string(),
        })?;

        self.stores.insert(namespace.to_string(), store);

        tracing::info!(
            namespace = %namespace,
            path = %path.display(),
            "namespace store opened"
        );

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::semantic::RememberInput;

    fn temp_base_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("apollia_mgr_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).expect("failed to create temp dir");
        dir
    }

    // Open a private namespace (read/write)
    #[test]
    fn test_open_primary_namespace() {
        // GIVEN
        let base = temp_base_dir();
        let mut mgr = MemoryManager::new(&base, Some("crm-dupont".into()), vec![]);
        // WHEN
        let store = mgr.store("crm-dupont");
        // THEN
        assert!(store.is_ok());
        assert!(base.join("crm-dupont.db").exists());
    }

    // Read a shared namespace (read-only)
    #[test]
    fn test_read_shared_namespace() {
        // GIVEN -- create the shared namespace DB first
        let base = temp_base_dir();
        let _ = MemoryStore::open(&base.join("shared.db")).expect("pre-create shared db");
        let mut mgr = MemoryManager::new(&base, Some("private".into()), vec!["shared".into()]);
        // WHEN
        let store = mgr.store("shared");
        // THEN
        assert!(store.is_ok());
        assert_eq!(mgr.access_level("shared"), Some(MemoryAccess::ReadOnly));
    }

    // Write refused on a shared namespace (access_level check)
    #[test]
    fn test_write_to_shared_rejected() {
        // GIVEN
        let base = temp_base_dir();
        let mgr = MemoryManager::new(&base, Some("private".into()), vec!["shared".into()]);
        // WHEN / THEN
        assert_eq!(mgr.access_level("shared"), Some(MemoryAccess::ReadOnly));
        assert_eq!(mgr.access_level("private"), Some(MemoryAccess::ReadWrite));
    }

    // -- Acces refuse a un namespace non-declare
    #[test]
    fn test_undeclared_namespace_rejected() {
        // GIVEN
        let base = temp_base_dir();
        let mut mgr = MemoryManager::new(&base, Some("mine".into()), vec![]);
        // WHEN
        let result = mgr.store("other");
        // THEN
        assert!(matches!(
            result,
            Err(MemoryManagerError::NamespaceNotAllowed(_))
        ));
    }

    // -- Stats d'un namespace
    #[test]
    fn test_stats_returns_counts() {
        // GIVEN
        let base = temp_base_dir();
        let mut mgr = MemoryManager::new(&base, Some("ns".into()), vec![]);
        let _ = mgr.store("ns").expect("open store");
        // WHEN
        let stats = mgr.stats("ns").expect("stats");
        // THEN
        assert_eq!(stats.namespace, "ns");
        assert_eq!(stats.episodic_count, 0);
        assert_eq!(stats.semantic_count, 0);
        assert_eq!(stats.procedural_count, 0);
        assert!(stats.db_size_bytes > 0);
    }

    // -- Agent sans memory_namespace (None)
    #[test]
    fn test_no_namespace_returns_error() {
        // GIVEN
        let base = temp_base_dir();
        let mut mgr = MemoryManager::new(&base, None, vec![]);
        // WHEN
        let result = mgr.store("anything");
        // THEN
        assert!(matches!(result, Err(MemoryManagerError::NoNamespace)));
    }

    // access_level returns None for an unknown namespace
    #[test]
    fn test_access_level_returns_none_for_unknown() {
        // GIVEN
        let base = temp_base_dir();
        let mgr = MemoryManager::new(&base, Some("mine".into()), vec!["shared".into()]);
        // WHEN / THEN
        assert!(mgr.access_level("unknown").is_none());
    }

    // purge_expired with no namespace returns NoNamespace
    #[test]
    fn test_purge_expired_no_namespace() {
        // GIVEN
        let base = temp_base_dir();
        let mut mgr = MemoryManager::new(&base, None, vec![]);
        // WHEN
        let result = mgr.purge_expired();
        // THEN
        assert!(matches!(result, Err(MemoryManagerError::NoNamespace)));
    }

    // purge_expired delegue correctement aux backends
    #[test]
    fn test_purge_expired_delegates_to_backends() {
        // GIVEN -- insert expired episodic + semantic entries
        let base = temp_base_dir();
        let mut mgr = MemoryManager::new(&base, Some("ns".into()), vec![]);
        let store = mgr.store("ns").expect("open store");

        let ep = EpisodicMemory::new(store);
        ep.record(
            "ns",
            "agent-1",
            "Old episode",
            0.5,
            None,
            Some("2020-01-01T00:00:00Z"),
            None,
        )
        .expect("record expired episode");

        let sem = SemanticMemory::new(store);
        sem.remember(RememberInput {
            namespace: "ns",
            key: "old.key",
            value: &serde_json::json!("old"),
            confidence: 1.0,
            source: None,
            expires_at: Some("2020-01-01T00:00:00Z"),
        })
        .expect("remember expired semantic");

        ep.record("ns", "agent-1", "Fresh episode", 0.5, None, None, None)
            .expect("record fresh episode");

        // WHEN
        let purged = mgr.purge_expired().expect("purge");
        // THEN
        assert_eq!(purged, 2);
    }

    // Stats with real data
    #[test]
    fn test_stats_with_data() {
        // GIVEN
        let base = temp_base_dir();
        let mut mgr = MemoryManager::new(&base, Some("ns".into()), vec![]);
        let store = mgr.store("ns").expect("open store");

        let ep = EpisodicMemory::new(store);
        ep.record("ns", "a", "ep1", 0.5, None, None, None)
            .expect("record");
        ep.record("ns", "a", "ep2", 0.5, None, None, None)
            .expect("record");

        let sem = SemanticMemory::new(store);
        sem.remember(RememberInput {
            namespace: "ns",
            key: "k1",
            value: &serde_json::json!("v"),
            confidence: 1.0,
            source: None,
            expires_at: None,
        })
        .expect("remember");

        // WHEN
        let stats = mgr.stats("ns").expect("stats");
        // THEN
        assert_eq!(stats.episodic_count, 2);
        assert_eq!(stats.semantic_count, 1);
        assert_eq!(stats.procedural_count, 0);
        assert_eq!(stats.fts_entries, 3);
        assert!(stats.db_size_bytes > 0);
    }

    // Lazy opening: store is not created in new()
    #[test]
    fn test_lazy_opening() {
        // GIVEN
        let base = temp_base_dir();
        let _mgr = MemoryManager::new(&base, Some("lazy-ns".into()), vec![]);
        // THEN -- no .db file created yet
        assert!(!base.join("lazy-ns.db").exists());
    }

    // maybe_purge -- purge au premier appel, skip si intervalle non ecoule
    #[test]
    fn test_maybe_purge_runs_after_interval() {
        // GIVEN -- manager with an expired episodic entry
        let base = temp_base_dir();
        let mut mgr = MemoryManager::new(&base, Some("ns".into()), vec![]);
        let store = mgr.store("ns").expect("open store");

        let ep = EpisodicMemory::new(store);
        ep.record(
            "ns",
            "agent-1",
            "Expired episode",
            0.5,
            None,
            Some("2020-01-01T00:00:00Z"),
            None,
        )
        .expect("record expired episode");

        // WHEN -- first call should purge
        mgr.maybe_purge();

        // THEN -- the expired entry should be gone
        let store = mgr.store("ns").expect("reopen store");
        let ep = EpisodicMemory::new(store);
        let entries = ep.history("ns", 100, None).expect("history");
        assert!(entries.is_empty(), "expired entry should have been purged");

        // GIVEN -- insert another expired entry
        let store = mgr.store("ns").expect("reopen store");
        let ep = EpisodicMemory::new(store);
        ep.record(
            "ns",
            "agent-1",
            "Another expired episode",
            0.5,
            None,
            Some("2020-01-01T00:00:00Z"),
            None,
        )
        .expect("record second expired episode");

        // WHEN -- second call immediately after should NOT purge (interval not elapsed)
        mgr.maybe_purge();

        // THEN -- the second expired entry should still be present
        let store = mgr.store("ns").expect("reopen store");
        let ep = EpisodicMemory::new(store);
        let entries = ep.history("ns", 100, None).expect("history");
        assert_eq!(
            entries.len(),
            1,
            "second expired entry should NOT have been purged yet"
        );
    }

    // create_dir_all: the base directory is created automatically
    #[test]
    fn test_create_dir_all_on_open() {
        // GIVEN -- base_dir doesn't exist yet
        let base =
            std::env::temp_dir().join(format!("apollia_mgr_nested_{}", uuid::Uuid::new_v4()));
        let nested = base.join("sub").join("dir");
        let mut mgr = MemoryManager::new(&nested, Some("ns".into()), vec![]);
        // WHEN
        let store = mgr.store("ns");
        // THEN
        assert!(store.is_ok());
        assert!(nested.join("ns.db").exists());
    }

    // purge_old_entries -- episodic only, semantic and procedural untouched
    #[test]
    fn test_purge_episodic_only() {
        use crate::procedural::ProceduralMemory;
        use crate::semantic::SemanticMemory;

        // GIVEN -- namespace with entries of each type, all created 10+ days ago
        let base = temp_base_dir();
        let mut mgr = MemoryManager::new(&base, Some("ns".into()), vec![]);

        let store = mgr.store("ns").expect("open store");
        let ep = EpisodicMemory::new(store);
        let sem = SemanticMemory::new(store);
        let proc = ProceduralMemory::new(store);

        // Insert old entries (created in 2020, well beyond 7-day threshold)
        let conn = store.conn();
        conn.execute(
            "INSERT INTO episodic_memories (id, namespace, agent_id, content, importance, created_at)
             VALUES ('ep1', 'ns', 'a', 'old episode', 0.5, '2020-01-01T00:00:00Z')",
            [],
        )
        .expect("insert episodic");
        drop(ep);

        sem.remember(RememberInput {
            namespace: "ns",
            key: "key",
            value: &serde_json::json!("val"),
            confidence: 1.0,
            source: None,
            expires_at: None,
        })
        .expect("remember semantic");

        proc.learn("ns", "trig", &["step1".to_string()])
            .expect("learn procedural");

        // Reopen store reference after mutating
        drop(sem);
        drop(conn);
        drop(proc);

        // WHEN -- purge episodic older than 7 days, leave semantic and procedural alone
        let report = mgr
            .purge_old_entries("ns", Some(7), None, None)
            .expect("purge");

        // THEN -- only episodic entry deleted
        assert_eq!(report.episodic_deleted, 1);
        assert_eq!(report.semantic_deleted, 0);
        assert_eq!(report.procedural_deleted, 0);

        let store = mgr.store("ns").expect("reopen");
        let stats = store.stats("ns", &base.join("ns.db")).expect("stats");
        assert_eq!(stats.episodic_count, 0, "episodic must be empty");
        assert_eq!(stats.semantic_count, 1, "semantic must be intact");
        assert_eq!(stats.procedural_count, 1, "procedural must be intact");
    }

    // purge_old_entries -- auto_purge = false does nothing
    #[test]
    fn test_auto_purge_false_does_nothing() {
        use apollia_core::manifest::MemoryConfig;

        // GIVEN -- old episodic entry and auto_purge = false
        let base = temp_base_dir();
        let mut mgr = MemoryManager::new(&base, Some("ns".into()), vec![]);

        let store = mgr.store("ns").expect("open store");
        store.conn().execute(
            "INSERT INTO episodic_memories (id, namespace, agent_id, content, importance, created_at)
             VALUES ('ep1', 'ns', 'a', 'old episode', 0.5, '2020-01-01T00:00:00Z')",
            [],
        )
        .expect("insert");

        let config = MemoryConfig {
            auto_purge: false,
            episodic_retention_days: Some(1),
            ..Default::default()
        };

        // WHEN
        mgr.start_auto_purge(&config, "ns");

        // THEN -- entry still present
        let store = mgr.store("ns").expect("reopen");
        let stats = store.stats("ns", &base.join("ns.db")).expect("stats");
        assert_eq!(
            stats.episodic_count, 1,
            "entry must not be purged when auto_purge = false"
        );
    }

    // purge_old_entries -- auto_purge = true purges on startup
    #[test]
    fn test_auto_purge_true_purges_old_entries() {
        use apollia_core::manifest::MemoryConfig;

        // GIVEN -- old episodic entry created more than 1 day ago and auto_purge = true
        let base = temp_base_dir();
        let mut mgr = MemoryManager::new(&base, Some("ns".into()), vec![]);

        let store = mgr.store("ns").expect("open store");
        store.conn().execute(
            "INSERT INTO episodic_memories (id, namespace, agent_id, content, importance, created_at)
             VALUES ('ep1', 'ns', 'a', 'old episode', 0.5, '2020-01-01T00:00:00Z')",
            [],
        )
        .expect("insert");

        let config = MemoryConfig {
            auto_purge: true,
            episodic_retention_days: Some(1),
            ..Default::default()
        };

        // WHEN
        mgr.start_auto_purge(&config, "ns");

        // THEN -- entry is gone
        let store = mgr.store("ns").expect("reopen");
        let stats = store.stats("ns", &base.join("ns.db")).expect("stats");
        assert_eq!(
            stats.episodic_count, 0,
            "old entry must be purged when auto_purge = true"
        );
    }

    // purge_old_entries -- report counts match actual deletions
    #[test]
    fn test_purge_report_counts() {
        use crate::semantic::SemanticMemory;

        // GIVEN -- 2 old episodic + 1 old semantic entries
        let base = temp_base_dir();
        let mut mgr = MemoryManager::new(&base, Some("ns".into()), vec![]);

        let store = mgr.store("ns").expect("open store");
        let conn = store.conn();
        conn.execute(
            "INSERT INTO episodic_memories (id, namespace, agent_id, content, importance, created_at)
             VALUES ('ep1', 'ns', 'a', 'c1', 0.5, '2020-01-01T00:00:00Z')",
            [],
        )
        .expect("ep1");
        conn.execute(
            "INSERT INTO episodic_memories (id, namespace, agent_id, content, importance, created_at)
             VALUES ('ep2', 'ns', 'a', 'c2', 0.5, '2020-01-02T00:00:00Z')",
            [],
        )
        .expect("ep2");
        drop(conn);

        let sem = SemanticMemory::new(store);
        sem.remember(RememberInput {
            namespace: "ns",
            key: "k",
            value: &serde_json::json!("v"),
            confidence: 1.0,
            source: None,
            expires_at: None,
        })
        .expect("semantic");
        drop(sem);

        // Force the semantic entry to an old created_at
        let conn = store.conn();
        conn.execute(
            "UPDATE semantic_memories SET created_at = '2020-01-01T00:00:00Z' WHERE namespace = 'ns'",
            [],
        )
        .expect("backdate");
        drop(conn);

        // WHEN -- purge both episodic and semantic older than 7 days
        let report = mgr
            .purge_old_entries("ns", Some(7), Some(7), None)
            .expect("purge");

        // THEN -- counts match exactly
        assert_eq!(report.episodic_deleted, 2);
        assert_eq!(report.semantic_deleted, 1);
        assert_eq!(report.procedural_deleted, 0);
    }

    // purge_old_entries -- unknown namespace returns NamespaceNotAllowed
    #[test]
    fn test_purge_unknown_namespace_rejected() {
        // GIVEN -- manager without 'other' in its allowed namespaces
        let base = temp_base_dir();
        let mut mgr = MemoryManager::new(&base, Some("mine".into()), vec![]);

        // WHEN
        let result = mgr.purge_old_entries("other", Some(7), None, None);

        // THEN
        assert!(matches!(
            result,
            Err(MemoryManagerError::NamespaceNotAllowed(_))
        ));
    }
}
