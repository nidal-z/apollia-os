//! Ollama through its native `/api/chat` endpoint.
//!
//! Ollama also serves an OpenAI-compatible endpoint, which is how it was
//! reached until 2026-09-22, and that endpoint has no field for the context
//! window. Every call through it runs at Ollama's default, which Ollama sizes
//! from video memory: 4096 tokens on a 16 GB card. A request larger than that is
//! cut from the front without an error, so the model lost first its system
//! prompt and tool schemas, then the conversation, and answered as if it had
//! neither. Once Apollia learned the small window it compacted on every turn,
//! and the operator saw a model summarizing instead of answering.
//!
//! The native endpoint takes `options.num_ctx`, separates a thinking model's
//! reasoning into its own field, and carries tool calls with object arguments.
//! See [`wire`] for the shapes and [`OllamaClient::num_ctx`] for how the
//! window is chosen.
//!
//! This module compiles only with `feature = "cloud"`.

#![cfg(feature = "cloud")]

use std::pin::Pin;
use std::time::Instant;

use futures::Stream;
use tokio_util::sync::CancellationToken;

use crate::retry::RetryPolicy;
use crate::types::{CompletionModel, CompletionRequest, CompletionResponse, LlmError, StreamChunk};

pub mod wire;

/// Window requested when neither the operator nor the model says otherwise.
///
/// The same figure the embedded engine launches with, so a conversation that
/// fits one local backend fits the other. Capped by the model's own trained
/// length when that is smaller.
pub const DEFAULT_NUM_CTX: u32 = 32_768;

/// Largest answer body buffered for a non-streamed call.
const MAX_RESPONSE_BYTES: u64 = 32 * 1024 * 1024;

/// Where and how an Ollama backend is reached.
#[derive(Debug, Clone)]
pub struct OllamaConfig {
    /// Logical backend name (the key used in `LlmRouter`).
    pub name: String,
    /// Server root, for example `http://127.0.0.1:11434`, without `/v1`.
    pub root: String,
    /// Default model tag, for example `qwen3.5` or `qwen3.5:9b`.
    pub model: String,
    /// Context window requested on every call, in tokens.
    pub num_ctx: u32,
}

impl OllamaConfig {
    /// The native API root for a configured base URL.
    ///
    /// Backends were configured against the OpenAI-compatible path, so a
    /// stored `.../v1` is accepted and trimmed rather than migrated.
    #[must_use]
    pub fn root_of(base_url: &str) -> String {
        let trimmed = base_url.trim_end_matches('/');
        trimmed.strip_suffix("/v1").unwrap_or(trimmed).to_owned()
    }
}

/// A client for one Ollama backend.
pub struct OllamaClient {
    http: reqwest::Client,
    config: OllamaConfig,
    retry_policy: RetryPolicy,
    cancel: CancellationToken,
}

impl OllamaClient {
    /// Build a client. `idle_timeout` bounds silence, never the whole call.
    #[must_use]
    pub fn new(
        config: OllamaConfig,
        cancel: CancellationToken,
        idle_timeout: std::time::Duration,
    ) -> Self {
        Self {
            http: crate::http_client::build_llm_http_client(idle_timeout, &config.root),
            config,
            retry_policy: RetryPolicy::default(),
            cancel,
        }
    }

    /// Choose the window to request for `model`.
    ///
    /// The operator's configured `context_window`, else [`DEFAULT_NUM_CTX`],
    /// lowered to the model's trained length when the server reports a shorter
    /// one through `/api/show`: past it the model reads positions it was never
    /// trained on (see [`crate::context_window::cap_at_trained`]). A server
    /// that cannot be asked leaves the window as chosen.
    pub async fn num_ctx(root: &str, model: &str, configured: Option<u32>) -> u32 {
        let asked = configured.filter(|n| *n > 0).unwrap_or(DEFAULT_NUM_CTX);
        let client =
            crate::http_client::build_llm_http_client(std::time::Duration::from_secs(5), root);
        let trained = async {
            let resp = client
                .post(format!("{root}/api/show"))
                .json(&serde_json::json!({ "model": model }))
                .send()
                .await
                .ok()?;
            if !resp.status().is_success() {
                return None;
            }
            let body: serde_json::Value =
                apollia_core::net::read_capped_json(resp, apollia_core::net::MAX_METADATA_BYTES)
                    .await
                    .ok()?;
            wire::trained_context(&body)
        }
        .await;
        crate::context_window::cap_at_trained(asked, trained.map(u64::from))
    }

    fn model(&self) -> &str {
        &self.config.model
    }

    async fn send(&self, body: &serde_json::Value) -> Result<reqwest::Response, LlmError> {
        let resp = self
            .http
            .post(format!("{}/api/chat", self.config.root))
            .json(body)
            .send()
            .await
            .map_err(|e| LlmError::BackendUnavailable {
                backend: self.config.name.clone(),
                reason: format!("ollama unreachable: {e}"),
            })?;
        let status = resp.status();
        if status.is_success() {
            return Ok(resp);
        }
        let body = apollia_core::net::read_capped_text(resp, apollia_core::net::MAX_METADATA_BYTES)
            .await
            .unwrap_or_default();
        Err(LlmError::HttpError {
            status: status.as_u16(),
            body,
        })
    }

    async fn do_complete(&self, req: CompletionRequest) -> Result<CompletionResponse, LlmError> {
        let started = Instant::now();
        let body = wire::request_body(&req, self.model(), self.config.num_ctx, false);
        let resp = self.send(&body).await?;
        let value: serde_json::Value =
            apollia_core::net::read_capped_json(resp, MAX_RESPONSE_BYTES)
                .await
                .map_err(|e| LlmError::ParseError(format!("ollama response: {e}")))?;
        wire::check_error(&value)?;

        let message = value.get("message").cloned().unwrap_or_default();
        let constrained = req.response_schema.is_some() || req.grammar.is_some();
        let tool_calls = wire::tool_calls(&message, 0);
        Ok(CompletionResponse {
            engine_timings: None,
            content: wire::answer_text(&message, constrained),
            finish_reason: wire::finish_reason(&value, !tool_calls.is_empty()),
            tool_calls,
            usage: wire::usage(&value),
            latency_ms: started.elapsed().as_millis() as u64,
            ttft_ms: None,
        })
    }
}

/// What the streaming unfold carries from one poll to the next.
struct Streaming {
    response: reqwest::Response,
    buffer: Vec<u8>,
    state: wire::StreamState,
    pending: std::collections::VecDeque<Result<StreamChunk, LlmError>>,
    ended: bool,
}

impl Streaming {
    /// Move every complete line in the buffer into `pending`.
    fn drain_lines(&mut self) {
        while let Some(pos) = self.buffer.iter().position(|b| *b == b'\n') {
            let line: Vec<u8> = self.buffer.drain(..=pos).collect();
            let text = String::from_utf8_lossy(&line);
            match self.state.line(&text) {
                Ok(chunks) => self.pending.extend(chunks.into_iter().map(Ok)),
                Err(e) => {
                    self.pending.push_back(Err(e));
                    self.ended = true;
                    return;
                }
            }
        }
    }
}

async fn next_item(mut s: Streaming) -> Option<(Result<StreamChunk, LlmError>, Streaming)> {
    loop {
        if let Some(item) = s.pending.pop_front() {
            return Some((item, s));
        }
        if s.ended || s.state.done {
            return None;
        }
        // SAFETY: a token stream, not a body to buffer. Each chunk is split into
        // NDJSON lines and handed on as it arrives, the same way the OpenAI
        // client consumes SSE; only an unterminated line is held, and a
        // generation ends when the server sends its final line.
        match s.response.chunk().await {
            Ok(Some(bytes)) => {
                s.buffer.extend_from_slice(&bytes);
                s.drain_lines();
            }
            Ok(None) => {
                s.ended = true;
                if !s.buffer.is_empty() {
                    s.buffer.push(b'\n');
                    s.drain_lines();
                }
                s.pending.extend(s.state.finish().into_iter().map(Ok));
            }
            Err(e) => {
                s.ended = true;
                s.pending
                    .push_back(Err(LlmError::InferenceError(format!("ollama stream: {e}"))));
            }
        }
    }
}

#[async_trait::async_trait]
impl CompletionModel for OllamaClient {
    async fn complete(&self, req: CompletionRequest) -> Result<CompletionResponse, LlmError> {
        self.retry_policy
            .execute(self.cancel.clone(), || {
                let req = req.clone();
                async move { self.do_complete(req).await }
            })
            .await
    }

    async fn stream(
        &self,
        req: CompletionRequest,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<StreamChunk, LlmError>> + Send>>, LlmError> {
        let body = wire::request_body(&req, self.model(), self.config.num_ctx, true);
        let response = self.send(&body).await?;
        let state = Streaming {
            response,
            buffer: Vec::new(),
            state: wire::StreamState::default(),
            pending: std::collections::VecDeque::new(),
            ended: false,
        };
        Ok(Box::pin(futures::stream::unfold(state, next_item)))
    }

    fn is_available(&self) -> bool {
        true
    }

    fn backend_name(&self) -> &str {
        &self.config.name
    }

    fn model_id(&self) -> &str {
        &self.config.model
    }

    fn context_window(&self) -> Option<usize> {
        // The window this client asks for on every call, which Ollama honours,
        // so compaction is sized against what the model actually holds.
        Some(self.config.num_ctx as usize)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_stored_openai_path_is_trimmed_to_the_native_root() {
        // GIVEN base URLs as backends were configured before this client
        // WHEN the native root is derived
        // THEN the `/v1` suffix and a trailing slash are dropped
        assert_eq!(
            OllamaConfig::root_of("http://127.0.0.1:11434/v1"),
            "http://127.0.0.1:11434"
        );
        assert_eq!(
            OllamaConfig::root_of("http://127.0.0.1:11434/v1/"),
            "http://127.0.0.1:11434"
        );
        assert_eq!(
            OllamaConfig::root_of("http://gpu-box:11434"),
            "http://gpu-box:11434"
        );
    }

    #[tokio::test]
    async fn a_configured_window_is_kept_when_the_server_cannot_say_otherwise() {
        // GIVEN an operator-configured window and a server that cannot answer
        // WHEN the window is chosen
        let n = OllamaClient::num_ctx("http://127.0.0.1:1", "qwen3.5", Some(16_384)).await;

        // THEN the configured value is used as is
        assert_eq!(n, 16_384);
    }

    #[tokio::test]
    async fn an_unreachable_server_leaves_the_default_window() {
        // GIVEN no configured window and a server that cannot be asked
        // WHEN the window is chosen
        let n = OllamaClient::num_ctx("http://127.0.0.1:1", "qwen3.5", None).await;

        // THEN the default applies, never Ollama's own 4096
        assert_eq!(n, DEFAULT_NUM_CTX);
    }
}
