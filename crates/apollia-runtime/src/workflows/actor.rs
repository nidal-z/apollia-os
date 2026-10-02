//! The workflow actor: it owns the runs in progress and their tokens.

use std::collections::HashMap;
use std::sync::Arc;

use apollia_core::StepBudgetConfig;
use apollia_oria::budget::StepBudget;
use apollia_tools::{NewWorkflowRun, TaskRepository, WorkflowRunRow};
use serde_json::Value;
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;

use super::run::{finish, RunServices};
use super::{
    inputs, qualify, AgentRunner, Launched, ParentRun, RunEnd, WorkflowCall, WorkflowCommand,
    WorkflowError, WorkflowHost, MAX_DEPTH,
};
use crate::eventbus::EventBusSender;

pub(super) struct WorkflowActor {
    pub(super) rx: mpsc::Receiver<WorkflowCommand>,
    /// Reaches the actor back from a run, for its end and its nested calls,
    /// without keeping the actor alive.
    pub(super) engine: mpsc::WeakSender<WorkflowCommand>,
    pub(super) repository: Arc<TaskRepository>,
    pub(super) event_bus: EventBusSender,
    /// The budget a run started on its own takes.
    pub(super) budget: StepBudgetConfig,
    pub(super) agents: Option<Arc<dyn AgentRunner>>,
    pub(super) host: Option<Arc<dyn WorkflowHost>>,
    /// The token of every run in progress.
    pub(super) running: HashMap<String, CancellationToken>,
    /// The callers waiting for a stopped run to end.
    pub(super) stopping:
        HashMap<String, Vec<oneshot::Sender<Result<WorkflowRunRow, WorkflowError>>>>,
}

impl WorkflowActor {
    pub(super) async fn run(mut self) {
        match self.repository.fail_interrupted_workflow_runs().await {
            Ok(0) => {}
            Ok(failed) => {
                tracing::event!(tracing::Level::INFO, failed, "workflow.runs.interrupted")
            }
            Err(error) => tracing::event!(
                tracing::Level::WARN,
                error = %error,
                "workflow.runs.interrupted.failed"
            ),
        }
        while let Some(command) = self.rx.recv().await {
            match command {
                WorkflowCommand::SetHost(host) => {
                    self.host = Some(host);
                    tracing::event!(tracing::Level::INFO, "workflow.host.attached");
                }
                WorkflowCommand::Launch {
                    workflow,
                    inputs,
                    parent,
                    reply,
                } => {
                    let launched = self.launch(&workflow, inputs, parent).await;
                    let _ = reply.send(launched);
                }
                WorkflowCommand::Cancel { run_id, reply } => self.cancel(run_id, reply).await,
                WorkflowCommand::Catalogue { reply } => {
                    let mut workflows = self.host.as_ref().map(|h| h.list()).unwrap_or_default();
                    workflows.sort_by(|a, b| a.id.cmp(&b.id));
                    let _ = reply.send(workflows);
                }
                WorkflowCommand::Ended { run_id, row } => {
                    self.running.remove(&run_id);
                    for waiter in self.stopping.remove(&run_id).unwrap_or_default() {
                        let answer = row
                            .clone()
                            .ok_or_else(|| WorkflowError::UnknownRun(run_id.clone()));
                        let _ = waiter.send(answer);
                    }
                }
            }
        }
    }

    async fn launch(
        &mut self,
        workflow: &str,
        inputs: Value,
        parent: Option<ParentRun>,
    ) -> Result<Launched, WorkflowError> {
        let workflow_id = qualify(workflow, parent.as_ref().map(|p| p.workflow_id.as_str()))?;
        let depth = parent.as_ref().map_or(1, |p| p.depth + 1);
        if depth > MAX_DEPTH {
            return Err(WorkflowError::TooDeep(depth));
        }
        let host = self.host.clone().ok_or(WorkflowError::NoHost)?;
        let descriptor = host
            .describe(&workflow_id)
            .ok_or_else(|| WorkflowError::NotFound(workflow_id.clone()))?;
        let inputs = inputs::validate(descriptor.input_schema.as_ref(), &inputs)
            .map_err(WorkflowError::InvalidInput)?;
        let (cancel, budget) = match &parent {
            Some(parent) => (parent.cancel.child_token(), Arc::clone(&parent.budget)),
            None => (
                CancellationToken::new(),
                Arc::new(StepBudget::new(&self.budget)),
            ),
        };
        let run_id = uuid::Uuid::new_v4().to_string();
        let row = self
            .repository
            .create_workflow_run(NewWorkflowRun {
                run_id: run_id.clone(),
                workflow_id: workflow_id.clone(),
                parent_run_id: parent.as_ref().map(|p| p.run_id.clone()),
                depth,
                inputs: inputs.clone(),
                steps: descriptor.steps.clone(),
            })
            .await?;
        self.running.insert(run_id.clone(), cancel.clone());
        tracing::event!(
            tracing::Level::INFO,
            run_id = %run_id,
            workflow = %workflow_id,
            depth,
            "workflow.run.started"
        );

        let services = Arc::new(RunServices {
            run_id: run_id.clone(),
            workflow_id,
            depth,
            cancel: cancel.clone(),
            budget,
            repository: Arc::clone(&self.repository),
            event_bus: self.event_bus.clone(),
            agents: self.agents.clone(),
            engine: self.engine.clone(),
            state: std::sync::Mutex::default(),
        });
        let call = WorkflowCall {
            run_id,
            workflow: descriptor,
            inputs,
            cancel,
            services: services.clone(),
        };
        let (end_tx, end) = oneshot::channel::<RunEnd>();
        tokio::spawn(async move {
            let outcome = host.run(call).await;
            finish(&services, outcome, end_tx).await;
        });
        Ok(Launched { row, end })
    }

    async fn cancel(
        &mut self,
        run_id: String,
        reply: oneshot::Sender<Result<WorkflowRunRow, WorkflowError>>,
    ) {
        if let Some(token) = self.running.get(&run_id) {
            token.cancel();
            tracing::event!(tracing::Level::INFO, run_id = %run_id, "workflow.run.stopping");
            self.stopping.entry(run_id).or_default().push(reply);
            return;
        }
        let answer = match self.repository.workflow_run(&run_id).await {
            Ok(Some(_)) => Err(WorkflowError::AlreadyEnded(run_id)),
            Ok(None) => Err(WorkflowError::UnknownRun(run_id)),
            Err(error) => Err(error.into()),
        };
        let _ = reply.send(answer);
    }
}
