//! TimeoutWatcher: automatic cancellation of expired `input_required` tasks.
//!
//! Periodically scans the DB for tasks suspended for too long and cancels them
//! by emitting [`RuntimeEvent::TaskApprovalTimeout`] then
//! [`RuntimeEvent::TaskCanceled`] on the EventBus.
//!
//! Started by the [`Supervisor`](crate::supervisor::Supervisor) after the APIServer.
//! Enforces fail-fast behavior and non-negotiable safety guardrails.

use std::sync::Arc;
use std::time::Duration;

use apollia_core::{RuntimeEvent, TaskId};
use apollia_tools::TaskRepository;

use crate::eventbus::EventBusSender;

// Configuration

/// Configuration for the HITL approval timeout watcher.
///
/// Defaults: no global timeout (indefinite pause), 60 s scan interval.
#[derive(Debug, Clone)]
pub struct TimeoutWatcherConfig {
    /// Duration after which an `input_required` task is cancelled automatically.
    ///
    /// `None` (default): no automatic cancellation, the task stays suspended
    /// indefinitely until the operator responds explicitly.
    /// `Some(d)`: cancellation after `d` (opt-in via `[hitl] timeout_hours` in `apollia.toml`).
    pub input_required_timeout: Option<Duration>,
    /// Interval between two consecutive scans.
    ///
    /// Uses `tokio::time::interval` to avoid time drift.
    /// Default: 60 seconds. No effect when `input_required_timeout` is `None`.
    pub scan_interval: Duration,
}

impl Default for TimeoutWatcherConfig {
    fn default() -> Self {
        Self {
            input_required_timeout: None,
            scan_interval: Duration::from_secs(60),
        }
    }
}

// Errors

/// Internal errors of the [`TimeoutWatcher`].
///
/// Returned by [`TimeoutWatcher::scan_and_cancel`]. The main loop
/// [`TimeoutWatcher::run`] logs the error and continues without propagating or panicking.
#[derive(Debug, thiserror::Error)]
pub enum TimeoutWatcherError {
    /// SQLite error while accessing the [`TaskRepository`].
    #[error("erreur DB : {0}")]
    Database(#[from] apollia_tools::TaskRepoError),
}

// TimeoutWatcher

/// Tokio task that automatically cancels expired `input_required` tasks.
///
/// Every [`scan_interval`] seconds, scans tasks whose `input_required_at`
/// exceeds [`input_required_timeout`] (when configured). Behavior by config:
///
/// - `input_required_timeout = None`: no automatic cancellation, no-op each tick.
/// - `input_required_timeout = Some(d)`: for each expired task:
///   1. Update its status to `cancelled` via [`TaskRepository::cancel_task`].
///   2. Emit [`RuntimeEvent::TaskApprovalTimeout`] on the EventBus.
///   3. Emit [`RuntimeEvent::TaskCanceled`] on the EventBus.
///
/// On a SQLite error, logs a `tracing::warn!` and continues the loop without crashing.
///
/// [`scan_interval`]: TimeoutWatcherConfig::scan_interval
/// [`input_required_timeout`]: TimeoutWatcherConfig::input_required_timeout
pub struct TimeoutWatcher {
    config: TimeoutWatcherConfig,
    db: Arc<TaskRepository>,
    event_bus: EventBusSender,
}

impl TimeoutWatcher {
    /// Create a new `TimeoutWatcher`.
    ///
    /// # Arguments
    ///
    /// - `config`: intervals and timeout threshold.
    /// - `db`: access to the SQLite `TaskRepository`.
    /// - `event_bus`: channel for emitting runtime events.
    pub fn new(
        config: TimeoutWatcherConfig,
        db: Arc<TaskRepository>,
        event_bus: EventBusSender,
    ) -> Self {
        Self {
            config,
            db,
            event_bus,
        }
    }

    /// Run the watch loop, never returns unless the runtime shuts down.
    ///
    /// Uses `tokio::time::interval` (not `sleep`) to avoid time drift.
    /// On a SQLite error during a scan, logs a `tracing::warn!` and continues.
    pub async fn run(self) {
        let mut interval = tokio::time::interval(self.config.scan_interval);
        loop {
            interval.tick().await;
            match self.scan_and_cancel().await {
                Ok(n) if n > 0 => {
                    tracing::info!(cancelled = n, "tâches HITL expirées annulées");
                }
                Ok(_) => {}
                Err(e) => {
                    tracing::warn!(error = %e, "erreur scan timeout watcher");
                }
            }
        }
    }

    /// Scan expired `input_required` tasks and cancel them.
    ///
    /// Returns `Ok(0)` immediately when `input_required_timeout` is `None` (no-op).
    /// Otherwise returns the number of tasks actually cancelled.
    /// Returns an error only when the initial DB scan fails.
    /// Per-task cancellation errors are logged via `warn!` but do not fail the scan.
    async fn scan_and_cancel(&self) -> Result<usize, TimeoutWatcherError> {
        let timeout = match self.config.input_required_timeout {
            Some(t) => t,
            None => return Ok(0),
        };

        let expired_ids = self.db.find_input_required_older_than(timeout).await?;

        let mut cancelled = 0usize;

        for task_id_str in &expired_ids {
            if let Err(e) = self
                .db
                .cancel_task(task_id_str, "input_required_timeout")
                .await
            {
                tracing::warn!(
                    task_id = %task_id_str,
                    error = %e,
                    "erreur annulation tâche HITL expirée"
                );
                continue;
            }

            tracing::warn!(
                task_id = %task_id_str,
                "tâche annulée - timeout input_required"
            );

            let task_id: TaskId = task_id_str.as_str().into();

            let after_secs = timeout.as_secs();
            let _ = self.event_bus.send(RuntimeEvent::TaskApprovalTimeout {
                task_id: task_id.clone(),
                after_secs,
            });

            let _ = self.event_bus.send(RuntimeEvent::TaskCanceled { task_id });

            cancelled += 1;
        }

        Ok(cancelled)
    }
}

// Tests

#[cfg(test)]
mod tests {
    use super::*;
    use apollia_tools::TaskRepository;
    use rusqlite::params;
    use std::path::PathBuf;

    /// Open a `TaskRepository` on a unique temporary file.
    async fn open_test_repo() -> (TaskRepository, PathBuf) {
        let path = std::env::temp_dir().join(format!("apollia_tw_{}.db", uuid::Uuid::new_v4()));
        let repo = TaskRepository::open(&path)
            .await
            .expect("TaskRepository::open failed");
        (repo, path)
    }

    /// Insert an `input_required` task and backdate `input_required_at` by `hours_ago`.
    async fn insert_expired_task(db_path: &PathBuf, task_id: &str, hours_ago: i64) {
        let task_id = task_id.to_string();
        let path = db_path.clone();
        tokio::task::spawn_blocking(move || {
            let conn = rusqlite::Connection::open(&path).unwrap();
            conn.execute_batch("PRAGMA journal_mode=WAL;").unwrap();
            conn.execute(
                "INSERT INTO tasks (task_id, status, input_required_prompt, input_required_at) \
                 VALUES (?1, 'input_required', 'confirmer ?', \
                         datetime('now', ?2))",
                params![&task_id, format!("-{} hours", hours_ago)],
            )
            .unwrap();
        })
        .await
        .unwrap();
    }

    /// Read a task's status from the DB.
    async fn get_task_status(db_path: &PathBuf, task_id: &str) -> Option<String> {
        let path = db_path.clone();
        let task_id = task_id.to_string();
        tokio::task::spawn_blocking(move || {
            let conn = rusqlite::Connection::open(&path).unwrap();
            conn.query_row(
                "SELECT status FROM tasks WHERE task_id = ?1",
                params![&task_id],
                |row| row.get::<_, String>(0),
            )
            .ok()
        })
        .await
        .unwrap()
    }

    // Expired task is cancelled and two events are emitted.

    #[tokio::test]
    async fn test_expired_task_is_cancelled() {
        // GIVEN an input_required task that is 25h old
        let (repo, db_path) = open_test_repo().await;
        let task_id = "t-tw-001";
        insert_expired_task(&db_path, task_id, 25).await;

        let (tx, mut rx) = tokio::sync::broadcast::channel::<RuntimeEvent>(16);
        let watcher = TimeoutWatcher::new(
            TimeoutWatcherConfig {
                input_required_timeout: Some(Duration::from_secs(24 * 3600)),
                scan_interval: Duration::from_secs(3600),
            },
            Arc::new(repo),
            tx,
        );

        // WHEN scan_and_cancel() runs with timeout=24h
        let result = watcher.scan_and_cancel().await;

        // THEN 1 task cancelled, DB status = 'cancelled'
        assert!(result.is_ok(), "scan_and_cancel doit réussir");
        assert_eq!(result.unwrap(), 1, "1 tâche doit être annulée");

        let status = get_task_status(&db_path, task_id).await;
        assert_eq!(
            status.as_deref(),
            Some("cancelled"),
            "statut DB doit être 'cancelled'"
        );

        // AND 2 events emitted: TaskApprovalTimeout + TaskCanceled
        let ev1 = rx.try_recv().expect("TaskApprovalTimeout doit être émis");
        let ev2 = rx.try_recv().expect("TaskCanceled doit être émis");

        assert!(
            matches!(
                ev1,
                RuntimeEvent::TaskApprovalTimeout {
                    after_secs: 86400,
                    ..
                }
            ),
            "premier event doit être TaskApprovalTimeout ; got={ev1:?}"
        );
        assert!(
            matches!(ev2, RuntimeEvent::TaskCanceled { .. }),
            "second event doit être TaskCanceled ; got={ev2:?}"
        );
    }

    // Recent task (30 min) is not cancelled.

    #[tokio::test]
    async fn test_recent_task_not_cancelled() {
        // GIVEN an input_required task only 30 min old
        let (repo, db_path) = open_test_repo().await;
        let task_id = "t-tw-002";

        // Insert with input_required_at = "now" (0 hours)
        insert_expired_task(&db_path, task_id, 0).await;

        let (tx, _rx) = tokio::sync::broadcast::channel::<RuntimeEvent>(16);
        let watcher = TimeoutWatcher::new(
            TimeoutWatcherConfig {
                input_required_timeout: Some(Duration::from_secs(24 * 3600)),
                scan_interval: Duration::from_secs(3600),
            },
            Arc::new(repo),
            tx,
        );

        // WHEN scan_and_cancel() runs with timeout=24h
        let result = watcher.scan_and_cancel().await;

        // THEN 0 tasks cancelled, DB status unchanged
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 0, "aucune tâche ne doit être annulée");

        let status = get_task_status(&db_path, task_id).await;
        assert_eq!(
            status.as_deref(),
            Some("input_required"),
            "statut doit rester 'input_required'"
        );
    }

    // Configurable timeout: 2h, task 3h old, gets cancelled.

    #[tokio::test]
    async fn test_custom_timeout_2h() {
        // GIVEN an input_required task 3h old + config timeout=2h
        let (repo, db_path) = open_test_repo().await;
        let task_id = "t-tw-003";
        insert_expired_task(&db_path, task_id, 3).await;

        let (tx, _rx) = tokio::sync::broadcast::channel::<RuntimeEvent>(16);
        let watcher = TimeoutWatcher::new(
            TimeoutWatcherConfig {
                input_required_timeout: Some(Duration::from_secs(2 * 3600)),
                scan_interval: Duration::from_secs(3600),
            },
            Arc::new(repo),
            tx,
        );

        // WHEN scan_and_cancel() runs with timeout=2h
        let result = watcher.scan_and_cancel().await;

        // THEN task cancelled (3h > 2h)
        assert!(result.is_ok());
        assert_eq!(
            result.unwrap(),
            1,
            "1 tâche doit être annulée (3h > timeout 2h)"
        );

        let status = get_task_status(&db_path, task_id).await;
        assert_eq!(status.as_deref(), Some("cancelled"));
    }

    // DB error returns Err, no panic.

    #[tokio::test]
    async fn test_db_error_does_not_crash() {
        // GIVEN a TaskRepository whose DB file is corrupted
        let (repo, db_path) = open_test_repo().await;

        // Corruption: overwrite the SQLite file with invalid bytes
        std::fs::write(&db_path, b"not a valid sqlite database").unwrap();

        let (tx, _rx) = tokio::sync::broadcast::channel::<RuntimeEvent>(16);
        let watcher = TimeoutWatcher::new(
            TimeoutWatcherConfig {
                input_required_timeout: Some(Duration::from_secs(24 * 3600)),
                scan_interval: Duration::from_secs(3600),
            },
            Arc::new(repo),
            tx,
        );

        // WHEN scan_and_cancel() is called on a corrupted DB
        let result = watcher.scan_and_cancel().await;

        // THEN Err returned (no panic), the run() loop can keep going
        assert!(
            result.is_err(),
            "scan_and_cancel doit retourner Err sur DB corrompue"
        );
    }

    // No timeout configured: no-op, expired task not cancelled.

    #[tokio::test]
    async fn test_no_global_timeout_is_noop() {
        // GIVEN an input_required task 100h old + config with no global timeout
        let (repo, db_path) = open_test_repo().await;
        let task_id = "t-tw-006";
        insert_expired_task(&db_path, task_id, 100).await;

        let (tx, _rx) = tokio::sync::broadcast::channel::<RuntimeEvent>(16);
        let watcher = TimeoutWatcher::new(
            TimeoutWatcherConfig {
                input_required_timeout: None,
                scan_interval: Duration::from_secs(60),
            },
            Arc::new(repo),
            tx,
        );

        // WHEN scan_and_cancel() runs with no global timeout
        let result = watcher.scan_and_cancel().await;

        // THEN 0 tasks cancelled, the task stays paused indefinitely
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 0, "aucune annulation sans timeout global");

        let status = get_task_status(&db_path, task_id).await;
        assert_eq!(
            status.as_deref(),
            Some("input_required"),
            "statut doit rester 'input_required' - pause indéfinie"
        );
    }
}
