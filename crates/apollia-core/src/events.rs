use std::fmt;

use serde::{Deserialize, Serialize};

/// Identifiant unique d'un agent dans le runtime (UUID v4 ou nom slug).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AgentId(String);

impl AgentId {
    /// Create a new AgentId with a random UUID v4.
    pub fn new_v4() -> Self {
        Self(uuid::Uuid::new_v4().to_string())
    }

    /// Borrow the inner string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for AgentId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<String> for AgentId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for AgentId {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

impl AsRef<str> for AgentId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl PartialEq<str> for AgentId {
    fn eq(&self, other: &str) -> bool {
        self.0 == other
    }
}

impl PartialEq<&str> for AgentId {
    fn eq(&self, other: &&str) -> bool {
        self.0 == *other
    }
}

/// Identifiant unique d'une tâche dans le runtime (UUID v4).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TaskId(String);

impl TaskId {
    /// Create a new TaskId with a random UUID v4.
    pub fn new_v4() -> Self {
        Self(uuid::Uuid::new_v4().to_string())
    }

    /// Borrow the inner string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for TaskId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<String> for TaskId {
    fn from(s: String) -> Self {
        Self(s.to_string())
    }
}

impl From<&str> for TaskId {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

impl AsRef<str> for TaskId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl PartialEq<str> for TaskId {
    fn eq(&self, other: &str) -> bool {
        self.0 == other
    }
}

impl PartialEq<&str> for TaskId {
    fn eq(&self, other: &&str) -> bool {
        self.0 == *other
    }
}

/// Catalogue complet des événements du runtime Apollia OS.
///
/// Défini dans `apollia-core` pour éviter les dépendances circulaires :
/// tous les acteurs (`apollia-runtime`, `apollia-oria`, etc.) importent
/// ce type sans créer de cycle.
///
/// Transporté via `tokio::sync::broadcast` par l'`EventBus`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RuntimeEvent {
    /// Un agent a été enregistré dans le Registry (état: Initializing).
    AgentRegistered(AgentId),
    /// Un agent a terminé son initialisation et est opérationnel (état: Active).
    AgentReady(AgentId),
    /// Un agent est passé en état dégradé.
    AgentDegraded { agent_id: AgentId, reason: String },
    /// Un agent est en cours d'arrêt (état: Stopping, drain des tâches).
    AgentStopping(AgentId),
    /// Un agent s'est arrêté proprement.
    AgentStopped(AgentId),
    /// Une tâche a démarré sur un agent.
    TaskStarted { agent_id: AgentId, task_id: TaskId },
    /// Une tâche s'est terminée (succès ou échec).
    TaskCompleted {
        agent_id: AgentId,
        task_id: TaskId,
        success: bool,
    },
    /// Une tâche a été annulée.
    TaskCanceled { task_id: TaskId },
    /// Un step a été exécuté dans une tâche.
    StepExecuted {
        task_id: TaskId,
        step: u32,
        tool: Option<String>,
    },
    /// Le circuit breaker d'un outil s'est ouvert.
    ToolCircuitBroken { tool_name: String },
    /// Le circuit breaker d'un outil s'est refermé après recovery.
    ToolCircuitRestored { tool_name: String },
    /// Tous les composants sont prêts — runtime opérationnel.
    AllReady,
    /// Arrêt demandé (SIGTERM ou commande CLI).
    ShutdownRequested,
    /// Erreur fatale non récupérable.
    FatalError(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ac4_all_variants_exist_and_clone() {
        // GIVEN / WHEN — instancier chaque variante et la cloner
        let variants: Vec<RuntimeEvent> = vec![
            RuntimeEvent::AgentRegistered("agent-1".into()),
            RuntimeEvent::AgentReady("agent-1".into()),
            RuntimeEvent::AgentDegraded {
                agent_id: "agent-1".into(),
                reason: "tool missing".into(),
            },
            RuntimeEvent::AgentStopping("agent-1".into()),
            RuntimeEvent::AgentStopped("agent-1".into()),
            RuntimeEvent::TaskStarted {
                agent_id: "agent-1".into(),
                task_id: "task-1".into(),
            },
            RuntimeEvent::TaskCompleted {
                agent_id: "agent-1".into(),
                task_id: "task-1".into(),
                success: true,
            },
            RuntimeEvent::TaskCanceled {
                task_id: "task-1".into(),
            },
            RuntimeEvent::StepExecuted {
                task_id: "task-1".into(),
                step: 1,
                tool: Some("file_io".into()),
            },
            RuntimeEvent::ToolCircuitBroken {
                tool_name: "bash_executor".into(),
            },
            RuntimeEvent::ToolCircuitRestored {
                tool_name: "bash_executor".into(),
            },
            RuntimeEvent::AllReady,
            RuntimeEvent::ShutdownRequested,
            RuntimeEvent::FatalError("out of memory".into()),
        ];

        // THEN — toutes les variantes sont clonables et debuggables
        for event in &variants {
            let cloned = event.clone();
            let debug_str = format!("{:?}", cloned);
            assert!(!debug_str.is_empty());
        }
    }

    #[test]
    fn test_runtime_event_debug_format() {
        // GIVEN
        let event = RuntimeEvent::AgentRegistered("agent-42".into());
        // WHEN
        let s = format!("{:?}", event);
        // THEN
        assert!(s.contains("agent-42"));
    }
}
