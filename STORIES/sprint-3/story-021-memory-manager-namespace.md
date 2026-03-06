# [Sprint 3][apollia-memory] MemoryManager namespace isolation

**ID :** STORY-021
**Sprint :** 3
**Crate cible :** `apollia-memory`
**Fichier(s) cible(s) :** `crates/apollia-memory/src/manager.rs`
**Taille :** M
**Depend de :** STORY-018, STORY-019, STORY-020
**Statut :** 🔲 A faire

---

## User Story

```
En tant que runtime,
je veux un MemoryManager qui gere l'isolation par namespace et le controle d'acces cross-namespace,
afin que chaque agent ait sa propre base memoire isolee tout en pouvant lire
les namespaces partages declares dans son manifest.
```

---

## Contexte technique

Chaque agent declare un `memory_namespace` prive (lecture/ecriture) et optionnellement
des `shared_memory_namespaces` (lecture seule). Le MemoryManager est le point d'entree
unique pour acceder a la memoire — il route vers le bon fichier `.db` et enforce les
permissions.

Un fichier SQLite par namespace : `~/.apollia/memory/<namespace>.db`.

**Principe(s) architecturaux concernes :**
- Principe #1 — Local-first (un fichier .db par namespace)
- Principe #5 — Un acteur, une responsabilite (MemoryManager = routage + permissions)
- Principe #6 — Memoire a initiative de l'agent

**Position dans l'architecture :**
```
apollia-memory
  ├── store.rs          (STORY-017 ✅)
  ├── episodic.rs       (STORY-018)
  ├── semantic.rs       (STORY-019)
  ├── search.rs         (STORY-020)
  └── manager.rs        <- cette story
        ├── MemoryManager        (struct, possede les stores)
        ├── MemoryAccess         (enum Read/Write)
        ├── MemoryManagerError   (enum thiserror)
        └── MemoryStats          (struct)
```

---

## Criteres d'Acceptation

### AC-1 — Ouvrir un namespace prive (lecture/ecriture)

```
ETANT DONNE un agent avec memory_namespace = "crm-dupont"
QUAND le MemoryManager ouvre ce namespace
ALORS un fichier ~/.apollia/memory/crm-dupont.db est cree (ou ouvert)
ET l'agent peut record(), remember(), search() dans ce namespace
```

### AC-2 — Lire un namespace partage (lecture seule)

```
ETANT DONNE un agent avec shared_memory_namespaces = ["shared-knowledge"]
QUAND l'agent appelle search() sur "shared-knowledge"
ALORS les resultats sont retournes normalement
```

### AC-3 — Ecriture refusee sur namespace partage

```
ETANT DONNE un agent dont "shared-knowledge" est dans shared_memory_namespaces (pas son namespace prive)
QUAND l'agent appelle record() ou remember() sur "shared-knowledge"
ALORS une erreur MemoryManagerError::ReadOnlyNamespace est retournee
```

### AC-4 — Acces refuse a un namespace non-declare

```
ETANT DONNE un agent avec memory_namespace = "crm-dupont" et shared = ["shared"]
QUAND l'agent appelle search() sur "autre-namespace"
ALORS une erreur MemoryManagerError::NamespaceNotAllowed est retournee
```

### AC-5 — Stats d'un namespace

```
ETANT DONNE un namespace avec 10 episodes, 5 connaissances semantiques, 2 procedures
QUAND on appelle manager.stats(namespace)
ALORS un MemoryStats est retourne avec episodic_count=10, semantic_count=5, procedural_count=2
ET la taille du fichier .db est incluse
```

### AC-6 — Agent sans memory_namespace (None)

```
ETANT DONNE un agent avec memory_namespace = None
QUAND le MemoryManager est cree pour cet agent
ALORS aucun fichier .db n'est cree
ET toute operation memoire retourne MemoryManagerError::NoNamespace
```

---

## Specification technique

### Types a creer dans `crates/apollia-memory/src/manager.rs`

```rust
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use crate::store::MemoryStore;

/// Gestionnaire de memoire avec isolation par namespace.
///
/// Point d'entree unique pour acceder a la memoire d'un agent.
/// Gere l'ouverture des fichiers .db, les permissions (read/write vs read-only),
/// et le routage vers le bon store.
pub struct MemoryManager {
    /// Repertoire racine des fichiers memoire (~/.apollia/memory/).
    base_dir: PathBuf,
    /// Namespace prive de l'agent (lecture/ecriture).
    primary_namespace: Option<String>,
    /// Namespaces partages (lecture seule).
    shared_namespaces: Vec<String>,
    /// Stores ouverts (lazy-opened).
    stores: HashMap<String, MemoryStore>,
}

/// Niveau d'acces a un namespace.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryAccess {
    ReadWrite,
    ReadOnly,
}

/// Statistiques d'un namespace memoire.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MemoryStats {
    pub namespace: String,
    pub episodic_count: u64,
    pub semantic_count: u64,
    pub procedural_count: u64,
    pub fts_entries: u64,
    pub db_size_bytes: u64,
}

/// Erreurs du MemoryManager.
#[derive(Debug, thiserror::Error)]
pub enum MemoryManagerError {
    #[error("no memory namespace configured for this agent")]
    NoNamespace,

    #[error("namespace '{0}' is read-only (shared namespace)")]
    ReadOnlyNamespace(String),

    #[error("namespace '{0}' is not allowed for this agent")]
    NamespaceNotAllowed(String),

    #[error("failed to open namespace '{namespace}': {reason}")]
    OpenFailed { namespace: String, reason: String },

    #[error("memory store error: {0}")]
    Store(#[from] crate::store::MemoryStoreError),

    #[error("episodic memory error: {0}")]
    Episodic(#[from] crate::episodic::EpisodicMemoryError),

    #[error("semantic memory error: {0}")]
    Semantic(#[from] crate::semantic::SemanticMemoryError),

    #[error("search error: {0}")]
    Search(#[from] crate::search::MemorySearchError),
}

impl MemoryManager {
    /// Cree un MemoryManager pour un agent.
    ///
    /// - `base_dir` : repertoire racine (~/.apollia/memory/)
    /// - `primary_namespace` : namespace prive (None si pas de memoire)
    /// - `shared_namespaces` : namespaces en lecture seule
    pub fn new(
        base_dir: &Path,
        primary_namespace: Option<String>,
        shared_namespaces: Vec<String>,
    ) -> Self { ... }

    /// Verifie le niveau d'acces a un namespace.
    pub fn access_level(&self, namespace: &str) -> Option<MemoryAccess> { ... }

    /// Retourne une reference au store d'un namespace (l'ouvre si necessaire).
    pub fn store(&mut self, namespace: &str) -> Result<&MemoryStore, MemoryManagerError> { ... }

    /// Statistiques d'un namespace.
    pub fn stats(&mut self, namespace: &str) -> Result<MemoryStats, MemoryManagerError> { ... }

    /// Purge les entries expirees dans le namespace prive.
    pub fn purge_expired(&mut self) -> Result<u64, MemoryManagerError> { ... }
}
```

### Dependances Cargo

```toml
# Aucune nouvelle dependance
```

### Comportement attendu

- Les stores sont ouverts **lazily** au premier acces (pas au `new()`).
- `access_level()` retourne `ReadWrite` pour le primary, `ReadOnly` pour les shared, `None` sinon.
- `store()` verifie les permissions avant de retourner le store.
- `stats()` execute des `SELECT COUNT(*)` sur chaque table + `fs::metadata` pour la taille fichier.
- `purge_expired()` delegue a `EpisodicMemory::purge_expired()` + `SemanticMemory::purge_expired()`.
- Le chemin du fichier est `<base_dir>/<namespace>.db`.
- Le repertoire `base_dir` est cree automatiquement s'il n'existe pas (via `fs::create_dir_all`).

### Ce que cette story N'implemente PAS

- Le pattern acteur Tokio (le MemoryManager est synchrone, comme MemoryStore)
- La concurrence multi-agents sur le meme namespace (WAL gere les conflits au niveau SQLite)
- L'export/import de namespaces (Sprint 5, CLI)

---

## Tests requis

### Tests unitaires dans `crates/apollia-memory/src/manager.rs`

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn temp_base_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("apollia_mgr_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

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

    #[test]
    fn test_ac2_read_shared_namespace() {
        // GIVEN — create the shared namespace DB first
        let base = temp_base_dir();
        let _ = MemoryStore::open(&base.join("shared.db")).unwrap();
        let mut mgr = MemoryManager::new(&base, Some("private".into()), vec!["shared".into()]);
        // WHEN
        let store = mgr.store("shared");
        // THEN
        assert!(store.is_ok());
        assert_eq!(mgr.access_level("shared"), Some(MemoryAccess::ReadOnly));
    }

    #[test]
    fn test_ac3_write_to_shared_rejected() {
        // GIVEN
        let base = temp_base_dir();
        let mgr = MemoryManager::new(&base, Some("private".into()), vec!["shared".into()]);
        // WHEN / THEN
        assert_eq!(mgr.access_level("shared"), Some(MemoryAccess::ReadOnly));
        assert_eq!(mgr.access_level("private"), Some(MemoryAccess::ReadWrite));
    }

    #[test]
    fn test_ac4_undeclared_namespace_rejected() {
        // GIVEN
        let base = temp_base_dir();
        let mut mgr = MemoryManager::new(&base, Some("mine".into()), vec![]);
        // WHEN
        let result = mgr.store("other");
        // THEN
        assert!(matches!(result, Err(MemoryManagerError::NamespaceNotAllowed(_))));
    }

    #[test]
    fn test_ac5_stats_returns_counts() {
        // GIVEN
        let base = temp_base_dir();
        let mut mgr = MemoryManager::new(&base, Some("ns".into()), vec![]);
        let _ = mgr.store("ns").unwrap();
        // WHEN
        let stats = mgr.stats("ns").unwrap();
        // THEN
        assert_eq!(stats.namespace, "ns");
        assert_eq!(stats.episodic_count, 0);
        assert_eq!(stats.semantic_count, 0);
        assert!(stats.db_size_bytes > 0);
    }

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

    #[test]
    fn test_access_level_returns_none_for_unknown() {
        // GIVEN
        let base = temp_base_dir();
        let mgr = MemoryManager::new(&base, Some("mine".into()), vec!["shared".into()]);
        // WHEN / THEN
        assert!(mgr.access_level("unknown").is_none());
    }
}
```

---

## Definition of Done

**Qualite code :**
- [ ] `cargo test -p apollia-memory` passe (0 test ignore)
- [ ] `cargo clippy -p apollia-memory -- -D warnings` : zero warning
- [ ] `cargo fmt --check` : code formate
- [ ] Zero `unwrap()` dans le code de production
- [ ] Zero `todo!()` dans le code de production
- [ ] Docstring `///` sur chaque struct, enum, et fonction publique

**Architectural :**
- [ ] Un fichier .db par namespace (Principe #1)
- [ ] Permissions read/write vs read-only enforcees
- [ ] Lazy opening des stores
- [ ] `create_dir_all` pour le repertoire memoire

**Commit :**
- [ ] Commit conventionnel : `feat(apollia-memory): add MemoryManager with namespace isolation and access control`

---

## Notes d'implementation

**Decisions prises pendant l'implementation :**

**Deviations par rapport a la spec :**

**Dette technique identifiee :**

---

## Liens

- Epic parent : Sprint 3 — Memory Engine
- Story precedente : STORY-020 (FTS5 search)
- Story suivante : STORY-022 (ProceduralMemory)
- ADR associe : aucun prevu
