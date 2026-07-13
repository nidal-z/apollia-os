//! `RunnerLlmBackend`: adapts `CompletionModel` (apollia-llm) onto the
//! [`RunnerProxy`] via HTTP/JSON IPC.

use std::pin::Pin;
use std::sync::Arc;
use std::time::Instant;

use apollia_llm::model_defaults::{resolve, ModelDefaults, ModelHints, UserOverrides};
use apollia_llm::types::{
    ChatMessage, CompletionModel, CompletionRequest, CompletionResponse, FinishReason,
    MessageContent, Role, StreamChunk, TokenUsage, ToolCall,
};
use apollia_llm::LlmError;
use futures::{Stream, StreamExt};
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
    /// Serialise concurrent first-use so only ONE `load_model` is ever in
    /// flight. Without this, several callers (chat + agents sharing this
    /// backend) pass the `loaded` check and POST `load_model` in parallel, which
    /// deadlocks the runner during model load.
    load_lock: tokio::sync::Mutex<()>,
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
    /// Number of persistent inference slots to request from the runner. Each
    /// slot owns a full KV cache and serves one request at a time, so `> 1`
    /// lets independent concurrent requests (batch workloads, parallel agents)
    /// run in parallel instead of serializing. The runner degrades gracefully to
    /// fewer slots when memory is too tight for the full count.
    slot_count: u32,
    /// KV cache data type to request (e.g. `"q8_0"`), or `None` for the default
    /// `f16`. Quantizing shrinks the KV cache footprint (larger context / more
    /// concurrent sequences). Read from the backend's `config_json`.
    kv_cache_type: Option<String>,
}

/// Expand a leading `~/` to `$HOME/` so the runner (which requires an absolute
/// path) accepts model paths stored with a tilde. Idempotent: a path without a
/// leading `~/` is returned unchanged.
fn expand_home(path: &str) -> String {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Ok(home) = std::env::var("HOME") {
            return format!("{home}/{rest}");
        }
    }
    path.to_string()
}

impl RunnerLlmBackend {
    // Constructor takes each collaborator explicitly; bundling them would only
    // move the argument list into a struct literal at every call site.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        proxy: RunnerProxy,
        backend_name: String,
        model_id: String,
        model_path: String,
        slot_count: u32,
        kv_cache_type: Option<String>,
    ) -> Arc<Self> {
        // Le runner exige un chemin absolu. Les configs stockent souvent le
        // modele avec un tilde (~/.apollia/models/...): on l'expanse ici, seul
        // point de passage de tous les appelants (chat comme taches).
        let model_path = expand_home(&model_path);
        Arc::new(Self {
            proxy,
            backend_name,
            model_id,
            model_path,
            loaded: std::sync::Mutex::new(false),
            load_lock: tokio::sync::Mutex::new(()),
            defaults: std::sync::OnceLock::new(),
            n_ctx_train: std::sync::OnceLock::new(),
            effective_ctx: std::sync::OnceLock::new(),
            slot_count: slot_count.max(1),
            kv_cache_type,
        })
    }

    /// Load the model on the runner if not already done. Idempotent, and safe
    /// under concurrency: only one `load_model` is ever posted, others wait.
    async fn ensure_loaded(&self) -> Result<(), LlmError> {
        // Chemin rapide: deja charge, aucune synchronisation necessaire.
        if *self.loaded.lock().expect("loaded-state mutex poisoned") {
            return Ok(());
        }

        // Serialiser le premier chargement: un seul POST load_model en vol. Les
        // appelants concurrents attendent ici puis retrouvent le flag `loaded`.
        let _load_guard = self.load_lock.lock().await;

        // Re-verifier apres acquisition: un autre appelant a pu charger pendant
        // l'attente du verrou.
        if *self.loaded.lock().expect("loaded-state mutex poisoned") {
            return Ok(());
        }

        let params = serde_json::json!({
            "model_id": self.model_id,
            "model_path": self.model_path,
            // Cap the working window rather than using the model's full trained
            // window: some models advertise a huge window (e.g. Ministral's 262144)
            // whose KV cache is tens of GB per context, enough to exceed the GPU
            // working-set and hard-abort the runner at load. 32768 is generous for
            // chat/agent use; the runner caps at min(model_window, this), so smaller
            // models are unaffected. Operators raise it via `config_json` if needed.
            "n_ctx": 32_768,
            "n_gpu_layers": -1,
            "use_mmap": true,
            "use_mlock": false,
            // Persistent inference slots (each with its own KV cache and prompt
            // prefix reuse). `> 1` lets concurrent requests run in parallel; the
            // runner degrades to fewer slots when memory is tight.
            "slot_count": self.slot_count,
            // Absent -> the runner keeps `f16` (default). Quantized KV is opt-in.
            "kv_cache_type": self.kv_cache_type,
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
                            id: tc
                                .get("id")
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .to_string(),
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
        let repeat_penalty = defaults.and_then(|d| d.repetition_penalty).unwrap_or(1.1);

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
        self.ensure_loaded().await?;

        let params = self.complete_params(&req);
        let resp = self
            .proxy
            .post_sse("/llm/stream", params)
            .await
            .map_err(|e| LlmError::InferenceError(format!("runner /llm/stream: {e}")))?;

        // Parse the `text/event-stream` body incrementally: buffer raw bytes,
        // split on the SSE event boundary (blank line), and decode each complete
        // block. Decoding only complete blocks avoids corrupting a multi-byte
        // char split across TCP chunks (the boundary is always ASCII `\n\n`).
        let body = Box::pin(resp.bytes_stream());
        let init = (
            body,
            Vec::<u8>::new(),
            std::collections::VecDeque::<Result<StreamChunk, LlmError>>::new(),
            false,
        );
        let stream = futures::stream::unfold(
            init,
            |(mut body, mut buf, mut queue, mut done)| async move {
                loop {
                    if let Some(item) = queue.pop_front() {
                        return Some((item, (body, buf, queue, done)));
                    }
                    if done {
                        return None;
                    }
                    match body.next().await {
                        Some(Ok(bytes)) => {
                            buf.extend_from_slice(&bytes);
                            while let Some(pos) = buf.windows(2).position(|w| w == b"\n\n") {
                                let block: Vec<u8> = buf.drain(..pos + 2).collect();
                                let text = String::from_utf8_lossy(&block);
                                for item in parse_sse_block(&text) {
                                    queue.push_back(item);
                                }
                            }
                        }
                        Some(Err(e)) => {
                            done = true;
                            queue.push_back(Err(LlmError::InferenceError(format!(
                                "runner stream body: {e}"
                            ))));
                        }
                        None => {
                            done = true;
                            if !buf.is_empty() {
                                let text = String::from_utf8_lossy(&buf);
                                if !text.trim().is_empty() {
                                    for item in parse_sse_block(&text) {
                                        queue.push_back(item);
                                    }
                                }
                            }
                        }
                    }
                }
            },
        );
        Ok(Box::pin(stream))
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

    /// Local sidecar backend: GBNF grammars are applied at decode time.
    fn is_local(&self) -> bool {
        true
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

/// Parse one SSE event block into zero or more stream items.
///
/// A block is `field: value` lines terminated by a blank line. The runner sends,
/// per event, an envelope `{request_id, ok, chunk:{text, tool_calls, finish_reason}}`
/// on the default/`done` event, or `{ok:false, error}` on the `error` event.
/// Incremental chunks carry clean text; the final (`done`) chunk carries the
/// structured tool calls. Comment lines (`:` keep-alives) and unknown fields are
/// ignored, yielding no items.
fn parse_sse_block(block: &str) -> Vec<Result<StreamChunk, LlmError>> {
    let mut event_name: Option<&str> = None;
    let mut data = String::new();
    for line in block.lines() {
        if let Some(rest) = line.strip_prefix("data:") {
            data.push_str(rest.strip_prefix(' ').unwrap_or(rest));
        } else if let Some(rest) = line.strip_prefix("event:") {
            event_name = Some(rest.trim());
        }
    }
    if data.is_empty() {
        return Vec::new();
    }

    let value: Value = match serde_json::from_str(&data) {
        Ok(v) => v,
        Err(e) => {
            return vec![Err(LlmError::InferenceError(format!(
                "runner stream: invalid SSE payload: {e}"
            )))]
        }
    };

    let ok = value.get("ok").and_then(Value::as_bool).unwrap_or(true);
    if event_name == Some("error") || !ok {
        let message = value
            .get("error")
            .and_then(|e| e.get("message"))
            .and_then(Value::as_str)
            .unwrap_or("stream error")
            .to_string();
        return vec![Err(LlmError::InferenceError(format!(
            "runner stream: {message}"
        )))];
    }

    let Some(chunk) = value.get("chunk") else {
        return Vec::new();
    };

    let mut items: Vec<Result<StreamChunk, LlmError>> = Vec::new();
    if let Some(text) = chunk.get("text").and_then(Value::as_str) {
        if !text.is_empty() {
            items.push(Ok(StreamChunk::Text(text.to_string())));
        }
    }
    // Tool calls arrive only on the final chunk; map them to structured items so
    // the consumer (chat agent, SDK) executes them exactly as on the complete path.
    for call in RunnerLlmBackend::parse_tool_calls(chunk) {
        items.push(Ok(StreamChunk::ToolCall(call)));
    }
    items
}

#[cfg(test)]
mod tests {
    use super::*;
    use apollia_llm::types::ToolSpec;

    #[test]
    fn expand_home_resolves_leading_tilde() {
        // GIVEN a HOME and a tilde-prefixed model path
        std::env::set_var("HOME", "/home/apollia");

        // WHEN expanding the tilde
        let out = expand_home("~/.apollia/models/model.gguf");

        // THEN it becomes an absolute path under HOME
        assert_eq!(out, "/home/apollia/.apollia/models/model.gguf");
    }

    #[test]
    fn expand_home_leaves_absolute_path_unchanged() {
        // GIVEN an already-absolute path
        let abs = "/var/models/model.gguf";

        // WHEN expanding THEN it is returned verbatim (idempotent)
        assert_eq!(expand_home(abs), abs);
    }

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
    fn parse_sse_block_text_chunk_yields_text() {
        // GIVEN an incremental SSE data line carrying clean text
        let block = "data: {\"request_id\":\"r\",\"ok\":true,\"chunk\":{\"text\":\"Hello\"}}\n";
        // WHEN parsing the block
        let items = parse_sse_block(block);
        // THEN a single Text item is produced
        assert_eq!(items.len(), 1);
        match items.into_iter().next().expect("one item") {
            Ok(StreamChunk::Text(t)) => assert_eq!(t, "Hello"),
            _ => panic!("expected a Text chunk"),
        }
    }

    #[test]
    fn parse_sse_block_done_chunk_yields_tool_calls() {
        // GIVEN the final `done` event carrying structured tool calls
        let block = "event: done\ndata: {\"ok\":true,\"chunk\":{\"text\":\"\",\"tool_calls\":[{\"id\":\"c1\",\"name\":\"f\",\"arguments\":{\"x\":1}}],\"finish_reason\":\"eos\"}}\n";
        // WHEN parsing the block
        let items = parse_sse_block(block);
        // THEN the structured call surfaces as a ToolCall item (no leaked text)
        assert_eq!(items.len(), 1);
        match items.into_iter().next().expect("one item") {
            Ok(StreamChunk::ToolCall(c)) => {
                assert_eq!(c.id, "c1");
                assert_eq!(c.name, "f");
            }
            _ => panic!("expected a ToolCall chunk"),
        }
    }

    #[test]
    fn parse_sse_block_error_event_yields_err() {
        // GIVEN an SSE error event
        let block =
            "event: error\ndata: {\"ok\":false,\"error\":{\"code\":\"X\",\"message\":\"boom\"}}\n";
        // WHEN parsing
        let items = parse_sse_block(block);
        // THEN a single Err item is produced
        assert_eq!(items.len(), 1);
        assert!(items.into_iter().next().expect("one item").is_err());
    }

    #[test]
    fn parse_sse_block_keepalive_is_empty() {
        // GIVEN a keep-alive comment block with no data line
        // WHEN parsing
        // THEN nothing is emitted
        assert!(parse_sse_block(": keep-alive\n").is_empty());
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
            1,
            None,
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
