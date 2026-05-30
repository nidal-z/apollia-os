# Notifications

Quand une tâche se suspend pour attendre une décision humaine, l'opérateur doit en être informé. Sans notification, la tâche reste silencieusement bloquée jusqu'à ce que quelqu'un pense à vérifier son état.

Le **NotificationEngine** d'Apollia OS résout ce problème en envoyant une alerte dès qu'un événement critique se produit — tâche suspendue, tâche échouée, agent dégradé. Il dispatch via les canaux configurés : notification desktop native ou webhook HTTP.

---

## Architecture

Le `NotificationEngine` est un acteur de fond qui s'abonne à l'EventBus en lecture seule. Il ne bloque jamais l'exécution d'une tâche — une notification ratée ne perturbe pas l'agent.

```
EventBus (broadcast)
      │
      ▼
NotificationEngine
      │  map_event() — transforme RuntimeEvent → Notification
      │  6 événements mappés, les autres → ignorés
      ▼
  event_filter
      │
      ▼
  dispatch_notif()
    ├── DesktopChannel   → notification native OS (macOS, Linux, Windows)
    └── WebhookChannel   → POST JSON vers votre URL (Slack, n8n, custom)
```

**Principe de conception :** une erreur de canal (webhook timeout, desktop indisponible) est loguée en `warn!` et le dispatch continue vers les autres canaux. Jamais d'interruption de l'exécution pour une notification ratée.

---

## Les 6 événements notifiés

| Événement | Sévérité | Quand |
|---|---|---|
| `task.input_required` | Warning | Une tâche attend une décision HITL |
| `task.failed` | Error | Une tâche a échoué |
| `task.completed` | Info | Une tâche s'est complétée |
| `agent.degraded` | Warning | Un agent est passé en état DEGRADED |
| `trigger.error` | Error | Un trigger a échoué à son exécution |
| `llm.model_failed` | Error | Un backend LLM est indisponible |

`task.input_required` est l'événement HITL central — c'est lui qui alerte l'opérateur qu'une décision est attendue.

---

## Configurer les canaux

Les canaux se créent via l'API REST ou la CLI — plus dans `apollia.toml`.

### Canal desktop

```bash
# Créer un canal de notifications desktop
curl -X POST http://localhost:7771/api/v1/notifications/channels \
  -H "Content-Type: application/json" \
  -d '{
    "id": "desktop-main",
    "type": "desktop",
    "enabled": true,
    "events": ["task.input_required", "task.failed"]
  }'
```

Les notifications desktop utilisent l'API native de l'OS (macOS, Linux avec libnotify, Windows). Sur un environnement headless (Linux sans `DISPLAY`), le canal desktop retourne `Ok()` silencieusement — pas d'erreur fatale.

### Canal webhook

```bash
# Créer un canal webhook vers Slack
curl -X POST http://localhost:7771/api/v1/notifications/channels \
  -H "Content-Type: application/json" \
  -d '{
    "id": "slack-ops",
    "type": "webhook",
    "enabled": true,
    "url": "https://hooks.slack.com/services/XXX/YYY/ZZZ",
    "events": ["task.input_required", "task.failed", "agent.degraded"]
  }'
```

Le webhook reçoit un POST JSON avec cette structure :

```json
{
  "event": "task.input_required",
  "timestamp": "2026-04-02T14:32:01Z",
  "task_id": "t-abc123",
  "agent": "envoi-devis",
  "message": "Tâche t-abc123 (envoi-devis) en attente d'approbation",
  "severity": "warning",
  "metadata": {
    "prompt": "Envoyer le devis de 12 400 € à dupont@example.com ?",
    "approve_url": "http://localhost:7771/api/v1/tasks/t-abc123/resume"
  }
}
```

Le champ `metadata.approve_url` est particulièrement utile dans les intégrations Slack : vous pouvez créer un bouton qui pointe directement vers cet endpoint pour approuver en un clic depuis le channel Slack.

Timeout webhook : 5 secondes. En cas d'échec ou de code HTTP non-2xx, l'erreur est loguée en `warn!` et le dispatch continue.

---

## Gérer les événements globaux

Les événements globaux définissent quels événements sont notifiés sur tous les canaux qui n'ont pas de liste `events` propre :

```bash
# Définir les événements globaux
curl -X PUT http://localhost:7771/api/v1/notifications/events \
  -H "Content-Type: application/json" \
  -d '["task.input_required", "task.failed"]'

# Lister les événements globaux configurés
curl http://localhost:7771/api/v1/notifications/events
```

Un canal avec `"events": null` (ou champ absent) utilise la liste globale. Un canal avec `"events": ["task.input_required"]` n'est notifié que pour cet événement, indépendamment de la liste globale.

---

## Lister et modifier les canaux

```bash
# Lister tous les canaux
curl http://localhost:7771/api/v1/notifications/channels
# [{"id":"desktop-main","type":"desktop","enabled":true,...},
#  {"id":"slack-ops","type":"webhook","enabled":true,...}]

# Désactiver temporairement un canal
curl -X PUT http://localhost:7771/api/v1/notifications/channels/slack-ops \
  -H "Content-Type: application/json" \
  -d '{"enabled": false}'

# Supprimer un canal
curl -X DELETE http://localhost:7771/api/v1/notifications/channels/slack-ops
```

---

## Comportement si aucun canal n'est configuré

Si `notifications.db` est vide (aucun canal, aucun événement global), le Supervisor n'instancie pas le `NotificationEngine` — zéro overhead au runtime. Les tâches HITL fonctionnent normalement, sans alerte.

C'est le comportement recommandé pour les environnements de développement local où les notifications ne sont pas nécessaires.

---

## Flux complet HITL + notification

```
1. Agent calcule → AIPResult.input_required(prompt, context)
2. ORIA suspend → TaskInputRequired sur EventBus
3. NotificationEngine détecte TaskInputRequired
   → DesktopChannel : notification native "Approbation requise"
   → WebhookChannel : POST Slack avec approve_url
4. Opérateur reçoit l'alerte → clique le lien ou appelle l'API
5. POST /api/v1/tasks/{id}/resume {"approved": true}
6. ORIA reprend → TaskResumed sur EventBus
7. Agent continue → AIPResult.completed(...)
```

La notification est le pont entre l'exécution asynchrone des agents et la prise de décision humaine synchrone. Sans elle, le HITL est techniquement fonctionnel mais pratiquement inutilisable en production.
