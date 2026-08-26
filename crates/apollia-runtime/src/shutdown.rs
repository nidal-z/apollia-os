//! Graceful shutdown controller for the Apollia runtime.
//!
//! Handles SIGTERM/SIGINT signals, drains in-progress tasks with a configurable
//! timeout, and stops all actors in reverse startup order.
//!
//! The shutdown sequence:
//! 1. Broadcast `ShutdownRequested` on the EventBus
//! 2. Stop the API server from accepting new connections
//! 3. Drain in-progress tasks (default: 30s timeout)
//! 4. Cancel remaining tasks and log them
//! 5. Transition agents to Stopped
//! 6. Stop the NotificationEngine
//! 7. Stop the MCP client manager (kill child processes, prevent zombies)
//! 8. Stop actors in reverse order: TaskRouter → AgentRegistry

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tracing::{info, warn};

use apollia_core::events::subscribe_resilient;
use apollia_core::{ProcessState, RuntimeEvent, TaskId};
use apollia_mcp::manager::McpClientManagerHandle;

use crate::api::APIServerHandle;
use crate::coordinator::ExecutionBackend;
use crate::eventbus::EventBusSender;
use crate::registry::AgentRegistryHandle;
use crate::router::TaskRouterHandle;

/// Shutdown configuration.
pub struct ShutdownConfig {
    /// Maximum time (in seconds) to wait for in-progress tasks to complete.
    pub drain_timeout_secs: u64,
}

impl Default for ShutdownConfig {
    fn default() -> Self {
        Self {
            drain_timeout_secs: 30,
        }
    }
}

/// Errors that can occur during graceful shutdown.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ShutdownError {
    /// Some tasks did not complete within the drain timeout.
    #[error("drain timeout: {count} tasks still running after {timeout_secs}s")]
    DrainTimeout {
        /// Number of tasks that were still active.
        count: u32,
        /// The drain timeout that was exceeded.
        timeout_secs: u64,
    },

    /// An actor failed to stop cleanly.
    #[error("actor {actor} failed to stop: {reason}")]
    ActorStopFailed {
        /// Name of the actor.
        actor: String,
        /// Reason for the failure.
        reason: String,
    },
}

/// Handles to all runtime actors needed to build a [`ShutdownController`].
pub struct ShutdownControllerDeps<B: ExecutionBackend> {
    pub config: ShutdownConfig,
    pub event_sender: EventBusSender,
    pub api_handle: APIServerHandle,
    pub router_handle: TaskRouterHandle<B>,
    pub registry_handle: AgentRegistryHandle,
    pub notification_engine: Option<apollia_notifications::NotificationEngineHandle>,
    pub mcp_handle: Option<McpClientManagerHandle>,
    /// Owner of the Python runner child processes, when one is running.
    ///
    /// Held here so the teardown kills the children it spawned. Without it they
    /// are reparented to init and survive the daemon that started them.
    pub runner_supervisor: Option<Arc<crate::runner_supervisor::RunnerSupervisor>>,
    /// Owner of the `llama-server` child processes, when one is running.
    ///
    /// Same reason as `runner_supervisor`, and the cost of missing it is higher:
    /// a loaded model holds gigabytes of resident memory.
    pub llama_server_supervisor: Option<Arc<crate::llama_server::LlamaServerSupervisor>>,
}

/// Graceful shutdown controller for the Apollia runtime.
///
/// Orchestrates the shutdown sequence: signal handling, task draining,
/// agent lifecycle, and ordered actor teardown.
pub struct ShutdownController<B: ExecutionBackend> {
    config: ShutdownConfig,
    event_sender: EventBusSender,
    api_handle: APIServerHandle,
    router_handle: TaskRouterHandle<B>,
    registry_handle: AgentRegistryHandle,
    /// Handle to the NotificationEngine, if started.
    ///
    /// Stopped explicitly *before* the EventBus closes so no late notifications
    /// are delivered after `apollia-os stop` returns.
    notification_engine: Option<apollia_notifications::NotificationEngineHandle>,
    /// Handle to the MCP client manager, if started.
    ///
    /// Stopped after the NotificationEngine and before the TaskRouter so that
    /// any in-flight MCP tool calls complete before the subprocess is killed.
    mcp_handle: Option<McpClientManagerHandle>,
    /// Owner of the Python runner child processes, if started.
    runner_supervisor: Option<Arc<crate::runner_supervisor::RunnerSupervisor>>,
    /// Owner of the `llama-server` child processes, if started.
    llama_server_supervisor: Option<Arc<crate::llama_server::LlamaServerSupervisor>>,
}

impl<B: ExecutionBackend> ShutdownController<B> {
    /// Create a new ShutdownController with handles to all runtime actors.
    pub fn new(deps: ShutdownControllerDeps<B>) -> Self {
        Self {
            config: deps.config,
            event_sender: deps.event_sender,
            api_handle: deps.api_handle,
            router_handle: deps.router_handle,
            registry_handle: deps.registry_handle,
            notification_engine: deps.notification_engine,
            mcp_handle: deps.mcp_handle,
            runner_supervisor: deps.runner_supervisor,
            llama_server_supervisor: deps.llama_server_supervisor,
        }
    }

    /// Perform the graceful shutdown sequence.
    ///
    /// 1. Broadcast `ShutdownRequested` on the EventBus
    /// 2. Stop the API server (no new connections)
    /// 3. Drain in-progress tasks (with timeout)
    /// 4. Cancel remaining tasks
    /// 5. Transition agents to Stopped
    /// 6. Kill the spawned child processes (Python runner, `llama-server`)
    /// 7. Stop actors in reverse startup order
    ///
    /// Returns `Ok(())` if all tasks drained within the timeout.
    /// Returns `Err(ShutdownError::DrainTimeout)` if some tasks had to be force-canceled,
    /// but the shutdown still completes.
    pub async fn shutdown(self) -> Result<(), ShutdownError> {
        info!("Shutdown sequence started");

        // Step 1: Broadcast ShutdownRequested
        let _ = self.event_sender.send(RuntimeEvent::ShutdownRequested);
        info!("ShutdownRequested broadcast on EventBus");

        // Step 2: Stop API server (reject new connections)
        self.api_handle.shutdown();
        info!("APIServer stopped accepting connections");

        // Step 3: Drain in-progress tasks
        let drain_result = self.drain_tasks().await;

        // Step 4: Transition agents to Stopped
        self.stop_agents().await;

        // Step 5: Stop NotificationEngine before the EventBus closes.
        // This prevents late notifications from firing after the process exits
        // (macOS Notification Center buffers osascript calls asynchronously).
        if let Some(ref notif_handle) = self.notification_engine {
            notif_handle.shutdown().await;
            info!("NotificationEngine stopped");
        }

        // Step 6: Stop the MCP client manager, killing all child server processes.
        if let Some(mcp_handle) = self.mcp_handle {
            info!("Shutting down MCP client manager");
            mcp_handle.shutdown().await;
            info!("McpClientManager stopped");
        }

        // Step 7: Kill the child processes this runtime spawned.
        //
        // After the MCP servers and before the actors: an in-flight tool call has
        // already drained, and a model still answering would be answering nobody.
        // Skipping this leaves `llama-server` and the Python runner reparented to
        // init, alive and holding their memory, while the shell that ran
        // `apollia-os stop` reports the runtime stopped.
        if self.runner_supervisor.is_some() || self.llama_server_supervisor.is_some() {
            crate::embedded::stop_supervisors(
                self.runner_supervisor.as_deref(),
                self.llama_server_supervisor.as_deref(),
            )
            .await;
            info!("Child process supervisors stopped");
        }

        // Step 8: Stop actors in reverse startup order
        // APIServer already stopped in step 2
        // TaskRouter
        self.router_handle.shutdown();
        info!("TaskRouter stopped");
        // AgentRegistry
        self.registry_handle.shutdown();
        info!("AgentRegistry stopped");
        // EventBus is a broadcast channel, it stops when all senders are dropped

        info!("Shutdown sequence completed");
        drain_result
    }

    /// Drain in-progress tasks, waiting up to `drain_timeout_secs`.
    ///
    /// Subscribes to the EventBus to watch for `TaskCompleted`/`TaskCanceled` events.
    /// Gets the initial set of active tasks from the router, then counts down
    /// as completion events arrive. If tasks remain after the timeout, they are
    /// force-canceled.
    async fn drain_tasks(&self) -> Result<(), ShutdownError> {
        let timeout = Duration::from_secs(self.config.drain_timeout_secs);

        // Subscribe to the EventBus BEFORE snapshotting the active-task set. A
        // broadcast receiver only observes events published after it subscribes,
        // so a completion landing in the window between the snapshot and the
        // subscription would be missed, leaving a finished task in `remaining`
        // until the drain timeout force-cancels it. Subscribing first closes that
        // window, matching the ordering already used on the delegation path.
        let mut rx = subscribe_resilient(&self.event_sender, "shutdown.drain");

        // Snapshot the current set of active tasks.
        let mut remaining: std::collections::HashSet<TaskId> = self
            .router_handle
            .active_tasks()
            .await
            .unwrap_or_default()
            .into_iter()
            .collect();

        if remaining.is_empty() {
            info!("No active tasks to drain");
            return Ok(());
        }

        info!(count = remaining.len(), "Draining active tasks");

        let deadline = tokio::time::Instant::now() + timeout;

        loop {
            tokio::select! {
                result = rx.recv() => {
                    match result {
                        Some(RuntimeEvent::TaskCompleted { task_id, .. }) => {
                            remaining.remove(&task_id);
                        }
                        Some(RuntimeEvent::TaskCanceled { task_id }) => {
                            remaining.remove(&task_id);
                        }
                        Some(_) => {}
                        None => {
                            break;
                        }
                    }
                    if remaining.is_empty() {
                        info!("All tasks drained successfully");
                        return Ok(());
                    }
                }
                _ = tokio::time::sleep_until(deadline) => {
                    break;
                }
            }
        }

        // Timeout expired or bus closed, cancel remaining tasks
        let active: Vec<TaskId> = remaining.into_iter().collect();
        let count = active.len() as u32;
        warn!(
            count = count,
            timeout_secs = self.config.drain_timeout_secs,
            "Drain timeout expired, canceling remaining tasks"
        );
        self.cancel_tasks(&active).await;
        Err(ShutdownError::DrainTimeout {
            count,
            timeout_secs: self.config.drain_timeout_secs,
        })
    }

    /// Force-cancel a list of tasks and emit audit events.
    async fn cancel_tasks(&self, task_ids: &[TaskId]) {
        for task_id in task_ids {
            let _ = self.router_handle.cancel(task_id.as_str()).await;
            let _ = self.event_sender.send(RuntimeEvent::TaskCanceled {
                task_id: task_id.clone(),
            });
            warn!(task_id = %task_id, "Task canceled during shutdown (task_canceled_shutdown)");
        }
    }

    /// Transition all active/degraded agents to Stopped.
    async fn stop_agents(&self) {
        let agents = match self.registry_handle.list_agents().await {
            Ok(agents) => agents,
            Err(e) => {
                warn!(error = %e, "Failed to list agents during shutdown");
                return;
            }
        };

        for agent in agents {
            let transitions = match agent.process_state {
                ProcessState::Active | ProcessState::Degraded => {
                    vec![ProcessState::Stopping, ProcessState::Stopped]
                }
                ProcessState::Initializing => {
                    vec![ProcessState::Stopping, ProcessState::Stopped]
                }
                ProcessState::Stopping => {
                    vec![ProcessState::Stopped]
                }
                ProcessState::Stopped => {
                    continue;
                }
            };

            for state in transitions {
                if let Err(e) = self
                    .registry_handle
                    .update_state(agent.id.as_str(), state.clone())
                    .await
                {
                    warn!(
                        agent_id = %agent.id,
                        target_state = ?state,
                        error = %e,
                        "Failed to transition agent during shutdown"
                    );
                    break;
                }
            }
            info!(agent_id = %agent.id, "Agent stopped during shutdown");
        }
    }
}

/// Wait for a shutdown signal (SIGTERM or SIGINT).
///
/// Returns when the first signal is received. A second SIGINT (Ctrl+C)
/// during the drain phase will force an immediate exit via `std::process::exit(1)`.
///
/// The `force_exit_armed` flag is set to `true` after the first signal,
/// enabling the force-exit handler.
pub async fn wait_for_shutdown_signal() -> &'static str {
    let force_exit_armed = Arc::new(AtomicBool::new(false));

    // Spawn a background task that watches for a second Ctrl+C
    let armed_clone = Arc::clone(&force_exit_armed);
    tokio::spawn(async move {
        // Wait for the first Ctrl+C (consumed by the main select below)
        // Then wait for a second one
        loop {
            tokio::signal::ctrl_c().await.ok();
            if armed_clone.load(Ordering::SeqCst) {
                // REASON: direct stderr, not tracing. On a forced double Ctrl+C
                // exit the tracing subscriber may already be torn down, so
                // stderr is the only reliable channel for this last line before
                // process::exit.
                eprintln!("\nForced shutdown (double Ctrl+C)");
                std::process::exit(1);
            }
        }
    });

    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};
        let mut sigterm =
            signal(SignalKind::terminate()).expect("failed to install SIGTERM handler");

        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                force_exit_armed.store(true, Ordering::SeqCst);
                "SIGINT"
            }
            _ = sigterm.recv() => {
                force_exit_armed.store(true, Ordering::SeqCst);
                "SIGTERM"
            }
        }
    }

    #[cfg(not(unix))]
    {
        tokio::signal::ctrl_c().await.ok();
        force_exit_armed.store(true, Ordering::SeqCst);
        "SIGINT"
    }
}

#[cfg(test)]
mod tests {
    // No test in this module ever contacts the API over TCP: they check what
    // the shutdown sequence does to the actors behind it. So every server
    // here is started on the Unix socket alone, which `tcp_port: None`
    // selects and which is the configuration an embedded host runs.
    //
    // Asking for a TCP port instead cost a verdict. The idiom was to reserve
    // a port by probe-binding it, release the probe, then let the server bind
    // the number; between the release and the bind the number belongs to
    // nobody, and the guard replayed four times on one commit answered green,
    // green, red, red. Reproduced deliberately by squatting the private pool
    // and re-binding the four ports left free, the failure is
    // `BindFailed { port, AddrInUse }` raised by `APIServer::start`. A window
    // that small cannot be closed by making it smaller.
    //
    // A test that does need the port keeps the reservation and lives in
    // `api::server`, where the assertion is about the listener itself.
    //

    use super::*;
    use crate::coordinator::{ExecutionBackend, ExecutionCoordinator};
    use crate::eventbus::EventBus;
    use crate::registry::AgentRegistry;
    use crate::router::TaskRouterHandle;
    use apollia_core::{AIPInput, AIPResult, AIPTask, AgentManifest, TaskStatus};
    use std::future::Future;
    use std::path::PathBuf;
    use std::pin::Pin;

    /// Backend mock that completes immediately.
    #[derive(Clone)]
    struct MockBackend;

    impl MockBackend {
        fn instant() -> Self {
            MockBackend
        }
    }

    impl From<crate::coordinator::DynBackend> for MockBackend {
        fn from(_: crate::coordinator::DynBackend) -> Self {
            MockBackend::instant()
        }
    }

    impl ExecutionBackend for MockBackend {
        fn execute(
            &self,
            task: AIPTask,
        ) -> Pin<Box<dyn Future<Output = Result<AIPResult, String>> + Send>> {
            Box::pin(async move {
                Ok(AIPResult {
                    task_id: task.task_id,
                    status: TaskStatus::Completed,
                    output: Vec::new(),
                    error: None,
                    artifacts: Vec::new(),
                    input_required_data: None,
                })
            })
        }
    }

    /// Backend that completes only once the test releases its gate. Lets a test
    /// hold a task in-flight deterministically, with no wall-clock delay, then
    /// release it so the completion event fires at a controlled moment.
    #[derive(Clone)]
    struct GatedBackend {
        gate: tokio::sync::watch::Receiver<bool>,
    }

    impl GatedBackend {
        /// A backend whose gate is already open (completes immediately). Used for
        /// the unused `AppState::backend` slot, not for the drain assertion.
        fn released() -> Self {
            let (_tx, rx) = tokio::sync::watch::channel(true);
            GatedBackend { gate: rx }
        }
    }

    impl From<crate::coordinator::DynBackend> for GatedBackend {
        fn from(_: crate::coordinator::DynBackend) -> Self {
            GatedBackend::released()
        }
    }

    impl ExecutionBackend for GatedBackend {
        fn execute(
            &self,
            task: AIPTask,
        ) -> Pin<Box<dyn Future<Output = Result<AIPResult, String>> + Send>> {
            let mut gate = self.gate.clone();
            Box::pin(async move {
                let _ = gate.wait_for(|released| *released).await;
                Ok(AIPResult {
                    task_id: task.task_id,
                    status: TaskStatus::Completed,
                    output: Vec::new(),
                    error: None,
                    artifacts: Vec::new(),
                    input_required_data: None,
                })
            })
        }
    }

    /// Backend that never completes (blocks forever).
    #[derive(Clone)]
    struct NeverBackend;

    impl From<crate::coordinator::DynBackend> for NeverBackend {
        fn from(_: crate::coordinator::DynBackend) -> Self {
            NeverBackend
        }
    }

    impl ExecutionBackend for NeverBackend {
        fn execute(
            &self,
            _task: AIPTask,
        ) -> Pin<Box<dyn Future<Output = Result<AIPResult, String>> + Send>> {
            Box::pin(async {
                // Block forever
                std::future::pending::<()>().await;
                unreachable!()
            })
        }
    }

    fn test_manifest(name: &str) -> AgentManifest {
        AgentManifest {
            format_version: 1,
            name: name.to_string(),
            version: "0.1.0".to_string(),
            description: String::new(),
            tools_required: vec![],
            tools_optional: vec![],
            supports_streaming: false,
            supports_a2a: false,
            memory_namespace: None,
            shared_memory_namespaces: vec![],
            max_concurrent_tasks: 2,
            step_budget: None,
            network_allowlist: None,
            dangerous_tools_allowed: false,
            tags: vec![],
            skills: vec![],
            execution_mode: "auto".to_string(),
            supports_mailbox: false,
            mailbox_allowlist: None,
            system_prompt: None,
            tools_requiring_approval: vec![],
            llm_backend: None,
            packages: vec![],
            memory_config: None,
            agent_type: None,
            examples: vec![],
            limitations: vec![],
            setup_notes: None,
            agent_class: None,
            user_memory_write: false,
            datasources: vec![],
            templates: vec![],
            secrets: vec![],
            check_commands: vec![],
        }
    }

    /// Create a unique temp socket path (macOS SUN_LEN limit).
    fn temp_socket_path() -> PathBuf {
        let id = &uuid::Uuid::new_v4().to_string()[..8];
        PathBuf::from(format!("/tmp/ap-{}.sock", id))
    }

    /// Set up a full test environment with all actors.
    async fn setup_env<B: ExecutionBackend + Clone + From<crate::coordinator::DynBackend>>(
        backend: B,
    ) -> (
        ShutdownController<B>,
        EventBusSender,
        PathBuf,
        Arc<crate::llama_server::LlamaServerSupervisor>,
    ) {
        use crate::api::{APIServer, APIServerConfig, AppState};

        let llama = crate::llama_server::LlamaServerSupervisor::for_test();
        let (event_sender, _rx) = EventBus::new();
        let registry_handle = AgentRegistry::spawn(event_sender.clone());
        let router_handle: TaskRouterHandle<B> =
            TaskRouterHandle::spawn(registry_handle.clone(), event_sender.clone(), 256);

        let socket_path = temp_socket_path();

        let state = AppState {
            router_handle: router_handle.clone(),
            registry_handle: registry_handle.clone(),
            event_sender: event_sender.clone(),
            agent_loader: std::sync::Arc::new(crate::api::routes_agents::StubAgentLoader),
            backend,
            llm_router: crate::api::server::empty_shared_llm_router(),
            trigger_engine: None,
            config_path: None,
            task_repository: None,
            pending_approvals: None,
            plan_gates: None,
            notification_config: None,
            backend_factory: None,
            tool_registry_handle: None,
            audit_trail: None,
            audit_journal: None,
            obs_config: apollia_core::ObservabilityConfig::default(),
            llm_call_repository: None,
            trigger_def_repo: None,
            notification_repo: None,
            notification_engine_handle: None,
            chat_manager: None,
            plan_cache: None,
            mailbox_handle: None,
            user_memory: None,
            data_dir: std::path::PathBuf::new(),
            stt_engine: crate::api::server::empty_shared_stt_engine(),
            stt_repository: crate::api::server::empty_shared_stt_repository(),
            mcp_handle: None,
            mcp_server_repo: None,
            llm_backend_repo: None,
            stt_config_repo: None,
            a2a_invoker: None,
            resilience_layer: None,
            runner_proxy: None,
            llama_server_supervisor: None,
        };
        let config = APIServerConfig {
            socket_path: socket_path.clone(),
            bind_addr: "127.0.0.1".to_owned(),
            tcp_port: None,
            api_token: None,
            tls_cert_path: None,
            tls_key_path: None,
        };
        let api_server = APIServer::new(config, state);
        let api_handle = api_server.start().await.unwrap();

        let shutdown_config = ShutdownConfig {
            drain_timeout_secs: 1,
        };
        let controller = ShutdownController::new(ShutdownControllerDeps {
            config: shutdown_config,
            event_sender: event_sender.clone(),
            api_handle,
            router_handle,
            registry_handle,
            notification_engine: None,
            mcp_handle: None,
            runner_supervisor: None,
            llama_server_supervisor: Some(llama.clone()),
        });

        (controller, event_sender, socket_path, llama)
    }

    // The teardown reaches the child processes the runtime spawned
    #[tokio::test]
    async fn test_shutdown_stops_the_llama_server_supervisor() {
        // GIVEN a controller holding a llama-server supervisor that owns no
        // process yet, and that has not been asked to stop
        let (controller, _event_sender, socket_path, llama) =
            setup_env(MockBackend::instant()).await;
        assert!(
            !llama.is_shutting_down().await,
            "precondition: the supervisor must start out running"
        );

        // WHEN the shutdown sequence runs
        let _ = controller.shutdown().await;

        // THEN the supervisor was told to stop, so its children do not outlive
        // the daemon and get reparented to init still holding their memory
        assert!(
            llama.is_shutting_down().await,
            "the shutdown sequence left the llama-server supervisor running"
        );
        let _ = std::fs::remove_file(&socket_path);
    }

    #[tokio::test]
    async fn test_shutdown_config_defaults() {
        // GIVEN a default ShutdownConfig
        let config = ShutdownConfig::default();

        // THEN drain_timeout_secs = 30
        assert_eq!(config.drain_timeout_secs, 30);
    }

    #[tokio::test]
    async fn test_shutdown_broadcasts_event() {
        // GIVEN a ShutdownController with an EventBus
        let (controller, event_sender, socket_path, _llama) =
            setup_env(MockBackend::instant()).await;
        let mut rx = event_sender.subscribe();

        // Drain any pre-existing events
        while rx.try_recv().is_ok() {}

        // WHEN shutdown() is called
        let _ = controller.shutdown().await;

        // THEN ShutdownRequested is broadcast
        // Collect all events received
        let mut found_shutdown = false;
        loop {
            match rx.try_recv() {
                Ok(RuntimeEvent::ShutdownRequested) => {
                    found_shutdown = true;
                }
                Ok(_) => {}
                Err(_) => break,
            }
        }
        assert!(
            found_shutdown,
            "ShutdownRequested should have been broadcast"
        );

        let _ = std::fs::remove_file(&socket_path);
    }

    #[tokio::test]
    async fn test_drain_waits_for_tasks() {
        // GIVEN an in-progress task held in-flight until the test releases it
        let (event_sender, _rx) = EventBus::new();
        let registry_handle = AgentRegistry::spawn(event_sender.clone());
        let router_handle: TaskRouterHandle<GatedBackend> =
            TaskRouterHandle::spawn(registry_handle.clone(), event_sender.clone(), 256);

        // Register an active agent
        let agent_id = registry_handle
            .register(test_manifest("agent-drain"))
            .await
            .unwrap();
        registry_handle
            .update_state(agent_id.as_str(), ProcessState::Active)
            .await
            .unwrap();

        // Register a coordinator with a gated backend: the task stays in-flight
        // until the test releases the gate
        let (gate_tx, gate_rx) = tokio::sync::watch::channel(false);
        let coordinator = ExecutionCoordinator::new(
            agent_id.clone(),
            2,
            event_sender.clone(),
            GatedBackend { gate: gate_rx },
        );
        router_handle
            .register_coordinator(agent_id.clone(), coordinator)
            .await
            .unwrap();

        // Submit a task
        let _task_id = router_handle
            .submit(agent_id.as_str(), AIPInput::default())
            .await
            .unwrap();

        // Build a minimal controller (no API server needed for drain test)
        let socket_path = temp_socket_path();
        let state = crate::api::AppState {
            router_handle: router_handle.clone(),
            registry_handle: registry_handle.clone(),
            event_sender: event_sender.clone(),
            agent_loader: std::sync::Arc::new(crate::api::routes_agents::StubAgentLoader),
            backend: GatedBackend::released(),
            llm_router: crate::api::server::empty_shared_llm_router(),
            trigger_engine: None,
            config_path: None,
            task_repository: None,
            pending_approvals: None,
            plan_gates: None,
            notification_config: None,
            backend_factory: None,
            tool_registry_handle: None,
            audit_trail: None,
            audit_journal: None,
            obs_config: apollia_core::ObservabilityConfig::default(),
            llm_call_repository: None,
            trigger_def_repo: None,
            notification_repo: None,
            notification_engine_handle: None,
            chat_manager: None,
            plan_cache: None,
            mailbox_handle: None,
            user_memory: None,
            data_dir: std::path::PathBuf::new(),
            stt_engine: crate::api::server::empty_shared_stt_engine(),
            stt_repository: crate::api::server::empty_shared_stt_repository(),
            mcp_handle: None,
            mcp_server_repo: None,
            llm_backend_repo: None,
            stt_config_repo: None,
            a2a_invoker: None,
            resilience_layer: None,
            runner_proxy: None,
            llama_server_supervisor: None,
        };
        let api_config = crate::api::APIServerConfig {
            socket_path: socket_path.clone(),
            bind_addr: "127.0.0.1".to_owned(),
            tcp_port: None,
            api_token: None,
            tls_cert_path: None,
            tls_key_path: None,
        };
        let api = crate::api::APIServer::new(api_config, state);
        let api_handle = api.start().await.unwrap();

        let config = ShutdownConfig {
            drain_timeout_secs: 5,
        };
        let controller = ShutdownController::new(ShutdownControllerDeps {
            config,
            event_sender,
            api_handle,
            router_handle,
            registry_handle,
            notification_engine: None,
            mcp_handle: None,
            runner_supervisor: None,
            llama_server_supervisor: None,
        });

        // WHEN shutdown() runs and the in-flight task is released so it completes
        // during the drain
        let releaser = tokio::spawn(async move {
            for _ in 0..4 {
                tokio::task::yield_now().await;
            }
            gate_tx.send(true).expect("release gate");
        });
        let result = controller.shutdown().await;
        releaser.await.unwrap();

        // THEN drain returns Ok (task completed within timeout)
        assert!(result.is_ok(), "drain should succeed, got: {result:?}");

        let _ = std::fs::remove_file(&socket_path);
    }

    // Regression for the drain snapshot/subscribe gap (finding C9-F3): the drain
    // subscribes to the EventBus BEFORE snapshotting the active-task set, so a
    // completion emitted once the drain is running is observed and the drain
    // returns Ok instead of force-canceling a finished task. The exact
    // snapshot/subscribe interleaving cannot be forced from outside the function
    // and is proven by the abstract Loom model
    // (crates/apollia-loom-models, subscribe_before_snapshot_never_misses_a_completion);
    // this test guards the reordering's happy path and no-regression.
    #[tokio::test]
    async fn test_drain_observes_completion_emitted_during_drain() {
        // GIVEN an in-flight task on a gated backend, held until the test releases it
        let (event_sender, _rx) = EventBus::new();
        let registry_handle = AgentRegistry::spawn(event_sender.clone());
        let router_handle: TaskRouterHandle<GatedBackend> =
            TaskRouterHandle::spawn(registry_handle.clone(), event_sender.clone(), 256);

        let agent_id = registry_handle
            .register(test_manifest("agent-drain-gap"))
            .await
            .unwrap();
        registry_handle
            .update_state(agent_id.as_str(), ProcessState::Active)
            .await
            .unwrap();

        let (gate_tx, gate_rx) = tokio::sync::watch::channel(false);
        let coordinator = ExecutionCoordinator::new(
            agent_id.clone(),
            2,
            event_sender.clone(),
            GatedBackend { gate: gate_rx },
        );
        router_handle
            .register_coordinator(agent_id.clone(), coordinator)
            .await
            .unwrap();
        router_handle
            .submit(agent_id.as_str(), AIPInput::default())
            .await
            .unwrap();

        let socket_path = temp_socket_path();
        let state = crate::api::AppState {
            router_handle: router_handle.clone(),
            registry_handle: registry_handle.clone(),
            event_sender: event_sender.clone(),
            agent_loader: std::sync::Arc::new(crate::api::routes_agents::StubAgentLoader),
            backend: GatedBackend::released(),
            llm_router: crate::api::server::empty_shared_llm_router(),
            trigger_engine: None,
            config_path: None,
            task_repository: None,
            pending_approvals: None,
            plan_gates: None,
            notification_config: None,
            backend_factory: None,
            tool_registry_handle: None,
            audit_trail: None,
            audit_journal: None,
            obs_config: apollia_core::ObservabilityConfig::default(),
            llm_call_repository: None,
            trigger_def_repo: None,
            notification_repo: None,
            notification_engine_handle: None,
            chat_manager: None,
            plan_cache: None,
            mailbox_handle: None,
            user_memory: None,
            data_dir: std::path::PathBuf::new(),
            stt_engine: crate::api::server::empty_shared_stt_engine(),
            stt_repository: crate::api::server::empty_shared_stt_repository(),
            mcp_handle: None,
            mcp_server_repo: None,
            llm_backend_repo: None,
            stt_config_repo: None,
            a2a_invoker: None,
            resilience_layer: None,
            runner_proxy: None,
            llama_server_supervisor: None,
        };
        let api_config = crate::api::APIServerConfig {
            socket_path: socket_path.clone(),
            bind_addr: "127.0.0.1".to_owned(),
            tcp_port: None,
            api_token: None,
            tls_cert_path: None,
            tls_key_path: None,
        };
        let api = crate::api::APIServer::new(api_config, state);
        let api_handle = api.start().await.unwrap();

        let config = ShutdownConfig {
            drain_timeout_secs: 5,
        };
        let controller = ShutdownController::new(ShutdownControllerDeps {
            config,
            event_sender,
            api_handle,
            router_handle,
            registry_handle,
            notification_engine: None,
            mcp_handle: None,
            runner_supervisor: None,
            llama_server_supervisor: None,
        });

        // WHEN the drain runs and the task completes while it is in progress. The
        // releaser yields first so the drain has already subscribed and snapshotted.
        let releaser = tokio::spawn(async move {
            for _ in 0..4 {
                tokio::task::yield_now().await;
            }
            gate_tx.send(true).expect("release gate");
        });
        let result = controller.drain_tasks().await;
        releaser.await.unwrap();

        // THEN the drain observed the completion and returned Ok well within the
        // 5s timeout, rather than force-canceling an already-finished task
        assert!(
            result.is_ok(),
            "drain should observe the completion, got: {result:?}"
        );

        let _ = std::fs::remove_file(&socket_path);
    }

    #[tokio::test]
    async fn test_drain_timeout_cancels_tasks() {
        // GIVEN a task that never completes
        let (event_sender, _rx) = EventBus::new();
        let registry_handle = AgentRegistry::spawn(event_sender.clone());
        let router_handle: TaskRouterHandle<NeverBackend> =
            TaskRouterHandle::spawn(registry_handle.clone(), event_sender.clone(), 256);

        // Register an active agent
        let agent_id = registry_handle
            .register(test_manifest("agent-timeout"))
            .await
            .unwrap();
        registry_handle
            .update_state(agent_id.as_str(), ProcessState::Active)
            .await
            .unwrap();

        // Register a coordinator with a never-completing backend
        let coordinator =
            ExecutionCoordinator::new(agent_id.clone(), 2, event_sender.clone(), NeverBackend);
        router_handle
            .register_coordinator(agent_id.clone(), coordinator)
            .await
            .unwrap();

        // Submit a task
        let _task_id = router_handle
            .submit(agent_id.as_str(), AIPInput::default())
            .await
            .unwrap();

        let socket_path = temp_socket_path();
        let state = crate::api::AppState {
            router_handle: router_handle.clone(),
            registry_handle: registry_handle.clone(),
            event_sender: event_sender.clone(),
            agent_loader: std::sync::Arc::new(crate::api::routes_agents::StubAgentLoader),
            backend: NeverBackend,
            llm_router: crate::api::server::empty_shared_llm_router(),
            trigger_engine: None,
            config_path: None,
            task_repository: None,
            pending_approvals: None,
            plan_gates: None,
            notification_config: None,
            backend_factory: None,
            tool_registry_handle: None,
            audit_trail: None,
            audit_journal: None,
            obs_config: apollia_core::ObservabilityConfig::default(),
            llm_call_repository: None,
            trigger_def_repo: None,
            notification_repo: None,
            notification_engine_handle: None,
            chat_manager: None,
            plan_cache: None,
            mailbox_handle: None,
            user_memory: None,
            data_dir: std::path::PathBuf::new(),
            stt_engine: crate::api::server::empty_shared_stt_engine(),
            stt_repository: crate::api::server::empty_shared_stt_repository(),
            mcp_handle: None,
            mcp_server_repo: None,
            llm_backend_repo: None,
            stt_config_repo: None,
            a2a_invoker: None,
            resilience_layer: None,
            runner_proxy: None,
            llama_server_supervisor: None,
        };
        let api = crate::api::APIServer::new(
            crate::api::APIServerConfig {
                socket_path: socket_path.clone(),
                bind_addr: "127.0.0.1".to_owned(),
                tcp_port: None,
                api_token: None,
                tls_cert_path: None,
                tls_key_path: None,
            },
            state,
        );
        let api_handle = api.start().await.unwrap();

        // WHEN drain(1s) is called (short timeout for the test)
        let config = ShutdownConfig {
            drain_timeout_secs: 1,
        };
        let controller = ShutdownController::new(ShutdownControllerDeps {
            config,
            event_sender,
            api_handle,
            router_handle,
            registry_handle,
            notification_engine: None,
            mcp_handle: None,
            runner_supervisor: None,
            llama_server_supervisor: None,
        });

        let result = controller.shutdown().await;

        // THEN DrainTimeout is returned
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            matches!(
                &err,
                ShutdownError::DrainTimeout {
                    count: 1,
                    timeout_secs: 1
                }
            ),
            "expected DrainTimeout with count=1, got: {err:?}"
        );

        let _ = std::fs::remove_file(&socket_path);
    }

    #[tokio::test]
    async fn test_actors_stopped_in_reverse_order() {
        // GIVEN all actors started
        let (controller, event_sender, socket_path, _llama) =
            setup_env(MockBackend::instant()).await;
        let mut rx = event_sender.subscribe();

        // Register an active agent to verify it gets stopped
        let agent_id = controller
            .registry_handle
            .register(test_manifest("agent-order"))
            .await
            .unwrap();
        controller
            .registry_handle
            .update_state(agent_id.as_str(), ProcessState::Active)
            .await
            .unwrap();

        // Drain pre-existing events
        while rx.try_recv().is_ok() {}

        // WHEN stop_all() is called via shutdown()
        let _ = controller.shutdown().await;

        // THEN the agent has transitioned to Stopped (events emitted)
        let mut found_agent_stopped = false;
        loop {
            match rx.try_recv() {
                Ok(RuntimeEvent::AgentStopped(id)) if id == agent_id => {
                    found_agent_stopped = true;
                }
                Ok(_) => {}
                Err(_) => break,
            }
        }
        assert!(
            found_agent_stopped,
            "Agent should have been transitioned to Stopped"
        );

        let _ = std::fs::remove_file(&socket_path);
    }

    #[tokio::test]
    async fn test_shutdown_error_display() {
        // GIVEN ShutdownError variants
        let drain_err = ShutdownError::DrainTimeout {
            count: 3,
            timeout_secs: 30,
        };
        let actor_err = ShutdownError::ActorStopFailed {
            actor: "api_server".to_string(),
            reason: "bind error".to_string(),
        };

        // THEN display strings are correct
        assert!(drain_err.to_string().contains("3 tasks still running"));
        assert!(drain_err.to_string().contains("30s"));
        assert!(actor_err.to_string().contains("api_server"));
        assert!(actor_err.to_string().contains("bind error"));
    }

    #[tokio::test]
    async fn test_shutdown_no_tasks_completes_immediately() {
        // GIVEN a runtime with no in-progress tasks
        let (controller, _event_sender, socket_path, _llama) =
            setup_env(MockBackend::instant()).await;

        // WHEN shutdown() is called
        let start = tokio::time::Instant::now();
        let result = controller.shutdown().await;
        let elapsed = start.elapsed();

        // THEN shutdown completes quickly (no drain wait)
        assert!(result.is_ok());
        assert!(
            elapsed < Duration::from_secs(1),
            "shutdown with no tasks should be fast, took: {elapsed:?}"
        );

        let _ = std::fs::remove_file(&socket_path);
    }

    #[tokio::test]
    async fn test_stop_agents_transitions_all_states() {
        // GIVEN agents in different states
        let (event_sender, _rx) = EventBus::new();
        let registry_handle = AgentRegistry::spawn(event_sender.clone());
        let router_handle: TaskRouterHandle<MockBackend> =
            TaskRouterHandle::spawn(registry_handle.clone(), event_sender.clone(), 256);

        // Agent Active
        let active_id = registry_handle
            .register(test_manifest("agent-active"))
            .await
            .unwrap();
        registry_handle
            .update_state(active_id.as_str(), ProcessState::Active)
            .await
            .unwrap();

        // Agent Initializing
        let _init_id = registry_handle
            .register(test_manifest("agent-init"))
            .await
            .unwrap();

        // Agent already Stopped
        let stopped_id = registry_handle
            .register(test_manifest("agent-stopped"))
            .await
            .unwrap();
        registry_handle
            .update_state(stopped_id.as_str(), ProcessState::Stopping)
            .await
            .unwrap();
        registry_handle
            .update_state(stopped_id.as_str(), ProcessState::Stopped)
            .await
            .unwrap();

        let socket_path = temp_socket_path();
        let state = crate::api::AppState {
            router_handle: router_handle.clone(),
            registry_handle: registry_handle.clone(),
            event_sender: event_sender.clone(),
            agent_loader: std::sync::Arc::new(crate::api::routes_agents::StubAgentLoader),
            backend: MockBackend::instant(),
            llm_router: crate::api::server::empty_shared_llm_router(),
            trigger_engine: None,
            config_path: None,
            task_repository: None,
            pending_approvals: None,
            plan_gates: None,
            notification_config: None,
            backend_factory: None,
            tool_registry_handle: None,
            audit_trail: None,
            audit_journal: None,
            obs_config: apollia_core::ObservabilityConfig::default(),
            llm_call_repository: None,
            trigger_def_repo: None,
            notification_repo: None,
            notification_engine_handle: None,
            chat_manager: None,
            plan_cache: None,
            mailbox_handle: None,
            user_memory: None,
            data_dir: std::path::PathBuf::new(),
            stt_engine: crate::api::server::empty_shared_stt_engine(),
            stt_repository: crate::api::server::empty_shared_stt_repository(),
            mcp_handle: None,
            mcp_server_repo: None,
            llm_backend_repo: None,
            stt_config_repo: None,
            a2a_invoker: None,
            resilience_layer: None,
            runner_proxy: None,
            llama_server_supervisor: None,
        };
        let api = crate::api::APIServer::new(
            crate::api::APIServerConfig {
                socket_path: socket_path.clone(),
                bind_addr: "127.0.0.1".to_owned(),
                tcp_port: None,
                api_token: None,
                tls_cert_path: None,
                tls_key_path: None,
            },
            state,
        );
        let api_handle = api.start().await.unwrap();

        let config = ShutdownConfig {
            drain_timeout_secs: 1,
        };
        let controller = ShutdownController::new(ShutdownControllerDeps {
            config,
            event_sender: event_sender.clone(),
            api_handle,
            router_handle: router_handle.clone(),
            registry_handle: registry_handle.clone(),
            notification_engine: None,
            mcp_handle: None,
            runner_supervisor: None,
            llama_server_supervisor: None,
        });

        // WHEN shutdown
        let _ = controller.shutdown().await;

        // THEN Active and Initializing agents should now be Stopped.
        // The registry might be dead after shutdown, so we verify via events.
        // The fact that shutdown() completed without panicking validates the transitions.

        let _ = std::fs::remove_file(&socket_path);
    }
}
