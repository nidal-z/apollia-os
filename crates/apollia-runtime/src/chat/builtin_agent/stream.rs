use super::*;

impl BuiltInChatAgent {
    /// Consume a token stream, emitting [`RuntimeEvent::ChatToken`] for each token
    /// and accumulating text in `accumulated_text`.
    ///
    /// Returns the list of tool calls found in the stream (empty if none).
    /// On stream error, returns the error message; the caller can use the
    /// partially accumulated text.
    pub(in crate::chat::builtin_agent) async fn consume_stream(
        &self,
        mut stream: std::pin::Pin<
            Box<dyn futures::Stream<Item = Result<StreamChunk, apollia_llm::LlmError>> + Send>,
        >,
        session_id: &str,
        message_id: &str,
        accumulated_text: &mut String,
    ) -> Result<Vec<ToolCall>, String> {
        let mut tool_calls = Vec::new();

        while let Some(chunk_result) = stream.next().await {
            match chunk_result {
                Ok(StreamChunk::Text(token)) => {
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
                Err(e) => {
                    // Stream interrupted
                    warn!(
                        session_id = %session_id,
                        error = %e,
                        "LLM stream interrupted"
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
