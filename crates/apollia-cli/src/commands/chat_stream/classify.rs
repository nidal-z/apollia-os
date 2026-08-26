//! SSE line classification, and reading of the approval decision.

// ─── Event classification ───────────────────────────────────────────────────

/// Action derived from one SSE line of the chat session stream.
#[derive(Debug, PartialEq)]
pub(super) enum ChatStreamAction {
    /// A fragment of assistant text to display live.
    Token(String),
    /// A tool call starts (name, input preview, intent).
    ToolStarted {
        tool_name: String,
        input_preview: String,
        rationale: Option<String>,
    },
    /// A tool call finished (success, output preview, error analysis).
    ToolCompleted {
        tool_name: String,
        success: bool,
        output_preview: String,
        analysis: Option<String>,
    },
    /// The agent asks for permission to use a tool.
    ApprovalRequired {
        message_id: String,
        tool_name: String,
        prompt: String,
    },
    /// Reply finished for this turn (full content, fallback when zero token).
    Completed { content: String },
    /// Generation error for this turn.
    Error(String),
    /// Session fermee cote serveur, terminal global.
    SessionClosed,
    /// Nothing to display (prompt echo, other message, irrelevant line).
    Ignore,
}

/// Classifies a raw SSE line into a [`ChatStreamAction`], filtered on the
/// current turn `my_id`. A pure, testable function, with no I/O and no socket.
pub(super) fn classify_chat_event(line: &str, my_id: &str) -> ChatStreamAction {
    let Some(data) = line.strip_prefix("data: ") else {
        return ChatStreamAction::Ignore;
    };
    let Ok(parsed) = serde_json::from_str::<serde_json::Value>(data) else {
        return ChatStreamAction::Ignore;
    };
    let event = parsed.get("event").and_then(|v| v.as_str()).unwrap_or("");

    // `session_closed` carries no message_id: end of session, terminal.
    if event == "session_closed" {
        return ChatStreamAction::SessionClosed;
    }

    // Every other event is filtered on the message_id of the current turn.
    // The 202 returns the same id as the one the tokens and the reply carry.
    let mid = parsed
        .get("message_id")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if mid != my_id {
        return ChatStreamAction::Ignore;
    }

    match event {
        "token" => ChatStreamAction::Token(str_field(&parsed, "token")),
        "tool_call_started" => ChatStreamAction::ToolStarted {
            tool_name: str_field(&parsed, "tool_name"),
            input_preview: str_field(&parsed, "input_preview"),
            rationale: extract_rationale(&parsed),
        },
        "tool_call_completed" => ChatStreamAction::ToolCompleted {
            tool_name: str_field(&parsed, "tool_name"),
            success: parsed
                .get("success")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
            output_preview: str_field(&parsed, "output_preview"),
            analysis: extract_analysis(&parsed),
        },
        "approval_required" => ChatStreamAction::ApprovalRequired {
            message_id: mid.to_string(),
            tool_name: str_field(&parsed, "tool_name"),
            prompt: str_field(&parsed, "prompt"),
        },
        "response_completed" => ChatStreamAction::Completed {
            content: str_field(&parsed, "content"),
        },
        "error" => ChatStreamAction::Error(str_field(&parsed, "error")),
        // message_sent (prompt echo), response_started, approval_resolved:
        // nothing to display.
        _ => ChatStreamAction::Ignore,
    }
}

/// Reads a string field, empty string when absent.
pub(super) fn str_field(v: &serde_json::Value, key: &str) -> String {
    v.get(key)
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .to_string()
}

/// Extracts the intent summary of a serialised `ToolCallRationale`, if present.
pub(super) fn extract_rationale(v: &serde_json::Value) -> Option<String> {
    let r = v.get("rationale")?;
    if r.is_null() {
        return None;
    }
    let summary = r.get("summary")?.as_str()?;
    if summary.is_empty() {
        None
    } else {
        Some(summary.to_string())
    }
}

/// Extracts the human message of a serialised `ErrorAnalysis` (with its advice
/// when there is one), if present.
pub(super) fn extract_analysis(v: &serde_json::Value) -> Option<String> {
    let a = v.get("analysis")?;
    if a.is_null() {
        return None;
    }
    let human = a.get("human_message")?.as_str()?;
    if human.is_empty() {
        return None;
    }
    match a.get("suggested_action").and_then(|x| x.as_str()) {
        Some(action) if !action.is_empty() => Some(format!("{human}  (hint: {action})")),
        _ => Some(human.to_string()),
    }
}

// ─── Decision d'approbation ──────────────────────────────────────────────────

/// Decision typed at the tool approval prompt.
#[derive(Debug, PartialEq)]
pub(super) enum ToolDecisionInput {
    /// Allow once.
    Accept,
    /// Always allow (a scope is asked for next).
    Always,
    /// Refuse, with an optional reason.
    Refuse(Option<String>),
    /// Unrecognised input.
    Invalid,
}

/// Parses the decision typed at the prompt. Accepts French and English.
///
/// `a`/`autoriser`/`accept`/`oui` -> Accept ; `t`/`toujours`/`always` -> Always ;
/// `r`/`refuser`/`refuse`/`non` [reason] -> Refuse. Anything else is Invalid.
pub(super) fn parse_tool_decision(input: &str) -> ToolDecisionInput {
    let trimmed = input.trim();
    let (head, rest) = match trimmed.split_once(char::is_whitespace) {
        Some((h, r)) => (h, r.trim()),
        None => (trimmed, ""),
    };
    match head.to_ascii_lowercase().as_str() {
        "a" | "autoriser" | "accept" | "oui" | "y" => ToolDecisionInput::Accept,
        "t" | "toujours" | "always" => ToolDecisionInput::Always,
        "r" | "refuser" | "refuse" | "non" | "n" => {
            let reason = if rest.is_empty() {
                None
            } else {
                Some(rest.to_string())
            };
            ToolDecisionInput::Refuse(reason)
        }
        _ => ToolDecisionInput::Invalid,
    }
}

/// Translates a scope choice into the snake_case wire value the API expects.
///
/// Empty input or `1` -> `this_session` (the default, the least sticky) ; `2`
/// -> `this_tool` ; `3` -> `this_project`. Anything else is invalid.
pub(super) fn parse_scope_choice(input: &str) -> Option<&'static str> {
    match input.trim() {
        "" | "1" => Some("this_session"),
        "2" => Some("this_tool"),
        "3" => Some("this_project"),
        _ => None,
    }
}
