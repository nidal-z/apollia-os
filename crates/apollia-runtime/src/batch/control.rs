//! Pause, resume, stop and retry of a batch, as the batch actor applies them.

use tracing::Level;

use apollia_tools::BatchRow;

use super::access::resolve_grants;
use super::actor::BatchActor;
use super::write_back::SOURCE_WRITE_ACCESS;
use super::{BatchControl, BatchError, BatchRetry, ItemRunner};

/// Output of an item canceled because its batch was stopped before it ran.
const STOPPED_BEFORE_RUN: &str = "The batch was stopped before this item ran";

fn refuse(row: &BatchRow, verb: &str) -> BatchError {
    BatchError::State(format!(
        "cannot {verb} batch '{}': it is {}",
        row.batch_id, row.state
    ))
}

impl<R: ItemRunner> BatchActor<R> {
    /// Applies `control` to the batch `batch_id` and returns it as changed.
    pub(super) async fn control(
        &mut self,
        batch_id: &str,
        control: BatchControl,
    ) -> Result<BatchRow, BatchError> {
        let row = self
            .repository
            .batch(batch_id)
            .await?
            .ok_or_else(|| BatchError::NotFound(batch_id.to_string()))?;
        let verb = control.verb();
        match control {
            BatchControl::Pause => self.pause(&row).await?,
            BatchControl::Resume => self.resume_paused(&row).await?,
            BatchControl::Stop => self.stop(&row).await?,
            BatchControl::Retry(retry) => self.retry(&row, retry).await?,
        }
        if verb != "pause" {
            self.advance(batch_id).await;
        }
        tracing::event!(Level::INFO, batch_id, verb, "batch.controlled");
        self.repository
            .batch(batch_id)
            .await?
            .ok_or_else(|| BatchError::NotFound(batch_id.to_string()))
    }

    async fn pause(&mut self, row: &BatchRow) -> Result<(), BatchError> {
        if row.state != "running" {
            return Err(refuse(row, "pause"));
        }
        self.repository
            .set_batch_state(&row.batch_id, "paused", false)
            .await?;
        if let Some(active) = self.active.get_mut(&row.batch_id) {
            active.paused = true;
        }
        Ok(())
    }

    async fn resume_paused(&mut self, row: &BatchRow) -> Result<(), BatchError> {
        if row.state != "paused" {
            return Err(refuse(row, "resume"));
        }
        self.repository
            .set_batch_state(&row.batch_id, "running", false)
            .await?;
        match self.active.get_mut(&row.batch_id) {
            Some(active) => active.paused = false,
            None => {
                // Paused before a restart: its items left running by the
                // process that stopped are run again.
                self.repository.requeue_running_items(&row.batch_id).await?;
                let mut running = row.clone();
                running.state = "running".into();
                self.activate(&running);
            }
        }
        Ok(())
    }

    async fn stop(&mut self, row: &BatchRow) -> Result<(), BatchError> {
        if row.state != "running" && row.state != "paused" {
            return Err(refuse(row, "stop"));
        }
        self.repository
            .set_batch_state(&row.batch_id, "stopped", true)
            .await?;
        if !self.active.contains_key(&row.batch_id) {
            // Paused before a restart: nothing of it runs in this process.
            self.repository.requeue_running_items(&row.batch_id).await?;
            self.activate(row);
        }
        self.repository
            .cancel_pending_items(&row.batch_id, STOPPED_BEFORE_RUN)
            .await?;
        if let Some(active) = self.active.get_mut(&row.batch_id) {
            active.interrupted = true;
            active.paused = false;
        }
        for item in self.repository.batch_items(&row.batch_id).await? {
            if item.status != "running" {
                continue;
            }
            if let Some(task_id) = item.task_id {
                let runner = self.runner.clone();
                tokio::spawn(async move { runner.cancel(&task_id).await });
            }
        }
        Ok(())
    }

    async fn retry(&mut self, row: &BatchRow, retry: BatchRetry) -> Result<(), BatchError> {
        if row.failed + row.awaiting == 0 {
            return Err(BatchError::State(format!(
                "batch '{}' has no failed or waiting item to run again",
                row.batch_id
            )));
        }
        let sensitive = self
            .runner
            .check(&row.agent)
            .await
            .map_err(BatchError::Agent)?;
        let added = resolve_grants(&sensitive, &retry.grants)?;
        // The grants of the launch are given again: a batch run again is in
        // progress, and its consents were withdrawn when it ended. Those the
        // agent no longer holds are dropped.
        let mut grants: Vec<_> = sensitive
            .iter()
            .filter(|a| a.grantable && row.grants.contains(&a.access_id))
            .cloned()
            .collect();
        for access in added {
            if !grants.contains(&access) {
                grants.push(access);
            }
        }
        self.runner
            .grant(&row.batch_id, &row.agent, &grants)
            .await
            .map_err(BatchError::Invalid)?;
        // Writing into the original file stays granted for the batch run again.
        let mut granted: Vec<String> = grants.iter().map(|g| g.access_id.clone()).collect();
        if row.grants.iter().any(|g| g == SOURCE_WRITE_ACCESS) {
            granted.push(SOURCE_WRITE_ACCESS.to_owned());
        }
        let requeued = self
            .repository
            .requeue_for_retry(&row.batch_id, granted)
            .await?;
        tracing::event!(
            Level::INFO,
            batch_id = %row.batch_id,
            requeued,
            "batch.retry.requeued"
        );
        if row.state != "paused" {
            self.repository
                .set_batch_state(&row.batch_id, "running", false)
                .await?;
        }
        match self.active.get_mut(&row.batch_id) {
            Some(active) => active.interrupted = false,
            None => {
                let mut reopened = row.clone();
                if reopened.state != "paused" {
                    reopened.state = "running".into();
                }
                self.activate(&reopened);
            }
        }
        Ok(())
    }
}
