//! Plan-level execution: batches, sequential fallback, and tool fan-out.
//!
//! Split out of `actor.rs`: the loop's state stays in the parent, the code
//! that runs one topological level of a plan lives here.

use std::collections::HashMap;
use std::time::Instant;

use apollia_llm::LlmRouter;

use crate::actor::{ToolProxyTrait, MAX_CONCURRENT_ORIA_TOOLS};
use crate::resilience::ResilienceLayer;

use crate::actor::{
    budget_exhaustion_detail, interpolate_outputs, step_is_tool_call, ActorLoop, LevelOutcome,
    StepContext, StepDeps, StepError,
};
use crate::plan::PlanStep;
use crate::resilience::{RetryContext, RetryPolicy};

impl ActorLoop {
    /// Executes one topological level whose steps are all read-only tool calls,
    /// running them concurrently (batch path).
    ///
    /// Owns `completed_outputs` for the duration of the level and returns it via
    /// [`LevelOutcome::Continue`] when the whole level succeeds, or
    /// [`LevelOutcome::Terminal`] carrying the final [`AIPResult`] when the plan
    /// must stop (budget exhausted, replan, or terminal step failure).
    pub(super) async fn execute_level_batch<'a>(
        &'a mut self,
        level_steps: Vec<PlanStep>,
        mut completed_outputs: HashMap<String, String>,
        deps: StepDeps<'a>,
        resilience: &'a ResilienceLayer,
    ) -> LevelOutcome {
        // Phase 1 (sequential): budget guard, events, DB pre-execution.
        if deps.budget.is_exhausted() {
            return LevelOutcome::Terminal(
                self.fail_plan_budget_exhausted(&budget_exhaustion_detail(deps.budget)),
            );
        }

        // Clamp the level to what the budget still allows. Every batch step is a
        // read-only tool call that consumes one step and one tool-call unit, so
        // the level cannot run wider than min(steps_left, tool_calls_left).
        // Running the whole level unconditionally would let a batch overshoot the
        // budget (principle #7, guardrails are non-bypassable). The guard above
        // guarantees at least one unit remains here, so the head is never empty.
        let allowed = deps.budget.steps_left().min(deps.budget.tool_calls_left()) as usize;
        let budget_truncated = level_steps.len() > allowed;
        let level_steps: Vec<PlanStep> = if budget_truncated {
            level_steps.into_iter().take(allowed).collect()
        } else {
            level_steps
        };

        for step in &level_steps {
            let step_num = completed_outputs.len() + 1;
            self.persist_step_pre_execution(step, step_num, &completed_outputs);
        }

        // Phase 2: Concurrent invocations.
        let started = Instant::now();
        let batch_results = self
            .execute_tool_steps(
                &level_steps,
                &completed_outputs,
                deps.tool_proxy,
                deps.llm_router,
                resilience,
            )
            .await;
        let duration_ms = started.elapsed().as_millis() as u64;

        // Phase 3 (sequential): budget increment, DB post-execution, events, errors.
        for (step, (step_id, result)) in level_steps.iter().zip(batch_results) {
            deps.budget.increment_steps();
            // Every batch step is a native tool call (batch eligibility requires
            // a read-only tool_hint), so it consumes one tool-call budget unit.
            if step_is_tool_call(step) {
                deps.budget.increment_tool_calls();
            }
            if let Err(e) =
                self.db
                    .save_step_duration(&step_id, &self.plan.plan_id, duration_ms as i64)
            {
                tracing::warn!(
                    error = %e,
                    step_id = %step_id,
                    detail = "ignored",
                    "step.duration.persist.failed"
                );
            }
            match result {
                Ok(output) => {
                    self.persist_step_success(&step_id, &output, duration_ms);
                    self.record_step_memory(&step_id, &step.description, &output);
                    completed_outputs.insert(step_id, output);
                }
                Err(ref e) if e.is_retryable() && self.replan_count < self.max_replans => {
                    self.persist_step_failure(&step_id, &e.to_string());
                    self.emit_step_failed(&step_id, &e.to_string(), true);
                    return LevelOutcome::Terminal(
                        self.replan_and_continue(
                            step_id,
                            e.to_string(),
                            completed_outputs,
                            deps,
                            resilience,
                        )
                        .await,
                    );
                }
                Err(e) => {
                    return LevelOutcome::Terminal(
                        self.finalize_terminal_failure(&step_id, &e, true),
                    )
                }
            }
        }

        // The budget could not cover the whole level: stop cleanly after the
        // allowed prefix rather than silently dropping the remaining steps.
        if budget_truncated {
            return LevelOutcome::Terminal(
                self.fail_plan_budget_exhausted(&budget_exhaustion_detail(deps.budget)),
            );
        }

        LevelOutcome::Continue(completed_outputs)
    }
    /// Executes one topological level sequentially, processing each step one at
    /// a time (LLM steps, mutating tools, tools requiring approval, single-step
    /// levels).
    ///
    /// Mirrors [`execute_level_batch`](Self::execute_level_batch) for ownership
    /// of `completed_outputs` and the [`LevelOutcome`] return contract.
    pub(super) async fn execute_level_sequential<'a>(
        &'a mut self,
        level_ids: Vec<String>,
        mut completed_outputs: HashMap<String, String>,
        deps: StepDeps<'a>,
        resilience: &'a ResilienceLayer,
    ) -> LevelOutcome {
        for step_id in level_ids {
            let step = match self.plan.steps.iter().find(|s| s.step_id == step_id) {
                Some(s) => s.clone(),
                None => continue,
            };

            // check the budget before each step (steps, tool_calls, wall_clock).
            if deps.budget.is_exhausted() {
                return LevelOutcome::Terminal(
                    self.fail_plan_budget_exhausted(&budget_exhaustion_detail(deps.budget)),
                );
            }

            // Emit StepStarted + persist rendered input + tool name before execution.
            let step_num = completed_outputs.len() + 1;
            self.persist_step_pre_execution(&step, step_num, &completed_outputs);

            // build StepContext with accumulated outputs and budget snapshot.
            let step_ctx = StepContext {
                previous_outputs: completed_outputs.clone(),
                step_index: completed_outputs.len(),
                total_steps: self.plan.steps.len(),
                remaining_budget: deps.budget.to_budget_view(),
            };

            let started = Instant::now();
            let result = self
                .execute_step(
                    &step,
                    &step_ctx,
                    deps.tool_proxy,
                    deps.llm_router,
                    resilience,
                )
                .await;
            let duration_ms = started.elapsed().as_millis() as u64;
            deps.budget.increment_steps();
            // A native tool step consumes one tool-call budget unit (the
            // max_tool_calls guardrail). The next is_exhausted() check stops the
            // plan cleanly once the ceiling is reached.
            if step_is_tool_call(&step) {
                deps.budget.increment_tool_calls();
            }

            // persist duration unconditionally.
            if let Err(e) =
                self.db
                    .save_step_duration(&step_id, &self.plan.plan_id, duration_ms as i64)
            {
                tracing::warn!(
                    error = %e,
                    step_id = %step_id,
                    detail = "ignored",
                    "step.duration.persist.failed"
                );
            }

            match result {
                Ok(output) => {
                    self.persist_step_success(&step_id, &output, duration_ms);

                    // record episodic memory per step (fire-and-forget).
                    self.record_step_memory(&step_id, &step.description, &output);

                    completed_outputs.insert(step_id, output);
                }

                Err(ref e) if e.is_retryable() && self.replan_count < self.max_replans => {
                    self.persist_step_failure(&step_id, &e.to_string());
                    self.emit_step_failed(&step_id, &e.to_string(), true);
                    return LevelOutcome::Terminal(
                        self.replan_and_continue(
                            step_id,
                            e.to_string(),
                            completed_outputs,
                            deps,
                            resilience,
                        )
                        .await,
                    );
                }

                Err(e) => {
                    return LevelOutcome::Terminal(
                        self.finalize_terminal_failure(&step_id, &e, true),
                    )
                }
            }
        }

        LevelOutcome::Continue(completed_outputs)
    }
    /// Executes a batch of tool-only steps, parallelising when all tools are read-only.
    ///
    /// **Parallel path**: when every step in `steps` targets a read-only tool
    /// (as reported by [`ToolProxyTrait::is_tool_read_only`]) **and** no tool in the
    /// batch requires human approval, all invocations are driven concurrently via
    /// `futures::stream::StreamExt::buffered` with a cap of
    /// `MAX_CONCURRENT_READ_TOOLS` simultaneous calls.
    ///
    /// **Serial path**: in all other cases (LLM steps, mutating tools, tools
    /// requiring approval, or a batch of one): invocations run sequentially.
    ///
    /// Output order matches input order in both paths.
    /// Inputs are interpolated from `completed_outputs` before invocation.
    // REASON: batch execution dependencies (proxy, router, resilience) plus the
    // step set and accumulated outputs; the router is needed to resolve each
    // step's arguments before invocation.
    // REASON: threads the actor's borrowed state through one execution pass; a struct would borrow the same fields.
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn execute_tool_steps(
        &self,
        steps: &[PlanStep],
        completed_outputs: &HashMap<String, String>,
        tool_proxy: &dyn ToolProxyTrait,
        llm_router: &LlmRouter,
        resilience: &ResilienceLayer,
    ) -> Vec<(String, Result<String, StepError>)> {
        use futures::stream::{self, StreamExt};

        if steps.is_empty() {
            return vec![];
        }

        let all_read_only = steps.len() > 1
            && steps.iter().all(|s| {
                s.tool_hint.as_deref().is_some_and(|t| {
                    t != "llm"
                        && tool_proxy.is_tool_read_only(t)
                        && !self
                            .manifest
                            .tools_requiring_approval
                            .iter()
                            .any(|a| a == t)
                })
            });

        // Pre-compute per-call data to avoid lifetime issues with the async stream.
        let tool_names: Vec<String> = steps
            .iter()
            .map(|s| s.tool_hint.clone().unwrap_or_default())
            .collect();
        let mut inputs: Vec<serde_json::Value> = Vec::with_capacity(steps.len());
        for s in steps {
            let interpolated = interpolate_outputs(&s.description, completed_outputs);
            let tool_name = s.tool_hint.clone().unwrap_or_default();
            let payload = self
                .resolve_step_payload(s, &interpolated, &tool_name, tool_proxy, llm_router)
                .await;
            inputs.push(payload);
        }
        let step_ids: Vec<String> = steps.iter().map(|s| s.step_id.clone()).collect();

        // Register a breaker for every tool so the resilience pre_check never
        // trips on an unknown tool.
        for name in &tool_names {
            resilience.ensure_tool(name);
        }

        // Runs one tool call wrapped by the ResilienceLayer. pre_check (which
        // short-circuits an open breaker without invoking the tool), retry with
        // backoff, and success/failure recording all happen inside the layer.
        // Returns the original step id paired with the outcome so results can be
        // collected in input order.
        let run_one = |i: usize| {
            let tool_name = tool_names[i].clone();
            let input = inputs[i].clone();
            let step_id = step_ids[i].clone();
            async move {
                let policy = RetryPolicy::default();
                let (outcome, _attempts) = resilience
                    .execute_with_observability(
                        RetryContext {
                            tool_name: &tool_name,
                            tool_call_id: &step_id,
                            retry_policy: &policy,
                            bus: Some(&self.event_bus),
                        },
                        Self::classify_tool_error,
                        || tool_proxy.invoke(&tool_name, &input),
                    )
                    .await;
                (
                    step_id.clone(),
                    outcome.map_err(|e| StepError::ToolCallFailed(e.to_string())),
                )
            }
        };

        if all_read_only {
            stream::iter((0..steps.len()).map(&run_one))
                .buffered(MAX_CONCURRENT_ORIA_TOOLS)
                .collect::<Vec<_>>()
                .await
        } else {
            let mut results = Vec::with_capacity(steps.len());
            for i in 0..steps.len() {
                results.push(run_one(i).await);
            }
            results
        }
    }
    /// Returns `true` when every step in `level_steps` is a read-only tool call
    /// that does not require human approval, making the level eligible for
    /// concurrent batch execution. A single-step level is never eligible.
    pub(super) fn is_batch_eligible(
        &self,
        level_steps: &[PlanStep],
        tool_proxy: &dyn ToolProxyTrait,
    ) -> bool {
        level_steps.len() > 1
            && level_steps.iter().all(|s| {
                s.tool_hint.as_deref().is_some_and(|t| {
                    t != "llm"
                        && tool_proxy.is_tool_read_only(t)
                        && !self
                            .manifest
                            .tools_requiring_approval
                            .iter()
                            .any(|a| a == t)
                })
            })
    }
}
