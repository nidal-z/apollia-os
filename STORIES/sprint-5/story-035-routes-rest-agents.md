# [SPRINT-5][apollia-runtime] Routes REST agents (POST/GET/DELETE)

**ID :** STORY-035
**Sprint :** 5
**Crate cible :** `apollia-runtime`
**Fichier(s) cible(s) :** `crates/apollia-runtime/src/api/routes_agents.rs`
**Taille :** M
**Depend de :** STORY-033 (APIServer skeleton)
**Statut :** 🔲 A faire

---

## User Story

```
En tant que CLI ou SDK Python,
je veux lister, demarrer et arreter des agents via des routes REST,
afin de gerer le cycle de vie des agents sans redemarrer le runtime.
```

---

## Contexte technique

Les routes agents exposent la gestion du cycle de vie des agents via l'API REST. Elles utilisent `AgentRegistryHandle` (STORY-008) pour lire l'etat et modifier le `ProcessState` des agents.

**Principe(s) architectural(aux) concerne(s) :**
- Principe #4 — Fail fast (validation agent_id, etat de transition valide)
- Principe #5 — Un acteur, une responsabilite (les routes delegent a AgentRegistryHandle)

**Position dans l'architecture :**
```
APIServer (STORY-033)
    └── /api/v1/agents/*  ← cette story
          └── AgentRegistryHandle (STORY-008)
```

---

## Criteres d'Acceptation

### AC-1 — GET /api/v1/agents liste tous les agents

```
ETANT DONNE 2 agents enregistres ("hello-agent" ACTIVE, "crm-agent" STOPPED)
QUAND GET /api/v1/agents
ALORS 200 avec [{"agent_id": "hello-agent", "state": "active", ...}, {"agent_id": "crm-agent", "state": "stopped", ...}]
```

### AC-2 — POST /api/v1/agents demarre un agent

```
ETANT DONNE un module agent Python "hello-agent" disponible
QUAND POST /api/v1/agents avec {"agent_path": "/path/to/hello_agent.py"}
ALORS 201 Created avec {"agent_id": "hello-agent", "state": "initializing"}
ET l'agent est enregistre dans AgentRegistry
```

### AC-3 — GET /api/v1/agents/{id} retourne le detail

```
ETANT DONNE un agent "hello-agent" enregistre et ACTIVE
QUAND GET /api/v1/agents/hello-agent
ALORS 200 avec {"agent_id": "hello-agent", "state": "active", "manifest": {...}, "tasks_completed": 5}
```

### AC-4 — DELETE /api/v1/agents/{id} arrete un agent

```
ETANT DONNE un agent "hello-agent" en etat ACTIVE
QUAND DELETE /api/v1/agents/hello-agent
ALORS 200 avec {"agent_id": "hello-agent", "state": "stopping"}
ET l'agent transite vers STOPPING puis STOPPED via AgentRegistry
```

### AC-5 — GET agent inexistant retourne 404

```
ETANT DONNE aucun agent "ghost"
QUAND GET /api/v1/agents/ghost
ALORS 404 avec {"error": "agent not found: ghost"}
```

### AC-6 — DELETE agent deja stoppe retourne 409

```
ETANT DONNE un agent "hello-agent" en etat STOPPED
QUAND DELETE /api/v1/agents/hello-agent
ALORS 409 Conflict avec {"error": "agent already stopped: hello-agent"}
```

---

## Specification technique

### Types a creer

```rust
/// Request body for POST /api/v1/agents.
#[derive(Debug, Deserialize)]
pub struct StartAgentRequest {
    pub agent_path: String,
}

/// Response body for agent operations.
#[derive(Debug, Serialize)]
pub struct AgentResponse {
    pub agent_id: String,
    pub state: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub manifest: Option<serde_json::Value>,
}

/// Response body for agent list.
#[derive(Debug, Serialize)]
pub struct AgentListResponse {
    pub agents: Vec<AgentResponse>,
}
```

### Dependances Cargo

Aucune nouvelle.

### Comportement attendu

1. `GET /api/v1/agents` : appelle `AgentRegistryHandle::list()`, serialise chaque `AgentEntry` en JSON
2. `POST /api/v1/agents` : charge et valide l'agent via AIP (loader + validator), enregistre dans AgentRegistry, retourne 201
3. `GET /api/v1/agents/{id}` : appelle `AgentRegistryHandle::get()`, retourne le detail avec manifest
4. `DELETE /api/v1/agents/{id}` : verifie que l'agent n'est pas deja STOPPED, initie la transition STOPPING, retourne 200

### Ce que cette story N'implemente PAS

- Le chargement automatique d'agents au demarrage (via config) — hors scope MVP
- Le restart automatique d'agents — STORY-039 (Supervisor)
- Les logs d'un agent specifique (`agent logs`) — STORY-038 (CLI niveau 2)
- La validation complete du module Python (depend du contexte AIP) — simplifie pour le MVP

---

## Tests requis

### Tests unitaires

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_list_agents_returns_all() {
        // GIVEN 2 agents enregistres
        // WHEN GET /api/v1/agents
        // THEN 200 avec 2 agents
    }

    #[tokio::test]
    async fn test_list_agents_empty() {
        // GIVEN aucun agent
        // WHEN GET /api/v1/agents
        // THEN 200 avec []
    }

    #[tokio::test]
    async fn test_get_agent_detail() {
        // GIVEN un agent "hello-agent" ACTIVE
        // WHEN GET /api/v1/agents/hello-agent
        // THEN 200 avec detail complet
    }

    #[tokio::test]
    async fn test_get_agent_not_found() {
        // GIVEN aucun agent "ghost"
        // WHEN GET /api/v1/agents/ghost
        // THEN 404
    }

    #[tokio::test]
    async fn test_stop_agent_active() {
        // GIVEN un agent ACTIVE
        // WHEN DELETE /api/v1/agents/{id}
        // THEN 200 avec state "stopping"
    }

    #[tokio::test]
    async fn test_stop_agent_already_stopped() {
        // GIVEN un agent STOPPED
        // WHEN DELETE /api/v1/agents/{id}
        // THEN 409 Conflict
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

**Commit :**
- [ ] `feat(apollia-runtime): add REST routes for agents (GET/POST/DELETE)`

---

## Liens

- Story precedente : STORY-033 (APIServer)
- Story suivante : STORY-036 (SSE), STORY-039 (Supervisor)
- Spec : `docs/Briques-Runtime-Core.md` (section Routes REST)
