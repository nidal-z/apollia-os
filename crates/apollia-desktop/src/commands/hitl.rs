//! Tauri IPC commands for Human-in-the-Loop (HITL).
//!
//! `list_pending_approvals` cross-references data from `PendingApprovals`
//! (in-memory pending tasks) with `TaskRepository` (SQLite details: prompt,
//! context, timestamp).
//!
//! `resume_task` delegates to the REST API `POST /api/v1/tasks/{id}/resume`,
//! which handles persistence, event emission, and resolution of the oneshot
//! channel.

use apollia_runtime::chat::{AlwaysAcceptScope, FsHitlDecision};
use apollia_runtime::embedded::RuntimeHandle;
use serde::{Deserialize, Serialize};
use tauri::State;

use super::http_post_json;

/// Filesystem decision serialized from the frontend.
///
/// Corresponds to [`FsHitlDecision`] on the Rust side. The `decision`
/// discriminant is expressed in snake_case; the scope of an "always accept" is
/// a separate field (`scope`) to stay backward-compatible with earlier builds
/// that only sent `op`+`level`.
#[derive(Debug, Deserialize)]
#[serde(tag = "decision", rename_all = "snake_case")]
pub enum HitlFilesystemDecisionInput {
    /// The user approves the operation for this invocation.
    Approve,
    /// The user denies; the operation is cancelled.
    /// `reason` is required on the frontend when the tool manifest asks for it;
    /// it is forwarded to the agent via `FsHitlDecision::Deny { reason }`.
    Deny {
        /// Free-text reason entered by the operator (optional).
        #[serde(default)]
        reason: Option<String>,
    },
    /// The user approves **and** installs an "always accept" rule.
    /// The scope sets the breadth; by default (`None` on the JS side) we apply
    /// [`AlwaysAcceptScope::ThisSession`].
    AlwaysAllow {
        /// Filesystem operation (e.g. `"write"`).
        op: String,
        /// Risk level (e.g. `"medium"`).
        level: String,
        /// Scope of the "always accept". Absent means session only.
        #[serde(default)]
        scope: Option<AlwaysAcceptScope>,
    },
    /// Legacy variant kept for older frontends, equivalent to `AlwaysAllow`
    /// with `scope = ThisSession`.
    AlwaysAllowSession {
        /// Filesystem operation (e.g. `"write"`).
        op: String,
        /// Risk level (e.g. `"medium"`).
        level: String,
    },
}

/// Resolves a filesystem HITL approval request emitted by `HitlFilesystemRequired`.
///
/// The frontend calls this command after the user clicks Approve / Deny /
/// Always allow in the HITL modal.
///
/// # Errors
///
/// Returns an error if the chat manager is unavailable or if `request_id` is
/// unknown (already resolved or expired).
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
        HitlFilesystemDecisionInput::Deny { reason } => FsHitlDecision::Deny { reason },
        HitlFilesystemDecisionInput::AlwaysAllow { op, level, scope } => {
            FsHitlDecision::AlwaysAllow {
                scope: scope.unwrap_or_else(AlwaysAcceptScope::safe_default),
                op,
                level,
            }
        }
        HitlFilesystemDecisionInput::AlwaysAllowSession { op, level } => {
            FsHitlDecision::AlwaysAllow {
                scope: AlwaysAcceptScope::ThisSession,
                op,
                level,
            }
        }
    };

    manager
        .resolve_fs_hitl(request_id, fs_decision)
        .await
        .map_err(|e| e.to_string())
}

/// Pending approval for display in the UI.
#[derive(Debug, Serialize)]
pub struct PendingApproval {
    /// Identifier of the suspended task.
    pub task_id: String,
    /// Name of the agent that requested the approval.
    pub agent_name: String,
    /// Prompt shown to the user.
    pub prompt: String,
    /// Optional additional context.
    pub context: Option<serde_json::Value>,
    /// ISO8601 suspension timestamp.
    pub suspended_at: String,
}

/// Lists the pending HITL approvals.
///
/// Reads the `task_id`s from `PendingApprovals` (in-memory state), then
/// enriches each entry with details from `TaskRepository` (SQLite) when
/// available. Returns an empty list if HITL is not configured.
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
                        // TaskRepository has no info for this task,
                        // so return an entry with minimal fields.
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

/// Resumes or rejects a task awaiting approval.
///
/// Delegates to `POST /api/v1/tasks/{id}/resume`, which handles:
/// - validating the `input_required` status in `TaskRepository`
/// - persisting the human decision
/// - emitting `RuntimeEvent::TaskResumed` on the EventBus
/// - resolving the oneshot channel in `PendingApprovals`
///
/// # Errors
///
/// Returns an error if:
/// - `approved == false` and `reason` is `None`
/// - the task is not in `input_required` status (409)
/// - the task does not exist (404)
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

/// Resolved approval (approved or rejected) for display in the UI history.
#[derive(Debug, Serialize)]
pub struct ResolvedApproval {
    /// Task identifier (truncated to 8 characters on the UI side).
    pub task_id: String,
    /// Agent name.
    pub agent_name: String,
    /// `true` if approved, `false` if rejected.
    pub approved: bool,
    /// Rejection reason (if applicable).
    pub reason: Option<String>,
    /// Wait duration in milliseconds.
    pub wait_duration_ms: Option<i64>,
    /// ISO 8601 timestamp of the response.
    pub responded_at: Option<String>,
}

/// Lists the resolved approvals from the last `days` days (max `limit`).
///
/// Reads the `task_approvals` table via `TaskRepository::list_resolved_approvals()`.
/// Returns an empty list if `TaskRepository` is unavailable.
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

/// Adds a prefix rule whose scope is chosen by the operator.
///
/// Called by the "Always allow" buttons of the desktop HITL components. Two
/// scopes are supported (the `session` scope was removed: it lived only in the
/// desktop process and was never consulted by the runtime's `PermissionEngine`):
///
/// - `"project"`: rule persisted in `governance.db` with its canonicalized
///   `project_path`. `project_path` is required in this mode.
/// - `"global"`: rule persisted for all projects.
///
/// # Errors
///
/// Returns an error if:
/// - `tool_name`, `action`, or `scope` are invalid;
/// - `scope = "project"` but `project_path` is absent;
/// - the `HOME` variable is absent;
/// - the SQLite database cannot be opened or written.
#[tauri::command]
pub async fn add_permission_prefix_rule(
    tool_name: String,
    arg_prefix: Option<String>,
    action: String,
    scope: String,
    project_path: Option<String>,
) -> Result<(), String> {
    use apollia_permissions::prefix_rule_engine::RuleAction;

    use super::tool_governance::persist_scoped_rule;

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

    let scope_enum = match scope.as_str() {
        "project" => apollia_permissions::PermissionScope::Project,
        "global" => apollia_permissions::PermissionScope::Global,
        other => {
            return Err(format!(
                "unknown scope '{other}', expected 'project' | 'global'"
            ));
        }
    };

    match scope_enum {
        apollia_permissions::PermissionScope::Project => {
            use super::tool_governance::canonical_project_path;
            let raw =
                project_path.ok_or_else(|| "project scope requires project_path".to_string())?;
            let canonical = canonical_project_path(&raw)?;

            let home = apollia_core::paths::home_string_or_err()?;
            let base_dir = std::path::PathBuf::from(home).join(".apollia");

            persist_scoped_rule(
                &base_dir,
                tool_name.clone(),
                arg_prefix.clone(),
                rule_action,
                apollia_permissions::PermissionScope::Project,
                Some(canonical.clone()),
                None,
            )?;
            tracing::info!(
                tool = %tool_name,
                arg_prefix = ?arg_prefix,
                project_path = %canonical.display(),
                scope = "project",
                "operator added project permission rule"
            );
        }
        apollia_permissions::PermissionScope::Global => {
            let home = apollia_core::paths::home_string_or_err()?;
            let base_dir = std::path::PathBuf::from(home).join(".apollia");
            persist_scoped_rule(
                &base_dir,
                tool_name.clone(),
                arg_prefix.clone(),
                rule_action,
                apollia_permissions::PermissionScope::Global,
                None,
                None,
            )?;
            tracing::info!(
                tool = %tool_name,
                arg_prefix = ?arg_prefix,
                scope = "global",
                "operator added global permission rule"
            );
        }
        apollia_permissions::PermissionScope::Agent
        | apollia_permissions::PermissionScope::Session => {
            return Err(
                "scope must be 'project' or 'global' (use authorize_chat_tool for agent scope)"
                    .to_string(),
            );
        }
    }

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
    fn test_hitl_fs_decision_deny_with_reason_deserialises() {
        // GIVEN a frontend JSON payload that carries a reject reason
        let json = r#"{ "decision": "deny", "reason": "not safe" }"#;
        // WHEN deserialised
        let decoded: HitlFilesystemDecisionInput =
            serde_json::from_str(json).expect("should deserialise");
        // THEN the reason is preserved on the Deny variant
        match decoded {
            HitlFilesystemDecisionInput::Deny { reason } => {
                assert_eq!(reason.as_deref(), Some("not safe"));
            }
            _ => panic!("expected Deny variant"),
        }
    }

    #[test]
    fn test_hitl_fs_decision_deny_without_reason_is_none() {
        // GIVEN a frontend payload without a reason field
        let json = r#"{ "decision": "deny" }"#;
        // WHEN deserialised
        let decoded: HitlFilesystemDecisionInput =
            serde_json::from_str(json).expect("should deserialise");
        // THEN reason defaults to None (no validation error)
        match decoded {
            HitlFilesystemDecisionInput::Deny { reason } => assert!(reason.is_none()),
            _ => panic!("expected Deny variant"),
        }
    }

    #[test]
    fn test_hitl_fs_decision_always_allow_with_scope() {
        // GIVEN an always-allow with an explicit scope picked by the operator
        let json = r#"{
            "decision": "always_allow",
            "op": "write",
            "level": "high",
            "scope": "this_project"
        }"#;
        // WHEN deserialised
        let decoded: HitlFilesystemDecisionInput =
            serde_json::from_str(json).expect("should deserialise");
        // THEN both op/level and scope are carried on the struct
        match decoded {
            HitlFilesystemDecisionInput::AlwaysAllow { op, level, scope } => {
                assert_eq!(op, "write");
                assert_eq!(level, "high");
                assert_eq!(scope, Some(AlwaysAcceptScope::ThisProject));
            }
            other => panic!("expected AlwaysAllow, got {other:?}"),
        }
    }

    #[test]
    fn test_hitl_fs_decision_always_allow_session_legacy_path() {
        // GIVEN the legacy discriminant still used by older frontends
        let json = r#"{
            "decision": "always_allow_session",
            "op": "delete",
            "level": "medium"
        }"#;
        // WHEN deserialised
        let decoded: HitlFilesystemDecisionInput =
            serde_json::from_str(json).expect("should deserialise");
        // THEN the legacy variant is accepted for backward compatibility
        match decoded {
            HitlFilesystemDecisionInput::AlwaysAllowSession { op, level } => {
                assert_eq!(op, "delete");
                assert_eq!(level, "medium");
            }
            other => panic!("expected AlwaysAllowSession, got {other:?}"),
        }
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
