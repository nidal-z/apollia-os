# [Sprint 1][apollia-runtime] EventBus broadcast Tokio + RuntimeEvent catalogue

**ID :** STORY-006
**Sprint :** 1
**Crate cible :** `apollia-runtime`
**Fichier(s) cible(s) :** `crates/apollia-runtime/src/eventbus.rs`
**Taille :** M
**Dépend de :** STORY-005 ✅
**Statut :** ✅ Livré

---

## User Story

```
En tant que runtime,
je veux un EventBus basé sur tokio::sync::broadcast qui publie des RuntimeEvents typés,
afin que tous les acteurs Tokio puissent réagir aux changements d'état sans couplage direct.
```

---

## Contexte technique

L'EventBus est le premier composant du Runtime Core. Il est instancié avant tout autre acteur
et sa référence (`EventBusSender`) est injectée dans chaque acteur à l'initialisation.
Basé sur `tokio::sync::broadcast` (multi-producer, multi-consumer avec buffer).
Implémente le principe #5 (un acteur, une responsabilité) — les acteurs communiquent
uniquement via événements, jamais via état partagé.

**Principe(s) architectural(aux) concerné(s) :**
- Principe #5 — Un acteur, une responsabilité
- Principe #4 — Fail fast (buffer lagged → warning, pas de panic)

**Position dans l'architecture :**
```
Runtime Core
  └── EventBus  ← cette story (démarre en premier)
        ├── AgentRegistry (consommateur, STORY-007)
        └── [futurs acteurs Sprint 2+]
```

---

## Critères d'Acceptation

### AC-1 — Publication d'un événement

```
ÉTANT DONNÉ un EventBus créé avec EventBus::new()
QUAND on appelle bus.send(RuntimeEvent::AgentRegistered("agent-1".to_string()))
ALORS le Receiver souscrit reçoit l'événement avec la valeur correcte
```

### AC-2 — Multiple consumers

```
ÉTANT DONNÉ un EventBus et 3 receivers créés avant la publication
QUAND on publie RuntimeEvent::AllReady
ALORS les 3 receivers reçoivent l'événement (broadcast sémantique)
```

### AC-3 — Lagged consumer (buffer saturé)

```
ÉTANT DONNÉ un EventBus avec buffer de 1024 et un receiver qui ne consomme pas
QUAND on publie 1025 événements
ALORS le receiver retourne RecvError::Lagged et le programme ne panic pas
     ET un warning tracing est émis avec le nombre de messages perdus
```

### AC-4 — RuntimeEvent contient tous les cas catalogués

```
ÉTANT DONNÉ l'enum RuntimeEvent défini dans apollia-core
QUAND on compile le projet
ALORS toutes les variantes suivantes existent et dérivent Debug + Clone :
     AgentRegistered, AgentReady, AgentDegraded, AgentStopped,
     TaskStarted, TaskCompleted, TaskCanceled, StepExecuted,
     ToolCircuitBroken, AllReady, ShutdownRequested, FatalError
```

---

## Spécification technique

### Types à créer / modifier

```rust
// crates/apollia-runtime/src/eventbus.rs

/// Handle en écriture sur l'EventBus — clonable, partageable entre acteurs.
pub type EventBusSender = tokio::sync::broadcast::Sender<RuntimeEvent>;

/// Handle en lecture sur l'EventBus — un par acteur consommateur.
pub type EventBusReceiver = tokio::sync::broadcast::Receiver<RuntimeEvent>;

/// EventBus du runtime — point de création unique.
pub struct EventBus;

impl EventBus {
    /// Crée un nouveau bus avec un buffer de 1024 événements.
    /// Retourne le Sender partageable et un premier Receiver.
    pub fn new() -> (EventBusSender, EventBusReceiver) {
        tokio::sync::broadcast::channel(1024)
    }
}

// crates/apollia-core/src/events.rs (à ajouter dans apollia-core)
// RuntimeEvent est placé dans apollia-core pour éviter les dépendances circulaires

use crate::{AgentId, TaskId};

/// Catalogue complet des événements du runtime Apollia OS.
#[derive(Debug, Clone)]
pub enum RuntimeEvent {
    /// Un agent a été enregistré dans le Registry (état: Initializing).
    AgentRegistered(AgentId),
    /// Un agent a terminé son initialisation et est opérationnel (état: Active).
    AgentReady(AgentId),
    /// Un agent est passé en état dégradé.
    AgentDegraded { agent_id: AgentId, reason: String },
    /// Un agent s'est arrêté proprement.
    AgentStopped(AgentId),
    /// Une tâche a démarré sur un agent.
    TaskStarted { agent_id: AgentId, task_id: TaskId },
    /// Une tâche s'est terminée (succès ou échec).
    TaskCompleted { agent_id: AgentId, task_id: TaskId, success: bool },
    /// Une tâche a été annulée.
    TaskCanceled { task_id: TaskId },
    /// Un step a été exécuté dans une tâche.
    StepExecuted { task_id: TaskId, step: u32, tool: Option<String> },
    /// Le circuit breaker d'un outil s'est ouvert.
    ToolCircuitBroken { tool_name: String },
    /// Tous les composants sont prêts — runtime opérationnel.
    AllReady,
    /// Arrêt demandé (SIGTERM ou commande CLI).
    ShutdownRequested,
    /// Erreur fatale non récupérable.
    FatalError(String),
}
```

> `TaskId` est un alias `pub type TaskId = String;` à ajouter dans `apollia-core/src/lib.rs`
> si pas encore présent.

### Dépendances Cargo

```toml
# crates/apollia-runtime/Cargo.toml
[dependencies]
apollia-core = { workspace = true }
tokio = { workspace = true, features = ["sync", "rt-multi-thread", "macros"] }
tracing = { workspace = true }
```

### Comportement attendu

L'EventBus est créé une seule fois dans le `Supervisor` (Sprint 5). Durant Sprint 1, il est
créé directement dans les tests. Le `EventBusSender` est cloné et passé à chaque acteur.

En cas de `RecvError::Lagged(n)` lors d'un `recv()` dans un consumer :
- Le consumer log un warning : `tracing::warn!(messages_perdus = n, "EventBus: consumer laggé")`
- Le consumer continue de fonctionner normalement sur les prochains événements
- Jamais de panic

### Ce que cette story N'implémente PAS

- Le Supervisor qui orchestre le démarrage (Sprint 5, STORY-039)
- La persistance des événements (hors scope Apollia OS v1)
- Le filtrage d'événements par type (les consumers filtrent eux-mêmes via `match`)

---

## Tests requis

### Tests unitaires

```rust
// crates/apollia-runtime/src/eventbus.rs

#[cfg(test)]
mod tests {
    use super::*;
    use apollia_core::RuntimeEvent;

    #[tokio::test]
    async fn test_ac1_publication_reception() {
        // GIVEN
        let (tx, mut rx) = EventBus::new();

        // WHEN
        tx.send(RuntimeEvent::AgentRegistered("agent-1".to_string())).unwrap();

        // THEN
        let event = rx.recv().await.unwrap();
        assert!(matches!(event, RuntimeEvent::AgentRegistered(id) if id == "agent-1"));
    }

    #[tokio::test]
    async fn test_ac2_multiple_consumers() {
        // GIVEN
        let (tx, mut rx1) = EventBus::new();
        let mut rx2 = tx.subscribe();
        let mut rx3 = tx.subscribe();

        // WHEN
        tx.send(RuntimeEvent::AllReady).unwrap();

        // THEN — les 3 consumers reçoivent
        assert!(matches!(rx1.recv().await.unwrap(), RuntimeEvent::AllReady));
        assert!(matches!(rx2.recv().await.unwrap(), RuntimeEvent::AllReady));
        assert!(matches!(rx3.recv().await.unwrap(), RuntimeEvent::AllReady));
    }

    #[tokio::test]
    async fn test_ac3_lagged_consumer_ne_panic_pas() {
        // GIVEN — buffer de 8 pour le test (pas 1024)
        let (tx, mut rx) = tokio::sync::broadcast::channel::<RuntimeEvent>(8);

        // WHEN — on envoie 9 messages sans consommer
        for i in 0..9u32 {
            let _ = tx.send(RuntimeEvent::StepExecuted {
                task_id: format!("task-{}", i),
                step: i,
                tool: None,
            });
        }

        // THEN — RecvError::Lagged, pas de panic
        let result = rx.recv().await;
        assert!(matches!(result, Err(tokio::sync::broadcast::error::RecvError::Lagged(_))));
    }
}
```

---

## Definition of Done

**Qualité code :**
- [x] `cargo test -p apollia-runtime` passe (0 test ignoré)
- [x] `cargo clippy -p apollia-runtime -- -D warnings` : zéro warning
- [x] `cargo fmt --check` : code formatté
- [x] Zéro `unwrap()` dans le code de production (tests exclus)
- [x] Zéro `todo!()` dans le code de production
- [x] Docstring `///` sur chaque struct, enum, et fonction publique

**Architectural :**
- [x] `RuntimeEvent` défini dans `apollia-core` (pas dans `apollia-runtime`)
- [x] `EventBusSender` et `EventBusReceiver` sont des type aliases publics
- [x] Lagged consumer : warning tracé, pas de panic
- [x] Principe #5 respecté : zéro `Arc<Mutex<T>>`

**Documentation :**
- [x] `AgentId` et `TaskId` documentés dans `Decisions-Log.md` (ADR-011)

**Commit :**
- [ ] `feat(apollia-core): add RuntimeEvent catalogue and TaskId type alias`
- [ ] `feat(apollia-runtime): add EventBus with broadcast channel`

---

## Notes d'implémentation

**Décisions prises pendant l'implémentation :**
- `AgentId` et `TaskId` ajoutés dans `apollia-core/src/events.rs` (ADR-011)
- `EventBus::new()` : lint `clippy::new_ret_no_self` supprimé avec `#[allow]` documenté — pattern factory intentionnel selon la spec
- Test AC-3 : drain du buffer après `Lagged` requis pour vérifier que le consumer peut continuer

**Déviations par rapport à la spec :**
- Aucune déviation fonctionnelle

**Dette technique identifiée :**
- `AgentId`/`TaskId` sont des `String` aliases — potentiel newtype à Sprint 3+ si besoin de type-safety renforcé (ADR-011)

---

## Liens

- Epic parent : Runtime Core — acteurs Tokio
- Story précédente : STORY-005 (CI)
- Story suivante : STORY-007 (AgentRegistry)
- ADR associé : —
