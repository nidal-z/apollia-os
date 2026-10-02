//! Apollia OS desktop application (Tauri v2).
//!
//! The Apollia runtime runs embedded inside the Tauri process via
//! [`apollia_runtime::init_embedded()`], on the TCP port this process claims
//! at startup (see [`startup`]); when a compatible runtime already serves that
//! port, the application attaches to it instead (see [`attached`]), and any
//! other holder is named in a refusal window. The Unix socket remains active
//! so the CLI can be used alongside the desktop app.
//!
//! `unsafe_code` is allowed by this crate's manifest for one production site,
//! the window-handle cast to `isize` in `commands/automation.rs`, and for
//! test-only `std::env::set_var` blocks; every site carries its `// SAFETY:`
//! comment.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]
mod attached;
mod backend;
mod bootstrap;
mod bundled_agents;
mod commands;
// REASON: this module is the content side of the in-app documentation reader.
// The two Tauri commands that read the manifest and a page are registered by
// the reader itself, together with the screen that calls them, because a
// command without a caller fails `scripts/check_tauri_ipc_callers.py`. Until
// then only the tests reach the module.
// TODO(once the documentation reader registers its manifest and page commands
// and a screen calls them): remove this attribute; the expectation turns red
// on that day and forces the removal.
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "reached only by its tests until the documentation reader registers its commands"
    )
)]
mod embedded_docs;
mod events;
mod handlers;
pub mod i18n;
pub mod mcp;
mod process_job;
mod project_context;
mod startup;
pub mod stt;
mod token_coalescer;
pub mod tray;

use std::sync::Arc;

use apollia_core::{PendingApprovals, ToolsConfig};
use apollia_llm::LlmRouter;
use apollia_memory::user_memory::UserMemoryRepository;
use apollia_runtime::api::routes_agents::{AgentBackendFactory, AgentLoader};
use apollia_runtime::coordinator::DynBackend;
use apollia_runtime::embedded::{EmbeddedConfig, RuntimeHandle};
use apollia_runtime::eventbus::EventBusSender;
use apollia_runtime::mailbox::AgentMailboxHandle;
use apollia_runtime::registry::AgentRegistryHandle;
use apollia_runtime::router::TaskRouterHandle;
use apollia_tools::tools::ask_user::PendingUserInputs;
use apollia_tools::{AgentRepository, AuditTrailHandle, TaskRepository, ToolRegistryHandle};
use mcp::McpRegistryClient;
use tauri::Manager;

use bootstrap::{
    auto_load_installed_agents, backfill_mcp_origins, load_stt_config, load_toml_config, or_exit,
    populate_runtime_locks, setup_bundled_python, setup_stt_hotkey, RuntimeLocks,
};

/// Shared mutable LLM router: updated by `reload_llm`, read by
/// `ProductionChatAgentRunner` and `ProductionBackendFactory`.
pub type SharedLlmRouter = Arc<std::sync::RwLock<Option<Arc<LlmRouter>>>>;

fn main() {
    // Initialize tracing so Rust logs appear in the terminal during development.
    // RUST_LOG controls verbosity (e.g. RUST_LOG=apollia=debug); defaults to info.
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("apollia=info,warn"));
    // The IMAP library traces the commands it sends, a login's password
    // included; its target stays off whatever `RUST_LOG` asks.
    let env_filter = match apollia_runtime::connectors_bridge::IMAP_LOG_DIRECTIVE
        .parse::<tracing_subscriber::filter::Directive>()
    {
        Ok(directive) => env_filter.add_directive(directive),
        Err(_) => env_filter,
    };
    tracing_subscriber::fmt().with_env_filter(env_filter).init();

    let apollia_data_dir = {
        let home = apollia_core::paths::home_dir_or_temp()
            .display()
            .to_string();
        apollia_core::paths::data_dir_under(home)
    };

    // Start, attach or refuse, before PyO3, any database or any listener of
    // the runtime. The claimed port is handed to the runtime below as is.
    let claimed_port = match startup::decide(
        std::net::SocketAddr::from(([127, 0, 0, 1], startup::RUNTIME_PORT)),
        startup::read_token(&apollia_data_dir).as_deref(),
    ) {
        startup::StartupDecision::Launch(listener) => listener,
        // A compatible runtime already serves: attach to it over its API,
        // with the token of this installation, and build nothing of our own.
        startup::StartupDecision::Attach { version } => {
            match startup::read_token(&apollia_data_dir) {
                Some(token) => {
                    tracing::event!(
                        tracing::Level::INFO,
                        version = %version,
                        "desktop.startup.attach"
                    );
                    attached::run(startup::RUNTIME_PORT, token, tauri_context())
                }
                None => startup::refuse(
                    &startup::RefusalReason::TokenRefused {
                        port: startup::RUNTIME_PORT,
                    },
                    &apollia_data_dir,
                    tauri_context(),
                ),
            }
        }
        startup::StartupDecision::Refuse(reason) => {
            startup::refuse(&reason, &apollia_data_dir, tauri_context())
        }
    };

    // Before the first child is spawned: membership in the job is inherited
    // at process creation, never applied afterwards. On Windows this is what
    // stops `llama-server` and the runner from outliving a host that dies
    // without running its exit hook. A refusal is logged and not fatal.
    match process_job::bind_children_to_this_process() {
        Ok(()) => tracing::info!("process.job.bound"),
        Err(error) => tracing::warn!(
            error = %error,
            detail = "children are killed on graceful exits only",
            "process.job.bind_failed"
        ),
    }

    // Point PyO3 at the bundled Python BEFORE any Python runtime code runs.
    setup_bundled_python();

    // OnceLocks shared between ProductionBackendFactory and main().
    // Populated after init_embedded() returns, before any HTTP request arrives.
    let event_bus_lock: Arc<std::sync::OnceLock<EventBusSender>> =
        Arc::new(std::sync::OnceLock::new());
    let llm_router_lock: Arc<std::sync::RwLock<Option<Arc<LlmRouter>>>> =
        Arc::new(std::sync::RwLock::new(None));
    let tool_registry_lock: Arc<std::sync::OnceLock<ToolRegistryHandle>> =
        Arc::new(std::sync::OnceLock::new());
    let audit_trail_lock: Arc<std::sync::OnceLock<AuditTrailHandle>> =
        Arc::new(std::sync::OnceLock::new());
    let pending_approvals_lock: Arc<std::sync::OnceLock<Arc<PendingApprovals>>> =
        Arc::new(std::sync::OnceLock::new());
    let task_repository_lock: Arc<std::sync::OnceLock<Arc<TaskRepository>>> =
        Arc::new(std::sync::OnceLock::new());
    let pending_user_inputs_lock: Arc<std::sync::OnceLock<PendingUserInputs>> =
        Arc::new(std::sync::OnceLock::new());
    // Populated post-init_embedded() so the BridgeRunner can inject one
    // McpToolExecutor per registered MCP tool into the agent's dispatcher.
    let mcp_handle_lock: Arc<std::sync::OnceLock<apollia_mcp::manager::McpClientManagerHandle>> =
        Arc::new(std::sync::OnceLock::new());
    // Agent registry + task router, required to build A2A delegate/invoker so
    // task-mode (triggers) and chat-agent flows can call `ctx.a2a_invoke(...)`
    // and discover virtual `a2a:*` tools, on parity with Chat Libre.
    let agent_registry_lock: Arc<std::sync::OnceLock<AgentRegistryHandle>> =
        Arc::new(std::sync::OnceLock::new());
    let task_router_lock: Arc<std::sync::OnceLock<TaskRouterHandle<DynBackend>>> =
        Arc::new(std::sync::OnceLock::new());
    // Inter-agent mailbox, exposed as `ctx.mailbox` in Python agents.
    let mailbox_handle_lock: Arc<std::sync::OnceLock<AgentMailboxHandle>> =
        Arc::new(std::sync::OnceLock::new());
    // Global user memory, used by Chat Agent runner to build `ctx.user_context`.
    let user_memory_lock: Arc<std::sync::OnceLock<Arc<std::sync::Mutex<UserMemoryRepository>>>> =
        Arc::new(std::sync::OnceLock::new());
    // Tools config (`[tools]` from apollia.toml) drives web_search/web_read
    // params and statically-disabled tools. Populated from RuntimeHandle.
    let trusted_paths_lock: Arc<std::sync::OnceLock<Vec<std::path::PathBuf>>> =
        Arc::new(std::sync::OnceLock::new());
    let tools_config_lock: Arc<std::sync::OnceLock<ToolsConfig>> =
        Arc::new(std::sync::OnceLock::new());
    // Plan-gate registry, forwarded to per-agent engines so the desktop UI can
    // resolve a pending plan gate via submit_plan_decision.
    let plan_gates_lock: Arc<std::sync::OnceLock<Arc<apollia_oria::PendingPlanGates>>> =
        Arc::new(std::sync::OnceLock::new());
    // Plan cache, forwarded to per-agent engines so an orchestrated run can
    // reuse a plan instead of re-planning. The supervisor owns the repository;
    // this shares it rather than opening a second database.
    #[allow(clippy::type_complexity)]
    let plan_cache_lock: Arc<
        std::sync::OnceLock<Arc<std::sync::Mutex<apollia_oria::plan_cache::PlanCacheRepository>>>,
    > = Arc::new(std::sync::OnceLock::new());

    let factory: Arc<dyn AgentBackendFactory> =
        Arc::new(backend::factory::ProductionBackendFactory {
            event_bus: event_bus_lock.clone(),
            llm_router: llm_router_lock.clone(),
            tool_registry: tool_registry_lock.clone(),
            audit_trail: audit_trail_lock.clone(),
            pending_approvals: pending_approvals_lock.clone(),
            task_repository: task_repository_lock.clone(),
            mcp_handle: mcp_handle_lock.clone(),
            agent_registry: agent_registry_lock.clone(),
            task_router: task_router_lock.clone(),
            mailbox_handle: mailbox_handle_lock.clone(),
            tools_config: tools_config_lock.clone(),
            trusted_paths: trusted_paths_lock.clone(),
            plan_gates: plan_gates_lock.clone(),
            plan_cache: plan_cache_lock.clone(),
            agents_db: Some(
                apollia_data_dir.join(apollia_core::paths::DataFile::Agents.file_name()),
            ),
        });

    // Shared SttFlow state for push-to-talk IPC commands (`start_tour_recording`,
    // `stop_tour_recording`). Populated inside the Tauri setup closure once the
    // STT engine is confirmed available; remains `None` for graceful degradation.
    let stt_flow_state: commands::stt::SttFlowState = Arc::new(std::sync::Mutex::new(None));

    // Open AgentRepository for auto-load at boot (passed to Supervisor).
    // A separate instance is created later for Tauri IPC commands.
    let _ = std::fs::create_dir_all(&apollia_data_dir);
    let boot_agent_repo = {
        let db_path = apollia_data_dir.join(apollia_core::paths::DataFile::Agents.file_name());
        match AgentRepository::open(&db_path) {
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

    // Extract bundled system agents (e.g. onboarding-agent) to disk and register
    // them in the repository so the auto-load loop picks them up.
    if let Some(ref repo) = boot_agent_repo {
        bundled_agents::ensure_bundled_agents(repo, &apollia_data_dir);
    }

    // Build the ChatAgentRunner so Chat Agent mode works in the ChatSessionManager.
    // Uses its own AgentRepository instance (SQLite WAL supports concurrent readers).
    let chat_agent_runner: Option<Arc<dyn apollia_runtime::chat::ChatAgentRunner>> = {
        let db_path = apollia_data_dir.join(apollia_core::paths::DataFile::Agents.file_name());
        match apollia_tools::AgentRepository::open(&db_path) {
            Ok(repo) => Some(Arc::new(backend::chat_runner::ProductionChatAgentRunner {
                agent_repo: Arc::new(std::sync::Mutex::new(repo)),
                event_bus: event_bus_lock.clone(),
                llm_router: llm_router_lock.clone(),
                tool_registry: tool_registry_lock.clone(),
                audit_trail: audit_trail_lock.clone(),
                pending_approvals: pending_approvals_lock.clone(),
                task_repository: task_repository_lock.clone(),
                pending_user_inputs: pending_user_inputs_lock.clone(),
                mcp_handle: mcp_handle_lock.clone(),
                agent_registry: agent_registry_lock.clone(),
                task_router: task_router_lock.clone(),
                mailbox_handle: mailbox_handle_lock.clone(),
                user_memory: user_memory_lock.clone(),
                tools_config: tools_config_lock.clone(),
                trusted_paths: trusted_paths_lock.clone(),
            })),
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    detail = "the chat-agent mode is disabled",
                    "chat.agent_store.open.failed"
                );
                None
            }
        }
    };

    // Load (or generate) the API bearer token so the embedded runtime binds an
    // authenticated TCP listener for the desktop REST bridge. The Unix socket
    // stays local-trust; the TCP port is never served unauthenticated.
    let api_token = or_exit(
        apollia_runtime::api::load_or_generate_token(&apollia_data_dir),
        "load or generate the API token",
    );
    commands::set_api_token(api_token.clone());

    // Do NOT pass agent_repository here: auto-load inside the Supervisor
    // happens before OnceLocks are populated, causing "event bus not initialized"
    // errors. Instead, we auto-load manually after OnceLocks are set below.
    let config = load_toml_config(EmbeddedConfig {
        agent_loader: Arc::new(backend::AIPAgentLoader),
        backend_factory: Some(factory.clone()),
        agent_repository: None,
        chat_agent_runner,
        tcp_port: Some(startup::RUNTIME_PORT),
        tcp_listener: Some(claimed_port),
        api_token: Some(api_token),
        ..EmbeddedConfig::default()
    });

    let runtime_handle: RuntimeHandle = or_exit(
        apollia_runtime::init_embedded(config),
        "start the embedded runtime",
    );

    // Pin the PyO3 async bridge onto the worker runtime now that it is up, before
    // any Python agent runs a coroutine.
    apollia_aip::pin_async_runtime();

    // Load the STT config from SQLite for the desktop hotkey listener.
    let stt_config_for_hotkey = load_stt_config(&apollia_data_dir);

    // Servers stored before their origin was recorded get one now, while the
    // desktop is the caller that knows the curated selection.
    backfill_mcp_origins(&apollia_data_dir);

    // Populate OnceLocks now that the supervisor is fully running.
    populate_runtime_locks(
        &runtime_handle,
        RuntimeLocks {
            event_bus: &event_bus_lock,
            llm_router: &llm_router_lock,
            tool_registry: &tool_registry_lock,
            audit_trail: &audit_trail_lock,
            pending_approvals: &pending_approvals_lock,
            task_repository: &task_repository_lock,
            pending_user_inputs: &pending_user_inputs_lock,
            mcp_handle: &mcp_handle_lock,
            agent_registry: &agent_registry_lock,
            task_router: &task_router_lock,
            mailbox_handle: &mailbox_handle_lock,
            user_memory: &user_memory_lock,
            tools_config: &tools_config_lock,
        },
    );
    // The Python tools of the installed packages join the registry, read from
    // their manifests; each module is imported on its first call only.
    if let Some(worker) = apollia_runtime::worker_runtime() {
        let data_dir = apollia_data_dir.clone();
        let registry = runtime_handle.tool_registry_handle.clone();
        let chat = runtime_handle.chat_manager.clone();
        worker.spawn(async move {
            apollia_aip::package_tools::attach(&data_dir, &registry, chat.as_ref()).await;
        });
    }
    // The agent-mode file tools read these roots. They come from the same
    // `[filesystem] trusted_paths` the chat classifies against, so one setting
    // governs both surfaces instead of the hardcoded home directory this used
    // to be.
    let _ = trusted_paths_lock.set(runtime_handle.filesystem_trusted_paths.clone());
    if let Some(gates) = runtime_handle.plan_gates.clone() {
        let _ = plan_gates_lock.set(gates);
    }
    if let Some(cache) = runtime_handle.plan_cache.clone() {
        let _ = plan_cache_lock.set(cache);
    }

    // Auto-load installed agents now that the OnceLocks are populated so the
    // ProductionBackendFactory can create real backends.
    if let Some(repo) = boot_agent_repo {
        auto_load_installed_agents(repo, &factory, &runtime_handle);
    }

    // Open a second AgentRepository instance for Tauri IPC commands.
    // SQLite WAL mode supports concurrent readers.
    let agent_repo: Arc<std::sync::Mutex<AgentRepository>> = {
        let db_path = apollia_data_dir.join(apollia_core::paths::DataFile::Agents.file_name());
        Arc::new(std::sync::Mutex::new(or_exit(
            AgentRepository::open(&db_path),
            "open agents.db for the desktop app",
        )))
    };

    // Open PackageRepository for Tauri IPC commands (same agents.db, WAL allows concurrent readers).
    let pkg_repo: Arc<std::sync::Mutex<apollia_tools::PackageRepository>> = {
        let db_path = apollia_data_dir.join(apollia_core::paths::DataFile::Agents.file_name());
        Arc::new(std::sync::Mutex::new(or_exit(
            apollia_tools::PackageRepository::open(&db_path),
            "open agents.db for the package repository",
        )))
    };
    let agent_loader: Arc<dyn AgentLoader> = Arc::new(backend::AIPAgentLoader);

    let mcp_registry_client = match McpRegistryClient::new(&apollia_data_dir) {
        Ok(client) => client,
        Err(e) => {
            tracing::warn!(
                error = %e,
                detail = "the registry commands are disabled",
                "mcp.registry.client.init.failed"
            );
            // Fall back to a client with a writable temp dir so Tauri state is always populated.
            or_exit(
                McpRegistryClient::new(std::path::Path::new("/tmp")),
                "open the fallback MCP registry client",
            )
        }
    };

    let app = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        // External URL opener: bridges the webview to the system browser for
        // OAuth flows and outbound help/docs links.
        // Tauri 2 disabled `window.open()` and `<a target="_blank">` by
        // default; this plugin (plus the `opener:default` capability permission)
        // is the canonical replacement.
        .plugin(tauri_plugin_opener::init())
        .manage(runtime_handle.clone())
        .manage(attached::RuntimeLink::Embedded(Box::new(runtime_handle.clone())))
        .manage(agent_repo)
        .manage(pkg_repo)
        .manage(agent_loader)
        .manage(runtime_handle.event_sender.clone())
        .manage(llm_router_lock.clone())
        .manage(mcp_registry_client)
        .manage(stt_flow_state.clone())
        .manage(std::sync::Arc::new(
            tokio::sync::Mutex::new(apollia_llm::DownloadManager::new()),
        ) as commands::model_hub::SharedDownloadManager)
        .manage(std::sync::Arc::new(apollia_llm::HfModelTypeCache::new()))
        .manage(commands::llm::LlmPingCache::default())
        // Route the macOS app-menu / Cmd+Q "Quit" (id `MENU_QUIT_APP`) to the
        // shared graceful quit. Without this, the default native quit is
        // swallowed by the window `CloseRequested` handler below.
        .on_menu_event(|app, event| {
            if event.id().as_ref() == tray::MENU_QUIT_APP {
                tray::initiate_quit(app);
            }
        })
        .setup(move |app| {
            // Store the interface locale the frontend transmits, so the
            // native surfaces (tray, app menu, STT notifications) render in
            // the language of the interface. Attached before the tray so the
            // tray's own refresh listener never runs ahead of the store.
            i18n::attach_listener(app);

            tray::setup_tray(app)?;

            // Replace the default macOS menu so Cmd+Q / the app-menu "Quit"
            // route through `initiate_quit` instead of the native `terminate:`
            // that `prevent_close` cancels.
            #[cfg(target_os = "macos")]
            tray::setup_app_menu(app)?;

            // bridge EventBus → Tauri events (replaces polling).
            events::spawn_event_bridge(app.handle().clone(), runtime_handle.event_sender.clone());

            // Periodic heartbeat so the frontend `runtimeHealth` watchdog
            // never trips during idle chat sessions.
            events::spawn_heartbeat(app.handle().clone());

            // Reconcile the installed-model index with the disk: a model copied
            // into a sub-folder, or deleted, while the application was closed.
            tauri::async_runtime::spawn_blocking(commands::model_hub::reconcile_model_index);

            // Session metrics aggregator.
            // .setup() runs outside any Tokio context; enter the Tauri async
            // runtime before calling spawn_detached so tokio::spawn is valid.
            let metrics_store = {
                // .setup() runs outside any Tokio context; enter the Tauri
                // async runtime so tokio::spawn inside spawn_detached is valid.
                let rt_handle = tauri::async_runtime::handle();
                let _guard = match &rt_handle {
                    tauri::async_runtime::RuntimeHandle::Tokio(h) => Some(h.enter()),
                };
                apollia_runtime::session_metrics::spawn_detached(
                    runtime_handle.event_sender.subscribe(),
                    runtime_handle.event_sender.clone(),
                    apollia_core::session_metrics::SessionThresholds::default(),
                    0,
                    0,
                )
            };
            app.manage(metrics_store);

            // Register the global STT hotkey with the full SttFlow pipeline
            // when STT is enabled. The flow reads the shared engine cell on each
            // trigger, so it is armed even if no model is loaded yet (a
            // mid-session download + reload brings the engine online without
            // re-registering).
            if let Some(ref stt_cfg) = stt_config_for_hotkey {
                if stt_cfg.enabled {
                    setup_stt_hotkey(app.handle(), stt_cfg, &runtime_handle, &stt_flow_state);
                }
            }

            // What the window close button does is a platform convention, so
            // `tray::close_button_action` decides rather than this handler.
            //
            // On macOS it hides the window: the runtime keeps running, the tray
            // icon stays visible, and the user re-opens via the tray "open"
            // item. On Windows and Linux the close button means quit, and
            // hiding there left the runtime and its child processes alive with
            // no window to point at them.
            //
            // `prevent_close` on both branches. On the quit branch it stops the
            // window teardown from racing `initiate_quit`, which needs to reach
            // `POST /api/v1/shutdown` before `AppHandle::exit` runs.
            let main_window = app
                .get_webview_window("main")
                .ok_or("main window not found in tauri.conf.json")?;

            let window_for_close = main_window.clone();
            main_window.on_window_event(move |event| {
                if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    match tray::close_button_action() {
                        tray::CloseAction::HideToTray => {
                            let _ = window_for_close.hide();
                        }
                        tray::CloseAction::Quit => {
                            tray::initiate_quit(window_for_close.app_handle());
                        }
                    }
                }
            });

            Ok(())
        })
        .invoke_handler(attached::gated(
            attached::LinkMode::Embedded,
            handlers::commands(),
        ))
        .build(tauri_context());

    or_exit(app, "build the Tauri application").run(|app, event| {
        // Kill every child process before the process exits. Otherwise they
        // are orphaned: `kill_on_drop` never runs because `app.exit`
        // terminates the process before the managed `RuntimeHandle` (and
        // its child handles) is dropped. ExitRequested covers every quit
        // path: window close, Cmd+Q, and the tray "Quitter".
        //
        // This used to stop the runner alone, which left `llama-server`
        // holding its VRAM and its loopback port after the app had quit.
        if matches!(event, tauri::RunEvent::ExitRequested { .. }) {
            if let Some(rt) = app.try_state::<RuntimeHandle>() {
                let rt = rt.inner().clone();
                tauri::async_runtime::block_on(async move {
                    rt.stop_child_processes().await;
                });
            }
        }
    });
}

/// The Tauri context, generated once for the application and for the refusal
/// window alike.
fn tauri_context() -> tauri::Context<tauri::Wry> {
    tauri::generate_context!()
}
