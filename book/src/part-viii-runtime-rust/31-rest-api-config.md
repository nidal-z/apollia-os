# API REST et configuration

L'APIServer expose deux surfaces : un **socket Unix** pour la CLI locale, et une **API HTTP** sur TCP pour les intégrations externes. Les deux desservent les mêmes endpoints, le même runtime, les mêmes acteurs Tokio.

La configuration du runtime se sépare en deux : **structurelle** (`apollia.toml`, lue au démarrage) et **opérationnelle** (SQLite, modifiable à chaud).

---

## Deux surfaces, une API

| Surface | Adresse | Usage |
|---|---|---|
| Unix socket | `/tmp/apollia.sock` | CLI `apollia ...`, sécurisé par permissions fichier, ultra-rapide |
| HTTP/REST | `localhost:7771` | SDK Python, intégrations, Desktop Tauri, `curl` |

```bash
# Via HTTP (debug, intégrations)
curl http://localhost:7771/api/v1/agents

# Via socket Unix (même résultat)
curl --unix-socket /tmp/apollia.sock http://localhost/api/v1/agents
```

La CLI utilise toujours le socket Unix. Les SDK et intégrations utilisent TCP. Les deux sont disponibles simultanément, sur le même runtime.

> **Référence technique :** la liste complète des endpoints par domaine (agents, tasks, A2A, LLM, sessions, mémoire, STT, MCP, triggers, notifications) sera dans la page wiki `API-HTTP-Reference` *(wiki disponible prochainement)*.

---

## Streaming SSE

Pour les agents avec streaming activé, le résultat est streamé token-by-token via Server-Sent Events :

```bash
curl -N http://localhost:7771/api/v1/tasks/t-abc123/stream
```

```
data: {"event":"task_started","task_id":"t-abc123","agent":"research-director"}
data: {"event":"thought","step":1,"text":"Je vais appeler pdf.read_text..."}
data: {"event":"tool_call","tool":"a2a:pdf.read_text","input":"..."}
data: {"event":"observation","output":"{...}"}
data: {"event":"completed","result":{"status":"completed","output":[...]}}
```

Le stream reste ouvert jusqu'à la fin de la tâche. Si la tâche est suspendue HITL, un événement `input_required` est émis et le stream reste ouvert en attente de reprise.

---

## Trace d'exécution

`GET /api/v1/tasks/{id}/trace` retourne la trajectoire complète d'une tâche depuis la table append-only `runtime_events` : pensées du LLM, tool calls détaillés (args + outputs), `ctx.logger.info`, retries, invocations A2A.

```bash
curl http://localhost:7771/api/v1/tasks/t-abc123/trace?limit=500
```

```json
{
  "task_id": "t-abc123",
  "events": [
    {
      "event_id": "01900...",
      "kind": "thought",
      "step_num": 1,
      "payload_json": "{\"text\":\"Je vais chercher le PDF\"}",
      "ts": "..."
    }
  ],
  "next_cursor": "01900..."
}
```

Pagination par curseur UUIDv7. Passez `next_cursor` en `?since=` au prochain appel jusqu'à `null`.

---

## Appeler l'API depuis Python

Pour les intégrations qui ne sont pas elles-mêmes des agents Apollia :

```python
import httpx

async with httpx.AsyncClient(base_url="http://localhost:7771") as client:
    response = await client.post("/api/v1/tasks", json={
        "agent": "pdf-worker",
        "skill": "pdf.read_text",
        "input": {"path": "/tmp/report.pdf"},
    })
    task_id = response.json()["task_id"]

    while True:
        status = await client.get(f"/api/v1/tasks/{task_id}")
        data = status.json()
        if data["status"] in ("completed", "failed", "input_required"):
            print(data["output"])
            break
        await asyncio.sleep(0.5)
```

Note : depuis un autre agent Apollia, préférez `ctx.a2a.invoke(...)` qui passe par PyO3 sans HTTP (cf. [chapitre 14](../part-iii-the-ctx-protocol/14-ctx-a2a.md)).

---

## Format d'erreur standard

Toutes les erreurs API suivent le même format JSON :

```json
{
  "error": {
    "code": "AGENT_NOT_FOUND",
    "message": "Aucun agent avec l'identifiant 'pdf-worker' trouvé.",
    "details": null
  }
}
```

Les codes HTTP respectent les conventions REST : `404` pour les ressources absentes, `409` pour les conflits, `503` si le runtime n'est pas prêt, `422` pour les délégations A2A invalides (cycle détecté, profondeur maximale dépassée).

Le `code` métier est le même que celui que produit `DomainError(code, message)` côté agent (cf. [chapitre 22](../part-v-error-handling/22-domain-errors.md)).

---

## apollia.toml : configuration structurelle

Le fichier `~/.apollia/apollia.toml` est lu au démarrage. Il couvre les sections suivantes : `[runtime]`, `[memory]`, `[tools]`, `[logging]`, `[a2a]`, `[chat]`, `[observability]`, `[stt]`, et les backends LLM via `[[llm.backends]]`.

```toml
# Exemple minimal : un backend LLM Anthropic
[[llm.backends]]
name        = "anthropic"
type        = "anthropic"
model       = "claude-haiku-4-5"
api_key_env = "ANTHROPIC_API_KEY"
default     = true
```

Si aucun backend n'est configuré, `ctx.llm` lève une erreur à la première utilisation. Le runtime démarre quand même avec un warning.

> **Référence technique :** toutes les sections, clés et valeurs par défaut seront dans la page wiki `Config-apollia-toml` *(wiki disponible prochainement)*.

---

## Précédence de configuration

```
flags CLI > variables d'environnement > apollia.toml > valeurs par défaut
```

```bash
# Surcharger le port via flag
apollia-os start --api-port 7772

# Surcharger via variable d'environnement
APOLLIA_API_PORT=7772 apollia-os start

# Utiliser un fichier de config alternatif
apollia-os start --config ./dev.toml
```

---

## Configuration opérationnelle : SQLite

Les triggers, pipelines et canaux de notification ne sont **pas** dans `apollia.toml`. Ils sont gérés via l'API REST et persistés en SQLite :

| Données | Base SQLite | Géré via |
|---|---|---|
| Triggers | `~/.apollia/triggers_def.db` | API REST + Desktop |
| Pipelines | `~/.apollia/pipelines_def.db` | API REST + Desktop |
| Notifications | `~/.apollia/notifications.db` | API REST + Desktop |
| Sessions chat | `~/.apollia/chat.db` | API REST |
| Tâches | `~/.apollia/tasks.db` | Interne |
| Mémoire | `~/.apollia/memory/*.db` | Interne + CLI |
| Permissions et outils | `~/.apollia/governance.db` | Desktop + CLI |

La raison de cette séparation : `apollia.toml` est lu au démarrage et ne change pas. Les triggers et pipelines doivent être modifiables à chaud via l'UI sans redémarrer le runtime. SQLite avec hot reload est la solution compatible.

---

## Localiser et inspecter sa config

```bash
# Localisation par défaut
cat ~/.apollia/apollia.toml

# Ouvrir dans l'éditeur par défaut
apollia-os config edit

# Afficher la config effective (avec défauts appliqués)
apollia-os config show --json
```

---

## Lire les métriques de session

Le runtime maintient pour chaque exécution une `SessionMetrics` agrégée : steps consommés, appels d'outils par catégorie, latence cumulée, tokens LLM. Les métriques sont émises sur l'EventBus en fin de tâche (`RuntimeEvent::TaskCompleted`), et persistées dans `tasks.db` pour requêtes a posteriori.

Pour les lire en cours d'exécution depuis un agent, c'est `ctx.budget` (cf. [chapitre 17](../part-iii-the-ctx-protocol/17-ctx-events-logger-budget.md)) qui expose `steps_remaining`, `tool_calls_remaining`, `elapsed_seconds`.

---

## ADRs

- `ADR-006` : REST + JSON API locale
- `ADR-017` : hyper-util Unix socket serving
- `ADR-033` : Config opérateur en SQLite
- `ADR-051` : API auth

*(ADRs disponibles prochainement, cf. l'encadré "ADRs et wiki" en introduction.)*
