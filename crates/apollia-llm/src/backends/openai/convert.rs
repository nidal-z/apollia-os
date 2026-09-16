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
/// Apply the request's structured-output constraint to a built chat request.
///
/// Returns the request as a JSON value, because the two constraints do not both
/// fit the typed builder: `response_format` does, the llama.cpp `grammar`
/// extension does not.
///
/// Which one is sent depends on the backend, and the split is measured rather
/// than assumed (llama-server 10092, 2026-09-16, both forms accepted on
/// `/v1/chat/completions`):
///
/// - An embedded llama-server gets the GBNF grammar this crate builds, so the
///   constraint is the one Apollia can read, test and explain. An untranslatable
///   schema is refused before the call rather than silently relaxed.
/// - Every other OpenAI-compatible provider gets `response_format`, which is
///   the only structured-output surface the protocol defines.
///
/// A `grammar` already on the request (the `POST /llm` route carries one) is
/// honoured ahead of the schema: it is the more specific constraint of the two.
pub(super) fn with_structured_output(
    request: async_openai::types::CreateChatCompletionRequest,
    req: &crate::types::CompletionRequest,
    llama_cpp_extensions: bool,
) -> Result<serde_json::Value, LlmError> {
    let mut body = serde_json::to_value(request)
        .map_err(|e| LlmError::InferenceError(format!("serialize request: {e}")))?;

    if llama_cpp_extensions {
        let grammar = match (&req.grammar, &req.response_schema) {
            (Some(explicit), _) => Some(explicit.clone()),
            (None, Some(schema)) => Some(crate::grammar::json_schema_to_gbnf(schema)?),
            (None, None) => None,
        };
        if let (Some(grammar), Some(map)) = (grammar, body.as_object_mut()) {
            map.insert("grammar".to_string(), serde_json::Value::String(grammar));
        }
        return Ok(body);
    }

    if let (Some(schema), Some(map)) = (&req.response_schema, body.as_object_mut()) {
        map.insert(
            "response_format".to_string(),
            serde_json::json!({
                "type": "json_schema",
                "json_schema": {
                    "name": "apollia_response",
                    "schema": crate::schema_sanitize::grammar_safe_schema(schema),
                    "strict": true,
                }
            }),
        );
    }
    Ok(body)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{ChatMessage, CompletionRequest};
    use async_openai::types::CreateChatCompletionRequestArgs;
    use serde_json::json;

    fn built_request() -> async_openai::types::CreateChatCompletionRequest {
        let messages = build_messages(&[ChatMessage::user("hello")]).expect("valid message");
        CreateChatCompletionRequestArgs::default()
            .model("test-model")
            .messages(messages)
            .build()
            .expect("a buildable request")
    }

    fn flat_schema() -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {"title": {"type": "string"}},
            "required": ["title"]
        })
    }

    #[test]
    fn a_llama_cpp_endpoint_receives_the_grammar() {
        // GIVEN a structured-output request bound for an embedded llama-server
        let req = CompletionRequest {
            messages: vec![ChatMessage::user("hello")],
            response_schema: Some(flat_schema()),
            ..Default::default()
        };
        // WHEN the constraint is applied
        let body = with_structured_output(built_request(), &req, true).expect("translatable");
        // THEN the body carries the GBNF grammar, and no response_format: the
        // constraint is the one this crate builds and tests
        let grammar = body
            .get("grammar")
            .and_then(serde_json::Value::as_str)
            .expect("a grammar field");
        assert!(grammar.contains("\"title\""), "grammar: {grammar}");
        assert!(body.get("response_format").is_none());
    }

    #[test]
    fn a_plain_openai_endpoint_receives_response_format() {
        // GIVEN the same request bound for a provider that is not llama.cpp
        let req = CompletionRequest {
            messages: vec![ChatMessage::user("hello")],
            response_schema: Some(flat_schema()),
            ..Default::default()
        };
        // WHEN the constraint is applied
        let body = with_structured_output(built_request(), &req, false).expect("no grammar needed");
        // THEN it travels as `response_format`, the only form the protocol
        // defines, and the undeclared `grammar` field is absent: sending it to a
        // real OpenAI endpoint is a 400
        assert!(body.get("grammar").is_none(), "grammar must not be sent");
        let format = body.get("response_format").expect("a response_format");
        assert_eq!(format["type"], json!("json_schema"));
        assert_eq!(format["json_schema"]["schema"]["type"], json!("object"));
    }

    #[test]
    fn an_explicit_grammar_wins_over_the_schema() {
        // GIVEN a request carrying both a grammar (as `POST /llm` allows) and a
        // schema
        let req = CompletionRequest {
            messages: vec![ChatMessage::user("hello")],
            grammar: Some("root ::= \"yes\"\n".to_string()),
            response_schema: Some(flat_schema()),
            ..Default::default()
        };
        // WHEN the constraint is applied for a llama.cpp endpoint
        let body = with_structured_output(built_request(), &req, true).expect("translatable");
        // THEN the explicit grammar is the one sent: it is the more specific of
        // the two constraints
        assert_eq!(body["grammar"], json!("root ::= \"yes\"\n"));
    }

    #[test]
    fn an_untranslatable_schema_is_refused_before_the_call() {
        // GIVEN a schema the grammar builder cannot express
        let req = CompletionRequest {
            messages: vec![ChatMessage::user("hello")],
            response_schema: Some(json!({
                "type": "object",
                "properties": {"x": {"anyOf": [{"type": "string"}, {"type": "integer"}]}},
                "required": ["x"]
            })),
            ..Default::default()
        };
        // WHEN the constraint is applied for a llama.cpp endpoint
        let err =
            with_structured_output(built_request(), &req, true).expect_err("anyOf has no grammar");
        // THEN the call never leaves, and the error names the node
        match err {
            LlmError::StructuredOutputUnsupported { path, .. } => {
                assert_eq!(path, "$.properties.x");
            }
            other => panic!("expected StructuredOutputUnsupported, got {other:?}"),
        }
    }

    #[test]
    fn a_request_without_a_schema_is_left_alone() {
        // GIVEN an ordinary free-form request
        let req = CompletionRequest {
            messages: vec![ChatMessage::user("hello")],
            ..Default::default()
        };
        // WHEN it passes through the same path
        let body = with_structured_output(built_request(), &req, true).expect("nothing to apply");
        // THEN neither constraint is added
        assert!(body.get("grammar").is_none());
        assert!(body.get("response_format").is_none());
        assert_eq!(body["model"], json!("test-model"));
    }
}
