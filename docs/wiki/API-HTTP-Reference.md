# API HTTP — Référence — Apollia OS

> Référence complète de l'API REST locale d'Apollia OS : endpoints, schémas de requête/réponse, codes d'erreur.
> Public cible : développeur intégrant Apollia OS dans un système externe

---

## Vue d'ensemble

L'API HTTP locale est exposée sur deux transports :
- **Unix socket** : `/tmp/apollia.sock` — recommandé pour les processus locaux, **non authentifié** (accès par permissions filesystem)
- **TCP** : `http://localhost:7771` — compatible avec tout client HTTP, **authentification requise** (Sprint 34 — ADR-051)

Tous les endpoints retournent du JSON.

### Authentification TCP (Sprint 34)

Toutes les requêtes TCP doivent porter le header `Authorization: Bearer <token>` :

```http
GET /api/v1/agents HTTP/1.1
Host: localhost:7771
Authorization: Bearer 4a3b2c1d...  (64 hex chars)
```

Le token est généré au premier démarrage et stocké dans `~/.apollia/api-token` (permissions `0600`). La CLI le lit automatiquement. Les requêtes sans token ou avec token invalide reçoivent `401 Unauthorized`.

```json
{"error": "missing Authorization header"}
{"error": "invalid token"}
```

Pour afficher le token : `apollia-os config show-token`. Pour le régénérer : `apollia-os config rotate-token`.

> Le socket Unix reste non authentifié — les processus locaux sous le même UID (CLI, app desktop) l'utilisent sans token.

**Base URL :** `http://localhost:7771/api/v1`

---

## Santé

### GET /api/v1/health

Vérifier que le runtime tourne.

**Réponse 200 :**
```json
{
  "status": "ok"
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
      "agent_id": "agent-abc123",
      "state": "active"
    }
  ]
}
```

### POST /api/v1/agents

Démarrer un agent à partir d'un fichier Python.

**Corps de requête :**
```json
{
  "agent_path": "./agents/hello_agent.py"
}
```

**Réponse 201 :**
```json
{
  "agent_id": "agent-abc123",
  "state": "active"
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
  "agent_id": "agent-abc123",
  "state": "active",
  "manifest": {
    "name": "hello-agent",
    "version": "1.0.0",
    "description": "Agent de démonstration minimal",
    "tools_required": [],
    "tools_optional": []
  }
}
```

**Erreurs :**
- `404` — agent introuvable

### DELETE /api/v1/agents/:id

Arrêter un agent (graceful drain).

**Réponse 200 :**
```json
{
  "agent_id": "agent-abc123",
  "state": "stopping"
}
```

**Erreurs :**
- `404` — agent introuvable
- `409` — agent déjà en `Stopping` ou `Stopped`

---

## Tâches

### GET /api/v1/tasks

Lister toutes les tâches connues du runtime.

**Query params :**
- `status` (optionnel) — filtre par statut exact (`submitted`, `working`, `completed`, `failed`, `canceled`, `input_required`)

**Réponse 200 :**
```json
{
  "tasks": [
    {
      "task_id": "t-abc123",
      "agent_id": "agent-def456",
      "status": "working"
    },
    {
      "task_id": "t-xyz789",
      "agent_id": "agent-def456",
      "status": "completed"
    }
  ]
}
```

Si aucune tâche ne correspond au filtre : `{ "tasks": [] }`.

---

### POST /api/v1/tasks

Soumettre une tâche à un agent.

**Corps de requête :**
```json
{
  "agent_id": "agent-abc123",
  "input": {
    "parts": [
      {"type": "text", "text": "Dupont SA"}
    ]
  }
}
```

**Réponse 202 :**
```json
{
  "task_id": "t-abc123",
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

### POST /api/v1/tasks/:id/resume *(Sprint 11)*

Reprendre une tâche suspendue en attente d'approbation humaine (HITL).

**Corps de requête :**
```json
{
  "approved": true,
  "reason": null
}
```
ou pour un rejet :
```json
{
  "approved": false,
  "reason": "Budget insuffisant"
}
```

Le champ `approved` est obligatoire — son absence provoque HTTP 422. Le champ `reason` est optionnel, surtout utile en cas de rejet.

**Réponse 200 :**
```json
{
  "task_id": "t-abc123",
  "approved": true,
  "status": "working"
}
```

Le champ `status` vaut toujours `"working"` que la décision soit une approbation ou un rejet — l'agent reprend l'exécution dans les deux cas.

**Erreurs :**
- `404` — tâche introuvable dans le système HITL
- `409` — tâche connue mais pas en status `input_required`
- `422` — corps de requête invalide (champ `approved` manquant)
- `500` — erreur SQLite ou echec de reconstruction de la tâche (`rebuild_for_resume`)
- `503` — HITL non configuré (`task_repository` absent)

### GET /api/v1/tasks/:id/stream

Flux SSE temps réel des événements d'une tâche.

**Headers :** `Accept: text/event-stream`

**Événements Mode Direct :**
```
data: {"event":"started","task_id":"t-abc123","agent_id":"agent-def456"}

data: {"event":"completed","task_id":"t-abc123","status":"completed","output":"..."}
```

**Événements Mode Orchestré (Sprint 10) :**
```
data: {"event":"plan_generated","task_id":"t-abc123","plan_id":"p-001","step_count":3,
       "steps":[{"step_id":"s1","description":"Lire le fichier","tool_hint":"file_io","depends_on":[]},
                {"step_id":"s2","description":"Analyser","tool_hint":null,"depends_on":["s1"]}]}

data: {"event":"step_started","task_id":"t-abc123","plan_id":"p-001",
       "step_id":"s1","num":1,"total":3,"desc":"Lire le fichier"}

data: {"event":"step_completed","task_id":"t-abc123","plan_id":"p-001",
       "step_id":"s1","duration_ms":120}

data: {"event":"step_failed","task_id":"t-abc123","plan_id":"p-001",
       "step_id":"s2","error":"file not found","retryable":true}

data: {"event":"plan_replanning","task_id":"t-abc123","plan_id":"p-001",
       "attempt":1,"failed_step":"s2","reason":"file not found"}

data: {"event":"plan_completed","task_id":"t-abc123","plan_id":"p-001",
       "step_count":3,"duration_ms":4100}

data: {"event":"plan_failed","task_id":"t-abc123","plan_id":"p-001",
       "reason":"MAX_REPLAN_EXCEEDED"}
```

**Événements HITL *(Sprint 11)* :**
```
data: {"event":"input_required","task_id":"t-abc123","prompt":"Confirmer l'envoi ?","step_id":null}

data: {"event":"task_resumed","task_id":"t-abc123","approved":true}
```

`input_required` n'est **pas** un événement terminal — la tâche reste suspendue et attend une décision via `POST /api/v1/tasks/:id/resume`. Le flux reste ouvert. `task_resumed` est émis dès que la reprise est enregistrée ; la tâche repasse en `working`.

**Événements terminaux :** `completed`, `failed`, `canceled`, `plan_failed`. Le flux se ferme après réception d'un événement terminal. `input_required` et `task_resumed` ne ferment pas le flux.

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

### POST /api/v1/llm/complete

Envoyer un historique de conversation multi-tours à un backend LLM.

**Corps :**
```json
{
  "messages": [
    { "role": "system",    "content": "Tu es un assistant technique Apollia." },
    { "role": "user",      "content": "Qu'est-ce que le StepBudget ?" },
    { "role": "assistant", "content": "Le StepBudget est un garde-fou appliqué par..." },
    { "role": "user",      "content": "Combien de steps par défaut ?" }
  ],
  "backend": "anthropic"   // optionnel — backend par défaut si absent
}
```

Rôles valides : `"system"`, `"user"`, `"assistant"`, `"tool"`.

**Réponse 200 :**
```json
{
  "content":    "Par défaut, le StepBudget est fixé à 10 steps...",
  "usage": {
    "prompt_tokens":     284,
    "completion_tokens": 52,
    "cost_usd":          0.000178
  },
  "latency_ms": 892
}
```

**Erreurs :**
- `400` — rôle inconnu dans les messages
- `503` — aucun router LLM configuré ou backend indisponible

---

### GET /api/v1/llm/costs

Statistiques agrégées de coût et de tokens sur une fenêtre glissante.

**Query params :**
- `days` (optionnel, défaut: 7) — nombre de jours à agréger

**Réponse 200 :**
```json
{
  "rows": [
    {
      "backend":        "anthropic",
      "model":          "claude-haiku-4-5-20251001",
      "call_count":     142,
      "total_tokens":   48320,
      "total_cost_usd": 0.0241
    },
    {
      "backend":        "local",
      "model":          "llama3.2-3B-q4_K_M.gguf",
      "call_count":     89,
      "total_tokens":   21400,
      "total_cost_usd": 0.0
    }
  ],
  "days": 7
}
```

**Réponse 503 :** `{ "error": "no LLM call repository configured" }`

---

### GET /api/v1/llm/costs/daily

Coûts LLM ventilés par jour et par backend. Utile pour générer un graphique d'évolution.

**Query params :**
- `days` (optionnel, défaut: 7) — profondeur historique

**Réponse 200 :**
```json
{
  "entries": [
    { "date": "2026-03-26", "backend": "anthropic", "cost_usd": 0.0038 },
    { "date": "2026-03-27", "backend": "anthropic", "cost_usd": 0.0021 },
    { "date": "2026-03-27", "backend": "local",     "cost_usd": 0.0    },
    { "date": "2026-03-28", "backend": "anthropic", "cost_usd": 0.0055 }
  ],
  "days": 7
}
```

**Réponse 503 :** `{ "error": "no LLM call repository configured" }`

---

### GET /api/v1/llm/backends *(Sprint 28)*

Liste tous les backends LLM enregistrés dans `system.db`.

**Réponse 200 :**
```json
[
  {
    "name":        "local-code",
    "provider":    "llama-cpp",
    "model":       "~/.apollia/models/qwen2.5-coder-7b-q4.gguf",
    "config_json": {},
    "enabled":     true,
    "is_default":  false
  },
  {
    "name":        "anthropic",
    "provider":    "anthropic",
    "model":       "claude-haiku-4-5-20251001",
    "config_json": { "api_key": "${ANTHROPIC_API_KEY}" },
    "enabled":     true,
    "is_default":  true
  }
]
```

---

### GET /api/v1/llm/backends/:name *(Sprint 28)*

Retourne un backend par nom exact.

**Réponse 200 :** objet `LlmBackendConfig`
**Réponse 404 :** backend introuvable

---

### POST /api/v1/llm/backends *(Sprint 28)*

Crée un nouveau backend LLM.

**Corps :**
```json
{
  "name":        "mistral-small",
  "provider":    "mistral",
  "model":       "mistral-small-latest",
  "config_json": { "api_key": "${MISTRAL_API_KEY}" },
  "enabled":     true,
  "is_default":  false
}
```

**Réponse 201 :** objet créé
**Réponse 400 :** nom invalide (doit correspondre à `[a-z0-9_-]+`) ou provider inconnu

---

### PUT /api/v1/llm/backends/:name *(Sprint 28)*

Met à jour un backend existant (upsert).

**Corps :** objet `LlmBackendConfig` complet
**Réponse 200 :** objet mis à jour
**Réponse 404 :** backend introuvable

---

### DELETE /api/v1/llm/backends/:name *(Sprint 28)*

Supprime un backend.

**Réponse 204 :** supprimé avec succès
**Réponse 404 :** backend introuvable
**Réponse 409 :** impossible de supprimer le backend par défaut (définir un autre défaut d'abord)

---

### POST /api/v1/llm/backends/:name/set-default *(Sprint 28)*

Marque le backend comme défaut. L'ancien défaut est démarcé automatiquement.

**Réponse 200 :** `{ "default": "mistral-small" }`
**Réponse 404 :** backend introuvable

---

## Shutdown

### POST /api/v1/shutdown

Initier un graceful shutdown du runtime (drain 30s).

**Corps :** aucun

**Réponse 200 :**
```json
{
  "status": "shutting_down"
}
```

---

## Tools *(Sprint 20)*

### GET /api/v1/tools

Liste tous les outils enregistrés dans le Tool Registry.

**Réponse 200 :**
```json
{
  "tools": [
    {
      "name": "bash_executor",
      "version": "0.1.0",
      "description": "Exécute des commandes shell dans un environment isolé",
      "kind": "Native",
      "input_schema": { "type": "object", "properties": { "command": { "type": "string" } } },
      "output_schema": null,
      "sandbox_profile": "FileSystem",
      "tags": [],
      "dangerous": false
    }
  ]
}
```

### GET /api/v1/tools/:name

Retourne le descripteur complet d'un outil.

**Réponse 200 :** `ToolDescriptor` complet (même format qu'un élément de la liste ci-dessus).

**Réponse 404 :** `{ "error": "Tool 'xxx' not found" }`

---

## Agent Messaging *(Sprint 20)*

### GET /api/v1/agents/:name/messages

Liste les messages en file pour un agent. Max 200 messages par requête.

**Query params :**
- `limit` (optionnel, défaut: 50, max: 200) — nombre de messages à retourner

**Réponse 200 :**
```json
{
  "messages": [
    {
      "from_agent": "agent-a",
      "to_agent": "agent-b",
      "payload": { "type": "data", "content": "résultat" },
      "sent_at": "2026-03-24T10:00:00Z"
    }
  ]
}
```

**Réponse 503 :** `{ "error": "Mailbox not configured" }` — si `AgentMailbox` n'est pas activé.

---

## A2A — Routing Agent-to-Agent *(Sprint 32)*

### GET /api/v1/a2a/agents

Liste tous les agents actifs qui déclarent `supports_a2a: true` avec leurs skills.

**Réponse 200 :**
```json
{
  "agents": [
    {
      "agent_id": "agent-abc123",
      "name":     "excel-reader",
      "version":  "1.0.0",
      "state":    "active",
      "skills": [
        {
          "id":           "read-excel",
          "name":         "Lecture Excel",
          "description":  "Lit et extrait des données depuis un fichier .xlsx",
          "input_modes":  ["text", "file"],
          "output_modes": ["data", "text"]
        }
      ]
    }
  ]
}
```

Seuls les agents en état `active` ou `degraded` apparaissent dans la liste.

---

### GET /api/v1/a2a/skills

Liste plate de tous les skills A2A disponibles sur tous les agents actifs.

**Réponse 200 :**
```json
{
  "skills": [
    {
      "skill_id":    "read-excel",
      "agent_name":  "excel-reader",
      "skill_name":  "Lecture Excel",
      "description": "Lit et extrait des données depuis un fichier .xlsx"
    },
    {
      "skill_id":    "send-email",
      "agent_name":  "email-agent",
      "skill_name":  "Envoi Email",
      "description": "Compose et envoie un email via SMTP"
    }
  ]
}
```

**Réponse 503 :** `{ "error": "A2A invoker not initialized" }`

---

### POST /api/v1/a2a/delegate

Délègue une tâche à un Worker Agent par `skill_id`. Soumet la tâche, attend la complétion (sync), retourne le résultat.

**Corps :**
```json
{
  "skill_id":     "read-excel",
  "input":        { "file_path": "/data/clients.xlsx", "sheet": "Clients" },
  "timeout_secs": 60
}
```

| Champ | Requis | Défaut |
|---|---|---|
| `skill_id` | ✅ | — |
| `input` | ✅ | — |
| `timeout_secs` | — | 120 |

**Réponse 200 :**
```json
{
  "task_id":    "t-abc123",
  "agent_name": "excel-reader",
  "output":     "Trouvé 142 clients dans la feuille Clients."
}
```

**Erreurs :**
- `404` — `skill_id` introuvable, champ `available_skills` listé dans la réponse
- `409` — skill ambigu (plusieurs agents déclarent le même skill), champ `conflicting_agents` listé
- `504` — timeout dépassé
- `502` — Worker Agent a retourné une erreur

---

### POST /api/v1/a2a/invoke

Invocation haut niveau via l'`A2AInvoker` — applique les garde-fous (profondeur max, auto-invocation, timeout de chaîne).

**Corps :**
```json
{
  "skill_id":     "send-email",
  "input":        { "to": "admin@acme.com", "subject": "Rapport", "body": "..." },
  "caller":       "director-agent",
  "timeout_secs": 30
}
```

| Champ | Requis | Défaut |
|---|---|---|
| `skill_id` | ✅ | — |
| `input` | ✅ | — |
| `caller` | — | `"api"` |
| `timeout_secs` | — | 120 |

**Réponse 200 :**
```json
{
  "result": {
    "status": "completed",
    "output": [{ "type": "text", "text": "Email envoyé." }]
  },
  "agent_name":  "email-agent",
  "skill_id":    "send-email",
  "duration_ms": 1842
}
```

**Erreurs :**
- `404` — skill introuvable
- `503` — agent non actif ou A2A invoker non initialisé
- `429` — profondeur max dépassée (`MAX_DEPTH`), auto-invocation, ou timeout de chaîne global dépassé
- `504` — timeout par invocation dépassé
- `502` — agent a retourné une erreur

---

## Plan Cache *(Sprint 20)*

### GET /api/v1/plan-cache/stats

Statistiques du cache de plans ORIA.

**Réponse 200 :**
```json
{
  "total_entries": 42,
  "cache_hits": 128,
  "hit_rate_pct": 75.3,
  "oldest_entry_at": "2026-03-17T08:00:00Z",
  "newest_entry_at": "2026-03-24T09:45:00Z"
}
```

**Réponse 503 :** `{ "error": "Plan cache not configured" }`

### POST /api/v1/plan-cache/clear

Purge toutes les entrées du cache de plans.

**Corps :** aucun

**Réponse 200 :**
```json
{
  "cleared_count": 42
}
```

---

## Audit Trail *(Sprint 13)*

### GET /api/v1/audit

Dernières invocations d'outils enregistrées dans l'audit trail. Utile pour debug et conformité.

**Query params :**
- `limit` (optionnel, défaut: 20, max: 500) — nombre d'événements à retourner

**Réponse 200 :**
```json
{
  "events": [
    {
      "id":              "01J9X...",
      "agent_id":        "agent-abc123",
      "task_id":         "t-def456",
      "tool_name":       "bash_executor",
      "input_hash":      "sha256:abcd1234",
      "sandbox_profile": "FileSystem",
      "started_at":      "2026-03-28T10:00:00Z",
      "duration_ms":     42,
      "exit_code":       0,
      "success":         true,
      "error_code":      null,
      "args_json":       "{\"command\":\"ls -la /tmp\"}",
      "stdout":          "total 8\n...",
      "stderr":          null
    }
  ],
  "count": 1
}
```

Les champs `args_json`, `stdout`, `stderr` sont omis si absents.

**Réponse 503 :** `{ "error": "audit trail not configured" }`

---

### GET /api/v1/audit/stats

Statistiques agrégées de l'audit trail (toute l'histoire, pas de fenêtre de temps).

**Réponse 200 :**
```json
{
  "total_events":  2847,
  "unique_tools":  8,
  "unique_agents": 4
}
```

**Réponse 503 :** `{ "error": "audit trail not configured" }`

---

## Observabilité *(Sprint 13)*

### GET /api/v1/tasks/:id/timeline

Chronologie complète et unifiée d'une tâche : transitions d'état, appels outils, appels LLM, steps ORIA, suspensions HITL. Tous les événements sont triés par timestamp croissant.

**Réponse 200 :**
```json
{
  "task_id": "t-abc123",
  "events": [
    {
      "type": "task_transition",
      "status": "submitted",
      "timestamp": "2026-03-10T10:01:32Z"
    },
    {
      "type": "task_transition",
      "status": "working",
      "timestamp": "2026-03-10T10:01:32Z"
    },
    {
      "type": "step_started",
      "step_id": "s1",
      "tool": "file_io",
      "input_preview": "Lire le fichier clients/dupont_sa.json...",
      "timestamp": "2026-03-10T10:01:33Z"
    },
    {
      "type": "tool_call",
      "tool_name": "file_io",
      "duration_ms": 12,
      "exit_code": 0,
      "truncated": false,
      "timestamp": "2026-03-10T10:01:33Z"
    },
    {
      "type": "step_completed",
      "step_id": "s1",
      "duration_ms": 120,
      "success": true,
      "timestamp": "2026-03-10T10:01:33Z"
    },
    {
      "type": "llm_call",
      "backend": "anthropic",
      "model": "claude-haiku-4-5-20251001",
      "prompt_tokens": 234,
      "completion_tokens": 89,
      "cost_usd": 0.000123,
      "latency_ms": 187,
      "timestamp": "2026-03-10T10:01:34Z"
    },
    {
      "type": "hitl_suspended",
      "prompt": "Confirmer l'envoi du devis ?",
      "timestamp": "2026-03-10T10:01:35Z"
    },
    {
      "type": "hitl_resolved",
      "approved": true,
      "reason": null,
      "wait_ms": 45000,
      "timestamp": "2026-03-10T10:02:20Z"
    },
    {
      "type": "task_completed",
      "output_preview": "Devis #042 généré pour Dupont SA, 5100€...",
      "duration_ms": 48200,
      "timestamp": "2026-03-10T10:02:20Z"
    }
  ]
}
```

**Sources de données agrégées :**
- `hitl.db` → transitions de tâche + approbations HITL
- `plans.db` → steps Mode Orchestré
- `llm_calls.db` → appels LLM avec tokens et coûts
- `audit.db` → appels outils avec args/stdout/stderr

L'agrégation est faite côté serveur dans un seul `spawn_blocking` (5 lectures parallèles + tri par timestamp).

**Modes supportés :**
- **Mode Direct** : transitions + tool_calls + task_completed
- **Mode Orchestré** : transitions + steps + tool_calls + llm_calls + task_completed
- **HITL** : ajoute hitl_suspended + hitl_resolved avec wait_ms

**Troncature des previews :** `input_preview` est limité à 200 caractères, `output_preview` à 500 caractères (avec `...` en suffixe).

**Erreurs :**
- `404` — tâche introuvable

---

## Approbations HITL *(Sprint 11)*

### GET /api/v1/approvals/pending

Liste toutes les tâches actuellement suspendues en attente d'une approbation humaine.

**Réponse 200 :**
```json
[
  {
    "task_id":      "t-abc123",
    "agent_name":   "director-agent",
    "prompt":       "Confirmer l'envoi du devis à Dupont SA (5 100 €) ?",
    "context":      { "client": "Dupont SA", "amount_eur": 5100 },
    "suspended_at": "2026-03-28T14:32:01Z"
  }
]
```

Retourne `[]` si aucune tâche n'est en attente ou si HITL n'est pas configuré.

---

### GET /api/v1/approvals/resolved

Historique des approbations résolues (approuvées ou rejetées).

**Query params :**
- `limit` (optionnel, défaut: 20) — nombre d'entrées
- `days` (optionnel, défaut: 7) — fenêtre temporelle en jours

**Réponse 200 :**
```json
[
  {
    "task_id":          "t-xyz789",
    "agent_name":       "director-agent",
    "approved":         true,
    "reason":           null,
    "wait_duration_ms": 45000,
    "responded_at":     "2026-03-28T14:33:26Z"
  },
  {
    "task_id":          "t-mno456",
    "agent_name":       "director-agent",
    "approved":         false,
    "reason":           "Budget insuffisant",
    "wait_duration_ms": 12000,
    "responded_at":     "2026-03-27T09:10:05Z"
  }
]
```

Retourne `[]` si aucune approbation ou si TaskRepository n'est pas configuré.

---

## Profil Utilisateur *(Sprint 18)*

### GET /api/v1/user/profile

Retourne le profil utilisateur agrégé depuis les trois catégories de mémoire.

**Réponse 200 :**
```json
{
  "name": "Nidal",
  "preferences": {
    "language":       "fr",
    "output_format":  "markdown",
    "tone":           "direct"
  },
  "habits": {
    "working_hours":  "soir 20h-23h",
    "review_freq":    "quotidien"
  },
  "context": {
    "project":        "Apollia OS",
    "role":           "CTO",
    "team":           "Apollia"
  }
}
```

**Réponse 503 :** `{ "error": "user memory not configured" }`

---

### PUT /api/v1/user/profile

Met à jour le profil utilisateur (upsert, fusion par catégorie — les champs absents ne sont pas supprimés).

**Corps :**
```json
{
  "name": "Nidal",
  "preferences": {
    "language": "fr",
    "tone":     "concis"
  },
  "habits": {
    "working_hours": "soir 20h-23h"
  },
  "context": {
    "project": "Apollia OS v2"
  }
}
```

Tous les champs sont optionnels. Seules les catégories fournies sont fusionnées.

**Réponse 200 :** corps vide (succès silencieux).

**Réponse 503 :** `{ "error": "user memory not configured" }`

---

### GET /api/v1/user/memory

Liste les entrées de mémoire utilisateur brutes avec filtres optionnels.

**Query params :**
- `category` (optionnel) — `preferences`, `habits`, ou `context`
- `limit` (optionnel, défaut: 100) — nombre maximum d'entrées

**Réponse 200 :**
```json
{
  "entries": [
    {
      "key":        "language",
      "value":      "fr",
      "source":     "user_explicit",
      "updated_at": "2026-03-20T10:30:00Z"
    },
    {
      "key":        "working_hours",
      "value":      "soir 20h-23h",
      "source":     "agent_inferred",
      "updated_at": "2026-03-22T21:00:00Z"
    }
  ]
}
```

Sources possibles : `user_explicit`, `agent_inferred`.

**Erreurs :**
- `422` — valeur `category` invalide (n'est pas `preferences`, `habits`, ou `context`)
- `503` — mémoire utilisateur non configurée

---

### DELETE /api/v1/user/memory/:key

Supprime une entrée de mémoire par clé (toutes les catégories sont scrutées).

**Réponse 204 :** suppression réussie.

**Erreurs :**
- `404` — clé introuvable dans aucune catégorie
- `503` — mémoire utilisateur non configurée

---

## Codes d'erreur HTTP

| Code | Signification |
|---|---|
| `200` | Succès |
| `201` | Créé avec succès (`POST /api/v1/agents`, `POST /api/v1/mcp/servers`) |
| `202` | Accepté (`POST /api/v1/tasks` — tâche soumise, exécution asynchrone) |
| `204` | Supprimé avec succès (`DELETE /api/v1/llm/backends/:name`, `DELETE /api/v1/user/memory/:key`) |
| `400` | Requête invalide (manifest, champs manquants, rôle LLM inconnu) |
| `401` | Non autorisé (signature HMAC invalide sur webhook) |
| `404` | Ressource introuvable |
| `409` | Conflit d'état (agent déjà démarré, tâche déjà terminée, skill A2A ambigu) |
| `422` | Erreur de traitement (fichier Python invalide, corps de requête invalide, catégorie mémoire inconnue) |
| `429` | Trop de requêtes (garde-fous A2A : profondeur max, auto-invocation, timeout de chaîne) |
| `500` | Erreur interne (SQLite, rebuild HITL) |
| `502` | Bad gateway — Worker Agent a retourné une erreur (`POST /api/v1/a2a/delegate|invoke`) |
| `503` | Service indisponible (capacité saturée, composant non configuré) |
| `504` | Gateway timeout — timeout A2A dépassé (`POST /api/v1/a2a/delegate|invoke`) |

**Statut `input_required` :** statut intermédiaire émis par ORIA en mode Direct quand l'agent requiert une validation humaine. La tâche est suspendue et attend une décision via `POST /api/v1/tasks/:id/resume`. Ce n'est pas un état terminal — le flux SSE reste ouvert.

**Format d'erreur standard :**
```json
{
  "error": "description humaine de l'erreur"
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
  -d '{"agent_path": "./agents/hello_agent.py"}'

# Soumettre une tâche
curl -X POST http://localhost:7771/api/v1/tasks \
  -H "Content-Type: application/json" \
  -d '{"agent_id": "agent-abc123", "input": {"parts": [{"type": "text", "text": "test"}]}}'

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

## Triggers *(Sprint 9, CRUD Sprint 17)*

### POST /api/v1/triggers *(Sprint 17)*

Créer un nouveau trigger.

**Corps de requête :**
```json
{
  "id": "rapport-hebdomadaire",
  "agent": "rapport-agent",
  "enabled": true,
  "on_busy": "queue",
  "source": { "type": "cron", "schedule": "0 8 * * MON" },
  "input_template": "Génère le rapport de la semaine"
}
```

**Réponse 201 :** définition complète avec `created_at`.

**Erreurs :** `409` DuplicateId, `422` ValidationError.

### PUT /api/v1/triggers/:id *(Sprint 17)*

Modifier un trigger existant. Tous les champs sont modifiables.

**Réponse 200 :** définition mise à jour avec `updated_at`.

**Erreurs :** `404` NotFound, `422` ValidationError.

### DELETE /api/v1/triggers/:id *(Sprint 17)*

Supprimer un trigger.

**Réponse 200 :** `{ "deleted": true }`

**Erreurs :** `404` NotFound.

### GET /api/v1/triggers/:id *(Sprint 17)*

Lire la définition complète d'un trigger.

**Réponse 200 :** définition complète (id, agent/pipeline, source, enabled, on_busy, input_template, created_at, updated_at).

**Erreurs :** `404` NotFound.

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

## Notifications *(Sprint 11, CRUD Sprint 17)*

### POST /api/v1/notifications/channels *(Sprint 17)*

Créer un canal de notification.

**Corps de requête :**
```json
{
  "id": "slack-erreurs",
  "channel_type": "webhook",
  "enabled": true,
  "config": { "url": "https://hooks.slack.com/services/..." },
  "events": ["task.failed", "agent.degraded"]
}
```

**Réponse 201 :** canal complet avec timestamps.

**Erreurs :** `409` DuplicateId, `422` ValidationError (type inconnu, webhook sans URL).

### PUT /api/v1/notifications/channels/:id *(Sprint 17)*

Modifier un canal existant. Champs optionnels.

**Réponse 200 :** canal mis à jour avec `updated_at`.

**Erreurs :** `404` NotFound, `422` ValidationError.

### DELETE /api/v1/notifications/channels/:id *(Sprint 17)*

Supprimer un canal.

**Réponse 200 :** `{ "deleted": true }`

**Erreurs :** `404` NotFound.

### GET /api/v1/notifications/events *(Sprint 17)*

Lire les événements globaux configurés.

**Réponse 200 :**
```json
{ "events": ["task.input_required", "task.failed", "agent.degraded"] }
```

### PUT /api/v1/notifications/events *(Sprint 17)*

Définir les événements globaux (remplacement atomique via transaction SQLite).

**Corps de requête :**
```json
{ "events": ["task.input_required", "task.failed"] }
```

**Réponse 200 :** `{ "events": [...] }`

### GET /api/v1/notifications/channels

État de tous les canaux de notification configurés.

**Réponse 200 :**
```json
{
  "channels": [
    {
      "channel_id": "desktop",
      "type": "desktop",
      "enabled": true,
      "events": ["task.input_required", "task.completed", "task.failed"]
    },
    {
      "channel_id": "slack-webhook",
      "type": "webhook",
      "enabled": true,
      "events": ["task.input_required"]
    }
  ]
}
```

Si aucune section `[notifications]` n'est configurée dans `apollia.toml` : `{"channels": []}`.

Les canaux de type `"sse"` apparaissent également dans cette liste. Le champ `events` liste les événements que le canal accepte (hérité de la config globale `events` si non surchargé au niveau du canal).

### POST /api/v1/notifications/channels/:id/test

Envoyer une notification de test (`"test.ping"`) à un canal spécifique.

**Corps :** aucun

**Réponse 200 :**
```json
{
  "results": [
    {
      "channel_id": "slack-erreurs",
      "type":       "webhook",
      "status":     "ok",
      "error":      null,
      "latency_ms": 187
    }
  ]
}
```

**Réponse 404 :** canal introuvable.

---

### POST /api/v1/notifications/test

Envoyer une notification de test (`"test.ping"`) à tous les canaux actifs.

**Corps :** aucun

**Réponse 200 :**
```json
{
  "results": [
    {
      "channel_id": "desktop",
      "type": "desktop",
      "status": "ok",
      "error": null,
      "latency_ms": 12
    },
    {
      "channel_id": "slack-webhook",
      "type": "webhook",
      "status": "error",
      "error": "connection refused",
      "latency_ms": 5001
    },
    {
      "channel_id": "monitoring",
      "type": "webhook",
      "status": "disabled",
      "error": null,
      "latency_ms": null
    }
  ]
}
```

Les canaux désactivés (`enabled: false`) apparaissent avec `status: "disabled"` sans tentative d'envoi. Les canaux actifs ont `status: "ok"` ou `status: "error"` avec la latence mesurée.

### GET /api/v1/notifications/logs?last=N

Historique des N dernières notifications envoyées depuis SQLite (`~/.apollia/hitl.db`). Défaut `N=20`, maximum `N=1000`.

**Réponse 200 :**
```json
{
  "entries": [
    {
      "id": "01J9X...",
      "event_name": "task.input_required",
      "task_id": "t-abc123",
      "agent_id": "agent-def456",
      "sent_at": "2026-03-09T14:32:01Z",
      "channels": {"desktop": "ok"},
      "error": null
    }
  ]
}
```

La table `notification_logs` est créée de manière idempotente si elle n'existe pas encore. Les entrées sont triées par `sent_at` décroissant (la plus récente en premier).

---

## Pipelines *(Sprint 12, CRUD Sprint 17)*

### POST /api/v1/pipelines *(Sprint 17)*

Créer un pipeline.

**Corps de requête :**
```json
{
  "id": "traitement-facture",
  "description": "OCR → validation → comptabilisation",
  "on_failure": "fail",
  "enabled": true,
  "steps": [
    { "id": "ocr", "agent": "ocr-agent", "input": "{{trigger.payload}}" },
    { "id": "validation", "agent": "validation-agent",
      "input": "{{steps.ocr.output}}", "depends_on": ["ocr"] }
  ]
}
```

**Réponse 201 :** définition complète avec timestamps.

**Erreurs :** `409` DuplicateId, `422` ValidationError (cycle DAG, step ID dupliqué, depends_on invalide).

### PUT /api/v1/pipelines/:id *(Sprint 17)*

Modifier un pipeline existant (re-valide le DAG avant écriture).

**Réponse 200 :** définition mise à jour.

**Erreurs :** `404` NotFound, `422` ValidationError.

### DELETE /api/v1/pipelines/:id *(Sprint 17)*

Supprimer un pipeline.

**Réponse 200 :** `{ "deleted": true }`

**Erreurs :** `404` NotFound.

### GET /api/v1/pipelines/:id *(Sprint 17)*

Lire la définition complète d'un pipeline (steps inclus en JSON).

**Réponse 200 :** définition complète avec steps, on_failure, timestamps.

**Erreurs :** `404` NotFound.

### GET /api/v1/pipelines

Liste tous les pipelines.

**Réponse 200 :**
```json
{
  "pipelines": [
    {
      "id": "traitement-facture",
      "description": "OCR → validation → comptabilisation → archivage",
      "step_count": 4
    },
    {
      "id": "rapport-hebdomadaire",
      "description": "Génération automatique du rapport PME",
      "step_count": 2
    }
  ]
}
```

### POST /api/v1/pipelines/{id}/run

Démarre un nouveau run pour le pipeline `{id}`.

**Corps (optionnel) :**
```json
{
  "input": "facture-acme-2026-03.pdf",
  "trigger_id": null
}
```

**Réponse 200 :**
```json
{
  "run_id": "r-3f7a2b9c",
  "pipeline_id": "traitement-facture",
  "status": { "type": "running" },
  "started_at": "2026-03-10T10:01:32Z"
}
```

**Réponse 404 — pipeline inconnu :**
```json
{ "error": "pipeline not found: traitement-facture-typo" }
```

### GET /api/v1/pipelines/{id}/runs

Historique des runs du pipeline `{id}`. Paramètre optionnel : `?limit=20` (défaut 20, max 100).

**Réponse 200 :**
```json
{
  "runs": [
    {
      "run_id": "r-3f7a2b9c",
      "pipeline_id": "traitement-facture",
      "status": { "type": "completed" },
      "trigger_payload": "facture-acme.pdf",
      "started_at": "2026-03-10T10:01:32Z",
      "ended_at": "2026-03-10T10:02:55Z"
    }
  ]
}
```

### GET /api/v1/pipelines/{id}/runs/{run_id}

État détaillé d'un run incluant le statut par step.

**Réponse 200 :**
```json
{
  "run_id": "r-3f7a2b9c",
  "pipeline_id": "traitement-facture",
  "status": { "type": "completed" },
  "trigger_payload": "facture-acme.pdf",
  "started_at": "2026-03-10T10:01:32Z",
  "ended_at": "2026-03-10T10:02:55Z",
  "step_runs": {
    "ocr": {
      "step_id": "ocr",
      "task_id": "t-0021",
      "status": "completed",
      "output": "Facture ACME Corp — 12 500€ — 2026-03-01",
      "error": null,
      "started_at": "2026-03-10T10:01:32Z",
      "ended_at": "2026-03-10T10:01:45Z"
    },
    "validation": {
      "step_id": "validation",
      "task_id": "t-0022",
      "status": "completed",
      "output": "VALIDE",
      "error": null,
      "started_at": "2026-03-10T10:01:45Z",
      "ended_at": "2026-03-10T10:01:47Z"
    }
  }
}
```

**Statuts de step possibles :** `pending` / `running` / `waiting_approval` / `completed` / `failed` / `skipped` / `fallback_active`

**Statuts de run possibles :**
```json
{ "type": "running" }
{ "type": "waiting_approval", "step_id": "comptabilite", "task_id": "t-0023" }
{ "type": "completed" }
{ "type": "failed", "step_id": "validation", "reason": "timeout après 30s" }
```

**Réponse 404 :** `{ "error": "run not found: r-inexistant" }`

### GET /api/v1/runs/:run_id

Raccourci pour obtenir l'état détaillé d'un run sans connaître le `pipeline_id`. Équivalent fonctionnel de `GET /api/v1/pipelines/{pipeline_id}/runs/{run_id}`.

**Réponse 200 :** identique à `GET /api/v1/pipelines/{id}/runs/{run_id}` ci-dessus.

**Réponse 404 :** `{ "error": "run not found: r-inexistant" }`

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

## Chat *(Sprint 18)*

7 endpoints pour la gestion des sessions de chat interactif. Chemin d'exécution séparé du TaskRouter (ADR-034).

### Créer une session

```
POST /api/v1/sessions
```

**Body :**
```json
{
  "mode": "libre",
  "agent_name": null,
  "system_prompt": "Tu es un assistant technique.",
  "tools": ["bash_executor", "file_io"]
}
```

| Champ | Type | Requis | Description |
|---|---|---|---|
| `mode` | `"libre"` \| `"agent"` | ✅ | Mode de chat |
| `agent_name` | `string \| null` | Agent mode | Nom de l'agent installé |
| `system_prompt` | `string \| null` | — | Prompt système personnalisé |
| `tools` | `string[]` | Libre mode | Outils disponibles |

**Réponse (201) :**
```json
{
  "id": "chat_abc123",
  "mode": "libre",
  "status": "active",
  "created_at": "2026-03-20T10:30:00Z"
}
```

### Lister les sessions

```
GET /api/v1/sessions
```

**Réponse (200) :**
```json
[
  {
    "id": "chat_abc123",
    "mode": "libre",
    "agent_name": null,
    "status": "active",
    "message_count": 5,
    "last_message_preview": "Analyse le fichier...",
    "created_at": "2026-03-20T10:30:00Z"
  }
]
```

### Détail d'une session

```
GET /api/v1/sessions/:id
```

**Réponse (200) :** session complète avec `messages`, `authorized_tools`, `available_tools`.

### Fermer une session

```
DELETE /api/v1/sessions/:id
```

**Réponse (200) :** `{ "status": "closed" }`

### Envoyer un message

```
POST /api/v1/sessions/:id/messages
```

**Body :**
```json
{
  "content": "Liste les fichiers du répertoire courant"
}
```

**Réponse (202) :**
```json
{
  "message_id": "msg_xyz789",
  "status": "processing"
}
```

La réponse de l'assistant arrive via SSE (voir stream ci-dessous).

### Résoudre une approbation d'outil

```
POST /api/v1/sessions/:id/authorize
```

**Body :**
```json
{
  "message_id": "msg_xyz789",
  "tool_name": "bash_executor",
  "decision": "accept"
}
```

| Valeur `decision` | Comportement |
|---|---|
| `"accept"` | Exécuter l'outil une fois |
| `"refuse"` | Refuser, injecter message de refus |
| `"always_accept"` | Ajouter à la whitelist session, exécuter |

**Réponse (200) :** `{ "resolved": true }`

### SSE stream d'une session

```
GET /api/v1/sessions/:id/stream
```

Événements SSE nommés :

| Événement | Données | Fréquence |
|---|---|---|
| `message_sent` | `{ message_id }` | 1 par message utilisateur |
| `response_started` | `{ message_id }` | 1 par réponse assistant |
| `token` | `{ message_id, token }` | 1 par token LLM (Chat Libre) |
| `response_completed` | `{ message_id, content }` | 1 par réponse complète |
| `tool_call_started` | `{ tool_name }` | Par appel d'outil |
| `tool_call_completed` | `{ tool_name, success }` | Par appel d'outil |
| `approval_required` | `{ message_id, tool_name }` | Quand HITL requis |
| `approval_resolved` | `{ tool_name, decision }` | Après décision utilisateur |
| `error` | `{ error }` | En cas d'erreur |

```bash
curl -N -H "Accept: text/event-stream" \
  http://localhost:7771/api/v1/sessions/chat_abc123/stream
```

---

## STT — Speech-to-Text *(Sprint 24 + 28)*

7 endpoints pour la transcription audio locale et la gestion de la configuration STT. Les endpoints de transcription retournent `503` si le moteur STT est absent (`stt.enabled = false` ou modèle non chargé).

### GET /api/v1/stt/status

Statut du moteur STT.

**Réponse 200 :**
```json
{
  "enabled": true,
  "model_loaded": true,
  "model_path": "~/.apollia/models/whisper-large-v3-fr-q5_0.bin",
  "model_name": "whisper-large-v3-fr-q5_0",
  "backend_name": "whisper-cpp",
  "metal_enabled": true,
  "cuda_enabled": false
}
```

**Réponse 503 :** `{ "error": "STT engine not available" }`

### POST /api/v1/stt/transcribe

Transcrire un fichier audio envoyé en multipart.

**Corps :** `multipart/form-data` avec champ `file` (formats : WAV, MP3).

**Réponse 200 :**
```json
{
  "id": "a1b2c3d4e5f6...",
  "full_text": "Bonjour, je voudrais un devis.",
  "language": "fr",
  "source": "api",
  "audio_duration_ms": 3200,
  "processing_time_ms": 1100,
  "model_name": "whisper-large-v3-fr-q5_0",
  "created_at": "2026-03-25T14:30:00Z"
}
```

**Erreurs :**
- `400` — format audio non supporté ou fichier vide
- `503` — moteur STT absent

### GET /api/v1/stt/transcriptions

Historique des transcriptions.

**Query params :**
- `limit` (optionnel, défaut: 50) — nombre de résultats
- `offset` (optionnel, défaut: 0) — pagination

**Réponse 200 :**
```json
{
  "transcriptions": [
    {
      "id": "a1b2c3d4e5f6...",
      "full_text": "Bonjour, je voudrais un devis.",
      "language": "fr",
      "source": "hotkey",
      "audio_duration_ms": 3200,
      "processing_time_ms": 1100,
      "model_name": "whisper-large-v3-fr-q5_0",
      "created_at": "2026-03-25T14:30:00Z"
    }
  ]
}
```

### DELETE /api/v1/stt/transcriptions/:id

Supprimer une transcription.

**Réponse 204 :** suppression réussie.

**Erreurs :** `503` — moteur STT absent.

### GET /api/v1/stt/models

Lister les fichiers modèles `.bin` disponibles dans `~/.apollia/models/`.

**Réponse 200 :**
```json
{
  "models": [
    {
      "name": "whisper-large-v3-fr-q5_0",
      "path": "/Users/nidal/.apollia/models/whisper-large-v3-fr-q5_0.bin",
      "size_mb": 956.2
    }
  ]
}
```

### GET /api/v1/stt/config *(Sprint 28)*

Retourne la configuration STT persistée dans `system.db`. Si la table est vide (premier boot), les valeurs par défaut sont insérées et retournées.

**Réponse 200 :**
```json
{
  "enabled": true,
  "model_path": "~/.apollia/models/whisper-large-v3-fr-q5_0.bin",
  "hotkey": "ctrl+shift+space",
  "clipboard_mode": "paste",
  "clipboard_restore": true,
  "silence_threshold_db": -40.0,
  "max_recording_sec": 60,
  "language": "fr",
  "trigger_mode": "toggle"
}
```

**Réponse 503 :** `{ "error": "STT config repository not available" }`

---

### PUT /api/v1/stt/config *(Sprint 28)*

Met à jour la configuration STT (upsert). Remplace le singleton en base.

**Corps :** objet `SttConfigRow` complet (les champs avec valeurs par défaut peuvent être omis)

**Réponse 200 :** configuration mise à jour
**Réponse 503 :** dépôt non disponible

---

## MCP *(Sprint 26, ADR-044)*

### GET /api/v1/mcp/servers

Retourne la liste des serveurs MCP configurés et leur statut de connexion.

```bash
$ curl http://127.0.0.1:7771/api/v1/mcp/servers
```

```json
[
  {
    "name": "notion",
    "server_info": "notion-mcp-server 1.0.0",
    "tools_count": 8,
    "requires_approval": false,
    "connected": true,
    "pid": 12345,
    "uptime_secs": 3600,
    "last_call_at": "2026-06-15T10:30:00Z",
    "error": null,
    "package": "@notionhq/notion-mcp-server",
    "transport": "stdio"
  }
]
```

### GET /api/v1/mcp/servers/:name

Retourne le statut détaillé d'un serveur : configuration (secrets redactés) et liste des outils découverts.

```bash
$ curl http://127.0.0.1:7771/api/v1/mcp/servers/notion
```

### POST /api/v1/mcp/servers

Ajoute un nouveau serveur MCP à chaud : démarre le subprocess, effectue le handshake, enregistre les outils dans le Tool Registry, et persiste dans `mcp.db` via `McpServerRepository`.

```bash
$ curl -X POST http://127.0.0.1:7771/api/v1/mcp/servers \
  -H "Content-Type: application/json" \
  -d '{
    "name": "sqlite",
    "command": "uvx",
    "args": ["mcp-server-sqlite", "--db-path", "/home/user/data.db"],
    "transport": "stdio",
    "requires_approval": false
  }'
```

Retourne `201 Created` avec le `McpServerStatus` du serveur démarré.

### DELETE /api/v1/mcp/servers/:name

Arrête la session du serveur, retire ses outils du Tool Registry, et supprime l'entrée de `mcp.toml`.

```bash
$ curl -X DELETE http://127.0.0.1:7771/api/v1/mcp/servers/sqlite
```

### POST /api/v1/mcp/servers/:name/restart

Arrête et redémarre la session d'un serveur existant avec la configuration actuelle.

```bash
$ curl -X POST http://127.0.0.1:7771/api/v1/mcp/servers/notion/restart
```

### PUT /api/v1/mcp/servers/:name/config

Remplace la configuration d'un serveur et redémarre automatiquement la session. L'ordre des serveurs dans `mcp.toml` est préservé.

```bash
$ curl -X PUT http://127.0.0.1:7771/api/v1/mcp/servers/notion/config \
  -H "Content-Type: application/json" \
  -d '{
    "command": "npx",
    "args": ["-y", "@notionhq/notion-mcp-server"],
    "init_timeout_secs": 60
  }'
```

### POST /api/v1/mcp/servers/test

Effectue un handshake éphémère avec la configuration fournie, liste les outils, puis arrête le processus. Le Tool Registry n'est pas modifié. Utile pour valider une configuration avant de l'ajouter.

```bash
$ curl -X POST http://127.0.0.1:7771/api/v1/mcp/servers/test \
  -H "Content-Type: application/json" \
  -d '{
    "name": "brave-search",
    "command": "npx",
    "args": ["-y", "@modelcontextprotocol/server-brave-search"],
    "transport": "stdio",
    "env": {"BRAVE_API_KEY": "${BRAVE_API_KEY}"}
  }'
```

Retourne `{ "tools": [...], "server_info": "..." }` ou une erreur si le handshake échoue.

### PATCH /api/v1/mcp/servers/:name/approval

Met à jour le flag `requires_approval` d'un serveur MCP sans redémarrer la session. Le nouveau flag prend effet au prochain appel d'outil.

**Corps :**
```json
{
  "requires_approval": true
}
```

**Réponse 200 :** `McpServerStatus` mis à jour (même format que `GET /api/v1/mcp/servers`).

**Erreurs :**
- `404` — serveur introuvable
- `503` — MCP non configuré

---

## Voir aussi

- [Briques CLI](./Briques-CLI) — wrapper CLI sur cette API
- [Briques Runtime Core](./Briques-Runtime-Core) — implémentation APIServer axum
- [Briques Triggers](./Briques-Triggers) — moteur de déclenchement
- [Briques Notifications](./Briques-Notifications) — canaux de notification et moteur HITL
- [Dashboard Observabilité](./Dashboard-Observabilite) — dashboard embarqué
- [A2A-ACP-Alignement](./A2A-ACP-Alignement) — spécification des guards et de l'A2AInvoker
- [ADR-006](../adr/ADR-006-rest-json-api-locale) — pourquoi REST JSON plutôt qu'une autre API
- [ADR-017](../adr/ADR-017-hyper-util-unix-socket-serving) — Unix socket avec hyper-util
- [ADR-021](../adr/ADR-021-apollia-triggers-toml-hmac-hot-reload.md) — décisions TOML/HMAC/hot reload
- [ADR-023](../adr/ADR-023) — décisions architecture HITL (TaskRepository, PendingApprovals)
- [ADR-024](../adr/ADR-024) — décisions système de notifications (canaux, événements, SQLite)
- [ADR-025](../adr/ADR-025) — décisions pipelines multi-agents (TOML déclaratif, topologies natives)
- [ADR-026](../adr/ADR-026-observabilite-complete-persistance-timeline-troncature) — observabilité complète, timeline, troncature
- [ADR-033](../adr/ADR-033-config-operateur-sqlite.md) — config opérateur SQLite (CRUD triggers/pipelines/notifications)
- [ADR-034](../adr/ADR-034-chat-hybride-sessions-streaming-hitl-inline.md) — chat hybride : sessions, streaming, HITL inline
- [ADR-050](../adr/ADR-050) — distribution Worker Agents, registre communautaire
- [Briques Chat](./Briques-Chat) — sous-système de chat complet
- [Briques STT](./Briques-STT) — moteur Speech-to-Text embarqué (Sprint 24)
- [ADR-041](../adr/ADR-041-moteur-stt-embarque-whisper-rs-trait-stt-backend.md) — décisions moteur STT (whisper-rs, trait SttBackend)
- [MCP — Guide utilisateur](./MCP-Guide-Utilisateur) — configuration mcp.toml, exemples serveurs MCP
- [Briques MCP](./Briques-MCP) — spécification crate apollia-mcp (Sprint 26)
- [ADR-044](../adr/ADR-044-client-mcp.md) — décisions client MCP (transport stdio, McpClientManager, HITL)
