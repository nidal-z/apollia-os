//! OpenAI-compatible HTTP client via `async-openai`.
//!
//! This module compiles only with `feature = "cloud"`.
//!
//! Supports any OpenAI-compatible provider (OpenAI, Mistral, Groq, etc.) via a
//! configurable base URL. The API key is read from an environment variable at
//! construction time, never stored in clear text.

use std::pin::Pin;
use std::time::Instant;

use std::collections::HashMap;

use tokio_util::sync::CancellationToken;

use crate::retry::RetryPolicy;

use async_openai::{
    config::OpenAIConfig,
    types::{
        ChatCompletionMessageToolCall, ChatCompletionRequestAssistantMessageArgs,
        ChatCompletionRequestMessage, ChatCompletionRequestSystemMessageArgs,
        ChatCompletionRequestToolMessageArgs, ChatCompletionRequestUserMessageArgs,
        ChatCompletionResponseStream, ChatCompletionTool, ChatCompletionToolType,
        CreateChatCompletionRequestArgs, FunctionCall, FunctionObject,
    },
    Client,
};
use futures::{Stream, StreamExt};

use crate::types::{
    CompletionModel, CompletionRequest, CompletionResponse, FinishReason, LlmError, MessageContent,
    Role, StreamChunk, TokenUsage, ToolCall,
};

// ── Price table (per token, in USD) ──────────────────────────────────────

/// gpt-4o-mini input price per token (OpenAI 2024 rates).
const GPT_4O_MINI_PROMPT_RATE: f64 = 0.15e-6;
/// gpt-4o-mini output price per token.
const GPT_4O_MINI_COMPLETION_RATE: f64 = 0.60e-6;
/// gpt-4o input price per token.
const GPT_4O_PROMPT_RATE: f64 = 2.50e-6;
/// gpt-4o output price per token.
const GPT_4O_COMPLETION_RATE: f64 = 10.00e-6;
/// gpt-3.5-turbo input price per token.
const GPT_35_TURBO_PROMPT_RATE: f64 = 0.50e-6;
/// gpt-3.5-turbo output price per token.
const GPT_35_TURBO_COMPLETION_RATE: f64 = 1.50e-6;

/// Configuration for an OpenAI-compatible API backend.
///
/// Deserializable from TOML via `[[llm.backends]]` in `apollia.toml` for
/// entries of type `"api"`. The API key is never stored here; it is read from
/// the `api_key_env` environment variable.
///
/// # TOML example
///
/// ```toml
/// [[llm.backends]]
/// name        = "openai"
/// api_url     = "https://api.openai.com/v1"
/// api_key_env = "OPENAI_API_KEY"
/// model       = "gpt-4o-mini"
/// ```
#[derive(Debug, Clone, serde::Deserialize)]
pub struct ApiBackendConfig {
    /// Logical backend name (the key used in `LlmRouter`).
    pub name: String,
    /// API base URL (e.g. `https://api.openai.com/v1`).
    pub api_url: String,
    /// Name of the environment variable holding the API key.
    pub api_key_env: String,
    /// Default model identifier for this backend.
    pub model: String,
}

impl ApiBackendConfig {
    /// Read the API key from the `api_key_env` environment variable.
    ///
    /// Returns `Err(LlmError::ApiKeyMissing)` if the variable is not set.
    /// The key is never logged or stored beyond the call.
    pub fn resolve_api_key(&self) -> Result<String, LlmError> {
        std::env::var(&self.api_key_env).map_err(|_| LlmError::ApiKeyMissing {
            var: self.api_key_env.clone(),
        })
    }
}

/// HTTP client for any OpenAI-compatible backend.
///
/// Built via [`OpenAICompatibleClient::new`] with an [`ApiBackendConfig`] and a
/// resolved API key. Supports [`complete`](Self::complete) (full response) and
/// [`stream`](Self::stream) (SSE streaming chunk by chunk).
///
/// A single client can be shared via `Arc<OpenAICompatibleClient>`:
/// `async_openai::Client` is `Clone + Send + Sync`.
pub struct OpenAICompatibleClient {
    /// async-openai HTTP client configured with the base URL and API key.
    client: Client<OpenAIConfig>,
    /// Backend configuration (name, URL, default model).
    config: ApiBackendConfig,
    /// Exponential retry policy shared with the other backends.
    retry_policy: RetryPolicy,
    /// Session cancellation token: `cancel()` interrupts in-flight calls and delays.
    cancel: CancellationToken,
}

impl OpenAICompatibleClient {
    /// Build an OpenAI-compatible client ready to send requests.
    ///
    /// `api_key` must be obtained beforehand via
    /// [`ApiBackendConfig::resolve_api_key`]; it is passed in here and not
    /// re-read from the environment to avoid TOCTOU.
    ///
    /// `cancel` is the LLM session's `CancellationToken`, shared by the
    /// `LlmRouter`. A call to `cancel.cancel()` interrupts in-flight calls and
    /// retry delays.
    pub fn new(config: &ApiBackendConfig, api_key: String, cancel: CancellationToken) -> Self {
        let openai_config = OpenAIConfig::new()
            .with_api_key(api_key)
            .with_api_base(config.api_url.clone());
        Self {
            client: Client::with_config(openai_config),
            config: config.clone(),
            retry_policy: RetryPolicy::default(),
            cancel,
        }
    }
    /// Make a single call to the OpenAI-compatible API without retry.
    ///
    /// Maps transient HTTP statuses to retryable [`LlmError`] variants:
    /// - 429 to [`LlmError::RateLimit`]
    /// - 503 to [`LlmError::ServiceUnavailable`]
    /// - 529 to [`LlmError::Overload`]
    /// - 401 to [`LlmError::Unauthorized`]
    async fn do_complete(&self, req: CompletionRequest) -> Result<CompletionResponse, LlmError> {
        let started = Instant::now();
        let model = req
            .model
            .as_deref()
            .unwrap_or(&self.config.model)
            .to_owned();

        let messages = build_messages(&req.messages)?;
        let tools = build_tools(&req.tools);

        let mut builder = CreateChatCompletionRequestArgs::default();
        builder.model(&model).messages(messages);

        if !tools.is_empty() {
            builder.tools(tools);
        }
        if let Some(temp) = req.temperature {
            builder.temperature(temp);
        }
        if let Some(max_tokens) = req.max_tokens {
            builder.max_tokens(max_tokens);
        }

        let request = builder
            .build()
            .map_err(|e| LlmError::InferenceError(format!("build request: {e}")))?;

        let response = self
            .client
            .chat()
            .create(request)
            .await
            .map_err(map_openai_error)?;

        let latency_ms = started.elapsed().as_millis() as u64;

        let choice = response
            .choices
            .into_iter()
            .next()
            .ok_or_else(|| LlmError::ParseError("no choices in response".to_owned()))?;

        let finish_reason = map_finish_reason(choice.finish_reason.as_ref());
        let content = choice.message.content.unwrap_or_default();

        let tool_calls = choice
            .message
            .tool_calls
            .unwrap_or_default()
            .into_iter()
            .map(|tc| {
                let arguments = serde_json::from_str(&tc.function.arguments).unwrap_or_else(|e| {
                    tracing::warn!(
                        tool_id = %tc.id,
                        error = %e,
                        "failed to parse tool call arguments as JSON, using null"
                    );
                    serde_json::Value::Null
                });
                ToolCall {
                    id: tc.id,
                    name: tc.function.name,
                    arguments,
                }
            })
            .collect();

        let usage = match response.usage {
            Some(u) => TokenUsage {
                prompt_tokens: u.prompt_tokens,
                completion_tokens: u.completion_tokens,
                cost_usd: estimate_cost_usd(&model, u.prompt_tokens, u.completion_tokens),
                ..Default::default()
            },
            None => TokenUsage {
                prompt_tokens: 0,
                completion_tokens: 0,
                cost_usd: None,
                ..Default::default()
            },
        };

        Ok(CompletionResponse {
            content,
            tool_calls,
            usage,
            finish_reason,
            latency_ms,
            ttft_ms: None,
        })
    }
}

#[async_trait::async_trait]
impl CompletionModel for OpenAICompatibleClient {
    /// Send an inference request and return the full response.
    ///
    /// Delegates to [`do_complete`](Self::do_complete) via
    /// [`RetryPolicy::execute`]: transient errors (429, 503, 529) are retried
    /// with exponential backoff. Cancellation via the `CancellationToken`
    /// immediately interrupts the wait.
    async fn complete(&self, req: CompletionRequest) -> Result<CompletionResponse, LlmError> {
        self.retry_policy
            .execute(self.cancel.clone(), || {
                let req = req.clone();
                async move { self.do_complete(req).await }
            })
            .await
    }

    /// Return a stream of text chunks via SSE.
    ///
    /// Each item is a non-empty `Ok(String)`. Empty chunks (SSE heartbeats) are
    /// silently ignored. The stream ends normally at the end of generation
    /// (`finish_reason = Stop` or `Length`).
    async fn stream(
        &self,
        req: CompletionRequest,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<StreamChunk, LlmError>> + Send>>, LlmError> {
        let model = req
            .model
            .as_deref()
            .unwrap_or(&self.config.model)
            .to_owned();

        let messages = build_messages(&req.messages)?;
        let tools = build_tools(&req.tools);

        let mut builder = CreateChatCompletionRequestArgs::default();
        builder.model(&model).messages(messages);

        if !tools.is_empty() {
            builder.tools(tools);
        }
        if let Some(temp) = req.temperature {
            builder.temperature(temp);
        }
        if let Some(max_tokens) = req.max_tokens {
            builder.max_tokens(max_tokens);
        }

        let request = builder
            .build()
            .map_err(|e| LlmError::InferenceError(format!("build stream request: {e}")))?;

        let sse_stream = self
            .client
            .chat()
            .create_stream(request)
            .await
            .map_err(map_openai_error)?;

        // OpenAI streams tool calls as fragments across multiple SSE chunks,
        // keyed by `index`.  Text tokens are emitted immediately.  Tool call
        // fragments are accumulated and flushed when the SSE stream ends.
        //
        // State transitions: Streaming → Flushing → Done.
        let state = OpenAIStreamState::Streaming {
            inner: sse_stream,
            pending: HashMap::new(),
        };

        let mapped = futures::stream::unfold(state, next_openai_stream_item);

        Ok(Box::pin(mapped))
    }

    /// Return `true`: the client is configured and ready to send requests.
    fn is_available(&self) -> bool {
        true
    }

    /// Logical backend name as configured in `apollia.toml`.
    fn backend_name(&self) -> &str {
        &self.config.name
    }

    /// Default model identifier for this backend.
    fn model_id(&self) -> &str {
        &self.config.model
    }
}

// ── Private helpers ──────────────────────────────────────────────────────

/// Convert Apollia messages into `async-openai` messages.
fn build_messages(
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
fn build_tools(tools: &[crate::types::ToolSpec]) -> Vec<ChatCompletionTool> {
    tools
        .iter()
        .map(|spec| ChatCompletionTool {
            r#type: ChatCompletionToolType::Function,
            function: FunctionObject {
                name: spec.name.clone(),
                description: Some(spec.description.clone()),
                parameters: Some(spec.parameters.clone()),
                strict: None,
            },
        })
        .collect()
}

/// Map the `async-openai` `FinishReason` to the Apollia [`FinishReason`].
fn map_finish_reason(reason: Option<&async_openai::types::FinishReason>) -> FinishReason {
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
fn map_openai_error(err: async_openai::error::OpenAIError) -> LlmError {
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
        // schema expects a string, so the real HTTP error (a prompt exceeding the
        // context size, a bad request, ...) would otherwise be masked behind a
        // cryptic serde type mismatch. Surface an actionable message + the log hint.
        OpenAIError::JSONDeserialize(e) => LlmError::HttpError {
            status: 0,
            body: format!(
                "backend returned a response the OpenAI client could not parse; with \
                 llama.cpp/llama-server this is usually a non-conformant error body \
                 (integer 'code') masking an HTTP 4xx/5xx such as a prompt exceeding \
                 the context size. Check the backend server log. Parser detail: {e}"
            ),
        },
        other => LlmError::InferenceError(other.to_string()),
    }
}

/// State machine for the OpenAI streaming response.
///
/// Tool call fragments are accumulated during `Streaming` and flushed as
/// `StreamChunk::ToolCall` items during `Flushing`.
enum OpenAIStreamState {
    /// Reading SSE chunks from the inner stream.
    Streaming {
        inner: ChatCompletionResponseStream,
        pending: HashMap<u32, PartialToolCall>,
    },
    /// SSE stream ended; emitting accumulated tool calls one by one.
    Flushing { remaining: Vec<ToolCall> },
    /// Fully consumed.
    Done,
}

/// Accumulated fragments for a single tool call during OpenAI streaming.
#[derive(Default)]
struct PartialToolCall {
    id: String,
    name: String,
    arguments: String,
}

/// Single step of the OpenAI stream `unfold`: read the next SSE chunk, emit
/// text immediately, accumulate tool call fragments, then flush them when the
/// stream ends.
async fn next_openai_stream_item(
    state: OpenAIStreamState,
) -> Option<(Result<StreamChunk, LlmError>, OpenAIStreamState)> {
    match state {
        OpenAIStreamState::Done => None,
        OpenAIStreamState::Flushing { mut remaining } => remaining.pop().map(|call| {
            (
                Ok(StreamChunk::ToolCall(call)),
                OpenAIStreamState::Flushing { remaining },
            )
        }),
        OpenAIStreamState::Streaming {
            mut inner,
            mut pending,
        } => {
            loop {
                match inner.next().await {
                    Some(Ok(response)) => {
                        if let Some(text) = next_emittable_text(&mut pending, response) {
                            return Some((
                                Ok(StreamChunk::Text(text)),
                                OpenAIStreamState::Streaming { inner, pending },
                            ));
                        }
                        continue;
                    }
                    Some(Err(e)) => {
                        return Some((Err(map_openai_error(e)), OpenAIStreamState::Done));
                    }
                    None => {
                        // SSE stream ended, flush accumulated tool calls
                        return flush_pending_tool_calls(pending);
                    }
                }
            }
        }
    }
}

/// Process an SSE chunk: accumulate tool call fragments into `pending` and
/// return the delta text if it is non-empty (to emit immediately), otherwise
/// `None` (chunk consumed, keep reading).
fn next_emittable_text(
    pending: &mut HashMap<u32, PartialToolCall>,
    response: async_openai::types::CreateChatCompletionStreamResponse,
) -> Option<String> {
    let choice = response.choices.into_iter().next()?;

    if let Some(tc_chunks) = choice.delta.tool_calls {
        accumulate_tool_calls(pending, tc_chunks);
    }

    let text = choice.delta.content.unwrap_or_default();
    if text.is_empty() {
        None
    } else {
        Some(text)
    }
}

/// Accumulate the tool call fragments received in an SSE chunk, indexed by
/// `index`.
fn accumulate_tool_calls(
    pending: &mut HashMap<u32, PartialToolCall>,
    tc_chunks: Vec<async_openai::types::ChatCompletionMessageToolCallChunk>,
) {
    for tc in tc_chunks {
        let entry = pending.entry(tc.index).or_default();
        if let Some(id) = tc.id {
            entry.id = id;
        }
        if let Some(func) = tc.function {
            if let Some(name) = func.name {
                entry.name = name;
            }
            if let Some(args) = func.arguments {
                entry.arguments.push_str(&args);
            }
        }
    }
}

/// At the end of the stream: sort the accumulated calls by index and switch to
/// `Flushing` to emit them one by one (or finish if there are none).
fn flush_pending_tool_calls(
    pending: HashMap<u32, PartialToolCall>,
) -> Option<(Result<StreamChunk, LlmError>, OpenAIStreamState)> {
    if pending.is_empty() {
        return None;
    }
    let mut calls: Vec<(u32, PartialToolCall)> = pending.into_iter().collect();
    calls.sort_by_key(|(idx, _)| *idx);

    let mut tool_calls: Vec<ToolCall> = calls
        .into_iter()
        .map(|(_, partial)| {
            let arguments =
                serde_json::from_str(&partial.arguments).unwrap_or(serde_json::Value::Null);
            ToolCall {
                id: partial.id,
                name: partial.name,
                arguments,
            }
        })
        .collect();

    // Reverse so we can pop from the end in order
    tool_calls.reverse();
    tool_calls.pop().map(|call| {
        (
            Ok(StreamChunk::ToolCall(call)),
            OpenAIStreamState::Flushing {
                remaining: tool_calls,
            },
        )
    })
}

/// Estimate the cost in USD from the number of tokens consumed.
///
/// Returns `None` for models not listed in the price table. Rates are based on
/// the OpenAI prices published in May 2024. This estimate is indicative;
/// prices may vary.
fn estimate_cost_usd(model: &str, prompt_tokens: u32, completion_tokens: u32) -> Option<f64> {
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

#[cfg(test)]
mod tests {
    use super::*;

    // GIVEN an ApiBackendConfig whose api_key_env is not set in the environment
    // WHEN resolve_api_key() is called
    // THEN Err(LlmError::ApiKeyMissing { var: "APOLLIA_TEST_KEY_ABSENT_XYZ" }) is returned
    #[test]
    fn test_resolve_api_key_missing() {
        let config = ApiBackendConfig {
            name: "openai".into(),
            api_url: "https://api.openai.com/v1".into(),
            api_key_env: "APOLLIA_TEST_KEY_ABSENT_XYZ".into(),
            model: "gpt-4o-mini".into(),
        };

        let result = config.resolve_api_key();

        assert!(
            matches!(result, Err(LlmError::ApiKeyMissing { ref var }) if var == "APOLLIA_TEST_KEY_ABSENT_XYZ"),
            "expected ApiKeyMissing for missing env var, got: {result:?}"
        );
    }

    // GIVEN a JSONDeserialize error shaped like llama-server's (integer `code`
    // where the OpenAI schema expects a string)
    // WHEN map_openai_error() maps it
    // THEN it becomes an actionable HttpError pointing at the backend log, not a
    //      cryptic "invalid type: integer" inference error
    #[test]
    fn test_map_openai_error_unparseable_error_body() {
        let serde_err = serde_json::from_str::<String>("400").unwrap_err();
        let mapped = map_openai_error(async_openai::error::OpenAIError::JSONDeserialize(serde_err));
        match mapped {
            LlmError::HttpError { body, .. } => {
                assert!(body.contains("could not parse"), "body: {body}");
                assert!(
                    body.contains("server log"),
                    "should point at the log: {body}"
                );
            }
            other => panic!("expected HttpError, got: {other:?}"),
        }
    }

    // GIVEN an ApiBackendConfig whose api_key_env is set in the environment
    // WHEN resolve_api_key() is called
    // THEN Ok("sk-test-key") is returned
    #[test]
    fn test_resolve_api_key_present() {
        // GIVEN: set env var for this test only
        // Safety: test isolation via unique key name
        std::env::set_var("APOLLIA_TEST_KEY_PRESENT_XYZ", "sk-test-key");
        let config = ApiBackendConfig {
            name: "openai".into(),
            api_url: "https://api.openai.com/v1".into(),
            api_key_env: "APOLLIA_TEST_KEY_PRESENT_XYZ".into(),
            model: "gpt-4o-mini".into(),
        };

        let result = config.resolve_api_key();

        std::env::remove_var("APOLLIA_TEST_KEY_PRESENT_XYZ");
        assert_eq!(
            result.expect("resolve_api_key must succeed when env var is set"),
            "sk-test-key"
        );
    }

    // GIVEN a TOML string representing an ApiBackendConfig
    // WHEN deserializing with toml::from_str
    // THEN the name and model fields are correct
    #[test]
    fn test_api_backend_config_serde_toml() {
        let toml_str = r#"
            name        = "openai"
            api_url     = "https://api.openai.com/v1"
            api_key_env = "OPENAI_API_KEY"
            model       = "gpt-4o-mini"
        "#;

        let config: ApiBackendConfig =
            toml::from_str(toml_str).expect("TOML deserialization must succeed");

        assert_eq!(config.name, "openai");
        assert_eq!(config.model, "gpt-4o-mini");
        assert_eq!(config.api_url, "https://api.openai.com/v1");
        assert_eq!(config.api_key_env, "OPENAI_API_KEY");
    }

    // GIVEN a known model "gpt-4o-mini" with non-zero token counts
    // WHEN estimate_cost_usd is called
    // THEN Some(value > 0.0) is returned
    #[test]
    fn test_estimate_cost_usd_nonzero_for_known_model() {
        let cost = estimate_cost_usd("gpt-4o-mini", 1000, 500);

        assert!(cost.is_some(), "cost_usd must be Some for gpt-4o-mini");
        assert!(
            cost.unwrap() > 0.0,
            "cost_usd must be positive for non-zero token counts"
        );
    }

    // GIVEN the model "gpt-4o-mini" with 100 prompt + 50 completion tokens
    // WHEN computing the cost
    // THEN it matches the expected rates (0.15$/1M prompt, 0.60$/1M completion)
    #[test]
    fn test_estimate_cost_usd_exact_value_gpt4o_mini() {
        let cost = estimate_cost_usd("gpt-4o-mini", 100, 50);
        let expected = 100.0 * GPT_4O_MINI_PROMPT_RATE + 50.0 * GPT_4O_MINI_COMPLETION_RATE;

        assert_eq!(cost, Some(expected));
    }

    // GIVEN an unknown model
    // WHEN estimate_cost_usd is called
    // THEN None is returned
    #[test]
    fn test_estimate_cost_usd_none_for_unknown_model() {
        let cost = estimate_cost_usd("mistral-7b-instruct", 1000, 500);

        assert!(
            cost.is_none(),
            "cost_usd must be None for unknown model pricing"
        );
    }

    // GIVEN an OpenAICompatibleClient built with a valid config and a fake key
    // WHEN reading is_available(), backend_name(), model_id()
    // THEN the expected values are returned
    #[test]
    fn test_client_new_is_available() {
        let config = ApiBackendConfig {
            name: "test-openai".into(),
            api_url: "https://api.openai.com/v1".into(),
            api_key_env: "OPENAI_API_KEY".into(),
            model: "gpt-4o-mini".into(),
        };
        let client = OpenAICompatibleClient::new(
            &config,
            "sk-fake-key".into(),
            tokio_util::sync::CancellationToken::new(),
        );

        assert!(client.is_available(), "is_available() must return true");
        assert_eq!(client.backend_name(), "test-openai");
        assert_eq!(client.model_id(), "gpt-4o-mini");
    }

    // GIVEN the async-openai FinishReason::Stop
    // WHEN map_finish_reason is called
    // THEN the Apollia FinishReason::Stop is returned
    #[test]
    fn test_map_finish_reason_stop() {
        assert_eq!(
            map_finish_reason(Some(&async_openai::types::FinishReason::Stop)),
            FinishReason::Stop
        );
    }

    // GIVEN the async-openai FinishReason::ToolCalls
    // WHEN map_finish_reason is called
    // THEN the Apollia FinishReason::ToolCalls is returned
    #[test]
    fn test_map_finish_reason_tool_calls() {
        assert_eq!(
            map_finish_reason(Some(&async_openai::types::FinishReason::ToolCalls)),
            FinishReason::ToolCalls
        );
    }

    // GIVEN None (FinishReason absent from the SSE chunk)
    // WHEN map_finish_reason is called
    // THEN FinishReason::Stop is returned by default
    #[test]
    fn test_map_finish_reason_none_defaults_to_stop() {
        assert_eq!(map_finish_reason(None), FinishReason::Stop);
    }
}
