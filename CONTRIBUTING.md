# Contribuer à Apollia OS

---

## Prérequis

| Outil | Version | Installation |
|---|---|---|
| Rust | stable (1.75+) | `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \| sh` |
| Python | 3.11+ | `python.org` ou `brew install python@3.13` (macOS) |
| SQLite | 3.35+ (FTS5 activé) | Inclus dans la plupart des distributions |

Vérifier :

```bash
rustc --version   # >= 1.75
python3 --version # >= 3.11
sqlite3 --version # >= 3.35 (FTS5 requis)
```

---

## Build

```bash
git clone https://github.com/nidal-z/apollia-os.git
cd apollia-os

# Build complet du workspace
cargo build --workspace

# Sur macOS, PyO3 requiert de pointer vers un Python explicite
export PYO3_PYTHON=/opt/homebrew/bin/python3.13
cargo build --workspace
```

---

## Tests

```bash
# Tests unitaires et d'intégration (sans Python réel)
cargo test --workspace

# Tests exercant la chaîne Python complète
PYO3_PYTHON=/opt/homebrew/bin/python3.13 \
  cargo test --workspace --features python-tests

# Tests d'une crate spécifique
cargo test -p apollia-runtime

# Avec logs
cargo test -p apollia-runtime -- --nocapture
```

Les tests doivent tous passer avant chaque commit. `cargo clippy --workspace -- -D warnings` ne doit produire aucun warning.

---

## Conventions de commit

Commits conventionnels obligatoires, scope = nom de la crate :

```
feat(apollia-core): add AgentManifest type
fix(apollia-runtime): prevent duplicate agent registration
refactor(apollia-tools): extract sandbox logic to module
test(apollia-memory): add FTS5 search regression test
docs(apollia-aip): document ToolProxy Python API
chore(workspace): update tokio to 1.36
```

**Règles :**
- Un commit = une story ou une sous-tâche logique
- Jamais de commit avec `cargo test` qui échoue
- Jamais de `unwrap()` en code de production (uniquement dans les tests)
- Zéro `todo!()` avant de committer

---

## Branches

```
feature/<STORY-NNN>-short-description
bugfix/<STORY-NNN>-description
refactor/<STORY-NNN>-description
```

---

## Règles de code Rust

- `thiserror` pour toutes les erreurs dans les crates du workspace — `anyhow` interdit
- Pattern acteur Tokio : `mpsc::channel` + Handle clonable — jamais `Arc<Mutex<T>>` cross-acteurs
- `tracing::info!(champ = %val, "event")` — jamais de format string dans les logs
- Docstring `///` sur chaque struct, enum, fn publique

---

## Décisions architecturales (ADR)

Toute décision significative doit être documentée dans un ADR :

```bash
# Utiliser le template
cp docs/adr/ADR-Template.md docs/adr/ADR-NNN-titre-kebab.md
```

Voir [docs/adr/](docs/adr/) pour les 19 ADR existants et le template.

---

## Reporter un bug ou proposer une feature

Ouvrir une [issue GitHub](https://github.com/nidal-z/apollia-os/issues) avec :
- Pour un bug : version, OS, commandes exactes, output complet
- Pour une feature : cas d'usage, comportement attendu, impact sur l'architecture

---

## Licence

En contribuant, vous acceptez que votre code soit distribué sous la licence Apache-2.0 du projet.
