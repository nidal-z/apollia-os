## [SPRINT-0][CORE] Implémenter ProcessState, TaskStatus et AIPError avec serde

**ID :** STORY-003
**Sprint :** 0
**Crate cible :** `apollia-core`
**Fichier(s) cible(s) :**
- `crates/apollia-core/src/process.rs`
- `crates/apollia-core/src/result.rs` (complétion avec TaskStatus)
- `crates/apollia-core/src/lib.rs` (re-exports)

**Taille :** S
**Dépend de :** STORY-002
**Statut :** 🔲 À faire

---

## User Story

```
En tant que runtime Apollia OS,
je veux des enums Rust sérialisables représentant les machines d'état ProcessState et TaskStatus,
afin de suivre le cycle de vie des agents et des tâches de manière typée et sans ambiguïté.
```

---

## Contexte technique

Apollia OS maintient deux machines d'état indépendantes (documentées dans Architecture-Vue-Ensemble.md) :
1. **ProcessState** : état du processus agent (géré par AgentRegistry, Sprint 1)
2. **TaskStatus** : état d'une tâche (géré par TaskRouter + ORIA, Sprint 4)

Ces types doivent être définis dans `apollia-core` car ils sont utilisés par `apollia-runtime`, `apollia-oria`, et `apollia-aip`. Les transitions d'état elles-mêmes sont implémentées dans les crates concernées — ici on définit uniquement les valeurs possibles.

**Principe(s) architectural(aux) concerné(s) :**
- Principe #4 — Fail fast : ProcessState::Initializing est l'état où toutes les erreurs doivent être détectées

**Position dans l'architecture :**
```
apollia-core/src/
  ├── process.rs    ← ProcessState enum  ← cette story
  └── result.rs     ← TaskStatus enum    ← cette story (complète STORY-002)

apollia-runtime (Sprint 1)
  └── AgentRegistry  ← utilise ProcessState

apollia-oria (Sprint 4)
  └── TaskRouter    ← utilise TaskStatus
```

---

## Critères d'Acceptation

### AC-1 — ProcessState couvre tous les états du lifecycle agent

```
ÉTANT DONNÉ l'enum ProcessState
QUAND on liste ses variants
ALORS il contient exactement : Initializing, Active, Degraded, Stopping, Stopped
```

### AC-2 — TaskStatus couvre tous les états du lifecycle tâche

```
ÉTANT DONNÉ l'enum TaskStatus
QUAND on liste ses variants
ALORS il contient exactement : Submitted, Working, Completed, Failed, InputRequired, Canceled
```

### AC-3 — Les enums sont sérialisables en snake_case JSON

```
ÉTANT DONNÉ ProcessState::Active et TaskStatus::InputRequired
QUAND on les sérialise avec serde_json
ALORS le JSON produit est respectivement "active" et "input_required"
```

### AC-4 — La désérialisation d'une valeur inconnue retourne une erreur

```
ÉTANT DONNÉ le JSON "unknown_state"
QUAND on désérialise en ProcessState
ALORS serde_json::from_str retourne Err (pas de panic, pas de valeur par défaut)
```

---

## Spécification technique

### Types à créer

```rust
// crates/apollia-core/src/process.rs

/// Machine d'état du processus agent, alignée ACP (Agent Communication Protocol).
///
/// Transitions valides :
/// `Initializing` → `Active` → `Degraded` → `Stopping` → `Stopped`
///                               ↑                           ↑
///                          (outils optionnels          (erreur fatale,
///                            manquants)                skip vers Stopping)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProcessState {
    /// Résolution des outils, validation du manifest, ouverture SQLite.
    /// Toute erreur ici = échec de démarrage (Principe #4 — Fail fast).
    Initializing,
    /// Prêt à accepter des tâches.
    Active,
    /// Actif mais avec des `tools_optional` manquants ou dégradés.
    Degraded,
    /// Drain des tâches en cours (timeout 30s). Refuse les nouvelles tâches.
    Stopping,
    /// Arrêt propre. Plus aucune tâche acceptée.
    Stopped,
}

// À ajouter dans crates/apollia-core/src/result.rs (complète STORY-002)

/// Machine d'état d'une tâche individuelle, alignée A2A TaskState.
///
/// Transitions valides :
/// `Submitted` → `Working` → `Completed`
///                    ↓           ↑ (reprise après input)
///                `InputRequired` → `Working`
///                    ↓
///              `Failed` | `Canceled`
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    /// Tâche reçue, en attente d'un agent disponible.
    Submitted,
    /// Tâche en cours d'exécution par l'agent.
    Working,
    /// Tâche terminée avec succès.
    Completed,
    /// L'agent a rencontré une erreur non récupérable.
    Failed,
    /// L'agent attend une entrée humaine pour continuer (Human-in-the-Loop).
    InputRequired,
    /// Tâche annulée par l'opérateur ou timeout.
    Canceled,
}
```

### Dépendances Cargo

```toml
# Aucune nouvelle dépendance — serde déjà déclaré dans STORY-001
```

### Ce que cette story N'implémente PAS

- Les transitions d'état (validées par AgentRegistry → STORY-007, TaskRouter → STORY-032)
- Le `StepBudget` runtime → STORY-004 + STORY-030
- Les erreurs typed par crate (CoreError, RuntimeError...) → chaque crate gère les siennes

---

## Tests requis

### Tests unitaires

```rust
// crates/apollia-core/src/process.rs

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ac1_process_state_variants_exist() {
        // GIVEN / WHEN / THEN
        let states = [
            ProcessState::Initializing,
            ProcessState::Active,
            ProcessState::Degraded,
            ProcessState::Stopping,
            ProcessState::Stopped,
        ];
        assert_eq!(states.len(), 5);
    }

    #[test]
    fn test_ac3_process_state_serializes_snake_case() {
        // GIVEN
        let state = ProcessState::Active;
        // WHEN
        let json = serde_json::to_string(&state).expect("serialize failed");
        // THEN
        assert_eq!(json, "\"active\"");
    }

    #[test]
    fn test_ac3_task_status_input_required_serializes() {
        // GIVEN
        let status = TaskStatus::InputRequired;
        // WHEN
        let json = serde_json::to_string(&status).expect("serialize failed");
        // THEN
        assert_eq!(json, "\"input_required\"");
    }

    #[test]
    fn test_ac4_unknown_state_deserializes_to_error() {
        // GIVEN
        let invalid = "\"unknown_state\"";
        // WHEN
        let result: Result<ProcessState, _> = serde_json::from_str(invalid);
        // THEN
        assert!(result.is_err());
    }
}
```

---

## Definition of Done

**Qualité code :**
- [ ] `cargo test -p apollia-core` passe (0 test ignoré)
- [ ] `cargo clippy -p apollia-core -- -D warnings` : zéro warning
- [ ] `cargo fmt --check` : code formatté
- [ ] Zéro `unwrap()` dans le code de production
- [ ] Docstring `///` sur chaque variant public des enums

**Architectural :**
- [ ] `ProcessState` a exactement 5 variants (Initializing, Active, Degraded, Stopping, Stopped)
- [ ] `TaskStatus` a exactement 6 variants (Submitted, Working, Completed, Failed, InputRequired, Canceled)
- [ ] Sérialisation snake_case vérifiée par test

**Commit :**
- [ ] Commit conventionnel : `feat(apollia-core): add ProcessState and TaskStatus lifecycle enums`

---

## Notes d'implémentation

**Décisions prises pendant l'implémentation :**

**Déviations par rapport à la spec :**

**Dette technique identifiée :**

---

## Liens

- Epic parent : Sprint 0 — Fondations
- Story précédente : STORY-002
- Story suivante : STORY-004
- ADR associé : aucun (implémentation directe de la spec AIP)
