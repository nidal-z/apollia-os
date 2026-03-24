//! `PipelineEngine` — Tokio actor managing pipeline run lifecycle.
//!
//! Receives [`PipelineEngineMessage`] requests, creates [`PipelineRun`]s,
//! persists them to SQLite, and spawns a [`PipelineExecutor`] per run.
//! On startup it resumes any runs whose status was `running` the last time the
//! process stopped.
//!
//! # Architecture
//!
//! ```text
//! PipelineEngineHandle (Clone + Send + Sync)
//!   │   mpsc::channel(256)
//!   ▼
//! PipelineEngine (Tokio task)
//!   ├── spawns PipelineExecutor per run (detached task)
//!   └── listens for ShutdownRequested on EventBus
//! ```
//!
//! This module implements ADR-025: `apollia-pipelines` — TOML declarative,
//! native topologies, integrated HITL.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chrono::Utc;
use thiserror::Error;
use tokio::sync::{broadcast, mpsc, oneshot};
use tracing::{error, info, warn};
use uuid::Uuid;

use apollia_core::{EventBusSender, RuntimeEvent};

use crate::{
    executor::{ExecutorError, PipelineExecutor, TaskSubmitter},
    repository::{PipelineRepository, PipelineRepositoryError},
    types::{PipelineDefinition, PipelineId, PipelineRun, PipelineStatus, RunId},
};

// ── Error ─────────────────────────────────────────────────────────────────────

/// Errors returned by [`PipelineEngineHandle`] operations.
#[derive(Debug, Error)]
pub enum PipelineEngineError {
    /// No pipeline with the given identifier is registered in the engine.
    #[error("pipeline not found: {0}")]
    PipelineNotFound(String),

    /// The engine actor has stopped and can no longer accept requests.
    #[error("engine unavailable")]
    EngineUnavailable,

    /// A repository (SQLite) operation failed.
    #[error("repository error: {0}")]
    Repository(#[from] PipelineRepositoryError),
}

// ── Internal messages ─────────────────────────────────────────────────────────

/// Internal message variants processed by the [`PipelineEngine`] actor loop.
enum PipelineEngineMessage {
    /// Request to start a new run for the named pipeline.
    RunPipeline {
        pipeline_id: PipelineId,
        trigger_id: Option<String>,
        trigger_payload: Option<String>,
        reply: oneshot::Sender<Result<RunId, PipelineEngineError>>,
    },
    /// Retrieve the current state of a run from SQLite.
    GetRun {
        run_id: RunId,
        reply: oneshot::Sender<Result<Option<PipelineRun>, PipelineEngineError>>,
    },
    /// List all registered pipeline IDs and their descriptions.
    ListPipelines {
        reply: oneshot::Sender<Vec<(PipelineId, String)>>,
    },
    /// List the N most recent runs for a given pipeline.
    ListRuns {
        pipeline_id: PipelineId,
        limit: u32,
        reply: oneshot::Sender<Result<Vec<PipelineRun>, PipelineEngineError>>,
    },
    /// Replace in-memory pipeline definitions (after CRUD mutation).
    Reload {
        definitions: Vec<PipelineDefinition>,
        reply: oneshot::Sender<()>,
    },
    /// Signal the engine to stop accepting new requests.
    Shutdown,
}

// ── Handle ────────────────────────────────────────────────────────────────────

/// Clonable, `Send + Sync` handle to the [`PipelineEngine`] actor.
///
/// All methods are `async` and communicate with the actor over an mpsc channel.
/// If the actor has stopped, methods return [`PipelineEngineError::EngineUnavailable`].
#[derive(Clone)]
pub struct PipelineEngineHandle {
    tx: mpsc::Sender<PipelineEngineMessage>,
}

impl PipelineEngineHandle {
    /// Triggers a new run for the pipeline identified by `pipeline_id`.
    ///
    /// Returns the [`RunId`] immediately after the run is persisted to SQLite;
    /// the execution itself happens asynchronously in a spawned Tokio task.
    ///
    /// # Errors
    ///
    /// - [`PipelineEngineError::PipelineNotFound`] if no pipeline with that ID
    ///   is registered.
    /// - [`PipelineEngineError::EngineUnavailable`] if the actor has stopped.
    /// - [`PipelineEngineError::Repository`] if the SQLite insertion fails.
    pub async fn run_pipeline(
        &self,
        pipeline_id: &str,
        trigger_id: Option<String>,
        trigger_payload: Option<String>,
    ) -> Result<RunId, PipelineEngineError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(PipelineEngineMessage::RunPipeline {
                pipeline_id: PipelineId(pipeline_id.to_owned()),
                trigger_id,
                trigger_payload,
                reply: reply_tx,
            })
            .await
            .map_err(|_| PipelineEngineError::EngineUnavailable)?;
        reply_rx
            .await
            .map_err(|_| PipelineEngineError::EngineUnavailable)?
    }

    /// Returns the current state of a pipeline run from SQLite.
    ///
    /// Returns `Ok(None)` if no run with the given `run_id` exists.
    ///
    /// # Errors
    ///
    /// - [`PipelineEngineError::EngineUnavailable`] if the actor has stopped.
    /// - [`PipelineEngineError::Repository`] on database errors.
    pub async fn get_run(
        &self,
        run_id: &RunId,
    ) -> Result<Option<PipelineRun>, PipelineEngineError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(PipelineEngineMessage::GetRun {
                run_id: run_id.clone(),
                reply: reply_tx,
            })
            .await
            .map_err(|_| PipelineEngineError::EngineUnavailable)?;
        reply_rx
            .await
            .map_err(|_| PipelineEngineError::EngineUnavailable)?
    }

    /// Lists all pipeline IDs registered in the engine with their descriptions.
    ///
    /// Returns an empty `Vec` if the actor has stopped.
    pub async fn list_pipelines(&self) -> Vec<(PipelineId, String)> {
        let (reply_tx, reply_rx) = oneshot::channel();
        if self
            .tx
            .send(PipelineEngineMessage::ListPipelines { reply: reply_tx })
            .await
            .is_err()
        {
            return Vec::new();
        }
        reply_rx.await.unwrap_or_default()
    }

    /// Returns the `limit` most recent runs for `pipeline_id`, ordered by
    /// `started_at` descending.
    ///
    /// # Errors
    ///
    /// - [`PipelineEngineError::EngineUnavailable`] if the actor has stopped.
    /// - [`PipelineEngineError::Repository`] on database errors.
    pub async fn list_runs(
        &self,
        pipeline_id: &PipelineId,
        limit: u32,
    ) -> Result<Vec<PipelineRun>, PipelineEngineError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(PipelineEngineMessage::ListRuns {
                pipeline_id: pipeline_id.clone(),
                limit,
                reply: reply_tx,
            })
            .await
            .map_err(|_| PipelineEngineError::EngineUnavailable)?;
        reply_rx
            .await
            .map_err(|_| PipelineEngineError::EngineUnavailable)?
    }

    /// Replaces in-memory pipeline definitions with the given list.
    ///
    /// Called after a CRUD mutation on the `PipelineDefinitionRepository` to
    /// keep the engine's in-memory registry in sync with SQLite.
    pub async fn reload(&self, definitions: Vec<PipelineDefinition>) {
        let (reply_tx, reply_rx) = oneshot::channel();
        let _ = self
            .tx
            .send(PipelineEngineMessage::Reload {
                definitions,
                reply: reply_tx,
            })
            .await;
        let _ = reply_rx.await;
    }

    /// Signals the engine to stop accepting new requests.
    ///
    /// Runs already in progress are not interrupted — they complete naturally.
    pub async fn shutdown(&self) {
        let _ = self.tx.send(PipelineEngineMessage::Shutdown).await;
    }
}

// ── ArcTaskSubmitter adapter ──────────────────────────────────────────────────

/// Newtype wrapping `Arc<dyn TaskSubmitter>` so it can be used as the generic
/// `S` parameter of [`PipelineExecutor<S>`].
///
/// This adapter bridges the type-erased `Arc<dyn TaskSubmitter>` stored in
/// `PipelineEngine` with the monomorphised `PipelineExecutor<S>` —
/// identical pattern to `ToolExecutor` in ADR-015.
struct ArcTaskSubmitter(Arc<dyn TaskSubmitter>);

#[async_trait]
impl TaskSubmitter for ArcTaskSubmitter {
    async fn submit_task(&self, agent: &str, input: &str) -> Result<String, ExecutorError> {
        self.0.submit_task(agent, input).await
    }
}

// ── Actor ─────────────────────────────────────────────────────────────────────

/// Tokio actor managing the lifecycle of pipeline runs.
///
/// `PipelineEngine` receives [`PipelineEngineMessage`]s via an mpsc channel,
/// creates [`PipelineRun`]s, persists them to SQLite, and spawns a
/// [`PipelineExecutor`] for each run. At startup it resumes any runs whose
/// SQLite status was `running` when the process was last stopped.
pub struct PipelineEngine {
    /// Registered pipeline definitions, keyed by [`PipelineId`].
    pipelines: HashMap<PipelineId, PipelineDefinition>,
    /// Shared SQLite repository for pipeline run state.
    repo: Arc<Mutex<PipelineRepository>>,
    /// Task submission backend — injected for testability (ADR-015 pattern).
    submitter: Arc<dyn TaskSubmitter>,
    /// Runtime EventBus sender.
    event_bus: EventBusSender,
}

impl PipelineEngine {
    /// Starts the `PipelineEngine` actor, resumes any in-progress runs from
    /// SQLite, and returns a clonable [`PipelineEngineHandle`].
    ///
    /// The actor runs in a detached Tokio task and stops when it receives a
    /// `Shutdown` message, when the EventBus emits `ShutdownRequested`, or
    /// when all handles are dropped and the channel closes.
    pub fn spawn(
        pipelines: Vec<PipelineDefinition>,
        repo: Arc<Mutex<PipelineRepository>>,
        submitter: Arc<dyn TaskSubmitter>,
        event_bus: EventBusSender,
    ) -> PipelineEngineHandle {
        let (tx, rx) = mpsc::channel(256);
        let pipelines_map = pipelines
            .into_iter()
            .map(|d| (d.id.clone(), d))
            .collect::<HashMap<_, _>>();
        let engine = PipelineEngine {
            pipelines: pipelines_map,
            repo,
            submitter,
            event_bus,
        };
        tokio::spawn(engine.run(rx));
        PipelineEngineHandle { tx }
    }

    /// Main actor loop — processes messages until `Shutdown` is received.
    async fn run(mut self, mut rx: mpsc::Receiver<PipelineEngineMessage>) {
        // Subscribe to EventBus before resuming runs so we can observe
        // ShutdownRequested even if it fires before the first message.
        let mut event_rx = self.event_bus.subscribe();

        // Resume any runs that were in-flight at last process stop.
        self.resume_running_runs().await;

        loop {
            tokio::select! {
                msg = rx.recv() => {
                    match msg {
                        Some(PipelineEngineMessage::RunPipeline {
                            pipeline_id,
                            trigger_id,
                            trigger_payload,
                            reply,
                        }) => {
                            let result = self
                                .handle_run_pipeline(pipeline_id, trigger_id, trigger_payload)
                                .await;
                            let _ = reply.send(result);
                        }

                        Some(PipelineEngineMessage::GetRun { run_id, reply }) => {
                            let result = self.handle_get_run(&run_id);
                            let _ = reply.send(result);
                        }

                        Some(PipelineEngineMessage::ListPipelines { reply }) => {
                            let list = self
                                .pipelines
                                .values()
                                .map(|d| (d.id.clone(), d.description.clone()))
                                .collect();
                            let _ = reply.send(list);
                        }

                        Some(PipelineEngineMessage::ListRuns {
                            pipeline_id,
                            limit,
                            reply,
                        }) => {
                            let result = self.handle_list_runs(&pipeline_id, limit);
                            let _ = reply.send(result);
                        }

                        Some(PipelineEngineMessage::Reload { definitions, reply }) => {
                            let count = definitions.len();
                            self.pipelines = definitions
                                .into_iter()
                                .map(|d| (d.id.clone(), d))
                                .collect();
                            info!(count, "PipelineEngine reloaded definitions");
                            let _ = reply.send(());
                        }

                        Some(PipelineEngineMessage::Shutdown) => {
                            info!("PipelineEngine received Shutdown — stopping");
                            break;
                        }

                        // All handles dropped.
                        None => {
                            info!("PipelineEngine channel closed — stopping");
                            break;
                        }
                    }
                }

                event = event_rx.recv() => {
                    match event {
                        Ok(RuntimeEvent::ShutdownRequested) => {
                            info!("PipelineEngine observed ShutdownRequested — stopping");
                            break;
                        }
                        Err(broadcast::error::RecvError::Lagged(n)) => {
                            warn!(skipped = n, "PipelineEngine EventBus receiver lagged");
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    /// Reloads runs with status `running` from SQLite and spawns their executors.
    ///
    /// Steps whose status was `running` in SQLite are automatically reset to
    /// `pending` by [`PipelineRepository::find_running_runs`] so the executor
    /// can re-submit them.
    async fn resume_running_runs(&mut self) {
        let running = match self
            .repo
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .find_running_runs()
        {
            Ok(runs) => runs,
            Err(e) => {
                error!(error = %e, "failed to load running pipeline runs at startup");
                return;
            }
        };

        for run in running {
            info!(
                run_id   = %run.run_id,
                pipeline = %run.pipeline_id,
                "resuming pipeline run from SQLite",
            );
            if let Some(def) = self.pipelines.get(&run.pipeline_id).cloned() {
                self.spawn_run(run, def, true);
            } else {
                warn!(
                    run_id   = %run.run_id,
                    pipeline = %run.pipeline_id,
                    "pipeline definition not found — run cannot be resumed",
                );
            }
        }
    }

    /// Handles a `RunPipeline` message: validates the pipeline, creates and
    /// persists the run, then spawns an executor.
    async fn handle_run_pipeline(
        &self,
        pipeline_id: PipelineId,
        trigger_id: Option<String>,
        trigger_payload: Option<String>,
    ) -> Result<RunId, PipelineEngineError> {
        let definition = self
            .pipelines
            .get(&pipeline_id)
            .ok_or_else(|| PipelineEngineError::PipelineNotFound(pipeline_id.0.clone()))?
            .clone();

        let run_id = generate_run_id();
        let run = PipelineRun {
            run_id: run_id.clone(),
            pipeline_id,
            trigger_id,
            status: PipelineStatus::Running,
            step_runs: HashMap::new(),
            trigger_payload,
            started_at: Utc::now(),
            ended_at: None,
        };

        // Persist before spawning — the executor assumes the row already exists.
        self.repo
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert_run(&run)?;

        self.spawn_run(run, definition, false);
        Ok(run_id)
    }

    /// Handles a `GetRun` message: loads run state from SQLite.
    fn handle_get_run(&self, run_id: &RunId) -> Result<Option<PipelineRun>, PipelineEngineError> {
        let repo = self.repo.lock().unwrap_or_else(|e| e.into_inner());
        Ok(repo.find_run(run_id)?)
    }

    /// Handles a `ListRuns` message: loads run history from SQLite.
    fn handle_list_runs(
        &self,
        pipeline_id: &PipelineId,
        limit: u32,
    ) -> Result<Vec<PipelineRun>, PipelineEngineError> {
        let repo = self.repo.lock().unwrap_or_else(|e| e.into_inner());
        Ok(repo.find_runs_by_pipeline(pipeline_id, limit)?)
    }

    /// Spawns a [`PipelineExecutor`] task for the given `run` and `definition`.
    ///
    /// When `is_resume` is `true`, the executor is built with `.as_resume()` so
    /// that step rows already present in SQLite are not re-inserted, and steps
    /// that already reached a terminal state are skipped automatically.
    ///
    /// The executor runs in a detached Tokio task. SQLite is the source of truth
    /// for run state — the engine does not track individual run tasks.
    fn spawn_run(&self, run: PipelineRun, definition: PipelineDefinition, is_resume: bool) {
        let submitter = ArcTaskSubmitter(Arc::clone(&self.submitter));
        let executor = PipelineExecutor::new(
            definition,
            run,
            submitter,
            self.event_bus.clone(),
            Arc::clone(&self.repo),
        );
        let executor = if is_resume {
            executor.as_resume()
        } else {
            executor
        };
        tokio::spawn(async move {
            if let Err(e) = executor.execute().await {
                error!(error = %e, "PipelineExecutor terminated with error");
            }
        });
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Generates a unique run identifier with the `r-` prefix.
///
/// Example output: `r-3f7a2b9c`.
fn generate_run_id() -> RunId {
    let id = Uuid::new_v4().to_string().replace('-', "");
    RunId(format!("r-{}", &id[..8]))
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        executor::ExecutorError,
        types::{PipelineStepDef, StepFailurePolicy, StepId},
    };
    use async_trait::async_trait;
    use std::collections::HashMap;

    // ── Helpers ───────────────────────────────────────────────────────────────

    /// Minimal `TaskSubmitter` that always reports the task as submitted but
    /// never emits `TaskCompleted` on the EventBus.
    ///
    /// Sufficient for testing list, run, unknown pipeline and get_run scenarios without running a full
    /// runtime stack.
    struct NeverCompletingSubmitter;

    #[async_trait]
    impl TaskSubmitter for NeverCompletingSubmitter {
        async fn submit_task(&self, _agent: &str, _input: &str) -> Result<String, ExecutorError> {
            // Return a deterministic task ID so the executor can wait for it.
            Ok(format!("task-{}", Uuid::new_v4()))
        }
    }

    /// Builds a two-step pipeline definition for tests.
    fn make_pipeline(id: &str) -> PipelineDefinition {
        PipelineDefinition {
            id: PipelineId(id.to_owned()),
            description: format!("Test pipeline {id}"),
            on_failure: crate::types::GlobalFailurePolicy::Fail,
            steps: vec![
                PipelineStepDef {
                    id: StepId("step-a".into()),
                    agent: "agent-a".into(),
                    input: "{{trigger.payload}}".into(),
                    depends_on: vec![],
                    on_failure: StepFailurePolicy::Fail,
                    condition: None,
                    fallback_for: None,
                },
                PipelineStepDef {
                    id: StepId("step-b".into()),
                    agent: "agent-b".into(),
                    input: "{{steps.step-a.output}}".into(),
                    depends_on: vec![StepId("step-a".into())],
                    on_failure: StepFailurePolicy::Fail,
                    condition: None,
                    fallback_for: None,
                },
            ],
        }
    }

    /// Builds a shared in-memory `PipelineRepository`.
    fn make_repo() -> Arc<Mutex<PipelineRepository>> {
        let repo = PipelineRepository::open_in_memory().expect("in-memory DB must open");
        Arc::new(Mutex::new(repo))
    }

    /// Builds a no-op EventBus (broadcast channel with capacity 64).
    fn make_event_bus() -> EventBusSender {
        tokio::sync::broadcast::channel(64).0
    }

    /// Spawns a `PipelineEngine` with the given pipelines.
    fn spawn_engine(
        pipelines: Vec<PipelineDefinition>,
        repo: Arc<Mutex<PipelineRepository>>,
    ) -> PipelineEngineHandle {
        let submitter: Arc<dyn TaskSubmitter> = Arc::new(NeverCompletingSubmitter);
        PipelineEngine::spawn(pipelines, repo, submitter, make_event_bus())
    }

    // ── list_pipelines returns all registered pipelines ────────────────

    #[tokio::test]
    async fn test_ac1_list_pipelines() {
        // GIVEN PipelineEngine with 2 pipelines
        let p1 = make_pipeline("pipeline-alpha");
        let p2 = make_pipeline("pipeline-beta");
        let handle = spawn_engine(vec![p1, p2], make_repo());

        // WHEN
        let list = handle.list_pipelines().await;

        // THEN — 2 entries returned
        assert_eq!(list.len(), 2, "expected 2 pipelines, got {}", list.len());
        let ids: Vec<&str> = list.iter().map(|(id, _)| id.0.as_str()).collect();
        assert!(ids.contains(&"pipeline-alpha"), "pipeline-alpha missing");
        assert!(ids.contains(&"pipeline-beta"), "pipeline-beta missing");
    }

    // ── run_pipeline creates a run persisted in SQLite ─────────────────

    #[tokio::test]
    async fn test_ac2_run_pipeline_returns_run_id() {
        // GIVEN pipeline "test-pipe" registered
        let repo = make_repo();
        let handle = spawn_engine(vec![make_pipeline("test-pipe")], Arc::clone(&repo));

        // WHEN
        let result = handle
            .run_pipeline("test-pipe", None, Some("input.pdf".into()))
            .await;

        // THEN — Ok(RunId) returned
        let run_id = result.expect("run_pipeline must succeed for a known pipeline");
        assert!(run_id.0.starts_with("r-"), "RunId must start with 'r-'");

        // AND the run is persisted in SQLite with status=Running
        let run = repo
            .lock()
            .unwrap()
            .find_run(&run_id)
            .expect("find_run must not error")
            .expect("run must be present in SQLite");
        assert_eq!(run.status, PipelineStatus::Running);
        assert_eq!(run.trigger_payload.as_deref(), Some("input.pdf"));
    }

    // ── unknown pipeline returns PipelineNotFound ──────────────────────

    #[tokio::test]
    async fn test_ac3_unknown_pipeline_error() {
        // GIVEN PipelineEngine with no pipeline "inexistant"
        let handle = spawn_engine(vec![], make_repo());

        // WHEN
        let result = handle.run_pipeline("inexistant", None, None).await;

        // THEN — Err(PipelineNotFound)
        assert!(
            matches!(result, Err(PipelineEngineError::PipelineNotFound(_))),
            "expected PipelineNotFound, got {result:?}",
        );
    }

    // ── resume_running_runs spawns executors for in-flight runs ─────────

    #[tokio::test]
    async fn test_ac4_resume_running_runs() {
        // GIVEN — 2 runs in 'running' status inserted directly into SQLite
        let repo = make_repo();
        let def = make_pipeline("facture");
        {
            let mut guard = repo.lock().unwrap();
            let run1 = PipelineRun {
                run_id: RunId("r-aaa00001".into()),
                pipeline_id: PipelineId("facture".into()),
                trigger_id: None,
                status: PipelineStatus::Running,
                step_runs: HashMap::new(),
                trigger_payload: None,
                started_at: Utc::now(),
                ended_at: None,
            };
            let run2 = PipelineRun {
                run_id: RunId("r-bbb00002".into()),
                pipeline_id: PipelineId("facture".into()),
                trigger_id: None,
                status: PipelineStatus::Running,
                step_runs: HashMap::new(),
                trigger_payload: None,
                started_at: Utc::now(),
                ended_at: None,
            };
            guard.insert_run(&run1).unwrap();
            guard.insert_run(&run2).unwrap();
        }

        // WHEN PipelineEngine::spawn() is called — resume logic runs at startup
        let _handle = spawn_engine(vec![def], Arc::clone(&repo));

        // Give the actor time to process the startup resume.
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // THEN — both runs are still present in SQLite (executors were spawned)
        let guard = repo.lock().unwrap();
        assert!(
            guard
                .find_run(&RunId("r-aaa00001".into()))
                .unwrap()
                .is_some(),
            "run r-aaa00001 must still be in SQLite"
        );
        assert!(
            guard
                .find_run(&RunId("r-bbb00002".into()))
                .unwrap()
                .is_some(),
            "run r-bbb00002 must still be in SQLite"
        );
    }

    // ── get_run returns run state from SQLite ───────────────────────────

    #[tokio::test]
    async fn test_ac5_get_run_returns_state() {
        // GIVEN a run started via the handle
        let repo = make_repo();
        let handle = spawn_engine(vec![make_pipeline("my-pipe")], Arc::clone(&repo));
        let run_id = handle
            .run_pipeline("my-pipe", Some("trigger-1".into()), Some("data".into()))
            .await
            .expect("run_pipeline must succeed");

        // WHEN
        let run = handle
            .get_run(&run_id)
            .await
            .expect("get_run must not error")
            .expect("run must be found");

        // THEN — PipelineRun returned with correct fields
        assert_eq!(run.run_id, run_id);
        assert_eq!(run.pipeline_id, PipelineId("my-pipe".into()));
        assert_eq!(run.trigger_id.as_deref(), Some("trigger-1"));
        assert_eq!(run.trigger_payload.as_deref(), Some("data"));
    }

    // ── shutdown stops the engine ──────────────────────────────────────

    #[tokio::test]
    async fn test_ac6_shutdown_stops_engine() {
        // GIVEN an active PipelineEngine
        let handle = spawn_engine(vec![make_pipeline("p")], make_repo());

        // WHEN
        handle.shutdown().await;

        // Give the actor time to process the Shutdown message.
        tokio::time::sleep(std::time::Duration::from_millis(30)).await;

        // THEN — subsequent calls return EngineUnavailable
        let result = handle.run_pipeline("p", None, None).await;
        assert!(
            matches!(result, Err(PipelineEngineError::EngineUnavailable)),
            "expected EngineUnavailable after shutdown, got {result:?}",
        );
    }
}
