//! `TriggerEngine`: central Tokio actor of the trigger system.
//!
//! The `TriggerEngine` is a standard Tokio actor: an internal struct, a
//! clonable handle exposed via [`TriggerEngineHandle`], and a `run_loop` in a
//! `tokio::spawn`. Sources send [`crate::TriggerEvent`]s on the internal
//! channel; the engine evaluates the [`crate::OnBusyPolicy`], renders the input
//! template, and submits a task to the [`TaskSubmitter`].
//!
//! This module does NOT implement:
//! - The concrete sources (`CronTrigger`, `FileWatchTrigger`)
//! - The webhook route
//! - The real SQLite persistence

mod commands;
mod dispatch;
mod handle;

use std::collections::{HashMap, VecDeque};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use tokio::sync::{mpsc, oneshot};

use apollia_core::{truncate_with_marker, AIPInput, EventBusSender, ObservabilityConfig, TaskId};

use crate::persistence::TriggerPersistence;
use crate::sources::spawn_source;
use crate::types::{TriggerDefinition, TriggerEvent, TriggerPayload, TriggerSourceConfig};

// --- TaskSubmitter trait -----------------------------------------------------

/// Abstraction over `TaskRouterHandle` for submitting tasks from the `TriggerEngine`.
///
/// Keeps the `apollia-triggers` crate independent of `apollia-runtime`, which
/// avoids circular dependencies (the same pattern as `ToolExecutor` and
/// `AgentRunner`). The concrete `TaskRouterHandle<B>` implements this trait when
/// integrated with the Supervisor.
pub trait TaskSubmitter: Send + Sync + 'static {
    /// Submits a task for the given agent.
    ///
    /// Returns the generated `TaskId` if submission succeeds, or an error
    /// message as a `String` on failure.
    fn submit<'a>(
        &'a self,
        agent: &'a str,
        input: AIPInput,
    ) -> Pin<Box<dyn Future<Output = Result<TaskId, String>> + Send + 'a>>;

    /// Returns the number of pending or active tasks for the given agent.
    ///
    /// Used by [`OnBusyPolicy::Skip`] and [`OnBusyPolicy::Queue`] to decide the
    /// behavior when the agent is busy.
    fn pending_count<'a>(
        &'a self,
        agent: &'a str,
    ) -> Pin<Box<dyn Future<Output = usize> + Send + 'a>>;
}

// --- Queue types -------------------------------------------------------------

/// Trigger queued in an agent's bounded FIFO queue.
pub(crate) struct QueuedTriggerEvent {
    pub(crate) trigger_id: String,
    pub(crate) payload: TriggerPayload,
    /// Timestamp of entry into the queue, distinct from the source's `fired_at`.
    pub(crate) queued_at: DateTime<Utc>,
}

/// Bounded FIFO queue per agent for `OnBusyPolicy::Queue`.
///
/// The maximum size is configured via `[triggers] queue_max_depth` in
/// `apollia.toml`. When `max_depth == 0`, the queue is unbounded.
pub(crate) struct AgentQueue {
    pub(crate) inner: VecDeque<QueuedTriggerEvent>,
    pub(crate) max_depth: usize,
}

impl AgentQueue {
    /// Creates a new queue with the given maximum capacity.
    pub(crate) fn new(max_depth: usize) -> Self {
        Self {
            inner: VecDeque::new(),
            max_depth,
        }
    }

    /// Attempts to push an event onto the back of the queue.
    ///
    /// Returns `true` if the push succeeded, `false` if the queue is full
    /// (`max_depth > 0` and `len >= max_depth`).
    pub(crate) fn try_push(&mut self, event: QueuedTriggerEvent) -> bool {
        if self.max_depth > 0 && self.inner.len() >= self.max_depth {
            return false;
        }
        self.inner.push_back(event);
        true
    }

    /// Removes and returns the oldest event (FIFO).
    pub(crate) fn pop(&mut self) -> Option<QueuedTriggerEvent> {
        self.inner.pop_front()
    }

    /// Returns the number of elements currently queued.
    pub(crate) fn len(&self) -> usize {
        self.inner.len()
    }
}

// --- TriggerCommand ----------------------------------------------------------

/// Commands sent to the `TriggerEngine` via its handle.
pub(crate) enum TriggerCommand {
    /// Finds a webhook trigger by ID.
    FindWebhook {
        id: String,
        reply: oneshot::Sender<Option<TriggerDefinition>>,
    },
    /// Sends a webhook event to the engine (fire-and-forget).
    SendWebhookEvent {
        trigger_id: String,
        body: String,
        headers: HashMap<String, String>,
    },
    /// Forces a trigger to fire immediately.
    FireNow {
        id: String,
        reply: oneshot::Sender<Result<TaskId, TriggerEngineError>>,
    },
    /// Enables a trigger.
    Enable {
        id: String,
        reply: oneshot::Sender<Result<(), TriggerEngineError>>,
    },
    /// Disables a trigger.
    Disable {
        id: String,
        reply: oneshot::Sender<Result<(), TriggerEngineError>>,
    },
    /// Lists all triggers with their current status.
    List {
        reply: oneshot::Sender<Vec<TriggerStatus>>,
    },
    /// Returns the full definition of a trigger by ID.
    GetDefinition {
        id: String,
        reply: oneshot::Sender<Option<TriggerDefinition>>,
    },
    /// Returns the SQLite history of a trigger.
    QueryHistory {
        trigger_id: String,
        limit: usize,
        reply: oneshot::Sender<Vec<crate::persistence::TriggerHistoryEntry>>,
    },
    /// Reloads the trigger definitions (hot reload).
    Reload {
        definitions: Vec<TriggerDefinition>,
        reply: oneshot::Sender<()>,
    },
    /// Notifies the engine that an agent has become idle.
    ///
    /// Triggers the FIFO drain of that agent's queue if it holds pending
    /// triggers.
    NotifyAgentFree {
        /// Identifier of the agent that became available.
        agent_id: String,
    },
    /// Stops the actor cleanly.
    Shutdown,
}

// --- Public types ------------------------------------------------------------

/// Observed state of a trigger, returned by [`TriggerEngineHandle::list`].
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TriggerStatus {
    /// Trigger identifier.
    pub id: String,
    /// Target agent name.
    pub agent: String,
    /// Source type (`"cron"` | `"interval"` | `"file_watch"` | `"webhook"` | `"oneshot"`).
    pub source_kind: String,
    /// Source configuration detail (e.g. cron expression, interval, path).
    pub source_config: String,
    /// Whether the trigger is active.
    pub enabled: bool,
    /// Total number of successful fires since startup.
    pub fire_count: u64,
    /// Total number of skips since startup.
    pub skip_count: u64,
    /// Timestamp of the last fire (None if never fired).
    pub last_fired: Option<DateTime<Utc>>,
}

/// Errors from the `TriggerEngine`.
#[derive(thiserror::Error, Debug)]
#[non_exhaustive]
pub enum TriggerEngineError {
    /// No trigger found for the given identifier.
    #[error("trigger '{id}' not found")]
    NotFound {
        /// Identifier not found.
        id: String,
    },

    /// The trigger is already disabled.
    #[error("trigger '{id}' already disabled")]
    AlreadyDisabled {
        /// Trigger identifier.
        id: String,
    },

    /// The trigger is already enabled.
    #[error("trigger '{id}' already enabled")]
    AlreadyEnabled {
        /// Trigger identifier.
        id: String,
    },

    /// Task submission failed (TaskRouter error or Drop policy).
    #[error("submit failed: {0}")]
    SubmitFailed(String),
}

// --- TriggerEngine (internal actor) ------------------------------------------

/// Central `TriggerEngine` actor.
///
/// Receives [`TriggerEvent`]s from the sources, evaluates the [`OnBusyPolicy`],
/// renders the input template, and submits tasks to the [`TaskSubmitter`].
/// Never exposed directly; only reachable via [`TriggerEngineHandle`].
pub(crate) struct TriggerEngine {
    pub(crate) definitions: Vec<TriggerDefinition>,
    /// Internal sources-to-engine channel.
    ///
    /// Kept so it can be cloned and handed to new sources during hot reload
    /// ([`TriggerCommand::Reload`]).
    pub(crate) event_tx: mpsc::Sender<TriggerEvent>,
    pub(crate) task_router: Arc<dyn TaskSubmitter>,
    pub(crate) event_bus: EventBusSender,
    /// JoinHandles of the active sources, aborted on hot reload.
    pub(crate) handles: Vec<tokio::task::JoinHandle<()>>,
    pub(crate) fire_counts: HashMap<String, u64>,
    pub(crate) skip_counts: HashMap<String, u64>,
    pub(crate) last_fired: HashMap<String, DateTime<Utc>>,
    /// Bounded FIFO queues per agent, populated by `OnBusyPolicy::Queue`.
    ///
    /// Drained when the agent becomes available via [`TriggerCommand::NotifyAgentFree`].
    pub(crate) agent_queues: HashMap<String, AgentQueue>,
    /// SQLite persistence; `None` when not configured (e.g. unit tests).
    pub(crate) persistence: Option<TriggerPersistence>,
    /// Observability configuration for payload truncation.
    pub(crate) obs_config: ObservabilityConfig,
}

impl TriggerEngine {
    /// Starts the engine and returns its clonable handle.
    ///
    /// `persistence`: `None` disables SQLite persistence (useful for unit tests).
    /// `obs_config`: observability configuration for payload truncation.
    /// The sources in `definitions` are concrete implementations.
    pub async fn start<S: TaskSubmitter>(
        definitions: Vec<TriggerDefinition>,
        task_router: S,
        event_bus: EventBusSender,
        persistence: Option<TriggerPersistence>,
        obs_config: ObservabilityConfig,
    ) -> TriggerEngineHandle {
        let (event_tx, event_rx) = mpsc::channel::<TriggerEvent>(256);
        let (cmd_tx, cmd_rx) = mpsc::channel::<TriggerCommand>(64);

        // Spawn the sources for each active definition.
        let handles: Vec<tokio::task::JoinHandle<()>> = definitions
            .iter()
            .filter(|d| d.enabled)
            .map(|d| spawn_source(d.clone(), event_tx.clone()))
            .collect();

        // Restore counters from history so they survive runtime restarts.
        let (fire_counts, skip_counts, last_fired) = restore_counters(persistence.as_ref());

        let engine = TriggerEngine {
            definitions,
            event_tx: event_tx.clone(),
            task_router: Arc::new(task_router),
            event_bus,
            handles,
            fire_counts,
            skip_counts,
            last_fired,
            agent_queues: HashMap::new(),
            persistence,
            obs_config,
        };

        tokio::spawn(engine.run_loop(event_rx, cmd_rx));

        TriggerEngineHandle { tx: cmd_tx }
    }

    /// Main actor loop: selects over both events and commands.
    pub(crate) async fn run_loop(
        mut self,
        mut event_rx: mpsc::Receiver<TriggerEvent>,
        mut cmd_rx: mpsc::Receiver<TriggerCommand>,
    ) {
        loop {
            tokio::select! {
                Some(event) = event_rx.recv() => {
                    self.handle_event(event).await;
                }
                Some(cmd) = cmd_rx.recv() => {
                    if self.handle_command(cmd).await {
                        break;
                    }
                }
            }
        }
        // Abort the JoinHandles of the active sources.
        for handle in self.handles {
            handle.abort();
        }
        tracing::info!("trigger.engine.stopped");
    }

    /// Serializes the trigger payload to JSON and truncates if needed.
    ///
    /// Returns `None` if serialization fails (should not happen, since
    /// `TriggerPayload` implements `Serialize`).
    pub(crate) fn serialize_payload(&self, payload: &TriggerPayload) -> Option<String> {
        let json = serde_json::to_string(payload).ok()?;
        let (truncated, _) = truncate_with_marker(&json, self.obs_config.max_input_bytes);
        Some(truncated)
    }

    /// Persists a successful fire in `trigger_history` via [`TriggerPersistence`].
    ///
    /// If persistence is not configured or fails, a warning is logged without
    /// interrupting processing (fire-and-forget).
    pub(crate) async fn persist_fired(
        &mut self,
        event: &TriggerEvent,
        task_id: &TaskId,
        dispatch_ms: i64,
    ) {
        let payload_json = self.serialize_payload(&event.payload);
        if let Some(p) = self.persistence.as_mut() {
            if let Err(e) = p.record_fired(
                crate::persistence::TriggerRecord {
                    trigger_id: &event.trigger_id,
                    agent_name: &event.agent,
                    fired_at: event.fired_at,
                    payload_json: payload_json.as_deref(),
                },
                task_id.as_ref(),
                Some(dispatch_ms),
            ) {
                tracing::warn!(
                    trigger = %event.trigger_id,
                    error = %e,
                    "trigger.fire.persist.failed"
                );
            }
        } else {
            tracing::debug!(
                trigger = %event.trigger_id,
                task = %task_id,
                detail = "no persistence layer configured",
                "trigger.fired"
            );
        }
    }

    /// Persists a skip in `trigger_history` via [`TriggerPersistence`].
    pub(crate) async fn persist_skipped(&mut self, event: &TriggerEvent, reason: &str) {
        let payload_json = self.serialize_payload(&event.payload);
        if let Some(p) = self.persistence.as_mut() {
            if let Err(e) = p.record_skipped(
                crate::persistence::TriggerRecord {
                    trigger_id: &event.trigger_id,
                    agent_name: &event.agent,
                    fired_at: event.fired_at,
                    payload_json: payload_json.as_deref(),
                },
                reason,
            ) {
                tracing::warn!(
                    trigger = %event.trigger_id,
                    error = %e,
                    "trigger.skip.persist.failed"
                );
            }
        } else {
            tracing::debug!(
                trigger = %event.trigger_id,
                %reason,
                detail = "no persistence layer configured",
                "trigger.skipped"
            );
        }
    }

    /// Persists a submission error in `trigger_history` via [`TriggerPersistence`].
    pub(crate) async fn persist_error(&mut self, event: &TriggerEvent, error: &str) {
        let payload_json = self.serialize_payload(&event.payload);
        if let Some(p) = self.persistence.as_mut() {
            if let Err(e) = p.record_error(
                crate::persistence::TriggerRecord {
                    trigger_id: &event.trigger_id,
                    agent_name: &event.agent,
                    fired_at: event.fired_at,
                    payload_json: payload_json.as_deref(),
                },
                error,
            ) {
                tracing::warn!(
                    trigger = %event.trigger_id,
                    error = %e,
                    "trigger.error.persist.failed"
                );
            }
        } else {
            tracing::warn!(
                trigger = %event.trigger_id,
                %error,
                detail = "no persistence layer configured",
                "trigger.error"
            );
        }
    }
}

// --- Counter restoration -----------------------------------------------------

/// In-memory counters restored from the SQLite history.
type RestoredCounters = (
    HashMap<String, u64>,
    HashMap<String, u64>,
    HashMap<String, DateTime<Utc>>,
);

/// Restores the counters (`fire`, `skip`, `last_fired`) from persistence so they
/// survive runtime restarts.
///
/// Returns empty maps if persistence is absent or loading fails (the error is
/// logged).
fn restore_counters(persistence: Option<&TriggerPersistence>) -> RestoredCounters {
    let Some(p) = persistence else {
        return (HashMap::new(), HashMap::new(), HashMap::new());
    };
    let counters = match p.load_counters() {
        Ok(counters) => counters,
        Err(e) => {
            tracing::error!(
                error = %e,
                detail = "the counters start at zero",
                "trigger.counters.restore.failed"
            );
            return (HashMap::new(), HashMap::new(), HashMap::new());
        }
    };
    let mut fc: HashMap<String, u64> = HashMap::new();
    let mut sc: HashMap<String, u64> = HashMap::new();
    let mut lf: HashMap<String, DateTime<Utc>> = HashMap::new();
    for (id, stats) in counters {
        if stats.fire_count > 0 {
            fc.insert(id.clone(), stats.fire_count);
        }
        if stats.skip_count > 0 {
            sc.insert(id.clone(), stats.skip_count);
        }
        if let Some(ts) = stats.last_fired {
            lf.insert(id, ts);
        }
    }
    (fc, sc, lf)
}

// --- source_kind_str ---------------------------------------------------------

/// Returns the string representing a trigger's source type.
pub(crate) fn source_kind_str(source: &TriggerSourceConfig) -> String {
    match source {
        TriggerSourceConfig::Cron { .. } => "cron",
        TriggerSourceConfig::Interval { .. } => "interval",
        TriggerSourceConfig::Oneshot { .. } => "oneshot",
        TriggerSourceConfig::FileWatch { .. } => "file_watch",
        TriggerSourceConfig::Webhook { .. } => "webhook",
    }
    .to_string()
}

/// Returns the configuration detail of a source (cron expression, interval, path, etc.).
pub(crate) fn source_config_str(source: &TriggerSourceConfig) -> String {
    match source {
        TriggerSourceConfig::Cron { schedule } => schedule.clone(),
        TriggerSourceConfig::Interval { every } => every.clone(),
        TriggerSourceConfig::Oneshot { fire_at } => fire_at.to_rfc3339(),
        TriggerSourceConfig::FileWatch { path, .. } => path.display().to_string(),
        TriggerSourceConfig::Webhook { .. } => String::new(),
    }
}

// --- TriggerEngineHandle -----------------------------------------------------

/// Clonable handle for the `TriggerEngine`, injectable into `AppState<B>`.
///
/// `Clone + Send + Sync`, the same pattern as `AgentRegistryHandle` and
/// `TaskRouterHandle`. All methods are `async` and communicate with the actor
/// via `mpsc::Sender<TriggerCommand>`.
#[derive(Clone)]
pub struct TriggerEngineHandle {
    pub(crate) tx: mpsc::Sender<TriggerCommand>,
}

impl TriggerEngineHandle {}

// --- Tests -------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persistence::TriggerPersistence;
    use crate::types::OnBusyPolicy;
    use crate::types::{InputTemplate, TriggerSourceConfig};
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };
    use tokio::sync::broadcast;

    /// Builds a test `EventBusSender`.
    fn make_bus() -> EventBusSender {
        broadcast::channel(64).0
    }

    /// Builds a minimal `TriggerDefinition` for tests (agent trigger).
    fn make_definition(id: &str, on_busy: OnBusyPolicy) -> TriggerDefinition {
        TriggerDefinition {
            id: id.into(),
            agent: "test-agent".into(),
            enabled: true,
            on_busy,
            source: TriggerSourceConfig::Cron {
                schedule: "0 8 * * MON".into(),
            },
            input_template: InputTemplate("test {{scheduled_at}}".into()),
        }
    }

    // --- Mock TaskSubmitter ----------------------------------------------

    /// Mock `TaskSubmitter` for tests.
    struct MockTaskRouterHandle {
        calls: Arc<AtomicUsize>,
        should_fail: bool,
        pending: usize,
    }

    impl MockTaskRouterHandle {
        /// Creates a mock that always succeeds.
        fn new() -> (Self, Arc<AtomicUsize>) {
            let calls = Arc::new(AtomicUsize::new(0));
            (
                MockTaskRouterHandle {
                    calls: calls.clone(),
                    should_fail: false,
                    pending: 0,
                },
                calls,
            )
        }

        /// Same behavior as `new()`, named explicitly for counting tests.
        fn new_with_tracking() -> (Self, Arc<AtomicUsize>) {
            Self::new()
        }

        /// Creates a mock that always fails submission.
        fn new_always_fail() -> (Self, Arc<AtomicUsize>) {
            let calls = Arc::new(AtomicUsize::new(0));
            (
                MockTaskRouterHandle {
                    calls: calls.clone(),
                    should_fail: true,
                    pending: 0,
                },
                calls,
            )
        }

        /// Creates a mock that simulates a busy agent (pending_count > 0).
        fn new_with_pending(pending: usize) -> (Self, Arc<AtomicUsize>) {
            let calls = Arc::new(AtomicUsize::new(0));
            (
                MockTaskRouterHandle {
                    calls: calls.clone(),
                    should_fail: false,
                    pending,
                },
                calls,
            )
        }
    }

    impl TaskSubmitter for MockTaskRouterHandle {
        fn submit<'a>(
            &'a self,
            _agent: &'a str,
            _input: AIPInput,
        ) -> Pin<Box<dyn Future<Output = Result<TaskId, String>> + Send + 'a>> {
            let calls = self.calls.clone();
            let should_fail = self.should_fail;
            Box::pin(async move {
                calls.fetch_add(1, Ordering::SeqCst);
                if should_fail {
                    Err("mock failure".into())
                } else {
                    Ok(TaskId::new_v4())
                }
            })
        }

        fn pending_count<'a>(
            &'a self,
            _agent: &'a str,
        ) -> Pin<Box<dyn Future<Output = usize> + Send + 'a>> {
            let pending = self.pending;
            Box::pin(async move { pending })
        }
    }

    #[tokio::test]
    async fn test_start_empty_definitions() {
        // GIVEN an empty list of TriggerDefinition
        let (router, _) = MockTaskRouterHandle::new();
        // WHEN
        let handle = TriggerEngine::start(
            vec![],
            router,
            make_bus(),
            None,
            ObservabilityConfig::default(),
        )
        .await;
        // THEN list() returns an empty vec
        let list = handle.list().await;
        assert!(list.is_empty(), "expected an empty list, got {:?}", list);
    }

    #[tokio::test]
    async fn test_handle_event_queue_submits_task() {
        // GIVEN a trigger with OnBusyPolicy::Queue { max_depth: 10 } and a succeeding mock
        let def = make_definition("test-trigger", OnBusyPolicy::Queue { max_depth: 10 });
        let (router, calls) = MockTaskRouterHandle::new_with_tracking();
        let handle = TriggerEngine::start(
            vec![def],
            router,
            make_bus(),
            None,
            ObservabilityConfig::default(),
        )
        .await;
        // WHEN
        handle
            .fire_now("test-trigger")
            .await
            .expect("fire_now failed");
        // THEN submit was called exactly once
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    // --- (OnBusyPolicy::Skip) --------------------------------------------

    #[tokio::test]
    async fn test_drop_policy_skips_when_agent_busy() {
        // GIVEN a Drop trigger and a busy agent (pending_count = 1)
        let def = make_definition("busy-trigger", OnBusyPolicy::Skip);
        let (router, calls) = MockTaskRouterHandle::new_with_pending(1);
        let handle = TriggerEngine::start(
            vec![def],
            router,
            make_bus(),
            None,
            ObservabilityConfig::default(),
        )
        .await;
        // WHEN
        let result = handle.fire_now("busy-trigger").await;
        // THEN submit was NOT called
        assert_eq!(calls.load(Ordering::SeqCst), 0, "submit must not be called");
        // AND fire_now returns an error (SubmitFailed)
        assert!(
            matches!(result, Err(TriggerEngineError::SubmitFailed(_))),
            "expected SubmitFailed, got {:?}",
            result
        );
    }

    #[tokio::test]
    async fn test_fire_now_returns_task_id() {
        // GIVEN a registered trigger
        let def = make_definition("rapport-hebdo", OnBusyPolicy::Queue { max_depth: 10 });
        let (router, _) = MockTaskRouterHandle::new();
        let handle = TriggerEngine::start(
            vec![def],
            router,
            make_bus(),
            None,
            ObservabilityConfig::default(),
        )
        .await;
        // WHEN
        let result = handle.fire_now("rapport-hebdo").await;
        // THEN Ok(task_id)
        assert!(result.is_ok(), "expected Ok, got {:?}", result);
    }

    #[tokio::test]
    async fn test_fire_now_unknown_id_returns_error() {
        // GIVEN no registered trigger
        let (router, _) = MockTaskRouterHandle::new();
        let handle = TriggerEngine::start(
            vec![],
            router,
            make_bus(),
            None,
            ObservabilityConfig::default(),
        )
        .await;
        // WHEN
        let result = handle.fire_now("unknown-trigger").await;
        // THEN NotFound
        assert!(
            matches!(result, Err(TriggerEngineError::NotFound { .. })),
            "expected NotFound, got {:?}",
            result
        );
    }

    #[tokio::test]
    async fn test_enable_disable_toggle() {
        // GIVEN an active trigger
        let def = make_definition("factures", OnBusyPolicy::Skip);
        let (router, _) = MockTaskRouterHandle::new();
        let handle = TriggerEngine::start(
            vec![def],
            router,
            make_bus(),
            None,
            ObservabilityConfig::default(),
        )
        .await;

        // WHEN disable
        handle.disable("factures").await.expect("disable failed");
        let list = handle.list().await;
        // THEN enabled = false
        assert!(!list[0].enabled, "the trigger must be disabled");

        // WHEN re-enable
        handle.enable("factures").await.expect("enable failed");
        let list = handle.list().await;
        // THEN enabled = true
        assert!(list[0].enabled, "the trigger must be enabled again");
    }

    #[tokio::test]
    async fn test_submit_error_does_not_panic() {
        // GIVEN a trigger that always fails submission
        let def = make_definition("failing-trigger", OnBusyPolicy::Queue { max_depth: 10 });
        let (router, _) = MockTaskRouterHandle::new_always_fail();
        let handle = TriggerEngine::start(
            vec![def],
            router,
            make_bus(),
            None,
            ObservabilityConfig::default(),
        )
        .await;

        // WHEN must not panic
        let result = handle.fire_now("failing-trigger").await;

        // THEN the actor is still alive
        let list = handle.list().await;
        assert_eq!(list.len(), 1, "the actor must still answer");
        // fire_now returns Err(SubmitFailed) because submission failed
        assert!(
            matches!(result, Err(TriggerEngineError::SubmitFailed(_))),
            "expected SubmitFailed, got {:?}",
            result
        );
    }

    // --- Extra -----------------------------------------------------------

    #[tokio::test]
    async fn test_fire_count_increments_on_success() {
        // GIVEN a trigger
        let def = make_definition("compteur", OnBusyPolicy::Queue { max_depth: 10 });
        let (router, _) = MockTaskRouterHandle::new();
        let handle = TriggerEngine::start(
            vec![def],
            router,
            make_bus(),
            None,
            ObservabilityConfig::default(),
        )
        .await;

        // WHEN fire x 2
        handle
            .fire_now("compteur")
            .await
            .expect("first fire failed");
        handle
            .fire_now("compteur")
            .await
            .expect("second fire failed");

        // THEN the counter carries both fires, not just the last one
        let list = handle.list().await;
        assert_eq!(list[0].fire_count, 2, "fire_count must be 2");
    }

    #[tokio::test]
    async fn test_handle_is_clone_send_sync() {
        // THEN TriggerEngineHandle is Clone + Send + Sync (checked at compile time)
        // GIVEN the trigger engine handle, which callers clone across tasks
        // WHEN it is instantiated behind a Clone + Send + Sync bound
        fn assert_send_sync<T: Clone + Send + Sync>() {}
        assert_send_sync::<TriggerEngineHandle>();
    }

    // --- Hot reload ------------------------------------------------------

    /// reload() replaces all existing definitions.
    #[tokio::test]
    async fn test_reload_replaces_all_triggers() {
        // GIVEN an engine with 1 trigger
        let def1 = make_definition("trigger-1", OnBusyPolicy::Queue { max_depth: 10 });
        let (router, _) = MockTaskRouterHandle::new();
        let handle = TriggerEngine::start(
            vec![def1],
            router,
            make_bus(),
            None,
            ObservabilityConfig::default(),
        )
        .await;
        assert_eq!(handle.list().await.len(), 1);

        // WHEN reload with 2 new triggers
        let def2 = make_definition("trigger-2", OnBusyPolicy::Skip);
        let def3 = make_definition("trigger-3", OnBusyPolicy::Queue { max_depth: 10 });
        handle.reload(vec![def2, def3]).await;

        // THEN 2 triggers actifs, trigger-1 absent
        let list = handle.list().await;
        assert_eq!(list.len(), 2, "list() must return 2 triggers");
        assert!(
            !list.iter().any(|t| t.id == "trigger-1"),
            "trigger-1 must no longer be present"
        );
        assert!(list.iter().any(|t| t.id == "trigger-2"));
        assert!(list.iter().any(|t| t.id == "trigger-3"));
    }

    /// reload() emits RuntimeEvent::TriggersReloaded { count }.
    #[tokio::test]
    async fn test_triggers_reloaded_event_emitted() {
        // GIVEN a bus with an active subscriber
        let (bus_tx, mut bus_rx) = broadcast::channel::<apollia_core::RuntimeEvent>(64);
        let (router, _) = MockTaskRouterHandle::new();
        let handle =
            TriggerEngine::start(vec![], router, bus_tx, None, ObservabilityConfig::default())
                .await;

        // WHEN reload with 1 enabled trigger
        let def = make_definition("new-trigger", OnBusyPolicy::Queue { max_depth: 10 });
        handle.reload(vec![def]).await;

        // THEN TriggersReloaded { count: 1 } received within 500ms
        let received = tokio::time::timeout(std::time::Duration::from_millis(500), async {
            loop {
                match bus_rx.recv().await {
                    Ok(apollia_core::RuntimeEvent::TriggersReloaded { count: 1 }) => {
                        return true;
                    }
                    Ok(_) => {}
                    Err(_) => return false,
                }
            }
        })
        .await;
        assert_eq!(
            received,
            Ok(true),
            "TriggersReloaded {{ count: 1 }} must be emitted"
        );
    }

    /// Trigger with `agent` unaffected.
    #[tokio::test]
    async fn test_agent_trigger_unaffected() {
        // GIVEN an existing trigger with agent="hello-agent"
        let def = make_definition("rapport-hebdo", OnBusyPolicy::Queue { max_depth: 10 });
        let (router, calls) = MockTaskRouterHandle::new_with_tracking();
        let handle = TriggerEngine::start(
            vec![def],
            router,
            make_bus(),
            None,
            ObservabilityConfig::default(),
        )
        .await;

        // WHEN fire
        let result = handle.fire_now("rapport-hebdo").await;

        // THEN TaskRouter.submit() called exactly once; behavior unchanged
        assert!(result.is_ok(), "fire_now must succeed, got {result:?}");
        assert_eq!(
            calls.load(Ordering::SeqCst),
            1,
            "TaskRouter must be called exactly once"
        );
    }

    // --- Persisted counters ----------------------------------------------

    #[tokio::test]
    async fn test_engine_start_restores_counters() {
        // GIVEN a database with 3 fires and 2 skips for "my-trigger"
        let dir = tempfile::TempDir::new().unwrap();
        let db_path = dir.path().join("test.db");
        let base = chrono::Utc::now();
        {
            let mut p = TriggerPersistence::open(&db_path).unwrap();
            for i in 0..3u64 {
                p.record_fired(
                    crate::persistence::TriggerRecord {
                        trigger_id: "my-trigger",
                        agent_name: "test-agent",
                        fired_at: base + chrono::Duration::seconds(i as i64),
                        payload_json: None,
                    },
                    &format!("task-{i}"),
                    None,
                )
                .unwrap();
            }
            for _ in 0..2 {
                p.record_skipped(
                    crate::persistence::TriggerRecord {
                        trigger_id: "my-trigger",
                        agent_name: "test-agent",
                        fired_at: base,
                        payload_json: None,
                    },
                    "busy",
                )
                .unwrap();
            }
        }

        // WHEN TriggerEngine starts with this database
        let def = make_definition("my-trigger", OnBusyPolicy::Queue { max_depth: 10 });
        let (router, _) = MockTaskRouterHandle::new();
        let persistence = TriggerPersistence::open(&db_path).unwrap();
        let handle = TriggerEngine::start(
            vec![def],
            router,
            make_bus(),
            Some(persistence),
            ObservabilityConfig::default(),
        )
        .await;

        // THEN the historical counters are restored
        let list = handle.list().await;
        assert_eq!(list.len(), 1);
        assert_eq!(
            list[0].fire_count, 3,
            "fire_count must be 3 after the restart"
        );
        assert_eq!(
            list[0].skip_count, 2,
            "skip_count must be 2 after the restart"
        );
        assert!(list[0].last_fired.is_some(), "last_fired must be Some");
    }

    // --- OnBusyPolicy::Queue ---------------------------------------------

    #[tokio::test]
    async fn test_queue_accepts_up_to_max_depth() {
        // GIVEN policy Queue { max_depth: 3 }, busy agent (pending = 1)
        let def = make_definition("q-trigger", OnBusyPolicy::Queue { max_depth: 3 });
        let (router, calls) = MockTaskRouterHandle::new_with_pending(1);
        let handle = TriggerEngine::start(
            vec![def],
            router,
            make_bus(),
            None,
            ObservabilityConfig::default(),
        )
        .await;

        // WHEN 3 triggers fire
        for _ in 0..3 {
            let _ = handle.fire_now("q-trigger").await;
        }

        // THEN no submission (all queued)
        assert_eq!(
            calls.load(Ordering::SeqCst),
            0,
            "no submission while the agent is busy"
        );
    }

    #[tokio::test]
    async fn test_queue_full_drops_and_emits_event() {
        // GIVEN policy Queue { max_depth: 3 }, busy agent, queue already full
        let def = make_definition("full-trigger", OnBusyPolicy::Queue { max_depth: 3 });
        let (bus_tx, mut bus_rx) = broadcast::channel::<apollia_core::RuntimeEvent>(64);
        let (router, calls) = MockTaskRouterHandle::new_with_pending(1);
        let handle = TriggerEngine::start(
            vec![def],
            router,
            bus_tx,
            None,
            ObservabilityConfig::default(),
        )
        .await;

        // Fill the queue (3 elements).
        for _ in 0..3 {
            let _ = handle.fire_now("full-trigger").await;
        }

        // WHEN a 4th trigger arrives
        let result = handle.fire_now("full-trigger").await;

        // THEN submission still at 0 (the 4th is dropped)
        assert_eq!(
            calls.load(Ordering::SeqCst),
            0,
            "the 4th trigger must be dropped, not submitted"
        );
        // AND TriggerQueueFull emitted on the bus
        let queue_full = tokio::time::timeout(std::time::Duration::from_millis(200), async {
            loop {
                match bus_rx.recv().await {
                    Ok(apollia_core::RuntimeEvent::TriggerQueueFull { trigger_id }) => {
                        return trigger_id;
                    }
                    Ok(_) => {}
                    Err(_) => return String::new(),
                }
            }
        })
        .await
        .unwrap_or_default();
        assert_eq!(
            queue_full, "full-trigger",
            "TriggerQueueFull must name the right trigger"
        );
        // AND fire_now returns SubmitFailed
        assert!(
            matches!(result, Err(TriggerEngineError::SubmitFailed(_))),
            "expected SubmitFailed for a full queue, got {:?}",
            result
        );
    }

    #[tokio::test]
    async fn test_skip_policy_does_not_queue() {
        // GIVEN policy Skip, busy agent
        let def = make_definition("skip-trigger", OnBusyPolicy::Skip);
        let (router, calls) = MockTaskRouterHandle::new_with_pending(1);
        let handle = TriggerEngine::start(
            vec![def],
            router,
            make_bus(),
            None,
            ObservabilityConfig::default(),
        )
        .await;

        // WHEN the trigger fires
        let _ = handle.fire_now("skip-trigger").await;

        // THEN no submission, no queuing
        assert_eq!(
            calls.load(Ordering::SeqCst),
            0,
            "Skip must neither submit nor queue"
        );
    }

    #[tokio::test]
    async fn test_queue_drains_when_agent_free() {
        // GIVEN 2 queued triggers (busy agent, pending = 1)
        let def = make_definition("drain-trigger", OnBusyPolicy::Queue { max_depth: 10 });
        let (router, calls) = MockTaskRouterHandle::new_with_pending(1);
        let handle = TriggerEngine::start(
            vec![def],
            router,
            make_bus(),
            None,
            ObservabilityConfig::default(),
        )
        .await;

        // Queue 2 triggers.
        let _ = handle.fire_now("drain-trigger").await;
        let _ = handle.fire_now("drain-trigger").await;
        assert_eq!(
            calls.load(Ordering::SeqCst),
            0,
            "no submission while the agent is busy"
        );

        // WHEN the agent becomes free. `notify_agent_free` expects no reply,
        // so the barrier is the next request/reply on the same channel: the
        // actor drains the queue inside that command, then answers `list`.
        handle.notify_agent_free("test-agent".into()).await;
        let _ = handle.list().await;

        // THEN the 2 triggers are dispatched in FIFO order
        assert_eq!(
            calls.load(Ordering::SeqCst),
            2,
            "the 2 queued triggers must be dispatched"
        );
    }
}
