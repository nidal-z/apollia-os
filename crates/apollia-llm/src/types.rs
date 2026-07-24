//! Core types and the `CompletionModel` trait for `apollia-llm`.
//!
//! This module is the foundation of the crate: every backend and the router
//! depend on these types. No backend is imported here, so the types are
//! available regardless of which feature is enabled.

use std::path::PathBuf;
use std::pin::Pin;

use futures::Stream;

/// Unified trait for any LLM backend: local via sidecar runner or cloud HTTP.
///
/// Implemented by `RunnerLlmBackend` (apollia-runtime sidecar),
/// `OpenAICompatibleClient`, `AnthropicClient` and `VertexClient` (feature `"cloud"`).
/// Stored as `Arc<dyn CompletionModel>` in the `LlmRouter`.
#[async_trait::async_trait]
pub trait CompletionModel: Send + Sync {
    /// Send an inference request and return the full response.
    async fn complete(&self, req: CompletionRequest) -> Result<CompletionResponse, LlmError>;

    /// Return a stream of [`StreamChunk`]s (text tokens and/or tool calls).
    async fn stream(
        &self,
        req: CompletionRequest,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<StreamChunk, LlmError>> + Send>>, LlmError>;

    /// Whether the backend is ready to accept requests.
    fn is_available(&self) -> bool;

    /// Logical backend name as configured in `apollia.toml`.
    fn backend_name(&self) -> &str;

    /// Identifier of the loaded model (e.g. `llama3.2-3b-q4`, `claude-haiku-4-5-20251001`).
    fn model_id(&self) -> &str;

    /// Returns `true` if this backend runs inference locally (runner sidecar).
    ///
    /// Used by [`crate::tool_helper::ToolCallHelper`] to decide whether to attach a
    /// GBNF grammar that constrains tool-call decoding. Cloud backends return `false`
    /// (the default); only the runner-backed local backend overrides this to `true`.
    fn is_local(&self) -> bool {
        false
    }

    /// Maximum context window of the loaded model, in tokens, when known.
    ///
    /// Lets the router size context compaction to the real model window instead
    /// of a generic fallback. Returns `None` when the backend cannot report it
    /// (e.g. before the first load, or cloud backends without a fixed window).
    fn context_window(&self) -> Option<usize> {
        None
    }

    /// Estimate the token count for the given message list.
    ///
    /// Default implementation: the `(total_chars / 4) * 1.2` proxy, which
    /// over-estimates to prefer an early compaction over a context overflow.
    ///
    /// Local backends that have access to the real GGUF tokenizer (e.g. the
    /// runner-backed backend) should override this and tokenize the messages
    /// for an exact count. Cloud backends keep the proxy.
    fn count_tokens(&self, messages: &[ChatMessage]) -> usize {
        let total_chars: usize = messages.iter().map(message_char_len).sum();
        ((total_chars as f32) / 4.0 * 1.2) as usize
    }
}

/// Total character length of the text content carried by a `ChatMessage`.
///
/// Basis for the `(chars / 4) * 1.2` token proxy used by the default
/// [`CompletionModel::count_tokens`] and by `LlmRouter::count_tokens`. Kept
/// `pub(crate)` so both live in one place without an apollia-llm -> apollia-oria
/// dependency.
pub(crate) fn message_char_len(msg: &ChatMessage) -> usize {
    match &msg.content {
        MessageContent::Text(s) => s.len(),
        MessageContent::ToolResult { content, .. } => content.len(),
        MessageContent::WithToolCalls { text, tool_calls } => {
            text.len()
                + tool_calls
                    .iter()
                    .map(|tc| tc.arguments.to_string().len())
                    .sum::<usize>()
        }
    }
}

/// Unified inference request for all backends.
///
/// Derives `Default` to allow `..Default::default()` syntax for partial
/// construction.
#[derive(Debug, Clone, Default)]
pub struct CompletionRequest {
    /// Conversation history to send to the model.
    pub messages: Vec<ChatMessage>,
    /// Tools (functions) exposed to the LLM for tool calling.
    pub tools: Vec<ToolSpec>,
    /// One-off model override (otherwise the backend uses its default).
    pub model: Option<String>,
    /// Sampling temperature (0.0 = deterministic, 1.0 = creative).
    pub temperature: Option<f32>,
    /// Maximum number of tokens to generate.
    pub max_tokens: Option<u32>,
    /// RNG seed for stochastic sampling.
    ///
    /// `None` (default): seed derived at call time (system entropy or clock),
    /// so each run samples differently. `Some(n)`: replays the same token
    /// sequence for debugging / regression. Not honored when
    /// `temperature == Some(0.0)` since the sampler is then strictly
    /// deterministic (argmax).
    pub seed: Option<u64>,
    /// GBNF grammar string for decode-time constrained generation.
    ///
    /// When `Some`, the local runner backend prepends a grammar sampler stage
    /// that restricts the token sequence to the grammar. Cloud backends ignore
    /// this field. `None` (default) means unconstrained generation.
    ///
    /// Typically produced by [`crate::grammar::tool_specs_to_gbnf`] from the
    /// active tool set.
    pub grammar: Option<String>,
}

/// Unified inference response returned by all backends.
#[derive(Debug, Clone)]
pub struct CompletionResponse {
    /// Text content generated by the model.
    pub content: String,
    /// Tool calls requested by the model (empty if `finish_reason != ToolCalls`).
    pub tool_calls: Vec<ToolCall>,
    /// Token consumption statistics.
    pub usage: TokenUsage,
    /// Why generation stopped.
    pub finish_reason: FinishReason,
    /// Total call latency in milliseconds.
    pub latency_ms: u64,
    /// Time to first token: duration until the first token is received (ms).
    ///
    /// `Some` only for streaming backends that measure TTFT. `None` for
    /// non-streaming calls or when not measured.
    pub ttft_ms: Option<u64>,
}

/// Anthropic cache marker for prompt caching.
///
/// May be set on a `ChatMessage` to indicate the message should be included
/// in a cache breakpoint when serializing to the Anthropic API. Only the
/// `Ephemeral` variant is supported by the current API.
///
/// Non-Anthropic backends (OpenAI, Ollama, local) ignore this field.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CacheControl {
    /// Ephemeral cache: the prefix is cached for up to 5 minutes.
    Ephemeral,
}

/// Token consumption statistics and estimated cost.
///
/// `cost_usd` is `None` for local backends (cost = 0). The `cache_*` fields
/// are `0` for non-Anthropic backends.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct TokenUsage {
    /// Number of tokens in the prompt (input).
    pub prompt_tokens: u32,
    /// Number of tokens generated (output).
    pub completion_tokens: u32,
    /// Estimated cost in USD: `None` for local backends (runner sidecar).
    pub cost_usd: Option<f64>,
    /// Tokens read from the Anthropic cache (cost reduced by ~90%).
    /// `0` for backends that do not support prompt caching.
    #[serde(default)]
    pub cache_read_input_tokens: u32,
    /// Tokens written to the Anthropic cache (slightly more expensive than normal input).
    /// `0` for backends that do not support prompt caching.
    #[serde(default)]
    pub cache_write_input_tokens: u32,
}

impl TokenUsage {
    /// Merge the counters from another `TokenUsage` into this one.
    ///
    /// Sums all token fields. `cost_usd` is summed when both are `Some`,
    /// otherwise the available `Some` is kept.
    pub fn merge(&mut self, other: &TokenUsage) {
        self.prompt_tokens += other.prompt_tokens;
        self.completion_tokens += other.completion_tokens;
        self.cache_read_input_tokens += other.cache_read_input_tokens;
        self.cache_write_input_tokens += other.cache_write_input_tokens;
        self.cost_usd = match (self.cost_usd, other.cost_usd) {
            (Some(a), Some(b)) => Some(a + b),
            (Some(a), None) => Some(a),
            (None, Some(b)) => Some(b),
            (None, None) => None,
        };
    }

    /// Returns the sum of prompt and completion tokens.
    ///
    /// Widens each `u32` field to `u64` before adding, so the sum cannot wrap
    /// (or panic on overflow in debug) even when both counters are near
    /// `u32::MAX`.
    #[must_use]
    pub fn total_tokens(&self) -> u64 {
        u64::from(self.prompt_tokens) + u64::from(self.completion_tokens)
    }
}

/// Why the model stopped generating.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FinishReason {
    /// Generation ended naturally (EOS token reached).
    Stop,
    /// The model requested tool execution.
    ToolCalls,
    /// Token limit reached before the natural end.
    Length,
    /// The backend returned an error.
    Error,
}

/// A message in a multi-turn conversation history.
#[derive(Debug, Clone)]
pub struct ChatMessage {
    /// Role of the message sender.
    pub role: Role,
    /// Message content (text, tool result, or text + tool calls).
    pub content: MessageContent,
    /// Anthropic cache marker for prompt caching.
    ///
    /// When `Some(CacheControl::Ephemeral)`, the Anthropic backend includes
    /// this message in a cache breakpoint when building the request.
    /// Non-Anthropic backends ignore this field.
    pub cache_control: Option<CacheControl>,
}

impl ChatMessage {
    /// Build a system message (global behavior instructions).
    pub fn system(text: impl Into<String>) -> Self {
        Self {
            role: Role::System,
            content: MessageContent::Text(text.into()),
            cache_control: None,
        }
    }

    /// Build a user message.
    pub fn user(text: impl Into<String>) -> Self {
        Self {
            role: Role::User,
            content: MessageContent::Text(text.into()),
            cache_control: None,
        }
    }

    /// Build an assistant message with no tool calls.
    pub fn assistant(text: impl Into<String>) -> Self {
        Self {
            role: Role::Assistant,
            content: MessageContent::Text(text.into()),
            cache_control: None,
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
        }
    }
}

/// Role of a `ChatMessage` sender.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Role {
    /// System instructions (global assistant behavior).
    System,
    /// Message from the human user.
    User,
    /// Reply from the LLM assistant.
    Assistant,
    /// Result of a tool call returned to the model.
    Tool,
}

/// Content of a `ChatMessage`.
#[derive(Debug, Clone)]
pub enum MessageContent {
    /// Plain text content.
    Text(String),
    /// Result of a tool call (`Tool` role).
    ToolResult {
        /// Identifier of the matching tool call.
        tool_call_id: String,
        /// Content returned by the tool.
        content: String,
    },
    /// Assistant reply combining text and tool calls.
    WithToolCalls {
        /// Text accompanying the tool calls (may be empty).
        text: String,
        /// Tool calls requested by the model.
        tool_calls: Vec<ToolCall>,
    },
}

/// Tool specification sent to the LLM in JSON Schema format.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ToolSpec {
    /// Tool name as the model will invoke it.
    pub name: String,
    /// Functional description of the tool to guide the model.
    pub description: String,
    /// JSON schema of the parameters the tool accepts.
    pub parameters: serde_json::Value,
}

/// Estimate the token cost of the tool schemas advertised to the model.
///
/// The tools array travels in the same request as the messages, so its size
/// must be reserved when deciding whether to compact the history; otherwise a
/// large tool surface silently eats the window and the request overflows. Uses
/// the same conservative `(chars / 4) * 1.2` proxy as the default token
/// estimate, applied to the serialized specs (an over-estimate, biased toward
/// compacting early rather than overflowing).
#[must_use]
pub fn estimate_tool_specs_tokens(specs: &[ToolSpec]) -> usize {
    if specs.is_empty() {
        return 0;
    }
    let chars = serde_json::to_string(specs).map(|s| s.len()).unwrap_or(0);
    ((chars as f32) / 4.0 * 1.2) as usize
}

/// Tool call requested by the LLM in a `CompletionResponse`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ToolCall {
    /// Unique identifier of this call (to correlate with the `ToolResult`).
    pub id: String,
    /// Name of the tool to invoke.
    pub name: String,
    /// Arguments passed to the tool (JSON object).
    pub arguments: serde_json::Value,
}

/// A chunk emitted by a streaming LLM response.
///
/// The stream yields a sequence of `Text` chunks (tokens), optionally
/// followed by one or more `ToolCall` chunks if the model requests
/// tool invocations.
#[derive(Debug, Clone)]
pub enum StreamChunk {
    /// Incremental text token for progressive display.
    Text(String),
    /// Tool call requested by the LLM (emitted when tool calling is detected in the stream).
    ToolCall(ToolCall),
}

/// Summary information about a backend, returned by `LlmRouter::list()`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BackendInfo {
    /// Logical backend name (configuration key).
    pub name: String,
    /// Identifier of the loaded model.
    pub model_id: String,
    /// `true` if the backend is ready to accept requests.
    pub available: bool,
}

/// Unified errors for the `apollia-llm` crate.
///
/// Each variant covers a distinct failure mode so the caller can react
/// differently (retry, degraded, abort, etc.).
#[derive(thiserror::Error, Debug)]
pub enum LlmError {
    /// The requested backend is not available.
    #[error("backend '{backend}' unavailable: {reason}")]
    BackendUnavailable {
        /// Backend name.
        backend: String,
        /// Reason for unavailability.
        reason: String,
    },

    /// The model file (.gguf) could not be found.
    #[error("model file not found: {path}")]
    ModelNotFound {
        /// Path to the expected file.
        path: PathBuf,
    },

    /// Internal inference engine error.
    #[error("inference error: {0}")]
    InferenceError(String),

    /// HTTP error returned by a cloud backend.
    #[error("HTTP error {status}: {body}")]
    HttpError {
        /// HTTP status code (e.g. 401, 429, 500).
        status: u16,
        /// Error response body.
        body: String,
    },

    /// The environment variable holding the API key is missing.
    #[error("API key missing: env var '{var}' not set")]
    ApiKeyMissing {
        /// Name of the expected environment variable.
        var: String,
    },

    /// The agent's `StepBudget` was exhausted during the ReAct loop.
    #[error("step budget exhausted during tool loop")]
    BudgetExceeded,

    /// The maximum number of ReAct loop iterations was reached.
    #[error("max tool iterations reached ({iterations})")]
    MaxIterationsReached {
        /// Configured iteration count.
        iterations: u32,
    },

    /// The generation token limit was reached.
    #[error("max tokens reached")]
    MaxTokensReached,

    /// Request rejected by the API (error 400).
    ///
    /// Can happen when an invalid parameter is sent (e.g. `cache_control`
    /// without the matching beta header, or a nonexistent model).
    #[error("bad request: {0}")]
    BadRequest(String),

    /// Could not parse the backend response.
    #[error("response parse error: {0}")]
    ParseError(String),

    /// The GGUF model uses an architecture the inference engine does not support.
    ///
    /// The user must pick a compatible model (Llama, Mistral, Qwen2, Phi, etc.).
    #[error("unsupported model architecture '{architecture}', try a Llama, Mistral, Qwen2, or Phi model instead")]
    UnsupportedModel {
        /// The unrecognized GGUF architecture name (e.g. `"qwen35moe"`).
        architecture: String,
    },

    /// The requested accelerator is not compiled into this binary.
    ///
    /// Recompile with the feature given in `hint` to enable this device.
    #[error("device '{device}' not available, recompile with --features {hint}")]
    DeviceNotAvailable {
        /// Requested device name (e.g. `"cuda"`, `"metal"`).
        device: String,
        /// Cargo feature to enable (e.g. `"local-cuda"`, `"local-metal"`).
        hint: String,
    },

    /// The API returned HTTP 429 (too many requests), a transient retryable error.
    #[error("rate limited (429)")]
    RateLimit,

    /// The Anthropic API returned HTTP 529 (server overloaded), a transient retryable error.
    #[error("server overloaded (529)")]
    Overload,

    /// The API returned HTTP 503 (service unavailable), a transient retryable error.
    #[error("service unavailable (503)")]
    ServiceUnavailable,

    /// The API returned HTTP 401, an invalid or expired API key.
    ///
    /// Distinct from [`LlmError::ApiKeyMissing`], which means the environment
    /// variable is not set. Here the key is present but rejected.
    #[error("unauthorized (401)")]
    Unauthorized,

    /// The LLM call was cancelled by the session's `CancellationToken`.
    #[error("cancelled by caller")]
    Cancelled,

    /// The `[llm.routing]` section is missing from `apollia.toml`.
    ///
    /// Fatal at startup (fail fast). Add `precise` and `fast` under
    /// `[llm.routing]` in `apollia.toml`.
    #[error("routing config missing, add [llm.routing] precise and fast in apollia.toml")]
    RoutingConfigMissing,

    /// The backend named in the routing config is not found in the router.
    ///
    /// The name must match a backend declared under `[[llm.backends]]`.
    #[error("backend '{0}' not found in configured backends")]
    BackendNotFound(String),

    /// One or more expected GGUF shards are missing from the folder.
    ///
    /// Raised by the runner (apollia-runner) when loading a GGUF whose path
    /// matches a standard split shard (`<prefix>-NNNNN-of-NNNNN.gguf`) but
    /// not all expected shards are present on disk.
    #[error(
        "GGUF split incomplete: prefix={prefix}, {found}/{total} shards present, \
         missing at least: {expected}"
    )]
    ModelShardMissing {
        /// Common prefix of the shards (without the `-NNNNN-of-NNNNN` suffix).
        prefix: String,
        /// Path of the first missing shard detected.
        expected: PathBuf,
        /// Total number of expected shards (the `-of-NNNNN` value).
        total: usize,
        /// Number of shards actually found in the folder.
        found: usize,
    },

    /// The config points at a shard other than the first (`-00001-of-NNNNN.gguf`).
    ///
    /// llama.cpp always expects the first shard as the entry point; loading
    /// any other shard does not produce a usable model.
    #[error(
        "config points to shard {given_index} of {path:?}, use the first shard \
         (`-00001-of-NNNNN.gguf`); llama.cpp loads the following shards automatically"
    )]
    ShardIndexNotFirst {
        /// Index extracted from the filename (the first number's value).
        given_index: u32,
        /// Faulty path supplied by the config.
        path: PathBuf,
    },

    /// Inconsistent LLM configuration detected at startup.
    ///
    /// Used in particular to flag an inconsistency in the runner config
    /// (`apollia.toml [llm.runner]`) or an invalid GGUF file.
    #[error("invalid config for backend '{backend}': {reason}")]
    ConfigConflict {
        /// Logical name of the faulty backend (the config's `name` field).
        backend: String,
        /// Human-readable problem description.
        reason: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Backend that stubs inference but keeps the DEFAULT `count_tokens`.
    struct ProxyOnlyModel;

    /// Backend that OVERRIDES `count_tokens` with a fixed value.
    struct FixedCountModel(usize);

    macro_rules! stub_completion_model {
        ($ty:ty, $name:literal) => {
            #[async_trait::async_trait]
            impl CompletionModel for $ty {
                async fn complete(
                    &self,
                    _req: CompletionRequest,
                ) -> Result<CompletionResponse, LlmError> {
                    Err(LlmError::InferenceError("stub".into()))
                }
                async fn stream(
                    &self,
                    _req: CompletionRequest,
                ) -> Result<
                    Pin<Box<dyn Stream<Item = Result<StreamChunk, LlmError>> + Send>>,
                    LlmError,
                > {
                    Err(LlmError::InferenceError("stub".into()))
                }
                fn is_available(&self) -> bool {
                    true
                }
                fn backend_name(&self) -> &str {
                    $name
                }
                fn model_id(&self) -> &str {
                    $name
                }
            }
        };
    }

    stub_completion_model!(ProxyOnlyModel, "proxy");

    #[async_trait::async_trait]
    impl CompletionModel for FixedCountModel {
        async fn complete(&self, _req: CompletionRequest) -> Result<CompletionResponse, LlmError> {
            Err(LlmError::InferenceError("stub".into()))
        }
        async fn stream(
            &self,
            _req: CompletionRequest,
        ) -> Result<Pin<Box<dyn Stream<Item = Result<StreamChunk, LlmError>> + Send>>, LlmError>
        {
            Err(LlmError::InferenceError("stub".into()))
        }
        fn is_available(&self) -> bool {
            true
        }
        fn backend_name(&self) -> &str {
            "fixed"
        }
        fn model_id(&self) -> &str {
            "fixed-model"
        }
        fn count_tokens(&self, _messages: &[ChatMessage]) -> usize {
            self.0
        }
    }

    // GIVEN a backend that does NOT override count_tokens
    // WHEN count_tokens is called with 4000 chars of content
    // THEN it returns the (chars / 4) * 1.2 proxy = 1200
    #[test]
    fn test_default_count_tokens_proxy() {
        let model = ProxyOnlyModel;
        let messages = vec![ChatMessage::user("a".repeat(4000))];
        assert_eq!(model.count_tokens(&messages), 1200);
    }

    // GIVEN a backend overriding count_tokens to return 42
    // WHEN count_tokens is called with any messages
    // THEN the override value wins over the default proxy
    #[test]
    fn test_override_count_tokens() {
        let model = FixedCountModel(42);
        let messages = vec![ChatMessage::user("a".repeat(4000))];
        assert_eq!(model.count_tokens(&messages), 42);
    }

    // GIVEN a backend with the default count_tokens
    // WHEN called on an empty message slice
    // THEN it yields 0 tokens
    #[test]
    fn test_count_tokens_empty_messages() {
        let model = ProxyOnlyModel;
        assert_eq!(model.count_tokens(&[]), 0);
    }

    // GIVEN a TokenUsage whose prompt and completion counters are both near u32::MAX
    // WHEN total_tokens sums them
    // THEN the result is the exact u64 sum, with no u32 overflow wrap or panic
    #[test]
    fn test_total_tokens_widens_before_adding() {
        let usage = TokenUsage {
            prompt_tokens: u32::MAX,
            completion_tokens: u32::MAX,
            ..Default::default()
        };
        assert_eq!(usage.total_tokens(), u64::from(u32::MAX) * 2);
    }

    // GIVEN a CompletionRequest built with only `messages`
    // WHEN optional fields are accessed
    // THEN they are None / empty
    #[test]
    fn test_completion_request_defaults() {
        let req = CompletionRequest {
            messages: vec![ChatMessage::user("hello")],
            ..Default::default()
        };

        assert!(req.model.is_none());
        assert!(req.temperature.is_none());
        assert!(req.max_tokens.is_none());
        assert!(req.tools.is_empty());
    }

    // GIVEN a default CompletionRequest
    // WHEN the grammar field is read
    // THEN it is None
    #[test]
    fn test_completion_request_default_grammar_is_none() {
        let req = CompletionRequest::default();
        assert!(req.grammar.is_none(), "grammar should be None by default");
    }

    // GIVEN partial construction with only messages
    // WHEN built with ..Default::default()
    // THEN it compiles and grammar defaults to None (backward compatibility)
    #[test]
    fn test_partial_construction_remains_valid() {
        let req = CompletionRequest {
            messages: vec![ChatMessage::user("bonjour")],
            ..Default::default()
        };
        assert_eq!(req.messages.len(), 1);
        assert!(req.grammar.is_none());
    }

    // GIVEN a non-empty GBNF string
    // WHEN assigned to the grammar field
    // THEN the field carries the value
    #[test]
    fn test_grammar_field_accepts_some_value() {
        let gbnf = "root ::= \"{}\"".to_string();
        let req = CompletionRequest {
            grammar: Some(gbnf),
            ..Default::default()
        };
        assert_eq!(req.grammar.as_deref(), Some("root ::= \"{}\""));
    }

    // GIVEN a BackendUnavailable error
    // WHEN formatted with Display
    // THEN message matches the #[error(...)] template
    #[test]
    fn test_llm_error_display_backend_unavailable() {
        let err = LlmError::BackendUnavailable {
            backend: "local".into(),
            reason: "model not loaded".into(),
        };
        assert_eq!(
            format!("{err}"),
            "backend 'local' unavailable: model not loaded"
        );
    }

    // GIVEN an ApiKeyMissing error
    // WHEN formatted
    // THEN message is correct
    #[test]
    fn test_llm_error_display_api_key_missing() {
        let err = LlmError::ApiKeyMissing {
            var: "ANTHROPIC_API_KEY".into(),
        };
        assert_eq!(
            format!("{err}"),
            "API key missing: env var 'ANTHROPIC_API_KEY' not set"
        );
    }

    // GIVEN a MaxIterationsReached error with 5 iterations
    // WHEN formatted
    // THEN message includes the count
    #[test]
    fn test_llm_error_display_max_iterations() {
        let err = LlmError::MaxIterationsReached { iterations: 5 };
        assert_eq!(format!("{err}"), "max tool iterations reached (5)");
    }

    // GIVEN text for system and user roles
    // WHEN ChatMessage helpers are called
    // THEN role and content match
    #[test]
    fn test_chat_message_helpers() {
        let sys = ChatMessage::system("tu es utile");
        let usr = ChatMessage::user("bonjour");
        let ast = ChatMessage::assistant("réponse");

        assert_eq!(sys.role, Role::System);
        assert_eq!(usr.role, Role::User);
        assert_eq!(ast.role, Role::Assistant);

        assert!(matches!(
            sys.content,
            MessageContent::Text(ref t) if t == "tu es utile"
        ));
        assert!(matches!(
            usr.content,
            MessageContent::Text(ref t) if t == "bonjour"
        ));
    }

    // GIVEN a tool_result message
    // WHEN constructed
    // THEN role is Tool and content is ToolResult
    #[test]
    fn test_chat_message_tool_result() {
        let msg = ChatMessage::tool_result("call_01", "fichier créé");
        assert_eq!(msg.role, Role::Tool);
        assert!(matches!(
            msg.content,
            MessageContent::ToolResult { ref tool_call_id, ref content }
            if tool_call_id == "call_01" && content == "fichier créé"
        ));
    }

    // GIVEN an assistant_with_calls message
    // WHEN constructed
    // THEN role is Assistant and content is WithToolCalls
    #[test]
    fn test_chat_message_assistant_with_calls() {
        let calls = vec![ToolCall {
            id: "c1".into(),
            name: "file_io".into(),
            arguments: serde_json::json!({}),
        }];
        let msg = ChatMessage::assistant_with_calls("je lis le fichier", &calls);
        assert_eq!(msg.role, Role::Assistant);
        assert!(matches!(
            msg.content,
            MessageContent::WithToolCalls { ref text, ref tool_calls }
            if text == "je lis le fichier" && tool_calls.len() == 1
        ));
    }

    // GIVEN a TokenUsage with no cost_usd
    // WHEN serialized to JSON
    // THEN cost_usd is "null" (not absent)
    #[test]
    fn test_token_usage_cost_usd_null_in_json() {
        let usage = TokenUsage {
            prompt_tokens: 10,
            completion_tokens: 20,
            cost_usd: None,
            ..Default::default()
        };
        let json = serde_json::to_string(&usage).expect("serialization must succeed");
        assert!(json.contains("\"cost_usd\":null"));
    }

    // GIVEN CacheControl::Ephemeral
    // WHEN serialized to JSON
    // THEN produces the string "ephemeral"
    #[test]
    fn test_cache_control_serialization() {
        let cc = CacheControl::Ephemeral;
        let json = serde_json::to_string(&cc).expect("serialization must succeed");
        assert_eq!(json, r#""ephemeral""#);
    }

    // GIVEN a ChatMessage with cache_control = None
    // WHEN the field is inspected
    // THEN it is None by default from all constructors
    #[test]
    fn test_chat_message_cache_control_none_by_default() {
        let sys = ChatMessage::system("prompt");
        let usr = ChatMessage::user("hello");
        let ast = ChatMessage::assistant("reply");
        assert!(sys.cache_control.is_none());
        assert!(usr.cache_control.is_none());
        assert!(ast.cache_control.is_none());
    }

    // GIVEN two TokenUsage values
    // WHEN merge() is called
    // THEN all fields are summed, cost_usd is accumulated
    #[test]
    fn test_token_usage_merge() {
        let mut base = TokenUsage {
            prompt_tokens: 100,
            completion_tokens: 50,
            cost_usd: Some(0.001),
            cache_read_input_tokens: 0,
            cache_write_input_tokens: 200,
        };
        let extra = TokenUsage {
            prompt_tokens: 30,
            completion_tokens: 10,
            cost_usd: Some(0.0005),
            cache_read_input_tokens: 150,
            cache_write_input_tokens: 0,
        };
        base.merge(&extra);
        assert_eq!(base.prompt_tokens, 130);
        assert_eq!(base.completion_tokens, 60);
        assert_eq!(base.cache_read_input_tokens, 150);
        assert_eq!(base.cache_write_input_tokens, 200);
        let cost = base.cost_usd.expect("cost must be Some");
        assert!((cost - 0.0015).abs() < f64::EPSILON);
    }

    // GIVEN a TokenUsage with cache fields
    // WHEN deserialized from JSON without cache fields
    // THEN cache fields default to 0
    #[test]
    fn test_token_usage_cache_fields_default_on_deserialize() {
        let json = r#"{"prompt_tokens":10,"completion_tokens":5,"cost_usd":null}"#;
        let usage: TokenUsage = serde_json::from_str(json).expect("must deserialize");
        assert_eq!(usage.cache_read_input_tokens, 0);
        assert_eq!(usage.cache_write_input_tokens, 0);
    }

    // GIVEN a ToolCall
    // WHEN serialized then deserialized
    // THEN fields are preserved
    #[test]
    fn test_tool_call_serde_roundtrip() {
        let call = ToolCall {
            id: "call_01".into(),
            name: "file_io".into(),
            arguments: serde_json::json!({"path": "/tmp/test.txt"}),
        };
        let json = serde_json::to_string(&call).expect("serialization must succeed");
        let back: ToolCall = serde_json::from_str(&json).expect("deserialization must succeed");
        assert_eq!(back.id, "call_01");
        assert_eq!(back.name, "file_io");
    }

    // GIVEN all LlmError variants
    // WHEN formatted
    // THEN none panics and messages are non-empty
    #[test]
    fn test_all_error_variants_display() {
        let errors: Vec<LlmError> = vec![
            LlmError::BackendUnavailable {
                backend: "b".into(),
                reason: "r".into(),
            },
            LlmError::ModelNotFound {
                path: std::path::PathBuf::from("/tmp/model.gguf"),
            },
            LlmError::InferenceError("engine crash".into()),
            LlmError::HttpError {
                status: 429,
                body: "rate limited".into(),
            },
            LlmError::ApiKeyMissing { var: "KEY".into() },
            LlmError::BudgetExceeded,
            LlmError::MaxIterationsReached { iterations: 3 },
            LlmError::MaxTokensReached,
            LlmError::BadRequest("invalid cache_control".into()),
            LlmError::ParseError("invalid json".into()),
            LlmError::UnsupportedModel {
                architecture: "qwen35moe".into(),
            },
            LlmError::DeviceNotAvailable {
                device: "cuda".into(),
                hint: "local-cuda".into(),
            },
            LlmError::RateLimit,
            LlmError::Overload,
            LlmError::ServiceUnavailable,
            LlmError::Unauthorized,
            LlmError::Cancelled,
            LlmError::ConfigConflict {
                backend: "local".into(),
                reason: "mutuellement exclusifs".into(),
            },
        ];
        for err in &errors {
            assert!(
                !format!("{err}").is_empty(),
                "error display must not be empty: {err:?}"
            );
        }
    }
}
