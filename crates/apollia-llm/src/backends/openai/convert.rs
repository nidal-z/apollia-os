//! Conversion between the crate's chat types and the async-openai shapes.
//!
//! Split out of `openai.rs`: the client stays in the parent, the message and
//! tool builders, the error mapping, and the price table live here.

use async_openai::types::{
    ChatCompletionMessageToolCall, ChatCompletionRequestAssistantMessageArgs,
    ChatCompletionRequestMessage, ChatCompletionRequestMessageContentPartImage,
    ChatCompletionRequestMessageContentPartText, ChatCompletionRequestSystemMessageArgs,
    ChatCompletionRequestToolMessageArgs, ChatCompletionRequestUserMessageArgs,
    ChatCompletionRequestUserMessageContent, ChatCompletionRequestUserMessageContentPart,
    ChatCompletionTool, ChatCompletionToolType, FunctionCall, FunctionObject, ImageUrl,
};
use base64::Engine as _;

use crate::message_image::{describe_images, MessageImage};
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
                (Role::User, MessageContent::TextWithImages { text, images }) => {
                    ChatCompletionRequestUserMessageArgs::default()
                        .content(user_parts(text, images))
                        .build()
                        .map(Into::into)
                        .map_err(|e| LlmError::InferenceError(format!("user message: {e}")))
                }
                (role, MessageContent::TextWithImages { text, images }) => {
                    let (types, bytes) = describe_images(images);
                    tracing::event!(
                        tracing::Level::WARN,
                        role = ?role,
                        count = images.len(),
                        types = %types,
                        bytes = bytes,
                        "llm.images.dropped"
                    );
                    text_only(role, text)
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
/// The content parts of a user message with images: the text first, then one
/// `image_url` part per image, as a `data:<mime>;base64,` URL, in order.
fn user_parts(text: &str, images: &[MessageImage]) -> ChatCompletionRequestUserMessageContent {
    let mut parts = Vec::with_capacity(images.len() + 1);
    parts.push(ChatCompletionRequestUserMessageContentPart::Text(
        ChatCompletionRequestMessageContentPartText {
            text: text.to_owned(),
        },
    ));
    for image in images {
        let encoded = base64::engine::general_purpose::STANDARD.encode(image.bytes());
        parts.push(ChatCompletionRequestUserMessageContentPart::ImageUrl(
            ChatCompletionRequestMessageContentPartImage {
                image_url: ImageUrl {
                    url: format!("data:{};base64,{encoded}", image.mime()),
                    detail: None,
                },
            },
        ));
    }
    ChatCompletionRequestUserMessageContent::Array(parts)
}

/// A message whose images cannot travel in its role, sent as its text alone.
fn text_only(role: &Role, text: &str) -> Result<ChatCompletionRequestMessage, LlmError> {
    match role {
        Role::System => ChatCompletionRequestSystemMessageArgs::default()
            .content(text)
            .build()
            .map(Into::into)
            .map_err(|e| LlmError::InferenceError(format!("system message: {e}"))),
        Role::Assistant => ChatCompletionRequestAssistantMessageArgs::default()
            .content(text)
            .build()
            .map(Into::into)
            .map_err(|e| LlmError::InferenceError(format!("assistant message: {e}"))),
        Role::User | Role::Tool => ChatCompletionRequestUserMessageArgs::default()
            .content(text)
            .build()
            .map(Into::into)
            .map_err(|e| LlmError::InferenceError(format!("user message: {e}"))),
    }
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
        // Nothing answered at the address: the server is not started, or the
        // address is wrong. Said as such, with the address, where it used to
        // read as an HTTP error without a status.
        OpenAIError::Reqwest(req_err) if req_err.is_connect() => LlmError::BackendUnavailable {
            backend: server_address(&req_err),
            reason: "no server answers at this address: it is not started, or the address is wrong"
                .to_owned(),
        },
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
/// The scheme, host and port a request was sent to, without its path.
fn server_address(error: &reqwest::Error) -> String {
    let Some(url) = error.url() else {
        return "the configured server".to_owned();
    };
    let host = url.host_str().unwrap_or_default();
    match url.port() {
        Some(port) => format!("{}://{host}:{port}", url.scheme()),
        None => format!("{}://{host}", url.scheme()),
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
/// A schema travels as `response_format` on every backend. The embedded
/// llama-server used to get the GBNF grammar this crate builds instead, and that
/// stopped holding (llama-server `b11541`, 2026-10-12): a grammar constrains the
/// generation from its first token, and a model whose template opens on a
/// channel marker cannot write that marker under it. Measured on such a model,
/// the grammar form is answered with a 500 or runs until the caller gives up,
/// while `response_format` lets the server place the constraint after the
/// reasoning, which is where its template parser expects the answer.
///
/// Two things remain specific to llama.cpp:
///
/// - The schema is still translated by this crate before the call, and an
///   untranslatable one is refused there rather than silently relaxed by the
///   server. The translation is the check; its output is not sent.
/// - A `grammar` already on the request (the `POST /llm` route carries one) is
///   sent as is and wins over the schema: it is the more specific constraint of
///   the two, and no other backend has a field for it.
pub(super) fn with_structured_output(
    request: async_openai::types::CreateChatCompletionRequest,
    req: &crate::types::CompletionRequest,
    llama_cpp_extensions: bool,
) -> Result<serde_json::Value, LlmError> {
    let mut body = serde_json::to_value(request)
        .map_err(|e| LlmError::InferenceError(format!("serialize request: {e}")))?;

    if llama_cpp_extensions {
        if let (Some(grammar), Some(map)) = (&req.grammar, body.as_object_mut()) {
            map.insert(
                "grammar".to_string(),
                serde_json::Value::String(grammar.clone()),
            );
            return Ok(body);
        }
        if let Some(schema) = &req.response_schema {
            crate::grammar::json_schema_to_gbnf(schema)?;
        }
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

/// Write the sampling fields the OpenAI protocol does not declare onto a built
/// request body.
///
/// `top_k`, `min_p` and `repeat_penalty` have no place in the typed builder, so
/// they are added to the serialized body under the names llama-server reads
/// (`tools/server/README.md` at llama.cpp `b11541`). Each one is written only
/// when the caller fixed it: an unset field leaves the body as the protocol
/// shapes it, so a server that knows none of the three never sees them.
///
/// They are sent to every OpenAI-compatible backend when fixed, not only to the
/// embedded engine. A caller that asks for `top_k` on a server that does not
/// know the field gets whatever that server does with it, refusal included,
/// rather than a setting this crate dropped without a word.
///
/// The reasoning settings travel the same way, in `chat_template_kwargs` (see
/// [`chat_template_kwargs`]), and so does the reasoning of the earlier answers
/// (see [`with_earlier_reasoning`]).
pub(super) fn with_sampling_extensions(
    mut body: serde_json::Value,
    req: &crate::types::CompletionRequest,
) -> serde_json::Value {
    if let Some(map) = body.as_object_mut() {
        if let Some(top_k) = req.top_k {
            map.insert("top_k".to_string(), serde_json::json!(top_k));
        }
        if let Some(min_p) = req.min_p {
            map.insert("min_p".to_string(), serde_json::json!(min_p));
        }
        if let Some(penalty) = req.repeat_penalty {
            map.insert("repeat_penalty".to_string(), serde_json::json!(penalty));
        }
        if let Some(kwargs) = chat_template_kwargs(req) {
            map.insert("chat_template_kwargs".to_string(), kwargs);
        }
        if req.preserve_thinking == Some(true) {
            with_earlier_reasoning(map, &req.messages);
        }
    }
    body
}

/// Write the reasoning each assistant message carries onto the message built
/// from it, as `reasoning_content`.
///
/// The typed builder has no such field, so it is added to the serialized
/// body. [`build_messages`] builds one message per message, in order, which
/// is what lets the two lists be read side by side. `reasoning_content` is
/// the name the chat templates read on an assistant message, and the one
/// llama-server and vllm-mlx pass through to them.
fn with_earlier_reasoning(
    body: &mut serde_json::Map<String, serde_json::Value>,
    messages: &[crate::types::ChatMessage],
) {
    let Some(built) = body
        .get_mut("messages")
        .and_then(serde_json::Value::as_array_mut)
    else {
        return;
    };
    for (built, message) in built.iter_mut().zip(messages) {
        let reasoning = message.reasoning.as_deref().filter(|r| !r.is_empty());
        if let (Role::Assistant, Some(reasoning), Some(fields)) =
            (&message.role, reasoning, built.as_object_mut())
        {
            fields.insert(
                "reasoning_content".to_string(),
                serde_json::json!(reasoning),
            );
        }
    }
}

/// The `chat_template_kwargs` object that carries the reasoning settings a
/// caller fixed, `None` when it fixed none.
///
/// It is the one per-request form both local engines read: llama-server
/// (`chat_template_kwargs` in its request parser; at `b10092`
/// `reasoning_effort` was not a request field of its own, at `b11541` it is,
/// and the template arguments are read as before) and vllm-mlx 0.5.0
/// (`ChatCompletionRequest.chat_template_kwargs`, merged into the template
/// arguments). Each variable is written only when fixed, so the template keeps
/// its own default otherwise. `preserve_thinking` is written only when it is
/// on: a request that does not keep the earlier reasoning says nothing.
fn chat_template_kwargs(req: &crate::types::CompletionRequest) -> Option<serde_json::Value> {
    let mut kwargs = serde_json::Map::new();
    if let Some(thinking) = req.thinking {
        kwargs.insert(
            crate::capabilities::THINKING_VARIABLE.to_string(),
            serde_json::json!(thinking),
        );
    }
    if let Some(effort) = req.reasoning_effort {
        kwargs.insert(
            crate::capabilities::REASONING_EFFORT_VARIABLE.to_string(),
            serde_json::json!(effort.as_str()),
        );
    }
    if req.preserve_thinking == Some(true) {
        kwargs.insert(
            crate::capabilities::PRESERVE_THINKING_VARIABLE.to_string(),
            serde_json::json!(true),
        );
    }
    (!kwargs.is_empty()).then_some(serde_json::Value::Object(kwargs))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{ChatMessage, CompletionRequest, ReasoningEffort};
    use async_openai::types::CreateChatCompletionRequestArgs;
    use serde_json::json;

    #[tokio::test]
    async fn a_server_that_does_not_answer_is_named_with_its_address() {
        // GIVEN a request sent to an address where nothing listens
        let refused = reqwest::Client::new()
            .get("http://127.0.0.1:1/v1/chat/completions")
            .send()
            .await
            .expect_err("nothing listens on port 1");

        // WHEN the failure is mapped
        let mapped = map_openai_error(async_openai::error::OpenAIError::Reqwest(refused));

        // THEN it says the server is unavailable, at its address without the
        // route, where it used to read as an HTTP error with no status
        match mapped {
            LlmError::BackendUnavailable { backend, reason } => {
                assert_eq!(backend, "http://127.0.0.1:1");
                assert!(reason.contains("not started"), "{reason}");
            }
            other => panic!("expected BackendUnavailable, got {other:?}"),
        }
    }

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
    fn a_llama_cpp_endpoint_receives_the_schema_as_response_format() {
        // GIVEN a structured-output request bound for an embedded llama-server
        let req = CompletionRequest {
            messages: vec![ChatMessage::user("hello")],
            response_schema: Some(flat_schema()),
            ..Default::default()
        };
        // WHEN the constraint is applied
        let body = with_structured_output(built_request(), &req, true).expect("translatable");
        // THEN the schema travels as `response_format` and no raw grammar is
        // sent: a grammar binds the first token, which a model that opens on a
        // channel marker can never satisfy
        let format = body.get("response_format").expect("a response_format");
        assert_eq!(format["type"], json!("json_schema"));
        assert_eq!(
            format["json_schema"]["schema"]["properties"]["title"]["type"],
            json!("string")
        );
        assert!(body.get("grammar").is_none(), "{body}");
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
        assert!(body.get("response_format").is_none(), "{body}");
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

    fn reasoning_body(
        thinking: Option<bool>,
        effort: Option<ReasoningEffort>,
    ) -> serde_json::Value {
        let req = CompletionRequest {
            messages: vec![ChatMessage::user("hello")],
            thinking,
            reasoning_effort: effort,
            ..Default::default()
        };
        let body = serde_json::to_value(built_request()).expect("a serializable request");
        with_sampling_extensions(body, &req)
    }

    #[test]
    fn no_reasoning_setting_sends_no_template_kwargs() {
        // GIVEN a request that fixes neither reasoning setting
        // WHEN its body is built
        let body = reasoning_body(None, None);

        // THEN the body carries no `chat_template_kwargs`, so the template
        // keeps its own default
        assert!(body.get("chat_template_kwargs").is_none(), "{body}");
    }

    #[test]
    fn thinking_off_travels_as_the_enable_thinking_template_variable() {
        // GIVEN a request that turns reasoning off
        // WHEN its body is built
        let body = reasoning_body(Some(false), None);

        // THEN it carries exactly that switch in `chat_template_kwargs`
        assert_eq!(
            body["chat_template_kwargs"],
            json!({"enable_thinking": false})
        );
        assert!(body.get("enable_thinking").is_none());
        assert!(body.get("reasoning_effort").is_none());
    }

    #[test]
    fn a_reasoning_level_travels_as_the_reasoning_effort_template_variable() {
        // GIVEN a request that asks for each level, then one fixing both settings
        for (effort, wire) in [
            (ReasoningEffort::Low, "low"),
            (ReasoningEffort::Medium, "medium"),
            (ReasoningEffort::High, "high"),
        ] {
            // WHEN its body is built
            let body = reasoning_body(None, Some(effort));

            // THEN the level is a template variable, never a top-level field
            assert_eq!(
                body["chat_template_kwargs"],
                json!({"reasoning_effort": wire})
            );
            assert!(body.get("reasoning_effort").is_none());
        }
        let both = reasoning_body(Some(true), Some(ReasoningEffort::High));
        assert_eq!(
            both["chat_template_kwargs"],
            json!({"enable_thinking": true, "reasoning_effort": "high"})
        );
    }

    /// The body of the second turn of a conversation whose first answer was
    /// reasoned, with `preserve_thinking` as given.
    fn second_turn_body(preserve_thinking: Option<bool>) -> serde_json::Value {
        let req = CompletionRequest {
            messages: vec![
                ChatMessage::user("what is 17 times 3?"),
                ChatMessage::assistant("51").with_reasoning("17 times 3 is 30 plus 21."),
                ChatMessage::user("and times 2?"),
            ],
            thinking: Some(true),
            preserve_thinking,
            ..Default::default()
        };
        let messages = build_messages(&req.messages).expect("valid messages");
        let request = CreateChatCompletionRequestArgs::default()
            .model("test-model")
            .messages(messages)
            .build()
            .expect("a buildable request");
        let body = serde_json::to_value(request).expect("a serializable request");
        with_sampling_extensions(body, &req)
    }

    #[test]
    fn kept_reasoning_travels_with_the_earlier_answer_and_the_template_is_told() {
        // GIVEN a second turn whose conversation keeps the earlier reasoning
        // WHEN its body is built
        let body = second_turn_body(Some(true));

        // THEN the earlier answer carries its reasoning beside its content,
        // the user messages carry none, and the template variable is on next
        // to the reasoning switch
        assert_eq!(body["messages"][1]["content"], json!("51"));
        assert_eq!(
            body["messages"][1]["reasoning_content"],
            json!("17 times 3 is 30 plus 21.")
        );
        assert!(body["messages"][0].get("reasoning_content").is_none());
        assert!(body["messages"][2].get("reasoning_content").is_none());
        assert_eq!(
            body["chat_template_kwargs"],
            json!({"enable_thinking": true, "preserve_thinking": true})
        );
    }

    #[test]
    fn reasoning_that_is_not_kept_is_not_sent_and_the_variable_is_not_emitted() {
        // GIVEN the same second turn, with the setting left out and switched off
        for preserve_thinking in [None, Some(false)] {
            // WHEN its body is built
            let body = second_turn_body(preserve_thinking);

            // THEN the earlier answer travels as its content alone and the
            // template variable is absent, off included
            assert_eq!(body["messages"][1]["content"], json!("51"));
            assert!(
                body["messages"][1].get("reasoning_content").is_none(),
                "{body}"
            );
            assert_eq!(
                body["chat_template_kwargs"],
                json!({"enable_thinking": true}),
                "{preserve_thinking:?}"
            );
        }
    }

    #[test]
    fn a_user_message_with_two_images_carries_two_image_parts_after_the_text() {
        // GIVEN a user message with text and two images, PNG then JPEG
        let message = ChatMessage::user_with_images(
            "compare them",
            vec![
                crate::message_image::MessageImage::new("image/png", vec![1u8, 2, 3]),
                crate::message_image::MessageImage::new("image/jpeg", vec![4u8, 5]),
            ],
        );

        // WHEN the request body is built and serialized
        let messages = build_messages(&[message]).expect("valid message");
        let body = CreateChatCompletionRequestArgs::default()
            .model("test-model")
            .messages(messages)
            .build()
            .expect("a buildable request");
        let json = serde_json::to_value(&body).expect("serializable");

        // THEN the user content is an array: the text, then one image_url part
        // per image, in order, each a base64 data URL with its media type
        let parts = json["messages"][0]["content"]
            .as_array()
            .expect("an array of parts");
        assert_eq!(parts.len(), 3);
        assert_eq!(parts[0], json!({"type": "text", "text": "compare them"}));
        assert_eq!(parts[1]["type"], json!("image_url"));
        assert_eq!(
            parts[1]["image_url"]["url"],
            json!("data:image/png;base64,AQID")
        );
        assert_eq!(parts[2]["type"], json!("image_url"));
        assert_eq!(
            parts[2]["image_url"]["url"],
            json!("data:image/jpeg;base64,BAU=")
        );
    }

    #[test]
    fn images_in_a_role_that_cannot_carry_them_are_dropped_not_fatal() {
        // GIVEN an assistant message that somehow carries an image
        let message = ChatMessage {
            role: Role::Assistant,
            content: MessageContent::TextWithImages {
                text: "seen".into(),
                images: vec![crate::message_image::MessageImage::new(
                    "image/png",
                    vec![0u8; 4],
                )],
            },
            cache_control: None,
            reasoning: None,
        };

        // WHEN the messages are built
        let messages = build_messages(&[message]).expect("no error");

        // THEN the message travels as its text alone
        let json = serde_json::to_value(&messages[0]).expect("serializable");
        assert_eq!(json["content"], json!("seen"));
    }
}
