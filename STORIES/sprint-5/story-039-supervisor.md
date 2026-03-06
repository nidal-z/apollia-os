# [SPRINT-5][apollia-runtime] Supervisor demarrage ordonne + watchdog

**ID :** STORY-039
**Sprint :** 5
**Crate cible :** `apollia-runtime`
**Fichier(s) cible(s) :** `crates/apollia-runtime/src/supervisor.rs`
**Taille :** L
**Depend de :** STORY-033 (APIServer), STORY-034 (Routes tasks), STORY-035 (Routes agents)
**Statut :** 🔲 A faire

---

## User Story

```
En tant que runtime,
je veux un Supervisor qui demarre tous les acteurs dans l'ordre correct avec timeout,
afin de garantir que le systeme est dans un etat coherent avant d'accepter des requetes.
```

---

## Contexte technique

Le Supervisor est le composant d'orchestration central. Il demarre les 6 acteurs du runtime en sequence stricte (EventBus → AgentRegistry → ToolRegistry → MemoryEngine → TaskRouter → APIServer), chacun devant emettre `RuntimeEvent::Ready` avant que le suivant ne demarre. Si un acteur ne repond pas en 10s, le demarrage echoue. Le Supervisor implemente aussi une politique de restart simple (watchdog).

**Principe(s) architectural(aux) concerne(s) :**
- Principe #4 — Fail fast (timeout au demarrage, erreur immediate si un acteur ne repond pas)
- Principe #5 — Un acteur, une responsabilite (le Supervisor orchestre, chaque acteur gere son domaine)

**Position dans l'architecture :**
```
Supervisor  ← cette story
    ├── 1. EventBus
    ├── 2. AgentRegistry
    ├── 3. ToolRegistry
    ├── 4. MemoryEngine (MemoryManager)
    ├── 5. TaskRouter + ExecutionCoordinators
    └── 6. APIServer
```

---

## Criteres d'Acceptation

### AC-1 — Demarrage sequentiel ordonne

```
ETANT DONNE un Supervisor configure
QUAND start() est appele
ALORS les acteurs sont demarres dans l'ordre : EventBus, AgentRegistry, ToolRegistry, MemoryEngine, TaskRouter, APIServer
ET chaque acteur emet Ready sur l'EventBus avant que le suivant ne demarre
```

### AC-2 — Timeout au demarrage

```
ETANT DONNE un acteur qui ne repond pas dans les 10 secondes
QUAND le Supervisor attend son Ready
ALORS SupervisorError::StartupTimeout { actor: "tool_registry" } est retourne
ET les acteurs deja demarres sont arretes dans l'ordre inverse
```

### AC-3 — Tous les handles sont accessibles apres demarrage

```
ETANT DONNE un Supervisor dont start() a reussi
QUAND on accede aux handles
ALORS SupervisorHandles contient : EventBusSender, AgentRegistryHandle, TaskRouterHandle, APIServerHandle
ET chaque handle est Clone + Send + Sync
```

### AC-4 — AllReady event emis

```
ETANT DONNE tous les acteurs demarres avec succes
QUAND le dernier acteur (APIServer) emet Ready
ALORS le Supervisor emet RuntimeEvent::AllReady sur l'EventBus
```

### AC-5 — Restart policy basique

```
ETANT DONNE un acteur configure avec RestartPolicy::OnFailure
QUAND l'acteur panique (task Tokio completed avec erreur)
ALORS le Supervisor tente de le redemarrer
ET si le nombre de restarts depasse max_restarts (5) en 60s, le runtime s'arrete avec exit(1)
```

### AC-6 — Erreur de configuration au demarrage

```
ETANT DONNE une configuration invalide (port deja utilise, chemin invalide)
QUAND start() est appele
ALORS SupervisorError::ConfigError est retourne avec le detail
ET aucun acteur n'est laisse en cours d'execution
```

---

## Specification technique

### Types a creer

```rust
/// Restart policy for supervised actors.
#[derive(Debug, Clone)]
pub enum RestartPolicy {
    /// Always restart (ToolRegistry, MemoryEngine).
    Always,
    /// Restart only on failure/panic (APIServer).
    OnFailure,
    /// Never restart (one-shot actors).
    Never,
}

/// Specification for a supervised child actor.
#[derive(Debug)]
pub struct ChildSpec {
    pub name: String,
    pub restart_policy: RestartPolicy,
    pub max_restarts: u32,
    pub restart_window_secs: u64,
}

/// Supervisor configuration.
pub struct SupervisorConfig {
    pub api_config: APIServerConfig,
    pub startup_timeout_secs: u64,
}

/// Handles returned after successful startup.
pub struct SupervisorHandles {
    pub event_sender: EventBusSender,
    pub registry_handle: AgentRegistryHandle,
    pub router_handle: TaskRouterHandle</* B */>,
    pub api_handle: APIServerHandle,
}

/// Supervisor errors.
#[derive(Debug, thiserror::Error)]
pub enum SupervisorError {
    #[error("startup timeout: {actor} did not become ready within {timeout_secs}s")]
    StartupTimeout { actor: String, timeout_secs: u64 },

    #[error("actor {actor} failed to start: {reason}")]
    ActorStartFailed { actor: String, reason: String },

    #[error("configuration error: {0}")]
    ConfigError(String),

    #[error("max restarts exceeded for {actor}: {count} restarts in {window_secs}s")]
    MaxRestartsExceeded { actor: String, count: u32, window_secs: u64 },
}
```

### Dependances Cargo

Aucune nouvelle — utilise `tokio::time::timeout`, `tokio::sync::watch`, `tracing`.

### Comportement attendu

1. `Supervisor::new(config)` construit le Supervisor avec la configuration
2. `Supervisor::start()` execute la sequence de demarrage :
   - Pour chaque acteur dans l'ordre : spawn l'acteur, attend `Ready` sur l'EventBus avec timeout 10s
   - Si timeout : rollback (arret des acteurs deja demarres dans l'ordre inverse), retourne erreur
   - Si tous OK : emet `AllReady`, retourne `SupervisorHandles`
3. `Supervisor::watch()` (async) surveille les JoinHandle de chaque acteur :
   - Si un acteur termine avec erreur et policy = Always/OnFailure → restart
   - Si max_restarts depasse → arret complet du runtime
   - Si policy = Never → log et ignore
4. Le Supervisor ne detient aucun etat metier — il orchestre uniquement le cycle de vie

### Ce que cette story N'implemente PAS

- Le chargement d'agents au demarrage depuis un fichier de configuration — hors scope MVP
- Le health check periodique des acteurs (heartbeat) — hors scope MVP
- Le mode cluster / multi-Supervisor — hors scope
- La lecture de `apollia.toml` — hors scope MVP (configuration programmatique pour l'instant)

---

## Tests requis

### Tests unitaires

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_startup_sequence_all_ready() {
        // GIVEN un Supervisor avec des mock acteurs qui emettent Ready
        // WHEN start() est appele
        // THEN tous les acteurs demarrent dans l'ordre
        // ET AllReady est emis
    }

    #[tokio::test]
    async fn test_startup_timeout_rollback() {
        // GIVEN un acteur qui ne repond jamais
        // WHEN start() est appele avec timeout 1s
        // THEN StartupTimeout est retourne
        // ET les acteurs deja demarres sont arretes
    }

    #[tokio::test]
    async fn test_restart_on_failure() {
        // GIVEN un acteur configure OnFailure
        // WHEN l'acteur panique
        // THEN le Supervisor le redemarre
    }

    #[tokio::test]
    async fn test_max_restarts_exceeded() {
        // GIVEN un acteur qui panique en boucle
        // WHEN max_restarts (5) est depasse en 60s
        // THEN MaxRestartsExceeded est retourne
    }

    #[tokio::test]
    async fn test_all_ready_event_emitted() {
        // GIVEN tous les acteurs demarres
        // WHEN le dernier acteur est ready
        // THEN RuntimeEvent::AllReady est emis sur l'EventBus
    }

    #[tokio::test]
    async fn test_handles_accessible_after_start() {
        // GIVEN un Supervisor demarre avec succes
        // WHEN on accede a SupervisorHandles
        // THEN tous les handles sont presents et utilisables
    }
}
```

---

## Definition of Done

**Qualite code :**
- [ ] `cargo test -p apollia-runtime` passe
- [ ] `cargo clippy -p apollia-runtime -- -D warnings` : zero warning
- [ ] `cargo fmt --check` : code formate
- [ ] Zero `unwrap()` en production
- [ ] Docstring `///` sur chaque type/fn publique

**Architectural :**
- [ ] Demarrage sequentiel strict respecte
- [ ] Rollback en cas d'echec (pas d'acteur orphelin)
- [ ] Pattern acteur respecte (pas de Arc<Mutex> cross-acteurs)

**Commit :**
- [ ] `feat(apollia-runtime): add Supervisor with ordered startup and restart policy`

---

## Liens

- Story precedente : STORY-033 (APIServer), STORY-034, STORY-035
- Story suivante : STORY-040 (Graceful shutdown)
- Spec : `docs/Briques-Runtime-Core.md` (section Supervisor)
