# [Sprint 1][apollia-runtime] AgentRegistry acteur Tokio

**ID :** STORY-007
**Sprint :** 1
**Crate cible :** `apollia-runtime`
**Fichier(s) cible(s) :** `crates/apollia-runtime/src/registry.rs`
**Taille :** M
**Dépend de :** STORY-006
**Statut :** ✅ Livré

---

## User Story

```
En tant que runtime,
je veux un AgentRegistry acteur Tokio qui maintient l'état ProcessState de chaque agent enregistré,
afin que le TaskRouter (Sprint 4) puisse vérifier qu'un agent est Active avant de router une tâche.
```

---

## Contexte technique

L'AgentRegistry est le deuxième acteur Tokio du Runtime Core, instancié juste après l'EventBus.
Il maintient une `HashMap<AgentId, AgentEntry>` en mémoire, privée et uniquement accessible
via messages. Chaque transition d'état publie un `RuntimeEvent` sur l'EventBus.

Cette story implémente la structure interne de l'acteur. L'API publique (Handle)
fait l'objet de STORY-008 pour séparer les responsabilités et faciliter les tests.

**Principe(s) architectural(aux) concerné(s) :**
- Principe #5 — Un acteur, une responsabilité
- Principe #4 — Fail fast (transition invalide → erreur immédiate)

**Position dans l'architecture :**
```
Runtime Core
  └── EventBus (STORY-006 ✅)
  └── AgentRegistry  ← cette story
        └── [Handle public → STORY-008]
```

---

## Critères d'Acceptation

### AC-1 — Enregistrement d'un agent

```
ÉTANT DONNÉ un AgentRegistry spawné avec un EventBusSender
QUAND on envoie le message Register avec un AgentManifest valide
ALORS l'agent est créé avec ProcessState::Initializing
     ET un AgentId (UUID v4) est retourné
     ET RuntimeEvent::AgentRegistered(agent_id) est publié sur le bus
```

### AC-2 — Transition d'état valide

```
ÉTANT DONNÉ un agent en état ProcessState::Initializing
QUAND on envoie UpdateState { id, state: ProcessState::Active }
ALORS l'état est mis à jour
     ET RuntimeEvent::AgentReady(id) est publié
```

### AC-3 — Transition d'état invalide

```
ÉTANT DONNÉ un agent en état ProcessState::Stopped
QUAND on envoie UpdateState { id, state: ProcessState::Active }
ALORS AgentRegistryError::InvalidTransition { from: Stopped, to: Active } est retourné
     ET l'état de l'agent n'est pas modifié
```

### AC-4 — Agent inexistant

```
ÉTANT DONNÉ un registry sans agent "agent-xyz"
QUAND on envoie GetAgent { id: "agent-xyz" }
ALORS None est retourné (pas d'erreur — l'absence n'est pas une erreur)
```

### AC-5 — Désenregistrement

```
ÉTANT DONNÉ un agent enregistré en état Active
QUAND on envoie Unregister { id }
ALORS l'agent est retiré du registry
     ET RuntimeEvent::AgentStopped(id) est publié
```

---

## Spécification technique

### Types à créer / modifier

```rust
// crates/apollia-runtime/src/registry.rs

use std::collections::HashMap;
use tokio::sync::{mpsc, oneshot};
use tracing::{info, warn, instrument};
use apollia_core::{AgentManifest, ProcessState, RuntimeEvent};
use crate::eventbus::EventBusSender;

pub type AgentId = String;

/// Entrée dans le registry pour un agent enregistré.
#[derive(Debug, Clone)]
pub struct AgentEntry {
    /// Identifiant unique généré à l'enregistrement.
    pub id: AgentId,
    /// Manifest déclaré par l'agent à l'enregistrement.
    pub manifest: AgentManifest,
    /// État courant du processus agent.
    pub process_state: ProcessState,
}

/// Erreurs possibles des opérations sur le registry.
#[derive(Debug, thiserror::Error)]
pub enum AgentRegistryError {
    #[error("Agent '{0}' introuvable dans le registry")]
    NotFound(AgentId),
    #[error("Transition d'état invalide : {from:?} → {to:?}")]
    InvalidTransition { from: ProcessState, to: ProcessState },
    #[error("L'acteur AgentRegistry est mort (canal fermé)")]
    ActorDead,
}

// Messages internes — enum privé, jamais exposé publiquement
enum RegistryMessage {
    Register {
        manifest: AgentManifest,
        reply: oneshot::Sender<Result<AgentId, AgentRegistryError>>,
    },
    Unregister {
        id: AgentId,
        reply: oneshot::Sender<Result<(), AgentRegistryError>>,
    },
    UpdateState {
        id: AgentId,
        state: ProcessState,
        reply: oneshot::Sender<Result<(), AgentRegistryError>>,
    },
    GetAgent {
        id: AgentId,
        reply: oneshot::Sender<Option<AgentEntry>>,
    },
    ListAgents {
        reply: oneshot::Sender<Vec<AgentEntry>>,
    },
    Shutdown,
}

/// Acteur interne du registry — état privé, jamais exposé.
struct AgentRegistry {
    agents: HashMap<AgentId, AgentEntry>,
    bus: EventBusSender,
}

impl AgentRegistry {
    /// Spawn l'acteur et retourne son Handle public.
    pub fn spawn(bus: EventBusSender) -> AgentRegistryHandle {
        let (tx, rx) = mpsc::channel(256);
        let registry = Self {
            agents: HashMap::new(),
            bus,
        };
        tokio::spawn(registry.run(rx));
        AgentRegistryHandle { tx }
    }

    async fn run(mut self, mut rx: mpsc::Receiver<RegistryMessage>) {
        info!("AgentRegistry démarré");
        while let Some(msg) = rx.recv().await {
            match msg {
                RegistryMessage::Register { manifest, reply } => {
                    let result = self.handle_register(manifest);
                    let _ = reply.send(result);
                }
                RegistryMessage::Unregister { id, reply } => {
                    let result = self.handle_unregister(&id);
                    let _ = reply.send(result);
                }
                RegistryMessage::UpdateState { id, state, reply } => {
                    let result = self.handle_update_state(&id, state);
                    let _ = reply.send(result);
                }
                RegistryMessage::GetAgent { id, reply } => {
                    let entry = self.agents.get(&id).cloned();
                    let _ = reply.send(entry);
                }
                RegistryMessage::ListAgents { reply } => {
                    let list = self.agents.values().cloned().collect();
                    let _ = reply.send(list);
                }
                RegistryMessage::Shutdown => {
                    info!("AgentRegistry arrêt demandé");
                    break;
                }
            }
        }
        info!("AgentRegistry arrêté");
    }

    fn handle_register(&mut self, manifest: AgentManifest) -> Result<AgentId, AgentRegistryError> {
        let id = uuid::Uuid::new_v4().to_string();
        let entry = AgentEntry {
            id: id.clone(),
            manifest,
            process_state: ProcessState::Initializing,
        };
        self.agents.insert(id.clone(), entry);
        let _ = self.bus.send(RuntimeEvent::AgentRegistered(id.clone()));
        info!(agent_id = %id, "Agent enregistré");
        Ok(id)
    }

    fn handle_unregister(&mut self, id: &str) -> Result<(), AgentRegistryError> {
        if self.agents.remove(id).is_none() {
            return Err(AgentRegistryError::NotFound(id.to_string()));
        }
        let _ = self.bus.send(RuntimeEvent::AgentStopped(id.to_string()));
        info!(agent_id = %id, "Agent désenregistré");
        Ok(())
    }

    fn handle_update_state(
        &mut self,
        id: &str,
        new_state: ProcessState,
    ) -> Result<(), AgentRegistryError> {
        let entry = self
            .agents
            .get_mut(id)
            .ok_or_else(|| AgentRegistryError::NotFound(id.to_string()))?;

        if !entry.process_state.can_transition_to(&new_state) {
            return Err(AgentRegistryError::InvalidTransition {
                from: entry.process_state.clone(),
                to: new_state,
            });
        }

        let prev = entry.process_state.clone();
        entry.process_state = new_state.clone();

        info!(agent_id = %id, from = ?prev, to = ?new_state, "Transition ProcessState");

        let event = match &new_state {
            ProcessState::Active => RuntimeEvent::AgentReady(id.to_string()),
            ProcessState::Degraded => RuntimeEvent::AgentDegraded {
                agent_id: id.to_string(),
                reason: "transition manuelle".to_string(),
            },
            ProcessState::Stopped => RuntimeEvent::AgentStopped(id.to_string()),
            _ => return Ok(()),
        };
        let _ = self.bus.send(event);
        Ok(())
    }
}
```

### Dépendances Cargo

```toml
# crates/apollia-runtime/Cargo.toml
[dependencies]
apollia-core = { workspace = true }
tokio = { workspace = true, features = ["sync", "rt-multi-thread", "macros"] }
tracing = { workspace = true }
thiserror = { workspace = true }
uuid = { workspace = true }
```

### Comportement attendu

L'acteur tourne dans une boucle `while let Some(msg) = rx.recv().await`. Quand le dernier
`AgentRegistryHandle` est droppé, le canal mpsc se ferme et `rx.recv()` retourne `None`
— la boucle se termine naturellement (pas besoin du message `Shutdown` dans ce cas,
mais il est fourni pour un arrêt explicite depuis le Supervisor).

Les transitions d'état invalides retournent une erreur sans modifier l'état.
`ProcessState::can_transition_to()` est défini dans `apollia-core` — ne pas redéfinir la logique ici.

### Ce que cette story N'implémente PAS

- L'API publique `AgentRegistryHandle` avec les méthodes async (STORY-008)
- La persistance du registry sur disque (hors scope v1)
- La récupération après crash (hors scope v1)
- Le TaskRouter qui utilise le registry (Sprint 4)

---

## Tests requis

### Tests unitaires

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use apollia_core::{AgentManifest, ProcessState};
    use tokio::sync::broadcast;

    fn test_manifest(name: &str) -> AgentManifest {
        AgentManifest {
            name: name.to_string(),
            version: "0.1.0".to_string(),
            description: String::new(),
            tools_required: vec![],
            tools_optional: vec![],
            supports_streaming: false,
            supports_a2a: false,
            memory_namespace: None,
            max_concurrent_tasks: 1,
            step_budget: None,
            network_allowlist: None,
        }
    }

    #[tokio::test]
    async fn test_ac1_register_emet_event() {
        // GIVEN
        let (bus_tx, mut bus_rx) = broadcast::channel(16);
        let handle = AgentRegistry::spawn(bus_tx);

        // WHEN
        let result = handle.register(test_manifest("agent-test")).await;

        // THEN
        assert!(result.is_ok());
        let id = result.unwrap();
        assert!(!id.is_empty());
        let event = bus_rx.recv().await.unwrap();
        assert!(matches!(event, RuntimeEvent::AgentRegistered(eid) if eid == id));
    }

    #[tokio::test]
    async fn test_ac2_transition_valide() {
        // GIVEN
        let (bus_tx, mut bus_rx) = broadcast::channel(16);
        let handle = AgentRegistry::spawn(bus_tx);
        let id = handle.register(test_manifest("agent-test")).await.unwrap();
        let _ = bus_rx.recv().await; // AgentRegistered

        // WHEN
        let result = handle.update_state(&id, ProcessState::Active).await;

        // THEN
        assert!(result.is_ok());
        let event = bus_rx.recv().await.unwrap();
        assert!(matches!(event, RuntimeEvent::AgentReady(eid) if eid == id));
    }

    #[tokio::test]
    async fn test_ac3_transition_invalide() {
        // GIVEN
        let (bus_tx, _) = broadcast::channel(16);
        let handle = AgentRegistry::spawn(bus_tx);
        let id = handle.register(test_manifest("agent-test")).await.unwrap();
        handle.update_state(&id, ProcessState::Stopping).await.unwrap();
        handle.update_state(&id, ProcessState::Stopped).await.unwrap();

        // WHEN — Stopped → Active est invalide
        let result = handle.update_state(&id, ProcessState::Active).await;

        // THEN
        assert!(matches!(
            result.unwrap_err(),
            AgentRegistryError::InvalidTransition { .. }
        ));
    }

    #[tokio::test]
    async fn test_ac4_agent_inexistant() {
        // GIVEN
        let (bus_tx, _) = broadcast::channel(16);
        let handle = AgentRegistry::spawn(bus_tx);

        // WHEN
        let result = handle.get_agent("inexistant").await;

        // THEN
        assert!(result.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_ac5_unregister_emet_event() {
        // GIVEN
        let (bus_tx, mut bus_rx) = broadcast::channel(16);
        let handle = AgentRegistry::spawn(bus_tx);
        let id = handle.register(test_manifest("agent-test")).await.unwrap();
        let _ = bus_rx.recv().await; // consommer AgentRegistered

        // WHEN
        let result = handle.unregister(&id).await;

        // THEN
        assert!(result.is_ok());
        let event = bus_rx.recv().await.unwrap();
        assert!(matches!(event, RuntimeEvent::AgentStopped(eid) if eid == id));
    }
}
```

---

## Definition of Done

**Qualité code :**
- [ ] `cargo test -p apollia-runtime` passe (0 test ignoré)
- [ ] `cargo clippy -p apollia-runtime -- -D warnings` : zéro warning
- [ ] `cargo fmt --check` : code formatté
- [ ] Zéro `unwrap()` dans le code de production
- [ ] Zéro `todo!()` dans le code de production
- [ ] Docstring `///` sur chaque struct, enum, et fonction publique

**Architectural :**
- [ ] Pattern acteur Tokio strict : `mpsc::channel` + struct privée + Handle clonable
- [ ] Zéro `Arc<Mutex<T>>`
- [ ] `can_transition_to()` utilisé depuis `apollia-core`, pas redéfini
- [ ] Tous les événements publiés via `EventBusSender`

**Commit :**
- [ ] `feat(apollia-runtime): add AgentRegistry actor with ProcessState transitions`

---

## Notes d'implémentation

**Décisions prises pendant l'implémentation :**
- `AgentManifest` boxé dans `RegistryMessage::Register` pour respecter `clippy::large_enum_variant`
- `#[allow(dead_code)]` sur `RegistryMessage`, `AgentRegistry` struct et impl — code utilisé uniquement via `#[cfg(test)]` pour l'instant, sera activé par le Supervisor (STORY-039)
- `AgentRegistry` en `pub(crate)` pour le futur Supervisor (dans le même crate)
- `can_transition_to()` ajouté dans `apollia-core/src/process.rs` (manquait dans la spec)
- Handle API (STORY-008) implémentée dans le même commit car les tests STORY-007 l'exigent

**Déviations par rapport à la spec :**
- Fixtures de test mises à jour avec les champs `shared_memory_namespaces`, `tags`, `skills` (ajoutés à `AgentManifest` depuis la rédaction de la story)

**Dette technique identifiée :**
- Aucune

---

## Liens

- Epic parent : Runtime Core — acteurs Tokio
- Story précédente : STORY-006 (EventBus)
- Story suivante : STORY-008 (AgentRegistryHandle API publique)
- ADR associé : —
