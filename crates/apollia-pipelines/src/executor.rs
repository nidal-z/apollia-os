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

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use chrono::Utc;
use futures::StreamExt as _;
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

use apollia_core::{EventBusSender, PipelinesConfig, RuntimeEvent};

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

    /// A step failed with `on_failure = fallback` but no fallback step is
    /// declared for it. The pipeline is immediately terminated.
    #[error("no fallback declared for step '{0}'")]
    NoFallbackDeclared(String),
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
    /// The step was explicitly cancelled by the operator via [`StepCancelRegistry::cancel_step`].
    Cancelled,
}

// ── HitlResume ────────────────────────────────────────────────────────────────

/// Outcome of a HITL approval request for a suspended pipeline step.
///
/// Returned by [`PipelineExecutor::wait_for_resume`] after the operator has
/// acted on the suspended step via `apollia-os task resume --approve` or
/// `--reject`.
///
/// # Note on `new_task_id`
///
/// The spec originally defined a `new_task_id` field to allow the resume
/// handler to re-submit the task under a different identifier.  The actual
/// [`RuntimeEvent::TaskResumed`] in `apollia-core` (Sprint 11) does **not**
/// carry `new_task_id`; the resumed ORIA engine continues under the original
/// `task_id`.  This struct therefore omits `new_task_id` — see implementation
/// notes in `story-114-hitl-pipelines.md`.
pub struct HitlResume {
    /// `true` if the operator approved the suspended step; `false` if rejected.
    pub approved: bool,
    /// Identifier of the approving operator; `None` if rejected or runtime shut down.
    pub approved_by: Option<String>,
    /// Duration in milliseconds between the HITL request and this response.
    pub approval_duration_ms: Option<i64>,
}

// ── StepCancelRegistry ────────────────────────────────────────────────────────

/// Thread-safe registry that maps a `step_run_id` to a [`CancellationToken`].
///
/// The `step_run_id` is the composite key `"{run_id}/{step_id}"`.
/// [`PipelineExecutor`] registers tokens when steps start executing and deregisters
/// them when steps reach any terminal state.  [`crate::engine::PipelineEngineHandle`]
/// (and tests) call [`cancel_step`](Self::cancel_step) to trigger cancellation.
pub struct StepCancelRegistry {
    tokens: Mutex<HashMap<String, CancellationToken>>,
}

impl Default for StepCancelRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl StepCancelRegistry {
    /// Creates a new, empty registry.
    pub fn new() -> Self {
        Self {
            tokens: Mutex::new(HashMap::new()),
        }
    }

    /// Registers a cancellation token for the step identified by `step_run_id`.
    ///
    /// Called by the executor immediately after `submit_task` returns so that
    /// an in-flight step can be cancelled via [`cancel_step`](Self::cancel_step).
    pub(crate) fn register(&self, step_run_id: &str, token: CancellationToken) {
        self.tokens
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(step_run_id.to_owned(), token);
    }

    /// Removes the token for the given step.  No-op if the step is not registered.
    ///
    /// Called when a step reaches any terminal state (completed, failed, cancelled, skipped).
    pub(crate) fn deregister(&self, step_run_id: &str) {
        self.tokens
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(step_run_id);
    }

    /// Cancels the step identified by `step_run_id`.
    ///
    /// Triggers the [`CancellationToken`] for the step, causing
    /// [`PipelineExecutor::wait_for_task_completion`] to return
    /// [`StepResult::Cancelled`] instead of waiting for the task event.
    ///
    /// Returns `true` if the token was found and triggered, `false` if no
    /// running step with that `step_run_id` is currently registered.
    pub fn cancel_step(&self, step_run_id: &str) -> bool {
        let guard = self.tokens.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(token) = guard.get(step_run_id) {
            token.cancel();
            true
        } else {
            false
        }
    }
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
    /// Fallback step IDs that are currently active.
    ///
    /// When `activate_fallback` is called for step `"validation"`, the
    /// corresponding fallback step (e.g. `"validation-manuelle"`) is inserted
    /// here so the layer filter includes it on the next pass.
    active_fallbacks: HashSet<StepId>,
    /// Steps that have already reached a terminal state in this run.
    ///
    /// Tracked across fallback-triggered restarts to avoid re-submitting
    /// steps that already completed or were skipped in a previous pass.
    done_steps: HashSet<StepId>,
    /// Template context updated after each completed step.
    template_ctx: TemplateContext,
    /// Wall-clock start time used to compute `duration_ms` in `PipelineCompleted`.
    started_at: Instant,
    /// When `true`, `init_step_rows` is skipped because the step rows already
    /// exist in SQLite (resume after restart).  Set by [`as_resume`](Self::as_resume).
    is_resume: bool,
    /// Shared registry of per-step cancellation tokens.
    ///
    /// Steps register their token on submission and deregister on completion.
    /// The registry is Arc-shared with the `PipelineEngineHandle` so the engine
    /// can cancel individual steps from outside the executor task.
    cancel_registry: Arc<StepCancelRegistry>,
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
            done_steps: HashSet::new(),
            template_ctx,
            started_at: Instant::now(),
            is_resume: false,
            cancel_registry: Arc::new(StepCancelRegistry::new()),
        }
    }

    /// Overrides the shared [`StepCancelRegistry`] (builder pattern).
    ///
    /// Used by [`PipelineEngine`](crate::engine::PipelineEngine) to inject the
    /// engine-wide registry so that `PipelineEngineHandle::cancel_step` can reach
    /// running steps.
    pub fn with_cancel_registry(mut self, registry: Arc<StepCancelRegistry>) -> Self {
        self.cancel_registry = registry;
        self
    }

    /// Marks this executor as resuming an existing run after a process restart.
    ///
    /// When set:
    /// - `init_step_rows` is skipped because step rows already exist in SQLite.
    /// - Steps whose status in `run.step_runs` is already terminal
    ///   (`Completed`, `Skipped`, `FallbackActive`) are pre-inserted into
    ///   `done_steps` so the executor skips re-submitting them.
    pub fn as_resume(mut self) -> Self {
        self.is_resume = true;
        // Pre-populate done_steps from step_runs already in a terminal state.
        for (step_id, step_run) in &self.run.step_runs {
            if matches!(
                step_run.status,
                StepRunStatus::Completed | StepRunStatus::Skipped | StepRunStatus::FallbackActive
            ) {
                self.done_steps.insert(step_id.clone());
            }
        }
        self
    }

    /// Overrides the per-step timeout (builder pattern, mainly for tests).
    pub fn with_step_timeout(mut self, timeout: Duration) -> Self {
        self.step_timeout = timeout;
        self
    }

    /// Applies pipeline configuration read from `apollia.toml`.
    ///
    /// Sets `step_timeout` from [`PipelinesConfig::default_step_timeout_secs`].
    pub fn with_pipelines_config(mut self, config: &PipelinesConfig) -> Self {
        self.step_timeout = Duration::from_secs(config.default_step_timeout_secs);
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

        // 3 — Restart loop: repeated when a fallback is activated so the execution
        //     graph can be re-evaluated with the newly active fallback step.
        //     At most one restart occurs per activated fallback.
        'restart: loop {
            let layers = topological_layers(&self.definition.steps)?;

            // 4 — Layer loop.
            for layer in &layers {
                // Filter out steps that already reached a terminal state and
                // fallback steps that have not yet been activated.
                let active_steps: Vec<StepId> = layer
                    .iter()
                    .filter(|sid| {
                        if self.done_steps.contains(*sid) {
                            return false;
                        }
                        self.find_step(sid)
                            .map(|d| match &d.fallback_for {
                                None => true,
                                Some(_) => self.active_fallbacks.contains(*sid),
                            })
                            .unwrap_or(false)
                    })
                    .cloned()
                    .collect();

                if active_steps.is_empty() {
                    continue;
                }

                // Fan-out: subscribe → submit → wait, for each step in the layer.
                type StepFut = std::pin::Pin<
                    Box<dyn std::future::Future<Output = (StepId, StepResult)> + Send>,
                >;
                let mut futs: futures::stream::FuturesUnordered<StepFut> =
                    futures::stream::FuturesUnordered::new();

                for step_id in &active_steps {
                    let step_def = match self.find_step(step_id) {
                        Some(d) => d.clone(),
                        None => continue,
                    };

                    // Evaluate condition before submitting the step.
                    // A false condition skips the step without blocking downstream.
                    if let Some(cond) = &step_def.condition {
                        if !crate::condition::evaluate_condition(cond, &self.template_ctx) {
                            self.set_step_skipped(step_id, "condition=false")?;
                            self.done_steps.insert(step_id.clone());
                            // Insert empty output so downstream templates resolve cleanly.
                            self.template_ctx
                                .insert_step_output(step_id.clone(), String::new());
                            let _ = self.event_bus.send(RuntimeEvent::PipelineStepSkipped {
                                run_id: self.run.run_id.0.clone(),
                                step_id: step_id.0.clone(),
                                reason: "condition=false".into(),
                            });
                            info!(
                                run_id = %self.run.run_id,
                                step_id = %step_id,
                                "step skipped (condition=false)",
                            );
                            continue;
                        }
                    }

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

                    // Create a per-step cancellation token and register it in the
                    // shared registry so the engine can cancel this step by ID.
                    let cancel_token = CancellationToken::new();
                    let step_run_id = format!("{}/{}", self.run.run_id.0, step_id.0);
                    self.cancel_registry.register(&step_run_id, cancel_token.clone());

                    // Per-step timeout: use the step's own value if set, else
                    // fall back to the engine's configured global default.
                    let effective_timeout = step_def
                        .timeout_secs
                        .map(|s| Duration::from_secs(u64::from(s)))
                        .unwrap_or(self.step_timeout);

                    futs.push(Box::pin(Self::wait_for_task_completion(
                        step_id.clone(),
                        task_id,
                        rx,
                        effective_timeout,
                        cancel_token,
                    )));
                }

                // Fan-in: collect all results for this layer.
                //
                // NOTE: branch conditions are NOT re-evaluated after a cancel
                // or timeout — the DAG continues with the step's terminal status
                // as input. Cascading cancellation of dependent steps is out of scope.
                let mut layer_failed: Option<(StepId, String)> = None;

                while let Some((step_id, result)) = futs.next().await {
                    // Deregister the cancel token regardless of outcome — the step
                    // has reached a terminal state and the token is no longer needed.
                    let step_run_id = format!("{}/{}", self.run.run_id.0, step_id.0);
                    self.cancel_registry.deregister(&step_run_id);

                    let step_def = match self.find_step(&step_id) {
                        Some(d) => d.clone(),
                        None => continue,
                    };

                    match result {
                        StepResult::Completed(output) => {
                            self.done_steps.insert(step_id.clone());
                            self.set_step_completed(&step_id, &output)?;
                            self.template_ctx
                                .insert_step_output(step_id.clone(), output.clone());
                            // If this is a fallback step, also inject its output
                            // under the original step's name so downstream templates
                            // resolve `{{steps.<original>.output}}` correctly.
                            if let Some(ref fb_for) = step_def.fallback_for {
                                self.template_ctx
                                    .insert_step_output(fb_for.clone(), output.clone());
                            }
                            let _ = self.event_bus.send(RuntimeEvent::PipelineStepCompleted {
                                run_id: self.run.run_id.0.clone(),
                                step_id: step_id.0.clone(),
                            });
                            info!(
                                run_id = %self.run.run_id,
                                step_id = %step_id,
                                "step completed",
                            );
                        }

                        StepResult::Failed(reason) => {
                            match step_def.on_failure {
                                StepFailurePolicy::Fail => {
                                    self.done_steps.insert(step_id.clone());
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
                                    self.done_steps.insert(step_id.clone());
                                    self.set_step_skipped(&step_id, &reason)?;
                                    // Downstream templates resolve the skipped step to "".
                                    self.template_ctx
                                        .insert_step_output(step_id.clone(), String::new());
                                    let _ =
                                        self.event_bus.send(RuntimeEvent::PipelineStepSkipped {
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
                                    match self.activate_fallback(&step_id, &reason)? {
                                        Some(_fallback_id) => {
                                            // Fallback step is now active. Restart the
                                            // layer loop so the graph is re-evaluated
                                            // with the fallback step included.
                                            continue 'restart;
                                        }
                                        None => {
                                            // No fallback declared — pipeline already
                                            // marked as failed by activate_fallback.
                                            return Ok(());
                                        }
                                    }
                                }
                            }
                        }

                        StepResult::Cancelled => {
                            // The operator explicitly cancelled this step.
                            // Record Cancelled status in SQLite, then abort the pipeline.
                            self.done_steps.insert(step_id.clone());
                            self.set_step_cancelled(&step_id)?;
                            {
                                let mut repo =
                                    self.repo.lock().unwrap_or_else(|e| e.into_inner());
                                repo.fail_run_keep_step(&self.run.run_id, &step_id)?;
                            }
                            let _ = self.event_bus.send(RuntimeEvent::PipelineFailed {
                                run_id: self.run.run_id.0.clone(),
                                pipeline_id: self.definition.id.0.clone(),
                                step_id: step_id.0.clone(),
                                reason: "cancelled by operator".to_string(),
                            });
                            info!(
                                run_id   = %self.run.run_id,
                                step_id  = %step_id,
                                status   = "cancelled",
                                "step cancelled by operator",
                            );
                            return Ok(());
                        }

                        StepResult::InputRequired { task_id } => {
                            // ── HITL suspend/resume ──────────────
                            //
                            // Protocol: subscribe EventBus BEFORE persisting the
                            // suspension so a very fast operator response
                            // (TaskResumed) is never missed.
                            let rx_resume = self.event_bus.subscribe();

                            // Persist WaitingApproval + emit PipelineSuspended.
                            // Capture the request time for HITL audit.
                            let hitl_request_time = self.suspend_for_hitl(&step_id, &task_id)?;

                            // Block until the operator approves or rejects.
                            let resume =
                                Self::wait_for_resume(&task_id, rx_resume, hitl_request_time)
                                    .await;

                            if resume.approved {
                                // Record HITL audit fields before restoring run status.
                                if let (Some(ref by), Some(dur)) =
                                    (&resume.approved_by, resume.approval_duration_ms)
                                {
                                    let mut repo =
                                        self.repo.lock().unwrap_or_else(|e| e.into_inner());
                                    repo.record_hitl_approval(
                                        &self.run.run_id,
                                        &step_id,
                                        by,
                                        dur,
                                    )?;
                                }

                                // Restore run status to Running in SQLite.
                                {
                                    let mut repo =
                                        self.repo.lock().unwrap_or_else(|e| e.into_inner());
                                    repo.resume_run(&self.run.run_id)?;
                                }
                                let _ = self.event_bus.send(RuntimeEvent::PipelineResumed {
                                    run_id: self.run.run_id.0.clone(),
                                    step_id: step_id.0.clone(),
                                });
                                info!(
                                    run_id  = %self.run.run_id,
                                    step_id = %step_id,
                                    task_id = %task_id,
                                    "HITL approved — waiting for task to complete",
                                );

                                // NOTE: RuntimeEvent::TaskResumed (apollia-core Sprint 11)
                                // does not carry `new_task_id`. ORIA resumes the original
                                // task, so we subscribe fresh and wait for TaskCompleted on
                                // the same task_id.  The resumed wait uses the step's
                                // configured timeout (per-step or global default).
                                let hitl_step_timeout = self
                                    .find_step(&step_id)
                                    .and_then(|d| d.timeout_secs)
                                    .map(|s| Duration::from_secs(u64::from(s)))
                                    .unwrap_or(self.step_timeout);
                                let rx_complete = self.event_bus.subscribe();
                                let (_, final_result) = Self::wait_for_task_completion(
                                    step_id.clone(),
                                    task_id,
                                    rx_complete,
                                    hitl_step_timeout,
                                    CancellationToken::new(),
                                )
                                .await;

                                match final_result {
                                    StepResult::Completed(output) => {
                                        self.done_steps.insert(step_id.clone());
                                        self.set_step_completed(&step_id, &output)?;
                                        self.template_ctx
                                            .insert_step_output(step_id.clone(), output.clone());
                                        if let Some(ref fb_for) = step_def.fallback_for {
                                            self.template_ctx
                                                .insert_step_output(fb_for.clone(), output.clone());
                                        }
                                        let _ = self.event_bus.send(
                                            RuntimeEvent::PipelineStepCompleted {
                                                run_id: self.run.run_id.0.clone(),
                                                step_id: step_id.0.clone(),
                                            },
                                        );
                                        info!(
                                            run_id  = %self.run.run_id,
                                            step_id = %step_id,
                                            "step completed after HITL approval",
                                        );
                                    }

                                    StepResult::Failed(reason) => match step_def.on_failure {
                                        StepFailurePolicy::Fail => {
                                            self.done_steps.insert(step_id.clone());
                                            self.set_step_failed(&step_id, &reason)?;
                                            let _ = self.event_bus.send(
                                                RuntimeEvent::PipelineStepFailed {
                                                    run_id: self.run.run_id.0.clone(),
                                                    step_id: step_id.0.clone(),
                                                    reason: reason.clone(),
                                                    on_failure: "fail".into(),
                                                },
                                            );
                                            if layer_failed.is_none() {
                                                layer_failed = Some((step_id, reason));
                                            }
                                        }
                                        StepFailurePolicy::Skip => {
                                            self.done_steps.insert(step_id.clone());
                                            self.set_step_skipped(&step_id, &reason)?;
                                            self.template_ctx
                                                .insert_step_output(step_id.clone(), String::new());
                                            let _ = self.event_bus.send(
                                                RuntimeEvent::PipelineStepSkipped {
                                                    run_id: self.run.run_id.0.clone(),
                                                    step_id: step_id.0.clone(),
                                                    reason,
                                                },
                                            );
                                        }
                                        StepFailurePolicy::Fallback => {
                                            match self.activate_fallback(&step_id, &reason)? {
                                                Some(_) => continue 'restart,
                                                None => return Ok(()),
                                            }
                                        }
                                    },

                                    StepResult::Cancelled => {
                                        // Operator cancelled the resumed step.
                                        self.done_steps.insert(step_id.clone());
                                        self.set_step_cancelled(&step_id)?;
                                        {
                                            let mut repo =
                                                self.repo.lock().unwrap_or_else(|e| e.into_inner());
                                            repo.fail_run_keep_step(&self.run.run_id, &step_id)?;
                                        }
                                        let _ = self.event_bus.send(RuntimeEvent::PipelineFailed {
                                            run_id: self.run.run_id.0.clone(),
                                            pipeline_id: self.definition.id.0.clone(),
                                            step_id: step_id.0.clone(),
                                            reason: "cancelled by operator".to_string(),
                                        });
                                        info!(
                                            run_id  = %self.run.run_id,
                                            step_id = %step_id,
                                            status  = "cancelled",
                                            "step cancelled by operator after HITL resume",
                                        );
                                        return Ok(());
                                    }

                                    StepResult::InputRequired { .. } => {
                                        // Nested HITL not supported — fail fast (Principle #4).
                                        warn!(
                                            run_id  = %self.run.run_id,
                                            step_id = %step_id,
                                            "nested HITL suspension not supported \
                                             — failing pipeline",
                                        );
                                        return self.fail_pipeline_internal(
                                            &step_id,
                                            "nested HITL not supported",
                                        );
                                    }
                                }
                            } else {
                                // Operator rejected — fail the pipeline immediately
                                // (Principle #4: fail fast).
                                info!(
                                    run_id  = %self.run.run_id,
                                    step_id = %step_id,
                                    task_id = %task_id,
                                    "HITL rejected by operator",
                                );
                                return self
                                    .fail_pipeline_internal(&step_id, "rejected by operator");
                            }
                        }
                    }
                }

                // Abort after draining the layer if a fatal failure was recorded.
                if let Some((failed_step, reason)) = layer_failed {
                    return self.fail_pipeline_internal(&failed_step, &reason);
                }
            }

            // All layers processed without triggering a fallback restart.
            break 'restart;
        }

        // 5 — All layers completed successfully.
        self.complete_pipeline()
    }

    // ── Private helpers ───────────────────────────────────────────────────────

    /// Inserts a `Pending` step row for every step defined in the pipeline.
    fn init_step_rows(&self) -> Result<(), ExecutorError> {
        // When resuming after a restart the step rows already exist in SQLite;
        // re-inserting them would produce StepAlreadyExists errors.
        if self.is_resume {
            return Ok(());
        }
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
                approved_by: None,
                approval_duration_ms: None,
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

    /// Updates the step row to `FallbackActive` with the failure `reason`.
    ///
    /// Called by [`activate_fallback`](Self::activate_fallback) to record that
    /// the primary step was superseded by its designated fallback step.
    fn set_step_fallback_active(
        &self,
        step_id: &StepId,
        reason: &str,
    ) -> Result<(), ExecutorError> {
        let mut repo = self.repo.lock().unwrap_or_else(|e| e.into_inner());
        repo.update_step(
            &self.run.run_id,
            step_id,
            &StepRunStatus::FallbackActive,
            None,
            Some(reason),
            None,
        )?;
        Ok(())
    }

    /// Updates the step row to `Cancelled`.
    ///
    /// Called when the operator explicitly cancels a running step via
    /// [`StepCancelRegistry::cancel_step`]. The `Cancelled` status is distinct from
    /// `Failed` — it represents a deliberate operator action, not an error condition.
    fn set_step_cancelled(&self, step_id: &StepId) -> Result<(), ExecutorError> {
        let mut repo = self.repo.lock().unwrap_or_else(|e| e.into_inner());
        repo.update_step(
            &self.run.run_id,
            step_id,
            &StepRunStatus::Cancelled,
            None,
            None,
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

    /// Activates the fallback step for `failed_step_id`.
    ///
    /// Steps performed:
    /// 1. Marks `failed_step_id` as [`StepRunStatus::FallbackActive`] in SQLite
    ///    and inserts it into [`done_steps`](Self::done_steps).
    /// 2. Emits [`RuntimeEvent::PipelineStepFailed`] for the original step.
    /// 3. Finds the step declared with `fallback_for = failed_step_id`.
    /// 4. Inserts the fallback step's ID into [`active_fallbacks`](Self::active_fallbacks)
    ///    so the layer filter includes it on the next `'restart` pass.
    ///
    /// Returns `Some(fallback_step_id)` when a fallback is found and activated.
    /// Returns `None` when no fallback is declared — in that case
    /// [`fail_pipeline_internal`](Self::fail_pipeline_internal) has already
    /// been called and the caller must propagate the terminal state via `return Ok(())`.
    ///
    /// # Errors
    ///
    /// Returns [`ExecutorError::Repository`] if a SQLite operation fails.
    fn activate_fallback(
        &mut self,
        failed_step_id: &StepId,
        reason: &str,
    ) -> Result<Option<StepId>, ExecutorError> {
        // 1. Mark the original step as FallbackActive.
        self.set_step_fallback_active(failed_step_id, reason)?;
        self.done_steps.insert(failed_step_id.clone());

        // 2. Emit PipelineStepFailed for the original step.
        let _ = self.event_bus.send(RuntimeEvent::PipelineStepFailed {
            run_id: self.run.run_id.0.clone(),
            step_id: failed_step_id.0.clone(),
            reason: reason.to_string(),
            on_failure: "fallback".into(),
        });
        warn!(
            run_id = %self.run.run_id,
            step_id = %failed_step_id,
            reason = reason,
            "step failed — looking for fallback step",
        );

        // 3. Find the declared fallback step (fallback_for == failed_step_id).
        let fallback_id = self
            .definition
            .steps
            .iter()
            .find(|s| s.fallback_for.as_ref() == Some(failed_step_id))
            .map(|s| s.id.clone());

        match fallback_id {
            Some(fid) => {
                // 4. Activate the fallback step so the layer filter includes it.
                self.active_fallbacks.insert(fid.clone());
                info!(
                    run_id = %self.run.run_id,
                    failed_step = %failed_step_id,
                    fallback_step = %fid,
                    "fallback step activated — restarting layer evaluation",
                );
                Ok(Some(fid))
            }
            None => {
                // No fallback declared — fail the pipeline immediately (Principle #4).
                let no_fallback_reason =
                    format!("no fallback declared for step '{failed_step_id}'");
                self.fail_pipeline_internal(failed_step_id, &no_fallback_reason)?;
                Ok(None)
            }
        }
    }

    // ── HITL helpers ──────────────────────────────────────────────────────────

    /// Persists the pipeline suspension and emits [`RuntimeEvent::PipelineSuspended`].
    ///
    /// Transitions the run to [`PipelineStatus::WaitingApproval`] in SQLite and
    /// broadcasts `PipelineSuspended` on the EventBus.
    ///
    /// Returns the wall-clock time of the suspension request so the caller can
    /// compute `approval_duration_ms` once the operator responds.
    ///
    /// # Protocol
    ///
    /// The caller **must** subscribe to the EventBus for
    /// [`RuntimeEvent::TaskResumed`] **before** calling this method to avoid the
    /// race condition where the operator approves the step before the executor
    /// starts listening.
    ///
    /// # Errors
    ///
    /// Returns [`ExecutorError::Repository`] if the SQLite update fails.
    fn suspend_for_hitl(
        &self,
        step_id: &StepId,
        task_id: &str,
    ) -> Result<chrono::DateTime<Utc>, ExecutorError> {
        let request_time = Utc::now();
        {
            let mut repo = self.repo.lock().unwrap_or_else(|e| e.into_inner());
            repo.suspend_run(&self.run.run_id, step_id, task_id)?;
        }
        let _ = self.event_bus.send(RuntimeEvent::PipelineSuspended {
            run_id: self.run.run_id.0.clone(),
            step_id: step_id.0.clone(),
            task_id: task_id.to_string(),
        });
        info!(
            run_id  = %self.run.run_id,
            step_id = %step_id,
            task_id = %task_id,
            "pipeline suspended — awaiting HITL approval",
        );
        Ok(request_time)
    }

    /// Blocks until a [`RuntimeEvent::TaskResumed`] matching `task_id` arrives
    /// on the pre-subscribed `rx`.
    ///
    /// Returns a [`HitlResume`] with the approval decision and HITL audit fields
    /// (`approved_by`, `approval_duration_ms`).  The duration is computed from
    /// `request_time` to the moment the resume event arrives.
    ///
    /// # Deadlock safety
    ///
    /// `rx` must have been subscribed **before** the corresponding
    /// [`suspend_for_hitl`](Self::suspend_for_hitl) call so that a very fast
    /// operator response cannot be missed.
    ///
    /// On `EventBus` closure (runtime shutdown), the function returns a rejected
    /// [`HitlResume`] to avoid an infinite wait.
    async fn wait_for_resume(
        task_id: &str,
        mut rx: broadcast::Receiver<RuntimeEvent>,
        request_time: chrono::DateTime<Utc>,
    ) -> HitlResume {
        loop {
            match rx.recv().await {
                Ok(RuntimeEvent::TaskResumed {
                    task_id: tid,
                    approved,
                }) if tid == task_id => {
                    let duration_ms = (Utc::now() - request_time).num_milliseconds();
                    return HitlResume {
                        approved,
                        approved_by: if approved {
                            Some("operator".to_string())
                        } else {
                            None
                        },
                        approval_duration_ms: if approved { Some(duration_ms) } else { None },
                    };
                }

                Ok(_) => {
                    // Unrelated event — keep waiting.
                }

                Err(broadcast::error::RecvError::Lagged(n)) => {
                    warn!(
                        missed   = n,
                        task_id  = %task_id,
                        "EventBus lagged in wait_for_resume — some events skipped",
                    );
                    // Continue: TaskResumed may still arrive.
                }

                Err(broadcast::error::RecvError::Closed) => {
                    // EventBus shut down — treat as rejection to avoid blocking forever.
                    warn!(
                        task_id = %task_id,
                        "EventBus closed while waiting for HITL resume \
                         — treating as rejected",
                    );
                    return HitlResume {
                        approved: false,
                        approved_by: None,
                        approval_duration_ms: None,
                    };
                }
            }
        }
    }

    /// Returns the step definition matching `step_id`, or `None`.
    fn find_step(&self, step_id: &StepId) -> Option<&PipelineStepDef> {
        self.definition.steps.iter().find(|s| &s.id == step_id)
    }

    // ── Static async helper ───────────────────────────────────────────────────

    /// Waits for `TaskCompleted` or `TaskInputRequired` on an already-subscribed
    /// EventBus receiver, bounded by `step_timeout` and interruptible by `cancel_token`.
    ///
    /// The function races three futures:
    /// 1. The cancellation token — returns [`StepResult::Cancelled`] immediately.
    /// 2. The timeout — returns [`StepResult::Failed`] with a reason string.
    /// 3. The EventBus listener — returns `Completed`, `Failed`, or `InputRequired`.
    ///
    /// Accepting a pre-subscribed `rx` upholds the subscribe-before-submit
    /// invariant: the subscription is established in `execute` before the
    /// corresponding `submit_task` call returns.
    async fn wait_for_task_completion(
        step_id: StepId,
        task_id: String,
        mut rx: broadcast::Receiver<RuntimeEvent>,
        step_timeout: Duration,
        cancel_token: CancellationToken,
    ) -> (StepId, StepResult) {
        let timeout_secs = step_timeout.as_secs();

        let event_future = async move {
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
        };

        tokio::select! {
            _ = cancel_token.cancelled() => {
                (step_id, StepResult::Cancelled)
            }
            result = tokio::time::timeout(step_timeout, event_future) => {
                match result {
                    Ok(step_result) => (step_id, step_result),
                    Err(_elapsed) => (
                        step_id,
                        StepResult::Failed(format!("step timeout after {timeout_secs}s")),
                    ),
                }
            }
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
    /// any event — used to trigger the per-step timeout.  Agents listed in
    /// `input_required` emit `TaskInputRequired` instead of `TaskCompleted`.
    struct MockSubmitter {
        event_bus: EventBusSender,
        /// (agent_name → (success, output_text))
        agents: Arc<Mutex<HashMap<String, (bool, String)>>>,
        /// Agents for which no event is emitted (timeout simulation).
        hanging: Arc<Mutex<HashSet<String>>>,
        /// Agents that emit `TaskInputRequired` — used for HITL tests.
        input_required: Arc<Mutex<HashSet<String>>>,
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
                input_required: Arc::new(Mutex::new(HashSet::new())),
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

        /// Configures `agent` to emit `TaskInputRequired` — simulates HITL.
        fn with_input_required(self, agent: &str) -> Self {
            self.input_required
                .lock()
                .unwrap()
                .insert(agent.to_string());
            self
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

            // Emit TaskInputRequired for HITL-configured agents.
            if self.input_required.lock().unwrap().contains(agent) {
                let event_bus = self.event_bus.clone();
                let tid = task_id.clone();
                tokio::spawn(async move {
                    tokio::task::yield_now().await;
                    let _ = event_bus.send(RuntimeEvent::TaskInputRequired {
                        task_id: TaskId::from(tid.as_str()),
                        prompt: "Approval required for HITL step".into(),
                        step_id: None,
                    });
                });
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
            timeout_secs: None,
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

    // ── Sequential pipeline ─────────────────────────────────────────────

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

    // ── Fan-out ─────────────────────────────────────────────────────────

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

    // ── on_failure = skip ───────────────────────────────────────────────

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

    // ── on_failure = fail ───────────────────────────────────────────────

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

    // ── Downstream not submitted after fatal failure ────────────────────

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

    // ── Per-step timeout ────────────────────────────────────────────────

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

    // ── fallback activated on step failure ────────────────────

    #[tokio::test]
    async fn test_s113_ac1_fallback_activated_on_failure() {
        // GIVEN — pipeline: ocr → validation (fails, on_failure=Fallback)
        //                   ocr → validation-manuelle (fallback_for=validation)
        //         notification depends on [validation] and uses {{steps.validation.output}}
        let (tx, mut rx) = broadcast::channel::<RuntimeEvent>(128);

        let ocr = make_step(
            "ocr",
            "ocr-agent",
            "{{trigger.payload}}",
            &[],
            StepFailurePolicy::Fail,
        );

        let validation = PipelineStepDef {
            id: StepId("validation".into()),
            agent: "validation-agent".into(),
            input: "{{steps.ocr.output}}".into(),
            depends_on: vec![StepId("ocr".into())],
            on_failure: StepFailurePolicy::Fallback,
            condition: None,
            fallback_for: None,
            timeout_secs: None,
        };
        let validation_manuelle = PipelineStepDef {
            id: StepId("validation-manuelle".into()),
            agent: "validation-manuelle-agent".into(),
            input: "Manual: {{steps.ocr.output}}".into(),
            depends_on: vec![StepId("ocr".into())],
            on_failure: StepFailurePolicy::Fail,
            condition: None,
            fallback_for: Some(StepId("validation".into())),
            timeout_secs: None,
        };
        let notification = PipelineStepDef {
            id: StepId("notification".into()),
            agent: "notification-agent".into(),
            input: "Result: {{steps.validation.output}}".into(),
            depends_on: vec![StepId("validation".into())],
            on_failure: StepFailurePolicy::Fail,
            condition: None,
            fallback_for: None,
            timeout_secs: None,
        };

        let def = make_pipeline(vec![ocr, validation, validation_manuelle, notification]);
        let run = make_run(&def);

        let mock = MockSubmitter::new(tx.clone())
            .with_success("ocr-agent", "ocr-output")
            .with_failure("validation-agent")
            .with_success("validation-manuelle-agent", "validé manuellement")
            .with_success("notification-agent", "notified");
        let submitted = mock.submitted.clone();

        // WHEN
        let result = make_executor(def, run, mock, tx.clone(), 500)
            .execute()
            .await;

        // THEN — pipeline completes without an execution error
        assert!(result.is_ok(), "unexpected error: {result:?}");

        // AND all four agents were submitted (validation-manuelle replaced validation)
        let inputs = submitted.lock().unwrap().clone();
        let agents: Vec<&str> = inputs.iter().map(|(a, _)| a.as_str()).collect();
        assert!(agents.contains(&"ocr-agent"), "ocr-agent must be submitted");
        assert!(
            agents.contains(&"validation-agent"),
            "validation-agent must be submitted (and fail)"
        );
        assert!(
            agents.contains(&"validation-manuelle-agent"),
            "validation-manuelle-agent must be submitted as fallback"
        );
        assert!(
            agents.contains(&"notification-agent"),
            "notification-agent must be submitted after fallback completes"
        );

        // notification received the fallback output via {{steps.validation.output}}
        let notif_input = inputs
            .iter()
            .find(|(a, _)| a == "notification-agent")
            .map(|(_, i)| i.as_str());
        assert_eq!(
            notif_input,
            Some("Result: validé manuellement"),
            "notification input must contain fallback output via {{steps.validation.output}}"
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

    // ── no fallback declared → pipeline fails ─────────────────

    #[tokio::test]
    async fn test_s113_ac3_no_fallback_declared_fails_pipeline() {
        // GIVEN — pipeline [A, B(on_failure=Fallback)] without a fallback_for=B step
        let (tx, mut rx) = broadcast::channel::<RuntimeEvent>(64);

        let steps = vec![
            make_step("A", "agent-a", "start", &[], StepFailurePolicy::Fail),
            PipelineStepDef {
                id: StepId("B".into()),
                agent: "agent-b".into(),
                input: "{{steps.A.output}}".into(),
                depends_on: vec![StepId("A".into())],
                on_failure: StepFailurePolicy::Fallback,
                condition: None,
                fallback_for: None,
                timeout_secs: None,
            },
        ];
        let def = make_pipeline(steps);
        let run = make_run(&def);
        let mock = MockSubmitter::new(tx.clone())
            .with_success("agent-a", "a-ok")
            .with_failure("agent-b");

        // WHEN
        let result = make_executor(def, run, mock, tx, 500).execute().await;

        // THEN — execute returns Ok (pipeline failure is a terminal state, not an Err)
        assert!(result.is_ok(), "execute must return Ok on pipeline failure");

        // AND PipelineFailed was emitted with 'no fallback declared' reason
        let mut events = Vec::new();
        while let Ok(e) = rx.try_recv() {
            events.push(e);
        }
        let failed_with_reason = events.iter().any(|e| {
            matches!(
                e,
                RuntimeEvent::PipelineFailed { reason, .. }
                if reason.contains("no fallback declared")
            )
        });
        assert!(
            failed_with_reason,
            "PipelineFailed with 'no fallback declared' reason must be emitted"
        );
    }

    // ── FallbackActive status persisted in SQLite ────────────

    #[tokio::test]
    async fn test_s113_ac4_fallback_active_status_in_sqlite() {
        // GIVEN — pipeline: ocr → validation (fails, Fallback)
        //                   ocr → validation-manuelle (fallback_for=validation, succeeds)
        let (tx, _rx) = broadcast::channel::<RuntimeEvent>(64);

        let steps = vec![
            make_step("ocr", "ocr-agent", "input", &[], StepFailurePolicy::Fail),
            PipelineStepDef {
                id: StepId("validation".into()),
                agent: "validation-agent".into(),
                input: "{{steps.ocr.output}}".into(),
                depends_on: vec![StepId("ocr".into())],
                on_failure: StepFailurePolicy::Fallback,
                condition: None,
                fallback_for: None,
                timeout_secs: None,
            },
            PipelineStepDef {
                id: StepId("validation-manuelle".into()),
                agent: "validation-manuelle-agent".into(),
                input: "Manual".into(),
                depends_on: vec![StepId("ocr".into())],
                on_failure: StepFailurePolicy::Fail,
                condition: None,
                fallback_for: Some(StepId("validation".into())),
                timeout_secs: None,
            },
        ];
        let def = make_pipeline(steps);
        let run = make_run(&def);
        let repo = make_repo();
        {
            let mut r = repo.lock().unwrap();
            r.insert_run(&run).unwrap();
        }
        let mock = MockSubmitter::new(tx.clone())
            .with_success("ocr-agent", "ocr-done")
            .with_failure("validation-agent")
            .with_success("validation-manuelle-agent", "manuelle-done");

        // WHEN
        let executor = PipelineExecutor::new(def, run.clone(), mock, tx, Arc::clone(&repo))
            .with_step_timeout(Duration::from_millis(500));
        executor.execute().await.unwrap();

        // THEN — "validation" step has FallbackActive status in SQLite
        let r = repo.lock().unwrap();
        let found_run = r.find_run(&run.run_id).unwrap().unwrap();

        let validation_status = found_run
            .step_runs
            .get(&StepId("validation".into()))
            .expect("validation step must exist in SQLite")
            .status
            .clone();
        assert_eq!(
            validation_status,
            StepRunStatus::FallbackActive,
            "validation step must have FallbackActive status"
        );

        // AND "validation-manuelle" step has Completed status
        let fallback_status = found_run
            .step_runs
            .get(&StepId("validation-manuelle".into()))
            .expect("validation-manuelle step must exist in SQLite")
            .status
            .clone();
        assert_eq!(
            fallback_status,
            StepRunStatus::Completed,
            "validation-manuelle step must have Completed status"
        );
    }

    // ── EventBus events ─────────────────────────────────────────────────

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

    // ── InputRequired → PipelineSuspended ─────────────────────

    #[tokio::test]
    async fn test_s114_ac1_input_required_suspends_pipeline() {
        // GIVEN — single step "comptabilite" whose agent emits TaskInputRequired
        let (tx, mut rx) = broadcast::channel::<RuntimeEvent>(64);
        let steps = vec![make_step(
            "comptabilite",
            "comptabilite-agent",
            "process",
            &[],
            StepFailurePolicy::Fail,
        )];
        let def = make_pipeline(steps);
        let run = make_run(&def);
        let repo = make_repo();
        {
            let mut r = repo.lock().unwrap();
            r.insert_run(&run).unwrap();
        }
        let run_id = run.run_id.clone();
        let mock = MockSubmitter::new(tx.clone()).with_input_required("comptabilite-agent");

        // Spawn a watcher that unblocks execute() by sending TaskResumed { approved: false }
        // as soon as PipelineSuspended is observed — this only verifies the suspension event
        // and the SQLite status; the rejection path terminates the pipeline cleanly.
        let tx_watcher = tx.clone();
        let mut rx_watcher = tx.subscribe();
        tokio::spawn(async move {
            // GIVEN (watcher) — wait for PipelineSuspended, extract task_id
            while let Ok(event) = rx_watcher.recv().await {
                if let RuntimeEvent::PipelineSuspended { task_id, .. } = event {
                    // WHEN (watcher) — operator rejects (to unblock execute())
                    let _ = tx_watcher.send(RuntimeEvent::TaskResumed {
                        task_id: TaskId::from(task_id.as_str()),
                        approved: false,
                    });
                    break;
                }
            }
        });

        // WHEN — execute the pipeline; it suspends on "comptabilite" then fails on rejection
        let executor = PipelineExecutor::new(def, run, mock, tx, Arc::clone(&repo))
            .with_step_timeout(Duration::from_millis(500));
        let result = executor.execute().await;

        // THEN — execute returns Ok (suspension + rejection is a terminal state, not an Err)
        assert!(result.is_ok(), "unexpected error: {result:?}");

        // AND PipelineSuspended was emitted for "comptabilite"
        let mut events = Vec::new();
        while let Ok(e) = rx.try_recv() {
            events.push(e);
        }
        let suspended = events.iter().any(|e| {
            matches!(
                e,
                RuntimeEvent::PipelineSuspended { step_id, .. }
                if step_id == "comptabilite"
            )
        });
        assert!(
            suspended,
            "PipelineSuspended must be emitted for 'comptabilite'"
        );

        // AND SQLite run status is Failed (WaitingApproval → rejected → Failed)
        let r = repo.lock().unwrap();
        let found = r.find_run(&run_id).unwrap().unwrap();
        assert!(
            matches!(found.status, PipelineStatus::Failed { .. }),
            "run must end in Failed after rejection; got {:?}",
            found.status,
        );
    }

    // ── Approval → pipeline resumes and completes ─────────────

    #[tokio::test]
    async fn test_s114_ac2_approve_resumes_pipeline() {
        // GIVEN — pipeline [A] → [comptabilite (HITL)] → [B]
        let (tx, mut rx) = broadcast::channel::<RuntimeEvent>(128);
        let steps = vec![
            make_step("A", "agent-a", "start", &[], StepFailurePolicy::Fail),
            make_step(
                "comptabilite",
                "comptabilite-agent",
                "{{steps.A.output}}",
                &["A"],
                StepFailurePolicy::Fail,
            ),
            make_step(
                "B",
                "agent-b",
                "{{steps.comptabilite.output}}",
                &["comptabilite"],
                StepFailurePolicy::Fail,
            ),
        ];
        let def = make_pipeline(steps);
        let run = make_run(&def);
        let repo = make_repo();
        {
            let mut r = repo.lock().unwrap();
            r.insert_run(&run).unwrap();
        }

        let mock = MockSubmitter::new(tx.clone())
            .with_success("agent-a", "a-done")
            .with_input_required("comptabilite-agent")
            .with_success("agent-b", "b-done");

        // WHEN (watcher) — approve the HITL step, then simulate the resumed task completing
        let tx_watcher = tx.clone();
        let mut rx_watcher = tx.subscribe();
        tokio::spawn(async move {
            while let Ok(event) = rx_watcher.recv().await {
                if let RuntimeEvent::PipelineSuspended { task_id, .. } = event {
                    // Send approval
                    let _ = tx_watcher.send(RuntimeEvent::TaskResumed {
                        task_id: TaskId::from(task_id.as_str()),
                        approved: true,
                    });
                    // Yield briefly so execute() can re-subscribe for TaskCompleted
                    tokio::time::sleep(Duration::from_millis(10)).await;
                    // Simulate ORIA completing the resumed task
                    let _ = tx_watcher.send(RuntimeEvent::TaskCompleted {
                        agent_id: "comptabilite-agent".into(),
                        task_id: TaskId::from(task_id.as_str()),
                        success: true,
                        output: Some("comptabilite-approved-output".into()),
                    });
                    break;
                }
            }
        });

        // WHEN — run pipeline to completion
        let executor = PipelineExecutor::new(def, run, mock, tx, Arc::clone(&repo))
            .with_step_timeout(Duration::from_millis(500));
        let result = executor.execute().await;

        // THEN — pipeline completed without error
        assert!(result.is_ok(), "unexpected error: {result:?}");

        // AND collect events
        let mut events = Vec::new();
        while let Ok(e) = rx.try_recv() {
            events.push(e);
        }

        // PipelineSuspended was emitted
        let suspended = events
            .iter()
            .any(|e| matches!(e, RuntimeEvent::PipelineSuspended { step_id, .. } if step_id == "comptabilite"));
        assert!(suspended, "PipelineSuspended must be emitted");

        // PipelineResumed was emitted
        let resumed = events
            .iter()
            .any(|e| matches!(e, RuntimeEvent::PipelineResumed { step_id, .. } if step_id == "comptabilite"));
        assert!(resumed, "PipelineResumed must be emitted");

        // Pipeline completed (not failed)
        let completed = events
            .iter()
            .any(|e| matches!(e, RuntimeEvent::PipelineCompleted { .. }));
        let failed = events
            .iter()
            .any(|e| matches!(e, RuntimeEvent::PipelineFailed { .. }));
        assert!(
            completed,
            "PipelineCompleted must be emitted after approval"
        );
        assert!(!failed, "PipelineFailed must NOT be emitted");
    }

    // ── Rejection → PipelineFailed ────────────────────────────

    #[tokio::test]
    async fn test_s114_ac3_reject_fails_pipeline() {
        // GIVEN — single step "comptabilite" that triggers HITL
        let (tx, mut rx) = broadcast::channel::<RuntimeEvent>(64);
        let steps = vec![make_step(
            "comptabilite",
            "comptabilite-agent",
            "process",
            &[],
            StepFailurePolicy::Fail,
        )];
        let def = make_pipeline(steps);
        let run = make_run(&def);
        let repo = make_repo();
        {
            let mut r = repo.lock().unwrap();
            r.insert_run(&run).unwrap();
        }

        let mock = MockSubmitter::new(tx.clone()).with_input_required("comptabilite-agent");

        // WHEN (watcher) — operator rejects the HITL request
        let tx_watcher = tx.clone();
        let mut rx_watcher = tx.subscribe();
        tokio::spawn(async move {
            while let Ok(event) = rx_watcher.recv().await {
                if let RuntimeEvent::PipelineSuspended { task_id, .. } = event {
                    let _ = tx_watcher.send(RuntimeEvent::TaskResumed {
                        task_id: TaskId::from(task_id.as_str()),
                        approved: false,
                    });
                    break;
                }
            }
        });

        // WHEN — run pipeline
        let executor = PipelineExecutor::new(def, run, mock, tx, Arc::clone(&repo))
            .with_step_timeout(Duration::from_millis(500));
        let result = executor.execute().await;

        // THEN — execute returns Ok (rejection is a terminal state, not an Err)
        assert!(result.is_ok(), "unexpected error: {result:?}");

        // AND collect events
        let mut events = Vec::new();
        while let Ok(e) = rx.try_recv() {
            events.push(e);
        }

        // PipelineFailed emitted with "rejected by operator"
        let failed_with_reason = events.iter().any(|e| {
            matches!(
                e,
                RuntimeEvent::PipelineFailed { reason, .. }
                if reason.contains("rejected by operator")
            )
        });
        assert!(
            failed_with_reason,
            "PipelineFailed with 'rejected by operator' reason must be emitted",
        );

        // PipelineCompleted must NOT be emitted
        let completed = events
            .iter()
            .any(|e| matches!(e, RuntimeEvent::PipelineCompleted { .. }));
        assert!(
            !completed,
            "PipelineCompleted must NOT be emitted on rejection"
        );
    }

    // ── PipelinesConfig — step timeout enforcement ──────────────────────────

    #[tokio::test]
    async fn test_pipeline_step_timeout_enforced() {
        // GIVEN a pipeline with a hanging step and a 50 ms timeout via with_pipelines_config
        let (tx, mut rx) = broadcast::channel::<RuntimeEvent>(64);
        let steps = vec![make_step(
            "slow",
            "slow-agent",
            "run",
            &[],
            StepFailurePolicy::Fail,
        )];
        let def = make_pipeline(steps);
        let run = make_run(&def);
        let repo = make_repo();
        {
            let mut r = repo.lock().unwrap();
            r.insert_run(&run).unwrap();
        }
        let mock = MockSubmitter::new(tx.clone()).with_hang("slow-agent");

        // WHEN — use with_pipelines_config with a 1-second timeout (fast enough for a test)
        let config = apollia_core::PipelinesConfig {
            default_step_timeout_secs: 1,
        };
        let executor = PipelineExecutor::new(def, run, mock, tx, Arc::clone(&repo))
            .with_pipelines_config(&config);
        let result = executor.execute().await;

        // THEN — execution completes without an Err (timeout → Failed step → PipelineFailed)
        assert!(result.is_ok(), "unexpected error: {result:?}");

        // AND a PipelineFailed event is emitted
        let mut events = Vec::new();
        while let Ok(e) = rx.try_recv() {
            events.push(e);
        }
        let pipeline_failed = events
            .iter()
            .any(|e| matches!(e, RuntimeEvent::PipelineFailed { .. }));
        assert!(
            pipeline_failed,
            "PipelineFailed must be emitted after step timeout; events: {events:?}"
        );
    }

    // ── Per-step timeout overrides global ───────────────────────────────────

    #[tokio::test]
    async fn test_step_custom_timeout_overrides_global() {
        // GIVEN — a step with timeout_secs: 1 and a global timeout of 30s; agent hangs
        let (tx, _rx) = broadcast::channel::<RuntimeEvent>(64);
        let step = PipelineStepDef {
            id: StepId("slow".into()),
            agent: "slow-agent".into(),
            input: "start".into(),
            depends_on: vec![],
            on_failure: StepFailurePolicy::Fail,
            condition: None,
            fallback_for: None,
            timeout_secs: Some(1),
        };
        let def = make_pipeline(vec![step]);
        let run = make_run(&def);
        let repo = make_repo();
        {
            let mut r = repo.lock().unwrap();
            r.insert_run(&run).unwrap();
        }
        let mock = MockSubmitter::new(tx.clone()).with_hang("slow-agent");

        // WHEN — global timeout is 30s but step timeout is 1s
        let executor = PipelineExecutor::new(def, run.clone(), mock, tx, Arc::clone(&repo))
            .with_step_timeout(Duration::from_secs(30));

        // Bounded outer timeout: expect the step to time out within 3s (well under 30s)
        let start = std::time::Instant::now();
        let result = tokio::time::timeout(Duration::from_secs(3), executor.execute())
            .await
            .expect("executor must finish within 3s using the per-step 1s timeout");

        // THEN — pipeline failed (not Err), elapsed < 30s proves per-step timeout was used
        assert!(result.is_ok());
        assert!(
            start.elapsed() < Duration::from_secs(10),
            "per-step timeout must fire before the 30s global default"
        );

        let found = repo.lock().unwrap().find_run(&run.run_id).unwrap().unwrap();
        assert!(
            matches!(found.status, PipelineStatus::Failed { .. }),
            "pipeline must be Failed after step timeout"
        );
    }

    // ── Cancel running step → Cancelled status ──────────────────────────────

    #[tokio::test]
    async fn test_cancel_running_step_returns_cancelled_status() {
        // GIVEN — a single hanging step and a shared cancel registry
        let (tx, _rx) = broadcast::channel::<RuntimeEvent>(64);
        let steps = vec![make_step("A", "hang-agent", "in", &[], StepFailurePolicy::Fail)];
        let def = make_pipeline(steps);
        let run = make_run(&def);
        let repo = make_repo();
        {
            let mut r = repo.lock().unwrap();
            r.insert_run(&run).unwrap();
        }
        let mock = MockSubmitter::new(tx.clone()).with_hang("hang-agent");

        let registry = Arc::new(StepCancelRegistry::new());
        let step_run_id = format!("{}/A", run.run_id.0);
        let registry_clone = Arc::clone(&registry);

        let executor = PipelineExecutor::new(def, run.clone(), mock, tx, Arc::clone(&repo))
            .with_step_timeout(Duration::from_secs(30))
            .with_cancel_registry(Arc::clone(&registry));

        // Spawn a task that cancels the step after a short delay
        tokio::spawn(async move {
            // Give the executor time to register the cancel token
            tokio::time::sleep(Duration::from_millis(50)).await;
            registry_clone.cancel_step(&step_run_id);
        });

        // WHEN
        let result = tokio::time::timeout(Duration::from_secs(3), executor.execute())
            .await
            .expect("executor must finish within 3s after cancel");
        assert!(result.is_ok());

        // THEN — step has Cancelled status in SQLite
        let found = repo.lock().unwrap().find_run(&run.run_id).unwrap().unwrap();
        let step = found
            .step_runs
            .get(&StepId("A".into()))
            .expect("step A must be in SQLite");
        assert_eq!(
            step.status,
            StepRunStatus::Cancelled,
            "step must have Cancelled status after cancel_step()"
        );
    }

    // ── HITL approval records approved_by and approval_duration_ms ──────────

    #[tokio::test]
    async fn test_hitl_approval_records_approved_by_and_duration() {
        // GIVEN — a single HITL step
        let (tx, _rx) = broadcast::channel::<RuntimeEvent>(128);
        let steps = vec![make_step(
            "approval-step",
            "hitl-agent",
            "approve this",
            &[],
            StepFailurePolicy::Fail,
        )];
        let def = make_pipeline(steps);
        let run = make_run(&def);
        let repo = make_repo();
        {
            let mut r = repo.lock().unwrap();
            r.insert_run(&run).unwrap();
        }
        let mock = MockSubmitter::new(tx.clone()).with_input_required("hitl-agent");

        // Watcher: approve the step, then emit TaskCompleted so the executor finishes
        let tx_watcher = tx.clone();
        let mut rx_watcher = tx.subscribe();
        tokio::spawn(async move {
            while let Ok(event) = rx_watcher.recv().await {
                if let RuntimeEvent::PipelineSuspended { task_id, .. } = event {
                    let _ = tx_watcher.send(RuntimeEvent::TaskResumed {
                        task_id: TaskId::from(task_id.as_str()),
                        approved: true,
                    });
                    tokio::time::sleep(Duration::from_millis(10)).await;
                    let _ = tx_watcher.send(RuntimeEvent::TaskCompleted {
                        agent_id: "hitl-agent".into(),
                        task_id: TaskId::from(task_id.as_str()),
                        success: true,
                        output: Some("approved-output".into()),
                    });
                    break;
                }
            }
        });

        // WHEN
        let executor = PipelineExecutor::new(def, run.clone(), mock, tx, Arc::clone(&repo))
            .with_step_timeout(Duration::from_millis(500));
        let result = tokio::time::timeout(Duration::from_secs(3), executor.execute())
            .await
            .expect("executor must finish");
        assert!(result.is_ok());

        // THEN — approved_by and approval_duration_ms are populated in SQLite
        let found = repo.lock().unwrap().find_run(&run.run_id).unwrap().unwrap();
        let step = found
            .step_runs
            .get(&StepId("approval-step".into()))
            .expect("approval-step must be in SQLite");
        assert!(
            step.approved_by.is_some(),
            "approved_by must be set after HITL approval"
        );
        assert!(
            step.approval_duration_ms.is_some(),
            "approval_duration_ms must be set after HITL approval"
        );
        assert!(
            step.approval_duration_ms.unwrap() >= 0,
            "approval_duration_ms must be non-negative"
        );
    }

    // ── Step without custom timeout uses global default ──────────────────────

    #[tokio::test]
    async fn test_step_without_custom_timeout_uses_global_default() {
        // GIVEN — a step with no timeout_secs and a global timeout of 100ms; agent hangs
        let (tx, _rx) = broadcast::channel::<RuntimeEvent>(64);
        let steps = vec![make_step("A", "hang-agent", "in", &[], StepFailurePolicy::Fail)];
        let def = make_pipeline(steps);
        let run = make_run(&def);
        let repo = make_repo();
        {
            let mut r = repo.lock().unwrap();
            r.insert_run(&run).unwrap();
        }
        let mock = MockSubmitter::new(tx.clone()).with_hang("hang-agent");

        // WHEN — global timeout is 100ms; step has no override
        let executor = PipelineExecutor::new(def, run.clone(), mock, tx, Arc::clone(&repo))
            .with_step_timeout(Duration::from_millis(100));

        let start = std::time::Instant::now();
        let result = tokio::time::timeout(Duration::from_secs(3), executor.execute())
            .await
            .expect("executor must finish within 3s");
        assert!(result.is_ok());

        // THEN — elapsed is close to 100ms (global timeout applied)
        assert!(
            start.elapsed() < Duration::from_millis(800),
            "global 100ms timeout must fire well before 800ms"
        );

        let found = repo.lock().unwrap().find_run(&run.run_id).unwrap().unwrap();
        assert!(
            matches!(found.status, PipelineStatus::Failed { .. }),
            "pipeline must be Failed after timeout"
        );
    }

    // ── Cancelled status is distinct from Failed in the history ─────────────

    #[tokio::test]
    async fn test_cancelled_status_distinct_from_failed() {
        // GIVEN — two independent runs: one times out (Failed), one gets cancelled (Cancelled)

        // Run 1: step times out → status Failed
        let (tx1, _rx1) = broadcast::channel::<RuntimeEvent>(64);
        let def1 = make_pipeline(vec![make_step(
            "A",
            "hang-agent",
            "in",
            &[],
            StepFailurePolicy::Fail,
        )]);
        let run1 = PipelineRun {
            run_id: RunId("r-timeout-test".into()),
            pipeline_id: def1.id.clone(),
            trigger_id: None,
            status: PipelineStatus::Running,
            step_runs: HashMap::new(),
            trigger_payload: None,
            started_at: chrono::Utc::now(),
            ended_at: None,
        };
        let repo1 = make_repo();
        repo1.lock().unwrap().insert_run(&run1).unwrap();
        let mock1 = MockSubmitter::new(tx1.clone()).with_hang("hang-agent");
        let executor1 = PipelineExecutor::new(def1, run1.clone(), mock1, tx1, Arc::clone(&repo1))
            .with_step_timeout(Duration::from_millis(100));
        tokio::time::timeout(Duration::from_secs(3), executor1.execute())
            .await
            .expect("executor1 must finish")
            .unwrap();
        let run1_result = repo1.lock().unwrap().find_run(&run1.run_id).unwrap().unwrap();
        let step_a_timeout = run1_result.step_runs.get(&StepId("A".into())).unwrap();

        // Run 2: step gets cancelled → status Cancelled
        let (tx2, _rx2) = broadcast::channel::<RuntimeEvent>(64);
        let def2 = make_pipeline(vec![make_step(
            "A",
            "hang-agent",
            "in",
            &[],
            StepFailurePolicy::Fail,
        )]);
        let run2 = PipelineRun {
            run_id: RunId("r-cancel-test".into()),
            pipeline_id: def2.id.clone(),
            trigger_id: None,
            status: PipelineStatus::Running,
            step_runs: HashMap::new(),
            trigger_payload: None,
            started_at: chrono::Utc::now(),
            ended_at: None,
        };
        let repo2 = make_repo();
        repo2.lock().unwrap().insert_run(&run2).unwrap();
        let mock2 = MockSubmitter::new(tx2.clone()).with_hang("hang-agent");
        let registry2 = Arc::new(StepCancelRegistry::new());
        let step_run_id2 = format!("{}/A", run2.run_id.0);
        let registry2_clone = Arc::clone(&registry2);
        let executor2 = PipelineExecutor::new(def2, run2.clone(), mock2, tx2, Arc::clone(&repo2))
            .with_step_timeout(Duration::from_secs(30))
            .with_cancel_registry(Arc::clone(&registry2));
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(50)).await;
            registry2_clone.cancel_step(&step_run_id2);
        });
        tokio::time::timeout(Duration::from_secs(3), executor2.execute())
            .await
            .expect("executor2 must finish")
            .unwrap();
        let run2_result = repo2.lock().unwrap().find_run(&run2.run_id).unwrap().unwrap();
        let step_a_cancel = run2_result.step_runs.get(&StepId("A".into())).unwrap();

        // THEN — the two statuses are distinct
        assert_eq!(
            step_a_timeout.status,
            StepRunStatus::Failed,
            "timed-out step must have Failed status"
        );
        assert_eq!(
            step_a_cancel.status,
            StepRunStatus::Cancelled,
            "cancelled step must have Cancelled status"
        );
        assert_ne!(
            step_a_timeout.status, step_a_cancel.status,
            "Cancelled and Failed must be distinct statuses"
        );
    }

    // ── Cancel non-existent step returns false ───────────────────────────────

    #[tokio::test]
    async fn test_cancel_nonexistent_step_returns_error() {
        // GIVEN — an empty registry
        let registry = StepCancelRegistry::new();

        // WHEN — cancel a step that was never registered
        let cancelled = registry.cancel_step("r-unknown/step-x");

        // THEN — returns false (no token registered for that id)
        assert!(
            !cancelled,
            "cancel_step on a non-registered step must return false"
        );
    }
}
