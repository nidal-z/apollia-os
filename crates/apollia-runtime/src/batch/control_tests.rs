//! Pause, resume, stop and retry of a batch, its grants, the items that wait
//! for an access, and the event that ends it, over the scripted runner.

use std::sync::atomic::Ordering;

use apollia_core::RuntimeEvent;

use super::tests::{launch, registry, repository, spawn, wait_state, Scripted, WAIT};
use super::*;
use crate::test_support::{collect_events_until, poll_until_async};

async fn wait_items(repo: &TaskRepository, batch_id: &str, status: &str, n: usize) {
    let reached = poll_until_async(WAIT, || async {
        repo.batch_items(batch_id)
            .await
            .unwrap()
            .iter()
            .filter(|i| i.status == status)
            .count()
            == n
    })
    .await;
    let items = repo.batch_items(batch_id).await.unwrap();
    assert!(reached, "never {n} items {status}: {items:?}");
}

/// The events published up to the first `BatchEnded` of `batch_id`, that one
/// last.
async fn until_ended(
    events: &mut tokio::sync::broadcast::Receiver<RuntimeEvent>,
    batch_id: &str,
) -> Vec<RuntimeEvent> {
    let (seen, found) = collect_events_until(
        WAIT,
        events,
        |e| matches!(e, RuntimeEvent::BatchEnded { batch_id: b, .. } if b == batch_id),
    )
    .await;
    assert!(found, "the batch never ended: {seen:?}");
    seen
}

/// The first `BatchEnded` of `batch_id` on `events`.
async fn ended(
    events: &mut tokio::sync::broadcast::Receiver<RuntimeEvent>,
    batch_id: &str,
) -> RuntimeEvent {
    until_ended(events, batch_id)
        .await
        .pop()
        .expect("the end of the batch")
}

// GIVEN a batch of four items, two at a time, whose items hang
// WHEN it is paused, the two running items end, then it is resumed
// THEN nothing starts while paused, the batch is not ended by its drained
//      runs, and the resume starts the two others
#[tokio::test]
async fn test_a_paused_batch_starts_nothing_until_it_is_resumed() {
    // GIVEN
    let dir = tempfile::tempdir().unwrap();
    let repo = repository(&dir).await;
    let runner = Arc::new(Scripted {
        hang_all: true,
        ..Scripted::default()
    });
    let (handle, _bus) = spawn(repo.clone(), registry(&["ok"; 4]), runner.clone());
    let batch = handle.launch(launch(Some(2), None)).await.unwrap();
    wait_items(&repo, &batch.batch_id, "running", 2).await;

    // WHEN
    let paused = handle
        .control(&batch.batch_id, BatchControl::Pause)
        .await
        .unwrap();
    for task in ["task-0", "task-1"] {
        runner.cancel(task).await;
    }
    wait_items(&repo, &batch.batch_id, "canceled", 2).await;
    let while_paused = repo.batch(&batch.batch_id).await.unwrap().unwrap();
    let started_while_paused = runner.started.load(Ordering::SeqCst);
    let resumed = handle
        .control(&batch.batch_id, BatchControl::Resume)
        .await
        .unwrap();

    // THEN
    assert_eq!(paused.state, "paused");
    assert_eq!(while_paused.state, "paused");
    assert!(while_paused.ended_at.is_none());
    assert_eq!(started_while_paused, 2);
    assert_eq!(resumed.state, "running");
    wait_items(&repo, &batch.batch_id, "running", 2).await;
    assert_eq!(runner.started.load(Ordering::SeqCst), 4);
    let again = handle
        .control(&batch.batch_id, BatchControl::Resume)
        .await
        .unwrap_err();
    assert!(matches!(again, BatchError::State(_)), "{again:?}");
}

// GIVEN a batch of five items, two of them running and hanging
// WHEN it is stopped
// THEN the two running runs are cancelled through their task, the three others
//      never run, and the batch ends stopped, its grants withdrawn and its end
//      published with its counters
#[tokio::test]
async fn test_stop_cancels_the_running_items_by_their_task() {
    // GIVEN
    let dir = tempfile::tempdir().unwrap();
    let repo = repository(&dir).await;
    let runner = Arc::new(Scripted {
        hang_all: true,
        ..Scripted::default()
    });
    let (handle, bus) = spawn(repo.clone(), registry(&["ok"; 5]), runner.clone());
    let mut events = bus.subscribe();
    let batch = handle.launch(launch(Some(2), None)).await.unwrap();
    wait_items(&repo, &batch.batch_id, "running", 2).await;

    // WHEN
    let stopped = handle
        .control(&batch.batch_id, BatchControl::Stop)
        .await
        .unwrap();

    // THEN
    assert_eq!(stopped.state, "stopped");
    let end = ended(&mut events, &batch.batch_id).await;
    let mut cancelled = runner.cancelled.lock().unwrap().clone();
    cancelled.sort();
    assert_eq!(cancelled, vec!["task-0".to_string(), "task-1".to_string()]);
    assert_eq!(runner.started.load(Ordering::SeqCst), 2);
    let row = repo.batch(&batch.batch_id).await.unwrap().unwrap();
    assert_eq!((row.state.as_str(), row.canceled), ("stopped", 5));
    assert!(row.ended_at.is_some());
    assert!(matches!(
        end,
        RuntimeEvent::BatchEnded { ref state, total: 5, completed: 0, failed: 0, canceled: 5, .. }
            if state == "stopped"
    ));
    assert_eq!(
        *runner.released.lock().unwrap(),
        vec![batch.batch_id.clone()]
    );
    let twice = handle
        .control(&batch.batch_id, BatchControl::Stop)
        .await
        .unwrap_err();
    assert!(matches!(twice, BatchError::State(_)), "{twice:?}");
}

// GIVEN a completed batch whose second item failed
// WHEN its failures are retried
// THEN the failed item runs again from its whole input as read at launch, and
//      the batch completes again with no failure left
#[tokio::test]
async fn test_retry_runs_the_failed_items_again_from_their_input() {
    // GIVEN
    let dir = tempfile::tempdir().unwrap();
    let repo = repository(&dir).await;
    let runner = Arc::new(Scripted::default());
    let (handle, _bus) = spawn(
        repo.clone(),
        registry(&["ok", "fail", "ok"]),
        runner.clone(),
    );
    let batch = handle.launch(launch(Some(1), None)).await.unwrap();
    let done = wait_state(&repo, &batch.batch_id, "completed").await;
    assert_eq!(done.failed, 1);
    let failed_input = repo.batch_items(&batch.batch_id).await.unwrap()[1]
        .input
        .clone();

    // WHEN
    let retried = handle
        .control(&batch.batch_id, BatchControl::Retry(BatchRetry::default()))
        .await
        .unwrap();

    // THEN
    assert_eq!(retried.state, "running");
    let row = wait_state(&repo, &batch.batch_id, "completed").await;
    assert_eq!(runner.started.load(Ordering::SeqCst), 4);
    let inputs = runner.inputs.lock().unwrap().clone();
    assert_eq!(inputs[3], failed_input);
    assert_eq!(inputs[3], serde_json::json!({"outcome": "fail"}));
    assert_eq!((row.completed, row.failed), (2, 1));
    let nothing = handle
        .control(
            &batch.batch_id,
            BatchControl::Retry(BatchRetry {
                grants: vec!["send_email".into()],
            }),
        )
        .await;
    assert!(nothing.is_ok(), "{nothing:?}");
}

// GIVEN a batch whose second item reaches for an access nobody granted
// WHEN it runs
// THEN that item waits rather than failing, the batch goes on and completes,
//      the wait is published with the access, and the end counts it
#[tokio::test]
async fn test_an_ungranted_access_makes_the_item_wait_and_the_batch_go_on() {
    // GIVEN
    let dir = tempfile::tempdir().unwrap();
    let repo = repository(&dir).await;
    let runner = Arc::new(Scripted::default());
    let (handle, bus) = spawn(
        repo.clone(),
        registry(&["ok", "await", "ok"]),
        runner.clone(),
    );
    let mut events = bus.subscribe();

    // WHEN
    let batch = handle.launch(launch(Some(1), Some(0))).await.unwrap();

    // THEN
    let row = wait_state(&repo, &batch.batch_id, "completed").await;
    assert_eq!((row.completed, row.failed, row.awaiting), (2, 0, 1));
    let items = repo.batch_items(&batch.batch_id).await.unwrap();
    assert_eq!(items[1].status, "awaiting");
    let mut seen = until_ended(&mut events, &batch.batch_id).await;
    let end = seen.pop().expect("the end of the batch");
    let waited: Vec<(u64, String)> = seen
        .into_iter()
        .filter_map(|e| match e {
            RuntimeEvent::BatchItemAwaitingApproval { rank, access, .. } => Some((rank, access)),
            _ => None,
        })
        .collect();
    assert_eq!(waited, vec![(1, "send_email".to_string())]);
    assert!(matches!(
        end,
        RuntimeEvent::BatchEnded {
            completed: 2,
            failed: 0,
            awaiting: 1,
            ..
        }
    ));
}

// GIVEN an agent with one grantable sensitive access and one that is not
// WHEN batches are launched granting the first, granting code, and granting a
//      name the agent does not hold
// THEN the first is recorded for that batch and withdrawn at its end; the two
//      others are refused before anything is recorded
#[tokio::test]
async fn test_grants_are_checked_recorded_for_the_batch_and_withdrawn_at_its_end() {
    // GIVEN
    let dir = tempfile::tempdir().unwrap();
    let repo = repository(&dir).await;
    let runner = Arc::new(Scripted::default());
    let (handle, _bus) = spawn(repo.clone(), registry(&["ok"]), runner.clone());
    let with = |grant: &str| {
        let mut l = launch(None, None);
        l.grants = vec![grant.to_string()];
        l
    };

    // WHEN
    let batch = handle.launch(with("send_email")).await.unwrap();
    let code = handle.launch(with("bash_executor")).await.unwrap_err();
    let stranger = handle.launch(with("web_search")).await.unwrap_err();

    // THEN
    assert_eq!(batch.grants, vec!["send_email".to_string()]);
    assert_eq!(
        runner.granted.lock().unwrap()[0],
        (batch.batch_id.clone(), vec!["send_email".to_string()])
    );
    wait_state(&repo, &batch.batch_id, "completed").await;
    let released = poll_until_async(WAIT, || async {
        runner.released.lock().unwrap().contains(&batch.batch_id)
    })
    .await;
    assert!(released, "the grants were never withdrawn");
    assert!(
        matches!(code, BatchError::Invalid(ref m) if m.contains("execute_code")),
        "{code:?}"
    );
    assert!(matches!(stranger, BatchError::Invalid(_)), "{stranger:?}");
    assert_eq!(repo.list_batches(10).await.unwrap().len(), 1);
    let listed = handle.accesses("clerk").await.unwrap();
    assert_eq!(listed.len(), 2);
}

// GIVEN a finished batch whose two items wait to execute code
// WHEN the first is allowed once, then code is asked for an access that is
//      not code, and for an item that does not wait
// THEN the first item alone runs again, allowed one call of the code tool,
//      and completes; the second still waits; the other two are refused
#[tokio::test]
async fn test_an_item_waiting_on_code_runs_again_alone_allowed_one_call() {
    // GIVEN
    let dir = tempfile::tempdir().unwrap();
    let repo = repository(&dir).await;
    let runner = Arc::new(Scripted::default());
    let (handle, _bus) = spawn(
        repo.clone(),
        registry(&["ok", "code", "code"]),
        runner.clone(),
    );
    let batch = handle.launch(launch(Some(1), Some(5))).await.unwrap();
    let done = wait_state(&repo, &batch.batch_id, "completed").await;
    assert_eq!(done.awaiting, 2);

    // WHEN
    let allowed = handle
        .control(
            &batch.batch_id,
            BatchControl::RunOnce {
                rank: 1,
                access: "bash_executor".into(),
            },
        )
        .await;
    let row = wait_state(&repo, &batch.batch_id, "completed").await;
    let not_code = handle
        .control(
            &batch.batch_id,
            BatchControl::RunOnce {
                rank: 2,
                access: "send_email".into(),
            },
        )
        .await;
    let not_waiting = handle
        .control(
            &batch.batch_id,
            BatchControl::RunOnce {
                rank: 0,
                access: "bash_executor".into(),
            },
        )
        .await;

    // THEN
    assert!(allowed.is_ok(), "{allowed:?}");
    assert_eq!((row.completed, row.awaiting), (2, 1));
    let items = repo.batch_items(&batch.batch_id).await.unwrap();
    let statuses: Vec<&str> = items.iter().map(|i| i.status.as_str()).collect();
    assert_eq!(statuses, vec!["completed", "completed", "awaiting"]);
    assert_eq!(runner.started.load(Ordering::SeqCst), 4);
    let once = runner.once.lock().unwrap().clone();
    assert_eq!(once, vec![None, None, None, Some("bash_executor".into())]);
    assert!(matches!(not_code, Err(BatchError::Invalid(m)) if m.contains("executes code")));
    assert!(matches!(not_waiting, Err(BatchError::State(_))));
}
