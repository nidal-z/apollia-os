use std::collections::HashMap;

use apollia_core::RuntimeEvent;
use chrono::Utc;

use crate::{config::Severity, engine::Notification};

/// Complete list of event names that [`map_event`] can produce.
///
/// Used to validate event names declared in `apollia.toml` at startup.
pub const KNOWN_EVENT_NAMES: &[&str] = &[
    "task.input_required",
    "task.failed",
    "task.completed",
    "agent.degraded",
    "agent.inactivity",
    "llm.backend_down",
    "llm.cost_alert",
    "trigger.error",
    "pipeline.completed",
    "pipeline.failed",
    "pipeline.suspended",
    "chat.approval_required",
    "chat.tool_failed",
    "chat.user_input_required",
];

/// Extracts the text of the first question from the serialized JSON of the
/// `ask_user` tool (`Vec<UserQuestion>`).
///
/// Returns `None` if the JSON is invalid or the list is empty. Used to fill
/// `output_preview` in `chat.user_input_required` notifications.
fn first_question_preview(questions_json: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(questions_json).ok()?;
    let first = value.as_array()?.first()?;
    let text = first.get("question")?.as_str()?;
    Some(text.to_string())
}

/// Emit a `tracing::warn!` for every event name in `events` that is not in
/// [`KNOWN_EVENT_NAMES`].
///
/// The wildcard `"*"` is silently accepted without a warning.
/// This function never blocks the caller; unknown events are non-fatal.
pub fn warn_unknown_events(events: &[String]) {
    for event in events {
        if event == "*" {
            continue;
        }
        if !KNOWN_EVENT_NAMES.contains(&event.as_str()) {
            tracing::warn!(
                event = %event,
                "notification config: unknown event name - it will never fire"
            );
        }
    }
}

/// Builds the HITL resume URL from the API base URL.
///
/// `base_url`: e.g. `http://127.0.0.1:7771` (no trailing slash).
fn build_resume_url(base_url: &str, task_id: &str) -> String {
    format!("{}/api/v1/tasks/{}/resume", base_url, task_id)
}

/// Builds the task inspection URL in the dashboard.
///
/// `base_url`: e.g. `http://127.0.0.1:7771` (no trailing slash).
fn build_inspect_url(base_url: &str, task_id: &str) -> String {
    format!("{}/dashboard#tasks/{}", base_url, task_id)
}

/// Builds the general dashboard URL (no task anchor).
///
/// `base_url`: e.g. `http://127.0.0.1:7771` (no trailing slash).
fn build_dashboard_url(base_url: &str) -> String {
    format!("{}/dashboard", base_url)
}

/// Turns a [`RuntimeEvent`] into a [`Notification`].
///
/// Pure function: no side effects, testable without infrastructure.
///
/// `base_url`: base URL of the local REST API (e.g. `http://127.0.0.1:7771`),
/// used to build the HITL resume URLs in the metadata.
///
/// The events that produce a notification:
/// - `TaskInputRequired`, `TaskCompleted` (success and failure), `AgentDegraded`,
///   `LlmModelFailed`, `TriggerError`
/// - `PipelineCompleted`, `PipelineFailed`, `PipelineSuspended`
///
/// Everything else returns `None`.
pub fn map_event(base_url: &str, event: &RuntimeEvent) -> Option<Notification> {
    let dashboard_url = build_dashboard_url(base_url);

    match event {
        RuntimeEvent::TaskInputRequired {
            task_id,
            prompt,
            step_id: _,
        } => {
            let mut metadata = HashMap::new();
            metadata.insert(
                "resume_url".into(),
                build_resume_url(base_url, task_id.as_ref()),
            );
            metadata.insert(
                "inspect_url".into(),
                build_inspect_url(base_url, task_id.as_ref()),
            );
            metadata.insert("dashboard_url".into(), dashboard_url);
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
        } => {
            let mut metadata = HashMap::new();
            metadata.insert("dashboard_url".into(), dashboard_url);
            Some(Notification {
                event: "task.failed".into(),
                timestamp: Utc::now(),
                task_id: Some(task_id.to_string()),
                agent: Some(agent_id.to_string()),
                message: "Tâche échouée".into(),
                metadata,
                severity: Severity::Error,
            })
        }

        RuntimeEvent::TaskCompleted {
            agent_id,
            task_id,
            success: true,
            ..
        } => {
            let mut metadata = HashMap::new();
            metadata.insert("dashboard_url".into(), dashboard_url);
            Some(Notification {
                event: "task.completed".into(),
                timestamp: Utc::now(),
                task_id: Some(task_id.to_string()),
                agent: Some(agent_id.to_string()),
                message: "Tâche terminée avec succès".into(),
                metadata,
                severity: Severity::Info,
            })
        }

        RuntimeEvent::AgentDegraded { agent_id, reason } => {
            let mut metadata = HashMap::new();
            metadata.insert("dashboard_url".into(), dashboard_url);
            Some(Notification {
                event: "agent.degraded".into(),
                timestamp: Utc::now(),
                task_id: None,
                agent: Some(agent_id.to_string()),
                message: format!("Agent dégradé : {}", reason),
                metadata,
                severity: Severity::Warning,
            })
        }

        RuntimeEvent::LlmModelFailed { backend, reason } => {
            let mut metadata = HashMap::new();
            metadata.insert("backend".into(), backend.clone());
            metadata.insert("dashboard_url".into(), dashboard_url);
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
            metadata.insert("dashboard_url".into(), dashboard_url);
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

        // --- Pipeline events -------------------------------------------------
        RuntimeEvent::PipelineCompleted {
            run_id,
            pipeline_id,
            duration_ms,
        } => {
            let duration_s = *duration_ms as f64 / 1000.0;
            let mut metadata = HashMap::new();
            metadata.insert("run_id".into(), run_id.clone());
            metadata.insert("pipeline_id".into(), pipeline_id.clone());
            metadata.insert("dashboard_url".into(), dashboard_url);
            Some(Notification {
                event: "pipeline.completed".into(),
                timestamp: Utc::now(),
                task_id: None,
                agent: None,
                message: format!(
                    "Pipeline {pipeline_id} terminé - run {run_id} en {duration_s:.1}s"
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
            metadata.insert("dashboard_url".into(), dashboard_url);
            Some(Notification {
                event: "pipeline.failed".into(),
                timestamp: Utc::now(),
                task_id: None,
                agent: None,
                message: format!("Pipeline {pipeline_id} échoué - step [{step_id}] : {reason}"),
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
            metadata.insert("dashboard_url".into(), dashboard_url);
            Some(Notification {
                event: "pipeline.suspended".into(),
                timestamp: Utc::now(),
                task_id: Some(task_id.clone()),
                agent: None,
                message: format!(
                    "Pipeline suspendu - step [{step_id}] attend approbation (run {run_id})\napollia-os task resume {task_id} --approve"
                ),
                metadata,
                severity: Severity::Warning,
            })
        }

        // --- Chat events -----------------------------------------------------
        RuntimeEvent::ChatToolCallCompleted {
            session_id,
            tool_name,
            success: false,
            output_preview,
            ..
        } => {
            let mut metadata = HashMap::new();
            metadata.insert("session_id".into(), session_id.clone());
            metadata.insert("tool_name".into(), tool_name.clone());
            metadata.insert("dashboard_url".into(), dashboard_url);
            if let Some(preview) = output_preview {
                metadata.insert("output_preview".into(), preview.clone());
            }
            Some(Notification {
                event: "chat.tool_failed".into(),
                timestamp: Utc::now(),
                task_id: None,
                agent: None,
                message: format!("Outil « {} » échoué", tool_name),
                metadata,
                severity: Severity::Warning,
            })
        }

        RuntimeEvent::ChatApprovalRequired {
            session_id,
            tool_name,
            ..
        } => {
            let mut metadata = HashMap::new();
            metadata.insert("session_id".into(), session_id.clone());
            metadata.insert("tool_name".into(), tool_name.clone());
            metadata.insert("action_url".into(), format!("/chat/{session_id}"));
            metadata.insert("dashboard_url".into(), dashboard_url);
            Some(Notification {
                event: "chat.approval_required".into(),
                timestamp: Utc::now(),
                task_id: None,
                agent: None,
                message: format!("Approbation requise pour {tool_name}"),
                metadata,
                severity: Severity::Warning,
            })
        }

        RuntimeEvent::ChatUserInputRequired {
            request_id,
            session_id,
            questions_json,
            context,
            ..
        } => {
            let mut metadata = HashMap::new();
            metadata.insert("request_id".into(), request_id.clone());
            metadata.insert("session_id".into(), session_id.clone());
            metadata.insert("action_url".into(), format!("/chat/{session_id}"));
            metadata.insert("dashboard_url".into(), dashboard_url);
            if let Some(preview) = first_question_preview(questions_json) {
                metadata.insert("output_preview".into(), preview);
            }
            if let Some(ctx) = context {
                metadata.insert("context".into(), ctx.clone());
            }
            Some(Notification {
                event: "chat.user_input_required".into(),
                timestamp: Utc::now(),
                task_id: None,
                agent: None,
                message: "Un agent vous pose une question".into(),
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

    const DEFAULT_BASE_URL: &str = "http://127.0.0.1:7771";

    #[test]
    fn test_map_event_task_input_required() {
        // GIVEN
        let event = RuntimeEvent::TaskInputRequired {
            task_id: TaskId::from("t-001"),
            prompt: "Confirmer l'envoi ?".into(),
            step_id: None,
        };
        // WHEN
        let notif = map_event(DEFAULT_BASE_URL, &event).expect("doit retourner Some");
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
    fn test_map_event_task_failed() {
        // GIVEN TaskCompleted with success=false represents a failure
        let event = RuntimeEvent::TaskCompleted {
            agent_id: AgentId::from("devis-agent"),
            task_id: TaskId::from("t-002"),
            success: false,
            output: None,
        };
        // WHEN
        let notif = map_event(DEFAULT_BASE_URL, &event).expect("doit retourner Some");
        // THEN
        assert_eq!(notif.event, "task.failed");
        assert_eq!(notif.severity, Severity::Error);
        assert_eq!(notif.task_id.as_deref(), Some("t-002"));
        assert_eq!(notif.agent.as_deref(), Some("devis-agent"));
    }

    #[test]
    fn test_map_event_agent_degraded() {
        // GIVEN
        let event = RuntimeEvent::AgentDegraded {
            agent_id: AgentId::from("mon-agent"),
            reason: "outil manquant : smtp".into(),
        };
        // WHEN
        let notif = map_event(DEFAULT_BASE_URL, &event).expect("doit retourner Some");
        // THEN
        assert_eq!(notif.event, "agent.degraded");
        assert_eq!(notif.severity, Severity::Warning);
        assert!(notif.task_id.is_none());
        assert_eq!(notif.agent.as_deref(), Some("mon-agent"));
        assert!(notif.message.contains("smtp"));
    }

    #[test]
    fn test_map_event_task_completed() {
        // GIVEN
        let event = RuntimeEvent::TaskCompleted {
            agent_id: AgentId::from("hello-agent"),
            task_id: TaskId::from("t-003"),
            success: true,
            output: None,
        };
        // WHEN
        let notif = map_event(DEFAULT_BASE_URL, &event).expect("doit retourner Some");
        // THEN
        assert_eq!(notif.event, "task.completed");
        assert_eq!(notif.severity, Severity::Info);
        assert_eq!(notif.task_id.as_deref(), Some("t-003"));
    }

    #[test]
    fn test_map_event_llm_model_failed() {
        // GIVEN
        let event = RuntimeEvent::LlmModelFailed {
            backend: "anthropic".into(),
            reason: "API key invalide".into(),
        };
        // WHEN
        let notif = map_event(DEFAULT_BASE_URL, &event).expect("doit retourner Some");
        // THEN
        assert_eq!(notif.event, "llm.backend_down");
        assert_eq!(notif.severity, Severity::Error);
        assert_eq!(
            notif.metadata.get("backend").map(String::as_str),
            Some("anthropic")
        );
    }

    #[test]
    fn test_map_event_trigger_error() {
        // GIVEN
        let event = RuntimeEvent::TriggerError {
            trigger_id: "rapport-hebdo".into(),
            error: "agent non trouvé".into(),
        };
        // WHEN
        let notif = map_event(DEFAULT_BASE_URL, &event).expect("doit retourner Some");
        // THEN
        assert_eq!(notif.event, "trigger.error");
        assert_eq!(notif.severity, Severity::Error);
        assert_eq!(
            notif.metadata.get("trigger_id").map(String::as_str),
            Some("rapport-hebdo")
        );
    }

    #[test]
    fn test_map_event_unknown_returns_none() {
        // GIVEN TaskStarted is not in the list of notifiable events
        let event = RuntimeEvent::TaskStarted {
            agent_id: AgentId::from("agent-1"),
            task_id: TaskId::from("t-004"),
        };
        // WHEN
        let result = map_event(DEFAULT_BASE_URL, &event);
        // THEN
        assert!(result.is_none());
    }

    #[test]
    fn test_agent_registered_returns_none() {
        // GIVEN
        let event = RuntimeEvent::AgentRegistered("agent-1".into());
        // WHEN / THEN
        assert!(map_event(DEFAULT_BASE_URL, &event).is_none());
    }

    #[test]
    fn test_all_ready_returns_none() {
        // GIVEN
        let event = RuntimeEvent::AllReady;
        // WHEN / THEN
        assert!(map_event(DEFAULT_BASE_URL, &event).is_none());
    }

    // --- Pipeline notifications ------------------------------------------

    #[test]
    fn test_pipeline_completed_maps_to_info_notification() {
        // GIVEN
        let event = RuntimeEvent::PipelineCompleted {
            run_id: "r-0017".into(),
            pipeline_id: "traitement-facture".into(),
            duration_ms: 9400,
        };
        // WHEN
        let notif = map_event(DEFAULT_BASE_URL, &event).expect("doit retourner Some");
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
    fn test_pipeline_failed_maps_to_warning_notification() {
        // GIVEN
        let event = RuntimeEvent::PipelineFailed {
            run_id: "r-0016".into(),
            pipeline_id: "traitement-facture".into(),
            step_id: "validation".into(),
            reason: "timeout".into(),
        };
        // WHEN
        let notif = map_event(DEFAULT_BASE_URL, &event).expect("doit retourner Some");
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
    fn test_pipeline_suspended_maps_to_warning_with_resume_metadata() {
        // GIVEN
        let event = RuntimeEvent::PipelineSuspended {
            run_id: "r-0018".into(),
            step_id: "comptabilite".into(),
            task_id: "t-0051".into(),
        };
        // WHEN
        let notif = map_event(DEFAULT_BASE_URL, &event).expect("doit retourner Some");
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

    // --- Chat approval notification --------------------------------------

    #[test]
    fn test_chat_approval_required_maps_to_warning_notification() {
        // GIVEN a ChatApprovalRequired event
        let event = RuntimeEvent::ChatApprovalRequired {
            session_id: "sess-001".into(),
            message_id: "msg-005".into(),
            tool_name: "bash_executor".into(),
            prompt: "L'outil 'bash_executor' demande à être exécuté".into(),
        };
        // WHEN
        let notif = map_event(DEFAULT_BASE_URL, &event).expect("doit retourner Some");
        // THEN
        assert_eq!(notif.event, "chat.approval_required");
        assert_eq!(notif.severity, Severity::Warning);
        assert!(notif.message.contains("bash_executor"));
        assert_eq!(
            notif.metadata.get("session_id").map(String::as_str),
            Some("sess-001")
        );
        assert_eq!(
            notif.metadata.get("tool_name").map(String::as_str),
            Some("bash_executor")
        );
        assert_eq!(
            notif.metadata.get("action_url").map(String::as_str),
            Some("/chat/sess-001")
        );
    }

    #[test]
    fn test_chat_tool_failed_maps_to_warning_notification() {
        // GIVEN a ChatToolCallCompleted event with success=false
        let event = RuntimeEvent::ChatToolCallCompleted {
            session_id: "sess-001".into(),
            message_id: "msg-004".into(),
            tool_name: "bash_executor".into(),
            success: false,
            output_preview: Some("exit_code: 1".into()),
            analysis: None,
        };
        // WHEN
        let notif = map_event(DEFAULT_BASE_URL, &event).expect("doit retourner Some");
        // THEN
        assert_eq!(notif.event, "chat.tool_failed");
        assert_eq!(notif.severity, Severity::Warning);
        assert!(notif.message.contains("bash_executor"));
        assert_eq!(
            notif.metadata.get("session_id").map(String::as_str),
            Some("sess-001")
        );
        assert_eq!(
            notif.metadata.get("tool_name").map(String::as_str),
            Some("bash_executor")
        );
        assert!(notif.metadata.contains_key("output_preview"));
    }

    #[test]
    fn test_chat_tool_success_returns_none() {
        // GIVEN a ChatToolCallCompleted event with success=true
        let event = RuntimeEvent::ChatToolCallCompleted {
            session_id: "sess-001".into(),
            message_id: "msg-004".into(),
            tool_name: "bash_executor".into(),
            success: true,
            output_preview: Some("output".into()),
            analysis: None,
        };
        // WHEN / THEN no notification for successful tool calls
        assert!(map_event(DEFAULT_BASE_URL, &event).is_none());
    }

    #[test]
    fn test_chat_approval_timeout_returns_none() {
        // GIVEN a ChatApprovalTimeout event (not mapped to notification)
        let event = RuntimeEvent::ChatApprovalTimeout {
            session_id: "sess-001".into(),
            message_id: "msg-005".into(),
            tool_name: "bash_executor".into(),
        };
        // WHEN / THEN no notification produced
        assert!(map_event(DEFAULT_BASE_URL, &event).is_none());
    }

    #[test]
    fn test_agent_ready_unchanged() {
        // GIVEN a non-pipeline event: behavior unchanged (no regression)
        let event = RuntimeEvent::AgentReady("agent-1".into());
        // WHEN / THEN no notification, no panic
        let _result = map_event(DEFAULT_BASE_URL, &event);
    }

    // --- Dynamic URLs from the config ------------------------------------

    #[test]
    fn test_default_config_produces_default_url() {
        // GIVEN default config (127.0.0.1:7771)
        let event = RuntimeEvent::TaskInputRequired {
            task_id: TaskId::from("t-100"),
            prompt: "Valider ?".into(),
            step_id: None,
        };
        // WHEN
        let notif = map_event("http://127.0.0.1:7771", &event).expect("doit retourner Some");
        // THEN
        let resume_url = notif.metadata.get("resume_url").expect("clé présente");
        assert_eq!(
            resume_url,
            "http://127.0.0.1:7771/api/v1/tasks/t-100/resume"
        );
    }

    #[test]
    fn test_custom_port_reflected_in_url() {
        // GIVEN config [api] port = 8080
        let event = RuntimeEvent::TaskInputRequired {
            task_id: TaskId::from("t-200"),
            prompt: "Valider ?".into(),
            step_id: None,
        };
        // WHEN
        let notif = map_event("http://127.0.0.1:8080", &event).expect("doit retourner Some");
        // THEN
        let resume_url = notif.metadata.get("resume_url").expect("clé présente");
        assert_eq!(
            resume_url,
            "http://127.0.0.1:8080/api/v1/tasks/t-200/resume"
        );
    }

    #[test]
    fn test_custom_bind_and_port_reflected_in_url() {
        // GIVEN config [api] bind = "0.0.0.0", port = 9090
        let event = RuntimeEvent::TaskInputRequired {
            task_id: TaskId::from("t-300"),
            prompt: "Valider ?".into(),
            step_id: None,
        };
        // WHEN
        let notif = map_event("http://0.0.0.0:9090", &event).expect("doit retourner Some");
        // THEN
        let resume_url = notif.metadata.get("resume_url").expect("clé présente");
        assert_eq!(resume_url, "http://0.0.0.0:9090/api/v1/tasks/t-300/resume");
    }

    #[test]
    fn test_resume_url_format_contains_task_id() {
        // GIVEN
        let task_id = "task-abc-123";
        let event = RuntimeEvent::TaskInputRequired {
            task_id: TaskId::from(task_id),
            prompt: "Confirmer ?".into(),
            step_id: None,
        };
        // WHEN
        let notif = map_event("http://127.0.0.1:7771", &event).expect("doit retourner Some");
        // THEN
        let resume_url = notif.metadata.get("resume_url").expect("clé présente");
        assert!(
            resume_url.contains(task_id),
            "resume_url doit contenir le task_id : {resume_url}"
        );
        assert!(
            resume_url.ends_with("/resume"),
            "resume_url doit se terminer par /resume : {resume_url}"
        );
    }

    // ── Chat user-input notification ──────────────────────────────────────

    #[test]
    fn test_chat_user_input_required_maps_to_warning_notification() {
        // GIVEN a ChatUserInputRequired event with two questions
        let questions = serde_json::json!([
            {
                "id": "stack",
                "question": "Quelle stack web ?",
                "question_type": "single_choice",
                "options": ["FastAPI", "Django"],
                "hint": null,
            },
            {
                "id": "notes",
                "question": "Autre chose à préciser ?",
                "question_type": "open",
                "options": [],
                "hint": null,
            }
        ])
        .to_string();
        let event = RuntimeEvent::ChatUserInputRequired {
            request_id: "req-001".into(),
            session_id: "sess-001".into(),
            message_id: "msg-006".into(),
            questions_json: questions,
            context: Some("Besoin de votre input avant de coder".into()),
        };
        // WHEN
        let notif = map_event(DEFAULT_BASE_URL, &event).expect("doit retourner Some");
        // THEN
        assert_eq!(notif.event, "chat.user_input_required");
        assert_eq!(notif.severity, Severity::Warning);
        assert!(notif.task_id.is_none());
        assert_eq!(
            notif.metadata.get("request_id").map(String::as_str),
            Some("req-001")
        );
        assert_eq!(
            notif.metadata.get("session_id").map(String::as_str),
            Some("sess-001")
        );
        assert_eq!(
            notif.metadata.get("output_preview").map(String::as_str),
            Some("Quelle stack web ?")
        );
        assert_eq!(
            notif.metadata.get("context").map(String::as_str),
            Some("Besoin de votre input avant de coder")
        );
        assert_eq!(
            notif.metadata.get("action_url").map(String::as_str),
            Some("/chat/sess-001")
        );
    }

    #[test]
    fn test_chat_user_input_required_with_empty_payload() {
        // GIVEN a ChatUserInputRequired with an empty questions array and no context
        let event = RuntimeEvent::ChatUserInputRequired {
            request_id: "req-002".into(),
            session_id: "sess-002".into(),
            message_id: String::new(),
            questions_json: "[]".into(),
            context: None,
        };
        // WHEN
        let notif = map_event(DEFAULT_BASE_URL, &event).expect("doit retourner Some");
        // THEN the notification exists, but without preview or context
        assert_eq!(notif.event, "chat.user_input_required");
        assert_eq!(notif.severity, Severity::Warning);
        assert!(!notif.metadata.contains_key("output_preview"));
        assert!(!notif.metadata.contains_key("context"));
    }

    #[test]
    fn test_different_base_urls_produce_different_resume_urls() {
        // GIVEN two different configs
        let event = RuntimeEvent::TaskInputRequired {
            task_id: TaskId::from("t-999"),
            prompt: "Confirmer ?".into(),
            step_id: None,
        };
        // WHEN
        let notif_a = map_event("http://127.0.0.1:7771", &event).expect("doit retourner Some (a)");
        let notif_b = map_event("http://127.0.0.1:8080", &event).expect("doit retourner Some (b)");
        // THEN the resume URLs differ depending on the config
        let url_a = notif_a
            .metadata
            .get("resume_url")
            .expect("clé présente (a)");
        let url_b = notif_b
            .metadata
            .get("resume_url")
            .expect("clé présente (b)");
        assert_ne!(
            url_a, url_b,
            "des configs différentes doivent produire des URLs différentes"
        );
        assert!(url_a.contains("7771"));
        assert!(url_b.contains("8080"));
    }
}
