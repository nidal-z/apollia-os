//! Commandes IPC Tauri pour le Human-in-the-Loop (HITL).
//!
//! `list_pending_approvals` croise les données de `PendingApprovals` (tâches en
//! attente en mémoire) avec `TaskRepository` (détails SQLite : prompt, contexte,
//! timestamp).
//!
//! `resume_task` délègue à l'API REST `POST /api/v1/tasks/{id}/resume` qui gère
//! la persistance, l'émission d'événements et la résolution du oneshot channel.

use apollia_runtime::chat::FsHitlDecision;
use apollia_runtime::embedded::RuntimeHandle;
use serde::{Deserialize, Serialize};
use tauri::State;

use super::http_post_json;

/// Décision filesystem sérialisée depuis le frontend.
///
/// Correspond exactement à [`FsHitlDecision`] côté Rust, avec un discriminant
/// serde snake_case pour la désérialisation JSON depuis Tauri.
#[derive(Debug, Deserialize)]
#[serde(tag = "decision", rename_all = "snake_case")]
pub enum HitlFilesystemDecisionInput {
    /// L'utilisateur approuve l'opération pour cette invocation.
    Approve,
    /// L'utilisateur refuse — l'opération est annulée.
    Deny,
    /// L'utilisateur approuve pour toute la session pour cette combinaison op+level.
    AlwaysAllowSession {
        /// Opération filesystem (ex. `"write"`).
        op: String,
        /// Niveau de risque (ex. `"medium"`).
        level: String,
    },
}

/// Résout une demande d'approbation filesystem HITL émise par `HitlFilesystemRequired`.
///
/// Le frontend appelle cette commande après que l'utilisateur a cliqué sur
/// Approuver / Refuser / Toujours autoriser dans le modal HITL.
///
/// # Errors
///
/// Retourne une erreur si le gestionnaire de chat n'est pas disponible ou si
/// `request_id` est inconnu (déjà résolu ou expiré).
#[tauri::command]
pub async fn respond_hitl_filesystem(
    state: State<'_, RuntimeHandle>,
    request_id: String,
    decision: HitlFilesystemDecisionInput,
) -> Result<(), String> {
    let manager = state
        .chat_manager
        .as_ref()
        .ok_or_else(|| "chat subsystem not available".to_string())?;

    let fs_decision = match decision {
        HitlFilesystemDecisionInput::Approve => FsHitlDecision::Approve,
        HitlFilesystemDecisionInput::Deny => FsHitlDecision::Deny,
        HitlFilesystemDecisionInput::AlwaysAllowSession { op, level } => {
            FsHitlDecision::AlwaysAllowSession { op, level }
        }
    };

    manager
        .resolve_fs_hitl(request_id, fs_decision)
        .await
        .map_err(|e| e.to_string())
}

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
/// - `approved == false` et `reason` est `None`
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

/// Approbation résolue (approuvée ou rejetée) pour l'affichage dans l'historique UI.
#[derive(Debug, Serialize)]
pub struct ResolvedApproval {
    /// Identifiant de la tâche (tronqué à 8 caractères côté UI).
    pub task_id: String,
    /// Nom de l'agent.
    pub agent_name: String,
    /// `true` si approuvée, `false` si rejetée.
    pub approved: bool,
    /// Raison du rejet (si applicable).
    pub reason: Option<String>,
    /// Durée d'attente en millisecondes.
    pub wait_duration_ms: Option<i64>,
    /// Timestamp ISO 8601 de la réponse.
    pub responded_at: Option<String>,
}

/// Liste les approbations résolues des derniers `days` jours (max `limit`).
///
/// Lit la table `task_approvals` via `TaskRepository::list_resolved_approvals()`.
/// Retourne une liste vide si `TaskRepository` n'est pas disponible.
#[tauri::command]
pub async fn list_resolved_approvals(
    state: State<'_, RuntimeHandle>,
    limit: Option<u32>,
    days: Option<u32>,
) -> Result<Vec<ResolvedApproval>, String> {
    let repo = match state.task_repository.as_ref() {
        Some(r) => r,
        None => return Ok(Vec::new()),
    };

    let limit = limit.unwrap_or(20);
    let days = days.unwrap_or(7);

    let rows = repo
        .list_resolved_approvals(limit, days)
        .await
        .map_err(|e| format!("failed to list resolved approvals: {e}"))?;

    let approvals = rows
        .into_iter()
        .map(|row| ResolvedApproval {
            task_id: row.task_id,
            agent_name: row.agent_name,
            approved: row.approved,
            reason: row.reason,
            wait_duration_ms: row.wait_duration_ms,
            responded_at: row.responded_at,
        })
        .collect();

    Ok(approvals)
}

/// Ajoute une règle de préfixe dans le `PrefixRuleEngine` SQLite.
///
/// Appelé par le bouton "Toujours autoriser" des composants HITL desktop.
/// Ouvre directement la base de données `~/.apollia/permissions.db` pour
/// persister la règle sans passer par le runtime.
///
/// # Errors
///
/// Retourne une erreur si :
/// - la variable `HOME` est absente
/// - la base SQLite ne peut pas être ouverte ou écrite
/// - `action` n'est ni `"allow"` ni `"deny"`
#[tauri::command]
pub async fn add_permission_prefix_rule(
    tool_name: String,
    arg_prefix: Option<String>,
    action: String,
) -> Result<(), String> {
    use apollia_permissions::prefix_rule_engine::{PrefixRule, PrefixRuleEngine, RuleAction};

    if tool_name.trim().is_empty() {
        return Err("tool_name must not be empty".to_string());
    }

    let rule_action = match action.as_str() {
        "allow" => RuleAction::Allow,
        "deny" => RuleAction::Deny,
        other => {
            return Err(format!(
                "unknown action '{other}', expected 'allow' or 'deny'"
            ))
        }
    };

    let home = std::env::var("HOME").map_err(|e| format!("HOME variable not set: {e}"))?;
    let db_path = std::path::PathBuf::from(home)
        .join(".apollia")
        .join("permissions.db");

    let created_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    let mut engine = PrefixRuleEngine::new(&db_path)
        .map_err(|e| format!("failed to open permissions database: {e}"))?;

    let rule = PrefixRule {
        id: 0,
        tool_name,
        arg_prefix,
        action: rule_action,
        created_at,
        created_by_agent: None,
    };

    engine
        .add_rule(&rule)
        .map_err(|e| format!("failed to persist prefix rule: {e}"))?;

    tracing::info!(
        tool = %rule.tool_name,
        arg_prefix = ?rule.arg_prefix,
        "operator added always-allow prefix rule"
    );

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

    #[test]
    fn test_resolved_approval_serializes_to_json() {
        // GIVEN a ResolvedApproval with all fields
        let approval = ResolvedApproval {
            task_id: "t-hist-001".to_string(),
            agent_name: "review-agent".to_string(),
            approved: false,
            reason: Some("Budget insuffisant".to_string()),
            wait_duration_ms: Some(120_000),
            responded_at: Some("2026-03-13T15:00:00Z".to_string()),
        };

        // WHEN serialized to JSON
        let json = serde_json::to_value(&approval).expect("serialize");

        // THEN all fields are present
        assert_eq!(json["task_id"], "t-hist-001");
        assert_eq!(json["agent_name"], "review-agent");
        assert_eq!(json["approved"], false);
        assert_eq!(json["reason"], "Budget insuffisant");
        assert_eq!(json["wait_duration_ms"], 120_000);
        assert_eq!(json["responded_at"], "2026-03-13T15:00:00Z");
    }

    #[test]
    fn test_resolved_approval_without_optional_fields() {
        // GIVEN a ResolvedApproval with minimal fields (approved, no reason)
        let approval = ResolvedApproval {
            task_id: "t-hist-002".to_string(),
            agent_name: "deploy-agent".to_string(),
            approved: true,
            reason: None,
            wait_duration_ms: None,
            responded_at: None,
        };

        // WHEN serialized to JSON
        let json = serde_json::to_value(&approval).expect("serialize");

        // THEN optional fields are null
        assert_eq!(json["approved"], true);
        assert!(json["reason"].is_null());
        assert!(json["wait_duration_ms"].is_null());
        assert!(json["responded_at"].is_null());
    }
}
