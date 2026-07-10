//! The `plan_*` built-in tool surface of conversational plan mode.
//!
//! Mirrors `todo_tool.rs`: each function is stateless, parses its arguments,
//! delegates to the [`PlanHandle`] and shapes a JSON-serializable result for the
//! LLM. No function panics; failures come back as `ok: false` payloads. The
//! tools are advertised and executed in the ReAct loop only while the session is
//! in plan mode.
//!
//! `plan_modify_step` and `plan_remove_step` require a non-empty `reason`: any
//! modification or removal must be justified for audit and replay, and the
//! requirement is enforced here before the handle is ever touched, so an agent
//! cannot bypass it.

use apollia_core::plan::{Plan, PlanStep, StepStatus};
use apollia_llm::types::ToolSpec;

use super::plan_actor::{PlanHandle, PlanStoreError};

/// Name under which `plan_propose` is advertised and matched in the loop.
pub const PLAN_PROPOSE_TOOL_NAME: &str = "plan_propose";
/// Name under which `plan_add_step` is advertised and matched in the loop.
pub const PLAN_ADD_STEP_TOOL_NAME: &str = "plan_add_step";
/// Name under which `plan_modify_step` is advertised and matched in the loop.
pub const PLAN_MODIFY_STEP_TOOL_NAME: &str = "plan_modify_step";
/// Name under which `plan_remove_step` is advertised and matched in the loop.
pub const PLAN_REMOVE_STEP_TOOL_NAME: &str = "plan_remove_step";
/// Name under which `plan_reorder` is advertised and matched in the loop.
pub const PLAN_REORDER_TOOL_NAME: &str = "plan_reorder";
/// Name under which `plan_set_step_status` is advertised and matched in the loop.
pub const PLAN_SET_STEP_STATUS_TOOL_NAME: &str = "plan_set_step_status";
/// Name under which `plan_submit` is advertised and matched in the loop.
pub const PLAN_SUBMIT_TOOL_NAME: &str = "plan_submit";

/// Result returned to the LLM after a `plan_*` tool invocation.
#[derive(Debug, serde::Serialize)]
pub struct PlanToolResult {
    /// Whether the operation succeeded.
    pub ok: bool,
    /// Error message when `ok` is false; omitted otherwise.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Snapshot of the resulting plan when `ok` is true; omitted otherwise.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plan: Option<Plan>,
}

impl PlanToolResult {
    fn ok(plan: Plan) -> Self {
        Self {
            ok: true,
            error: None,
            plan: Some(plan),
        }
    }

    fn failure(message: impl Into<String>) -> Self {
        Self {
            ok: false,
            error: Some(message.into()),
            plan: None,
        }
    }
}

/// Human-readable message for a [`PlanStoreError`] surfaced to the LLM.
fn describe(error: &PlanStoreError) -> String {
    match error {
        PlanStoreError::Validation(e) => format!("invalid plan: {e}"),
        other => other.to_string(),
    }
}

/// All `plan_*` tool specs, advertised only when the session is in plan mode.
#[must_use]
pub fn plan_tool_specs() -> Vec<ToolSpec> {
    vec![
        plan_propose_spec(),
        plan_add_step_spec(),
        plan_modify_step_spec(),
        plan_remove_step_spec(),
        plan_reorder_spec(),
        plan_set_step_status_spec(),
        plan_submit_spec(),
    ]
}

/// Executes `plan_propose`: replaces the session's draft plan.
pub async fn run_plan_propose(
    handle: &PlanHandle,
    session_id: &str,
    raw_args: &serde_json::Value,
) -> PlanToolResult {
    let steps = match parse_steps(raw_args) {
        Ok(s) => s,
        Err(msg) => return PlanToolResult::failure(msg),
    };
    let summary = raw_args
        .get("summary")
        .and_then(serde_json::Value::as_str)
        .map(str::to_string);
    match handle.propose(session_id, steps, summary).await {
        Ok(plan) => PlanToolResult::ok(plan),
        Err(e) => PlanToolResult::failure(describe(&e)),
    }
}

/// Executes `plan_add_step`: appends a step to the current plan.
pub async fn run_plan_add_step(
    handle: &PlanHandle,
    session_id: &str,
    raw_args: &serde_json::Value,
) -> PlanToolResult {
    let step = match parse_step(raw_args, "step") {
        Ok(s) => s,
        Err(msg) => return PlanToolResult::failure(msg),
    };
    let reason = optional_reason(raw_args);
    match handle.add_step(session_id, step, reason).await {
        Ok(plan) => PlanToolResult::ok(plan),
        Err(e) => PlanToolResult::failure(describe(&e)),
    }
}

/// Executes `plan_modify_step`: the reason is mandatory.
pub async fn run_plan_modify_step(
    handle: &PlanHandle,
    session_id: &str,
    raw_args: &serde_json::Value,
) -> PlanToolResult {
    let step_id = match parse_step_id(raw_args) {
        Ok(id) => id,
        Err(msg) => return PlanToolResult::failure(msg),
    };
    let reason = match required_reason(raw_args) {
        Ok(r) => r,
        Err(msg) => return PlanToolResult::failure(msg),
    };
    let after = match parse_step(raw_args, "step") {
        Ok(s) => s,
        Err(msg) => return PlanToolResult::failure(msg),
    };
    match handle
        .modify_step(session_id, &step_id, after, Some(reason))
        .await
    {
        Ok(plan) => PlanToolResult::ok(plan),
        Err(e) => PlanToolResult::failure(describe(&e)),
    }
}

/// Executes `plan_remove_step`: the reason is mandatory.
pub async fn run_plan_remove_step(
    handle: &PlanHandle,
    session_id: &str,
    raw_args: &serde_json::Value,
) -> PlanToolResult {
    let step_id = match parse_step_id(raw_args) {
        Ok(id) => id,
        Err(msg) => return PlanToolResult::failure(msg),
    };
    let reason = match required_reason(raw_args) {
        Ok(r) => r,
        Err(msg) => return PlanToolResult::failure(msg),
    };
    match handle.remove_step(session_id, &step_id, Some(reason)).await {
        Ok(plan) => PlanToolResult::ok(plan),
        Err(e) => PlanToolResult::failure(describe(&e)),
    }
}

/// Executes `plan_reorder`: reorders the steps to match an explicit order.
pub async fn run_plan_reorder(
    handle: &PlanHandle,
    session_id: &str,
    raw_args: &serde_json::Value,
) -> PlanToolResult {
    let ordered_ids: Vec<String> = match raw_args.get("ordered_ids").cloned() {
        Some(v) => match serde_json::from_value(v) {
            Ok(ids) => ids,
            Err(e) => return PlanToolResult::failure(format!("invalid payload: {e}")),
        },
        None => return PlanToolResult::failure("invalid payload: missing 'ordered_ids' field"),
    };
    let reason = optional_reason(raw_args);
    match handle.reorder(session_id, ordered_ids, reason).await {
        Ok(plan) => PlanToolResult::ok(plan),
        Err(e) => PlanToolResult::failure(describe(&e)),
    }
}

/// Executes `plan_set_step_status`: changes the status of a step.
pub async fn run_plan_set_step_status(
    handle: &PlanHandle,
    session_id: &str,
    raw_args: &serde_json::Value,
) -> PlanToolResult {
    let step_id = match parse_step_id(raw_args) {
        Ok(id) => id,
        Err(msg) => return PlanToolResult::failure(msg),
    };
    let status = match raw_args.get("status").and_then(serde_json::Value::as_str) {
        Some(s) => match parse_status(s) {
            Some(status) => status,
            None => return PlanToolResult::failure(format!("invalid status: {s}")),
        },
        None => return PlanToolResult::failure("invalid payload: missing 'status' field"),
    };
    let reason = optional_reason(raw_args);
    match handle
        .set_step_status(session_id, &step_id, status, reason)
        .await
    {
        Ok(plan) => PlanToolResult::ok(plan),
        Err(e) => PlanToolResult::failure(describe(&e)),
    }
}

/// Executes `plan_submit`: submits the plan for approval.
pub async fn run_plan_submit(handle: &PlanHandle, session_id: &str) -> PlanToolResult {
    match handle.submit(session_id).await {
        Ok(plan) => PlanToolResult::ok(plan),
        Err(e) => PlanToolResult::failure(describe(&e)),
    }
}

/// Parse the full step list from a `steps` array argument.
fn parse_steps(raw_args: &serde_json::Value) -> Result<Vec<PlanStep>, String> {
    match raw_args.get("steps").cloned() {
        Some(v) => serde_json::from_value(v).map_err(|e| format!("invalid payload: {e}")),
        None => Err("invalid payload: missing 'steps' field".to_string()),
    }
}

/// Parse a single step object from the named field.
fn parse_step(raw_args: &serde_json::Value, field: &str) -> Result<PlanStep, String> {
    match raw_args.get(field).cloned() {
        Some(v) => serde_json::from_value(v).map_err(|e| format!("invalid payload: {e}")),
        None => Err(format!("invalid payload: missing '{field}' field")),
    }
}

/// Parse the `step_id` argument.
fn parse_step_id(raw_args: &serde_json::Value) -> Result<String, String> {
    match raw_args.get("step_id").and_then(serde_json::Value::as_str) {
        Some(id) if !id.is_empty() => Ok(id.to_string()),
        _ => Err("invalid payload: missing 'step_id'".to_string()),
    }
}

/// Extract an optional reason (`None` when absent or blank).
fn optional_reason(raw_args: &serde_json::Value) -> Option<String> {
    raw_args
        .get("reason")
        .and_then(serde_json::Value::as_str)
        .filter(|r| !r.trim().is_empty())
        .map(str::to_string)
}

/// Extract a non-empty reason; explicit error otherwise.
fn required_reason(raw_args: &serde_json::Value) -> Result<String, String> {
    match raw_args.get("reason").and_then(serde_json::Value::as_str) {
        Some(r) if !r.trim().is_empty() => Ok(r.to_string()),
        _ => Err("reason is required for this operation".to_string()),
    }
}

/// Parse a [`StepStatus`] from its wire string.
fn parse_status(raw: &str) -> Option<StepStatus> {
    match raw {
        "pending" => Some(StepStatus::Pending),
        "in_progress" => Some(StepStatus::InProgress),
        "completed" => Some(StepStatus::Completed),
        "skipped" => Some(StepStatus::Skipped),
        "failed" => Some(StepStatus::Failed),
        _ => None,
    }
}

/// JSON schema fragment describing a single plan step payload.
fn step_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "required": ["step_id", "description"],
        "properties": {
            "step_id":     { "type": "string" },
            "title":       { "type": "string" },
            "description": { "type": "string" },
            "status": {
                "type": "string",
                "enum": ["pending", "in_progress", "completed", "skipped", "failed"]
            },
            "depends_on":  { "type": "array", "items": { "type": "string" } },
            "tool_hint":   { "type": "string" },
            "model_hint":  { "type": "string" },
            "rationale":   { "type": "string" }
        }
    })
}

/// Build the [`ToolSpec`] for `plan_propose`.
#[must_use]
pub fn plan_propose_spec() -> ToolSpec {
    ToolSpec {
        name: PLAN_PROPOSE_TOOL_NAME.to_string(),
        description: "Replace the current draft plan with a complete list of steps. Use this to \
                      lay out or fully restate the plan. Each step needs a unique 'step_id' and a \
                      'description'; 'depends_on' lists the step ids that must finish first."
            .to_string(),
        parameters: serde_json::json!({
            "type": "object",
            "required": ["steps"],
            "properties": {
                "steps": {
                    "type": "array",
                    "description": "Complete replacement list of plan steps.",
                    "items": step_schema()
                },
                "summary": { "type": "string", "description": "Optional one-line plan summary." }
            }
        }),
    }
}

/// Build the [`ToolSpec`] for `plan_add_step`.
#[must_use]
pub fn plan_add_step_spec() -> ToolSpec {
    ToolSpec {
        name: PLAN_ADD_STEP_TOOL_NAME.to_string(),
        description: "Append a single step to the current plan. The new step's dependencies must \
                      reference existing step ids."
            .to_string(),
        parameters: serde_json::json!({
            "type": "object",
            "required": ["step"],
            "properties": {
                "step": step_schema(),
                "reason": { "type": "string", "description": "Why this step is added." }
            }
        }),
    }
}

/// Build the [`ToolSpec`] for `plan_modify_step`.
#[must_use]
pub fn plan_modify_step_spec() -> ToolSpec {
    ToolSpec {
        name: PLAN_MODIFY_STEP_TOOL_NAME.to_string(),
        description: "Modify an existing plan step. A non-empty 'reason' is required and recorded \
                      in the plan history."
            .to_string(),
        parameters: serde_json::json!({
            "type": "object",
            "required": ["step_id", "step", "reason"],
            "properties": {
                "step_id": { "type": "string", "description": "Id of the step to modify." },
                "step":    step_schema(),
                "reason":  { "type": "string", "description": "Why this step is modified." }
            }
        }),
    }
}

/// Build the [`ToolSpec`] for `plan_remove_step`.
#[must_use]
pub fn plan_remove_step_spec() -> ToolSpec {
    ToolSpec {
        name: PLAN_REMOVE_STEP_TOOL_NAME.to_string(),
        description:
            "Remove a step from the plan. A non-empty 'reason' is required and the removed \
                      step is kept as a tombstone in the plan history."
                .to_string(),
        parameters: serde_json::json!({
            "type": "object",
            "required": ["step_id", "reason"],
            "properties": {
                "step_id": { "type": "string", "description": "Id of the step to remove." },
                "reason":  { "type": "string", "description": "Why this step is removed." }
            }
        }),
    }
}

/// Build the [`ToolSpec`] for `plan_reorder`.
#[must_use]
pub fn plan_reorder_spec() -> ToolSpec {
    ToolSpec {
        name: PLAN_REORDER_TOOL_NAME.to_string(),
        description:
            "Reorder the plan steps. 'ordered_ids' must list exactly the current step ids \
                      in the desired order."
                .to_string(),
        parameters: serde_json::json!({
            "type": "object",
            "required": ["ordered_ids"],
            "properties": {
                "ordered_ids": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Every current step id, in the desired order."
                },
                "reason": { "type": "string", "description": "Why the order changes." }
            }
        }),
    }
}

/// Build the [`ToolSpec`] for `plan_set_step_status`.
#[must_use]
pub fn plan_set_step_status_spec() -> ToolSpec {
    ToolSpec {
        name: PLAN_SET_STEP_STATUS_TOOL_NAME.to_string(),
        description: "Update the execution status of a plan step as work progresses.".to_string(),
        parameters: serde_json::json!({
            "type": "object",
            "required": ["step_id", "status"],
            "properties": {
                "step_id": { "type": "string", "description": "Id of the step." },
                "status": {
                    "type": "string",
                    "enum": ["pending", "in_progress", "completed", "skipped", "failed"]
                },
                "reason": { "type": "string", "description": "Optional status change reason." }
            }
        }),
    }
}

/// Build the [`ToolSpec`] for `plan_submit`.
#[must_use]
pub fn plan_submit_spec() -> ToolSpec {
    ToolSpec {
        name: PLAN_SUBMIT_TOOL_NAME.to_string(),
        description: "Submit the plan for approval once it is complete. The plan moves to \
                      'awaiting_approval' and the user decides whether to execute it."
            .to_string(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {}
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chat::plan_actor::spawn_plan_actor;
    use rusqlite::Connection;

    fn handle() -> PlanHandle {
        spawn_plan_actor(Connection::open_in_memory().expect("open"), None).expect("spawn")
    }

    fn step_json(id: &str) -> serde_json::Value {
        serde_json::json!({
            "step_id": id, "title": id, "description": "d",
            "status": "pending", "depends_on": []
        })
    }

    #[test]
    fn test_specs_listed_for_plan_mode() {
        // GIVEN the plan tool spec list
        let names: Vec<String> = plan_tool_specs().into_iter().map(|s| s.name).collect();

        // WHEN inspecting the names
        // THEN all seven plan_* tools are advertised
        assert!(names.contains(&PLAN_PROPOSE_TOOL_NAME.to_string()));
        assert!(names.contains(&PLAN_ADD_STEP_TOOL_NAME.to_string()));
        assert!(names.contains(&PLAN_MODIFY_STEP_TOOL_NAME.to_string()));
        assert!(names.contains(&PLAN_REMOVE_STEP_TOOL_NAME.to_string()));
        assert!(names.contains(&PLAN_REORDER_TOOL_NAME.to_string()));
        assert!(names.contains(&PLAN_SET_STEP_STATUS_TOOL_NAME.to_string()));
        assert!(names.contains(&PLAN_SUBMIT_TOOL_NAME.to_string()));
        assert_eq!(names.len(), 7);
    }

    #[test]
    fn test_required_fields_declared_on_reason_tools() {
        // GIVEN the modify and remove specs
        let modify = plan_modify_step_spec();
        let remove = plan_remove_step_spec();

        // WHEN inspecting their required arrays
        // THEN both declare 'reason' as required
        assert!(modify.parameters["required"]
            .as_array()
            .expect("array")
            .iter()
            .any(|v| v == "reason"));
        assert!(remove.parameters["required"]
            .as_array()
            .expect("array")
            .iter()
            .any(|v| v == "reason"));
    }

    #[tokio::test]
    async fn test_propose_returns_snapshot_and_mutates() {
        // GIVEN a plan actor and a valid propose payload
        let h = handle();
        let args = serde_json::json!({ "steps": [step_json("a")], "summary": "s" });

        // WHEN running plan_propose
        let result = run_plan_propose(&h, "s1", &args).await;

        // THEN it succeeds, returns the snapshot, and the plan is readable
        assert!(result.ok);
        assert_eq!(result.plan.expect("plan").steps.len(), 1);
        assert!(h.get_plan("s1").await.expect("get").is_some());
    }

    #[tokio::test]
    async fn test_add_step_records_mutation() {
        // GIVEN a proposed plan
        let h = handle();
        run_plan_propose(&h, "s1", &serde_json::json!({ "steps": [step_json("a")] })).await;

        // WHEN adding a step with a reason
        let args = serde_json::json!({ "step": step_json("b"), "reason": "more work" });
        let result = run_plan_add_step(&h, "s1", &args).await;

        // THEN the snapshot contains the new step and the plan persists it
        assert!(result.ok);
        let plan = result.plan.expect("plan");
        assert!(plan.steps.iter().any(|s| s.step_id == "b"));
    }

    #[tokio::test]
    async fn test_modify_step_without_reason_rejected() {
        // GIVEN a proposed plan
        let h = handle();
        run_plan_propose(&h, "s1", &serde_json::json!({ "steps": [step_json("a")] })).await;

        // WHEN modifying a step with no reason
        let args = serde_json::json!({ "step_id": "a", "step": step_json("a") });
        let result = run_plan_modify_step(&h, "s1", &args).await;

        // THEN it is rejected with an explicit error and nothing changes
        assert!(!result.ok);
        assert_eq!(
            result.error.as_deref(),
            Some("reason is required for this operation")
        );
    }

    #[tokio::test]
    async fn test_remove_step_without_reason_rejected() {
        // GIVEN a proposed plan with two steps
        let h = handle();
        run_plan_propose(
            &h,
            "s1",
            &serde_json::json!({ "steps": [step_json("a"), step_json("b")] }),
        )
        .await;

        // WHEN removing a step with no reason
        let args = serde_json::json!({ "step_id": "b" });
        let result = run_plan_remove_step(&h, "s1", &args).await;

        // THEN it is rejected and the step survives
        assert!(!result.ok);
        let plan = h.get_plan("s1").await.expect("get").expect("plan");
        assert!(plan.steps.iter().any(|s| s.step_id == "b"));
    }

    #[tokio::test]
    async fn test_remove_step_with_reason_succeeds() {
        // GIVEN a proposed plan with two steps
        let h = handle();
        run_plan_propose(
            &h,
            "s1",
            &serde_json::json!({ "steps": [step_json("a"), step_json("b")] }),
        )
        .await;

        // WHEN removing a step with a reason
        let args = serde_json::json!({ "step_id": "b", "reason": "obsolete" });
        let result = run_plan_remove_step(&h, "s1", &args).await;

        // THEN it succeeds and the step is gone from the snapshot
        assert!(result.ok);
        let plan = result.plan.expect("plan");
        assert!(!plan.steps.iter().any(|s| s.step_id == "b"));
    }

    #[tokio::test]
    async fn test_propose_replaces_draft() {
        // GIVEN a first proposed plan
        let h = handle();
        run_plan_propose(&h, "s1", &serde_json::json!({ "steps": [step_json("a")] })).await;

        // WHEN proposing a fresh list
        let result =
            run_plan_propose(&h, "s1", &serde_json::json!({ "steps": [step_json("z")] })).await;

        // THEN only the new step remains
        let plan = result.plan.expect("plan");
        let ids: Vec<&str> = plan.steps.iter().map(|s| s.step_id.as_str()).collect();
        assert_eq!(ids, vec!["z"]);
    }

    #[tokio::test]
    async fn test_set_step_status_updates_snapshot() {
        // GIVEN a proposed plan
        let h = handle();
        run_plan_propose(&h, "s1", &serde_json::json!({ "steps": [step_json("a")] })).await;

        // WHEN setting the step status to in_progress
        let args = serde_json::json!({ "step_id": "a", "status": "in_progress" });
        let result = run_plan_set_step_status(&h, "s1", &args).await;

        // THEN the snapshot reflects the new status
        assert!(result.ok);
        assert_eq!(
            result.plan.expect("plan").steps[0].status,
            StepStatus::InProgress
        );
    }

    #[tokio::test]
    async fn test_submit_returns_awaiting_approval() {
        // GIVEN a proposed plan
        let h = handle();
        run_plan_propose(&h, "s1", &serde_json::json!({ "steps": [step_json("a")] })).await;

        // WHEN submitting it
        let result = run_plan_submit(&h, "s1").await;

        // THEN the snapshot status is awaiting approval
        assert!(result.ok);
        assert_eq!(
            result.plan.expect("plan").status,
            apollia_core::plan::PlanStatus::AwaitingApproval
        );
    }

    #[tokio::test]
    async fn test_malformed_propose_payload_rejected() {
        // GIVEN a payload missing the steps field
        let h = handle();
        let args = serde_json::json!({ "tasks": [] });

        // WHEN running plan_propose
        let result = run_plan_propose(&h, "s1", &args).await;

        // THEN a structured error is returned, never a panic
        assert!(!result.ok);
        assert!(result
            .error
            .as_deref()
            .is_some_and(|e| e.starts_with("invalid payload")));
    }
}
