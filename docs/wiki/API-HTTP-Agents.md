# API HTTP - Agents, Tâches & LLM - Apollia OS

> Référence des endpoints REST liés aux **agents, tâches, LLM, outils, A2A, plan cache, sessions de chat, santé et shutdown**.
> Public cible : développeur intégrant Apollia OS dans un système externe.
>
> Cette page fait partie d'un découpage en trois :
> - **API-HTTP-Agents** (cette page) - agents, tasks, chat, LLM, tools, a2a, plan-cache, sessions, health, shutdown
> - [API-HTTP-Workspace](./API-HTTP-Workspace) - triggers, webhooks, notifications
> - [API-HTTP-Observability](./API-HTTP-Observability) - audit, timeline, approvals, user memory, dashboard, STT, MCP

---

## Vue d'ensemble

L'API HTTP locale est exposée sur deux transports :
- **Unix socket** : `/tmp/apollia.sock` - recommandé pour les processus locaux, **non authentifié** (accès par permissions filesystem)
- **TCP** : `http://localhost:7771` - compatible avec tout client HTTP, **authentification requise** (ADR-051)

Tous les endpoints retournent du JSON.

### Authentification TCP

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

> Le socket Unix reste non authentifié - les processus locaux sous le même UID (CLI, app desktop) l'utilisent sans token.

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
- `400` - manifest invalide ou outil requis introuvable
- `409` - agent avec ce nom déjà déployé
- `422` - fichier Python introuvable ou erreur de chargement

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
- `404` - agent introuvable

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
- `404` - agent introuvable
- `409` - agent déjà en `Stopping` ou `Stopped`

---

## Tâches

### GET /api/v1/tasks

Lister toutes les tâches connues du runtime.

**Query params :**
- `status` (optionnel) - filtre par statut exact (`submitted`, `working`, `completed`, `failed`, `canceled`, `input_required`)

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
- `404` - agent introuvable
- `409` - agent en état non-acceptant (Stopping, Stopped)
- `503` - capacité de l'agent saturée (max_concurrent_tasks atteint)

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
- `404` - tâche introuvable

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
- `404` - tâche introuvable
- `409` - tâche déjà terminée (completed, failed, canceled)

### POST /api/v1/tasks/:id/resume

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

Le champ `approved` est obligatoire - son absence provoque HTTP 422. Le champ `reason` est optionnel, surtout utile en cas de rejet.

**Réponse 200 :**
```json
{
  "task_id": "t-abc123",
  "approved": true,
  "status": "working"
}
```

Le champ `status` vaut toujours `"working"` que la décision soit une approbation ou un rejet - l'agent reprend l'exécution dans les deux cas.

**Erreurs :**
- `404` - tâche introuvable dans le système HITL
- `409` - tâche connue mais pas en status `input_required`
- `422` - corps de requête invalide (champ `approved` manquant)
- `500` - erreur SQLite ou echec de reconstruction de la tâche (`rebuild_for_resume`)
- `503` - HITL non configuré (`task_repository` absent)

### GET /api/v1/tasks/:id/stream

Flux SSE temps réel des événements d'une tâche.

**Headers :** `Accept: text/event-stream`

**Événements Mode Direct :**
```
data: {"event":"started","task_id":"t-abc123","agent_id":"agent-def456"}

data: {"event":"completed","task_id":"t-abc123","status":"completed","output":"..."}
```

**Événements Mode Orchestré :**
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

**Événements HITL :**
```
data: {"event":"input_required","task_id":"t-abc123","prompt":"Confirmer l'envoi ?","step_id":null}

data: {"event":"task_resumed","task_id":"t-abc123","approved":true}
```

`input_required` n'est **pas** un événement terminal - la tâche reste suspendue et attend une décision via `POST /api/v1/tasks/:id/resume`. Le flux reste ouvert. `task_resumed` est émis dès que la reprise est enregistrée ; la tâche repasse en `working`.

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
  "backend": "anthropic"   // optionnel - utilise le backend par défaut si absent
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

**Réponse 200 (backend indisponible - clé API absente, etc.) :**
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
  "backend": "local"   // optionnel - backend par défaut si absent
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
  "backend": "anthropic"   // optionnel - backend par défaut si absent
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
- `400` - rôle inconnu dans les messages
- `503` - aucun router LLM configuré ou backend indisponible

---

### GET /api/v1/llm/costs

Statistiques agrégées de coût et de tokens sur une fenêtre glissante.

**Query params :**
- `days` (optionnel, défaut: 7) - nombre de jours à agréger

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
- `days` (optionnel, défaut: 7) - profondeur historique

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

### GET /api/v1/llm/backends

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

### GET /api/v1/llm/backends/:name

Retourne un backend par nom exact.

**Réponse 200 :** objet `LlmBackendConfig`
**Réponse 404 :** backend introuvable

---

### POST /api/v1/llm/backends

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

### PUT /api/v1/llm/backends/:name

Met à jour un backend existant (upsert).

**Corps :** objet `LlmBackendConfig` complet
**Réponse 200 :** objet mis à jour
**Réponse 404 :** backend introuvable

---

### DELETE /api/v1/llm/backends/:name

Supprime un backend.

**Réponse 204 :** supprimé avec succès
**Réponse 404 :** backend introuvable
**Réponse 409 :** impossible de supprimer le backend par défaut (définir un autre défaut d'abord)

---

### POST /api/v1/llm/backends/:name/set-default

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

## Tools

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

## Agent Messaging

### GET /api/v1/agents/:name/messages

Liste les messages en file pour un agent. Max 200 messages par requête.

**Query params :**
- `limit` (optionnel, défaut: 50, max: 200) - nombre de messages à retourner

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

**Réponse 503 :** `{ "error": "Mailbox not configured" }` - si `AgentMailbox` n'est pas activé.

---

## A2A - Routing Agent-to-Agent

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
| `skill_id` | ✅ | - |
| `input` | ✅ | - |
| `timeout_secs` | - | 120 |

**Réponse 200 :**
```json
{
  "task_id":    "t-abc123",
  "agent_name": "excel-reader",
  "output":     "Trouvé 142 clients dans la feuille Clients."
}
```

**Erreurs :**
- `404` - `skill_id` introuvable, champ `available_skills` listé dans la réponse
- `409` - skill ambigu (plusieurs agents déclarent le même skill), champ `conflicting_agents` listé
- `504` - timeout dépassé
- `502` - Worker Agent a retourné une erreur

---

### POST /api/v1/a2a/invoke

Invocation haut niveau via l'`A2AInvoker` - applique les garde-fous (profondeur max, auto-invocation, timeout de chaîne).

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
| `skill_id` | ✅ | - |
| `input` | ✅ | - |
| `caller` | - | `"api"` |
| `timeout_secs` | - | 120 |

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
- `404` - skill introuvable
- `503` - agent non actif ou A2A invoker non initialisé
- `429` - profondeur max dépassée (`MAX_DEPTH`), auto-invocation, ou timeout de chaîne global dépassé
- `504` - timeout par invocation dépassé
- `502` - agent a retourné une erreur

---

## Plan Cache

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

## Chat

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
| `system_prompt` | `string \| null` | - | Prompt système personnalisé |
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

## Codes d'erreur HTTP

| Code | Signification |
|---|---|
| `200` | Succès |
| `201` | Créé avec succès (`POST /api/v1/agents`, `POST /api/v1/mcp/servers`) |
| `202` | Accepté (`POST /api/v1/tasks` - tâche soumise, exécution asynchrone) |
| `204` | Supprimé avec succès (`DELETE /api/v1/llm/backends/:name`, `DELETE /api/v1/user/memory/:key`) |
| `400` | Requête invalide (manifest, champs manquants, rôle LLM inconnu) |
| `401` | Non autorisé (signature HMAC invalide sur webhook) |
| `404` | Ressource introuvable |
| `409` | Conflit d'état (agent déjà démarré, tâche déjà terminée, skill A2A ambigu) |
| `422` | Erreur de traitement (fichier Python invalide, corps de requête invalide, catégorie mémoire inconnue) |
| `429` | Trop de requêtes (garde-fous A2A : profondeur max, auto-invocation, timeout de chaîne) |
| `500` | Erreur interne (SQLite, rebuild HITL) |
| `502` | Bad gateway - Worker Agent a retourné une erreur (`POST /api/v1/a2a/delegate|invoke`) |
| `503` | Service indisponible (capacité saturée, composant non configuré) |
| `504` | Gateway timeout - timeout A2A dépassé (`POST /api/v1/a2a/delegate|invoke`) |

**Statut `input_required` :** statut intermédiaire émis par ORIA en mode Direct quand l'agent requiert une validation humaine. La tâche est suspendue et attend une décision via `POST /api/v1/tasks/:id/resume`. Ce n'est pas un état terminal - le flux SSE reste ouvert.

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

## Voir aussi

- [API-HTTP-Workspace](./API-HTTP-Workspace) - triggers, webhooks, notifications
- [API-HTTP-Observability](./API-HTTP-Observability) - audit, timeline, approvals, user, dashboard, STT, MCP
- [Briques CLI](./Briques-CLI) - wrapper CLI sur cette API
- [Briques Runtime Core](./Briques-Runtime-Core) - implémentation APIServer axum
- [A2A-ACP-Alignement](./A2A-ACP-Alignement) - spécification des guards et de l'A2AInvoker
- [Briques Chat](./Briques-Chat) - sous-système de chat complet
- [ADR-006](../adr/ADR-006-rest-json-api-locale) - pourquoi REST JSON plutôt qu'une autre API
- [ADR-017](../adr/ADR-017-hyper-util-unix-socket-serving) - Unix socket avec hyper-util
- [ADR-034](../adr/ADR-034-chat-hybride-sessions-streaming-hitl-inline.md) - chat hybride : sessions, streaming, HITL inline
- [ADR-050](../adr/ADR-050) - distribution Worker Agents, registre communautaire
