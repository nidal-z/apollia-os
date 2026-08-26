//! The Anthropic streaming decoder.
//!
//! Split out of `anthropic.rs`: the client stays in the parent, the SSE state
//! machine that turns a byte stream into completion chunks lives here.

use std::pin::Pin;

use futures::{Stream, StreamExt};

use crate::types::{LlmError, StreamChunk, ToolCall};

/// Convert a byte stream into a stream of Anthropic SSE chunks.
///
/// Parses the SSE events line by line:
/// - `content_block_delta` with `delta.type = "text_delta"` emits `StreamChunk::Text`
/// - `content_block_start` with `type = "tool_use"` records id + name
/// - `content_block_delta` with `delta.type = "input_json_delta"` accumulates the JSON arguments
/// - `content_block_stop` emits `StreamChunk::ToolCall` if a tool was in progress
/// - `message_stop` ends the stream
pub(super) fn parse_sse_stream(
    byte_stream: Pin<Box<dyn Stream<Item = Result<Vec<u8>, LlmError>> + Send>>,
) -> impl Stream<Item = Result<StreamChunk, LlmError>> + Send {
    futures::stream::unfold(
        SseState {
            stream: byte_stream,
            buffer: Vec::new(),
            pending_tool: None,
        },
        |mut state| async move {
            loop {
                // Look for the next newline in the buffer
                if let Some(nl) = state.buffer.iter().position(|&b| b == b'\n') {
                    match handle_sse_line(&mut state, nl) {
                        SseAction::Continue => continue,
                        SseAction::Stop => return None,
                        SseAction::Emit(chunk) => return Some((Ok(chunk), state)),
                    }
                }

                // Need more bytes from the HTTP stream
                match state.stream.next().await {
                    Some(Ok(chunk)) => {
                        state.buffer.extend_from_slice(&chunk);
                    }
                    Some(Err(e)) => {
                        return Some((Err(e), state));
                    }
                    None => {
                        // HTTP stream ended (normally via message_stop)
                        return None;
                    }
                }
            }
        },
    )
}
/// In-progress tool call being assembled from SSE fragments.
pub(super) struct PendingToolCall {
    id: String,
    name: String,
    arguments_json: String,
}
pub(super) struct SseState {
    stream: Pin<Box<dyn Stream<Item = Result<Vec<u8>, LlmError>> + Send>>,
    buffer: Vec<u8>,
    /// Tool call currently being accumulated (one at a time).
    pending_tool: Option<PendingToolCall>,
}
/// Decision after processing an SSE line.
pub(super) enum SseAction {
    /// Irrelevant line, keep reading the buffer.
    Continue,
    /// `message_stop` received, end the stream.
    Stop,
    /// Emit a chunk to the caller.
    Emit(StreamChunk),
}
/// Extract the next line from the buffer (up to and including `nl`) and process it.
pub(super) fn handle_sse_line(state: &mut SseState, nl: usize) -> SseAction {
    let raw: Vec<u8> = state.buffer.drain(..=nl).collect();
    // Strip trailing \n and \r
    let end = raw
        .iter()
        .rposition(|&b| b != b'\n' && b != b'\r')
        .map(|i| i + 1)
        .unwrap_or(0);
    let line = std::str::from_utf8(&raw[..end]).unwrap_or("");

    let Some(data) = line.strip_prefix("data: ") else {
        return SseAction::Continue;
    };
    let Ok(json) = serde_json::from_str::<serde_json::Value>(data) else {
        return SseAction::Continue;
    };
    handle_sse_event(&json, &mut state.pending_tool)
}
/// Process an already-parsed SSE event and update the in-progress tool state.
pub(super) fn handle_sse_event(
    json: &serde_json::Value,
    pending_tool: &mut Option<PendingToolCall>,
) -> SseAction {
    match json.get("type").and_then(|t| t.as_str()) {
        Some("message_stop") => SseAction::Stop,
        Some("content_block_delta") => handle_content_block_delta(json, pending_tool),
        Some("content_block_start") => {
            record_tool_use_start(json, pending_tool);
            SseAction::Continue
        }
        Some("content_block_stop") => emit_pending_tool(pending_tool),
        _ => SseAction::Continue,
    }
}
/// `content_block_delta`: emit text, or accumulate tool JSON fragments.
pub(super) fn handle_content_block_delta(
    json: &serde_json::Value,
    pending_tool: &mut Option<PendingToolCall>,
) -> SseAction {
    match json.pointer("/delta/type").and_then(|t| t.as_str()) {
        Some("text_delta") => {
            let text = json.pointer("/delta/text").and_then(|t| t.as_str());
            match text {
                Some(text) if !text.is_empty() => {
                    SseAction::Emit(StreamChunk::Text(text.to_owned()))
                }
                _ => SseAction::Continue,
            }
        }
        // Tool call arguments arrive as JSON fragments
        Some("input_json_delta") => {
            if let Some(partial) = json.pointer("/delta/partial_json").and_then(|t| t.as_str()) {
                if let Some(pending) = pending_tool {
                    pending.arguments_json.push_str(partial);
                }
            }
            SseAction::Continue
        }
        _ => SseAction::Continue,
    }
}
/// `content_block_start`: record id + name if a `tool_use` begins.
pub(super) fn record_tool_use_start(
    json: &serde_json::Value,
    pending_tool: &mut Option<PendingToolCall>,
) {
    let block_type = json.pointer("/content_block/type").and_then(|t| t.as_str());
    if block_type != Some("tool_use") {
        return;
    }
    let id = json
        .pointer("/content_block/id")
        .and_then(|t| t.as_str())
        .unwrap_or("")
        .to_owned();
    let name = json
        .pointer("/content_block/name")
        .and_then(|t| t.as_str())
        .unwrap_or("")
        .to_owned();
    *pending_tool = Some(PendingToolCall {
        id,
        name,
        arguments_json: String::new(),
    });
}
/// `content_block_stop`: emit the accumulated `ToolCall` if one is in progress.
pub(super) fn emit_pending_tool(pending_tool: &mut Option<PendingToolCall>) -> SseAction {
    match pending_tool.take() {
        Some(pending) => {
            let arguments =
                serde_json::from_str(&pending.arguments_json).unwrap_or(serde_json::Value::Null);
            SseAction::Emit(StreamChunk::ToolCall(ToolCall {
                id: pending.id,
                name: pending.name,
                arguments,
            }))
        }
        None => SseAction::Continue,
    }
}
