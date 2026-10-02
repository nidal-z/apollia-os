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
