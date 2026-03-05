# Architecture Summary — Apollia OS (Machine-Readable)

> Résumé condensé pour les agents IA. Pour la documentation complète, voir `docs/`.

---

## Stack technique

| Composant | Technologie | Crate |
|---|---|---|
| Runtime async | Tokio 1.x | toutes |
| Bridge Python | PyO3 + pyo3-async-runtimes | apollia-aip |
| Persistance | SQLite + rusqlite + FTS5 | apollia-memory |
| API locale | axum (Unix socket + TCP) | apollia-runtime |
| CLI | clap v4 derive | apollia-cli |
| Sérialisation | serde + serde_json | toutes |
| Erreurs (lib) | thiserror | toutes |
| Logging | tracing + tracing-subscriber | toutes |
| IDs | uuid v4 | apollia-core |

**Règle absolue : `thiserror` dans les libs, `anyhow` INTERDIT dans les crates du workspace.**

---

## Workspace Cargo

```
apollia-os/
├── apollia-core/     Types partagés. Dépendance de TOUTES les autres crates.
├── apollia-runtime/  Runtime Core. Dépend de core.
├── apollia-oria/     ORIA Engine. Dépend de core + tools + memory.
├── apollia-tools/    Tool Registry. Dépend de core.
├── apollia-memory/   Memory Engine. Dépend de core.
├── apollia-aip/      Bridge PyO3. Dépend de core + tools + memory.
└── apollia-cli/      Binaire. Dépend de runtime (lib).
```

**Dépendances circulaires interdites. `apollia-core` ne dépend de rien du workspace.**

---

## Types fondamentaux (apollia-core)

```rust
// Manifest d'un agent
pub struct AgentManifest {
    pub name: String,
    pub version: String,            // semver
    pub description: String,
    pub tools_required: Vec<String>,
    pub tools_optional: Vec<String>,
    pub supports_streaming: bool,
    pub supports_a2a: bool,
    pub memory_namespace: Option<String>,
    pub max_concurrent_tasks: u32,  // défaut: 1
    pub step_budget: Option<StepBudgetConfig>,
    pub network_allowlist: Option<Vec<String>>,
}

// Tâche soumise à un agent
pub struct AIPTask {
    pub task_id: String,            // UUID v4
    pub context_id: String,
    pub input: AIPInput,
    pub history: Vec<AIPMessage>,
    pub timeout_seconds: Option<u32>,
}

pub struct AIPInput { pub parts: Vec<AIPPart> }
pub enum AIPPart { Text(TextPart), File(FilePart), Data(DataPart) }

// Résultat retourné par l'agent
pub struct AIPResult {
    pub task_id: String,
    pub status: TaskStatus,
    pub output: Vec<AIPPart>,
    pub error: Option<AIPError>,
    pub artifacts: Vec<AIPArtifact>,
}

pub enum TaskStatus {
    Completed, Failed, InputRequired, Canceled
}

// Lifecycle processus (aligné ACP)
pub enum ProcessState {
    Initializing, Active, Degraded, Stopping, Stopped
}
```

---

## Pattern acteur Tokio (OBLIGATOIRE pour Runtime Core)

```rust
// Structure standard pour CHAQUE acteur du Runtime Core
// Ne pas dévier de ce pattern sans ADR

pub struct MonActeur {
    // état interne PRIVÉ — jamais exposé directement
    agents: HashMap<AgentId, AgentEntry>,
    bus: EventBusSender,
}

// Handle public — la seule interface vers l'acteur
#[derive(Clone)]
pub struct MonActeurHandle {
    tx: mpsc::Sender<MonActeurMessage>,
}

enum MonActeurMessage {
    Requete { param: Type, reply: oneshot::Sender<Result<Type, MonActeurError>> },
    Notification(Type),  // fire-and-forget
    Shutdown,
}

impl MonActeur {
    pub fn spawn(bus: EventBusSender) -> MonActeurHandle {
        let (tx, rx) = mpsc::channel(256);
        let acteur = Self { agents: HashMap::new(), bus };
        tokio::spawn(acteur.run(rx));
        MonActeurHandle { tx }
    }

    async fn run(mut self, mut rx: mpsc::Receiver<MonActeurMessage>) {
        while let Some(msg) = rx.recv().await {
            match msg {
                MonActeurMessage::Requete { param, reply } => {
                    let result = self.handle_requete(param).await;
                    let _ = reply.send(result);
                }
                MonActeurMessage::Shutdown => break,
                _ => {}
            }
        }
    }
}

impl MonActeurHandle {
    pub async fn requete(&self, param: Type) -> Result<Type, MonActeurError> {
        let (tx, rx) = oneshot::channel();
        self.tx.send(MonActeurMessage::Requete { param, reply: tx }).await
            .map_err(|_| MonActeurError::ActorDead)?;
        rx.await.map_err(|_| MonActeurError::ActorDead)?
    }
}
```

**INTERDIT** : `Arc<Mutex<T>>` partagé entre acteurs. Tout passage d'état = message.

---

## EventBus (apollia-runtime)

```rust
// Basé sur tokio::sync::broadcast — buffer 1024
pub type EventBusSender = broadcast::Sender<RuntimeEvent>;
pub type EventBusReceiver = broadcast::Receiver<RuntimeEvent>;

pub enum RuntimeEvent {
    AgentRegistered(AgentId), AgentReady(AgentId),
    AgentDegraded { agent_id: AgentId, reason: String },
    AgentStopped(AgentId),
    TaskStarted { agent_id: AgentId, task_id: TaskId },
    TaskCompleted { agent_id: AgentId, task_id: TaskId, success: bool },
    TaskCanceled { task_id: TaskId },
    StepExecuted { task_id: TaskId, step: u32, tool: Option<String> },
    ToolCircuitBroken { tool_name: String },
    AllReady, ShutdownRequested, FatalError(String),
}
```

---

## Tool Registry (apollia-tools)

```rust
pub struct ToolDescriptor {
    pub name: String,               // "bash_executor", "mcp_filesystem"
    pub version: String,
    pub kind: ToolKind,
    pub input_schema: serde_json::Value,  // JSON Schema (aligné MCP)
    pub sandbox_profile: SandboxProfile,
    pub dangerous: bool,
}

pub enum ToolKind {
    Native,
    McpServer { server_url: String, transport: McpTransport, tool_name: String },
    Custom { module_path: String, class_name: String },
}

pub enum SandboxProfile {
    ReadOnly,           // tmpfs ro + PID ns, 128MB, 30s
    FileSystem,         // sandbox rw + PID ns, 256MB, 60s
    NetworkRestricted,  // FileSystem + net ns + iptables whitelist
    Full,               // Tout autorisé — nécessite dangerous=true
}

// Résolution à INITIALIZING uniquement (fail fast)
pub enum ToolResolutionError {
    NotFound(String),
    McpServerUnreachable(String),
    PackageInstallFailed(String),
}
```

---

## Memory Engine (apollia-memory)

**Un fichier SQLite par namespace** : `~/.apollia/memory/<namespace>.db`

```sql
-- Tables principales
episodic_memories (id, namespace, task_id, agent_id, content, importance, created_at, expires_at, metadata)
semantic_memories (id, namespace, key, value, confidence, created_at, updated_at, expires_at)
procedural_memories (id, namespace, trigger, steps JSON, success_count, last_used_at)

-- Recherche plein texte (TOUJOURS présent)
memory_fts USING fts5(content, tokenize='unicode61')  -- unicode61 OBLIGATOIRE pour le français

-- Vectoriel (OPTIONNEL — uniquement si sqlite-vec installé)
memory_vec USING vec0(embedding float[384])
```

**Stratégie embedding** : FTS5 (défaut) → sqlite-vec + GGUF local → Ollama. Jamais de téléchargement automatique.

---

## ORIA Engine (apollia-oria)

```
Entrée : AIPTask
  └── Observer.enrich() → ContextBundle
  └── classify() → ExecutionMode::Direct | ExecutionMode::Orchestrated
  
  Mode Direct (≤ 4 tools, ≤ 15 steps)
    └── Supervision StepBudget + ResilienceLayer
    └── Appel agent.run(task, ctx) directement
  
  Mode Orchestré
    └── Reasoner.plan() → ExecutionPlan
    └── Actor.execute(plan) → step by step
    └── Max 2 replans si step échoue
```

**StepBudget (appliqué par le runtime, non modifiable par l'agent) :**
```rust
pub struct StepBudget {
    pub max: u32,               // défaut config: 10
    pub current: u32,
    pub tool_calls: u32,
    pub max_tool_calls: u32,    // défaut config: 20
    pub wall_clock_limit: Duration, // défaut config: 300s
}
```

**Circuit breaker par outil** : Closed → Open (5 failures) → HalfOpen (après 30s) → Closed.

---

## API REST locale (apollia-runtime)

```
Unix socket : /tmp/apollia.sock  (CLI)
TCP         : localhost:7771      (SDK, intégrations)

POST   /api/v1/tasks
GET    /api/v1/tasks/{id}
DELETE /api/v1/tasks/{id}
GET    /api/v1/tasks/{id}/stream  (SSE)
GET    /api/v1/agents
POST   /api/v1/agents
DELETE /api/v1/agents/{id}
GET    /api/v1/health
GET    /api/v1/audit
```

---

## Ordre de démarrage (STRICT)

```
1. EventBus  →  2. AgentRegistry  →  3. ToolRegistry  →
4. MemoryEngine  →  5. TaskRouter  →  6. APIServer
```

Chaque service émet `RuntimeEvent::Ready` avant que le suivant ne démarre.

---

## Séquence graceful shutdown

```
SIGTERM → ShutdownRequested → APIServer (ferme) → TaskRouter (refuse)
→ Agents (STOPPING, drain 30s) → Memory (flush) → Tools (ferme MCP) → exit(0)
```
