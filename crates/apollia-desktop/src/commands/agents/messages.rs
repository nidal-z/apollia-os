//! The mailbox view of the desktop: the messages one agent received from
//! another, capped so a runaway exchange cannot fill the window.

use apollia_runtime::embedded::RuntimeHandle;
use serde::Serialize;
use tauri::State;

use crate::commands::http_get_json;

// ─────────────────────────────────────────────────────────────────────────────
// Agent messages
// ─────────────────────────────────────────────────────────────────────────────

/// Message exchanged between two agents.
#[derive(Debug, Serialize)]
pub struct AgentMessageView {
    /// Name of the sending agent.
    pub from_agent: String,
    /// Name of the receiving agent.
    pub to_agent: String,
    /// JSON content of the message.
    pub payload: serde_json::Value,
    /// Send timestamp (RFC 3339).
    pub sent_at: String,
}

/// Maximum cap on returned messages.
const MAX_MESSAGE_LIMIT: u32 = 200;
/// Default limit when unspecified or invalid.
const DEFAULT_MESSAGE_LIMIT: u32 = 50;

/// Returns the messages received by an agent, sorted by `sent_at` descending.
///
/// `limit` is capped at 200; if `<= 0` or not provided, the default (50)
/// applies. Delegates to `GET /api/v1/agents/{name}/messages`.
#[tauri::command]
pub async fn list_agent_messages(
    runtime: State<'_, RuntimeHandle>,
    agent_name: String,
    limit: u32,
) -> Result<Vec<AgentMessageView>, String> {
    list_agent_messages_inner(runtime.api_port, &agent_name, limit).await
}

/// Inner logic for `list_agent_messages`, testable without Tauri State.
async fn list_agent_messages_inner(
    port: u16,
    agent_name: &str,
    limit: u32,
) -> Result<Vec<AgentMessageView>, String> {
    let effective = if limit == 0 {
        DEFAULT_MESSAGE_LIMIT
    } else if limit > MAX_MESSAGE_LIMIT {
        MAX_MESSAGE_LIMIT
    } else {
        limit
    };

    let path = format!("/api/v1/agents/{agent_name}/messages?limit={effective}");
    match http_get_json(port, &path).await {
        Ok(json) => {
            let messages = json
                .get("messages")
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default();

            let views: Vec<AgentMessageView> = messages
                .into_iter()
                .map(|m| AgentMessageView {
                    from_agent: m
                        .get("from_agent")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    to_agent: m
                        .get("to_agent")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    payload: m.get("payload").cloned().unwrap_or(serde_json::Value::Null),
                    sent_at: m
                        .get("sent_at")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                })
                .collect();

            Ok(views)
        }
        Err(e) if e.contains("404") => Ok(vec![]),
        Err(e) => Err(format!("list_agent_messages: {e}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_message_view_serializes() {
        // GIVEN an AgentMessageView
        let view = AgentMessageView {
            from_agent: "agent-a".to_string(),
            to_agent: "agent-b".to_string(),
            payload: serde_json::json!({"data": "hello"}),
            sent_at: "2026-03-24T10:00:00Z".to_string(),
        };

        // WHEN serialized
        let json = serde_json::to_value(&view).expect("serialize");

        // THEN all fields are present
        assert_eq!(json["from_agent"], "agent-a");
        assert_eq!(json["to_agent"], "agent-b");
        assert_eq!(json["payload"]["data"], "hello");
        assert_eq!(json["sent_at"], "2026-03-24T10:00:00Z");
    }

    #[test]
    fn test_message_limit_constants() {
        // GIVEN the limit constants
        // THEN they have expected values
        assert_eq!(MAX_MESSAGE_LIMIT, 200);
        assert_eq!(DEFAULT_MESSAGE_LIMIT, 50);
    }
}
