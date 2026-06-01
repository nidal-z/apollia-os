# API HTTP - Observability (Audit, Timeline, STT, MCP) - Apollia OS

> Référence des endpoints REST liés à **l'audit trail, la timeline, les approbations HITL, le profil / la mémoire utilisateur, le dashboard, STT et MCP**.
> Public cible : développeur intégrant Apollia OS dans un système externe.
>
> Cette page fait partie d'un découpage en trois :
> - [API-HTTP-Agents](./API-HTTP-Agents) - agents, tasks, chat, LLM, tools, a2a, plan-cache, sessions, health, shutdown
> - [API-HTTP-Workspace](./API-HTTP-Workspace) - triggers, webhooks, notifications
> - **API-HTTP-Observability** (cette page) - audit, timeline, approvals, user memory, dashboard, STT, MCP

---

## Vue d'ensemble

L'API HTTP locale est exposée sur deux transports :
- **Unix socket** : `/tmp/apollia.sock` - recommandé pour les processus locaux, **non authentifié** (accès par permissions filesystem)
- **TCP** : `http://localhost:7771` - compatible avec tout client HTTP, **authentification requise** (ADR-051)

Tous les endpoints retournent du JSON.

### Authentification TCP

Toutes les requêtes TCP doivent porter le header `Authorization: Bearer <token>` :

```http
GET /api/v1/audit HTTP/1.1
Host: localhost:7771
Authorization: Bearer 4a3b2c1d...  (64 hex chars)
```

Le token est généré au premier démarrage et stocké dans `~/.apollia/api-token` (permissions `0600`). Voir [API-HTTP-Agents](./API-HTTP-Agents#authentification-tcp-) pour les détails complets.

> Les endpoints `/dashboard` HTML et `/api/v1/dashboard/*` requièrent le même bearer token sur TCP.

**Base URL :** `http://localhost:7771/api/v1`

---

## Audit Trail

### GET /api/v1/audit

Dernières invocations d'outils enregistrées dans l'audit trail. Utile pour debug et conformité.

**Query params :**
- `limit` (optionnel, défaut: 20, max: 500) - nombre d'événements à retourner

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

## Observabilité - Timeline

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
- `404` - tâche introuvable

---

## Approbations HITL

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

> La reprise d'une tâche suspendue se fait via `POST /api/v1/tasks/:id/resume` - voir [API-HTTP-Agents](./API-HTTP-Agents).

---

### GET /api/v1/approvals/resolved

Historique des approbations résolues (approuvées ou rejetées).

**Query params :**
- `limit` (optionnel, défaut: 20) - nombre d'entrées
- `days` (optionnel, défaut: 7) - fenêtre temporelle en jours

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

## Profil Utilisateur

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

Met à jour le profil utilisateur (upsert, fusion par catégorie - les champs absents ne sont pas supprimés).

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

## Mémoire Utilisateur

### GET /api/v1/user/memory

Liste les entrées de mémoire utilisateur brutes avec filtres optionnels.

**Query params :**
- `category` (optionnel) - `preferences`, `habits`, ou `context`
- `limit` (optionnel, défaut: 100) - nombre maximum d'entrées

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
- `422` - valeur `category` invalide (n'est pas `preferences`, `habits`, ou `context`)
- `503` - mémoire utilisateur non configurée

---

### DELETE /api/v1/user/memory/:key

Supprime une entrée de mémoire par clé (toutes les catégories sont scrutées).

**Réponse 204 :** suppression réussie.

**Erreurs :**
- `404` - clé introuvable dans aucune catégorie
- `503` - mémoire utilisateur non configurée

---

## Dashboard

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

## STT - Speech-to-Text

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
- `400` - format audio non supporté ou fichier vide
- `503` - moteur STT absent

### GET /api/v1/stt/transcriptions

Historique des transcriptions.

**Query params :**
- `limit` (optionnel, défaut: 50) - nombre de résultats
- `offset` (optionnel, défaut: 0) - pagination

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

**Erreurs :** `503` - moteur STT absent.

### GET /api/v1/stt/models

Lister les fichiers modèles `.bin` disponibles dans `~/.apollia/models/`.

**Réponse 200 :**
```json
{
  "models": [
    {
      "name": "whisper-large-v3-fr-q5_0",
      "path": "~/.apollia/models/whisper-large-v3-fr-q5_0.bin",
      "size_mb": 956.2
    }
  ]
}
```

### GET /api/v1/stt/config

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

### PUT /api/v1/stt/config

Met à jour la configuration STT (upsert). Remplace le singleton en base.

**Corps :** objet `SttConfigRow` complet (les champs avec valeurs par défaut peuvent être omis)

**Réponse 200 :** configuration mise à jour
**Réponse 503 :** dépôt non disponible

---

## MCP *(ADR-044)*

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
- `404` - serveur introuvable
- `503` - MCP non configuré

---

## Codes d'erreur HTTP

Voir [API-HTTP-Agents - Codes d'erreur HTTP](./API-HTTP-Agents#codes-derreur-http) pour le tableau complet.

---

## Voir aussi

- [API-HTTP-Agents](./API-HTTP-Agents) - agents, tasks, LLM, tools, a2a, sessions
- [API-HTTP-Workspace](./API-HTTP-Workspace) - triggers, webhooks, notifications
- [Dashboard Observabilité](./Dashboard-Observabilite) - dashboard embarqué
- [Briques STT](./Briques-STT) - moteur Speech-to-Text embarqué
- [Briques MCP](./Briques-MCP) - spécification crate apollia-mcp
- [MCP - Guide utilisateur](./MCP-Guide-Utilisateur) - configuration mcp.toml, exemples serveurs MCP
- [Briques User Memory](./Briques-User-Memory) - mémoire utilisateur
- [ADR-026](../adr/ADR-026-observabilite-complete-persistance-timeline-troncature) - observabilité complète, timeline, troncature
- [ADR-041](../adr/ADR-041-moteur-stt-embarque-whisper-rs-trait-stt-backend.md) - décisions moteur STT (whisper-rs, trait SttBackend)
- [ADR-044](../adr/ADR-044-client-mcp.md) - décisions client MCP (transport stdio, McpClientManager, HITL)
