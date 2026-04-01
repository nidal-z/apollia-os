# Runtime Core — Supervision, Routing, API, EventBus

> *Le cerveau opérationnel d'Apollia OS : comment les briques sont orchestrées, supervisées, et exposées.*

---

## 1. Architecture interne — Un superviseur d'acteurs

Le Runtime Core n'est **pas un monolithe interne**. C'est un ensemble d'acteurs Tokio, chacun avec une responsabilité unique, communiquant exclusivement par messages.

```
┌─────────────────────────────────────────────────────────────┐
│                       RUNTIME CORE                          │
│                                                             │
│  ┌──────────────┐  ← watchdog tous acteurs                 │
│  │  Supervisor  │                                           │
│  └──────┬───────┘                                           │
│         │ démarre dans l'ordre                              │
│  ┌──────▼───────┐  ┌───────────────┐  ┌─────────────────┐ │
│  │  EventBus    │  │ AgentRegistry │  │   TaskRouter    │ │
│  │ (broadcast)  │  │  (état agents)│  │ (dispatch tasks)│ │
│  └──────┬───────┘  └───────┬───────┘  └────────┬────────┘ │
│         │ événements       │                    │          │
│  ┌──────▼───────────────────▼──────────────────▼────────┐  │
│  │            ExecutionCoordinator[agent_N]              │  │
│  │         (un par agent ACTIVE, sémaphore concurrence)  │  │
│  └──────────────────────────┬────────────────────────────┘  │
│                             │                               │
│                    ┌────────▼───────────────┐               │
│                    │     ORIA Engine         │               │
│                    └────────────────────────┘               │
│                                                             │
│  ┌──────────────────────────────────────────────────────┐  │
│  │  APIServer (axum)                                     │  │
│  │  Unix socket /tmp/apollia.sock + TCP localhost:7771   │  │
│  └──────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────┘
```

---

## 2. Supervisor — Démarrage et watchdog

### 2.1 Séquence de démarrage (ordre strict)

```
1.  EventBus            → bus interne (premier, tout le monde en dépend)
2.  AgentRegistry       → registre d'état
3.  Tool Registry       → catalogue outils + résolution MCP
4.  Memory Engine       → ouverture connexions SQLite
5.  LlmRouter           → backends LLM (embedded + cloud) [Sprint 8]
6.  TriggerEngine       → moteur de déclenchement automatique [Sprint 9]
    └── ouvre TriggerDefinitionRepository (triggers_def.db) [Sprint 17]
7.  PipelineEngine      → orchestration multi-agent [Sprint 12]
    └── ouvre PipelineDefinitionRepository (pipelines_def.db) [Sprint 17]
8.  APIServer           → accepte les connexions externes
9.  NotificationEngine  → alertes desktop / webhook [Sprint 11]
    └── ouvre NotificationConfigRepository (notifications.db) [Sprint 17]
10. AgentMailbox        → messagerie inter-agents [Sprint 20]
    └── files de messages par agent (max 100), AgentMailboxHandle (Clone+Send+Sync)
11. ChatSessionManager  → sessions de chat interactif [Sprint 18]
    └── ouvre ChatSessionRepository (chat.db), restaure autorisations
12. SttEngine           → moteur Speech-to-Text embarqué [Sprint 24]
    └── ouvre SttRepository (stt.db), charge WhisperCppBackend (conditionnel : stt.enabled)
13. BundledAgents        → auto-installation des agents bundled [Sprint 32]
    └── lit agents/bundled/manifest.json, installe les 4 agents si absents de la DB
```

Depuis le Sprint 17 (ADR-033), le Supervisor ouvre les repositories SQLite pour les triggers, pipelines et notifications au démarrage. Les définitions sont chargées depuis SQLite (plus depuis `apollia.toml`). Chaque repository est wrappé dans `Arc<Mutex<>>` et stocké dans `AppState` pour les routes CRUD.

Chaque acteur émet un événement `RuntimeEvent::Ready(actor_id)` sur l'EventBus quand son init est terminée. Le Supervisor attend ce signal (timeout 10s) avant de démarrer le suivant. **Démarrage séquentiel strict** — pas de démarrage parallèle qui masquerait des dépendances.

### 2.2 Mode embarqué (Desktop — ADR-027)

L'application desktop Tauri utilise `init_embedded()` pour démarrer le runtime dans un thread dédié :

```rust
pub fn init_embedded(config: EmbeddedConfig) -> Result<RuntimeHandle, EmbeddedError>
```

`init_embedded()` spawn un thread `"apollia-runtime"` qui crée un `tokio::Runtime`, démarre le `Supervisor`, et attend `AllReady` (timeout configurable, défaut 30s). Le `RuntimeHandle` retourné contient les handles Tokio de tous les acteurs — réutilisables directement par les commandes Tauri `#[tauri::command]` sans sérialisation HTTP.

Le socket Unix et l'API TCP restent actifs : la CLI fonctionne en parallèle du desktop.

### 2.3 Restart policy

```rust
pub enum RestartPolicy {
    Always,      // Tool Registry, Memory Engine — services critiques
    OnFailure,   // APIServer — redémarre seulement sur panique
    Never,       // One-shot actors
}

pub struct ChildSpec {
    pub restart_policy: RestartPolicy,
    pub restart_count: u32,
    pub max_restarts: u32,         // Défaut : 5
    pub restart_window_secs: u64,  // Défaut : 60s
}
```

Si un acteur dépasse `max_restarts` dans `restart_window_secs` : arrêt du runtime entier avec `exit(1)`. Le système préfère un arrêt net à un état incohérent.

---

## 3. AgentRegistry — Inventaire des agents

Source de vérité pour l'état de tous les agents actifs.

**Messages acceptés :**

| Message | Description |
|---|---|
| `Register(manifest)` | Enregistre un nouvel agent → retourne `AgentId` |
| `Unregister(agent_id)` | Supprime un agent du registre |
| `UpdateState(agent_id, state)` | Transition de `ProcessState` |
| `GetAgent(agent_id)` | Retourne `AgentEntry` ou `None` |
| `ListAgents(filter)` | Liste tous les agents (filtrable par `ProcessState`) |
| `ResolveSkill(skill_id)` | Résout un `skill_id` → `AgentEntry` via `SkillIndex` (Sprint 30) |

### 3.1 SkillIndex — Résolution A2A par skill_id (Sprint 30)

**Fichier** : `crates/apollia-runtime/src/registry.rs`

L'`AgentRegistry` intègre un `SkillIndex` — index inversé `skill_id → agent_name` alimenté automatiquement lors des `register()` / `unregister()` pour les agents avec `supports_a2a: true`.

```rust
pub enum SkillIndexError {
    SkillConflict { skill_id: String, existing_agent: String, new_agent: String },
    SkillNotFound  { skill_id: String, available: Vec<String> },
}
```

- **Fail-fast** : conflit de `skill_id` détecté au `Register()`, pas au runtime (Principe #4)
- **Unregister propre** : le `SkillIndex` est dépilé lors du `Unregister()`
- **Pas un acteur séparé** : le `SkillIndex` est un composant interne de l'`AgentRegistry` (Principe #5)

**Cycle de vie d'enregistrement :**

```
apollia-os agent start mon-agent.py
     │
     ▼
AgentRegistry.Register(manifest)      → INITIALIZING
     │
     ├── ToolResolver.resolve(tools_required)
     ├── MemoryManager.open(memory_namespace)
     └── AIPBridge.load_agent_module(path)
     │
     ▼ (succès)
ProcessState → ACTIVE
EventBus.broadcast(AgentReady(agent_id))
```

---

## 4. TaskRouter — Dispatch des tâches

Le TaskRouter reçoit toutes les requêtes de soumission (depuis l'APIServer) et les route vers le bon `ExecutionCoordinator`.

**Logique de routing :**

```
1. Vérifier ProcessState de l'agent cible
   - ACTIVE : passe
   - DEGRADED : passe avec warning EventBus
   - INITIALIZING : SubmitError::AgentNotReady
   - STOPPING/STOPPED : SubmitError::AgentUnavailable

2. Construire AIPTask (UUID, context_id, timeout depuis manifest)

3. Dispatcher vers ExecutionCoordinator de l'agent

4. Enregistrer dans pending_tasks pour tracking
```

---

## 5. ExecutionCoordinator — Un par agent

Chaque agent ACTIVE a son propre `ExecutionCoordinator`. C'est lui qui fait le pont entre le TaskRouter et l'ORIA Engine.

**Gestion de la concurrence :**

```rust
// Sémaphore Tokio — bloque si max_concurrent_tasks atteint
let permit = Arc::clone(&self.concurrency)
    .try_acquire_owned()
    .map_err(|_| CoordinatorError::ConcurrencyLimitReached)?;

let handle = tokio::spawn(async move {
    let _permit = permit;  // Libéré automatiquement à la fin du spawn

    event_bus.broadcast(RuntimeEvent::TaskStarted { agent_id, task_id }).await;
    let result = oria.execute(task).await;
    event_bus.broadcast(RuntimeEvent::TaskCompleted { agent_id, task_id, success }).await;

    result
});
```

Un agent PME typique est **séquentiel par défaut** (`max_concurrent_tasks=1`). Les agents batch peuvent déclarer jusqu'à N tâches parallèles dans leur manifest.

---

## 6. APIServer — Surface externe

Deux surfaces exposées :

| Surface | Adresse | Usage |
|---|---|---|
| Unix socket | `/tmp/apollia.sock` | CLI locale (plus rapide, sécurisé par permissions fichier) |
| HTTP/REST | `localhost:7771` | SDK Python, intégrations tierces, Apollia Workspace futur |

### 6.1 Endpoints REST

```
# STT [Sprint 24]
GET    /api/v1/stt/status                   → Statut moteur STT
POST   /api/v1/stt/transcribe               → Transcrire audio (multipart WAV/MP3)
GET    /api/v1/stt/transcriptions?limit=N   → Historique transcriptions
DELETE /api/v1/stt/transcriptions/:id       → Supprimer une transcription
GET    /api/v1/stt/models                   → Lister les modèles .bin disponibles

POST   /api/v1/tasks                        → Soumettre une tâche
GET    /api/v1/tasks/{id}                   → Statut d'une tâche
DELETE /api/v1/tasks/{id}                   → Annuler
GET    /api/v1/tasks/{id}/stream            → SSE streaming (si supports_streaming=True)

GET    /api/v1/agents                       → Lister les agents
POST   /api/v1/agents                       → Démarrer un agent
GET    /api/v1/agents/{id}                  → Détail d'un agent
DELETE /api/v1/agents/{id}                  → Arrêter un agent
GET    /api/v1/agents?supports_a2a=true     → Filtrer agents A2A [Sprint 30]

# A2A [Sprint 30]
GET    /api/v1/a2a/agents                   → Lister les AgentCards avec skills
GET    /api/v1/a2a/agents/{name}            → AgentCard d'un agent par nom
GET    /api/v1/a2a/skills                   → Lister tous les skills disponibles
POST   /api/v1/a2a/invoke                   → Invoquer un agent par skill_id
GET    /.well-known/agent.json              → AgentCard A2A standard (si un agent A2A est actif)

GET    /api/v1/tools                        → Lister les outils
GET    /api/v1/health                       → Santé du runtime
GET    /api/v1/audit                        → Log d'audit (filtrable)

# LLM [Sprint 8]
GET    /api/v1/llm/status                   → Statut backends LLM
POST   /api/v1/llm/ping                     → Test de connectivité
POST   /api/v1/llm/chat                     → Appel LLM direct

# Triggers [Sprint 9, CRUD Sprint 17]
POST   /api/v1/triggers                     → Créer un trigger [Sprint 17]
PUT    /api/v1/triggers/{id}                → Modifier un trigger [Sprint 17]
DELETE /api/v1/triggers/{id}                → Supprimer un trigger [Sprint 17]
GET    /api/v1/triggers                     → Lister les triggers
GET    /api/v1/triggers/{id}                → Définition/statut d'un trigger
POST   /api/v1/triggers/{id}/fire           → Déclencher immédiatement
POST   /api/v1/triggers/{id}/enable         → Activer
POST   /api/v1/triggers/{id}/disable        → Désactiver
GET    /api/v1/triggers/{id}/logs           → Historique SQLite
POST   /api/v1/triggers/reload              → Hot reload depuis SQLite

# Webhook [Sprint 9]
POST   /webhooks/:trigger_id                → Endpoint webhook HMAC-SHA256

# Dashboard [Sprint 9]
GET    /dashboard                           → Dashboard HTML embarqué
GET    /api/v1/dashboard/state              → Snapshot JSON état runtime
GET    /api/v1/dashboard/partials/{section} → Fragment HTML (HTMX)
GET    /api/v1/dashboard/stream             → SSE stream dashboard

# Pipelines CRUD [Sprint 17]
POST   /api/v1/pipelines                    → Créer un pipeline
PUT    /api/v1/pipelines/{id}               → Modifier un pipeline
DELETE /api/v1/pipelines/{id}               → Supprimer un pipeline
GET    /api/v1/pipelines/{id}               → Définition d'un pipeline

# Notifications CRUD [Sprint 17]
POST   /api/v1/notifications/channels       → Créer un canal
PUT    /api/v1/notifications/channels/{id}  → Modifier un canal
DELETE /api/v1/notifications/channels/{id}  → Supprimer un canal
GET    /api/v1/notifications/events         → Événements globaux
PUT    /api/v1/notifications/events         → Définir événements globaux

# Observabilité [Sprint 13]
GET    /api/v1/tasks/{id}/timeline          → Chronologie unifiée (5 sources SQLite)
```

### 6.2 Streaming SSE

Pour les agents avec `supports_streaming=True` :

```
GET /api/v1/tasks/{id}/stream
Content-Type: text/event-stream

data: {"event": "step", "step": 1, "thought": "Je recherche les infos client..."}
data: {"event": "tool_call", "tool": "file_io", "input": "clients/dupont.json"}
data: {"event": "observation", "output": "{siret: ...}"}
data: {"event": "completed", "result": {...}}
```

L'`EventBus` interne alimente les streams SSE. L'`ExecutionCoordinator` émet des événements progressifs, l'`APIServer` les consomme et les pousse aux clients abonnés.

---

## 7. EventBus — Découplage interne

```rust
pub enum RuntimeEvent {
    // Lifecycle agents
    AgentRegistered(AgentId),
    AgentReady(AgentId),
    AgentDegraded { agent_id: AgentId, reason: String },
    AgentStopped(AgentId),

    // Lifecycle tâches
    TaskStarted { agent_id: AgentId, task_id: TaskId },
    TaskCompleted { agent_id: AgentId, task_id: TaskId, success: bool },
    TaskCanceled { task_id: TaskId },

    // Exécution
    StepExecuted { task_id: TaskId, step: u32, tool: Option<String> },
    ToolCircuitBroken { tool_name: String },
    ToolCircuitRestored { tool_name: String },

    // LLM [Sprint 8]
    LlmModelLoading { backend: String },
    LlmModelReady   { backend: String },
    LlmModelFailed  { backend: String, error: String },
    LlmCallCompleted { backend: String, model: String, cost_usd: Option<f64> },

    // STT [Sprint 24]
    SttRecordingStarted,
    SttRecordingStopped { audio_duration_ms: u64 },
    SttModelLoaded { backend: String, model_path: String, model_name: String },
    SttTranscribed { text: String, language: Option<String>, source: String, duration_ms: u64, processing_time_ms: u64 },
    SttTranscriptionFailed { reason: String },

    // Triggers [Sprint 9]
    TriggerFired    { trigger_id: TriggerId, agent: String, task_id: TaskId },
    TriggerSkipped  { trigger_id: TriggerId, reason: String },
    TriggerError    { trigger_id: TriggerId, error: String },
    TriggerEnabled  { trigger_id: TriggerId },
    TriggerDisabled { trigger_id: TriggerId },
    TriggersReloaded { count: usize },

    // A2A [Sprint 30 + Sprint 32]
    A2AInvocationStarted { caller: String, skill_id: String, target: String },
    A2AInvocationCompleted { caller: String, skill_id: String, duration_ms: u64, success: bool },
    A2AGuardTriggered { guard_type: String, caller: String, skill_id: String, detail: String },

    // Système
    AllReady,
    FatalError(String),
    ShutdownRequested,
}
```

Basé sur `tokio::sync::broadcast` — abonnement multiple, non-bloquant, buffer borné (1024 événements).

---

## 8. Graceful Shutdown

```
SIGTERM / SIGINT / apollia-os stop
     │
     ▼
EventBus.broadcast(ShutdownRequested)
     │
     ▼
APIServer : refuse nouvelles connexions
     │
     ▼
TaskRouter : refuse nouvelles tâches (SubmitError::ShuttingDown)
     │
     ▼
Pour chaque agent ACTIVE → ProcessState = STOPPING
  ├── Drain des tâches en cours (timeout: 30s)
  ├── on_stop() callback Python
  └── ProcessState = STOPPED
     │
     ▼
Memory Engine → flush SQLite + fermeture connexions
Tool Registry → fermeture connexions MCP ouvertes
     │
     ▼
Supervisor → arrêt tous acteurs Tokio
     │
     ▼
exit(0)
```

**Timeout de drain : 30s.** Si une tâche n'est pas terminée après 30s, elle est annulée (`CANCELED`) et tracée dans l'audit log.

---

## 9. Configuration `apollia.toml`

```toml
[runtime]
socket_path        = "/tmp/apollia.sock"
api_port           = 7771
max_concurrent_agents = 10
shutdown_drain_timeout_secs = 30

[oria]
max_steps          = 10
max_tool_calls     = 20
wall_clock_timeout = 300
max_replans        = 2

[memory]
base_path          = "~/.apollia/memory"
episodic_ttl_days  = 90
purge_on_startup   = true
embedding_strategy = "auto"
gguf_model_path    = ""
ollama_url         = ""

[tools]
sandbox_base_path  = "~/.apollia/sandboxes"
audit_log_path     = "~/.apollia/audit.db"

[logging]
level              = "info"
format             = "text"
path               = "~/.apollia/runtime.log"

[a2a]                                        # Sprint 32
max_depth                 = 3                # Profondeur max chaîne A2A (défaut : 3)
invocation_timeout_secs   = 120              # Timeout par invocation A2A (défaut : 120)
chain_timeout_secs        = 300              # Budget cumulé chaîne A2A (défaut : 300)

[observability]                              # Sprint 13
max_input_bytes       = 32768               # troncature input tâches/steps (32 KB)
max_output_bytes      = 32768               # troncature output tâches/steps (32 KB)
max_tool_output_bytes = 10240               # troncature stdout/stderr outils (10 KB)
debug_log_prompt      = false               # persister les prompts LLM (RGPD — false par défaut)
```

---

## 10. Décisions architecturales clés

| Décision | Justification |
|---|---|
| Acteurs Tokio (pas god object) | Testabilité, isolation des paniques, restart granulaire |
| Démarrage séquentiel avec signal Ready | Pas de race condition, erreurs précoces claires |
| Un ExecutionCoordinator par agent | Panique d'un agent n'affecte pas les autres |
| Concurrence = 1 tâche/agent par défaut | Comportement déterministe, PME n'a pas besoin de parallélisme implicite |
| REST JSON (pas gRPC) | Debuggable avec curl, pas de génération protobuf, CLI simple |
| SSE pour streaming | Unidirectionnel suffisant, compatible tout client HTTP |
| Graceful shutdown avec drain 30s | Jamais de tâche perdue silencieusement |
| `apollia.toml` structurel + SQLite opérationnel | TOML pour la config immuable, SQLite pour les triggers/pipelines/notifications CRUD (ADR-033) |
| HITL via `oneshot` channel dans ORIA | Suspension sans polling, reprise déterministe via `ResumeHandler` (ADR-023) |
| `TimeoutWatcher` scan 60s | Tâches orphelines nettoyées automatiquement sans intervention utilisateur |
| `ChatSessionManager` séparé du `TaskRouter` (Phase 13) | Chat = sessions longues stateful, TaskRouter = fire-and-forget stateless. Sémantiques incompatibles (ADR-034) |
| `NotificationEngine` optionnel (Phase 9) | Zéro overhead si `[notifications]` absent — runtime léger par défaut |
| Timeline API agrégée server-side (ADR-026) | 5 sources SQLite lues en parallèle, triées, retournées en JSON — pas de calcul client |
| Troncature configurable `ObservabilityConfig` (ADR-026) | UTF-8 safe, marqueur `[TRONQUÉ — N octets total]`, jamais de rejet — observabilité partielle > aucune |
| `SkillIndex` dans `AgentRegistry` (Sprint 30, ADR-049) | Index inversé skill_id → agent_name — pas un acteur séparé, cohérence garantie par le même acteur que l'état agent (Principe #5) |
| `A2AInvoker` timeout 120s (Sprint 30) | Invocations A2A synchrones — timeout explicite évite que le Director Agent soit bloqué indéfiniment si le Worker Agent plante |
| Auto-installation des agents bundled (Sprint 32, ADR-050) | 4 agents bundled auto-installés au premier boot via `agents/bundled/manifest.json` — idempotent (pas de réinstallation si déjà présent) |
| `A2AToolsProvider` (Sprint 32) | Injecte dynamiquement les skills A2A comme outils virtuels `a2a:{skill_id}` dans la boucle ReAct ORIA — backward-compatible (sans agents A2A = pas de changement) |
| Garde-fous A2A (Sprint 32) | `max_depth`, `chain_timeout`, self-invocation — trois protections runtime non contournables pour les chaînes A2A (Principe #7) |

---

## 11. Diagrammes de référence

- [Démarrage ordonné Supervisor](../diagrams/seq-supervisor-startup.puml) — 13 phases, TriggerEngine → NotificationEngine → ChatSessionManager
- [CRUD Config opérationnelle](../diagrams/seq-config-crud.puml) — POST → SQLite → Engine.reload() (Sprint 17, ADR-033)
- [HITL Flow complet](../diagrams/seq-hitl-flow.puml) — suspend → notify → approve/reject → resume
- [Task Lifecycle](../diagrams/seq-task-lifecycle.puml) — flux complet soumission → résultat
- [Timeline Aggregation](../diagrams/seq-timeline-aggregation.puml) — agrégation 5 sources → chronologie unifiée
- [Chat Libre sequence](../diagrams/seq-chat-libre.puml) — boucle ReAct + streaming token-by-token (Sprint 18)
- [Chat session state machine](../diagrams/state-chat-session.puml) — Active → Processing → Closed (Sprint 18)
- [STT Flow](../diagrams/seq-stt-flow.puml) — hotkey → capture → transcribe → clipboard (Sprint 24)
- [A2A Guards sequence](../diagrams/seq-a2a-guards.puml) — garde-fous invocation A2A : depth, self-invocation, chain timeout (Sprint 32)
