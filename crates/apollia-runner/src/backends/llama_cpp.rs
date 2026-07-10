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
use llama_cpp_2::context::params::{KvCacheType, LlamaContextParams};
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::model::{
    AddBos, ChatTemplateResult, GrammarTrigger, GrammarTriggerType, LlamaChatMessage,
    LlamaChatTemplate, LlamaModel,
};
use llama_cpp_2::sampling::LlamaSampler;
use llama_cpp_2::token::LlamaToken;

use crate::ipc::{
    ChatMessage, CompleteData, CompleteParams, ErrorBody, ErrorCode, LoadModelData,
    LoadModelParams, Role, StreamChunk, Timing, TokenUsage, TokenizeData, ToolCall,
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
/// Logical batch size (tokens submitted per decode call). The prefill loop
/// chunks the prompt by this, so a modest cap keeps the compute buffer small
/// instead of scaling it to the whole context window. Matches llama.cpp's own
/// default of 2048.
const DEFAULT_N_BATCH: u32 = 2048;
/// Physical (micro) batch size, bounded by `n_batch`. Matches llama.cpp's
/// default of 512, set explicitly so the value lives in one visible place.
const DEFAULT_N_UBATCH: u32 = 512;
/// Window of recent tokens the repetition penalty looks back over.
const DEFAULT_PENALTY_LAST_N: i32 = 64;

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
        LlamaBackend::init().map_err(|e| internal(format!("llama backend init failed: {e}")))?,
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
        let kv_type = kv_cache_type_from_opt(params.kv_cache_type.as_deref());

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
            let arch = model
                .meta_val_str("general.architecture")
                .unwrap_or_default();
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
            let effective_ctx = probe_context_size(&model, &backend, target, kv_type)?;
            let slots = SlotPool::spawn(&model, &backend, effective_ctx, slot_count, kv_type)?;

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
            backend = active_backend_label(),
            // `999` here means "all layers": if the backend is `cpu`, the
            // request is inert and inference runs entirely on CPU. This pairing
            // (backend vs requested layers) is the first thing to check when
            // prod inference is unexpectedly slow.
            n_gpu_layers_requested = n_gpu_layers,
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
        entry.slots.dispatch(
            fingerprint,
            SlotJob::Complete {
                req,
                reply: reply_tx,
            },
        )?;
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

    /// Tokenize `text` with the loaded model's GGUF tokenizer (`POST /llm/tokenize`).
    ///
    /// Returns [`ErrorCode::ModelNotLoaded`] when `model_id` is not in the cache.
    /// Empty text yields `token_count: 0` (no BOS is added).
    pub fn tokenize(&self, model_id: &str, text: &str) -> Result<TokenizeData, ErrorBody> {
        let entry = self.get_model(model_id).ok_or_else(|| {
            ErrorBody::new(
                ErrorCode::ModelNotLoaded,
                format!("model '{model_id}' not loaded"),
            )
        })?;
        Ok(TokenizeData {
            token_count: entry.slots.tokenize(text),
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
        repeat_penalty: params.repeat_penalty,
        seed: params.seed,
        grammar: params.grammar,
    };
    (req, fingerprint)
}

/// Context parameters shared by the allocation probe and every slot, so the
/// probe measures the same memory footprint the slots will actually use.
///
/// `n_batch` is bounded to [`DEFAULT_N_BATCH`] rather than the full window: the
/// prefill loop chunks the prompt by `n_batch`, so a modest value keeps the
/// compute buffer small without slowing prefill. Flash attention is left at the
/// llama.cpp default (AUTO in this build), which turns it on when the active
/// backend supports it.
pub(super) fn build_context_params(n_ctx: u32, kv_type: KvCacheType) -> LlamaContextParams {
    let n_batch = n_ctx.min(DEFAULT_N_BATCH);
    let n_ubatch = n_batch.min(DEFAULT_N_UBATCH);
    // `KvCacheType::F16` is the llama.cpp default, so passing it is a no-op: the
    // quantized types (e.g. Q8_0) are opt-in and only shrink the KV cache footprint.
    LlamaContextParams::default()
        .with_n_ctx(NonZeroU32::new(n_ctx))
        .with_n_batch(n_batch)
        .with_n_ubatch(n_ubatch)
        .with_type_k(kv_type)
        .with_type_v(kv_type)
}

/// Map an optional IPC string to a KV cache data type. Absent or unrecognized
/// resolves to `F16` (the llama.cpp default, i.e. no change). Quantizing the KV
/// cache (e.g. `q8_0`) trades a little precision for a smaller footprint, letting
/// a larger context or more concurrent sequences fit. Quantizing V generally
/// needs flash attention, which llama.cpp enables automatically (AUTO policy)
/// when the active backend supports it.
fn kv_cache_type_from_opt(s: Option<&str>) -> KvCacheType {
    match s.map(str::to_ascii_lowercase).as_deref() {
        Some("q8_0") => KvCacheType::Q8_0,
        Some("q5_1") => KvCacheType::Q5_1,
        Some("q5_0") => KvCacheType::Q5_0,
        Some("q4_1") => KvCacheType::Q4_1,
        Some("q4_0") => KvCacheType::Q4_0,
        Some("f16") | None => KvCacheType::F16,
        Some(other) => {
            tracing::warn!(kv_cache_type = %other, "unknown kv_cache_type, using f16");
            KvCacheType::F16
        }
    }
}

/// Compile-time label of the exclusive backend feature this binary was built
/// with. Purely diagnostic: it tells the operator which `apollia-runner-*`
/// binary is actually running, so a CPU-only binary silently replacing the
/// GPU one (all layers on CPU despite `n_gpu_layers`) is visible in the logs.
fn active_backend_label() -> &'static str {
    if cfg!(feature = "local-cuda") {
        "cuda"
    } else if cfg!(feature = "local-metal") {
        "metal"
    } else if cfg!(feature = "local-rocm") {
        "rocm"
    } else if cfg!(feature = "local-vulkan") {
        "vulkan"
    } else {
        "cpu"
    }
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
    kv_type: KvCacheType,
) -> Result<u32, ErrorBody> {
    let mut size = target.max(MIN_CTX_SIZE);
    loop {
        let params = build_context_params(size, kv_type);
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
    // Preferred path: the model's native chat-template parser, used only when
    // the template advertised tool support.
    let native = if template_result.parse_tool_calls {
        native_tool_calls(template_result, &raw_text)
    } else {
        None
    };
    if let Some((content, calls)) = &native {
        if !calls.is_empty() {
            return (content.clone(), calls.clone());
        }
    }

    // Fallback: many GGUF chat templates (Hermes / Qwen lineage) instruct the
    // model to emit `<tool_call>{...}</tool_call>` tags, but llama.cpp does not
    // always detect that format, so the native parser yields nothing and the
    // call would leak to the user as plain text. Recover those tags ourselves.
    let (content, tool_calls) = extract_tag_tool_calls(&raw_text);
    if !tool_calls.is_empty() {
        tracing::debug!(
            count = tool_calls.len(),
            "recovered tool calls from <tool_call> tags (native parser found none)"
        );
        return (content, tool_calls);
    }

    // No tool calls anywhere: keep the native cleaned content if we have it,
    // otherwise the raw text verbatim (the validated tool-free path).
    match native {
        Some((content, _)) => (content, Vec::new()),
        None => (raw_text, Vec::new()),
    }
}

/// Run llama.cpp's own OpenAI-compat response parser. Returns the cleaned
/// content plus any structured tool calls, or `None` when the parser fails
/// (so the caller can fall back to a tag scan).
fn native_tool_calls(
    template_result: &ChatTemplateResult,
    raw_text: &str,
) -> Option<(String, Vec<ToolCall>)> {
    let parsed_json = match template_result.parse_response_oaicompat(raw_text, false) {
        Ok(json) => json,
        Err(e) => {
            tracing::warn!(error = %e, "native tool-call parse failed, falling back to tag scan");
            return None;
        }
    };

    let value: serde_json::Value = match serde_json::from_str(&parsed_json) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(error = %e, "native tool-call parse produced invalid JSON, falling back to tag scan");
            return None;
        }
    };

    let content = value
        .get("content")
        .and_then(|c| c.as_str())
        .unwrap_or(raw_text)
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

    Some((content, tool_calls))
}

/// Recover Hermes / Qwen style `<tool_call>{...}</tool_call>` calls from raw
/// model output. Returns the text with the recognized tag blocks removed plus
/// the parsed calls. A tag whose inner payload is not a JSON tool call is left
/// verbatim in the content and produces no call.
fn extract_tag_tool_calls(raw_text: &str) -> (String, Vec<ToolCall>) {
    const OPEN: &str = "<tool_call>";
    const CLOSE: &str = "</tool_call>";

    let mut tool_calls = Vec::new();
    let mut content = String::new();
    let mut rest = raw_text;

    while let Some(open_at) = rest.find(OPEN) {
        let after_open = open_at + OPEN.len();
        let Some(close_rel) = rest[after_open..].find(CLOSE) else {
            // Unterminated tag: stop scanning, keep the remainder verbatim.
            break;
        };
        let close_at = after_open + close_rel;
        let block_end = close_at + CLOSE.len();
        let inner = rest[after_open..close_at].trim();

        let call = serde_json::from_str::<serde_json::Value>(inner)
            .ok()
            .and_then(|value| oai_tool_call(&value, tool_calls.len()));

        match call {
            Some(call) => {
                content.push_str(&rest[..open_at]);
                tool_calls.push(call);
            }
            None => {
                // Not a tool call we understand: preserve the block as text.
                content.push_str(&rest[..block_end]);
            }
        }
        rest = &rest[block_end..];
    }
    content.push_str(rest);

    (content.trim().to_string(), tool_calls)
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

/// Clean text that is safe to STREAM right now, given the raw model output so
/// far. Mirrors [`extract_tag_tool_calls`]'s text handling for incremental
/// output: complete, valid `<tool_call>` blocks are suppressed (their structured
/// call is delivered separately on the final chunk), a block whose payload is not
/// a valid tool call is kept verbatim, and the tail is held back when it is an
/// unterminated opener or a partial opener prefix. The returned prefix therefore
/// only ever grows as more tokens arrive, so the streaming path can emit the
/// delta beyond what it already sent. Not trimmed (unlike the final parse), so
/// the caller can prefix-match its already-streamed text exactly.
pub(super) fn stream_clean_prefix(raw: &str) -> String {
    const OPEN: &str = "<tool_call>";
    const CLOSE: &str = "</tool_call>";

    let mut content = String::new();
    let mut rest = raw;
    loop {
        match rest.find(OPEN) {
            Some(open_at) => {
                let after_open = open_at + OPEN.len();
                match rest[after_open..].find(CLOSE) {
                    Some(close_rel) => {
                        let close_at = after_open + close_rel;
                        let block_end = close_at + CLOSE.len();
                        let inner = rest[after_open..close_at].trim();
                        let is_call = serde_json::from_str::<serde_json::Value>(inner)
                            .ok()
                            .and_then(|value| oai_tool_call(&value, 0))
                            .is_some();
                        if is_call {
                            // Valid call: its text is suppressed from the stream.
                            content.push_str(&rest[..open_at]);
                        } else {
                            // Not a call we understand: keep the block verbatim,
                            // matching the non-streaming path.
                            content.push_str(&rest[..block_end]);
                        }
                        rest = &rest[block_end..];
                    }
                    None => {
                        // Opener with no close yet: everything from here is
                        // unresolved, hold it back.
                        content.push_str(&rest[..open_at]);
                        return content;
                    }
                }
            }
            None => {
                // Hold back a trailing partial opener that may complete next token.
                let hold = partial_opener_suffix(rest, OPEN);
                content.push_str(&rest[..rest.len() - hold]);
                return content;
            }
        }
    }
}

/// Longest trailing suffix of `s` that is a proper prefix of `opener` (a partial
/// `<tool_call>` opener that could complete on the next token). `0` when none.
fn partial_opener_suffix(s: &str, opener: &str) -> usize {
    let max = opener.len().saturating_sub(1).min(s.len());
    (1..=max)
        .rev()
        .find(|&k| s.ends_with(&opener[..k]))
        .unwrap_or(0)
}

/// Builds the sampler chain for one inference request.
///
/// The tail is greedy when `temperature <= 0`, otherwise the stochastic
/// `top_k`/`top_p`/`temp`/`dist` chain. When `req.grammar` is `Some`, a grammar
/// stage is prepended as the first element of the chain, restricting the token
/// sequence to the GBNF. An invalid grammar returns `Err` with
/// [`ErrorCode::InferenceFailed`] and never crashes the slot. When `req.grammar`
/// is `None`, the chain is identical to the pre-grammar behavior.
fn build_sampler(
    req: &InferenceRequest,
    model: &LlamaModel,
    template: &ChatTemplateResult,
) -> Result<LlamaSampler, ErrorBody> {
    let tail = if req.temperature <= 0.0 {
        LlamaSampler::greedy()
    } else {
        let resolved_seed = req.seed.unwrap_or_else(|| {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos() as u64)
                .unwrap_or(0)
        });
        let dist_seed = ((resolved_seed >> 32) as u32) ^ (resolved_seed as u32);

        LlamaSampler::chain_simple([
            LlamaSampler::top_k(req.top_k),
            LlamaSampler::top_p(req.top_p, 1),
            LlamaSampler::temp(req.temperature),
            LlamaSampler::dist(dist_seed),
        ])
    };

    // Repetition penalty over a recent window, applied before the selection tail
    // when enabled. `<= 1.0` disables it (llama.cpp convention: 1.0 is a no-op),
    // so the pre-existing behavior is preserved when no penalty is requested.
    // Frequency and presence penalties stay at 0 (repeat-only).
    let with_penalty = if req.repeat_penalty > 1.0 {
        LlamaSampler::chain_simple([
            LlamaSampler::penalties(DEFAULT_PENALTY_LAST_N, req.repeat_penalty, 0.0, 0.0),
            tail,
        ])
    } else {
        tail
    };

    // Grammar precedence: an explicit daemon-provided GBNF (the Python ReAct path
    // sends one via `tool_specs_to_gbnf`) wins. Otherwise, when the model's chat
    // template generated a tool grammar, apply it: it is format-correct per model
    // family (Mistral `[TOOL_CALLS]`, Qwen `<tool_call>`, ...) and lazy, so free
    // text flows until a tool call begins, then the call is constrained to be
    // well-formed. This is what enables reliable tool calls on the chat path
    // without forcing tool-only output.
    let grammar_stage = if let Some(gbnf) = req.grammar.as_deref() {
        Some(
            LlamaSampler::grammar(model, gbnf, "root")
                .map_err(|e| inference_failed(format!("invalid grammar: {e}")))?,
        )
    } else if let Some(gbnf) = template.grammar.as_deref() {
        // Best-effort: a malformed template grammar (rare, model-specific) falls
        // back to unconstrained decoding rather than failing the inference, so
        // this can never regress a model that worked before. The explicit
        // daemon-provided grammar above still hard-errors on purpose.
        match build_template_grammar_sampler(model, gbnf, template) {
            Ok(sampler) => Some(sampler),
            Err(e) => {
                tracing::warn!(error = %e.message, "template grammar unusable, decoding unconstrained");
                None
            }
        }
    } else {
        None
    };

    match grammar_stage {
        None => Ok(with_penalty),
        Some(grammar) => Ok(LlamaSampler::chain_simple([grammar, with_penalty])),
    }
}

/// Build the grammar sampler stage from a chat-template-generated grammar.
///
/// Eager when the template requests it; otherwise lazy, activated by the
/// template's triggers (literal words, specific tokens, or regex patterns) so the
/// grammar only kicks in once a tool call starts and free-text answers stay
/// unconstrained.
fn build_template_grammar_sampler(
    model: &LlamaModel,
    gbnf: &str,
    template: &ChatTemplateResult,
) -> Result<LlamaSampler, ErrorBody> {
    if !template.grammar_lazy {
        return LlamaSampler::grammar(model, gbnf, "root")
            .map_err(|e| inference_failed(format!("invalid template grammar: {e}")));
    }
    let (words, tokens, patterns) = partition_triggers(&template.grammar_triggers);
    // A template uses one trigger style; when regex patterns are present they take
    // the pattern-aware constructor, otherwise literal words drive activation.
    let sampler = if patterns.is_empty() {
        LlamaSampler::grammar_lazy(
            model,
            gbnf,
            "root",
            words.iter().map(String::as_bytes),
            &tokens,
        )
    } else {
        LlamaSampler::grammar_lazy_patterns(model, gbnf, "root", &patterns, &tokens)
    };
    sampler.map_err(|e| inference_failed(format!("invalid lazy template grammar: {e}")))
}

/// Split chat-template grammar triggers into the shapes the lazy sampler needs:
/// literal words, specific token ids, and regex patterns.
fn partition_triggers(triggers: &[GrammarTrigger]) -> (Vec<String>, Vec<LlamaToken>, Vec<String>) {
    let mut words = Vec::new();
    let mut tokens = Vec::new();
    let mut patterns = Vec::new();
    for trigger in triggers {
        match trigger.trigger_type {
            GrammarTriggerType::Token => {
                if let Some(token) = trigger.token {
                    tokens.push(token);
                }
            }
            GrammarTriggerType::Word => words.push(trigger.value.clone()),
            GrammarTriggerType::Pattern | GrammarTriggerType::PatternFull => {
                patterns.push(trigger.value.clone());
            }
        }
    }
    (words, tokens, patterns)
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

    // GIVEN a backend with no model loaded
    // WHEN tokenize is called for an unknown model id
    // THEN it returns a ModelNotLoaded error
    #[test]
    fn tokenize_unknown_model_returns_model_not_loaded() {
        let backend = LlamaCppBackend::new();
        let err = backend
            .tokenize("inexistant", "test")
            .expect_err("unknown model must be rejected");
        assert_eq!(err.code, ErrorCode::ModelNotLoaded);
    }

    #[test]
    fn context_params_cap_batch_on_large_window() {
        // GIVEN a very large context window
        // WHEN building the shared context params
        let p = build_context_params(131_072, KvCacheType::F16);
        // THEN the logical/micro batch are capped instead of scaling to the
        // whole window (which would inflate the compute buffer), and n_ctx is
        // preserved.
        assert_eq!(p.n_batch(), DEFAULT_N_BATCH);
        assert_eq!(p.n_ubatch(), DEFAULT_N_UBATCH);
        assert_eq!(p.n_ctx(), NonZeroU32::new(131_072));
    }

    #[test]
    fn context_params_small_window_batch_follows_window() {
        // GIVEN a window smaller than the default batch cap
        // WHEN building the params
        let p = build_context_params(1024, KvCacheType::F16);
        // THEN n_batch follows the window and n_ubatch stays within n_batch
        assert_eq!(p.n_batch(), 1024);
        assert_eq!(p.n_ubatch(), DEFAULT_N_UBATCH);
    }

    #[test]
    fn context_params_applies_quantized_kv_type() {
        // GIVEN an opt-in quantized KV type
        // WHEN building the params
        let p = build_context_params(4096, KvCacheType::Q8_0);
        // THEN both K and V caches use it (F16 stays the default elsewhere)
        assert_eq!(p.type_k(), KvCacheType::Q8_0);
        assert_eq!(p.type_v(), KvCacheType::Q8_0);
    }

    #[test]
    fn kv_cache_type_maps_known_and_defaults_to_f16() {
        // GIVEN various IPC strings
        // THEN known types map, absent/unknown fall back to F16 (no change)
        assert_eq!(kv_cache_type_from_opt(Some("q8_0")), KvCacheType::Q8_0);
        assert_eq!(kv_cache_type_from_opt(Some("Q4_0")), KvCacheType::Q4_0);
        assert_eq!(kv_cache_type_from_opt(Some("f16")), KvCacheType::F16);
        assert_eq!(kv_cache_type_from_opt(None), KvCacheType::F16);
        assert_eq!(kv_cache_type_from_opt(Some("bogus")), KvCacheType::F16);
    }

    #[test]
    fn build_request_forwards_repeat_penalty() {
        // GIVEN complete params carrying a repetition penalty
        let params = CompleteParams {
            model_id: "m".to_string(),
            messages: vec![msg(Role::User, "hi")],
            max_tokens: 0,
            temperature: 0.7,
            top_p: 0.9,
            top_k: 40,
            repeat_penalty: 1.3,
            seed: None,
            stop: vec![],
            tools: None,
            grammar: None,
        };

        // WHEN building the slot request
        let (req, _fp) = build_request(params);

        // THEN the penalty reaches the sampler input verbatim (was silently
        // dropped before: carried by IPC but never copied into InferenceRequest)
        assert_eq!(req.repeat_penalty, 1.3);
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

    #[test]
    fn tag_scan_recovers_single_qwen_tool_call() {
        // GIVEN Hermes/Qwen output with one <tool_call> block (object arguments)
        let raw = "Sure, writing it now.\n<tool_call>{\"name\": \"gdrive.workspace_write\", \"arguments\": {\"name\": \"bateau.txt\", \"content\": \"hello\"}}</tool_call>";

        // WHEN scanning for tag-style tool calls
        let (content, calls) = extract_tag_tool_calls(raw);

        // THEN one call is recovered and the tag is stripped from the content
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "gdrive.workspace_write");
        assert_eq!(
            calls[0].arguments,
            serde_json::json!({ "name": "bateau.txt", "content": "hello" })
        );
        assert_eq!(content, "Sure, writing it now.");
    }

    #[test]
    fn tag_scan_recovers_multiple_tool_calls() {
        // GIVEN two consecutive <tool_call> blocks
        let raw = "<tool_call>{\"name\": \"a\", \"arguments\": {}}</tool_call><tool_call>{\"name\": \"b\", \"arguments\": {\"x\": 1}}</tool_call>";

        // WHEN scanning
        let (content, calls) = extract_tag_tool_calls(raw);

        // THEN both calls are recovered with synthesized ids and content is empty
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].name, "a");
        assert_eq!(calls[0].id, "call_0");
        assert_eq!(calls[1].name, "b");
        assert_eq!(calls[1].id, "call_1");
        assert_eq!(content, "");
    }

    #[test]
    fn tag_scan_leaves_plain_text_untouched() {
        // GIVEN ordinary prose with no tool-call tags
        let raw = "Here is the summary you asked for, no tools needed.";

        // WHEN scanning
        let (content, calls) = extract_tag_tool_calls(raw);

        // THEN nothing is extracted and the text is preserved
        assert!(calls.is_empty());
        assert_eq!(content, raw);
    }

    #[test]
    fn tag_scan_keeps_malformed_block_as_text_without_panicking() {
        // GIVEN a <tool_call> block whose inner payload is not valid JSON
        let raw = "before <tool_call>not json at all</tool_call> after";

        // WHEN scanning
        let (content, calls) = extract_tag_tool_calls(raw);

        // THEN no call is produced and the block is preserved verbatim
        assert!(calls.is_empty());
        assert_eq!(
            content,
            "before <tool_call>not json at all</tool_call> after"
        );
    }

    #[test]
    fn tag_scan_ignores_unterminated_tag() {
        // GIVEN an opening tag with no closing tag (truncated generation)
        let raw = "thinking <tool_call>{\"name\": \"a\"";

        // WHEN scanning
        let (content, calls) = extract_tag_tool_calls(raw);

        // THEN nothing is extracted and the remainder is kept
        assert!(calls.is_empty());
        assert_eq!(content, "thinking <tool_call>{\"name\": \"a\"");
    }

    #[test]
    fn stream_clean_prefix_passes_plain_text() {
        // GIVEN output with no tool-call markers at all
        // WHEN computing the streamable clean prefix
        // THEN the whole text is emittable
        assert_eq!(stream_clean_prefix("hello world"), "hello world");
    }

    #[test]
    fn stream_clean_prefix_holds_back_partial_opener() {
        // GIVEN a trailing partial `<tool_call>` opener (mid-token)
        // WHEN computing the prefix
        // THEN the partial opener is held back until it completes or resolves
        assert_eq!(stream_clean_prefix("hello <tool"), "hello ");
    }

    #[test]
    fn stream_clean_prefix_holds_back_unterminated_block() {
        // GIVEN an opener whose closing tag has not arrived yet
        // WHEN computing the prefix
        // THEN everything from the opener is held back
        assert_eq!(
            stream_clean_prefix("before <tool_call>{\"name\": \"f\""),
            "before "
        );
    }

    #[test]
    fn stream_clean_prefix_suppresses_complete_valid_call() {
        // GIVEN a complete, valid tool-call block surrounded by text
        let raw = "before <tool_call>{\"name\": \"f\", \"arguments\": {}}</tool_call>after";
        // WHEN computing the prefix
        // THEN the block text is suppressed (the call is delivered structured later)
        assert_eq!(stream_clean_prefix(raw), "before after");
    }

    #[test]
    fn stream_clean_prefix_keeps_malformed_block_verbatim() {
        // GIVEN a `<tool_call>` block whose payload is not a valid call
        let raw = "x <tool_call>not json</tool_call> y";
        // WHEN computing the prefix
        // THEN it is kept verbatim, matching the non-streaming path
        assert_eq!(
            stream_clean_prefix(raw),
            "x <tool_call>not json</tool_call> y"
        );
    }

    #[test]
    fn stream_clean_prefix_is_monotonic_across_growth() {
        // GIVEN the same output revealed token by token
        // WHEN the prefix is recomputed as text grows
        // THEN each result extends the previous (never retracts already-safe text)
        let steps = [
            "Let me ",
            "Let me check <tool",
            "Let me check <tool_call>{\"name\":\"f\",\"arguments\":{}}",
            "Let me check <tool_call>{\"name\":\"f\",\"arguments\":{}}</tool_call> done",
        ];
        let mut prev = String::new();
        for s in steps {
            let clean = stream_clean_prefix(s);
            assert!(
                clean.starts_with(&prev),
                "prefix retracted: {prev:?} -> {clean:?}"
            );
            prev = clean;
        }
        // The final clean text is the surrounding prose, tool-call block removed.
        assert_eq!(prev, "Let me check  done");
    }

    #[test]
    fn test_build_sampler_signature_with_grammar() {
        // GIVEN build_sampler now takes the request (carrying the grammar) and a model
        // WHEN binding it as a typed function pointer
        // THEN the signature returns Result<LlamaSampler, ErrorBody> (the None grammar
        //      path is identical to pre-grammar behavior; the Some path needs a loaded
        //      model and is exercised by integration, not here)
        let _: fn(
            &InferenceRequest,
            &LlamaModel,
            &ChatTemplateResult,
        ) -> Result<LlamaSampler, ErrorBody> = build_sampler;
    }

    #[test]
    fn partition_triggers_splits_by_kind() {
        // GIVEN a mix of word, token and pattern triggers from a chat template
        let triggers = vec![
            GrammarTrigger {
                trigger_type: GrammarTriggerType::Word,
                value: "<tool_call>".to_string(),
                token: None,
            },
            GrammarTrigger {
                trigger_type: GrammarTriggerType::Token,
                value: String::new(),
                token: Some(LlamaToken(128010)),
            },
            GrammarTrigger {
                trigger_type: GrammarTriggerType::Pattern,
                value: "\\[TOOL_CALLS\\]".to_string(),
                token: None,
            },
        ];

        // WHEN partitioning them
        let (words, tokens, patterns) = partition_triggers(&triggers);

        // THEN each lands in the bucket the lazy sampler expects
        assert_eq!(words, vec!["<tool_call>".to_string()]);
        assert_eq!(tokens, vec![LlamaToken(128010)]);
        assert_eq!(patterns, vec!["\\[TOOL_CALLS\\]".to_string()]);
    }
}

/// Compile-time verification that `llama-cpp-2` exposes the grammar sampler
/// surface this backend will build on. These tests assert the API contract,
/// not runtime behavior: loading a real model needs a GGUF file, so the
/// constrained-decode path is exercised by integration, not here.
#[cfg(all(test, feature = "local-cpu"))]
mod spike_gbnf_tests {
    use llama_cpp_2::model::LlamaModel;
    use llama_cpp_2::sampling::LlamaSampler;
    use llama_cpp_2::GrammarError;

    #[test]
    fn grammar_symbol_has_expected_signature() {
        // GIVEN the llama-cpp-2 version resolved in Cargo.lock
        // WHEN binding LlamaSampler::grammar as a typed function pointer
        // THEN the symbol exists with the (model, grammar_str, grammar_root) signature
        let _grammar_fn: fn(&LlamaModel, &str, &str) -> Result<LlamaSampler, GrammarError> =
            LlamaSampler::grammar;
    }

    #[test]
    fn grammar_error_variants_are_accessible() {
        // GIVEN the grammar sampler error type re-exported at the crate root
        // WHEN matching one of its variants
        // THEN GrammarError is accessible with the documented variant set
        let err = GrammarError::NullGrammar;
        assert!(matches!(
            err,
            GrammarError::RootNotFound
                | GrammarError::TriggerWordNullBytes
                | GrammarError::GrammarNullBytes
                | GrammarError::NullGrammar
        ));
    }
}
