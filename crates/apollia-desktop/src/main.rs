//! Apollia Desktop application (Tauri v2).
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
mod bootstrap;
mod bundled_packages;
mod commands;
mod embedded_docs;
mod events;
mod file_log;
mod handlers;
pub mod i18n;
pub mod mcp;
mod package_open;
mod process_job;
mod startup;
pub mod stt;
mod token_coalescer;
pub mod tray;

use std::sync::Arc;

use apollia_llm::LlmRouter;
use apollia_runtime::embedded::{EmbeddedConfig, RuntimeHandle};
use apollia_tools::AgentRepository;
use tauri::Manager;

use bootstrap::{
    backfill_mcp_origins, load_stt_config, load_toml_config, or_exit, setup_bundled_python,
    setup_stt_hotkey,
};

/// Shared mutable LLM router: updated by `reload_llm`, read by the commands
/// that list and probe the models.
pub type SharedLlmRouter = Arc<std::sync::RwLock<Option<Arc<LlmRouter>>>>;

fn main() {
    // Initialize tracing so Rust logs appear in the terminal during development
    // and in the log file the settings open.
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
    let apollia_data_dir = {
        let home = apollia_core::paths::home_dir_or_temp()
            .display()
            .to_string();
        apollia_core::paths::data_dir_under(home)
    };
    // A factory reset asked on Windows is carried out here, before the log
    // file or any database under the directory is opened by this process.
    let pending_reset = tauri::async_runtime::block_on(
        apollia_runtime::app_lifecycle::apply_pending_factory_reset(
            &apollia_data_dir,
            apollia_runtime::app_lifecycle::PENDING_RESET_WAIT,
        ),
    );
    // The same traces go to the terminal and to `~/.apollia/logs/desktop.log`,
    // rotated by size; a folder that cannot be opened leaves the terminal
    // alone, and says so once the subscriber is up.
    let logs_dir = file_log::logs_dir_under(&apollia_data_dir);
    let (file_layer, file_error) = match file_log::RotatingFile::open(&logs_dir) {
        Ok(file) => (
            Some(
                tracing_subscriber::fmt::layer()
                    .with_ansi(false)
                    .with_writer(std::sync::Mutex::new(file)),
            ),
            None,
        ),
        Err(error) => (None, Some(error)),
    };
    {
        use tracing_subscriber::layer::SubscriberExt as _;
        use tracing_subscriber::util::SubscriberInitExt as _;
        tracing_subscriber::registry()
            .with(env_filter)
            .with(tracing_subscriber::fmt::layer())
            .with(file_layer)
            .init();
    }
    apollia_runtime::app_lifecycle::log_pending_reset(&pending_reset, &apollia_data_dir);
    if let Some(error) = file_error {
        tracing::event!(
            tracing::Level::WARN,
            path = %logs_dir.display(),
            error = %error,
            detail = "the traces go to the terminal only",
            "desktop.log_file.unavailable"
        );
    }

    // The port of the native API: the one a serving daemon recorded, else
    // `[api].port`, else the default.
    let config_text = apollia_core::paths::config_file()
        .filter(|p| p.exists())
        .and_then(|p| std::fs::read_to_string(p).ok());
    let runtime_port = startup::resolve_port(&apollia_data_dir, config_text.as_deref());

    // Start, attach or refuse, before PyO3, any database or any listener of
    // the runtime. The claimed port is handed to the runtime below as is.
    // When the runtime is installed as a service of the session, it is
    // brought up first, so the decision finds it serving and attaches.
    let runtime_addr = std::net::SocketAddr::from(([127, 0, 0, 1], runtime_port));
    startup::service::bring_up(runtime_addr, &apollia_data_dir);
    let claimed_port = match startup::decide(
        runtime_addr,
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
                    attached::run(runtime_port, token, tauri_context())
                }
                None => startup::refuse(
                    &startup::RefusalReason::TokenRefused { port: runtime_port },
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

    // The LLM router the commands read, set once the runtime is up and
    // replaced by `reload_llm`.
    let llm_router_lock: Arc<std::sync::RwLock<Option<Arc<LlmRouter>>>> =
        Arc::new(std::sync::RwLock::new(None));

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

    // The guide and the onboarding an earlier version installed as code are
    // retired before the runtime registers the stored agents; both are
    // installed as packages once the runtime, and its audit journal, are up.
    if let Some(ref repo) = boot_agent_repo {
        bundled_packages::retire_code_agents(repo);
    }

    // Load (or generate) the API bearer token so the embedded runtime binds an
    // authenticated TCP listener for the desktop REST bridge. The Unix socket
    // stays local-trust; the TCP port is never served unauthenticated.
    let api_token = or_exit(
        apollia_runtime::api::load_or_generate_token(&apollia_data_dir),
        "load or generate the API token",
    );
    commands::set_api_token(api_token.clone());

    // The supervisor registers the stored agents at boot: an agent is a
    // definition, nothing is imported to make it available.
    let config = load_toml_config(EmbeddedConfig {
        agent_repository: boot_agent_repo,
        tcp_port: Some(runtime_port),
        tcp_listener: Some(claimed_port),
        api_token: Some(api_token),
        ..EmbeddedConfig::default()
    });

    let agent_idle_unload =
        std::time::Duration::from_secs(config.runtime_config.agent_idle_unload_secs);
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

    // The commands read the router of the runtime now that it is up.
    *llm_router_lock
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = runtime_handle.llm_router.clone();
    // The packages the binary ships are installed, recorded in the audit
    // journal, and their agents made available; then the Python tools of the
    // installed packages join the registry, read from their manifests, each
    // module imported on its first call only.
    if let Some(worker) = apollia_runtime::worker_runtime() {
        let data_dir = apollia_data_dir.clone();
        let registry = runtime_handle.tool_registry_handle.clone();
        let chat = runtime_handle.chat_manager.clone();
        let workflows = runtime_handle.workflows.clone();
        let governance = runtime_handle.governance.clone();
        let runtime = runtime_handle.clone();
        // Apollia searches the curated selection, in a conversation, for a
        // connection a request needs; an entry the policy forbids is left out.
        let selection = std::sync::Arc::new(mcp::chat_selection::connection_selection(
            &mcp::load_builtin_enrichments(),
            runtime_handle.connection_governance.clone(),
        ));
        worker.spawn(async move {
            if let Some(chat) = chat.as_ref() {
                chat.set_connection_selection(selection).await;
            }
            bundled_packages::install_bundled_packages(&data_dir, governance).await;
            runtime.register_stored_agents(&data_dir).await;
            apollia_aip::package_tools::attach(
                &data_dir,
                &registry,
                chat.as_ref(),
                agent_idle_unload,
                &[apollia_aip::package_tools::ProfileGrant::onboarding()],
            )
            .await;
            // The steps of a workflow call the models through the router of
            // the runtime, each run bounded by the step limit of `[workflows]`.
            let models = runtime.llm_router.clone().map(|router| {
                apollia_aip::package_tools::workflows::WorkflowModels::new(
                    router,
                    Some(runtime.event_sender.clone()),
                    apollia_core::WorkflowsConfig::default().max_steps,
                )
            });
            apollia_aip::package_tools::workflows::attach(
                &data_dir,
                workflows.as_ref(),
                models.clone(),
            )
            .await;
            // A package installed from now on brings its tools and its
            // workflows at once.
            apollia_runtime::packages::install_package_reload(
                apollia_aip::package_tools::reload_hook(apollia_aip::package_tools::PackageCode {
                    data_dir: data_dir.clone(),
                    registry: registry.clone(),
                    chat: chat.clone(),
                    grants: vec![apollia_aip::package_tools::ProfileGrant::onboarding()],
                    engine: workflows.clone(),
                    models,
                }),
            );
        });
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

    let app = package_open::attach(tauri::Builder::default())
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
        .manage(startup::ApiPort(runtime_handle.api_port))
        .manage(attached::RuntimeLink::Embedded(Box::new(runtime_handle.clone())))
        .manage(agent_repo)
        .manage(runtime_handle.event_sender.clone())
        .manage(llm_router_lock.clone())
        .manage(stt_flow_state.clone())
        .manage(std::sync::Arc::new(
            tokio::sync::Mutex::new(apollia_llm::DownloadManager::new()),
        ) as commands::model_hub::SharedDownloadManager)
        .manage(std::sync::Arc::new(apollia_llm::HfModelTypeCache::new()))
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

            // A package file that started the application opens its preview.
            package_open::launch(app.handle());

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
        package_open::on_run_event(app, &event);
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
