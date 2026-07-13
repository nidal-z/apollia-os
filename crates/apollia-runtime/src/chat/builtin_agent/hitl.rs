use super::*;

impl BuiltInChatAgent {
    /// Fire the `PostToolUse` hooks for a completed tool call, returning any
    /// requested context injection. No-op (returns `None`) without an executor.
    pub(in crate::chat::builtin_agent) async fn fire_post_tool_use(
        &self,
        tool_name: &str,
        output: &str,
        success: bool,
        session_id: &str,
    ) -> Option<String> {
        let executor = self.hook_executor.as_ref()?;
        executor
            .run_post_tool_use(tool_name, output, success, session_id)
            .await
    }

    /// Run the blocking `PreToolUse` hooks over every call in a turn.
    ///
    /// Returns the working set to execute (with any `Rewrite` applied) plus a
    /// per-call refusal reason. When no hook executor is attached, or no
    /// `PreToolUse` handler is registered, this borrows the original calls and
    /// reports no denials, so the loop incurs no extra work. Decisions are
    /// traced with structured fields: `allow` at debug, `rewrite` at info; the
    /// `deny` warn is emitted at the blocking site in the loop.
    pub(in crate::chat::builtin_agent) async fn apply_pre_tool_use<'a>(
        &self,
        tool_calls: &'a [ToolCall],
        session_id: &str,
        run_id: &RunId,
    ) -> PreToolUseOutcome<'a> {
        let no_op = || PreToolUseOutcome {
            calls: std::borrow::Cow::Borrowed(tool_calls),
            denied: vec![None; tool_calls.len()],
        };
        let Some(executor) = self.hook_executor.as_ref() else {
            return no_op();
        };
        if executor
            .registry()
            .handlers_for(apollia_core::HookEventKind::PreToolUse)
            .is_empty()
        {
            return no_op();
        }

        let mut calls = tool_calls.to_vec();
        let mut denied: Vec<Option<String>> = vec![None; tool_calls.len()];
        for (i, call) in tool_calls.iter().enumerate() {
            // Record the decision on the bus for the live PreToolUse log.
            // `rewritten` is set only on the rewrite branch.
            let (decision, rewritten): (&str, Option<String>) = match executor
                .run_pre_tool_use(&call.name, &call.arguments, session_id)
                .await
            {
                HookDecision::Allow => {
                    tracing::debug!(
                        tool_name = %call.name,
                        decision = "allow",
                        session_id = %session_id,
                        "hook.pretooluse.decision"
                    );
                    ("allow", None)
                }
                HookDecision::Rewrite { arguments } => {
                    let rewritten = serde_json::to_string(&arguments).unwrap_or_default();
                    tracing::info!(
                        tool_name = %call.name,
                        decision = "rewrite",
                        original_args = %truncate_preview(
                            &serde_json::to_string(&call.arguments).unwrap_or_default()
                        ),
                        rewritten_args = %truncate_preview(&rewritten),
                        session_id = %session_id,
                        "hook.pretooluse.decision"
                    );
                    calls[i].arguments = arguments;
                    ("rewrite", Some(rewritten))
                }
                HookDecision::Deny { reason } => {
                    denied[i] = Some(reason);
                    ("deny", None)
                }
            };
            let _ = self.event_bus.send(RuntimeEvent::HookDecisionRecorded {
                run_id: run_id.clone(),
                session_id: session_id.to_string(),
                tool_name: call.name.clone(),
                decision: decision.to_string(),
                rewritten_args: rewritten,
            });
        }
        PreToolUseOutcome {
            calls: std::borrow::Cow::Owned(calls),
            denied,
        }
    }
}
