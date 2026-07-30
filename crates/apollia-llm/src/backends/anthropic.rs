//! Anthropic HTTP client via direct `reqwest`.
//!
//! This module compiles only with `feature = "cloud"`.
//!
//! Uses the Anthropic Messages API (`/v1/messages`) with the native headers
//! `x-api-key`, `anthropic-version: 2023-06-01` and the beta header
//! `prompt-caching-2024-07-31` for prompt caching.
//!
//! # Architecture
//!
//! ```text
//! apollia-llm [feature = "cloud"]
//!   └── AnthropicClient : CompletionModel
//!         ├── new()                    builds reqwest::Client with API key
//!         ├── parse_response()         pure function testable without HTTP
//!         ├── complete()               POST /v1/messages -> CompletionResponse
//!         └── stream()                 POST /v1/messages (stream=true) -> SSE chunks
//! ```
//!
//! # Prompt caching
//!
//! Each request includes the beta header `prompt-caching-2024-07-31`.
//! `build_request()` applies three automatic breakpoints:
//! - system prompt (stable breakpoint)
//! - last tool (stable breakpoint)
//! - 3rd message from the end (sliding breakpoint)
//!
//! Messages with `ChatMessage.cache_control = Some(CacheControl::Ephemeral)`
//! are also marked during conversion.

use std::pin::Pin;
use std::time::Instant;

use futures::{Stream, StreamExt};
use reqwest::header::{HeaderName, HeaderValue, CONTENT_TYPE};
use tokio_util::sync::CancellationToken;

use crate::pricing::{self, PricingTier};
use crate::retry::RetryPolicy;
use crate::types::{
    CacheControl, CompletionModel, CompletionRequest, CompletionResponse, FinishReason, LlmError,
    MessageContent, Role, StreamChunk, TokenUsage, ToolCall,
};

use super::openai::ApiBackendConfig;

// ── Constants ─────────────────────────────────────────────────────────────

/// Anthropic API version sent in the `anthropic-version` header.
const ANTHROPIC_VERSION: &str = "2023-06-01";

/// Name of the header carrying the Anthropic API key.
const ANTHROPIC_API_KEY_HEADER: &str = "x-api-key";

/// Name of the Anthropic API version header.
const ANTHROPIC_VERSION_HEADER: &str = "anthropic-version";

/// Anthropic beta header enabling prompt caching.
const PROMPT_CACHING_BETA_HEADER: &str = "anthropic-beta";

/// Beta header value for prompt caching.
const PROMPT_CACHING_BETA: &str = "prompt-caching-2024-07-31";

/// Cost multiplier for cache writes (25% more expensive than normal input).
const CACHE_WRITE_COST_MULTIPLIER: f64 = 1.25;

/// Cost multiplier for cache reads (90% cheaper than normal input).
const CACHE_READ_COST_MULTIPLIER: f64 = 0.1;

// ── Internal types: cache_control ─────────────────────────────────────────

/// Representation of `cache_control` in the Anthropic API format.
///
/// Serialized as `{"type": "ephemeral"}`.
#[derive(serde::Serialize, Clone)]
struct AnthropicCacheControl {
    #[serde(rename = "type")]
    cache_type: &'static str,
}

impl AnthropicCacheControl {
    /// Build a `cache_control` of type `ephemeral`.
    fn ephemeral() -> Self {
        Self {
            cache_type: "ephemeral",
        }
    }
}

// ── Internal types: request serialization ─────────────────────────────────

/// Anthropic Messages API request format.
#[derive(serde::Serialize)]
struct AnthropicRequest {
    model: String,
    max_tokens: u32,
    messages: Vec<AnthropicMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<AnthropicSystem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<AnthropicTool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
}

/// System prompt content: either a plain string or an array of blocks with
/// cache_control.
///
/// The Anthropic API accepts both forms. The array is used when prompt caching
/// is enabled to allow including `cache_control` on the last block.
#[derive(serde::Serialize, Clone)]
#[serde(untagged)]
enum AnthropicSystem {
    /// System prompt without caching, serialized as a JSON string.
    Plain(String),
    /// System prompt with caching, serialized as an array of blocks.
    Blocks(Vec<AnthropicSystemBlock>),
}

/// Content block in the `system` field of the Anthropic request.
#[derive(serde::Serialize, Clone)]
struct AnthropicSystemBlock {
    #[serde(rename = "type")]
    block_type: &'static str,
    text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    cache_control: Option<AnthropicCacheControl>,
}

/// Message in the Anthropic format (`role` + `content`).
#[derive(serde::Serialize, Clone)]
struct AnthropicMessage {
    role: &'static str,
    content: AnthropicContent,
}

/// Content of an Anthropic message: plain text or an array of blocks.
#[derive(serde::Serialize, Clone)]
#[serde(untagged)]
enum AnthropicContent {
    /// Plain text content.
    Text(String),
    /// Array of content blocks (tool, tool result, or structured text).
    Blocks(Vec<AnthropicBlock>),
}

/// Content block in the Anthropic format.
#[derive(serde::Serialize, Clone)]
#[serde(tag = "type", rename_all = "snake_case")]
enum AnthropicBlock {
    /// Text block with optional cache support.
    Text {
        text: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        cache_control: Option<AnthropicCacheControl>,
    },
    /// Tool call requested by the model.
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    /// Result of a tool call returned to the model, with optional cache support.
    ToolResult {
        tool_use_id: String,
        content: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        cache_control: Option<AnthropicCacheControl>,
    },
}

/// Tool specification in the Anthropic format.
#[derive(serde::Serialize, Clone)]
struct AnthropicTool {
    name: String,
    description: String,
    /// JSON schema of the parameters (equivalent to `parameters` in the OpenAI format).
    input_schema: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    cache_control: Option<AnthropicCacheControl>,
}

// ── Internal types: response deserialization ──────────────────────────────

/// Anthropic response parsed from JSON.
#[derive(serde::Deserialize)]
struct AnthropicResponse {
    content: Vec<AnthropicResponseContent>,
    stop_reason: Option<String>,
    usage: AnthropicUsage,
}

/// Content item in the Anthropic response.
#[derive(serde::Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum AnthropicResponseContent {
    /// Text generated by the model.
    Text { text: String },
    /// Tool call requested by the model.
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
}

/// Token statistics in the Anthropic response, including the cache fields.
#[derive(serde::Deserialize)]
struct AnthropicUsage {
    input_tokens: u32,
    output_tokens: u32,
    /// Tokens read from the cache, present only if prompt caching is active.
    #[serde(default)]
    cache_read_input_tokens: u32,
    /// Tokens written to the cache, present only if prompt caching is active.
    #[serde(default)]
    cache_write_input_tokens: u32,
}

/// Native HTTP client for the Anthropic Messages API (`/v1/messages`).
///
/// Built via [`AnthropicClient::new`] with an [`ApiBackendConfig`] and an API
/// key resolved from the environment variable. Supports
/// [`complete`](Self::complete) (full response) and [`stream`](Self::stream)
/// (SSE streaming chunk by chunk).
///
/// A single client can be shared via `Arc<AnthropicClient>`:
/// `reqwest::Client` is `Clone + Send + Sync`.
///
/// # Mandatory Anthropic headers
///
/// Each request includes:
/// - `x-api-key: {api_key}`
/// - `anthropic-version: 2023-06-01`
/// - `anthropic-beta: prompt-caching-2024-07-31`
/// - `content-type: application/json`
pub struct AnthropicClient {
    /// Base reqwest HTTP client.
    client: reqwest::Client,
    /// Backend configuration (name, base URL, default model).
    config: ApiBackendConfig,
    /// Anthropic API key, included in each request via `x-api-key`.
    /// Never logged or serialized.
    api_key: String,
    /// Default pricing table built at startup via [`pricing::default_pricing`].
    pricing_table: std::collections::HashMap<&'static str, PricingTier>,
    /// Operator overrides loaded from `[llm.pricing_overrides]` in `apollia.toml`.
    pricing_overrides: std::collections::HashMap<String, PricingTier>,
    /// Exponential retry policy shared with the other backends.
    retry_policy: RetryPolicy,
    /// Session cancellation token: `cancel()` interrupts in-flight calls and delays.
    cancel: CancellationToken,
}

impl AnthropicClient {
    /// Build an Anthropic client ready to send requests.
    ///
    /// `api_key` must be obtained beforehand via
    /// [`ApiBackendConfig::resolve_api_key`]; it is passed in here and not
    /// re-read from the environment to avoid TOCTOU.
    ///
    /// `pricing_overrides` are loaded from `[llm.pricing_overrides]` in
    /// `apollia.toml` and take priority over the default table when computing cost.
    ///
    /// `cancel` is the LLM session's `CancellationToken`, shared by the
    /// `LlmRouter`. A call to `cancel.cancel()` interrupts in-flight calls and
    /// retry delays.
    pub fn new(
        config: &ApiBackendConfig,
        api_key: String,
        pricing_overrides: std::collections::HashMap<String, PricingTier>,
        cancel: CancellationToken,
    ) -> Self {
        Self::with_idle_timeout(
            config,
            api_key,
            pricing_overrides,
            cancel,
            crate::http_client::DEFAULT_IDLE_TIMEOUT,
        )
    }

    /// Same as [`new`](Self::new) with an explicit tolerance to backend silence.
    ///
    /// Bounds the connect phase and the read-idle phase, never the total
    /// request: a long generation is legitimate, prolonged silence is not.
    /// See [`crate::http_client`].
    pub fn with_idle_timeout(
        config: &ApiBackendConfig,
        api_key: String,
        pricing_overrides: std::collections::HashMap<String, PricingTier>,
        cancel: CancellationToken,
        idle_timeout: std::time::Duration,
    ) -> Self {
        Self {
            client: crate::http_client::build_llm_http_client(idle_timeout),
            config: config.clone(),
            api_key,
            pricing_table: pricing::default_pricing(),
            pricing_overrides,
            retry_policy: RetryPolicy::default(),
            cancel,
        }
    }

    /// Parse an Anthropic JSON response into a [`CompletionResponse`].
    ///
    /// Pure function, testable without HTTP. Maps:
    /// - `stop_reason = "end_turn"` to [`FinishReason::Stop`]
    /// - `stop_reason = "tool_use"` to [`FinishReason::ToolCalls`]
    /// - `content[].type = "text"` to `response.content`
    /// - `content[].type = "tool_use"` to `response.tool_calls`
    /// - `usage.cache_read_input_tokens` / `cache_write_input_tokens` to `TokenUsage`
    ///
    /// Returns `LlmError::ParseError` if the JSON does not match the expected
    /// Anthropic Messages API format.
    pub fn parse_response(json: &serde_json::Value) -> Result<CompletionResponse, LlmError> {
        let response: AnthropicResponse = serde_json::from_value(json.clone())
            .map_err(|e| LlmError::ParseError(format!("invalid Anthropic response: {e}")))?;

        let stop_reason = response.stop_reason.as_deref().unwrap_or("end_turn");
        let finish_reason = map_stop_reason(stop_reason);

        let mut content_text = String::new();
        let mut tool_calls: Vec<ToolCall> = Vec::new();

        for item in response.content {
            match item {
                AnthropicResponseContent::Text { text } => {
                    content_text = text;
                }
                AnthropicResponseContent::ToolUse { id, name, input } => {
                    tool_calls.push(ToolCall {
                        id,
                        name,
                        arguments: input,
                    });
                }
            }
        }

        Ok(CompletionResponse {
            engine_timings: None,
            content: content_text,
            tool_calls,
            usage: TokenUsage {
                prompt_tokens: response.usage.input_tokens,
                completion_tokens: response.usage.output_tokens,
                cost_usd: None, // filled by complete() with the model id
                cache_read_input_tokens: response.usage.cache_read_input_tokens,
                cache_write_input_tokens: response.usage.cache_write_input_tokens,
            },
            finish_reason,
            latency_ms: 0, // filled by complete() after the HTTP call
            ttft_ms: None,
        })
    }

    /// Build a [`reqwest::RequestBuilder`] with the mandatory Anthropic headers.
    ///
    /// Includes the beta header `prompt-caching-2024-07-31` on every request to
    /// enable prompt caching.
    ///
    /// Returns `LlmError::InferenceError` if the API key contains non-ASCII
    /// characters (cannot happen with a valid Anthropic key).
    fn request_builder(&self, url: &str) -> Result<reqwest::RequestBuilder, LlmError> {
        let api_key_val = HeaderValue::try_from(self.api_key.as_str())
            .map_err(|e| LlmError::InferenceError(format!("invalid api_key header value: {e}")))?;

        Ok(self
            .client
            .post(url)
            .header(
                HeaderName::from_static(ANTHROPIC_API_KEY_HEADER),
                api_key_val,
            )
            .header(
                HeaderName::from_static(ANTHROPIC_VERSION_HEADER),
                HeaderValue::from_static(ANTHROPIC_VERSION),
            )
            .header(
                HeaderName::from_static(PROMPT_CACHING_BETA_HEADER),
                HeaderValue::from_static(PROMPT_CACHING_BETA),
            )
            .header(CONTENT_TYPE, HeaderValue::from_static("application/json")))
    }

    /// Make a single HTTP POST `/v1/messages` call without retry.
    ///
    /// Maps transient HTTP statuses to retryable [`LlmError`] variants:
    /// - 429 to [`LlmError::RateLimit`]
    /// - 503 to [`LlmError::ServiceUnavailable`]
    /// - 529 to [`LlmError::Overload`]
    /// - 401 to [`LlmError::Unauthorized`]
    /// - 400 to [`LlmError::BadRequest`]
    /// - other >= 400 to [`LlmError::HttpError`]
    async fn do_complete(&self, req: CompletionRequest) -> Result<CompletionResponse, LlmError> {
        let started = Instant::now();
        let body = self.build_request(&req, false);
        let model = body.model.clone();
        let url = format!("{}/v1/messages", self.config.api_url);

        let http_response = self
            .request_builder(&url)?
            .json(&body)
            .send()
            .await
            .map_err(|e| LlmError::HttpError {
                status: e.status().map(|s| s.as_u16()).unwrap_or(0),
                body: e.to_string(),
            })?;

        let status = http_response.status();
        if !status.is_success() {
            let body_text = http_response.text().await.unwrap_or_default();
            return Err(match status.as_u16() {
                400 => LlmError::BadRequest(body_text),
                401 => LlmError::Unauthorized,
                429 => LlmError::RateLimit,
                503 => LlmError::ServiceUnavailable,
                529 => LlmError::Overload,
                code => LlmError::HttpError {
                    status: code,
                    body: body_text,
                },
            });
        }

        let json: serde_json::Value = http_response.json().await.map_err(|e| {
            LlmError::ParseError(format!("failed to decode Anthropic response body: {e}"))
        })?;

        let mut result = Self::parse_response(&json)?;
        result.latency_ms = started.elapsed().as_millis() as u64;
        result.usage.cost_usd =
            match pricing::lookup_pricing(&model, &self.pricing_table, &self.pricing_overrides) {
                Some(tier) => {
                    let input_cost =
                        result.usage.prompt_tokens as f64 * tier.input_per_mtok / 1_000_000.0;
                    let output_cost =
                        result.usage.completion_tokens as f64 * tier.output_per_mtok / 1_000_000.0;
                    let cache_write_cost = result.usage.cache_write_input_tokens as f64
                        * tier.input_per_mtok
                        * CACHE_WRITE_COST_MULTIPLIER
                        / 1_000_000.0;
                    let cache_read_cost = result.usage.cache_read_input_tokens as f64
                        * tier.input_per_mtok
                        * CACHE_READ_COST_MULTIPLIER
                        / 1_000_000.0;
                    Some(input_cost + output_cost + cache_write_cost + cache_read_cost)
                }
                None => {
                    tracing::warn!(model_id = %model, "unknown model for pricing");
                    None
                }
            };

        tracing::info!(
            backend = %self.config.name,
            model = %model,
            prompt_tokens = result.usage.prompt_tokens,
            completion_tokens = result.usage.completion_tokens,
            cache_read_tokens = result.usage.cache_read_input_tokens,
            cache_write_tokens = result.usage.cache_write_input_tokens,
            latency_ms = result.latency_ms,
            "Anthropic complete() done"
        );

        Ok(result)
    }

    /// Convert a [`CompletionRequest`] into an [`AnthropicRequest`] with cache breakpoints.
    ///
    /// Extracts the `System` role message (first found) into the Anthropic
    /// request's `system` field. Other messages are converted via
    /// [`convert_message`].
    ///
    /// Then applies three automatic cache breakpoints via
    /// [`apply_cache_breakpoints`]:
    /// 1. System prompt
    /// 2. Last tool
    /// 3. 3rd message from the end of the history (sliding breakpoint)
    fn build_request(&self, req: &CompletionRequest, stream: bool) -> AnthropicRequest {
        let model = req
            .model
            .as_deref()
            .unwrap_or(&self.config.model)
            .to_owned();

        // First System message goes into the separate `system` field (Anthropic format)
        let mut system: Option<AnthropicSystem> = req.messages.iter().find_map(|msg| {
            if msg.role == Role::System {
                if let MessageContent::Text(text) = &msg.content {
                    return Some(AnthropicSystem::Plain(text.clone()));
                }
            }
            None
        });

        // Non-System messages converted to the Anthropic format
        let mut messages: Vec<AnthropicMessage> = req
            .messages
            .iter()
            .filter(|msg| msg.role != Role::System)
            .filter_map(convert_message)
            .collect();

        // Apollia tools to Anthropic format (input_schema instead of parameters)
        let mut tools: Option<Vec<AnthropicTool>> = if req.tools.is_empty() {
            None
        } else {
            Some(
                req.tools
                    .iter()
                    .map(|spec| AnthropicTool {
                        name: spec.name.clone(),
                        description: spec.description.clone(),
                        input_schema: spec.parameters.clone(),
                        cache_control: None,
                    })
                    .collect(),
            )
        };

        // Apply the automatic cache breakpoints
        apply_cache_breakpoints(&mut messages, &mut system, &mut tools);

        AnthropicRequest {
            model,
            max_tokens: req.max_tokens.unwrap_or(4096),
            messages,
            system,
            tools,
            stream: if stream { Some(true) } else { None },
            temperature: req.temperature,
        }
    }
}

#[async_trait::async_trait]
impl CompletionModel for AnthropicClient {
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

    /// Return a stream of text chunks via Anthropic SSE.
    ///
    /// Sends POST `/v1/messages` with `stream: true`, consumes the SSE events
    /// line by line, extracts the `content_block_delta.delta.text` and emits
    /// them. The stream ends at `message_stop`.
    ///
    /// Returns `LlmError::HttpError` for any HTTP status >= 400 before the
    /// stream starts. Network errors during the stream are propagated in the
    /// items.
    async fn stream(
        &self,
        req: CompletionRequest,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<StreamChunk, LlmError>> + Send>>, LlmError> {
        let body = self.build_request(&req, true);
        let url = format!("{}/v1/messages", self.config.api_url);

        let http_response = self
            .request_builder(&url)?
            .json(&body)
            .send()
            .await
            .map_err(|e| LlmError::HttpError {
                status: e.status().map(|s| s.as_u16()).unwrap_or(0),
                body: e.to_string(),
            })?;

        let status = http_response.status();
        if !status.is_success() {
            let body_text = http_response.text().await.unwrap_or_default();
            if status.as_u16() == 400 {
                return Err(LlmError::BadRequest(body_text));
            }
            return Err(LlmError::HttpError {
                status: status.as_u16(),
                body: body_text,
            });
        }

        // Convert the bytes_stream to Vec<u8> to avoid naming bytes::Bytes
        let byte_stream: Pin<Box<dyn Stream<Item = Result<Vec<u8>, LlmError>> + Send>> =
            Box::pin(http_response.bytes_stream().map(|result| {
                result.map(|b| b.to_vec()).map_err(|e| LlmError::HttpError {
                    status: e.status().map(|s| s.as_u16()).unwrap_or(0),
                    body: e.to_string(),
                })
            }));

        Ok(Box::pin(parse_sse_stream(byte_stream)))
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

// ── Private helpers: prompt caching ───────────────────────────────────────

/// Apply the three automatic cache breakpoints on the Anthropic request.
///
/// Breakpoint 1, system prompt: converted to an array of blocks with
/// `cache_control: ephemeral`.
/// Breakpoint 2, last tool: marked with `cache_control: ephemeral`.
/// Breakpoint 3, 3rd message from the end: its last content block marked.
///
/// The breakpoints are cumulative with the individual marks from
/// `ChatMessage.cache_control` (already applied by [`convert_message`]).
fn apply_cache_breakpoints(
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
fn mark_message_cache_control(msg: &mut AnthropicMessage) {
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
                        tracing::debug!(
                            "cache breakpoint on message ending with tool_use block - ignored"
                        );
                    }
                }
            }
            AnthropicContent::Blocks(blocks)
        }
    };
    msg.content = new_content;
}

// ── Private helpers: message conversion ───────────────────────────────────

/// Build a text [`AnthropicContent`], as a block with `cache_control` if
/// `cache` is `Some`, otherwise as plain text.
fn text_content(text: &str, cache: Option<AnthropicCacheControl>) -> AnthropicContent {
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
fn tool_call_blocks(
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

fn convert_message(msg: &crate::types::ChatMessage) -> Option<AnthropicMessage> {
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
                "AnthropicClient: combinaison rôle/contenu non supportée, message ignoré"
            );
            None
        }
    }
}

/// Map the Anthropic `stop_reason` to the Apollia [`FinishReason`].
fn map_stop_reason(stop_reason: &str) -> FinishReason {
    match stop_reason {
        "end_turn" => FinishReason::Stop,
        "tool_use" => FinishReason::ToolCalls,
        "max_tokens" => FinishReason::Length,
        _ => FinishReason::Stop,
    }
}

/// Convert a byte stream into a stream of Anthropic SSE chunks.
///
/// Parses the SSE events line by line:
/// - `content_block_delta` with `delta.type = "text_delta"` emits `StreamChunk::Text`
/// - `content_block_start` with `type = "tool_use"` records id + name
/// - `content_block_delta` with `delta.type = "input_json_delta"` accumulates the JSON arguments
/// - `content_block_stop` emits `StreamChunk::ToolCall` if a tool was in progress
/// - `message_stop` ends the stream
fn parse_sse_stream(
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
struct PendingToolCall {
    id: String,
    name: String,
    arguments_json: String,
}

struct SseState {
    stream: Pin<Box<dyn Stream<Item = Result<Vec<u8>, LlmError>> + Send>>,
    buffer: Vec<u8>,
    /// Tool call currently being accumulated (one at a time).
    pending_tool: Option<PendingToolCall>,
}

/// Decision after processing an SSE line.
enum SseAction {
    /// Irrelevant line, keep reading the buffer.
    Continue,
    /// `message_stop` received, end the stream.
    Stop,
    /// Emit a chunk to the caller.
    Emit(StreamChunk),
}

/// Extract the next line from the buffer (up to and including `nl`) and process it.
fn handle_sse_line(state: &mut SseState, nl: usize) -> SseAction {
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
fn handle_sse_event(
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
fn handle_content_block_delta(
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
fn record_tool_use_start(json: &serde_json::Value, pending_tool: &mut Option<PendingToolCall>) {
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
fn emit_pending_tool(pending_tool: &mut Option<PendingToolCall>) -> SseAction {
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

#[cfg(all(test, feature = "cloud"))]
mod tests {
    use super::*;
    use serde_json::json;

    // GIVEN an Anthropic response with stop_reason = "end_turn" and content[].text
    // WHEN parse_response() is called
    // THEN finish_reason == Stop, content == extracted text, correct tokens
    #[test]
    fn test_parse_end_turn_response() {
        let json = json!({
            "content": [{"type": "text", "text": "Bonjour !"}],
            "stop_reason": "end_turn",
            "usage": {"input_tokens": 10, "output_tokens": 5}
        });

        let response = AnthropicClient::parse_response(&json).unwrap();

        assert_eq!(response.finish_reason, FinishReason::Stop);
        assert_eq!(response.content, "Bonjour !");
        assert_eq!(response.usage.prompt_tokens, 10);
        assert_eq!(response.usage.completion_tokens, 5);
        assert_eq!(response.usage.cache_read_input_tokens, 0);
        assert_eq!(response.usage.cache_write_input_tokens, 0);
    }

    // GIVEN an Anthropic response with stop_reason = "tool_use" and content[].type = "tool_use"
    // WHEN parse_response() is called
    // THEN finish_reason == ToolCalls, tool_calls[0] has correct id and name
    #[test]
    fn test_parse_tool_use_response() {
        let json = json!({
            "content": [
                {"type": "tool_use", "id": "toolu_01", "name": "file_io",
                 "input": {"path": "/tmp/test.txt"}}
            ],
            "stop_reason": "tool_use",
            "usage": {"input_tokens": 20, "output_tokens": 10}
        });

        let response = AnthropicClient::parse_response(&json).unwrap();

        assert_eq!(response.finish_reason, FinishReason::ToolCalls);
        assert_eq!(response.tool_calls.len(), 1);
        assert_eq!(response.tool_calls[0].id, "toolu_01");
        assert_eq!(response.tool_calls[0].name, "file_io");
    }

    // GIVEN a malformed Anthropic JSON (expected fields absent)
    // WHEN parse_response() is called
    // THEN Err(LlmError::ParseError) is returned
    #[test]
    fn test_parse_error_on_malformed_json() {
        let json = json!({"unexpected": "format"});

        let result = AnthropicClient::parse_response(&json);

        assert!(
            matches!(result, Err(LlmError::ParseError(_))),
            "expected ParseError for malformed response, got: {result:?}"
        );
    }

    // GIVEN an ApiBackendConfig for Anthropic with a fake key
    // WHEN AnthropicClient::new() is called
    // THEN the client is built without panicking and backend_name() returns config.name
    #[test]
    fn test_new_does_not_panic() {
        std::env::set_var("APOLLIA_ANT_TEST_KEY", "sk-ant-test");
        let config = ApiBackendConfig {
            name: "anthropic".into(),
            api_url: "https://api.anthropic.com".into(),
            api_key_env: "APOLLIA_ANT_TEST_KEY".into(),
            model: "claude-haiku-4-5-20251001".into(),
        };

        let client = AnthropicClient::new(
            &config,
            "sk-ant-test".into(),
            std::collections::HashMap::new(),
            tokio_util::sync::CancellationToken::new(),
        );

        std::env::remove_var("APOLLIA_ANT_TEST_KEY");
        assert!(client.is_available());
        assert_eq!(client.backend_name(), "anthropic");
        assert_eq!(client.model_id(), "claude-haiku-4-5-20251001");
    }

    // GIVEN a response with stop_reason = "max_tokens"
    // WHEN parse_response() is called
    // THEN finish_reason == Length
    #[test]
    fn test_parse_max_tokens_finish_reason() {
        let json = json!({
            "content": [{"type": "text", "text": "tronqué"}],
            "stop_reason": "max_tokens",
            "usage": {"input_tokens": 100, "output_tokens": 500}
        });

        let response = AnthropicClient::parse_response(&json).unwrap();

        assert_eq!(response.finish_reason, FinishReason::Length);
    }

    // GIVEN a response with text + tool_use in the content
    // WHEN parse_response() is called
    // THEN content holds the text AND tool_calls holds the tool
    #[test]
    fn test_parse_mixed_content_text_and_tool_use() {
        let json = json!({
            "content": [
                {"type": "text", "text": "Je vais lire ce fichier."},
                {"type": "tool_use", "id": "toolu_02", "name": "bash_executor",
                 "input": {"command": "cat /tmp/data.txt"}}
            ],
            "stop_reason": "tool_use",
            "usage": {"input_tokens": 30, "output_tokens": 15}
        });

        let response = AnthropicClient::parse_response(&json).unwrap();

        assert_eq!(response.finish_reason, FinishReason::ToolCalls);
        assert_eq!(response.content, "Je vais lire ce fichier.");
        assert_eq!(response.tool_calls.len(), 1);
        assert_eq!(response.tool_calls[0].id, "toolu_02");
        assert_eq!(response.tool_calls[0].name, "bash_executor");
    }

    // GIVEN a response with cache_read_input_tokens and cache_write_input_tokens
    // WHEN parse_response() is called
    // THEN the cache fields are correctly extracted into TokenUsage
    #[test]
    fn test_parse_response_extracts_cache_tokens() {
        let json = json!({
            "content": [{"type": "text", "text": "response"}],
            "stop_reason": "end_turn",
            "usage": {
                "input_tokens": 10,
                "output_tokens": 5,
                "cache_read_input_tokens": 150,
                "cache_write_input_tokens": 300
            }
        });

        let response = AnthropicClient::parse_response(&json).unwrap();

        assert_eq!(response.usage.cache_read_input_tokens, 150);
        assert_eq!(response.usage.cache_write_input_tokens, 300);
        assert_eq!(response.usage.prompt_tokens, 10);
        assert_eq!(response.usage.completion_tokens, 5);
    }

    // GIVEN a system prompt + a list of messages (>= 3)
    // WHEN apply_cache_breakpoints() is called
    // THEN the system is in Blocks form with cache_control
    //   AND the 3rd message from the end has cache_control
    #[test]
    fn test_apply_cache_breakpoints_system_and_sliding() {
        let mut system = Some(AnthropicSystem::Plain("Be helpful".to_string()));
        let mut messages = vec![
            AnthropicMessage {
                role: "user",
                content: AnthropicContent::Text("msg1".to_string()),
            },
            AnthropicMessage {
                role: "assistant",
                content: AnthropicContent::Text("msg2".to_string()),
            },
            AnthropicMessage {
                role: "user",
                content: AnthropicContent::Text("msg3".to_string()),
            },
            AnthropicMessage {
                role: "assistant",
                content: AnthropicContent::Text("msg4".to_string()),
            },
            AnthropicMessage {
                role: "user",
                content: AnthropicContent::Text("msg5".to_string()),
            },
        ];
        let mut tools: Option<Vec<AnthropicTool>> = None;

        apply_cache_breakpoints(&mut messages, &mut system, &mut tools);

        // System must be in Blocks form with cache_control
        match &system {
            Some(AnthropicSystem::Blocks(blocks)) => {
                assert_eq!(blocks.len(), 1);
                assert!(blocks[0].cache_control.is_some());
            }
            _ => panic!("system must be Blocks after apply_cache_breakpoints"),
        }

        // messages[2] = 3rd from the end (index len-3 = 5-3 = 2) must have cache_control
        match &messages[2].content {
            AnthropicContent::Blocks(blocks) => {
                assert!(
                    blocks.iter().any(|b| matches!(
                        b,
                        AnthropicBlock::Text {
                            cache_control: Some(_),
                            ..
                        }
                    )),
                    "messages[2] must have cache_control"
                );
            }
            _ => panic!("messages[2] must be Blocks after breakpoint"),
        }

        // messages[4] (last) must NOT have an automatic cache_control
        match &messages[4].content {
            AnthropicContent::Text(_) => {}
            _ => panic!("messages[4] must remain Text (no auto breakpoint)"),
        }
    }

    // GIVEN a list of tools
    // WHEN apply_cache_breakpoints() is called
    // THEN the last tool has cache_control: ephemeral
    #[test]
    fn test_apply_cache_breakpoints_last_tool() {
        let mut system: Option<AnthropicSystem> = None;
        let mut messages: Vec<AnthropicMessage> = Vec::new();
        let mut tools = Some(vec![
            AnthropicTool {
                name: "tool_a".to_string(),
                description: "first".to_string(),
                input_schema: json!({}),
                cache_control: None,
            },
            AnthropicTool {
                name: "tool_b".to_string(),
                description: "last".to_string(),
                input_schema: json!({}),
                cache_control: None,
            },
        ]);

        apply_cache_breakpoints(&mut messages, &mut system, &mut tools);

        let tools_vec = tools.as_ref().unwrap();
        assert!(
            tools_vec[0].cache_control.is_none(),
            "tool_a must not be marked"
        );
        assert!(
            tools_vec[1].cache_control.is_some(),
            "tool_b (last) must be marked"
        );
    }

    // GIVEN a message with ChatMessage.cache_control = Some(Ephemeral)
    // WHEN convert_message() is called
    // THEN the text content is converted to a block with cache_control: ephemeral
    #[test]
    fn test_convert_message_respects_cache_control() {
        use crate::types::ChatMessage;

        let mut msg = ChatMessage::user("hello");
        msg.cache_control = Some(CacheControl::Ephemeral);

        let anthropic_msg = convert_message(&msg).expect("must convert");

        match &anthropic_msg.content {
            AnthropicContent::Blocks(blocks) => {
                assert_eq!(blocks.len(), 1);
                assert!(
                    matches!(
                        &blocks[0],
                        AnthropicBlock::Text {
                            cache_control: Some(_),
                            ..
                        }
                    ),
                    "user message with cache_control must produce a cached text block"
                );
            }
            _ => panic!("expected Blocks content for cached user message"),
        }
    }

    // GIVEN a message without cache_control
    // WHEN convert_message() is called
    // THEN the content is plain Text (no blocks)
    #[test]
    fn test_convert_message_no_cache_control_stays_text() {
        use crate::types::ChatMessage;

        let msg = ChatMessage::user("hello");
        let anthropic_msg = convert_message(&msg).expect("must convert");

        assert!(
            matches!(anthropic_msg.content, AnthropicContent::Text(_)),
            "user message without cache_control must remain Text"
        );
    }

    // GIVEN AnthropicCacheControl::ephemeral()
    // WHEN serialized
    // THEN produces {"type": "ephemeral"}
    #[test]
    fn test_anthropic_cache_control_serialization() {
        let cc = AnthropicCacheControl::ephemeral();
        let json = serde_json::to_value(&cc).unwrap();
        assert_eq!(json["type"], "ephemeral");
    }
}
