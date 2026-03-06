# [Sprint 3][apollia-memory] FTS5 search avec tokenizer unicode61 + BM25

**ID :** STORY-020
**Sprint :** 3
**Crate cible :** `apollia-memory`
**Fichier(s) cible(s) :** `crates/apollia-memory/src/search.rs`
**Taille :** M
**Depend de :** STORY-017 ✅, STORY-018, STORY-019 (donnees dans FTS)
**Statut :** 🔲 A faire

---

## User Story

```
En tant que developpeur d'agent,
je veux rechercher dans la memoire de mon agent par mots-cles avec un classement par pertinence,
afin de retrouver rapidement les episodes et connaissances les plus pertinents
pour la tache en cours.
```

---

## Contexte technique

STORY-018 et 019 inserent les contenus dans la table `memory_fts` a chaque ecriture.
Cette story implemente la recherche unifiee FTS5 avec ranking BM25 par-dessus.
C'est la story qui realise le Sprint Goal : `memory.search("devis Dupont")` retourne
des resultats classes.

Le tokenizer `unicode61` est deja configure dans le schema (STORY-017) — il normalise
les accents pour le francais (`reunion` match `reunion`, `societe` match `societe`).

**Principe(s) architecturaux concernes :**
- Principe #1 — Local-first (FTS5 integre dans SQLite, zero cloud)
- Principe #6 — Memoire a initiative de l'agent (la recherche est explicite)

**Position dans l'architecture :**
```
apollia-memory
  ├── store.rs          (STORY-017 ✅)
  ├── episodic.rs       (STORY-018)
  ├── semantic.rs       (STORY-019)
  └── search.rs         <- cette story
        ├── MemorySearch       (struct)
        ├── SearchResult       (struct)
        ├── SearchSource       (enum)
        └── MemorySearchError  (enum thiserror)
```

---

## Criteres d'Acceptation

### AC-1 — Recherche par mots-cles retourne des resultats classes BM25

```
ETANT DONNE 5 episodes et 3 connaissances semantiques indexees dans memory_fts
QUAND on appelle search.query("devis Dupont", limit=3, sources=None, min_importance=None)
ALORS les resultats sont retournes tries par score BM25 descendant
ET chaque resultat indique sa source (episodic/semantic) et son ID
```

### AC-2 — Filtrage par source (episodic uniquement)

```
ETANT DONNE des entries episodiques et semantiques dans memory_fts
QUAND on appelle search.query("devis", limit=10, sources=Some(["episodic"]), min_importance=None)
ALORS seules les entries episodiques sont retournees
```

### AC-3 — Filtrage par importance minimale

```
ETANT DONNE des episodes avec importance 0.3, 0.5, 0.8
QUAND on appelle search.query("devis", limit=10, sources=None, min_importance=Some(0.6))
ALORS seul l'episode avec importance 0.8 est retourne
```

### AC-4 — Unicode61 normalise les accents

```
ETANT DONNE un episode "reunion avec societe Dupont"
QUAND on recherche "reunion societe"
ALORS l'episode est retrouve (les accents sont normalises par unicode61)
```

### AC-5 — Requete sans resultats retourne un vecteur vide

```
ETANT DONNE un namespace avec des donnees
QUAND on recherche un terme absent "xyznonexistent"
ALORS Ok(vec![]) est retourne (pas d'erreur)
```

---

## Specification technique

### Types a creer dans `crates/apollia-memory/src/search.rs`

```rust
use crate::store::MemoryStore;

/// Moteur de recherche FTS5 avec ranking BM25.
///
/// Interroge la table `memory_fts` et joint les tables source
/// (episodic/semantic) pour enrichir les resultats.
pub struct MemorySearch<'a> {
    store: &'a MemoryStore,
}

/// Resultat d'une recherche memoire.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SearchResult {
    /// Score BM25 (plus negatif = plus pertinent, normalise en positif).
    pub score: f64,
    /// Table source du resultat.
    pub source: SearchSource,
    /// ID de l'entree dans la table source.
    pub source_id: String,
    /// Contenu textuel du resultat.
    pub content: String,
    /// Importance (episodic) ou confiance (semantic). None si non applicable.
    pub relevance: Option<f64>,
    /// Date de creation.
    pub created_at: String,
}

/// Source d'un resultat de recherche.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum SearchSource {
    Episodic,
    Semantic,
}

/// Erreurs de recherche.
#[derive(Debug, thiserror::Error)]
pub enum MemorySearchError {
    #[error("search query failed: {0}")]
    QueryFailed(String),

    #[error("empty search query")]
    EmptyQuery,

    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
}

impl<'a> MemorySearch<'a> {
    /// Cree un moteur de recherche lie a un MemoryStore.
    pub fn new(store: &'a MemoryStore) -> Self { ... }

    /// Recherche dans la memoire par mots-cles avec ranking BM25.
    ///
    /// - `namespace` : filtre par namespace
    /// - `query` : requete FTS5 (mots-cles)
    /// - `limit` : nombre max de resultats
    /// - `sources` : filtrer par type (None = tous)
    /// - `min_importance` : seuil d'importance/confiance (None = pas de filtre)
    pub fn query(
        &self,
        namespace: &str,
        query: &str,
        limit: u32,
        sources: Option<&[SearchSource]>,
        min_importance: Option<f64>,
    ) -> Result<Vec<SearchResult>, MemorySearchError> { ... }
}
```

### Requete SQL cible

```sql
-- Recherche FTS5 avec BM25
SELECT
    memory_fts.content,
    memory_fts.source_table,
    memory_fts.source_id,
    rank  -- BM25 score (negatif, plus proche de 0 = meilleur)
FROM memory_fts
WHERE memory_fts MATCH ?1
ORDER BY rank
LIMIT ?2
```

Le score FTS5 `rank` est negatif par defaut (convention BM25). La normalisation
en positif se fait via `abs(rank)` ou `-rank` pour l'affichage.

Pour le filtrage par namespace et importance, on fait un JOIN sur la table source :

```sql
SELECT f.content, f.source_table, f.source_id, f.rank
FROM memory_fts f
JOIN episodic_memories e ON f.source_table = 'episodic' AND f.source_id = e.id
WHERE memory_fts MATCH ?1
AND e.namespace = ?2
AND e.importance >= ?3
ORDER BY f.rank
LIMIT ?4
```

L'implementation fera un UNION ALL pour combiner episodic et semantic.

### Dependances Cargo

```toml
# Aucune nouvelle dependance
```

### Comportement attendu

- `query()` construit une requete FTS5 MATCH avec les mots-cles fournis.
- Les resultats sont tries par BM25 score (descendant = plus pertinent d'abord).
- Le score BM25 brut est negatif (convention SQLite FTS5). Le `SearchResult.score` est normalise en positif.
- Si `sources` est fourni, seules les tables specifiees sont interrogees.
- Si `min_importance` est fourni, un JOIN avec la table source filtre par `importance >= min_importance` (episodic) ou `confidence >= min_importance` (semantic).
- `query` vide → `EmptyQuery` error.
- Les caracteres speciaux FTS5 dans la requete utilisateur doivent etre echappes (guillemets, `*`, `AND`, `OR`, `NOT`, `NEAR`).

### Ce que cette story N'implemente PAS

- La recherche vectorielle (sqlite-vec) — hors scope MVP
- La recherche hybride FTS5 + vectoriel — v1.0
- Le highlighting/snippet des resultats
- La recherche cross-namespace (STORY-021)

---

## Tests requis

### Tests unitaires dans `crates/apollia-memory/src/search.rs`

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::MemoryStore;
    use crate::episodic::EpisodicMemory;
    use crate::semantic::SemanticMemory;
    use serde_json::json;

    fn setup_with_data() -> MemoryStore {
        let dir = std::env::temp_dir().join(format!("apollia_search_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test.db");
        let store = MemoryStore::open(&path).unwrap();

        let ep = EpisodicMemory::new(&store);
        ep.record("ns", "agent-1", "Devis envoye a Dupont SA pour 5000 euros", 0.8,
            Some("t1"), None, None).unwrap();
        ep.record("ns", "agent-1", "Reunion planifiee avec Martin SAS", 0.5,
            None, None, None).unwrap();
        ep.record("ns", "agent-1", "Devis refuse par Dupont SA budget insuffisant", 0.9,
            None, None, None).unwrap();

        let sem = SemanticMemory::new(&store);
        sem.remember("ns", "client.dupont.budget_max", &json!(15000),
            1.0, Some("crm"), None).unwrap();

        store
    }

    #[test]
    fn test_ac1_search_returns_bm25_ranked_results() {
        // GIVEN
        let store = setup_with_data();
        let search = MemorySearch::new(&store);
        // WHEN
        let results = search.query("ns", "devis Dupont", 3, None, None).unwrap();
        // THEN
        assert!(!results.is_empty());
        assert!(results.len() <= 3);
        // Results should be sorted by score (higher = more relevant)
        for i in 1..results.len() {
            assert!(results[i - 1].score >= results[i].score);
        }
    }

    #[test]
    fn test_ac2_filter_by_episodic_source() {
        // GIVEN
        let store = setup_with_data();
        let search = MemorySearch::new(&store);
        // WHEN
        let results = search.query("ns", "Dupont", 10,
            Some(&[SearchSource::Episodic]), None).unwrap();
        // THEN
        for r in &results {
            assert_eq!(r.source, SearchSource::Episodic);
        }
    }

    #[test]
    fn test_ac3_filter_by_min_importance() {
        // GIVEN
        let store = setup_with_data();
        let search = MemorySearch::new(&store);
        // WHEN
        let results = search.query("ns", "Dupont", 10, None, Some(0.7)).unwrap();
        // THEN
        for r in &results {
            if let Some(rel) = r.relevance {
                assert!(rel >= 0.7);
            }
        }
    }

    #[test]
    fn test_ac4_unicode61_accent_normalization() {
        // GIVEN
        let dir = std::env::temp_dir().join(format!("apollia_s_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let store = MemoryStore::open(&dir.join("test.db")).unwrap();
        let ep = EpisodicMemory::new(&store);
        ep.record("ns", "a", "reunion avec societe Dupont", 0.5, None, None, None).unwrap();
        let search = MemorySearch::new(&store);
        // WHEN — search without accents
        let results = search.query("ns", "reunion societe", 10, None, None).unwrap();
        // THEN
        assert!(!results.is_empty());
    }

    #[test]
    fn test_ac5_no_results_returns_empty_vec() {
        // GIVEN
        let store = setup_with_data();
        let search = MemorySearch::new(&store);
        // WHEN
        let results = search.query("ns", "xyznonexistent", 10, None, None).unwrap();
        // THEN
        assert!(results.is_empty());
    }

    #[test]
    fn test_empty_query_returns_error() {
        // GIVEN
        let store = setup_with_data();
        let search = MemorySearch::new(&store);
        // WHEN
        let result = search.query("ns", "", 10, None, None);
        // THEN
        assert!(matches!(result, Err(MemorySearchError::EmptyQuery)));
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
- [ ] BM25 ranking via FTS5 `rank` natif
- [ ] Unicode61 valide (accents normalises)
- [ ] Sprint Goal demo-able : `search("devis Dupont")` retourne des resultats classes

**Commit :**
- [ ] Commit conventionnel : `feat(apollia-memory): add FTS5 search with BM25 ranking and unicode61`

---

## Notes d'implementation

**Decisions prises pendant l'implementation :**

**Deviations par rapport a la spec :**

**Dette technique identifiee :**

---

## Liens

- Epic parent : Sprint 3 — Memory Engine
- Story precedente : STORY-019 (SemanticMemory)
- Story suivante : STORY-021 (MemoryManager namespace isolation)
- ADR associe : aucun prevu
