//! The actor that owns the progress of every running batch.

use std::collections::HashMap;
use std::sync::Arc;

use apollia_core::batch_source::{SourceRegistry, SourceSpec};
use apollia_tools::{BatchRow, NewBatch, TaskRepository};
use tokio::sync::mpsc;
use tracing::Level;

use super::{
    BatchCommand, BatchError, BatchLaunch, ItemEnd, ItemRunner, ItemStatus, DEFAULT_CONCURRENCY,
    DEFAULT_MAX_FAILURES, ITEM_END_CAPACITY, MAX_CONCURRENCY,
};

/// The end of one item, as it comes back to the actor.
struct ItemEnded {
    batch_id: String,
    rank: u64,
    end: ItemEnd,
}

/// A batch whose items are still running or pending.
struct Active {
    agent: String,
    concurrency: u32,
    max_failures: u32,
    in_flight: u32,
    interrupted: bool,
}

pub(super) struct BatchActor<R: ItemRunner> {
    commands: mpsc::Receiver<BatchCommand>,
    ends_tx: mpsc::Sender<ItemEnded>,
    ends: mpsc::Receiver<ItemEnded>,
    repository: Arc<TaskRepository>,
    sources: Arc<SourceRegistry>,
    runner: Arc<R>,
    active: HashMap<String, Active>,
}

impl<R: ItemRunner> BatchActor<R> {
    pub(super) fn new(
        commands: mpsc::Receiver<BatchCommand>,
        repository: Arc<TaskRepository>,
        sources: Arc<SourceRegistry>,
        runner: Arc<R>,
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

    fn activate(&mut self, batch: &BatchRow) {
        self.active.insert(
            batch.batch_id.clone(),
            Active {
                agent: batch.agent.clone(),
                concurrency: batch.concurrency.max(1),
                max_failures: batch.max_failures,
                in_flight: 0,
                interrupted: false,
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
        self.runner
            .check(&launch.agent)
            .await
            .map_err(BatchError::Agent)?;
        let items = self.read_items(&launch).await?;
        let source =
            serde_json::to_value(&launch.source).map_err(|e| BatchError::Invalid(e.to_string()))?;
        let batch = self
            .repository
            .create_batch(NewBatch {
                batch_id: uuid::Uuid::new_v4().to_string(),
                agent: launch.agent,
                source,
                grants: launch.grants,
                concurrency,
                max_failures,
                items,
            })
            .await?;
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

    /// Opens the source and reads every item, off the actor's loop.
    async fn read_items(
        &self,
        launch: &BatchLaunch,
    ) -> Result<Vec<(String, serde_json::Value)>, BatchError> {
        let sources = self.sources.clone();
        let kind = launch.source.kind.clone();
        let spec = SourceSpec {
            location: launch.source.location.clone(),
            options: launch.source.options.clone(),
        };
        tokio::task::spawn_blocking(move || {
            let source = sources.open(&kind, &spec)?;
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
    async fn advance(&mut self, batch_id: &str) {
        loop {
            let Some(active) = self.active.get(batch_id) else {
                return;
            };
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
        match self.runner.start(agent, input).await {
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
        let counted = self
            .repository
            .finish_batch_item(batch_id, rank, end.status.as_str(), &end.output)
            .await;
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

    async fn end_if_drained(&mut self, batch_id: &str) {
        let Some(active) = self.active.get(batch_id) else {
            return;
        };
        if active.in_flight > 0 {
            return;
        }
        if !active.interrupted {
            if let Err(e) = self.repository.end_batch(batch_id, "completed").await {
                tracing::event!(Level::WARN, batch_id, error = %e, "batch.record.failed");
            }
            tracing::event!(Level::INFO, batch_id, "batch.completed");
        }
        self.active.remove(batch_id);
    }
}
