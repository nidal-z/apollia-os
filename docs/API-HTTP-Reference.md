# Référence API HTTP — Apollia OS

> API REST locale exposée par le runtime Apollia OS sur TCP (port 7771) et Unix socket.
> Tous les endpoints retournent du JSON. Authentification non requise (local-first).

---

## Santé

### `GET /api/v1/health`

Vérifie que le runtime est opérationnel.

**Réponse** `200 OK` :
```json
{ "status": "ok" }
```

---

## Agents

### `GET /api/v1/agents`

Liste tous les agents enregistrés avec leur état courant.

### `POST /api/v1/agents`

Démarre un agent. Corps : `{ "manifest_path": "/path/to/agent" }`.

### `GET /api/v1/agents/:name`

Détails d'un agent spécifique.

### `DELETE /api/v1/agents/:name`

Arrête un agent (transition vers `Stopped`).

---

## Tâches

### `POST /api/v1/tasks`

Soumet une nouvelle tâche. Corps : `{ "agent": "name", "input": {...} }`.

### `GET /api/v1/tasks/:id`

Statut et détails d'une tâche.

### `DELETE /api/v1/tasks/:id`

Annule une tâche en cours.

### `POST /api/v1/tasks/:id/resume`

Reprend une tâche en attente d'approbation (HITL).

### `GET /api/v1/tasks/:id/stream`

Flux SSE pour suivre l'exécution d'une tâche en temps réel.

### `GET /api/v1/tasks/:id/timeline`

Timeline agrégée d'une tâche (5 sources SQLite, 9 types d'événements).

---

## Chat

### `POST /api/v1/sessions`

Crée une session de chat. Corps :
```json
{
  "mode": "libre",
  "tools": ["bash_executor", "file_io"],
  "system_prompt": "Tu es un assistant..."
}
```

Modes : `"libre"` (BuiltInChatAgent) ou `"agent"` (agent Python avec chat).

### `GET /api/v1/sessions`

Liste les sessions de chat. Paramètre query optionnel : `status`.

### `GET /api/v1/sessions/:id`

Détails d'une session avec nombre de messages.

### `DELETE /api/v1/sessions/:id`

Ferme une session de chat. Déclenche l'extraction mémoire post-session si ≥ 4 messages.

### `POST /api/v1/sessions/:id/messages`

Envoie un message dans une session. Corps : `{ "content": "..." }`.
Déclenche la boucle ReAct avec streaming de tokens via SSE.

### `POST /api/v1/sessions/:id/authorize`

Résout une demande d'approbation d'outil (HITL chat).

### `GET /api/v1/sessions/:id/stream`

Flux SSE pour les événements chat en temps réel (tokens, tool calls, approbations).

---

## Profil utilisateur et mémoire

### `GET /api/v1/user/profile`

Retourne le profil utilisateur agrégé (preferences, habits, context).

**Réponse** `200 OK` :
```json
{
  "name": "Nidal",
  "preferences": { "language": "français" },
  "habits": { "working_hours": "9h-18h" },
  "context": { "current_project": "apollia-os" }
}
```

### `PUT /api/v1/user/profile`

Met à jour le profil utilisateur (merge/upsert). Seuls les champs fournis sont modifiés.

**Corps** :
```json
{
  "name": "Nidal",
  "preferences": { "language": "français" }
}
```

**Réponse** : `200 OK`

### `GET /api/v1/user/memory`

Liste les entrées mémoire utilisateur.

**Paramètres query** :
- `category` (optionnel) : `preferences`, `habits`, ou `context` — `422` si invalide
- `limit` (optionnel) : nombre max d'entrées (défaut : 100)

**Réponse** `200 OK` :
```json
{
  "entries": [
    {
      "key": "language",
      "value": "français",
      "source": "user_explicit",
      "updated_at": "2026-03-24T10:00:00Z"
    }
  ]
}
```

### `DELETE /api/v1/user/memory/:key`

Supprime une entrée mémoire par clé (scan toutes catégories).

**Réponse** : `204 No Content` — ou `404 Not Found`.

---

## LLM

### `GET /api/v1/llm/status`

Statut du routeur LLM et des backends configurés.

### `POST /api/v1/llm/ping`

Vérifie la connectivité d'un backend LLM.

### `POST /api/v1/llm/chat`

Appel de complétion LLM direct (hors contexte agent).

---

## Triggers

### `GET /api/v1/triggers`

Liste les triggers configurés.

### `POST /api/v1/triggers/reload`

Recharge à chaud la configuration des triggers.

### `POST /webhooks/:id`

Endpoint webhook avec vérification HMAC-SHA256 (`X-Apollia-Signature`).

---

## Pipelines

### `GET /api/v1/pipelines`

Liste les pipelines et leurs runs.

---

## Configuration (CRUD opérateur)

### Triggers, Pipelines, Notifications

Les endpoints CRUD pour la configuration opérateur (triggers, pipelines, notifications) sont documentés dans les crates respectives. Ces configurations vivent dans SQLite (pas dans `apollia.toml`).

---

## Shutdown

### `POST /api/v1/shutdown`

Déclenche l'arrêt gracieux du runtime. Émet `ShutdownRequested` sur l'EventBus. Drain de 30 secondes par défaut.

---

## Codes d'erreur

| Code | Signification |
|---|---|
| `200` | Succès |
| `201` | Ressource créée |
| `204` | Succès sans contenu |
| `400` | Requête invalide |
| `404` | Ressource non trouvée |
| `422` | Paramètre invalide (ex. catégorie inconnue) |
| `500` | Erreur interne |
| `503` | Service non disponible (composant non configuré) |
