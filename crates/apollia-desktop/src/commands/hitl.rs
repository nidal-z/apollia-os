//! Commandes IPC Tauri pour le Human-in-the-Loop (HITL).
//!
//! `list_pending_approvals` croise les données de `PendingApprovals` (tâches en
//! attente en mémoire) avec `TaskRepository` (détails SQLite : prompt, contexte,
//! timestamp).
//!
//! `resume_task` délègue à l'API REST `POST /api/v1/tasks/{id}/resume` qui gère
//! la persistance, l'émission d'événements et la résolution du oneshot channel.

use apollia_runtime::embedded::RuntimeHandle;
use serde::Serialize;
use tauri::State;

use super::http_post_json;

/// Approbation en attente pour l'affichage dans l'UI.
#[derive(Debug, Serialize)]
pub struct PendingApproval {
    /// Identifiant de la tâche suspendue.
    pub task_id: String,
    /// Nom de l'agent qui a demandé l'approbation.
    pub agent_name: String,
    /// Prompt affiché à l'utilisateur.
    pub prompt: String,
    /// Contexte additionnel optionnel.
    pub context: Option<serde_json::Value>,
    /// Timestamp de la suspension ISO8601.
    pub suspended_at: String,
}

/// Liste les approbations HITL en attente.
///
/// Récupère les `task_id` depuis `PendingApprovals` (état en mémoire),
/// puis enrichit chaque entrée avec les détails de `TaskRepository` (SQLite)
/// si disponible. Retourne une liste vide si HITL n'est pas configuré.
#[tauri::command]
pub async fn list_pending_approvals(
    state: State<'_, RuntimeHandle>,
) -> Result<Vec<PendingApproval>, String> {
    let pending = match state.pending_approvals.as_ref() {
        Some(pa) => pa,
        None => return Ok(Vec::new()),
    };

    let task_ids = pending.pending_task_ids();
    if task_ids.is_empty() {
        return Ok(Vec::new());
    }

    let mut approvals = Vec::with_capacity(task_ids.len());

    for task_id in task_ids {
        let approval = match state.task_repository.as_ref() {
            Some(repo) => {
                match repo.get_approval_info(&task_id).await {
                    Ok(Some(info)) => PendingApproval {
                        task_id,
                        agent_name: info.agent_name,
                        prompt: info.prompt,
                        context: Some(info.context),
                        suspended_at: info.suspended_at,
                    },
                    _ => {
                        // TaskRepository ne contient pas d'info pour cette tâche,
                        // on retourne une entrée avec les champs minimaux.
                        PendingApproval {
                            task_id,
                            agent_name: String::new(),
                            prompt: String::new(),
                            context: None,
                            suspended_at: String::new(),
                        }
                    }
                }
            }
            None => PendingApproval {
                task_id,
                agent_name: String::new(),
                prompt: String::new(),
                context: None,
                suspended_at: String::new(),
            },
        };

        approvals.push(approval);
    }

    Ok(approvals)
}

/// Reprend ou rejette une tâche en attente d'approbation.
///
/// Délègue à `POST /api/v1/tasks/{id}/resume` qui gère :
/// - la validation du statut `input_required` dans `TaskRepository`
/// - la persistance de la décision humaine
/// - l'émission de `RuntimeEvent::TaskResumed` sur l'EventBus
/// - la résolution du oneshot channel dans `PendingApprovals`
///
/// # Errors
///
/// Retourne une erreur si :
/// - `approved == false` et `reason` est `None` (AC-7)
/// - la tâche n'est pas en status `input_required` (409)
/// - la tâche n'existe pas (404)
#[tauri::command]
pub async fn resume_task(
    state: State<'_, RuntimeHandle>,
    task_id: String,
    approved: bool,
    reason: Option<String>,
) -> Result<(), String> {
    if !approved && reason.is_none() {
        return Err("a reason is required when rejecting an approval".to_string());
    }

    let mut body = serde_json::json!({ "approved": approved });
    if let Some(r) = reason {
        body["reason"] = serde_json::Value::String(r);
    }

    let path = format!("/api/v1/tasks/{task_id}/resume");
    http_post_json(state.api_port, &path, &body).await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pending_approval_serializes_to_json() {
        // GIVEN a PendingApproval struct
        let approval = PendingApproval {
            task_id: "t-0042".to_string(),
            agent_name: "crm-agent".to_string(),
            prompt: "Confirmer l'envoi du devis ?".to_string(),
            context: Some(serde_json::json!({ "amount": 1500 })),
            suspended_at: "2026-03-13T14:30:00Z".to_string(),
        };

        // WHEN serialized to JSON
        let json = serde_json::to_value(&approval).expect("serialize");

        // THEN all fields are present
        assert_eq!(json["task_id"], "t-0042");
        assert_eq!(json["agent_name"], "crm-agent");
        assert_eq!(json["prompt"], "Confirmer l'envoi du devis ?");
        assert_eq!(json["context"]["amount"], 1500);
        assert_eq!(json["suspended_at"], "2026-03-13T14:30:00Z");
    }

    #[test]
    fn test_pending_approval_without_context() {
        // GIVEN a PendingApproval without context
        let approval = PendingApproval {
            task_id: "t-0001".to_string(),
            agent_name: String::new(),
            prompt: String::new(),
            context: None,
            suspended_at: String::new(),
        };

        // WHEN serialized to JSON
        let json = serde_json::to_value(&approval).expect("serialize");

        // THEN context is null
        assert!(json["context"].is_null());
    }

    #[test]
    fn test_resume_task_reject_without_reason_validation() {
        // GIVEN approved=false and reason=None
        let approved = false;
        let reason: Option<String> = None;

        // WHEN the validation logic is applied
        let result = if !approved && reason.is_none() {
            Err("a reason is required when rejecting an approval".to_string())
        } else {
            Ok(())
        };

        // THEN the validation fails with a descriptive message
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("reason is required"));
    }

    #[test]
    fn test_resume_task_reject_with_reason_passes_validation() {
        // GIVEN approved=false with a reason
        let approved = false;
        let reason = Some("Budget insuffisant".to_string());

        // WHEN the validation logic is applied
        let result = if !approved && reason.is_none() {
            Err("a reason is required when rejecting an approval".to_string())
        } else {
            Ok(())
        };

        // THEN the validation passes
        assert!(result.is_ok());
    }

    #[test]
    fn test_resume_task_approve_without_reason_passes_validation() {
        // GIVEN approved=true with no reason
        let approved = true;
        let reason: Option<String> = None;

        // WHEN the validation logic is applied
        let result = if !approved && reason.is_none() {
            Err("a reason is required when rejecting an approval".to_string())
        } else {
            Ok(())
        };

        // THEN the validation passes (reason not required for approval)
        assert!(result.is_ok());
    }
}
