//! llama-cpp-2 wrapper for the runner.
//!
//! Simplified scope for the current version:
//!
//! - Single-file `.gguf` models only (custom-split FFI loading is not wired).
//! - Tooling and GBNF grammars are not ported; not required by the IPC protocol.
//! - No embeddings: returns `UnsupportedOperation` (to be wired when a local
//!   embeddings backend is added).
//!
//! Implements IPC protocol v1 (`/llm/load_model`, `/llm/unload_model`,
//! `/llm/complete`, `/llm/stream`).

mod slot;

use std::collections::HashMap;
use std::num::NonZeroU32;
use std::path::Path;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use futures::Stream;
use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::model::{
    AddBos, ChatTemplateResult, LlamaChatMessage, LlamaChatTemplate, LlamaModel,
};
use llama_cpp_2::sampling::LlamaSampler;

use crate::ipc::{
    ChatMessage, CompleteData, CompleteParams, ErrorBody, ErrorCode, LoadModelData, LoadModelParams,
    Role, StreamChunk, Timing, TokenUsage, ToolCall,
};

use slot::{conversation_fingerprint, InferenceRequest, SlotJob, SlotPool};

/// Sampling defaults applied when the request leaves the corresponding fields
/// at their zero value.
const DEFAULT_TOP_P: f32 = 0.95;
const DEFAULT_TOP_K: i32 = 40;
/// Lower bound on `n_ctx` so a short prompt still has working headroom.
const MIN_CTX_SIZE: u32 = 1024;
/// Upper bound on the number of persistent inference slots per model.
const MAX_SLOTS: u32 = 8;
/// Working window used when the GGUF reports no trained context and no cap.
const DEFAULT_CTX_FALLBACK: u32 = 8192;

/// Entry for a model loaded into VRAM/RAM.
///
/// Holds no `LlamaModel`/`LlamaContext` directly: those live inside the slot
/// threads ([`SlotPool`]), each owning its own persistent context and a shared
/// `Arc<LlamaModel>`. This struct is therefore plain `Send + Sync` with no
/// `unsafe` impl: every llama.cpp handle is confined to a slot thread.
struct LoadedModel {
    /// Effective context window each slot was created with (tokens).
    context_size: u32,
    /// Model's trained context window (GGUF `<arch>.context_length`), `0` if absent.
    n_ctx_train: u32,
    /// Rough estimate of VRAM used (file size in MiB).
    memory_used_mb: u32,
    /// Persistent inference slots holding the KV caches for this model.
    slots: SlotPool,
}

/// In-process inference backend backed by llama.cpp.
///
/// Holds a `model_id -> LoadedModel` cache shared across the axum handlers via
/// an `Arc`. The global `LlamaBackend` is initialized lazily the first time a
/// model is loaded.
pub struct LlamaCppBackend {
    /// Cache of currently loaded models, keyed by `model_id`.
    models: Mutex<HashMap<String, Arc<LoadedModel>>>,
}

impl Default for LlamaCppBackend {
    fn default() -> Self {
        Self::new()
    }
}

/// Global singleton for `LlamaBackend`: `LlamaBackend::init()` may only be
/// called once per process (internal atomic).
static LLAMA_BACKEND: Mutex<Option<Arc<LlamaBackend>>> = Mutex::new(None);

/// Acquires or initializes the global `LlamaBackend` (call from a blocking
/// thread only: `LlamaBackend::init()` is not async-safe).
fn acquire_backend() -> Result<Arc<LlamaBackend>, ErrorBody> {
    let mut guard = LLAMA_BACKEND
        .lock()
        .map_err(|e| internal(format!("LLAMA_BACKEND poisoned: {e}")))?;
    if let Some(b) = guard.as_ref() {
        return Ok(b.clone());
    }
    let backend = Arc::new(
        LlamaBackend::init()
            .map_err(|e| internal(format!("llama backend init failed: {e}")))?,
    );
    *guard = Some(backend.clone());
    Ok(backend)
}

fn internal(msg: impl Into<String>) -> ErrorBody {
    ErrorBody::new(ErrorCode::Internal, msg)
}

fn load_failed(msg: impl Into<String>) -> ErrorBody {
    ErrorBody::new(ErrorCode::ModelLoadFailed, msg)
}

fn inference_failed(msg: impl Into<String>) -> ErrorBody {
    ErrorBody::new(ErrorCode::InferenceFailed, msg)
}

impl LlamaCppBackend {
    /// Creates an empty backend: no model is loaded until `/llm/load_model`
    /// is called.
    pub fn new() -> Self {
        Self {
            models: Mutex::new(HashMap::new()),
        }
    }

    /// Lists the currently loaded `model_id`s (useful for `/health`).
    pub fn loaded_ids(&self) -> Vec<String> {
        let guard = self.models.lock().expect("llama models lock poisoned");
        guard.keys().cloned().collect()
    }

    /// Total memory (MiB) reported by the loaded models.
    pub fn total_memory_mb(&self) -> u32 {
        let guard = self.models.lock().expect("llama models lock poisoned");
        guard.values().map(|m| m.memory_used_mb).sum()
    }

    /// Unloads a model. Returns `true` if it was loaded.
    pub fn unload(&self, model_id: &str) -> bool {
        let mut guard = self.models.lock().expect("llama models lock poisoned");
        guard.remove(model_id).is_some()
    }

    fn get_model(&self, model_id: &str) -> Option<Arc<LoadedModel>> {
        let guard = self.models.lock().expect("llama models lock poisoned");
        guard.get(model_id).cloned()
    }

    /// Loads a GGUF file from disk (`POST /llm/load_model`).
    ///
    /// Loading runs inside a `spawn_blocking` because
    /// `LlamaModel::load_from_file` is synchronous and expensive (mmap + parse).
    pub async fn load_model(&self, params: LoadModelParams) -> Result<LoadModelData, ErrorBody> {
        let started = Instant::now();

        let model_id = params.model_id.clone();
        let model_path = params.model_path.clone();
        // IPC convention: `n_gpu_layers = -1` means "all layers on GPU". Map it
        // to the legacy 999 sentinel used by the embedded backend; any value
        // >= 0 is kept as-is.
        let n_gpu_layers: u32 = if params.n_gpu_layers < 0 {
            999
        } else {
            params.n_gpu_layers as u32
        };
        // `n_ctx` is an optional cap on the working window: `0` means "use the
        // model's full trained window". Slots pre-allocate this much KV.
        let ctx_cap = params.n_ctx;
        let slot_count = params.slot_count.clamp(1, MAX_SLOTS);
        let use_mmap = params.use_mmap;
        let use_mlock = params.use_mlock;

        let memory_used_mb = std::fs::metadata(&model_path)
            .map(|m| (m.len() / (1024 * 1024)) as u32)
            .unwrap_or(0);

        let (loaded, arch) = tokio::task::spawn_blocking(move || -> Result<_, ErrorBody> {
            let backend = acquire_backend()?;

            let mut model_params = LlamaModelParams::default().with_n_gpu_layers(n_gpu_layers);
            model_params = model_params.with_use_mmap(use_mmap);
            model_params = model_params.with_use_mlock(use_mlock);

            let model = LlamaModel::load_from_file(&backend, &model_path, &model_params)
                .map_err(|e| load_failed(format!("model load failed: {e}")))?;
            let arch = model.meta_val_str("general.architecture").unwrap_or_default();
            let n_ctx_train = model.n_ctx_train();

            // Default to the model's full window; an explicit cap shrinks it.
            let model_window = if n_ctx_train > 0 {
                n_ctx_train
            } else if ctx_cap > 0 {
                ctx_cap
            } else {
                DEFAULT_CTX_FALLBACK
            };
            let target = if ctx_cap > 0 {
                model_window.min(ctx_cap)
            } else {
                model_window
            }
            .max(MIN_CTX_SIZE);

            let model = Arc::new(model);
            // Find the largest context that actually allocates (adaptive
            // halving): "the most open window that fits" without failing the load.
            let effective_ctx = probe_context_size(&model, &backend, target)?;
            let slots = SlotPool::spawn(&model, &backend, effective_ctx, slot_count)?;

            Ok::<_, ErrorBody>((
                LoadedModel {
                    context_size: effective_ctx,
                    n_ctx_train,
                    memory_used_mb,
                    slots,
                },
                arch,
            ))
        })
        .await
        .map_err(|e| internal(format!("model load task panicked: {e}")))??;

        let context_size = loaded.context_size;
        let n_ctx_train = loaded.n_ctx_train;

        {
            let mut guard = self.models.lock().expect("llama models lock poisoned");
            guard.insert(model_id.clone(), Arc::new(loaded));
        }

        tracing::info!(
            model_id = %model_id,
            load_time_ms = started.elapsed().as_millis() as u64,
            context_size,
            n_ctx_train,
            slot_count,
            "llama model loaded"
        );

        Ok(LoadModelData {
            model_id,
            load_time_ms: started.elapsed().as_millis() as u64,
            context_size,
            memory_used_mb,
            arch,
            n_ctx_train,
            effective_ctx: context_size,
        })
    }

    /// Non-streaming inference (`POST /llm/complete`).
    pub async fn complete(&self, params: CompleteParams) -> Result<CompleteData, ErrorBody> {
        let entry = self.get_model(&params.model_id).ok_or_else(|| {
            ErrorBody::new(
                ErrorCode::ModelNotLoaded,
                format!("model '{}' not loaded", params.model_id),
            )
        })?;

        let started = Instant::now();
        let (req, fingerprint) = build_request(params);
        let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
        entry
            .slots
            .dispatch(fingerprint, SlotJob::Complete { req, reply: reply_tx })?;
        let out = reply_rx
            .await
            .map_err(|_| inference_failed("slot dropped before replying"))??;

        Ok(CompleteData {
            text: out.text,
            finish_reason: out.finish_reason,
            usage: TokenUsage {
                prompt_tokens: out.prompt_tokens,
                completion_tokens: out.completion_tokens,
                total_tokens: out.prompt_tokens + out.completion_tokens,
            },
            timing: Timing {
                queue_ms: 0,
                prefill_ms: out.prefill_ms,
                decode_ms: out.decode_ms,
                total_ms: started.elapsed().as_millis() as u64,
            },
            tool_calls: out.tool_calls,
        })
    }

    /// Token-by-token streaming (`POST /llm/stream`).
    ///
    /// Returns a `Stream` that yields:
    /// - `Ok(StreamChunk)` for each decoded token (partial text).
    /// - a final `Ok(StreamChunk { finish_reason: Some(...) })` to close.
    /// - `Err(ErrorBody)` if inference fails.
    pub async fn stream(
        &self,
        params: CompleteParams,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<StreamChunk, ErrorBody>> + Send>>, ErrorBody> {
        let entry = self.get_model(&params.model_id).ok_or_else(|| {
            ErrorBody::new(
                ErrorCode::ModelNotLoaded,
                format!("model '{}' not loaded", params.model_id),
            )
        })?;

        let (req, fingerprint) = build_request(params);
        let (tx, rx) = tokio::sync::mpsc::channel::<Result<StreamChunk, ErrorBody>>(32);
        entry
            .slots
            .dispatch(fingerprint, SlotJob::Stream { req, tx })?;

        Ok(Box::pin(tokio_stream::wrappers::ReceiverStream::new(rx)))
    }
}

/// Build the owned [`InferenceRequest`] for a slot and the routing fingerprint.
///
/// Resolves sampling defaults exactly as before (zero `top_p`/`top_k` fall back
/// to the family defaults; `max_tokens == 0` is forwarded as the
/// "full window" sentinel).
fn build_request(params: CompleteParams) -> (InferenceRequest, u64) {
    let fingerprint = conversation_fingerprint(&params.messages);
    let top_p = if params.top_p > 0.0 {
        params.top_p
    } else {
        DEFAULT_TOP_P
    };
    let top_k = if params.top_k > 0 {
        params.top_k as i32
    } else {
        DEFAULT_TOP_K
    };
    let req = InferenceRequest {
        tools_json: params.tools.as_ref().map(|t| t.to_string()),
        messages: params.messages,
        max_tokens: params.max_tokens,
        temperature: params.temperature,
        top_p,
        top_k,
        seed: params.seed,
    };
    (req, fingerprint)
}

/// Find the largest context window that actually allocates, starting at
/// `target` and halving on failure down to [`MIN_CTX_SIZE`].
///
/// Honours "the most open window possible" without failing the load on
/// large-context models that cannot fit their full window in available memory.
/// The probe context is dropped immediately; slots create their own at the
/// returned size.
fn probe_context_size(
    model: &LlamaModel,
    backend: &LlamaBackend,
    target: u32,
) -> Result<u32, ErrorBody> {
    let mut size = target.max(MIN_CTX_SIZE);
    loop {
        let params = LlamaContextParams::default()
            .with_n_ctx(NonZeroU32::new(size))
            .with_n_batch(size);
        match model.new_context(backend, params) {
            Ok(_ctx) => return Ok(size),
            Err(e) => {
                if size <= MIN_CTX_SIZE {
                    return Err(load_failed(format!(
                        "context allocation failed even at minimum {MIN_CTX_SIZE}: {e}"
                    )));
                }
                let next = (size / 2).max(MIN_CTX_SIZE);
                tracing::warn!(attempted = size, next, "context allocation failed, halving");
                size = next;
            }
        }
    }
}

/// Resolve how many tokens to generate for one request, given the slot's fixed
/// context window and the already-tokenized prompt length.
///
/// `max_tokens == 0` means "fill the remaining window": the budget is the slot
/// window minus the prompt. The slot's context is already allocated to this
/// window (memory was bounded once, at slot creation, via the adaptive probe),
/// so generation can safely use all of the remaining space. An explicit
/// `max_tokens` is honoured but capped at the remaining window so generation
/// never overruns the context.
fn resolve_effective_max(model_window: u32, prompt_tokens: u32, max_tokens: u32) -> u32 {
    let remaining = model_window.saturating_sub(prompt_tokens).max(1);
    if max_tokens == 0 {
        remaining
    } else {
        max_tokens.min(remaining)
    }
}

/// Collapses consecutive same-role turns into a single turn.
///
/// Strict chat templates (Mistral, Gemma) raise when the role sequence does not
/// strictly alternate user/assistant after an optional leading system turn.
/// The daemon legitimately produces non-alternating sequences: tool results are
/// folded into `user` turns, so back-to-back tool calls yield consecutive
/// `user` messages, and a context summary is sent as a second `system` turn.
/// Merging adjacent same-role turns (joined by a blank line) keeps the rendered
/// prompt faithful while satisfying the template contract. Lenient templates
/// (ChatML) render the merged sequence identically.
fn normalize_roles(messages: &[ChatMessage]) -> Vec<(&'static str, String)> {
    let mut out: Vec<(&'static str, String)> = Vec::with_capacity(messages.len());
    for m in messages {
        let role = match m.role {
            Role::System => "system",
            Role::User => "user",
            Role::Assistant => "assistant",
        };
        match out.last_mut() {
            Some((prev_role, prev_content)) if *prev_role == role => {
                prev_content.push_str("\n\n");
                prev_content.push_str(&m.content);
            }
            _ => out.push((role, m.content.clone())),
        }
    }
    out
}

/// Builds the formatted prompt using the template embedded in the GGUF
/// (chatml fallback). Returns the full [`ChatTemplateResult`] (the prompt plus
/// the `chat_format` / parser needed to decode tool calls from the response)
/// and the prompt token count.
fn build_prompt(
    model: &LlamaModel,
    messages: &[ChatMessage],
    tools: Option<&str>,
) -> Result<(ChatTemplateResult, u32), ErrorBody> {
    let template = model.chat_template(None).unwrap_or_else(|_| {
        LlamaChatTemplate::new("chatml").expect("chatml template must be valid")
    });

    let chat_msgs: Vec<LlamaChatMessage> = normalize_roles(messages)
        .into_iter()
        .map(|(role, content)| {
            LlamaChatMessage::new(role.to_string(), content)
                .map_err(|e| inference_failed(format!("invalid chat message: {e}")))
        })
        .collect::<Result<Vec<_>, _>>()?;

    let result = model
        .apply_chat_template_with_tools_oaicompat(&template, &chat_msgs, tools, None, true)
        .map_err(|e| inference_failed(format!("chat template failed: {e}")))?;

    let prompt_tokens = model
        .str_to_token(&result.prompt, AddBos::Always)
        .map_err(|e| inference_failed(format!("tokenization failed: {e}")))?
        .len() as u32;

    Ok((result, prompt_tokens))
}

/// Decodes the model's raw output into display text plus structured tool calls.
///
/// When the request carried tools, the model's own chat-template parser
/// (`common_chat`, selected via `chat_format`) splits the response into clean
/// content and tool calls, in the model's native convention. With no tools the
/// raw text is returned verbatim (the validated tool-free path is untouched).
fn parse_completion(
    template_result: &ChatTemplateResult,
    raw_text: String,
) -> (String, Vec<ToolCall>) {
    if !template_result.parse_tool_calls {
        return (raw_text, Vec::new());
    }

    let parsed_json = match template_result.parse_response_oaicompat(&raw_text, false) {
        Ok(json) => json,
        Err(e) => {
            // The parser is best-effort: on failure keep the raw text so the
            // user still sees the answer rather than losing the turn.
            tracing::warn!(error = %e, "tool-call parse failed, returning raw text");
            return (raw_text, Vec::new());
        }
    };

    let value: serde_json::Value = match serde_json::from_str(&parsed_json) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(error = %e, "tool-call parse produced invalid JSON, returning raw text");
            return (raw_text, Vec::new());
        }
    };

    let content = value
        .get("content")
        .and_then(|c| c.as_str())
        .unwrap_or(&raw_text)
        .to_string();

    let tool_calls = value
        .get("tool_calls")
        .and_then(|tc| tc.as_array())
        .map(|arr| {
            arr.iter()
                .enumerate()
                .filter_map(|(i, call)| oai_tool_call(call, i))
                .collect()
        })
        .unwrap_or_default();

    (content, tool_calls)
}

/// Converts one OpenAI-format `tool_calls[]` entry into the IPC [`ToolCall`].
///
/// OpenAI nests the call under `function` and encodes `arguments` as a JSON
/// string; we parse that string back into a value, falling back to a string
/// node if it is not valid JSON.
fn oai_tool_call(call: &serde_json::Value, index: usize) -> Option<ToolCall> {
    let function = call.get("function").unwrap_or(call);
    let name = function.get("name")?.as_str()?.to_string();
    let arguments = match function.get("arguments") {
        Some(serde_json::Value::String(s)) => {
            serde_json::from_str(s).unwrap_or(serde_json::Value::String(s.clone()))
        }
        Some(other) => other.clone(),
        None => serde_json::json!({}),
    };
    let id = call
        .get("id")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| format!("call_{index}"));
    Some(ToolCall {
        id,
        name,
        arguments,
    })
}

/// Builds the tail sampler (greedy if `temperature <= 0`, otherwise stochastic).
fn build_sampler(temperature: f32, top_p: f32, top_k: i32, seed: Option<u64>) -> LlamaSampler {
    if temperature <= 0.0 {
        return LlamaSampler::greedy();
    }
    let resolved_seed = seed.unwrap_or_else(|| {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0)
    });
    let dist_seed = ((resolved_seed >> 32) as u32) ^ (resolved_seed as u32);

    LlamaSampler::chain_simple([
        LlamaSampler::top_k(top_k),
        LlamaSampler::top_p(top_p, 1),
        LlamaSampler::temp(temperature),
        LlamaSampler::dist(dist_seed),
    ])
}


/// Checks that a GGUF path exists and ends with `.gguf`.
pub fn validate_model_path(path: &Path) -> Result<(), ErrorBody> {
    if !path.is_absolute() {
        return Err(ErrorBody::new(
            ErrorCode::BadRequest,
            format!("model_path must be absolute: {}", path.display()),
        ));
    }
    if !path.exists() {
        return Err(ErrorBody::new(
            ErrorCode::BadRequest,
            format!("model_path does not exist: {}", path.display()),
        ));
    }
    match path.extension().and_then(|s| s.to_str()) {
        Some(ext) if ext.eq_ignore_ascii_case("gguf") => Ok(()),
        _ => Err(ErrorBody::new(
            ErrorCode::BadRequest,
            format!("model_path must end with .gguf: {}", path.display()),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn msg(role: Role, content: &str) -> ChatMessage {
        ChatMessage {
            role,
            content: content.to_string(),
        }
    }

    #[test]
    fn budget_zero_fills_small_model_window() {
        // GIVEN an 8k model and a 1000-token prompt, no explicit cap
        // WHEN resolving the budget with the sentinel 0
        let budget = resolve_effective_max(8192, 1000, 0);
        // THEN the whole remaining window is available (no 512 truncation)
        assert_eq!(budget, 8192 - 1000);
    }

    #[test]
    fn budget_zero_uses_full_window_on_large_context_model() {
        // GIVEN a large-window slot and a small prompt, no explicit cap
        // WHEN resolving the budget
        let budget = resolve_effective_max(131_072, 1000, 0);
        // THEN the whole remaining window is available (no artificial cap): the
        // slot already allocated this window, so generation may use all of it.
        assert_eq!(budget, 131_072 - 1000);
    }

    #[test]
    fn budget_zero_with_large_prompt_keeps_full_remaining_window() {
        // GIVEN a 40k slot and a 33k prompt (e.g. summarizing a long page)
        // WHEN resolving the budget
        let budget = resolve_effective_max(40_960, 33_516, 0);
        // THEN the budget is the real remaining window, not a tiny leftover
        // (this is the bug that truncated long-page summaries at 512 tokens).
        assert_eq!(budget, 40_960 - 33_516);
    }

    #[test]
    fn explicit_max_tokens_is_capped_at_remaining_window() {
        // GIVEN a 4k model, a 3000-token prompt, and a request for 4000 tokens
        // WHEN resolving the budget
        let budget = resolve_effective_max(4096, 3000, 4000);
        // THEN it is clamped to the remaining window so generation never overruns
        assert_eq!(budget, 4096 - 3000);
    }

    #[test]
    fn explicit_max_tokens_honoured_when_it_fits() {
        // GIVEN ample remaining window and an explicit 800-token request
        // WHEN resolving the budget
        let budget = resolve_effective_max(8192, 1000, 800);
        // THEN the explicit value is honoured verbatim
        assert_eq!(budget, 800);
    }

    #[test]
    fn budget_never_zero_when_prompt_exceeds_window() {
        // GIVEN a prompt that already exceeds the model window (overflow)
        // WHEN resolving with an explicit cap
        let budget = resolve_effective_max(4096, 5000, 1000);
        // THEN at least one token is allowed (no zero-length generation)
        assert_eq!(budget, 1);
    }

    #[test]
    fn normalize_merges_consecutive_user_turns() {
        // GIVEN a sequence with back-to-back user turns (e.g. folded tool results)
        let messages = [
            msg(Role::System, "sys"),
            msg(Role::User, "hello"),
            msg(Role::Assistant, "hi"),
            msg(Role::User, "tool result A"),
            msg(Role::User, "tool result B"),
        ];

        // WHEN normalizing the role sequence
        let out = normalize_roles(&messages);

        // THEN consecutive user turns collapse into one, preserving content
        assert_eq!(
            out,
            vec![
                ("system", "sys".to_string()),
                ("user", "hello".to_string()),
                ("assistant", "hi".to_string()),
                ("user", "tool result A\n\ntool result B".to_string()),
            ]
        );
    }

    #[test]
    fn normalize_merges_consecutive_system_turns() {
        // GIVEN a system prompt followed by a context-summary system turn
        let messages = [
            msg(Role::System, "prompt"),
            msg(Role::System, "summary"),
            msg(Role::User, "hello"),
        ];

        // WHEN normalizing
        let out = normalize_roles(&messages);

        // THEN the two system turns merge and strict alternation is preserved
        assert_eq!(
            out,
            vec![
                ("system", "prompt\n\nsummary".to_string()),
                ("user", "hello".to_string()),
            ]
        );
    }

    #[test]
    fn normalize_leaves_alternating_sequence_untouched() {
        // GIVEN an already strictly-alternating sequence
        let messages = [
            msg(Role::System, "sys"),
            msg(Role::User, "a"),
            msg(Role::Assistant, "b"),
            msg(Role::User, "c"),
        ];

        // WHEN normalizing
        let out = normalize_roles(&messages);

        // THEN it is unchanged
        assert_eq!(
            out,
            vec![
                ("system", "sys".to_string()),
                ("user", "a".to_string()),
                ("assistant", "b".to_string()),
                ("user", "c".to_string()),
            ]
        );
    }

    #[test]
    fn oai_tool_call_parses_openai_function_entry() {
        // GIVEN an OpenAI-shaped tool call with arguments encoded as a JSON string
        let call = serde_json::json!({
            "id": "abc",
            "type": "function",
            "function": { "name": "list_files", "arguments": "{\"path\":\"/tmp\"}" }
        });

        // WHEN converting it to the IPC tool call
        let tc = oai_tool_call(&call, 0).expect("should parse");

        // THEN id/name are taken verbatim and arguments are decoded into a value
        assert_eq!(tc.id, "abc");
        assert_eq!(tc.name, "list_files");
        assert_eq!(tc.arguments, serde_json::json!({ "path": "/tmp" }));
    }

    #[test]
    fn oai_tool_call_synthesizes_id_when_missing() {
        // GIVEN a call without an id and arguments already as an object
        let call = serde_json::json!({
            "function": { "name": "now", "arguments": { "tz": "UTC" } }
        });

        // WHEN converting at index 2
        let tc = oai_tool_call(&call, 2).expect("should parse");

        // THEN a deterministic fallback id is synthesized
        assert_eq!(tc.id, "call_2");
        assert_eq!(tc.name, "now");
        assert_eq!(tc.arguments, serde_json::json!({ "tz": "UTC" }));
    }

    #[test]
    fn validate_rejects_relative_path() {
        let p = PathBuf::from("relative.gguf");
        let err = validate_model_path(&p).expect_err("relative path must be rejected");
        assert_eq!(err.code, ErrorCode::BadRequest);
    }

    #[test]
    fn validate_rejects_missing_extension() {
        let p = PathBuf::from("/tmp/foo.bin");
        let err = validate_model_path(&p).expect_err("non-gguf path must be rejected");
        assert_eq!(err.code, ErrorCode::BadRequest);
    }

    #[test]
    fn validate_rejects_nonexistent_path() {
        let p = PathBuf::from("/tmp/does-not-exist-apollia-test.gguf");
        let err = validate_model_path(&p).expect_err("missing file must be rejected");
        assert_eq!(err.code, ErrorCode::BadRequest);
    }

    #[test]
    fn loaded_ids_starts_empty() {
        let backend = LlamaCppBackend::new();
        assert!(backend.loaded_ids().is_empty());
        assert_eq!(backend.total_memory_mb(), 0);
    }
}
