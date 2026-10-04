//! Tauri IPC commands for Human-in-the-Loop (HITL).
//!
//! `list_pending_approvals` cross-references data from `PendingApprovals`
//! (in-memory pending tasks) with `TaskRepository` (SQLite details: prompt,
//! context, timestamp).
//!
//! `resume_task` delegates to the REST API `POST /api/v1/tasks/{id}/resume`,
//! which handles persistence, event emission, and resolution of the oneshot
//! channel.

use apollia_runtime::api::routes_permissions::{AddPermissionRuleBody, AddedPermissionRule};
use serde::{Deserialize, Serialize};
use tauri::State;

use super::http_post_json;
use crate::attached::{segment, AttachedClient, RuntimeLink};

/// Pending approval for display in the UI. A subset of the daemon's
/// `GET /api/v1/approvals/pending` entry, so the attached arm reads it as is.
#[derive(Debug, Serialize, Deserialize)]
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
/// available. Returns an empty list if HITL is not configured. Attached to a
/// daemon, the daemon's route answers the same entries.
#[tauri::command]
pub async fn list_pending_approvals(
    link: State<'_, RuntimeLink>,
) -> Result<Vec<PendingApproval>, String> {
    let state = match link.inner() {
        RuntimeLink::Embedded(state) => state,
        RuntimeLink::Attached(remote) => {
            return Ok(remote.get("/api/v1/approvals/pending").await?)
        }
    };
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
    link: State<'_, RuntimeLink>,
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

    let path = format!("/api/v1/tasks/{}/resume", segment(&task_id));
    match link.inner() {
        RuntimeLink::Embedded(state) => {
            http_post_json(state.api_port, &path, &body).await?;
        }
        RuntimeLink::Attached(remote) => {
            let _: serde_json::Value = remote.post(&path, &body).await?;
        }
    }

    Ok(())
}

/// Resolved approval (approved or rejected) for display in the UI history.
/// The daemon's `GET /api/v1/approvals/resolved` entry carries the same fields.
#[derive(Debug, Serialize, Deserialize)]
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
/// Returns an empty list if `TaskRepository` is unavailable. Attached to a
/// daemon, the daemon's route answers.
#[tauri::command]
pub async fn list_resolved_approvals(
    link: State<'_, RuntimeLink>,
    limit: Option<u32>,
    days: Option<u32>,
) -> Result<Vec<ResolvedApproval>, String> {
    let state = match link.inner() {
        RuntimeLink::Embedded(state) => state,
        RuntimeLink::Attached(remote) => {
            let path = format!(
                "/api/v1/approvals/resolved?limit={}&days={}",
                limit.unwrap_or(20),
                days.unwrap_or(7)
            );
            return Ok(remote.get(&path).await?);
        }
    };
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

/// Adds a prefix rule for every agent.
///
/// Called by the "Always allow" buttons of the desktop HITL components. A
/// rule targets one agent or every agent; this command writes the latter
/// (`"global"`), and the chat writes agent rules through
/// `authorize_chat_tool`. The `"project"` scope was retired: a rule never
/// targets a project, and `project_path` is accepted only so the current
/// caller keeps its argument list. Attached to a daemon, the daemon's
/// `POST /api/v1/permissions/rules` stores it.
///
/// # Errors
///
/// Returns an error if:
/// - `tool_name`, `action`, or `scope` are invalid, `"project"` included;
/// - the `HOME` variable is absent;
/// - the SQLite database cannot be opened or written.
// REASON: a Tauri command; the arguments are those the interface invokes it with.
#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub async fn add_permission_prefix_rule(
    link: State<'_, RuntimeLink>,
    tool_name: String,
    arg_prefix: Option<String>,
    action: String,
    scope: String,
    project_path: Option<String>,
) -> Result<(), String> {
    let _ = project_path;
    match link.inner() {
        RuntimeLink::Embedded(_) => {
            let home = apollia_core::paths::home_string_or_err()?;
            apollia_runtime::permission_rules::add_global_rule(
                apollia_core::paths::data_dir_under(home),
                tool_name,
                arg_prefix,
                action,
                scope,
            )
            .await
            .map(|_| ())
            .map_err(|e| e.to_string())
        }
        RuntimeLink::Attached(remote) => {
            let body = AddPermissionRuleBody {
                tool_name,
                arg_prefix,
                action,
                scope,
            };
            add_rule_attached(remote, &body).await
        }
    }
}

async fn add_rule_attached(
    remote: &AttachedClient,
    body: &AddPermissionRuleBody,
) -> Result<(), String> {
    let _: AddedPermissionRule = remote.post("/api/v1/permissions/rules", body).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use wiremock::matchers::{body_json, header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;

    const TOKEN: &str = "token-of-this-installation";

    #[tokio::test]
    async fn test_attached_a_rule_is_added_on_the_daemon_and_its_refusal_shown() {
        // GIVEN a daemon storing a global rule and refusing a project one
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/v1/permissions/rules"))
            .and(header("authorization", format!("Bearer {TOKEN}").as_str()))
            .and(body_json(serde_json::json!({
                "tool_name": "file_write", "arg_prefix": "/tmp/", "action": "allow", "scope": "global"
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({ "rule_id": 7 })))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/api/v1/permissions/rules"))
            .and(body_json(serde_json::json!({
                "tool_name": "file_write", "arg_prefix": null, "action": "allow", "scope": "project"
            })))
            .respond_with(ResponseTemplate::new(400).set_body_json(serde_json::json!({
                "error": "unknown scope 'project', expected 'global' (a rule targets one agent or every agent)"
            })))
            .mount(&server)
            .await;
        let client = AttachedClient::new(server.uri(), TOKEN).unwrap();
        let rule = |prefix: Option<&str>, scope: &str| AddPermissionRuleBody {
            tool_name: "file_write".into(),
            arg_prefix: prefix.map(str::to_owned),
            action: "allow".into(),
            scope: scope.into(),
        };

        // WHEN the attached arm adds both
        let stored = add_rule_attached(&client, &rule(Some("/tmp/"), "global")).await;
        let refused = add_rule_attached(&client, &rule(None, "project"))
            .await
            .unwrap_err();

        // THEN the first is stored by the daemon and the second shows its words
        assert!(stored.is_ok(), "{stored:?}");
        assert!(refused.contains("unknown scope 'project'"), "{refused}");
    }

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
