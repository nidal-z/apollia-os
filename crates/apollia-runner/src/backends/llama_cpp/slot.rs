//! Persistent inference slots with prompt-prefix KV reuse.
//!
//! A *slot* is a dedicated long-lived thread that owns one `LlamaModel` handle
//! (shared `Arc`) and one `LlamaContext` as **stack locals**. Because the
//! context never enters a struct, there is no self-referential storage and no
//! `unsafe`: the context borrows the model for the thread's lifetime and is
//! dropped before it. The slot keeps the token sequence currently materialized
//! in its KV cache (`cached_tokens`) and, on each request, reuses the longest
//! common prefix with the new prompt, decoding only the diverging tail.
//!
//! Requests reach a slot over a bounded channel; a slot serializes them (a
//! context can decode only one thing at a time). A [`SlotPool`] holds N slots
//! per model and routes a request to the slot whose conversation matches, so
//! follow-up turns land on the slot that already holds their prefix.

use std::sync::{Arc, Mutex};
use std::time::Instant;

use llama_cpp_2::context::params::KvCacheType;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::{AddBos, LlamaModel};
use llama_cpp_2::token::LlamaToken;

use crate::ipc::{ChatMessage, ErrorBody, FinishReason, StreamChunk, ToolCall};

use super::{
    build_prompt, build_sampler, inference_failed, internal, parse_completion,
    resolve_effective_max,
};

/// Owned, `Send` inference parameters handed to a slot.
pub(super) struct InferenceRequest {
    pub messages: Vec<ChatMessage>,
    pub tools_json: Option<String>,
    pub max_tokens: u32,
    pub temperature: f32,
    pub top_p: f32,
    pub top_k: i32,
    /// Repetition penalty over the recent token window. `1.0` (or less) disables it.
    pub repeat_penalty: f32,
    pub seed: Option<u64>,
    /// GBNF grammar string; `None` means unconstrained decoding.
    pub grammar: Option<String>,
}

/// Result of a non-streaming inference, returned over the reply channel.
pub(super) struct SlotOutput {
    pub text: String,
    pub tool_calls: Vec<ToolCall>,
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub finish_reason: FinishReason,
    pub prefill_ms: u64,
    pub decode_ms: u64,
}

/// One unit of work for a slot. The reply path is embedded so the slot answers
/// without any shared result state.
pub(super) enum SlotJob {
    Complete {
        req: InferenceRequest,
        reply: tokio::sync::oneshot::Sender<Result<SlotOutput, ErrorBody>>,
    },
    Stream {
        req: InferenceRequest,
        tx: tokio::sync::mpsc::Sender<Result<StreamChunk, ErrorBody>>,
    },
}

/// Routing hint published by a slot for the dispatcher. Single-writer (the slot
/// thread) / multi-reader (the dispatcher). Never the source of truth: the
/// authoritative `cached_tokens` lives as a thread local, and a stale hint only
/// costs a missed reuse, never correctness.
struct RoutingState {
    /// Fingerprint of the conversation currently held by this slot (hash of its
    /// opening messages). `0` for a never-used slot.
    fingerprint: u64,
    /// `true` while the slot is executing a job.
    busy: bool,
}

/// Dispatcher-side handle to one slot thread.
struct SlotHandle {
    /// Unbounded so `dispatch` never blocks the async reactor; the slot still
    /// serializes jobs (it processes one at a time), queued jobs are tiny.
    tx: Option<std::sync::mpsc::Sender<SlotJob>>,
    join: Option<std::thread::JoinHandle<()>>,
    routing: Arc<Mutex<RoutingState>>,
}

impl Drop for SlotHandle {
    fn drop(&mut self) {
        // Close the channel first so the slot loop observes a disconnect and
        // exits, dropping its context; then join so teardown is deterministic
        // and the KV memory is released before we return.
        self.tx.take();
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

/// A pool of N persistent slots for a single loaded model.
pub(super) struct SlotPool {
    handles: Vec<SlotHandle>,
    /// Shared model handle, kept at the pool level so tokenization (vocab only,
    /// no KV cache) runs directly without routing through a slot thread.
    model: Arc<LlamaModel>,
}

impl SlotPool {
    /// Spawn up to `count` slots, each creating its own context at `n_ctx`.
    ///
    /// Each slot allocates a full KV cache, so requesting several slots multiplies
    /// the memory footprint. Rather than failing the whole load when memory is too
    /// tight for the full count, the pool degrades gracefully: it keeps the slots
    /// that did allocate (at least one) and logs the shortfall. Batch throughput is
    /// then reduced, not lost. The load only fails when even a single slot cannot
    /// allocate its context (a genuine out-of-memory at the model's window).
    pub(super) fn spawn(
        model: &Arc<LlamaModel>,
        backend: &Arc<LlamaBackend>,
        n_ctx: u32,
        count: u32,
        kv_type: KvCacheType,
    ) -> Result<Self, ErrorBody> {
        let requested = count.max(1);
        let mut handles = Vec::with_capacity(requested as usize);
        for index in 0..requested {
            match spawn_slot(index, Arc::clone(model), Arc::clone(backend), n_ctx, kv_type) {
                Ok(handle) => handles.push(handle),
                Err(e) => {
                    if handles.is_empty() {
                        // Not even one slot fits: a real load failure, surfaced.
                        return Err(e);
                    }
                    tracing::warn!(
                        requested,
                        spawned = handles.len(),
                        reason = %e.message,
                        "slot pool degraded: not enough memory for the full slot count"
                    );
                    break;
                }
            }
        }
        Ok(Self {
            handles,
            model: Arc::clone(model),
        })
    }

    /// Count the tokens `text` produces with the model's GGUF tokenizer.
    ///
    /// `AddBos::Never` so an empty string yields `0`. Tokenization needs only
    /// the model vocabulary, so it runs on the caller's thread without a slot
    /// job. A tokenizer failure maps to `0` (the caller falls back to a proxy).
    pub(super) fn tokenize(&self, text: &str) -> u32 {
        self.model
            .str_to_token(text, AddBos::Never)
            .map(|tokens| tokens.len() as u32)
            .unwrap_or(0)
    }

    /// Send a job to the best slot for `fingerprint`. Picks the idle slot whose
    /// fingerprint matches; else any idle slot; else the matching busy slot;
    /// else slot 0. Each slot processes its queue serially.
    pub(super) fn dispatch(&self, fingerprint: u64, job: SlotJob) -> Result<(), ErrorBody> {
        let idx = self.route(fingerprint);
        let handle = &self.handles[idx];
        match handle.tx.as_ref() {
            Some(tx) => tx
                .send(job)
                .map_err(|_| internal("slot thread is gone")),
            None => Err(internal("slot channel closed")),
        }
    }

    fn route(&self, fingerprint: u64) -> usize {
        let mut best_idle: Option<(usize, bool)> = None; // (idx, matches)
        let mut best_busy_match: Option<usize> = None;
        for (i, h) in self.handles.iter().enumerate() {
            let (slot_fp, busy) = match h.routing.lock() {
                Ok(g) => (g.fingerprint, g.busy),
                Err(_) => (0, true),
            };
            let matches = fingerprint != 0 && slot_fp == fingerprint;
            if busy {
                if matches && best_busy_match.is_none() {
                    best_busy_match = Some(i);
                }
            } else {
                match best_idle {
                    // Prefer an idle slot that already matches.
                    Some((_, true)) => {}
                    _ => best_idle = Some((i, matches)),
                }
                if matches {
                    best_idle = Some((i, true));
                }
            }
        }
        best_idle
            .map(|(i, _)| i)
            .or(best_busy_match)
            .unwrap_or(0)
    }
}

/// Spawn one slot thread. The `LlamaContext` is created on this thread and lives
/// as a stack local for the thread's whole life, so the `model` borrow it holds
/// is structurally sound (model `Arc` dropped after the context) without
/// `unsafe` or a self-referential struct.
fn spawn_slot(
    index: u32,
    model: Arc<LlamaModel>,
    backend: Arc<LlamaBackend>,
    n_ctx: u32,
    kv_type: KvCacheType,
) -> Result<SlotHandle, ErrorBody> {
    let routing = Arc::new(Mutex::new(RoutingState {
        fingerprint: 0,
        busy: false,
    }));
    // Unbounded: sends never block the async reactor. The slot serializes jobs
    // by processing them one at a time off this queue.
    let (tx, rx) = std::sync::mpsc::channel::<SlotJob>();
    // Readiness rendezvous: the worker reports whether its context allocated, so
    // a KV-allocation failure surfaces synchronously at load time.
    let (ready_tx, ready_rx) = std::sync::mpsc::channel::<Result<(), ErrorBody>>();

    let routing_thread = Arc::clone(&routing);
    let join = std::thread::Builder::new()
        .name(format!("apollia-llm-slot-{index}"))
        .spawn(move || {
            let ctx_params = super::build_context_params(n_ctx, kv_type);
            let mut ctx = match model.new_context(&backend, ctx_params) {
                Ok(c) => c,
                Err(e) => {
                    let _ = ready_tx.send(Err(inference_failed(format!(
                        "slot {index} context creation failed: {e}"
                    ))));
                    return;
                }
            };
            let _ = ready_tx.send(Ok(()));
            tracing::info!(slot = index, n_ctx, "llm inference slot ready");

            let mut cached_tokens: Vec<LlamaToken> = Vec::new();
            while let Ok(job) = rx.recv() {
                let fingerprint = match &job {
                    SlotJob::Complete { req, .. } | SlotJob::Stream { req, .. } => {
                        conversation_fingerprint(&req.messages)
                    }
                };
                set_busy(&routing_thread, true);
                run_job(&model, &mut ctx, &mut cached_tokens, n_ctx, index, job);
                // Publish after serving so the next turn of this conversation
                // routes back to this slot (which now holds its prefix).
                publish_fingerprint(&routing_thread, fingerprint);
                set_busy(&routing_thread, false);
            }
            tracing::debug!(slot = index, "llm inference slot stopped");
        })
        .map_err(|e| internal(format!("slot thread spawn failed: {e}")))?;

    // Block until the worker confirms its context allocated (or failed).
    match ready_rx.recv() {
        Ok(Ok(())) => Ok(SlotHandle {
            tx: Some(tx),
            join: Some(join),
            routing,
        }),
        Ok(Err(e)) => {
            let _ = join.join();
            Err(e)
        }
        Err(_) => {
            let _ = join.join();
            Err(internal("slot thread exited before signalling readiness"))
        }
    }
}

fn set_busy(routing: &Arc<Mutex<RoutingState>>, busy: bool) {
    if let Ok(mut g) = routing.lock() {
        g.busy = busy;
    }
}

fn publish_fingerprint(routing: &Arc<Mutex<RoutingState>>, fingerprint: u64) {
    if let Ok(mut g) = routing.lock() {
        g.fingerprint = fingerprint;
    }
}

/// Run one job on the slot's persistent context.
#[allow(clippy::too_many_arguments)]
fn run_job(
    model: &LlamaModel,
    ctx: &mut llama_cpp_2::context::LlamaContext<'_>,
    cached_tokens: &mut Vec<LlamaToken>,
    n_ctx: u32,
    index: u32,
    job: SlotJob,
) {
    match job {
        SlotJob::Complete { req, reply } => {
            let out = generate(model, ctx, cached_tokens, n_ctx, index, &req, None);
            let _ = reply.send(out);
        }
        SlotJob::Stream { req, tx } => {
            let out = generate(model, ctx, cached_tokens, n_ctx, index, &req, Some(&tx));
            match out {
                Ok(o) => {
                    // Final chunk: carries the structured tool calls (recovered
                    // from the full output) plus the finish reason. The text was
                    // already streamed live and cleaned, so it stays empty here.
                    let _ = tx.blocking_send(Ok(StreamChunk {
                        text: String::new(),
                        tool_calls: o.tool_calls,
                        finish_reason: Some(o.finish_reason),
                    }));
                }
                Err(e) => {
                    let _ = tx.blocking_send(Err(e));
                }
            }
        }
    }
}

/// Core inference shared by complete and stream. When `stream_tx` is `Some`,
/// pieces are pushed as they decode; otherwise they are accumulated.
#[allow(clippy::too_many_arguments)]
fn generate(
    model: &LlamaModel,
    ctx: &mut llama_cpp_2::context::LlamaContext<'_>,
    cached_tokens: &mut Vec<LlamaToken>,
    n_ctx: u32,
    index: u32,
    req: &InferenceRequest,
    stream_tx: Option<&tokio::sync::mpsc::Sender<Result<StreamChunk, ErrorBody>>>,
) -> Result<SlotOutput, ErrorBody> {
    let (template_result, _count) = build_prompt(model, &req.messages, req.tools_json.as_deref())?;
    let new_tokens = model
        .str_to_token(&template_result.prompt, AddBos::Always)
        .map_err(|e| inference_failed(format!("tokenization failed: {e}")))?;
    let new_len = new_tokens.len();

    if new_len == 0 {
        return Err(inference_failed("empty prompt after templating"));
    }
    if new_len > n_ctx as usize {
        // Upstream model-aware compaction should keep prompts within the slot
        // window. Truncating here would corrupt BOS/template framing, so error
        // out instead and let the daemon re-compact.
        cached_tokens.clear();
        return Err(inference_failed(format!(
            "prompt of {new_len} tokens exceeds slot context window {n_ctx}"
        )));
    }

    // Longest common prefix with what is already in the KV cache, leaving at
    // least one token to decode so sampling has fresh logits.
    let p = cap_prefix(common_prefix_len(cached_tokens, &new_tokens), new_len);

    // Drop KV at positions >= p for sequence 0; on failure force a cold prefill.
    match ctx.clear_kv_cache_seq(Some(0), Some(p as u32), None) {
        Ok(true) => {}
        Ok(false) => {
            cached_tokens.clear();
            return Err(inference_failed("kv cache seq_rm returned false"));
        }
        Err(e) => {
            cached_tokens.clear();
            return Err(inference_failed(format!("kv cache seq_rm failed: {e}")));
        }
    }

    let reused = p;
    let prefill_start = Instant::now();
    let n_batch = ctx.n_batch() as usize;
    let last = new_len - 1;
    let mut batch = LlamaBatch::new(n_ctx as usize, 1);
    let mut pos = p;
    while pos < new_len {
        let chunk_end = (pos + n_batch.max(1)).min(new_len);
        batch.clear();
        for (offset, token) in new_tokens[pos..chunk_end].iter().enumerate() {
            let i = pos + offset;
            batch.add(*token, i as i32, &[0], i == last).map_err(|e| {
                cached_tokens.clear();
                inference_failed(format!("batch add failed: {e}"))
            })?;
        }
        ctx.decode(&mut batch).map_err(|e| {
            cached_tokens.clear();
            inference_failed(format!("prefill decode failed: {e}"))
        })?;
        pos = chunk_end;
    }
    let prefill_ms = prefill_start.elapsed().as_millis() as u64;

    tracing::debug!(
        slot = index,
        prompt_tokens = new_len,
        reused_prefix = reused,
        decoded = new_len - reused,
        "llm prefix reuse"
    );

    // Generation loop, bounded by the slot's actual window (`n_ctx`), so a
    // request can never ask for more tokens than the live context can hold.
    let prompt_tokens = new_len as u32;
    let effective_max = resolve_effective_max(n_ctx, prompt_tokens, req.max_tokens);
    let decode_start = Instant::now();
    let mut sampler = build_sampler(req, model, &template_result)?;
    let mut decoder = encoding_rs::UTF_8.new_decoder();
    let mut n_cur = new_len as i32;
    let n_max = n_cur + effective_max as i32;
    let mut sample_idx = batch.n_tokens() - 1;
    let mut generated = String::new();
    let mut generated_tokens: Vec<LlamaToken> = Vec::new();
    // Clean text already streamed to the consumer (tool-call tags stripped). Used
    // to compute the delta to send each token and to prefix-match the final parse.
    let mut emitted_text = String::new();
    let mut completion_tokens: u32 = 0;
    let mut finish_reason = FinishReason::Length;

    while n_cur < n_max {
        let token = sampler.sample(ctx, sample_idx);
        sampler.accept(token);

        if model.is_eog_token(token) {
            finish_reason = FinishReason::Eos;
            break;
        }

        let piece = model
            .token_to_piece(token, &mut decoder, true, None)
            .unwrap_or_default();

        // Accumulate first so the streaming cleaner sees the full output so far.
        generated.push_str(&piece);

        if let Some(tx) = stream_tx {
            // Stream only clean text: `<tool_call>` blocks are suppressed and a
            // partial opener is held back, so the consumer accumulates the same
            // content the non-streaming path produces and never sees raw tags.
            let clean = super::stream_clean_prefix(&generated);
            if let Some(delta) = clean.strip_prefix(emitted_text.as_str()) {
                if !delta.is_empty() {
                    let out = delta.to_string();
                    if tx
                        .blocking_send(Ok(StreamChunk {
                            text: out.clone(),
                            tool_calls: Vec::new(),
                            finish_reason: None,
                        }))
                        .is_err()
                    {
                        // Receiver dropped: uncommit this token (it was not yet
                        // decoded into KV) so the cached sequence stays consistent.
                        generated.truncate(generated.len() - piece.len());
                        finish_reason = FinishReason::Abort;
                        break;
                    }
                    emitted_text.push_str(&out);
                }
            }
        }
        // Record the token id: it is about to be decoded into the KV cache, so
        // the next turn of this conversation can reuse it as part of its prefix
        // instead of re-prefilling the reply. (EOG and stream-abort tokens break
        // out above, before this point, so they never enter the cached sequence.)
        generated_tokens.push(token);
        completion_tokens += 1;

        batch.clear();
        if let Err(e) = batch.add(token, n_cur, &[0], true) {
            cached_tokens.clear();
            return Err(inference_failed(format!("batch add failed: {e}")));
        }
        n_cur += 1;
        if let Err(e) = ctx.decode(&mut batch) {
            cached_tokens.clear();
            return Err(inference_failed(format!("decode failed: {e}")));
        }
        sample_idx = batch.n_tokens() - 1;
    }
    if completion_tokens >= effective_max {
        finish_reason = FinishReason::Length;
    }
    let decode_ms = decode_start.elapsed().as_millis() as u64;

    // Diagnostic: why generation stopped and against which budget. `length`
    // with completion_tokens == effective_max means the budget bound it;
    // `eos` means the model emitted an end-of-generation token on its own.
    tracing::info!(
        slot = index,
        finish = ?finish_reason,
        completion_tokens,
        effective_max,
        prompt_tokens,
        n_ctx,
        "llm generation stopped"
    );

    // The prompt AND the tokens we just generated are materialized in KV. Cache
    // the whole sequence so the next turn of this conversation reuses the full
    // prefix (prompt + prior completion) instead of re-prefilling the assistant
    // reply it now contains. This stays within the context window: generation is
    // bounded to the remaining window, so `new_len + completion_tokens <= n_ctx`.
    let mut sequence = new_tokens;
    sequence.append(&mut generated_tokens);
    *cached_tokens = sequence;

    // The complete path returns cleaned content plus structured tool calls.
    // The streaming path already forwarded the cleaned text live, so it returns
    // empty text; it recovers tool calls from the authoritative final parse (which
    // also covers native, non-tag formats) and flushes any clean text the final
    // parse resolved beyond what was streamed (e.g. a tail held back as a partial
    // opener that turned out to be plain text).
    let (text, tool_calls) = if let Some(tx) = stream_tx {
        let (clean, calls) = parse_completion(&template_result, generated);
        if let Some(delta) = clean.strip_prefix(emitted_text.as_str()) {
            if !delta.is_empty() {
                let _ = tx.blocking_send(Ok(StreamChunk {
                    text: delta.to_string(),
                    tool_calls: Vec::new(),
                    finish_reason: None,
                }));
            }
        }
        (String::new(), calls)
    } else {
        parse_completion(&template_result, generated)
    };

    Ok(SlotOutput {
        text,
        tool_calls,
        prompt_tokens,
        completion_tokens,
        finish_reason,
        prefill_ms,
        decode_ms,
    })
}

/// Length of the longest common prefix of two token slices.
pub(super) fn common_prefix_len(a: &[LlamaToken], b: &[LlamaToken]) -> usize {
    a.iter().zip(b.iter()).take_while(|(x, y)| x == y).count()
}

/// Cap the reuse length so at least one token is left to decode (sampling needs
/// fresh logits from a decoded token).
pub(super) fn cap_prefix(p: usize, new_len: usize) -> usize {
    if new_len == 0 {
        0
    } else {
        p.min(new_len - 1)
    }
}

/// Stable fingerprint of a conversation, used only for slot routing. Hashes the
/// opening messages (system + first user turn), which stay constant across the
/// turns of one conversation, so follow-ups route back to the holding slot.
pub(super) fn conversation_fingerprint(messages: &[ChatMessage]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    for m in messages.iter().take(2) {
        (m.role as u8).hash(&mut hasher);
        m.content.hash(&mut hasher);
    }
    // Avoid 0 (reserved for "never used").
    hasher.finish() | 1
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tok(id: i32) -> LlamaToken {
        LlamaToken(id)
    }

    #[test]
    fn common_prefix_full_match_returns_len() {
        let a = [tok(1), tok(2), tok(3)];
        let b = [tok(1), tok(2), tok(3)];
        assert_eq!(common_prefix_len(&a, &b), 3);
    }

    #[test]
    fn common_prefix_no_match_returns_zero() {
        let a = [tok(9), tok(2)];
        let b = [tok(1), tok(2)];
        assert_eq!(common_prefix_len(&a, &b), 0);
    }

    #[test]
    fn common_prefix_partial_returns_divergence_index() {
        let a = [tok(1), tok(2), tok(3), tok(9)];
        let b = [tok(1), tok(2), tok(3), tok(8)];
        assert_eq!(common_prefix_len(&a, &b), 3);
    }

    #[test]
    fn common_prefix_shorter_cache_returns_cache_len() {
        let cached = [tok(1), tok(2)];
        let new = [tok(1), tok(2), tok(3), tok(4)];
        assert_eq!(common_prefix_len(&cached, &new), 2);
    }

    #[test]
    fn common_prefix_empty_cache_returns_zero() {
        let cached: [LlamaToken; 0] = [];
        let new = [tok(1), tok(2)];
        assert_eq!(common_prefix_len(&cached, &new), 0);
    }

    #[test]
    fn cap_prefix_full_match_leaves_one_token() {
        assert_eq!(cap_prefix(5, 5), 4);
    }

    #[test]
    fn cached_prompt_plus_generated_is_reused_next_turn() {
        // GIVEN a first turn whose KV now holds prompt [1,2,3] plus the reply it
        // generated [4,5] (the sequence this slot caches after generation)
        let cached = [tok(1), tok(2), tok(3), tok(4), tok(5)];
        // WHEN the next turn's prompt is the whole conversation so far (prompt +
        // prior reply) followed by one new user token
        let next = [tok(1), tok(2), tok(3), tok(4), tok(5), tok(6)];
        // THEN the common prefix covers the entire prior sequence, so only the
        // new token is re-prefilled: the reply is no longer reprocessed. Before
        // caching generated tokens, the cache stopped at [1,2,3] and [4,5] were
        // decoded again every turn.
        assert_eq!(common_prefix_len(&cached, &next), 5);
    }

    #[test]
    fn cap_prefix_within_bounds_unchanged() {
        assert_eq!(cap_prefix(3, 5), 3);
    }

    #[test]
    fn cap_prefix_empty_prompt_is_zero() {
        assert_eq!(cap_prefix(0, 0), 0);
    }
}
