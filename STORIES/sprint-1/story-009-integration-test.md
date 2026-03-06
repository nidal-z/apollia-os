# [Sprint 1][apollia-runtime] Test d'intégration EventBus ↔ AgentRegistry

**ID :** STORY-009
**Sprint :** 1
**Crate cible :** `apollia-runtime`
**Fichier(s) cible(s) :** `crates/apollia-runtime/tests/integration_registry.rs`
**Taille :** M
**Dépend de :** STORY-008
**Statut :** ✅ Terminée

---

## User Story

```
En tant que runtime,
je veux un test d'intégration qui valide la communication entre EventBus et AgentRegistry,
afin de prouver que les transitions ProcessState déclenchent les bons RuntimeEvents
dans un scénario multi-composants réaliste.
```

---

## Contexte technique

Ce test est le Sprint Goal de Sprint 1. Il valide que les deux acteurs Tokio fonctionnent
ensemble dans un scénario proche de la réalité :
un agent est enregistré, passe par toutes les transitions d'état valides, et chaque
transition émet le bon `RuntimeEvent` que n'importe quel abonné au bus peut recevoir.

Ce test est placé dans `tests/` (tests d'intégration Rust) car il utilise
`apollia-runtime` comme crate externe — il ne peut pas accéder aux internals.

**Principe(s) architectural(aux) concerné(s) :**
- Principe #4 — Fail fast (transitions invalides vérifiées en test)
- Principe #5 — Un acteur, une responsabilité (EventBus et Registry découplés)

**Position dans l'architecture :**
```
Test d'intégration Sprint 1
  ├── EventBus (STORY-006 ✅)
  └── AgentRegistryHandle (STORY-008 ✅)
        → valide la communication entre les deux
```

---

## Critères d'Acceptation

### AC-1 — Cycle de vie complet d'un agent

```
ÉTANT DONNÉ un EventBus et un AgentRegistry spawné avec ce bus
QUAND on exécute le cycle complet :
     register(manifest) → update_state(Active) → update_state(Degraded)
     → update_state(Active) → update_state(Stopping) → update_state(Stopped)
     → unregister(id)
ALORS chaque étape retourne Ok
     ET les événements reçus sur le bus sont dans l'ordre exact :
     AgentRegistered → AgentReady → AgentDegraded → AgentReady
     → AgentStopped (transition) → AgentStopped (unregister)
```

### AC-2 — Plusieurs agents simultanés

```
ÉTANT DONNÉ un EventBus et un AgentRegistry
QUAND on enregistre 3 agents via 3 tokio::tasks concurrentes
ALORS les 3 registrations retournent Ok
     ET list_agents() retourne exactement 3 entrées
     ET le bus a reçu 3 événements AgentRegistered
```

### AC-3 — Transition invalide n'altère pas l'état

```
ÉTANT DONNÉ un agent en état Active
QUAND on tente update_state(Initializing) (transition invalide)
ALORS AgentRegistryError::InvalidTransition est retourné
     ET get_agent(id) retourne toujours ProcessState::Active
     ET aucun event supplémentaire n'est publié sur le bus
```

### AC-4 — Unregister d'un agent inconnu

```
ÉTANT DONNÉ un AgentRegistry sans agent "ghost-id"
QUAND on appelle unregister("ghost-id")
ALORS AgentRegistryError::NotFound("ghost-id") est retourné
```

---

## Spécification technique

### Types à créer / modifier

Aucun nouveau type — ce test utilise uniquement les APIs publiques de STORY-006/007/008.

### Dépendances Cargo

```toml
# crates/apollia-runtime/Cargo.toml — section [dev-dependencies]
[dev-dependencies]
tokio = { workspace = true, features = ["rt-multi-thread", "macros", "time"] }
```

### Comportement attendu

Le test utilise un `broadcast::Receiver` dédié pour collecter les événements.
Pour éviter les race conditions dans la vérification des événements, on utilise
`tokio::time::timeout` avec un délai court (100ms) pour éviter que le test
ne bloque indéfiniment si un événement n'arrive pas.

Helper `collect_events(rx, count, timeout_ms)` : collecte `count` événements du bus
avec un timeout, retourne `Vec<RuntimeEvent>`. Pratique pour les ACs qui vérifient
plusieurs événements en séquence.

### Ce que cette story N'implémente PAS

- Tests de performance ou de charge (hors scope Sprint 1)
- Tests avec de vrais agents Python (Sprint 4)
- Tests du graceful shutdown complet (Sprint 5)

---

## Tests requis

### Test d'intégration

```rust
// crates/apollia-runtime/tests/integration_registry.rs

use apollia_core::{AgentManifest, ProcessState, RuntimeEvent};
use apollia_runtime::{AgentRegistry, EventBus};
use tokio::time::{timeout, Duration};

fn make_manifest(name: &str) -> AgentManifest {
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

/// Collecte `count` événements du receiver avec un timeout.
async fn collect_events(
    rx: &mut tokio::sync::broadcast::Receiver<RuntimeEvent>,
    count: usize,
    timeout_ms: u64,
) -> Vec<RuntimeEvent> {
    let mut events = Vec::with_capacity(count);
    for _ in 0..count {
        match timeout(Duration::from_millis(timeout_ms), rx.recv()).await {
            Ok(Ok(event)) => events.push(event),
            _ => break,
        }
    }
    events
}

#[tokio::test]
async fn test_ac1_cycle_de_vie_complet() {
    // GIVEN
    let (bus_tx, mut bus_rx) = EventBus::new();
    let registry = AgentRegistry::spawn(bus_tx);

    // WHEN — cycle complet
    let id = registry.register(make_manifest("agent-lifecycle")).await.unwrap();
    registry.update_state(&id, ProcessState::Active).await.unwrap();
    registry.update_state(&id, ProcessState::Degraded).await.unwrap();
    registry.update_state(&id, ProcessState::Active).await.unwrap();
    registry.update_state(&id, ProcessState::Stopping).await.unwrap();
    registry.update_state(&id, ProcessState::Stopped).await.unwrap();
    registry.unregister(&id).await.unwrap();

    // THEN — collecter les événements (7 attendus)
    let events = collect_events(&mut bus_rx, 7, 200).await;
    assert_eq!(events.len(), 7, "Attendu 7 événements, reçu {}", events.len());

    assert!(matches!(&events[0], RuntimeEvent::AgentRegistered(eid) if eid == &id));
    assert!(matches!(&events[1], RuntimeEvent::AgentReady(eid) if eid == &id));
    assert!(matches!(&events[2], RuntimeEvent::AgentDegraded { agent_id, .. } if agent_id == &id));
    assert!(matches!(&events[3], RuntimeEvent::AgentReady(eid) if eid == &id));
    // events[4] = AgentStopped (transition Stopping→Stopped n'émet pas d'event — à vérifier)
    assert!(matches!(&events[4], RuntimeEvent::AgentStopped(eid) if eid == &id));
    assert!(matches!(&events[5], RuntimeEvent::AgentStopped(eid) if eid == &id));
}

#[tokio::test]
async fn test_ac2_agents_simultanes() {
    // GIVEN
    let (bus_tx, mut bus_rx) = EventBus::new();
    let registry = AgentRegistry::spawn(bus_tx.clone());
    let r2 = registry.clone();
    let r3 = registry.clone();

    // WHEN — 3 registrations concurrentes
    let (id1, id2, id3) = tokio::join!(
        registry.register(make_manifest("agent-a")),
        r2.register(make_manifest("agent-b")),
        r3.register(make_manifest("agent-c")),
    );

    assert!(id1.is_ok() && id2.is_ok() && id3.is_ok());

    // THEN — list_agents retourne 3 entrées
    let agents = registry.list_agents().await.unwrap();
    assert_eq!(agents.len(), 3);

    // ET 3 événements AgentRegistered sur le bus
    let events = collect_events(&mut bus_rx, 3, 200).await;
    assert_eq!(events.len(), 3);
    assert!(events
        .iter()
        .all(|e| matches!(e, RuntimeEvent::AgentRegistered(_))));
}

#[tokio::test]
async fn test_ac3_transition_invalide_preserve_etat() {
    // GIVEN
    let (bus_tx, mut bus_rx) = EventBus::new();
    let registry = AgentRegistry::spawn(bus_tx);
    let id = registry.register(make_manifest("agent-stable")).await.unwrap();
    registry.update_state(&id, ProcessState::Active).await.unwrap();

    // Vider les 2 événements déjà publiés
    collect_events(&mut bus_rx, 2, 100).await;

    // WHEN — transition invalide Active → Initializing
    let result = registry.update_state(&id, ProcessState::Initializing).await;

    // THEN — erreur retournée
    assert!(matches!(
        result.unwrap_err(),
        apollia_runtime::AgentRegistryError::InvalidTransition { .. }
    ));

    // L'état est toujours Active
    let entry = registry.get_agent(&id).await.unwrap().unwrap();
    assert!(matches!(entry.process_state, ProcessState::Active));

    // Aucun événement supplémentaire publié
    let extra_events = collect_events(&mut bus_rx, 1, 50).await;
    assert!(extra_events.is_empty());
}

#[tokio::test]
async fn test_ac4_unregister_agent_inconnu() {
    // GIVEN
    let (bus_tx, _) = EventBus::new();
    let registry = AgentRegistry::spawn(bus_tx);

    // WHEN
    let result = registry.unregister("ghost-id").await;

    // THEN
    assert!(matches!(
        result.unwrap_err(),
        apollia_runtime::AgentRegistryError::NotFound(id) if id == "ghost-id"
    ));
}
```

---

## Definition of Done

**Qualité code :**
- [ ] `cargo test -p apollia-runtime` passe (tous les tests, y compris intégration)
- [ ] `cargo clippy -p apollia-runtime -- -D warnings` : zéro warning
- [ ] `cargo fmt --check` : code formatté
- [ ] Zéro `unwrap()` dans le code de test (utiliser `assert!` ou `unwrap_err()` avec assertion)
- [ ] Pas de `tokio::time::sleep` arbitraire — utiliser `timeout` + `collect_events`

**Architectural :**
- [ ] Tests en `tests/` (intégration) — pas dans `src/` (unitaire)
- [ ] Les tests n'accèdent pas aux internals de l'acteur (uniquement via Handle public)
- [ ] Sprint Goal validé : le test AC-1 (cycle de vie complet) passe

**Documentation :**
- [ ] Entrée dans `docs/Decisions-Log.md` si une décision sur les events a été faite
- [ ] `sprint-index.md` mis à jour : Sprint 1 → toutes stories ✅

**Commit :**
- [ ] `test(apollia-runtime): add EventBus ↔ AgentRegistry integration tests`

---

## Notes d'implémentation

**Décisions prises pendant l'implémentation :**
- `AgentRegistry` rendu `pub` (était `pub(crate)`) pour permettre l'import depuis les tests d'intégration.
  Le struct n'expose aucun champ public, seul `spawn()` est accessible — pas de fuite d'internals.
- Re-exporté dans `lib.rs` aux côtés de `AgentRegistryHandle`.

**Déviations par rapport à la spec :**
- La spec indiquait 7 événements en AC-1, mais le cycle réel en produit 6 : la transition vers
  `Stopping` n'émet aucun événement car `RuntimeEvent` ne possède pas de variante `AgentStopping`.
  Le texte de l'AC-1 (description) confirmait bien 6 événements — seul le code d'exemple était erroné.
  Corrigé sans ADR (pas de décision architecturale, simple bug dans le template de story).

**Dette technique identifiée :**
- Aucune.

---

## Liens

- Epic parent : Runtime Core — acteurs Tokio
- Story précédente : STORY-008 (AgentRegistryHandle)
- Story suivante : STORY-010 (Sprint 2 — ToolDescriptor)
- ADR associé : —
