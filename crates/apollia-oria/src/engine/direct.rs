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
    /// 3. While the result is `InputRequired`, pause:
    ///    - parse the payload, if any; an invalid one fails the task with
    ///      `INVALID_INPUT_PAYLOAD` and is never persisted or shown;
    ///    - persist the pause, emit `RuntimeEvent::TaskInputRequired`, and
    ///      **wait** on `pending_approvals` for the human decision;
    ///    - resume the agent with `is_resumed = true` and the response.
    /// 4. Return the first result that is not a pause.
    ///
    /// The loop is what lets an agent pause more than once in one task, a
    /// question and then an approval for instance. The previous shape paused
    /// once and returned whatever the resumed run produced, so a second pause
    /// ended the task in `input_required` with no one left waiting on it.
    ///
    /// A declined pause is resumed rather than failed when it carries a
    /// payload: the agent asked a typed question and is owed the typed answer,
    /// `approved = false` included, so it can decide what a refusal means. A
    /// prompt-only pause keeps its historical behaviour, a declined one fails
    /// the task with `REJECTED` without calling the agent again.
    ///
    /// **StepBudget paused during suspension**: waiting on the oneshot is a pure
    /// `await`, budget polling does not run during suspension.
    pub async fn execute_direct(
        &self,
        task: AIPTask,
        runner: &dyn AgentRunner,
        budget: Arc<StepBudget>,
    ) -> Result<AIPResult, ORIAError> {
        if budget.is_exhausted() {
            let reason = budget
                .exhaustion_reason()
                .unwrap_or_else(|| "budget already exhausted".into());
            return Err(ORIAError::BudgetExceeded { reason });
        }

        let mut result = Self::run_with_budget(runner, task.clone(), &budget).await?;

        while result.status == TaskStatus::InputRequired {
            let (prompt, context, raw_payload) = match result.input_required_data {
                Some(data) => (data.prompt, data.context, data.payload),
                None => (
                    "Approbation requise".to_string(),
                    serde_json::Value::Null,
                    None,
                ),
            };

            let payload = match raw_payload.as_ref().map(apollia_core::HitlPayload::parse) {
                None => None,
                Some(Ok(parsed)) => Some(parsed),
                Some(Err(e)) => {
                    tracing::warn!(
                        task_id = %task.task_id,
                        error = %e,
                        detail = "the task fails and the payload is not persisted",
                        "task.input_payload.invalid"
                    );
                    return Ok(invalid_payload_result(&e));
                }
            };
            // The validated form, re-serialised: what is stored, listed and
            // given back to the agent is what the parser accepted, not the raw
            // JSON the agent sent.
            let payload_value = payload.as_ref().and_then(|p| serde_json::to_value(p).ok());

            let response = match self
                .suspend(&task, &prompt, &context, payload_value.as_ref())
                .await?
            {
                Some(response) => response,
                None => {
                    let mut unsuspended = match payload_value {
                        Some(p) => AIPResult::input_required_with_payload(&prompt, context, p),
                        None => AIPResult::input_required(&prompt, context),
                    };
                    unsuspended.task_id = task.task_id.clone();
                    return Ok(unsuspended);
                }
            };

            if !response.approved && payload_value.is_none() {
                return Ok(AIPResult::failed(
                    "REJECTED",
                    response.reason.as_deref().unwrap_or("Refused"),
                ));
            }

            let mut response = response;
            if response.payload.is_none() {
                response.payload = payload_value;
            }
            let resumed_task = AIPTask {
                is_resumed: true,
                input_response: Some(response),
                ..task.clone()
            };
            result = Self::run_with_budget(runner, resumed_task, &budget).await?;
        }

        Ok(result)
    }

    /// Persist one pause, announce it, and wait for the human.
    ///
    /// Returns `None` when no approval registry is wired, in which case the
    /// caller hands the pause back unsuspended, as before.
    async fn suspend(
        &self,
        task: &AIPTask,
        prompt: &str,
        context: &serde_json::Value,
        payload: Option<&serde_json::Value>,
    ) -> Result<Option<apollia_core::InputResponseData>, ORIAError> {
        if let Some(repo) = self.task_repository.as_ref() {
            if let Err(e) = repo
                .save_pause(apollia_tools::PauseRecord {
                    task_id: &task.task_id,
                    step_id: None,
                    prompt,
                    context,
                    payload,
                    agent_name: self.agent_name.as_deref(),
                    skill_id: task.skill_id.as_deref(),
                })
                .await
            {
                tracing::warn!(
                    task_id = %task.task_id,
                    error = %e,
                    detail = "continuing without a database record",
                    "task.input_required.persist.failed"
                );
            }

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

        // step_id=None in Mode Direct: the whole task is suspended.
        let _ = self.event_bus.send(RuntimeEvent::TaskInputRequired {
            task_id: task.task_id.clone().into(),
            prompt: prompt.to_string(),
            step_id: None,
        });

        tracing::info!(
            task_id = %task.task_id,
            typed = payload.is_some(),
            "task.approval.suspended"
        );

        let pending = match self.pending_approvals.as_ref() {
            Some(p) => p,
            None => {
                tracing::warn!(
                    task_id = %task.task_id,
                    detail = "returning input_required without suspending",
                    "task.approval.unconfigured"
                );
                return Ok(None);
            }
        };

        let rx = pending.register(&task.task_id);
        let response = rx.await.map_err(|_| ORIAError::ApprovalChannelClosed)?;

        tracing::info!(
            task_id = %task.task_id,
            approved = response.approved,
            "task.approval.received"
        );
        Ok(Some(response))
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

/// The failure a task ends on when it pauses with a payload that does not parse.
///
/// Typed, so a caller branches on `INVALID_INPUT_PAYLOAD` rather than on the
/// message, and carrying the parser's reason in `details` so the agent author
/// learns which field is wrong.
fn invalid_payload_result(error: &apollia_core::PayloadError) -> AIPResult {
    let mut result = AIPResult::failed(
        "INVALID_INPUT_PAYLOAD",
        "the pause payload is invalid, so the task was not paused",
    );
    if let Some(err) = result.error.as_mut() {
        err.details = Some(serde_json::json!({ "reason": error.to_string() }));
    }
    result
}

#[cfg(test)]
mod typed_pause_tests {
    use super::*;
    use apollia_core::budget::StepBudgetConfig;
    use apollia_core::{InputResponseData, PendingApprovals};
    use serde_json::json;
    use std::sync::Mutex;

    /// Runner answering a scripted sequence of results and recording every task
    /// it was given, so a test can read what each resumed run received.
    struct ScriptedRunner {
        script: Mutex<Vec<AIPResult>>,
        seen: Mutex<Vec<AIPTask>>,
    }

    impl ScriptedRunner {
        fn new(script: Vec<AIPResult>) -> Self {
            Self {
                script: Mutex::new(script),
                seen: Mutex::new(Vec::new()),
            }
        }
        fn seen(&self) -> Vec<AIPTask> {
            self.seen.lock().map(|s| s.clone()).unwrap_or_default()
        }
    }

    impl AgentRunner for ScriptedRunner {
        fn call_run(
            &self,
            task: AIPTask,
        ) -> std::pin::Pin<
            Box<dyn std::future::Future<Output = Result<AIPResult, String>> + Send + '_>,
        > {
            if let Ok(mut seen) = self.seen.lock() {
                seen.push(task);
            }
            let next = self.script.lock().ok().and_then(|mut s| {
                if s.is_empty() {
                    None
                } else {
                    Some(s.remove(0))
                }
            });
            Box::pin(async move { next.ok_or_else(|| "script exhausted".to_string()) })
        }
    }

    fn budget() -> Arc<StepBudget> {
        Arc::new(StepBudget::new(&StepBudgetConfig {
            max_steps: 10,
            max_tool_calls: 20,
            wall_clock_secs: 300,
        }))
    }

    fn question() -> serde_json::Value {
        json!({"genre": "choix", "question": "Which list?",
               "propositions": [{"id": "a", "libelle": "A"}, {"id": "b", "libelle": "B"}]})
    }

    fn approbation() -> serde_json::Value {
        json!({"genre": "approbation", "geste": "crm/delete", "risque": "critical"})
    }

    fn response(approved: bool, answer: Option<serde_json::Value>) -> InputResponseData {
        InputResponseData {
            approved,
            reason: None,
            context: json!({}),
            responded_at: "2026-09-16T10:00:00Z".into(),
            answer,
            payload: None,
        }
    }

    /// Resolve the next pause of `task_id` once the engine has registered it.
    async fn answer_when_paused(pending: &PendingApprovals, task_id: &str, r: InputResponseData) {
        while !pending.pending_task_ids().iter().any(|id| id == task_id) {
            tokio::task::yield_now().await;
        }
        pending.resolve(task_id, r).expect("resolve");
    }

    #[tokio::test]
    async fn an_agent_can_pause_twice_in_one_task() {
        // GIVEN an agent that asks a question, then an approval, then completes
        let pending = Arc::new(PendingApprovals::new());
        let engine = ORIAEngine::new().with_pending_approvals(pending.clone());
        let runner = ScriptedRunner::new(vec![
            AIPResult::input_required_with_payload("Which list?", json!({}), question()),
            AIPResult::input_required_with_payload("Delete?", json!({"list": "b"}), approbation()),
            AIPResult::completed("done"),
        ]);
        let task = AIPTask {
            task_id: "t-twice".into(),
            ..AIPTask::default()
        };

        // WHEN the operator answers both pauses
        let operator = {
            let pending = pending.clone();
            async move {
                answer_when_paused(&pending, "t-twice", response(true, Some(json!("b")))).await;
                answer_when_paused(&pending, "t-twice", response(true, None)).await;
            }
        };
        let (result, ()) = tokio::join!(engine.execute_direct(task, &runner, budget()), operator);

        // THEN the task completes rather than ending on the second pause
        let result = result.expect("engine ok");
        assert_eq!(result.status, TaskStatus::Completed);

        // AND the agent ran three times, each resumed run seeing its own answer
        // and the payload it answers
        let seen = runner.seen();
        assert_eq!(seen.len(), 3);
        assert!(!seen[0].is_resumed);
        let first = seen[1].input_response.as_ref().expect("first response");
        assert_eq!(first.answer, Some(json!("b")));
        assert_eq!(
            first.payload.as_ref().map(|p| p["genre"].clone()),
            Some(json!("choix"))
        );
        let second = seen[2].input_response.as_ref().expect("second response");
        assert!(second.approved);
        assert_eq!(
            second.payload.as_ref().map(|p| p["genre"].clone()),
            Some(json!("approbation"))
        );
    }

    #[tokio::test]
    async fn an_invalid_payload_fails_the_task_with_a_typed_error() {
        // GIVEN an agent pausing on a payload whose genre does not exist
        let pending = Arc::new(PendingApprovals::new());
        let engine = ORIAEngine::new().with_pending_approvals(pending.clone());
        let runner = ScriptedRunner::new(vec![AIPResult::input_required_with_payload(
            "?",
            json!({}),
            json!({"genre": "sondage", "question": "?"}),
        )]);
        let task = AIPTask {
            task_id: "t-invalid".into(),
            ..AIPTask::default()
        };

        // WHEN it runs
        let result = engine
            .execute_direct(task, &runner, budget())
            .await
            .expect("engine ok");

        // THEN the task fails with the typed code, and nothing was left waiting
        assert_eq!(result.status, TaskStatus::Failed);
        assert_eq!(
            result.error.as_ref().map(|e| e.code.as_str()),
            Some("INVALID_INPUT_PAYLOAD")
        );
        assert!(pending.pending_task_ids().is_empty());
    }

    #[tokio::test]
    async fn a_declined_typed_pause_resumes_the_agent_with_the_refusal() {
        // GIVEN an agent pausing on an approval
        let pending = Arc::new(PendingApprovals::new());
        let engine = ORIAEngine::new().with_pending_approvals(pending.clone());
        let runner = ScriptedRunner::new(vec![
            AIPResult::input_required_with_payload("Delete?", json!({}), approbation()),
            AIPResult::completed("kept"),
        ]);
        let task = AIPTask {
            task_id: "t-declined".into(),
            ..AIPTask::default()
        };

        // WHEN the operator declines
        let operator = {
            let pending = pending.clone();
            async move { answer_when_paused(&pending, "t-declined", response(false, None)).await }
        };
        let (result, ()) = tokio::join!(engine.execute_direct(task, &runner, budget()), operator);

        // THEN the agent is resumed and reads the refusal itself
        assert_eq!(result.expect("ok").status, TaskStatus::Completed);
        let seen = runner.seen();
        assert_eq!(seen.len(), 2);
        assert!(!seen[1].input_response.as_ref().expect("response").approved);
    }

    #[tokio::test]
    async fn a_declined_prompt_only_pause_still_fails_without_calling_the_agent() {
        // GIVEN a historical pause carrying a prompt and no payload
        let pending = Arc::new(PendingApprovals::new());
        let engine = ORIAEngine::new().with_pending_approvals(pending.clone());
        let runner = ScriptedRunner::new(vec![AIPResult::input_required("Send?", json!({}))]);
        let task = AIPTask {
            task_id: "t-legacy".into(),
            ..AIPTask::default()
        };

        // WHEN the operator declines
        let operator = {
            let pending = pending.clone();
            async move { answer_when_paused(&pending, "t-legacy", response(false, None)).await }
        };
        let (result, ()) = tokio::join!(engine.execute_direct(task, &runner, budget()), operator);

        // THEN the task fails with REJECTED and the agent ran once, unchanged
        let result = result.expect("ok");
        assert_eq!(
            result.error.as_ref().map(|e| e.code.as_str()),
            Some("REJECTED")
        );
        assert_eq!(runner.seen().len(), 1);
    }
}
