//! `apollia-os start` — start the runtime in foreground.
//!
//! Uses the Supervisor for ordered startup (EventBus → AgentRegistry → TaskRouter
//! → APIServer) with timeout and rollback on failure. Shutdown is handled by the
//! ShutdownController with graceful drain.

use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use std::time::Instant;

use apollia_aip::bridge::AIPBridge;
use apollia_aip::context::RuntimeContext;
use apollia_core::{AIPResult, AIPTask, AgentManifest, RuntimeEvent, TaskStatus};
use apollia_llm::{
    CompletionModel, CompletionRequest, CompletionResponse, LlmError, LlmRouter,
    ObservabilityConfig, StepBudgetView, ToolCallHelper, ToolInvoker,
};
use apollia_runtime::api::routes_agents::{AgentBackendFactory, AgentLoader};
use apollia_runtime::api::APIServerConfig;
use apollia_runtime::coordinator::{DynBackend, ExecutionBackend};
use apollia_runtime::eventbus::EventBusSender;
use apollia_runtime::shutdown::{ShutdownConfig, ShutdownController};
use apollia_runtime::supervisor::{Supervisor, SupervisorConfig};
use futures::stream;
use pyo3::prelude::*;

use crate::client::{DEFAULT_SOCKET_PATH, DEFAULT_TCP_PORT};

/// Errors that can occur during runtime startup.
#[derive(Debug, thiserror::Error)]
pub enum StartError {
    /// Supervisor failed to start actors.
    #[error("failed to start runtime: {0}")]
    Supervisor(#[from] apollia_runtime::supervisor::SupervisorError),
    /// Config file found but invalid.
    #[error("invalid config file {path}: {reason}")]
    Config {
        path: std::path::PathBuf,
        reason: String,
    },
}

/// Real agent loader using AIPLoader + validate_agent (ADR-019).
///
/// Loads a Python module via PyO3, validates AIP duck typing, and returns
/// the deserialized [`AgentManifest`].
struct AIPAgentLoader;

impl AgentLoader for AIPAgentLoader {
    fn load_and_validate(&self, path: &Path) -> Result<AgentManifest, String> {
        let module = apollia_aip::loader::load_agent_module(path).map_err(|e| e.to_string())?;
        let validated =
            apollia_aip::validator::validate_agent(&module).map_err(|e| e.to_string())?;
        Ok(validated.manifest)
    }
}

/// Fallback backend — only used when agent loading fails at start time.
///
/// Returns a `Failed` result immediately without calling Python.
#[derive(Clone)]
struct NoopBackend;

impl ExecutionBackend for NoopBackend {
    fn execute(
        &self,
        task: AIPTask,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<AIPResult, String>> + Send>> {
        Box::pin(async move {
            Ok(AIPResult {
                task_id: task.task_id,
                status: TaskStatus::Failed,
                output: Vec::new(),
                error: Some(apollia_core::AIPError {
                    code: "NO_BACKEND".to_string(),
                    message: "no execution backend configured for this agent".to_string(),
                    details: None,
                }),
                artifacts: Vec::new(),
                input_required_data: None,
            })
        })
    }
}

// ─────────────────────────────────────────────────────────────
// Stub LLM types required by ToolCallHelper constructor.
// RouterModel delegates to the real LlmRouter; NoopToolInvoker returns errors.
// These stubs are only invoked when an agent uses the LLM ReAct loop.
// ─────────────────────────────────────────────────────────────

struct RouterModel(Arc<LlmRouter>);

#[async_trait::async_trait]
impl CompletionModel for RouterModel {
    async fn complete(&self, req: CompletionRequest) -> Result<CompletionResponse, LlmError> {
        self.0
            .complete_with_observability(None, req, None, &ObservabilityConfig::default())
            .await
    }

    async fn stream(
        &self,
        _req: CompletionRequest,
    ) -> Result<Pin<Box<dyn futures::Stream<Item = Result<String, LlmError>> + Send>>, LlmError>
    {
        let s: Pin<Box<dyn futures::Stream<Item = Result<String, LlmError>> + Send>> =
            Box::pin(stream::empty());
        Ok(s)
    }

    fn is_available(&self) -> bool {
        !self.0.list().is_empty()
    }
    fn backend_name(&self) -> &str { "router" }
    fn model_id(&self) -> &str { "router" }
}

struct NoopToolInvoker;

#[async_trait::async_trait]
impl ToolInvoker for NoopToolInvoker {
    async fn invoke(&self, name: &str, _args: &serde_json::Value) -> Result<String, String> {
        Err(format!("tool '{name}' invocation via LLM loop not wired — use ctx.tools directly"))
    }
}

// ─────────────────────────────────────────────────────────────
// Real per-agent execution backend (AIPBridge + RuntimeContext)
// ─────────────────────────────────────────────────────────────

/// Per-agent backend that calls Python via `AIPBridge`.
///
/// Created once per agent at start time by `ProductionBackendFactory`.
/// All fields are `Arc`-wrapped — cloning is cheap.
struct AIPProductionBackend {
    bridge: Arc<AIPBridge>,
    agent_id: String,
    llm_router: Option<Arc<LlmRouter>>,
    event_bus: EventBusSender,
}

impl Clone for AIPProductionBackend {
    fn clone(&self) -> Self {
        Self {
            bridge: Arc::clone(&self.bridge),
            agent_id: self.agent_id.clone(),
            llm_router: self.llm_router.clone(),
            event_bus: self.event_bus.clone(),
        }
    }
}

impl ExecutionBackend for AIPProductionBackend {
    fn execute(
        &self,
        task: AIPTask,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<AIPResult, String>> + Send>> {
        let bridge = Arc::clone(&self.bridge);
        let llm_router = self.llm_router.clone();
        let event_bus = self.event_bus.clone();
        let agent_id = self.agent_id.clone();

        Box::pin(async move {
            // Build the ToolCallHelper backed by the real LlmRouter (or an empty one).
            let router_for_helper = llm_router
                .clone()
                .unwrap_or_else(|| Arc::new(LlmRouter::empty()));
            let tool_helper = Arc::new(ToolCallHelper::new(
                Arc::new(RouterModel(router_for_helper)),
                Arc::new(NoopToolInvoker),
            ));

            // Build a RuntimeContext and wrap it as a Python object.
            let ctx: PyObject = Python::with_gil(|py| {
                let ctx = RuntimeContext::new_with_llm(
                    llm_router,
                    Arc::new(StepBudgetView::unlimited()),
                    tool_helper,
                    Arc::new(ObservabilityConfig::default()),
                    event_bus,
                    agent_id.into(),
                    None, // tool_proxy: None — direct tool access not wired in this sprint
                );
                Py::new(py, ctx)
                    .map(|p| p.into_any())
                    .expect("RuntimeContext PyObject construction failed")
            });

            bridge.call_run(&task, ctx).await.map_err(|e| e.to_string())
        })
    }
}

// ─────────────────────────────────────────────────────────────
// Factory — creates one AIPProductionBackend per agent at `agent start`
// ─────────────────────────────────────────────────────────────

/// Creates a real `AIPProductionBackend` per agent (ADR-019 extension).
///
/// Called once from `POST /api/v1/agents` — loads Python, validates AIP duck typing,
/// and bakes an `AIPBridge` into a backend registered with the `TaskRouter`.
///
/// Uses `OnceLock` for `event_bus` and `llm_router` because they are created
/// inside `supervisor.start()`, which runs after this factory is constructed.
/// Both locks are populated before the first HTTP request arrives.
struct ProductionBackendFactory {
    event_bus: Arc<std::sync::OnceLock<EventBusSender>>,
    llm_router: Arc<std::sync::OnceLock<Option<Arc<LlmRouter>>>>,
}

impl AgentBackendFactory for ProductionBackendFactory {
    fn create_for_agent(&self, agent_path: &Path, manifest: &AgentManifest) -> DynBackend {
        let agent_id = manifest.name.clone();

        // Retrieve the lazily-initialized event bus and LLM router.
        let event_bus = match self.event_bus.get() {
            Some(bus) => bus.clone(),
            None => {
                tracing::error!(
                    agent = %agent_id,
                    "event bus not initialized — factory called before supervisor.start() returned"
                );
                return DynBackend::new(NoopBackend);
            }
        };
        let llm_router = self.llm_router.get().cloned().flatten();

        let result: Result<AIPProductionBackend, String> = (|| {
            let module = apollia_aip::loader::load_agent_module(agent_path)
                .map_err(|e| e.to_string())?;
            let validated =
                apollia_aip::validator::validate_agent(&module).map_err(|e| e.to_string())?;
            let bridge = Arc::new(AIPBridge::new(validated).map_err(|e| e.to_string())?);
            Ok(AIPProductionBackend {
                bridge,
                agent_id: agent_id.clone(),
                llm_router,
                event_bus,
            })
        })();

        match result {
            Ok(backend) => DynBackend::new(backend),
            Err(e) => {
                tracing::error!(
                    agent = %agent_id,
                    path = %agent_path.display(),
                    error = %e,
                    "failed to load agent Python module — falling back to NoopBackend"
                );
                DynBackend::new(NoopBackend)
            }
        }
    }
}

/// Resolves `~` to `$HOME` in a path string.
fn expand_tilde_str(s: &str) -> PathBuf {
    if s.starts_with("~/") {
        let home = std::env::var("HOME").unwrap_or_default();
        PathBuf::from(format!("{}{}", home, &s[1..]))
    } else {
        PathBuf::from(s)
    }
}

/// Finds `apollia.toml` by searching in order:
///   1. `./apollia.toml`      (current working directory)
///   2. `~/.config/apollia/apollia.toml`  (user config dir)
/// Returns `None` if neither exists.
fn find_config_file() -> Option<PathBuf> {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let local = cwd.join("apollia.toml");
    if local.exists() {
        return Some(local);
    }
    let user_cfg = expand_tilde_str("~/.config/apollia/apollia.toml");
    if user_cfg.exists() {
        return Some(user_cfg);
    }
    None
}

/// Bootstrap and run the runtime in foreground.
///
/// Uses the Supervisor for ordered startup with timeout and rollback.
/// Blocks until Ctrl+C, SIGTERM, or `POST /api/v1/shutdown` is received.
/// Graceful shutdown drains in-progress tasks (30s default).
pub async fn run(socket: Option<PathBuf>, port: Option<u16>) -> Result<(), StartError> {
    let start = Instant::now();
    let socket_path = socket.unwrap_or_else(|| PathBuf::from(DEFAULT_SOCKET_PATH));
    let tcp_port = port.unwrap_or(DEFAULT_TCP_PORT);

    let home = std::env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| std::env::temp_dir());

    // Load apollia.toml if found.
    let (llm_config, triggers, notifications, pipelines, config_path) =
        match find_config_file() {
            Some(path) => {
                tracing::info!(config = %path.display(), "loading config");
                let cfg = crate::config::parse_apollia_toml(&path).map_err(|e| {
                    StartError::Config {
                        path: path.clone(),
                        reason: e.to_string(),
                    }
                })?;
                (cfg.llm, cfg.triggers, cfg.notifications, cfg.pipelines, Some(path))
            }
            None => {
                tracing::info!("no apollia.toml found — starting with defaults");
                (None, vec![], None, vec![], None)
            }
        };

    let trigger_count = triggers.len();
    let llm_label = llm_config
        .as_ref()
        .map(|l| format!("backend \"{}\"", l.default))
        .unwrap_or_else(|| "disabled".to_string());
    let notification_label = notifications
        .as_ref()
        .map(|n| {
            let count = n.channels.iter().filter(|c| c.enabled).count();
            format!("{count} channel(s)")
        })
        .unwrap_or_else(|| "disabled".to_string());
    let pipeline_label = if pipelines.is_empty() {
        "disabled (no [[pipelines]] defined)".to_string()
    } else {
        format!("{} pipeline(s)", pipelines.len())
    };

    // Start all actors via Supervisor (ordered, with timeout + rollback)
    let config = SupervisorConfig {
        api_config: APIServerConfig {
            socket_path: socket_path.clone(),
            tcp_port,
        },
        startup_timeout_secs: 10,
        llm_config,
        triggers,
        config_path,
        input_required_timeout_hours: 24,
        notifications,
        pipelines,
        data_dir: home.join(".apollia"),
    };
    let supervisor = Supervisor::new(config);
    let agent_loader: Arc<dyn AgentLoader> = Arc::new(AIPAgentLoader);

    // The ProductionBackendFactory needs the EventBusSender, which is created
    // inside supervisor.start(). We use a shared OnceLock so the factory can be
    // constructed before start() returns, then initialized lazily before first use.
    //
    // Safety: create_for_agent() is called only from POST /api/v1/agents, which
    // happens after the runtime is fully up — well after start() returns and the
    // OnceLock is populated.
    let event_bus_lock: Arc<std::sync::OnceLock<EventBusSender>> =
        Arc::new(std::sync::OnceLock::new());
    let llm_router_lock: Arc<std::sync::OnceLock<Option<Arc<LlmRouter>>>> =
        Arc::new(std::sync::OnceLock::new());

    let factory: Arc<dyn AgentBackendFactory> = Arc::new(ProductionBackendFactory {
        event_bus: event_bus_lock.clone(),
        llm_router: llm_router_lock.clone(),
    });

    let handles = supervisor
        .start(DynBackend::new(NoopBackend), agent_loader, Some(factory))
        .await?;

    // Populate the OnceLocks now that the supervisor is running.
    let _ = event_bus_lock.set(handles.event_sender.clone());
    let _ = llm_router_lock.set(handles.llm_router.clone());

    let elapsed = start.elapsed();
    println!("  * EventBus            ready");
    println!("  * AgentRegistry       ready");
    println!("  * ToolRegistry        ready (3 native tools)");
    println!("  * LlmRouter           {llm_label}");
    println!("  * TaskRouter          ready");
    println!("  * TriggerEngine       ready ({trigger_count} trigger(s))");
    println!("  * PipelineEngine      {pipeline_label}");
    println!(
        "  * APIServer           listening on {} + localhost:{}",
        socket_path.display(),
        tcp_port
    );
    println!("  * NotificationEngine  {notification_label}");
    println!("  -------------------------------------------------");
    println!("  * Runtime ready in {:.1}s", elapsed.as_secs_f64());
    println!();
    println!("  Press Ctrl+C or run `apollia-os stop` to shut down.");

    // Wait for shutdown signal (Ctrl+C, SIGTERM, or ShutdownRequested via API)
    let mut shutdown_rx = handles.event_sender.subscribe();
    tokio::select! {
        signal = apollia_runtime::shutdown::wait_for_shutdown_signal() => {
            println!();
            println!("  {signal} received, draining tasks...");
        }
        _ = wait_for_shutdown_event(&mut shutdown_rx) => {
            println!("  Shutdown requested via API, draining tasks...");
        }
    }

    // Graceful shutdown via ShutdownController (drain + ordered teardown)
    let tool_registry_handle = handles.tool_registry_handle;
    let shutdown = ShutdownController::new(
        ShutdownConfig::default(),
        handles.event_sender,
        handles.api_handle,
        handles.router_handle,
        handles.registry_handle,
    );

    match shutdown.shutdown().await {
        Ok(()) => println!("  * Runtime stopped."),
        Err(e) => eprintln!("  * Runtime stopped with warnings: {e}"),
    }

    // Stop the tool registry after the main shutdown sequence
    tool_registry_handle.shutdown().await;

    // Clean up socket file
    if socket_path.exists() {
        let _ = std::fs::remove_file(&socket_path);
    }

    Ok(())
}

/// Wait until a `RuntimeEvent::ShutdownRequested` event is received on the bus.
async fn wait_for_shutdown_event(rx: &mut tokio::sync::broadcast::Receiver<RuntimeEvent>) {
    loop {
        match rx.recv().await {
            Ok(RuntimeEvent::ShutdownRequested) => return,
            Ok(_) => continue,
            Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                tracing::warn!(lagged = n, "EventBus receiver lagged");
                continue;
            }
            Err(tokio::sync::broadcast::error::RecvError::Closed) => return,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_noop_backend_is_clone() {
        let backend = NoopBackend;
        let _cloned = backend.clone();
    }

    #[test]
    fn test_start_error_display() {
        let err = StartError::Supervisor(
            apollia_runtime::supervisor::SupervisorError::ConfigError("bad config".to_string()),
        );
        assert!(err.to_string().contains("bad config"));
    }
}
