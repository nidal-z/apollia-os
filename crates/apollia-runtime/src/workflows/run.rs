//! One run in progress: the services its code calls, and its end.

use std::sync::{Arc, Mutex, MutexGuard};

use apollia_core::RuntimeEvent;
use apollia_oria::budget::StepBudget;
use apollia_tools::TaskRepository;
use serde_json::Value;
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;

use super::{
    AgentRunner, BoxFuture, ParentRun, RunEnd, WorkflowCommand, WorkflowHandle, WorkflowServices,
    MAX_DEPTH,
};
use crate::agents::task::Nesting;
use crate::eventbus::EventBusSender;

/// Longest output a step keeps, in characters.
const STEP_OUTPUT_MAX_CHARS: usize = 4000;

/// The step a run is in, and what its nested calls produced.
#[derive(Default)]
pub(super) struct StepState {
    open: Option<String>,
    output: Option<String>,
}

/// The services of one run. Shared by the run's code and its end, never
/// between runs.
pub(super) struct RunServices {
    pub(super) run_id: String,
    pub(super) workflow_id: String,
    pub(super) depth: u32,
    pub(super) cancel: CancellationToken,
    pub(super) budget: Arc<StepBudget>,
    pub(super) repository: Arc<TaskRepository>,
    pub(super) event_bus: EventBusSender,
    pub(super) agents: Option<Arc<dyn AgentRunner>>,
    pub(super) engine: mpsc::WeakSender<WorkflowCommand>,
    pub(super) state: Mutex<StepState>,
}

impl RunServices {
    fn state(&self) -> MutexGuard<'_, StepState> {
        // A panic while the lock was held leaves plain data behind; read on.
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Keep `output` as the output of the step in progress, when one is.
    fn record_output(&self, output: String) {
        let mut state = self.state();
        if state.open.is_some() {
            state.output = Some(output.chars().take(STEP_OUTPUT_MAX_CHARS).collect());
        }
    }

    fn nested_depth(&self) -> Result<u32, String> {
        let depth = self.depth + 1;
        if depth > MAX_DEPTH {
            return Err(format!(
                "a run may nest {MAX_DEPTH} levels deep, this call would be level {depth}"
            ));
        }
        Ok(depth)
    }

    fn publish_started(&self, step: &str) {
        let _ = self.event_bus.send(RuntimeEvent::WorkflowStepStarted {
            run_id: self.run_id.clone(),
            workflow_id: self.workflow_id.clone(),
            step: step.to_string(),
        });
    }

    fn publish_ended(&self, step: &str, error: Option<String>) {
        let event = match error {
            None => RuntimeEvent::WorkflowStepCompleted {
                run_id: self.run_id.clone(),
                workflow_id: self.workflow_id.clone(),
                step: step.to_string(),
            },
            Some(error) => RuntimeEvent::WorkflowStepFailed {
                run_id: self.run_id.clone(),
                workflow_id: self.workflow_id.clone(),
                step: step.to_string(),
                error,
            },
        };
        let _ = self.event_bus.send(event);
    }

    async fn open_step(&self, step: String) -> Result<(), String> {
        if self.cancel.is_cancelled() {
            return Err("the run was stopped".into());
        }
        if let Some(reason) = self.budget.exhaustion_reason() {
            return Err(format!("the workflow budget is spent: {reason}"));
        }
        let open = self.state().open.clone();
        if let Some(open) = open {
            return Err(format!("step '{open}' is still open"));
        }
        let opened = self
            .repository
            .start_workflow_step(&self.run_id, &step)
            .await
            .map_err(|e| e.to_string())?;
        if !opened {
            return Err(format!(
                "step '{step}' is not a declared step of {} left to run",
                self.workflow_id
            ));
        }
        self.budget.increment_steps();
        *self.state() = StepState {
            open: Some(step.clone()),
            output: None,
        };
        tracing::event!(
            tracing::Level::INFO,
            run_id = %self.run_id,
            step = %step,
            "workflow.step.started"
        );
        self.publish_started(&step);
        Ok(())
    }

    async fn close_step(&self, step: String, error: Option<String>) -> Result<(), String> {
        let output = {
            let mut state = self.state();
            if state.open.as_deref() != Some(step.as_str()) {
                return Err(format!("step '{step}' is not open"));
            }
            state.open = None;
            state.output.take()
        };
        let status = match (&error, self.cancel.is_cancelled()) {
            (None, _) => "completed",
            (Some(_), true) => "canceled",
            (Some(_), false) => "failed",
        };
        self.repository
            .finish_workflow_step(
                &self.run_id,
                &step,
                apollia_tools::WorkflowStepEnd {
                    status,
                    output,
                    error: error.clone(),
                },
            )
            .await
            .map_err(|e| e.to_string())?;
        tracing::event!(
            tracing::Level::INFO,
            run_id = %self.run_id,
            step = %step,
            status,
            "workflow.step.ended"
        );
        self.publish_ended(&step, error);
        Ok(())
    }

    async fn agent(&self, agent: String, message: String) -> Result<String, String> {
        let depth = self.nested_depth()?;
        let runner = self
            .agents
            .clone()
            .ok_or_else(|| "agents cannot run from a workflow in this runtime".to_string())?;
        let answer = runner
            .run(
                agent,
                message,
                Nesting {
                    parent: self.cancel.clone(),
                    budget: Arc::clone(&self.budget),
                    depth,
                },
            )
            .await?;
        self.record_output(answer.clone());
        Ok(answer)
    }

    async fn workflow(&self, workflow: String, inputs: Value) -> Result<Value, String> {
        self.nested_depth()?;
        let tx = self
            .engine
            .upgrade()
            .ok_or_else(|| "the workflow engine is not running".to_string())?;
        let engine = WorkflowHandle {
            tx,
            repository: Arc::clone(&self.repository),
        };
        let parent = ParentRun {
            run_id: Some(self.run_id.clone()),
            workflow_id: Some(self.workflow_id.clone()),
            depth: self.depth,
            cancel: self.cancel.clone(),
            budget: Arc::clone(&self.budget),
        };
        let output = engine.run_nested(&workflow, inputs, parent).await?;
        self.record_output(output.to_string());
        Ok(output)
    }
}

impl WorkflowServices for RunServices {
    fn step_started(&self, step: String) -> BoxFuture<'_, Result<(), String>> {
        Box::pin(self.open_step(step))
    }

    fn step_ended(&self, step: String, error: Option<String>) -> BoxFuture<'_, Result<(), String>> {
        Box::pin(self.close_step(step, error))
    }

    fn run_agent(&self, agent: String, message: String) -> BoxFuture<'_, Result<String, String>> {
        Box::pin(self.agent(agent, message))
    }

    fn run_workflow(
        &self,
        workflow: String,
        inputs: Value,
    ) -> BoxFuture<'_, Result<Value, String>> {
        Box::pin(self.workflow(workflow, inputs))
    }
}

/// Record the end of the run, publish the step it was stopped in, tell the
/// actor and the caller waiting on it.
pub(super) async fn finish(
    services: &Arc<RunServices>,
    outcome: Result<Value, String>,
    end: oneshot::Sender<RunEnd>,
) {
    let (status, output, error) = if services.cancel.is_cancelled() {
        ("canceled", None, Some("the run was stopped".to_string()))
    } else {
        match outcome {
            Ok(output) => ("completed", Some(output), None),
            Err(error) => ("failed", None, Some(error)),
        }
    };
    let open = services.state().open.take();
    if let Some(step) = &open {
        services.publish_ended(step, error.clone());
    }
    let row = match services
        .repository
        .end_workflow_run(&services.run_id, status, output, error)
        .await
    {
        Ok(row) => Some(row),
        Err(e) => {
            tracing::event!(
                tracing::Level::WARN,
                run_id = %services.run_id,
                error = %e,
                "workflow.run.record_end.failed"
            );
            None
        }
    };
    tracing::event!(
        tracing::Level::INFO,
        run_id = %services.run_id,
        workflow = %services.workflow_id,
        status,
        "workflow.run.ended"
    );
    if let Some(tx) = services.engine.upgrade() {
        let _ = tx
            .send(WorkflowCommand::Ended {
                run_id: services.run_id.clone(),
                row: row.clone(),
            })
            .await;
    }
    if let Some(row) = row {
        let _ = end.send(RunEnd { row });
    }
}
