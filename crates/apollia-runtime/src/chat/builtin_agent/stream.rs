use super::*;

/// Identifiers and control handle threaded into [`BuiltInChatAgent::consume_stream`].
pub(in crate::chat::builtin_agent) struct StreamConsumeParams<'a> {
    /// Session the tokens belong to (for `ChatToken` events).
    pub session_id: &'a str,
    /// Assistant message id the tokens accumulate into.
    pub message_id: &'a str,
    /// Cooperative stop token: cancellation ends the stream at the next chunk.
    pub cancel: &'a tokio_util::sync::CancellationToken,
    /// Instant the completion request was dispatched, owned by the caller.
    ///
    /// Time to first token is measured from here rather than from the start of
    /// this function, because the backend awaits the first chunk before handing
    /// the stream over: prefill has already elapsed by the time consumption
    /// begins. Threading the origin in is what keeps the measured interval the
    /// one the contract defines.
    pub dispatched_at: std::time::Instant,
}

impl BuiltInChatAgent {
    /// Consume a token stream, emitting [`RuntimeEvent::ChatToken`] for each token
    /// and accumulating text in `accumulated_text`.
    ///
    /// Returns the list of tool calls found in the stream (empty if none). Any
    /// terminal [`StreamChunk::Usage`] is merged into `usage`, so the caller can
    /// fold this call's token accounting into the exchange total. On stream error,
    /// returns the error message; the caller can use the partially accumulated
    /// text (and whatever usage was reported before the error).
    pub(in crate::chat::builtin_agent) async fn consume_stream(
        &self,
        mut stream: std::pin::Pin<
            Box<dyn futures::Stream<Item = Result<StreamChunk, apollia_llm::LlmError>> + Send>,
        >,
        params: StreamConsumeParams<'_>,
        accumulated_text: &mut String,
        usage: &mut TokenUsage,
    ) -> Result<Vec<ToolCall>, String> {
        let StreamConsumeParams {
            session_id,
            message_id,
            cancel,
            dispatched_at,
        } = params;
        let mut tool_calls = Vec::new();
        // Turn instrumentation only, inert outside an instrumented turn where
        // the recorder entry points are no-ops.
        let mut first_token_seen = false;

        loop {
            // Race the stop token against the next chunk so a Stop takes effect
            // immediately, even while the model is "thinking" (a slow completion
            // with no chunks arriving yet). Without the select, we would be
            // parked on `stream.next().await` and the cooperative check would
            // only fire once the next token lands, so Stop appeared to work
            // during streaming but not during the thinking phase. `biased`
            // checks the token first. The accumulated text stays as the frozen
            // partial; the caller returns a paused response.
            let chunk_result = tokio::select! {
                biased;
                () = cancel.cancelled() => {
                    // A Stop can land while the terminal usage chunk is already
                    // in flight; without a drain, every cancelled turn loses its
                    // token accounting. Briefly drain the stream for accounting
                    // chunks only; text and tool calls stay frozen as they were
                    // at the checkpoint. An aborted generation may legitimately
                    // produce no usage at all, in which case the caller falls
                    // back to the previous iteration's prompt size.
                    Self::drain_accounting_chunks(&mut stream, usage).await;
                    break;
                }
                next = stream.next() => match next {
                    Some(chunk) => chunk,
                    None => break,
                },
            };
            match chunk_result {
                Ok(StreamChunk::Text(token)) => {
                    // Time to first token, client-observed: from the dispatch of
                    // the request to the first content delta, which is exactly
                    // what the user waits through.
                    if !first_token_seen {
                        first_token_seen = true;
                        crate::perf_trace::iteration_ttft(
                            dispatched_at.elapsed().as_secs_f64() * 1000.0,
                        );
                    }
                    // Emit ChatToken and accumulate
                    let _ = self.event_bus.send(RuntimeEvent::ChatToken {
                        session_id: session_id.to_string(),
                        message_id: message_id.to_string(),
                        token: token.clone(),
                    });
                    accumulated_text.push_str(&token);
                }
                Ok(StreamChunk::ToolCall(call)) => {
                    // Tool call detected in stream
                    tool_calls.push(call);
                }
                Ok(StreamChunk::Usage(chunk_usage)) => {
                    // Terminal token accounting for this call; fold it in.
                    usage.merge(&chunk_usage);
                }
                Ok(StreamChunk::Timings(timings)) => {
                    // Already emitted as an event by the backend that produced
                    // them. Kept here so the turn decomposition can attribute
                    // engine time to this iteration.
                    crate::perf_trace::iteration_engine_timings(&timings);
                }
                Err(e) => {
                    // Stream interrupted
                    warn!(
                        session_id = %session_id,
                        error = %e,
                        "llm.stream.interrupted"
                    );
                    return Err(e.to_string());
                }
            }
        }

        // Tool calls now arrive as structured `StreamChunk::ToolCall` from every
        // backend: cloud providers emit them natively, and the local runner
        // decodes them through the GGUF's own chat-template parser (common_chat)
        // before returning. No text-level `<tool_call>` scraping is needed.
        Ok(tool_calls)
    }

    /// Fold any in-flight accounting chunks into `usage` after a cancellation.
    ///
    /// Bounded: stops at the first per-item timeout, at end of stream, or on
    /// error, so a Stop never waits on a model that keeps generating. Content
    /// chunks are deliberately dropped: the turn is frozen at its checkpoint
    /// and only the measurement is worth salvaging.
    async fn drain_accounting_chunks(
        stream: &mut std::pin::Pin<
            Box<dyn futures::Stream<Item = Result<StreamChunk, apollia_llm::LlmError>> + Send>,
        >,
        usage: &mut TokenUsage,
    ) {
        const DRAIN_ITEM_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(200);
        loop {
            match tokio::time::timeout(DRAIN_ITEM_TIMEOUT, stream.next()).await {
                Ok(Some(Ok(StreamChunk::Usage(chunk_usage)))) => {
                    usage.merge(&chunk_usage);
                }
                Ok(Some(Ok(StreamChunk::Timings(timings)))) => {
                    crate::perf_trace::iteration_engine_timings(&timings);
                }
                Ok(Some(Ok(_))) => {}
                Ok(Some(Err(_)) | None) | Err(_) => break,
            }
        }
    }

    /// Extracts the content of `<think>...</think>` blocks from reasoning models.
    ///
    /// Returns the concatenated thinking text if any blocks are found, or `None`.
    /// Called before [`strip_think_blocks`] to capture reasoning for metadata.
    pub(in crate::chat::builtin_agent) fn extract_think_blocks(text: &str) -> Option<String> {
        let tag_open = "<think>";
        let tag_close = "</think>";
        let mut blocks = Vec::new();
        let mut cursor = 0;

        while let Some(start) = text[cursor..].find(tag_open) {
            let after_open = cursor + start + tag_open.len();
            if let Some(end) = text[after_open..].find(tag_close) {
                let block = text[after_open..after_open + end].trim();
                if !block.is_empty() {
                    blocks.push(block.to_string());
                }
                cursor = after_open + end + tag_close.len();
            } else {
                // Unclosed <think> tag, capture remaining as partial thinking.
                let block = text[after_open..].trim();
                if !block.is_empty() {
                    blocks.push(block.to_string());
                }
                break;
            }
        }

        if blocks.is_empty() {
            None
        } else {
            Some(blocks.join("\n\n"))
        }
    }

    /// Strips `<think>...</think>` blocks emitted by reasoning models (e.g. Qwen3).
    ///
    /// Called before re-injecting the assistant's turn into `llm_messages` and before
    /// returning the final content to the user. This prevents thinking tokens from
    /// polluting the context window across turns.
    pub(in crate::chat::builtin_agent) fn strip_think_blocks(text: &str) -> String {
        let tag_open = "<think>";
        let tag_close = "</think>";
        let mut result = String::with_capacity(text.len());
        let mut cursor = 0;

        while let Some(start) = text[cursor..].find(tag_open) {
            result.push_str(&text[cursor..cursor + start]);
            let after_open = cursor + start + tag_open.len();
            if let Some(end) = text[after_open..].find(tag_close) {
                cursor = after_open + end + tag_close.len();
            } else {
                // Unclosed <think> tag, discard everything after it.
                break;
            }
        }
        result.push_str(&text[cursor..]);
        result.trim().to_string()
    }

    /// Build the [`ErrorAnalysis`] attached to a `ChatToolCallCompleted` event.
    ///
    /// On failure, classifies the raw output via [`crate::analyzers::classify_tool_error`]
    /// and, if the user has opted in, enriches the message via the meta-LLM.
    /// On success, runs only the hallucination heuristic (zero-cost) and
    /// returns `Some(...)` only when the heuristic flags the output.
    pub(in crate::chat::builtin_agent) async fn build_error_analysis(
        &self,
        session_id: &str,
        tool_name: &str,
        output: &str,
        success: bool,
    ) -> Option<apollia_core::ErrorAnalysis> {
        use crate::analyzers::hallucination_detector::analysis_from_report;
        use crate::analyzers::{classify_tool_error, detect_hallucination, enrich_with_llm};

        if !success {
            let analysis = classify_tool_error(output);
            let analysis = if let Some(handle) = self.meta_handle.as_ref() {
                let context = format!("tool={tool_name}");
                enrich_with_llm(handle, analysis, &context, session_id).await
            } else {
                analysis
            };
            return Some(analysis);
        }

        // Success path: only flag if the heuristic fires (no schema
        // validators are wired up yet; those come with the per-tool registry).
        let report = detect_hallucination(output, None);
        if report.is_suspect() {
            Some(analysis_from_report(&report, output))
        } else {
            None
        }
    }
}
