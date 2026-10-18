//! Running an exposed agent or workflow for one call, and following it.
//!
//! An agent runs as a batch of one item, launched on the request source: the
//! batch engine starts it as an agent run outside any conversation, with the
//! budget, the accesses and the connections of the agent, and with nobody to
//! answer an approval. An access nobody granted makes the item wait for a
//! decision; the call then ends on that wait, and the item stays where the
//! user decides it. A workflow runs through the workflow engine.
//!
//! The run is followed on the bus: each tool the agent calls, and each step
//! of a workflow, becomes a step the caller can show; the tokens of every
//! model turn of the agent are added up. A caller that goes away before the
//! end stops the run ([`StopOnDrop`]); one that received the end leaves it.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use apollia_core::RuntimeEvent;
use apollia_tools::TaskRepository;
use serde_json::Value;
use tokio::sync::{mpsc, oneshot};

use super::agents::Engines;
use crate::batch::{
    awaited_access, BatchControl, BatchHandle, BatchLaunch, SourceDescriptor, REQUEST_SOURCE_KIND,
};
use crate::workflows::{RunEnd, WorkflowHandle};

/// Steps waiting to be read by the caller: a burst of tool calls fits.
const PROGRESS_CAPACITY: usize = 64;

/// How often the end of a batch item is read from `hitl.db`, should its event
/// have been missed by a lagging reader.
const SETTLE_PERIOD: Duration = Duration::from_secs(1);

/// The longest summary of a tool's arguments or result shown as a step.
const STEP_SUMMARY_CHARS: usize = 200;

/// Where a call to an agent came from, named in the source of its batch.
const REQUEST_LOCATION: &str = "compatible server";

/// The tokens a run spent, every model turn added up.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Usage {
    pub(crate) prompt: u64,
    pub(crate) completion: u64,
}

/// How a run ended, for the caller.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RunOutcome {
    /// The final answer.
    Answered { content: String, usage: Usage },
    /// The agent reached for an access nobody granted: the item waits for a
    /// decision in the list of pending decisions under `pending_id`.
    Pending {
        pending_id: String,
        batch_id: String,
        task_id: Option<String>,
        access: Option<String>,
    },
    /// The run reached for a tool of an MCP server the request brought, which
    /// nobody was there to approve. Nothing waits for a decision: the
    /// connection ended with the call.
    Unapproved { access: String, task_id: String },
    /// The run failed or was stopped.
    Failed { message: String },
}

/// What the follower of a run tells the caller.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Progress {
    /// A step of the work, one line.
    Step(String),
    /// The end; nothing follows.
    Ended(RunOutcome),
}

/// What stops a run.
enum Stop {
    Batch(BatchHandle, String),
    Workflow(WorkflowHandle, String),
}

/// Stops the run it guards when dropped before [`Self::release`]: the caller
/// went away before the end.
pub(crate) struct StopOnDrop {
    stop: Option<Stop>,
}

impl StopOnDrop {
    /// A guard that stops nothing: what follows the run stops it itself.
    pub(crate) fn nothing() -> Self {
        Self { stop: None }
    }

    /// The run ended and the caller received it: nothing is stopped.
    pub(crate) fn release(&mut self) {
        self.stop = None;
    }
}

impl Drop for StopOnDrop {
    fn drop(&mut self) {
        let Some(stop) = self.stop.take() else {
            return;
        };
        let Ok(runtime) = tokio::runtime::Handle::try_current() else {
            return;
        };
        runtime.spawn(async move {
            let (kind, id, result) = match stop {
                Stop::Batch(batches, batch_id) => {
                    let result = batches
                        .control(&batch_id, BatchControl::Stop)
                        .await
                        .map(drop)
                        .map_err(|e| e.to_string());
                    ("agent", batch_id, result)
                }
                Stop::Workflow(workflows, run_id) => {
                    let result = workflows
                        .cancel(&run_id)
                        .await
                        .map(drop)
                        .map_err(|e| e.to_string());
                    ("workflow", run_id, result)
                }
            };
            match result {
                Ok(()) => tracing::event!(
                    tracing::Level::INFO,
                    kind,
                    run = %id,
                    "compat_server.agent_call.client_gone"
                ),
                Err(error) => tracing::event!(
                    tracing::Level::WARN,
                    kind,
                    run = %id,
                    error = %error,
                    "compat_server.agent_call.stop_failed"
                ),
            }
        });
    }
}

/// A run that has started: its progress, and the guard that stops it.
pub(crate) struct StartedRun {
    /// The task or the workflow run, named in the answer's identifier.
    pub(crate) run_id: String,
    /// The task of an agent run, for the journal entry of the call.
    pub(crate) task_id: Option<String>,
    /// The run the audit journal keeps the entries of an agent under; for a
    /// workflow, its workflow run.
    pub(crate) journal_run: Option<String>,
    pub(crate) progress: mpsc::Receiver<Progress>,
    pub(crate) guard: StopOnDrop,
}

/// Why a run could not start.
#[derive(Debug, thiserror::Error)]
pub(crate) enum StartError {
    /// This runtime has no engine to run it.
    #[error("{0}")]
    Unavailable(String),
    /// The engine refused the launch.
    #[error("{0}")]
    Refused(String),
}

/// Runs `agent` on one item whose fields are `fields`, as a batch of one.
///
/// The launch runs on a task of its own: a caller that goes away while the
/// batch is being created still has it stopped once it exists.
///
/// # Errors
///
/// [`StartError`] when there is no batch engine or the launch is refused.
pub(crate) async fn start_agent(
    engines: &Engines<'_>,
    agent: &str,
    fields: BTreeMap<String, String>,
) -> Result<StartedRun, StartError> {
    let batches = engines.batches.cloned().ok_or_else(|| {
        StartError::Unavailable("this runtime has no batch engine to run an agent".to_owned())
    })?;
    // Subscribed before the launch, so no step of the run is missed.
    let events = apollia_core::subscribe_resilient(engines.bus, "compat_server.agent_call");
    let tasks = engines.tasks.cloned();
    let agent = agent.to_owned();
    let (reply, answer) = oneshot::channel();
    tokio::spawn(async move {
        let started = launch_agent(batches, tasks, events, agent, fields).await;
        // A caller gone before the answer drops the run here, which stops it.
        let _ = reply.send(started);
    });
    answer
        .await
        .map_err(|_| StartError::Unavailable("the launch of the agent was dropped".to_owned()))?
}

async fn launch_agent(
    batches: BatchHandle,
    tasks: Option<Arc<TaskRepository>>,
    events: apollia_core::events::ResilientReceiver,
    agent: String,
    fields: BTreeMap<String, String>,
) -> Result<StartedRun, StartError> {
    let batch = batches
        .launch(BatchLaunch {
            agent: agent.clone(),
            source: SourceDescriptor {
                kind: REQUEST_SOURCE_KIND.to_owned(),
                location: REQUEST_LOCATION.to_owned(),
                options: fields,
                write: None,
            },
            grants: Vec::new(),
            concurrency: None,
            max_failures: None,
            task: None,
        })
        .await
        .map_err(|e| StartError::Refused(e.to_string()))?;
    let batch_id = batch.batch_id;
    let guard = StopOnDrop {
        stop: Some(Stop::Batch(batches.clone(), batch_id.clone())),
    };
    // The one item has started by the time the launch answers.
    let task_id = match batches.detail(&batch_id).await {
        Ok(Some(detail)) => detail.items.into_iter().next().and_then(|i| i.task_id),
        _ => None,
    };
    let journal_run = match (&tasks, &task_id) {
        (Some(tasks), Some(task_id)) => tasks.get_run_id(task_id).await.ok().flatten(),
        _ => None,
    };
    tracing::event!(
        tracing::Level::INFO,
        agent_id = %agent,
        batch_id = %batch_id,
        task_id = task_id.as_deref().unwrap_or_default(),
        "compat_server.agent_call.started"
    );
    let (tx, progress) = mpsc::channel(PROGRESS_CAPACITY);
    let follower = AgentFollower {
        batches,
        batch_id: batch_id.clone(),
        task_id: task_id.clone(),
        journal_run: journal_run.clone(),
        tx,
    };
    tokio::spawn(follower.follow(events));
    Ok(StartedRun {
        run_id: task_id.clone().unwrap_or(batch_id),
        task_id,
        journal_run,
        progress,
        guard,
    })
}

/// Runs the workflow `workflow_id` on `inputs`.
///
/// # Errors
///
/// [`StartError`] when there is no workflow engine or the launch is refused.
pub(crate) async fn start_workflow(
    engines: &Engines<'_>,
    workflow_id: &str,
    inputs: Value,
) -> Result<StartedRun, StartError> {
    let workflows = engines
        .workflows
        .cloned()
        .ok_or_else(|| StartError::Unavailable("this runtime has no workflow engine".to_owned()))?;
    let events = apollia_core::subscribe_resilient(engines.bus, "compat_server.workflow_call");
    let launched = workflows
        .start(workflow_id, inputs, None)
        .await
        .map_err(|e| StartError::Refused(e.to_string()))?;
    let run_id = launched.row.run_id.clone();
    tracing::event!(
        tracing::Level::INFO,
        workflow_id = %workflow_id,
        run_id = %run_id,
        "compat_server.workflow_call.started"
    );
    let (tx, progress) = mpsc::channel(PROGRESS_CAPACITY);
    tokio::spawn(follow_workflow(events, run_id.clone(), launched.end, tx));
    Ok(StartedRun {
        run_id: run_id.clone(),
        task_id: None,
        journal_run: Some(run_id.clone()),
        progress,
        guard: StopOnDrop {
            stop: Some(Stop::Workflow(workflows, run_id)),
        },
    })
}

/// One line, at most [`STEP_SUMMARY_CHARS`] characters.
fn summary(text: &str) -> String {
    let line = text.split_whitespace().collect::<Vec<_>>().join(" ");
    match line.char_indices().nth(STEP_SUMMARY_CHARS) {
        Some((cut, _)) => format!("{}...", &line[..cut]),
        None => line,
    }
}

/// The step `event` shows, when it is a tool call of the run `task_id`.
pub(crate) fn agent_step(event: &RuntimeEvent, task_id: &str) -> Option<String> {
    match event {
        RuntimeEvent::ChatToolCallStarted {
            session_id,
            tool_name,
            input_preview,
            ..
        } if session_id == task_id => {
            Some(format!("Calling {tool_name}: {}\n", summary(input_preview)))
        }
        RuntimeEvent::ChatToolCallCompleted {
            session_id,
            tool_name,
            success,
            output_preview,
            ..
        } if session_id == task_id => {
            let result = summary(output_preview.as_deref().unwrap_or_default());
            Some(if *success {
                format!("{tool_name} answered: {result}\n")
            } else {
                format!("{tool_name} failed: {result}\n")
            })
        }
        _ => None,
    }
}

/// Follows the batch of one item an agent call runs as.
struct AgentFollower {
    batches: BatchHandle,
    batch_id: String,
    task_id: Option<String>,
    /// The run the model turns of the task report their tokens under.
    journal_run: Option<String>,
    tx: mpsc::Sender<Progress>,
}

impl AgentFollower {
    async fn follow(self, mut events: apollia_core::events::ResilientReceiver) {
        let run_id = self.journal_run.clone();
        let task_id = self.task_id.clone().unwrap_or_default();
        let mut usage = Usage::default();
        let mut settle = tokio::time::interval(SETTLE_PERIOD);
        let outcome = loop {
            tokio::select! {
                biased;
                event = events.recv() => {
                    let Some(event) = event else {
                        match self.settled(usage).await {
                            Some(outcome) => break outcome,
                            None => break RunOutcome::Failed {
                                message: "the runtime is stopping".to_owned(),
                            },
                        }
                    };
                    if let Some(step) = agent_step(&event, &task_id) {
                        if self.tx.send(Progress::Step(step)).await.is_err() {
                            return;
                        }
                    }
                    match event {
                        RuntimeEvent::LlmResponseCaptured {
                            run_id: turn_run,
                            prompt_tokens,
                            completion_tokens,
                            ..
                        } if run_id.as_deref() == Some(turn_run.as_str()) => {
                            usage.prompt += u64::from(prompt_tokens);
                            usage.completion += u64::from(completion_tokens);
                        }
                        RuntimeEvent::BatchItemAwaitingApproval {
                            batch_id, rank, access, ..
                        } if batch_id == self.batch_id => {
                            break RunOutcome::Pending {
                                pending_id: format!("batch:{batch_id}:{rank}"),
                                batch_id,
                                task_id: self.task_id.clone(),
                                access: Some(access),
                            };
                        }
                        RuntimeEvent::BatchEnded { batch_id, .. } if batch_id == self.batch_id => {
                            if let Some(outcome) = self.settled(usage).await {
                                break outcome;
                            }
                        }
                        _ => {}
                    }
                }
                _ = settle.tick() => {
                    if let Some(outcome) = self.settled(usage).await {
                        break outcome;
                    }
                }
                () = self.tx.closed() => return,
            }
        };
        tracing::event!(
            tracing::Level::INFO,
            batch_id = %self.batch_id,
            outcome = outcome_name(&outcome),
            "compat_server.agent_call.ended"
        );
        let _ = self.tx.send(Progress::Ended(outcome)).await;
    }

    /// The end of the item as recorded, `None` while it runs.
    async fn settled(&self, usage: Usage) -> Option<RunOutcome> {
        let detail = self.batches.detail(&self.batch_id).await.ok()??;
        let item = detail.items.into_iter().next()?;
        let output = item.output.unwrap_or_default();
        match item.status.as_str() {
            "completed" => Some(RunOutcome::Answered {
                content: output,
                usage,
            }),
            "awaiting" => Some(RunOutcome::Pending {
                pending_id: format!("batch:{}:{}", self.batch_id, item.rank),
                batch_id: self.batch_id.clone(),
                task_id: item.task_id,
                access: awaited_access(&output).map(str::to_owned),
            }),
            "failed" => Some(RunOutcome::Failed { message: output }),
            "canceled" => Some(RunOutcome::Failed {
                message: "the run was stopped before its end".to_owned(),
            }),
            _ => None,
        }
    }
}

fn outcome_name(outcome: &RunOutcome) -> &'static str {
    match outcome {
        RunOutcome::Answered { .. } => "answered",
        RunOutcome::Pending { .. } => "pending",
        RunOutcome::Unapproved { .. } => "unapproved",
        RunOutcome::Failed { .. } => "failed",
    }
}

/// The step `event` shows, when it is a step of the workflow run `run_id`.
fn workflow_step(event: &RuntimeEvent, run_id: &str) -> Option<String> {
    match event {
        RuntimeEvent::WorkflowStepStarted {
            run_id: run, step, ..
        } if run == run_id => Some(format!("Step {step} started\n")),
        RuntimeEvent::WorkflowStepCompleted {
            run_id: run, step, ..
        } if run == run_id => Some(format!("Step {step} done\n")),
        RuntimeEvent::WorkflowStepFailed {
            run_id: run,
            step,
            error,
            ..
        } if run == run_id => Some(format!("Step {step} failed: {}\n", summary(error))),
        _ => None,
    }
}

/// Follows a workflow run until `end` reports it.
async fn follow_workflow(
    mut events: apollia_core::events::ResilientReceiver,
    run_id: String,
    mut end: oneshot::Receiver<RunEnd>,
    tx: mpsc::Sender<Progress>,
) {
    let outcome = loop {
        tokio::select! {
            biased;
            Some(event) = events.recv() => {
                if let Some(step) = workflow_step(&event, &run_id) {
                    if tx.send(Progress::Step(step)).await.is_err() {
                        return;
                    }
                }
            }
            ended = &mut end => break match ended {
                Ok(RunEnd { row }) if row.status == "completed" => RunOutcome::Answered {
                    content: match row.output {
                        Some(Value::String(text)) => text,
                        Some(other) => other.to_string(),
                        None => String::new(),
                    },
                    usage: Usage::default(),
                },
                Ok(RunEnd { row }) => RunOutcome::Failed {
                    message: row
                        .error
                        .unwrap_or_else(|| format!("the workflow run ended {}", row.status)),
                },
                Err(_) => RunOutcome::Failed {
                    message: "the workflow run was dropped".to_owned(),
                },
            },
            () = tx.closed() => return,
        }
    };
    let _ = tx.send(Progress::Ended(outcome)).await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_a_step_summary_is_one_bounded_line() {
        // GIVEN a tool result over several lines and longer than the bound
        let long = format!("first\nsecond {}", "x".repeat(STEP_SUMMARY_CHARS));

        // WHEN it is summarised
        let line = summary(&long);

        // THEN it is one line, cut at the bound and marked as cut
        assert!(!line.contains('\n'));
        assert!(line.starts_with("first second "));
        assert!(line.ends_with("..."));
        assert_eq!(line.chars().count(), STEP_SUMMARY_CHARS + 3);
    }

    #[test]
    fn test_only_the_tool_calls_of_the_run_are_steps() {
        // GIVEN a tool call of the run, and one of another task
        let call = |session: &str| RuntimeEvent::ChatToolCallStarted {
            session_id: session.to_owned(),
            message_id: session.to_owned(),
            tool_name: "file_read".to_owned(),
            input_preview: "{\"path\": \"notes.md\"}".to_owned(),
            rationale: None,
        };

        // WHEN each is read for the run `t-1`
        // THEN only the first shows, naming the tool and its arguments
        assert_eq!(
            agent_step(&call("t-1"), "t-1").as_deref(),
            Some("Calling file_read: {\"path\": \"notes.md\"}\n")
        );
        assert!(agent_step(&call("t-2"), "t-1").is_none());
    }
}
