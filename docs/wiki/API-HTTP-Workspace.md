# API HTTP — Workspace (Triggers, Notifications) — Apollia OS

> Référence des endpoints REST liés aux **triggers, webhooks et notifications**.
> Public cible : développeur intégrant Apollia OS dans un système externe.
>
> Cette page fait partie d'un découpage en trois :
> - [API-HTTP-Agents](./API-HTTP-Agents) — agents, tasks, chat, LLM, tools, a2a, plan-cache, sessions, health, shutdown
> - **API-HTTP-Workspace** (cette page) — triggers, webhooks, notifications
> - [API-HTTP-Observability](./API-HTTP-Observability) — audit, timeline, approvals, user memory, dashboard, STT, MCP

---

## Vue d'ensemble

L'API HTTP locale est exposée sur deux transports :
- **Unix socket** : `/tmp/apollia.sock` — recommandé pour les processus locaux, **non authentifié** (accès par permissions filesystem)
- **TCP** : `http://localhost:7771` — compatible avec tout client HTTP, **authentification requise** (ADR-051)

Tous les endpoints retournent du JSON.

### Authentification TCP

Toutes les requêtes TCP doivent porter le header `Authorization: Bearer <token>` :

```http
GET /api/v1/triggers HTTP/1.1
Host: localhost:7771
Authorization: Bearer 4a3b2c1d...  (64 hex chars)
```

Le token est généré au premier démarrage et stocké dans `~/.apollia/api-token` (permissions `0600`). Voir [API-HTTP-Agents](./API-HTTP-Agents#authentification-tcp-sprint-34) pour les détails complets.

> Le webhook `/webhooks/:trigger_id` utilise une authentification HMAC-SHA256 spécifique (voir section Webhook ci-dessous), pas le bearer token.

**Base URL :** `http://localhost:7771/api/v1`

---

## Triggers *(CRUD)*

### POST /api/v1/triggers

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

### PUT /api/v1/triggers/:id

Modifier un trigger existant. Tous les champs sont modifiables.

**Réponse 200 :** définition mise à jour avec `updated_at`.

**Erreurs :** `404` NotFound, `422` ValidationError.

### DELETE /api/v1/triggers/:id

Supprimer un trigger.

**Réponse 200 :** `{ "deleted": true }`

**Erreurs :** `404` NotFound.

### GET /api/v1/triggers/:id

Lire la définition complète d'un trigger.

**Réponse 200 :** définition complète (id, agent, source, enabled, on_busy, input_template, created_at, updated_at).

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

## Webhook

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

## Notifications *(CRUD)*

### POST /api/v1/notifications/channels

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

### PUT /api/v1/notifications/channels/:id

Modifier un canal existant. Champs optionnels.

**Réponse 200 :** canal mis à jour avec `updated_at`.

**Erreurs :** `404` NotFound, `422` ValidationError.

### DELETE /api/v1/notifications/channels/:id

Supprimer un canal.

**Réponse 200 :** `{ "deleted": true }`

**Erreurs :** `404` NotFound.

### GET /api/v1/notifications/events

Lire les événements globaux configurés.

**Réponse 200 :**
```json
{ "events": ["task.input_required", "task.failed", "agent.degraded"] }
```

### PUT /api/v1/notifications/events

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

## Codes d'erreur HTTP

Voir [API-HTTP-Agents — Codes d'erreur HTTP](./API-HTTP-Agents#codes-derreur-http) pour le tableau complet.

---

## Voir aussi

- [API-HTTP-Agents](./API-HTTP-Agents) — agents, tasks, LLM, tools, a2a, sessions
- [API-HTTP-Observability](./API-HTTP-Observability) — audit, timeline, approvals, user, dashboard, STT, MCP
- [Briques Triggers](./Briques-Triggers) — moteur de déclenchement
- [Briques Notifications](./Briques-Notifications) — canaux de notification et moteur HITL
- [ADR-021](../adr/ADR-021-apollia-triggers-toml-hmac-hot-reload.md) — décisions TOML/HMAC/hot reload
- [ADR-024](../adr/ADR-024) — décisions système de notifications (canaux, événements, SQLite)
- [ADR-033](../adr/ADR-033-config-operateur-sqlite.md) — config opérateur SQLite (CRUD triggers/notifications)
