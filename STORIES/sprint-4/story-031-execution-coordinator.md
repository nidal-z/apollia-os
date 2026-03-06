# [Sprint 4][apollia-runtime] ExecutionCoordinator + semaphore concurrence

**ID :** STORY-031
**Sprint :** 4
**Crate cible :** `apollia-runtime`
**Fichier(s) cible(s) :** `crates/apollia-runtime/src/coordinator.rs`
**Taille :** M (3h)
**Depend de :** STORY-030 (ORIAEngine execute_direct) ✅, STORY-007 (AgentRegistry) ✅
**Statut :** 🔲 A faire

---

## User Story

En tant que runtime, je veux un ExecutionCoordinator par agent actif qui gere la concurrence des taches via un semaphore Tokio et delegue l'execution a ORIA, afin de garantir le respect de max_concurrent_tasks.

## Contexte technique

Chaque agent actif possede un `ExecutionCoordinator` dedie. Ce coordinateur utilise un `tokio::sync::Semaphore` pour limiter le nombre de taches executees en parallele. Quand une tache arrive :

1. Tentative d'acquisition d'un permit sur le semaphore (`try_acquire_owned`)
2. Si permit obtenu : spawn d'une tache Tokio qui execute via ORIA
3. Emission de `RuntimeEvent::TaskStarted` sur l'EventBus
4. A la fin (succes ou echec) : emission de `RuntimeEvent::TaskCompleted` + liberation du permit
5. Si semaphore plein : retourne `CoordinatorError::ConcurrencyLimitReached`

La valeur par defaut de `max_concurrent_tasks` est 1, ce qui garantit une execution sequentielle pour les agents PME simples. Les agents avances peuvent demander plus via leur manifeste.

Le permit est garanti d'etre libere meme en cas d'echec ORIA (pas de fuite de permit).

## Criteres d'Acceptation

- **AC-1 :** `submit_task()` acquiert un permit semaphore, spawne l'execution ORIA, et retourne un `JoinHandle`
- **AC-2 :** Les taches concurrentes sont limitees par le semaphore (`max_concurrent_tasks=1` implique execution sequentielle)
- **AC-3 :** `ConcurrencyLimitReached` retourne quand le semaphore est plein et `try_acquire` echoue
- **AC-4 :** `RuntimeEvent::TaskStarted` est emis sur l'EventBus quand l'execution commence
- **AC-5 :** `RuntimeEvent::TaskCompleted` est emis sur l'EventBus quand l'execution termine (avec indicateur succes/echec)
- **AC-6 :** Le permit semaphore est libere meme si l'execution ORIA echoue (pas de fuite de permit)

## Specification technique

### coordinator.rs

```rust
use std::sync::Arc;
use tokio::sync::Semaphore;
use apollia_core::{AgentId, TaskId, RuntimeEvent, AIPTask, AIPResult};
use crate::eventbus::EventBusSender;

/// Erreurs du coordinateur d'execution.
#[derive(Debug, thiserror::Error)]
pub enum CoordinatorError {
    /// Limite de concurrence atteinte pour cet agent.
    #[error("concurrency limit reached for agent '{0}' (max_concurrent_tasks)")]
    ConcurrencyLimitReached(AgentId),

    /// Echec de l'execution de la tache.
    #[error("execution failed: {0}")]
    ExecutionFailed(String),
}

/// Coordinateur d'execution pour un agent actif.
///
/// Gere la concurrence des taches via un semaphore Tokio.
/// Un coordinateur est cree par agent actif et possede son propre semaphore.
pub struct ExecutionCoordinator {
    agent_id: AgentId,
    concurrency: Arc<Semaphore>,
    event_bus: EventBusSender,
}

impl ExecutionCoordinator {
    /// Cree un nouveau coordinateur pour l'agent donne.
    ///
    /// # Arguments
    /// - `agent_id` : identifiant de l'agent
    /// - `max_concurrent` : nombre maximal de taches en parallele (defaut: 1)
    /// - `event_bus` : canal d'emission des evenements runtime
    pub fn new(
        agent_id: AgentId,
        max_concurrent: u32,
        event_bus: EventBusSender,
    ) -> Self {
        Self {
            agent_id,
            concurrency: Arc::new(Semaphore::new(max_concurrent as usize)),
            event_bus,
        }
    }

    /// Soumet une tache pour execution.
    ///
    /// Tente d'acquerir un permit sur le semaphore :
    /// - Si obtenu : spawne une tache Tokio, emet TaskStarted, retourne JoinHandle
    /// - Si semaphore plein : retourne ConcurrencyLimitReached
    ///
    /// Le permit est libere automatiquement quand la tache spawned termine
    /// (via drop du OwnedSemaphorePermit dans la closure).
    pub async fn submit_task(
        &self,
        task: AIPTask,
    ) -> Result<
        tokio::task::JoinHandle<Result<AIPResult, CoordinatorError>>,
        CoordinatorError,
    > { ... }

    /// Retourne le nombre de permits disponibles (taches pouvant etre acceptees).
    pub fn available_permits(&self) -> usize {
        self.concurrency.available_permits()
    }
}
```

### Pattern d'execution dans submit_task

```rust
// Pseudo-code du flow interne
let permit = self.concurrency.clone().try_acquire_owned()
    .map_err(|_| CoordinatorError::ConcurrencyLimitReached(self.agent_id.clone()))?;

let agent_id = self.agent_id.clone();
let event_bus = self.event_bus.clone();
let task_id = task.task_id.clone();

let handle = tokio::spawn(async move {
    // Le permit est move dans la closure — libere au drop
    let _permit = permit;

    // Emettre TaskStarted
    let _ = event_bus.send(RuntimeEvent::TaskStarted {
        agent_id: agent_id.clone(),
        task_id: task_id.clone(),
    });

    // Executer via ORIA (inject via closure ou trait)
    let result = execute_via_oria(task).await;

    // Emettre TaskCompleted (succes ou echec)
    let is_success = result.is_ok();
    let _ = event_bus.send(RuntimeEvent::TaskCompleted {
        agent_id: agent_id.clone(),
        task_id: task_id.clone(),
        success: is_success,
    });

    result
});

Ok(handle)
```

## Tests requis

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::broadcast;

    #[tokio::test]
    async fn test_submit_task_returns_join_handle() {
        // GIVEN un coordinator avec max_concurrent=1
        // WHEN on soumet une tache
        // THEN submit_task retourne Ok(JoinHandle)
    }

    #[tokio::test]
    async fn test_concurrency_limit_sequential() {
        // GIVEN un coordinator avec max_concurrent=1
        // AND une tache deja en cours d'execution
        // WHEN on soumet une deuxieme tache
        // THEN retourne ConcurrencyLimitReached
    }

    #[tokio::test]
    async fn test_concurrency_limit_parallel() {
        // GIVEN un coordinator avec max_concurrent=3
        // WHEN on soumet 3 taches simultanement
        // THEN toutes sont acceptees
        // AND une 4eme retourne ConcurrencyLimitReached
    }

    #[tokio::test]
    async fn test_task_started_event_emitted() {
        // GIVEN un coordinator et un receiver sur l'EventBus
        // WHEN on soumet une tache
        // THEN un RuntimeEvent::TaskStarted est recu avec le bon agent_id et task_id
    }

    #[tokio::test]
    async fn test_task_completed_event_emitted() {
        // GIVEN un coordinator et un receiver sur l'EventBus
        // WHEN une tache termine (succes)
        // THEN un RuntimeEvent::TaskCompleted est recu avec success=true
    }

    #[tokio::test]
    async fn test_permit_released_on_failure() {
        // GIVEN un coordinator avec max_concurrent=1
        // AND une tache qui echoue
        // WHEN la tache echoue et termine
        // THEN available_permits() == 1 (permit libere)
        // AND on peut soumettre une nouvelle tache
    }
}
```

## Ce que cette story N'implemente PAS

- La creation automatique des coordinateurs lors de l'enregistrement d'un agent — gere par TaskRouter (STORY-032)
- Le mecanisme de cancel/abort d'une tache en cours — story future
- La persistence de l'etat des taches — story future
- Le retry automatique en cas d'echec — story future
- La priorisation des taches dans la queue — story future
- L'integration directe avec ORIAEngine — le callable d'execution est injecte (trait ou closure)

## Definition of Done

- [ ] `coordinator.rs` compile sans warning
- [ ] 6+ tests passent (`cargo test -p apollia-runtime`)
- [ ] Zero `unwrap()` en production
- [ ] Zero `todo!()` dans le code commite
- [ ] `thiserror` pour toutes les erreurs
- [ ] Docstrings `///` sur chaque struct, enum, fn publique
- [ ] `cargo clippy -p apollia-runtime` sans warning
- [ ] Permit semaphore garanti d'etre libere en cas d'erreur (test AC-6)
- [ ] Commit conventionnel : `feat(apollia-runtime): add ExecutionCoordinator with semaphore concurrency`

## Notes d'implementation

- Utiliser `try_acquire_owned()` (pas `acquire()`) pour un comportement non-bloquant — on veut rejeter immediatement si le semaphore est plein
- Le `OwnedSemaphorePermit` est move dans la closure du `tokio::spawn` — il est libere automatiquement quand la closure termine (drop)
- L'EventBus utilise `broadcast::Sender` — le `send()` peut retourner `Err` si personne n'ecoute, c'est normal (fire-and-forget)
- Pour les tests, utiliser un `broadcast::channel(64)` local et un receiver pour verifier les evenements
- L'execution ORIA est abstraite pour faciliter le mocking dans les tests (closure ou trait `ExecutionBackend`)

## Liens

- Spec Runtime Core : `docs/Briques-Runtime-Core.md`
- Spec ORIA Engine : `docs/Briques-ORIA-Engine.md`
- STORY-030 : `STORIES/sprint-4/story-030-oria-mode-direct-step-budget.md`
- STORY-007 : `STORIES/sprint-1/story-007-agent-registry.md`
- EventBus implementation : `crates/apollia-runtime/src/eventbus.rs`
