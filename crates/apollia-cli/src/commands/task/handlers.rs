//! One function per `apollia-os task` verb.

use apollia_oria::plan_repository::PlanRepositoryError;

use crate::client::RuntimeClient;
use crate::exit_codes;
use crate::note;

use super::display::{
    build_pending_json, display_plan_human, format_pending_table, format_task_list, plan_to_json,
};
use super::util::{
    extract_error_message, extract_tasks_array, format_approvals_list, handle_error,
    handle_server_error,
};

/// `apollia-os task list`: display recent tasks.
///
/// Resolves `agent_id` (UUID) to the human-readable agent name by fetching
/// the agents list once and joining locally. The runtime does not embed the
/// agent name in `GET /api/v1/tasks` responses, and a UUID-only table is
/// hostile to operators.
pub(super) async fn run_list(client: &RuntimeClient, json: bool) -> i32 {
    let resp = match client.get("/api/v1/tasks").await {
        Ok(r) => r,
        Err(e) => return handle_error(e, json),
    };

    if resp.status >= 400 {
        return handle_server_error(resp.status, &resp.body, json);
    }

    let parsed: serde_json::Value = match serde_json::from_str(&resp.body) {
        Ok(v) => v,
        Err(e) => {
            return crate::output::emit_error(
                json,
                exit_codes::GENERAL_ERROR,
                &format!("invalid JSON response: {e}"),
            );
        }
    };

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&parsed).unwrap_or_default()
        );
    } else {
        // Best-effort agent_id → name lookup. If the agents endpoint is
        // unreachable, we just fall back to displaying the raw UUIDs.
        let agent_names = client
            .list_agents()
            .await
            .ok()
            .and_then(|v| {
                v.get("agents")
                    .or(Some(&v))
                    .and_then(|x| x.as_array())
                    .cloned()
            })
            .map(|agents| {
                agents
                    .iter()
                    .filter_map(|a| {
                        let id = a.get("agent_id").or_else(|| a.get("id"))?.as_str()?;
                        let name = a.get("name")?.as_str()?;
                        Some((id.to_string(), name.to_string()))
                    })
                    .collect::<std::collections::HashMap<String, String>>()
            })
            .unwrap_or_default();
        format_task_list(&parsed, &agent_names);
    }
    exit_codes::SUCCESS
}

/// `apollia-os task list --pending-approval`: display tasks awaiting HITL approval.
///
/// Calls `GET /api/v1/tasks?status=input_required` and renders a table with
/// `TASK_ID | AGENT | DEPUIS | PROMPT` columns, or a JSON array.
pub(super) async fn run_list_pending(client: &RuntimeClient, json: bool) -> i32 {
    let resp = match client.get("/api/v1/tasks?status=input_required").await {
        Ok(r) => r,
        Err(e) => return handle_error(e, json),
    };

    if resp.status >= 400 {
        return handle_server_error(resp.status, &resp.body, json);
    }

    let parsed: serde_json::Value = match serde_json::from_str(&resp.body) {
        Ok(v) => v,
        Err(e) => {
            return crate::output::emit_error(
                json,
                exit_codes::GENERAL_ERROR,
                &format!("invalid JSON response: {e}"),
            );
        }
    };

    let tasks = extract_tasks_array(&parsed);

    if json {
        let output = build_pending_json(&tasks);
        match serde_json::to_string_pretty(&output) {
            Ok(s) => println!("{s}"),
            Err(e) => {
                return crate::output::emit_error(
                    json,
                    exit_codes::GENERAL_ERROR,
                    &format!("JSON serialization failed: {e}"),
                );
            }
        }
    } else {
        format_pending_table(&tasks);
    }
    exit_codes::SUCCESS
}

/// Prints an indented, labelled multi-line block when the field is present.
pub(super) fn print_status_block(resp: &serde_json::Value, key: &str, label: &str) {
    let Some(text) = resp.get(key).and_then(|v| v.as_str()) else {
        return;
    };
    println!("  {label}:");
    for line in text.lines() {
        println!("    {line}");
    }
}

/// Prints the token budget line when usage is recorded.
pub(super) fn print_token_budget(resp: &serde_json::Value) {
    let Some(budget) = resp.get("token_budget") else {
        return;
    };
    let input = budget
        .get("input_tokens")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let output = budget
        .get("output_tokens")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    if input + output > 0 {
        println!("  Tokens    : {input} in / {output} out");
    }
}

/// Renders the human-readable status summary for a task response.
pub(super) fn print_status_human(resp: &serde_json::Value, task_id: &str) {
    let status = resp.get("status").and_then(|v| v.as_str()).unwrap_or("?");
    println!("  Task      : {task_id}");
    println!("  Status    : {status}");
    print_status_block(resp, "error", "Error     ");
    print_status_block(resp, "result", "Result    ");
    print_token_budget(resp);
}

/// `apollia-os task status <id>`: display task status.
pub(super) async fn run_status(client: &RuntimeClient, task_id: &str, json: bool) -> i32 {
    let resp = match client.get_task(task_id).await {
        Ok(resp) => resp,
        Err(e) => return handle_error(e, json),
    };
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&resp).unwrap_or_default()
        );
    } else {
        print_status_human(&resp, task_id);
    }
    exit_codes::SUCCESS
}

/// `apollia-os task cancel <id>`: cancel a running task.
pub(super) async fn run_cancel(
    client: &RuntimeClient,
    task_id: &str,
    confirm: bool,
    json: bool,
) -> i32 {
    if let Some(code) =
        crate::output::require_confirmation(confirm, json, &format!("cancel task '{task_id}'"))
    {
        return code;
    }
    match client.cancel_task(task_id).await {
        Ok(resp) => {
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&resp).unwrap_or_default()
                );
            } else {
                println!("Task {task_id} canceled");
            }
            exit_codes::SUCCESS
        }
        Err(e) => handle_error(e, json),
    }
}

/// Decision fields supplied to `apollia-os task resume`.
pub(super) struct ResumeArgs<'a> {
    pub(super) task_id: &'a str,
    pub(super) approve: bool,
    pub(super) reject: bool,
    pub(super) reason: Option<String>,
}

/// `apollia-os task resume <id> --approve|--reject [--reason "..."]`
///
/// Posts `{ approved: bool, reason?: String }` to
/// `POST /api/v1/tasks/{id}/resume` and prints the result.
pub(super) async fn run_resume(client: &RuntimeClient, args: ResumeArgs<'_>, json: bool) -> i32 {
    let ResumeArgs {
        task_id,
        approve,
        reject,
        reason,
    } = args;
    // Manual guard: clap groups make --approve/--reject mutually exclusive but
    // not required; validate the "neither" case here.
    if !approve && !reject {
        return crate::output::emit_error(
            json,
            exit_codes::GENERAL_ERROR,
            "one of --approve or --reject must be specified",
        );
    }

    let approved = approve;
    let resp = match client.resume_task(task_id, approved, reason).await {
        Ok(r) => r,
        Err(e) => return handle_error(e, json),
    };

    // HTTP 409 means the task is not in input_required state.
    if resp.status == 409 {
        let msg = extract_error_message(&resp, "task is not awaiting approval");
        return crate::output::emit_error(json, exit_codes::GENERAL_ERROR, &msg);
    }

    if resp.status >= 400 {
        return handle_server_error(resp.status, &resp.body, json);
    }

    let parsed: serde_json::Value = match serde_json::from_str(&resp.body) {
        Ok(v) => v,
        Err(e) => {
            return crate::output::emit_error(
                json,
                exit_codes::GENERAL_ERROR,
                &format!("invalid JSON response: {e}"),
            );
        }
    };

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&parsed).unwrap_or_default()
        );
    } else {
        let agent = parsed
            .get("agent_id")
            .or_else(|| parsed.get("agent"))
            .and_then(|v| v.as_str())
            .unwrap_or("?");
        let status = parsed.get("status").and_then(|v| v.as_str()).unwrap_or("?");

        if approved {
            note!("✔ Task {task_id} approved - {agent} › {status}...");
        } else {
            note!("✔ Task {task_id} rejected - {agent} › done ({status})");
        }
    }
    exit_codes::SUCCESS
}

/// `apollia-os task inspect <id>`: display the full execution plan of an orchestrated task.
///
/// Opens `~/.apollia/plans.db` directly (no runtime required) and renders the plan
/// with per-step statuses, outputs, and errors. On `NotFound`, exits with code 0 and
/// an informative message (direct-mode tasks have no persisted plan, this is normal).
pub(super) fn run_inspect(task_id: &str, json: bool) -> i32 {
    let db_path = {
        let home = apollia_core::paths::home_dir().unwrap_or_default();
        apollia_core::paths::DataFile::Plans
            .path(&apollia_core::paths::data_dir_under(home))
            .display()
            .to_string()
    };

    let repo = match apollia_oria::plan_repository::PlanRepository::new(&db_path) {
        Ok(r) => r,
        Err(e) => {
            return crate::output::emit_error(
                json,
                exit_codes::GENERAL_ERROR,
                &format!("failed to open the plans database: {e}"),
            );
        }
    };

    match repo.get_plan_with_steps(task_id) {
        Ok(plan) => {
            if json {
                let json_val = plan_to_json(&plan);
                match serde_json::to_string_pretty(&json_val) {
                    Ok(s) => println!("{s}"),
                    Err(e) => {
                        return crate::output::emit_error(
                            json,
                            exit_codes::GENERAL_ERROR,
                            &format!("JSON serialization failed: {e}"),
                        );
                    }
                }
            } else {
                display_plan_human(&plan);
            }
            exit_codes::SUCCESS
        }
        Err(PlanRepositoryError::NotFound(_)) => {
            if json {
                println!(
                    "{}",
                    serde_json::json!({
                        "task_id": task_id,
                        "plan": null,
                        "reason": "no_plan_persisted",
                    })
                );
            } else {
                println!("Task {task_id} has no execution plan.");
                note!();
                println!("Plans are only generated for agents with `execution_mode =");
                println!("\"orchestrated\"` in their manifest. Agents marked `direct` drive");
                println!("their own logic (state machines, A2A delegation, ReAct loops) and");
                println!("never go through the ORIA Reasoner, so plans.db has nothing to");
                println!("show for them.");
                note!();
                println!("To see a real plan, run an orchestrated agent (e.g. email-triage)");
                println!("then re-run `apollia-os task inspect <id>`.");
            }
            exit_codes::SUCCESS
        }
        Err(e) => crate::output::emit_error(json, exit_codes::GENERAL_ERROR, &e.to_string()),
    }
}

/// `apollia-os task approvals [--pending]`: list resolved (or pending) HITL approvals.
///
/// Without `--pending`: calls `GET /api/v1/approvals/resolved`.
/// With `--pending`: calls `GET /api/v1/approvals/pending`.
pub(super) async fn run_approvals(client: &RuntimeClient, pending: bool, json: bool) -> i32 {
    let uri = if pending {
        "/api/v1/approvals/pending"
    } else {
        "/api/v1/approvals/resolved"
    };

    let resp = match client.get(uri).await {
        Ok(r) => r,
        Err(e) => return handle_error(e, json),
    };

    if resp.status >= 400 {
        return handle_server_error(resp.status, &resp.body, json);
    }

    let parsed: serde_json::Value = match serde_json::from_str(&resp.body) {
        Ok(v) => v,
        Err(e) => {
            return crate::output::emit_error(
                json,
                exit_codes::GENERAL_ERROR,
                &format!("invalid JSON response: {e}"),
            );
        }
    };

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&parsed).unwrap_or_default()
        );
    } else {
        format_approvals_list(&parsed, pending);
    }
    exit_codes::SUCCESS
}
