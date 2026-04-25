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

Basé sur `tokio::sync::broadcast` — abonnement multiple, non-bloquant, buffer borné (1024 événements). Défini dans `apollia-core` pour éviter les dépendances circulaires : toutes les crates importent `RuntimeEvent` sans créer de cycle.

### 7.1 Agents — Lifecycle

```rust
/// Agent enregistré dans le Registry (état: Initializing).
AgentRegistered(AgentId),

/// Agent initialisé, opérationnel (état: Active).
AgentReady(AgentId),

/// Agent passé en état dégradé (outil manquant, erreur Python, etc.).
AgentDegraded { agent_id: AgentId, reason: String },

/// Agent en cours d'arrêt (drain des tâches, état: Stopping).
AgentStopping(AgentId),

/// Agent arrêté proprement.
AgentStopped(AgentId),
```

### 7.2 Agents — Installation *(Sprint 32)*

```rust
/// Chargement d'un agent installé échoué au boot (runtime continue, dégradation gracieuse).
AgentLoadFailed { name: String, error: String },

/// Agent installé de façon permanente.
AgentInstalled { name: String, version: String },

/// Agent installé supprimé.
AgentUninstalled { name: String },

/// Agent activé pour l'auto-start au boot.
AgentEnabled { name: String },

/// Agent désactivé (ne sera plus chargé au boot).
AgentDisabled { name: String },
```

### 7.3 Tâches — Lifecycle

```rust
/// Tâche démarrée sur un agent.
TaskStarted { agent_id: AgentId, task_id: TaskId },

/// Tâche terminée (succès ou échec).
/// Le champ `output` contient la sortie texte sur succès ; `None` sur échec.
TaskCompleted {
    agent_id: AgentId,
    task_id:  TaskId,
    success:  bool,
    output:   Option<String>,
},

/// Tâche annulée.
TaskCanceled { task_id: TaskId },
```

### 7.4 Tâches — HITL *(Sprint 11)*

```rust
/// Tâche suspendue en attente d'une décision humaine.
/// `step_id` est `Some` en mode orchestré (step spécifique), `None` en mode direct.
TaskInputRequired { task_id: TaskId, prompt: String, step_id: Option<String> },

/// Tâche reprise après décision humaine.
TaskResumed { task_id: TaskId, approved: bool },

/// Tâche `input_required` expirée — annulée automatiquement par le TimeoutWatcher.
/// Suivi immédiatement de `TaskCanceled` pour la même tâche.
TaskApprovalTimeout { task_id: TaskId, after_secs: u64 },
```

### 7.5 Exécution réactive — Step legacy

```rust
/// Un step de la boucle ReAct a été exécuté (mode direct).
/// Champ `tool` est `None` si l'agent a seulement réfléchi sans appeler d'outil.
StepExecuted { task_id: TaskId, step: u32, tool: Option<String> },
```

### 7.6 Plan / Step — Mode orchestré *(Sprint 10)*

```rust
/// Plan généré par le Reasoner et persisté en SQLite.
PlanGenerated {
    task_id:    TaskId,
    agent_name: String,
    plan_id:    String,
    step_count: usize,
},

/// Step démarré (émis par ActorLoop avant chaque appel outil ou LLM).
StepStarted {
    task_id:  TaskId,
    plan_id:  String,
    step_id:  String,
    step_num: usize,   // 1-based
    total:    usize,
    desc:     String,
},

/// Step terminé avec succès.
StepCompleted {
    task_id:     TaskId,
    plan_id:     String,
    step_id:     String,
    duration_ms: u64,
},

/// Step échoué.
StepFailed {
    task_id:   TaskId,
    plan_id:   String,
    step_id:   String,
    error:     String,
    retryable: bool,   // true = peut déclencher une replanification
},

/// Replanification déclenchée après un step retryable échoué.
PlanReplanning {
    task_id:     TaskId,
    plan_id:     String,
    attempt:     u32,    // 1-based, max MAX_REPLANS=2
    failed_step: String,
    reason:      String,
},

/// Tous les steps complétés — plan terminé avec succès.
PlanCompleted {
    task_id:     TaskId,
    plan_id:     String,
    step_count:  usize,
    duration_ms: u64,
},

/// Plan échoué de manière irrémédiable (MAX_REPLANS dépassé ou erreur permanente).
PlanFailed { task_id: TaskId, plan_id: String, reason: String },
```

### 7.7 Plan Cache *(Sprint 20)*

```rust
/// Plan récupéré depuis le cache au lieu d'être régénéré par le Reasoner.
PlanCacheHit { task_id: TaskId, cache_key: String },
```

### 7.8 Outils — Circuit Breaker

```rust
/// Circuit breaker d'un outil ouvert (seuil d'échecs dépassé).
ToolCircuitBroken { tool_name: String },

/// Circuit breaker d'un outil refermé après recovery (HalfOpen → Closed).
ToolCircuitRestored { tool_name: String },
```

### 7.9 LLM *(Sprint 8 + 28)*

```rust
/// Backend LLM en cours de chargement (avant load() ou init HTTP).
LlmModelLoading { backend: String, model_path: String },

/// Backend LLM prêt — modèle chargé ou connexion cloud vérifiée.
LlmModelReady { backend: String, model_id: String },

/// Chargement d'un backend LLM échoué — backend ignoré, runtime continue.
LlmModelFailed { backend: String, reason: String },

/// Appel LLM terminé (émis par complete_with_observability()).
LlmCallCompleted {
    backend:           String,
    model:             String,
    task_id:           Option<String>,   // None hors contexte task
    step_id:           Option<String>,   // None en mode direct
    prompt_tokens:     u32,
    completion_tokens: u32,
    latency_ms:        u64,
    cost_usd:          Option<f64>,      // None = inférence locale
},
```

### 7.10 Triggers *(Sprint 9)*

```rust
/// Trigger déclenché — tâche soumise au TaskRouter.
TriggerFired    { trigger_id: String, agent: String, task_id: TaskId },

/// Trigger ignoré (OnBusyPolicy::Drop ou agent occupé).
TriggerSkipped  { trigger_id: String, reason: String },

/// Erreur lors du traitement d'un trigger.
TriggerError    { trigger_id: String, error: String },

/// Trigger activé via CLI ou API.
TriggerEnabled  { trigger_id: String },

/// Trigger désactivé via CLI ou API.
TriggerDisabled { trigger_id: String },

/// TriggerEngine rechargé (hot reload ou démarrage).
TriggersReloaded { count: usize },
```

### 7.11 Pipelines *(Sprint 12)*

```rust
/// Run de pipeline démarré.
PipelineStarted {
    run_id:      String,
    pipeline_id: String,
    trigger_id:  Option<String>,  // None si démarrage manuel
    step_count:  usize,
},

/// Step de pipeline soumis au TaskRouter.
PipelineStepStarted  { run_id: String, step_id: String, task_id: String, agent: String },

/// Step de pipeline terminé avec succès.
PipelineStepCompleted { run_id: String, step_id: String },

/// Step de pipeline échoué.
PipelineStepFailed {
    run_id:     String,
    step_id:    String,
    reason:     String,
    on_failure: String,   // "skip" | "fallback" | "fail"
},

/// Step de pipeline sauté (condition=false ou on_failure=skip).
PipelineStepSkipped  { run_id: String, step_id: String, reason: String },

/// Pipeline suspendu en attente d'une approbation HITL.
PipelineSuspended    { run_id: String, step_id: String, task_id: String },

/// Pipeline repris après approbation HITL.
PipelineResumed      { run_id: String, step_id: String },

/// Tous les steps complétés — pipeline terminé avec succès.
PipelineCompleted    { run_id: String, pipeline_id: String, duration_ms: u64 },

/// Pipeline échoué suite à un step avec on_failure=fail.
PipelineFailed       { run_id: String, pipeline_id: String, step_id: String, reason: String },
```

### 7.12 Chat *(Sprint 18)*

```rust
/// Session de chat créée.
ChatSessionCreated  { session_id: String, mode: String, agent_name: Option<String> },

/// Session de chat fermée.
ChatSessionClosed   { session_id: String },

/// Message utilisateur envoyé dans une session.
ChatMessageSent     { session_id: String, message_id: String },

/// Runtime commence à générer une réponse.
ChatResponseStarted { session_id: String, message_id: String },

/// Token LLM produit en streaming (Chat Libre uniquement).
ChatToken           { session_id: String, message_id: String, token: String },

/// Réponse complète générée.
ChatResponseCompleted { session_id: String, message_id: String, content: String },

/// Erreur dans une session de chat.
ChatError           { session_id: String, message_id: Option<String>, error: String },

/// Appel outil démarré dans une session de chat.
ChatToolCallStarted {
    session_id:    String,
    message_id:    String,
    tool_name:     String,
    input_preview: String,   // tronqué
},

/// Appel outil terminé dans une session de chat.
ChatToolCallCompleted {
    session_id:     String,
    message_id:     String,
    tool_name:      String,
    success:        bool,
    output_preview: Option<String>,   // tronqué
},

/// Approbation humaine requise pour un appel outil dans le chat.
ChatApprovalRequired {
    session_id: String,
    message_id: String,
    tool_name:  String,
    prompt:     String,
},

/// Décision prise par l'utilisateur sur un appel outil.
/// `decision` vaut `"accept"`, `"refuse"` ou `"always_accept"`.
ChatApprovalResolved {
    session_id: String,
    message_id: String,
    tool_name:  String,
    decision:   String,
},

/// Approbation expirée (timeout sans décision utilisateur).
ChatApprovalTimeout  { session_id: String, message_id: String, tool_name: String },
```

### 7.13 Messagerie inter-agents *(Sprint 20)*

```rust
/// Message envoyé entre deux agents via AgentMailbox.
AgentMessageSent { from: String, to: String },
```

### 7.14 A2A — Invocation *(Sprint 30 + 32)*

```rust
/// Invocation A2A démarrée (émis avant soumission de la tâche au TaskRouter).
A2AInvocationStarted {
    caller:   String,   // nom du Director Agent
    target:   String,   // nom du Worker Agent résolu
    skill_id: String,
},

/// Invocation A2A terminée.
/// `status` vaut `"completed"` ou `"failed"`.
A2AInvocationCompleted {
    caller:      String,
    target:      String,
    skill_id:    String,
    status:      String,
    duration_ms: u64,
},
```

### 7.15 A2A — Garde-fous *(Sprint 32)*

```rust
/// Garde-fou A2A déclenché — invocation bloquée avant soumission.
/// `guard_type` vaut `"max_depth"`, `"self_invocation"` ou `"chain_timeout"`.
A2AGuardTriggered {
    guard_type: String,
    caller:     String,
    skill_id:   String,
    detail:     String,
},
```

### 7.16 STT *(Sprint 24)*

```rust
/// Enregistrement audio démarré (hotkey activée).
SttRecordingStarted,

/// Enregistrement audio arrêté (hotkey relâchée ou silence détecté).
SttRecordingStopped { audio_duration_ms: u64 },

/// Modèle STT chargé avec succès — moteur opérationnel.
SttModelLoaded { backend: String, model_path: String, model_name: String },

/// Transcription terminée avec succès.
/// `source` vaut `"hotkey"`, `"file"` ou `"api"`.
SttTranscribed {
    text:                String,
    language:            Option<String>,
    source:              String,
    duration_ms:         u64,
    processing_time_ms:  u64,
},

/// Erreur de transcription STT.
SttTranscriptionFailed { reason: String },
```

### 7.17 Onboarding *(Sprint 18)*

```rust
/// Premier lancement détecté — UserMemory vide.
/// Le frontend intercepte cet événement via SSE pour afficher l'écran d'accueil.
OnboardingRequired,

/// Session d'onboarding déclenchée.
/// `mode` vaut `"full"` ou `"partial"` ; `topic` précise le domaine en mode partial.
OnboardingStarted { session_id: String, mode: String, topic: Option<String> },
```

### 7.18 Système

```rust
/// Tous les composants prêts — runtime opérationnel.
AllReady,

/// Arrêt demandé (SIGTERM, SIGINT ou commande CLI).
ShutdownRequested,

/// Erreur fatale non récupérable.
FatalError(String),
```

---

### Récapitulatif par catégorie

| Catégorie | Variants | Sprint |
|---|---|---|
| Agent lifecycle | `AgentRegistered`, `AgentReady`, `AgentDegraded`, `AgentStopping`, `AgentStopped` | Cœur |
| Agent install | `AgentLoadFailed`, `AgentInstalled`, `AgentUninstalled`, `AgentEnabled`, `AgentDisabled` | 32 |
| Task lifecycle | `TaskStarted`, `TaskCompleted`, `TaskCanceled` | Cœur |
| Task HITL | `TaskInputRequired`, `TaskResumed`, `TaskApprovalTimeout` | 11 |
| Step legacy | `StepExecuted` | Cœur |
| Plan / Step | `PlanGenerated`, `StepStarted`, `StepCompleted`, `StepFailed`, `PlanReplanning`, `PlanCompleted`, `PlanFailed` | 10 |
| Plan Cache | `PlanCacheHit` | 20 |
| Outils | `ToolCircuitBroken`, `ToolCircuitRestored` | Cœur |
| LLM | `LlmModelLoading`, `LlmModelReady`, `LlmModelFailed`, `LlmCallCompleted` | 8 / 28 |
| Triggers | `TriggerFired`, `TriggerSkipped`, `TriggerError`, `TriggerEnabled`, `TriggerDisabled`, `TriggersReloaded` | 9 |
| Pipelines | `PipelineStarted`, `PipelineStepStarted`, `PipelineStepCompleted`, `PipelineStepFailed`, `PipelineStepSkipped`, `PipelineSuspended`, `PipelineResumed`, `PipelineCompleted`, `PipelineFailed` | 12 |
| Chat | `ChatSessionCreated`, `ChatSessionClosed`, `ChatMessageSent`, `ChatResponseStarted`, `ChatToken`, `ChatResponseCompleted`, `ChatError`, `ChatToolCallStarted`, `ChatToolCallCompleted`, `ChatApprovalRequired`, `ChatApprovalResolved`, `ChatApprovalTimeout` | 18 |
| Agent messaging | `AgentMessageSent` | 20 |
| A2A invocation | `A2AInvocationStarted`, `A2AInvocationCompleted` | 30 |
| A2A gardes-fous | `A2AGuardTriggered` | 32 |
| STT | `SttRecordingStarted`, `SttRecordingStopped`, `SttModelLoaded`, `SttTranscribed`, `SttTranscriptionFailed` | 24 |
| Onboarding | `OnboardingRequired`, `OnboardingStarted` | 18 |
| Système | `AllReady`, `ShutdownRequested`, `FatalError` | Cœur |

**Total : 75 variants** (source de vérité : `crates/apollia-core/src/events.rs`)

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

## 11. Session Tool Access Control + Conversation Forking — Sprint 36

### `SessionConfig` — Contrôle d'accès aux outils (STORY-491)

Depuis le Sprint 36, `SessionConfig` supporte un allow-list et un deny-list par session CLI, sans modifier la config globale.

```rust
pub struct SessionConfig {
    // ...champs existants...
    /// None = tous les outils autorisés. Some(vec) = liste restrictive.
    pub allowed_tools: Option<Vec<String>>,
    /// Outils toujours refusés pour cette session.
    pub disallowed_tools: Vec<String>,
}
```

`disallowed_tools` a **priorité absolue** sur `allowed_tools` en cas de conflit.

Nouvelle variante `ToolError` :
```rust
#[error("tool not allowed for this session: {tool_name}")]
ToolNotAllowed { tool_name: String },
```

### `ChatSession::fork()` — Forking de conversations (STORY-492)

```rust
impl ChatSession {
    /// Crée une session fille avec une copie de l'historique jusqu'à `up_to_index`.
    pub async fn fork(
        &self,
        up_to_index: Option<usize>,
        repo: &SessionRepository,
    ) -> Result<ChatSession, RuntimeError> { ... }
}
```

**Migration SQLite :**
```sql
ALTER TABLE chat_sessions ADD COLUMN parent_session_id TEXT REFERENCES chat_sessions(id);
ALTER TABLE chat_sessions ADD COLUMN fork_depth INTEGER DEFAULT 0;
CREATE INDEX IF NOT EXISTS idx_sessions_parent ON chat_sessions(parent_session_id);
```

Les sessions filles sont **indépendantes** — ajouter un message dans la fille ne modifie pas le parent.

### `CommandRegistry` — Slash commands custom (STORY-493)

```rust
/// Charge les commandes custom depuis :
/// 1. {CWD}/.apollia/commands/*.md (priorité)
/// 2. ~/.apollia/commands/*.md
pub struct CommandRegistry {
    commands: HashMap<String, CustomCommand>,
}

pub struct CustomCommand {
    pub name: String,
    pub description: String,
    pub prompt_template: String,
    pub args: Vec<String>,       // Variables {{arg}} dans le template
}

impl CommandRegistry {
    pub async fn load(cwd: &Path) -> Self { ... }
    pub fn get(&self, name: &str) -> Option<&CustomCommand> { ... }
    pub fn list(&self) -> Vec<&CustomCommand> { ... }  // trié alphabétiquement
}
```

Hot reload via `FileTimestampCache` (Sprint 36, STORY-476) si le répertoire `.apollia/commands/` est modifié.

---

## 12. Diagrammes de référence

- [Démarrage ordonné Supervisor](https://github.com/nidal-z/apollia-os/blob/main/docs/diagrams/seq-supervisor-startup.puml) — 13 phases, TriggerEngine → NotificationEngine → ChatSessionManager
- [CRUD Config opérationnelle](https://github.com/nidal-z/apollia-os/blob/main/docs/diagrams/seq-config-crud.puml) — POST → SQLite → Engine.reload() (Sprint 17, ADR-033)
- [HITL Flow complet](https://github.com/nidal-z/apollia-os/blob/main/docs/diagrams/seq-hitl-flow.puml) — suspend → notify → approve/reject → resume
- [Task Lifecycle](https://github.com/nidal-z/apollia-os/blob/main/docs/diagrams/seq-task-lifecycle.puml) — flux complet soumission → résultat
- [Timeline Aggregation](https://github.com/nidal-z/apollia-os/blob/main/docs/diagrams/seq-timeline-aggregation.puml) — agrégation 5 sources → chronologie unifiée
- [Chat Libre sequence](https://github.com/nidal-z/apollia-os/blob/main/docs/diagrams/seq-chat-libre.puml) — boucle ReAct + streaming token-by-token (Sprint 18)
- [Chat session state machine](https://github.com/nidal-z/apollia-os/blob/main/docs/diagrams/state-chat-session.puml) — Active → Processing → Closed (Sprint 18)
- [STT Flow](https://github.com/nidal-z/apollia-os/blob/main/docs/diagrams/seq-stt-flow.puml) — hotkey → capture → transcribe → clipboard (Sprint 24)
- [A2A Guards sequence](https://github.com/nidal-z/apollia-os/blob/main/docs/diagrams/seq-a2a-guards.puml) — garde-fous invocation A2A : depth, self-invocation, chain timeout (Sprint 32)
