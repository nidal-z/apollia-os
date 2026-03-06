# [Sprint 1][apollia-runtime] AgentRegistryHandle API publique async

**ID :** STORY-008
**Sprint :** 1
**Crate cible :** `apollia-runtime`
**Fichier(s) cible(s) :** `crates/apollia-runtime/src/registry.rs`
**Taille :** S
**Dépend de :** STORY-007
**Statut :** ✅ Livré

---

## User Story

```
En tant que développeur d'agent ou composant runtime (TaskRouter, CLI),
je veux utiliser une API async ergonomique sur AgentRegistryHandle sans manipuler les messages internes,
afin d'interagir avec l'AgentRegistry sans connaître son protocole de messages.
```

---

## Contexte technique

STORY-007 crée l'acteur `AgentRegistry` et son enum `RegistryMessage` (privés).
Cette story expose l'`AgentRegistryHandle` avec des méthodes async publiques qui encapsulent
la création de `oneshot::channel` et l'envoi de messages.

Le Handle est `Clone` — il peut être partagé entre plusieurs composants sans contrainte.

**Principe(s) architectural(aux) concerné(s) :**
- Principe #5 — Un acteur, une responsabilité
- Principe #8 — CLI humaine, API machine (le Handle est l'interface "machine")

**Position dans l'architecture :**
```
Runtime Core
  └── AgentRegistry (STORY-007)
        └── AgentRegistryHandle  ← cette story (interface publique)
              ├── TaskRouter (Sprint 4)
              ├── APIServer (Sprint 5)
              └── CLI (Sprint 5)
```

---

## Critères d'Acceptation

### AC-1 — Handle est Clone et Send

```
ÉTANT DONNÉ un AgentRegistryHandle
QUAND on le clone et on l'utilise depuis 2 tokio::tasks différentes simultanément
ALORS les deux appels fonctionnent correctement sans erreur de concurrence
```

### AC-2 — ActorDead si acteur arrêté

```
ÉTANT DONNÉ un AgentRegistryHandle dont l'acteur sous-jacent a été arrêté (Shutdown)
QUAND on appelle handle.register(manifest).await
ALORS AgentRegistryError::ActorDead est retourné
```

### AC-3 — API complète couvre toutes les opérations

```
ÉTANT DONNÉ un AgentRegistryHandle
QUAND on appelle chaque méthode publique
ALORS les méthodes suivantes existent et compilent :
     register(manifest) → Result<AgentId, AgentRegistryError>
     unregister(id) → Result<(), AgentRegistryError>
     update_state(id, state) → Result<(), AgentRegistryError>
     get_agent(id) → Result<Option<AgentEntry>, AgentRegistryError>
     list_agents() → Result<Vec<AgentEntry>, AgentRegistryError>
     shutdown() → () (fire-and-forget)
```

---

## Spécification technique

### Types à créer / modifier

```rust
// Complétion de crates/apollia-runtime/src/registry.rs

/// Handle public vers l'acteur AgentRegistry — clonable, thread-safe.
#[derive(Clone)]
pub struct AgentRegistryHandle {
    tx: mpsc::Sender<RegistryMessage>,
}

impl AgentRegistryHandle {
    /// Enregistre un nouvel agent avec son manifest.
    /// Retourne l'AgentId généré (UUID v4).
    /// L'agent est créé en état ProcessState::Initializing.
    pub async fn register(
        &self,
        manifest: AgentManifest,
    ) -> Result<AgentId, AgentRegistryError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(RegistryMessage::Register { manifest, reply: reply_tx })
            .await
            .map_err(|_| AgentRegistryError::ActorDead)?;
        reply_rx.await.map_err(|_| AgentRegistryError::ActorDead)?
    }

    /// Retire un agent du registry et publie AgentStopped.
    pub async fn unregister(&self, id: &str) -> Result<(), AgentRegistryError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(RegistryMessage::Unregister { id: id.to_string(), reply: reply_tx })
            .await
            .map_err(|_| AgentRegistryError::ActorDead)?;
        reply_rx.await.map_err(|_| AgentRegistryError::ActorDead)?
    }

    /// Met à jour l'état ProcessState d'un agent.
    /// Retourne une erreur si la transition est invalide.
    pub async fn update_state(
        &self,
        id: &str,
        state: ProcessState,
    ) -> Result<(), AgentRegistryError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(RegistryMessage::UpdateState { id: id.to_string(), state, reply: reply_tx })
            .await
            .map_err(|_| AgentRegistryError::ActorDead)?;
        reply_rx.await.map_err(|_| AgentRegistryError::ActorDead)?
    }

    /// Retourne l'entrée d'un agent ou None s'il n'est pas enregistré.
    pub async fn get_agent(&self, id: &str) -> Result<Option<AgentEntry>, AgentRegistryError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(RegistryMessage::GetAgent { id: id.to_string(), reply: reply_tx })
            .await
            .map_err(|_| AgentRegistryError::ActorDead)?;
        reply_rx.await.map_err(|_| AgentRegistryError::ActorDead)
    }

    /// Retourne tous les agents enregistrés.
    pub async fn list_agents(&self) -> Result<Vec<AgentEntry>, AgentRegistryError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(RegistryMessage::ListAgents { reply: reply_tx })
            .await
            .map_err(|_| AgentRegistryError::ActorDead)?;
        reply_rx.await.map_err(|_| AgentRegistryError::ActorDead)
    }

    /// Demande l'arrêt de l'acteur (fire-and-forget).
    /// Les messages déjà en file sont traités avant l'arrêt.
    pub fn shutdown(&self) {
        let _ = self.tx.try_send(RegistryMessage::Shutdown);
    }
}
```

### Dépendances Cargo

Aucune nouvelle dépendance — tout est déjà présent depuis STORY-007.

### Comportement attendu

Chaque méthode du Handle suit le même protocole :
1. Créer un `oneshot::channel()`
2. Envoyer le message via `mpsc::Sender` (async, avec `await`)
3. Attendre la réponse sur le `oneshot::Receiver`
4. Mapper les erreurs canal vers `AgentRegistryError::ActorDead`

Le Handle ne contient aucun état — tout l'état est dans l'acteur.
La méthode `shutdown()` est fire-and-forget (`try_send`) car si l'acteur est déjà mort,
on n'a pas besoin de propager l'erreur.

### Ce que cette story N'implémente PAS

- Le routing de tâches via le registry (Sprint 4, TaskRouter)
- La sérialisation JSON du Handle pour l'API REST (Sprint 5)
- La supervision et le restart de l'acteur (Sprint 5, STORY-039)

---

## Tests requis

### Tests unitaires

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use apollia_core::{AgentManifest, ProcessState};
    use tokio::sync::broadcast;

    fn test_manifest() -> AgentManifest {
        AgentManifest {
            name: "handle-test".to_string(),
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
    async fn test_ac1_handle_clone_concurrent() {
        // GIVEN
        let (bus_tx, _) = broadcast::channel(16);
        let handle = AgentRegistry::spawn(bus_tx);
        let handle2 = handle.clone();

        // WHEN — deux appels concurrent via deux handles distincts
        let (r1, r2) = tokio::join!(
            handle.register(test_manifest()),
            handle2.register(test_manifest()),
        );

        // THEN
        assert!(r1.is_ok());
        assert!(r2.is_ok());
        // Les deux IDs sont différents
        assert_ne!(r1.unwrap(), r2.unwrap());
    }

    #[tokio::test]
    async fn test_ac2_actor_dead() {
        // GIVEN
        let (bus_tx, _) = broadcast::channel(16);
        let handle = AgentRegistry::spawn(bus_tx);
        handle.shutdown();
        // Laisser l'acteur se terminer
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        // WHEN
        let result = handle.register(test_manifest()).await;

        // THEN
        assert!(matches!(result.unwrap_err(), AgentRegistryError::ActorDead));
    }

    #[tokio::test]
    async fn test_ac3_list_agents() {
        // GIVEN
        let (bus_tx, _) = broadcast::channel(16);
        let handle = AgentRegistry::spawn(bus_tx);
        handle.register(test_manifest()).await.unwrap();
        handle.register(test_manifest()).await.unwrap();

        // WHEN
        let agents = handle.list_agents().await.unwrap();

        // THEN
        assert_eq!(agents.len(), 2);
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
- [ ] Docstring `///` sur chaque méthode publique du Handle

**Architectural :**
- [ ] `AgentRegistryHandle` est `Clone` + `Send` + `Sync`
- [ ] Aucune méthode du Handle ne contient de logique métier (délégation pure)
- [ ] `ActorDead` retourné sur toute erreur canal

**Commit :**
- [ ] `feat(apollia-runtime): expose AgentRegistryHandle public async API`

---

## Notes d'implémentation

**Décisions prises pendant l'implémentation :**

**Déviations par rapport à la spec :**

**Dette technique identifiée :**

---

## Liens

- Epic parent : Runtime Core — acteurs Tokio
- Story précédente : STORY-007 (AgentRegistry acteur)
- Story suivante : STORY-009 (Test d'intégration)
- ADR associé : —
