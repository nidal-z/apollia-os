//! The batch engine: one agent run on every item of a source.
//!
//! A batch is launched on an agent, a source descriptor (`{kind, location,
//! options}`, opened through the [`SourceRegistry`]), a concurrency and a
//! failure threshold. Its items are read once, at launch, and written to
//! `hitl.db` with the batch (`batches`, `batch_items`); each item then runs
//! as one agent run outside any conversation, recorded in `tasks` and joined
//! to its item by `batch_items.task_id`.
//!
//! The [`BatchActor`](actor) owns the progress of every running batch and is
//! reached through a bounded channel. At most `concurrency` items of a batch
//! run at once. When the failed items of a batch exceed its threshold, the
//! batch is interrupted: no further item starts, the items already running
//! end on their own, and the remaining ones stay pending. When the runtime
//! starts, the batches still running are resumed from their pending items;
//! an item left running by the process that stopped is run again.
//!
//! Defaults: a concurrency of [`DEFAULT_CONCURRENCY`] (at most
//! [`MAX_CONCURRENCY`]) and a threshold of [`DEFAULT_MAX_FAILURES`] failed
//! items.
//!
//! A running batch is paused (no further item starts, those running end on
//! their own, and a restart leaves it paused), resumed, or stopped: its
//! running items are cancelled through their task, the pending ones are
//! canceled without running. The items that failed, or that wait for an
//! access, are run again from their input as read at launch.
//!
//! An item runs unattended. Before the batch starts, the sensitive accesses of
//! its agent are listed ([`BatchHandle::accesses`]); those granted at launch
//! are recorded as consents scoped to the batch and withdrawn when it ends.
//! An item whose agent reaches for an access nobody granted stops and waits
//! (`awaiting`) instead of failing, the batch goes on with the others, and
//! [`RuntimeEvent::BatchItemAwaitingApproval`] asks for the decision. When a
//! batch ends, [`RuntimeEvent::BatchEnded`] carries its counters.
//!
//! [`RuntimeEvent::BatchItemAwaitingApproval`]: apollia_core::RuntimeEvent::BatchItemAwaitingApproval
//! [`RuntimeEvent::BatchEnded`]: apollia_core::RuntimeEvent::BatchEnded

mod access;
mod actor;
mod control;
mod export;
mod runner;

#[cfg(test)]
mod control_tests;
#[cfg(test)]
mod export_tests;
#[cfg(test)]
mod tests;

use std::collections::BTreeMap;
use std::future::Future;
use std::sync::Arc;

use apollia_core::batch_source::{SourceError, SourceRegistry};
use apollia_tools::{BatchItemRow, BatchRow, TaskRepoError, TaskRepository};
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, oneshot};

use crate::eventbus::EventBusSender;

pub use access::BatchAccess;
pub use export::{export, BatchExport, ExportError, ExportFormat, XLSX_CELL_MAX};
pub use runner::AgentItemRunner;

/// Items of one batch run at once when the launch names no concurrency.
pub const DEFAULT_CONCURRENCY: u32 = 1;
/// The highest concurrency a batch accepts.
pub const MAX_CONCURRENCY: u32 = 16;
/// Failed items a batch tolerates when the launch names no threshold; the
/// next failure interrupts it.
pub const DEFAULT_MAX_FAILURES: u32 = 10;

/// Capacity of the command channel of the batch actor.
const COMMAND_CAPACITY: usize = 64;
/// Capacity of the channel the ends of item runs come back on.
const ITEM_END_CAPACITY: usize = 256;

/// The source a batch reads its items from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SourceDescriptor {
    /// The kind of source, as registered (a spreadsheet, a folder, a mailbox).
    pub kind: String,
    /// What the source is opened on: a file path, a folder, a connection.
    pub location: String,
    /// Options the kind understands, such as a sheet or a header row.
    #[serde(default)]
    pub options: BTreeMap<String, String>,
}

/// What a batch is launched with.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct BatchLaunch {
    /// The agent every item is run by.
    pub agent: String,
    /// Where the items come from.
    pub source: SourceDescriptor,
    /// The sensitive accesses granted for the duration of the batch, by
    /// their id as `GET /api/v1/batches/accesses` lists them.
    #[serde(default)]
    pub grants: Vec<String>,
    /// How many items run at once, 1 to 16; 1 when absent.
    #[serde(default)]
    pub concurrency: Option<u32>,
    /// How many failed items the batch tolerates; 10 when absent.
    #[serde(default)]
    pub max_failures: Option<u32>,
}

/// Why a batch could not be launched or read.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum BatchError {
    /// The launch is malformed.
    #[error("{0}")]
    Invalid(String),
    /// The source could not be opened or read; an unknown kind is one.
    #[error(transparent)]
    Source(#[from] SourceError),
    /// The agent cannot run a batch.
    #[error("{0}")]
    Agent(String),
    /// `hitl.db` could not be written or read.
    #[error(transparent)]
    Store(#[from] TaskRepoError),
    /// The batch actor is gone.
    #[error("the batch engine is not running")]
    Stopped,
    /// No batch has this id.
    #[error("no batch '{0}'")]
    NotFound(String),
    /// The batch is not in a state the command applies to.
    #[error("{0}")]
    State(String),
}

/// What the failed items of a batch are run again with.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct BatchRetry {
    /// Accesses granted for the rest of the batch, on top of those granted
    /// at launch: an item that waited for one of them runs again with it.
    #[serde(default)]
    pub grants: Vec<String>,
}

/// How the run of one item ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemStatus {
    Completed,
    Failed,
    Canceled,
    /// Stopped on an access nobody granted; waits for a decision.
    Awaiting,
}

impl ItemStatus {
    /// The status stored in `batch_items.status`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Canceled => "canceled",
            Self::Awaiting => "awaiting",
        }
    }
}

/// The end of the run of one item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemEnd {
    pub status: ItemStatus,
    /// The answer, or the failure.
    pub output: String,
    /// For an item that waits: the access its agent reached for.
    pub awaiting: Option<String>,
}

/// An item run that has started.
#[derive(Debug)]
pub struct StartedItem {
    /// The task that runs it, a key of `tasks`.
    pub task_id: String,
    /// Where its end is reported.
    pub end: oneshot::Receiver<ItemEnd>,
}

/// Runs the items of a batch: the agent run outside any conversation in
/// production, a scripted runner in tests.
pub trait ItemRunner: Send + Sync + 'static {
    /// Checks that `agent` can run a batch, before anything is recorded, and
    /// returns its sensitive accesses.
    fn check(&self, agent: &str) -> impl Future<Output = Result<Vec<BatchAccess>, String>> + Send;

    /// Records `grants` as consents of `agent` for the duration of the batch
    /// `batch_id`.
    fn grant(
        &self,
        batch_id: &str,
        agent: &str,
        grants: &[BatchAccess],
    ) -> impl Future<Output = Result<(), String>> + Send;

    /// Withdraws every consent given for the batch `batch_id`.
    fn release(&self, batch_id: &str) -> impl Future<Output = Result<(), String>> + Send;

    /// Starts `agent` on one item of the batch `batch_id` whose input is
    /// `input`, with the accesses granted to it now.
    fn start(
        &self,
        batch_id: &str,
        agent: &str,
        input: &serde_json::Value,
    ) -> impl Future<Output = Result<StartedItem, String>> + Send;

    /// Cancels the run of the task `task_id`; its end is reported as usual.
    fn cancel(&self, task_id: &str) -> impl Future<Output = ()> + Send;
}

/// A batch with its items, as read.
#[derive(Debug, Clone)]
pub struct BatchDetail {
    pub batch: BatchRow,
    pub items: Vec<BatchItemRow>,
}

/// A change asked of one batch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BatchControl {
    /// No further item starts; those running end on their own.
    Pause,
    /// A paused batch starts its items again.
    Resume,
    /// Running items are cancelled, pending ones never run, the batch ends.
    Stop,
    /// The failed and waiting items run again, with more grants if given.
    Retry(BatchRetry),
    /// The item at `rank`, which waits for an access, is refused: it fails
    /// with the refusal and does not run again unless the batch is retried.
    Refuse {
        /// Position of the item in its source.
        rank: u64,
        /// Why, when the user said.
        reason: Option<String>,
    },
}

impl BatchControl {
    /// The verb, as the routes and the traces name it.
    pub fn verb(&self) -> &'static str {
        match self {
            Self::Pause => "pause",
            Self::Resume => "resume",
            Self::Stop => "stop",
            Self::Retry(_) => "retry",
            Self::Refuse { .. } => "refuse",
        }
    }
}

enum BatchCommand {
    Launch {
        launch: BatchLaunch,
        reply: oneshot::Sender<Result<BatchRow, BatchError>>,
    },
    Control {
        batch_id: String,
        control: BatchControl,
        reply: oneshot::Sender<Result<BatchRow, BatchError>>,
    },
    Accesses {
        agent: String,
        reply: oneshot::Sender<Result<Vec<BatchAccess>, BatchError>>,
    },
}

/// The handle of the batch actor. Reads go to `hitl.db` directly; a launch
/// goes through the actor, which owns the progress of every batch.
#[derive(Clone)]
pub struct BatchHandle {
    tx: mpsc::Sender<BatchCommand>,
    repository: Arc<TaskRepository>,
}

impl BatchHandle {
    /// Starts the batch actor, which first resumes the batches `hitl.db`
    /// holds as running. The actor stops when every handle is dropped.
    pub fn spawn<R: ItemRunner>(
        repository: Arc<TaskRepository>,
        sources: Arc<SourceRegistry>,
        runner: R,
        event_bus: EventBusSender,
    ) -> Self {
        let (tx, rx) = mpsc::channel(COMMAND_CAPACITY);
        let actor =
            actor::BatchActor::new(rx, repository.clone(), sources, Arc::new(runner), event_bus);
        tokio::spawn(actor.run());
        Self { tx, repository }
    }

    /// Launches a batch. Returns it once its items are recorded; they run in
    /// the background.
    ///
    /// # Errors
    ///
    /// [`BatchError::Invalid`] for a concurrency out of range,
    /// [`BatchError::Agent`] for an agent that cannot run a batch,
    /// [`BatchError::Source`] for a source that cannot be opened or read,
    /// [`BatchError::Store`] or [`BatchError::Stopped`] otherwise.
    pub async fn launch(&self, launch: BatchLaunch) -> Result<BatchRow, BatchError> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(BatchCommand::Launch { launch, reply })
            .await
            .map_err(|_| BatchError::Stopped)?;
        rx.await.map_err(|_| BatchError::Stopped)?
    }

    /// Pauses, resumes, stops or retries a batch. Returns it as changed.
    ///
    /// # Errors
    ///
    /// [`BatchError::NotFound`] for an unknown batch, [`BatchError::State`]
    /// when the batch is not in a state the command applies to,
    /// [`BatchError::Invalid`] for a grant the agent cannot take,
    /// [`BatchError::Store`] or [`BatchError::Stopped`] otherwise.
    pub async fn control(
        &self,
        batch_id: &str,
        control: BatchControl,
    ) -> Result<BatchRow, BatchError> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(BatchCommand::Control {
                batch_id: batch_id.to_string(),
                control,
                reply,
            })
            .await
            .map_err(|_| BatchError::Stopped)?;
        rx.await.map_err(|_| BatchError::Stopped)?
    }

    /// The sensitive accesses of `agent`: those a launch may grant for the
    /// duration of a batch.
    ///
    /// # Errors
    ///
    /// [`BatchError::Agent`] for an agent that cannot run a batch, or
    /// [`BatchError::Stopped`].
    pub async fn accesses(&self, agent: &str) -> Result<Vec<BatchAccess>, BatchError> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(BatchCommand::Accesses {
                agent: agent.to_string(),
                reply,
            })
            .await
            .map_err(|_| BatchError::Stopped)?;
        rx.await.map_err(|_| BatchError::Stopped)?
    }

    /// One batch with its items, or `None`.
    ///
    /// # Errors
    ///
    /// [`BatchError::Store`].
    pub async fn detail(&self, batch_id: &str) -> Result<Option<BatchDetail>, BatchError> {
        let Some(batch) = self.repository.batch(batch_id).await? else {
            return Ok(None);
        };
        let items = self.repository.batch_items(batch_id).await?;
        Ok(Some(BatchDetail { batch, items }))
    }

    /// Every item that waits for a decision, with its batch, oldest batch
    /// first.
    ///
    /// # Errors
    ///
    /// [`BatchError::Store`].
    pub async fn awaiting_items(&self) -> Result<Vec<(BatchRow, BatchItemRow)>, BatchError> {
        let mut batches = self.repository.list_batches(u32::MAX).await?;
        batches.retain(|b| b.awaiting > 0);
        batches.reverse();
        let mut waiting = Vec::new();
        for batch in batches {
            for item in self.repository.batch_items(&batch.batch_id).await? {
                if item.status == ItemStatus::Awaiting.as_str() {
                    waiting.push((batch.clone(), item));
                }
            }
        }
        Ok(waiting)
    }

    /// The batches, newest first, at most `limit`.
    ///
    /// # Errors
    ///
    /// [`BatchError::Store`].
    pub async fn list(&self, limit: u32) -> Result<Vec<BatchRow>, BatchError> {
        Ok(self.repository.list_batches(limit).await?)
    }
}

/// The output an item that waits is recorded with, naming the access its agent
/// reached for. [`awaited_access`] reads the access back.
pub fn awaiting_output(access: &str) -> String {
    format!("{AWAITING_PREFIX}{access}{AWAITING_SUFFIX}")
}

/// The access an item that waits reached for, read from its recorded output,
/// or `None` for an output [`awaiting_output`] did not write.
pub fn awaited_access(output: &str) -> Option<&str> {
    output
        .strip_prefix(AWAITING_PREFIX)?
        .strip_suffix(AWAITING_SUFFIX)
        .filter(|access| !access.is_empty())
}

const AWAITING_PREFIX: &str = "Waiting for a decision: the agent asked to use '";
const AWAITING_SUFFIX: &str = "', which is not granted";

/// The message an agent reads for one item: each field on its own line.
pub fn item_message(input: &serde_json::Value) -> String {
    match input.as_object() {
        Some(fields) => fields
            .iter()
            .map(|(name, value)| match value.as_str() {
                Some(text) => format!("{name}: {text}"),
                None => format!("{name}: {value}"),
            })
            .collect::<Vec<_>>()
            .join("\n"),
        None => input.to_string(),
    }
}
