# [Sprint 4][apollia-runtime] TaskRouter dispatch

**ID :** STORY-032
**Sprint :** 4
**Crate cible :** `apollia-runtime`
**Fichier(s) cible(s) :** `crates/apollia-runtime/src/router.rs`
**Taille :** M (3h)
**Depend de :** STORY-031 (ExecutionCoordinator) ✅, STORY-007 (AgentRegistry) ✅
**Statut :** ✅ Termine

---

## User Story

En tant que runtime, je veux un TaskRouter acteur Tokio qui recoit les soumissions de taches, verifie l'etat de l'agent cible, et dispatche vers le bon ExecutionCoordinator, afin de centraliser le point d'entree des taches.

## Contexte technique

Le TaskRouter est un acteur Tokio suivant le meme pattern que AgentRegistry : `mpsc::channel` + Handle clonable. C'est le point d'entree unique pour soumettre des taches au runtime.

Quand une tache est soumise :

1. Le TaskRouter genere un `TaskId` (UUID v4)
2. Il consulte `AgentRegistryHandle` pour obtenir le `ProcessState` de l'agent cible
3. Selon l'etat :
   - **Active** : dispatch vers le `ExecutionCoordinator` de l'agent
   - **Degraded** : dispatch avec emission d'un evenement warning sur l'EventBus
   - **Initializing** : rejet avec `SubmitError::AgentNotReady`
   - **Stopping / Stopped** : rejet avec `SubmitError::AgentUnavailable`
4. Le TaskRouter maintient une `HashMap<AgentId, ExecutionCoordinator>` pour le dispatch
5. Il maintient un `HashMap<TaskId, TaskStatus>` pour les requetes de statut

Le TaskRouter ne cree pas lui-meme les ExecutionCoordinator — ils sont enregistres via un message `RegisterCoordinator` quand un agent passe en etat Active.

## Criteres d'Acceptation

- **AC-1 :** `submit(agent_id, input)` cree un `AIPTask`, dispatche vers le coordinateur, et retourne `task_id`
- **AC-2 :** Soumission vers un agent ACTIVE dispatche la tache avec succes
- **AC-3 :** Soumission vers un agent INITIALIZING retourne `SubmitError::AgentNotReady`
- **AC-4 :** Soumission vers un agent STOPPED retourne `SubmitError::AgentUnavailable`
- **AC-5 :** Soumission vers un agent DEGRADED dispatche la tache + emet un evenement warning sur l'EventBus
- **AC-6 :** `get_status(task_id)` retourne le `TaskStatus` courant de la tache
- **AC-7 :** TaskRouter est un acteur : canal mpsc + Handle pattern (Clone + Send + Sync)

## Specification technique

### router.rs

```rust
use std::collections::HashMap;
use tokio::sync::{mpsc, oneshot};
use apollia_core::{AgentId, TaskId, AIPInput, AIPTask, TaskStatus, RuntimeEvent};
use crate::registry::AgentRegistryHandle;
use crate::coordinator::ExecutionCoordinator;
use crate::eventbus::EventBusSender;

/// Messages internes du TaskRouter acteur.
enum RouterMessage {
    /// Soumettre une tache pour un agent.
    Submit {
        agent_id: AgentId,
        input: AIPInput,
        reply: oneshot::Sender<Result<TaskId, SubmitError>>,
    },
    /// Obtenir le statut d'une tache.
    GetStatus {
        task_id: TaskId,
        reply: oneshot::Sender<Option<TaskStatus>>,
    },
    /// Enregistrer un ExecutionCoordinator pour un agent.
    RegisterCoordinator {
        agent_id: AgentId,
        coordinator: ExecutionCoordinator,
    },
    /// Retirer le coordinateur d'un agent (agent stopping).
    UnregisterCoordinator {
        agent_id: AgentId,
    },
    /// Arreter l'acteur.
    Shutdown,
}

/// Erreurs de soumission de tache.
#[derive(Debug, thiserror::Error)]
pub enum SubmitError {
    /// L'agent est encore en initialisation.
    #[error("agent '{0}' not ready (still initializing)")]
    AgentNotReady(AgentId),

    /// L'agent est en arret ou arrete.
    #[error("agent '{0}' unavailable (stopping or stopped)")]
    AgentUnavailable(AgentId),

    /// L'agent n'existe pas dans le registre.
    #[error("agent '{0}' not found")]
    AgentNotFound(AgentId),

    /// Le coordinateur de l'agent a atteint sa limite de concurrence.
    #[error("concurrency limit reached for agent '{0}'")]
    ConcurrencyLimit(AgentId),

    /// Pas de coordinateur enregistre pour cet agent.
    #[error("no coordinator registered for agent '{0}'")]
    NoCoordinator(AgentId),

    /// L'acteur TaskRouter est mort.
    #[error("router actor is dead")]
    ActorDead,
}

/// Acteur TaskRouter — point d'entree centralisee pour les soumissions de taches.
///
/// Gere le dispatch des taches vers les ExecutionCoordinator des agents actifs.
/// Maintient la table de correspondance agent_id -> coordinator et task_id -> status.
struct TaskRouter {
    rx: mpsc::Receiver<RouterMessage>,
    registry: AgentRegistryHandle,
    event_bus: EventBusSender,
    coordinators: HashMap<AgentId, ExecutionCoordinator>,
    task_statuses: HashMap<TaskId, TaskStatus>,
}

impl TaskRouter {
    /// Boucle principale de l'acteur.
    ///
    /// Traite les messages jusqu'a reception de Shutdown.
    async fn run(mut self) {
        while let Some(msg) = self.rx.recv().await {
            match msg {
                RouterMessage::Submit { agent_id, input, reply } => {
                    let result = self.handle_submit(agent_id, input).await;
                    let _ = reply.send(result);
                }
                RouterMessage::GetStatus { task_id, reply } => {
                    let status = self.task_statuses.get(&task_id).cloned();
                    let _ = reply.send(status);
                }
                RouterMessage::RegisterCoordinator { agent_id, coordinator } => {
                    self.coordinators.insert(agent_id, coordinator);
                }
                RouterMessage::UnregisterCoordinator { agent_id } => {
                    self.coordinators.remove(&agent_id);
                }
                RouterMessage::Shutdown => break,
            }
        }
    }

    /// Gere la soumission d'une tache.
    ///
    /// 1. Verifie l'etat de l'agent via AgentRegistryHandle
    /// 2. Genere un TaskId (UUID v4)
    /// 3. Construit l'AIPTask
    /// 4. Dispatche vers le coordinateur
    async fn handle_submit(
        &mut self,
        agent_id: AgentId,
        input: AIPInput,
    ) -> Result<TaskId, SubmitError> { ... }
}

/// Handle clonable pour interagir avec le TaskRouter acteur.
///
/// Thread-safe : Clone + Send + Sync.
#[derive(Clone)]
pub struct TaskRouterHandle {
    tx: mpsc::Sender<RouterMessage>,
}

impl TaskRouterHandle {
    /// Spawne le TaskRouter acteur et retourne un Handle.
    ///
    /// # Arguments
    /// - `registry` : handle vers l'AgentRegistry pour verifier les etats
    /// - `event_bus` : canal d'emission des evenements runtime
    /// - `buffer_size` : taille du canal mpsc (defaut recommande: 256)
    pub fn spawn(
        registry: AgentRegistryHandle,
        event_bus: EventBusSender,
        buffer_size: usize,
    ) -> Self { ... }

    /// Soumet une tache pour un agent.
    ///
    /// Retourne le TaskId genere en cas de succes.
    pub async fn submit(
        &self,
        agent_id: &str,
        input: AIPInput,
    ) -> Result<TaskId, SubmitError> { ... }

    /// Obtient le statut d'une tache.
    pub async fn get_status(
        &self,
        task_id: &str,
    ) -> Result<Option<TaskStatus>, SubmitError> { ... }

    /// Enregistre un coordinateur pour un agent.
    pub async fn register_coordinator(
        &self,
        agent_id: AgentId,
        coordinator: ExecutionCoordinator,
    ) -> Result<(), SubmitError> { ... }

    /// Retire le coordinateur d'un agent.
    pub async fn unregister_coordinator(
        &self,
        agent_id: &AgentId,
    ) -> Result<(), SubmitError> { ... }

    /// Demande l'arret de l'acteur.
    pub fn shutdown(&self) {
        let _ = self.tx.try_send(RouterMessage::Shutdown);
    }
}
```

### Flow de handle_submit

```rust
// 1. Verifier l'agent dans le registre
let agent_entry = self.registry.get_agent(&agent_id).await
    .map_err(|_| SubmitError::ActorDead)?
    .ok_or_else(|| SubmitError::AgentNotFound(agent_id.clone()))?;

// 2. Verifier le ProcessState
match agent_entry.process_state {
    ProcessState::Initializing => return Err(SubmitError::AgentNotReady(agent_id)),
    ProcessState::Stopping | ProcessState::Stopped => {
        return Err(SubmitError::AgentUnavailable(agent_id));
    }
    ProcessState::Degraded => {
        let _ = self.event_bus.send(RuntimeEvent::AgentDegraded {
            agent_id: agent_id.clone(),
            reason: "task submitted to degraded agent".into(),
        });
        // Continue vers dispatch
    }
    ProcessState::Active => {
        // OK, dispatch normal
    }
}

// 3. Generer TaskId + construire AIPTask
let task_id = TaskId(uuid::Uuid::new_v4().to_string());
let task = AIPTask { task_id: task_id.clone(), input, /* ... */ };

// 4. Dispatcher vers le coordinateur
let coordinator = self.coordinators.get(&agent_id)
    .ok_or_else(|| SubmitError::NoCoordinator(agent_id.clone()))?;

coordinator.submit_task(task).await
    .map_err(|_| SubmitError::ConcurrencyLimit(agent_id.clone()))?;

// 5. Enregistrer le statut
self.task_statuses.insert(task_id.clone(), TaskStatus::Running);

Ok(task_id)
```

## Tests requis

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use apollia_core::ProcessState;
    use tokio::sync::broadcast;

    // Helper pour creer un environnement de test complet
    async fn setup_test_env() -> (TaskRouterHandle, AgentRegistryHandle, broadcast::Receiver<RuntimeEvent>) {
        let (event_tx, event_rx) = broadcast::channel(64);
        let registry = AgentRegistryHandle::spawn(event_tx.clone(), 256);
        let router = TaskRouterHandle::spawn(registry.clone(), event_tx, 256);
        (router, registry, event_rx)
    }

    #[tokio::test]
    async fn test_submit_to_active_agent_returns_task_id() {
        // GIVEN un agent enregistre en etat Active avec un coordinateur
        // WHEN on soumet une tache via router.submit()
        // THEN un TaskId est retourne (non vide, format UUID)
    }

    #[tokio::test]
    async fn test_submit_to_initializing_agent_rejected() {
        // GIVEN un agent enregistre en etat Initializing
        // WHEN on soumet une tache
        // THEN retourne SubmitError::AgentNotReady
    }

    #[tokio::test]
    async fn test_submit_to_stopped_agent_rejected() {
        // GIVEN un agent enregistre en etat Stopped
        // WHEN on soumet une tache
        // THEN retourne SubmitError::AgentUnavailable
    }

    #[tokio::test]
    async fn test_submit_to_degraded_agent_dispatches_with_warning() {
        // GIVEN un agent enregistre en etat Degraded avec un coordinateur
        // AND un receiver sur l'EventBus
        // WHEN on soumet une tache
        // THEN la tache est dispatche (TaskId retourne)
        // AND un RuntimeEvent::AgentDegraded est emis sur l'EventBus
    }

    #[tokio::test]
    async fn test_submit_to_unknown_agent_not_found() {
        // GIVEN aucun agent enregistre avec l'id "unknown-agent"
        // WHEN on soumet une tache pour "unknown-agent"
        // THEN retourne SubmitError::AgentNotFound
    }

    #[tokio::test]
    async fn test_get_status_returns_task_status() {
        // GIVEN une tache soumise avec succes (task_id connu)
        // WHEN on appelle get_status(task_id)
        // THEN retourne Some(TaskStatus::Running)
    }

    #[tokio::test]
    async fn test_router_is_actor_handle_clone_send_sync() {
        // GIVEN un TaskRouterHandle
        // WHEN on clone le handle
        // THEN le clone fonctionne
        // AND le handle est Send + Sync (verifie a la compilation)
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<TaskRouterHandle>();
    }
}
```

## Ce que cette story N'implemente PAS

- La creation automatique de coordinateurs lors du passage Active — story future (orchestration lifecycle)
- La suppression automatique des taches terminees de `task_statuses` (garbage collection) — story future
- Le cancel/abort de taches en cours via le router — story future
- Le load balancing entre agents capables de traiter la meme tache — story future
- La queue de taches en attente (backpressure) — story future, pour l'instant rejet immediat si semaphore plein
- La persistence des statuts de taches (survie au restart) — story future

## Definition of Done

- [ ] `router.rs` compile sans warning
- [ ] 7+ tests passent (`cargo test -p apollia-runtime`)
- [ ] Zero `unwrap()` en production
- [ ] Zero `todo!()` dans le code commite
- [ ] `thiserror` pour toutes les erreurs
- [ ] Docstrings `///` sur chaque struct, enum, fn publique
- [ ] Pattern acteur Tokio respecte : `mpsc::channel` + Handle clonable
- [ ] `TaskRouterHandle` est `Clone + Send + Sync`
- [ ] `cargo clippy -p apollia-runtime` sans warning
- [ ] Commit conventionnel : `feat(apollia-runtime): add TaskRouter actor with dispatch and state verification`

## Notes d'implementation

- Le canal mpsc utilise un buffer de 256 par defaut (meme convention que AgentRegistry)
- Les methodes du Handle utilisent `oneshot::channel()` pour la reponse (pattern request-reply)
- `tx.send().await.map_err(|_| SubmitError::ActorDead)` — si le canal est ferme, l'acteur est mort
- Le `TaskId` est genere cote router (pas cote appelant) pour garantir l'unicite
- `task_statuses` croit sans bound dans cette story — le garbage collection est prevu dans une story future
- Pour les tests, creer un vrai `AgentRegistryHandle` avec des agents pre-enregistres dans les bons etats
- Les coordinateurs de test peuvent utiliser un mock simple qui accepte toujours les taches

## Liens

- Spec Runtime Core : `docs/Briques-Runtime-Core.md`
- STORY-031 : `STORIES/sprint-4/story-031-execution-coordinator.md`
- STORY-007 : `STORIES/sprint-1/story-007-agent-registry.md`
- AgentRegistry pattern : `crates/apollia-runtime/src/registry.rs`
- EventBus implementation : `crates/apollia-runtime/src/eventbus.rs`
