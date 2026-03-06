# [Sprint 4][apollia-aip] MemoryInterface Python vers apollia-memory

**ID :** STORY-028
**Sprint :** 4
**Crate cible :** `apollia-aip`
**Fichier(s) cible(s) :** `crates/apollia-aip/src/context.rs`
**Taille :** M (3h)
**Depend de :** STORY-026 (bridge async PyO3), apollia-memory Sprint 3 (STORY-017 a STORY-022 toutes livrees)
**Statut :** A faire

---

## User Story

En tant qu'agent Python, je veux acceder a la memoire via `ctx.memory.record()`, `ctx.memory.remember()`, `ctx.memory.recall()`, `ctx.memory.search()` et `ctx.memory.forget()`, afin de persister et recuperer des informations entre les executions sans gerer directement SQLite.

## Contexte technique

Le `RuntimeContext` Python contient un attribut `memory` qui est soit une instance de `MemoryInterface` (si l'agent a un `memory_namespace` dans son manifest), soit `None` (si pas de namespace configure). Ce design respecte le Principe #6 d'Apollia : "Memoire a initiative de l'agent" — le runtime ne pre-charge jamais de contexte memoriel automatiquement, c'est l'agent qui decide quand lire ou ecrire.

`MemoryInterface` encapsule un `MemoryManager` qui gere :
- L'isolation par namespace (chaque agent a son propre namespace)
- L'acces ReadWrite pour le namespace propre de l'agent
- L'acces ReadOnly pour les namespaces partages (`shared_memory_namespaces` du manifest)
- L'ouverture lazy des stores SQLite

Les operations exposees couvrent les 3 types de memoire :
- **Episodique** : `record()` — evenements horodates avec importance
- **Semantique** : `remember()`, `recall()`, `forget()` — paires cle/valeur persistantes
- **Recherche** : `search()` — recherche full-text FTS5 avec scores BM25

## Criteres d'Acceptation

### AC-1 : Enregistrement memoire episodique
`memory.record("le client a valide le devis", importance=0.8, task_id="task-123")` stocke un evenement episodique dans le namespace de l'agent.

### AC-2 : Memorisation semantique
`memory.remember("client.dupont.email", "marie@dupont.fr")` stocke une paire cle/valeur dans la memoire semantique du namespace de l'agent.

### AC-3 : Rappel semantique
`memory.recall("client.dupont.email")` retourne `"marie@dupont.fr"` si la cle existe, `None` sinon.

### AC-4 : Recherche full-text
`memory.search("devis Dupont", limit=5)` retourne une liste de resultats avec scores BM25, couvrant memoire episodique et semantique.

### AC-5 : Oubli semantique
`memory.forget("client.dupont.email")` supprime la paire cle/valeur. Un `recall()` subsequent retourne `None`.

### AC-6 : Agent sans namespace
Si `AgentManifest.memory_namespace` est `None` ou vide, `RuntimeContext.memory` est `None` cote Python. Toute tentative d'acces leve `AttributeError` naturellement.

### AC-7 : Namespace partage en lecture seule
Si l'agent accede a un namespace partage (via `shared_memory_namespaces`), les operations d'ecriture (`record()`, `remember()`, `forget()`) retournent `MemoryInterfaceError::ReadOnly`. Seuls `recall()` et `search()` fonctionnent.

## Specification technique

### Types principaux

```rust
use pyo3::prelude::*;
use apollia_memory::manager::MemoryManager;

/// Interface Python pour acceder a la memoire d'un agent.
/// Encapsule un MemoryManager avec le namespace de l'agent.
#[pyclass]
pub struct MemoryInterface {
    manager: MemoryManager,
    namespace: String,
    agent_id: String,
}

/// Erreurs possibles lors des operations memoire via le proxy Python.
#[derive(Debug, thiserror::Error)]
pub enum MemoryInterfaceError {
    #[error("memory operation failed: {0}")]
    OperationFailed(String),
    #[error("namespace is read-only: {0}")]
    ReadOnly(String),
    #[error("no memory namespace configured")]
    NoNamespace,
}
```

### Methodes #[pymethods]

```rust
#[pymethods]
impl MemoryInterface {
    /// Enregistre un evenement episodique dans la memoire de l'agent.
    /// importance: score entre 0.0 et 1.0 (defaut 0.5)
    /// task_id: identifiant de la tache en cours (optionnel)
    fn record<'py>(
        &self,
        py: Python<'py>,
        content: String,
        importance: Option<f64>,
        task_id: Option<String>,
    ) -> PyResult<Bound<'py, PyAny>> {
        // 1. Verifier acces ReadWrite sur le namespace
        // 2. Appeler EpisodicMemory::record() via MemoryManager
        // 3. Retourner Ok(()) ou erreur
        ...
    }

    /// Stocke une paire cle/valeur dans la memoire semantique.
    /// source: provenance de l'information (optionnel)
    fn remember<'py>(
        &self,
        py: Python<'py>,
        key: String,
        value: String,
        source: Option<String>,
    ) -> PyResult<Bound<'py, PyAny>> {
        // 1. Verifier acces ReadWrite sur le namespace
        // 2. Appeler SemanticMemory::remember() via MemoryManager
        ...
    }

    /// Recupere une valeur par cle depuis la memoire semantique.
    /// Retourne la valeur (str) ou None si la cle n'existe pas.
    fn recall<'py>(
        &self,
        py: Python<'py>,
        key: String,
    ) -> PyResult<Bound<'py, PyAny>> {
        // 1. Appeler SemanticMemory::recall() via MemoryManager
        // 2. Retourner Some(value) ou None
        ...
    }

    /// Recherche full-text dans la memoire de l'agent.
    /// Retourne une liste de dicts {content, score, source, timestamp}.
    /// limit: nombre max de resultats (defaut 10)
    fn search<'py>(
        &self,
        py: Python<'py>,
        query: String,
        limit: Option<usize>,
    ) -> PyResult<Bound<'py, PyAny>> {
        // 1. Appeler search::search_fts() via MemoryManager
        // 2. Convertir resultats en liste de dicts Python
        ...
    }

    /// Supprime une paire cle/valeur de la memoire semantique.
    fn forget<'py>(
        &self,
        py: Python<'py>,
        key: String,
    ) -> PyResult<Bound<'py, PyAny>> {
        // 1. Verifier acces ReadWrite sur le namespace
        // 2. Appeler SemanticMemory::forget() via MemoryManager
        ...
    }
}
```

### Constructeur interne

```rust
impl MemoryInterface {
    /// Cree une nouvelle MemoryInterface pour un agent donne.
    /// Retourne None si le namespace est vide ou absent.
    pub(crate) fn new(
        manager: MemoryManager,
        namespace: String,
        agent_id: String,
    ) -> Option<Self> {
        if namespace.is_empty() {
            return None;
        }
        Some(Self {
            manager,
            namespace,
            agent_id,
        })
    }
}
```

### Integration dans RuntimeContext

```rust
#[pyclass]
pub struct RuntimeContext {
    #[pyo3(get)]
    pub tools: ToolProxy,       // STORY-027
    #[pyo3(get)]
    pub memory: Option<MemoryInterface>,  // None si pas de namespace
    // ...
}
```

## Tests requis

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn setup_memory_interface(namespace: &str) -> (MemoryInterface, TempDir) {
        // Cree un MemoryManager avec un repertoire temporaire
        // Retourne l'interface et le TempDir (pour cleanup)
        ...
    }

    // AC-1
    #[tokio::test]
    async fn test_record_episodic_memory() {
        // GIVEN une MemoryInterface avec namespace "agent-alpha"
        // WHEN on appelle record("evenement important", importance=0.9, task_id="t-1")
        // THEN l'evenement est stocke et recuperable via search()
    }

    // AC-2
    #[tokio::test]
    async fn test_remember_semantic_memory() {
        // GIVEN une MemoryInterface avec namespace "agent-alpha"
        // WHEN on appelle remember("contact.email", "test@example.com")
        // THEN la valeur est stockee dans la memoire semantique
    }

    // AC-3
    #[tokio::test]
    async fn test_recall_existing_key() {
        // GIVEN une MemoryInterface avec "contact.email" = "test@example.com"
        // WHEN on appelle recall("contact.email")
        // THEN le resultat est Some("test@example.com")
    }

    #[tokio::test]
    async fn test_recall_missing_key() {
        // GIVEN une MemoryInterface sans donnees
        // WHEN on appelle recall("cle.inexistante")
        // THEN le resultat est None
    }

    // AC-4
    #[tokio::test]
    async fn test_search_fts_with_results() {
        // GIVEN une MemoryInterface avec plusieurs entrees contenant "Dupont"
        // WHEN on appelle search("Dupont", limit=5)
        // THEN le resultat contient les entrees correspondantes avec scores BM25
    }

    // AC-5
    #[tokio::test]
    async fn test_forget_removes_key() {
        // GIVEN une MemoryInterface avec "contact.email" = "test@example.com"
        // WHEN on appelle forget("contact.email")
        // THEN recall("contact.email") retourne None
    }

    // AC-6
    #[test]
    fn test_no_namespace_returns_none() {
        // GIVEN un AgentManifest avec memory_namespace = None
        // WHEN on construit MemoryInterface::new(manager, "", agent_id)
        // THEN le resultat est None
    }

    // AC-7
    #[tokio::test]
    async fn test_shared_namespace_read_only() {
        // GIVEN une MemoryInterface sur un namespace partage (ReadOnly)
        // WHEN on appelle remember("key", "value") ou record("content")
        // THEN l'erreur est MemoryInterfaceError::ReadOnly
        // ET recall("key") et search("query") fonctionnent normalement
    }
}
```

## Definition of Done

- [ ] `MemoryInterface` expose via `#[pyclass]` avec `record()`, `remember()`, `recall()`, `search()`, `forget()`
- [ ] `MemoryInterfaceError` avec `thiserror` (3 variantes)
- [ ] `RuntimeContext.memory` est `None` quand pas de namespace
- [ ] Namespace partage = lecture seule (ecriture retourne erreur)
- [ ] 7+ tests passent (`cargo test -p apollia-aip`)
- [ ] Zero `unwrap()` en production
- [ ] Zero `todo!()` avant commit
- [ ] Docstring `///` sur chaque struct, enum, fn publique
- [ ] `cargo clippy -p apollia-aip` sans warning
- [ ] Principe #6 respecte : aucune injection automatique de memoire

## Ce que cette story N'implemente PAS

- La memoire procedurale (STORY-022 la fournit en Rust, l'interface Python est prevue Sprint 5)
- Le pre-chargement automatique de memoire dans le contexte agent (violerait Principe #6)
- La synchronisation memoire entre agents (hors scope — chaque agent a son namespace isole)
- L'export/import de memoire (fonctionnalite CLI future)
- Les callbacks de notification quand la memoire change (hors scope)

## Notes d'implementation

- `MemoryManager` gere deja l'isolation par namespace et le mode d'acces (ReadWrite/ReadOnly) — ne pas reimplementer cette logique dans `MemoryInterface`
- Les methodes async PyO3 utilisent `pyo3_async_runtimes::tokio::future_into_py()` pour convertir les futures Rust en awaitables Python
- `importance` par defaut a 0.5 si non specifie par l'agent
- `limit` par defaut a 10 pour `search()` si non specifie
- Les resultats de `search()` sont convertis en liste de dicts Python : `{"content": str, "score": float, "source": str, "timestamp": str}`
- `tempfile::TempDir` est utilise dans les tests pour creer des stores SQLite ephemeres

## Liens

- Spec Memory Engine : `docs/Briques-Memory-Engine.md`
- Spec bridge PyO3 : `docs/Briques-AIP-Bridge.md`
- Principe #6 : `docs/Architecture-Principes.md` (Memoire a initiative de l'agent)
- STORY-021 : MemoryManager namespace isolation
- STORY-020 : FTS5 search + BM25
- STORY-022 : ProceduralMemory backend
