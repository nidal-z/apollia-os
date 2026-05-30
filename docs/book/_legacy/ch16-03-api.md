# L'API REST

L'APIServer expose deux surfaces : un socket Unix pour la CLI locale, et une API HTTP sur TCP pour les intégrations externes. Les deux desservent les mêmes endpoints.

> **Référence technique :** [API-HTTP-Reference](https://github.com/nidal-z/apollia-os/wiki/API-HTTP-Reference) — liste complète des endpoints par domaine, corps de requêtes, réponses JSON, codes d'erreur.

---

## Deux surfaces, une API

| Surface | Adresse | Usage |
|---|---|---|
| Unix socket | `/tmp/apollia.sock` | CLI `apollia-os` — plus rapide, sécurisé par permissions fichier |
| HTTP/REST | `localhost:7771` | SDK Python, intégrations, Desktop Tauri, `curl` |

```bash
# Via HTTP (debuggage, intégrations)
curl http://localhost:7771/api/v1/agents

# Via socket Unix (CLI interne — même résultat)
curl --unix-socket /tmp/apollia.sock http://localhost/api/v1/agents
```

La CLI utilise toujours le socket Unix. Les SDK et intégrations utilisent TCP. Les deux sont disponibles simultanément — même runtime, mêmes acteurs Tokio.

---

## Streaming SSE

Pour les agents avec `supports_streaming: true`, le résultat est streamé token-by-token via Server-Sent Events :

```bash
curl -N http://localhost:7771/api/v1/tasks/t-abc123/stream
```

```
data: {"event":"task_started","task_id":"t-abc123","agent":"facture-director"}
data: {"event":"step","step":1,"thought":"Je vais appeler extract-invoice..."}
data: {"event":"tool_call","tool":"a2a:extract-invoice","input":"Extrais /data/acme.pdf"}
data: {"event":"observation","output":"{\"numero\":\"FAC-2026-0142\",...}"}
data: {"event":"step","step":2,"thought":"Validation en cours..."}
data: {"event":"tool_call","tool":"a2a:validate-invoice","input":"..."}
data: {"event":"observation","output":"{\"statut\":\"VALIDE\",...}"}
data: {"event":"completed","result":{"status":"completed","output":[...]}}
```

Le stream reste ouvert jusqu'à la completion de la tâche. Si la tâche est suspendue HITL, un événement `input_required` est émis et le stream reste ouvert en attente de reprise.

---

## Trace d'exécution event-sourced

`GET /api/v1/tasks/{id}/trace` retourne la trajectoire complète d'une
tâche depuis la table append-only `runtime_events` : pensées du LLM,
tool calls détaillés (args + outputs), `ctx.log()`, retries,
invocations A2A. C'est la source consommée par la vue `ExecutionTrace`
du desktop.

```bash
curl http://localhost:7771/api/v1/tasks/t-abc123/trace?limit=500
```

```json
{
  "task_id": "t-abc123",
  "events": [
    { "event_id": "01900...", "kind": "thought", "step_num": 1,
      "payload_json": "{\"text\":\"Je vais chercher la facture\"}", "ts": "..." },
    { "event_id": "01900...", "kind": "tool_call_started",
      "payload_json": "{\"tool_name\":\"web_search\",\"args_json\":\"{...}\"}", "ts": "..." }
  ],
  "next_cursor": "01900..."
}
```

Pagination par curseur UUIDv7 — passez `next_cursor` en `?since=` au
prochain appel jusqu'à `null`. Voir [Briques-Runtime-Core](https://github.com/nidal-z/apollia-os/wiki/Briques-Runtime-Core)
pour la spécification complète des `kind` et payloads.

> Distinct de `/timeline` qui agrège 5 bases legacy (audit, plans,
> hitl, llm_calls). `/trace` lit une source unique
> `runtime_events.db`.

---

## Appeler l'API depuis Python

```python
import httpx

# Soumettre une tâche
async with httpx.AsyncClient(base_url="http://localhost:7771") as client:
    response = await client.post("/api/v1/tasks", json={
        "agent": "pdf-invoice-worker",
        "input": "Extrais /home/user/factures/acme.pdf"
    })
    task_id = response.json()["task_id"]

    # Attendre le résultat (polling simple)
    import asyncio
    while True:
        status = await client.get(f"/api/v1/tasks/{task_id}")
        data = status.json()
        if data["status"] in ("completed", "failed", "canceled"):
            print(data["output"])
            break
        await asyncio.sleep(0.5)
```

---

## Format d'erreur standard

Toutes les erreurs suivent le même format JSON :

```json
{
  "error": {
    "code": "AGENT_NOT_FOUND",
    "message": "Aucun agent avec l'identifiant 'pdf-worker' trouvé.",
    "details": null
  }
}
```

Les codes HTTP respectent les conventions REST : `404` pour les ressources absentes, `409` pour les conflits (agent déjà démarré), `503` si le runtime n'est pas prêt, `422` pour les délégations A2A invalides (cycle détecté ou profondeur maximale dépassée).

---

> **Référence complète :** [API-HTTP-Reference](https://github.com/nidal-z/apollia-os/wiki/API-HTTP-Reference) — tous les endpoints agents, tâches, A2A, LLM, sessions, mémoire utilisateur, STT, MCP, triggers, notifications, observabilité, et dashboard SSE.
