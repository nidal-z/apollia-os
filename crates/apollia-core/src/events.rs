use std::fmt;

use serde::{Deserialize, Serialize};

/// Handle en écriture sur l'EventBus — clonable, partageable entre acteurs.
///
/// Alias public défini dans `apollia-core` pour permettre à `apollia-llm`
/// (et toute autre crate sans dépendance sur `apollia-runtime`) d'émettre
/// des événements sur le bus sans créer de dépendance circulaire.
///
/// La publication est non-bloquante ; si le buffer est plein, l'envoi
/// retourne une erreur silencieusement ignorée (fire-and-forget).
pub type EventBusSender = tokio::sync::broadcast::Sender<RuntimeEvent>;

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

    /// Un trigger a été déclenché — tâche soumise au runtime.
    TriggerFired {
        /// Identifiant du trigger qui a produit l'événement.
        trigger_id: String,
        /// Nom de l'agent cible.
        agent: String,
        /// Identifiant de la tâche soumise au TaskRouter.
        task_id: TaskId,
    },
    /// Un trigger a été ignoré (OnBusyPolicy::Drop ou agent occupé).
    TriggerSkipped {
        /// Identifiant du trigger.
        trigger_id: String,
        /// Raison du skip.
        reason: String,
    },
    /// Une erreur s'est produite lors du traitement d'un trigger.
    TriggerError {
        /// Identifiant du trigger.
        trigger_id: String,
        /// Message d'erreur.
        error: String,
    },
    /// Un trigger a été activé via la CLI ou l'API.
    TriggerEnabled {
        /// Identifiant du trigger activé.
        trigger_id: String,
    },
    /// Un trigger a été désactivé via la CLI ou l'API.
    TriggerDisabled {
        /// Identifiant du trigger désactivé.
        trigger_id: String,
    },
    /// Le TriggerEngine a rechargé sa configuration (hot reload ou démarrage initial).
    TriggersReloaded {
        /// Nombre de triggers actifs après rechargement.
        count: usize,
    },

    /// Un backend LLM est en cours de chargement (avant `load()` ou initialisation HTTP).
    LlmModelLoading {
        /// Nom logique du backend tel que configuré dans `apollia.toml`.
        backend: String,
        /// Chemin du fichier `.gguf` (backend local) ou URL de l'API (backend cloud).
        model_path: String,
    },
    /// Un backend LLM est prêt — modèle chargé en mémoire ou connexion cloud vérifiée.
    LlmModelReady {
        /// Nom logique du backend.
        backend: String,
        /// Identifiant du modèle : nom de fichier sans extension (.gguf) ou model_id API.
        model_id: String,
    },
    /// Le chargement d'un backend LLM a échoué — backend ignoré, runtime continue.
    LlmModelFailed {
        /// Nom logique du backend.
        backend: String,
        /// Raison de l'échec (message d'erreur).
        reason: String,
    },
    /// Un appel LLM s'est terminé — émis par `complete_with_observability()`.
    LlmCallCompleted {
        /// Nom logique du backend qui a traité la requête.
        backend: String,
        /// Nombre de tokens dans le prompt.
        prompt_tokens: u32,
        /// Nombre de tokens générés.
        completion_tokens: u32,
        /// Latence totale de l'appel en millisecondes.
        latency_ms: u64,
        /// Coût estimé en USD (backends cloud uniquement ; `None` pour l'inférence locale).
        cost_usd: Option<f64>,
    },

    // ── Plan / Step events (STORY-084) ─────────────────────────────────────

    /// Un `ExecutionPlan` a été généré par le Reasoner et persisté en SQLite.
    PlanGenerated {
        /// Identifiant de la tâche ayant déclenché la planification.
        task_id: TaskId,
        /// Nom de l'agent propriétaire du plan.
        agent_name: String,
        /// Identifiant unique du plan (UUID v4).
        plan_id: String,
        /// Nombre de steps dans le plan.
        step_count: usize,
    },

    /// Un step a démarré son exécution — émis par `ActorLoop` avant chaque appel outil ou LLM.
    StepStarted {
        /// Identifiant de la tâche parente.
        task_id: TaskId,
        /// Identifiant du plan.
        plan_id: String,
        /// Identifiant du step (ex: `"s1"`).
        step_id: String,
        /// Numéro séquentiel du step dans l'exécution (1-based).
        step_num: usize,
        /// Nombre total de steps dans le plan courant.
        total: usize,
        /// Description en langage naturel du step.
        desc: String,
    },

    /// Un step s'est terminé avec succès — émis par `ActorLoop` après chaque appel réussi.
    StepCompleted {
        /// Identifiant de la tâche parente.
        task_id: TaskId,
        /// Identifiant du plan.
        plan_id: String,
        /// Identifiant du step.
        step_id: String,
        /// Durée d'exécution du step en millisecondes.
        duration_ms: u64,
    },

    /// Un step a échoué — émis par `ActorLoop` après chaque échec.
    StepFailed {
        /// Identifiant de la tâche parente.
        task_id: TaskId,
        /// Identifiant du plan.
        plan_id: String,
        /// Identifiant du step.
        step_id: String,
        /// Message d'erreur.
        error: String,
        /// `true` si l'erreur peut déclencher une replanification.
        retryable: bool,
    },

    /// Une replanification a été déclenchée après l'échec d'un step retryable.
    PlanReplanning {
        /// Identifiant de la tâche parente.
        task_id: TaskId,
        /// Identifiant du plan.
        plan_id: String,
        /// Numéro de la tentative de replanification (1-based).
        attempt: u32,
        /// Identifiant du step qui a échoué et déclenché la replanification.
        failed_step: String,
        /// Raison d'échec du step.
        reason: String,
    },

    /// Tous les steps ont été complétés avec succès — plan terminé.
    PlanCompleted {
        /// Identifiant de la tâche parente.
        task_id: TaskId,
        /// Identifiant du plan.
        plan_id: String,
        /// Nombre de steps complétés.
        step_count: usize,
        /// Durée totale d'exécution du plan en millisecondes.
        duration_ms: u64,
    },

    /// Le plan a échoué de manière irrémédiable.
    PlanFailed {
        /// Identifiant de la tâche parente.
        task_id: TaskId,
        /// Identifiant du plan.
        plan_id: String,
        /// Raison de l'échec.
        reason: String,
    },
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
            RuntimeEvent::LlmModelLoading {
                backend: "local".into(),
                model_path: "/tmp/model.gguf".into(),
            },
            RuntimeEvent::LlmModelReady {
                backend: "local".into(),
                model_id: "llama3.2-q4".into(),
            },
            RuntimeEvent::LlmModelFailed {
                backend: "local".into(),
                reason: "file not found".into(),
            },
            RuntimeEvent::LlmCallCompleted {
                backend: "anthropic".into(),
                prompt_tokens: 100,
                completion_tokens: 50,
                latency_ms: 250,
                cost_usd: Some(0.001),
            },
            RuntimeEvent::TriggerFired {
                trigger_id: "rapport-hebdo".into(),
                agent: "rapport-agent".into(),
                task_id: "task-1".into(),
            },
            RuntimeEvent::TriggerSkipped {
                trigger_id: "rapport-hebdo".into(),
                reason: "agent busy, on_busy=drop".into(),
            },
            RuntimeEvent::TriggerError {
                trigger_id: "rapport-hebdo".into(),
                error: "agent not found".into(),
            },
            RuntimeEvent::TriggerEnabled {
                trigger_id: "rapport-hebdo".into(),
            },
            RuntimeEvent::TriggerDisabled {
                trigger_id: "rapport-hebdo".into(),
            },
            RuntimeEvent::TriggersReloaded { count: 3 },
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
