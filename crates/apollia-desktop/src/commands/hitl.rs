//! Tauri IPC commands for Human-in-the-Loop (HITL).
//!
//! `list_pending_approvals` cross-references data from `PendingApprovals`
//! (in-memory pending tasks) with `TaskRepository` (SQLite details: prompt,
//! context, timestamp).
//!
//! `list_pending_decisions` and `decide_pending_decision` reach the one list of
//! every decision waiting for the user and the one call that answers an item
//! of it, over `/api/v1/pending-decisions`: the task pauses above, the
//! consents of the conversations, the batch items and the questions of agents.

use apollia_runtime::api::routes_permissions::{AddPermissionRuleBody, AddedPermissionRule};
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::attached::{AttachedClient, RuntimeLink};

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

/// Route of the decisions waiting for the user.
const PENDING_DECISIONS: &str = "/api/v1/pending-decisions";

/// Every decision waiting for the user: the consents of the conversations,
/// the task pauses, the batch items and the questions of agents, each with
/// the answers it takes, as `GET /api/v1/pending-decisions` lists them.
#[tauri::command]
pub async fn list_pending_decisions(
    link: State<'_, RuntimeLink>,
) -> Result<serde_json::Value, String> {
    link.get_json(PENDING_DECISIONS).await
}

/// Answers one waiting decision over `POST /api/v1/pending-decisions/decide`,
/// which goes through the call that answers it where it was born: the
/// conversation, the task or the batch goes on as if it had been answered
/// there. `request` is the body of that route.
#[tauri::command]
pub async fn decide_pending_decision(
    link: State<'_, RuntimeLink>,
    request: serde_json::Value,
) -> Result<serde_json::Value, String> {
    link.post_json(&format!("{PENDING_DECISIONS}/decide"), &request)
        .await
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
}
