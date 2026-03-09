use std::collections::HashMap;

use apollia_core::RuntimeEvent;
use chrono::Utc;

use crate::{config::Severity, engine::Notification};

/// Transforme un [`RuntimeEvent`] en [`Notification`].
///
/// Fonction pure — pas d'effet de bord, testable sans infrastructure.
///
/// Seuls 6 types d'événements produisent une notification :
/// `TaskInputRequired`, `TaskCompleted` (succès et échec), `AgentDegraded`,
/// `LlmModelFailed`, `TriggerError`. Tous les autres retournent `None`.
pub fn map_event(event: &RuntimeEvent) -> Option<Notification> {
    match event {
        RuntimeEvent::TaskInputRequired {
            task_id,
            prompt,
            step_id: _,
        } => {
            let mut metadata = HashMap::new();
            metadata.insert(
                "resume_url".into(),
                format!("http://localhost:7771/api/v1/tasks/{}/resume", task_id),
            );
            metadata.insert(
                "inspect_url".into(),
                format!("http://localhost:7771/dashboard#tasks/{}", task_id),
            );
            Some(Notification {
                event: "task.input_required".into(),
                timestamp: Utc::now(),
                task_id: Some(task_id.to_string()),
                agent: None,
                message: prompt.clone(),
                metadata,
                severity: Severity::Warning,
            })
        }

        RuntimeEvent::TaskCompleted {
            agent_id,
            task_id,
            success: false,
        } => Some(Notification {
            event: "task.failed".into(),
            timestamp: Utc::now(),
            task_id: Some(task_id.to_string()),
            agent: Some(agent_id.to_string()),
            message: "Tâche échouée".into(),
            metadata: HashMap::new(),
            severity: Severity::Error,
        }),

        RuntimeEvent::TaskCompleted {
            agent_id,
            task_id,
            success: true,
        } => Some(Notification {
            event: "task.completed".into(),
            timestamp: Utc::now(),
            task_id: Some(task_id.to_string()),
            agent: Some(agent_id.to_string()),
            message: "Tâche terminée avec succès".into(),
            metadata: HashMap::new(),
            severity: Severity::Info,
        }),

        RuntimeEvent::AgentDegraded { agent_id, reason } => Some(Notification {
            event: "agent.degraded".into(),
            timestamp: Utc::now(),
            task_id: None,
            agent: Some(agent_id.to_string()),
            message: format!("Agent dégradé : {}", reason),
            metadata: HashMap::new(),
            severity: Severity::Warning,
        }),

        RuntimeEvent::LlmModelFailed { backend, reason } => {
            let mut metadata = HashMap::new();
            metadata.insert("backend".into(), backend.clone());
            Some(Notification {
                event: "llm.backend_down".into(),
                timestamp: Utc::now(),
                task_id: None,
                agent: None,
                message: format!("Backend LLM indisponible : {}", reason),
                metadata,
                severity: Severity::Error,
            })
        }

        RuntimeEvent::TriggerError { trigger_id, error } => {
            let mut metadata = HashMap::new();
            metadata.insert("trigger_id".into(), trigger_id.clone());
            Some(Notification {
                event: "trigger.error".into(),
                timestamp: Utc::now(),
                task_id: None,
                agent: None,
                message: format!("Erreur trigger : {}", error),
                metadata,
                severity: Severity::Error,
            })
        }

        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use apollia_core::{AgentId, TaskId};

    #[test]
    fn test_ac1_map_event_task_input_required() {
        // GIVEN
        let event = RuntimeEvent::TaskInputRequired {
            task_id: TaskId::from("t-001"),
            prompt: "Confirmer l'envoi ?".into(),
            step_id: None,
        };
        // WHEN
        let notif = map_event(&event).expect("doit retourner Some");
        // THEN
        assert_eq!(notif.event, "task.input_required");
        assert_eq!(notif.severity, Severity::Warning);
        assert_eq!(notif.task_id.as_deref(), Some("t-001"));
        assert_eq!(notif.message, "Confirmer l'envoi ?");
        assert!(notif.metadata.contains_key("resume_url"));
        assert!(notif.metadata.contains_key("inspect_url"));
        assert!(notif.metadata["resume_url"].contains("t-001"));
    }

    #[test]
    fn test_ac1_map_event_task_failed() {
        // GIVEN — TaskCompleted avec success=false représente un échec
        let event = RuntimeEvent::TaskCompleted {
            agent_id: AgentId::from("devis-agent"),
            task_id: TaskId::from("t-002"),
            success: false,
        };
        // WHEN
        let notif = map_event(&event).expect("doit retourner Some");
        // THEN
        assert_eq!(notif.event, "task.failed");
        assert_eq!(notif.severity, Severity::Error);
        assert_eq!(notif.task_id.as_deref(), Some("t-002"));
        assert_eq!(notif.agent.as_deref(), Some("devis-agent"));
    }

    #[test]
    fn test_ac1_map_event_agent_degraded() {
        // GIVEN
        let event = RuntimeEvent::AgentDegraded {
            agent_id: AgentId::from("mon-agent"),
            reason: "outil manquant : smtp".into(),
        };
        // WHEN
        let notif = map_event(&event).expect("doit retourner Some");
        // THEN
        assert_eq!(notif.event, "agent.degraded");
        assert_eq!(notif.severity, Severity::Warning);
        assert!(notif.task_id.is_none());
        assert_eq!(notif.agent.as_deref(), Some("mon-agent"));
        assert!(notif.message.contains("smtp"));
    }

    #[test]
    fn test_ac1_map_event_task_completed() {
        // GIVEN
        let event = RuntimeEvent::TaskCompleted {
            agent_id: AgentId::from("hello-agent"),
            task_id: TaskId::from("t-003"),
            success: true,
        };
        // WHEN
        let notif = map_event(&event).expect("doit retourner Some");
        // THEN
        assert_eq!(notif.event, "task.completed");
        assert_eq!(notif.severity, Severity::Info);
        assert_eq!(notif.task_id.as_deref(), Some("t-003"));
    }

    #[test]
    fn test_ac1_map_event_llm_model_failed() {
        // GIVEN
        let event = RuntimeEvent::LlmModelFailed {
            backend: "anthropic".into(),
            reason: "API key invalide".into(),
        };
        // WHEN
        let notif = map_event(&event).expect("doit retourner Some");
        // THEN
        assert_eq!(notif.event, "llm.backend_down");
        assert_eq!(notif.severity, Severity::Error);
        assert_eq!(
            notif.metadata.get("backend").map(String::as_str),
            Some("anthropic")
        );
    }

    #[test]
    fn test_ac1_map_event_trigger_error() {
        // GIVEN
        let event = RuntimeEvent::TriggerError {
            trigger_id: "rapport-hebdo".into(),
            error: "agent non trouvé".into(),
        };
        // WHEN
        let notif = map_event(&event).expect("doit retourner Some");
        // THEN
        assert_eq!(notif.event, "trigger.error");
        assert_eq!(notif.severity, Severity::Error);
        assert_eq!(
            notif.metadata.get("trigger_id").map(String::as_str),
            Some("rapport-hebdo")
        );
    }

    #[test]
    fn test_ac2_map_event_unknown_returns_none() {
        // GIVEN — TaskStarted n'est pas dans la liste des événements notifiables
        let event = RuntimeEvent::TaskStarted {
            agent_id: AgentId::from("agent-1"),
            task_id: TaskId::from("t-004"),
        };
        // WHEN
        let result = map_event(&event);
        // THEN
        assert!(result.is_none());
    }

    #[test]
    fn test_ac2_agent_registered_returns_none() {
        // GIVEN
        let event = RuntimeEvent::AgentRegistered("agent-1".into());
        // WHEN / THEN
        assert!(map_event(&event).is_none());
    }

    #[test]
    fn test_ac2_all_ready_returns_none() {
        // GIVEN
        let event = RuntimeEvent::AllReady;
        // WHEN / THEN
        assert!(map_event(&event).is_none());
    }
}
