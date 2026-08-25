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
        ChatCompletionStreamOptions, ChatCompletionTool, ChatCompletionToolType,
        CreateChatCompletionRequestArgs, CreateChatCompletionResponse,
        CreateChatCompletionStreamResponse, FunctionCall, FunctionObject,
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
    /// Usable context window in tokens, when it is known for this backend.
    ///
    /// The OpenAI-compatible protocol carries no way to ask, so this is either
    /// configured by the operator or resolved by the router from a
    /// provider-specific endpoint. `None` means unknown, and the router then
    /// sizes compaction from a generic fallback, which is only safe as long as
    /// nothing pretends the value was measured.
    #[serde(default)]
    pub context_window: Option<usize>,
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
        Self::with_idle_timeout(
            config,
            api_key,
            cancel,
            crate::http_client::DEFAULT_IDLE_TIMEOUT,
        )
    }

    /// Same as [`new`](Self::new) with an explicit tolerance to backend silence.
    ///
    /// Both constructors bound the connect phase and the read-idle phase; they
    /// never bound the total request, because a legitimate generation on a
    /// large or remote model runs for minutes. See [`crate::http_client`].
    pub fn with_idle_timeout(
        config: &ApiBackendConfig,
        api_key: String,
        cancel: CancellationToken,
        idle_timeout: std::time::Duration,
    ) -> Self {
        let openai_config = OpenAIConfig::new()
            .with_api_key(api_key)
            .with_api_base(config.api_url.clone());
        Self {
            client: Client::with_config(openai_config)
                .with_http_client(crate::http_client::build_llm_http_client(idle_timeout)),
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

        // `create_byot` rather than `create`: the crate's own response type drops
        // any field it does not declare, which discards the `timings` object the
        // embedded llama-server attaches. Flattening the same type inside a
        // wrapper keeps every existing behaviour and recovers that object.
        // Requires `stream` to be unset, which it is: this is the non-streaming
        // path and the builder never sets it.
        let envelope: WithExtras<CreateChatCompletionResponse> = self
            .client
            .chat()
            .create_byot(request)
            .await
            .map_err(map_openai_error)?;
        let engine_timings = envelope.timings;
        let reasoning = envelope.reasoning;
        let response = envelope.inner;

        let latency_ms = started.elapsed().as_millis() as u64;

        let choice = response
            .choices
            .into_iter()
            .next()
            .ok_or_else(|| LlmError::ParseError("no choices in response".to_owned()))?;

        let finish_reason = map_finish_reason(choice.finish_reason.as_ref());
        let content = inline_reasoning(reasoning, choice.message.content.unwrap_or_default());

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
            engine_timings,
            content,
            tool_calls,
            usage,
            finish_reason,
            latency_ms,
            ttft_ms: None,
        })
    }
}

/// A response type plus the engine timings the declared type would discard.
///
/// `serde` drops unknown fields, so the crate's own response struct silently
/// loses the `timings` object that `llama-server` attaches. Flattening that
/// struct inside this wrapper preserves its parsing exactly while capturing the
/// extra key. Backends that report nothing simply yield `None`.
#[derive(serde::Deserialize)]
struct WithTimings<T> {
    #[serde(flatten)]
    inner: T,
    #[serde(default)]
    timings: Option<serde_json::Value>,
}

/// The separate reasoning field carried by a non-streaming response.
///
/// See [`RawReasoning`] for why this is captured at all. Parsed from the same
/// body as [`WithTimings`], in a second pass over the raw bytes, because the
/// field sits inside `choices[].message` where a top-level flatten cannot reach.
#[derive(serde::Deserialize)]
struct ReasoningEnvelope {
    #[serde(default)]
    choices: Vec<ReasoningChoice>,
}

#[derive(serde::Deserialize)]
struct ReasoningChoice {
    /// Non-streaming responses carry the field under `message`, streamed ones
    /// under `delta`. One of the two is present; both are optional.
    #[serde(default)]
    message: Option<RawReasoning>,
    #[serde(default)]
    delta: Option<RawReasoning>,
}

/// Reasoning a server streams beside the content instead of inside it.
///
/// The embedded `llama-server` runs with `--reasoning-format none`, so a
/// reasoning model's thoughts stay inline in `content` as `<think>` tags, which
/// is what the chat pipeline parses. Other OpenAI-compatible servers split them
/// out instead: Ollama uses `reasoning`, vLLM and DeepSeek use
/// `reasoning_content`. A client that reads only `content` drops them entirely,
/// which on a reasoning model means the user watches an empty screen for the
/// whole thinking phase and the pipeline sees no reasoning at all.
///
/// Both spellings are accepted here and re-inlined as `<think>` tags, so every
/// backend reaches the rest of the runtime in the same shape.
#[derive(serde::Deserialize)]
struct RawReasoning {
    #[serde(default, alias = "reasoning_content")]
    reasoning: Option<String>,
}

/// Opening tag used to re-inline reasoning that arrived in a separate field.
const THINK_OPEN: &str = "<think>";
/// Closing counterpart of [`THINK_OPEN`].
const THINK_CLOSE: &str = "</think>";

/// Prefix `content` with the separate `reasoning`, wrapped in `<think>` tags.
///
/// A backend that already inlines its reasoning reports no separate field and
/// its content passes through untouched, so the two shapes converge without the
/// caller having to know which server answered.
fn inline_reasoning(reasoning: Option<String>, content: String) -> String {
    match reasoning {
        Some(r) if !r.is_empty() => format!("{THINK_OPEN}{r}{THINK_CLOSE}{content}"),
        _ => content,
    }
}

impl ReasoningEnvelope {
    /// The first choice's reasoning delta, when it is present and non-empty.
    fn first(&self) -> Option<&str> {
        self.choices
            .first()
            .and_then(|c| c.message.as_ref().or(c.delta.as_ref()))
            .and_then(|r| r.reasoning.as_deref())
            .filter(|s| !s.is_empty())
    }
}

/// A response or chunk, plus both fields the declared type would discard.
struct WithExtras<T> {
    inner: T,
    timings: Option<serde_json::Value>,
    /// Reasoning from a server that reports it beside the content.
    reasoning: Option<String>,
}

impl<'de, T> serde::Deserialize<'de> for WithExtras<T>
where
    T: serde::de::DeserializeOwned,
{
    /// Parses the body twice from one buffered value.
    ///
    /// A single derive cannot do this: `#[serde(flatten)]` reaches top-level
    /// keys only, and the reasoning field sits two levels down inside `choices`,
    /// which the flattened type already owns. Buffering into a `Value` and
    /// reading it twice keeps the crate's own parsing byte-for-byte identical
    /// rather than re-declaring the response type here, where it would drift on
    /// every upstream change.
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = serde_json::Value::deserialize(deserializer)?;
        // A body that carries no reasoning must still parse: this probe is
        // best-effort by construction and never turns a good response into an
        // error.
        let reasoning = serde_json::from_value::<ReasoningEnvelope>(value.clone())
            .ok()
            .and_then(|e| e.first().map(str::to_owned));
        let envelope: WithTimings<T> =
            serde_json::from_value(value).map_err(serde::de::Error::custom)?;
        Ok(Self {
            inner: envelope.inner,
            timings: envelope.timings,
            reasoning,
        })
    }
}

/// One streamed chunk, plus the engine timings and reasoning riding on it.
type TimedStreamResponse = WithExtras<CreateChatCompletionStreamResponse>;

/// The byot equivalent of `ChatCompletionResponseStream`.
type TimedChatStream = Pin<
    Box<dyn Stream<Item = Result<TimedStreamResponse, async_openai::error::OpenAIError>> + Send>,
>;

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
        // Ask the server for a terminal usage chunk so token accounting is not
        // lost on the streaming path. OpenAI-compatible servers (including the
        // embedded llama-server) append a final chunk with empty `choices` and a
        // populated `usage`; it is surfaced downstream as `StreamChunk::Usage`.
        builder.stream_options(ChatCompletionStreamOptions {
            include_usage: true,
        });

        let mut request = builder
            .build()
            .map_err(|e| LlmError::InferenceError(format!("build stream request: {e}")))?;
        // `create_stream` sets this itself; `create_stream_byot` does not, and a
        // request without it would come back as a single non-streamed body.
        request.stream = Some(true);

        // See `do_complete`: byot recovers the `timings` object that the crate's
        // declared chunk type would drop. The engine attaches it to the final
        // chunk whether or not per-token timings were requested, so nothing else
        // about the request changes.
        let sse_stream = match self
            .client
            .chat()
            .create_stream_byot::<_, TimedStreamResponse>(request)
            .await
        {
            Ok(stream) => stream,
            Err(async_openai::error::OpenAIError::StreamError(detail)) => {
                // async-openai reports a non-2xx SSE setup status as an opaque
                // `StreamError` ("Invalid status code: ...") and drops the
                // response body, hiding the real reason (unknown model, rejected
                // parameter, oversized prompt). Re-issue the same request without
                // streaming to recover the API error body and surface it.
                return Err(match self.do_complete(req).await {
                    Err(recovered) => recovered,
                    Ok(_) => LlmError::HttpError {
                        status: 0,
                        body: format!(
                            "backend accepted a non-streaming request but rejected \
                             streaming ({detail}); it may not support SSE streaming"
                        ),
                    },
                });
            }
            Err(other) => return Err(map_openai_error(other)),
        };

        // async-openai defers the HTTP status check to the first poll, so a
        // non-2xx (e.g. a 400 for an unknown model or a rejected parameter)
        // surfaces as the first stream item rather than a setup error, with the
        // response body dropped. Peek the first item: on that opaque
        // StreamError, recover the real reason via a non-streaming request.
        let mut sse_stream = sse_stream;
        let first = sse_stream.next().await;
        if let Some(Err(async_openai::error::OpenAIError::StreamError(detail))) = &first {
            let detail = detail.clone();
            return Err(match self.do_complete(req).await {
                Err(recovered) => recovered,
                Ok(_) => LlmError::HttpError {
                    status: 0,
                    body: format!(
                        "backend accepted a non-streaming request but rejected \
                         streaming ({detail}); it may not support SSE streaming"
                    ),
                },
            });
        }

        // OpenAI streams tool calls as fragments across multiple SSE chunks,
        // keyed by `index`.  Text tokens are emitted immediately.  Tool call
        // fragments are accumulated and flushed when the SSE stream ends.
        //
        // State transitions: Streaming → Flushing → Done.
        let state = OpenAIStreamState::Streaming {
            inner: sse_stream,
            first: first.map(Box::new),
            pending: HashMap::new(),
            model,
            pending_timings: None,
            in_think: false,
        };

        let mapped = futures::stream::unfold(state, next_openai_stream_item);

        Ok(Box::pin(mapped))
    }

    /// Return `true`: the client is configured and ready to send requests.
    fn is_available(&self) -> bool {
        true
    }

    /// Report the configured context window, when the operator or the router
    /// established one.
    ///
    /// Left `None` otherwise rather than guessed: a self-hosted server sizes its
    /// window from its own configuration and the machine it runs on, and an
    /// invented number would have the router compact against a window no server
    /// ever had.
    fn context_window(&self) -> Option<usize> {
        self.config.context_window
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
///
/// Tool `parameters` are normalized by [`crate::schema_sanitize::grammar_safe_schema`]
/// so a llama.cpp-backed server can build a valid tool-calling grammar from
/// them; see that module for the constructs it neutralizes.
fn build_tools(tools: &[crate::types::ToolSpec]) -> Vec<ChatCompletionTool> {
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
fn status_from_unparseable_body(detail: &str) -> Option<u16> {
    let after = detail.split_once("invalid type: integer `")?.1;
    after.split_once('`')?.0.parse().ok()
}

/// State machine for the OpenAI streaming response.
///
/// Tool call fragments are accumulated during `Streaming` and flushed as
/// `StreamChunk::ToolCall` items during `Flushing`.
enum OpenAIStreamState {
    /// Reading SSE chunks from the inner stream.
    Streaming {
        inner: TimedChatStream,
        /// First item peeked in `stream()` to surface a lazy non-2xx status;
        /// drained before polling `inner`. Boxed to keep the variant small.
        first: Option<Box<Result<TimedStreamResponse, async_openai::error::OpenAIError>>>,
        pending: HashMap<u32, PartialToolCall>,
        /// Resolved model id, kept to price the terminal usage chunk the same
        /// way the non-streaming `do_complete` path does.
        model: String,
        /// Engine timings seen on a chunk that also carried something else to
        /// emit. Held back so they surface on the following poll, since the
        /// state machine yields one item at a time.
        pending_timings: Option<serde_json::Value>,
        /// Whether a `<think>` block opened by a separate reasoning field is
        /// still open. See [`RawReasoning`].
        in_think: bool,
    },
    /// SSE ended inside a `<think>` block: the closing tag has been emitted and
    /// the terminal items still owe an appearance. A distinct state because the
    /// inner stream is exhausted and must not be polled again.
    Ending {
        timings: Option<serde_json::Value>,
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
        OpenAIStreamState::Ending { timings, pending } => match timings {
            Some(t) => Some((
                Ok(StreamChunk::Timings(t)),
                OpenAIStreamState::Flushing {
                    remaining: drain_pending_tool_calls(pending),
                },
            )),
            None => flush_pending_tool_calls(pending),
        },
        OpenAIStreamState::Streaming {
            mut inner,
            mut first,
            mut pending,
            model,
            mut pending_timings,
            mut in_think,
        } => {
            // Timings held back from an earlier chunk take priority, so they are
            // never lost behind a later text delta.
            if let Some(timings) = pending_timings.take() {
                return Some((
                    Ok(StreamChunk::Timings(timings)),
                    OpenAIStreamState::Streaming {
                        inner,
                        first,
                        pending,
                        model,
                        pending_timings: None,
                        in_think,
                    },
                ));
            }
            loop {
                let next = match first.take() {
                    Some(item) => Some(*item),
                    None => inner.next().await,
                };
                match next {
                    Some(Ok(envelope)) => {
                        // The engine attaches timings to its final chunk. Hold
                        // them until whatever else this chunk carries has been
                        // emitted.
                        if let Some(t) = envelope.timings {
                            pending_timings = Some(t);
                        }
                        let reasoning = envelope.reasoning;
                        let response = envelope.inner;
                        // The terminal chunk (empty `choices`, populated `usage`)
                        // carries the call's token accounting. Surface it as
                        // `StreamChunk::Usage`, priced like the non-streaming path.
                        if let Some(usage) = response.usage.as_ref() {
                            let mapped = TokenUsage {
                                prompt_tokens: usage.prompt_tokens,
                                completion_tokens: usage.completion_tokens,
                                cost_usd: estimate_cost_usd(
                                    &model,
                                    usage.prompt_tokens,
                                    usage.completion_tokens,
                                ),
                                ..Default::default()
                            };
                            return Some((
                                Ok(StreamChunk::Usage(mapped)),
                                OpenAIStreamState::Streaming {
                                    inner,
                                    first,
                                    pending,
                                    model,
                                    pending_timings,
                                    in_think,
                                },
                            ));
                        }
                        if let Some(text) =
                            next_emittable_text(&mut pending, response, reasoning, &mut in_think)
                        {
                            return Some((
                                Ok(StreamChunk::Text(text)),
                                OpenAIStreamState::Streaming {
                                    inner,
                                    first,
                                    pending,
                                    model,
                                    pending_timings,
                                    in_think,
                                },
                            ));
                        }
                        continue;
                    }
                    Some(Err(e)) => {
                        return Some((Err(map_openai_error(e)), OpenAIStreamState::Done));
                    }
                    None => {
                        // A stream that ends while a reasoning block is open
                        // (the model went straight from thinking to a tool call,
                        // or was cut off) would leave an unbalanced `<think>`
                        // that the downstream parser reads as reasoning
                        // swallowing the rest of the turn.
                        if in_think {
                            return Some((
                                Ok(StreamChunk::Text(THINK_CLOSE.to_owned())),
                                OpenAIStreamState::Ending {
                                    timings: pending_timings.take(),
                                    pending,
                                },
                            ));
                        }
                        // SSE ended. Timings from the final chunk outrank the
                        // tool-call flush: dropping them here would lose the
                        // measurement on every turn that ends in a tool call.
                        if let Some(timings) = pending_timings.take() {
                            return Some((
                                Ok(StreamChunk::Timings(timings)),
                                OpenAIStreamState::Flushing {
                                    remaining: drain_pending_tool_calls(pending),
                                },
                            ));
                        }
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
///
/// A `reasoning` delta is re-inlined as a `<think>` block so a backend that
/// splits reasoning out reaches the pipeline in the same shape as one that
/// keeps it in the content stream. `in_think` carries the open/closed state
/// across chunks, since the tags straddle them.
fn next_emittable_text(
    pending: &mut HashMap<u32, PartialToolCall>,
    response: async_openai::types::CreateChatCompletionStreamResponse,
    reasoning: Option<String>,
    in_think: &mut bool,
) -> Option<String> {
    let choice = response.choices.into_iter().next()?;

    if let Some(tc_chunks) = choice.delta.tool_calls {
        accumulate_tool_calls(pending, tc_chunks);
    }

    let text = choice.delta.content.unwrap_or_default();
    let reasoning = reasoning.unwrap_or_default();
    if text.is_empty() && reasoning.is_empty() {
        return None;
    }

    let mut out = String::with_capacity(text.len() + reasoning.len());
    if !reasoning.is_empty() {
        if !*in_think {
            out.push_str(THINK_OPEN);
            *in_think = true;
        }
        out.push_str(&reasoning);
    }
    if !text.is_empty() {
        if *in_think {
            out.push_str(THINK_CLOSE);
            *in_think = false;
        }
        out.push_str(&text);
    }
    Some(out)
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
    let mut tool_calls = drain_pending_tool_calls(pending);
    tool_calls.pop().map(|call| {
        (
            Ok(StreamChunk::ToolCall(call)),
            OpenAIStreamState::Flushing {
                remaining: tool_calls,
            },
        )
    })
}

/// Sort the accumulated calls by index and reverse them, so `Flushing` can pop
/// from the end and still emit in index order.
fn drain_pending_tool_calls(pending: HashMap<u32, PartialToolCall>) -> Vec<ToolCall> {
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

    tool_calls.reverse();
    tool_calls
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
            context_window: None,
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
                assert!(body.contains("HTTP 400"), "body: {body}");
                assert!(
                    body.contains("server log"),
                    "should point at the log: {body}"
                );
            }
            other => panic!("expected HttpError, got: {other:?}"),
        }
    }

    // GIVEN the same unparseable-body failure, but carrying a 404
    // WHEN map_openai_error() maps it
    // THEN the message points at the base URL instead of blaming the context
    //      size, which is the wrong thing to go debug when the route is simply
    //      missing the provider's `/v1` prefix
    #[test]
    fn test_map_openai_error_404_points_at_the_base_url() {
        let serde_err = serde_json::from_str::<String>("404").unwrap_err();
        let mapped = map_openai_error(async_openai::error::OpenAIError::JSONDeserialize(serde_err));
        match mapped {
            LlmError::HttpError { body, .. } => {
                assert!(body.contains("base URL"), "body: {body}");
                assert!(body.contains("/v1"), "should name the prefix: {body}");
                assert!(
                    !body.contains("context size"),
                    "must not blame the context size on a 404: {body}"
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
        // GIVEN: the env var set, serialised and restored by the shared lock
        crate::backends::test_env::with_env_var(
            "APOLLIA_TEST_KEY_PRESENT_XYZ",
            "sk-test-key",
            || {
                let config = ApiBackendConfig {
                    name: "openai".into(),
                    api_url: "https://api.openai.com/v1".into(),
                    api_key_env: "APOLLIA_TEST_KEY_PRESENT_XYZ".into(),
                    model: "gpt-4o-mini".into(),
                    context_window: None,
                };

                let result = config.resolve_api_key();

                assert_eq!(
                    result.expect("resolve_api_key must succeed when env var is set"),
                    "sk-test-key"
                );
            },
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
            context_window: None,
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

    // ── HTTP round-trip against a mock OpenAI-compatible server ──────────────
    //
    // Everything above tests pure mapping helpers, so nothing here proved which
    // URL the client actually calls. That gap let a wrong Ollama base URL ship:
    // the CLI wrote `http://localhost:11434` with no `/v1`, and since the client
    // appends `/chat/completions` to the base, every completion 404d. These
    // tests pin the request path and the tool-call parsing to a real server.

    use crate::types::ChatMessage;
    use wiremock::matchers::{header_exists, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    /// Config pointing at a mock server, with the base URL passed verbatim.
    fn config_for(api_url: &str) -> ApiBackendConfig {
        ApiBackendConfig {
            name: "mock".to_string(),
            api_url: api_url.to_string(),
            api_key_env: "UNUSED_IN_TESTS".to_string(),
            model: "test-model".to_string(),
            context_window: None,
        }
    }

    fn user_request() -> CompletionRequest {
        CompletionRequest {
            messages: vec![ChatMessage::user("bonjour")],
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn test_complete_posts_to_chat_completions_under_the_configured_base() {
        // GIVEN a server that only answers on `/v1/chat/completions`, which is
        // what an Ollama-style base URL ending in `/v1` must produce
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .and(header_exists("authorization"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": "chatcmpl-1",
                "object": "chat.completion",
                "created": 1,
                "model": "test-model",
                "choices": [{
                    "index": 0,
                    "message": { "role": "assistant", "content": "salut" },
                    "finish_reason": "stop"
                }],
                "usage": { "prompt_tokens": 3, "completion_tokens": 2, "total_tokens": 5 }
            })))
            .expect(1)
            .mount(&server)
            .await;

        let client = OpenAICompatibleClient::new(
            &config_for(&format!("{}/v1", server.uri())),
            "test-key".to_string(),
            CancellationToken::new(),
        );

        // WHEN a completion is requested
        let response = client.complete(user_request()).await.expect("completion");

        // THEN the call landed on the expected path (asserted by the mock on
        // drop) and the body was parsed
        assert_eq!(response.content, "salut");
        assert_eq!(response.usage.prompt_tokens, 3);
    }

    #[tokio::test]
    async fn test_base_url_without_v1_does_not_reach_the_v1_route() {
        // GIVEN the same server, still only serving `/v1/chat/completions`
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200))
            .expect(0)
            .mount(&server)
            .await;

        // WHEN the base URL omits `/v1`, which is exactly the shape the CLI
        // used to persist for Ollama
        let client = OpenAICompatibleClient::new(
            &config_for(&server.uri()),
            "test-key".to_string(),
            CancellationToken::new(),
        );
        let result = client.complete(user_request()).await;

        // THEN the request never reaches the real route and the call fails,
        // instead of silently succeeding
        assert!(
            result.is_err(),
            "a base URL without /v1 must not resolve, got {result:?}"
        );
    }

    #[tokio::test]
    async fn test_complete_parses_a_tool_call() {
        // GIVEN a server answering with a tool call rather than text
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": "chatcmpl-2",
                "object": "chat.completion",
                "created": 1,
                "model": "test-model",
                "choices": [{
                    "index": 0,
                    "message": {
                        "role": "assistant",
                        "content": null,
                        "tool_calls": [{
                            "id": "call_abc123",
                            "type": "function",
                            "function": {
                                "name": "file_read",
                                "arguments": "{\"path\":\"/tmp/x\"}"
                            }
                        }]
                    },
                    "finish_reason": "tool_calls"
                }],
                "usage": { "prompt_tokens": 8, "completion_tokens": 4, "total_tokens": 12 }
            })))
            .mount(&server)
            .await;

        let client = OpenAICompatibleClient::new(
            &config_for(&format!("{}/v1", server.uri())),
            "test-key".to_string(),
            CancellationToken::new(),
        );

        // WHEN the completion runs
        let response = client.complete(user_request()).await.expect("completion");

        // THEN the tool call is surfaced with its id, name and parsed arguments,
        // which is what the ReAct loop correlates approvals on
        assert_eq!(response.finish_reason, FinishReason::ToolCalls);
        assert_eq!(response.tool_calls.len(), 1);
        let call = &response.tool_calls[0];
        assert_eq!(call.id, "call_abc123");
        assert_eq!(call.name, "file_read");
        assert_eq!(call.arguments["path"], "/tmp/x");
    }

    // ── Reasoning reported beside the content ───────────────────────────────

    fn stream_chunk(delta: serde_json::Value) -> TimedStreamResponse {
        serde_json::from_value(serde_json::json!({
            "id": "c1",
            "object": "chat.completion.chunk",
            "created": 0,
            "model": "m",
            "choices": [{"index": 0, "delta": delta, "finish_reason": null}],
        }))
        .expect("chunk must parse")
    }

    fn emit(chunk: TimedStreamResponse, in_think: &mut bool) -> Option<String> {
        let mut pending = HashMap::new();
        next_emittable_text(&mut pending, chunk.inner, chunk.reasoning, in_think)
    }

    // GIVEN a streamed chunk shaped like Ollama's, with the reasoning in its own
    // field and an empty content
    // WHEN it is deserialized
    // THEN the reasoning is recovered rather than dropped with the unknown keys
    #[test]
    fn test_stream_chunk_recovers_separate_reasoning_field() {
        let chunk = stream_chunk(serde_json::json!({"content": "", "reasoning": "Here"}));

        assert_eq!(chunk.reasoning.as_deref(), Some("Here"));
    }

    // GIVEN the other spelling of the same field, used by vLLM and DeepSeek
    // WHEN it is deserialized
    // THEN it is recovered too, so one client covers both conventions
    #[test]
    fn test_stream_chunk_recovers_reasoning_content_alias() {
        let chunk = stream_chunk(serde_json::json!({"reasoning_content": "step"}));

        assert_eq!(chunk.reasoning.as_deref(), Some("step"));
    }

    // GIVEN a chunk from a server that inlines its reasoning (the embedded
    // llama-server with --reasoning-format none)
    // WHEN it is deserialized
    // THEN no separate reasoning is reported and the content is untouched, so
    // the inlining below never fires and cannot double-wrap
    #[test]
    fn test_stream_chunk_without_reasoning_field_reports_none() {
        let chunk = stream_chunk(serde_json::json!({"content": "<think>a</think>b"}));

        assert!(chunk.reasoning.is_none());
        assert_eq!(
            chunk.inner.choices[0].delta.content.as_deref(),
            Some("<think>a</think>b")
        );
    }

    // GIVEN a reasoning stream followed by the visible answer
    // WHEN the chunks are emitted in order
    // THEN the block is opened once, kept open across chunks, and closed exactly
    // when the first content arrives
    #[test]
    fn test_separate_reasoning_is_reinlined_as_one_think_block() {
        let mut in_think = false;

        let first = emit(
            stream_chunk(serde_json::json!({"content": "", "reasoning": "Here"})),
            &mut in_think,
        );
        let second = emit(
            stream_chunk(serde_json::json!({"content": "", "reasoning": "'s why"})),
            &mut in_think,
        );
        let third = emit(
            stream_chunk(serde_json::json!({"content": "Answer"})),
            &mut in_think,
        );

        assert_eq!(first.as_deref(), Some("<think>Here"));
        assert_eq!(second.as_deref(), Some("'s why"));
        assert_eq!(third.as_deref(), Some("</think>Answer"));
        assert!(!in_think, "the block must be closed once content started");
    }

    // GIVEN a chunk carrying reasoning and content at once
    // WHEN it is emitted
    // THEN the block opens and closes within that single chunk
    #[test]
    fn test_reasoning_and_content_in_one_chunk_close_immediately() {
        let mut in_think = false;

        let out = emit(
            stream_chunk(serde_json::json!({"content": "B", "reasoning": "A"})),
            &mut in_think,
        );

        assert_eq!(out.as_deref(), Some("<think>A</think>B"));
        assert!(!in_think);
    }

    // GIVEN a chunk with neither content nor reasoning (an SSE heartbeat)
    // WHEN it is emitted
    // THEN nothing is produced and no block is opened
    #[test]
    fn test_empty_chunk_emits_nothing() {
        let mut in_think = false;

        let out = emit(
            stream_chunk(serde_json::json!({"content": ""})),
            &mut in_think,
        );

        assert!(out.is_none());
        assert!(!in_think);
    }

    // GIVEN a non-streaming response whose reasoning came in its own field
    // WHEN the content is assembled
    // THEN the reasoning precedes it inside a think block, the shape the chat
    // pipeline already parses
    #[test]
    fn test_inline_reasoning_prefixes_content() {
        let out = inline_reasoning(Some("why".to_owned()), "answer".to_owned());

        assert_eq!(out, "<think>why</think>answer");
    }

    // GIVEN a response with no separate reasoning
    // WHEN the content is assembled
    // THEN it passes through byte for byte
    #[test]
    fn test_inline_reasoning_passthrough_without_reasoning() {
        assert_eq!(inline_reasoning(None, "answer".to_owned()), "answer");
        assert_eq!(
            inline_reasoning(Some(String::new()), "answer".to_owned()),
            "answer"
        );
    }
}
