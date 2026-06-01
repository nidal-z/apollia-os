# ADR-062 - MCP Server Mode

**Date :** 2026-04-04
**Statut :** Accepté
**Décideur :** Nidal
**Sprint :** 36 (planifié)

---

## Contexte

Apollia OS est actuellement un **client MCP** : il se connecte à des serveurs MCP externes pour utiliser leurs outils. L'écosystème MCP (16 000+ serveurs) permet également à un processus d'être simultanément client ET serveur.

**Cas d'usage :** un IDE (VS Code, Cursor) ou un autre agent IA peut appeler les outils d'Apollia OS via le protocole MCP. Cela permet d'intégrer Apollia dans des workflows existants sans modifier le client.

**Exemple concret :** VS Code avec l'extension MCP peut appeler `submit_task` sur Apollia et recevoir les résultats - l'IDE pilote Apollia comme un outil parmi d'autres.

---

## Décision

Apollia OS expose un **serveur MCP en mode stdio** via `StdioServerTransport` (JSON-RPC 2.0 sur stdin/stdout).

### 9 outils natifs exposés

| Outil MCP | Mappe vers |
|-----------|-----------|
| `list_agents` | `GET /api/v1/agents` |
| `start_agent` | `POST /api/v1/agents/:name/start` |
| `stop_agent` | `POST /api/v1/agents/:name/stop` |
| `get_agent_status` | `GET /api/v1/agents/:name/status` |
| `list_tasks` | `GET /api/v1/tasks` |
| `get_task` | `GET /api/v1/tasks/:id` |
| `list_tools` | `GET /api/v1/tools` |
| `read_memory` | `GET /api/v1/memory/search` |
| `get_runtime_status` | `GET /api/v1/status` |

### Outil `submit_task`

Outil principal du serveur MCP. Soumet une tâche à un agent et retourne le résultat en mode synchrone (attend la completion) :

```json
{
  "name": "submit_task",
  "description": "Soumet une tâche à un agent Apollia et attend le résultat",
  "inputSchema": {
    "type": "object",
    "properties": {
      "agent_name": { "type": "string" },
      "task_input": { "type": "string" },
      "timeout_secs": { "type": "integer", "default": 300 }
    },
    "required": ["agent_name", "task_input"]
  }
}
```

### Apollia comme client ET serveur simultanément

Le runtime peut être configuré pour démarrer le serveur MCP en plus du serveur HTTP REST :

```toml
[mcp.server]
enabled = true       # Démarre le mode serveur MCP stdio
```

Le serveur MCP stdio est géré par un acteur Tokio dédié (`McpServerActor`) - conforme Principe #5.

### Rejet du transport SSE en priorité

Le transport HTTP/SSE permettrait à des clients distants de se connecter au serveur MCP d'Apollia. Ce transport est différé car :
1. Complexité d'implémentation significativement supérieure (authentification, CORS, reconnexion)
2. Contraire au Principe #1 (Local-first) si activé sans restriction
3. Le transport stdio couvre 100% des cas d'usage V1 (IDE local)

Le transport HTTP/SSE sera implémenté dans un sprint futur avec des garde-fous d'authentification.

---

## Conséquences

**Positives :**
- Apollia s'intègre dans tout IDE ou agent supportant MCP (VS Code, Cursor, Claude Desktop)
- Aucune modification du client nécessaire - protocole MCP standard
- Le mode stdio est local-only par construction - conforme Principe #1

**Négatives / Compromis :**
- `submit_task` en mode synchrone bloque jusqu'à la completion - les tâches longues (analyse de repo) peuvent dépasser le timeout du client MCP. `timeout_secs` configurable comme mitigation.

---

## Principes architecturaux impactés

- **Principe #1 - Local-first** : Transport stdio = local uniquement. Conforme.
- **Principe #5 - Un acteur, une responsabilité** : `McpServerActor` gère uniquement le serveur MCP. Conforme.

---

## Liens

- Story d'implémentation : STORY-468 (Sprint 36)
- Implémenté dans : `crates/apollia-mcp/src/server/`, `crates/apollia-runtime/src/mcp_server_actor.rs`
