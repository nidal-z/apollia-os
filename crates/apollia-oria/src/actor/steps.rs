//! Single-step execution: payload resolution, tool and LLM steps, memory.
//!
//! Split out of `actor.rs`: the loop's state stays in the parent, what one
//! plan step does lives here.

use std::sync::Arc;

use apollia_core::events::RuntimeEvent;
use apollia_core::PendingApprovals;
use apollia_llm::{
    router::ObservabilityConfig as LlmObsConfig, ChatMessage, CompletionRequest, LlmRouter,
};

use crate::actor::{ToolProxyTrait, STEP_MEMORY_IMPORTANCE};
use crate::context_manager::message_char_len;
use crate::resilience::ResilienceLayer;

use crate::actor::{interpolate_outputs, truncate_chars, ActorLoop, StepContext, StepError};
use crate::plan::PlanStep;
use crate::resilience::{ErrorClass, RetryContext, RetryPolicy};

impl ActorLoop {
    /// Resolves the JSON payload passed to a tool step.
    ///
    /// Resolution order:
    /// 1. **Plan-time args (path A)**: if the step carries `args` and they
    ///    validate against the tool schema (or no schema is registered to check
    ///    against), use them verbatim.
    /// 2. **Just-in-time extraction (path B)**: if a schema is available, ask the
    ///    model to generate valid arguments from the step description, constrained
    ///    to that schema.
    /// 3. **Legacy fallback**: wrap the interpolated description as
    ///    `{"input": ...}`, preserving the historical behaviour for tools with a
    ///    trivial input contract and for backends without an LLM.
    ///
    /// The JIT call is not counted as a tool call: it produces the tool's
    /// arguments, it does not invoke the tool. The step's `is_exhausted()` guard
    /// upstream still bounds the run.
    // REASON: argument resolution needs the step, its interpolated description,
    // the tool name, the schema source (proxy) and the model source (router).
    // REASON: threads the actor's borrowed state through one step resolution; a struct would borrow the same fields.
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn resolve_step_payload(
        &self,
        step: &PlanStep,
        interpolated_description: &str,
        tool_name: &str,
        tool_proxy: &dyn ToolProxyTrait,
        llm_router: &LlmRouter,
    ) -> serde_json::Value {
        let schema = tool_proxy.tool_schema(tool_name).await;

        // Path A: plan-time args, accepted when valid (or unverifiable).
        if let Some(args) = step.args.as_ref() {
            let acceptable = match schema.as_ref() {
                Some(s) => crate::arg_resolver::validate_args(args, s).is_ok(),
                None => true,
            };
            if acceptable {
                return args.clone();
            }
            tracing::event!(
                tracing::Level::WARN,
                step_id = %step.step_id,
                tool = %tool_name,
                "oria.step.args_invalid_falling_back"
            );
        }

        // Path B: just-in-time extraction against the tool schema.
        if let Some(s) = schema.as_ref() {
            if let Some(model) = llm_router.get(step.model_hint.as_deref()) {
                match crate::arg_resolver::resolve_tool_args(
                    &model,
                    tool_name,
                    s,
                    interpolated_description,
                    0.0,
                )
                .await
                {
                    Ok(args) => return args,
                    Err(e) => tracing::event!(
                        tracing::Level::WARN,
                        step_id = %step.step_id,
                        tool = %tool_name,
                        error = %e,
                        "oria.step.jit_extraction_failed"
                    ),
                }
            }
        }

        // Legacy fallback.
        serde_json::json!({ "input": interpolated_description })
    }
    /// Execute a single step, tool or LLM depending on `tool_hint`.
    ///
    /// Before the actual execution, checks whether the step's tool is in
    /// `manifest.tools_requiring_approval`. If so and `pending_approvals` is
    /// configured, calls [`suspend_for_approval`] and waits for the human decision.
    ///
    /// - `tool_hint = Some("llm")` or `None`: LLM call, routed via `model_hint`
    ///   when present, otherwise the default backend. Previous outputs are
    ///   injected into the system message.
    /// - `tool_hint = Some(tool_name)`: call via `ToolProxyTrait::invoke`
    ///   (`model_hint` ignored for tool steps).
    ///
    /// Previous step outputs are interpolated into the step description via
    /// [`interpolate_outputs`] before being passed to the tool or the LLM.
    ///
    /// [`suspend_for_approval`]: ActorLoop::suspend_for_approval
    // REASON: cohesive execution dependencies (proxy, router, resilience) plus
    // the step context. A future consolidation may move the resilience layer
    // into the StepDeps bundle once the batch path needs it too.
    // REASON: threads the actor's borrowed state through one step execution; a struct would borrow the same fields.
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn execute_step(
        &self,
        step: &PlanStep,
        step_ctx: &StepContext,
        tool_proxy: &dyn ToolProxyTrait,
        llm_router: &LlmRouter,
        resilience: &ResilienceLayer,
    ) -> Result<String, StepError> {
        // Check whether the step's tool requires human approval.
        let tool_needs_approval = step
            .tool_hint
            .as_deref()
            .map(|t| {
                self.manifest
                    .tools_requiring_approval
                    .iter()
                    .any(|a| a == t)
            })
            .unwrap_or(false);

        if tool_needs_approval {
            if let Some(pending) = self.pending_approvals.as_ref() {
                self.suspend_for_approval(step, pending).await?;
            } else {
                tracing::warn!(
                    step_id = %step.step_id,
                    tool = ?step.tool_hint,
                    detail = "the sensitive step runs without approval",
                    "step.approval.unconfigured"
                );
            }
        }

        // Normal step execution after approval (or for a non-sensitive tool).
        let input = interpolate_outputs(&step.description, &step_ctx.previous_outputs);

        match step.tool_hint.as_deref() {
            // LLM step: routed to the backend specified by model_hint.
            // previous outputs injected into the system message.
            Some("llm") | None => {
                self.execute_llm_step(step, input, llm_router, step_ctx)
                    .await
            }
            // Tool step: model_hint ignored. The invocation is wrapped by the
            // ResilienceLayer so a flaky tool trips its circuit breaker and
            // transient failures are retried with backoff before bubbling up.
            Some(tool_name) => {
                resilience.ensure_tool(tool_name);
                let policy = RetryPolicy::default();
                let payload = self
                    .resolve_step_payload(step, &input, tool_name, tool_proxy, llm_router)
                    .await;
                let (outcome, _attempts) = resilience
                    .execute_with_observability(
                        RetryContext {
                            tool_name,
                            tool_call_id: step.step_id.as_str(),
                            retry_policy: &policy,
                            bus: Some(&self.event_bus),
                        },
                        Self::classify_tool_error,
                        || tool_proxy.invoke(tool_name, &payload),
                    )
                    .await;
                outcome.map_err(|e| StepError::ToolCallFailed(e.to_string()))
            }
        }
    }
    /// Maps a `ToolProxyTrait::invoke` error message to the [`ErrorClass`] that
    /// drives circuit-breaker and retry decisions.
    ///
    /// Tool invocations return their error as a plain `String`, so the class is
    /// inferred from the message. Unknown shapes default to `Transient` so a
    /// genuine transient fault is retried rather than silently dropped; the
    /// circuit breaker still bounds repeated transient failures.
    pub(super) fn classify_tool_error(err: &str) -> ErrorClass {
        let lower = err.to_lowercase();
        if lower.contains("budget") {
            ErrorClass::BudgetExceeded
        } else if lower.contains("sandbox")
            || lower.contains("path traversal")
            || lower.contains("unauthorized")
        {
            ErrorClass::SandboxViolation
        } else if lower.contains("not found")
            || lower.contains("invalid input")
            || lower.contains("invalid argument")
        {
            ErrorClass::Permanent
        } else {
            ErrorClass::Transient
        }
    }
    /// Execute an LLM call for a step, honoring `model_hint`.
    ///
    /// - If `model_hint = Some(hint)` and the backend exists in the `LlmRouter`,
    ///   the call is routed to that backend.
    /// - If `model_hint = Some(hint)` but the backend does not exist, a `tracing::warn!`
    ///   is emitted and the default backend is used as fallback.
    /// - If `model_hint = None`, the default backend is used.
    /// - if previous steps completed, their outputs are formatted into a system
    ///   message `"Previous step results:\n- s1: ..."`.
    pub(super) async fn execute_llm_step(
        &self,
        step: &PlanStep,
        input: String,
        llm_router: &LlmRouter,
        step_ctx: &StepContext,
    ) -> Result<String, StepError> {
        // Build messages: combine manifest system prompt and previous step outputs into a single
        // system message (preserved verbatim by ContextManager during compaction).
        // Omit the system message entirely when neither the manifest nor previous outputs
        // provide any content, preserving existing behaviour for simple steps.
        let system_text_opt = match (
            self.manifest.system_prompt.as_deref(),
            step_ctx.format_previous_outputs(),
        ) {
            (Some(sp), Some(ctx)) => Some(format!("{sp}\n\n{ctx}")),
            (Some(sp), None) => Some(sp.to_owned()),
            (None, Some(ctx)) => Some(ctx),
            (None, None) => None,
        };
        let mut messages: Vec<ChatMessage> = system_text_opt
            .map(|text| vec![ChatMessage::system(text), ChatMessage::user(input.clone())])
            .unwrap_or_else(|| vec![ChatMessage::user(input)]);

        // Compact context if it approaches the model's context limit.
        let (compacted, was_compacted) = self
            .context_manager
            .maybe_compact(&messages, llm_router)
            .await;
        if was_compacted {
            let summary_chars = compacted.get(1).map(message_char_len).unwrap_or(0);
            let original_messages = messages.len();
            messages = compacted;
            tracing::info!(
                summary_chars,
                original_messages,
                step_id = %step.step_id,
                "step.context.compacted"
            );
            let _ = self
                .event_bus
                .send(apollia_core::RuntimeEvent::ContextCompacted {
                    summary_chars,
                    original_messages,
                });
        }

        let request = CompletionRequest {
            messages,
            ..Default::default()
        };

        let backend_name = match &step.model_hint {
            Some(hint) => {
                if llm_router.get(Some(hint)).is_some() {
                    Some(hint.as_str())
                } else {
                    tracing::warn!(
                        step_id = %step.step_id,
                        model_hint = %hint,
                        detail = "falling back to the default backend",
                        "step.model_hint.unknown"
                    );
                    None
                }
            }
            None => None,
        };

        let obs = LlmObsConfig::default();
        let response = llm_router
            .complete_with_observability(backend_name, request, Some(&self.event_bus), &obs)
            .await
            .map_err(|e| StepError::LlmCallFailed(e.to_string()))?;

        Ok(response.content)
    }
    /// Suspend step execution and wait for the human decision (HITL Orchestrated mode).
    ///
    /// ## Sequence
    ///
    /// 1. Register a oneshot channel in `pending_approvals`, receiver `rx`.
    /// 2. Emit [`RuntimeEvent::TaskInputRequired`] with `step_id: Some(step.step_id)`
    ///    on the `EventBus` to notify the user.
    /// 3. Await `rx.await`: the `ResumeHandler` sends on the sender.
    /// 4. If `approved=true`: `Ok(())`, the step's tool runs normally.
    /// 5. If `approved=false`: `Err(StepError::RejectedByUser { reason })`.
    /// 6. If the channel is closed (runtime shutdown): `Err(StepError::ApprovalChannelClosed)`.
    ///
    /// **StepBudget paused during suspension**: the wait is a pure `await`,
    /// the step counter does not advance during the human suspension.
    pub(super) async fn suspend_for_approval(
        &self,
        step: &PlanStep,
        pending_approvals: &PendingApprovals,
    ) -> Result<(), StepError> {
        // Registration key: task_id + step_id to identify the suspension precisely.
        let approval_key = format!("{}::{}", self.plan.task_id, step.step_id);

        // 1. Register in PendingApprovals, get rx
        let rx = pending_approvals.register(&approval_key);

        // 2. Emit TaskInputRequired with step_id set (distinguishes Direct / Orchestrated mode)
        let prompt = format!(
            "Approval required before running '{}' (step: {})",
            step.tool_hint.as_deref().unwrap_or("llm"),
            step.step_id
        );
        let _ = self.event_bus.send(RuntimeEvent::TaskInputRequired {
            task_id: self.plan.task_id.clone().into(),
            prompt,
            step_id: Some(step.step_id.clone()),
        });

        tracing::info!(
            task_id = %self.plan.task_id,
            step_id = %step.step_id,
            tool = ?step.tool_hint,
            "step.approval.suspended"
        );

        // 3. Wait for the human decision (pure await: StepBudget does not advance)
        let response = rx.await.map_err(|_| StepError::ApprovalChannelClosed)?;

        tracing::info!(
            task_id = %self.plan.task_id,
            step_id = %step.step_id,
            approved = response.approved,
            "step.approval.decided"
        );

        // 4/5. Return based on the decision
        if response.approved {
            Ok(())
        } else {
            Err(StepError::RejectedByUser {
                reason: response
                    .reason
                    .unwrap_or_else(|| "Rejected by the user".into()),
            })
        }
    }
    /// Records an episodic memory entry for a completed step.
    ///
    /// Fire-and-forget: errors are logged as warnings but never interrupt execution.
    /// Skipped silently when `memory_manager` is `None` or when the agent manifest
    /// has no `memory_namespace` configured.
    ///
    /// Output is truncated to [`STEP_MEMORY_OUTPUT_MAX_CHARS`] characters.
    pub(super) fn record_step_memory(&self, step_id: &str, description: &str, output: &str) {
        // skip if no memory_manager or no namespace configured.
        let mm = match self.memory_manager.as_ref() {
            Some(mm) => mm,
            None => return,
        };
        let namespace = match self.manifest.memory_namespace.as_deref() {
            Some(ns) => ns,
            None => return,
        };

        let truncated_output = truncate_chars(output, self.step_memory_max_chars);
        let content = format!("step {step_id}: {description} -> {truncated_output}");
        let task_id = self.plan.task_id.clone();
        let agent_name = self.manifest.name.clone();
        let namespace_owned = namespace.to_string();
        let metadata = serde_json::json!({
            "source": "oria_orchestrated",
            "step_id": step_id,
        });

        let mm = Arc::clone(mm);
        // Fire-and-forget: spawn_blocking for the sync SQLite write.
        tokio::task::spawn_blocking(move || {
            let mut guard = match mm.lock() {
                Ok(g) => g,
                Err(e) => {
                    tracing::warn!(
                        error = %e,
                        step_id = %task_id,
                        detail = "ignored",
                        "step.memory.lock.failed"
                    );
                    return;
                }
            };
            let store = match guard.store(&namespace_owned) {
                Ok(s) => s,
                Err(e) => {
                    tracing::warn!(
                        error = %e,
                        namespace = %namespace_owned,
                        detail = "ignored",
                        "step.memory.store.open.failed"
                    );
                    return;
                }
            };
            let episodic = apollia_memory::episodic::EpisodicMemory::new(store);
            if let Err(e) = episodic.record(
                &namespace_owned,
                &agent_name,
                &content,
                STEP_MEMORY_IMPORTANCE,
                Some(task_id.as_str()),
                None,
                Some(&metadata),
            ) {
                // warn but don't interrupt execution.
                tracing::warn!(
                    error = %e,
                    namespace = %namespace_owned,
                    detail = "ignored",
                    "step.memory.record.failed"
                );
            }
        });
    }
}
