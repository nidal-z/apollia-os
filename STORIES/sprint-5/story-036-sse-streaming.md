# [SPRINT-5][apollia-runtime] SSE streaming pour taches

**ID :** STORY-036
**Sprint :** 5
**Crate cible :** `apollia-runtime`
**Fichier(s) cible(s) :** `crates/apollia-runtime/src/api/routes_sse.rs`
**Taille :** M
**Depend de :** STORY-034 (Routes REST tasks)
**Statut :** 🔲 A faire

---

## User Story

```
En tant que CLI ou SDK Python,
je veux streamer les evenements d'execution d'une tache en temps reel via SSE,
afin d'afficher la progression step-by-step sans polling.
```

---

## Contexte technique

Le SSE (Server-Sent Events) permet a la CLI et aux SDK de recevoir les evenements d'execution en temps reel : chaque step, tool call, observation, et completion. L'EventBus (STORY-006) emet des `RuntimeEvent` en broadcast ; cette story filtre les evenements par `task_id` et les pousse via `text/event-stream`.

**Principe(s) architectural(aux) concerne(s) :**
- Principe #8 — CLI humaine, API machine (streaming pour feedback temps reel)

**Position dans l'architecture :**
```
ExecutionCoordinator (STORY-031)
    └── EventBus broadcast (STORY-006)
          └── APIServer SSE endpoint  ← cette story
                └── GET /api/v1/tasks/{id}/stream
```

---

## Criteres d'Acceptation

### AC-1 — SSE endpoint retourne un flux d'evenements

```
ETANT DONNE une tache "t-001" en cours d'execution
QUAND GET /api/v1/tasks/t-001/stream avec Accept: text/event-stream
ALORS le serveur retourne Content-Type: text/event-stream
ET chaque RuntimeEvent lie a "t-001" est envoye comme SSE event
```

### AC-2 — Evenements step et tool_call sont streames

```
ETANT DONNE une tache en execution qui effectue des steps et tool calls
QUAND le client ecoute sur /api/v1/tasks/{id}/stream
ALORS il recoit des events :
  data: {"event": "step", "step": 1}
  data: {"event": "tool_call", "tool": "file_io", "input": "..."}
  data: {"event": "completed", "result": {...}}
```

### AC-3 — Le stream se ferme a la completion

```
ETANT DONNE une tache "t-001" en cours de streaming
QUAND la tache se termine (completed/failed/canceled)
ALORS un event final "completed"/"failed"/"canceled" est envoye
ET le stream SSE se ferme
```

### AC-4 — Task inexistante retourne 404

```
ETANT DONNE aucune tache "t-999"
QUAND GET /api/v1/tasks/t-999/stream
ALORS 404 avec {"error": "task not found: t-999"}
```

### AC-5 — Deconnexion client ne bloque pas le runtime

```
ETANT DONNE un client SSE connecte a /api/v1/tasks/{id}/stream
QUAND le client se deconnecte (ferme la connexion)
ALORS le subscriber EventBus pour ce client est nettoye
ET aucune fuite de memoire ou de goroutine
```

---

## Specification technique

### Types a creer

```rust
/// SSE event sent to the client.
#[derive(Debug, Serialize)]
pub struct SseTaskEvent {
    pub event: String,  // "step", "tool_call", "observation", "completed", "failed", "canceled"
    #[serde(flatten)]
    pub data: serde_json::Value,
}
```

### Dependances Cargo

```toml
# Potentiellement necessaire pour convertir broadcast → Stream
[dependencies]
tokio-stream = { workspace = true }
```

### Comportement attendu

1. `GET /api/v1/tasks/{id}/stream` : verifie que la tache existe, puis cree un `EventBusReceiver` via `EventBus::subscribe()`
2. Le receiver est wrappe dans un `tokio_stream::wrappers::BroadcastStream` et filtre les `RuntimeEvent` par `task_id`
3. Chaque event pertinent est converti en `axum::response::sse::Event` et pousse au client
4. Quand un event terminal (TaskCompleted, TaskCanceled) est recu, le stream se ferme proprement
5. Si le client se deconnecte, le drop du stream nettoie automatiquement le subscriber

### Ce que cette story N'implemente PAS

- Le streaming de logs texte (stdout/stderr de l'agent) — hors scope MVP
- La reconnexion SSE avec `Last-Event-ID` — hors scope MVP
- Le SSE pour les events systeme (pas par task) — hors scope MVP

---

## Tests requis

### Tests unitaires

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_sse_stream_receives_step_event() {
        // GIVEN une tache en execution
        // WHEN un step event est emis sur l'EventBus
        // THEN le stream SSE contient l'event
    }

    #[tokio::test]
    async fn test_sse_stream_filters_by_task_id() {
        // GIVEN deux taches en execution
        // WHEN des events sont emis pour les deux taches
        // THEN le stream SSE de t-001 ne contient que les events de t-001
    }

    #[tokio::test]
    async fn test_sse_stream_closes_on_completion() {
        // GIVEN une tache en streaming
        // WHEN TaskCompleted est emis
        // THEN le stream se ferme apres l'event final
    }

    #[tokio::test]
    async fn test_sse_unknown_task_returns_404() {
        // GIVEN aucune tache "t-999"
        // WHEN GET /api/v1/tasks/t-999/stream
        // THEN 404
    }
}
```

---

## Definition of Done

**Qualite code :**
- [ ] `cargo test -p apollia-runtime` passe
- [ ] `cargo clippy -p apollia-runtime -- -D warnings` : zero warning
- [ ] Zero `unwrap()` en production
- [ ] Docstring `///` sur chaque type/fn publique

**Commit :**
- [ ] `feat(apollia-runtime): add SSE streaming for task events`

---

## Liens

- Story precedente : STORY-034 (Routes tasks)
- Story suivante : STORY-037 (CLI avec --stream)
- Spec : `docs/Briques-Runtime-Core.md` (section SSE)
