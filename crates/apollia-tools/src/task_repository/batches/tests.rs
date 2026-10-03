use serde_json::json;

use super::*;

async fn repo(dir: &tempfile::TempDir) -> TaskRepository {
    TaskRepository::open(&dir.path().join("hitl.db"))
        .await
        .unwrap()
}

fn new_batch(id: &str, items: usize) -> NewBatch {
    NewBatch {
        batch_id: id.into(),
        agent: "clerk".into(),
        source: json!({"kind": "memory", "location": "/data/a.csv"}),
        grants: vec!["file_read".into()],
        concurrency: 2,
        max_failures: 3,
        items: (0..items)
            .map(|i| (format!("row-{i}"), json!({"text": format!("item {i}")})))
            .collect(),
    }
}

// GIVEN a batch of three items
// WHEN it is recorded
// THEN the batch is running with its launch settings and every item is pending, in order
#[tokio::test]
async fn create_batch_records_the_batch_and_its_pending_items() {
    let dir = tempfile::tempdir().unwrap();
    let repo = repo(&dir).await;

    let row = repo.create_batch(new_batch("b-1", 3)).await.unwrap();

    assert_eq!(row.state, "running");
    assert_eq!(row.total, 3);
    assert_eq!((row.concurrency, row.max_failures), (2, 3));
    assert_eq!(row.grants, vec!["file_read".to_string()]);
    assert_eq!(row.source["kind"], "memory");
    let items = repo.batch_items("b-1").await.unwrap();
    let ranks: Vec<(u64, &str, &str)> = items
        .iter()
        .map(|i| (i.rank, i.origin.as_str(), i.status.as_str()))
        .collect();
    assert_eq!(
        ranks,
        vec![
            (0, "row-0", "pending"),
            (1, "row-1", "pending"),
            (2, "row-2", "pending")
        ]
    );
    assert_eq!(items[1].input, json!({"text": "item 1"}));
}

// GIVEN a batch with one item completed, one failed and one canceled
// WHEN each end is recorded
// THEN the counters of the batch follow, and the item keeps its task and output
#[tokio::test]
async fn finishing_items_counts_them_in_the_batch() {
    let dir = tempfile::tempdir().unwrap();
    let repo = repo(&dir).await;
    repo.create_batch(new_batch("b-1", 4)).await.unwrap();
    for (rank, task) in [(0, "t-0"), (1, "t-1"), (2, "t-2")] {
        repo.start_batch_item("b-1", rank, task).await.unwrap();
    }

    repo.finish_batch_item("b-1", 0, "completed", "ok")
        .await
        .unwrap();
    repo.finish_batch_item("b-1", 1, "failed", "boom")
        .await
        .unwrap();
    let row = repo
        .finish_batch_item("b-1", 2, "canceled", "")
        .await
        .unwrap();

    assert_eq!(
        (row.completed, row.failed, row.canceled, row.total),
        (1, 1, 1, 4)
    );
    let items = repo.batch_items("b-1").await.unwrap();
    assert_eq!(items[1].task_id.as_deref(), Some("t-1"));
    assert_eq!(items[1].output.as_deref(), Some("boom"));
    assert_eq!(items[3].status, "pending");
}

// GIVEN a running batch with an item left running by a stopped process
// WHEN its running items are requeued
// THEN that item is pending again without a task, and it comes first among the pending
#[tokio::test]
async fn requeue_puts_running_items_back_to_pending() {
    let dir = tempfile::tempdir().unwrap();
    let repo = repo(&dir).await;
    repo.create_batch(new_batch("b-1", 3)).await.unwrap();
    repo.start_batch_item("b-1", 0, "t-0").await.unwrap();
    repo.finish_batch_item("b-1", 0, "completed", "ok")
        .await
        .unwrap();
    repo.start_batch_item("b-1", 1, "t-1").await.unwrap();

    let requeued = repo.requeue_running_items("b-1").await.unwrap();

    assert_eq!(requeued, 1);
    let pending = repo.pending_batch_items("b-1", 10).await.unwrap();
    assert_eq!(
        pending.iter().map(|i| i.rank).collect::<Vec<_>>(),
        vec![1, 2]
    );
    assert!(pending[0].task_id.is_none());
    let running = repo.running_batches().await.unwrap();
    assert_eq!(running.len(), 1);
}

// GIVEN two batches, one of them ended
// WHEN the batches are listed and the running ones read
// THEN the list holds both and only the other is running
#[tokio::test]
async fn ended_batches_leave_the_running_set() {
    let dir = tempfile::tempdir().unwrap();
    let repo = repo(&dir).await;
    repo.create_batch(new_batch("b-1", 1)).await.unwrap();
    repo.create_batch(new_batch("b-2", 1)).await.unwrap();

    repo.end_batch("b-1", "interrupted").await.unwrap();

    assert_eq!(repo.list_batches(10).await.unwrap().len(), 2);
    let running: Vec<String> = repo
        .running_batches()
        .await
        .unwrap()
        .into_iter()
        .map(|b| b.batch_id)
        .collect();
    assert_eq!(running, vec!["b-2".to_string()]);
    let ended = repo.batch("b-1").await.unwrap().unwrap();
    assert_eq!(ended.state, "interrupted");
    assert!(ended.ended_at.is_some());
    assert!(repo.batch("missing").await.unwrap().is_none());
}

// GIVEN a batch whose items failed, waited for an access, completed and never ran
// WHEN the batch is retried with one more grant
// THEN only the failed and waiting items are pending again, from their input as
//      read at launch, their output and task forgotten, and the grant is added
#[tokio::test]
async fn retrying_puts_failed_and_waiting_items_back_from_their_input() {
    // GIVEN
    let dir = tempfile::tempdir().unwrap();
    let repo = repo(&dir).await;
    repo.create_batch(new_batch("b-r", 4)).await.unwrap();
    for (rank, status) in [(0, "failed"), (1, "awaiting"), (2, "completed")] {
        repo.start_batch_item("b-r", rank, &format!("t-{rank}"))
            .await
            .unwrap();
        repo.finish_batch_item("b-r", rank, status, "a short preview")
            .await
            .unwrap();
    }
    let before = repo.batch("b-r").await.unwrap().unwrap();
    assert_eq!(
        (before.failed, before.awaiting, before.completed),
        (1, 1, 1)
    );

    // WHEN
    let requeued = repo
        .requeue_for_retry("b-r", vec!["send_email".into(), "file_read".into()])
        .await
        .unwrap();

    // THEN
    assert_eq!(requeued, 2);
    let items = repo.batch_items("b-r").await.unwrap();
    let statuses: Vec<&str> = items.iter().map(|i| i.status.as_str()).collect();
    assert_eq!(statuses, vec!["pending", "pending", "completed", "pending"]);
    assert_eq!(items[0].input, json!({"text": "item 0"}));
    assert_eq!(items[1].input, json!({"text": "item 1"}));
    assert_eq!(
        (items[0].output.as_deref(), items[0].task_id.as_deref()),
        (None, None)
    );
    let after = repo.batch("b-r").await.unwrap().unwrap();
    assert_eq!((after.failed, after.awaiting, after.completed), (0, 0, 1));
    assert_eq!(
        after.grants,
        vec!["file_read".to_string(), "send_email".to_string()]
    );
}

// GIVEN a batch of three items, one of them running
// WHEN its pending items are canceled and it is stopped, then run again
// THEN the two that never ran are canceled and counted, the stop stamps the end,
//      and running it again clears that end
#[tokio::test]
async fn stopping_cancels_what_never_ran_and_a_new_run_clears_the_end() {
    // GIVEN
    let dir = tempfile::tempdir().unwrap();
    let repo = repo(&dir).await;
    repo.create_batch(new_batch("b-s", 3)).await.unwrap();
    repo.start_batch_item("b-s", 0, "t-0").await.unwrap();

    // WHEN
    let canceled = repo
        .cancel_pending_items("b-s", "stopped before it ran")
        .await
        .unwrap();
    repo.set_batch_state("b-s", "stopped", true).await.unwrap();
    let stopped = repo.batch("b-s").await.unwrap().unwrap();
    repo.set_batch_state("b-s", "running", false).await.unwrap();
    let running = repo.batch("b-s").await.unwrap().unwrap();

    // THEN
    assert_eq!(canceled, 2);
    assert_eq!((stopped.state.as_str(), stopped.canceled), ("stopped", 2));
    assert!(stopped.ended_at.is_some());
    assert_eq!(running.state, "running");
    assert!(running.ended_at.is_none());
    let items = repo.batch_items("b-s").await.unwrap();
    assert_eq!(items[0].status, "running");
    assert_eq!(items[2].output.as_deref(), Some("stopped before it ran"));
}
