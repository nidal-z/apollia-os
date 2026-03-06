# [SPRINT-5][apollia-runtime] Graceful shutdown SIGTERM/drain 30s

**ID :** STORY-040
**Sprint :** 5
**Crate cible :** `apollia-runtime`
**Fichier(s) cible(s) :** `crates/apollia-runtime/src/shutdown.rs`
**Taille :** M
**Depend de :** STORY-039 (Supervisor)
**Statut :** ✅ Terminee

---

## User Story

```
En tant que runtime,
je veux un mecanisme de graceful shutdown qui draine les taches en cours avant de s'arreter,
afin de ne jamais perdre silencieusement une tache en execution.
```

---

## Contexte technique

Le graceful shutdown est declenche par SIGTERM, SIGINT (Ctrl+C), ou `apollia-os stop`. Il doit drainer les taches en cours (timeout 30s), appeler `on_stop()` sur chaque agent Python, fermer les connexions SQLite, et arreter tous les acteurs Tokio dans l'ordre inverse du demarrage. Les taches non terminees apres 30s sont annulees et tracees dans l'audit log.

**Principe(s) architectural(aux) concerne(s) :**
- Principe #7 — Garde-fous non-negociables (drain timeout, aucune tache perdue silencieusement)
- Principe #1 — Local-first (flush SQLite avant fermeture)

**Position dans l'architecture :**
```
SIGTERM / SIGINT / POST /api/v1/shutdown
    └── ShutdownController  ← cette story
          ├── EventBus.broadcast(ShutdownRequested)
          ├── APIServer.stop_accepting()
          ├── TaskRouter.reject_new()
          ├── Drain in-progress tasks (30s timeout)
          ├── MemoryEngine.flush()
          └── Supervisor.stop_all()
```

---

## Criteres d'Acceptation

### AC-1 — SIGTERM declenche le shutdown

```
ETANT DONNE un runtime demarre
QUAND SIGTERM est recu
ALORS ShutdownRequested est broadcast sur l'EventBus
ET le processus de shutdown commence
```

### AC-2 — SIGINT (Ctrl+C) declenche le shutdown

```
ETANT DONNE un runtime demarre en foreground
QUAND Ctrl+C est presse
ALORS le meme processus de shutdown que SIGTERM est declenche
ET un second Ctrl+C force l'arret immediat (exit(1))
```

### AC-3 — APIServer refuse les nouvelles connexions

```
ETANT DONNE un shutdown en cours
QUAND un client HTTP tente une nouvelle requete
ALORS 503 Service Unavailable avec {"error": "runtime shutting down"}
```

### AC-4 — Taches en cours sont drainees (30s)

```
ETANT DONNE une tache en cours d'execution
QUAND le shutdown est declenche
ALORS le runtime attend jusqu'a 30s que la tache se termine
ET si la tache se termine dans les 30s, elle est completee normalement
```

### AC-5 — Taches non terminees sont annulees apres 30s

```
ETANT DONNE une tache qui ne se termine pas dans les 30s
QUAND le drain timeout expire
ALORS la tache est marquee CANCELED
ET un audit event "task_canceled_shutdown" est enregistre
ET tracing::warn! est emis avec le task_id
```

### AC-6 — Agents recoivent on_stop()

```
ETANT DONNE des agents Python ACTIVE
QUAND le shutdown est declenche
ALORS chaque agent transite ACTIVE → STOPPING
ET on_stop() est appele via AIPBridge (si disponible)
ET chaque agent transite STOPPING → STOPPED
```

### AC-7 — Arret ordonne des acteurs

```
ETANT DONNE le shutdown en cours, toutes les taches drainees
QUAND les acteurs sont arretes
ALORS l'ordre d'arret est inverse du demarrage :
  APIServer → TaskRouter → MemoryEngine → ToolRegistry → AgentRegistry → EventBus
```

### AC-8 — Exit code 0 sur shutdown propre

```
ETANT DONNE un shutdown complete sans erreur
QUAND le processus se termine
ALORS exit code = 0
ET tracing::info! "Runtime stopped gracefully"
```

---

## Specification technique

### Types a creer

```rust
/// Shutdown configuration.
pub struct ShutdownConfig {
    pub drain_timeout_secs: u64,  // default: 30
}

/// Shutdown controller.
pub struct ShutdownController {
    config: ShutdownConfig,
    event_sender: EventBusSender,
}

/// Shutdown errors.
#[derive(Debug, thiserror::Error)]
pub enum ShutdownError {
    #[error("drain timeout: {count} tasks still running after {timeout_secs}s")]
    DrainTimeout { count: u32, timeout_secs: u64 },

    #[error("actor {actor} failed to stop: {reason}")]
    ActorStopFailed { actor: String, reason: String },
}
```

### Dependances Cargo

```toml
# Signal handling
[dependencies]
tokio = { workspace = true, features = ["signal"] }
```

### Comportement attendu

1. `ShutdownController::new(config, event_sender)` construit le controller
2. `ShutdownController::install_signal_handlers()` enregistre les handlers SIGTERM et SIGINT via `tokio::signal`
3. Quand un signal est recu :
   - Broadcast `RuntimeEvent::ShutdownRequested` sur l'EventBus
   - APIServer arrete d'accepter des connexions (via `APIServerHandle::shutdown()`)
   - TaskRouter rejette les nouvelles soumissions
   - Attend que toutes les taches en cours se terminent (avec timeout 30s)
   - Appelle `on_stop()` sur chaque agent via leur coordinator
   - Arrete les acteurs dans l'ordre inverse
4. Un second SIGINT pendant le drain force `exit(1)` (panic-safe)
5. Les taches non drainees sont annulees et tracees

### Ce que cette story N'implemente PAS

- Le drain de connexions HTTP existantes (axum gere cela en interne) — hors scope
- La sauvegarde d'etat pour reprise apres restart — hors scope MVP
- Le signal SIGHUP pour reload configuration — hors scope MVP

---

## Tests requis

### Tests unitaires

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_shutdown_broadcasts_event() {
        // GIVEN un ShutdownController avec un EventBus
        // WHEN shutdown() est appele
        // THEN ShutdownRequested est broadcast
    }

    #[tokio::test]
    async fn test_drain_waits_for_tasks() {
        // GIVEN une tache en cours
        // WHEN drain(30s) est appele
        // ET la tache se termine en 2s
        // THEN drain retourne Ok(())
    }

    #[tokio::test]
    async fn test_drain_timeout_cancels_tasks() {
        // GIVEN une tache qui ne se termine jamais
        // WHEN drain(1s) est appele (timeout court pour le test)
        // THEN DrainTimeout est retourne
        // ET la tache est marquee canceled
    }

    #[tokio::test]
    async fn test_actors_stopped_in_reverse_order() {
        // GIVEN tous les acteurs demarres
        // WHEN stop_all() est appele
        // THEN l'ordre d'arret est inverse du demarrage
    }

    #[tokio::test]
    async fn test_shutdown_config_defaults() {
        // GIVEN ShutdownConfig par defaut
        // THEN drain_timeout_secs = 30
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
- [ ] Aucune tache perdue silencieusement (toutes tracees dans l'audit)
- [ ] Arret ordonne respecte (inverse du demarrage)
- [ ] Double Ctrl+C → force exit (safety net)

**Commit :**
- [ ] `feat(apollia-runtime): add graceful shutdown with 30s drain timeout`

---

## Liens

- Story precedente : STORY-039 (Supervisor)
- Story suivante : STORY-037 (CLI start/stop)
- Spec : `docs/Briques-Runtime-Core.md` (section Graceful Shutdown)
