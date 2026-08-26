//! The direct path and the plan gate that guards it.
//!
//! Split out of `engine.rs`: the engine's state stays in the parent, the
//! single-shot execution under budget and the human plan gate live here.

use std::sync::Arc;
use std::time::Duration;

use apollia_core::{AIPResult, AIPTask, RuntimeEvent, TaskStatus};

use crate::budget::StepBudget;
use crate::engine::{AgentRunner, ORIAEngine, ORIAError};
use crate::plan::ExecutionPlan;
use crate::plan_gate::PlanGateDecision;

impl ORIAEngine {
    /// Full orchestrated execution: plan, persist, ActorLoop, concat.
    ///
    /// Implements the pipeline:
    /// 1. Validate `system_prompt` is present (fail fast)
    /// 2. Generate the plan via `Reasoner` (internal retry x3)
    /// 3. Persist plan + steps in SQLite (non-blocking on error)
    /// 4. Emit `RuntimeEvent::PlanGenerated`
    /// 5. Create `StepBudget::from_capped(manifest, runtime)`
    /// 6. Execute via `ActorLoop`
    /// 7. Concatenate outputs (or stub `on_plan_complete`)
    ///
    /// Whether the plan gate is active for the current run.
    ///
    /// A per-run override (`--plan`) wins when present: `Some(true)` gates,
    /// `Some(false)` bypasses. Without an override the autonomy tier decides; the
    /// tier defaults to `Assisted` (gate active) when unset, so the safe default
    /// is to gate.
    pub(super) fn plan_gate_active(&self) -> bool {
        if let Some(forced) = self.plan_gate_override {
            return forced;
        }
        let tier = self
            .oria_config
            .autonomy_level
            .unwrap_or(apollia_core::AutonomyLevel::Assisted);
        tier.gate_policy() == apollia_core::GatePolicy::Active
    }
    /// Suspend after plan generation and await an approve/reject decision.
    ///
    /// Registers a oneshot in [`PendingPlanGates`], emits
    /// [`RuntimeEvent::PlanApprovalRequired`], and waits up to
    /// `oria_config.plan_gate_ttl_secs`. No `StepBudget` exists yet, so the
    /// budget cannot progress during the wait.
    ///
    /// # Errors
    ///
    /// - [`ORIAError::PlanGateTimeout`] when no decision arrives within the TTL.
    /// - [`ORIAError::PlanGateChannelClosed`] when the sender is dropped first.
    pub(super) async fn await_plan_gate(
        &self,
        run_id: &str,
        plan: &ExecutionPlan,
    ) -> Result<PlanGateDecision, ORIAError> {
        let plan_id = &plan.plan_id;
        let ttl_secs = self.oria_config.plan_gate_ttl_secs;
        let gates = match self.pending_plan_gates.as_ref() {
            Some(g) => g,
            None => {
                // No registry wired for this run: the gate cannot collect a
                // decision, so execution proceeds (gate is effectively inactive).
                tracing::debug!(run_id = %run_id, "plan.gate.no_registry");
                return Ok(PlanGateDecision::Approved);
            }
        };

        let rx = gates.register(run_id);
        let _ = self.event_bus.send(RuntimeEvent::PlanApprovalRequired {
            run_id: run_id.to_string(),
            plan_id: plan_id.to_string(),
            task_id: run_id.to_string(),
            step_count: plan.steps.len(),
            steps: plan.steps.clone(),
            ttl_secs,
        });

        match tokio::time::timeout(Duration::from_secs(ttl_secs), rx).await {
            Ok(Ok(decision)) => Ok(decision),
            Ok(Err(_)) => Err(ORIAError::PlanGateChannelClosed {
                run_id: run_id.to_string(),
            }),
            Err(_) => Err(ORIAError::PlanGateTimeout {
                run_id: run_id.to_string(),
                plan_id: plan_id.to_string(),
                ttl_secs,
            }),
        }
    }
    /// Execute a task in Mode Direct with HITL support.
    ///
    /// 1. Check the budget is not already exhausted.
    /// 2. Call `runner.call_run(task)` with `StepBudget` supervision.
    /// 3. On `AIPResult::InputRequired`:
    ///    - Persist prompt + context in SQLite via `task_repository` (when configured).
    ///    - Emit `RuntimeEvent::TaskInputRequired` on the EventBus.
    ///    - Register a oneshot in `pending_approvals` and **wait** for the human decision.
    ///    - If `approved=true`: rebuild `AIPTask` with `is_resumed=true` and call `run()` again.
    ///    - If `approved=false`: return `AIPResult::failed("REJECTED", reason)`.
    /// 4. Otherwise return the result directly.
    ///
    /// **StepBudget paused during suspension**: waiting on the oneshot is a pure
    /// `await`, budget polling does not run during suspension.
    /// The budget does not advance until the human responds.
    pub async fn execute_direct(
        &self,
        task: AIPTask,
        runner: &dyn AgentRunner,
        budget: Arc<StepBudget>,
    ) -> Result<AIPResult, ORIAError> {
        // Check budget before starting
        if budget.is_exhausted() {
            let reason = budget
                .exhaustion_reason()
                .unwrap_or_else(|| "budget already exhausted".into());
            return Err(ORIAError::BudgetExceeded { reason });
        }

        // First run, with budget supervision
        let result = Self::run_with_budget(runner, task.clone(), &budget).await?;

        // Non-HITL path: return immediately
        if result.status != TaskStatus::InputRequired {
            return Ok(result);
        }

        // HITL Suspension
        let (prompt, context) = match result.input_required_data {
            Some(data) => (data.prompt, data.context),
            None => ("Approbation requise".to_string(), serde_json::Value::Null),
        };

        // persist input_required in SQLite (non-blocking on error)
        if let Some(repo) = self.task_repository.as_ref() {
            if let Err(e) = repo
                .save_input_required(&task.task_id, None, &prompt, &context)
                .await
            {
                tracing::warn!(
                    task_id = %task.task_id,
                    error = %e,
                    detail = "continuing without a database record",
                    "task.input_required.persist.failed"
                );
            }

            // record suspended_at timestamp for HITL timing
            let suspended_at =
                chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
            if let Err(e) = repo
                .save_suspended_at(&task.task_id, None, &suspended_at)
                .await
            {
                tracing::warn!(
                    task_id = %task.task_id,
                    error = %e,
                    detail = "continuing without a timing record",
                    "task.suspended_at.persist.failed"
                );
            }
        }

        // broadcast TaskInputRequired on EventBus
        // step_id=None in Mode Direct: the whole task is suspended (not a specific step).
        let _ = self.event_bus.send(RuntimeEvent::TaskInputRequired {
            task_id: task.task_id.clone().into(),
            prompt: prompt.clone(),
            step_id: None,
        });

        tracing::info!(
            task_id = %task.task_id,
            %prompt,
            "task.approval.suspended"
        );

        // register on PendingApprovals: if not configured, degrade gracefully
        let pending = match self.pending_approvals.as_ref() {
            Some(p) => p,
            None => {
                tracing::warn!(
                    task_id = %task.task_id,
                    detail = "returning input_required without suspending",
                    "task.approval.unconfigured"
                );
                return Ok(AIPResult::input_required(&prompt, context));
            }
        };

        let rx = pending.register(&task.task_id);

        // plain await: StepBudget does NOT advance during suspension
        let response = rx.await.map_err(|_| ORIAError::ApprovalChannelClosed)?;

        tracing::info!(
            task_id = %task.task_id,
            approved = response.approved,
            "task.approval.received"
        );

        // rejection: AIPResult::failed without calling run()
        if !response.approved {
            return Ok(AIPResult::failed(
                "REJECTED",
                response.reason.as_deref().unwrap_or("Refusé"),
            ));
        }

        // approval: rebuild AIPTask with is_resumed=true and call run() again
        let resumed_task = AIPTask {
            is_resumed: true,
            input_response: Some(response),
            ..task
        };

        // Run resumed task with budget protection
        Self::run_with_budget(runner, resumed_task, &budget).await
    }
    /// Execute `runner.call_run(task)` with concurrent `StepBudget` supervision.
    ///
    /// Returns immediately with `ORIAError::BudgetExceeded` if the budget expires
    /// before execution completes. Used for the first call and for the resume
    /// after HITL.
    ///
    /// Supervision uses a `oneshot` notified by `StepBudget::increment_steps` /
    /// `increment_tool_calls`, combined with a sleep on the remaining wall-clock
    /// duration. No periodic polling.
    pub(super) async fn run_with_budget(
        runner: &dyn AgentRunner,
        task: AIPTask,
        budget: &Arc<StepBudget>,
    ) -> Result<AIPResult, ORIAError> {
        tokio::select! {
            result = runner.call_run(task) => {
                result.map_err(ORIAError::BridgeError)
            }
            _ = budget.wait_for_exhaustion() => {
                let reason = budget
                    .exhaustion_reason()
                    .unwrap_or_else(|| "budget exhausted during execution".into());
                Err(ORIAError::BudgetExceeded { reason })
            }
        }
    }
}
