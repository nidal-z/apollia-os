//! Human-readable rendering of the task and plan payloads.

use apollia_oria::plan_repository::PlanWithSteps;

use crate::note;

use super::util::{
    elapsed_seconds, extract_tasks_array, format_duration_since, step_duration, step_status_icon,
    truncate_output, truncate_prompt,
};

/// Format task list as a human-readable table.
///
/// The `agent_names` map (agent_id → name) is populated by the caller from
/// `GET /api/v1/agents`. When the lookup misses (deleted agent, stale UUID)
/// we display the truncated UUID instead so the column still aligns and
/// the operator can still copy-paste it.
pub(super) fn format_task_list(
    resp: &serde_json::Value,
    agent_names: &std::collections::HashMap<String, String>,
) {
    let tasks = extract_tasks_array(resp);

    println!("  {:<36} {:<26} {:<12}", "TASK_ID", "AGENT", "STATUS");

    if tasks.is_empty() {
        println!("  (no tasks)");
    } else {
        for task in &tasks {
            let id = task.get("task_id").and_then(|v| v.as_str()).unwrap_or("?");
            let agent_id = task.get("agent_id").and_then(|v| v.as_str()).unwrap_or("?");
            let agent_label = agent_names
                .get(agent_id)
                .cloned()
                .unwrap_or_else(|| short_uuid(agent_id));
            let status = task.get("status").and_then(|v| v.as_str()).unwrap_or("?");
            println!("  {:<36} {:<26} {status}", id, agent_label);
        }
    }
}

/// Shorten a UUID to its first segment for display when the agent name is
/// unknown. Keeps the column aligned and the value copy-paste-traceable.
pub(super) fn short_uuid(uuid: &str) -> String {
    uuid.split('-').next().unwrap_or(uuid).to_string()
}

/// Render pending-approval tasks as a human-readable table.
///
/// Columns: `TASK_ID | AGENT | SINCE | PROMPT` (prompt truncated to 60 chars).
pub(super) fn format_pending_table(tasks: &[serde_json::Value]) {
    println!("  {:<36} {:<20} {:<8} PROMPT", "TASK_ID", "AGENT", "SINCE");

    if tasks.is_empty() {
        println!("  (no pending approvals)");
        return;
    }

    for task in tasks {
        let id = task.get("task_id").and_then(|v| v.as_str()).unwrap_or("?");
        let agent = task
            .get("agent_id")
            .or_else(|| task.get("agent"))
            .and_then(|v| v.as_str())
            .unwrap_or("?");
        let since = task
            .get("input_required_at")
            .and_then(|v| v.as_str())
            .map(format_duration_since)
            .unwrap_or_else(|| "-".to_string());
        let prompt = task
            .get("input_required_prompt")
            .or_else(|| task.get("prompt"))
            .and_then(|v| v.as_str())
            .unwrap_or("");

        println!(
            "  {:<36} {:<20} {:<8} \"{}\"",
            id,
            agent,
            since,
            truncate_prompt(prompt)
        );
    }
}

/// Build the JSON array for `--pending-approval --json` output.
///
/// Each element has: `task_id`, `agent`, `waiting_since_secs`, `prompt`, `step_id`.
pub fn build_pending_json(tasks: &[serde_json::Value]) -> Vec<serde_json::Value> {
    tasks
        .iter()
        .map(|task| {
            let task_id = task.get("task_id").and_then(|v| v.as_str()).unwrap_or("?");
            let agent = task
                .get("agent_id")
                .or_else(|| task.get("agent"))
                .and_then(|v| v.as_str())
                .unwrap_or("?");
            let prompt = task
                .get("input_required_prompt")
                .or_else(|| task.get("prompt"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let step_id = task
                .get("step_id")
                .cloned()
                .unwrap_or(serde_json::Value::Null);
            let waiting_since_secs = task
                .get("input_required_at")
                .and_then(|v| v.as_str())
                .map(elapsed_seconds)
                .unwrap_or(0);

            serde_json::json!({
                "task_id": task_id,
                "agent": agent,
                "waiting_since_secs": waiting_since_secs,
                "prompt": prompt,
                "step_id": step_id,
            })
        })
        .collect()
}

/// Render an orchestrated plan as a human-readable table on stdout.
///
/// Displays a header with task metadata followed by one block per step showing
/// its status icon, description, tool hint, output (truncated to 120 chars), and error if any.
pub(super) fn display_plan_human(plan: &PlanWithSteps) {
    note!();
    println!("  Task        : {}", plan.task_id);
    println!("  Agent       : {}", plan.agent_name);
    println!("  Mode        : orchestrated");
    println!("  Status      : {}", plan.status);
    println!(
        "  Created     : {}",
        format_rfc3339_compact(&plan.created_at)
    );
    println!("  Replans     : {}/2", plan.replan_count);
    note!();
    note!("  Execution plan:");

    for step in &plan.steps {
        let icon = step_status_icon(&step.status);
        let replan_marker = if step.step_id.ends_with('b') || step.step_id.ends_with('c') {
            "  [replanned]"
        } else {
            ""
        };

        note!();
        println!(
            "  {} [{}]  {}{}",
            icon, step.step_id, step.description, replan_marker
        );

        if let Some(ref tool) = step.tool_hint {
            println!(
                "          tool: {} | duration: {}",
                tool,
                step_duration(step)
            );
        }

        if let Some(ref output) = step.output {
            let truncated = truncate_output(output);
            println!("          output: \"{truncated}\"");
        }

        if let Some(ref error) = step.error {
            println!("          error: \"{error}\"");
            if step.status == "failed" {
                note!("          → replanning triggered");
            }
        }
    }
    note!();
}

/// Render an RFC3339 timestamp as `YYYY-MM-DD HH:MM:SS` for compact display.
///
/// Falls back to the raw value when parsing fails; the formatter must
/// never lose information just because the runtime grew a new format.
pub(super) fn format_rfc3339_compact(raw: &str) -> String {
    chrono::DateTime::parse_from_rfc3339(raw)
        .map(|dt| {
            dt.with_timezone(&chrono::Utc)
                .format("%Y-%m-%d %H:%M:%S")
                .to_string()
        })
        .unwrap_or_else(|_| raw.to_string())
}

/// Serialize a plan with its steps to a `serde_json::Value` for `--json` output.
///
/// Produces the JSON structure :
/// `{ plan_id, task_id, agent_name, status, replan_count, created_at, steps: [...] }`.
pub(super) fn plan_to_json(plan: &PlanWithSteps) -> serde_json::Value {
    serde_json::json!({
        "plan_id":      plan.plan_id,
        "task_id":      plan.task_id,
        "agent_name":   plan.agent_name,
        "status":       plan.status,
        "replan_count": plan.replan_count,
        "created_at":   plan.created_at,
        "steps": plan.steps.iter().map(|s| serde_json::json!({
            "step_id":      s.step_id,
            "description":  s.description,
            "tool_hint":    s.tool_hint,
            "depends_on":   s.depends_on,
            "status":       s.status,
            "output":       s.output,
            "error":        s.error,
            "started_at":   s.started_at,
            "completed_at": s.completed_at,
        })).collect::<Vec<_>>()
    })
}
