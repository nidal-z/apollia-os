//! The rows of a batch and of its items, in `hitl.db`.
//!
//! A batch is written whole at launch: its row and one row per item, every
//! item `pending`. The batch engine then moves each item to `running` with
//! the task that runs it, and to a terminal status with its output; the
//! counters of the batch follow in the same transaction.
//!
//! A batch is `running`, `paused`, or ended `completed`, `interrupted` or
//! `stopped`. An item is `pending`, `running`, `completed`, `failed`,
//! `canceled`, or `awaiting`: stopped because the agent reached for an access
//! nobody granted for the batch, and run again once it is granted.

use rusqlite::{params, OptionalExtension};

use crate::task_repository::{open_conn, TaskRepoError, TaskRepository};

/// A batch as launched, before its first item runs.
#[derive(Debug, Clone)]
pub struct NewBatch {
    /// The batch.
    pub batch_id: String,
    /// The agent every item is run by.
    pub agent: String,
    /// The source descriptor the items were read from.
    pub source: serde_json::Value,
    /// The accesses granted for the duration of the batch.
    pub grants: Vec<String>,
    /// How many items run at once.
    pub concurrency: u32,
    /// How many failed items the batch tolerates before it is interrupted.
    pub max_failures: u32,
    /// The items, in source order: their origin reference and their input.
    pub items: Vec<(String, serde_json::Value)>,
}

/// A batch as stored.
#[derive(Debug, Clone, PartialEq)]
pub struct BatchRow {
    pub batch_id: String,
    pub agent: String,
    pub source: serde_json::Value,
    pub grants: Vec<String>,
    pub concurrency: u32,
    pub max_failures: u32,
    /// `running`, `paused`, `completed`, `interrupted` or `stopped`.
    pub state: String,
    pub total: u64,
    pub completed: u64,
    pub failed: u64,
    pub canceled: u64,
    /// Items waiting for an access to be granted.
    pub awaiting: u64,
    pub created_at: String,
    pub updated_at: String,
    pub ended_at: Option<String>,
}

/// One item of a batch as stored.
#[derive(Debug, Clone, PartialEq)]
pub struct BatchItemRow {
    /// Its position in the source, from 0.
    pub rank: u64,
    /// Where it comes from inside the source.
    pub origin: String,
    /// Its input as read from the source.
    pub input: serde_json::Value,
    /// `pending`, `running`, `completed`, `failed`, `canceled` or `awaiting`.
    pub status: String,
    /// The answer, or the failure.
    pub output: Option<String>,
    /// The sources the answer cites, when the run reported any.
    pub sources: Option<serde_json::Value>,
    /// The task that ran it, a key of `tasks`.
    pub task_id: Option<String>,
    /// When it last changed state, UTC as SQLite writes it: for a running
    /// item, when its run started.
    pub updated_at: String,
}

const BATCH_COLUMNS: &str = "batch_id, agent_name, source_json, grants_json, concurrency, \
     max_failures, state, total, completed, failed, canceled, created_at, updated_at, ended_at, \
     (SELECT COUNT(*) FROM batch_items i \
      WHERE i.batch_id = batches.batch_id AND i.status = 'awaiting')";

/// Recounts the terminal items of batch `?1` into its counters.
const RECOUNT: &str = "UPDATE batches SET \
         completed  = (SELECT COUNT(*) FROM batch_items \
                       WHERE batch_id = ?1 AND status = 'completed'), \
         failed     = (SELECT COUNT(*) FROM batch_items \
                       WHERE batch_id = ?1 AND status = 'failed'), \
         canceled   = (SELECT COUNT(*) FROM batch_items \
                       WHERE batch_id = ?1 AND status = 'canceled'), \
         updated_at = CURRENT_TIMESTAMP \
     WHERE batch_id = ?1";

const ITEM_COLUMNS: &str =
    "rank, origin_ref, input_json, status, output_text, sources_json, task_id, updated_at";

fn json_column(idx: usize, raw: &str) -> Result<serde_json::Value, rusqlite::Error> {
    serde_json::from_str(raw).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(idx, rusqlite::types::Type::Text, Box::new(e))
    })
}

fn count(row: &rusqlite::Row<'_>, idx: usize) -> Result<u64, rusqlite::Error> {
    Ok(u64::try_from(row.get::<_, i64>(idx)?).unwrap_or(0))
}

fn batch_row(row: &rusqlite::Row<'_>) -> Result<BatchRow, rusqlite::Error> {
    let source: String = row.get(2)?;
    let grants: String = row.get(3)?;
    let grants = serde_json::from_str(&grants).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(3, rusqlite::types::Type::Text, Box::new(e))
    })?;
    Ok(BatchRow {
        batch_id: row.get(0)?,
        agent: row.get(1)?,
        source: json_column(2, &source)?,
        grants,
        concurrency: u32::try_from(count(row, 4)?).unwrap_or(1),
        max_failures: u32::try_from(count(row, 5)?).unwrap_or(0),
        state: row.get(6)?,
        total: count(row, 7)?,
        completed: count(row, 8)?,
        failed: count(row, 9)?,
        canceled: count(row, 10)?,
        created_at: row.get(11)?,
        updated_at: row.get(12)?,
        ended_at: row.get(13)?,
        awaiting: count(row, 14)?,
    })
}

fn item_row(row: &rusqlite::Row<'_>) -> Result<BatchItemRow, rusqlite::Error> {
    let input: String = row.get(2)?;
    let sources: Option<String> = row.get(5)?;
    Ok(BatchItemRow {
        rank: count(row, 0)?,
        origin: row.get(1)?,
        input: json_column(2, &input)?,
        status: row.get(3)?,
        output: row.get(4)?,
        sources: sources.map(|s| json_column(5, &s)).transpose()?,
        task_id: row.get(6)?,
        updated_at: row.get(7)?,
    })
}

fn get_batch(
    conn: &rusqlite::Connection,
    batch_id: &str,
) -> Result<Option<BatchRow>, TaskRepoError> {
    Ok(conn
        .query_row(
            &format!("SELECT {BATCH_COLUMNS} FROM batches WHERE batch_id = ?1"),
            params![batch_id],
            batch_row,
        )
        .optional()?)
}

impl TaskRepository {
    pub(super) async fn blocking<T: Send + 'static>(
        &self,
        f: impl FnOnce(&mut rusqlite::Connection) -> Result<T, TaskRepoError> + Send + 'static,
    ) -> Result<T, TaskRepoError> {
        let path = self.db_path.clone();
        tokio::task::spawn_blocking(move || {
            let mut conn = open_conn(&path)?;
            f(&mut conn)
        })
        .await
        .map_err(|e| TaskRepoError::Internal(e.to_string()))?
    }

    /// Records a batch and all its items, `running` and `pending`, in one
    /// transaction.
    ///
    /// # Errors
    ///
    /// [`TaskRepoError::Sqlite`] or [`TaskRepoError::Json`].
    pub async fn create_batch(&self, batch: NewBatch) -> Result<BatchRow, TaskRepoError> {
        let source = serde_json::to_string(&batch.source)?;
        let grants = serde_json::to_string(&batch.grants)?;
        let items = batch
            .items
            .iter()
            .map(|(origin, input)| Ok((origin.clone(), serde_json::to_string(input)?)))
            .collect::<Result<Vec<_>, serde_json::Error>>()?;
        self.blocking(move |conn| {
            let tx = conn.transaction()?;
            tx.execute(
                "INSERT INTO batches \
                     (batch_id, agent_name, source_json, grants_json, concurrency, max_failures, \
                      state, total) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'running', ?7)",
                params![
                    &batch.batch_id,
                    &batch.agent,
                    &source,
                    &grants,
                    batch.concurrency,
                    batch.max_failures,
                    i64::try_from(items.len()).unwrap_or(i64::MAX)
                ],
            )?;
            {
                let mut insert = tx.prepare(
                    "INSERT INTO batch_items (batch_id, rank, origin_ref, input_json) \
                     VALUES (?1, ?2, ?3, ?4)",
                )?;
                for (rank, (origin, input)) in items.iter().enumerate() {
                    insert.execute(params![
                        &batch.batch_id,
                        i64::try_from(rank).unwrap_or(i64::MAX),
                        origin,
                        input
                    ])?;
                }
            }
            let row = get_batch(&tx, &batch.batch_id)?
                .ok_or_else(|| TaskRepoError::NotFound(batch.batch_id.clone()))?;
            tx.commit()?;
            Ok(row)
        })
        .await
    }

    /// One batch, or `None`.
    ///
    /// # Errors
    ///
    /// [`TaskRepoError::Sqlite`] or [`TaskRepoError::Json`].
    pub async fn batch(&self, batch_id: &str) -> Result<Option<BatchRow>, TaskRepoError> {
        let batch_id = batch_id.to_string();
        self.blocking(move |conn| get_batch(conn, &batch_id)).await
    }

    /// The batches, newest first, at most `limit`.
    ///
    /// # Errors
    ///
    /// [`TaskRepoError::Sqlite`] or [`TaskRepoError::Json`].
    pub async fn list_batches(&self, limit: u32) -> Result<Vec<BatchRow>, TaskRepoError> {
        self.blocking(move |conn| {
            let mut stmt = conn.prepare(&format!(
                "SELECT {BATCH_COLUMNS} FROM batches \
                 ORDER BY created_at DESC, rowid DESC LIMIT ?1"
            ))?;
            let rows = stmt
                .query_map(params![limit], batch_row)?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(rows)
        })
        .await
    }

    /// The items of a batch, in source order.
    ///
    /// # Errors
    ///
    /// [`TaskRepoError::Sqlite`] or [`TaskRepoError::Json`].
    pub async fn batch_items(&self, batch_id: &str) -> Result<Vec<BatchItemRow>, TaskRepoError> {
        let batch_id = batch_id.to_string();
        self.blocking(move |conn| {
            let mut stmt = conn.prepare(&format!(
                "SELECT {ITEM_COLUMNS} FROM batch_items WHERE batch_id = ?1 ORDER BY rank"
            ))?;
            let rows = stmt
                .query_map(params![&batch_id], item_row)?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(rows)
        })
        .await
    }

    /// The batches still `running`, oldest first: those a restart resumes.
    ///
    /// # Errors
    ///
    /// [`TaskRepoError::Sqlite`] or [`TaskRepoError::Json`].
    pub async fn running_batches(&self) -> Result<Vec<BatchRow>, TaskRepoError> {
        self.blocking(move |conn| {
            let mut stmt = conn.prepare(&format!(
                "SELECT {BATCH_COLUMNS} FROM batches WHERE state = 'running' \
                 ORDER BY created_at, rowid"
            ))?;
            let rows = stmt
                .query_map([], batch_row)?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(rows)
        })
        .await
    }

    /// Puts back to `pending` the items of a batch left `running` by a
    /// process that stopped, and forgets their task. Returns how many.
    ///
    /// # Errors
    ///
    /// [`TaskRepoError::Sqlite`].
    pub async fn requeue_running_items(&self, batch_id: &str) -> Result<u64, TaskRepoError> {
        let batch_id = batch_id.to_string();
        self.blocking(move |conn| {
            let n = conn.execute(
                "UPDATE batch_items SET status = 'pending', task_id = NULL, \
                     updated_at = CURRENT_TIMESTAMP \
                 WHERE batch_id = ?1 AND status = 'running'",
                params![&batch_id],
            )?;
            Ok(n as u64)
        })
        .await
    }

    /// The next `limit` pending items of a batch, in source order.
    ///
    /// # Errors
    ///
    /// [`TaskRepoError::Sqlite`] or [`TaskRepoError::Json`].
    pub async fn pending_batch_items(
        &self,
        batch_id: &str,
        limit: u32,
    ) -> Result<Vec<BatchItemRow>, TaskRepoError> {
        let batch_id = batch_id.to_string();
        self.blocking(move |conn| {
            let mut stmt = conn.prepare(&format!(
                "SELECT {ITEM_COLUMNS} FROM batch_items \
                 WHERE batch_id = ?1 AND status = 'pending' ORDER BY rank LIMIT ?2"
            ))?;
            let rows = stmt
                .query_map(params![&batch_id, limit], item_row)?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(rows)
        })
        .await
    }

    /// Marks an item `running` under the task that runs it.
    ///
    /// # Errors
    ///
    /// [`TaskRepoError::Sqlite`].
    pub async fn start_batch_item(
        &self,
        batch_id: &str,
        rank: u64,
        task_id: &str,
    ) -> Result<(), TaskRepoError> {
        let batch_id = batch_id.to_string();
        let task_id = task_id.to_string();
        self.blocking(move |conn| {
            conn.execute(
                "UPDATE batch_items SET status = 'running', task_id = ?3, \
                     updated_at = CURRENT_TIMESTAMP \
                 WHERE batch_id = ?1 AND rank = ?2",
                params![&batch_id, i64::try_from(rank).unwrap_or(i64::MAX), &task_id],
            )?;
            Ok(())
        })
        .await
    }

    /// Records the end of an item, `completed`, `failed` or `canceled`, with
    /// its output, and counts it in its batch. Returns the batch as counted.
    ///
    /// # Errors
    ///
    /// [`TaskRepoError::NotFound`] for an unknown batch, otherwise
    /// [`TaskRepoError::Sqlite`] or [`TaskRepoError::Json`].
    pub async fn finish_batch_item(
        &self,
        batch_id: &str,
        rank: u64,
        status: &str,
        output: &str,
    ) -> Result<BatchRow, TaskRepoError> {
        let batch_id = batch_id.to_string();
        let status = status.to_string();
        let output = output.to_string();
        self.blocking(move |conn| {
            let tx = conn.transaction()?;
            tx.execute(
                "UPDATE batch_items SET status = ?3, output_text = ?4, \
                     updated_at = CURRENT_TIMESTAMP \
                 WHERE batch_id = ?1 AND rank = ?2",
                params![
                    &batch_id,
                    i64::try_from(rank).unwrap_or(i64::MAX),
                    &status,
                    &output
                ],
            )?;
            tx.execute(RECOUNT, params![&batch_id])?;
            let row = get_batch(&tx, &batch_id)?
                .ok_or_else(|| TaskRepoError::NotFound(batch_id.clone()))?;
            tx.commit()?;
            Ok(row)
        })
        .await
    }

    /// Ends a batch in `state`, `completed`, `interrupted` or `stopped`.
    ///
    /// # Errors
    ///
    /// [`TaskRepoError::Sqlite`].
    pub async fn end_batch(&self, batch_id: &str, state: &str) -> Result<(), TaskRepoError> {
        self.set_batch_state(batch_id, state, true).await
    }

    /// Moves a batch to `state`. An ended state stamps `ended_at`; `running`
    /// and `paused` clear it, so a batch run again is no longer ended.
    ///
    /// # Errors
    ///
    /// [`TaskRepoError::Sqlite`].
    pub async fn set_batch_state(
        &self,
        batch_id: &str,
        state: &str,
        ended: bool,
    ) -> Result<(), TaskRepoError> {
        let batch_id = batch_id.to_string();
        let state = state.to_string();
        self.blocking(move |conn| {
            conn.execute(
                "UPDATE batches SET state = ?2, \
                     ended_at = CASE WHEN ?3 THEN CURRENT_TIMESTAMP ELSE NULL END, \
                     updated_at = CURRENT_TIMESTAMP \
                 WHERE batch_id = ?1",
                params![&batch_id, &state, ended],
            )?;
            Ok(())
        })
        .await
    }

    /// Cancels the items of a batch that never started, with `reason` as
    /// their output, and counts them. Returns how many.
    ///
    /// # Errors
    ///
    /// [`TaskRepoError::Sqlite`].
    pub async fn cancel_pending_items(
        &self,
        batch_id: &str,
        reason: &str,
    ) -> Result<u64, TaskRepoError> {
        let batch_id = batch_id.to_string();
        let reason = reason.to_string();
        self.blocking(move |conn| {
            let tx = conn.transaction()?;
            let n = tx.execute(
                "UPDATE batch_items SET status = 'canceled', output_text = ?2, \
                     updated_at = CURRENT_TIMESTAMP \
                 WHERE batch_id = ?1 AND status = 'pending'",
                params![&batch_id, &reason],
            )?;
            tx.execute(RECOUNT, params![&batch_id])?;
            tx.commit()?;
            Ok(n as u64)
        })
        .await
    }

    /// Puts back to `pending` the one item at `rank` of a batch, when it waits
    /// for an access, from its input as read at launch: its output, sources
    /// and task are forgotten, the grants of the batch are left as they are.
    /// Returns whether the item waited.
    ///
    /// # Errors
    ///
    /// [`TaskRepoError::Sqlite`].
    pub async fn requeue_awaiting_item(
        &self,
        batch_id: &str,
        rank: u64,
    ) -> Result<bool, TaskRepoError> {
        let batch_id = batch_id.to_string();
        self.blocking(move |conn| {
            let tx = conn.transaction()?;
            let n = tx.execute(
                "UPDATE batch_items SET status = 'pending', output_text = NULL, \
                     sources_json = NULL, task_id = NULL, updated_at = CURRENT_TIMESTAMP \
                 WHERE batch_id = ?1 AND rank = ?2 AND status = 'awaiting'",
                params![&batch_id, i64::try_from(rank).unwrap_or(i64::MAX)],
            )?;
            tx.execute(RECOUNT, params![&batch_id])?;
            tx.commit()?;
            Ok(n == 1)
        })
        .await
    }

    /// Puts back to `pending` the items of a batch that failed or wait for an
    /// access, from their input as read at launch: their output, sources and
    /// task are forgotten. `grants` are added to those of the batch. Returns
    /// how many items are run again.
    ///
    /// # Errors
    ///
    /// [`TaskRepoError::Sqlite`] or [`TaskRepoError::Json`].
    pub async fn requeue_for_retry(
        &self,
        batch_id: &str,
        grants: Vec<String>,
    ) -> Result<u64, TaskRepoError> {
        let batch_id = batch_id.to_string();
        self.blocking(move |conn| {
            let tx = conn.transaction()?;
            let held: String = tx.query_row(
                "SELECT grants_json FROM batches WHERE batch_id = ?1",
                params![&batch_id],
                |row| row.get(0),
            )?;
            let mut merged: Vec<String> = serde_json::from_str(&held)?;
            for grant in grants {
                if !merged.contains(&grant) {
                    merged.push(grant);
                }
            }
            tx.execute(
                "UPDATE batches SET grants_json = ?2 WHERE batch_id = ?1",
                params![&batch_id, serde_json::to_string(&merged)?],
            )?;
            let n = tx.execute(
                "UPDATE batch_items SET status = 'pending', output_text = NULL, \
                     sources_json = NULL, task_id = NULL, updated_at = CURRENT_TIMESTAMP \
                 WHERE batch_id = ?1 AND status IN ('failed', 'awaiting')",
                params![&batch_id],
            )?;
            tx.execute(RECOUNT, params![&batch_id])?;
            tx.commit()?;
            Ok(n as u64)
        })
        .await
    }

    /// Removes a batch and its items, in one transaction, unless it is
    /// active: `running` or `paused`, or with an item still `running`. The
    /// state is read and the rows removed in the same transaction, so a batch
    /// that starts again in between is never removed.
    ///
    /// The tasks that ran the items stay in `tasks`, as the history of the
    /// agent. Nothing outside `hitl.db` is touched: a file the batch was
    /// exported to, or the source its answers were written back into, stays
    /// as it is.
    ///
    /// # Errors
    ///
    /// [`TaskRepoError::Sqlite`] or [`TaskRepoError::Json`].
    pub async fn delete_batch(&self, batch_id: &str) -> Result<BatchDeletion, TaskRepoError> {
        let batch_id = batch_id.to_string();
        self.blocking(move |conn| {
            let tx = conn.transaction()?;
            let Some(row) = get_batch(&tx, &batch_id)? else {
                return Ok(BatchDeletion::NotFound);
            };
            let in_flight: i64 = tx.query_row(
                "SELECT COUNT(*) FROM batch_items WHERE batch_id = ?1 AND status = 'running'",
                params![&batch_id],
                |r| r.get(0),
            )?;
            if row.state == "running" || row.state == "paused" || in_flight > 0 {
                return Ok(BatchDeletion::Active {
                    state: row.state,
                    in_flight: u64::try_from(in_flight).unwrap_or(0),
                });
            }
            let items = tx.execute(
                "DELETE FROM batch_items WHERE batch_id = ?1",
                params![&batch_id],
            )?;
            tx.execute(
                "DELETE FROM batches WHERE batch_id = ?1",
                params![&batch_id],
            )?;
            tx.commit()?;
            Ok(BatchDeletion::Deleted {
                items: u64::try_from(items).unwrap_or(u64::MAX),
            })
        })
        .await
    }
}

/// What [`TaskRepository::delete_batch`] did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BatchDeletion {
    /// The batch and its items were removed.
    Deleted {
        /// Items removed with it.
        items: u64,
    },
    /// No batch has this id.
    NotFound,
    /// The batch is active and was left as it is.
    Active {
        /// Its state, `running`, `paused`, or an ended state whose items
        /// still run.
        state: String,
        /// Its items still running.
        in_flight: u64,
    },
}

#[cfg(test)]
mod tests;
