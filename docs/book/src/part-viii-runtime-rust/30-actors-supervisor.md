# Acteurs Tokio et Supervisor

Un acteur est une tâche asynchrone Tokio qui **possède exclusivement son état interne** et communique avec l'extérieur uniquement via des messages sur un canal `mpsc`. Personne ne peut lire ou modifier l'état d'un acteur directement, il faut lui envoyer un message et attendre sa réponse.

Ce chapitre couvre le pattern Handle (l'interface unique d'un acteur), la liste des principaux acteurs du runtime, et le Supervisor qui orchestre leur cycle de vie.

---

## Le pattern Handle

Chaque acteur expose un **Handle** clonable, c'est son unique interface publique.

```rust
struct AgentRegistry {
    agents: HashMap<AgentId, AgentRecord>,
    name_index: HashMap<String, AgentId>,
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

Le Handle est `Clone + Send + Sync`. Il peut être partagé librement entre threads, handlers axum, tâches Tokio, sans jamais exposer l'état interne de l'acteur.

### Règle absolue : zéro `Arc<Mutex<T>>` entre acteurs

```rust
// INTERDIT : accès direct à l'état interne d'un autre acteur
struct TaskRouter {
    registry: Arc<Mutex<AgentRegistry>>,
}

// CORRECT : communication par Handle
struct TaskRouter {
    registry: AgentRegistryHandle,
}
```

Cette règle est vérifiable à la compilation : `AgentRegistry` n'est pas `Clone` et n'est pas publique. Seul `AgentRegistryHandle` est accessible depuis l'extérieur du module.

---

## Les acteurs principaux

### EventBus

Diffuse les événements système à tous les abonnés via `tokio::sync::broadcast`. État interne : `broadcast::Sender<RuntimeEvent>` avec buffer 1 024 événements.

```rust
let mut rx = event_bus.subscribe();
tokio::spawn(async move {
    while let Ok(event) = rx.recv().await {
        match event {
            RuntimeEvent::TaskCompleted { task_id, .. } => { /* react */ }
            RuntimeEvent::ShutdownRequested => break,
            _ => {}
        }
    }
});
```

### AgentRegistry

Source de vérité pour l'état (`ProcessState`) de tous les agents actifs. État interne : `HashMap<AgentId, AgentRecord>` + `SkillIndex` (index inversé `skill_id → agent`).

Transitions valides :

```
INITIALIZING → ACTIVE     (démarrage réussi)
INITIALIZING → STOPPED    (échec au démarrage)
ACTIVE       → DEGRADED   (outil optionnel indisponible)
ACTIVE       → STOPPING   (arrêt demandé)
STOPPING     → STOPPED    (drain terminé)
```

### TaskRouter

Réceptionne les soumissions de tâches, vérifie l'état de l'agent cible, dispatche vers son `ExecutionCoordinator`.

```
Submit(agent, input)
  ├── AgentRegistry.get_state(agent)
  │     ACTIVE    → OK
  │     DEGRADED  → OK + warning EventBus
  │     STOPPING  → SubmitError::AgentUnavailable
  │
  └── ExecutionCoordinator[agent].submit(task)
```

### ExecutionCoordinator (un par agent actif)

Gère la concurrence par agent via un sémaphore Tokio. `max_concurrent_tasks` (défaut 1) contrôle combien de tâches peuvent s'exécuter simultanément.

```rust
let permit = semaphore.try_acquire_owned()?;  // non-bloquant

tokio::spawn(async move {
    let _permit = permit;  // libéré quand la tâche se termine
    dispatch.execute(task).await
});
```

### APIServer

Expose l'API REST via Unix socket et TCP, alimente les streams SSE. Deux surfaces, mêmes endpoints (cf. [chapitre 31](31-rest-api-config.md)).

### ChatSessionManager

Gère les sessions de chat interactif. Chemin d'exécution séparé du `TaskRouter` : les sessions de chat ne passent pas par le dispatch de tâches standard.

### TriggerEngine

Surveille les sources d'événements (cron, interval, filewatch, webhook) et déclenche des tâches vers les agents. Suit le même pattern Handle (cf. [chapitre 36](36-triggers.md)).

---

## Flux de messages typique

Pour ancrer le pattern Handle dans un cas réel, voici ce qui se passe entre l'instant où un client soumet une tâche et celui où la réponse remonte :

```
   Client HTTP
       │  POST /tasks {"agent": "summarize", "input": "..."}
       ▼
  ┌─────────────┐
  │  APIServer  │ valide la requête, émet TaskSubmitted sur EventBus
  └──────┬──────┘
         │ ExecutionCoordinatorHandle.submit(task)
         ▼
  ┌──────────────────────┐
  │ ExecutionCoordinator │ acquiert un permit du sémaphore
  └──────────┬───────────┘
             │ Dispatch.execute(task, ctx)
             ▼
  ┌──────────────────────┐
  │  Bridge PyO3 +       │ marshalle payload, appelle Python async
  │  __apollia_dispatch__│
  └────────┬─────────────┘
           │ ctx.tools.call / ctx.a2a.invoke / ctx.llm
           ▼
  ┌───────────────┐         ToolRegistryHandle.call(tool, args)
  │ Agent Python  │ ────────────────────────────────────────► outils sandbox
  │ (@skill /     │ ◄──────────────────────────────────────── résultat JSON
  │  @on_message) │
  └───────┬───────┘
          │ AIPResult.completed(data) ou DomainError trapée par boundary
          ▼
   réponse remontée → ExecutionCoordinator → APIServer → Client HTTP
```

Aucun acteur ne partage de mémoire mutable avec un autre, tout transite par les canaux. C'est ce qui garantit qu'on peut redémarrer un acteur sans corrompre les autres (principe #5).

---

## Le Supervisor

Le Supervisor est le gardien du runtime. Il démarre les acteurs dans le bon ordre, surveille leur santé, les redémarre s'ils tombent, et orchestre l'arrêt graceful quand vous demandez `apollia-os stop`.

### Séquence de démarrage

Les acteurs démarrent dans un ordre strict. Chaque acteur dépend de ceux qui précèdent.

```
Phase  1 : EventBus           : bus interne (tout le monde en dépend)
Phase  2 : AgentRegistry      : état des agents
Phase  3 : Tool Registry      : catalogue outils + résolution MCP
Phase  4 : Memory Engine      : connexions SQLite
Phase  5 : LlmRouter          : backends LLM (local + cloud)
Phase  6 : TriggerEngine      : ouvre triggers_def.db, charge les triggers
Phase  7 : APIServer          : accepte les connexions externes
Phase  8 : NotificationEngine : ouvre notifications.db
Phase  9 : ChatSessionManager : ouvre chat.db, restaure sessions
Phase 10 : SttEngine          : charge le modèle Whisper (conditionnel)
Phase 11 : BundledAgents      : auto-installe les agents bundled si absents
```

Si la phase N échoue, toutes les phases précédentes sont arrêtées en ordre inverse avant que le processus se termine. Aucun démarrage partiel silencieux.

```bash
apollia-os start
# ✔ EventBus         prêt
# ✔ AgentRegistry    prêt
# ✔ Tool Registry    outils chargés
# ✔ Memory Engine    prêt
# ✔ LlmRouter        2 backends (local, anthropic)
# ✔ TriggerEngine    3 triggers actifs
# ✔ APIServer        localhost:7771, /tmp/apollia.sock
# ✔ Runtime prêt en 1.4s
```

### RestartPolicy

Chaque acteur a une politique de redémarrage :

```rust
pub enum RestartPolicy {
    Always,      // Redémarre toujours après une panique
    OnFailure,   // Redémarre seulement si exit non-normal
    Never,       // Pas de redémarrage
}
```

| Acteur | Policy | Raison |
|---|---|---|
| EventBus | `Always` | Canal central, indisponible = runtime aveugle |
| AgentRegistry | `Always` | État des agents, indisponible = dispatch impossible |
| TaskRouter | `Always` | Dispatch, indisponible = plus de tâches acceptées |
| APIServer | `OnFailure` | Peut rebinder si le port était occupé |

Si un acteur dépasse `max_restarts` (défaut 5) dans `restart_window_secs` (défaut 60s), le runtime s'arrête entièrement avec `exit(1)`. Le système préfère un arrêt net à un état incohérent.

### Arrêt graceful (drain)

```
SIGTERM / SIGINT / apollia-os stop
       │
       ▼
EventBus.broadcast(ShutdownRequested)
       │
       ▼
APIServer : refuse les nouvelles connexions
TaskRouter : refuse les nouvelles soumissions
       │
       ▼
Pour chaque agent ACTIVE :
  ProcessState → STOPPING
  ├── Drain des tâches en cours (timeout 30s)
  └── ProcessState → STOPPED
       │
       ▼
Memory Engine → flush SQLite + fermeture
Tool Registry → fermeture connexions MCP
       │
       ▼
exit(0)
```

Timeout de drain : 30 secondes. Si une tâche n'est pas terminée dans ce délai, elle est annulée (`CANCELED`) et tracée dans l'audit log. Aucune tâche n'est perdue silencieusement.

Pour forcer l'arrêt immédiat sans drain : `apollia-os stop --force`. À utiliser seulement en cas de blocage : les tâches HITL en attente d'approbation seront perdues.

---

## Mode embarqué Tauri

Quand l'application Desktop démarre, elle appelle `init_embedded` qui spawne un thread dédié, crée un `tokio::Runtime`, démarre le `Supervisor` complet, et attend `AllReady` (timeout 30s par défaut).

Le socket Unix et l'API TCP restent actifs : la CLI fonctionne en parallèle du Desktop, sur le même runtime (cf. [chapitre 32](32-desktop.md)).

---

## ADRs

- `ADR-014` : Bridge PyO3 async
- `ADR-017` : hyper-util Unix socket serving
- `ADR-018` : CLI bootstrap sans Supervisor (mode dégradé pour `apollia inspect`)
- `ADR-027` : Desktop Tauri runtime embarqué

*(ADRs disponibles prochainement, cf. l'encadré "ADRs et wiki" en introduction.)*
