# [SPRINT-5][apollia-runtime] Routes REST tasks (POST/GET/DELETE)

**ID :** STORY-034
**Sprint :** 5
**Crate cible :** `apollia-runtime`
**Fichier(s) cible(s) :** `crates/apollia-runtime/src/api/routes_tasks.rs`
**Taille :** M
**Depend de :** STORY-033 (APIServer skeleton)
**Statut :** ✅ Terminee

---

## User Story

```
En tant que CLI ou SDK Python,
je veux soumettre, consulter et annuler des taches via des routes REST,
afin de piloter l'execution d'agents sans modifier le code du runtime.
```

---

## Contexte technique

Les routes tasks sont le coeur de l'API : elles permettent de soumettre une tache a un agent, consulter son statut, et l'annuler. Elles utilisent le `TaskRouterHandle` (STORY-032) pour le dispatch et l'`AgentRegistryHandle` (STORY-008) pour verifier l'etat des agents.

**Principe(s) architectural(aux) concerne(s) :**
- Principe #8 — CLI humaine, API machine (routes REST JSON)
- Principe #4 — Fail fast (validation input au niveau API)

**Position dans l'architecture :**
```
APIServer (STORY-033)
    └── /api/v1/tasks/*  ← cette story
          └── TaskRouterHandle (STORY-032)
```

---

## Criteres d'Acceptation

### AC-1 — POST /api/v1/tasks soumet une tache

```
ETANT DONNE un agent "hello-agent" enregistre et ACTIVE
QUAND POST /api/v1/tasks avec {"agent_id": "hello-agent", "input": {"prompt": "Bonjour"}}
ALORS 202 Accepted avec {"task_id": "t-xxx", "status": "submitted"}
ET la tache est routee via TaskRouterHandle
```

### AC-2 — GET /api/v1/tasks/{id} retourne le statut

```
ETANT DONNE une tache "t-001" soumise
QUAND GET /api/v1/tasks/t-001
ALORS 200 avec {"task_id": "t-001", "status": "running|completed|failed|canceled", ...}
```

### AC-3 — DELETE /api/v1/tasks/{id} annule une tache

```
ETANT DONNE une tache "t-001" en cours d'execution
QUAND DELETE /api/v1/tasks/t-001
ALORS 200 avec {"task_id": "t-001", "status": "canceled"}
ET la tache est marquee comme annulee
```

### AC-4 — POST avec agent inexistant retourne 404

```
ETANT DONNE aucun agent "ghost-agent" enregistre
QUAND POST /api/v1/tasks avec {"agent_id": "ghost-agent", "input": {...}}
ALORS 404 avec {"error": "agent not found: ghost-agent"}
```

### AC-5 — POST avec body invalide retourne 400

```
ETANT DONNE un body JSON malformed ou champs manquants
QUAND POST /api/v1/tasks
ALORS 400 avec {"error": "invalid request: ..."}
```

### AC-6 — GET avec task_id inexistant retourne 404

```
ETANT DONNE aucune tache "t-999"
QUAND GET /api/v1/tasks/t-999
ALORS 404 avec {"error": "task not found: t-999"}
```

---

## Specification technique

### Types a creer

```rust
/// Request body for POST /api/v1/tasks.
#[derive(Debug, Deserialize)]
pub struct SubmitTaskRequest {
    pub agent_id: String,
    pub input: serde_json::Value,
}

/// Response body for task operations.
#[derive(Debug, Serialize)]
pub struct TaskResponse {
    pub task_id: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Standard error response.
#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub error: String,
}
```

### Dependances Cargo

Aucune nouvelle — utilise `axum`, `serde`, `serde_json` deja dans le workspace.

### Comportement attendu

1. `POST /api/v1/tasks` : deserialise `SubmitTaskRequest`, convertit en `AIPTask`, appelle `TaskRouterHandle::submit()`, retourne 202 avec le `task_id`
2. `GET /api/v1/tasks/{id}` : interroge le `TaskRouterHandle::get_status()` (a ajouter si inexistant), retourne 200 avec le statut complet
3. `DELETE /api/v1/tasks/{id}` : appelle `TaskRouterHandle::cancel()` (a ajouter si inexistant), retourne 200
4. Les erreurs de routing (agent non trouve, agent degrade) sont converties en codes HTTP 404/503 via un mapping `SubmitError → StatusCode`

### Ce que cette story N'implemente PAS

- Le SSE streaming des taches — STORY-036
- La pagination des taches (GET /api/v1/tasks sans id) — hors scope MVP
- Le retry automatique — STORY-042 (Sprint 6)

---

## Tests requis

### Tests unitaires

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::StatusCode;
    use axum_test::TestServer; // ou axum::test helpers

    #[tokio::test]
    async fn test_submit_task_returns_202() {
        // GIVEN un AppState avec un mock TaskRouterHandle
        // WHEN POST /api/v1/tasks avec agent_id valide
        // THEN 202 Accepted
    }

    #[tokio::test]
    async fn test_submit_task_unknown_agent_returns_404() {
        // GIVEN un AppState sans l'agent "ghost"
        // WHEN POST /api/v1/tasks avec agent_id "ghost"
        // THEN 404
    }

    #[tokio::test]
    async fn test_submit_task_invalid_body_returns_400() {
        // GIVEN un body JSON invalide
        // WHEN POST /api/v1/tasks
        // THEN 400
    }

    #[tokio::test]
    async fn test_get_task_status_returns_200() {
        // GIVEN une tache soumise
        // WHEN GET /api/v1/tasks/{id}
        // THEN 200 avec le statut
    }

    #[tokio::test]
    async fn test_get_task_not_found_returns_404() {
        // GIVEN aucune tache avec cet id
        // WHEN GET /api/v1/tasks/unknown
        // THEN 404
    }

    #[tokio::test]
    async fn test_cancel_task_returns_200() {
        // GIVEN une tache en cours
        // WHEN DELETE /api/v1/tasks/{id}
        // THEN 200 avec status "canceled"
    }
}
```

---

## Definition of Done

**Qualite code :**
- [ ] `cargo test -p apollia-runtime` passe (0 test ignore)
- [ ] `cargo clippy -p apollia-runtime -- -D warnings` : zero warning
- [ ] `cargo fmt --check` : code formate
- [ ] Zero `unwrap()` dans le code de production
- [ ] Docstring `///` sur chaque struct, enum, et fonction publique

**Commit :**
- [ ] `feat(apollia-runtime): add REST routes for tasks (POST/GET/DELETE)`

---

## Notes d'implementation

**Decisions prises pendant l'implementation :**
- `SubmitTaskRequest.input` (serde_json::Value) est converti en `AIPInput` avec un seul `DataPart` — API flexible, pas de couplage callers/AIPPart
- Ajout de `Cancel` message + `cancel()` method sur `TaskRouterHandle` (inexistant avant cette story)
- `handle_cancel()` ne cancel que les taches en etat Submitted/Working/InputRequired, retourne le statut courant sinon
- axum 0.7.9 : path params utilisent la syntaxe `:id` (et non `{id}` qui est axum 0.8+)
- axum retourne 422 (pas 400) pour les erreurs de deserialisation JSON — AC-5 adapte en consequence

**Deviations par rapport a la spec :**
- AC-5 : axum renvoie 422 Unprocessable Entity au lieu de 400 Bad Request pour body invalide (comportement natif axum, pas de custom error handler)

**Dette technique identifiee :**
- Le TaskRouter ne met pas a jour le statut des taches completees (il les garde en Working) — sera corrige quand le ExecutionBackend reportera les completions

---

## Liens

- Story precedente : STORY-033 (APIServer)
- Story suivante : STORY-035 (Routes agents), STORY-036 (SSE)
- Spec : `docs/Briques-Runtime-Core.md` (section Routes REST)
