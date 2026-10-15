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
        ChatCompletionStreamOptions, CreateChatCompletionRequestArgs, CreateChatCompletionResponse,
        CreateChatCompletionStreamResponse,
    },
    Client,
};
use futures::{Stream, StreamExt};

mod convert;
mod reasoning;
mod stream;

use convert::{
    build_messages, build_tools, estimate_cost_usd, map_finish_reason, map_openai_error,
    with_sampling_extensions, with_structured_output,
};
use reasoning::{constrained_content, inline_reasoning, ReasoningEnvelope, WithTimings};
use stream::{next_openai_stream_item, OpenAIStreamState};

use crate::types::{
    CompletionModel, CompletionRequest, CompletionResponse, LlmError, StreamChunk, TokenUsage,
    ToolCall,
};

// ── Price table (per token, in USD) ──────────────────────────────────────

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
    /// Whether this endpoint is a llama.cpp server, which accepts the `grammar`
    /// field its own protocol adds on top of the OpenAI one.
    ///
    /// Set by the embedded `llama-server` backend and by nothing else: sending
    /// an undeclared field to a real OpenAI endpoint is a 400. It decides which
    /// structured-output transport [`with_structured_output`] picks.
    ///
    /// [`with_structured_output`]: super::openai::convert::with_structured_output
    #[serde(default)]
    pub llama_cpp_extensions: bool,
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

/// Narrow a seed onto the signed range the OpenAI protocol declares.
///
/// The wire field is a signed integer and [`CompletionRequest::seed`] is
/// unsigned, so a seed above `i64::MAX` has no wire spelling. Keeping the low 63
/// bits is total and injective on everything below that bound, which is every
/// seed a caller writes by hand, and it never maps two usable seeds onto one.
/// Clamping would have done exactly that.
fn narrow_seed(seed: u64) -> i64 {
    (seed & (i64::MAX as u64)) as i64
}

/// Build the request common to the completion and the streaming path.
///
/// Both carry the same messages, tools and sampling settings; only the stream
/// options and the structured-output transport differ, and those are applied by
/// the caller. Shared so a sampling field cannot be honoured on one path and
/// dropped on the other, which is how `seed` came to reach neither.
fn sampling_builder(
    req: &CompletionRequest,
    model: &str,
) -> Result<CreateChatCompletionRequestArgs, LlmError> {
    let messages = build_messages(&req.messages)?;
    let tools = build_tools(&req.tools);

    let mut builder = CreateChatCompletionRequestArgs::default();
    builder.model(model).messages(messages);

    if !tools.is_empty() {
        builder.tools(tools);
    }
    if let Some(temp) = req.temperature {
        builder.temperature(temp);
    }
    if let Some(max_tokens) = req.max_tokens {
        builder.max_tokens(max_tokens);
    }
    if let Some(seed) = req.seed {
        builder.seed(narrow_seed(seed));
    }
    if let Some(top_p) = req.top_p {
        builder.top_p(top_p);
    }
    if let Some(penalty) = req.presence_penalty {
        builder.presence_penalty(penalty);
    }
    if let Some(penalty) = req.frequency_penalty {
        builder.frequency_penalty(penalty);
    }
    if let Some(stop) = req.stop.as_ref().filter(|stop| !stop.is_empty()) {
        builder.stop(stop.clone());
    }
    Ok(builder)
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
    /// Fields added to every request body, for a setting of the server the
    /// OpenAI protocol does not carry. Empty for every backend but a declared
    /// server that was given some.
    extra_params: serde_json::Map<String, serde_json::Value>,
}

/// Add `extra` to a request body, leaving alone every field the body already
/// carries: what the request fixed, per agent or per preset, wins over what
/// the server was declared with.
fn with_extra_params(
    mut body: serde_json::Value,
    extra: &serde_json::Map<String, serde_json::Value>,
) -> serde_json::Value {
    if let Some(map) = body.as_object_mut() {
        for (field, value) in extra {
            if !map.contains_key(field) {
                map.insert(field.clone(), value.clone());
            }
        }
    }
    body
}

/// How long a server error is retried before it is reported.
///
/// The client retries every 5xx, and its default policy keeps at it for
/// fifteen minutes: an engine that refuses a request for its shape (a chat
/// template rejecting the conversation answers 500 every time) then held a
/// batch item that long. One minute covers an engine that is briefly busy.
/// See `docs/site/docs/architecture/08-decisions.md#llm-retry`.
pub(crate) const SERVER_ERROR_RETRY_BUDGET: std::time::Duration =
    std::time::Duration::from_secs(60);

/// The retry policy handed to the HTTP client, bounded by
/// [`SERVER_ERROR_RETRY_BUDGET`].
fn server_error_backoff() -> backoff::ExponentialBackoff {
    backoff::ExponentialBackoff {
        max_elapsed_time: Some(SERVER_ERROR_RETRY_BUDGET),
        ..backoff::ExponentialBackoff::default()
    }
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
                .with_http_client(crate::http_client::build_llm_http_client(
                    idle_timeout,
                    &config.api_url,
                ))
                .with_backoff(server_error_backoff()),
            config: config.clone(),
            retry_policy: RetryPolicy::default(),
            cancel,
            extra_params: serde_json::Map::new(),
        }
    }

    /// Add `extra_params` to every request this client sends. A field a
    /// request already carries is left as the request set it.
    #[must_use]
    pub fn with_extra_params(
        mut self,
        extra_params: serde_json::Map<String, serde_json::Value>,
    ) -> Self {
        self.extra_params = extra_params;
        self
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

        let request = sampling_builder(&req, &model)?
            .build()
            .map_err(|e| LlmError::InferenceError(format!("build request: {e}")))?;
        // A structured-output constraint is applied here, on the serialized
        // body, because the grammar half has no place in the typed builder.
        let request = with_extra_params(
            with_sampling_extensions(
                with_structured_output(request, &req, self.config.llama_cpp_extensions)?,
                &req,
            ),
            &self.extra_params,
        );

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
        // A constrained call reads the two channels as one: see
        // `constrained_content` for what a reasoning model does with a grammar.
        let raw_content = choice.message.content.unwrap_or_default();
        let content = if req.response_schema.is_some() || req.grammar.is_some() {
            constrained_content(reasoning, raw_content)
        } else {
            inline_reasoning(reasoning, raw_content)
        };

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
                        detail = "the arguments read as null",
                        "llm.tool_call.arguments.parse.failed"
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

/// A response or chunk, plus both fields the declared type would discard.
pub(crate) struct WithExtras<T> {
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
pub(crate) type TimedStreamResponse = WithExtras<CreateChatCompletionStreamResponse>;

/// The byot equivalent of `ChatCompletionResponseStream`.
pub(crate) type TimedChatStream = Pin<
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

        let mut builder = sampling_builder(&req, &model)?;
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
        let request = with_extra_params(
            with_sampling_extensions(
                with_structured_output(request, &req, self.config.llama_cpp_extensions)?,
                &req,
            ),
            &self.extra_params,
        );

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
            finish: None,
        };

        let mapped = futures::stream::unfold(state, next_openai_stream_item);

        Ok(Box::pin(mapped))
    }

    /// Return `true`: the client is configured and ready to send requests.
    fn is_available(&self) -> bool {
        true
    }

    /// Return `true`: `seed` is a field of the OpenAI chat protocol, and it is
    /// written into every request that carries one. The embedded llama-server
    /// reads it too, which is what makes a local call replayable.
    fn carries_seed(&self) -> bool {
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

#[cfg(test)]
mod tests {
    use super::convert::{GPT_4O_MINI_COMPLETION_RATE, GPT_4O_MINI_PROMPT_RATE};
    use super::stream::next_emittable_text;
    use super::*;

    #[test]
    fn test_a_server_error_is_retried_for_one_minute_at_most() {
        // GIVEN the retry policy the client is built with
        let policy = server_error_backoff();

        // WHEN its total budget is read
        let budget = policy.max_elapsed_time;

        // THEN it is bounded, and to one minute rather than the client's
        // default fifteen
        assert_eq!(budget, Some(std::time::Duration::from_secs(60)));
    }
    use crate::types::FinishReason;

    /// Build the body both transport paths send, from a bare request.
    fn built_body(req: &CompletionRequest) -> serde_json::Value {
        let request = sampling_builder(req, "test-model")
            .expect("a buildable request")
            .build()
            .expect("a buildable request");
        serde_json::to_value(request).expect("a serializable request")
    }

    /// Build the body the client sends, structured-output transport and
    /// sampling extensions included, for a plain or a llama-server backend.
    fn sent_body(req: &CompletionRequest, llama_cpp_extensions: bool) -> serde_json::Value {
        let request = sampling_builder(req, "test-model")
            .expect("a buildable request")
            .build()
            .expect("a buildable request");
        with_sampling_extensions(
            with_structured_output(request, req, llama_cpp_extensions)
                .expect("a serializable request"),
            req,
        )
    }

    const SAMPLING_FIELDS: [&str; 7] = [
        "top_p",
        "top_k",
        "min_p",
        "presence_penalty",
        "frequency_penalty",
        "repeat_penalty",
        "stop",
    ];

    fn fully_tuned_request() -> CompletionRequest {
        CompletionRequest {
            messages: vec![crate::types::ChatMessage::user("hello")],
            top_p: Some(0.5),
            top_k: Some(40),
            min_p: Some(0.25),
            presence_penalty: Some(0.75),
            frequency_penalty: Some(-0.5),
            repeat_penalty: Some(1.125),
            stop: Some(vec!["</answer>".to_string(), "\n\n".to_string()]),
            ..Default::default()
        }
    }

    #[test]
    fn an_untuned_request_sends_no_sampling_field() {
        // GIVEN a call that fixes none of the request sampling settings
        let req = CompletionRequest {
            messages: vec![crate::types::ChatMessage::user("hello")],
            ..Default::default()
        };

        for llama in [false, true] {
            // WHEN the body is built, for a plain server and for llama-server
            let body = sent_body(&req, llama);

            // THEN none of the seven fields is on the wire, so the engine
            // keeps its own defaults
            for field in SAMPLING_FIELDS {
                assert!(
                    body.get(field).is_none(),
                    "{field} sent unset (llama={llama}): {body}"
                );
            }
        }
    }

    #[test]
    fn every_fixed_sampling_field_reaches_an_openai_compatible_server() {
        // GIVEN a call that fixes all seven request sampling settings
        let req = fully_tuned_request();

        // WHEN the body is built for a plain OpenAI-compatible server
        let body = sent_body(&req, false);

        // THEN each one is present with the value the caller gave
        assert_eq!(body["top_p"], serde_json::json!(0.5));
        assert_eq!(body["top_k"], serde_json::json!(40));
        assert_eq!(body["min_p"], serde_json::json!(0.25));
        assert_eq!(body["presence_penalty"], serde_json::json!(0.75));
        assert_eq!(body["frequency_penalty"], serde_json::json!(-0.5));
        assert_eq!(body["repeat_penalty"], serde_json::json!(1.125));
        assert_eq!(body["stop"], serde_json::json!(["</answer>", "\n\n"]));
    }

    #[test]
    fn llama_server_reads_its_own_sampling_names() {
        // GIVEN a call fixing the three settings llama-server names itself,
        // alongside a structured-output schema
        let mut req = fully_tuned_request();
        req.response_schema = Some(serde_json::json!({
            "type": "object",
            "properties": {"title": {"type": "string"}},
            "required": ["title"]
        }));

        // WHEN the body is built for the embedded llama-server
        let body = sent_body(&req, true);

        // THEN they travel as `min_p`, `repeat_penalty` and `top_k`, the names
        // of its completion API, and the schema transport is unchanged
        assert_eq!(body["min_p"], serde_json::json!(0.25));
        assert_eq!(body["repeat_penalty"], serde_json::json!(1.125));
        assert_eq!(body["top_k"], serde_json::json!(40));
        assert!(body
            .get("response_format")
            .is_some_and(serde_json::Value::is_object));
        assert!(body.get("grammar").is_none());
    }

    #[test]
    fn an_empty_stop_list_sends_no_stop_field() {
        // GIVEN a call whose stop list is present but empty
        let req = CompletionRequest {
            messages: vec![crate::types::ChatMessage::user("hello")],
            stop: Some(Vec::new()),
            ..Default::default()
        };

        // WHEN the body is built
        let body = sent_body(&req, false);

        // THEN no stop field is sent, since the protocol wants at least one
        assert!(body.get("stop").is_none());
    }

    #[test]
    fn a_requested_seed_reaches_the_request_body() {
        // GIVEN a call asking for a reproducible answer, the way
        // `ctx.llm.complete(seed=1)` does
        let req = CompletionRequest {
            messages: vec![crate::types::ChatMessage::user("hello")],
            seed: Some(1),
            temperature: Some(0.0),
            ..Default::default()
        };

        // WHEN the request is built
        let body = built_body(&req);

        // THEN the seed is a field of what the server receives, where it used
        // to be read off the request and never written
        assert_eq!(body["seed"], serde_json::json!(1));
        assert_eq!(body["temperature"], serde_json::json!(0.0));
    }

    #[test]
    fn a_call_without_a_seed_sends_no_seed_field() {
        // GIVEN an ordinary call that asks for nothing reproducible
        let req = CompletionRequest {
            messages: vec![crate::types::ChatMessage::user("hello")],
            ..Default::default()
        };

        // WHEN the request is built
        let body = built_body(&req);

        // THEN the field is absent rather than sent as a null, which some
        // OpenAI-compatible servers refuse with a 400
        assert!(body.get("seed").is_none_or(serde_json::Value::is_null));
    }

    #[test]
    fn a_seed_above_the_signed_range_stays_distinct_from_its_neighbours() {
        // GIVEN two seeds past `i64::MAX`, which the wire field cannot spell
        // WHEN they are narrowed
        let a = narrow_seed(u64::MAX);
        let b = narrow_seed(u64::MAX - 1);

        // THEN both are representable and still tell each other apart, where
        // clamping would have collapsed them onto one value
        assert_ne!(a, b);
        assert!(a >= 0 && b >= 0);
        // AND a seed inside the range is untouched
        assert_eq!(narrow_seed(42), 42);
    }

    #[test]
    fn the_backend_declares_that_it_carries_a_seed() {
        // GIVEN the OpenAI-compatible client, which every remote provider and
        // the embedded llama-server are reached through
        let config = ApiBackendConfig {
            name: "probe".into(),
            api_url: "http://127.0.0.1:1/v1".into(),
            api_key_env: String::new(),
            model: "test-model".into(),
            context_window: None,
            llama_cpp_extensions: false,
        };
        let client =
            OpenAICompatibleClient::new(&config, "sk-no-key".to_owned(), CancellationToken::new());

        // WHEN its capabilities are read
        // THEN it says a seed reaches the server, so `GET /llm/status` can tell
        // a caller which backends a seeded call means anything on
        assert!(client.carries_seed());
    }

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
            llama_cpp_extensions: false,
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
                    llama_cpp_extensions: false,
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
            llama_cpp_extensions: false,
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
            llama_cpp_extensions: false,
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
        let mut finish = None;
        next_emittable_text(
            &mut pending,
            chunk.inner,
            chunk.reasoning,
            super::stream::StreamFlags {
                in_think,
                finish: &mut finish,
            },
        )
    }

    // GIVEN the last chunk of a streamed answer, cut by the token limit
    // WHEN it is read
    // THEN its finish reason is kept for the stream's terminal chunk
    #[test]
    fn test_stream_chunk_finish_reason_is_kept() {
        let chunk: TimedStreamResponse = serde_json::from_value(serde_json::json!({
            "id": "c1",
            "object": "chat.completion.chunk",
            "created": 0,
            "model": "m",
            "choices": [{"index": 0, "delta": {"content": "fin"}, "finish_reason": "length"}],
        }))
        .expect("chunk must parse");
        let mut pending = HashMap::new();
        let mut in_think = false;
        let mut finish = None;

        let text = next_emittable_text(
            &mut pending,
            chunk.inner,
            chunk.reasoning,
            super::stream::StreamFlags {
                in_think: &mut in_think,
                finish: &mut finish,
            },
        );

        assert_eq!(text.as_deref(), Some("fin"));
        assert_eq!(finish, Some(FinishReason::Length));
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

    // GIVEN a chunk from a server that inlines its reasoning (any server run
    // without a reasoning parser, such as llama-server with --reasoning-format none)
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
