//! The `todo_write` built-in tool.
//!
//! Lets the agent replace its full session todo list in one call. The list is
//! validated and persisted by the [`TodoHandle`] actor, which owns the
//! at-most-one-`InProgress` invariant. This module is stateless: it parses the
//! payload, delegates to the handle, and shapes a JSON-serializable result for
//! the LLM. It never panics; failures come back as `ok: false` payloads.

use apollia_core::todo::{TodoError, TodoItem};
use apollia_llm::types::ToolSpec;

use super::todo_handle::TodoHandle;

/// Name under which the tool is advertised to the LLM and matched in the loop.
pub const TODO_WRITE_TOOL_NAME: &str = "todo_write";

/// Result returned to the LLM after a `todo_write` invocation.
#[derive(Debug, serde::Serialize)]
pub struct TodoWriteResult {
    /// Whether the write succeeded.
    pub ok: bool,
    /// Error message when `ok` is false; omitted otherwise.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Number of items persisted when `ok` is true; omitted otherwise.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub count: Option<usize>,
}

impl TodoWriteResult {
    fn success(count: usize) -> Self {
        Self {
            ok: true,
            error: None,
            count: Some(count),
        }
    }

    fn failure(message: impl Into<String>) -> Self {
        Self {
            ok: false,
            error: Some(message.into()),
            count: None,
        }
    }
}

/// Human-readable message for a [`TodoError`] surfaced to the LLM.
fn describe_error(error: &TodoError) -> String {
    match error {
        TodoError::MultipleInProgress { .. } => "at most one item may be in_progress".to_string(),
        other => other.to_string(),
    }
}

/// Execute the `todo_write` built-in tool.
///
/// Parses `raw_args` (the tool call arguments object, expected to carry an
/// `items` array), validates and persists it via the [`TodoHandle`] for
/// `session_id`, and returns a structured result. A malformed payload or a
/// rejected write both yield `ok: false` with an explanatory `error`; the
/// caller injects the serialized result as the tool message and continues the
/// loop.
pub async fn run_todo_write(
    handle: &TodoHandle,
    session_id: &str,
    raw_args: &serde_json::Value,
) -> TodoWriteResult {
    let items_value = match raw_args.get("items") {
        Some(value) => value.clone(),
        None => return TodoWriteResult::failure("invalid payload: missing 'items' field"),
    };

    let items: Vec<TodoItem> = match serde_json::from_value(items_value) {
        Ok(items) => items,
        Err(e) => return TodoWriteResult::failure(format!("invalid payload: {e}")),
    };

    let count = items.len();
    match handle.set_items(session_id, items).await {
        Ok(()) => TodoWriteResult::success(count),
        Err(e) => TodoWriteResult::failure(describe_error(&e)),
    }
}

/// JSON tool descriptor for `todo_write`.
///
/// Declared as a module-level constant so the schema has a single source of
/// truth shared by the LLM spec and the tests.
pub const TODO_WRITE_SCHEMA: &str = r#"{
  "name": "todo_write",
  "description": "Replace the full todo list for the current session. Use this tool to track tasks and their status. Constraint: at most one item may have status 'in_progress' at any time.",
  "parameters": {
    "type": "object",
    "required": ["items"],
    "properties": {
      "items": {
        "type": "array",
        "description": "Complete replacement list of todo items.",
        "items": {
          "type": "object",
          "required": ["id", "content", "status"],
          "properties": {
            "id":         { "type": "string" },
            "content":    { "type": "string" },
            "status":     { "type": "string", "enum": ["pending", "in_progress", "completed"] },
            "depends_on": { "type": "array", "items": { "type": "string" } }
          }
        }
      }
    }
  }
}"#;

/// Build the [`ToolSpec`] advertised to the LLM for `todo_write`.
///
/// Available whenever the session has a todo store. The description states the
/// at-most-one-`in_progress` constraint so the model self-corrects.
#[must_use]
pub fn todo_write_spec() -> ToolSpec {
    ToolSpec {
        name: TODO_WRITE_TOOL_NAME.to_string(),
        description: "Replace the full todo list for the current session. Use this to track \
                      tasks and their status. Provide the complete list every time. Constraint: \
                      at most one item may have status 'in_progress' at any time."
            .to_string(),
        parameters: serde_json::json!({
            "type": "object",
            "required": ["items"],
            "properties": {
                "items": {
                    "type": "array",
                    "description": "Complete replacement list of todo items.",
                    "items": {
                        "type": "object",
                        "required": ["id", "content", "status"],
                        "properties": {
                            "id":         { "type": "string" },
                            "content":    { "type": "string" },
                            "status":     { "type": "string", "enum": ["pending", "in_progress", "completed"] },
                            "depends_on": { "type": "array", "items": { "type": "string" } }
                        }
                    }
                }
            }
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chat::todo_actor::spawn_todo_actor;
    use apollia_core::todo::TodoStatus;
    use rusqlite::Connection;

    fn handle() -> TodoHandle {
        spawn_todo_actor(Connection::open_in_memory().expect("open"), None).expect("spawn")
    }

    #[tokio::test]
    async fn test_todo_write_nominal_replaces_list() {
        // GIVEN a handle and a valid payload with one in_progress + one pending
        let h = handle();
        let args = serde_json::json!({
            "items": [
                { "id": "t1", "content": "Lire config", "status": "completed", "depends_on": [] },
                { "id": "t2", "content": "Analyser logs", "status": "in_progress", "depends_on": ["t1"] }
            ]
        });

        // WHEN running the tool
        let result = run_todo_write(&h, "s1", &args).await;

        // THEN the result reports success with the count and the items persist
        assert!(result.ok);
        assert_eq!(result.count, Some(2));
        let stored = h.get_items("s1").await.expect("get");
        assert_eq!(stored.len(), 2);
        assert_eq!(stored[1].status, TodoStatus::InProgress);
    }

    #[tokio::test]
    async fn test_todo_write_multiple_in_progress_returns_error() {
        // GIVEN a payload with two in_progress items
        let h = handle();
        let args = serde_json::json!({
            "items": [
                { "id": "t1", "content": "a", "status": "in_progress" },
                { "id": "t2", "content": "b", "status": "in_progress" }
            ]
        });

        // WHEN running the tool
        let result = run_todo_write(&h, "s1", &args).await;

        // THEN a structured error is returned and nothing is persisted
        assert!(!result.ok);
        assert_eq!(
            result.error.as_deref(),
            Some("at most one item may be in_progress")
        );
        assert!(h.get_items("s1").await.expect("get").is_empty());
    }

    #[tokio::test]
    async fn test_todo_write_malformed_payload_returns_error() {
        // GIVEN a payload without the items field
        let h = handle();
        let args = serde_json::json!({ "tasks": [] });

        // WHEN running the tool
        let result = run_todo_write(&h, "s1", &args).await;

        // THEN a deserialization-style error is returned
        assert!(!result.ok);
        assert!(result
            .error
            .as_deref()
            .is_some_and(|e| e.starts_with("invalid payload")));
    }

    #[test]
    fn test_todo_write_schema_is_valid_json() {
        // GIVEN the declared schema constant
        // WHEN parsing it as JSON
        let value: serde_json::Value =
            serde_json::from_str(TODO_WRITE_SCHEMA).expect("schema parses");

        // THEN the tool name is todo_write
        assert_eq!(value["name"], "todo_write");
    }

    #[test]
    fn test_todo_write_spec_matches_tool_name() {
        // GIVEN the LLM spec builder
        // WHEN building the spec
        let spec = todo_write_spec();

        // THEN the name matches and the items parameter is declared required
        assert_eq!(spec.name, TODO_WRITE_TOOL_NAME);
        assert_eq!(spec.parameters["required"][0], "items");
    }
}
