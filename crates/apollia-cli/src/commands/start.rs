//! `apollia-os start`: start the runtime in foreground.
//!
//! Uses the Supervisor for ordered startup (EventBus, AgentRegistry, TaskRouter,
//! APIServer) with timeout and rollback on failure. Shutdown is handled by the
//! ShutdownController with graceful drain.

use std::path::PathBuf;
use std::time::Instant;

use apollia_core::subscribe_resilient;
use apollia_runtime::api::APIServerConfig;
use apollia_runtime::coordinator::DynBackend;
use apollia_runtime::shutdown::{ShutdownConfig, ShutdownController, ShutdownControllerDeps};
use apollia_runtime::supervisor::{Supervisor, SupervisorConfig};

use crate::client::default_socket_path;

pub(crate) use probe::find_config_file;
mod bootstrap;
mod probe;

use bootstrap::{load_start_config, resolve_tcp_port, wait_for_shutdown_event, NoopBackend};
use probe::{cleanup_stale_socket, port_is_in_use, socket_is_in_use};

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

/// Bootstrap and run the runtime in foreground.
///
/// Uses the Supervisor for ordered startup with timeout and rollback.
/// Blocks until Ctrl+C, SIGTERM, or `POST /api/v1/shutdown` is received.
/// Graceful shutdown drains in-progress tasks (30s default).
///
/// Returns `Ok(true)` when shutdown was triggered by SIGINT (Ctrl+C), so the
/// caller can exit with code 5 to distinguish voluntary interruption from
/// success (0) or error (1–4).
///
/// `config` is the configuration file named by `--config`; without it the
/// runtime reads `~/.apollia/apollia.toml`.
pub async fn run(
    socket: Option<PathBuf>,
    port: Option<u16>,
    config: Option<PathBuf>,
) -> Result<bool, StartError> {
    let start = Instant::now();
    let socket_path = socket.unwrap_or_else(default_socket_path);

    // Load apollia.toml if found. Agents, triggers, pipelines, notifications, and stt
    // are loaded from SQLite by the Supervisor; only static sections are parsed here.
    // Read before the port check, since `[api].port` names the port to check.
    let (loaded_config, config_path) = load_start_config(config.as_deref())?;
    let file_config = loaded_config.unwrap_or_default();
    // The live sections of the configuration: the runtime sends a saved
    // change on them.
    let config_channels = apollia_runtime::config_reload::ConfigChannels::new(&file_config);
    let tcp_port = resolve_tcp_port(port, &file_config.api);

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

    let daemon_settings = bootstrap::daemon_file_settings(&file_config);
    let llm_config = file_config.llm;
    let llm_label = llm_config
        .as_ref()
        .map(|l| format!("backend \"{}\"", l.default))
        .unwrap_or_else(|| "disabled".to_string());

    // Open AgentRepository for auto-load at boot.
    let data_dir = apollia_core::paths::data_dir_under(home);
    let data_dir_for_package_tools = data_dir.clone();
    let data_dir_for_port = data_dir.clone();
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
    let api_cfg = file_config.api;
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
    let runtime_config = file_config.runtime;
    let agent_idle_unload = std::time::Duration::from_secs(runtime_config.agent_idle_unload_secs);
    let hitl_config = file_config.hitl;
    let tools_config = file_config.tools;
    let mcp_config = file_config.mcp;
    let hooks_config = file_config.hooks;
    let chat_config = file_config.chat;
    let config = SupervisorConfig {
        api_config: APIServerConfig {
            socket_path: socket_path.clone(),
            bind_addr,
            tcp_port: Some(tcp_port),
            tcp_listener: None,
            api_token,
            tls_cert_path: api_cfg.tls_cert.clone(),
            tls_key_path: api_cfg.tls_key.clone(),
        },
        startup_timeout_secs: daemon_settings.startup_timeout_secs,
        llm_config,
        config_path,
        runtime_config,
        hitl_config,
        data_dir,
        obs_config: daemon_settings.observability,
        agent_repository,
        package_repository,
        tools_config: tools_config.clone(),
        mcp_loading: apollia_mcp::session::LoadingMode::from(mcp_config.tool_loading),
        tool_search_limit: mcp_config.tool_search_limit,
        hooks_config,
        plan_mode_default: chat_config.plan_mode_default,
        chat_tool_turn_temperature: chat_config.tool_turn_temperature,
        workflows_config: file_config.workflows,
        config_channels: Some(config_channels),
    };
    let workflow_max_steps = config.workflows_config.max_steps;
    let supervisor = Supervisor::new(config);

    // An agent is a definition: the supervisor registers the stored ones, and
    // the agent executor runs them. No Python object stands for an agent.
    let handles = supervisor.start(DynBackend::new(NoopBackend), None).await?;

    // The Python tools of the installed packages join the registry, read from
    // their manifests; each module is imported on its first call only. The
    // onboarding tool the desktop installed keeps the profile it records.
    apollia_aip::package_tools::attach(
        &data_dir_for_package_tools,
        &handles.tool_registry_handle,
        handles.chat_manager.as_ref(),
        agent_idle_unload,
        &[apollia_aip::package_tools::ProfileGrant::onboarding()],
    )
    .await;
    // The workflows of the installed packages, read from the same manifests.
    // Their steps call the models through the router of the runtime, each
    // run bounded by the step limit of `[workflows]`.
    let workflow_models = handles.llm_router.clone().map(|router| {
        apollia_aip::package_tools::workflows::WorkflowModels::new(
            router,
            Some(handles.event_sender.clone()),
            workflow_max_steps,
        )
    });
    apollia_aip::package_tools::workflows::attach(
        &data_dir_for_package_tools,
        handles.workflows.as_ref(),
        workflow_models.clone(),
    )
    .await;
    // A package installed from now on brings its tools and its workflows at
    // once.
    apollia_runtime::packages::install_package_reload(apollia_aip::package_tools::reload_hook(
        apollia_aip::package_tools::PackageCode {
            data_dir: data_dir_for_package_tools.clone(),
            registry: handles.tool_registry_handle.clone(),
            chat: handles.chat_manager.clone(),
            grants: vec![apollia_aip::package_tools::ProfileGrant::onboarding()],
            engine: handles.workflows.clone(),
            models: workflow_models,
        },
    ));

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
    // The desktop application finds a daemon by this record, whatever port it
    // was given.
    if let Err(e) = apollia_runtime::api::write_port_file(&data_dir_for_port, tcp_port) {
        tracing::warn!(error = %e, port = tcp_port, "api.port_file.write_failed");
    }

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
        mlx_engine: handles.mlx_engine,
    });

    match shutdown.shutdown().await {
        Ok(()) => println!("  * Runtime stopped."),
        Err(e) => eprintln!("  * Runtime stopped with warnings: {e}"),
    }

    // Stop the tool registry after the main shutdown sequence
    tool_registry_handle.shutdown().await;

    apollia_runtime::api::remove_port_file(&data_dir_for_port);

    // Clean up socket file
    if socket_path.exists() {
        let _ = std::fs::remove_file(&socket_path);
    }

    Ok(interrupted)
}
#[cfg(test)]
mod tests {
    use super::*;

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
        // GIVEN a start failure wrapping a supervisor configuration error
        let err = StartError::Supervisor(
            apollia_runtime::supervisor::SupervisorError::ConfigError("bad config".to_string()),
        );
        // WHEN it is rendered for the operator
        // THEN the underlying cause is in the message rather than being swallowed by the wrapper
        assert!(err.to_string().contains("bad config"));
    }

    #[tokio::test]
    async fn test_a_task_reaching_the_noop_backend_is_answered_failed() {
        use apollia_runtime::coordinator::ExecutionBackend;

        // GIVEN a task sent straight to the backend of a registered agent
        let task = apollia_core::AIPTask {
            task_id: "task-1".to_string(),
            ..apollia_core::AIPTask::default()
        };

        // WHEN the backend executes it
        let result = NoopBackend.execute(task).await.expect("an answer");

        // THEN it is answered, failed, under its own identifier
        assert_eq!(result.task_id, "task-1");
        assert_eq!(result.status, apollia_core::TaskStatus::Failed);
        assert_eq!(result.error.map(|e| e.code).as_deref(), Some("NO_BACKEND"));
    }
}
