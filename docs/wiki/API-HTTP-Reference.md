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

## Codes d'erreur HTTP

| Code | Signification |
|---|---|
| `200` | Succès |
| `201` | Créé avec succès (POST /api/v1/agents) |
| `202` | Accepté (POST /api/v1/tasks — tâche soumise, exécution asynchrone) |
| `400` | Requête invalide (manifest, champs manquants) |
| `404` | Ressource introuvable |
| `409` | Conflit d'état (agent déjà démarré, tâche déjà terminée, tâche non en `input_required`) |
| `422` | Erreur de traitement (fichier Python invalide, corps de requête invalide) |
| `500` | Erreur interne (SQLite, rebuild HITL) |
| `503` | Service indisponible (capacité saturée, HITL non configuré) |

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

## Voir aussi

- [Briques CLI](./Briques-CLI) — wrapper CLI sur cette API
- [Briques Runtime Core](./Briques-Runtime-Core) — implémentation APIServer axum
- [Briques Triggers](./Briques-Triggers) — moteur de déclenchement
- [Briques Notifications](./Briques-Notifications) — canaux de notification et moteur HITL
- [Dashboard Observabilité](./Dashboard-Observabilite) — dashboard embarqué
- [ADR-006](../adr/ADR-006-rest-json-api-locale) — pourquoi REST JSON plutôt qu'une autre API
- [ADR-017](../adr/ADR-017-hyper-util-unix-socket-serving) — Unix socket avec hyper-util
- [ADR-021](../adr/ADR-021-apollia-triggers-toml-hmac-hot-reload.md) — décisions TOML/HMAC/hot reload
- [ADR-023](../adr/ADR-023) — décisions architecture HITL (TaskRepository, PendingApprovals)
- [ADR-024](../adr/ADR-024) — décisions système de notifications (canaux, événements, SQLite)
- [ADR-025](../adr/ADR-025) — décisions pipelines multi-agents (TOML déclaratif, topologies natives)
- [ADR-026](../adr/ADR-026-observabilite-complete-persistance-timeline-troncature) — observabilité complète, timeline, troncature
- [ADR-033](../adr/ADR-033-config-operateur-sqlite.md) — config opérateur SQLite (CRUD triggers/pipelines/notifications)
- [ADR-034](../adr/ADR-034-chat-hybride-sessions-streaming-hitl-inline.md) — chat hybride : sessions, streaming, HITL inline
- [Briques Chat](./Briques-Chat) — sous-système de chat complet
