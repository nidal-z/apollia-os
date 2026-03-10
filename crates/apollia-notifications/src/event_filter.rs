use std::collections::HashMap;

use apollia_core::RuntimeEvent;
use chrono::Utc;

use crate::{config::Severity, engine::Notification};

/// Transforme un [`RuntimeEvent`] en [`Notification`].
///
/// Fonction pure — pas d'effet de bord, testable sans infrastructure.
///
/// Les événements qui produisent une notification :
/// - `TaskInputRequired`, `TaskCompleted` (succès et échec), `AgentDegraded`,
///   `LlmModelFailed`, `TriggerError` — existants (Sprints 9-11)
/// - `PipelineCompleted`, `PipelineFailed`, `PipelineSuspended` — ajoutés STORY-123
///
/// Tous les autres retournent `None`.
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
            ..
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
            ..
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

        // ── Pipeline events (STORY-123) ───────────────────────────────────
        RuntimeEvent::PipelineCompleted {
            run_id,
            pipeline_id,
            duration_ms,
        } => {
            let duration_s = *duration_ms as f64 / 1000.0;
            let mut metadata = HashMap::new();
            metadata.insert("run_id".into(), run_id.clone());
            metadata.insert("pipeline_id".into(), pipeline_id.clone());
            Some(Notification {
                event: "pipeline.completed".into(),
                timestamp: Utc::now(),
                task_id: None,
                agent: None,
                message: format!(
                    "Pipeline {pipeline_id} terminé — run {run_id} en {duration_s:.1}s"
                ),
                metadata,
                severity: Severity::Info,
            })
        }

        RuntimeEvent::PipelineFailed {
            run_id,
            pipeline_id,
            step_id,
            reason,
        } => {
            let mut metadata = HashMap::new();
            metadata.insert("run_id".into(), run_id.clone());
            metadata.insert("step_id".into(), step_id.clone());
            Some(Notification {
                event: "pipeline.failed".into(),
                timestamp: Utc::now(),
                task_id: None,
                agent: None,
                message: format!("Pipeline {pipeline_id} échoué — step [{step_id}] : {reason}"),
                metadata,
                severity: Severity::Warning,
            })
        }

        RuntimeEvent::PipelineSuspended {
            run_id,
            step_id,
            task_id,
        } => {
            let mut metadata = HashMap::new();
            metadata.insert("task_id".into(), task_id.clone());
            metadata.insert(
                "resume_approve".into(),
                format!("apollia-os task resume {task_id} --approve"),
            );
            metadata.insert(
                "resume_reject".into(),
                format!("apollia-os task resume {task_id} --reject"),
            );
            Some(Notification {
                event: "pipeline.suspended".into(),
                timestamp: Utc::now(),
                task_id: Some(task_id.clone()),
                agent: None,
                message: format!(
                    "Pipeline suspendu — step [{step_id}] attend approbation (run {run_id})\napollia-os task resume {task_id} --approve"
                ),
                metadata,
                severity: Severity::Warning,
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
            output: None,
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
            output: None,
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

    // ── STORY-123 : Pipeline notifications ───────────────────────────────

    #[test]
    fn test_ac1_pipeline_completed_maps_to_info_notification() {
        // GIVEN
        let event = RuntimeEvent::PipelineCompleted {
            run_id: "r-0017".into(),
            pipeline_id: "traitement-facture".into(),
            duration_ms: 9400,
        };
        // WHEN
        let notif = map_event(&event).expect("doit retourner Some");
        // THEN
        assert_eq!(notif.severity, Severity::Info);
        assert_eq!(notif.event, "pipeline.completed");
        assert!(notif.message.contains("traitement-facture"));
        assert!(notif.message.contains("9.4s"));
        assert_eq!(
            notif.metadata.get("pipeline_id").map(String::as_str),
            Some("traitement-facture")
        );
        assert_eq!(
            notif.metadata.get("run_id").map(String::as_str),
            Some("r-0017")
        );
    }

    #[test]
    fn test_ac2_pipeline_failed_maps_to_warning_notification() {
        // GIVEN
        let event = RuntimeEvent::PipelineFailed {
            run_id: "r-0016".into(),
            pipeline_id: "traitement-facture".into(),
            step_id: "validation".into(),
            reason: "timeout".into(),
        };
        // WHEN
        let notif = map_event(&event).expect("doit retourner Some");
        // THEN
        assert_eq!(notif.severity, Severity::Warning);
        assert_eq!(notif.event, "pipeline.failed");
        assert!(notif.message.contains("validation"));
        assert!(notif.message.contains("timeout"));
        assert_eq!(
            notif.metadata.get("step_id").map(String::as_str),
            Some("validation")
        );
    }

    #[test]
    fn test_ac3_pipeline_suspended_maps_to_warning_with_resume_metadata() {
        // GIVEN
        let event = RuntimeEvent::PipelineSuspended {
            run_id: "r-0018".into(),
            step_id: "comptabilite".into(),
            task_id: "t-0051".into(),
        };
        // WHEN
        let notif = map_event(&event).expect("doit retourner Some");
        // THEN
        assert_eq!(notif.severity, Severity::Warning);
        assert_eq!(notif.event, "pipeline.suspended");
        assert!(notif.message.contains("t-0051"));
        assert!(notif.metadata.contains_key("resume_approve"));
        assert!(notif.metadata.contains_key("resume_reject"));
        let approve_cmd = notif.metadata.get("resume_approve").expect("clé présente");
        assert!(approve_cmd.contains("--approve"));
        assert_eq!(notif.task_id.as_deref(), Some("t-0051"));
    }

    #[test]
    fn test_ac4_agent_ready_unchanged() {
        // GIVEN — non-pipeline event : comportement inchangé (non-régression)
        let event = RuntimeEvent::AgentReady("agent-1".into());
        // WHEN / THEN — aucune notification, pas de panic
        let _result = map_event(&event);
    }
}
