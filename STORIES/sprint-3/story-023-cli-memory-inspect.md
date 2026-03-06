# [Sprint 3][apollia-cli] CLI `apollia-os memory inspect` preview

**ID :** STORY-023
**Sprint :** 3
**Crate cible :** `apollia-cli`
**Fichier(s) cible(s) :** `crates/apollia-cli/src/commands/memory.rs`
**Taille :** S
**Depend de :** STORY-021 (MemoryManager avec stats)
**Statut :** ✅ Terminee

---

## User Story

```
En tant que operateur CLI,
je veux inspecter l'etat d'un namespace memoire directement depuis le terminal,
afin de diagnostiquer l'etat de la memoire d'un agent sans ouvrir SQLite manuellement.
```

---

## Contexte technique

Premiere commande CLI reelle du projet. Preview minimaliste qui lit directement
le fichier SQLite (pas besoin du runtime) pour afficher les statistiques du namespace.
Cela pose les fondations de la CLI et valide le pattern clap v4 derive.

**Principe(s) architecturaux concernes :**
- Principe #8 — CLI humaine, API machine (`--json` global, TTY auto-detecte)

**Position dans l'architecture :**
```
apollia-cli
  └── commands/
        └── memory.rs     <- cette story
              └── MemoryCommand (clap subcommand)
                    └── inspect <namespace>
```

---

## Criteres d'Acceptation

### AC-1 — `apollia-os memory inspect <namespace>` affiche les stats

```
ETANT DONNE un fichier ~/.apollia/memory/crm-dupont.db existant avec des donnees
QUAND on execute `apollia-os memory inspect crm-dupont`
ALORS la sortie affiche :
  Namespace   : crm-dupont
  Fichier     : ~/.apollia/memory/crm-dupont.db (X.X MB)
  Episodes    : N
  Semantique  : N cles
  Procedures  : N
```

### AC-2 — `--json` retourne du JSON structure

```
ETANT DONNE un namespace existant
QUAND on execute `apollia-os memory inspect crm-dupont --json`
ALORS la sortie est un JSON valide avec les champs namespace, db_size_bytes,
     episodic_count, semantic_count, procedural_count
```

### AC-3 — Namespace inexistant retourne une erreur claire

```
ETANT DONNE un namespace "nonexistent" sans fichier .db
QUAND on execute `apollia-os memory inspect nonexistent`
ALORS un message d'erreur clair est affiche :
  Error: namespace 'nonexistent' not found (~/.apollia/memory/nonexistent.db does not exist)
ET le code de sortie est 1
```

### AC-4 — `--data-dir` override le repertoire par defaut

```
ETANT DONNE un fichier /tmp/test/my-ns.db existant
QUAND on execute `apollia-os memory inspect my-ns --data-dir /tmp/test`
ALORS les stats du fichier /tmp/test/my-ns.db sont affichees
```

---

## Specification technique

### Types a creer/modifier

```rust
// crates/apollia-cli/src/commands/memory.rs

use clap::Subcommand;

/// Commandes de gestion de la memoire.
#[derive(Debug, Subcommand)]
pub enum MemoryCommand {
    /// Inspecter l'etat d'un namespace memoire.
    Inspect {
        /// Nom du namespace a inspecter.
        namespace: String,

        /// Repertoire des fichiers memoire (defaut: ~/.apollia/memory/).
        #[arg(long, value_name = "DIR")]
        data_dir: Option<std::path::PathBuf>,

        /// Sortie JSON.
        #[arg(long)]
        json: bool,
    },
}
```

```rust
// Modification de crates/apollia-cli/src/main.rs
// Ajouter la sous-commande memory au CLI principal

#[derive(Debug, clap::Parser)]
#[command(name = "apollia-os", version, about = "Apollia OS — Sovereign AI Agent Runtime")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, clap::Subcommand)]
enum Commands {
    /// Memory management.
    Memory {
        #[command(subcommand)]
        command: MemoryCommand,
    },
}
```

### Dependances Cargo

```toml
# crates/apollia-cli/Cargo.toml — ajouter la dependance apollia-memory
[dependencies]
apollia-memory = { workspace = true }
clap = { workspace = true }
serde_json = { workspace = true }
```

### Comportement attendu

- La commande lit directement le fichier `.db` via `MemoryStore::open()` — pas besoin de runtime.
- Le repertoire par defaut est `~/.apollia/memory/` (resolu via `dirs::home_dir()` ou `$HOME`).
- `--json` produit un `serde_json::to_string_pretty(&stats)`.
- Le format humain utilise des unites lisibles (KB, MB) pour la taille fichier.
- Si le namespace n'existe pas, l'erreur est claire avec le chemin attendu.
- Code de sortie : 0 en succes, 1 en erreur.

### Ce que cette story N'implemente PAS

- Les sous-commandes `memory search`, `memory get`, `memory forget`, `memory purge`, `memory export`, `memory import` (Sprint 5)
- La detection TTY pour les couleurs (dette technique acceptable pour le preview)
- Le flag `--json` global (sera refactorise dans Sprint 5 quand plus de commandes existent)

---

## Tests requis

### Tests unitaires dans `crates/apollia-cli/src/commands/memory.rs`

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use apollia_memory::store::MemoryStore;

    fn setup_test_db() -> (std::path::PathBuf, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("apollia_cli_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let db_path = dir.join("test-ns.db");
        let _ = MemoryStore::open(&db_path).unwrap();
        (dir, db_path)
    }

    #[test]
    fn test_ac1_inspect_existing_namespace() {
        // GIVEN
        let (dir, _) = setup_test_db();
        // WHEN — execute inspect logic (unit test the function, not the CLI binary)
        let result = execute_inspect("test-ns", &dir, false);
        // THEN
        assert!(result.is_ok());
    }

    #[test]
    fn test_ac2_inspect_json_output() {
        // GIVEN
        let (dir, _) = setup_test_db();
        // WHEN
        let result = execute_inspect("test-ns", &dir, true);
        // THEN
        assert!(result.is_ok());
        let output = result.unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
        assert_eq!(parsed["namespace"], "test-ns");
    }

    #[test]
    fn test_ac3_nonexistent_namespace_error() {
        // GIVEN
        let dir = std::env::temp_dir().join(format!("apollia_cli_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        // WHEN
        let result = execute_inspect("nonexistent", &dir, false);
        // THEN
        assert!(result.is_err());
    }
}
```

**Note :** Les tests testent la logique `execute_inspect()` extraite en fonction,
pas le binaire CLI. Les tests E2E du binaire sont hors scope Sprint 3.

---

## Definition of Done

**Qualite code :**
- [ ] `cargo test -p apollia-cli` passe (0 test ignore)
- [ ] `cargo clippy -p apollia-cli -- -D warnings` : zero warning
- [ ] `cargo fmt --check` : code formate
- [ ] Zero `unwrap()` dans le code de production
- [ ] Zero `todo!()` dans le code de production
- [ ] Docstring `///` sur chaque struct, enum, et fonction publique

**Architectural :**
- [ ] Principe #8 (CLI humaine, API machine) : `--json` fonctionne
- [ ] Pattern clap v4 derive valide
- [ ] `apollia-memory` importe dans `apollia-cli`

**Commit :**
- [ ] Commit conventionnel : `feat(apollia-cli): add memory inspect command with JSON output`

---

## Notes d'implementation

**Decisions prises pendant l'implementation :**
- `MemoryStats` deplace de `manager.rs` vers `store.rs` pour permettre a `MemoryStore::stats()` de retourner les statistiques sans passer par `MemoryManager`. Re-export via `pub use` dans `manager.rs` pour compatibilite.
- `MemoryStore::stats(namespace, db_path)` ajoute comme methode publique — le CLI lit directement le `.db` sans runtime, comme prevu par la spec.
- `MemoryManager::stats()` simplifie pour deleguer a `MemoryStore::stats()` (suppression de duplication).

**Deviations par rapport a la spec :**
- Aucune deviation significative.

**Dette technique identifiee :**
- Le flag `--json` est local a la sous-commande `inspect`, pas global (prevu Sprint 5).
- Pas de detection TTY pour couleurs (acceptable pour preview).

---

## Liens

- Epic parent : Sprint 3 — Memory Engine
- Story precedente : STORY-022 (ProceduralMemory)
- Story suivante : STORY-024 (Chargement module Python, Sprint 4)
- ADR associe : aucun prevu
