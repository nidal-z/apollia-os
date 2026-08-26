//! One tool invocation, from the A2A route to the audit record.
//!
//! Split out of `context.rs`: the proxy stays in its own module, the paths it
//! calls to reach a tool or a remote agent, and the events they emit, live
//! here.

use std::sync::Arc;
use std::time::Instant;

use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;

use apollia_tools::{
    compute_input_hash, AuditTrailHandle, ToolInvocationRecord, ToolRegistryHandle,
};

use apollia_core::AIPPart;

use crate::context::tool_proxy::{ToolExecutor, ToolProxyError};

/// Extracts the original A2A `skill_id` from a tool name, supporting both
/// the legacy `"a2a:{skill_id}"` prefix and the new
/// `"a2a__{skill_id_with_dots_as_double_underscores}"` prefix introduced for
/// OpenAI compatibility (which rejects `:` in tool names; see
/// `A2AInterface::skill_as_tool`). Returns `None` if the name doesn't match
/// either A2A pattern.
///
/// Examples:
/// - `extract_a2a_skill_id("a2a:read-excel")` -> `Some("read-excel")`
/// - `extract_a2a_skill_id("a2a__pdf__read_text")` -> `Some("pdf.read_text")`
/// - `extract_a2a_skill_id("bash")` -> `None`
pub(crate) fn extract_a2a_skill_id(tool_name: &str) -> Option<String> {
    if let Some(rest) = tool_name.strip_prefix("a2a:") {
        return Some(rest.to_string());
    }
    if let Some(rest) = tool_name.strip_prefix("a2a__") {
        // Reverse the encoding applied by `A2AInterface::skill_as_tool`:
        // `.` was replaced by `__`. We restore it here so the invoker
        // receives the canonical skill_id.
        return Some(rest.replace("__", "."));
    }
    None
}
/// Grouped parameters for [`invoke_a2a_tool`] to avoid too many function arguments.
pub(crate) struct A2AInvokeContext<'a> {
    pub(crate) invoker: &'a apollia_runtime::a2a::A2AInvoker,
    pub(crate) skill_id: &'a str,
    pub(crate) caller: &'a str,
    pub(crate) a2a_depth: u32,
    pub(crate) chain_deadline: Option<Instant>,
}
/// Routes an `"a2a:{skill_id}"` tool call to the [`A2AInvoker`].
///
/// Calls `invoker.invoke()` with `a2a_depth + 1` and formats the result as
/// `"[{skill_id} via {agent_name}]\n{output_text}"` where `output_text` is
/// the concatenation of all [`AIPPart::Text`] parts from the worker's output.
///
/// Returns `ToolProxyError::ExecutionFailed` if the invocation fails.
pub(crate) async fn invoke_a2a_tool(
    ctx: &A2AInvokeContext<'_>,
    input: serde_json::Value,
) -> Result<serde_json::Value, ToolProxyError> {
    let skill_id = ctx.skill_id;
    let result = ctx
        .invoker
        .invoke(apollia_runtime::a2a::A2AInvokeRequest {
            skill_id,
            input,
            caller: ctx.caller,
            a2a_depth: ctx.a2a_depth.saturating_add(1),
            timeout: None,
            chain_deadline: ctx.chain_deadline,
        })
        .await
        .map_err(|e| ToolProxyError::ExecutionFailed(e.to_string()))?;

    let output_text: String = result
        .result
        .output
        .iter()
        .filter_map(|part| match part {
            AIPPart::Text(t) => Some(t.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n");

    let formatted = format!("[{skill_id} via {}]\n{output_text}", result.agent_name);
    Ok(serde_json::json!({ "text": formatted }))
}
/// Converts a `serde_json::Value` into a Python object via `json.loads`.
pub(crate) fn json_value_to_py(value: &serde_json::Value) -> PyResult<PyObject> {
    let json_str = serde_json::to_string(value)
        .map_err(|e| PyRuntimeError::new_err(format!("result serialization: {e}")))?;
    Python::with_gil(|py| {
        let json_mod = py
            .import("json")
            .map_err(|e| PyRuntimeError::new_err(format!("import json: {e}")))?;
        let py_obj: PyObject = json_mod
            .call_method1("loads", (json_str,))
            .map_err(|e| PyRuntimeError::new_err(format!("json.loads: {e}")))?
            .unbind();
        Ok(py_obj)
    })
}
/// Owned identifiers needed to emit A2A completion events for one tool call.
pub(crate) struct A2ACompletionEvent {
    pub(crate) parent_id: String,
    pub(crate) task_id: String,
    pub(crate) agent_id: String,
    pub(crate) tool_name: String,
    pub(crate) skill_id: String,
    pub(crate) duration_ms: u64,
    pub(crate) run_id: Option<apollia_core::events::RunId>,
}
/// Emits `ToolCallCompleted` + `A2AInvokeCompleted` for an A2A tool call result.
///
/// No-op when no event bus is wired (`bus` is `None`).
pub(crate) fn emit_a2a_completion_events(
    bus: Option<&apollia_core::events::EventBusSender>,
    result: &Result<serde_json::Value, ToolProxyError>,
    ev: A2ACompletionEvent,
) {
    let Some(bus) = bus else { return };
    let A2ACompletionEvent {
        parent_id,
        task_id,
        agent_id,
        tool_name,
        skill_id,
        duration_ms,
        run_id,
    } = ev;
    match result {
        Ok(value) => {
            let output_str = serde_json::to_string(value).ok();
            let summary = output_str.as_deref().map(|s| {
                let mut out = s.chars().take(200).collect::<String>();
                if s.chars().count() > 200 {
                    out.push('…');
                }
                out
            });
            let _ = bus.send(apollia_core::events::RuntimeEvent::ToolCallCompleted {
                parent_event_id: parent_id.clone(),
                task_id: task_id.clone().into(),
                agent_id: agent_id.into(),
                tool_name,
                output_json: output_str,
                exit_code: None,
                duration_ms,
                success: true,
                run_id,
            });
            let _ = bus.send(apollia_core::events::RuntimeEvent::A2AInvokeCompleted {
                parent_event_id: parent_id,
                task_id: task_id.into(),
                skill_id,
                success: true,
                output_summary: summary,
                duration_ms,
            });
        }
        Err(e) => {
            let _ = bus.send(apollia_core::events::RuntimeEvent::ToolCallCompleted {
                parent_event_id: parent_id.clone(),
                task_id: task_id.clone().into(),
                agent_id: agent_id.into(),
                tool_name,
                output_json: Some(format!("{{\"error\":{:?}}}", e.to_string())),
                exit_code: None,
                duration_ms,
                success: false,
                run_id,
            });
            let _ = bus.send(apollia_core::events::RuntimeEvent::A2AInvokeCompleted {
                parent_event_id: parent_id,
                task_id: task_id.into(),
                skill_id,
                success: false,
                output_summary: Some(e.to_string()),
                duration_ms,
            });
        }
    }
}
/// Owned identifiers needed to emit completion/denial events for one tool call.
pub(crate) struct ToolCompletionEvent {
    pub(crate) parent_id: String,
    pub(crate) task_id: String,
    pub(crate) agent_id: String,
    pub(crate) tool_name: String,
    pub(crate) duration_ms: u64,
    pub(crate) run_id: Option<apollia_core::events::RunId>,
}
/// Emits `ToolCallCompleted` or `ToolCallDenied` for a registry tool call result.
///
/// No-op when no event bus is wired (`bus` is `None`).
pub(crate) fn emit_tool_completion_events(
    bus: Option<&apollia_core::events::EventBusSender>,
    result: &Result<serde_json::Value, ToolProxyError>,
    ev: ToolCompletionEvent,
) {
    let Some(bus) = bus else { return };
    let ToolCompletionEvent {
        parent_id,
        task_id,
        agent_id,
        tool_name,
        duration_ms,
        run_id,
    } = ev;
    match result {
        Ok(value) => {
            let _ = bus.send(apollia_core::events::RuntimeEvent::ToolCallCompleted {
                parent_event_id: parent_id,
                task_id: task_id.into(),
                agent_id: agent_id.into(),
                tool_name,
                output_json: serde_json::to_string(value).ok(),
                exit_code: None,
                duration_ms,
                success: true,
                run_id,
            });
        }
        Err(ToolProxyError::ToolNotAllowed(name)) => {
            let _ = bus.send(apollia_core::events::RuntimeEvent::ToolCallDenied {
                parent_event_id: parent_id,
                task_id: task_id.into(),
                agent_id: agent_id.into(),
                tool_name: name.clone(),
                reason: "not_in_manifest".to_string(),
                detail: None,
            });
        }
        Err(e) => {
            let _ = bus.send(apollia_core::events::RuntimeEvent::ToolCallCompleted {
                parent_event_id: parent_id,
                task_id: task_id.into(),
                agent_id: agent_id.into(),
                tool_name,
                output_json: Some(format!("{{\"error\":{:?}}}", e.to_string())),
                exit_code: None,
                duration_ms,
                success: false,
                run_id,
            });
        }
    }
}
/// Grouped parameters for [`execute_tool`] to avoid too many function arguments.
pub(crate) struct ToolCallContext<'a> {
    pub(crate) registry: &'a ToolRegistryHandle,
    pub(crate) audit: &'a AuditTrailHandle,
    pub(crate) executor: &'a Arc<dyn ToolExecutor>,
    pub(crate) allowed_tools: &'a [String],
    pub(crate) agent_id: &'a str,
    pub(crate) task_id: &'a str,
    /// Run this call belongs to, recorded on the audit trail so `audit list`
    /// can surface it and a task_id can be resolved to its run_id.
    pub(crate) run_id: Option<&'a str>,
}
/// Shared tool execution logic used by both the Python `call()` and Rust `call_inner()`.
pub(crate) async fn execute_tool(
    ctx: &ToolCallContext<'_>,
    tool_name: &str,
    input: serde_json::Value,
) -> Result<serde_json::Value, ToolProxyError> {
    let start = Instant::now();
    let input_hash = compute_input_hash(&input);
    let args_json = serde_json::to_string(&input).ok();

    // 1. Check permission BEFORE registry lookup (don't reveal tool existence)
    if !ctx.allowed_tools.iter().any(|t| t == tool_name) {
        let duration = start.elapsed();
        emit_audit_record(
            ctx,
            tool_name,
            &input_hash,
            "unknown",
            AuditOutcome {
                duration_ms: duration.as_millis() as u64,
                success: false,
                error_code: Some(ToolProxyError::ToolNotAllowed(tool_name.to_string()).to_string()),
                args_json,
                stdout: None,
                stderr: None,
            },
        );
        return Err(ToolProxyError::ToolNotAllowed(tool_name.to_string()));
    }

    // 2. Lookup in registry. Deferred MCP tools carry no descriptor (only a
    //    lightweight index), so an allowlisted `mcp:` tool with no descriptor
    //    still proceeds: the dispatcher fetches the schema on demand and is the
    //    real existence gate (raising UnknownTool if genuinely absent). Native
    //    tools always have a descriptor, so they keep the strict ToolNotFound.
    let descriptor = ctx
        .registry
        .get(tool_name)
        .await
        .map_err(|e| ToolProxyError::ExecutionFailed(e.to_string()))?;
    let sandbox_profile = match &descriptor {
        Some(d) => format!("{:?}", d.sandbox_profile),
        None if tool_name.starts_with("mcp:") => "McpDeferred".to_string(),
        None => return Err(ToolProxyError::ToolNotFound(tool_name.to_string())),
    };

    // 3. Execute, timed so the audit record below carries the duration
    let exec_result = ctx.executor.execute(tool_name, input);
    let duration = start.elapsed();

    let (success, error_code, stdout, stderr) = match &exec_result {
        Ok(val) => (true, None, serde_json::to_string(val).ok(), None),
        Err(e) => (false, Some(e.clone()), None, Some(e.clone())),
    };

    // 4. Record audit (always, success or failure)
    emit_audit_record(
        ctx,
        tool_name,
        &input_hash,
        &sandbox_profile,
        AuditOutcome {
            duration_ms: duration.as_millis() as u64,
            success,
            error_code,
            args_json,
            stdout,
            stderr,
        },
    );

    // 5. Return result
    exec_result.map_err(ToolProxyError::ExecutionFailed)
}
/// Outcome fields for an audit record, grouped to keep `emit_audit_record` under 7 params.
pub(crate) struct AuditOutcome {
    pub(crate) duration_ms: u64,
    pub(crate) success: bool,
    pub(crate) error_code: Option<String>,
    pub(crate) args_json: Option<String>,
    pub(crate) stdout: Option<String>,
    pub(crate) stderr: Option<String>,
}
/// Records a tool invocation in the audit trail (fire-and-forget).
pub(crate) fn emit_audit_record(
    ctx: &ToolCallContext<'_>,
    tool_name: &str,
    input_hash: &str,
    sandbox_profile: &str,
    outcome: AuditOutcome,
) {
    ctx.audit.record(ToolInvocationRecord {
        id: uuid::Uuid::new_v4().to_string(),
        agent_id: ctx.agent_id.to_string(),
        task_id: ctx.task_id.to_string(),
        run_id: ctx.run_id.map(String::from),
        tool_name: tool_name.to_string(),
        input_hash: input_hash.to_string(),
        sandbox_profile: sandbox_profile.to_string(),
        started_at: now_rfc3339(),
        duration_ms: Some(outcome.duration_ms),
        exit_code: None,
        success: outcome.success,
        error_code: outcome.error_code,
        resources_used: None,
        args_json: outcome.args_json,
        stdout: outcome.stdout,
        stderr: outcome.stderr,
    });
}
/// Returns the current UTC time as an RFC3339 string.
pub(crate) fn now_rfc3339() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let (year, month, day) = epoch_secs_to_ymd(secs);
    let day_secs = (secs % 86400) as u32;
    let h = day_secs / 3600;
    let m = (day_secs % 3600) / 60;
    let s = day_secs % 60;

    format!("{year:04}-{month:02}-{day:02}T{h:02}:{m:02}:{s:02}Z")
}
/// Converts epoch seconds to (year, month, day).
pub(crate) fn epoch_secs_to_ymd(secs: u64) -> (i32, u32, u32) {
    let mut days = (secs / 86400) as i64;
    let mut year = 1970i32;

    loop {
        let days_in_year: i64 = if is_leap(year) { 366 } else { 365 };
        if days < days_in_year {
            break;
        }
        days -= days_in_year;
        year += 1;
    }

    let month_lengths: [i64; 12] = if is_leap(year) {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };

    let mut month = 1u32;
    for &ml in &month_lengths {
        if days < ml {
            break;
        }
        days -= ml;
        month += 1;
    }

    (year, month, days as u32 + 1)
}
/// Returns `true` if the given year is a leap year.
pub(crate) fn is_leap(y: i32) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}
