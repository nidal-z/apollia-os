## [SPRINT-0][WORKSPACE] Configurer la CI : cargo fmt + clippy + test

**ID :** STORY-005
**Sprint :** 0
**Crate cible :** workspace (root)
**Fichier(s) cible(s) :**
- `.github/workflows/ci.yml`
- `rustfmt.toml` (configuration rustfmt)
- `clippy.toml` (configuration clippy, optionnel)

**Taille :** S
**Dépend de :** STORY-004
**Statut :** ✅ Terminé

---

## User Story

```
En tant que développeur contribuant au projet Apollia OS,
je veux une CI GitHub Actions qui vérifie automatiquement le format, les warnings clippy
et les tests à chaque push,
afin de garantir qu'aucun code non-conforme ne soit mergé dans main.
```

---

## Contexte technique

Le projet est développé sur soir/weekend en mode solo. La CI remplace la rigueur d'une code review équipe. Elle doit bloquer tout commit qui :
- Ne passe pas `cargo fmt --check`
- Génère des warnings clippy (`-D warnings`)
- Échoue des tests (`cargo test --workspace`)

La CI doit être rapide (< 5 minutes) pour ne pas bloquer le rythme de développement. Elle utilise la cache Rust pour éviter de recompiler les dépendances à chaque run.

**Principe(s) architectural(aux) concerné(s) :**
- Principe #4 — Fail fast : La CI détecte les problèmes avant le merge, pas en production

**Position dans l'architecture :**
```
.github/workflows/ci.yml  ← cette story
  └── déclenché sur push/PR vers main
  └── jobs : fmt → clippy → test (séquentiels pour fail-fast)
```

---

## Critères d'Acceptation

### AC-1 — La CI se déclenche sur push et PR vers main

```
ÉTANT DONNÉ un push ou une PR vers la branche main
QUAND GitHub Actions traite l'événement
ALORS le workflow ci.yml se déclenche automatiquement
```

### AC-2 — Un code non-formatté fait échouer la CI

```
ÉTANT DONNÉ du code Rust avec indentation incorrecte committé dans une PR
QUAND le job fmt s'exécute avec `cargo fmt --check`
ALORS le job échoue avec exit code non-zéro et la CI bloque le merge
```

### AC-3 — Un warning clippy fait échouer la CI

```
ÉTANT DONNÉ du code Rust avec un unused variable warning non supprimé
QUAND le job clippy s'exécute avec `cargo clippy --workspace -- -D warnings`
ALORS le job échoue et la CI bloque le merge
```

### AC-4 — Un test en échec bloque la CI

```
ÉTANT DONNÉ un test unitaire qui assert!(false)
QUAND le job test s'exécute avec `cargo test --workspace`
ALORS le job échoue et la CI bloque le merge
```

### AC-5 — La CI utilise la cache Rust pour les builds rapides

```
ÉTANT DONNÉ un second run de la CI sur le même code sans changement de Cargo.lock
QUAND la CI s'exécute
ALORS le temps de compilation des dépendances est < 30s (cache hit)
```

---

## Spécification technique

### GitHub Actions workflow

```yaml
# .github/workflows/ci.yml
name: CI

on:
  push:
    branches: [main]
  pull_request:
    branches: [main]

env:
  CARGO_TERM_COLOR: always
  RUST_BACKTRACE: 1

jobs:
  fmt:
    name: Format
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: rustfmt
      - name: cargo fmt --check
        run: cargo fmt --all -- --check

  clippy:
    name: Clippy
    runs-on: ubuntu-latest
    needs: fmt
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: clippy
      - uses: Swatinem/rust-cache@v2
      - name: cargo clippy
        run: cargo clippy --workspace -- -D warnings

  test:
    name: Test
    runs-on: ubuntu-latest
    needs: clippy
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      - name: cargo test
        run: cargo test --workspace
```

### rustfmt.toml

```toml
# rustfmt.toml
edition = "2021"
max_width = 100
use_small_heuristics = "Default"
imports_granularity = "Crate"
group_imports = "StdExternalCrate"
```

### Dépendances Cargo

```toml
# Aucune — CI uniquement, pas de dépendances Rust
```

### Ce que cette story N'implémente PAS

- Release pipeline (build binaire + upload artifacts)
- Tests d'intégration cross-crate (feront l'objet de stories dédiées)
- Publication crates.io
- Benchmarks

---

## Tests requis

### Vérification manuelle avant commit

```bash
# Vérifier localement ce que la CI vérifiera :

# 1. Format
cargo fmt --all -- --check

# 2. Clippy sans warnings
cargo clippy --workspace -- -D warnings

# 3. Tests
cargo test --workspace

# 4. Si tout passe : commit autorisé
```

### Test de la CI elle-même

Il n'y a pas de test automatisé pour la CI. La validation est :
- Créer une PR avec du code volontairement mal formatté → CI doit échouer sur `fmt`
- Corriger → CI doit passer

---

## Definition of Done

**CI :**
- [x] `.github/workflows/ci.yml` créé et valide
- [x] La CI se déclenche sur la branche main (vérifiable dans GitHub Actions)
- [x] Les 3 jobs (fmt, clippy, test) passent sur le code du workspace actuel

**Qualité :**
- [x] `rustfmt.toml` configuré avec `edition = "2021"`
- [x] `cargo fmt --all -- --check` passe localement
- [x] `cargo clippy --workspace -- -D warnings` : zéro warning localement
- [x] `cargo test --workspace` : tous les tests passent localement

**Commit :**
- [x] Commit conventionnel : `chore(ci): add GitHub Actions workflow for fmt, clippy and test`

---

## Notes d'implémentation

**Décisions prises pendant l'implémentation :**
- `imports_granularity = "Crate"` et `group_imports = "StdExternalCrate"` retirés de `rustfmt.toml` : ces options sont nightly-only et génèrent des warnings sur stable, qui est le toolchain de la CI.

**Déviations par rapport à la spec :**
- `rustfmt.toml` réduit à 3 options stables (`edition`, `max_width`, `use_small_heuristics`) au lieu des 5 de la spec — les 2 options nightly exclues ne changent pas le comportement sur stable.

**Dette technique identifiée :**
- Aucune

---

## Liens

- Epic parent : Sprint 0 — Fondations
- Story précédente : STORY-004
- Story suivante : STORY-006 (Sprint 1 — EventBus)
- ADR associé : aucun
