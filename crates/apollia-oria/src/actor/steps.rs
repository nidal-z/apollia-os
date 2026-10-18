//! Single-step execution: payload resolution, tool and LLM steps, memory.
//!
//! Split out of `actor.rs`: the loop's state stays in the parent, what one
//! plan step does lives here.

use std::sync::Arc;

use apollia_llm::{
    router::ObservabilityConfig as LlmObsConfig, ChatMessage, CompletionRequest, LlmRouter,
    StreamChunk,
};
use futures::StreamExt;

/// The most one model step of a plan may generate. An intermediate step
/// extracts, compares or drafts: a few hundred tokens. With no limit, a model
/// that looped generated for over an hour, holding the engine, and the batch
/// waited behind it with nothing on the screen.
const STEP_MAX_TOKENS: u32 = 4_096;

/// The temperature of an intermediate step: it extracts and compares facts,
/// wanted the same from one item to the next.
const STEP_TEMPERATURE: f32 = 0.2;

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
            if let Some(model) = llm_router.get(self.agent_backend(llm_router)) {
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
    /// A step asks nobody before it runs: the tool proxy decides each call
    /// where it is made, and a call it refuses comes back as a failed step.
    ///
    /// - `tool_hint = Some("llm")` or `None`: LLM call on the agent's model
    ///   (see [`ActorLoop::agent_backend`]). Previous outputs are injected into
    ///   the system message.
    /// - `tool_hint = Some(tool_name)`: call via `ToolProxyTrait::invoke`.
    ///
    /// Previous step outputs are interpolated into the step description via
    /// [`interpolate_outputs`] before being passed to the tool or the LLM.
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
        let input = interpolate_outputs(&step.description, &step_ctx.previous_outputs);

        match step.tool_hint.as_deref() {
            // LLM step: on the agent's model, previous outputs injected into
            // the system message.
            Some("llm") | None => {
                self.execute_llm_step(step, input, llm_router, step_ctx)
                    .await
            }
            // Tool step. The invocation is wrapped by the
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
    /// The backend every model call of a step goes to: the model of the agent
    /// (the one its run was given) when the router holds it, the router's
    /// default otherwise.
    ///
    /// A step never picks its own model. A plan may still carry a
    /// `model_hint`, written by an earlier planner or by the model on its own;
    /// it is not read, so a run stays on the one model its agent was given.
    pub(super) fn agent_backend<'a>(&'a self, llm_router: &LlmRouter) -> Option<&'a str> {
        let backend = self.model_backend.as_deref()?;
        if llm_router.get(Some(backend)).is_some() {
            Some(backend)
        } else {
            tracing::event!(
                tracing::Level::WARN,
                backend = %backend,
                detail = "falling back to the default backend",
                "step.agent_backend.unknown"
            );
            None
        }
    }
    /// Execute an LLM call for a step, on the agent's model.
    ///
    /// - The call goes to [`ActorLoop::agent_backend`].
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

        // An intermediate step does its one job without reasoning first: the
        // plan already says what to do, and reasoning on every step tripled
        // the time of a batch item. The written answer still reasons.
        let request = CompletionRequest {
            messages,
            max_tokens: Some(STEP_MAX_TOKENS),
            thinking: Some(false),
            temperature: Some(STEP_TEMPERATURE),
            // Recorded under the task and the step, so the time of each
            // step of a batch item is measured.
            call_context: Some(apollia_llm::CallContext {
                session_id: self.plan.task_id.clone(),
                turn_id: format!("step:{}", step.step_id),
            }),
            ..Default::default()
        };

        let backend_name = self.agent_backend(llm_router);

        let obs = LlmObsConfig::default();
        // Streamed, so what the step writes is seen as it is written, under
        // the task, as the conversation sees a turn.
        let written = match llm_router
            .stream_with_observability(backend_name, request.clone(), Some(&self.event_bus), &obs)
            .await
        {
            Ok(mut stream) => {
                let mut text = String::new();
                while let Some(chunk) = stream.next().await {
                    match chunk {
                        Ok(StreamChunk::Text(token)) => {
                            let _ = self.event_bus.send(apollia_core::RuntimeEvent::ChatToken {
                                session_id: self.plan.task_id.clone(),
                                message_id: format!("{}:{}", self.plan.plan_id, step.step_id),
                                token: token.clone(),
                            });
                            text.push_str(&token);
                        }
                        Ok(_) => {}
                        Err(e) => return Err(StepError::LlmCallFailed(e.to_string())),
                    }
                }
                text
            }
            Err(_) => {
                llm_router
                    .complete_with_observability(backend_name, request, Some(&self.event_bus), &obs)
                    .await
                    .map_err(|e| StepError::LlmCallFailed(e.to_string()))?
                    .content
            }
        };

        Ok(apollia_llm::reasoning_markers::strip_reasoning(&written)
            .trim()
            .to_string())
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
