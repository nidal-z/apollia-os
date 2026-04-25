# Architecture acteurs Tokio

Un acteur est une tâche asynchrone Tokio qui **possède exclusivement son état interne** et communique avec l'extérieur uniquement via des messages sur un canal `mpsc`. Personne ne peut lire ou modifier l'état d'un acteur directement — il faut lui envoyer un message et attendre sa réponse.

---

## Le pattern Handle

Chaque acteur expose un **Handle** clonable — c'est son unique interface publique.

```rust
// L'acteur : possède son état, jamais accessible directement
struct AgentRegistry {
    agents: HashMap<AgentId, AgentRecord>,
    name_index: HashMap<String, AgentId>,
}

// Le Handle : poignée légère, clonable, partageable librement
pub struct AgentRegistryHandle {
    tx: mpsc::Sender<RegistryMessage>,
}

impl AgentRegistryHandle {
    pub fn start(event_tx: broadcast::Sender<RuntimeEvent>) -> Self {
        let (tx, rx) = mpsc::channel(1024);
        tokio::spawn(AgentRegistry::run(rx, event_tx));
        Self { tx }
    }

    pub async fn register(&self, manifest: AgentManifest) -> Result<AgentId, RegistryError> {
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

Le Handle est `Clone + Send + Sync`. Il peut être partagé librement entre threads, handlers axum, tâches Tokio — sans jamais exposer l'état interne de l'acteur.

### Règle absolue : zéro `Arc<Mutex<T>>` entre acteurs

```rust
// ❌ INTERDIT — accès direct à l'état interne d'un autre acteur
struct TaskRouter {
    registry: Arc<Mutex<AgentRegistry>>,
}

// ✅ CORRECT — communication par Handle
struct TaskRouter {
    registry: AgentRegistryHandle,
}
```

Cette règle est vérifiable à la compilation : `AgentRegistry` n'est pas `Clone` et n'est pas publique. Seul `AgentRegistryHandle` est accessible depuis l'extérieur.

---

## Les 8 acteurs principaux

### 1. EventBus — diffusion interne

**Rôle :** diffuser les événements système à tous les abonnés via `tokio::sync::broadcast`.

**État interne :** `broadcast::Sender<RuntimeEvent>` avec buffer 1 024 événements.

```rust
// Abonnement — n'importe quel acteur peut s'abonner
let mut rx = event_bus.subscribe();
tokio::spawn(async move {
    while let Ok(event) = rx.recv().await {
        match event {
            RuntimeEvent::TaskCompleted { task_id, .. } => { /* réagir */ }
            RuntimeEvent::ShutdownRequested => break,
            _ => {}
        }
    }
});
```

### 2. AgentRegistry — inventaire des agents

**Rôle :** source de vérité pour l'état (`ProcessState`) de tous les agents actifs.

**État interne :** `HashMap<AgentId, AgentRecord>` + `SkillIndex` (index inversé `skill_id → agent`).

**Transitions valides :**
```
INITIALIZING → ACTIVE        (démarrage réussi)
INITIALIZING → STOPPED       (échec au démarrage)
ACTIVE       → DEGRADED      (outil optionnel indisponible)
ACTIVE       → STOPPING      (arrêt demandé)
DEGRADED     → STOPPING      (arrêt demandé)
STOPPING     → STOPPED       (drain terminé)
```

### 3. TaskRouter — dispatch des tâches

**Rôle :** réceptionner les soumissions de tâches, vérifier l'état de l'agent cible, dispatcher vers son `ExecutionCoordinator`.

**Logique :**
```
Submit(agent, input)
  │
  ├── AgentRegistry.get_state(agent)
  │     ACTIVE    → OK
  │     DEGRADED  → OK + warning EventBus
  │     STOPPING  → SubmitError::AgentUnavailable
  │
  └── ExecutionCoordinator[agent].submit(task)
```

### 4. ExecutionCoordinator — un par agent actif

**Rôle :** gérer la concurrence par agent via un sémaphore Tokio.

Un `ExecutionCoordinator` existe pour chaque agent en état `ACTIVE`. Il contrôle combien de tâches peuvent s'exécuter simultanément via `max_concurrent_tasks` (défaut : 1).

```rust
// try_acquire_owned() est non-bloquant
// → CapacityExceeded si max_concurrent_tasks est atteint
let permit = semaphore.try_acquire_owned()?;

tokio::spawn(async move {
    let _permit = permit;  // libéré automatiquement quand la tâche se termine
    oria.execute(task).await
});
```

Un agent PME typique est séquentiel — il ne traite qu'une tâche à la fois. Les agents batch peuvent déclarer `max_concurrent_tasks: 3` dans leur manifest pour du parallélisme.

### 5. APIServer — surface externe

**Rôle :** exposer l'API REST via Unix socket et TCP, alimenter les streams SSE.

**Deux surfaces :**
- **Unix socket** `/tmp/apollia.sock` — CLI locale, permissions fichier, ultra-rapide
- **HTTP TCP** `localhost:7771` — SDK Python, intégrations, Desktop Tauri

### 6. ChatSessionManager — sessions conversationnelles

**Rôle :** gérer les sessions de chat interactif (Chat Libre Rust et Chat Agent Python).

**État interne :** `HashMap<String, ChatSession>` + `ChatSessionRepository` (SQLite) + `PendingChatApprovals`.

Chemin d'exécution séparé du `TaskRouter` — les sessions de chat ne passent pas par le dispatch de tâches standard (ADR-034).

### 7. AgentMailbox — messagerie inter-agents

**Rôle :** files de messages par agent pour la communication asynchrone entre agents.

```rust
// Un agent envoie un message à un autre
ctx.mailbox.send("rapport-agent", json!({"type": "refresh", "reason": "data_updated"})).await?;

// Un agent lit ses messages en attente
if let Some(msg) = ctx.mailbox.receive(Duration::from_millis(0)).await {
    // traiter le message
}
```

File FIFO par agent, capacité 100 messages. `MailboxError::QueueFull` si la file est pleine.

### 8. PipelineEngine et TriggerEngine

**Rôle :** PipelineEngine orchestre les DAGs multi-agents. TriggerEngine surveille les sources d'événements et déclenche des tâches.

Tous deux suivent le même pattern Handle — ils ne sont qu'acteurs parmi les autres, sans statut spécial.

---

## Flux de messages — exemple concret

Pour ancrer le pattern Handle dans un cas réel, voici ce qui se passe entre l'instant où un client soumet une tâche et celui où la réponse remonte :

```
   Client HTTP
       │  POST /tasks {"agent": "summarize", "input": "..."}
       ▼
  ┌─────────────┐
  │  APIServer  │──► valide la requête, émet TaskSubmitted sur EventBus
  └──────┬──────┘
         │  ExecutionCoordinatorHandle.submit(task)
         ▼
  ┌──────────────────────┐
  │ ExecutionCoordinator │──► acquiert un permit du sémaphore (max_concurrent_tasks)
  └──────────┬───────────┘
             │  AgentSupervisorHandle.dispatch(task)
             ▼
  ┌──────────────────┐
  │ AgentSupervisor  │──► sélectionne le worker AgentRuntime libre
  └────────┬─────────┘
           │  AgentRuntime.execute(task)
           ▼
  ┌───────────────┐         ToolRegistryHandle.call(tool, args)
  │ AgentRuntime  │ ───────────────────────────────────────────► outils sandbox
  │  (Python)     │ ◄─────────────────────────────────────────── résultat JSON
  └───────┬───────┘
          │  AIPResult { status: "completed", output: [...] }
          ▼
   réponse remontée → ExecutionCoordinator → APIServer → Client HTTP
```

**Lecture de chaque flèche :**

- **APIServer → ExecutionCoordinator** : appel via `ExecutionCoordinatorHandle.submit` — un `mpsc::Sender` clonable, jamais d'accès direct à l'état.
- **ExecutionCoordinator → AgentSupervisor** : passage du `task` après validation du sémaphore (refus immédiat si `max_concurrent_tasks` est atteint).
- **AgentSupervisor → AgentRuntime** : dispatch vers un worker Python dédié, ouverture du `RuntimeContext`.
- **AgentRuntime → outils** : chaque `ctx.tools.call` traverse le `ToolRegistryHandle` qui applique manifest check, step_budget, sandbox, audit trail.
- **Réponse remontée** : `AIPResult` Python sérialisé, oneshot vers le coordinator, libération du permit, réponse HTTP au client.

Aucun acteur ne partage de mémoire mutable avec un autre — tout transite par les canaux. C'est ce qui garantit qu'on peut redémarrer un acteur sans corrompre les autres (principe #5).

---

## Séquence de démarrage

```
EventBus::start()               → broadcast::channel(1024)
AgentRegistry::start(event_tx)  → mpsc::channel(1024) + tokio::spawn
TaskRouter::start(...)          → mpsc::channel(1024) + tokio::spawn
                                  s'abonne à l'EventBus
APIServer::start(state, config) → bind TCP 7771 + Unix socket
LlmRouter::start(...)
TriggerEngine::start(...)
PipelineEngine::start(...)
NotificationEngine::start(...)
AgentMailbox::spawn(event_tx)
ChatSessionManager::spawn(...)

Supervisor::watch() → attend ShutdownRequested ou FatalError
```

Chaque acteur émet `RuntimeEvent::Ready(actor_id)` quand son initialisation est terminée. Le Supervisor attend ce signal (timeout 10s) avant de démarrer le suivant — démarrage séquentiel strict.
