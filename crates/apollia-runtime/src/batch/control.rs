//! Pause, resume, stop, retry and deletion of a batch, and the refusal of an
//! item that waits, as the batch actor applies them.

use tracing::Level;

use apollia_tools::{BatchDeletion, BatchRow};

use super::access::resolve_grants;
use super::actor::BatchActor;
use super::{BatchAccess, BatchControl, BatchError, BatchRetry, ItemRunner, ItemStatus};

/// Output of an item canceled because its batch was stopped before it ran.
const STOPPED_BEFORE_RUN: &str = "The batch was stopped before this item ran";
/// Output of a waiting item the user refused.
const REFUSED_BY_THE_USER: &str = "Refused by the user";

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
            BatchControl::Refuse { rank, reason } => {
                self.refuse_item(&row, rank, reason.as_deref()).await?
            }
            BatchControl::RunOnce { rank, access } => self.run_once(&row, rank, access).await?,
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

    /// Deletes the batch `batch_id` and its items, unless it is active: still
    /// held by this actor (an item in flight, or a pause), or recorded as
    /// `running` or `paused`, or with an item recorded as running. Returns
    /// how many items were removed. Files outside `hitl.db` are not touched.
    pub(super) async fn delete(&mut self, batch_id: &str) -> Result<u64, BatchError> {
        if let Some(active) = self.active.get(batch_id) {
            return Err(BatchError::State(format!(
                "cannot delete batch '{batch_id}': it is {}; stop it first",
                if active.paused {
                    "paused".to_string()
                } else if active.interrupted {
                    format!("ending, with {} items still running", active.in_flight)
                } else {
                    "running".to_string()
                }
            )));
        }
        match self.repository.delete_batch(batch_id).await? {
            BatchDeletion::Deleted { items } => {
                self.once.retain(|(batch, _), _| batch != batch_id);
                tracing::event!(Level::INFO, batch_id, items, "batch.deleted");
                Ok(items)
            }
            BatchDeletion::NotFound => Err(BatchError::NotFound(batch_id.to_string())),
            BatchDeletion::Active { state, in_flight } => {
                Err(BatchError::State(if in_flight > 0 {
                    format!(
                    "cannot delete batch '{batch_id}': {in_flight} of its items are still running"
                )
                } else {
                    format!("cannot delete batch '{batch_id}': it is {state}; stop it first")
                }))
            }
        }
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

    async fn refuse_item(
        &mut self,
        row: &BatchRow,
        rank: u64,
        reason: Option<&str>,
    ) -> Result<(), BatchError> {
        let waits = self
            .repository
            .batch_items(&row.batch_id)
            .await?
            .iter()
            .any(|item| item.rank == rank && item.status == ItemStatus::Awaiting.as_str());
        if !waits {
            return Err(BatchError::State(format!(
                "item {rank} of batch '{}' does not wait for a decision",
                row.batch_id
            )));
        }
        let output = match reason {
            Some(reason) if !reason.trim().is_empty() => {
                format!("{REFUSED_BY_THE_USER}: {}", reason.trim())
            }
            _ => REFUSED_BY_THE_USER.to_string(),
        };
        self.repository
            .finish_batch_item(&row.batch_id, rank, ItemStatus::Failed.as_str(), &output)
            .await?;
        tracing::event!(Level::INFO, batch_id = %row.batch_id, rank, "batch.item.refused");
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
        let grants = self.grant_again(row, &sensitive, added).await?;
        let requeued = self
            .repository
            .requeue_for_retry(
                &row.batch_id,
                grants.iter().map(|g| g.access_id.clone()).collect(),
            )
            .await?;
        tracing::event!(
            Level::INFO,
            batch_id = %row.batch_id,
            requeued,
            "batch.retry.requeued"
        );
        self.reopen(row).await
    }

    /// Runs again the item at `rank`, which waits to execute code, alone,
    /// allowed one call of `access`. The grants of the batch are given again
    /// as a retry gives them; no consent is recorded for the code itself.
    async fn run_once(
        &mut self,
        row: &BatchRow,
        rank: u64,
        access: String,
    ) -> Result<(), BatchError> {
        let sensitive = self
            .runner
            .check(&row.agent)
            .await
            .map_err(BatchError::Agent)?;
        if !sensitive
            .iter()
            .any(|a| a.access_id == access && a.executes_code())
        {
            return Err(BatchError::Invalid(format!(
                "'{access}' is not an access of this agent that executes code; \
                 only those are allowed once"
            )));
        }
        self.grant_again(row, &sensitive, Vec::new()).await?;
        if !self
            .repository
            .requeue_awaiting_item(&row.batch_id, rank)
            .await?
        {
            return Err(BatchError::State(format!(
                "item {rank} of batch '{}' does not wait for a decision",
                row.batch_id
            )));
        }
        self.once.insert((row.batch_id.clone(), rank), access);
        tracing::event!(Level::INFO, batch_id = %row.batch_id, rank, "batch.item.allowed_once");
        self.reopen(row).await
    }

    /// Records again the grants of the launch, plus `added`: a batch run again
    /// is in progress, and its consents were withdrawn when it ended. Those
    /// the agent no longer holds are dropped. Returns the grants in force.
    async fn grant_again(
        &self,
        row: &BatchRow,
        sensitive: &[BatchAccess],
        added: Vec<BatchAccess>,
    ) -> Result<Vec<BatchAccess>, BatchError> {
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
        Ok(grants)
    }

    /// Puts a batch that has items to run again back in progress, unless it
    /// is paused, and takes it back in this process when it had ended.
    async fn reopen(&mut self, row: &BatchRow) -> Result<(), BatchError> {
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
