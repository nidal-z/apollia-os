//! Integration tests for HITL TimeoutWatcher.
//!
//! Tests the automatic cancellation of `input_required` tasks that have been
//! pending for longer than the configured timeout.
//!
//! Uses a real `TaskRepository` (temp SQLite) and the public `TimeoutWatcher::run()`.
//! Time manipulation is done by backdating `input_required_at` in the DB
//! (SQL `datetime('now', '-N hours')`) rather than calling `tokio::time::advance()`,
//! which does not affect SQLite wall-clock comparisons.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use apollia_core::RuntimeEvent;
use apollia_runtime::timeout_watcher::{TimeoutWatcher, TimeoutWatcherConfig};
use apollia_tools::TaskRepository;
use rusqlite::params;

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Opens a `TaskRepository` on a unique temp file.
async fn open_test_repo() -> (TaskRepository, PathBuf) {
    let path = std::env::temp_dir().join(format!("apollia_hitl_tw_{}.db", uuid::Uuid::new_v4()));
    let repo = TaskRepository::open(&path)
        .await
        .expect("TaskRepository::open must succeed");
    (repo, path)
}

/// Inserts a task with `status = 'input_required'` and `input_required_at`
/// backdated by `hours_ago` hours, simulating an expired suspension.
async fn insert_input_required_task(db_path: &Path, task_id: &str, hours_ago: i64) {
    let task_id = task_id.to_string();
    let path = db_path.to_path_buf();
    tokio::task::spawn_blocking(move || {
        let conn = rusqlite::Connection::open(&path).expect("open DB for insert");
        conn.execute_batch("PRAGMA journal_mode=WAL;").expect("WAL");
        conn.execute(
            "INSERT INTO tasks \
             (task_id, status, input_required_prompt, input_required_at) \
             VALUES (?1, 'input_required', 'Confirmer ?', \
                     datetime('now', ?2))",
            params![task_id, format!("-{hours_ago} hours")],
        )
        .expect("insert task must succeed");
    })
    .await
    .expect("spawn_blocking must not fail");
}

/// Reads the `status` column of a task from the DB.
async fn get_task_status(db_path: &Path, task_id: &str) -> Option<String> {
    let path = db_path.to_path_buf();
    let task_id = task_id.to_string();
    tokio::task::spawn_blocking(move || {
        let conn = rusqlite::Connection::open(&path).ok()?;
        conn.query_row(
            "SELECT status FROM tasks WHERE task_id = ?1",
            params![task_id],
            |row| row.get::<_, String>(0),
        )
        .ok()
    })
    .await
    .expect("spawn_blocking must not fail")
}

// ── TimeoutWatcher: expired input_required task -> TaskCanceled ───────

/// GIVEN a task in the `input_required` status in SQLite,
///       with `input_required_at` set 25h before now
/// WHEN `TimeoutWatcher::run()` runs with timeout=24h and scan_interval=1ms
/// THEN `TaskApprovalTimeout{task_id, after_secs:86400}` is emitted on the EventBus,
///      `TaskCanceled{task_id}` is emitted on the EventBus,
///      the task is in the `cancelled` status in SQLite
#[tokio::test]
async fn test_ac5_timeout_watcher_cancels_expired_task() {
    // GIVEN - a task input_required for 25h (past the 24h timeout)
    let (repo, db_path) = open_test_repo().await;
    let task_id = "t-timeout-001";
    insert_input_required_task(&db_path, task_id, 25).await;

    let (event_tx, mut event_rx) = tokio::sync::broadcast::channel::<RuntimeEvent>(32);

    // Use a 1ms scan_interval so the watcher fires on the very first tick.
    let watcher = TimeoutWatcher::new(
        TimeoutWatcherConfig {
            input_required_timeout: Some(Duration::from_secs(24 * 3600)),
            scan_interval: Duration::from_millis(1),
        },
        Arc::new(repo),
        event_tx,
    );

    // WHEN - spawn the watcher loop; it will scan immediately on the first tick.
    let watcher_handle = tokio::spawn(watcher.run());

    // Collect events for up to 2 seconds.
    let mut got_timeout_event = false;
    let mut got_canceled_event = false;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(2);

    loop {
        // Yield to let the watcher task run.
        tokio::time::sleep(Duration::from_millis(10)).await;

        // Drain all events received so far.
        while let Ok(event) = event_rx.try_recv() {
            match event {
                RuntimeEvent::TaskApprovalTimeout {
                    ref task_id,
                    after_secs,
                } if task_id == "t-timeout-001" => {
                    assert_eq!(
                        after_secs, 86400,
                        "after_secs must equal the 24h timeout in seconds"
                    );
                    got_timeout_event = true;
                }
                RuntimeEvent::TaskCanceled { ref task_id } if task_id == "t-timeout-001" => {
                    got_canceled_event = true;
                }
                _ => {}
            }
        }

        if got_timeout_event && got_canceled_event {
            break;
        }

        if tokio::time::Instant::now() >= deadline {
            break;
        }
    }

    // Stop the watcher - it runs an infinite loop, so we abort the task.
    watcher_handle.abort();

    // THEN - both events must have been emitted
    assert!(
        got_timeout_event,
        "RuntimeEvent::TaskApprovalTimeout must be emitted for the expired task"
    );
    assert!(
        got_canceled_event,
        "RuntimeEvent::TaskCanceled must be emitted for the expired task"
    );

    // THEN - task status updated to 'cancelled' in SQLite
    let status = get_task_status(&db_path, "t-timeout-001").await;
    assert_eq!(
        status.as_deref(),
        Some("cancelled"),
        "DB status must be 'cancelled' after timeout; got: {status:?}"
    );
}
