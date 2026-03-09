# API HTTP — Référence — Apollia OS

> Référence complète de l'API REST locale d'Apollia OS : endpoints, schémas de requête/réponse, codes d'erreur.
> Public cible : développeur intégrant Apollia OS dans un système externe

---

## Vue d'ensemble

L'API HTTP locale est exposée sur deux transports :
- **Unix socket** : `/tmp/apollia.sock` — recommandé pour les processus locaux
- **TCP** : `http://localhost:7771` — compatible avec tout client HTTP

Tous les endpoints retournent du JSON. Authentification : aucune (API locale uniquement).

**Base URL :** `http://localhost:7771/api/v1`

---

## Santé

### GET /api/v1/health

Vérifier que le runtime tourne.

**Réponse 200 :**
```json
{
  "status": "ok",
  "version": "0.1.0",
  "uptime_seconds": 3600
}
```

---

## Agents

### GET /api/v1/agents

Lister tous les agents enregistrés.

**Réponse 200 :**
```json
{
  "agents": [
    {
      "id": "agent-abc123",
      "name": "hello-agent",
      "version": "1.0.0",
      "state": "Active",
      "active_tasks": 0,
      "max_concurrent_tasks": 1
    }
  ]
}
```

### POST /api/v1/agents

Démarrer un agent à partir d'un fichier Python.

**Corps de requête :**
```json
{
  "path": "./agents/hello_agent.py"
}
```

**Réponse 201 :**
```json
{
  "id": "agent-abc123",
  "name": "hello-agent",
  "version": "1.0.0",
  "state": "Initializing"
}
```

**Erreurs :**
- `400` — manifest invalide ou outil requis introuvable
- `409` — agent avec ce nom déjà déployé
- `422` — fichier Python introuvable ou erreur de chargement

### GET /api/v1/agents/:id

Obtenir les détails d'un agent.

**Réponse 200 :**
```json
{
  "id": "agent-abc123",
  "name": "hello-agent",
  "version": "1.0.0",
  "state": "Active",
  "description": "Agent de démonstration minimal",
  "tools_required": [],
  "tools_optional": [],
  "memory_namespace": null,
  "active_tasks": 0,
  "max_concurrent_tasks": 1
}
```

**Erreurs :**
- `404` — agent introuvable

### DELETE /api/v1/agents/:id

Arrêter un agent (graceful drain).

**Réponse 200 :**
```json
{
  "id": "agent-abc123",
  "state": "Stopping"
}
```

**Erreurs :**
- `404` — agent introuvable
- `409` — agent déjà en `Stopping` ou `Stopped`

---

## Tâches

### POST /api/v1/tasks

Soumettre une tâche à un agent.

**Corps de requête :**
```json
{
  "agent_name": "hello-agent",
  "input": {
    "parts": [
      {"type": "text", "text": "Dupont SA"}
    ]
  },
  "timeout_seconds": 60
}
```

`timeout_seconds` est optionnel (défaut : wall_clock du StepBudget de l'agent).

**Réponse 201 :**
```json
{
  "task_id": "t-abc123",
  "agent_id": "agent-def456",
  "status": "submitted"
}
```

**Erreurs :**
- `404` — agent introuvable
- `409` — agent en état non-acceptant (Stopping, Stopped)
- `503` — capacité de l'agent saturée (max_concurrent_tasks atteint)

### GET /api/v1/tasks/:id

Obtenir l'état et le résultat d'une tâche.

**Réponse 200 (en cours) :**
```json
{
  "task_id": "t-abc123",
  "agent_id": "agent-def456",
  "status": "working",
  "created_at": "2026-03-07T14:32:01Z",
  "steps": 3,
  "tool_calls": 1
}
```

**Réponse 200 (terminée) :**
```json
{
  "task_id": "t-abc123",
  "agent_id": "agent-def456",
  "status": "completed",
  "created_at": "2026-03-07T14:32:01Z",
  "completed_at": "2026-03-07T14:32:01Z",
  "duration_ms": 312,
  "output": [
    {"type": "text", "text": "Bonjour ! J'ai reçu : Dupont SA"}
  ]
}
```

**Réponse 200 (échouée) :**
```json
{
  "task_id": "t-abc123",
  "status": "failed",
  "error": {
    "code": "BUDGET_EXCEEDED",
    "message": "Step budget exhausted: 10/10 steps used"
  }
}
```

**Erreurs :**
- `404` — tâche introuvable

### DELETE /api/v1/tasks/:id

Annuler une tâche.

**Réponse 200 :**
```json
{
  "task_id": "t-abc123",
  "status": "canceled"
}
```

**Erreurs :**
- `404` — tâche introuvable
- `409` — tâche déjà terminée (completed, failed, canceled)

### GET /api/v1/tasks/:id/stream

Flux SSE temps réel des événements d'une tâche.

**Headers :** `Accept: text/event-stream`

**Événements :**
```
data: {"event":"TaskStarted","task_id":"t-abc123","agent_id":"agent-def456"}

data: {"event":"StepCompleted","task_id":"t-abc123","step":1}

data: {"event":"ToolCalled","task_id":"t-abc123","tool":"file_io","duration_ms":12}

data: {"event":"TaskCompleted","task_id":"t-abc123","status":"completed"}
```

Le flux se ferme après l'événement terminal (`TaskCompleted`, `TaskFailed`, `TaskCanceled`).

---

## LLM

### GET /api/v1/llm/status

État de tous les backends LLM configurés.

**Réponse 200 :**
```json
{
  "backends": [
    {
      "name":         "local",
      "model_id":     "llama3.2-3B-q4_K_M.gguf",
      "backend_type": "embedded",
      "is_local":     true
    },
    {
      "name":         "anthropic",
      "model_id":     "claude-haiku-4-5-20251001",
      "backend_type": "anthropic",
      "is_local":     false
    }
  ]
}
```

Si aucun `LlmRouter` n'est configuré : `{"backends": []}`.

---

### POST /api/v1/llm/ping

Mesurer la latence d'un backend LLM.

**Corps :**
```json
{
  "backend": "anthropic"   // optionnel — utilise le backend par défaut si absent
}
```

**Réponse 200 :**
```json
{
  "backend":    "anthropic",
  "available":  true,
  "latency_ms": 187,
  "error":      null
}
```

**Réponse 200 (backend indisponible — clé API absente, etc.) :**
```json
{
  "backend":    "anthropic",
  "available":  false,
  "latency_ms": null,
  "error":      "ANTHROPIC_API_KEY not set"
}
```

---

### POST /api/v1/llm/chat

Envoyer un prompt direct à un backend LLM et récupérer la réponse.

**Corps :**
```json
{
  "prompt":  "Résume les avantages du local-first en 3 points",
  "backend": "local"   // optionnel — backend par défaut si absent
}
```

**Réponse 200 :**
```json
{
  "content":    "1. Pas de latence réseau...",
  "usage": {
    "prompt_tokens":     12,
    "completion_tokens": 48,
    "cost_usd":          null
  },
  "latency_ms": 1243
}
```

**Réponse 503 :** Aucun backend LLM disponible.

---

## Shutdown

### POST /api/v1/shutdown

Initier un graceful shutdown du runtime (drain 30s).

**Corps :** aucun

**Réponse 200 :**
```json
{
  "status": "shutting_down",
  "drain_timeout_seconds": 30
}
```

---

## Codes d'erreur HTTP

| Code | Signification |
|---|---|
| `200` | Succès |
| `201` | Créé avec succès |
| `400` | Requête invalide (manifest, champs manquants) |
| `404` | Ressource introuvable |
| `409` | Conflit d'état (agent déjà démarré, tâche déjà terminée) |
| `422` | Erreur de traitement (fichier Python invalide) |
| `503` | Service indisponible (capacité saturée) |

**Format d'erreur standard :**
```json
{
  "error": "REASON_CODE",
  "message": "Description humaine de l'erreur"
}
```

---

## Utiliser l'API avec curl

```bash
# Health check
curl http://localhost:7771/api/v1/health

# Lister les agents
curl http://localhost:7771/api/v1/agents

# Démarrer un agent
curl -X POST http://localhost:7771/api/v1/agents \
  -H "Content-Type: application/json" \
  -d '{"path": "./agents/hello_agent.py"}'

# Soumettre une tâche
curl -X POST http://localhost:7771/api/v1/tasks \
  -H "Content-Type: application/json" \
  -d '{"agent_name": "hello-agent", "input": {"parts": [{"type": "text", "text": "test"}]}}'

# Résultat d'une tâche
curl http://localhost:7771/api/v1/tasks/t-abc123

# Streaming SSE
curl -N -H "Accept: text/event-stream" \
  http://localhost:7771/api/v1/tasks/t-abc123/stream
```

## Utiliser l'API sur Unix socket

```bash
# curl via Unix socket
curl --unix-socket /tmp/apollia.sock http://localhost/api/v1/health
```

---

## Triggers *(Sprint 9)*

### GET /api/v1/triggers

Lister tous les triggers avec leur statut.

**Réponse 200 :**
```json
{
  "triggers": [
    {
      "id": "rapport-hebdomadaire",
      "agent": "rapport-agent",
      "type": "cron",
      "enabled": true,
      "fire_count": 42,
      "skip_count": 3,
      "error_count": 0,
      "last_fired_at": "2026-03-08T08:00:00Z"
    }
  ]
}
```

### GET /api/v1/triggers/:id

Détail d'un trigger.

**Erreurs :** `404 Not Found` si trigger inconnu.

### POST /api/v1/triggers/:id/fire

Déclencher immédiatement un trigger (test ou opération manuelle).

**Réponse 200 :**
```json
{ "task_id": "t-abc123" }
```

### POST /api/v1/triggers/:id/enable | /disable

Activer ou désactiver un trigger sans modifier `apollia.toml`.

### GET /api/v1/triggers/:id/logs?last=N

Historique des fires/skips/erreurs depuis SQLite. `last` (défaut 50) limite les entrées.

**Réponse 200 :**
```json
{
  "entries": [
    {
      "status": "fired",
      "task_id": "t-abc123",
      "fired_at": "2026-03-09T08:00:00Z"
    },
    {
      "status": "skipped",
      "reason": "agent working (on_busy=drop)",
      "fired_at": "2026-03-08T08:00:00Z"
    }
  ]
}
```

### POST /api/v1/triggers/reload

Hot reload : relit `apollia.toml`, redémarre les sources modifiées sans arrêter le runtime.

**Réponse 200 :**
```json
{ "reloaded": 3, "added": 1, "removed": 0, "modified": 2 }
```

---

## Webhook *(Sprint 9)*

### POST /webhooks/:trigger_id

Endpoint d'ingestion webhook avec authentification HMAC-SHA256.

**Headers requis :**
```
X-Apollia-Signature: sha256=<hmac_sha256_hex>
Content-Type: application/json
```

**Corps :** JSON quelconque.

**Réponses :**

| Code | Condition |
|---|---|
| `200` | Signature valide, TriggerEvent envoyé |
| `401` | Signature invalide ou header manquant |
| `404` | trigger_id inconnu |
| `503` | TriggerEngine non démarré |

**Exemple :**
```bash
# Calculer la signature
BODY='{"ref":"refs/heads/main"}'
SIG=$(echo -n "$BODY" | openssl dgst -sha256 -hmac "un-secret-robuste" | cut -d' ' -f2)

curl -X POST http://localhost:7771/webhooks/github-push \
  -H "X-Apollia-Signature: sha256=$SIG" \
  -H "Content-Type: application/json" \
  -d "$BODY"
```

---

## Dashboard *(Sprint 9)*

### GET /dashboard

Retourne le dashboard HTML complet (HTMX embarqué, CSS inline).

### GET /api/v1/dashboard/state

Snapshot JSON complet de l'état du runtime (agents, tasks, triggers, LLM, outils).

### GET /api/v1/dashboard/partials/:section

Fragment HTML d'une section pour HTMX polling. Sections : `agents`, `tasks`, `triggers`, `tools`, `llm`, `audit`.

### GET /api/v1/dashboard/stream

SSE stream pour mises à jour temps réel. Événements nommés : `agents`, `tasks`, `triggers`, `llm`, `tools`.

```bash
curl -N -H "Accept: text/event-stream" \
  http://localhost:7771/api/v1/dashboard/stream
```

---

## Voir aussi

- [Briques CLI](./Briques-CLI) — wrapper CLI sur cette API
- [Briques Runtime Core](./Briques-Runtime-Core) — implémentation APIServer axum
- [Briques Triggers](./Briques-Triggers) — moteur de déclenchement
- [Dashboard Observabilité](./Dashboard-Observabilite) — dashboard embarqué
- [ADR-006](../adr/ADR-006-rest-json-api-locale) — pourquoi REST JSON plutôt qu'une autre API
- [ADR-017](../adr/ADR-017-hyper-util-unix-socket-serving) — Unix socket avec hyper-util
- [ADR-021](../adr/ADR-021-apollia-triggers-toml-hmac-hot-reload.md) — décisions TOML/HMAC/hot reload
