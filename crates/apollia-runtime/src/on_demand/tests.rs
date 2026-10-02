use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, Weak};

use apollia_core::TaskStatus;

use super::*;

/// A backend that answers every task as completed.
struct Probe;

impl ExecutionBackend for Probe {
    fn execute(
        &self,
        task: AIPTask,
    ) -> Pin<Box<dyn Future<Output = Result<AIPResult, String>> + Send>> {
        Box::pin(async move {
            Ok(AIPResult {
                task_id: task.task_id,
                status: TaskStatus::Completed,
                output: vec![],
                error: None,
                artifacts: vec![],
                input_required_data: None,
            })
        })
    }
}

/// Counts the loads and keeps a weak reference to every backend it built,
/// so a test can tell whether a loaded backend is still referenced.
#[derive(Default)]
struct Loads {
    count: AtomicUsize,
    built: Mutex<Vec<Weak<dyn ExecutionBackend + Send + Sync>>>,
}

impl Loads {
    fn loader(self: &Arc<Self>) -> BackendLoader {
        let loads = Arc::clone(self);
        Arc::new(move || {
            loads.count.fetch_add(1, Ordering::SeqCst);
            let backend = DynBackend::new(Probe);
            loads.built.lock().unwrap().push(Arc::downgrade(&backend.0));
            backend
        })
    }

    fn count(&self) -> usize {
        self.count.load(Ordering::SeqCst)
    }

    fn live(&self) -> usize {
        self.built
            .lock()
            .unwrap()
            .iter()
            .filter(|weak| weak.strong_count() > 0)
            .count()
    }
}

fn task(id: &str) -> AIPTask {
    AIPTask {
        task_id: id.to_string(),
        ..AIPTask::default()
    }
}

const TEN_MINUTES: Duration = Duration::from_secs(600);

#[tokio::test]
async fn test_nothing_is_loaded_before_the_first_call() {
    // GIVEN an agent registered on demand
    let loads = Arc::new(Loads::default());
    let backend = OnDemandBackend::spawn("probe", loads.loader(), TEN_MINUTES);

    // WHEN nobody calls it
    let loaded = backend.check_idle_at(Instant::now()).await.unwrap();

    // THEN its loader never ran
    assert!(!loaded);
    assert_eq!(loads.count(), 0);
}

#[tokio::test]
async fn test_the_first_call_loads_and_the_next_ones_reuse() {
    // GIVEN an agent registered on demand
    let loads = Arc::new(Loads::default());
    let backend = OnDemandBackend::spawn("probe", loads.loader(), TEN_MINUTES);

    // WHEN it is called twice
    let first = backend.execute(task("t1")).await.unwrap();
    let second = backend.execute(task("t2")).await.unwrap();

    // THEN both calls succeed on a single load
    assert_eq!(first.status, TaskStatus::Completed);
    assert_eq!(second.status, TaskStatus::Completed);
    assert_eq!(loads.count(), 1);
}

#[tokio::test]
async fn test_an_agent_is_unloaded_after_ten_minutes_without_a_call() {
    // GIVEN an agent loaded by one call
    let loads = Arc::new(Loads::default());
    let backend = OnDemandBackend::spawn("probe", loads.loader(), TEN_MINUTES);
    backend.execute(task("t1")).await.unwrap();
    let called_at = Instant::now();

    // WHEN the clock reads nine minutes later
    let after_nine = backend
        .check_idle_at(called_at + Duration::from_secs(540))
        .await
        .unwrap();

    // THEN the agent is still loaded, and its backend still referenced
    assert!(after_nine);
    assert_eq!(loads.live(), 1);

    // WHEN the clock reads ten minutes and one second after the call
    let after_ten = backend
        .check_idle_at(called_at + Duration::from_secs(601))
        .await
        .unwrap();

    // THEN the agent is unloaded and nothing references its backend any more
    assert!(!after_ten);
    assert_eq!(loads.live(), 0);
}

#[tokio::test]
async fn test_a_call_after_an_unload_loads_again() {
    // GIVEN an agent unloaded after going idle
    let loads = Arc::new(Loads::default());
    let backend = OnDemandBackend::spawn("probe", loads.loader(), TEN_MINUTES);
    backend.execute(task("t1")).await.unwrap();
    let later = Instant::now() + Duration::from_secs(601);
    assert!(!backend.check_idle_at(later).await.unwrap());

    // WHEN it is called again
    let result = backend.execute(task("t2")).await.unwrap();

    // THEN the call succeeds on a second load
    assert_eq!(result.status, TaskStatus::Completed);
    assert_eq!(loads.count(), 2);
}

#[tokio::test]
async fn test_the_idle_delay_is_a_setting() {
    // GIVEN an agent whose idle delay is set to one minute
    let loads = Arc::new(Loads::default());
    let backend = OnDemandBackend::spawn("probe", loads.loader(), Duration::from_secs(60));
    backend.execute(task("t1")).await.unwrap();

    // WHEN the clock reads two minutes later
    let loaded = backend
        .check_idle_at(Instant::now() + Duration::from_secs(120))
        .await
        .unwrap();

    // THEN the agent is already unloaded
    assert!(!loaded);
    assert_eq!(loads.live(), 0);
}

#[tokio::test]
async fn test_unload_releases_the_backend_at_once() {
    // GIVEN an agent loaded by one call
    let loads = Arc::new(Loads::default());
    let backend = OnDemandBackend::spawn("probe", loads.loader(), TEN_MINUTES);
    backend.execute(task("t1")).await.unwrap();

    // WHEN it is parked
    backend.unload();

    // THEN its backend is released without waiting for the idle delay
    let loaded = backend.check_idle_at(Instant::now()).await.unwrap();
    assert!(!loaded);
    assert_eq!(loads.live(), 0);
}
