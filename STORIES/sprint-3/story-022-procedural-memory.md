# [Sprint 3][apollia-memory] ProceduralMemory backend

**ID :** STORY-022
**Sprint :** 3
**Crate cible :** `apollia-memory`
**Fichier(s) cible(s) :** `crates/apollia-memory/src/procedural.rs`
**Taille :** S
**Depend de :** STORY-017 (MemoryStore avec schema)
**Statut :** ✅ Terminee

---

## User Story

```
En tant que developpeur d'agent,
je veux stocker des workflows (trigger → steps) qui ont bien fonctionne,
afin que mon agent puisse reutiliser des sequences d'actions eprouvees
pour des situations recurrentes.
```

---

## Contexte technique

La memoire procedurale stocke des patterns trigger→steps. Quand un agent reconnaît
une situation similaire, il peut recuperer le workflow associe plutot que de repartir
de zero. Chaque procedure a un compteur de succes et une date de dernier usage.

**Principe(s) architecturaux concernes :**
- Principe #6 — Memoire a initiative de l'agent (l'agent appelle `learn_procedure()`)

**Position dans l'architecture :**
```
apollia-memory
  ├── store.rs          (STORY-017 ✅)
  ├── episodic.rs       (STORY-018)
  ├── semantic.rs       (STORY-019)
  ├── search.rs         (STORY-020)
  ├── manager.rs        (STORY-021)
  └── procedural.rs     <- cette story
        ├── ProceduralMemory       (struct)
        ├── ProcedureEntry         (struct)
        └── ProceduralMemoryError  (enum thiserror)
```

---

## Criteres d'Acceptation

### AC-1 — Apprendre une nouvelle procedure

```
ETANT DONNE un MemoryStore ouvert et un ProceduralMemory initialise
QUAND on appelle procedural.learn(namespace, trigger, steps)
ALORS un enregistrement est insere dans procedural_memories avec success_count=1
ET l'ID de la procedure est retourne
```

### AC-2 — Recuperer une procedure par trigger

```
ETANT DONNE une procedure stockee avec trigger="devis client grand compte"
QUAND on appelle procedural.recall(namespace, "devis client grand compte")
ALORS les steps sont retournes en ordre
ET success_count et last_used_at sont disponibles
```

### AC-3 — Upsert incremente success_count

```
ETANT DONNE une procedure existante avec trigger="devis grand compte" et success_count=3
QUAND on appelle procedural.learn(namespace, "devis grand compte", memes_steps)
ALORS success_count passe a 4
ET last_used_at est rafraichi
```

### AC-4 — Recall d'un trigger inexistant retourne None

```
ETANT DONNE un namespace sans procedure "inexistante"
QUAND on appelle procedural.recall(namespace, "inexistante")
ALORS Ok(None) est retourne
```

### AC-5 — Lister les procedures d'un namespace

```
ETANT DONNE 3 procedures dans un namespace
QUAND on appelle procedural.list(namespace)
ALORS les 3 procedures sont retournees, triees par success_count DESC
```

---

## Specification technique

### Types a creer dans `crates/apollia-memory/src/procedural.rs`

```rust
use crate::store::MemoryStore;

/// Backend de memoire procedurale — workflows appris par l'agent.
///
/// Stocke des patterns trigger→steps avec compteur de succes.
pub struct ProceduralMemory<'a> {
    store: &'a MemoryStore,
}

/// Entree retournee par `recall()` et `list()`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ProcedureEntry {
    pub id: String,
    pub namespace: String,
    pub trigger: String,
    pub steps: Vec<String>,
    pub success_count: u32,
    pub last_used_at: String,
    pub created_at: String,
}

/// Erreurs du backend procedural.
#[derive(Debug, thiserror::Error)]
pub enum ProceduralMemoryError {
    #[error("failed to learn procedure: {0}")]
    LearnFailed(String),

    #[error("empty trigger")]
    EmptyTrigger,

    #[error("empty steps list")]
    EmptySteps,

    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
}

impl<'a> ProceduralMemory<'a> {
    /// Cree un backend procedural lie a un MemoryStore.
    pub fn new(store: &'a MemoryStore) -> Self { ... }

    /// Apprend une procedure. Si le trigger existe, incremente success_count.
    pub fn learn(
        &self,
        namespace: &str,
        trigger: &str,
        steps: &[String],
    ) -> Result<String, ProceduralMemoryError> { ... }

    /// Recupere une procedure par trigger exact. None si absente.
    pub fn recall(
        &self,
        namespace: &str,
        trigger: &str,
    ) -> Result<Option<ProcedureEntry>, ProceduralMemoryError> { ... }

    /// Liste toutes les procedures d'un namespace, triees par success_count DESC.
    pub fn list(
        &self,
        namespace: &str,
    ) -> Result<Vec<ProcedureEntry>, ProceduralMemoryError> { ... }
}
```

### Dependances Cargo

```toml
# Aucune nouvelle dependance
```

### Comportement attendu

- `learn()` genere un UUID v4 pour une nouvelle procedure.
- `learn()` avec un trigger existant dans le meme namespace fait un UPDATE : `success_count += 1`, `last_used_at` rafraichi, `steps` mis a jour.
- `steps` est stocke comme JSON array (`serde_json::to_string(&steps)`).
- `recall()` fait un `SELECT WHERE namespace = ? AND trigger_text = ?`.
- `list()` fait un `SELECT WHERE namespace = ? ORDER BY success_count DESC`.
- Le trigger est match exact (pas de FTS ici — la memoire procedurale est consultee par trigger precis).
- Validation : trigger non-vide, steps non-vide.

### Ce que cette story N'implemente PAS

- La recherche FTS sur les triggers (matching flou) — potentiel v1.0
- La suppression de procedures (pas de cas d'usage identifie au MVP)
- Le scoring de similarite entre triggers

---

## Tests requis

### Tests unitaires dans `crates/apollia-memory/src/procedural.rs`

```rust
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
    fn test_ac1_learn_new_procedure() {
        // GIVEN
        let (store, _) = setup();
        let proc = ProceduralMemory::new(&store);
        // WHEN
        let id = proc.learn("ns", "devis grand compte",
            &["Verifier SIRET".into(), "Calculer remise".into()]).unwrap();
        // THEN
        assert!(!id.is_empty());
    }

    #[test]
    fn test_ac2_recall_existing_procedure() {
        // GIVEN
        let (store, _) = setup();
        let proc = ProceduralMemory::new(&store);
        proc.learn("ns", "devis grand compte",
            &["Verifier SIRET".into(), "Calculer remise".into()]).unwrap();
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
    fn test_ac3_learn_again_increments_count() {
        // GIVEN
        let (store, _) = setup();
        let proc = ProceduralMemory::new(&store);
        proc.learn("ns", "trigger",
            &["step1".into()]).unwrap();
        // WHEN
        proc.learn("ns", "trigger",
            &["step1".into(), "step2".into()]).unwrap();
        // THEN
        let entry = proc.recall("ns", "trigger").unwrap().unwrap();
        assert_eq!(entry.success_count, 2);
        assert_eq!(entry.steps.len(), 2); // steps updated
    }

    #[test]
    fn test_ac4_recall_nonexistent_returns_none() {
        // GIVEN
        let (store, _) = setup();
        let proc = ProceduralMemory::new(&store);
        // WHEN / THEN
        assert!(proc.recall("ns", "nope").unwrap().is_none());
    }

    #[test]
    fn test_ac5_list_sorted_by_success_count() {
        // GIVEN
        let (store, _) = setup();
        let proc = ProceduralMemory::new(&store);
        proc.learn("ns", "rare", &["step".into()]).unwrap();
        proc.learn("ns", "popular", &["step".into()]).unwrap();
        proc.learn("ns", "popular", &["step".into()]).unwrap(); // count=2
        proc.learn("ns", "popular", &["step".into()]).unwrap(); // count=3
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
- [ ] Upsert via `ON CONFLICT` ou `INSERT OR REPLACE` pour le trigger existant
- [ ] Steps stockes comme JSON array
- [ ] Pas d'insertion dans memory_fts (la memoire procedurale est exact-match)

**Commit :**
- [ ] Commit conventionnel : `feat(apollia-memory): add ProceduralMemory backend with learn and recall`

---

## Notes d'implementation

**Decisions prises pendant l'implementation :**
- Upsert via SELECT + UPDATE/INSERT (meme pattern que SemanticMemory) plutot que ON CONFLICT — la table `procedural_memories` n'a pas de UNIQUE constraint sur (namespace, trigger_text), uniquement un index. Ce choix est coherent avec le reste du codebase.
- Ajout d'un test `test_namespace_isolation` et `test_list_empty_namespace` en plus des 7 tests specifies, pour 9 tests total.

**Deviations par rapport a la spec :** Aucune.

**Dette technique identifiee :** Aucune.

---

## Liens

- Epic parent : Sprint 3 — Memory Engine
- Story precedente : STORY-021 (MemoryManager)
- Story suivante : STORY-023 (CLI memory inspect)
- ADR associe : aucun prevu
