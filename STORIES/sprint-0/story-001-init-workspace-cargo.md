## [SPRINT-0][WORKSPACE] Initialiser le workspace Cargo avec les 7 crates

**ID :** STORY-001
**Sprint :** 0
**Crate cible :** workspace (root)
**Fichier(s) cible(s) :**
- `Cargo.toml` — workspace manifest avec `[workspace.dependencies]`
- `crates/apollia-core/Cargo.toml` + `src/lib.rs`
- `crates/apollia-runtime/Cargo.toml` + `src/lib.rs`
- `crates/apollia-oria/Cargo.toml` + `src/lib.rs`
- `crates/apollia-tools/Cargo.toml` + `src/lib.rs`
- `crates/apollia-memory/Cargo.toml` + `src/lib.rs`
- `crates/apollia-aip/Cargo.toml` + `src/lib.rs`
- `crates/apollia-cli/Cargo.toml` + `src/main.rs`

**Taille :** S
**Dépend de :** aucune
**Statut :** 🔲 À faire

---

## User Story

```
En tant que développeur contribuant au projet Apollia OS,
je veux un workspace Cargo structuré avec les 7 crates vides compilant sans erreur,
afin de disposer d'une base de travail valide pour toutes les stories suivantes.
```

---

## Contexte technique

Le workspace Cargo est la fondation de tout le projet. Sans lui, aucune autre story ne peut démarrer. Il doit déclarer les 7 crates dans le bon ordre de dépendance, centraliser toutes les versions de dépendances dans `[workspace.dependencies]`, et compiler avec `cargo build --workspace` sans aucune erreur ni warning.

Le graphe de dépendances est strict (aucune dépendance circulaire autorisée) :
- `apollia-core` ne dépend de rien du workspace
- `apollia-runtime`, `apollia-tools`, `apollia-memory` dépendent de `apollia-core`
- `apollia-oria`, `apollia-aip` dépendent de `apollia-core` + `apollia-tools` + `apollia-memory`
- `apollia-cli` dépend de `apollia-runtime`

**Principe(s) architectural(aux) concerné(s) :**
- Principe #2 — Zéro dépendance externe : `rusqlite` bundlé, `pyo3` in-process

**Position dans l'architecture :**
```
workspace root (Cargo.toml)
  ├── apollia-core       ← fondation, zéro dépendance workspace
  ├── apollia-tools      ← dépend de core
  ├── apollia-memory     ← dépend de core
  ├── apollia-runtime    ← dépend de core
  ├── apollia-oria       ← dépend de core + tools + memory
  ├── apollia-aip        ← dépend de core + tools + memory
  └── apollia-cli        ← dépend de runtime (binaire final)
```

---

## Critères d'Acceptation

### AC-1 — Le workspace compile sans erreur

```
ÉTANT DONNÉ le workspace Cargo avec les 7 crates déclarées dans Cargo.toml
QUAND on exécute `cargo build --workspace`
ALORS la compilation réussit avec 0 erreur et 0 warning
```

### AC-2 — Les dépendances circulaires sont impossibles

```
ÉTANT DONNÉ le graphe de dépendances déclaré dans les Cargo.toml individuels
QUAND on exécute `cargo metadata --no-deps`
ALORS apollia-core n'a aucune dépendance workspace,
  ET apollia-cli ne dépend pas directement de apollia-oria, apollia-tools, apollia-memory, apollia-aip
```

### AC-3 — Les versions sont centralisées dans le workspace

```
ÉTANT DONNÉ les Cargo.toml des crates individuelles
QUAND on inspecte chaque [dependencies]
ALORS aucune version de dépendance externe n'est spécifiée inline (toutes utilisent { workspace = true })
```

### AC-4 — anyhow est absent de toutes les dépendances

```
ÉTANT DONNÉ le workspace Cargo.toml et les 7 Cargo.toml de crates
QUAND on recherche "anyhow" dans tous les fichiers Cargo.toml
ALORS aucune occurrence n'est trouvée
```

---

## Spécification technique

### Structure Cargo.toml workspace

```toml
[workspace]
resolver = "2"
members = [
    "crates/apollia-core",
    "crates/apollia-runtime",
    "crates/apollia-oria",
    "crates/apollia-tools",
    "crates/apollia-memory",
    "crates/apollia-aip",
    "crates/apollia-cli",
]

[workspace.dependencies]
# Async runtime
tokio = { version = "1", features = ["full"] }
# Serialization
serde = { version = "1", features = ["derive"] }
serde_json = "1"
# Errors — JAMAIS anyhow
thiserror = "2"
# Logging
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter", "fmt"] }
# IDs
uuid = { version = "1", features = ["v4", "serde"] }
# API
axum = { version = "0.7", features = ["macros"] }
tower = "0.4"
tower-http = { version = "0.5", features = ["trace"] }
# CLI
clap = { version = "4", features = ["derive"] }
# SQLite bundlé (Principe #2)
rusqlite = { version = "0.32", features = ["bundled"] }
# Python bridge
pyo3 = { version = "0.22", features = ["auto-initialize"] }
pyo3-async-runtimes = { version = "0.22", features = ["tokio-runtime"] }
# Internal
apollia-core    = { path = "crates/apollia-core" }
apollia-runtime = { path = "crates/apollia-runtime" }
apollia-oria    = { path = "crates/apollia-oria" }
apollia-tools   = { path = "crates/apollia-tools" }
apollia-memory  = { path = "crates/apollia-memory" }
apollia-aip     = { path = "crates/apollia-aip" }
```

### apollia-cli/src/main.rs (minimal)

```rust
//! Apollia OS — CLI binary entry point.
//! Commands implemented in STORY-037 and STORY-038.

fn main() {
    eprintln!("apollia-os: not yet implemented");
    std::process::exit(1);
}
```

### Dépendances Cargo

Toutes déclarées dans `[workspace.dependencies]` du root `Cargo.toml`.
Les crates individuelles n'ont que `{ workspace = true }`.

### Ce que cette story N'implémente PAS

- Aucun type Rust (AgentManifest, AIPTask, etc.) — STORY-002
- Aucune logique métier
- Aucun test au-delà de `cargo build`
- Pas de CI — STORY-005

---

## Tests requis

### Tests de build

```bash
# AC-1 : build complet sans erreur
cargo build --workspace

# AC-4 : vérifier l'absence de anyhow
grep -r "anyhow" crates/*/Cargo.toml  # doit retourner 0 ligne

# Vérification du graphe de dépendances
cargo metadata --format-version 1 | jq '.packages[] | select(.name == "apollia-core") | .dependencies'
# doit retourner [] (aucune dépendance workspace)
```

### Test unitaire (minimal — la crate doit compiler)

```rust
// Dans chaque crates/apollia-XXX/src/lib.rs :
// Pas de tests unitaires à ce stade — la story suivante les ajoute.
// Le test de cette story est la compilation sans erreur.
```

---

## Definition of Done

**Qualité code :**
- [ ] `cargo build --workspace` : 0 erreur, 0 warning
- [ ] `cargo check --workspace` : 0 erreur, 0 warning
- [ ] `cargo fmt --check` : code formatté
- [ ] Zéro `anyhow` dans aucun Cargo.toml du workspace

**Architectural :**
- [ ] 7 crates déclarées dans le workspace
- [ ] Graphe de dépendances conforme (core → outils/mémoire/runtime → oria/aip → cli)
- [ ] Toutes les versions dans `[workspace.dependencies]`

**Documentation :**
- [ ] Chaque `lib.rs` a un docstring `//!` décrivant le rôle de la crate
- [ ] `main.rs` a un commentaire expliquant pourquoi il est vide

**Commit :**
- [ ] Commit conventionnel : `feat(workspace): init Cargo workspace with 7 skeleton crates`

---

## Notes d'implémentation

**Décisions prises pendant l'implémentation :**

**Déviations par rapport à la spec :**

**Dette technique identifiée :**

---

## Liens

- Epic parent : Sprint 0 — Fondations
- Story précédente : aucune
- Story suivante : STORY-002
- ADR associé : ADR-001 (Rust comme langage runtime), ADR-010 (Pivot SaaS → Runtime)
