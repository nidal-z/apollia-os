# [Sprint 3][apollia-memory] EpisodicMemory backend (record/history/TTL)

**ID :** STORY-018
**Sprint :** 3
**Crate cible :** `apollia-memory`
**Fichier(s) cible(s) :** `crates/apollia-memory/src/episodic.rs`
**Taille :** M
**Depend de :** STORY-017 (MemoryStore avec schema)
**Statut :** 🔲 A faire

---

## User Story

```
En tant que developpeur d'agent,
je veux enregistrer des episodes (evenements dates) et consulter l'historique,
afin que mon agent puisse se souvenir de ce qui s'est passe dans les taches precedentes
et prendre de meilleures decisions contextuelles.
```

---

## Contexte technique

La memoire episodique est le journal des evenements de l'agent. Chaque episode est date,
porte un score d'importance (0.0 a 1.0), et peut avoir un TTL (expiration). C'est le type
de memoire le plus utilise dans les cas PME (historique devis, interactions client, etc.).

**Principe(s) architecturaux concernes :**
- Principe #6 — Memoire a initiative de l'agent (l'agent appelle `record()`, pas d'injection auto)
- Principe #1 — Local-first (tout dans SQLite)

**Position dans l'architecture :**
```
apollia-memory
  ├── store.rs          (STORY-017 ✅)
  └── episodic.rs       <- cette story
        ├── EpisodicMemory       (struct)
        ├── EpisodicEntry        (struct, retour de history())
        └── EpisodicMemoryError  (enum thiserror)
```

---

## Criteres d'Acceptation

### AC-1 — Enregistrer un episode avec tous les champs

```
ETANT DONNE un MemoryStore ouvert et un EpisodicMemory initialise
QUAND on appelle episodic.record(namespace, agent_id, content, importance, task_id, expires_at, metadata)
ALORS un nouvel enregistrement est insere dans episodic_memories avec un UUID v4
ET le contenu est egalement insere dans memory_fts
ET l'id de l'episode est retourne
```

### AC-2 — Consulter l'historique par namespace

```
ETANT DONNE 5 episodes enregistres dans le namespace "crm-dupont"
QUAND on appelle episodic.history("crm-dupont", limit=3, since=None)
ALORS les 3 episodes les plus recents sont retournes, tries par created_at DESC
```

### AC-3 — Filtrer l'historique par date (since)

```
ETANT DONNE 5 episodes dont 2 datent d'avant 2026-01-01
QUAND on appelle episodic.history("ns", limit=10, since=Some("2026-01-01T00:00:00Z"))
ALORS seuls les 3 episodes posterieurs a cette date sont retournes
```

### AC-4 — Purger les episodes expires

```
ETANT DONNE 3 episodes dont 1 a un expires_at dans le passe
QUAND on appelle episodic.purge_expired("namespace")
ALORS l'episode expire est supprime de episodic_memories ET de memory_fts
ET le nombre d'episodes purges (1) est retourne
```

### AC-5 — Episode sans metadata ni task_id

```
ETANT DONNE les champs optionnels task_id=None et metadata=None
QUAND on appelle episodic.record(namespace, agent_id, content, 0.5, None, None, None)
ALORS l'episode est cree avec task_id=NULL et metadata='{}'
```

---

## Specification technique

### Types a creer dans `crates/apollia-memory/src/episodic.rs`

```rust
use crate::store::MemoryStore;

/// Backend de memoire episodique — journal des evenements de l'agent.
///
/// Chaque episode est date, porte un score d'importance, et peut expirer.
/// L'agent est seul maitre de ce qui est enregistre (Principe #6).
pub struct EpisodicMemory<'a> {
    store: &'a MemoryStore,
}

/// Entree retournee par `history()`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct EpisodicEntry {
    pub id: String,
    pub namespace: String,
    pub agent_id: String,
    pub task_id: Option<String>,
    pub content: String,
    pub importance: f64,
    pub created_at: String,
    pub expires_at: Option<String>,
    pub metadata: serde_json::Value,
}

/// Erreurs du backend episodique.
#[derive(Debug, thiserror::Error)]
pub enum EpisodicMemoryError {
    #[error("failed to record episode: {0}")]
    RecordFailed(String),

    #[error("failed to query history: {0}")]
    QueryFailed(String),

    #[error("invalid importance score: {0} (must be 0.0..=1.0)")]
    InvalidImportance(f64),

    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
}

impl<'a> EpisodicMemory<'a> {
    /// Cree un backend episodique lie a un MemoryStore.
    pub fn new(store: &'a MemoryStore) -> Self { ... }

    /// Enregistre un episode. Retourne l'UUID de l'episode cree.
    pub fn record(
        &self,
        namespace: &str,
        agent_id: &str,
        content: &str,
        importance: f64,
        task_id: Option<&str>,
        expires_at: Option<&str>,
        metadata: Option<&serde_json::Value>,
    ) -> Result<String, EpisodicMemoryError> { ... }

    /// Retourne l'historique du namespace, trie par date descendante.
    pub fn history(
        &self,
        namespace: &str,
        limit: u32,
        since: Option<&str>,
    ) -> Result<Vec<EpisodicEntry>, EpisodicMemoryError> { ... }

    /// Supprime les episodes expires. Retourne le nombre d'episodes purges.
    pub fn purge_expired(
        &self,
        namespace: &str,
    ) -> Result<u64, EpisodicMemoryError> { ... }
}
```

### Dependances Cargo

```toml
# Aucune nouvelle dependance
```

### Comportement attendu

- `record()` genere un UUID v4 pour chaque episode.
- `record()` insere dans `episodic_memories` ET dans `memory_fts` (contenu indexe pour STORY-020).
- `importance` doit etre dans [0.0, 1.0] — sinon `InvalidImportance`.
- `history()` utilise `ORDER BY created_at DESC LIMIT ?`.
- `purge_expired()` supprime les lignes ou `expires_at IS NOT NULL AND expires_at < datetime('now')`.
- `purge_expired()` nettoie aussi les entrees FTS correspondantes.
- Les dates sont en ISO 8601 : `2026-03-05T14:30:00Z`.

### Ce que cette story N'implemente PAS

- La recherche FTS5 (STORY-020) — on insere dans `memory_fts` mais la recherche est dans STORY-020
- Le champ `summary` (consolidation automatique) — hors scope MVP
- L'embedding vectoriel — hors scope MVP
- L'historique filtre par `agent_id` — sera ajoute si besoin dans STORY-021

---

## Tests requis

### Tests unitaires dans `crates/apollia-memory/src/episodic.rs`

```rust
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

    #[test]
    fn test_ac1_record_episode_and_fts_insert() {
        // GIVEN
        let (store, _) = setup();
        let ep = EpisodicMemory::new(&store);
        // WHEN
        let id = ep.record("ns", "agent-1", "Devis envoye a Dupont", 0.8,
            Some("task-1"), None, Some(&json!({"client": "Dupont"}))).unwrap();
        // THEN
        assert!(!id.is_empty());
        // Verify FTS entry exists
        let count: i64 = store.conn().query_row(
            "SELECT COUNT(*) FROM memory_fts WHERE source_id = ?1",
            [&id], |row| row.get(0)).unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn test_ac2_history_returns_recent_first() {
        // GIVEN — 5 episodes
        let (store, _) = setup();
        let ep = EpisodicMemory::new(&store);
        for i in 0..5 {
            ep.record("ns", "agent-1", &format!("Episode {i}"), 0.5,
                None, None, None).unwrap();
        }
        // WHEN
        let history = ep.history("ns", 3, None).unwrap();
        // THEN
        assert_eq!(history.len(), 3);
    }

    #[test]
    fn test_ac4_purge_expired() {
        // GIVEN — 1 expired, 1 not expired
        let (store, _) = setup();
        let ep = EpisodicMemory::new(&store);
        ep.record("ns", "agent-1", "Old episode", 0.5,
            None, Some("2020-01-01T00:00:00Z"), None).unwrap();
        ep.record("ns", "agent-1", "Fresh episode", 0.5,
            None, None, None).unwrap();
        // WHEN
        let purged = ep.purge_expired("ns").unwrap();
        // THEN
        assert_eq!(purged, 1);
        let remaining = ep.history("ns", 10, None).unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].content, "Fresh episode");
    }

    #[test]
    fn test_ac5_optional_fields_default() {
        // GIVEN / WHEN
        let (store, _) = setup();
        let ep = EpisodicMemory::new(&store);
        let id = ep.record("ns", "agent-1", "Simple episode", 0.5,
            None, None, None).unwrap();
        // THEN
        let entry = ep.history("ns", 1, None).unwrap();
        assert_eq!(entry[0].id, id);
        assert!(entry[0].task_id.is_none());
        assert_eq!(entry[0].metadata, json!({}));
    }

    #[test]
    fn test_invalid_importance_rejected() {
        // GIVEN
        let (store, _) = setup();
        let ep = EpisodicMemory::new(&store);
        // WHEN
        let result = ep.record("ns", "agent-1", "Bad", 1.5, None, None, None);
        // THEN
        assert!(matches!(result, Err(EpisodicMemoryError::InvalidImportance(_))));
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
- [ ] Principe #6 respecte : l'agent appelle `record()` explicitement
- [ ] Insertion dans `memory_fts` a chaque `record()`
- [ ] Purge expire nettoie aussi `memory_fts`

**Commit :**
- [ ] Commit conventionnel : `feat(apollia-memory): add EpisodicMemory backend with record, history, and TTL purge`

---

## Notes d'implementation

**Decisions prises pendant l'implementation :**

**Deviations par rapport a la spec :**

**Dette technique identifiee :**

---

## Liens

- Epic parent : Sprint 3 — Memory Engine
- Story precedente : STORY-017 (Schema SQLite)
- Story suivante : STORY-019 (SemanticMemory backend)
- ADR associe : aucun prevu
