//! Shaping of the exchange outcome into a [`ChatAgentResponse`].
//!
//! Three exits share one builder family: a pause checkpoint, a paused text
//! answer, and the converged final text.

use super::super::*;

impl BuiltInChatAgent {
    /// Build the [`ChatAgentResponse`] returned when the loop stops at a pause
    /// checkpoint.
    ///
    /// Carries the work already done this turn (tool-call records, reasoning,
    /// terminal plan phase) and sets `paused`. No `ChatResponseCompleted` event is
    /// emitted: the turn did not converge, it was suspended, and the manager
    /// records the session as paused so it can be resumed from the persisted plan
    /// state.
    pub(super) fn paused_response(
        &self,
        reasoning_fragments: &[(String, usize)],
        ctx: ResponseContext<'_>,
    ) -> ChatAgentResponse {
        let ResponseContext {
            acc,
            total_usage,
            session_id,
            frontier_ceiling_reached,
            final_plan_phase,
            context_window_tokens,
            context_tokens_used,
            ..
        } = ctx;
        let (thinking_trace, reasoning_boundaries) =
            Self::build_thinking_trace(reasoning_fragments);
        tracing::info!(
            session_id = %session_id,
            tool_calls = acc.all_tool_calls.len(),
            "chat.react.pause_response"
        );
        ChatAgentResponse {
            content: String::new(),
            tool_calls: acc.all_tool_calls,
            newly_authorized: acc.newly_authorized,
            tokens_used: total_usage,
            thinking_trace,
            reasoning_boundaries,
            verification_report: None,
            frontier_ceiling_reached,
            final_plan_phase,
            paused: true,
            context_window_tokens,
            context_tokens_used,
        }
    }

    /// Join the per-step reasoning fragments into the `thinking_trace` blob and
    /// return the parallel tool-call boundaries. `(None, empty)` when there is
    /// no reasoning.
    pub(super) fn build_thinking_trace(
        fragments: &[(String, usize)],
    ) -> (Option<String>, Vec<usize>) {
        if fragments.is_empty() {
            return (None, Vec::new());
        }
        let trace = fragments
            .iter()
            .map(|(text, _)| text.as_str())
            .collect::<Vec<_>>()
            .join("\n\n---\n\n");
        let boundaries = fragments.iter().map(|(_, before)| *before).collect();
        (Some(trace), boundaries)
    }

    /// Build a paused response that freezes the partial streamed text as the
    /// assistant turn.
    ///
    /// Used when the user stops generation mid-stream: the token stream was cut
    /// inside [`consume_stream`](Self::consume_stream), the accumulated partial
    /// is carried as `content` so the manager persists it, and `paused` is set so
    /// no [`RuntimeEvent::ChatResponseCompleted`] is emitted and the session is
    /// left resumable.
    pub(super) fn paused_text_response(
        &self,
        accumulated_text: &str,
        reasoning_fragments: &mut Vec<(String, usize)>,
        ctx: ResponseContext<'_>,
    ) -> ChatAgentResponse {
        let ResponseContext {
            acc,
            total_usage,
            session_id,
            frontier_ceiling_reached,
            final_plan_phase,
            context_window_tokens,
            context_tokens_used,
            ..
        } = ctx;
        let final_thinking = Self::extract_think_blocks(accumulated_text);
        let clean = Self::strip_think_blocks(accumulated_text);
        if let Some(ft) = &final_thinking {
            reasoning_fragments.push((ft.clone(), acc.all_tool_calls.len()));
        }
        let (thinking_trace, reasoning_boundaries) =
            Self::build_thinking_trace(reasoning_fragments);
        tracing::info!(
            session_id = %session_id,
            partial_len = clean.len(),
            "chat.react.pause_text_response"
        );
        ChatAgentResponse {
            content: clean,
            tool_calls: acc.all_tool_calls,
            newly_authorized: acc.newly_authorized,
            tokens_used: total_usage,
            thinking_trace,
            reasoning_boundaries,
            verification_report: None,
            frontier_ceiling_reached,
            final_plan_phase,
            paused: true,
            context_window_tokens,
            context_tokens_used,
        }
    }

    /// Build the final [`ChatAgentResponse`] when the LLM produced no tool calls.
    ///
    /// Combines the accumulated reasoning fragments with the final thinking
    /// trace and emits [`RuntimeEvent::ChatResponseCompleted`].
    pub(super) fn finalize_text_response(
        &self,
        accumulated_text: &str,
        reasoning_fragments: &mut Vec<(String, usize)>,
        ctx: ResponseContext<'_>,
    ) -> ChatAgentResponse {
        let ResponseContext {
            acc,
            total_usage,
            session_id,
            message_id,
            run_id,
            frontier_ceiling_reached,
            final_plan_phase,
            context_window_tokens,
            context_tokens_used,
        } = ctx;
        // Extract thinking trace before stripping.
        let final_thinking = Self::extract_think_blocks(accumulated_text);
        let clean = Self::strip_think_blocks(accumulated_text);
        let _ = self.event_bus.send(RuntimeEvent::ChatResponseCompleted {
            session_id: session_id.to_string(),
            message_id: message_id.to_string(),
            content: clean.clone(),
            run_id: Some(run_id.clone()),
        });

        // Combine accumulated reasoning fragments with final thinking.
        if let Some(ft) = &final_thinking {
            reasoning_fragments.push((ft.clone(), acc.all_tool_calls.len()));
        }
        let (thinking_trace, reasoning_boundaries) =
            Self::build_thinking_trace(reasoning_fragments);

        tracing::info!(
            fragment_count = reasoning_fragments.len(),
            has_trace = thinking_trace.is_some(),
            trace_len = thinking_trace.as_ref().map(|t| t.len()).unwrap_or(0),
            session_id = %session_id,
            "chat.react.completed"
        );

        ChatAgentResponse {
            content: clean,
            tool_calls: acc.all_tool_calls,
            newly_authorized: acc.newly_authorized,
            tokens_used: total_usage,
            thinking_trace,
            reasoning_boundaries,
            verification_report: None,
            frontier_ceiling_reached,
            final_plan_phase,
            paused: false,
            context_window_tokens,
            context_tokens_used,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// GIVEN two per-step fragments captured at tool-call counts 0 and 1
    /// WHEN build_thinking_trace runs
    /// THEN the trace joins them with the separator and the boundaries are kept
    #[test]
    fn test_build_thinking_trace_carries_boundaries() {
        let fragments = vec![("plan".to_string(), 0), ("act".to_string(), 1)];
        let (trace, boundaries) = BuiltInChatAgent::build_thinking_trace(&fragments);
        assert_eq!(trace.as_deref(), Some("plan\n\n---\n\nact"));
        assert_eq!(boundaries, vec![0, 1]);
    }

    /// GIVEN no fragments
    /// WHEN build_thinking_trace runs
    /// THEN it yields no trace and empty boundaries
    #[test]
    fn test_build_thinking_trace_empty() {
        let (trace, boundaries) = BuiltInChatAgent::build_thinking_trace(&[]);
        assert!(trace.is_none());
        assert!(boundaries.is_empty());
    }
}
