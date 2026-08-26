//! `apollia-os start`: start the runtime in foreground.
//!
//! Uses the Supervisor for ordered startup (EventBus, AgentRegistry, TaskRouter,
//! APIServer) with timeout and rollback on failure. Shutdown is handled by the
//! ShutdownController with graceful drain.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use apollia_core::{subscribe_resilient, AgentManifest, PendingApprovals};
use apollia_llm::LlmRouter;
use apollia_runtime::api::routes_agents::{AgentBackendFactory, AgentLoader};
use apollia_runtime::api::APIServerConfig;
use apollia_runtime::coordinator::DynBackend;
use apollia_runtime::eventbus::EventBusSender;
use apollia_runtime::registry::AgentRegistryHandle;
use apollia_runtime::router::TaskRouterHandle;
use apollia_runtime::shutdown::{ShutdownConfig, ShutdownController, ShutdownControllerDeps};
use apollia_runtime::supervisor::{Supervisor, SupervisorConfig};
use apollia_tools::tools::ask_user::PendingUserInputs;
use apollia_tools::{AuditTrailHandle, TaskRepository, ToolCredentialStore, ToolRegistryHandle};

use crate::client::{default_socket_path, DEFAULT_TCP_PORT};

mod chat_runner;
mod engine;
mod llm_glue;
mod runner;

pub(crate) use factory::find_config_file;
mod backend;
mod bootstrap;
mod factory;

use bootstrap::{
    load_start_config, rewire_auto_loaded_agents, set_lock_if_some, wait_for_shutdown_event,
};
use chat_runner::{AIPChatAgentRunner, NoopBackend};
use factory::{cleanup_stale_socket, port_is_in_use, socket_is_in_use, ProductionBackendFactory};

/// Errors that can occur during runtime startup.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
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
    /// A runtime is already listening on the requested port or socket.
    #[error("runtime already running on {address} - use `apollia-os stop` first")]
    AlreadyRunning { address: String },
    /// API token could not be loaded or generated while `require_token = true`.
    #[error("failed to load or generate API token: {0}")]
    ApiToken(#[from] apollia_runtime::api::TokenFileError),
}

/// Real agent loader using AIPLoader + validate_agent.
///
/// Loads a Python module via PyO3, validates AIP duck typing, and returns
/// the deserialized [`AgentManifest`].
struct AIPAgentLoader;

impl AgentLoader for AIPAgentLoader {
    fn load_and_validate(&self, path: &Path) -> Result<AgentManifest, String> {
        // Re-use the same sys.path assembly as `apollia-os agent
        // install / validate` so the runtime's POST /api/v1/agents path
        // resolves the SDK + the enclosing package root the same way
        // (otherwise installed agents that validated fine through the CLI
        // would 400 the moment the runtime tries to load them).
        let extras = crate::community::validation_sys_paths(path);
        let module = apollia_aip::loader::load_agent_module_with_sys_paths(path, &extras)
            .map_err(|e| e.to_string())?;
        let validated =
            apollia_aip::validator::validate_agent(&module).map_err(|e| e.to_string())?;
        Ok(validated.manifest)
    }
}

/// Open the shared [`ToolCredentialStore`] used for `ctx.secrets`.
///
/// Returns `None` when the governance database does not exist yet (first
/// start before `apollia-os tools secret set ...`) or when opening it fails.
/// The agent then gets `None` for every `ctx.secrets.get(key)`, consistent
/// with the non-fatal secrets semantics.
fn open_secret_store(data_dir: &Path) -> Option<Arc<std::sync::Mutex<ToolCredentialStore>>> {
    let db_path = data_dir.join(apollia_tools::GOVERNANCE_DB_FILENAME);
    if !db_path.exists() {
        return None;
    }
    let keyfile_path = data_dir.join(".keyfile");
    match ToolCredentialStore::new(&db_path, &keyfile_path) {
        Ok(store) => Some(Arc::new(std::sync::Mutex::new(store))),
        Err(e) => {
            tracing::warn!(
                target: "apollia.aip.secrets",
                error = %e,
                detail = "the agent sees no key",
                "secrets.store.open.failed"
            );
            None
        }
    }
}
/// Bootstrap and run the runtime in foreground.
///
/// Uses the Supervisor for ordered startup with timeout and rollback.
/// Blocks until Ctrl+C, SIGTERM, or `POST /api/v1/shutdown` is received.
/// Graceful shutdown drains in-progress tasks (30s default).
///
/// Returns `Ok(true)` when shutdown was triggered by SIGINT (Ctrl+C), so the
/// caller can exit with code 5 to distinguish voluntary interruption from
/// success (0) or error (1–4).
pub async fn run(socket: Option<PathBuf>, port: Option<u16>) -> Result<bool, StartError> {
    let start = Instant::now();
    let socket_path = socket.unwrap_or_else(default_socket_path);
    let tcp_port = port.unwrap_or(DEFAULT_TCP_PORT);

    // Detect an already-running runtime before attempting to bind, so the user
    // gets a clear message instead of a low-level "Address already in use" error.
    if port_is_in_use(tcp_port).await {
        return Err(StartError::AlreadyRunning {
            address: format!("localhost:{tcp_port}"),
        });
    }
    if socket_is_in_use(&socket_path).await {
        return Err(StartError::AlreadyRunning {
            address: socket_path.display().to_string(),
        });
    }
    // The file exists but nobody is listening: clean it up so the bind succeeds.
    cleanup_stale_socket(&socket_path);

    let home = apollia_core::paths::home_dir_or_temp();

    // Load apollia.toml if found. Agents, triggers, pipelines, notifications, and stt
    // are loaded from SQLite by the Supervisor; only static sections are parsed here.
    let (loaded_config, config_path) = load_start_config()?;
    let (
        llm_config,
        api_file_config,
        runtime_file_config,
        hitl_file_config,
        tools_file_config,
        mcp_file_config,
        hooks_file_config,
        chat_file_config,
    ) = match loaded_config {
        Some(cfg) => (
            cfg.llm,
            cfg.api,
            cfg.runtime,
            cfg.hitl,
            cfg.tools,
            cfg.mcp,
            cfg.hooks,
            cfg.chat,
        ),
        None => (None, None, None, None, None, None, None, None),
    };

    let llm_label = llm_config
        .as_ref()
        .map(|l| format!("backend \"{}\"", l.default))
        .unwrap_or_else(|| "disabled".to_string());

    // Open AgentRepository for auto-load at boot.
    let data_dir = apollia_core::paths::data_dir_under(home);
    let data_dir_for_chat = data_dir.clone();
    let agent_repository: Option<apollia_tools::AgentRepository> = {
        let db_path = data_dir.join(apollia_core::paths::DataFile::Agents.file_name());
        match apollia_tools::AgentRepository::open(&db_path) {
            Ok(repo) => {
                tracing::info!("agent.repository.opened");
                Some(repo)
            }
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    detail = "the auto-load is disabled",
                    "agent.repository.open.failed"
                );
                None
            }
        }
    };
    // Keep a separate handle so we can rebuild auto-loaded backends after the
    // Supervisor has finished and the factory OnceLocks are fully populated
    // (workaround for the Phase 11 init-order race; see post-rewire below).
    let agent_repository_for_rewire = agent_repository.clone();

    // Open PackageRepository for Phase 10.6 integrity check.
    let package_repository: Option<apollia_tools::PackageRepository> = {
        let db_path = data_dir.join(apollia_core::paths::DataFile::Agents.file_name());
        match apollia_tools::PackageRepository::open(&db_path) {
            Ok(repo) => Some(repo),
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    detail = "the package integrity check is disabled",
                    "package.repository.open.failed"
                );
                None
            }
        }
    };

    // Resolve [api] section (absent = all defaults).
    let api_cfg = api_file_config.unwrap_or_default();
    let bind_addr = api_cfg.bind.clone();

    // Load or generate the API token when require_token = true.
    // Principle #4 (Fail fast): if the token cannot be loaded or generated, refuse to start
    // rather than silently degrading to an unauthenticated API.
    let api_token: Option<String> = if api_cfg.require_token {
        let token = apollia_runtime::api::load_or_generate_token(&data_dir)
            .map_err(StartError::ApiToken)?;
        Some(token)
    } else {
        tracing::info!(reason = "require_token is false", "api.token.auth.disabled");
        None
    };

    // Start all actors via Supervisor (ordered, with timeout + rollback)
    let runtime_config = runtime_file_config.unwrap_or_default();
    let hitl_config = hitl_file_config.unwrap_or_default();
    let tools_config = tools_file_config.unwrap_or_default();
    let mcp_config = mcp_file_config.unwrap_or_default();
    let hooks_config = hooks_file_config.unwrap_or_default();
    let chat_config = chat_file_config.unwrap_or_default();
    let config = SupervisorConfig {
        api_config: APIServerConfig {
            socket_path: socket_path.clone(),
            bind_addr,
            tcp_port: Some(tcp_port),
            api_token,
            tls_cert_path: api_cfg.tls_cert.clone(),
            tls_key_path: api_cfg.tls_key.clone(),
        },
        startup_timeout_secs: 10,
        llm_config,
        config_path,
        runtime_config,
        hitl_config,
        data_dir,
        obs_config: apollia_core::ObservabilityConfig::default(),
        agent_repository,
        package_repository,
        bundled_agents_path: {
            // Look for agents/bundled/ adjacent to the binary, then in the current directory.
            let from_exe = std::env::current_exe()
                .ok()
                .and_then(|exe| exe.parent().map(|d| d.join("agents").join("bundled")))
                .filter(|p| p.exists());
            let from_cwd = std::env::current_dir()
                .ok()
                .map(|d| d.join("agents").join("bundled"))
                .filter(|p| p.exists());
            from_exe.or(from_cwd)
        },
        tools_config: tools_config.clone(),
        mcp_loading: apollia_mcp::session::LoadingMode::from(mcp_config.tool_loading),
        tool_search_limit: mcp_config.tool_search_limit,
        hooks_config,
        plan_mode_default: chat_config.plan_mode_default,
        chat_default_workspace: chat_config.default_workspace.clone(),
        chat_tool_turn_temperature: chat_config.tool_turn_temperature,
    };
    let supervisor = Supervisor::new(config);
    let agent_loader: Arc<dyn AgentLoader> = Arc::new(AIPAgentLoader);

    // The ProductionBackendFactory needs the EventBusSender, which is created
    // inside supervisor.start(). We use a shared OnceLock so the factory can be
    // constructed before start() returns, then initialized lazily before first use.
    //
    // Safety: create_for_agent() is called only from POST /api/v1/agents, which
    // happens after the runtime is fully up, well after start() returns and the
    // OnceLock is populated.
    let event_bus_lock: Arc<std::sync::OnceLock<EventBusSender>> =
        Arc::new(std::sync::OnceLock::new());
    let llm_router_lock: Arc<std::sync::OnceLock<Option<Arc<LlmRouter>>>> =
        Arc::new(std::sync::OnceLock::new());
    let tool_registry_lock: Arc<std::sync::OnceLock<ToolRegistryHandle>> =
        Arc::new(std::sync::OnceLock::new());
    let audit_trail_lock: Arc<std::sync::OnceLock<AuditTrailHandle>> =
        Arc::new(std::sync::OnceLock::new());
    let pending_approvals_lock: Arc<std::sync::OnceLock<Arc<PendingApprovals>>> =
        Arc::new(std::sync::OnceLock::new());
    #[allow(clippy::type_complexity)]
    let plan_cache_lock: Arc<
        std::sync::OnceLock<Arc<std::sync::Mutex<apollia_oria::plan_cache::PlanCacheRepository>>>,
    > = Arc::new(std::sync::OnceLock::new());
    let plan_gates_lock: Arc<std::sync::OnceLock<Arc<apollia_oria::PendingPlanGates>>> =
        Arc::new(std::sync::OnceLock::new());
    let task_repository_lock: Arc<std::sync::OnceLock<Arc<TaskRepository>>> =
        Arc::new(std::sync::OnceLock::new());
    let registry_lock: Arc<std::sync::OnceLock<AgentRegistryHandle>> =
        Arc::new(std::sync::OnceLock::new());
    let router_lock: Arc<std::sync::OnceLock<TaskRouterHandle<DynBackend>>> =
        Arc::new(std::sync::OnceLock::new());
    let user_memory_lock: Arc<
        std::sync::OnceLock<
            Option<Arc<std::sync::Mutex<apollia_memory::user_memory::UserMemoryRepository>>>,
        >,
    > = Arc::new(std::sync::OnceLock::new());
    let pending_user_inputs_lock: Arc<std::sync::OnceLock<PendingUserInputs>> =
        Arc::new(std::sync::OnceLock::new());
    let mcp_handle_lock: Arc<
        std::sync::OnceLock<Option<apollia_mcp::manager::McpClientManagerHandle>>,
    > = Arc::new(std::sync::OnceLock::new());

    let factory: Arc<dyn AgentBackendFactory> = Arc::new(ProductionBackendFactory {
        event_bus: event_bus_lock.clone(),
        llm_router: llm_router_lock.clone(),
        tool_registry: tool_registry_lock.clone(),
        audit_trail: audit_trail_lock.clone(),
        pending_approvals: pending_approvals_lock.clone(),
        plan_gates: plan_gates_lock.clone(),
        plan_cache: plan_cache_lock.clone(),
        task_repository: task_repository_lock.clone(),
        registry: registry_lock.clone(),
        router: router_lock.clone(),
        tools_config: tools_config.clone(),
        data_dir: data_dir_for_chat.clone(),
        mcp_handle: mcp_handle_lock.clone(),
    });
    // Keep a handle for the post-supervisor rewire pass.
    let factory_for_rewire = factory.clone();

    // Concrete ChatAgentRunner for Chat Agent mode.
    let chat_agent_runner: Option<Arc<dyn apollia_runtime::chat::ChatAgentRunner>> =
        Some(Arc::new(AIPChatAgentRunner {
            event_bus: event_bus_lock.clone(),
            llm_router: llm_router_lock.clone(),
            tool_registry: tool_registry_lock.clone(),
            audit_trail: audit_trail_lock.clone(),
            user_memory: user_memory_lock.clone(),
            pending_user_inputs: pending_user_inputs_lock.clone(),
            agent_registry: registry_lock.clone(),
            task_router: router_lock.clone(),
            data_dir: data_dir_for_chat,
            tools_config,
            mcp_handle: mcp_handle_lock.clone(),
        }));

    let handles = supervisor
        .start(
            DynBackend::new(NoopBackend),
            agent_loader,
            Some(factory),
            chat_agent_runner,
        )
        .await?;

    // Populate the OnceLocks now that the supervisor is running.
    let _ = event_bus_lock.set(handles.event_sender.clone());
    let _ = llm_router_lock.set(handles.llm_router.clone());
    let _ = tool_registry_lock.set(handles.tool_registry_handle.clone());
    let _ = registry_lock.set(handles.registry_handle.clone());
    let _ = router_lock.set(handles.router_handle.clone());
    set_lock_if_some(&audit_trail_lock, handles.audit_trail.clone());
    set_lock_if_some(&pending_approvals_lock, handles.pending_approvals.clone());
    set_lock_if_some(&plan_gates_lock, handles.plan_gates.clone());
    set_lock_if_some(&plan_cache_lock, handles.plan_cache.clone());
    set_lock_if_some(&task_repository_lock, handles.task_repository.clone());
    let _ = user_memory_lock.set(handles.user_memory.clone());
    // Cloned (not moved) so the ShutdownController still receives handles.mcp_handle.
    let _ = mcp_handle_lock.set(handles.mcp_handle.clone());
    set_lock_if_some(
        &pending_user_inputs_lock,
        handles
            .chat_manager
            .as_ref()
            .map(|c| c.pending_user_inputs()),
    );

    // Rewire auto-loaded agents now that the factory's OnceLocks are populated.
    //
    // The Supervisor's Phase 11 (auto-load) calls factory.create_for_agent
    // BEFORE we get a chance to populate the OnceLocks the factory closes
    // over. As a result every auto-loaded agent is initially registered with
    // a NoopBackend that fails every task. Iterate the same repository the
    // Supervisor used and reissue create_for_agent (now wired) + replace the
    // coordinator in the router. Idempotent: register_coordinator inserts
    // into a HashMap so the previous entry is dropped cleanly.
    if let Some(ref repo) = agent_repository_for_rewire {
        rewire_auto_loaded_agents(repo, &factory_for_rewire, &handles, &handles.event_sender).await;
    }

    let elapsed = start.elapsed();
    // Query the registry for an accurate count instead of hard-coding the old
    // "3 native tools" string, which under-reports the ~60 native + connector
    // + MCP tools the runtime actually registers at boot.
    let tool_count_display = match handles.tool_registry_handle.list().await {
        Ok(descriptors) => format!("{} tools", descriptors.len()),
        Err(_) => "tool count unavailable".to_string(),
    };
    println!("  * EventBus            ready");
    println!("  * AgentRegistry       ready");
    println!("  * ToolRegistry        ready ({tool_count_display})");
    println!("  * LlmRouter           {llm_label}");
    println!("  * TaskRouter          ready");
    println!("  * TriggerEngine       ready (loaded from SQLite)");
    println!("  * PipelineEngine      ready (loaded from SQLite)");
    println!(
        "  * APIServer           listening on {} + localhost:{}",
        socket_path.display(),
        tcp_port
    );
    println!("  * NotificationEngine  ready (loaded from SQLite)");
    println!("  -------------------------------------------------");
    println!("  * Runtime ready in {:.1}s", elapsed.as_secs_f64());
    println!();
    println!("  Press Ctrl+C or run `apollia-os stop` to shut down.");

    // Wait for shutdown signal (Ctrl+C, SIGTERM, or ShutdownRequested via API)
    let mut shutdown_rx = subscribe_resilient(&handles.event_sender, "cli.start.shutdown");
    let interrupted = tokio::select! {
        signal = apollia_runtime::shutdown::wait_for_shutdown_signal() => {
            println!();
            println!("  {signal} received, draining tasks...");
            signal == "SIGINT"
        }
        _ = wait_for_shutdown_event(&mut shutdown_rx) => {
            println!("  Shutdown requested via API, draining tasks...");
            false
        }
    };

    // Graceful shutdown via ShutdownController (drain + ordered teardown)
    let tool_registry_handle = handles.tool_registry_handle;
    let shutdown = ShutdownController::new(ShutdownControllerDeps {
        config: ShutdownConfig::default(),
        event_sender: handles.event_sender,
        api_handle: handles.api_handle,
        router_handle: handles.router_handle,
        registry_handle: handles.registry_handle,
        notification_engine: handles.notification_engine,
        mcp_handle: handles.mcp_handle,
        runner_supervisor: handles.runner_supervisor,
        llama_server_supervisor: handles.llama_server_supervisor,
    });

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

    Ok(interrupted)
}
#[cfg(test)]
mod tests {
    use super::engine::{direct_path_budget, wire_engine_with_llm, AIPProductionBackend};
    use super::*;
    use apollia_aip::bridge::AIPBridge;
    use apollia_core::TaskStatus;
    use apollia_core::{AIPTask, StepBudgetConfig};
    use apollia_oria::engine::AIPAgent;
    use apollia_oria::engine::ORIAEngine;
    use apollia_runtime::coordinator::ExecutionBackend;
    use pyo3::prelude::*;

    // GIVEN the no-op backend the CLI installs when no engine is wired
    // WHEN a second owner of it is asked for
    // THEN the type supplies one. The red of a bound check is a compilation
    // error, which is what makes this one able to fail at all.
    #[test]
    fn test_noop_backend_is_clone() {
        fn assert_clone<T: Clone>() {}
        assert_clone::<NoopBackend>();
    }

    #[test]
    fn test_start_error_display() {
        let err = StartError::Supervisor(
            apollia_runtime::supervisor::SupervisorError::ConfigError("bad config".to_string()),
        );
        assert!(err.to_string().contains("bad config"));
    }

    // ── Direct-path budget guardrail (principle #7) ─────────────────────

    /// GIVEN an agent manifest declaring no budget (the unwrap_or_default case)
    /// WHEN the direct-path StepBudget is built
    /// THEN it is bounded, never the previous unlimited() (u32::MAX + 24h)
    #[test]
    fn test_direct_path_budget_is_bounded() {
        // GIVEN
        let agent = StepBudgetConfig::default();

        // WHEN
        let budget = direct_path_budget(&agent);

        // THEN it is bounded on every dimension
        assert!(budget.max_steps < u32::MAX, "steps must be bounded");
        assert!(
            budget.max_tool_calls < u32::MAX,
            "tool calls must be bounded"
        );
        assert!(
            budget.wall_clock_limit < std::time::Duration::from_secs(86_400),
            "wall clock must be bounded well under the old 24h unlimited() value"
        );
    }

    /// GIVEN an agent that declares an oversized budget
    /// WHEN the direct-path StepBudget is built
    /// THEN the runtime ceiling wins on every dimension (agent cannot exceed it)
    #[test]
    fn test_direct_path_budget_caps_oversized_manifest() {
        // GIVEN an agent asking for far more than the runtime ceiling
        let agent = StepBudgetConfig {
            max_steps: 100_000,
            max_tool_calls: 100_000,
            wall_clock_secs: 999_999,
        };

        // WHEN
        let budget = direct_path_budget(&agent);

        // THEN the runtime ceiling (StepBudgetConfig::default) clamps it
        let ceiling = StepBudgetConfig::default();
        assert_eq!(budget.max_steps, ceiling.max_steps);
        assert_eq!(budget.max_tool_calls, ceiling.max_tool_calls);
        assert_eq!(
            budget.wall_clock_limit,
            std::time::Duration::from_secs(ceiling.wall_clock_secs)
        );
    }

    // ── Orchestrated-routing regression guards ──────────────────────────
    //
    // Two failure modes were reported by manual testing of orchestrated
    // agents on an earlier runtime:
    //
    //  1. `apollia-os run <orchestrated>` returning
    //     `[NO_HANDLER] agent has neither @skill nor @on_message handler`,
    //     caused by AIPProductionBackend always dispatching to
    //     `execute_direct` (which goes through __apollia_dispatch__) even
    //     for orchestrated manifests. Fix: branch on
    //     `manifest.execution_mode == "orchestrated"` and call
    //     `engine.execute` instead.
    //
    //  2. `apollia-os run <orchestrated>` returning
    //     `[NO_LLM] Orchestrated mode requires a configured LLM
    //     (use with_reasoner())`, caused by the same fix routing to
    //     `engine.execute` without wiring a Reasoner. Fix:
    //     `wire_engine_with_llm` chains `.with_llm_router(...).
    //     with_reasoner(model, ...)` whenever the LlmRouter exposes
    //     a `precise` backend.
    //
    // The tests below cover the second mode at the engine boundary; the
    // first mode is enforced by the explicit branch in
    // `AIPProductionBackend::execute` and surfaces here as NO_LLM
    // (engine-level error) rather than NO_HANDLER (SDK-level error) when no
    // Reasoner is wired.

    /// Without an LlmRouter, the engine returned by `wire_engine_with_llm`
    /// has no Reasoner. Sanity check on the fast path.
    #[test]
    fn wire_engine_without_router_leaves_no_reasoner() {
        let engine = ORIAEngine::new();
        assert!(!engine.has_reasoner());

        let wired = wire_engine_with_llm(engine, None, "agent-under-test", 20);
        assert!(
            !wired.has_reasoner(),
            "wire_engine_with_llm(None) must not synthesise a Reasoner"
        );
    }

    /// `LlmRouter::empty()` carries no `[llm.routing]` section, so
    /// `route_precise()` errors out. The router is still attached (so
    /// step LLM calls would surface a descriptive error rather than
    /// silently noop), but no Reasoner is configured. Orchestrated
    /// execution will fail with NO_LLM at engine.execute() time, which
    /// is exactly what the second failure mode reported.
    #[test]
    fn wire_engine_with_empty_router_attaches_router_but_no_reasoner() {
        let engine = ORIAEngine::new();
        let router = Arc::new(LlmRouter::empty());

        let wired = wire_engine_with_llm(engine, Some(router), "agent-under-test", 20);

        assert!(
            !wired.has_reasoner(),
            "an empty LlmRouter must not produce a Reasoner - \
             would yield NO_LLM at runtime"
        );
    }

    /// End-to-end behaviour: an orchestrated agent submitted to an
    /// engine with no Reasoner must surface NO_LLM (not NO_HANDLER).
    ///
    /// Before the fix, `AIPProductionBackend::execute` routed every task
    /// through `execute_direct`, which goes through the SDK
    /// `__apollia_dispatch__`. An orchestrated manifest fell into the
    /// `failed("NO_HANDLER", ...)` branch of `dispatch_task`. After the
    /// fix, orchestrated tasks hit `engine.execute` directly; missing
    /// LLM is the only remaining failure mode and it has a stable code.
    /// Full-stack guard: builds a real `AIPProductionBackend` (PyO3 bridge
    /// + manifest + engine wiring) for an agent whose Python manifest
    /// declares `execution_mode = "orchestrated"`, and submits a task.
    ///
    /// The forged Python agent's `__apollia_dispatch__` returns a unique
    /// sentinel code (`SDK_DISPATCH_REACHED`) when invoked. If
    /// `AIPProductionBackend::execute` regresses to routing orchestrated
    /// tasks through `execute_direct` (the original mode 1), the result
    /// will carry that sentinel. If the routing stays correct but the
    /// Reasoner is missing (mode 2 wiring), the result will be ORIA's
    /// `NO_LLM`. Anything else means a regression.
    ///
    /// This is the test that, had it existed pre-patch, would have
    /// surfaced both modes immediately.
    #[tokio::test]
    async fn aip_production_backend_routes_orchestrated_to_oria_full_stack() {
        use apollia_aip::validator::validate_agent;
        use apollia_runtime::eventbus::EventBusSender;
        use pyo3::types::PyModule;
        use std::ffi::CString;

        // 1. Forge a minimal Python agent class with the orchestrated
        //    manifest shape the validator + bridge expect. We don't
        //    import the real Apollia SDK from this Rust test (it would
        //    require a pip-installed environment). The forged class has
        //    the exact attributes the validator and bridge introspect:
        //    `__apollia_manifest__` (dict) and async
        //    `__apollia_dispatch__`. The dispatch returns a sentinel so
        //    we can tell whether we ended up there (= routing regression).
        let code = r#"
class A:
    __apollia_manifest__ = {
        "name": "regression-orch-full",
        "version": "0.1.0",
        "description": "BUG-004 full-stack regression guard",
        "execution_mode": "orchestrated",
        "system_prompt": "Plan a tiny task and answer.",
        "tools_required": [],
    }

    async def __apollia_dispatch__(self, task, ctx):
        # Reaching this means AIPProductionBackend routed the task to
        # execute_direct (which goes through __apollia_dispatch__) - i.e.
        # BUG-004 mode 1 has regressed.
        return {
            "task_id": task.get("task_id", ""),
            "status": "failed",
            "output": [],
            "error": {
                "code": "SDK_DISPATCH_REACHED",
                "message": "orchestrated task was routed through SDK dispatch",
                "details": None,
            },
            "artifacts": [],
            "input_required_data": None,
        }

agent = A()
"#;

        let py_agent: pyo3::Py<pyo3::PyAny> = Python::with_gil(|py| {
            let code_c = CString::new(code).expect("test code contains NUL byte");
            let module = PyModule::from_code(
                py,
                &code_c,
                c"bug004_full_stack_test.py",
                c"bug004_full_stack_test",
            )
            .expect("failed to create test module");
            module
                .getattr("agent")
                .expect("forged module must expose `agent`")
                .into()
        });

        // 2. Validate via apollia_aip: confirms the manifest shape is
        //    well-formed and produces a ValidatedAgent with `execution_mode`
        //    correctly set.
        let validated = validate_agent(&py_agent).expect("validation must succeed");
        assert_eq!(
            validated.manifest.execution_mode, "orchestrated",
            "forged manifest must surface execution_mode = orchestrated"
        );
        let manifest_snapshot = validated.manifest.clone();

        // 3. Build the bridge.
        let bridge = Arc::new(AIPBridge::new(validated).expect("bridge construction failed"));

        // 4. Assemble a minimal AIPProductionBackend. Most optional
        //    components (tool registry, audit trail, A2A invoker, llm
        //    router, ...) are intentionally None so we exercise the
        //    no-LLM path. The branching we care about (execution_mode
        //    routing in `execute()`) is independent of these.
        let (event_bus, _event_rx): (EventBusSender, _) =
            apollia_runtime::eventbus::EventBus::new();
        let tmp = std::env::temp_dir().join("apollia-bug004-test");
        let _ = std::fs::create_dir_all(&tmp);
        let backend = AIPProductionBackend {
            bridge,
            agent_id: "regression-orch-full".to_string(),
            manifest: manifest_snapshot,
            allowed_tools: vec![],
            llm_router: None,
            event_bus,
            pending_approvals: None,
            plan_gates: None,
            plan_cache: None,
            task_repository: None,
            tool_registry: None,
            audit_trail: None,
            memory_namespace: None,
            memory_base_dir: tmp.clone(),
            user_memory_write: false,
            a2a_invoker: None,
            tools_config: apollia_core::ToolsConfig::default(),
            datasources_declared: vec![],
            templates_declared: vec![],
            agent_dir: Some(tmp.clone()),
            secrets_declared: vec![],
            secrets_data_dir: None,
            mcp_handle: None,
        };

        let task = AIPTask {
            task_id: "task-bug004-full".to_string(),
            ..AIPTask::default()
        };

        // 5. Run the full path. Must not panic, must produce an
        //    AIPResult, must NOT carry our SDK_DISPATCH_REACHED sentinel.
        let result = backend
            .execute(task)
            .await
            .expect("AIPProductionBackend::execute must not return Err");

        // 6. Assertions: routing went to ORIA (engine.execute), the
        //    missing Reasoner surfaced cleanly as NO_LLM, and the SDK
        //    dispatch path was never touched.
        assert_eq!(
            result.status,
            TaskStatus::Failed,
            "without an LLM router, orchestrated agents must Failed-fast"
        );
        let err = result.error.expect("Failed status must carry an AIPError");
        assert_ne!(
            err.code, "SDK_DISPATCH_REACHED",
            "regression: orchestrated task was routed through SDK dispatch \
             instead of engine.execute (BUG-004 mode 1)"
        );
        assert_ne!(
            err.code, "NO_HANDLER",
            "regression: orchestrated task fell through to SDK NO_HANDLER \
             branch (BUG-004 mode 1)"
        );
        assert!(
            err.code.to_uppercase().contains("LLM") || err.message.to_lowercase().contains("llm"),
            "expected an LLM-related error code, got code={} message={}",
            err.code,
            err.message
        );
    }

    #[tokio::test]
    async fn orchestrated_without_reasoner_yields_no_llm_not_no_handler() {
        let engine = ORIAEngine::new();
        assert!(!engine.has_reasoner());

        let manifest = AgentManifest {
            format_version: 1,
            name: "regression-orchestrated".to_string(),
            version: "0.1.0".to_string(),
            description: "BUG-004 regression guard".to_string(),
            tools_required: vec![],
            tools_optional: vec![],
            supports_streaming: false,
            supports_a2a: false,
            memory_namespace: None,
            shared_memory_namespaces: vec![],
            max_concurrent_tasks: 1,
            step_budget: None,
            network_allowlist: None,
            dangerous_tools_allowed: false,
            tags: vec![],
            skills: vec![],
            execution_mode: "orchestrated".to_string(),
            supports_mailbox: false,
            mailbox_allowlist: None,
            system_prompt: Some("You are a planning assistant.".to_string()),
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
        };

        struct MockOrchestratedAgent {
            manifest: AgentManifest,
        }
        impl AIPAgent for MockOrchestratedAgent {
            fn manifest(&self) -> AgentManifest {
                self.manifest.clone()
            }
        }
        let agent = MockOrchestratedAgent {
            manifest: manifest.clone(),
        };

        let task = AIPTask {
            task_id: "test-task-bug004".to_string(),
            ..AIPTask::default()
        };

        let result = engine.execute(task, &agent).await;

        assert_eq!(
            result.status,
            TaskStatus::Failed,
            "missing Reasoner must produce a Failed result"
        );
        let err = result.error.expect("Failed status must carry an AIPError");
        assert_ne!(
            err.code, "NO_HANDLER",
            "regression: orchestrated agents must not fall through to \
             SDK dispatch when the engine has no Reasoner"
        );
        assert!(
            err.message.to_lowercase().contains("llm") || err.code.to_uppercase().contains("LLM"),
            "expected an LLM-related error, got code={} message={}",
            err.code,
            err.message
        );
    }
}

#[cfg(test)]
mod orchestrated_step_args_master_proof {
    //! End-to-end proof for the orchestrated step-argument contract.
    //!
    //! Drives the orchestrated `ActorLoop` through the production `OriaToolProxy`
    //! over a real governed `ToolProxy`, executing a native tool that requires
    //! structured arguments, and asserts the file was written, the invocation was
    //! audited, and the tool-call budget was consumed.

    use std::pin::Pin;
    use std::sync::Arc;
    use std::time::Duration;

    use super::llm_glue::{OriaToolProxy, RouterModel};
    use apollia_aip::context::{DispatcherExecutor, ToolProxy, ToolProxyConfig};
    use apollia_core::plan::PlanStep;
    use apollia_core::{SandboxProfile, StepBudgetConfig, TaskStatus};
    use apollia_llm::{CompletionModel, LlmRouter};
    use apollia_oria::actor::{ActorLoop, StepDeps};
    use apollia_oria::budget::StepBudget;
    use apollia_oria::plan::ExecutionPlan;
    use apollia_oria::plan_repository::PlanRepository;
    use apollia_oria::reasoner::Reasoner;
    use apollia_oria::ResilienceLayer;
    use apollia_tools::{
        ToolDescriptor, ToolDispatcher, ToolExecutionError, ToolExecutor, ToolKind,
        ToolRegistryHandle,
    };
    use serde_json::Value;

    /// Native tool with a structured (path, content) input contract: writes the
    /// content to the path. Stands in for `file_write`/`bash`/`http` in the test.
    struct WriteNoteExecutor;

    impl ToolExecutor for WriteNoteExecutor {
        fn name(&self) -> &str {
            "write_note"
        }
        fn execute(
            &self,
            input: Value,
        ) -> Pin<Box<dyn std::future::Future<Output = Result<Value, ToolExecutionError>> + Send + '_>>
        {
            Box::pin(async move {
                let path = input.get("path").and_then(Value::as_str).ok_or_else(|| {
                    ToolExecutionError::InvalidInput {
                        message: "missing 'path'".to_string(),
                    }
                })?;
                let content = input
                    .get("content")
                    .and_then(Value::as_str)
                    .ok_or_else(|| ToolExecutionError::InvalidInput {
                        message: "missing 'content'".to_string(),
                    })?;
                std::fs::write(path, content).map_err(|e| ToolExecutionError::ExecutionFailed {
                    code: "io_error".to_string(),
                    message: e.to_string(),
                })?;
                Ok(Value::String(format!("wrote {} bytes", content.len())))
            })
        }
    }

    fn write_note_descriptor() -> ToolDescriptor {
        ToolDescriptor {
            name: "write_note".to_string(),
            version: "1.0.0".to_string(),
            description: "Write content to a file path".to_string(),
            kind: ToolKind::Native,
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string" },
                    "content": { "type": "string" }
                },
                "required": ["path", "content"]
            }),
            output_schema: None,
            sandbox_profile: SandboxProfile::FileSystem,
            tags: vec![],
            dangerous: false,
            is_read_only: false,
            risk_score: 5,
            approval_risk_level: None,
            impact_description: None,
            reject_reason_required: false,
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_orchestrated_native_tool_with_structured_args_executes_and_audits() {
        // GIVEN a real governed ToolProxy exposing a structured-args native tool
        let tmp = std::env::temp_dir().join(format!(
            "apollia_stepargs_{}_{}.txt",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        let _ = std::fs::remove_file(&tmp);

        let registry = ToolRegistryHandle::start();
        registry
            .register(write_note_descriptor())
            .await
            .expect("register descriptor");
        let audit_db = std::env::temp_dir().join(format!(
            "apollia_stepargs_audit_{}.db",
            uuid::Uuid::new_v4()
        ));
        let audit = apollia_tools::AuditTrailHandle::open(&audit_db)
            .await
            .expect("open audit trail");
        let dispatcher = Arc::new(ToolDispatcher::new(vec![Box::new(WriteNoteExecutor)]));
        let proxy = ToolProxy::new(ToolProxyConfig {
            registry: registry.clone(),
            audit: audit.clone(),
            executor: Arc::new(DispatcherExecutor::new(dispatcher)),
            allowed_tools: vec!["write_note".to_string()],
            agent_id: "orchestrated-agent".to_string(),
            task_id: "task-stepargs".to_string(),
            run_id: None,
        });
        let oria_proxy = OriaToolProxy { proxy };

        // AND a plan whose single tool step carries structured plan-time args
        // (path A output: the persisted plan is fully specified).
        let mut step = PlanStep::new("s1", "write the greeting to the note file");
        step.tool_hint = Some("write_note".to_string());
        step.args = Some(serde_json::json!({
            "path": tmp.to_str().expect("utf-8 path"),
            "content": "hello orchestrated"
        }));
        let plan = ExecutionPlan {
            plan_id: "p-stepargs".to_string(),
            task_id: "task-stepargs".to_string(),
            steps: vec![step],
        };

        let db = PlanRepository::new(":memory:").expect("in-memory plan db");
        db.insert_plan(&plan, "orchestrated-agent")
            .expect("insert_plan");
        db.insert_steps(&plan.plan_id, &plan.steps)
            .expect("insert_steps");

        let (bus_tx, _bus_rx) = tokio::sync::broadcast::channel(64);
        let manifest = serde_json::from_str(
            r#"{"name":"orch","version":"0.1.0","description":"t","tools_required":["write_note"]}"#,
        )
        .expect("minimal manifest");
        let mut actor = ActorLoop::new(plan, 0, db, bus_tx, manifest);

        let cap = StepBudgetConfig {
            max_steps: 10,
            max_tool_calls: 10,
            wall_clock_secs: 60,
        };
        let budget = StepBudget::from_capped(&cap, &cap);
        let router = LlmRouter::empty();
        let reasoner = Reasoner::new(
            Arc::new(RouterModel(Arc::new(LlmRouter::empty()))) as Arc<dyn CompletionModel>,
            10,
        );
        let resilience = ResilienceLayer::default();

        // WHEN the orchestrated ActorLoop executes the plan
        let result = actor
            .execute(
                StepDeps {
                    tool_proxy: &oria_proxy,
                    llm_router: &router,
                    budget: &budget,
                    reasoner: &reasoner,
                },
                &resilience,
            )
            .await;

        // THEN the plan completed and the native tool ran with the structured args
        assert_eq!(
            result.status,
            TaskStatus::Completed,
            "orchestrated run should complete: {:?}",
            result.error
        );
        let written = std::fs::read_to_string(&tmp).expect("note file must exist");
        assert_eq!(written, "hello orchestrated");

        // AND the invocation was recorded in the audit trail (governance holds)
        tokio::time::sleep(Duration::from_millis(150)).await;
        let records = audit.query_last(1).await;
        assert_eq!(records.len(), 1, "one audited invocation expected");
        assert_eq!(records[0].tool_name, "write_note");
        assert_eq!(records[0].agent_id, "orchestrated-agent");
        assert!(records[0].success);

        // AND the tool-call budget was consumed (guardrail applies)
        assert_eq!(budget.tool_calls_left(), 9);

        let _ = std::fs::remove_file(&tmp);
        registry.shutdown().await;
        audit.shutdown().await;
    }
}
