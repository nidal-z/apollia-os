//! Replanning after a step failure, and resuming the remainder.
//!
//! Split out of `actor.rs`: the loop's state stays in the parent, the path
//! that asks the reasoner for a new plan and continues live here.

use std::collections::HashMap;
use std::time::Instant;

use apollia_core::events::RuntimeEvent;
use apollia_core::AIPResult;

use crate::actor::build_replan_context;
use crate::resilience::ResilienceLayer;
use crate::topo::topological_sort;

use crate::actor::{budget_exhaustion_detail, step_is_tool_call, ActorLoop, StepContext, StepDeps};
use crate::plan::PlanStep;

impl ActorLoop {
    /// Trigger a replan after the retryable failure of a step.
    ///
    /// Increments `replan_count`, emits [`RuntimeEvent::PlanReplanning`],
    /// calls `Reasoner::replan()`, updates the SQLite plan and internal state,
    /// then delegates the rest to [`execute_remaining`](Self::execute_remaining).
    ///
    /// Returns `MAX_REPLAN_EXCEEDED` if the Reasoner fails.
    ///
    /// Returns a boxed `Future` to allow mutual recursion with
    /// [`execute_remaining`](Self::execute_remaining).
    // REASON: replanning needs the failed step, the error, accumulated outputs,
    // the execution deps and the resilience layer; the set is cohesive. A future
    // consolidation may move the resilience layer into the StepDeps bundle.
    // REASON: threads the actor's borrowed state into the recursive replan future; a struct would borrow the same fields.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn replan_and_continue<'a>(
        &'a mut self,
        failed_step_id: String,
        error_message: String,
        completed_outputs: HashMap<String, String>,
        deps: StepDeps<'a>,
        resilience: &'a ResilienceLayer,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = AIPResult> + Send + 'a>> {
        Box::pin(async move {
            self.replan_count += 1;
            let attempt = self.replan_count;

            let _ = self.event_bus.send(RuntimeEvent::PlanReplanning {
                task_id: self.plan.task_id.clone().into(),
                plan_id: self.plan.plan_id.clone(),
                attempt,
                failed_step: failed_step_id.clone(),
                reason: error_message.clone(),
            });

            // Build a minimal context for the Reasoner.
            let ctx = build_replan_context(&self.plan);

            let new_plan = match deps
                .reasoner
                .replan(&ctx, &completed_outputs, &failed_step_id, &error_message)
                .await
            {
                Ok(p) => p,
                Err(e) => {
                    if let Err(db_err) = self.db.fail_plan(&self.plan.plan_id, &e.to_string()) {
                        tracing::warn!(
                            error = %db_err,
                            detail = "ignored",
                            "plan.fail.persist.failed"
                        );
                    }
                    let _ = self.event_bus.send(RuntimeEvent::PlanFailed {
                        task_id: self.plan.task_id.clone().into(),
                        plan_id: self.plan.plan_id.clone(),
                        reason: e.to_string(),
                    });
                    return AIPResult::failed("REPLAN_FAILED", &e.to_string());
                }
            };

            // Update SQLite: begin_replan removes pending steps, then we reinsert.
            if let Err(e) = self.db.begin_replan(&self.plan.plan_id, self.replan_count) {
                tracing::warn!(error = %e, detail = "ignored", "plan.replan.persist.failed");
            }
            if let Err(e) = self.db.insert_steps(&self.plan.plan_id, &new_plan.steps) {
                tracing::warn!(error = %e, detail = "ignored", "plan.steps.persist.failed");
            }

            let _ = self.event_bus.send(RuntimeEvent::PlanGenerated {
                task_id: self.plan.task_id.clone().into(),
                agent_name: String::new(),
                plan_id: self.plan.plan_id.clone(),
                step_count: new_plan.steps.len(),
                run_id: None,
            });

            // Update internal state: keep only completed steps plus the new ones.
            self.plan
                .steps
                .retain(|s| completed_outputs.contains_key(&s.step_id));
            self.plan.steps.extend(new_plan.steps);

            self.execute_remaining(completed_outputs, deps, resilience)
                .await
        }) // end Box::pin
    }
    /// Execute the remaining (not yet completed) steps after a replan.
    ///
    /// Determines the remaining steps by filtering `self.plan.steps` to those absent
    /// from `completed_outputs`, performs a topological sort, then runs each one.
    ///
    /// Returns a boxed `Future` to allow mutual recursion with
    /// [`replan_and_continue`](Self::replan_and_continue).
    pub(super) fn execute_remaining<'a>(
        &'a mut self,
        mut completed_outputs: HashMap<String, String>,
        deps: StepDeps<'a>,
        resilience: &'a ResilienceLayer,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = AIPResult> + Send + 'a>> {
        Box::pin(async move {
            let remaining: Vec<PlanStep> = self
                .plan
                .steps
                .iter()
                .filter(|s| !completed_outputs.contains_key(&s.step_id))
                .cloned()
                .collect();

            let order = match topological_sort(&remaining) {
                Ok(o) => o,
                Err(_) => {
                    if let Err(e) = self.db.fail_plan(&self.plan.plan_id, "INVALID_REPLAN") {
                        tracing::warn!(error = %e, detail = "ignored", "plan.fail.persist.failed");
                    }
                    return AIPResult::failed("INVALID_REPLAN", "Circular dependency in replan");
                }
            };

            for step_id in order {
                let step = match remaining.iter().find(|s| s.step_id == step_id) {
                    Some(s) => s.clone(),
                    None => continue,
                };

                if deps.budget.is_exhausted() {
                    return self.fail_plan_budget_exhausted(&budget_exhaustion_detail(deps.budget));
                }

                let step_num = completed_outputs.len() + 1;
                self.persist_step_pre_execution(&step, step_num, &completed_outputs);

                // build StepContext for execute_remaining steps.
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
                // A native tool step consumes one tool-call budget unit here too,
                // so replanned tool steps stay under the max_tool_calls ceiling.
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
                        completed_outputs.insert(step_id, output);
                    }

                    Err(ref e) if e.is_retryable() && self.replan_count < self.max_replans => {
                        self.persist_step_failure(&step_id, &e.to_string());
                        self.emit_step_failed(&step_id, &e.to_string(), true);
                        return self
                            .replan_and_continue(
                                step_id,
                                e.to_string(),
                                completed_outputs,
                                deps,
                                resilience,
                            )
                            .await;
                    }

                    Err(e) => return self.finalize_terminal_failure(&step_id, &e, false),
                }
            }

            if let Err(e) = self.db.complete_plan(&self.plan.plan_id) {
                tracing::warn!(error = %e, detail = "ignored", "plan.complete.persist.failed");
            }
            let _ = self.event_bus.send(RuntimeEvent::PlanCompleted {
                task_id: self.plan.task_id.clone().into(),
                plan_id: self.plan.plan_id.clone(),
                step_count: completed_outputs.len(),
                duration_ms: 0,
            });

            AIPResult::completed_with_steps(completed_outputs)
        }) // end Box::pin
    }
}
