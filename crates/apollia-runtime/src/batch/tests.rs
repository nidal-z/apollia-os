//! The batch actor over a scripted runner: threshold, defaults, refusals and
//! the resumption after a restart.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use apollia_core::batch_source::{BatchSource, MemorySource};

use super::*;
use crate::test_support::poll_until_async;

pub(super) const WAIT: Duration = Duration::from_secs(10);

/// What the scripted runner does with an item, read from its `outcome` field:
/// `ok`, `fail`, `hang` until cancelled, `await` an access, or `code`: wait to
/// execute code unless allowed one call of it.
#[derive(Default)]
pub(super) struct Scripted {
    pub(super) started: AtomicU32,
    pub(super) held: Mutex<Vec<(String, oneshot::Sender<ItemEnd>)>>,
    /// Every item hangs until cancelled or the runner is dropped.
    pub(super) hang_all: bool,
    /// The inputs started, in order.
    pub(super) inputs: Mutex<Vec<serde_json::Value>>,
    /// The grants recorded, per batch.
    pub(super) granted: Mutex<Vec<(String, Vec<String>)>>,
    /// The batches whose grants were withdrawn.
    pub(super) released: Mutex<Vec<String>>,
    /// The tasks cancelled.
    pub(super) cancelled: Mutex<Vec<String>>,
    /// Governance refuses writing into the original file.
    pub(super) deny_original_write: bool,
    /// The writes into the original file recorded, as `(batch, location)`.
    pub(super) original_writes: Mutex<Vec<(String, String)>>,
    /// The access each start was allowed one call of, in order.
    pub(super) once: Mutex<Vec<Option<String>>>,
}

/// The sensitive accesses of the scripted agent: one to grant, one never.
pub(super) fn scripted_accesses() -> Vec<BatchAccess> {
    vec![
        BatchAccess {
            access_id: "send_email".into(),
            permits: "Send an email.".into(),
            family: "send_external".into(),
            grantable: true,
        },
        BatchAccess {
            access_id: "bash_executor".into(),
            permits: "Run a command.".into(),
            family: "execute_code".into(),
            grantable: false,
        },
    ]
}

impl ItemRunner for Arc<Scripted> {
    async fn check(&self, agent: &str) -> Result<Vec<BatchAccess>, String> {
        if agent == "clerk" {
            Ok(scripted_accesses())
        } else {
            Err(format!("agent '{agent}' cannot run a batch"))
        }
    }

    async fn grant(
        &self,
        batch_id: &str,
        _agent: &str,
        grants: &[BatchAccess],
    ) -> Result<(), String> {
        self.granted.lock().unwrap().push((
            batch_id.to_string(),
            grants.iter().map(|g| g.access_id.clone()).collect(),
        ));
        Ok(())
    }

    async fn allow_original_write(&self, _agent: &str, _location: &str) -> Result<(), String> {
        if self.deny_original_write {
            Err("refused by the policy".into())
        } else {
            Ok(())
        }
    }

    async fn record_original_write(
        &self,
        batch_id: &str,
        _agent: &str,
        location: &str,
    ) -> Result<(), String> {
        self.original_writes
            .lock()
            .unwrap()
            .push((batch_id.to_string(), location.to_string()));
        Ok(())
    }

    async fn release(&self, batch_id: &str) -> Result<(), String> {
        self.released.lock().unwrap().push(batch_id.to_string());
        Ok(())
    }

    async fn start(
        &self,
        _batch_id: &str,
        _agent: &str,
        input: &serde_json::Value,
        once: Option<&str>,
    ) -> Result<StartedItem, String> {
        let n = self.started.fetch_add(1, Ordering::SeqCst);
        self.inputs.lock().unwrap().push(input.clone());
        self.once.lock().unwrap().push(once.map(str::to_owned));
        let task_id = format!("task-{n}");
        let (tx, end) = oneshot::channel();
        let outcome = input["outcome"].as_str().unwrap_or("ok");
        if self.hang_all || outcome == "hang" {
            self.held.lock().unwrap().push((task_id.clone(), tx));
        } else {
            let (status, awaiting) = match outcome {
                "fail" => (ItemStatus::Failed, None),
                "await" => (ItemStatus::Awaiting, Some("send_email".to_string())),
                "code" if once != Some("bash_executor") => {
                    (ItemStatus::Awaiting, Some("bash_executor".to_string()))
                }
                _ => (ItemStatus::Completed, None),
            };
            let output = input["answer"]
                .as_str()
                .map_or_else(|| format!("{outcome} {n}"), str::to_owned);
            let _ = tx.send(ItemEnd {
                status,
                output,
                awaiting,
            });
        }
        Ok(StartedItem { task_id, end })
    }

    async fn cancel(&self, task_id: &str) {
        self.cancelled.lock().unwrap().push(task_id.to_string());
        let mut held = self.held.lock().unwrap();
        if let Some(i) = held.iter().position(|(id, _)| id == task_id) {
            let (_, tx) = held.remove(i);
            let _ = tx.send(ItemEnd {
                status: ItemStatus::Canceled,
                output: "cancelled".into(),
                awaiting: None,
            });
        }
    }
}

/// A batch actor over `runner`, publishing on a bus of its own.
pub(super) fn spawn(
    repo: Arc<TaskRepository>,
    sources: Arc<SourceRegistry>,
    runner: Arc<Scripted>,
) -> (BatchHandle, crate::eventbus::EventBusSender) {
    let (bus, _) = tokio::sync::broadcast::channel(256);
    (BatchHandle::spawn(repo, sources, runner, bus.clone()), bus)
}

/// A registry with one `memory` kind whose items carry `outcomes`, in order.
pub(super) fn registry(outcomes: &[&'static str]) -> Arc<SourceRegistry> {
    let outcomes: Vec<&'static str> = outcomes.to_vec();
    let mut registry = SourceRegistry::new();
    registry
        .register(
            "memory",
            Box::new(move |spec| {
                let mut source = MemorySource::new(&spec.location, &["outcome"]);
                for (i, outcome) in outcomes.iter().enumerate() {
                    source = source.with_item(&format!("row-{i}"), &[("outcome", outcome)]);
                }
                Ok(Box::new(source) as Box<dyn BatchSource>)
            }),
        )
        .unwrap();
    Arc::new(registry)
}

pub(super) fn launch(concurrency: Option<u32>, max_failures: Option<u32>) -> BatchLaunch {
    BatchLaunch {
        agent: "clerk".into(),
        source: SourceDescriptor {
            kind: "memory".into(),
            location: "/data/requests.csv".into(),
            options: BTreeMap::new(),
            write: None,
        },
        grants: vec![],
        concurrency,
        max_failures,
    }
}

pub(super) async fn repository(dir: &tempfile::TempDir) -> Arc<TaskRepository> {
    Arc::new(
        TaskRepository::open(&dir.path().join("hitl.db"))
            .await
            .unwrap(),
    )
}

pub(super) async fn wait_state(repo: &TaskRepository, batch_id: &str, state: &str) -> BatchRow {
    let reached = poll_until_async(WAIT, || async {
        repo.batch(batch_id).await.unwrap().unwrap().state == state
    })
    .await;
    let row = repo.batch(batch_id).await.unwrap().unwrap();
    assert!(reached, "the batch never reached {state}: {row:?}");
    row
}

// GIVEN a launch that names neither a concurrency nor a threshold, on three
//       items that succeed
// WHEN the batch is launched
// THEN it runs one item at a time with the documented threshold, and completes
#[tokio::test]
async fn test_launch_defaults_and_completion() {
    // GIVEN
    let dir = tempfile::tempdir().unwrap();
    let repo = repository(&dir).await;
    let runner = Arc::new(Scripted::default());
    let (handle, _bus) = spawn(repo.clone(), registry(&["ok", "ok", "ok"]), runner.clone());

    // WHEN
    let batch = handle.launch(launch(None, None)).await.unwrap();

    // THEN
    assert_eq!(batch.concurrency, DEFAULT_CONCURRENCY);
    assert_eq!(batch.concurrency, 1);
    assert_eq!(batch.max_failures, DEFAULT_MAX_FAILURES);
    assert_eq!(batch.total, 3);
    let done = wait_state(&repo, &batch.batch_id, "completed").await;
    assert_eq!((done.completed, done.failed), (3, 0));
    let detail = handle.detail(&batch.batch_id).await.unwrap().unwrap();
    let tasks: Vec<Option<String>> = detail.items.iter().map(|i| i.task_id.clone()).collect();
    assert_eq!(
        tasks,
        vec![
            Some("task-0".to_string()),
            Some("task-1".to_string()),
            Some("task-2".to_string())
        ]
    );
    assert_eq!(handle.list(10).await.unwrap().len(), 1);
}

#[tokio::test]
async fn test_a_batch_launched_while_another_runs_waits_for_it() {
    // GIVEN a first batch whose item hangs, holding the engine
    let dir = tempfile::tempdir().unwrap();
    let repo = repository(&dir).await;
    let runner = Arc::new(Scripted::default());
    let (handle, _bus) = spawn(repo.clone(), registry(&["hang"]), runner.clone());
    let first = handle.launch(launch(None, None)).await.unwrap();
    assert!(
        poll_until_async(WAIT, || async {
            runner.started.load(Ordering::SeqCst) == 1
        })
        .await
    );

    // WHEN a second batch is launched, then the first one's item ends
    // A launch starts what it may before it answers: the count read after
    // it is the count of the moment.
    let second = handle.launch(launch(None, None)).await.unwrap();
    let started_while_busy = runner.started.load(Ordering::SeqCst);
    handle
        .control(&first.batch_id, BatchControl::Stop)
        .await
        .unwrap();

    // THEN the second batch started nothing while the first ran, and runs
    // once the first has ended
    assert_eq!(started_while_busy, 1);
    assert!(
        poll_until_async(WAIT, || async {
            runner.started.load(Ordering::SeqCst) == 2
        })
        .await
    );
    wait_state(&repo, &first.batch_id, "stopped").await;
    assert_eq!(
        repo.batch(&second.batch_id).await.unwrap().unwrap().state,
        "running"
    );
}

#[tokio::test]
async fn test_an_item_past_its_deadline_fails_and_the_batch_goes_on() {
    // GIVEN a batch whose first item hangs and whose items may run 100 ms
    let dir = tempfile::tempdir().unwrap();
    let repo = repository(&dir).await;
    let runner = Arc::new(Scripted::default());
    let (bus, _) = tokio::sync::broadcast::channel(256);
    let handle = BatchHandle::spawn_with_item_deadline(
        repo.clone(),
        registry(&["hang", "ok"]),
        runner.clone(),
        bus,
        Duration::from_millis(100),
    );

    // WHEN the batch runs
    let batch = handle.launch(launch(None, None)).await.unwrap();

    // THEN the hanging item is cancelled and failed with its reason, and the
    // next item runs to the end of the batch
    let done = wait_state(&repo, &batch.batch_id, "completed").await;
    assert_eq!((done.completed, done.failed), (1, 1));
    assert_eq!(
        runner.cancelled.lock().unwrap().clone(),
        vec!["task-0".to_string()]
    );
    let detail = handle.detail(&batch.batch_id).await.unwrap().unwrap();
    assert!(detail.items[0]
        .output
        .as_deref()
        .is_some_and(|out| out.contains("ran past")));
}

// GIVEN six items that all fail and a threshold of two failures
// WHEN the batch runs
// THEN it is interrupted at the third failure, and the other items stay pending
#[tokio::test]
async fn test_failures_past_the_threshold_interrupt_the_batch() {
    // GIVEN
    let dir = tempfile::tempdir().unwrap();
    let repo = repository(&dir).await;
    let runner = Arc::new(Scripted::default());
    let (handle, _bus) = spawn(repo.clone(), registry(&["fail"; 6]), runner.clone());

    // WHEN
    let batch = handle.launch(launch(Some(1), Some(2))).await.unwrap();

    // THEN
    let row = wait_state(&repo, &batch.batch_id, "interrupted").await;
    assert_eq!(row.failed, 3);
    assert!(row.ended_at.is_some());
    let pending = repo.pending_batch_items(&batch.batch_id, 10).await.unwrap();
    assert_eq!(
        pending.iter().map(|i| i.rank).collect::<Vec<_>>(),
        vec![3, 4, 5]
    );
    assert_eq!(runner.started.load(Ordering::SeqCst), 3);
}

// GIVEN a batch whose failures stay within its threshold
// WHEN it runs to its end
// THEN it completes rather than being interrupted
#[tokio::test]
async fn test_failures_within_the_threshold_let_the_batch_complete() {
    // GIVEN
    let dir = tempfile::tempdir().unwrap();
    let repo = repository(&dir).await;
    let (handle, _bus) = spawn(
        repo.clone(),
        registry(&["fail", "ok", "fail", "ok"]),
        Arc::new(Scripted::default()),
    );

    // WHEN
    let batch = handle.launch(launch(Some(2), Some(2))).await.unwrap();

    // THEN
    let row = wait_state(&repo, &batch.batch_id, "completed").await;
    assert_eq!((row.completed, row.failed), (2, 2));
}

// GIVEN a running batch of four items, two of them running when its process
//       stops
// WHEN a new batch actor starts on the same hitl.db
// THEN the batch resumes from its pending items, the interrupted runs included,
//      and completes
#[tokio::test]
async fn test_a_running_batch_resumes_after_a_restart() {
    // GIVEN
    let dir = tempfile::tempdir().unwrap();
    let repo = repository(&dir).await;
    let stalled = Arc::new(Scripted {
        hang_all: true,
        ..Scripted::default()
    });
    let (first, _bus) = spawn(repo.clone(), registry(&["ok"; 4]), stalled.clone());
    let batch = first.launch(launch(Some(2), None)).await.unwrap();
    let running = poll_until_async(WAIT, || async {
        repo.batch_items(&batch.batch_id)
            .await
            .unwrap()
            .iter()
            .filter(|i| i.status == "running")
            .count()
            == 2
    })
    .await;
    assert!(running, "two items never ran");
    drop(first);

    // WHEN
    let resumed = Arc::new(Scripted::default());
    let (_second, _bus) = spawn(repo.clone(), registry(&[]), resumed.clone());

    // THEN
    let row = wait_state(&repo, &batch.batch_id, "completed").await;
    assert_eq!((row.completed, row.failed), (4, 0));
    assert_eq!(resumed.started.load(Ordering::SeqCst), 4);
    assert_eq!(stalled.started.load(Ordering::SeqCst), 2);
}

// GIVEN an interrupted batch and a restart
// WHEN the new actor starts
// THEN the interrupted batch is left as it is
#[tokio::test]
async fn test_an_interrupted_batch_is_not_resumed() {
    // GIVEN
    let dir = tempfile::tempdir().unwrap();
    let repo = repository(&dir).await;
    let (first, _bus) = spawn(
        repo.clone(),
        registry(&["fail", "ok"]),
        Arc::new(Scripted::default()),
    );
    let batch = first.launch(launch(Some(1), Some(0))).await.unwrap();
    wait_state(&repo, &batch.batch_id, "interrupted").await;
    drop(first);

    // WHEN
    let resumed = Arc::new(Scripted::default());
    let (second, _bus) = spawn(repo.clone(), registry(&[]), resumed.clone());
    // A launch is answered after the resumption, which is the actor's first act.
    let probe = second.launch(launch(Some(1), None)).await;

    // THEN
    assert!(probe.is_ok());
    let row = repo.batch(&batch.batch_id).await.unwrap().unwrap();
    assert_eq!(row.state, "interrupted");
    let pending = repo.pending_batch_items(&batch.batch_id, 10).await.unwrap();
    assert_eq!(pending.len(), 1);
}

// GIVEN a registry without the kind a launch names
// WHEN the batch is launched
// THEN it is refused as an unknown source type and nothing is recorded
#[tokio::test]
async fn test_an_unknown_source_type_is_refused() {
    // GIVEN
    let dir = tempfile::tempdir().unwrap();
    let repo = repository(&dir).await;
    let (handle, _bus) = spawn(
        repo.clone(),
        Arc::new(SourceRegistry::new()),
        Arc::new(Scripted::default()),
    );

    // WHEN
    let err = handle.launch(launch(None, None)).await.unwrap_err();

    // THEN
    assert!(
        matches!(err, BatchError::Source(SourceError::UnknownKind { ref kind }) if kind == "memory"),
        "{err:?}"
    );
    assert!(repo.list_batches(10).await.unwrap().is_empty());
}

// GIVEN launches with a concurrency of 0 and of 17, and one on an agent that
//       cannot run a batch
// WHEN each is launched
// THEN each is refused before anything is recorded
#[tokio::test]
async fn test_invalid_launches_are_refused() {
    // GIVEN
    let dir = tempfile::tempdir().unwrap();
    let repo = repository(&dir).await;
    let (handle, _bus) = spawn(
        repo.clone(),
        registry(&["ok"]),
        Arc::new(Scripted::default()),
    );
    let mut stranger = launch(None, None);
    stranger.agent = "stranger".into();

    // WHEN
    let zero = handle.launch(launch(Some(0), None)).await.unwrap_err();
    let many = handle
        .launch(launch(Some(MAX_CONCURRENCY + 1), None))
        .await
        .unwrap_err();
    let agent = handle.launch(stranger).await.unwrap_err();

    // THEN
    assert!(matches!(zero, BatchError::Invalid(_)), "{zero:?}");
    assert!(matches!(many, BatchError::Invalid(_)), "{many:?}");
    assert!(matches!(agent, BatchError::Agent(_)), "{agent:?}");
    assert!(repo.list_batches(10).await.unwrap().is_empty());
}

// GIVEN an item with two fields
// WHEN its message is built
// THEN each field is on its own line
#[test]
fn test_item_message_lists_the_fields() {
    // GIVEN
    let input = serde_json::json!({"client": "Durand", "request": "Refund order 12"});

    // WHEN
    let message = item_message(&input);

    // THEN
    assert_eq!(message, "client: Durand\nrequest: Refund order 12");
}

#[test]
fn test_the_access_an_item_waits_for_is_read_back_from_its_output() {
    // GIVEN the output an item that waits for a connector tool is recorded with
    let output = awaiting_output("conn:google:ada@example.com/send_email");

    // WHEN the access is read back from it, and from outputs it did not write
    let read = awaited_access(&output);

    // THEN the access comes back whole, and nothing comes from anything else
    assert_eq!(read, Some("conn:google:ada@example.com/send_email"));
    assert_eq!(awaited_access("sent"), None);
    assert_eq!(awaited_access(&awaiting_output("")), None);
}
