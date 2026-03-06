# [Sprint 3][apollia-memory] Schema SQLite complet + migrations versionnees

**ID :** STORY-017
**Sprint :** 3
**Crate cible :** `apollia-memory`
**Fichier(s) cible(s) :** `crates/apollia-memory/src/store.rs`
**Taille :** M
**Depend de :** STORY-016 ✅ (pattern rusqlite WAL valide dans apollia-tools)
**Statut :** ✅ Terminee

---

## User Story

```
En tant que runtime,
je veux un MemoryStore qui cree et migre le schema SQLite (episodic, semantic, procedural, FTS5),
afin que tous les backends memoire disposent d'une base de donnees correctement initialisee
avec WAL, unicode61, et un systeme de migration versionne.
```

---

## Contexte technique

Premiere story de la crate `apollia-memory`. Etablit le schema SQLite et le systeme de migration
sur lequel toutes les stories suivantes (STORY-018 a 023) s'appuient. Le pattern SQLite + WAL
est deja valide dans `apollia-tools/src/audit.rs` — on reutilise l'approche mais avec un schema
plus riche (3 tables de donnees + 1 table FTS5 + 1 table de version).

**Principe(s) architecturaux concernes :**
- Principe #1 — Local-first (un fichier `.db` par namespace, zero cloud)
- Principe #2 — Zero dependance externe (rusqlite bundled, pas de framework de migration)
- Principe #4 — Fail fast (schema invalide detecte au demarrage)

**Position dans l'architecture :**
```
apollia-memory
  └── store.rs        <- cette story
        ├── MemoryStore      (struct, possede la Connection)
        ├── MemoryStoreError (enum thiserror)
        └── SCHEMA_VERSION   (const)
  [importe] apollia-core (types de base)
```

---

## Criteres d'Acceptation

### AC-1 — Creation du schema complet sur base vierge

```
ETANT DONNE un chemin vers un fichier SQLite inexistant
QUAND on appelle MemoryStore::open(path)
ALORS la base est creee avec les tables : episodic_memories, semantic_memories,
      procedural_memories, memory_fts, _schema_version
ET le mode WAL est active
ET _schema_version contient version = 1
```

### AC-2 — FTS5 avec tokenizer unicode61 fonctionne

```
ETANT DONNE un MemoryStore fraichement cree
QUAND on insere un contenu "reunion avec societe" dans memory_fts
ET on recherche "reunion societe"
ALORS les resultats sont retrouves (unicode61 normalise les accents)
```

### AC-3 — Ouverture d'une base existante sans re-migration

```
ETANT DONNE un MemoryStore deja cree avec schema version 1
QUAND on appelle MemoryStore::open(path) une seconde fois
ALORS la base s'ouvre sans erreur
ET aucune table n'est recree (CREATE IF NOT EXISTS)
ET _schema_version reste a 1
```

### AC-4 — Erreur propre si le chemin est invalide

```
ETANT DONNE un chemin vers un repertoire inexistant (/nonexistent/dir/memory.db)
QUAND on appelle MemoryStore::open(path)
ALORS une erreur MemoryStoreError::OpenFailed est retournee avec le contexte
```

### AC-5 — Schema version trackee

```
ETANT DONNE un MemoryStore ouvert
QUAND on appelle store.schema_version()
ALORS la version actuelle du schema est retournee (1 pour le MVP)
```

---

## Specification technique

### Types a creer dans `crates/apollia-memory/src/store.rs`

```rust
use std::path::Path;
use rusqlite::Connection;

/// Version actuelle du schema de la base memoire.
const SCHEMA_VERSION: u32 = 1;

/// Gestionnaire de la base SQLite d'un namespace memoire.
///
/// Un `MemoryStore` correspond a un fichier `<namespace>.db`.
/// Il possede la `Connection` rusqlite et garantit que le schema
/// est correctement initialise et migre au demarrage.
pub struct MemoryStore {
    conn: Connection,
}

/// Erreurs du MemoryStore.
#[derive(Debug, thiserror::Error)]
pub enum MemoryStoreError {
    #[error("failed to open memory database: {0}")]
    OpenFailed(String),

    #[error("schema migration failed at version {version}: {reason}")]
    MigrationFailed { version: u32, reason: String },

    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
}

impl MemoryStore {
    /// Ouvre (ou cree) la base SQLite pour un namespace memoire.
    ///
    /// Active WAL, cree le schema si absent, et applique les migrations
    /// necessaires si la version est inferieure a SCHEMA_VERSION.
    pub fn open(path: &Path) -> Result<Self, MemoryStoreError> { ... }

    /// Retourne la version actuelle du schema.
    pub fn schema_version(&self) -> Result<u32, MemoryStoreError> { ... }

    /// Acces direct a la connexion (pour les backends).
    pub(crate) fn conn(&self) -> &Connection { ... }
}
```

### Schema SQL (version 1)

```sql
-- Table de version
CREATE TABLE IF NOT EXISTS _schema_version (
    version INTEGER NOT NULL
);

-- Memoire episodique
CREATE TABLE IF NOT EXISTS episodic_memories (
    id         TEXT PRIMARY KEY,
    namespace  TEXT NOT NULL,
    task_id    TEXT,
    agent_id   TEXT NOT NULL,
    content    TEXT NOT NULL,
    importance REAL NOT NULL DEFAULT 0.5,
    created_at TEXT NOT NULL,
    expires_at TEXT,
    metadata   TEXT NOT NULL DEFAULT '{}'
);

-- Memoire semantique
CREATE TABLE IF NOT EXISTS semantic_memories (
    id         TEXT PRIMARY KEY,
    namespace  TEXT NOT NULL,
    key        TEXT NOT NULL,
    value      TEXT NOT NULL,
    source     TEXT,
    confidence REAL NOT NULL DEFAULT 1.0,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    expires_at TEXT,
    UNIQUE(namespace, key)
);

-- Memoire procedurale
CREATE TABLE IF NOT EXISTS procedural_memories (
    id            TEXT PRIMARY KEY,
    namespace     TEXT NOT NULL,
    trigger_text  TEXT NOT NULL,
    steps         TEXT NOT NULL,
    success_count INTEGER NOT NULL DEFAULT 1,
    last_used_at  TEXT NOT NULL,
    created_at    TEXT NOT NULL
);

-- Index plein texte FTS5
CREATE VIRTUAL TABLE IF NOT EXISTS memory_fts USING fts5(
    content,
    source_table UNINDEXED,
    source_id UNINDEXED,
    tokenize='unicode61'
);

-- Index pour les requetes par namespace et date
CREATE INDEX IF NOT EXISTS idx_episodic_namespace_created
    ON episodic_memories(namespace, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_semantic_namespace_key
    ON semantic_memories(namespace, key);
CREATE INDEX IF NOT EXISTS idx_procedural_namespace_trigger
    ON procedural_memories(namespace, trigger_text);
```

**Note :** Le champ `trigger` de la spec est renomme `trigger_text` car `trigger` est un mot reserve SQL.

### Dependances Cargo

```toml
# Aucune nouvelle dependance — tout est deja dans apollia-memory/Cargo.toml
# apollia-core, rusqlite (bundled), uuid, serde, serde_json, thiserror, tracing
```

### Comportement attendu

- `MemoryStore::open()` est **synchrone** (comme `audit.rs`) — pas d'acteur Tokio pour le store lui-meme.
- WAL active via `PRAGMA journal_mode=WAL` a l'ouverture.
- Si `_schema_version` est vide, inserer version = 1 apres creation des tables.
- Si `_schema_version.version < SCHEMA_VERSION`, appliquer les migrations incrementales.
- Les dates sont stockees en format ISO 8601 (`YYYY-MM-DDTHH:MM:SSZ`) comme `TEXT`.
- `conn()` expose la connexion en `pub(crate)` — les backends (STORY-018, 019, 022) operent directement dessus.

### Ce que cette story N'implemente PAS

- Les backends episodic/semantic/procedural (STORY-018, 019, 022)
- La recherche FTS5 (STORY-020) — on cree la table FTS5 mais pas la logique de search
- L'embedding vectoriel (sqlite-vec) — hors scope MVP
- Le namespace isolation (STORY-021) — ici on gere un seul fichier DB
- La colonne `summary` de episodic_memories (spec doc) — hors scope MVP, consolidation opt-in v1.0

---

## Tests requis

### Tests unitaires dans `crates/apollia-memory/src/store.rs`

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn temp_db_path() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("apollia_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        dir.join("test.db")
    }

    #[test]
    fn test_ac1_schema_created_on_open() {
        // GIVEN
        let path = temp_db_path();
        // WHEN
        let store = MemoryStore::open(&path).unwrap();
        // THEN — toutes les tables existent
        let tables: Vec<String> = store.conn()
            .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
            .unwrap()
            .query_map([], |row| row.get(0)).unwrap()
            .filter_map(|r| r.ok()).collect();
        assert!(tables.contains(&"episodic_memories".to_string()));
        assert!(tables.contains(&"semantic_memories".to_string()));
        assert!(tables.contains(&"procedural_memories".to_string()));
        assert!(tables.contains(&"_schema_version".to_string()));
    }

    #[test]
    fn test_ac1_wal_mode_active() {
        // GIVEN
        let path = temp_db_path();
        let store = MemoryStore::open(&path).unwrap();
        // WHEN
        let mode: String = store.conn()
            .query_row("PRAGMA journal_mode", [], |row| row.get(0)).unwrap();
        // THEN
        assert_eq!(mode, "wal");
    }

    #[test]
    fn test_ac2_fts5_unicode61_works() {
        // GIVEN
        let path = temp_db_path();
        let store = MemoryStore::open(&path).unwrap();
        // WHEN — insert accented content
        store.conn().execute(
            "INSERT INTO memory_fts(content, source_table, source_id) VALUES (?1, ?2, ?3)",
            ("reunion avec societe", "episodic", "test-id"),
        ).unwrap();
        // THEN — search without accents finds it
        let count: i64 = store.conn().query_row(
            "SELECT COUNT(*) FROM memory_fts WHERE memory_fts MATCH 'reunion'",
            [], |row| row.get(0),
        ).unwrap();
        assert!(count >= 1);
    }

    #[test]
    fn test_ac3_reopen_existing_db() {
        // GIVEN
        let path = temp_db_path();
        let _ = MemoryStore::open(&path).unwrap();
        // WHEN
        let store2 = MemoryStore::open(&path).unwrap();
        // THEN
        assert_eq!(store2.schema_version().unwrap(), SCHEMA_VERSION);
    }

    #[test]
    fn test_ac4_invalid_path_returns_error() {
        // GIVEN
        let path = PathBuf::from("/nonexistent/dir/that/does/not/exist/memory.db");
        // WHEN
        let result = MemoryStore::open(&path);
        // THEN
        assert!(result.is_err());
    }

    #[test]
    fn test_ac5_schema_version_returns_current() {
        // GIVEN
        let path = temp_db_path();
        let store = MemoryStore::open(&path).unwrap();
        // WHEN
        let version = store.schema_version().unwrap();
        // THEN
        assert_eq!(version, SCHEMA_VERSION);
    }
}
```

---

## Definition of Done

**Qualite code :**
- [x] `cargo test -p apollia-memory` passe (7 tests, 0 ignore)
- [x] `cargo clippy -p apollia-memory -- -D warnings` : zero warning
- [x] `cargo fmt --check` : code formate
- [x] Zero `unwrap()` dans le code de production
- [x] Zero `todo!()` dans le code de production
- [x] Docstring `///` sur chaque struct, enum, et fonction publique

**Architectural :**
- [x] `thiserror` utilise pour `MemoryStoreError`, jamais `anyhow`
- [x] WAL active a l'ouverture
- [x] FTS5 avec `unicode61` fonctionne (test AC-2)
- [x] Principe #1 (local-first) : un fichier par namespace
- [x] Principe #4 (fail fast) : schema invalide detecte a l'ouverture

**Commit :**
- [ ] Commit conventionnel : `feat(apollia-memory): add MemoryStore with SQLite schema and versioned migrations`

---

## Notes d'implementation

**Decisions prises pendant l'implementation :**
- FTS5 DDL execute separement de `execute_batch` car certaines builds SQLite
  necessitent une execution individuelle pour les tables virtuelles.
- `conn()` annote `#[allow(dead_code)]` car `pub(crate)` mais pas encore utilise
  (le sera par STORY-018, 019, 022).

**Deviations par rapport a la spec :**
- Aucune.

**Dette technique identifiee :**
- Aucune.

---

## Liens

- Epic parent : Sprint 3 — Memory Engine
- Story precedente : STORY-016 (Audit trail SQLite)
- Story suivante : STORY-018 (EpisodicMemory backend)
- ADR associe : aucun prevu (sauf si FTS5 bundled pose probleme)
