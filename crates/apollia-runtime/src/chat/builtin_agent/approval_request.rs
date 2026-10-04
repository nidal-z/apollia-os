//! What a consent request shows: the whole command for a tool that runs
//! code, a short preview with its full detail for any other tool.
//!
//! A command is never cut: what the user approves is what runs. Every other
//! tool keeps a short preview on the card, the full arguments travelling
//! beside it for the card's "see more".

use apollia_llm::types::ToolCall;
use apollia_permissions::is_code_executor;

use crate::governance::ConsentAnswer;

use super::helpers::truncate_preview;

/// The content of one consent request, as the approval event carries it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ApprovalRequest {
    /// What the request says: the short preview, or the whole code.
    pub(super) prompt: String,
    /// The answers offered, as their wire names.
    pub(super) answers: Vec<String>,
    /// The full arguments of the call, as indented JSON.
    pub(super) detail: String,
    /// The whole command or source, for a tool that runs code.
    pub(super) code: Option<String>,
    /// The folder the code runs in, for a tool that runs code.
    pub(super) working_dir: Option<String>,
}

/// The key of the argument holding the code of each code executor.
fn code_key(tool_name: &str) -> &'static str {
    match tool_name {
        "python_executor" => "code",
        _ => "command",
    }
}

/// The wire name of an answer.
fn answer_name(answer: ConsentAnswer) -> &'static str {
    match answer {
        ConsentAnswer::Once => "once",
        ConsentAnswer::Always => "always",
        ConsentAnswer::Refuse => "refuse",
    }
}

/// The folder a code executor runs `arguments` in: the one the call names,
/// or the folder the runtime runs in, which the executor inherits.
fn working_dir_of(arguments: &serde_json::Value) -> Option<String> {
    let named = arguments
        .get("working_dir")
        .and_then(serde_json::Value::as_str)
        .filter(|dir| !dir.trim().is_empty());
    match named {
        Some(dir) => Some(dir.to_owned()),
        None => std::env::current_dir()
            .ok()
            .map(|dir| dir.display().to_string()),
    }
}

/// The request for `call`, offering `answers`.
pub(super) fn approval_request(call: &ToolCall, answers: &[ConsentAnswer]) -> ApprovalRequest {
    let answers = answers.iter().map(|a| answer_name(*a).to_owned()).collect();
    let detail = serde_json::to_string_pretty(&call.arguments).unwrap_or_default();
    if is_code_executor(&call.name) {
        let code = call
            .arguments
            .get(code_key(&call.name))
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned)
            .or_else(|| apollia_permissions::extract_first_arg(&call.arguments))
            .unwrap_or_default();
        return ApprovalRequest {
            prompt: code.clone(),
            answers,
            detail,
            code: Some(code),
            working_dir: working_dir_of(&call.arguments),
        };
    }
    let preview = truncate_preview(&serde_json::to_string(&call.arguments).unwrap_or_default());
    ApprovalRequest {
        prompt: format!("Tool '{}' asks to run with: {}", call.name, preview),
        answers,
        detail,
        code: None,
        working_dir: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::governance::{ALL_ANSWERS, NO_ALWAYS_ANSWERS};

    fn call(name: &str, arguments: serde_json::Value) -> ToolCall {
        ToolCall {
            id: "c0".into(),
            name: name.into(),
            arguments,
        }
    }

    #[test]
    fn test_a_long_shell_command_is_shown_whole_with_its_folder() {
        // GIVEN a shell command of 5,000 characters run in a named folder
        let command = format!("echo {}", "x".repeat(4_995));
        assert_eq!(command.chars().count(), 5_000);
        let shell = call(
            "bash_executor",
            serde_json::json!({ "command": command, "timeout_secs": 10, "working_dir": "/srv/data" }),
        );

        // WHEN its consent request is built
        let request = approval_request(&shell, &NO_ALWAYS_ANSWERS);

        // THEN the command is carried whole, in the code and in the prompt
        assert_eq!(request.code.as_deref(), Some(command.as_str()));
        assert_eq!(request.prompt, command);
        // AND the folder is the one the call names, the answers the two offered
        assert_eq!(request.working_dir.as_deref(), Some("/srv/data"));
        assert_eq!(request.answers, vec!["once", "refuse"]);
    }

    #[test]
    fn test_python_source_is_shown_whole_in_the_runtime_folder() {
        // GIVEN Python source of 5,000 characters, with no folder named
        let source = format!("print('{}')", "y".repeat(4_991));
        assert_eq!(source.chars().count(), 5_000);
        let python = call(
            "python_executor",
            serde_json::json!({ "code": source, "timeout_secs": 10 }),
        );

        // WHEN its consent request is built
        let request = approval_request(&python, &NO_ALWAYS_ANSWERS);

        // THEN the source is carried whole, and the folder is where the
        // runtime runs, which the executor inherits
        assert_eq!(request.code.as_deref(), Some(source.as_str()));
        let here = std::env::current_dir().unwrap().display().to_string();
        assert_eq!(request.working_dir.as_deref(), Some(here.as_str()));
    }

    #[test]
    fn test_another_tool_keeps_a_short_preview_and_its_full_detail() {
        // GIVEN a file write whose content runs past the preview
        let content = "z".repeat(1_000);
        let write = call(
            "file_write",
            serde_json::json!({ "path": "notes.txt", "content": content }),
        );

        // WHEN its consent request is built
        let request = approval_request(&write, &ALL_ANSWERS);

        // THEN the prompt is short, nothing is code, and the detail is whole
        assert!(request.prompt.chars().count() < 300, "{}", request.prompt);
        assert!(request.code.is_none());
        assert!(request.working_dir.is_none());
        assert!(request.detail.contains(&content));
        assert_eq!(request.answers, vec!["once", "always", "refuse"]);
    }
}
