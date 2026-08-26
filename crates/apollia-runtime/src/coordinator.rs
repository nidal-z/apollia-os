use std::sync::Arc;
use std::time::Instant;

use apollia_core::{
    AIPPart, AIPResult, AIPTask, AgentId, ObservabilityConfig, RuntimeEvent, TaskId, TaskStatus,
};
use apollia_tools::TaskRepository;
use tokio::sync::Semaphore;

use crate::eventbus::EventBusSender;

/// Abstracts the execution backend (ORIA or a mock for tests).
///
/// An injectable trait that decouples the coordinator from the concrete
/// execution engine, mirroring the pattern used by `ToolExecutor` and
/// `AgentRunner`.
pub trait ExecutionBackend: Send + Sync + 'static {
    /// Executes a task and returns its result.
    fn execute(
        &self,
        task: AIPTask,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<AIPResult, String>> + Send>>;
}

/// Type-erased execution backend for dynamic dispatch.
///
/// Wraps `Arc<dyn ExecutionBackend + Send + Sync>` and implements `Clone`
/// cheaply (atomic reference count increment). Used in production so that
/// different agents can each have a different concrete backend while sharing
/// the same generic `TaskRouter<DynBackend>`.
#[derive(Clone)]
pub struct DynBackend(pub Arc<dyn ExecutionBackend + Send + Sync + 'static>);

impl DynBackend {
    /// Wraps any `ExecutionBackend` implementation in a `DynBackend`.
    pub fn new<B: ExecutionBackend + Send + Sync + 'static>(backend: B) -> Self {
        Self(Arc::new(backend))
    }
}

impl ExecutionBackend for DynBackend {
    fn execute(
        &self,
        task: AIPTask,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<AIPResult, String>> + Send>>
    {
        self.0.execute(task)
    }
}

/// Errors from the execution coordinator.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum CoordinatorError {
    /// Concurrency limit reached for this agent.
    #[error("concurrency limit reached for agent '{0}' (max_concurrent_tasks)")]
    ConcurrencyLimitReached(AgentId),

    /// Task execution failed.
    #[error("execution failed: {0}")]
    ExecutionFailed(String),
}

/// Execution coordinator for one active agent.
///
/// Manages task concurrency through a Tokio semaphore. One coordinator is
/// created per active agent and owns its own semaphore. When a
/// [`TaskRepository`] is available, it persists observability data
/// (input, output, transitions, duration).
pub struct ExecutionCoordinator<B: ExecutionBackend> {
    agent_id: AgentId,
    /// Human-readable agent name (manifest.name), persisted in the observability database.
    agent_name: String,
    concurrency: Arc<Semaphore>,
    event_bus: EventBusSender,
    backend: Arc<B>,
    /// SQLite repository for observability persistence, `None` in tests.
    task_repo: Option<Arc<TaskRepository>>,
    /// Truncation configuration for observability.
    obs_config: ObservabilityConfig,
}

impl<B: ExecutionBackend> ExecutionCoordinator<B> {
    /// Creates a new coordinator for the given agent.
    ///
    /// # Arguments
    /// - `agent_id`: agent identifier
    /// - `max_concurrent`: maximum number of tasks running in parallel (default: 1)
    /// - `event_bus`: channel for emitting runtime events
    /// - `backend`: execution backend (ORIA or mock)
    pub fn new(
        agent_id: AgentId,
        max_concurrent: u32,
        event_bus: EventBusSender,
        backend: B,
    ) -> Self {
        Self {
            agent_id,
            agent_name: String::new(),
            concurrency: Arc::new(Semaphore::new(max_concurrent as usize)),
            event_bus,
            backend: Arc::new(backend),
            task_repo: None,
            obs_config: ObservabilityConfig::default(),
        }
    }

    /// Sets the observability repository used to persist task data.
    ///
    /// When configured, the coordinator automatically persists the input,
    /// output, state transitions, and duration of each task.
    pub fn with_task_repository(
        mut self,
        repo: Arc<TaskRepository>,
        config: ObservabilityConfig,
    ) -> Self {
        self.task_repo = Some(repo);
        self.obs_config = config;
        self
    }

    /// Sets the human-readable agent name (manifest.name).
    ///
    /// Persisted in the `agent_name` column of the `tasks` table so history
    /// can still be displayed after a runtime restart.
    pub fn with_agent_name(mut self, name: String) -> Self {
        self.agent_name = name;
        self
    }

    /// Submits a task for execution.
    ///
    /// Tries to acquire a semaphore permit:
    /// - On success: spawns a Tokio task, emits `TaskStarted`, returns a `JoinHandle`
    /// - When the semaphore is full: returns `ConcurrencyLimitReached`
    ///
    /// The permit is released automatically when the spawned task finishes
    /// (the `OwnedSemaphorePermit` is dropped inside the closure).
    pub fn submit_task(
        &self,
        task: AIPTask,
    ) -> Result<tokio::task::JoinHandle<Result<AIPResult, CoordinatorError>>, CoordinatorError>
    {
        let permit = Arc::clone(&self.concurrency)
            .try_acquire_owned()
            .map_err(|_| CoordinatorError::ConcurrencyLimitReached(self.agent_id.clone()))?;

        let ctx = SubmittedTaskCtx {
            agent_id: self.agent_id.clone(),
            agent_name_for_db: self.agent_name.clone(),
            event_bus: self.event_bus.clone(),
            task_id: TaskId::from(task.task_id.clone()),
            backend: Arc::clone(&self.backend),
            task_repo: self.task_repo.clone(),
            obs_config: self.obs_config.clone(),
            // Extract the input text for observability persistence.
            input_text: aip_input_to_text(&task.input),
            task,
        };

        let handle = tokio::spawn(async move { run_submitted_task(ctx, permit).await });

        Ok(handle)
    }

    /// Returns the number of available permits (tasks that can still be accepted).
    pub fn available_permits(&self) -> usize {
        self.concurrency.available_permits()
    }
}

/// Context captured to run a submitted task inside the spawned Tokio task.
struct SubmittedTaskCtx<B: ExecutionBackend> {
    agent_id: AgentId,
    agent_name_for_db: String,
    event_bus: EventBusSender,
    task_id: TaskId,
    backend: Arc<B>,
    task_repo: Option<Arc<TaskRepository>>,
    obs_config: ObservabilityConfig,
    input_text: String,
    task: AIPTask,
}

/// Runs a submitted task: observability persistence, lifecycle event
/// emission, and execution through the backend.
///
/// The `permit` is consumed here and released on drop when the function ends.
async fn run_submitted_task<B: ExecutionBackend>(
    ctx: SubmittedTaskCtx<B>,
    permit: tokio::sync::OwnedSemaphorePermit,
) -> Result<AIPResult, CoordinatorError> {
    // The permit is moved into the function and released on drop.
    let _permit = permit;

    let SubmittedTaskCtx {
        agent_id,
        agent_name_for_db,
        event_bus,
        task_id,
        backend,
        task_repo,
        obs_config,
        input_text,
        task,
    } = ctx;

    let started_at = Instant::now();

    // Observability: persist input, agent_name, and the "submitted" transition.
    persist_submission(
        task_repo.as_deref(),
        &task_id,
        &input_text,
        &agent_name_for_db,
        &obs_config,
    )
    .await;

    // Record the run_id so a task_id can be resolved to it (`audit verify`).
    if let (Some(repo), Some(run_id)) = (task_repo.as_deref(), task.run_id.as_ref()) {
        if let Err(e) = repo.set_run_id(task_id.as_str(), run_id.as_str()).await {
            tracing::warn!(task_id = %task_id, error = %e, "task.run_id.persist.failed");
        }
    }

    let _ = event_bus.send(RuntimeEvent::TaskStarted {
        agent_id: agent_id.clone(),
        task_id: task_id.clone(),
    });

    // Observability: persist the "running" transition.
    if let Some(repo) = task_repo.as_deref() {
        if let Err(e) = repo
            .append_transition(task_id.as_str(), "running", &now_rfc3339())
            .await
        {
            tracing::warn!(task_id = %task_id, error = %e, "task.run.transition.persist.failed");
        }
    }

    let result = backend.execute(task).await;

    let elapsed_ms = started_at.elapsed().as_millis() as i64;

    let is_success = task_is_success(&result, &agent_id, &task_id);
    let output = build_output_text(&result);

    // Observability: persist output, the terminal transition, and duration.
    persist_completion(CompletionRecord {
        repo: task_repo.as_deref(),
        task_id: &task_id,
        output: output.as_deref(),
        is_success,
        elapsed_ms,
        obs_config: &obs_config,
    })
    .await;

    let _ = event_bus.send(RuntimeEvent::TaskCompleted {
        agent_id,
        task_id,
        success: is_success,
        output,
    });

    result.map_err(CoordinatorError::ExecutionFailed)
}

/// Persists the input, the agent name, and the "submitted" transition.
async fn persist_submission(
    repo: Option<&TaskRepository>,
    task_id: &TaskId,
    input_text: &str,
    agent_name_for_db: &str,
    obs_config: &ObservabilityConfig,
) {
    let Some(repo) = repo else { return };
    if let Err(e) = repo
        .save_input(task_id.as_str(), input_text, obs_config)
        .await
    {
        tracing::warn!(task_id = %task_id, error = %e, "task.input.persist.failed");
    }
    if !agent_name_for_db.is_empty() {
        if let Err(e) = repo
            .set_agent_name(task_id.as_str(), agent_name_for_db)
            .await
        {
            tracing::warn!(task_id = %task_id, error = %e, "task.agent_name.persist.failed");
        }
    }
    if let Err(e) = repo
        .append_transition(task_id.as_str(), "submitted", &now_rfc3339())
        .await
    {
        tracing::warn!(task_id = %task_id, error = %e, "task.submit.transition.persist.failed");
    }
}

/// Inputs for [`persist_completion`].
struct CompletionRecord<'a> {
    repo: Option<&'a TaskRepository>,
    task_id: &'a TaskId,
    output: Option<&'a str>,
    is_success: bool,
    elapsed_ms: i64,
    obs_config: &'a ObservabilityConfig,
}

/// Persists the output, the terminal transition, and the duration.
async fn persist_completion(record: CompletionRecord<'_>) {
    let CompletionRecord {
        repo,
        task_id,
        output,
        is_success,
        elapsed_ms,
        obs_config,
    } = record;
    let Some(repo) = repo else { return };
    let terminal_status = if is_success { "completed" } else { "failed" };

    if let Some(output_text) = output {
        if let Err(e) = repo
            .save_output(task_id.as_str(), output_text, obs_config)
            .await
        {
            tracing::warn!(task_id = %task_id, error = %e, "task.output.persist.failed");
        }
    }
    if let Err(e) = repo
        .append_transition(task_id.as_str(), terminal_status, &now_rfc3339())
        .await
    {
        tracing::warn!(task_id = %task_id, error = %e, "task.terminal.transition.persist.failed");
    }
    if let Err(e) = repo.set_duration(task_id.as_str(), elapsed_ms).await {
        tracing::warn!(task_id = %task_id, error = %e, "task.duration.persist.failed");
    }
}

/// Determines whether the task succeeded.
///
/// `is_success` must reflect the status reported on the Python side, not just
/// the success of the Rust call. An `Ok(AIPResult { status: Failed, .. })`
/// means the agent explicitly signaled a failure and must propagate
/// `success=false`. A result carrying a populated `error` envelope is likewise
/// a failure even when its `status` was not set to `Failed` (e.g. a NO_HANDLER
/// result whose status did not survive the bridge), so callers never see a
/// failed task reported as `completed`.
fn task_is_success(
    result: &Result<AIPResult, String>,
    agent_id: &AgentId,
    task_id: &TaskId,
) -> bool {
    match result {
        Ok(aip_result) => aip_result.status != TaskStatus::Failed && aip_result.error.is_none(),
        Err(e) => {
            tracing::error!(
                agent_id = %agent_id,
                task_id  = %task_id,
                error    = %e,
                "task.execution.failed"
            );
            false
        }
    }
}

/// Builds the output text persisted and emitted for the task.
///
/// For `Err`, the error string is surfaced. For `Ok(failed)`, the worker's
/// code, message, and details are included so the real cause is visible.
fn build_output_text(result: &Result<AIPResult, String>) -> Option<String> {
    match result {
        Ok(aip_result) => {
            if aip_result.status == TaskStatus::Failed || aip_result.error.is_some() {
                let err_text = aip_result
                    .error
                    .as_ref()
                    .map(|e| {
                        let details_str = e
                            .details
                            .as_ref()
                            .map(|d| format!(" details={d}"))
                            .unwrap_or_default();
                        format!("[{}] {}{}", e.code, e.message, details_str)
                    })
                    .unwrap_or_else(|| "(worker failed without error details)".to_string());
                Some(err_text)
            } else {
                Some(aip_result_to_text(aip_result)).filter(|s| !s.is_empty())
            }
        }
        // A failing runner/backend must never yield an empty reason (a blank
        // error is impossible to root-cause, especially under concurrency).
        Err(e) if e.trim().is_empty() => {
            Some("task failed without an error message (runner returned no detail)".to_string())
        }
        Err(e) => Some(e.to_string()),
    }
}

/// Concatenates the text parts of an [`AIPInput`] into a single `String`.
///
/// Only [`AIPPart::Text`] parts contribute; file and data parts are ignored.
/// Returns an empty string when the input contains no text parts.
fn aip_input_to_text(input: &apollia_core::AIPInput) -> String {
    input
        .parts
        .iter()
        .filter_map(|part| {
            if let AIPPart::Text(tp) = part {
                Some(tp.text.as_str())
            } else {
                None
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Returns the current instant formatted as RFC 3339, without a chrono dependency.
fn now_rfc3339() -> String {
    let now = std::time::SystemTime::now();
    let duration = now
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = duration.as_secs();
    let days = secs / 86400;
    let time_secs = secs % 86400;
    let hours = time_secs / 3600;
    let minutes = (time_secs % 3600) / 60;
    let seconds = time_secs % 60;

    // Date from epoch days using the civil-from-days algorithm.
    let z = days + 719468;
    let era = z / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };

    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        y, m, d, hours, minutes, seconds
    )
}

/// Flattens the parts of an [`AIPResult`] into a single `String` for the
/// `output` field of [`RuntimeEvent::TaskCompleted`].
///
/// Strategy:
/// - [`AIPPart::Text`] parts are concatenated with a single space separator.
/// - When the result contains no `Text` part but does contain `Data`
///   parts, the data is serialised as JSON so the caller (A2A invoker,
///   audit trail, CLI) can still read the structured return value.
/// - When both `Text` and `Data` are present, only the text is returned
///   (callers that want the structured payload should consult the
///   `AIPResult` directly, not the flattened text).
/// - File parts are ignored.
fn aip_result_to_text(result: &AIPResult) -> String {
    let texts: Vec<&str> = result
        .output
        .iter()
        .filter_map(|part| {
            if let AIPPart::Text(tp) = part {
                Some(tp.text.as_str())
            } else {
                None
            }
        })
        .collect();
    if !texts.is_empty() {
        return texts.join(" ");
    }

    let data_values: Vec<&serde_json::Value> = result
        .output
        .iter()
        .filter_map(|part| {
            if let AIPPart::Data(dp) = part {
                Some(&dp.data)
            } else {
                None
            }
        })
        .collect();
    if data_values.is_empty() {
        return String::new();
    }
    if data_values.len() == 1 {
        return serde_json::to_string(data_values[0]).unwrap_or_default();
    }
    serde_json::to_string(&data_values).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use apollia_core::{AIPInput, TaskStatus};
    use std::sync::atomic::{AtomicBool, Ordering};
    use tokio::sync::{broadcast, watch};

    /// Mock backend returning a configurable result.
    struct MockBackend {
        should_fail: AtomicBool,
        /// When set, `execute` blocks until the test flips the gate to `true`.
        /// This holds the coordinator's concurrency permit in-flight with no
        /// wall-clock delay, so concurrency-limit assertions are deterministic.
        gate: Option<watch::Receiver<bool>>,
    }

    impl MockBackend {
        fn success() -> Self {
            Self {
                should_fail: AtomicBool::new(false),
                gate: None,
            }
        }

        /// Backend whose task stays in-flight until `gate` is released to `true`.
        fn gated(gate: watch::Receiver<bool>) -> Self {
            Self {
                should_fail: AtomicBool::new(false),
                gate: Some(gate),
            }
        }

        fn failing() -> Self {
            Self {
                should_fail: AtomicBool::new(true),
                gate: None,
            }
        }
    }

    impl ExecutionBackend for MockBackend {
        fn execute(
            &self,
            task: AIPTask,
        ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<AIPResult, String>> + Send>>
        {
            let fail = self.should_fail.load(Ordering::SeqCst);
            let gate = self.gate.clone();
            Box::pin(async move {
                if let Some(mut gate) = gate {
                    let _ = gate.wait_for(|released| *released).await;
                }
                if fail {
                    Err("mock execution failure".to_string())
                } else {
                    Ok(AIPResult {
                        task_id: task.task_id,
                        status: TaskStatus::Completed,
                        output: vec![],
                        error: None,
                        artifacts: vec![],
                        input_required_data: None,
                    })
                }
            })
        }
    }

    fn make_task(id: &str) -> AIPTask {
        AIPTask {
            task_id: id.to_string(),
            context_id: "ctx-test".to_string(),
            input: AIPInput::default(),
            history: vec![],
            timeout_seconds: None,
            ..AIPTask::default()
        }
    }

    #[tokio::test]
    async fn test_submit_task_returns_join_handle() {
        // GIVEN a coordinator with max_concurrent=1
        let (tx, _rx) = broadcast::channel::<RuntimeEvent>(64);
        let coord = ExecutionCoordinator::new("agent-1".into(), 1, tx, MockBackend::success());

        // WHEN a task is submitted
        let handle = coord.submit_task(make_task("task-1"));

        // THEN submit_task returns Ok(JoinHandle)
        assert!(handle.is_ok());
        let result = handle.unwrap().await.expect("join failed");
        assert!(result.is_ok());
        assert_eq!(result.unwrap().task_id, "task-1");
    }

    #[tokio::test]
    async fn test_concurrency_limit_sequential() {
        // GIVEN a coordinator with max_concurrent=1 and a gated backend
        let (tx, _rx) = broadcast::channel::<RuntimeEvent>(64);
        let (gate_tx, gate_rx) = watch::channel(false);
        let coord = ExecutionCoordinator::new("agent-1".into(), 1, tx, MockBackend::gated(gate_rx));

        // AND a task already running, parked in-flight on the gate so it keeps
        // holding the single permit
        let h1 = coord
            .submit_task(make_task("task-1"))
            .expect("first submit should succeed");

        // WHEN a second task is submitted
        let result = coord.submit_task(make_task("task-2"));

        // THEN it returns ConcurrencyLimitReached
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            matches!(&err, CoordinatorError::ConcurrencyLimitReached(id) if id == "agent-1"),
            "expected ConcurrencyLimitReached, got: {err:?}"
        );

        // Release the gate so the in-flight task finishes cleanly
        gate_tx.send(true).expect("release gate");
        h1.await.expect("join").expect("execution");
    }

    #[tokio::test]
    async fn test_concurrency_limit_parallel() {
        // GIVEN a coordinator with max_concurrent=3 and a gated backend
        let (tx, _rx) = broadcast::channel::<RuntimeEvent>(64);
        let (gate_tx, gate_rx) = watch::channel(false);
        let coord = ExecutionCoordinator::new("agent-1".into(), 3, tx, MockBackend::gated(gate_rx));

        // WHEN 3 tasks are submitted at once, all parked in-flight on the gate
        let h1 = coord.submit_task(make_task("task-1"));
        let h2 = coord.submit_task(make_task("task-2"));
        let h3 = coord.submit_task(make_task("task-3"));

        // THEN all are accepted
        assert!(h1.is_ok());
        assert!(h2.is_ok());
        assert!(h3.is_ok());

        // AND a 4th returns ConcurrencyLimitReached
        let h4 = coord.submit_task(make_task("task-4"));
        assert!(h4.is_err());
        assert!(matches!(
            h4.unwrap_err(),
            CoordinatorError::ConcurrencyLimitReached(_)
        ));

        // Release the gate so the in-flight tasks finish cleanly
        gate_tx.send(true).expect("release gate");
        for h in [h1, h2, h3] {
            h.expect("submit ok")
                .await
                .expect("join")
                .expect("execution");
        }
    }

    #[tokio::test]
    async fn test_task_started_event_emitted() {
        // GIVEN a coordinator and a receiver on the EventBus
        let (tx, mut rx) = broadcast::channel::<RuntimeEvent>(64);
        let coord = ExecutionCoordinator::new("agent-1".into(), 1, tx, MockBackend::success());

        // WHEN a task is submitted
        let handle = coord
            .submit_task(make_task("task-42"))
            .expect("submit should succeed");
        handle
            .await
            .expect("join failed")
            .expect("execution failed");

        // THEN a RuntimeEvent::TaskStarted is received with the right agent_id and task_id
        let event = rx.recv().await.expect("should receive TaskStarted");
        assert!(
            matches!(&event, RuntimeEvent::TaskStarted { agent_id, task_id }
                if agent_id == "agent-1" && task_id == "task-42"),
            "expected TaskStarted, got: {event:?}"
        );
    }

    #[tokio::test]
    async fn test_task_completed_event_emitted() {
        // GIVEN a coordinator and a receiver on the EventBus
        let (tx, mut rx) = broadcast::channel::<RuntimeEvent>(64);
        let coord = ExecutionCoordinator::new("agent-1".into(), 1, tx, MockBackend::success());

        // WHEN a task finishes (success)
        let handle = coord
            .submit_task(make_task("task-99"))
            .expect("submit should succeed");
        handle
            .await
            .expect("join failed")
            .expect("execution failed");

        // THEN a RuntimeEvent::TaskCompleted is received with success=true
        // (skip TaskStarted first)
        let _started = rx.recv().await.expect("should receive TaskStarted");
        let completed = rx.recv().await.expect("should receive TaskCompleted");
        assert!(
            matches!(&completed, RuntimeEvent::TaskCompleted { agent_id, task_id, success, .. }
                if agent_id == "agent-1" && task_id == "task-99" && *success),
            "expected TaskCompleted with success=true, got: {completed:?}"
        );
    }

    #[tokio::test]
    async fn test_permit_released_on_failure() {
        // GIVEN a coordinator with max_concurrent=1
        let (tx, mut rx) = broadcast::channel::<RuntimeEvent>(64);
        let coord = ExecutionCoordinator::new("agent-1".into(), 1, tx, MockBackend::failing());

        // AND a task that fails
        let handle = coord
            .submit_task(make_task("task-fail"))
            .expect("submit should succeed");
        let result = handle.await.expect("join failed");
        assert!(result.is_err(), "execution should have failed");

        // THEN available_permits() == 1 (permit released)
        assert_eq!(coord.available_permits(), 1);

        // AND a new task can be submitted
        let handle2 = coord.submit_task(make_task("task-retry"));
        assert!(handle2.is_ok(), "should be able to submit after failure");

        // AND TaskCompleted with success=false was emitted
        let _started = rx.recv().await.expect("should receive TaskStarted");
        let completed = rx.recv().await.expect("should receive TaskCompleted");
        assert!(
            matches!(&completed, RuntimeEvent::TaskCompleted { success, .. } if !success),
            "expected TaskCompleted with success=false, got: {completed:?}"
        );
    }

    /// When the backend returns `Ok(AIPResult { status: Failed })` the coordinator
    /// must emit `TaskCompleted { success: false }`.  Without this, a Python agent
    /// that returns `{"status": "failed"}` would silently appear as "completed".
    #[tokio::test]
    async fn test_python_level_failure_is_success_false() {
        // GIVEN a backend that returns Ok(AIPResult { status: Failed })
        struct AgentLevelFailBackend;
        impl ExecutionBackend for AgentLevelFailBackend {
            fn execute(
                &self,
                task: AIPTask,
            ) -> std::pin::Pin<
                Box<dyn std::future::Future<Output = Result<AIPResult, String>> + Send>,
            > {
                Box::pin(async move {
                    Ok(AIPResult {
                        task_id: task.task_id,
                        status: TaskStatus::Failed,
                        output: vec![],
                        error: Some(apollia_core::AIPError {
                            code: "MISSING_INPUT".into(),
                            message: "no text part".into(),
                            details: None,
                        }),
                        artifacts: vec![],
                        input_required_data: None,
                    })
                })
            }
        }

        let (tx, mut rx) = broadcast::channel(16);
        let coord = ExecutionCoordinator::new("agent-42".into(), 1, tx, AgentLevelFailBackend);

        // WHEN
        let handle = coord
            .submit_task(make_task("task-agent-fail"))
            .expect("submit should succeed");
        let _ = handle.await.expect("join should succeed");

        // THEN TaskCompleted carries success=false
        let _started = rx.recv().await.expect("TaskStarted");
        let completed = rx.recv().await.expect("TaskCompleted");
        assert!(
            matches!(&completed, RuntimeEvent::TaskCompleted { success, .. } if !success),
            "agent-level Failed must propagate as success=false, got: {completed:?}"
        );
    }

    #[test]
    fn test_error_envelope_without_failed_status_is_not_success() {
        // GIVEN an Ok(AIPResult) that carries a NO_HANDLER error but whose
        // status did not survive the bridge as Failed (the T3b bug shape).
        let result: Result<AIPResult, String> = Ok(AIPResult {
            task_id: "task-x".to_string(),
            status: TaskStatus::Completed,
            output: vec![],
            error: Some(apollia_core::AIPError {
                code: "NO_HANDLER".to_string(),
                message: "agent has neither @skill nor @on_message handler".to_string(),
                details: None,
            }),
            artifacts: vec![],
            input_required_data: None,
        });
        let agent = AgentId::from("worker");
        let task = TaskId::from("task-x".to_string());

        // WHEN / THEN it is treated as a failure and the error text is surfaced.
        assert!(
            !task_is_success(&result, &agent, &task),
            "a result carrying an error envelope must not be a success"
        );
        let output = build_output_text(&result).expect("error text expected");
        assert!(
            output.contains("[NO_HANDLER]") && output.contains("handler"),
            "output should surface the error envelope, got: {output}"
        );
    }

    #[test]
    fn test_failed_task_never_has_empty_error() {
        // GIVEN a backend that failed with an empty error string (W4: a runner
        // returning no detail, e.g. under concurrency).
        let result: Result<AIPResult, String> = Err("   ".to_string());

        // WHEN building the output text
        let output = build_output_text(&result).expect("some output expected");

        // THEN a non-empty reason is always surfaced.
        assert!(!output.trim().is_empty(), "failed task must carry a reason");
        assert!(output.contains("without an error message"), "got: {output}");
    }

    #[test]
    fn test_clean_completed_result_is_success() {
        // GIVEN a normal completed result with no error envelope.
        let result: Result<AIPResult, String> = Ok(AIPResult {
            task_id: "task-y".to_string(),
            status: TaskStatus::Completed,
            output: vec![],
            error: None,
            artifacts: vec![],
            input_required_data: None,
        });
        let agent = AgentId::from("worker");
        let task = TaskId::from("task-y".to_string());

        // WHEN / THEN it stays a success.
        assert!(task_is_success(&result, &agent, &task));
    }
}
