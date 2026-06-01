# Rust Patterns - Apollia OS

> Patterns attendus par brique. Claude Code doit s'y conformer sans dévier.

---

## Patterns transversaux (toutes crates)

### Gestion d'erreurs

```rust
// TOUJOURS thiserror dans les libs - jamais anyhow
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AgentRegistryError {
    #[error("Agent {0} introuvable")]
    NotFound(String),
    #[error("Transition d'état invalide : {from:?} → {to:?}")]
    InvalidTransition { from: ProcessState, to: ProcessState },
    #[error("Acteur mort - canal fermé")]
    ActorDead,
}

// Dans les binaires (apollia-cli) : anyhow OK
```

### Logging structuré

```rust
use tracing::{info, warn, error, debug, instrument};

// TOUJOURS des champs nommés - jamais de format string
tracing::info!(agent_id = %id, state = ?new_state, "Transition ProcessState");
// PAS : tracing::info!("Agent {} passe à {:?}", id, new_state);

// Instrument les fonctions async importantes
#[instrument(skip(self), fields(agent_id = %agent_id))]
pub async fn update_state(&self, agent_id: &str, state: ProcessState) -> Result<(), Error> {
    ...
}
```

### Tests unitaires async

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::broadcast;

    // Helper fixture réutilisable dans le module de test
    fn test_manifest() -> AgentManifest {
        AgentManifest {
            name: "test-agent".to_string(),
            version: "0.1.0".to_string(),
            ..Default::default()  // si Default implémenté
        }
    }

    #[tokio::test]
    async fn test_nominal() {
        // GIVEN
        let (bus_tx, _) = broadcast::channel(16);
        let registry = AgentRegistry::spawn(bus_tx);

        // WHEN
        let result = registry.register(test_manifest()).await;

        // THEN
        assert!(result.is_ok());
        let id = result.unwrap();
        assert!(!id.is_empty());
    }

    #[tokio::test]
    async fn test_erreur() {
        // GIVEN état invalide
        // WHEN action qui doit échouer
        // THEN type d'erreur précis vérifié
        let err = registry.get_agent("inexistant").await.unwrap_err();
        assert!(matches!(err, AgentRegistryError::NotFound(_)));
    }
}
```

---

## apollia-core

### Implémentation des types avec Serde

```rust
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AIPTask {
    pub task_id: String,
    pub context_id: String,
    pub input: AIPInput,
    #[serde(default)]
    pub history: Vec<AIPMessage>,
    pub timeout_seconds: Option<u32>,
}

impl AIPTask {
    pub fn new(input: AIPInput) -> Self {
        Self {
            task_id: Uuid::new_v4().to_string(),
            context_id: Uuid::new_v4().to_string(),
            input,
            history: Vec::new(),
            timeout_seconds: None,
        }
    }
}

// Machines d'état : transitions valides explicites
impl ProcessState {
    pub fn can_transition_to(&self, next: &ProcessState) -> bool {
        matches!(
            (self, next),
            (Self::Initializing, Self::Active)
            | (Self::Initializing, Self::Stopped)
            | (Self::Active, Self::Degraded)
            | (Self::Active, Self::Stopping)
            | (Self::Degraded, Self::Active)
            | (Self::Degraded, Self::Stopping)
            | (Self::Stopping, Self::Stopped)
        )
    }
}
```

---

## apollia-runtime

### Pattern acteur Tokio complet

```rust
// registry.rs - exemple complet AgentRegistry

use std::collections::HashMap;
use tokio::sync::{mpsc, oneshot};
use crate::eventbus::EventBusSender;
use apollia_core::{AgentManifest, ProcessState, RuntimeEvent};

pub type AgentId = String;

#[derive(Debug, Clone)]
pub struct AgentEntry {
    pub id: AgentId,
    pub manifest: AgentManifest,
    pub process_state: ProcessState,
}

// Messages internes - enum privé
enum RegistryMessage {
    Register {
        manifest: AgentManifest,
        reply: oneshot::Sender<Result<AgentId, AgentRegistryError>>,
    },
    UpdateState {
        id: AgentId,
        state: ProcessState,
        reply: oneshot::Sender<Result<(), AgentRegistryError>>,
    },
    GetAgent {
        id: AgentId,
        reply: oneshot::Sender<Option<AgentEntry>>,
    },
    ListAgents {
        reply: oneshot::Sender<Vec<AgentEntry>>,
    },
    Shutdown,
}

struct AgentRegistry {
    agents: HashMap<AgentId, AgentEntry>,
    bus: EventBusSender,
}

// Handle clonable - interface publique
#[derive(Clone)]
pub struct AgentRegistryHandle {
    tx: mpsc::Sender<RegistryMessage>,
}

impl AgentRegistry {
    pub fn spawn(bus: EventBusSender) -> AgentRegistryHandle {
        let (tx, rx) = mpsc::channel(256);
        let registry = Self { agents: HashMap::new(), bus };
        tokio::spawn(registry.run(rx));
        AgentRegistryHandle { tx }
    }

    async fn run(mut self, mut rx: mpsc::Receiver<RegistryMessage>) {
        while let Some(msg) = rx.recv().await {
            match msg {
                RegistryMessage::Register { manifest, reply } => {
                    let result = self.handle_register(manifest).await;
                    let _ = reply.send(result);
                }
                RegistryMessage::Shutdown => {
                    tracing::info!("AgentRegistry arrêt");
                    break;
                }
                // ... autres messages
            }
        }
    }

    async fn handle_register(
        &mut self,
        manifest: AgentManifest,
    ) -> Result<AgentId, AgentRegistryError> {
        let id = uuid::Uuid::new_v4().to_string();
        let entry = AgentEntry {
            id: id.clone(),
            manifest,
            process_state: ProcessState::Initializing,
        };
        self.agents.insert(id.clone(), entry);
        let _ = self.bus.send(RuntimeEvent::AgentRegistered(id.clone()));
        Ok(id)
    }
}

impl AgentRegistryHandle {
    pub async fn register(
        &self,
        manifest: AgentManifest,
    ) -> Result<AgentId, AgentRegistryError> {
        let (tx, rx) = oneshot::channel();
        self.tx
            .send(RegistryMessage::Register { manifest, reply: tx })
            .await
            .map_err(|_| AgentRegistryError::ActorDead)?;
        rx.await.map_err(|_| AgentRegistryError::ActorDead)?
    }
}
```

---

## apollia-tools

### Outil natif - trait Tool

```rust
use async_trait::async_trait;
use serde_json::Value;

#[async_trait]
pub trait Tool: Send + Sync {
    fn descriptor(&self) -> &ToolDescriptor;

    async fn execute(
        &self,
        input: Value,
        sandbox: &SandboxProfile,
        audit: &AuditTrailHandle,
    ) -> Result<ToolOutput, ToolError>;
}

pub struct ToolOutput {
    pub stdout: Option<String>,
    pub stderr: Option<String>,
    pub exit_code: Option<i32>,
    pub duration_ms: u64,
    pub data: Option<Value>,
}

// Implémentation bash_executor
pub struct BashExecutor;

#[async_trait]
impl Tool for BashExecutor {
    fn descriptor(&self) -> &ToolDescriptor {
        // static descriptor
    }

    async fn execute(
        &self,
        input: Value,
        sandbox: &SandboxProfile,
        audit: &AuditTrailHandle,
    ) -> Result<ToolOutput, ToolError> {
        let command: String = serde_json::from_value(input["command"].clone())?;
        let timeout = input["timeout"].as_u64().unwrap_or(30);

        // Construire la commande avec unshare pour l'isolation
        let mut cmd = tokio::process::Command::new("unshare");
        cmd.args(["--pid", "--fork", "--mount", "bash", "-c", &command]);

        // Appliquer les limites selon sandbox
        // ...

        let start = std::time::Instant::now();
        let output = tokio::time::timeout(
            Duration::from_secs(timeout),
            cmd.output(),
        )
        .await
        .map_err(|_| ToolError::Timeout)??;

        let duration_ms = start.elapsed().as_millis() as u64;

        // Écrire dans l'audit trail
        audit.record(/* ... */).await;

        Ok(ToolOutput {
            stdout: Some(String::from_utf8_lossy(&output.stdout).to_string()),
            stderr: Some(String::from_utf8_lossy(&output.stderr).to_string()),
            exit_code: output.status.code(),
            duration_ms,
            data: None,
        })
    }
}
```

---

## apollia-memory

### Connexion SQLite avec rusqlite

```rust
use rusqlite::{Connection, Result, params};

pub struct MemoryStore {
    conn: Connection,
    namespace: String,
}

impl MemoryStore {
    pub fn open(namespace: &str, base_path: &Path) -> Result<Self, MemoryError> {
        let path = base_path.join(format!("{}.db", namespace));
        let conn = Connection::open(&path)?;

        // WAL mode OBLIGATOIRE
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;

        let store = Self { conn, namespace: namespace.to_string() };
        store.run_migrations()?;
        Ok(store)
    }

    fn run_migrations(&self) -> Result<(), MemoryError> {
        self.conn.execute_batch(include_str!("migrations/001_init.sql"))?;
        Ok(())
    }

    pub fn record_episode(
        &self,
        content: &str,
        importance: f64,
        task_id: Option<&str>,
    ) -> Result<String, MemoryError> {
        let id = uuid::Uuid::new_v4().to_string();
        self.conn.execute(
            "INSERT INTO episodic_memories (id, namespace, content, importance, task_id, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, datetime('now'))",
            params![id, self.namespace, content, importance, task_id],
        )?;

        // Mettre à jour le FTS
        self.conn.execute(
            "INSERT INTO memory_fts (rowid, content, source_table, source_id)
             SELECT rowid, content, 'episodic', id FROM episodic_memories WHERE id = ?1",
            params![id],
        )?;

        Ok(id)
    }

    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<MemoryEntry>, MemoryError> {
        // FTS5 avec ranking BM25
        let mut stmt = self.conn.prepare(
            "SELECT e.id, e.content, e.importance, e.created_at,
                    bm25(memory_fts) as score
             FROM memory_fts f
             JOIN episodic_memories e ON f.source_id = e.id
             WHERE memory_fts MATCH ?1
               AND e.namespace = ?2
             ORDER BY score
             LIMIT ?3",
        )?;
        // ...
    }
}
```

---

## apollia-aip (Bridge PyO3)

### Chargement et appel agent Python

```rust
use pyo3::prelude::*;
use pyo3_async_runtimes::tokio as pyo3_tokio;

pub struct AIPBridge {
    agent: Py<PyAny>,  // Instance Python de l'agent
}

impl AIPBridge {
    pub fn load(module_path: &Path) -> Result<Self, AIPBridgeError> {
        Python::with_gil(|py| {
            // Charger le module Python
            let spec = py.import("importlib.util")?
                .call_method1("spec_from_file_location", ("agent", module_path))?;
            let module = py.import("importlib.util")?
                .call_method1("module_from_spec", (spec,))?;
            spec.call_method1("loader.exec_module", (module,))?;

            // Récupérer l'objet 'agent' exposé par le module
            let agent = module.getattr("agent")?;

            // Valider AIP duck typing
            if !agent.hasattr("manifest")? || !agent.hasattr("run")? {
                return Err(AIPBridgeError::InvalidAIP(
                    "L'agent doit exposer manifest() et run()".to_string()
                ));
            }

            Ok(Self { agent: agent.into() })
        })
    }

    pub async fn run(
        &self,
        task: AIPTask,
        ctx: RuntimeContextPy,
    ) -> Result<AIPResult, AIPBridgeError> {
        // Bridge Tokio → asyncio via pyo3-async-runtimes
        pyo3_tokio::into_future(Python::with_gil(|py| {
            let task_py = task.into_py(py);
            let ctx_py = ctx.into_py(py);
            self.agent.call_method1(py, "run", (task_py, ctx_py))
        })?)
        .await?
        .extract::<AIPResult>()
        .map_err(AIPBridgeError::from)
    }
}
```

---

## apollia-cli

### Pattern clap derive

```rust
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "apollia-os", about = "Runtime d'agents IA autonomes souverains")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,

    #[arg(long, global = true, help = "Sortie JSON")]
    pub json: bool,

    #[arg(short, long, global = true, help = "Sortie minimale")]
    pub quiet: bool,

    #[arg(long, global = true, help = "Logs détaillés")]
    pub verbose: bool,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Démarrer le runtime
    Start {
        #[arg(long)]
        foreground: bool,
        #[arg(long, value_name = "FILE")]
        config: Option<PathBuf>,
    },
    /// Gérer les agents
    Agent {
        #[command(subcommand)]
        command: AgentCommands,
    },
    // ...
}

// Output helper - respecte --json et --quiet
pub fn print_output<T: serde::Serialize>(
    value: &T,
    json: bool,
    quiet: bool,
    human_fn: impl FnOnce(&T),
) {
    if json {
        println!("{}", serde_json::to_string_pretty(value).unwrap());
    } else if !quiet {
        human_fn(value);
    }
}
```
