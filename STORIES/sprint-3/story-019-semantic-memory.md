# [Sprint 3][apollia-memory] SemanticMemory backend (remember/recall/forget)

**ID :** STORY-019
**Sprint :** 3
**Crate cible :** `apollia-memory`
**Fichier(s) cible(s) :** `crates/apollia-memory/src/semantic.rs`
**Taille :** M
**Depend de :** STORY-017 (MemoryStore avec schema)
**Statut :** ✅ Terminee

---

## User Story

```
En tant que developpeur d'agent,
je veux stocker des connaissances structurees (cle/valeur avec confiance et source),
afin que mon agent puisse se souvenir de faits durables sur les clients, preferences,
et configurations sans les redemander a chaque tache.
```

---

## Contexte technique

La memoire semantique est la base de connaissances de l'agent. Chaque entree est une paire
cle/valeur avec un score de confiance, une source d'origine, et un TTL optionnel.
Les cles sont hiérarchiques par convention (`client.dupont.budget_max`) mais pas enforced.

**Principe(s) architecturaux concernes :**
- Principe #6 — Memoire a initiative de l'agent (l'agent appelle `remember()`)
- Principe #1 — Local-first (tout dans SQLite)

**Position dans l'architecture :**
```
apollia-memory
  ├── store.rs          (STORY-017 ✅)
  ├── episodic.rs       (STORY-018)
  └── semantic.rs       <- cette story
        ├── SemanticMemory       (struct)
        ├── SemanticEntry        (struct, retour de recall/list)
        └── SemanticMemoryError  (enum thiserror)
```

---

## Criteres d'Acceptation

### AC-1 — Stocker une connaissance (remember)

```
ETANT DONNE un MemoryStore ouvert et un SemanticMemory initialise
QUAND on appelle semantic.remember(namespace, key, value, confidence, source, expires_at)
ALORS un nouvel enregistrement est insere dans semantic_memories avec un UUID v4
ET la valeur est egalement indexee dans memory_fts
```

### AC-2 — Recuperer une connaissance (recall)

```
ETANT DONNE une connaissance stockee avec la cle "client.dupont.budget_max"
QUAND on appelle semantic.recall(namespace, "client.dupont.budget_max")
ALORS l'entree SemanticEntry est retournee avec value, confidence, source
```

### AC-3 — Upsert sur cle existante (remember avec cle deja presente)

```
ETANT DONNE une connaissance existante cle="client.dupont.budget" value="15000"
QUAND on appelle semantic.remember(namespace, "client.dupont.budget", "20000", ...)
ALORS la valeur est mise a jour a "20000"
ET updated_at est rafraichi
ET l'entree FTS est mise a jour
```

### AC-4 — Supprimer une connaissance (forget)

```
ETANT DONNE une connaissance existante cle="client.dupont.old_email"
QUAND on appelle semantic.forget(namespace, "client.dupont.old_email")
ALORS l'entree est supprimee de semantic_memories ET de memory_fts
ET le retour est Ok(true)
```

### AC-5 — Recall d'une cle inexistante retourne None

```
ETANT DONNE un namespace sans la cle "inexistante"
QUAND on appelle semantic.recall(namespace, "inexistante")
ALORS Ok(None) est retourne (pas d'erreur)
```

### AC-6 — Forget d'une cle inexistante retourne Ok(false)

```
ETANT DONNE un namespace sans la cle "inexistante"
QUAND on appelle semantic.forget(namespace, "inexistante")
ALORS Ok(false) est retourne (pas d'erreur, rien supprime)
```

---

## Specification technique

### Types a creer dans `crates/apollia-memory/src/semantic.rs`

```rust
use crate::store::MemoryStore;

/// Backend de memoire semantique — base de connaissances structuree.
///
/// Stocke des paires cle/valeur avec confiance et source.
/// Les cles sont hiérarchiques par convention (`client.dupont.budget_max`).
/// L'upsert est natif : `remember()` avec une cle existante met a jour.
pub struct SemanticMemory<'a> {
    store: &'a MemoryStore,
}

/// Entree retournee par `recall()`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SemanticEntry {
    pub id: String,
    pub namespace: String,
    pub key: String,
    pub value: serde_json::Value,
    pub source: Option<String>,
    pub confidence: f64,
    pub created_at: String,
    pub updated_at: String,
    pub expires_at: Option<String>,
}

/// Erreurs du backend semantique.
#[derive(Debug, thiserror::Error)]
pub enum SemanticMemoryError {
    #[error("failed to store knowledge: {0}")]
    StoreFailed(String),

    #[error("failed to recall knowledge: {0}")]
    RecallFailed(String),

    #[error("invalid confidence score: {0} (must be 0.0..=1.0)")]
    InvalidConfidence(f64),

    #[error("empty key")]
    EmptyKey,

    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
}

impl<'a> SemanticMemory<'a> {
    /// Cree un backend semantique lie a un MemoryStore.
    pub fn new(store: &'a MemoryStore) -> Self { ... }

    /// Stocke ou met a jour une connaissance.
    /// Si la cle existe deja dans le namespace, la valeur est mise a jour (upsert).
    pub fn remember(
        &self,
        namespace: &str,
        key: &str,
        value: &serde_json::Value,
        confidence: f64,
        source: Option<&str>,
        expires_at: Option<&str>,
    ) -> Result<String, SemanticMemoryError> { ... }

    /// Recupere une connaissance par cle. None si absente.
    pub fn recall(
        &self,
        namespace: &str,
        key: &str,
    ) -> Result<Option<SemanticEntry>, SemanticMemoryError> { ... }

    /// Supprime une connaissance. Retourne true si supprimee, false si absente.
    pub fn forget(
        &self,
        namespace: &str,
        key: &str,
    ) -> Result<bool, SemanticMemoryError> { ... }

    /// Supprime les connaissances expirees. Retourne le nombre purge.
    pub fn purge_expired(
        &self,
        namespace: &str,
    ) -> Result<u64, SemanticMemoryError> { ... }
}
```

### Dependances Cargo

```toml
# Aucune nouvelle dependance
```

### Comportement attendu

- `remember()` utilise `INSERT ... ON CONFLICT(namespace, key) DO UPDATE` pour l'upsert.
- Lors d'un upsert, `updated_at` est rafraichi et l'entree FTS est mise a jour (delete + re-insert).
- `value` est stocke comme `serde_json::Value` serialise en TEXT JSON.
- `confidence` doit etre dans [0.0, 1.0] — sinon `InvalidConfidence`.
- `key` ne doit pas etre vide — sinon `EmptyKey`.
- `forget()` supprime aussi l'entree FTS correspondante.
- `purge_expired()` fonctionne comme dans EpisodicMemory.

### Ce que cette story N'implemente PAS

- La recherche FTS5 par contenu (STORY-020)
- L'historique des modifications d'une cle (pas de versioning)
- La propagation de changements entre namespaces (STORY-021)

---

## Tests requis

### Tests unitaires dans `crates/apollia-memory/src/semantic.rs`

```rust
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

    #[test]
    fn test_ac1_remember_stores_entry_and_fts() {
        // GIVEN
        let (store, _) = setup();
        let sem = SemanticMemory::new(&store);
        // WHEN
        let id = sem.remember("ns", "client.dupont.budget", &json!(15000),
            1.0, Some("crm-agent"), None).unwrap();
        // THEN
        assert!(!id.is_empty());
        let fts_count: i64 = store.conn().query_row(
            "SELECT COUNT(*) FROM memory_fts WHERE source_id = ?1",
            [&id], |row| row.get(0)).unwrap();
        assert_eq!(fts_count, 1);
    }

    #[test]
    fn test_ac2_recall_existing_key() {
        // GIVEN
        let (store, _) = setup();
        let sem = SemanticMemory::new(&store);
        sem.remember("ns", "client.dupont.budget", &json!(15000),
            0.9, Some("crm"), None).unwrap();
        // WHEN
        let entry = sem.recall("ns", "client.dupont.budget").unwrap();
        // THEN
        assert!(entry.is_some());
        let e = entry.unwrap();
        assert_eq!(e.value, json!(15000));
        assert_eq!(e.confidence, 0.9);
    }

    #[test]
    fn test_ac3_upsert_updates_value() {
        // GIVEN
        let (store, _) = setup();
        let sem = SemanticMemory::new(&store);
        sem.remember("ns", "key", &json!("old"), 1.0, None, None).unwrap();
        // WHEN
        sem.remember("ns", "key", &json!("new"), 0.8, None, None).unwrap();
        // THEN
        let entry = sem.recall("ns", "key").unwrap().unwrap();
        assert_eq!(entry.value, json!("new"));
        assert_eq!(entry.confidence, 0.8);
    }

    #[test]
    fn test_ac4_forget_removes_entry() {
        // GIVEN
        let (store, _) = setup();
        let sem = SemanticMemory::new(&store);
        sem.remember("ns", "old_key", &json!("val"), 1.0, None, None).unwrap();
        // WHEN
        let removed = sem.forget("ns", "old_key").unwrap();
        // THEN
        assert!(removed);
        assert!(sem.recall("ns", "old_key").unwrap().is_none());
    }

    #[test]
    fn test_ac5_recall_nonexistent_returns_none() {
        // GIVEN
        let (store, _) = setup();
        let sem = SemanticMemory::new(&store);
        // WHEN / THEN
        assert!(sem.recall("ns", "nope").unwrap().is_none());
    }

    #[test]
    fn test_ac6_forget_nonexistent_returns_false() {
        // GIVEN
        let (store, _) = setup();
        let sem = SemanticMemory::new(&store);
        // WHEN / THEN
        assert!(!sem.forget("ns", "nope").unwrap());
    }

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

    #[test]
    fn test_invalid_confidence_rejected() {
        // GIVEN
        let (store, _) = setup();
        let sem = SemanticMemory::new(&store);
        // WHEN
        let result = sem.remember("ns", "k", &json!("v"), -0.1, None, None);
        // THEN
        assert!(matches!(result, Err(SemanticMemoryError::InvalidConfidence(_))));
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
- [ ] Upsert via `ON CONFLICT` SQL natif
- [ ] Insertion/mise a jour de `memory_fts` a chaque `remember()`
- [ ] `forget()` nettoie aussi `memory_fts`

**Commit :**
- [ ] Commit conventionnel : `feat(apollia-memory): add SemanticMemory backend with remember, recall, forget`

---

## Notes d'implementation

**Decisions prises pendant l'implementation :**
- Upsert implementé via SELECT + UPDATE/INSERT plutot que `ON CONFLICT DO UPDATE`, car il faut gérer le nettoyage FTS (delete + re-insert) dans le même flux. Le résultat fonctionnel est identique.
- Le contenu FTS indexe `"{key} {value}"` pour permettre la recherche par clé et par valeur.
- `chrono_now_utc()` réutilise le pattern de `episodic.rs` (délégation à SQLite `strftime`).

**Deviations par rapport a la spec :**
- Pas d'utilisation de `ON CONFLICT` SQL natif — remplacé par un SELECT préalable pour gérer le FTS proprement (voir décision ci-dessus). Pas d'ADR nécessaire, c'est un détail d'implémentation.

**Dette technique identifiee :**
- La fonction `chrono_now_utc()` est dupliquée entre `episodic.rs` et `semantic.rs`. A factoriser dans un module utilitaire commun si un 3e backend l'utilise.

---

## Liens

- Epic parent : Sprint 3 — Memory Engine
- Story precedente : STORY-018 (EpisodicMemory)
- Story suivante : STORY-020 (FTS5 search)
- ADR associe : aucun prevu
