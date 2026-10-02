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

mod actor;
mod runner;

#[cfg(test)]
mod tests;

use std::collections::BTreeMap;
use std::future::Future;
use std::sync::Arc;

use apollia_core::batch_source::{SourceError, SourceRegistry};
use apollia_tools::{BatchItemRow, BatchRow, TaskRepoError, TaskRepository};
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, oneshot};

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
    /// The accesses granted for the duration of the batch.
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
}

/// How the run of one item ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemStatus {
    Completed,
    Failed,
    Canceled,
}

impl ItemStatus {
    /// The status stored in `batch_items.status`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Canceled => "canceled",
        }
    }
}

/// The end of the run of one item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemEnd {
    pub status: ItemStatus,
    /// The answer, or the failure.
    pub output: String,
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
    /// Checks that `agent` can run a batch, before anything is recorded.
    fn check(&self, agent: &str) -> impl Future<Output = Result<(), String>> + Send;

    /// Starts `agent` on one item whose input is `input`.
    fn start(
        &self,
        agent: &str,
        input: &serde_json::Value,
    ) -> impl Future<Output = Result<StartedItem, String>> + Send;
}

/// A batch with its items, as read.
#[derive(Debug, Clone)]
pub struct BatchDetail {
    pub batch: BatchRow,
    pub items: Vec<BatchItemRow>,
}

enum BatchCommand {
    Launch {
        launch: BatchLaunch,
        reply: oneshot::Sender<Result<BatchRow, BatchError>>,
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
    ) -> Self {
        let (tx, rx) = mpsc::channel(COMMAND_CAPACITY);
        let actor = actor::BatchActor::new(rx, repository.clone(), sources, Arc::new(runner));
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

    /// The batches, newest first, at most `limit`.
    ///
    /// # Errors
    ///
    /// [`BatchError::Store`].
    pub async fn list(&self, limit: u32) -> Result<Vec<BatchRow>, BatchError> {
        Ok(self.repository.list_batches(limit).await?)
    }
}

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
