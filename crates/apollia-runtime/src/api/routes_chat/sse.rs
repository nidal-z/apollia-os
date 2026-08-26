//! Translation of runtime events into the chat SSE wire payload.
//!
//! One session stream carries the whole conversation: the filter below keeps
//! the events that belong to the session, and flags the terminal one so the
//! stream can close after emitting it.

use serde::Serialize;

use apollia_core::RuntimeEvent;

/// SSE event payload for chat events.
#[derive(Debug, Serialize)]
pub(super) struct SseChatEvent {
    /// Event type discriminator.
    pub(super) event: String,
    /// Additional event data.
    #[serde(flatten)]
    pub(super) data: serde_json::Value,
}

/// Convert a [`RuntimeEvent`] to an SSE payload and its terminal flag if it
/// matches the session.
///
/// Returns `None` for events not relevant to this session. The boolean is
/// `true` for the terminal `session_closed` event so the caller can close the
/// stream after emitting it.
pub(super) fn chat_event_to_sse(
    event: &RuntimeEvent,
    session_id: &str,
) -> Option<(SseChatEvent, bool)> {
    let (sse_event, is_terminal) = match event {
        RuntimeEvent::ChatMessageSent {
            session_id: sid,
            message_id,
        } if sid == session_id => (
            SseChatEvent {
                event: "message_sent".into(),
                data: serde_json::json!({ "message_id": message_id }),
            },
            false,
        ),
        RuntimeEvent::ChatResponseStarted {
            session_id: sid,
            message_id,
            run_id: _,
        } if sid == session_id => (
            SseChatEvent {
                event: "response_started".into(),
                data: serde_json::json!({ "message_id": message_id }),
            },
            false,
        ),
        RuntimeEvent::ChatToken {
            session_id: sid,
            message_id,
            token,
        } if sid == session_id => (
            SseChatEvent {
                event: "token".into(),
                data: serde_json::json!({ "message_id": message_id, "token": token }),
            },
            false,
        ),
        RuntimeEvent::ChatResponseCompleted {
            session_id: sid,
            message_id,
            content,
            run_id: _,
        } if sid == session_id => (
            SseChatEvent {
                event: "response_completed".into(),
                data: serde_json::json!({ "message_id": message_id, "content": content }),
            },
            false,
        ),
        RuntimeEvent::ChatError {
            session_id: sid,
            message_id,
            error,
        } if sid == session_id => (
            SseChatEvent {
                event: "error".into(),
                data: serde_json::json!({ "message_id": message_id, "error": error }),
            },
            false,
        ),
        RuntimeEvent::ChatToolCallStarted {
            session_id: sid,
            message_id,
            tool_name,
            input_preview,
            rationale,
        } if sid == session_id => (
            SseChatEvent {
                event: "tool_call_started".into(),
                data: serde_json::json!({
                    "message_id": message_id,
                    "tool_name": tool_name,
                    "input_preview": input_preview,
                    "rationale": rationale,
                }),
            },
            false,
        ),
        RuntimeEvent::ChatToolCallCompleted {
            session_id: sid,
            message_id,
            tool_name,
            success,
            output_preview,
            analysis,
        } if sid == session_id => (
            SseChatEvent {
                event: "tool_call_completed".into(),
                data: serde_json::json!({
                    "message_id": message_id,
                    "tool_name": tool_name,
                    "success": success,
                    "output_preview": output_preview,
                    "analysis": analysis,
                }),
            },
            false,
        ),
        RuntimeEvent::ChatApprovalRequired {
            session_id: sid,
            message_id,
            tool_call_id,
            tool_name,
            prompt,
        } if sid == session_id => (
            SseChatEvent {
                event: "approval_required".into(),
                data: serde_json::json!({
                    "message_id": message_id,
                    "tool_call_id": tool_call_id,
                    "tool_name": tool_name,
                    "prompt": prompt,
                }),
            },
            false,
        ),
        RuntimeEvent::ChatApprovalResolved {
            session_id: sid,
            message_id,
            tool_call_id,
            tool_name,
            decision,
        } if sid == session_id => (
            SseChatEvent {
                event: "approval_resolved".into(),
                data: serde_json::json!({
                    "message_id": message_id,
                    "tool_call_id": tool_call_id,
                    "tool_name": tool_name,
                    "decision": decision,
                }),
            },
            false,
        ),
        RuntimeEvent::ChatSessionClosed { session_id: sid } if sid == session_id => (
            SseChatEvent {
                event: "session_closed".into(),
                data: serde_json::json!({}),
            },
            true,
        ),
        _ => return None,
    };

    Some((sse_event, is_terminal))
}
