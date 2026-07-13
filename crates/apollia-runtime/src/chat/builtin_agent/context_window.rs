use super::*;

impl BuiltInChatAgent {
    /// Build the partial [`ChatAgentResponse`] returned when the stream was
    /// interrupted, emitting [`RuntimeEvent::ChatError`] and the completion event.
    pub(in crate::chat::builtin_agent) fn stream_error_response(
        &self,
        err: String,
        accumulated_text: String,
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
        } = ctx;
        // Stream interrupted: emit ChatError, return partial content
        let _ = self.event_bus.send(RuntimeEvent::ChatError {
            session_id: session_id.to_string(),
            message_id: Some(message_id.to_string()),
            error: err.clone(),
        });

        // Return partial content so the caller can save what was received
        let content = if accumulated_text.is_empty() {
            format!("[erreur streaming : {err}]")
        } else {
            accumulated_text
        };

        let _ = self.event_bus.send(RuntimeEvent::ChatResponseCompleted {
            session_id: session_id.to_string(),
            message_id: message_id.to_string(),
            content: content.clone(),
            run_id: Some(run_id.clone()),
        });

        ChatAgentResponse {
            content,
            tool_calls: acc.all_tool_calls,
            newly_authorized: acc.newly_authorized,
            tokens_used: total_usage,
            thinking_trace: None,
            verification_report: None,
            frontier_ceiling_reached,
            final_plan_phase,
            paused: false,
        }
    }

    /// Compact the LLM message buffer in place when it approaches the context
    /// limit, emitting [`RuntimeEvent::ContextCompacted`] on success.
    ///
    /// Returns `true` when a compaction actually occurred, so the caller can
    /// re-inject any state (such as the todo list) that the truncation dropped.
    pub(in crate::chat::builtin_agent) async fn maybe_compact_context(
        &self,
        llm_messages: &mut Vec<LlmChatMessage>,
        session_id: &str,
    ) -> bool {
        let (compacted, was_compacted) = self
            .context_manager
            .maybe_compact(llm_messages, &self.llm_router)
            .await;
        if !was_compacted {
            return false;
        }
        let summary_chars = compacted
            .get(1)
            .map(apollia_oria::context_manager::message_char_len)
            .unwrap_or(0);
        let original_messages = llm_messages.len();
        // PreCompact / PostCompact hooks (non-blocking, best-effort). Both fire
        // only when a compaction actually occurred. The context manager performs
        // the summarization atomically, so PreCompact fires immediately before
        // the compacted history is committed and PostCompact immediately after.
        if let Some(executor) = self.hook_executor.as_ref() {
            executor.run_pre_compact(session_id).await;
        }
        *llm_messages = compacted;
        tracing::info!(
            summary_chars,
            original_messages,
            session_id = %session_id,
            "ReAct context compacted before LLM call"
        );
        let _ = self
            .event_bus
            .send(apollia_core::RuntimeEvent::ContextCompacted {
                summary_chars,
                original_messages,
            });
        if let Some(executor) = self.hook_executor.as_ref() {
            executor
                .run_post_compact(summary_chars, original_messages, session_id)
                .await;
        }
        true
    }

    /// Re-inject the current todo list as a reminder message after a context
    /// compaction dropped the conversation history.
    ///
    /// Called only when a compaction actually occurred and a todo store is
    /// attached. Reads the list through the [`TodoHandle`] and, when non-empty,
    /// appends a single user-role reminder message enumerating each item with
    /// its status. The agent never loses track of pending work even when the
    /// history is truncated. Errors are logged and swallowed: the loop continues
    /// without the reminder rather than failing.
    ///
    /// The Markdown rendering is intentionally simple here; a structured
    /// JSON or XML format may replace it in a later iteration.
    pub(in crate::chat::builtin_agent) async fn inject_todo_after_compaction(
        todo: &TodoHandle,
        session_id: &str,
        messages: &mut Vec<LlmChatMessage>,
    ) {
        let items = match todo.get_items(session_id).await {
            Ok(items) => items,
            Err(e) => {
                warn!(
                    session_id = %session_id,
                    error = %e,
                    "todo.reinject.failed"
                );
                return;
            }
        };
        if items.is_empty() {
            return;
        }
        let mut reminder = String::from(TODO_REMINDER_PREFIX);
        for item in &items {
            reminder.push_str(&format!("\n- [{}] {}", item.status.as_str(), item.content));
        }
        messages.push(LlmChatMessage::user(reminder));
        tracing::info!(
            session_id = %session_id,
            todo_count = items.len(),
            compaction_triggered = true,
            "chat.todo.reinjected_after_compaction"
        );
    }
}
