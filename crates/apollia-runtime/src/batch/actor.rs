//! The actor that owns the progress of every running batch.

use std::collections::HashMap;
use std::sync::Arc;

use apollia_core::batch_source::SourceRegistry;
use apollia_core::{AgentId, RuntimeEvent};
use apollia_tools::{BatchRow, NewBatch, TaskRepository};
use tokio::sync::mpsc;
use tracing::Level;

use super::access::resolve_grants;
use super::write_back::{source_spec, write_results, WriteDestination, SOURCE_WRITE_ACCESS};
use super::{
    BatchCommand, BatchError, BatchLaunch, ItemEnd, ItemRunner, ItemStatus, SourceDescriptor,
    DEFAULT_CONCURRENCY, DEFAULT_MAX_FAILURES, ITEM_END_CAPACITY, MAX_CONCURRENCY,
};
use crate::eventbus::EventBusSender;

/// The end of one item, as it comes back to the actor.
struct ItemEnded {
    batch_id: String,
    rank: u64,
    end: ItemEnd,
}

/// A batch whose items are still running or pending.
pub(super) struct Active {
    pub(super) agent: String,
    pub(super) concurrency: u32,
    pub(super) max_failures: u32,
    pub(super) in_flight: u32,
    /// Interrupted or stopped: no item starts again, and the batch ends once
    /// those running have ended, in the state already recorded.
    pub(super) interrupted: bool,
    /// Paused: no item starts, and the batch does not end.
    pub(super) paused: bool,
    /// The source the answers are written back into when the batch ends,
    /// when the launch asked for it.
    pub(super) write_back: Option<SourceDescriptor>,
    /// Whether the launch granted writing into the original file.
    pub(super) original_granted: bool,
}

pub(super) struct BatchActor<R: ItemRunner> {
    commands: mpsc::Receiver<BatchCommand>,
    ends_tx: mpsc::Sender<ItemEnded>,
    ends: mpsc::Receiver<ItemEnded>,
    pub(super) repository: Arc<TaskRepository>,
    sources: Arc<SourceRegistry>,
    pub(super) runner: Arc<R>,
    pub(super) active: HashMap<String, Active>,
    /// The access each item answered "once" may call one time on its next
    /// run, by batch and rank; spent when the item starts.
    pub(super) once: HashMap<(String, u64), String>,
    event_bus: EventBusSender,
}

impl<R: ItemRunner> BatchActor<R> {
    pub(super) fn new(
        commands: mpsc::Receiver<BatchCommand>,
        repository: Arc<TaskRepository>,
        sources: Arc<SourceRegistry>,
        runner: Arc<R>,
        event_bus: EventBusSender,
    ) -> Self {
        let (ends_tx, ends) = mpsc::channel(ITEM_END_CAPACITY);
        Self {
            commands,
            ends_tx,
            ends,
            repository,
            sources,
            runner,
            active: HashMap::new(),
            once: HashMap::new(),
            event_bus,
        }
    }

    pub(super) async fn run(mut self) {
        self.resume().await;
        loop {
            tokio::select! {
                command = self.commands.recv() => match command {
                    Some(BatchCommand::Launch { launch, reply }) => {
                        let _ = reply.send(self.launch(launch).await);
                    }
                    Some(BatchCommand::Control { batch_id, control, reply }) => {
                        let _ = reply.send(self.control(&batch_id, control).await);
                    }
                    Some(BatchCommand::Accesses { agent, reply }) => {
                        let _ = reply.send(
                            self.runner.check(&agent).await.map_err(BatchError::Agent),
                        );
                    }
                    None => break,
                },
                Some(ended) = self.ends.recv() => self.item_ended(ended).await,
            }
        }
        tracing::event!(Level::INFO, "batch.actor.stopped");
    }

    /// Takes back the batches `hitl.db` holds as running.
    async fn resume(&mut self) {
        let running = match self.repository.running_batches().await {
            Ok(running) => running,
            Err(e) => {
                tracing::event!(Level::WARN, error = %e, "batch.resume.failed");
                return;
            }
        };
        for batch in running {
            match self.repository.requeue_running_items(&batch.batch_id).await {
                Ok(requeued) => tracing::event!(
                    Level::INFO,
                    batch_id = %batch.batch_id,
                    requeued,
                    "batch.resumed"
                ),
                Err(e) => {
                    tracing::event!(
                        Level::WARN,
                        batch_id = %batch.batch_id,
                        error = %e,
                        "batch.resume.failed"
                    );
                    continue;
                }
            }
            let id = batch.batch_id.clone();
            self.activate(&batch);
            if batch.failed > u64::from(batch.max_failures) {
                self.interrupt(&id, batch.failed).await;
            }
            self.advance(&id).await;
        }
    }

    pub(super) fn activate(&mut self, batch: &BatchRow) {
        self.active.insert(
            batch.batch_id.clone(),
            Active {
                agent: batch.agent.clone(),
                concurrency: batch.concurrency.max(1),
                max_failures: batch.max_failures,
                in_flight: 0,
                interrupted: false,
                paused: batch.state == "paused",
                write_back: serde_json::from_value::<SourceDescriptor>(batch.source.clone())
                    .ok()
                    .filter(|source| source.write.is_some()),
                original_granted: batch.grants.iter().any(|g| g == SOURCE_WRITE_ACCESS),
            },
        );
    }

    async fn launch(&mut self, launch: BatchLaunch) -> Result<BatchRow, BatchError> {
        let concurrency = launch.concurrency.unwrap_or(DEFAULT_CONCURRENCY);
        if !(1..=MAX_CONCURRENCY).contains(&concurrency) {
            return Err(BatchError::Invalid(format!(
                "concurrency must be between 1 and {MAX_CONCURRENCY}, got {concurrency}"
            )));
        }
        let max_failures = launch.max_failures.unwrap_or(DEFAULT_MAX_FAILURES);
        let sensitive = self
            .runner
            .check(&launch.agent)
            .await
            .map_err(BatchError::Agent)?;
        let writes_original = self.check_write(&launch).await?;
        let agent_grants: Vec<String> = launch
            .grants
            .iter()
            .filter(|g| g.as_str() != SOURCE_WRITE_ACCESS)
            .cloned()
            .collect();
        let grants = resolve_grants(&sensitive, &agent_grants)?;
        let items = self.read_items(&launch).await?;
        let source =
            serde_json::to_value(&launch.source).map_err(|e| BatchError::Invalid(e.to_string()))?;
        let batch_id = uuid::Uuid::new_v4().to_string();
        if writes_original {
            self.runner
                .record_original_write(&batch_id, &launch.agent, &launch.source.location)
                .await
                .map_err(BatchError::Invalid)?;
        }
        self.runner
            .grant(&batch_id, &launch.agent, &grants)
            .await
            .map_err(BatchError::Invalid)?;
        let mut granted: Vec<String> = grants.into_iter().map(|g| g.access_id).collect();
        if writes_original {
            granted.push(SOURCE_WRITE_ACCESS.to_owned());
        }
        let created = self
            .repository
            .create_batch(NewBatch {
                batch_id: batch_id.clone(),
                agent: launch.agent,
                source,
                grants: granted,
                concurrency,
                max_failures,
                items,
            })
            .await;
        let batch = match created {
            Ok(batch) => batch,
            Err(e) => {
                let _ = self.runner.release(&batch_id).await;
                return Err(e.into());
            }
        };
        tracing::event!(
            Level::INFO,
            batch_id = %batch.batch_id,
            agent = %batch.agent,
            total = batch.total,
            concurrency,
            max_failures,
            "batch.launched"
        );
        self.activate(&batch);
        self.advance(&batch.batch_id).await;
        Ok(batch)
    }

    /// Checks the write option of a launch, and asks governance when it
    /// writes into the original file. Returns whether it does.
    async fn check_write(&self, launch: &BatchLaunch) -> Result<bool, BatchError> {
        let source_granted = launch.grants.iter().any(|g| g == SOURCE_WRITE_ACCESS);
        let writes_original = match &launch.source.write {
            Some(write) => {
                write.check()?;
                write.destination == WriteDestination::Original
            }
            None => false,
        };
        if source_granted && !writes_original {
            return Err(BatchError::Invalid(format!(
                "'{SOURCE_WRITE_ACCESS}' is granted only when the results go into the original file"
            )));
        }
        if !writes_original {
            return Ok(false);
        }
        if !source_granted {
            return Err(BatchError::Invalid(format!(
                "writing the results into the original file is an access to grant at launch: \
                 add '{SOURCE_WRITE_ACCESS}' to the grants, or write them into a copy"
            )));
        }
        self.runner
            .allow_original_write(&launch.agent, &launch.source.location)
            .await
            .map_err(BatchError::Invalid)?;
        Ok(true)
    }

    /// Opens the source and reads every item, off the actor's loop. A launch
    /// that writes back into the source is refused when it cannot write.
    async fn read_items(
        &self,
        launch: &BatchLaunch,
    ) -> Result<Vec<(String, serde_json::Value)>, BatchError> {
        let sources = self.sources.clone();
        let kind = launch.source.kind.clone();
        let spec = source_spec(&launch.source);
        let writes = launch.source.write.is_some();
        tokio::task::spawn_blocking(move || {
            let mut source = sources.open(&kind, &spec)?;
            if writes && source.writer().is_none() {
                return Err(BatchError::Invalid(format!(
                    "a source of kind '{kind}' cannot be written: the results can only be exported"
                )));
            }
            let items = source.items()?;
            items
                .items
                .map(|item| {
                    let item = item?;
                    let fields: serde_json::Map<String, serde_json::Value> = item
                        .fields
                        .into_iter()
                        .map(|(k, v)| (k, serde_json::Value::String(v)))
                        .collect();
                    Ok((item.origin.to_string(), serde_json::Value::Object(fields)))
                })
                .collect::<Result<Vec<_>, BatchError>>()
        })
        .await
        .map_err(|e| BatchError::Invalid(format!("the source reader stopped: {e}")))?
    }

    /// Starts pending items while the batch has room, and ends it when
    /// nothing is left to run.
    pub(super) async fn advance(&mut self, batch_id: &str) {
        loop {
            let Some(active) = self.active.get(batch_id) else {
                return;
            };
            if active.paused {
                return;
            }
            if active.interrupted {
                break;
            }
            let room = active.concurrency.saturating_sub(active.in_flight);
            if room == 0 {
                return;
            }
            let agent = active.agent.clone();
            let pending = match self.repository.pending_batch_items(batch_id, room).await {
                Ok(pending) => pending,
                Err(e) => {
                    tracing::event!(Level::WARN, batch_id, error = %e, "batch.read.failed");
                    return;
                }
            };
            if pending.is_empty() {
                break;
            }
            for item in pending {
                self.start_item(batch_id, &agent, item.rank, &item.input)
                    .await;
                if self.active.get(batch_id).is_none_or(|a| a.interrupted) {
                    break;
                }
            }
        }
        self.end_if_drained(batch_id).await;
    }

    async fn start_item(
        &mut self,
        batch_id: &str,
        agent: &str,
        rank: u64,
        input: &serde_json::Value,
    ) {
        let once = self.once.remove(&(batch_id.to_string(), rank));
        match self
            .runner
            .start(batch_id, agent, input, once.as_deref())
            .await
        {
            Ok(started) => {
                if let Err(e) = self
                    .repository
                    .start_batch_item(batch_id, rank, &started.task_id)
                    .await
                {
                    tracing::event!(Level::WARN, batch_id, rank, error = %e, "batch.record.failed");
                }
                if let Some(active) = self.active.get_mut(batch_id) {
                    active.in_flight += 1;
                }
                let ends = self.ends_tx.clone();
                let batch_id = batch_id.to_string();
                tokio::spawn(async move {
                    let end = started.end.await.unwrap_or_else(|_| ItemEnd {
                        status: ItemStatus::Failed,
                        output: "the run was dropped".into(),
                        awaiting: None,
                    });
                    let _ = ends
                        .send(ItemEnded {
                            batch_id,
                            rank,
                            end,
                        })
                        .await;
                });
            }
            Err(reason) => {
                tracing::event!(Level::WARN, batch_id, rank, reason = %reason, "batch.item.start_failed");
                let end = ItemEnd {
                    status: ItemStatus::Failed,
                    output: reason,
                    awaiting: None,
                };
                self.record_end(batch_id, rank, &end).await;
            }
        }
    }

    async fn item_ended(&mut self, ended: ItemEnded) {
        if let Some(active) = self.active.get_mut(&ended.batch_id) {
            active.in_flight = active.in_flight.saturating_sub(1);
        }
        self.record_end(&ended.batch_id, ended.rank, &ended.end)
            .await;
        self.advance(&ended.batch_id).await;
    }

    /// Records the end of an item and interrupts its batch past the threshold.
    async fn record_end(&mut self, batch_id: &str, rank: u64, end: &ItemEnd) {
        tracing::event!(
            Level::INFO,
            batch_id,
            rank,
            status = end.status.as_str(),
            "batch.item.ended"
        );
        // An item that waits is recorded with the access it reached for, in
        // the one form the list of decisions reads back.
        let output = match &end.awaiting {
            Some(access) if end.status == ItemStatus::Awaiting => super::awaiting_output(access),
            _ => end.output.clone(),
        };
        let counted = self
            .repository
            .finish_batch_item(batch_id, rank, end.status.as_str(), &output)
            .await;
        // The decision is asked for once the item is recorded as waiting, not
        // before: whoever hears the event may read the list of decisions at
        // once, and has to find the item there.
        if let (Some(access), Some(active)) = (&end.awaiting, self.active.get(batch_id)) {
            let _ = self
                .event_bus
                .send(RuntimeEvent::BatchItemAwaitingApproval {
                    batch_id: batch_id.to_string(),
                    rank,
                    agent_id: AgentId::from(active.agent.as_str()),
                    access: access.clone(),
                });
        }
        match counted {
            Ok(batch) if batch.failed > u64::from(batch.max_failures) => {
                self.interrupt(batch_id, batch.failed).await;
            }
            Ok(_) => {}
            Err(e) => {
                tracing::event!(Level::WARN, batch_id, rank, error = %e, "batch.record.failed");
            }
        }
    }

    /// Stops starting items of a batch. It is ended `interrupted` at once, so
    /// a restart does not resume it; its running items still report.
    async fn interrupt(&mut self, batch_id: &str, failed: u64) {
        let Some(active) = self.active.get_mut(batch_id) else {
            return;
        };
        if active.interrupted {
            return;
        }
        active.interrupted = true;
        let max_failures = active.max_failures;
        tracing::event!(
            Level::WARN,
            batch_id,
            failed,
            max_failures,
            "batch.interrupted"
        );
        if let Err(e) = self.repository.end_batch(batch_id, "interrupted").await {
            tracing::event!(Level::WARN, batch_id, error = %e, "batch.record.failed");
        }
    }

    /// Ends a batch once none of its items runs: `completed` unless it was
    /// interrupted or stopped, which is already recorded. Its consents are
    /// withdrawn and its end is published with its counters.
    pub(super) async fn end_if_drained(&mut self, batch_id: &str) {
        let Some(active) = self.active.get(batch_id) else {
            return;
        };
        if active.in_flight > 0 || active.paused {
            return;
        }
        if !active.interrupted {
            if let Err(e) = self.repository.end_batch(batch_id, "completed").await {
                tracing::event!(Level::WARN, batch_id, error = %e, "batch.record.failed");
            }
            tracing::event!(Level::INFO, batch_id, "batch.completed");
        }
        let agent = active.agent.clone();
        let write_back = active.write_back.clone();
        let original_granted = active.original_granted;
        self.active.remove(batch_id);
        if let Err(reason) = self.runner.release(batch_id).await {
            tracing::event!(Level::WARN, batch_id, reason = %reason, "batch.release.failed");
        }
        let repository = self.repository.clone();
        let event_bus = self.event_bus.clone();
        let batch_id = batch_id.to_string();
        match write_back {
            // The answers are written off the actor's loop; the end is
            // announced once they are in the file.
            Some(descriptor) => {
                let sources = self.sources.clone();
                tokio::spawn(async move {
                    write_answers(
                        &repository,
                        sources,
                        &batch_id,
                        descriptor,
                        original_granted,
                    )
                    .await;
                    publish_end(&repository, &event_bus, &batch_id, &agent).await;
                });
            }
            None => publish_end(&repository, &event_bus, &batch_id, &agent).await,
        }
    }
}

/// Writes the answers of the completed items of `batch_id` back into its
/// source, and traces what was written.
async fn write_answers(
    repository: &TaskRepository,
    sources: Arc<SourceRegistry>,
    batch_id: &str,
    descriptor: SourceDescriptor,
    original_granted: bool,
) {
    let items = match repository.batch_items(batch_id).await {
        Ok(items) => items,
        Err(e) => {
            tracing::event!(Level::WARN, batch_id, error = %e, "batch.read.failed");
            return;
        }
    };
    let written = tokio::task::spawn_blocking(move || {
        write_results(&sources, &descriptor, original_granted, &items)
    })
    .await;
    match written {
        Ok(Ok(Some(written))) => tracing::event!(
            Level::INFO,
            batch_id,
            path = %written.backup.display(),
            items = written.items,
            refused = written.refused,
            "batch.results.written"
        ),
        Ok(Ok(None)) => tracing::event!(Level::INFO, batch_id, "batch.results.nothing_to_write"),
        Ok(Err(e)) => {
            tracing::event!(Level::WARN, batch_id, error = %e, "batch.results.write_failed");
        }
        Err(e) => {
            tracing::event!(Level::WARN, batch_id, error = %e, "batch.results.write_failed");
        }
    }
}

/// Publishes the end of `batch_id` with its counters.
async fn publish_end(
    repository: &TaskRepository,
    event_bus: &EventBusSender,
    batch_id: &str,
    agent: &str,
) {
    match repository.batch(batch_id).await {
        Ok(Some(row)) => {
            let _ = event_bus.send(RuntimeEvent::BatchEnded {
                batch_id: row.batch_id,
                agent_id: AgentId::from(agent),
                state: row.state,
                total: row.total,
                completed: row.completed,
                failed: row.failed,
                canceled: row.canceled,
                awaiting: row.awaiting,
            });
        }
        Ok(None) => {}
        Err(e) => {
            tracing::event!(Level::WARN, batch_id, error = %e, "batch.read.failed");
        }
    }
}
