//! `RunnerLlmBackend`: adapts `CompletionModel` (apollia-llm) onto the
//! [`RunnerProxy`] via HTTP/JSON IPC.

use std::pin::Pin;
use std::sync::Arc;
use std::time::Instant;

use apollia_llm::types::{
    ChatMessage, CompletionModel, CompletionRequest, CompletionResponse, FinishReason,
    MessageContent, Role, StreamChunk, TokenUsage, ToolCall,
};
use apollia_llm::model_defaults::{resolve, ModelDefaults, ModelHints, UserOverrides};
use apollia_llm::LlmError;
use futures::Stream;
use serde_json::Value;

use super::proxy::RunnerProxy;

/// `CompletionModel` backend that routes calls to the runner sidecar.
///
/// Instantiated by the `Supervisor` at boot when a `LlamaCpp` provider
/// `LlmBackendConfig` is found in the DB AND a runner is available.
pub struct RunnerLlmBackend {
    proxy: RunnerProxy,
    backend_name: String,
    model_id: String,
    model_path: String,
    /// True after the first successful `load_model` on the runner.
    loaded: std::sync::Mutex<bool>,
    /// Per-model sampling defaults, resolved once at load time from the model
    /// family (user override file then embedded table). Empty until loaded.
    defaults: std::sync::OnceLock<ModelDefaults>,
    /// Model's trained context window (tokens), cached from `load_model`.
    /// Drives the router's model-aware context limit for compaction. Unset
    /// (and `0` is treated as unknown) until the first successful load.
    n_ctx_train: std::sync::OnceLock<u32>,
    /// Effective working window each slot was actually allocated with. The
    /// runner rejects prompts beyond it, so compaction must size to THIS rather
    /// than `n_ctx_train` (which may be larger than what fit in memory).
    effective_ctx: std::sync::OnceLock<u32>,
}

impl RunnerLlmBackend {
    pub fn new(
        proxy: RunnerProxy,
        backend_name: String,
        model_id: String,
        model_path: String,
    ) -> Arc<Self> {
        Arc::new(Self {
            proxy,
            backend_name,
            model_id,
            model_path,
            loaded: std::sync::Mutex::new(false),
            defaults: std::sync::OnceLock::new(),
            n_ctx_train: std::sync::OnceLock::new(),
            effective_ctx: std::sync::OnceLock::new(),
        })
    }

    /// Load the model on the runner if not already done. Idempotent.
    async fn ensure_loaded(&self) -> Result<(), LlmError> {
        {
            let guard = self.loaded.lock().expect("loaded-state mutex poisoned");
            if *guard {
                return Ok(());
            }
        }

        let params = serde_json::json!({
            "model_id": self.model_id,
            "model_path": self.model_path,
            // `0` = use the model's full trained window (the runner sizes each
            // slot to it, shrinking adaptively only if memory is tight).
            "n_ctx": 0,
            "n_gpu_layers": -1,
            "use_mmap": true,
            "use_mlock": false,
            // One persistent inference slot: reuses the prompt prefix across
            // turns. Concurrent calls to this model serialize on the slot.
            "slot_count": 1,
        });

        let data: Value = self
            .proxy
            .post_json("/llm/load_model", params)
            .await
            .map_err(|e| LlmError::BackendUnavailable {
                backend: self.backend_name.clone(),
                reason: format!("load_model via runner: {e}"),
            })?;

        let arch = data.get("arch").and_then(|v| v.as_str()).unwrap_or("");
        let _ = self.defaults.set(self.resolve_defaults(arch));

        // Cache the model's trained window and the slot's effective window so
        // the router can size compaction to what the runner actually allocated.
        if let Some(train) = data.get("n_ctx_train").and_then(|v| v.as_u64()) {
            if train > 0 {
                let _ = self.n_ctx_train.set(train as u32);
            }
        }
        if let Some(eff) = data.get("effective_ctx").and_then(|v| v.as_u64()) {
            if eff > 0 {
                let _ = self.effective_ctx.set(eff as u32);
            }
        }

        *self.loaded.lock().expect("loaded-state mutex poisoned") = true;
        Ok(())
    }

    /// Resolves per-model sampling defaults through the shared `model_defaults`
    /// resolver: user override file (`sampling-defaults.json`) first, then the
    /// embedded per-family table, keyed by the GGUF `arch` and file name.
    fn resolve_defaults(&self, arch: &str) -> ModelDefaults {
        let file_name = std::path::Path::new(&self.model_path)
            .file_name()
            .and_then(|n| n.to_str());
        let hints = ModelHints {
            arch: if arch.is_empty() { None } else { Some(arch) },
            name: file_name,
            file_name,
            model_id: Some(&self.model_id),
            ..Default::default()
        };
        let overrides = UserOverrides::load(&UserOverrides::default_path()).unwrap_or_default();
        let resolved = resolve(&hints, &overrides);
        tracing::info!(
            backend = %self.backend_name,
            arch = %arch,
            temperature = ?resolved.temperature,
            top_p = ?resolved.top_p,
            top_k = ?resolved.top_k,
            repetition_penalty = ?resolved.repetition_penalty,
            "runner sampling defaults resolved"
        );
        resolved
    }

    fn map_messages(messages: &[ChatMessage]) -> Vec<Value> {
        messages
            .iter()
            .map(|m| {
                let role = match m.role {
                    Role::System => "system",
                    Role::User => "user",
                    Role::Assistant => "assistant",
                    // Tool results: not yet wired through IPC (Phase 2 limitation),
                    // serialize as a user message with the textual content.
                    Role::Tool => "user",
                };
                let content = match &m.content {
                    MessageContent::Text(t) => t.clone(),
                    MessageContent::WithToolCalls { text, .. } => text.clone(),
                    MessageContent::ToolResult { content, .. } => content.clone(),
                };
                serde_json::json!({
                    "role": role,
                    "content": content,
                })
            })
            .collect()
    }

    /// Serializes the request's tools into the OpenAI `tools` array shape the
    /// runner forwards to the model's chat template. Returns `None` when the
    /// request carries no tools, which disables tool rendering for that call.
    fn map_tools(req: &CompletionRequest) -> Option<Value> {
        if req.tools.is_empty() {
            return None;
        }
        Some(Value::Array(
            req.tools
                .iter()
                .map(|t| {
                    serde_json::json!({
                        "type": "function",
                        "function": {
                            "name": t.name,
                            "description": t.description,
                            "parameters": t.parameters,
                        }
                    })
                })
                .collect(),
        ))
    }

    /// Maps the runner's `tool_calls` payload into `apollia-llm` tool calls.
    ///
    /// The runner already decoded the model output through the GGUF's own chat
    /// template parser, so this is a straight shape conversion. Entries missing
    /// a name are skipped rather than failing the whole response.
    fn parse_tool_calls(data: &Value) -> Vec<ToolCall> {
        data.get("tool_calls")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|tc| {
                        Some(ToolCall {
                            id: tc.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                            name: tc.get("name").and_then(|v| v.as_str())?.to_string(),
                            arguments: tc
                                .get("arguments")
                                .cloned()
                                .unwrap_or_else(|| serde_json::json!({})),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    fn complete_params(&self, req: &CompletionRequest) -> Value {
        // Precedence per field: explicit request value, then the resolved
        // per-model default, then a hard global fallback.
        let defaults = self.defaults.get();
        let temperature = req
            .temperature
            .or_else(|| defaults.and_then(|d| d.temperature))
            .unwrap_or(0.7);
        let top_p = defaults.and_then(|d| d.top_p).unwrap_or(0.95);
        let top_k = defaults.and_then(|d| d.top_k).unwrap_or(40);
        let repeat_penalty = defaults
            .and_then(|d| d.repetition_penalty)
            .unwrap_or(1.1);

        serde_json::json!({
            "model_id": self.model_id,
            "messages": Self::map_messages(&req.messages),
            // `0` = "fill the model's remaining context window" (resolved by the
            // runner after tokenization). A caller that does not set max_tokens
            // gets the full window instead of an arbitrary truncation.
            "max_tokens": req.max_tokens.unwrap_or(0),
            "temperature": temperature,
            "top_p": top_p,
            "top_k": top_k,
            "repeat_penalty": repeat_penalty,
            "seed": req.seed,
            "stop": Vec::<String>::new(),
            "tools": Self::map_tools(req),
            // GBNF grammar for constrained decoding; `None` is omitted-equivalent
            // (CompleteParams.grammar defaults to None). Cloud backends ignore it.
            "grammar": req.grammar,
        })
    }
}

#[async_trait::async_trait]
impl CompletionModel for RunnerLlmBackend {
    async fn complete(&self, req: CompletionRequest) -> Result<CompletionResponse, LlmError> {
        self.ensure_loaded().await?;

        let started = Instant::now();
        let params = self.complete_params(&req);

        let data: Value = self
            .proxy
            .post_json("/llm/complete", params)
            .await
            .map_err(|e| LlmError::InferenceError(format!("runner /llm/complete: {e}")))?;

        let text = data
            .get("text")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let usage = data
            .get("usage")
            .and_then(|u| {
                Some(TokenUsage {
                    prompt_tokens: u.get("prompt_tokens")?.as_u64()? as u32,
                    completion_tokens: u.get("completion_tokens")?.as_u64()? as u32,
                    cost_usd: None,
                    cache_read_input_tokens: 0,
                    cache_write_input_tokens: 0,
                })
            })
            .unwrap_or_default();

        let tool_calls = Self::parse_tool_calls(&data);

        // Tool calls take precedence: the agent loop branches on them before
        // the textual finish reason.
        let finish_reason = if !tool_calls.is_empty() {
            FinishReason::ToolCalls
        } else {
            match data
                .get("finish_reason")
                .and_then(|v| v.as_str())
                .unwrap_or("stop")
            {
                "length" => FinishReason::Length,
                _ => FinishReason::Stop,
            }
        };

        Ok(CompletionResponse {
            content: text,
            tool_calls,
            usage,
            finish_reason,
            latency_ms: started.elapsed().as_millis() as u64,
            ttft_ms: None,
        })
    }

    async fn stream(
        &self,
        req: CompletionRequest,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<StreamChunk, LlmError>> + Send>>, LlmError> {
        // Minimal: delegate to complete() and replay the result as a short
        // chunk sequence (text, then one chunk per tool call). Real SSE
        // streaming is wired up later (parse the text/event-stream live).
        let resp = self.complete(req).await?;
        let mut chunks: Vec<Result<StreamChunk, LlmError>> = Vec::new();
        if !resp.content.is_empty() {
            chunks.push(Ok(StreamChunk::Text(resp.content)));
        }
        for call in resp.tool_calls {
            chunks.push(Ok(StreamChunk::ToolCall(call)));
        }
        Ok(Box::pin(futures::stream::iter(chunks)))
    }

    fn is_available(&self) -> bool {
        true
    }

    fn backend_name(&self) -> &str {
        &self.backend_name
    }

    fn model_id(&self) -> &str {
        &self.model_id
    }

    fn context_window(&self) -> Option<usize> {
        // Prefer the slot's effective window (what the runner will accept);
        // fall back to the trained window before the first load reports back.
        self.effective_ctx
            .get()
            .or_else(|| self.n_ctx_train.get())
            .map(|&n| n as usize)
    }

    fn count_tokens(&self, messages: &[ChatMessage]) -> usize {
        // Concatenate role-prefixed text for a faithful tokenizer estimate.
        let text: String = messages
            .iter()
            .map(|m| {
                let role = match m.role {
                    Role::System => "system",
                    Role::User => "user",
                    Role::Assistant => "assistant",
                    Role::Tool => "tool",
                };
                let content = match &m.content {
                    MessageContent::Text(s) => s.as_str(),
                    MessageContent::ToolResult { content, .. } => content.as_str(),
                    MessageContent::WithToolCalls { text, .. } => text.as_str(),
                };
                format!("{role}: {content}\n")
            })
            .collect();

        match self.proxy.tokenize_blocking(&self.model_id, &text) {
            Ok(count) => count as usize,
            Err(e) => {
                tracing::warn!(
                    model_id = %self.model_id,
                    error = %e,
                    "tokenize.ipc_failed_fallback_proxy"
                );
                // Proxy identical to the CompletionModel::count_tokens default.
                ((text.len() as f32) / 4.0 * 1.2) as usize
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use apollia_llm::types::ToolSpec;

    #[test]
    fn parse_tool_calls_maps_runner_payload() {
        // GIVEN a runner response carrying one structured tool call
        let data = serde_json::json!({
            "text": "",
            "tool_calls": [
                { "id": "x1", "name": "list_files", "arguments": { "path": "/tmp" } }
            ]
        });

        // WHEN parsing the tool calls
        let calls = RunnerLlmBackend::parse_tool_calls(&data);

        // THEN the shape is carried through verbatim
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].id, "x1");
        assert_eq!(calls[0].name, "list_files");
        assert_eq!(calls[0].arguments, serde_json::json!({ "path": "/tmp" }));
    }

    #[test]
    fn parse_tool_calls_empty_when_absent() {
        // GIVEN a response with no tool_calls field
        let data = serde_json::json!({ "text": "hello" });

        // WHEN parsing
        let calls = RunnerLlmBackend::parse_tool_calls(&data);

        // THEN none are produced
        assert!(calls.is_empty());
    }

    #[test]
    fn map_tools_returns_none_when_request_has_no_tools() {
        // GIVEN a request with an empty tool list
        let req = CompletionRequest::default();

        // WHEN mapping its tools to the runner payload
        let mapped = RunnerLlmBackend::map_tools(&req);

        // THEN no tools array is sent, leaving the template in tool-free mode
        assert!(mapped.is_none());
    }

    #[test]
    fn map_tools_emits_openai_function_shape() {
        // GIVEN a request carrying one tool spec
        let req = CompletionRequest {
            tools: vec![ToolSpec {
                name: "list_files".to_string(),
                description: "List files in a directory.".to_string(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": { "path": { "type": "string" } },
                    "required": ["path"]
                }),
            }],
            ..Default::default()
        };

        // WHEN mapping its tools
        let mapped = RunnerLlmBackend::map_tools(&req).expect("tools must be present");

        // THEN the OpenAI `{type: function, function: {...}}` shape is produced
        let expected = serde_json::json!([{
            "type": "function",
            "function": {
                "name": "list_files",
                "description": "List files in a directory.",
                "parameters": {
                    "type": "object",
                    "properties": { "path": { "type": "string" } },
                    "required": ["path"]
                }
            }
        }]);
        assert_eq!(mapped, expected);
    }

    /// A backend whose runner sidecar is not started (proxy holds `None`).
    fn unconnected_backend() -> Arc<RunnerLlmBackend> {
        let proxy = RunnerProxy::new(Arc::new(tokio::sync::RwLock::new(None)));
        RunnerLlmBackend::new(
            proxy,
            "local".to_string(),
            "test-model".to_string(),
            "/tmp/model.gguf".to_string(),
        )
    }

    // GIVEN a RunnerLlmBackend whose runner sidecar is unreachable
    // WHEN count_tokens is called
    // THEN the chars/4 proxy is returned (> 0) without panic or error
    #[test]
    fn count_tokens_falls_back_to_proxy_when_runner_absent() {
        let backend = unconnected_backend();
        let messages = vec![ChatMessage::user("hello world")];
        let count = backend.count_tokens(&messages);
        assert!(count > 0);
    }

    // GIVEN an unconnected backend and a multi-message history
    // WHEN count_tokens is called
    // THEN the result is strictly positive (fallback proxy invariant)
    #[test]
    fn count_tokens_nonzero_for_nonempty_messages() {
        let backend = unconnected_backend();
        let messages = vec![
            ChatMessage::system("you are helpful"),
            ChatMessage::user("bonjour"),
        ];
        assert!(backend.count_tokens(&messages) > 0);
    }
}
