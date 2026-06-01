# API HTTP - Référence - Apollia OS

> Référence complète de l'API REST locale d'Apollia OS. Le contenu a été découpé en trois pages par domaine pour faciliter la navigation et la maintenance. Cette page sert d'index d'entrée.

---

## Pages de référence

### [API-HTTP-Agents](./API-HTTP-Agents) - Agents, Tasks, Chat, LLM, A2A

Endpoints cœur du runtime : cycle de vie des agents, soumission et streaming des tâches, backends LLM (config, chat, complete, costs), Tool Registry, Agent Messaging, A2A (skills, delegate, invoke), plan cache, sessions de chat interactif, santé et shutdown.

Principaux endpoints couverts :
- `/api/v1/health`, `/api/v1/shutdown`
- `/api/v1/agents` (CRUD) + `/api/v1/agents/:name/messages`
- `/api/v1/tasks` (CRUD, `/resume`, `/stream`)
- `/api/v1/llm/*` (status, ping, chat, complete, costs, backends CRUD)
- `/api/v1/tools`, `/api/v1/plan-cache/*`
- `/api/v1/a2a/*` (agents, skills, delegate, invoke)
- `/api/v1/sessions/*` (chat interactif + SSE)

### [API-HTTP-Workspace](./API-HTTP-Workspace) - Triggers, Notifications

Endpoints d'automatisation workspace : déclencheurs (cron, interval, filewatch, webhook), webhooks d'ingestion avec HMAC-SHA256, canaux de notification (desktop, webhook, SSE).

Principaux endpoints couverts :
- `/api/v1/triggers/*` (CRUD, `/fire`, `/enable`, `/disable`, `/logs`, `/reload`)
- `/webhooks/:trigger_id` (HMAC-SHA256)
- `/api/v1/notifications/*` (channels CRUD, events, test, logs)

### [API-HTTP-Observability](./API-HTTP-Observability) - Observability, STT, MCP

Endpoints d'observabilité et d'introspection : audit trail des outils, timeline unifiée des tâches, approbations HITL (pending / resolved), profil et mémoire utilisateur, dashboard HTML/SSE, moteur Speech-to-Text et serveurs MCP.

Principaux endpoints couverts :
- `/api/v1/audit`, `/api/v1/audit/stats`
- `/api/v1/tasks/:id/timeline`
- `/api/v1/approvals/pending`, `/api/v1/approvals/resolved`
- `/api/v1/user/profile`, `/api/v1/user/memory`
- `/dashboard`, `/api/v1/dashboard/*`
- `/api/v1/stt/*` (status, transcribe, transcriptions, models, config)
- `/api/v1/mcp/servers/*` (CRUD, `/restart`, `/config`, `/test`, `/approval`)

---

## Conventions communes

**Transports :**
- **Unix socket** `/tmp/apollia.sock` - non authentifié (permissions filesystem).
- **TCP** `http://localhost:7771` - bearer token obligatoire (`Authorization: Bearer <token>`, voir [API-HTTP-Agents](./API-HTTP-Agents#authentification-tcp-)).

**Base URL :** `http://localhost:7771/api/v1`

**Format :** toutes les réponses sont JSON (`application/json`), sauf `/dashboard` (HTML) et les endpoints SSE (`text/event-stream`).

**Format d'erreur standard :**
```json
{ "error": "description humaine de l'erreur" }
```

**Codes HTTP principaux :** `200` succès, `201` créé, `202` accepté (tâche asynchrone), `204` supprimé, `400` requête invalide, `401` token manquant/invalide, `404` ressource introuvable, `409` conflit d'état, `422` validation, `429` garde-fous A2A, `500` erreur interne, `502`/`504` erreurs A2A, `503` composant non configuré. Le tableau détaillé complet figure dans [API-HTTP-Agents - Codes d'erreur HTTP](./API-HTTP-Agents#codes-derreur-http).

---

## Voir aussi

- [Briques CLI](./Briques-CLI) - wrapper CLI sur cette API
- [Briques Runtime Core](./Briques-Runtime-Core) - implémentation APIServer axum
- [ADR-006](../adr/ADR-006-rest-json-api-locale) - pourquoi REST JSON plutôt qu'une autre API
- [ADR-017](../adr/ADR-017-hyper-util-unix-socket-serving) - Unix socket avec hyper-util
