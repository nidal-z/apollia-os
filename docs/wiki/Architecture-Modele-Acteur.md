# Architecture — Modèle Acteur Tokio — Apollia OS

> Comment le Runtime Core est structuré en acteurs Tokio indépendants, et pourquoi ce modèle garantit l'absence d'état partagé.
> Public cible : contributeur Rust, architecte

---

## Vue d'ensemble

Le Runtime Core d'Apollia OS est composé de huit acteurs Tokio. Un acteur est une tâche Tokio qui possède exclusivement son état interne et communique avec l'extérieur uniquement via des messages sur un canal `mpsc`. Zéro `Arc<Mutex<T>>` entre acteurs.

Ce modèle s'inspire du pattern acteur documenté par Alice Ryhl (blog Tokio). Il force la séparation des responsabilités par construction : il est architecturalement impossible pour un acteur de modifier l'état d'un autre sans passer par un message.

---

## Les 8 acteurs

### 1. EventBus

**Rôle :** diffuser les événements système à tous les abonnés.

**État interne :** canal `broadcast::Sender<RuntimeEvent>`.

**Messages entrants :** `Publish(RuntimeEvent)`, `Subscribe` (retourne un `Receiver`).

```rust
// Événements représentatifs
RuntimeEvent::TaskSubmitted { task_id, agent_id }
RuntimeEvent::TaskCompleted { task_id, output }
RuntimeEvent::AgentTransitioned { agent_id, from, to }
RuntimeEvent::ToolCircuitRestored { tool_name }
RuntimeEvent::ShutdownRequested
```

### 2. AgentRegistry

**Rôle :** inventaire des agents déployés et de leur `ProcessState`.

**État interne :** `HashMap<AgentId, AgentRecord>` + index `HashMap<String, AgentId>` par nom.

**Messages entrants :** `Register`, `Transition`, `GetById`, `GetByName`, `List`, `Remove`.

**Transitions valides :**
```
INITIALIZING → ACTIVE
INITIALIZING → STOPPED   (échec au démarrage)
ACTIVE → DEGRADED
ACTIVE → STOPPING
DEGRADED → STOPPING
STOPPING → STOPPED
```

### 3. TaskRouter

**Rôle :** réceptionner les demandes de tâches et les dispatcher vers le bon `ExecutionCoordinator`.

**État interne :** `HashMap<AgentId, ExecutionCoordinator>` + `HashMap<TaskId, TaskState>`.

**Messages entrants :** `Submit`, `GetStatus`, `Cancel`, `GetActiveTasks`, `RegisterCoordinator`, `UnregisterCoordinator`, `Shutdown`.

**Comportement :**
- Vérifie `ProcessState` via `AgentRegistryHandle` avant dispatch
- Émet `TaskSubmitted` sur EventBus après acceptation
- Émet `AgentDegraded` warning pour les agents en état DEGRADED

### 4. ExecutionCoordinator

**Rôle :** gérer la concurrence par agent via un semaphore Tokio.

**Un `ExecutionCoordinator` par agent actif.**

**État interne :** `Semaphore` (capacité = `max_concurrent_tasks`).

**Comportement :**
- `submit_task()` : `try_acquire_owned()` non-bloquant — retourne `CapacityExceeded` si plein
- `tokio::spawn` : la permit est movée dans la closure, droppée à la fin de la tâche
- Émet `TaskCompleted` ou `TaskFailed` via EventBus (fire-and-forget)

### 5. APIServer

**Rôle :** exposer l'API REST locale sur Unix socket et TCP.

**État interne :** `AppState<B>` partagé entre handlers via `Arc` (read-only après init).

**Double écoute :**
- TCP `0.0.0.0:7771` via `axum::serve()`
- Unix socket `/tmp/apollia.sock` via `hyper-util` boucle accept manuelle (ADR-017)

**Shutdown :** via `watch::channel` — `graceful_shutdown()` signal propre.

### 6. Supervisor

**Rôle :** démarrage ordonné, healthcheck et restart policy des acteurs.

**Séquence de démarrage :**
```
EventBus → AgentRegistry → TaskRouter → APIServer
```

**RestartPolicy par acteur :**

| Acteur | Policy | Raison |
|---|---|---|
| EventBus | Always | Critique — canal central |
| AgentRegistry | Always | Critique — état des agents |
| TaskRouter | Always | Critique — dispatch |
| APIServer | OnFailure | Redémarrable en cas de bind error |

**Rollback :** si l'APIServer échoue au démarrage, tous les acteurs précédemment démarrés sont arrêtés en ordre inverse.

### 7. ChatSessionManager *(Sprint 18)*

**Rôle :** gérer les sessions de chat interactif (Chat Libre et Chat Agent).

**État interne :** `HashMap<String, ChatSession>` + `ChatSessionRepository` (SQLite) + `PendingChatApprovals`.

**Messages entrants :** `CreateSession`, `SendMessage`, `ResolveTool`, `ListSessions`, `GetSession`, `CloseSession`, `Shutdown`.

**Comportement :**
- Chat Libre : boucle ReAct Rust native via `BuiltInChatAgent` avec streaming token-by-token
- Chat Agent : délègue à `AIPBridge.call_run()` (agent Python installé)
- HITL inline : tous les outils requièrent approbation (Accept/Refuse/AlwaysAccept)
- Persistance `chat.db` SQLite (sessions, messages, autorisations)
- Chemin d'exécution séparé du `TaskRouter` (ADR-034)

### 8. AgentMailbox *(Sprint 20)*

**Rôle :** gérer la messagerie inter-agents (agent-to-agent communication).

**État interne :** `HashMap<String, VecDeque<AgentMessage>>` — file de messages par agent (max 100 par agent).

**Messages entrants :** `Send(from, to, payload)`, `Receive(agent_name, timeout)`, `PendingCount(agent_name)`, `ListMessages(agent_name, limit)`, `Shutdown`.

**Comportement :**
- `send()` : ajoute un message à la file de l'agent destinataire — erreur `MailboxError::QueueFull` si la file atteint 100 messages
- `receive()` : retourne le plus ancien message non-lu pour un agent (FIFO), avec timeout optionnel
- Émet `RuntimeEvent::AgentMessageSent` sur EventBus après chaque envoi

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentMessage {
    pub from: String,
    pub payload: serde_json::Value,
    pub sent_at: String,
}

#[derive(Clone)]
pub struct AgentMailboxHandle { /* mpsc::Sender */ }

impl AgentMailboxHandle {
    pub fn spawn(event_bus: EventBusSender) -> Self;
    pub async fn send(&self, from: &str, to: &str, payload: Value) -> Result<(), MailboxError>;
    pub async fn receive(&self, agent_name: &str, timeout: Duration) -> Option<AgentMessage>;
    pub async fn pending_count(&self, agent_name: &str) -> usize;
    pub async fn list_messages(&self, agent_name: &str, limit: usize) -> Vec<AgentMessage>;
}
```

---

## Pattern Handle

Chaque acteur expose un `Handle` clonable — c'est l'unique interface publique vers l'acteur.

```rust
// Pattern standard — AgentRegistry comme exemple
pub struct AgentRegistry {
    agents: HashMap<AgentId, AgentRecord>,
    name_index: HashMap<String, AgentId>,
    event_tx: broadcast::Sender<RuntimeEvent>,
}

pub struct AgentRegistryHandle {
    tx: mpsc::Sender<RegistryMessage>,
}

impl AgentRegistryHandle {
    pub fn start(event_tx: broadcast::Sender<RuntimeEvent>) -> Self {
        let (tx, rx) = mpsc::channel(1024);
        tokio::spawn(AgentRegistry::run(rx, event_tx));
        Self { tx }
    }

    pub async fn register(&self, manifest: AgentManifest) -> Result<AgentId, AgentRegistryError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx.send(RegistryMessage::Register { manifest, reply: reply_tx }).await?;
        reply_rx.await?
    }
}

impl Clone for AgentRegistryHandle {
    fn clone(&self) -> Self {
        Self { tx: self.tx.clone() }
    }
}
```

Le `Handle` est `Clone + Send + Sync`. Il peut être partagé librement entre threads, entre handlers axum, entre tâches Tokio — sans jamais exposer l'état interne de l'acteur.

---

## Séquence de démarrage complète

```
apollia-os start
    │
    ▼
Supervisor::start()
    │
    ├── 1. EventBus::start() → EventBusHandle
    │        └── broadcast::channel(1024)
    │
    ├── 2. AgentRegistry::start(event_tx) → AgentRegistryHandle
    │        └── mpsc::channel(1024) + tokio::spawn(run loop)
    │
    ├── 3. TaskRouter::start(registry, event_tx) → TaskRouterHandle
    │        └── mpsc::channel(1024) + tokio::spawn(run loop)
    │        └── s'abonne aux événements EventBus (tokio::select!)
    │
    └── 4. APIServer::start(state, config) → APIServerHandle
             └── bind TCP 7771 + Unix socket
             └── axum::serve() + boucle accept hyper-util
    │
    └── … 5-12. (LlmRouter, TriggerEngine, PipelineEngine,
    │        NotificationEngine…)
    │
    └── 12c. AgentMailbox::spawn(event_tx)
             └── files de messages per-agent, max 100
    │
    └── 13. ChatSessionManager::spawn(event_tx, llm, tools)
             └── ouvre chat.db, restaure autorisations

Supervisor::watch() → attend ShutdownRequested ou FatalError
```

---

## Règle absolue : zéro état partagé entre acteurs

```rust
// ❌ INTERDIT — Arc<Mutex<T>> entre acteurs
struct BadRouter {
    registry: Arc<Mutex<AgentRegistry>>,  // accès direct à l'état interne
}

// ✅ CORRECT — communication par Handle (messages)
struct TaskRouter {
    registry: AgentRegistryHandle,  // passe par mpsc, jamais directement
}
```

Cette règle est vérifiable à la compilation : `AgentRegistry` (l'acteur) n'est pas `Clone` et n'est pas exposé publiquement. Seul `AgentRegistryHandle` est accessible depuis l'extérieur.

---

## Voir aussi

- [Architecture Principes](./Architecture-Principes) — Principe #5 : Un acteur, une responsabilité
- [Briques Runtime Core](./Briques-Runtime-Core) — détail des composants Runtime
- [ADR-011](../adr/ADR-011-agentid-taskid-string-aliases-dans-core) — AgentId / TaskId comme string aliases
- [ADR-017](../adr/ADR-017-hyper-util-unix-socket-serving) — Unix socket avec hyper-util
- [Briques Chat](./Briques-Chat) — détail du sous-système de chat
- [ADR-034](../adr/ADR-034-chat-hybride-sessions-streaming-hitl-inline) — Chat hybride : sessions, streaming, HITL inline
- [ADR-035](../adr/ADR-035-per-step-observation-orchestrated) — Per-step observation en mode Orchestré
- [ADR-036](../adr/ADR-036-plan-cache-strategy) — Stratégie de cache de plans
