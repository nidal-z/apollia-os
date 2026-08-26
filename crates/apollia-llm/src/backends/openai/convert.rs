//! Conversion between the crate's chat types and the async-openai shapes.
//!
//! Split out of `openai.rs`: the client stays in the parent, the message and
//! tool builders, the error mapping, and the price table live here.

use async_openai::types::{
    ChatCompletionMessageToolCall, ChatCompletionRequestAssistantMessageArgs,
    ChatCompletionRequestMessage, ChatCompletionRequestSystemMessageArgs,
    ChatCompletionRequestToolMessageArgs, ChatCompletionRequestUserMessageArgs, ChatCompletionTool,
    ChatCompletionToolType, FunctionCall, FunctionObject,
};

use crate::types::{FinishReason, LlmError, MessageContent, Role};

/// gpt-4o-mini input price per token (OpenAI 2024 rates).
pub(super) const GPT_4O_MINI_PROMPT_RATE: f64 = 0.15e-6;
/// gpt-4o-mini output price per token.
pub(super) const GPT_4O_MINI_COMPLETION_RATE: f64 = 0.60e-6;
/// gpt-4o input price per token.
pub(super) const GPT_4O_PROMPT_RATE: f64 = 2.50e-6;
/// gpt-4o output price per token.
pub(super) const GPT_4O_COMPLETION_RATE: f64 = 10.00e-6;
/// gpt-3.5-turbo input price per token.
pub(super) const GPT_35_TURBO_PROMPT_RATE: f64 = 0.50e-6;
/// gpt-3.5-turbo output price per token.
pub(super) const GPT_35_TURBO_COMPLETION_RATE: f64 = 1.50e-6;
/// Convert Apollia messages into `async-openai` messages.
pub(super) fn build_messages(
    messages: &[crate::types::ChatMessage],
) -> Result<Vec<ChatCompletionRequestMessage>, LlmError> {
    messages
        .iter()
        .map(|msg| -> Result<ChatCompletionRequestMessage, LlmError> {
            match (&msg.role, &msg.content) {
                (Role::System, MessageContent::Text(text)) => {
                    ChatCompletionRequestSystemMessageArgs::default()
                        .content(text.as_str())
                        .build()
                        .map(Into::into)
                        .map_err(|e| LlmError::InferenceError(format!("system message: {e}")))
                }
                (Role::User, MessageContent::Text(text)) => {
                    ChatCompletionRequestUserMessageArgs::default()
                        .content(text.as_str())
                        .build()
                        .map(Into::into)
                        .map_err(|e| LlmError::InferenceError(format!("user message: {e}")))
                }
                (Role::Assistant, MessageContent::Text(text)) => {
                    ChatCompletionRequestAssistantMessageArgs::default()
                        .content(text.as_str())
                        .build()
                        .map(Into::into)
                        .map_err(|e| LlmError::InferenceError(format!("assistant message: {e}")))
                }
                (Role::Assistant, MessageContent::WithToolCalls { text, tool_calls }) => {
                    let openai_calls: Vec<ChatCompletionMessageToolCall> = tool_calls
                        .iter()
                        .map(|tc| ChatCompletionMessageToolCall {
                            id: tc.id.clone(),
                            r#type: ChatCompletionToolType::Function,
                            function: FunctionCall {
                                name: tc.name.clone(),
                                arguments: tc.arguments.to_string(),
                            },
                        })
                        .collect();
                    let mut builder = ChatCompletionRequestAssistantMessageArgs::default();
                    if !text.is_empty() {
                        builder.content(text.as_str());
                    }
                    builder
                        .tool_calls(openai_calls)
                        .build()
                        .map(Into::into)
                        .map_err(|e| {
                            LlmError::InferenceError(format!("assistant+tools message: {e}"))
                        })
                }
                (
                    Role::Tool,
                    MessageContent::ToolResult {
                        tool_call_id,
                        content,
                    },
                ) => ChatCompletionRequestToolMessageArgs::default()
                    .content(content.as_str())
                    .tool_call_id(tool_call_id.as_str())
                    .build()
                    .map(Into::into)
                    .map_err(|e| LlmError::InferenceError(format!("tool message: {e}"))),
                (role, content) => Err(LlmError::InferenceError(format!(
                    "unsupported role/content combination: {role:?}/{content:?}"
                ))),
            }
        })
        .collect()
}
/// Convert Apollia tool specs into `async-openai` tools.
///
/// Tool `parameters` are normalized by [`crate::schema_sanitize::grammar_safe_schema`]
/// so a llama.cpp-backed server can build a valid tool-calling grammar from
/// them; see that module for the constructs it neutralizes.
pub(super) fn build_tools(tools: &[crate::types::ToolSpec]) -> Vec<ChatCompletionTool> {
    tools
        .iter()
        .map(|spec| ChatCompletionTool {
            r#type: ChatCompletionToolType::Function,
            function: FunctionObject {
                name: spec.name.clone(),
                description: Some(spec.description.clone()),
                parameters: Some(crate::schema_sanitize::grammar_safe_schema(
                    &spec.parameters,
                )),
                strict: None,
            },
        })
        .collect()
}
/// Map the `async-openai` `FinishReason` to the Apollia [`FinishReason`].
pub(super) fn map_finish_reason(
    reason: Option<&async_openai::types::FinishReason>,
) -> FinishReason {
    match reason {
        Some(async_openai::types::FinishReason::Stop) => FinishReason::Stop,
        Some(async_openai::types::FinishReason::Length) => FinishReason::Length,
        Some(async_openai::types::FinishReason::ToolCalls) => FinishReason::ToolCalls,
        Some(async_openai::types::FinishReason::FunctionCall) => FinishReason::ToolCalls,
        Some(async_openai::types::FinishReason::ContentFilter) => FinishReason::Error,
        None => FinishReason::Stop,
    }
}
/// Map an `async-openai` error to [`LlmError`].
///
/// Transient HTTP statuses (429, 503, 529) are mapped to retryable [`LlmError`]
/// variants so [`RetryPolicy`] can detect them.
pub(super) fn map_openai_error(err: async_openai::error::OpenAIError) -> LlmError {
    use async_openai::error::OpenAIError;
    match err {
        OpenAIError::Reqwest(req_err) => {
            let status = req_err.status().map(|s| s.as_u16()).unwrap_or(0);
            match status {
                401 => LlmError::Unauthorized,
                429 => LlmError::RateLimit,
                503 => LlmError::ServiceUnavailable,
                529 => LlmError::Overload,
                _ => LlmError::HttpError {
                    status,
                    body: req_err.to_string(),
                },
            }
        }
        OpenAIError::ApiError(api_err) => LlmError::HttpError {
            status: 0,
            body: api_err.message,
        },
        // async-openai fails to parse a response body that is not OpenAI-conformant.
        // With llama.cpp / llama-server this is almost always an ERROR body whose
        // `code` is an integer (e.g. `{"error":{"code":400,...}}`) where the OpenAI
        // schema expects a string, so the real HTTP error would otherwise be masked
        // behind a cryptic serde type mismatch. Recover the status when it is in
        // there and name the likely cause, rather than blaming the context size for
        // every failure: a 404 is a routing mistake, not an oversized prompt.
        OpenAIError::JSONDeserialize(e) => {
            let detail = e.to_string();
            let body = match status_from_unparseable_body(&detail) {
                Some(404) => format!(
                    "backend returned 404 for this route, so the base URL is very \
                     likely wrong. The OpenAI-compatible client appends \
                     `/chat/completions` to the configured base, which must therefore \
                     already carry the provider's API prefix (`/v1` for Ollama, \
                     OpenAI and Mistral). Parser detail: {detail}"
                ),
                Some(status) => format!(
                    "backend returned HTTP {status} in a body the OpenAI client could \
                     not parse (integer 'code', as llama.cpp/llama-server emits). \
                     Usual causes are a prompt exceeding the context size or a \
                     malformed request. Check the backend server log. Parser detail: \
                     {detail}"
                ),
                None => format!(
                    "backend returned a response the OpenAI client could not parse. \
                     Check the backend server log. Parser detail: {detail}"
                ),
            };
            LlmError::HttpError { status: 0, body }
        }
        other => LlmError::InferenceError(other.to_string()),
    }
}
/// Recovers the HTTP status from a body the OpenAI schema could not parse.
///
/// Backends that emit `{"error":{"code":404,...}}` put an integer where the
/// OpenAI schema expects a string, and serde reports that as
/// ``invalid type: integer `404` ``. The status is the only actionable part of
/// the failure, so it is worth digging out of the message.
pub(super) fn status_from_unparseable_body(detail: &str) -> Option<u16> {
    let after = detail.split_once("invalid type: integer `")?.1;
    after.split_once('`')?.0.parse().ok()
}
/// Estimate the cost in USD from the number of tokens consumed.
///
/// Returns `None` for models not listed in the price table. Rates are based on
/// the OpenAI prices published in May 2024. This estimate is indicative;
/// prices may vary.
pub(super) fn estimate_cost_usd(
    model: &str,
    prompt_tokens: u32,
    completion_tokens: u32,
) -> Option<f64> {
    let (prompt_rate, completion_rate) = if model.contains("gpt-4o-mini") {
        (GPT_4O_MINI_PROMPT_RATE, GPT_4O_MINI_COMPLETION_RATE)
    } else if model.contains("gpt-4o") {
        (GPT_4O_PROMPT_RATE, GPT_4O_COMPLETION_RATE)
    } else if model.contains("gpt-3.5-turbo") {
        (GPT_35_TURBO_PROMPT_RATE, GPT_35_TURBO_COMPLETION_RATE)
    } else {
        return None;
    };

    Some(prompt_tokens as f64 * prompt_rate + completion_tokens as f64 * completion_rate)
}
