//! How the text of a plan is read into steps the agent can run: the plan
//! found after a reasoning written in the clear, the names models give the
//! model and the file tools, and the tools checked against those the agent
//! holds.

use super::{ExecutionPlan, PlanValidationError};

/// The names models give the file tools, read as the tools they mean.
const TOOL_ALIASES: [(&str, &str); 8] = [
    ("read_file", "file_read"),
    ("write_file", "file_write"),
    ("edit_file", "file_edit"),
    ("list_files", "file_list"),
    ("list_directory", "file_list"),
    ("glob", "file_glob"),
    ("grep", "file_grep"),
    ("search_files", "file_grep"),
];

/// `plan` with each step's tool checked against the tools the agent holds.
///
/// A tool the model invented (`read_file` for `file_read`) used to be planned,
/// run, refused as not granted, and replanned: a minute lost per item. A known
/// alias is read as its tool; any other unknown tool refuses the plan at once
/// with the list of the tools there are, and the planner is asked again.
pub(super) fn grounded(
    mut plan: ExecutionPlan,
    available: &[String],
) -> Result<ExecutionPlan, PlanValidationError> {
    if available.is_empty() {
        return Ok(plan);
    }
    let holds = |name: &str| {
        available
            .iter()
            .any(|tool| tool == name || tool.ends_with(&format!("/{name}")))
    };
    let mut unknown = Vec::new();
    for step in &mut plan.steps {
        let Some(tool) = step.tool_hint.clone() else {
            continue;
        };
        if tool == "llm" || holds(&tool) {
            continue;
        }
        match TOOL_ALIASES
            .iter()
            .find(|(alias, real)| tool.eq_ignore_ascii_case(alias) && holds(real))
        {
            Some((_, real)) => step.tool_hint = Some((*real).to_string()),
            None => unknown.push(tool),
        }
    }
    if unknown.is_empty() {
        return Ok(plan);
    }
    let mut names: Vec<&str> = available
        .iter()
        .map(|tool| tool.rsplit('/').next().unwrap_or(tool))
        .collect();
    names.sort_unstable();
    names.dedup();
    Err(PlanValidationError::InvalidStructure(format!(
        "unknown tool(s) {}: use only these tools, or \"llm\" for a step that only reasons: {}",
        unknown.join(", "),
        names.join(", ")
    )))
}

/// The last JSON object of `text` that holds a `steps` field, read with its
/// strings and nested braces: the plan a model wrote after reasoning in the
/// clear. `None` when the text holds none.
pub(super) fn last_plan_object(text: &str) -> Option<serde_json::Value> {
    let bytes = text.as_bytes();
    let mut found = None;
    let mut start = 0;
    while let Some(offset) = text[start..].find('{') {
        let open = start + offset;
        let mut depth = 0usize;
        let mut in_string = false;
        let mut escaped = false;
        let mut close = None;
        for (index, &byte) in bytes.iter().enumerate().skip(open) {
            if in_string {
                match byte {
                    b'\\' if !escaped => escaped = true,
                    b'"' if !escaped => in_string = false,
                    _ => escaped = false,
                }
                continue;
            }
            match byte {
                b'"' => in_string = true,
                b'{' => depth += 1,
                b'}' => {
                    depth -= 1;
                    if depth == 0 {
                        close = Some(index);
                        break;
                    }
                }
                _ => {}
            }
        }
        let Some(close) = close else { break };
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(&text[open..=close]) {
            if value.get("steps").is_some_and(serde_json::Value::is_array) {
                found = Some(value);
            }
            start = close + 1;
        } else {
            start = open + 1;
        }
    }
    found
}

/// The tool hint of a planned step, with every way a model names the model
/// itself read as the one the executor knows, `llm`.
///
/// The planner prompt shows `tool_or_llm_name`, and a model answers `LLM`,
/// `Llm`, an empty string or `none` for a step of pure reasoning; taken as a
/// tool name, each was refused as "tool 'LLM' is not granted to this agent".
pub(super) fn model_step_hint(hint: Option<String>) -> Option<String> {
    match hint {
        Some(name) => {
            let bare = name.trim();
            if bare.is_empty()
                || ["llm", "none", "null", "model"]
                    .iter()
                    .any(|word| bare.eq_ignore_ascii_case(word))
            {
                Some("llm".to_string())
            } else {
                Some(name)
            }
        }
        None => None,
    }
}
