//! The workflow engine: a workflow of an installed package, run step by step.
//!
//! A workflow is named `<package>/<workflow>`. Its code is Python and runs in
//! the embedded interpreter, which lives in `apollia-aip`, a crate that
//! depends on this one: the engine reaches it through [`WorkflowHost`],
//! implemented there and handed over once the runtime is up, as the package
//! tools are. Until then a launch answers that no workflow can run.
//!
//! A launch checks the inputs against the schema the workflow declares and
//! records the run in `hitl.db` with one row per declared step, every step
//! `pending`. The workflow then reports each step it opens and closes through
//! [`WorkflowServices`]: each report is written to its step row and published
//! on the bus ([`RuntimeEvent::WorkflowStepStarted`],
//! [`RuntimeEvent::WorkflowStepCompleted`], [`RuntimeEvent::WorkflowStepFailed`]).
//!
//! A step may run an agent (`ctx.agents.run`) or another workflow
//! (`ctx.workflows.run`). Either call is nested in the run that makes it: it
//! stops when that run is stopped, it spends from the same budget, and the
//! nesting is bounded at [`MAX_DEPTH`] levels, the first run counting as one.
//! A run started on its own takes the budget of the `[workflows]`
//! configuration.
//!
//! Stopping a run cancels its token: the step in progress is interrupted at
//! its next await point, the run ends `canceled`, the step it was in is
//! `canceled` and the steps it never reached are `not_started`.
//!
//! [`RuntimeEvent::WorkflowStepStarted`]: apollia_core::RuntimeEvent::WorkflowStepStarted
//! [`RuntimeEvent::WorkflowStepCompleted`]: apollia_core::RuntimeEvent::WorkflowStepCompleted
//! [`RuntimeEvent::WorkflowStepFailed`]: apollia_core::RuntimeEvent::WorkflowStepFailed

mod actor;
mod agents;
pub mod fields;
pub mod inputs;
mod run;

#[cfg(test)]
mod inputs_tests;
#[cfg(test)]
mod tests;

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use apollia_core::StepBudgetConfig;
use apollia_tools::{TaskRepoError, TaskRepository, WorkflowRunRow, WorkflowStepRow};
use serde_json::Value;
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;

use crate::agents::task::Nesting;
use crate::eventbus::EventBusSender;

pub use agents::LaunchedAgentRunner;
pub use fields::{input_fields, InputField, InputKind};
pub use inputs::{InputError, InputErrorCode};

/// Levels a nesting of runs may reach: a workflow, a workflow it calls, and
/// one more. An agent run started by a step counts as a level.
pub const MAX_DEPTH: u32 = 3;

/// Capacity of the command channel of the workflow actor.
const COMMAND_CAPACITY: usize = 64;

/// A boxed future, the return of the object-safe traits of this module.
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// A workflow as its package declares it.
#[derive(Debug, Clone, PartialEq)]
pub struct WorkflowDescriptor {
    /// `<package>/<workflow>`.
    pub id: String,
    /// The version of the package that declares it.
    pub version: String,
    /// What the workflow does.
    pub description: String,
    /// The declared steps, in order.
    pub steps: Vec<String>,
    /// JSON Schema of the inputs; `None` accepts any object.
    pub input_schema: Option<Value>,
}

/// What a running workflow reaches the runtime through.
pub trait WorkflowServices: Send + Sync {
    /// The workflow opens its declared step `step`. An error refuses the
    /// step: the run was stopped, its budget is spent, or the step is not
    /// one it declared and has not run yet.
    fn step_started(&self, step: String) -> BoxFuture<'_, Result<(), String>>;

    /// The step `step` closes, with the error it raised when it did.
    fn step_ended(&self, step: String, error: Option<String>) -> BoxFuture<'_, Result<(), String>>;

    /// Run the agent `agent` on `message` and return its answer.
    fn run_agent(&self, agent: String, message: String) -> BoxFuture<'_, Result<String, String>>;

    /// Run the workflow `workflow`, `<package>/<workflow>` or a bare name of
    /// the caller's package, and return what it returns.
    fn run_workflow(&self, workflow: String, inputs: Value)
        -> BoxFuture<'_, Result<Value, String>>;
}

/// One run handed to the host.
pub struct WorkflowCall {
    /// The run, a key of `workflow_runs`.
    pub run_id: String,
    /// The workflow.
    pub workflow: WorkflowDescriptor,
    /// The inputs, as checked.
    pub inputs: Value,
    /// Cancelled when the run is stopped; the host interrupts the code.
    pub cancel: CancellationToken,
    /// What the code reports its steps and makes its nested calls through.
    pub services: Arc<dyn WorkflowServices>,
}

/// Where the workflows come from and where their code runs.
pub trait WorkflowHost: Send + Sync {
    /// The workflow `workflow_id`, `<package>/<workflow>`, when an installed
    /// package declares it.
    fn describe(&self, workflow_id: &str) -> Option<WorkflowDescriptor>;

    /// Every workflow the installed packages declare, by identifier.
    fn list(&self) -> Vec<WorkflowDescriptor>;

    /// Run `call` to its end and return what the workflow returned, or why
    /// it failed.
    fn run(&self, call: WorkflowCall) -> BoxFuture<'static, Result<Value, String>>;
}

/// Runs the agent of a step.
pub trait AgentRunner: Send + Sync {
    /// Run `agent` on `message`, stopped with and spending from `nesting`,
    /// and return its answer.
    fn run(
        &self,
        agent: String,
        message: String,
        nesting: Nesting,
    ) -> BoxFuture<'_, Result<String, String>>;
}

/// Why a run could not be launched, read or stopped.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum WorkflowError {
    /// No installed package declares the workflow.
    #[error("no installed workflow '{0}'")]
    NotFound(String),
    /// The identifier is not `<package>/<workflow>`.
    #[error("'{0}' is not a workflow identifier: expected <package>/<workflow>")]
    BadIdentifier(String),
    /// An input does not fit the schema.
    #[error("input '{}': {}", .0.field, .0.message)]
    InvalidInput(InputError),
    /// The call would nest deeper than [`MAX_DEPTH`].
    #[error("a run may nest {MAX_DEPTH} levels deep, this call would be level {0}")]
    TooDeep(u32),
    /// No run by that identifier.
    #[error("no workflow run '{0}'")]
    UnknownRun(String),
    /// The run already ended.
    #[error("workflow run '{0}' already ended")]
    AlreadyEnded(String),
    /// The Python host is not attached: no workflow can run yet.
    #[error("workflows cannot run in this runtime: the package host is not attached")]
    NoHost,
    /// `hitl.db` could not be written or read.
    #[error(transparent)]
    Store(#[from] TaskRepoError),
    /// The workflow actor is gone.
    #[error("the workflow engine is not running")]
    Stopped,
}

/// The full identifier of `workflow` called from a run of `caller`: a bare
/// name is read in the caller's package.
///
/// # Errors
///
/// [`WorkflowError::BadIdentifier`] for an empty part, more than one `/`, or
/// a bare name with no caller to take the package from.
pub fn qualify(workflow: &str, caller: Option<&str>) -> Result<String, WorkflowError> {
    let bad = || WorkflowError::BadIdentifier(workflow.to_string());
    match workflow.split_once('/') {
        Some((package, name)) => {
            if package.is_empty() || name.is_empty() || name.contains('/') {
                return Err(bad());
            }
            Ok(workflow.to_string())
        }
        None => {
            let package = caller
                .and_then(|c| c.split_once('/'))
                .map(|(package, _)| package)
                .ok_or_else(bad)?;
            if workflow.is_empty() {
                return Err(bad());
            }
            Ok(format!("{package}/{workflow}"))
        }
    }
}

/// A run with its steps, as read.
#[derive(Debug, Clone)]
pub struct WorkflowRunDetail {
    pub run: WorkflowRunRow,
    pub steps: Vec<WorkflowStepRow>,
}

/// How a run ended.
#[derive(Debug, Clone)]
pub(crate) struct RunEnd {
    pub row: WorkflowRunRow,
}

/// The run a nested call is made from: a workflow run, or an agent run that
/// was granted the workflow, which has neither a run of `workflow_runs` nor a
/// package to read a bare name in.
#[derive(Clone)]
pub(crate) struct ParentRun {
    pub run_id: Option<String>,
    pub workflow_id: Option<String>,
    pub depth: u32,
    pub cancel: CancellationToken,
    pub budget: Arc<apollia_oria::budget::StepBudget>,
}

/// A run that has started.
pub(crate) struct Launched {
    pub row: WorkflowRunRow,
    pub end: oneshot::Receiver<RunEnd>,
}

pub(crate) enum WorkflowCommand {
    SetHost(Arc<dyn WorkflowHost>),
    Launch {
        workflow: String,
        inputs: Value,
        parent: Option<ParentRun>,
        reply: oneshot::Sender<Result<Launched, WorkflowError>>,
    },
    Cancel {
        run_id: String,
        reply: oneshot::Sender<Result<WorkflowRunRow, WorkflowError>>,
    },
    Catalogue {
        reply: oneshot::Sender<Vec<WorkflowDescriptor>>,
    },
    Ended {
        run_id: String,
        row: Option<WorkflowRunRow>,
    },
    Describe {
        workflow: String,
        reply: oneshot::Sender<Option<WorkflowDescriptor>>,
    },
}

/// The handle of the workflow actor. Reads go to `hitl.db` directly; a launch
/// and a stop go through the actor, which owns the runs in progress.
#[derive(Clone)]
pub struct WorkflowHandle {
    tx: mpsc::Sender<WorkflowCommand>,
    repository: Arc<TaskRepository>,
}

impl WorkflowHandle {
    /// Starts the workflow actor. The runs `hitl.db` still holds as running
    /// were left by a process that stopped; they are failed first, since a
    /// workflow does not resume. `agents` runs the agent of a step; `None`
    /// refuses `ctx.agents.run`. The actor stops when every handle is dropped.
    pub fn spawn(
        repository: Arc<TaskRepository>,
        event_bus: EventBusSender,
        budget: StepBudgetConfig,
        agents: Option<Arc<dyn AgentRunner>>,
    ) -> Self {
        let (tx, rx) = mpsc::channel(COMMAND_CAPACITY);
        let actor = actor::WorkflowActor {
            rx,
            engine: tx.downgrade(),
            repository: repository.clone(),
            event_bus,
            budget,
            agents,
            host: None,
            running: std::collections::HashMap::new(),
            stopping: std::collections::HashMap::new(),
        };
        tokio::spawn(actor.run());
        Self { tx, repository }
    }

    /// Hands the engine the host the workflows run in.
    ///
    /// # Errors
    ///
    /// [`WorkflowError::Stopped`].
    pub async fn set_host(&self, host: Arc<dyn WorkflowHost>) -> Result<(), WorkflowError> {
        self.tx
            .send(WorkflowCommand::SetHost(host))
            .await
            .map_err(|_| WorkflowError::Stopped)
    }

    /// Launches `workflow` on `inputs`. Returns the run once it is recorded;
    /// it goes on in the background.
    ///
    /// # Errors
    ///
    /// [`WorkflowError::BadIdentifier`], [`WorkflowError::NotFound`],
    /// [`WorkflowError::InvalidInput`], [`WorkflowError::NoHost`],
    /// [`WorkflowError::Store`] or [`WorkflowError::Stopped`].
    pub async fn launch(
        &self,
        workflow: &str,
        inputs: Value,
    ) -> Result<WorkflowRunRow, WorkflowError> {
        Ok(self.start(workflow, inputs, None).await?.row)
    }

    /// Launches `workflow` on `inputs` and waits for its end.
    ///
    /// # Errors
    ///
    /// As [`Self::launch`].
    pub async fn run_to_end(
        &self,
        workflow: &str,
        inputs: Value,
    ) -> Result<WorkflowRunRow, WorkflowError> {
        let launched = self.start(workflow, inputs, None).await?;
        launched
            .end
            .await
            .map(|end| end.row)
            .map_err(|_| WorkflowError::Stopped)
    }

    pub(crate) async fn start(
        &self,
        workflow: &str,
        inputs: Value,
        parent: Option<ParentRun>,
    ) -> Result<Launched, WorkflowError> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(WorkflowCommand::Launch {
                workflow: workflow.to_string(),
                inputs,
                parent,
                reply,
            })
            .await
            .map_err(|_| WorkflowError::Stopped)?;
        rx.await.map_err(|_| WorkflowError::Stopped)?
    }

    /// The workflow `workflow`, `<package>/<workflow>`, when an installed
    /// package declares it; `None` too while no host is attached.
    pub async fn describe(&self, workflow: &str) -> Option<WorkflowDescriptor> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(WorkflowCommand::Describe {
                workflow: workflow.to_string(),
                reply,
            })
            .await
            .ok()?;
        rx.await.ok().flatten()
    }

    /// Runs `workflow` on `inputs` nested in `parent`, and returns what it
    /// returned, or why it did not complete.
    pub(crate) async fn run_nested(
        &self,
        workflow: &str,
        inputs: Value,
        parent: ParentRun,
    ) -> Result<Value, String> {
        let launched = self
            .start(workflow, inputs, Some(parent))
            .await
            .map_err(|e| e.to_string())?;
        let end = launched
            .end
            .await
            .map_err(|_| format!("the run of {workflow} was dropped"))?;
        match end.row.status.as_str() {
            "completed" => Ok(end.row.output.unwrap_or(Value::Null)),
            status => Err(format!(
                "{} {status}: {}",
                end.row.workflow_id,
                end.row.error.unwrap_or_default()
            )),
        }
    }

    /// Every workflow that can be launched, by identifier; none while the
    /// package host is not attached.
    ///
    /// # Errors
    ///
    /// [`WorkflowError::Stopped`].
    pub async fn catalogue(&self) -> Result<Vec<WorkflowDescriptor>, WorkflowError> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(WorkflowCommand::Catalogue { reply })
            .await
            .map_err(|_| WorkflowError::Stopped)?;
        rx.await.map_err(|_| WorkflowError::Stopped)
    }

    /// Stops a running run and returns it once it has ended.
    ///
    /// # Errors
    ///
    /// [`WorkflowError::UnknownRun`], [`WorkflowError::AlreadyEnded`],
    /// [`WorkflowError::Store`] or [`WorkflowError::Stopped`].
    pub async fn cancel(&self, run_id: &str) -> Result<WorkflowRunRow, WorkflowError> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(WorkflowCommand::Cancel {
                run_id: run_id.to_string(),
                reply,
            })
            .await
            .map_err(|_| WorkflowError::Stopped)?;
        rx.await.map_err(|_| WorkflowError::Stopped)?
    }

    /// One run with its steps, or `None`.
    ///
    /// # Errors
    ///
    /// [`WorkflowError::Store`].
    pub async fn detail(&self, run_id: &str) -> Result<Option<WorkflowRunDetail>, WorkflowError> {
        let Some(run) = self.repository.workflow_run(run_id).await? else {
            return Ok(None);
        };
        let steps = self.repository.workflow_steps(run_id).await?;
        Ok(Some(WorkflowRunDetail { run, steps }))
    }

    /// The runs of `workflow_id`, newest first, at most `limit`.
    ///
    /// # Errors
    ///
    /// [`WorkflowError::BadIdentifier`] or [`WorkflowError::Store`].
    pub async fn history(
        &self,
        workflow_id: &str,
        limit: u32,
    ) -> Result<Vec<WorkflowRunRow>, WorkflowError> {
        let workflow_id = qualify(workflow_id, None)?;
        Ok(self.repository.workflow_runs(&workflow_id, limit).await?)
    }
}
