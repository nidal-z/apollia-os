# Dashboard d'Observabilité Embarqué

> *Le dashboard Apollia OS est un outil de monitoring temps réel 100% embarqué dans le binaire via `include_str!`. Zéro CDN externe, zéro build step, zéro fichier externe requis.*

---

## 1. Accès

```bash
# Ouvrir dans le navigateur
$ open http://localhost:7771/dashboard

# ou depuis curl (HTML brut)
$ curl http://localhost:7771/dashboard
```

Le dashboard s'affiche dans n'importe quel navigateur moderne. Toutes les ressources (HTML, CSS, JavaScript HTMX minifié) sont servies depuis le binaire `apollia-os`.

L'[application desktop](./Briques-Desktop.md) offre une interface Svelte plus riche avec gestion des agents, tasks, timeline interactive et approbations HITL temps reel. Le dashboard HTMX reste disponible pour le monitoring leger via navigateur.

---

## 2. Interface

Le dashboard est organisé en **7 sections** mises à jour en temps réel via SSE + HTMX :

| Section | Données affichées | Événements SSE |
|---|---|---|
| **Agents** | ProcessState, manifest, tâches actives | `AgentReady`, `AgentDegraded`, `AgentStopped` |
| **Tasks** | TaskState, agent cible, durée, erreur | `TaskStarted`, `TaskCompleted`, `TaskCanceled` |
| **Triggers** | Fires, skips, erreurs, dernier fire | `TriggerFired`, `TriggerSkipped`, `TriggerError` |
| **Tools** | Circuit breaker state, appels récents | `ToolCircuitBroken`, `ToolCircuitRestored` |
| **LLM** | Backends actifs, coût estimé, latence | `LlmModelReady`, `LlmCallCompleted` |
| **Audit** | Dernières invocations d'outils | Polling `GET /api/v1/dashboard/state` |
| **Plan Cache** | Entrées cachées, taux de hit, purge | `PlanCacheHit`, API `GET /api/v1/plan-cache/stats` |
| **Agent Messages** | Messages inter-agents, files | `AgentMessageSent`, API `GET /api/v1/agents/:name/messages` |

---

## 3. Architecture technique

### 3.1 Pattern HTMX + SSE

Le dashboard suit un pattern hybride :

1. **Chargement initial** - `GET /api/v1/dashboard/state` retourne un snapshot JSON complet
2. **Mise à jour temps réel** - `GET /api/v1/dashboard/stream` pousse des fragments HTML via SSE
3. **HTMX** reçoit les fragments et met à jour les sections correspondantes via `hx-swap-oob`

```
Browser
  │
  ├── GET /dashboard → HTML embarqué (include_str!)
  │
  ├── GET /api/v1/dashboard/state → JSON snapshot initial
  │   {agents: [...], tasks: [...], triggers: [...], ...}
  │
  └── GET /api/v1/dashboard/stream (SSE persistant)
        │
        ├── event: agents   → <div id="section-agents">...</div>
        ├── event: tasks    → <div id="section-tasks">...</div>
        ├── event: triggers → <div id="section-triggers">...</div>
        ├── event: llm      → <div id="section-llm">...</div>
        └── event: tools    → <div id="section-tools">...</div>
```

### 3.2 Routes REST

```
GET /dashboard
    → Sert dashboard.html (embarqué via include_str! depuis assets/)
    → Content-Type: text/html

GET /api/v1/dashboard/state
    → Snapshot JSON de l'état complet du runtime
    → Utilisé au chargement initial

GET /api/v1/dashboard/partials/{section}
    → Fragment HTML d'une section (agents | tasks | triggers | tools | llm | audit)
    → Utilisé pour le rafraîchissement à la demande

GET /api/v1/dashboard/stream
    → SSE stream - événements nommés alignés sur les RuntimeEvent
    → Connection: keep-alive, Content-Type: text/event-stream
```

### 3.3 Mapping RuntimeEvent → SSE

```rust
// Événements → canaux SSE nommés
AgentReady | AgentDegraded | AgentStopped           → event: "agents"
TaskStarted | TaskCompleted | TaskCanceled           → event: "tasks"
TriggerFired | TriggerSkipped | TriggerError
| TriggerEnabled | TriggerDisabled
| TriggersReloaded                                   → event: "triggers"
LlmModelReady | LlmCallCompleted                    → event: "llm"
ToolCircuitBroken | ToolCircuitRestored             → event: "tools"
```

---

## 4. Implémentation embarquée

Le dashboard est **100% inclus dans le binaire** à la compilation :

```rust
// crates/apollia-runtime/src/api/routes_dashboard.rs

const DASHBOARD_HTML: &str = include_str!(
    concat!(env!("CARGO_MANIFEST_DIR"), "/assets/dashboard.html")
);
```

`assets/dashboard.html` contient :
- Le HTML complet du dashboard
- HTMX minifié (inline `<script>`)
- CSS minimal inline (pas de fichier externe)

**Avantages :**
- Zéro fichier à déployer côté ops
- Fonctionne hors ligne
- Compatible principe #2 (Zéro dépendance externe)
- Reproductible : le dashboard est identique quelle que soit la machine

---

## 5. Snapshot état initial

`GET /api/v1/dashboard/state` retourne :

```json
{
  "agents": [
    {
      "id": "agent-uuid",
      "name": "rapport-agent",
      "state": "ACTIVE",
      "active_tasks": 0,
      "manifest": { "version": "1.0.0", "tools_required": ["file_io"] }
    }
  ],
  "tasks": [
    {
      "task_id": "task-uuid",
      "agent": "rapport-agent",
      "state": "working",
      "submitted_at": "2026-03-09T08:00:00Z"
    }
  ],
  "triggers": [
    {
      "id": "rapport-hebdomadaire",
      "agent": "rapport-agent",
      "type": "cron",
      "enabled": true,
      "fire_count": 42,
      "skip_count": 3,
      "last_fired_at": "2026-03-08T08:00:00Z"
    }
  ],
  "llm_backends": [
    {
      "name": "local",
      "kind": "embedded",
      "status": "ready"
    }
  ],
  "runtime": {
    "version": "0.9.0",
    "uptime_secs": 3600,
    "total_tasks_processed": 150
  }
}
```

---

## 6. Sécurité

Le dashboard est exposé **uniquement sur `localhost:7771`** (lié à `127.0.0.1`). Il n'est pas accessible depuis l'extérieur sans tunnel explicite.

Si le runtime est exposé en production sur un port public, le dashboard doit être protégé par un reverse proxy avec authentification. Apollia OS ne gère pas l'authentification dashboard - c'est délibéré (principe local-first).

---

---

## 7. Timeline API

En complément du dashboard temps réel, l'API Timeline fournit une vue chronologique structurée de chaque tâche :

```bash
$ curl http://localhost:7771/api/v1/tasks/t-abc123/timeline
```

Cette API agrège 5 sources SQLite (hitl.db, plans.db, llm_calls.db, audit.db) et retourne une liste d'événements ordonnés par timestamp : transitions d'état, steps ORIA, appels outils, appels LLM, suspensions HITL.

Voir [API-HTTP-Observability - Timeline](./API-HTTP-Observability#get-apiv1tasksidtimeline) pour le schéma complet de la réponse.

---

*Voir aussi : [Briques Runtime Core](./Briques-Runtime-Core) · [Briques Triggers](./Briques-Triggers) · [API-HTTP-Observability](./API-HTTP-Observability) · [ADR-012](../adr/ADR-012-observability-feedback.md)*
