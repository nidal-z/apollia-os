//! Pipeline executor — sequential and fan-out step scheduling.
//!
//! [`PipelineExecutor`] drives a [`PipelineDefinition`] through its topological
//! layers, submitting steps concurrently within each layer via [`TaskSubmitter`]
//! and collecting results via `FuturesUnordered`.
//!
//! # Execution model
//!
//! 1. Topological layers are computed by [`topological_layers`].
//! 2. Steps in the same layer are submitted concurrently (fan-out).
//! 3. Each submitted step waits for a matching `TaskCompleted` or
//!    `TaskInputRequired` event on the EventBus (fan-in).
//! 4. Failure handling follows each step's [`StepFailurePolicy`].
//!
//! # Subscribe-before-submit invariant
//!
//! The EventBus receiver is created **before** `submit_task` is called for each
//! step. This guarantees that even a very fast task router cannot emit
//! `TaskCompleted` before the executor has started listening.

use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use futures::StreamExt as _;
use tokio::sync::broadcast;
use tracing::{info, warn};

use apollia_core::{EventBusSender, RuntimeEvent};

use crate::{
    repository::PipelineRepository,
    template::TemplateContext,
    topo::{topological_layers, TopologicalError},
    types::{
        PipelineDefinition, PipelineRun, PipelineStepDef, StepFailurePolicy, StepId, StepRun,
        StepRunStatus,
    },
};

// ── Error ─────────────────────────────────────────────────────────────────────

/// Errors produced during pipeline execution.
#[derive(Debug, thiserror::Error)]
pub enum ExecutorError {
    /// A repository (SQLite) operation failed.
    #[error("repository error: {0}")]
    Repository(#[from] crate::repository::PipelineRepositoryError),

    /// The task submitter was unable to dispatch the task.
    #[error("task router unavailable: {0}")]
    TaskRouterUnavailable(String),

    /// The pipeline dependency graph is invalid (cycle or unknown dependency).
    #[error("topological error: {0}")]
    Topological(#[from] TopologicalError),
}

// ── StepResult ────────────────────────────────────────────────────────────────

/// Outcome of a single pipeline step.
#[derive(Debug)]
pub enum StepResult {
    /// The step's agent task completed successfully; carries the text output.
    Completed(String),
    /// The step's agent task failed; carries the error description.
    Failed(String),
    /// The task returned `InputRequired` (HITL); carries the task identifier.
    InputRequired {
        /// Identifier of the suspended task awaiting human approval.
        task_id: String,
    },
}

// ── TaskSubmitter trait ───────────────────────────────────────────────────────

/// Abstraction over task submission — decouples the executor from the concrete
/// `TaskRouterHandle` for testability (same pattern as `ToolExecutor` in ADR-015).
///
/// # Contract
///
/// The caller **must** subscribe to the EventBus **before** invoking
/// `submit_task` so that `TaskCompleted` events emitted by very fast routers
/// are not missed.
#[async_trait]
pub trait TaskSubmitter: Send + Sync + 'static {
    /// Dispatches a task to `agent` with the given rendered `input`.
    ///
    /// Returns the task identifier assigned by the router. The executor uses
    /// this identifier to correlate `TaskCompleted` / `TaskInputRequired`
    /// events on the EventBus.
    ///
    /// # Errors
    ///
    /// Returns [`ExecutorError::TaskRouterUnavailable`] when the router is
    /// unavailable or the named agent is not registered.
    async fn submit_task(&self, agent: &str, input: &str) -> Result<String, ExecutorError>;
}

// ── PipelineExecutor ──────────────────────────────────────────────────────────

/// Drives a pipeline run through its topological execution layers.
///
/// Each layer's steps are submitted concurrently via [`TaskSubmitter`] and
/// collected via `FuturesUnordered`. Results are persisted to SQLite after
/// every step transition. Pipeline-level events are emitted on the EventBus.
pub struct PipelineExecutor<S: TaskSubmitter> {
    /// Static pipeline topology — read-only during execution.
    definition: PipelineDefinition,
    /// Mutable run record; updated in-place as state progresses.
    run: PipelineRun,
    /// Task submission backend (shared via `Arc` with per-step futures).
    submitter: Arc<S>,
    /// EventBus sender — used both to emit pipeline events and to subscribe
    /// per-step receivers before each `submit_task` call.
    event_bus: EventBusSender,
    /// SQLite repository wrapped for shared access.
    repo: Arc<Mutex<PipelineRepository>>,
    /// Maximum time a single step may wait for a `TaskCompleted` event.
    step_timeout: Duration,
    /// Fallback step IDs that are currently active (populated by STORY-113).
    active_fallbacks: HashSet<StepId>,
    /// Template context updated after each completed step.
    template_ctx: TemplateContext,
    /// Wall-clock start time used to compute `duration_ms` in `PipelineCompleted`.
    started_at: Instant,
}

impl<S: TaskSubmitter> PipelineExecutor<S> {
    /// Default per-step timeout: 60 seconds.
    pub const DEFAULT_STEP_TIMEOUT: Duration = Duration::from_secs(60);

    /// Creates a new `PipelineExecutor`.
    ///
    /// The `run` must already be persisted in `repo` by the caller before
    /// `execute` is invoked.
    ///
    /// # Arguments
    ///
    /// * `definition` — static pipeline topology.
    /// * `run` — initial run state.
    /// * `submitter` — task submission backend.
    /// * `event_bus` — runtime EventBus sender.
    /// * `repo` — shared SQLite repository.
    pub fn new(
        definition: PipelineDefinition,
        run: PipelineRun,
        submitter: S,
        event_bus: EventBusSender,
        repo: Arc<Mutex<PipelineRepository>>,
    ) -> Self {
        let template_ctx = TemplateContext::new(
            run.trigger_payload.clone().unwrap_or_default(),
            definition.id.0.clone(),
            run.run_id.0.clone(),
        );
        Self {
            definition,
            run,
            submitter: Arc::new(submitter),
            event_bus,
            repo,
            step_timeout: Self::DEFAULT_STEP_TIMEOUT,
            active_fallbacks: HashSet::new(),
            template_ctx,
            started_at: Instant::now(),
        }
    }

    /// Overrides the per-step timeout (builder pattern, mainly for tests).
    pub fn with_step_timeout(mut self, timeout: Duration) -> Self {
        self.step_timeout = timeout;
        self
    }

    // ── Public API ───────────────────────────────────────────────────────────

    /// Executes the pipeline run to completion or failure.
    ///
    /// Iterates through topological layers, submitting each layer's active
    /// steps concurrently. Persists every state transition to SQLite and
    /// emits the corresponding EventBus events.
    ///
    /// Returns `Ok(())` when the run reaches a terminal state
    /// (`Completed`, `Failed`, or `WaitingApproval`).
    pub async fn execute(mut self) -> Result<(), ExecutorError> {
        // 1 — Insert all step rows as Pending.
        self.init_step_rows()?;

        // 2 — Announce pipeline start.
        let _ = self.event_bus.send(RuntimeEvent::PipelineStarted {
            run_id: self.run.run_id.0.clone(),
            pipeline_id: self.definition.id.0.clone(),
            trigger_id: self.run.trigger_id.clone(),
            step_count: self.definition.steps.len(),
        });
        info!(
            run_id = %self.run.run_id,
            pipeline_id = %self.definition.id,
            steps = self.definition.steps.len(),
            "pipeline started",
        );

        // 3 — Compute topological layers (Kahn's BFS).
        let layers = topological_layers(&self.definition.steps)?;

        // 4 — Layer loop.
        for layer in layers {
            // Filter out fallback steps that have not been activated.
            let active_steps: Vec<StepId> = layer
                .into_iter()
                .filter(|sid| {
                    self.find_step(sid)
                        .map(|d| match &d.fallback_for {
                            None => true,
                            Some(_) => self.active_fallbacks.contains(sid),
                        })
                        .unwrap_or(false)
                })
                .collect();

            if active_steps.is_empty() {
                continue;
            }

            // Fan-out: subscribe → submit → wait, for each step in the layer.
            type StepFut =
                std::pin::Pin<Box<dyn std::future::Future<Output = (StepId, StepResult)> + Send>>;
            let mut futs: futures::stream::FuturesUnordered<StepFut> =
                futures::stream::FuturesUnordered::new();

            for step_id in &active_steps {
                let step_def = match self.find_step(step_id) {
                    Some(d) => d.clone(),
                    None => continue,
                };
                let input = self.template_ctx.render(&step_def.input);

                // Subscribe BEFORE submit (subscribe-before-submit invariant).
                let rx = self.event_bus.subscribe();

                // Submit task.
                let task_id = match self.submitter.submit_task(&step_def.agent, &input).await {
                    Ok(id) => id,
                    Err(e) => {
                        let reason = format!("submission failed: {e}");
                        let sid = step_id.clone();
                        futs.push(Box::pin(async move { (sid, StepResult::Failed(reason)) }));
                        continue;
                    }
                };

                // Persist Running state.
                self.set_step_running(step_id, &task_id)?;

                // Emit PipelineStepStarted with the now-known task_id.
                let _ = self.event_bus.send(RuntimeEvent::PipelineStepStarted {
                    run_id: self.run.run_id.0.clone(),
                    step_id: step_id.0.clone(),
                    task_id: task_id.clone(),
                    agent: step_def.agent.clone(),
                });

                futs.push(Box::pin(Self::wait_for_task_completion(
                    step_id.clone(),
                    task_id,
                    rx,
                    self.step_timeout,
                )));
            }

            // Fan-in: collect all results for this layer.
            let mut layer_failed: Option<(StepId, String)> = None;

            while let Some((step_id, result)) = futs.next().await {
                let step_def = match self.find_step(&step_id) {
                    Some(d) => d.clone(),
                    None => continue,
                };

                match result {
                    StepResult::Completed(output) => {
                        self.set_step_completed(&step_id, &output)?;
                        self.template_ctx
                            .insert_step_output(step_id.clone(), output);
                        let _ = self.event_bus.send(RuntimeEvent::PipelineStepCompleted {
                            run_id: self.run.run_id.0.clone(),
                            step_id: step_id.0.clone(),
                        });
                        info!(run_id = %self.run.run_id, step_id = %step_id, "step completed");
                    }

                    StepResult::Failed(reason) => {
                        match step_def.on_failure {
                            StepFailurePolicy::Fail => {
                                self.set_step_failed(&step_id, &reason)?;
                                let _ = self.event_bus.send(RuntimeEvent::PipelineStepFailed {
                                    run_id: self.run.run_id.0.clone(),
                                    step_id: step_id.0.clone(),
                                    reason: reason.clone(),
                                    on_failure: "fail".into(),
                                });
                                // Record the first fatal failure; drain remaining
                                // futures in this layer before aborting.
                                if layer_failed.is_none() {
                                    layer_failed = Some((step_id, reason));
                                }
                            }
                            StepFailurePolicy::Skip => {
                                self.set_step_skipped(&step_id, &reason)?;
                                // Downstream templates resolve the skipped step to "".
                                self.template_ctx
                                    .insert_step_output(step_id.clone(), String::new());
                                let _ = self.event_bus.send(RuntimeEvent::PipelineStepSkipped {
                                    run_id: self.run.run_id.0.clone(),
                                    step_id: step_id.0.clone(),
                                    reason: reason.clone(),
                                });
                                info!(
                                    run_id = %self.run.run_id,
                                    step_id = %step_id,
                                    "step skipped (on_failure=skip)",
                                );
                            }
                            StepFailurePolicy::Fallback => {
                                // STORY-113 stub: treat as Skip until full fallback
                                // activation is implemented.
                                self.activate_fallback(&step_id, &reason)?;
                                self.template_ctx
                                    .insert_step_output(step_id.clone(), String::new());
                            }
                        }
                    }

                    StepResult::InputRequired { task_id } => {
                        // STORY-114 stub: suspend the pipeline and return.
                        warn!(
                            run_id = %self.run.run_id,
                            step_id = %step_id,
                            task_id = %task_id,
                            "step requires input — pipeline suspended (HITL STORY-114)",
                        );
                        let _ = self.event_bus.send(RuntimeEvent::PipelineSuspended {
                            run_id: self.run.run_id.0.clone(),
                            step_id: step_id.0.clone(),
                            task_id: task_id.clone(),
                        });
                        {
                            let mut repo = self.repo.lock().unwrap_or_else(|e| e.into_inner());
                            repo.suspend_run(&self.run.run_id, &step_id, &task_id)?;
                        }
                        return Ok(());
                    }
                }
            }

            // Abort after draining the layer if a fatal failure was recorded.
            if let Some((failed_step, reason)) = layer_failed {
                return self.fail_pipeline_internal(&failed_step, &reason);
            }
        }

        // 5 — All layers completed successfully.
        self.complete_pipeline()
    }

    // ── Private helpers ───────────────────────────────────────────────────────

    /// Inserts a `Pending` step row for every step defined in the pipeline.
    fn init_step_rows(&self) -> Result<(), ExecutorError> {
        let mut repo = self.repo.lock().unwrap_or_else(|e| e.into_inner());
        for step_def in &self.definition.steps {
            let step_run = StepRun {
                step_id: step_def.id.clone(),
                task_id: None,
                status: StepRunStatus::Pending,
                output: None,
                error: None,
                started_at: None,
                ended_at: None,
            };
            repo.insert_step(&self.run.run_id, &step_run, &step_def.agent)?;
        }
        Ok(())
    }

    /// Updates the step row to `Running` with the submitted `task_id`.
    fn set_step_running(&self, step_id: &StepId, task_id: &str) -> Result<(), ExecutorError> {
        let mut repo = self.repo.lock().unwrap_or_else(|e| e.into_inner());
        repo.update_step(
            &self.run.run_id,
            step_id,
            &StepRunStatus::Running,
            None,
            None,
            Some(task_id),
        )?;
        Ok(())
    }

    /// Updates the step row to `Completed` with the agent's text `output`.
    fn set_step_completed(&self, step_id: &StepId, output: &str) -> Result<(), ExecutorError> {
        let mut repo = self.repo.lock().unwrap_or_else(|e| e.into_inner());
        repo.update_step(
            &self.run.run_id,
            step_id,
            &StepRunStatus::Completed,
            Some(output),
            None,
            None,
        )?;
        Ok(())
    }

    /// Updates the step row to `Skipped` with the skip `reason`.
    fn set_step_skipped(&self, step_id: &StepId, reason: &str) -> Result<(), ExecutorError> {
        let mut repo = self.repo.lock().unwrap_or_else(|e| e.into_inner());
        repo.update_step(
            &self.run.run_id,
            step_id,
            &StepRunStatus::Skipped,
            None,
            Some(reason),
            None,
        )?;
        Ok(())
    }

    /// Updates the step row to `Failed` with the error `reason`.
    fn set_step_failed(&self, step_id: &StepId, reason: &str) -> Result<(), ExecutorError> {
        let mut repo = self.repo.lock().unwrap_or_else(|e| e.into_inner());
        repo.update_step(
            &self.run.run_id,
            step_id,
            &StepRunStatus::Failed,
            None,
            Some(reason),
            None,
        )?;
        Ok(())
    }

    /// Marks the run as `Failed`, persists state, and emits `PipelineFailed`.
    fn fail_pipeline_internal(
        &self,
        failed_step: &StepId,
        reason: &str,
    ) -> Result<(), ExecutorError> {
        {
            let mut repo = self.repo.lock().unwrap_or_else(|e| e.into_inner());
            repo.fail_run(&self.run.run_id, failed_step, reason)?;
        }
        let _ = self.event_bus.send(RuntimeEvent::PipelineFailed {
            run_id: self.run.run_id.0.clone(),
            pipeline_id: self.definition.id.0.clone(),
            step_id: failed_step.0.clone(),
            reason: reason.to_string(),
        });
        warn!(
            run_id = %self.run.run_id,
            pipeline_id = %self.definition.id,
            step_id = %failed_step,
            reason = reason,
            "pipeline failed",
        );
        Ok(())
    }

    /// Marks the run as `Completed`, persists state, and emits `PipelineCompleted`.
    fn complete_pipeline(&self) -> Result<(), ExecutorError> {
        {
            let mut repo = self.repo.lock().unwrap_or_else(|e| e.into_inner());
            repo.complete_run(&self.run.run_id)?;
        }
        let duration_ms = self.started_at.elapsed().as_millis() as u64;
        let _ = self.event_bus.send(RuntimeEvent::PipelineCompleted {
            run_id: self.run.run_id.0.clone(),
            pipeline_id: self.definition.id.0.clone(),
            duration_ms,
        });
        info!(
            run_id = %self.run.run_id,
            pipeline_id = %self.definition.id,
            duration_ms = duration_ms,
            "pipeline completed",
        );
        Ok(())
    }

    /// STORY-113 stub: marks the primary step as `Skipped`.
    ///
    /// Full fallback activation (finding the fallback step, injecting it into
    /// the active layer, re-evaluating the graph) will be implemented in
    /// STORY-113. Until then, treating Fallback like Skip is safe and avoids
    /// infinite execution loops.
    fn activate_fallback(&self, step_id: &StepId, reason: &str) -> Result<(), ExecutorError> {
        warn!(
            run_id = %self.run.run_id,
            step_id = %step_id,
            "fallback activation not yet implemented (STORY-113) — treating as skip",
        );
        self.set_step_skipped(step_id, reason)?;
        let _ = self.event_bus.send(RuntimeEvent::PipelineStepSkipped {
            run_id: self.run.run_id.0.clone(),
            step_id: step_id.0.clone(),
            reason: format!("fallback stub: {reason}"),
        });
        Ok(())
    }

    /// Returns the step definition matching `step_id`, or `None`.
    fn find_step(&self, step_id: &StepId) -> Option<&PipelineStepDef> {
        self.definition.steps.iter().find(|s| &s.id == step_id)
    }

    // ── Static async helper ───────────────────────────────────────────────────

    /// Waits for `TaskCompleted` or `TaskInputRequired` on an already-subscribed
    /// EventBus receiver, bounded by `step_timeout`.
    ///
    /// Accepting a pre-subscribed `rx` upholds the subscribe-before-submit
    /// invariant: the subscription is established in `execute` before the
    /// corresponding `submit_task` call returns.
    async fn wait_for_task_completion(
        step_id: StepId,
        task_id: String,
        mut rx: broadcast::Receiver<RuntimeEvent>,
        step_timeout: Duration,
    ) -> (StepId, StepResult) {
        let timeout_secs = step_timeout.as_secs();
        let result = tokio::time::timeout(step_timeout, async move {
            loop {
                match rx.recv().await {
                    Ok(RuntimeEvent::TaskCompleted {
                        task_id: tid,
                        success,
                        output,
                        ..
                    }) if tid == task_id.as_str() => {
                        if success {
                            return StepResult::Completed(output.unwrap_or_default());
                        } else {
                            return StepResult::Failed("task execution failed".into());
                        }
                    }

                    Ok(RuntimeEvent::TaskInputRequired { task_id: tid, .. })
                        if tid == task_id.as_str() =>
                    {
                        return StepResult::InputRequired { task_id };
                    }

                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        warn!(
                            missed = n,
                            "EventBus lagged in wait_for_task_completion — some events skipped",
                        );
                        // Continue: the target event may still arrive.
                    }

                    Err(broadcast::error::RecvError::Closed) => {
                        return StepResult::Failed("event bus closed unexpectedly".into());
                    }

                    Ok(_) => {
                        // Unrelated event — keep waiting.
                    }
                }
            }
        })
        .await;

        match result {
            Ok(step_result) => (step_id, step_result),
            Err(_elapsed) => (
                step_id,
                StepResult::Failed(format!("step timeout after {timeout_secs}s")),
            ),
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use apollia_core::TaskId;
    use std::collections::HashMap;
    use tokio::sync::broadcast;

    use crate::types::{
        GlobalFailurePolicy, PipelineDefinition, PipelineId, PipelineRun, PipelineStatus,
        PipelineStepDef, RunId, StepFailurePolicy, StepId,
    };

    // ── MockSubmitter ─────────────────────────────────────────────────────────

    /// Test double for `TaskSubmitter`.
    ///
    /// After `submit_task` returns the task ID, it spawns a Tokio task that
    /// yields once (ensuring the receiver is listening) then emits
    /// `TaskCompleted` on the EventBus. Agents listed in `hanging` never emit
    /// any event — used to trigger the per-step timeout.
    struct MockSubmitter {
        event_bus: EventBusSender,
        /// (agent_name → (success, output_text))
        agents: Arc<Mutex<HashMap<String, (bool, String)>>>,
        /// Agents for which no event is emitted (timeout simulation).
        hanging: Arc<Mutex<HashSet<String>>>,
        /// All (agent, rendered_input) pairs submitted, in order.
        submitted: Arc<Mutex<Vec<(String, String)>>>,
        /// Counter for generating unique task IDs.
        counter: Arc<std::sync::atomic::AtomicU32>,
    }

    impl MockSubmitter {
        fn new(event_bus: EventBusSender) -> Self {
            Self {
                event_bus,
                agents: Arc::new(Mutex::new(HashMap::new())),
                hanging: Arc::new(Mutex::new(HashSet::new())),
                submitted: Arc::new(Mutex::new(Vec::new())),
                counter: Arc::new(std::sync::atomic::AtomicU32::new(0)),
            }
        }

        /// Configures `agent` to succeed with `output`.
        fn with_success(self, agent: &str, output: &str) -> Self {
            self.agents
                .lock()
                .unwrap()
                .insert(agent.to_string(), (true, output.to_string()));
            self
        }

        /// Configures `agent` to fail.
        fn with_failure(self, agent: &str) -> Self {
            self.agents
                .lock()
                .unwrap()
                .insert(agent.to_string(), (false, String::new()));
            self
        }

        /// Configures `agent` to hang — no event emitted, triggers timeout.
        fn with_hang(self, agent: &str) -> Self {
            self.hanging.lock().unwrap().insert(agent.to_string());
            self
        }

        fn submitted_inputs(&self) -> Vec<(String, String)> {
            self.submitted.lock().unwrap().clone()
        }
    }

    #[async_trait]
    impl TaskSubmitter for MockSubmitter {
        async fn submit_task(&self, agent: &str, input: &str) -> Result<String, ExecutorError> {
            let n = self
                .counter
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            let task_id = format!("mock-task-{n}");

            self.submitted
                .lock()
                .unwrap()
                .push((agent.to_string(), input.to_string()));

            if self.hanging.lock().unwrap().contains(agent) {
                return Ok(task_id);
            }

            let (success, output) = self
                .agents
                .lock()
                .unwrap()
                .get(agent)
                .cloned()
                .unwrap_or((true, "default-output".into()));

            let event_bus = self.event_bus.clone();
            let tid = task_id.clone();
            let agent_id: apollia_core::AgentId = agent.into();

            tokio::spawn(async move {
                tokio::task::yield_now().await;
                let _ = event_bus.send(RuntimeEvent::TaskCompleted {
                    agent_id,
                    task_id: TaskId::from(tid.as_str()),
                    success,
                    output: if success { Some(output) } else { None },
                });
            });

            Ok(task_id)
        }
    }

    // ── Test helpers ──────────────────────────────────────────────────────────

    fn make_step(
        id: &str,
        agent: &str,
        input: &str,
        depends_on: &[&str],
        on_failure: StepFailurePolicy,
    ) -> PipelineStepDef {
        PipelineStepDef {
            id: StepId(id.into()),
            agent: agent.to_string(),
            input: input.to_string(),
            depends_on: depends_on.iter().map(|s| StepId(s.to_string())).collect(),
            on_failure,
            condition: None,
            fallback_for: None,
        }
    }

    fn make_pipeline(steps: Vec<PipelineStepDef>) -> PipelineDefinition {
        PipelineDefinition {
            id: PipelineId("test-pipeline".into()),
            description: "Test pipeline".into(),
            on_failure: GlobalFailurePolicy::Fail,
            steps,
        }
    }

    fn make_run(def: &PipelineDefinition) -> PipelineRun {
        PipelineRun {
            run_id: RunId("r-test".into()),
            pipeline_id: def.id.clone(),
            trigger_id: None,
            status: PipelineStatus::Running,
            step_runs: HashMap::new(),
            trigger_payload: Some("invoice.pdf".into()),
            started_at: chrono::Utc::now(),
            ended_at: None,
        }
    }

    fn make_repo() -> Arc<Mutex<PipelineRepository>> {
        Arc::new(Mutex::new(
            PipelineRepository::open_in_memory().expect("in-memory db"),
        ))
    }

    /// Builds an executor with a 200 ms step timeout and the run pre-inserted.
    fn make_executor<S: TaskSubmitter>(
        def: PipelineDefinition,
        run: PipelineRun,
        submitter: S,
        tx: EventBusSender,
        timeout_ms: u64,
    ) -> PipelineExecutor<S> {
        let repo = make_repo();
        {
            let mut r = repo.lock().unwrap();
            r.insert_run(&run).unwrap();
        }
        PipelineExecutor::new(def, run, submitter, tx, repo)
            .with_step_timeout(Duration::from_millis(timeout_ms))
    }

    // ── AC-1: Sequential pipeline ─────────────────────────────────────────────

    #[tokio::test]
    async fn test_ac1_sequential_pipeline() {
        // GIVEN — A → B → C, each succeeds; B's input references A's output.
        let (tx, _rx) = broadcast::channel::<RuntimeEvent>(64);
        let steps = vec![
            make_step("A", "agent-a", "start", &[], StepFailurePolicy::Fail),
            make_step(
                "B",
                "agent-b",
                "{{steps.A.output}}",
                &["A"],
                StepFailurePolicy::Fail,
            ),
            make_step("C", "agent-c", "last", &["B"], StepFailurePolicy::Fail),
        ];
        let def = make_pipeline(steps);
        let run = make_run(&def);
        let mock = MockSubmitter::new(tx.clone())
            .with_success("agent-a", "out-A")
            .with_success("agent-b", "out-B")
            .with_success("agent-c", "out-C");

        // WHEN
        let result = make_executor(def, run, mock, tx, 500).execute().await;

        // THEN — pipeline completes without error
        assert!(result.is_ok(), "unexpected error: {result:?}");
    }

    #[tokio::test]
    async fn test_ac1_template_resolved_in_sequential_pipeline() {
        // GIVEN — B's input is a template referencing A's output
        let (tx, _rx) = broadcast::channel::<RuntimeEvent>(64);
        let steps = vec![
            make_step("A", "agent-a", "start", &[], StepFailurePolicy::Fail),
            make_step(
                "B",
                "agent-b",
                "{{steps.A.output}}",
                &["A"],
                StepFailurePolicy::Fail,
            ),
        ];
        let def = make_pipeline(steps);
        let run = make_run(&def);
        let mock = MockSubmitter::new(tx.clone())
            .with_success("agent-a", "resolved-output")
            .with_success("agent-b", "done");
        let submitted = mock.submitted.clone();

        // WHEN
        make_executor(def, run, mock, tx, 500)
            .execute()
            .await
            .unwrap();

        // THEN — B was submitted with A's output as input
        let inputs = submitted.lock().unwrap().clone();
        assert_eq!(inputs.len(), 2);
        assert_eq!(inputs[0].0, "agent-a");
        assert_eq!(inputs[1].0, "agent-b");
        assert_eq!(
            inputs[1].1, "resolved-output",
            "B's input should be A's resolved output"
        );
    }

    // ── AC-2: Fan-out ─────────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_ac2_fan_out_both_steps_submitted() {
        // GIVEN — A → [B, C]: B and C are in the same layer (no inter-dependency)
        let (tx, _rx) = broadcast::channel::<RuntimeEvent>(64);
        let steps = vec![
            make_step("A", "agent-a", "start", &[], StepFailurePolicy::Fail),
            make_step("B", "agent-b", "input-b", &["A"], StepFailurePolicy::Fail),
            make_step("C", "agent-c", "input-c", &["A"], StepFailurePolicy::Fail),
        ];
        let def = make_pipeline(steps);
        let run = make_run(&def);
        let mock = MockSubmitter::new(tx.clone())
            .with_success("agent-a", "a-done")
            .with_success("agent-b", "b-done")
            .with_success("agent-c", "c-done");
        let submitted = mock.submitted.clone();

        // WHEN
        make_executor(def, run, mock, tx, 500)
            .execute()
            .await
            .unwrap();

        // THEN — all three agents were submitted
        let agents: Vec<String> = submitted
            .lock()
            .unwrap()
            .iter()
            .map(|(a, _)| a.clone())
            .collect();
        assert!(agents.contains(&"agent-a".to_string()));
        assert!(agents.contains(&"agent-b".to_string()));
        assert!(agents.contains(&"agent-c".to_string()));
        assert_eq!(agents.len(), 3);
    }

    // ── AC-3: on_failure = skip ───────────────────────────────────────────────

    #[tokio::test]
    async fn test_ac3_skip_continues_pipeline() {
        // GIVEN — A fails with on_failure=Skip; B depends on A
        let (tx, mut rx) = broadcast::channel::<RuntimeEvent>(64);
        let steps = vec![
            make_step("A", "agent-a", "start", &[], StepFailurePolicy::Skip),
            make_step("B", "agent-b", "after-a", &["A"], StepFailurePolicy::Fail),
        ];
        let def = make_pipeline(steps);
        let run = make_run(&def);
        let mock = MockSubmitter::new(tx.clone())
            .with_failure("agent-a")
            .with_success("agent-b", "b-ok");
        let submitted = mock.submitted.clone();

        // WHEN
        let result = make_executor(def, run, mock, tx, 500).execute().await;

        // THEN — pipeline completes (not fails)
        assert!(result.is_ok());

        // AND B was still submitted
        let agents: Vec<String> = submitted
            .lock()
            .unwrap()
            .iter()
            .map(|(a, _)| a.clone())
            .collect();
        assert!(
            agents.contains(&"agent-b".to_string()),
            "B should be submitted after A is skipped"
        );

        // AND PipelineCompleted (not PipelineFailed) was emitted
        let mut events = Vec::new();
        while let Ok(e) = rx.try_recv() {
            events.push(e);
        }
        let completed = events
            .iter()
            .any(|e| matches!(e, RuntimeEvent::PipelineCompleted { .. }));
        let failed = events
            .iter()
            .any(|e| matches!(e, RuntimeEvent::PipelineFailed { .. }));
        assert!(completed, "PipelineCompleted should be emitted");
        assert!(!failed, "PipelineFailed should NOT be emitted");
    }

    // ── AC-4: on_failure = fail ───────────────────────────────────────────────

    #[tokio::test]
    async fn test_ac4_fail_stops_pipeline() {
        // GIVEN — A fails with on_failure=Fail (default)
        let (tx, mut rx) = broadcast::channel::<RuntimeEvent>(64);
        let steps = vec![
            make_step("A", "agent-a", "start", &[], StepFailurePolicy::Fail),
            make_step("B", "agent-b", "after-a", &["A"], StepFailurePolicy::Fail),
        ];
        let def = make_pipeline(steps);
        let run = make_run(&def);
        let mock = MockSubmitter::new(tx.clone()).with_failure("agent-a");

        // WHEN
        let result = make_executor(def, run, mock, tx, 500).execute().await;

        // THEN — execute returns Ok (failure is a terminal state, not an error)
        assert!(result.is_ok());

        // AND PipelineFailed was emitted
        let mut events = Vec::new();
        while let Ok(e) = rx.try_recv() {
            events.push(e);
        }
        let failed = events
            .iter()
            .any(|e| matches!(e, RuntimeEvent::PipelineFailed { .. }));
        assert!(failed, "PipelineFailed should be emitted");
    }

    // ── AC-5: Downstream not submitted after fatal failure ────────────────────

    #[tokio::test]
    async fn test_ac5_downstream_not_submitted() {
        // GIVEN — A fails (Fail policy); B and C are downstream
        let (tx, _rx) = broadcast::channel::<RuntimeEvent>(64);
        let steps = vec![
            make_step("A", "agent-a", "start", &[], StepFailurePolicy::Fail),
            make_step("B", "agent-b", "b-input", &["A"], StepFailurePolicy::Fail),
            make_step("C", "agent-c", "c-input", &["B"], StepFailurePolicy::Fail),
        ];
        let def = make_pipeline(steps);
        let run = make_run(&def);
        let mock = MockSubmitter::new(tx.clone()).with_failure("agent-a");
        let submitted = mock.submitted.clone();

        // WHEN
        make_executor(def, run, mock, tx, 500)
            .execute()
            .await
            .unwrap();

        // THEN — only A was submitted; B and C were not
        let agents: Vec<String> = submitted
            .lock()
            .unwrap()
            .iter()
            .map(|(a, _)| a.clone())
            .collect();
        assert_eq!(agents, vec!["agent-a".to_string()]);
        assert!(
            !agents.contains(&"agent-b".to_string()),
            "agent-b must NOT be submitted"
        );
        assert!(
            !agents.contains(&"agent-c".to_string()),
            "agent-c must NOT be submitted"
        );
    }

    // ── AC-6: Per-step timeout ────────────────────────────────────────────────

    #[tokio::test]
    async fn test_ac6_step_timeout_fails_pipeline() {
        // GIVEN — A hangs (never emits TaskCompleted); timeout = 100 ms
        let (tx, mut rx) = broadcast::channel::<RuntimeEvent>(64);
        let steps = vec![make_step(
            "A",
            "agent-a",
            "start",
            &[],
            StepFailurePolicy::Fail,
        )];
        let def = make_pipeline(steps);
        let run = make_run(&def);
        let mock = MockSubmitter::new(tx.clone()).with_hang("agent-a");

        // WHEN
        let result = make_executor(def, run, mock, tx, 100).execute().await;

        // THEN — execute returns Ok (timeout leads to pipeline failure, not Err)
        assert!(result.is_ok());

        // AND PipelineFailed was emitted with a timeout reason
        let mut events = Vec::new();
        while let Ok(e) = rx.try_recv() {
            events.push(e);
        }
        let failed = events.iter().any(|e| {
            matches!(e, RuntimeEvent::PipelineFailed { reason, .. }
                if reason.contains("timeout"))
        });
        assert!(
            failed,
            "PipelineFailed with timeout reason should be emitted"
        );
    }

    // ── AC-7: EventBus events ─────────────────────────────────────────────────

    #[tokio::test]
    async fn test_ac7_events_emitted_in_order() {
        // GIVEN — simple A → B sequential pipeline
        let (tx, mut rx) = broadcast::channel::<RuntimeEvent>(128);
        let steps = vec![
            make_step("A", "agent-a", "start", &[], StepFailurePolicy::Fail),
            make_step("B", "agent-b", "next", &["A"], StepFailurePolicy::Fail),
        ];
        let def = make_pipeline(steps);
        let run = make_run(&def);
        let mock = MockSubmitter::new(tx.clone())
            .with_success("agent-a", "a-out")
            .with_success("agent-b", "b-out");

        // WHEN
        make_executor(def, run, mock, tx, 500)
            .execute()
            .await
            .unwrap();

        // THEN — collect all pipeline-level events
        let mut events = Vec::new();
        while let Ok(e) = rx.try_recv() {
            events.push(e);
        }

        let has_started = events
            .iter()
            .any(|e| matches!(e, RuntimeEvent::PipelineStarted { .. }));
        let step_started_count = events
            .iter()
            .filter(|e| matches!(e, RuntimeEvent::PipelineStepStarted { .. }))
            .count();
        let step_completed_count = events
            .iter()
            .filter(|e| matches!(e, RuntimeEvent::PipelineStepCompleted { .. }))
            .count();
        let has_completed = events
            .iter()
            .any(|e| matches!(e, RuntimeEvent::PipelineCompleted { .. }));

        assert!(has_started, "PipelineStarted must be emitted");
        assert_eq!(step_started_count, 2, "PipelineStepStarted for each step");
        assert_eq!(
            step_completed_count, 2,
            "PipelineStepCompleted for each step"
        );
        assert!(has_completed, "PipelineCompleted must be emitted");

        // AND PipelineStarted appears before PipelineCompleted
        let started_pos = events
            .iter()
            .position(|e| matches!(e, RuntimeEvent::PipelineStarted { .. }))
            .unwrap();
        let completed_pos = events
            .iter()
            .position(|e| matches!(e, RuntimeEvent::PipelineCompleted { .. }))
            .unwrap();
        assert!(started_pos < completed_pos);
    }
}
