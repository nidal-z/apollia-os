//! MemoryManager — namespace isolation and cross-namespace access control.
//!
//! Point d'entree unique pour acceder a la memoire d'un agent.
//! Gere l'ouverture lazy des fichiers `.db`, les permissions (read/write vs read-only),
//! et le routage vers le bon store.
//!
//! Un fichier SQLite par namespace : `<base_dir>/<namespace>.db` (Principe #1 local-first).

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

/// Gestionnaire de memoire avec isolation par namespace.
///
/// Point d'entree unique pour acceder a la memoire d'un agent.
/// Gere l'ouverture des fichiers `.db`, les permissions (read/write vs read-only),
/// et le routage vers le bon store.
pub struct MemoryManager {
    /// Repertoire racine des fichiers memoire (`~/.apollia/memory/`).
    base_dir: PathBuf,
    /// Namespace prive de l'agent (lecture/ecriture).
    primary_namespace: Option<String>,
    /// Namespaces partages (lecture seule).
    shared_namespaces: Vec<String>,
    /// Stores ouverts (lazy-opened).
    stores: HashMap<String, MemoryStore>,
    /// Instant de la derniere purge automatique (None si jamais purgee).
    last_purge: Option<Instant>,
}

/// Niveau d'acces a un namespace.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryAccess {
    /// Lecture et ecriture (namespace prive de l'agent).
    ReadWrite,
    /// Lecture seule (namespace partage).
    ReadOnly,
}

/// Erreurs du MemoryManager.
#[derive(Debug, thiserror::Error)]
pub enum MemoryManagerError {
    /// Aucun namespace memoire configure pour cet agent.
    #[error("no memory namespace configured for this agent")]
    NoNamespace,

    /// Le namespace est en lecture seule (namespace partage).
    #[error("namespace '{0}' is read-only (shared namespace)")]
    ReadOnlyNamespace(String),

    /// Le namespace n'est pas autorise pour cet agent.
    #[error("namespace '{0}' is not allowed for this agent")]
    NamespaceNotAllowed(String),

    /// L'ouverture du namespace a echoue.
    #[error("failed to open namespace '{namespace}': {reason}")]
    OpenFailed {
        /// Namespace concerne.
        namespace: String,
        /// Raison de l'echec.
        reason: String,
    },

    /// Erreur du MemoryStore sous-jacent.
    #[error("memory store error: {0}")]
    Store(#[from] crate::store::MemoryStoreError),

    /// Erreur du backend episodique.
    #[error("episodic memory error: {0}")]
    Episodic(#[from] crate::episodic::EpisodicMemoryError),

    /// Erreur du backend semantique.
    #[error("semantic memory error: {0}")]
    Semantic(#[from] crate::semantic::SemanticMemoryError),

    /// Erreur du backend procedural.
    #[error("procedural memory error: {0}")]
    Procedural(#[from] crate::procedural::ProceduralMemoryError),

    /// Erreur de recherche FTS5.
    #[error("search error: {0}")]
    Search(#[from] crate::search::MemorySearchError),

    /// Erreur d'I/O (fichier, repertoire).
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

impl MemoryManager {
    /// Cree un MemoryManager pour un agent.
    ///
    /// - `base_dir` : repertoire racine (`~/.apollia/memory/`)
    /// - `primary_namespace` : namespace prive (None si pas de memoire)
    /// - `shared_namespaces` : namespaces en lecture seule
    ///
    /// Les stores sont ouverts lazily au premier acces, pas au `new()`.
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

    /// Verifie le niveau d'acces a un namespace.
    ///
    /// Retourne `ReadWrite` pour le namespace prive, `ReadOnly` pour les
    /// namespaces partages, `None` si le namespace n'est pas autorise.
    pub fn access_level(&self, namespace: &str) -> Option<MemoryAccess> {
        if self.primary_namespace.as_deref() == Some(namespace) {
            return Some(MemoryAccess::ReadWrite);
        }
        if self.shared_namespaces.iter().any(|ns| ns == namespace) {
            return Some(MemoryAccess::ReadOnly);
        }
        None
    }

    /// Retourne une reference au store d'un namespace (l'ouvre si necessaire).
    ///
    /// Verifie que l'agent a le droit d'acceder au namespace.
    /// Cree le repertoire `base_dir` s'il n'existe pas.
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

    /// Statistiques d'un namespace.
    ///
    /// Delegue a [`MemoryStore::stats`] apres verification des permissions.
    pub fn stats(&mut self, namespace: &str) -> Result<MemoryStats, MemoryManagerError> {
        let db_path = self.db_path(namespace);
        let store = self.store(namespace)?;
        let stats = store.stats(namespace, &db_path)?;
        Ok(stats)
    }

    /// Purge les entrees expirees dans le namespace prive.
    ///
    /// Delegue a `EpisodicMemory::purge_expired()` et
    /// `SemanticMemory::purge_expired()`. Retourne le nombre total d'entrees purgees.
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

    /// Purge automatique conditionnelle des entrees expirees.
    ///
    /// Appelle `purge_expired()` uniquement si au moins `PURGE_INTERVAL_SECS`
    /// secondes se sont ecoulees depuis la derniere purge (ou si aucune purge
    /// n'a encore eu lieu). Les erreurs sont loguees mais ignorees (fire-and-forget).
    ///
    /// Destinee a etre appelee apres chaque ecriture memoire.
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

    /// Construit le chemin du fichier `.db` pour un namespace.
    fn db_path(&self, namespace: &str) -> PathBuf {
        self.base_dir.join(format!("{namespace}.db"))
    }

    /// Ouvre un store pour un namespace et l'insere dans le cache.
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

    fn temp_base_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("apollia_mgr_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).expect("failed to create temp dir");
        dir
    }

    // -- Ouvrir un namespace prive (lecture/ecriture)
    #[test]
    fn test_ac1_open_primary_namespace() {
        // GIVEN
        let base = temp_base_dir();
        let mut mgr = MemoryManager::new(&base, Some("crm-dupont".into()), vec![]);
        // WHEN
        let store = mgr.store("crm-dupont");
        // THEN
        assert!(store.is_ok());
        assert!(base.join("crm-dupont.db").exists());
    }

    // -- Lire un namespace partage (lecture seule)
    #[test]
    fn test_ac2_read_shared_namespace() {
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

    // -- Ecriture refusee sur namespace partage (verification access_level)
    #[test]
    fn test_ac3_write_to_shared_rejected() {
        // GIVEN
        let base = temp_base_dir();
        let mgr = MemoryManager::new(&base, Some("private".into()), vec!["shared".into()]);
        // WHEN / THEN
        assert_eq!(mgr.access_level("shared"), Some(MemoryAccess::ReadOnly));
        assert_eq!(mgr.access_level("private"), Some(MemoryAccess::ReadWrite));
    }

    // -- Acces refuse a un namespace non-declare
    #[test]
    fn test_ac4_undeclared_namespace_rejected() {
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
    fn test_ac5_stats_returns_counts() {
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
    fn test_ac6_no_namespace_returns_error() {
        // GIVEN
        let base = temp_base_dir();
        let mut mgr = MemoryManager::new(&base, None, vec![]);
        // WHEN
        let result = mgr.store("anything");
        // THEN
        assert!(matches!(result, Err(MemoryManagerError::NoNamespace)));
    }

    // access_level retourne None pour un namespace inconnu
    #[test]
    fn test_access_level_returns_none_for_unknown() {
        // GIVEN
        let base = temp_base_dir();
        let mgr = MemoryManager::new(&base, Some("mine".into()), vec!["shared".into()]);
        // WHEN / THEN
        assert!(mgr.access_level("unknown").is_none());
    }

    // purge_expired sans namespace retourne NoNamespace
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
        sem.remember(
            "ns",
            "old.key",
            &serde_json::json!("old"),
            1.0,
            None,
            Some("2020-01-01T00:00:00Z"),
        )
        .expect("remember expired semantic");

        ep.record("ns", "agent-1", "Fresh episode", 0.5, None, None, None)
            .expect("record fresh episode");

        // WHEN
        let purged = mgr.purge_expired().expect("purge");
        // THEN
        assert_eq!(purged, 2);
    }

    // Stats avec des donnees reelles
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
        sem.remember("ns", "k1", &serde_json::json!("v"), 1.0, None, None)
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

    // Lazy opening -- store n'est pas cree au new()
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

    // create_dir_all -- le repertoire base est cree automatiquement
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

        sem.remember("ns", "key", &serde_json::json!("val"), 1.0, None, None)
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
        sem.remember("ns", "k", &serde_json::json!("v"), 1.0, None, None)
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
