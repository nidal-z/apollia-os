# [Sprint 4][apollia-oria] Observer + ContextBundle + classify()

**ID :** STORY-029
**Sprint :** 4
**Crate cible :** `apollia-oria`
**Fichier(s) cible(s) :** `crates/apollia-oria/src/observer.rs`
**Taille :** M (3h)
**Depend de :** apollia-memory Sprint 3 (STORY-017 a STORY-022 toutes livrees), apollia-core Sprint 0 (STORY-001 a STORY-005 toutes livrees)
**Statut :** Done ✅

---

## User Story

En tant que moteur ORIA, je veux enrichir une AIPTask en ContextBundle (snapshot memoire, historique, etat agent) et classifier sa complexite (Direct vs Orchestre), afin de choisir le mode d'execution optimal pour chaque tache recue.

## Contexte technique

L'Observer est le premier composant du pipeline ORIA (Observer-Reasoner-Initiator-Actor) declenche a la reception d'une tache. Son role est double :

1. **Enrichissement** : construire un `ContextBundle` qui regroupe la tache originale, un snapshot de la memoire pertinente (episodes recents + faits semantiques), et le mode d'execution determine.

2. **Classification** : determiner si la tache doit etre executee en mode Direct (un seul step) ou Orchestre (multi-step avec planification). L'algorithme est base sur des heuristiques simples issues de la spec ORIA.

L'Observer est une **fonction pure** (pas un acteur Tokio) — il prend des entrees et retourne un resultat sans etat interne. Ce choix simplifie les tests et le raisonnement.

L'algorithme de classification :
```
is_complex = manifest.step_budget.max_steps > 15
  || task.input.parts.len() > 3
  || manifest.tags.contains("multi-step")
  || manifest.tools_required.len() > 4
```

Pour Sprint 4, seul le mode Direct est implemente dans le pipeline. Le mode Orchestre est detecte et etiquete mais son execution est prevue Sprint 6 (STORY-043).

## Criteres d'Acceptation

### AC-1 : Observation avec memoire
`observe(task, manifest, Some(&memory_manager))` retourne un `ContextBundle` avec `memory_snapshot` peuple (episodes recents + faits semantiques pertinents).

### AC-2 : Observation sans memoire
`observe(task, manifest, None)` retourne un `ContextBundle` avec `memory_snapshot = None`. Pas d'erreur.

### AC-3 : Classification agent simple vers Direct
`classify(task, manifest)` avec un manifest ayant 2 outils requis, max_steps=10, pas de tag "multi-step", et une tache avec 1 input part retourne `ExecutionMode::Direct`.

### AC-4 : Classification agent complexe vers Orchestrated
`classify(task, manifest)` avec un manifest ayant 5 outils requis, max_steps=20 retourne `ExecutionMode::Orchestrated`.

### AC-5 : Tag multi-step force Orchestrated
`classify(task, manifest)` avec un manifest contenant le tag "multi-step" retourne `ExecutionMode::Orchestrated`, meme si les autres criteres sont simples.

### AC-6 : Nombre d'input parts force Orchestrated
`classify(task, manifest)` avec une tache ayant 4+ input parts retourne `ExecutionMode::Orchestrated`, meme si le manifest est simple.

## Specification technique

### Types principaux

```rust
use apollia_core::{AIPTask, AgentManifest};
use apollia_memory::manager::MemoryManager;

/// Mode d'execution determine par l'Observer pour une tache.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecutionMode {
    /// Execution simple en un seul step.
    Direct,
    /// Execution multi-step avec planification.
    Orchestrated,
}

/// Snapshot de la memoire pertinente pour une tache.
/// Construit par l'Observer lors de l'enrichissement.
#[derive(Debug, Clone)]
pub struct MemorySnapshot {
    /// Episodes recents de la memoire episodique (contenu textuel).
    pub episodic_recent: Vec<String>,
    /// Faits semantiques pertinents (paires cle/valeur).
    pub semantic_relevant: Vec<(String, String)>,
}

/// Bundle de contexte enrichi pour une tache.
/// Produit par observe(), consomme par le Reasoner.
#[derive(Debug, Clone)]
pub struct ContextBundle {
    /// Tache originale.
    pub task: AIPTask,
    /// Snapshot memoire (None si pas de namespace configure).
    pub memory_snapshot: Option<MemorySnapshot>,
    /// Mode d'execution determine par classify().
    pub execution_mode: ExecutionMode,
}

/// Erreurs possibles lors de l'observation.
#[derive(Debug, thiserror::Error)]
pub enum ObserverError {
    #[error("failed to build context: {0}")]
    ContextBuildFailed(String),
    #[error("memory access failed: {0}")]
    MemoryError(String),
}
```

### Fonctions publiques

```rust
/// Classifie une tache en mode Direct ou Orchestre.
/// Fonction pure basee sur des heuristiques simples.
///
/// Criteres de complexite (un seul suffit pour Orchestrated) :
/// - manifest.step_budget.max_steps > 15
/// - task.input.parts.len() > 3
/// - manifest.tags contient "multi-step"
/// - manifest.tools_required.len() > 4
pub fn classify(task: &AIPTask, manifest: &AgentManifest) -> ExecutionMode {
    let is_complex = manifest.step_budget.max_steps > 15
        || task.input.parts.len() > 3
        || manifest.tags.iter().any(|t| t == "multi-step")
        || manifest.tools_required.len() > 4;

    if is_complex {
        ExecutionMode::Orchestrated
    } else {
        ExecutionMode::Direct
    }
}

/// Observe une tache et construit un ContextBundle enrichi.
///
/// Si un MemoryManager est fourni, charge un snapshot memoire
/// (episodes recents + faits semantiques pertinents).
/// Si aucun MemoryManager n'est fourni, memory_snapshot est None.
///
/// Le mode d'execution est determine par classify().
pub fn observe(
    task: AIPTask,
    manifest: &AgentManifest,
    memory: Option<&MemoryManager>,
) -> Result<ContextBundle, ObserverError> {
    let execution_mode = classify(&task, manifest);

    let memory_snapshot = match memory {
        Some(mgr) => {
            // 1. Recuperer les N derniers episodes (N = 10 par defaut)
            // 2. Recuperer tous les faits semantiques du namespace
            // 3. Construire MemorySnapshot
            ...
        }
        None => None,
    };

    Ok(ContextBundle {
        task,
        memory_snapshot,
        execution_mode,
    })
}
```

### Constantes

```rust
/// Nombre maximum d'episodes recents charges dans le snapshot.
const MAX_RECENT_EPISODES: usize = 10;

/// Seuil de max_steps au-dessus duquel la tache est consideree complexe.
const COMPLEXITY_STEP_THRESHOLD: u32 = 15;

/// Seuil de nombre d'input parts au-dessus duquel la tache est complexe.
const COMPLEXITY_PARTS_THRESHOLD: usize = 3;

/// Seuil de nombre d'outils requis au-dessus duquel la tache est complexe.
const COMPLEXITY_TOOLS_THRESHOLD: usize = 4;
```

## Tests requis

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use apollia_core::{AIPTask, AIPInput, AIPPart, TextPart, AgentManifest, StepBudgetConfig};
    use tempfile::TempDir;

    fn simple_manifest() -> AgentManifest {
        // Manifest avec 2 tools_required, max_steps=10,
        // pas de tag "multi-step", dangerous_tools_allowed=false
        ...
    }

    fn complex_manifest() -> AgentManifest {
        // Manifest avec 5 tools_required, max_steps=20,
        // dangerous_tools_allowed=false
        ...
    }

    fn simple_task() -> AIPTask {
        // Tache avec 1 input part (TextPart)
        ...
    }

    fn multi_part_task() -> AIPTask {
        // Tache avec 4 input parts
        ...
    }

    // AC-1
    #[tokio::test]
    async fn test_observe_with_memory_populates_snapshot() {
        // GIVEN un MemoryManager avec des episodes et des faits semantiques
        //   dans un namespace "agent-test"
        // WHEN on appelle observe(task, manifest, Some(&manager))
        // THEN le ContextBundle a memory_snapshot != None
        //   ET episodic_recent contient les episodes recents
        //   ET semantic_relevant contient les faits pertinents
    }

    // AC-2
    #[test]
    fn test_observe_without_memory_snapshot_is_none() {
        // GIVEN une tache et un manifest simples
        // WHEN on appelle observe(task, manifest, None)
        // THEN le ContextBundle a memory_snapshot == None
        //   ET pas d'erreur
    }

    // AC-3
    #[test]
    fn test_classify_simple_agent_returns_direct() {
        // GIVEN un manifest avec 2 outils, max_steps=10, pas de tag "multi-step"
        //   ET une tache avec 1 input part
        // WHEN on appelle classify(&task, &manifest)
        // THEN le resultat est ExecutionMode::Direct
    }

    // AC-4
    #[test]
    fn test_classify_complex_agent_returns_orchestrated() {
        // GIVEN un manifest avec 5 outils, max_steps=20
        //   ET une tache avec 1 input part
        // WHEN on appelle classify(&task, &manifest)
        // THEN le resultat est ExecutionMode::Orchestrated
    }

    // AC-5
    #[test]
    fn test_classify_multi_step_tag_returns_orchestrated() {
        // GIVEN un manifest simple (2 outils, max_steps=10)
        //   MAIS avec le tag "multi-step"
        //   ET une tache avec 1 input part
        // WHEN on appelle classify(&task, &manifest)
        // THEN le resultat est ExecutionMode::Orchestrated
    }

    // AC-6
    #[test]
    fn test_classify_many_input_parts_returns_orchestrated() {
        // GIVEN un manifest simple (2 outils, max_steps=10, pas de tags)
        //   ET une tache avec 4 input parts
        // WHEN on appelle classify(&task, &manifest)
        // THEN le resultat est ExecutionMode::Orchestrated
    }
}
```

## Definition of Done

- [ ] `ExecutionMode` enum avec `Direct` et `Orchestrated`
- [ ] `MemorySnapshot` struct avec `episodic_recent` et `semantic_relevant`
- [ ] `ContextBundle` struct avec `task`, `memory_snapshot`, `execution_mode`
- [ ] `ObserverError` avec `thiserror` (2 variantes)
- [ ] `classify()` fonction pure implementant les 4 heuristiques
- [ ] `observe()` fonction construisant le ContextBundle avec snapshot memoire optionnel
- [ ] Constantes nommees pour les seuils (pas de magic numbers)
- [ ] 6 tests passent (`cargo test -p apollia-oria`)
- [ ] Zero `unwrap()` en production
- [ ] Zero `todo!()` avant commit
- [ ] Docstring `///` sur chaque struct, enum, fn publique
- [ ] `cargo clippy -p apollia-oria` sans warning

## Ce que cette story N'implemente PAS

- Le mode Orchestre complet (planification multi-step) — Sprint 6 STORY-043
- Le Reasoner (prochaine etape du pipeline ORIA) — STORY-030+
- Le scoring de pertinence pour le snapshot memoire (pour Sprint 4, on charge les N derniers episodes bruts)
- La mise en cache du ContextBundle entre les steps
- L'observation continue pendant l'execution (l'Observer est appele une seule fois par tache)

## Notes d'implementation

- `classify()` est une fonction pure sans effet de bord — ideale pour les tests unitaires simples
- `observe()` accede au MemoryManager en lecture seule — pas besoin de verrou exclusif
- Les seuils de complexite sont extraits en constantes nommees pour faciliter l'ajustement futur
- Le nombre d'episodes recents (`MAX_RECENT_EPISODES = 10`) est un compromis entre contexte suffisant et performance
- `MemorySnapshot` utilise des `String` bruts pour les episodes (pas de struct Episode) afin de decoupler l'Observer de la structure interne de la memoire episodique
- Pour le test AC-1, utiliser un `MemoryManager` reel avec `tempfile::TempDir` et des donnees pre-inserees via `EpisodicMemory::record()` et `SemanticMemory::remember()`

## Liens

- Spec ORIA Engine : `docs/Briques-ORIA-Engine.md`
- Architecture principes : `docs/Architecture-Principes.md`
- STORY-021 : MemoryManager namespace isolation
- STORY-018 : EpisodicMemory backend
- STORY-019 : SemanticMemory backend
