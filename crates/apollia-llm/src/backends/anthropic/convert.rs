//! Conversion from the crate's chat types to the Anthropic wire shapes.
//!
//! Split out of `anthropic.rs`: the client stays in the parent, the pure
//! functions that build blocks, place the cache breakpoints, and map a stop
//! reason live here.

use crate::backends::anthropic::wire::{
    AnthropicBlock, AnthropicCacheControl, AnthropicContent, AnthropicMessage, AnthropicSystem,
    AnthropicSystemBlock, AnthropicTool,
};
use crate::types::{CacheControl, FinishReason, MessageContent, Role};

/// Apply the three automatic cache breakpoints on the Anthropic request.
///
/// Breakpoint 1, system prompt: converted to an array of blocks with
/// `cache_control: ephemeral`.
/// Breakpoint 2, last tool: marked with `cache_control: ephemeral`.
/// Breakpoint 3, 3rd message from the end: its last content block marked.
///
/// The breakpoints are cumulative with the individual marks from
/// `ChatMessage.cache_control` (already applied by [`convert_message`]).
pub(super) fn apply_cache_breakpoints(
    messages: &mut [AnthropicMessage],
    system: &mut Option<AnthropicSystem>,
    tools: &mut Option<Vec<AnthropicTool>>,
) {
    // Breakpoint 1: system prompt to a block with cache_control
    if let Some(sys) = system.as_mut() {
        *sys = match sys.clone() {
            AnthropicSystem::Plain(text) => AnthropicSystem::Blocks(vec![AnthropicSystemBlock {
                block_type: "text",
                text,
                cache_control: Some(AnthropicCacheControl::ephemeral()),
            }]),
            AnthropicSystem::Blocks(mut blocks) => {
                if let Some(last) = blocks.last_mut() {
                    last.cache_control = Some(AnthropicCacheControl::ephemeral());
                }
                AnthropicSystem::Blocks(blocks)
            }
        };
    }

    // Breakpoint 2: last tool
    if let Some(tools_vec) = tools.as_mut() {
        if let Some(last_tool) = tools_vec.last_mut() {
            last_tool.cache_control = Some(AnthropicCacheControl::ephemeral());
        }
    }

    // Breakpoint 3: 3rd message from the end (sliding breakpoint)
    let len = messages.len();
    if len >= 3 {
        mark_message_cache_control(&mut messages[len - 3]);
    }
}
/// Mark the last content block of a message with `cache_control: ephemeral`.
///
/// If the content is `Text(String)`, converts it to `Blocks([Text { cache_control }])`.
/// If the content is `Blocks(...)`, applies `cache_control` to the last `Text` or `ToolResult` block.
pub(super) fn mark_message_cache_control(msg: &mut AnthropicMessage) {
    let new_content = match msg.content.clone() {
        AnthropicContent::Text(text) => AnthropicContent::Blocks(vec![AnthropicBlock::Text {
            text,
            cache_control: Some(AnthropicCacheControl::ephemeral()),
        }]),
        AnthropicContent::Blocks(mut blocks) => {
            if let Some(last) = blocks.last_mut() {
                match last {
                    AnthropicBlock::Text { cache_control, .. } => {
                        *cache_control = Some(AnthropicCacheControl::ephemeral());
                    }
                    AnthropicBlock::ToolResult { cache_control, .. } => {
                        *cache_control = Some(AnthropicCacheControl::ephemeral());
                    }
                    AnthropicBlock::ToolUse { .. } => {
                        // ToolUse does not support cache_control, breakpoint ignored
                        tracing::debug!("llm.cache.breakpoint.skipped");
                    }
                }
            }
            AnthropicContent::Blocks(blocks)
        }
    };
    msg.content = new_content;
}
/// Build a text [`AnthropicContent`], as a block with `cache_control` if
/// `cache` is `Some`, otherwise as plain text.
pub(super) fn text_content(text: &str, cache: Option<AnthropicCacheControl>) -> AnthropicContent {
    if let Some(cc) = cache {
        AnthropicContent::Blocks(vec![AnthropicBlock::Text {
            text: text.to_owned(),
            cache_control: Some(cc),
        }])
    } else {
        AnthropicContent::Text(text.to_owned())
    }
}
/// Build the blocks of a `WithToolCalls` assistant message (optional text plus
/// one `tool_use` block per call), with `cache_control` set on the last
/// compatible block if `cache` is `Some`.
pub(super) fn tool_call_blocks(
    text: &str,
    tool_calls: &[crate::types::ToolCall],
    cache: Option<AnthropicCacheControl>,
) -> Vec<AnthropicBlock> {
    let mut blocks: Vec<AnthropicBlock> = Vec::new();
    if !text.is_empty() {
        blocks.push(AnthropicBlock::Text {
            text: text.to_owned(),
            cache_control: None,
        });
    }
    for tc in tool_calls {
        blocks.push(AnthropicBlock::ToolUse {
            id: tc.id.clone(),
            name: tc.name.clone(),
            input: tc.arguments.clone(),
        });
    }
    // cache_control on the last block if marked
    if let Some(cc) = cache {
        if let Some(last) = blocks.last_mut() {
            match last {
                AnthropicBlock::Text { cache_control, .. } => {
                    *cache_control = Some(cc);
                }
                AnthropicBlock::ToolUse { .. } => {
                    // ToolUse does not support cache_control, ignored
                }
                AnthropicBlock::ToolResult { cache_control, .. } => {
                    *cache_control = Some(cc);
                }
            }
        }
    }
    blocks
}
pub(super) fn convert_message(msg: &crate::types::ChatMessage) -> Option<AnthropicMessage> {
    let cache = msg
        .cache_control
        .as_ref()
        .filter(|cc| **cc == CacheControl::Ephemeral)
        .map(|_| AnthropicCacheControl::ephemeral());

    match (&msg.role, &msg.content) {
        (Role::User, MessageContent::Text(text)) => Some(AnthropicMessage {
            role: "user",
            content: text_content(text, cache),
        }),
        (Role::Assistant, MessageContent::Text(text)) => Some(AnthropicMessage {
            role: "assistant",
            content: text_content(text, cache),
        }),
        (Role::Assistant, MessageContent::WithToolCalls { text, tool_calls }) => {
            Some(AnthropicMessage {
                role: "assistant",
                content: AnthropicContent::Blocks(tool_call_blocks(text, tool_calls, cache)),
            })
        }
        (
            Role::Tool,
            MessageContent::ToolResult {
                tool_call_id,
                content,
            },
        ) => {
            // Tool results are user messages with a tool_result block
            Some(AnthropicMessage {
                role: "user",
                content: AnthropicContent::Blocks(vec![AnthropicBlock::ToolResult {
                    tool_use_id: tool_call_id.clone(),
                    content: content.clone(),
                    cache_control: cache,
                }]),
            })
        }
        // System handled separately, ignored here
        (Role::System, _) => None,
        // Unsupported combinations, ignored with a warning
        (role, content) => {
            tracing::warn!(
                role = ?role,
                content = ?content,
                reason = "unsupported role and content combination",
                "llm.message.skipped"
            );
            None
        }
    }
}
/// Map the Anthropic `stop_reason` to the Apollia [`FinishReason`].
pub(super) fn map_stop_reason(stop_reason: &str) -> FinishReason {
    match stop_reason {
        "end_turn" => FinishReason::Stop,
        "tool_use" => FinishReason::ToolCalls,
        "max_tokens" => FinishReason::Length,
        _ => FinishReason::Stop,
    }
}
