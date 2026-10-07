//! One message of a conversation, as every backend receives it.
//!
//! Split out of `types.rs`: the roles and the content shapes stay in the
//! parent, the message and its constructors live here.

use super::{CacheControl, MessageContent, Role, ToolCall};

/// A message in a multi-turn conversation history.
#[derive(Debug, Clone)]
pub struct ChatMessage {
    /// Role of the message sender.
    pub role: Role,
    /// Message content (text, tool result, or text + tool calls).
    pub content: MessageContent,
    /// Prompt-caching marker, ignored by every current backend (see
    /// [`CacheControl`]).
    pub cache_control: Option<CacheControl>,
    /// The reasoning that came before this answer, kept apart from its
    /// content. Only an assistant message of an earlier turn carries one, and
    /// only a request that sets [`CompletionRequest::preserve_thinking`] sends
    /// it. `None` on every other message.
    pub reasoning: Option<String>,
}

impl ChatMessage {
    /// Build a system message (global behavior instructions).
    pub fn system(text: impl Into<String>) -> Self {
        Self {
            role: Role::System,
            content: MessageContent::Text(text.into()),
            cache_control: None,
            reasoning: None,
        }
    }

    /// Build a user message.
    pub fn user(text: impl Into<String>) -> Self {
        Self {
            role: Role::User,
            content: MessageContent::Text(text.into()),
            cache_control: None,
            reasoning: None,
        }
    }

    /// Build an assistant message with no tool calls.
    pub fn assistant(text: impl Into<String>) -> Self {
        Self {
            role: Role::Assistant,
            content: MessageContent::Text(text.into()),
            cache_control: None,
            reasoning: None,
        }
    }

    /// Build an assistant message that includes tool calls.
    pub fn assistant_with_calls(text: &str, calls: &[ToolCall]) -> Self {
        Self {
            role: Role::Assistant,
            content: MessageContent::WithToolCalls {
                text: text.to_owned(),
                tool_calls: calls.to_vec(),
            },
            cache_control: None,
            reasoning: None,
        }
    }

    /// Build a message carrying the result of a tool call.
    pub fn tool_result(call_id: &str, content: &str) -> Self {
        Self {
            role: Role::Tool,
            content: MessageContent::ToolResult {
                tool_call_id: call_id.to_owned(),
                content: content.to_owned(),
            },
            cache_control: None,
            reasoning: None,
        }
    }

    /// The same message carrying the reasoning that came before it.
    #[must_use]
    pub fn with_reasoning(mut self, reasoning: impl Into<String>) -> Self {
        self.reasoning = Some(reasoning.into());
        self
    }
}
