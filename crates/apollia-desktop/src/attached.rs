//! The desktop attached to a runtime that another process serves.
//!
//! The runtime runs either inside this process or as the daemon
//! `apollia-os start`. [`RuntimeLink`] is the seam between the two: an
//! embedded arm holding the [`RuntimeHandle`], and an HTTP arm holding an
//! [`AttachedClient`] that reaches the daemon over its local API with the
//! bearer token of this installation. A command routed through the seam
//! matches on it and serves both modes.
//!
//! A command not routed yet is refused in attached mode before its body runs,
//! by [`gate`], with an error that names it and says why, instead of reaching
//! for state this process never built. [`ROUTED_COMMANDS`] is the list the
//! gate reads, and `scripts/check_attached_coverage.py` counts it against every
//! registered command.

mod client;
mod proxy;
mod relay;

use apollia_core::EventBusSender;
use apollia_runtime::embedded::RuntimeHandle;
use tauri::Manager;

pub(crate) use client::AttachedClient;

/// Capacity of the local bus the relay publishes the daemon's events on, the
/// same order as the runtime's own bus.
const RELAY_BUS_CAPACITY: usize = 1024;

/// The commands that serve the attached mode. Every other registered command
/// answers [`AttachedError::NotRouted`] while the runtime is a daemon.
///
/// `start_variant_download` needs no route over HTTP: it never reads the
/// runtime, only the Hub, the models folder and its index, which the daemon
/// shares since it listens on this machine. [`run`] manages the download
/// manager it holds. `list_model_cards` needs none either: it reads the model
/// registry, the data directory and its `apollia.toml`, never the runtime.
pub(crate) const ROUTED_COMMANDS: &[&str] = &[
    "create_chat_session",
    "get_chat_session",
    "list_chat_sessions",
    "pause_chat_session",
    "send_chat_message",
    // The conversations as the list manages them: configure, rename, name,
    // erase, link to a project, list by project.
    "delete_chat_session",
    "generate_chat_session_name",
    "link_chat_to_project",
    "list_chats_by_project",
    "rename_chat_session",
    "update_chat_session",
    // Agents: the registry and the install.
    "agent_status_snapshot",
    "install_agent",
    "list_agents",
    // Tasks: the list, the launch, the detail, the cancellation.
    "cancel_task",
    "get_task_timeline",
    "list_tasks",
    "submit_task",
    // Approvals: the pending and resolved ones, and the decision that resumes
    // the task.
    "list_pending_approvals",
    "list_resolved_approvals",
    "resume_task",
    // The questions an agent asked the user.
    "list_pending_user_inputs",
    "respond_user_input",
    "respond_user_input_rejected",
    // Commands whose whole body is a call to a runtime route, served through
    // the methods of `proxy.rs` in both modes.
    // Models: the inference servers, the loaded models, the costs.
    "create_llm_backend",
    "delete_llm_backend",
    "get_llm_cost_stats",
    "list_loaded_models",
    "set_default_llm_backend",
    "unload_loaded_model",
    "update_llm_backend",
    // MCP servers and the curated catalogue.
    "add_mcp_server",
    "fetch_mcp_curated",
    "get_mcp_server_detail",
    "get_mcp_server_raw_config",
    "list_mcp_resources",
    "list_mcp_servers",
    "remove_mcp_server",
    "restart_mcp_server",
    "set_mcp_server_approval",
    "test_mcp_connection",
    "test_mcp_live_server",
    "update_mcp_server_config",
    // Connections.
    "list_connections",
    // Notifications and the activity feed.
    "create_notification_channel",
    "delete_notification_channel",
    "get_notification_events",
    "get_notification_logs",
    "list_notification_channels",
    "list_runtime_activity",
    "set_notification_events",
    "test_notification_channel",
    "update_notification_channel",
    // Triggers.
    "create_trigger",
    "delete_trigger",
    "fire_trigger",
    "get_trigger_definition",
    "get_trigger_logs",
    "list_triggers",
    "set_trigger_enabled",
    "update_trigger",
    // Tools, observability, traces and messages between agents.
    "clear_plan_cache",
    "describe_tool",
    "get_active_hooks",
    "get_audit_stats",
    "get_llm_daily_costs",
    "get_plan_cache_stats",
    "get_task_trace",
    "list_agent_messages",
    "list_tools",
    "verify_audit_run",
    // The settings of the chat, which are those of the Apollia agent.
    "get_chat_libre_config",
    "update_chat_libre_config",
    // The user profile, over `/api/v1/profile`.
    "delete_profile_entry",
    "get_profile",
    "reset_user_profile",
    "set_profile_entry",
    // Models: the download of a whole variant from the Hub.
    "start_variant_download",
    // Models: the signed cards of the catalogue, read when it opens.
    "list_model_cards",
];

/// The commands that belong to the window, not to the runtime: they read no
/// state of the runtime and serve the attached mode as they are, each with the
/// reason it needs no route. `scripts/check_attached_coverage.py` reads the
/// reasons and refuses an entry without one.
pub(crate) const WINDOW_COMMANDS: &[(&str, &str)] = &[
    (
        "app_restart",
        "relaunches this window, which attaches again at startup",
    ),
    (
        "quit_app",
        "closes this window and leaves the daemon serving",
    ),
    (
        "check_for_update",
        "asks the update endpoint about this application",
    ),
    (
        "install_update",
        "replaces this application, not the daemon",
    ),
    (
        "get_cli_status",
        "reads the command-line link this application installs",
    ),
    (
        "install_cli",
        "links the command line bundled with this application",
    ),
    (
        "uninstall_cli",
        "removes the command-line link this application installed",
    ),
    (
        "link_preview",
        "fetches the card of a link the window shows",
    ),
    (
        "list_audio_input_devices",
        "lists the microphones of this machine",
    ),
    (
        "open_config_in_editor",
        "opens the configuration file in the editor of this machine",
    ),
    (
        "automation_script",
        "drives this window in a development build",
    ),
    (
        "automation_capture",
        "captures this window in a development build",
    ),
    (
        "automation_resize",
        "resizes this window in a development build",
    ),
    (
        "automation_finish",
        "ends the run driving this window in a development build",
    ),
];

/// Encodes one path segment of a daemon route.
pub(crate) fn segment(value: &str) -> std::borrow::Cow<'_, str> {
    urlencoding::encode(value)
}

/// Where the runtime this application drives lives.
#[derive(Clone)]
pub(crate) enum RuntimeLink {
    /// In this process.
    Embedded(Box<RuntimeHandle>),
    /// In another process, reached over HTTP.
    Attached(AttachedClient),
}

/// Which arm of the seam the application runs, for the gate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LinkMode {
    /// The runtime runs in this process: every command is served.
    Embedded,
    /// The runtime is a daemon: only routed commands are served.
    Attached,
}

/// Why an attached command did not complete.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub(crate) enum AttachedError {
    /// The command reads state that only exists when the runtime runs in this
    /// process, and no HTTP route serves it yet.
    #[error("{command} is unavailable while Apollia runs as a service")]
    NotRouted {
        /// The command refused.
        command: String,
    },
    /// The HTTP stack could not be initialised.
    #[error("the HTTP client cannot be initialised")]
    Client(#[source] reqwest::Error),
    /// The runtime did not answer: stopped, restarting, or past the deadline.
    #[error("the Apollia service does not answer")]
    Unreachable(#[source] reqwest::Error),
    /// The runtime answered with an error status.
    #[error("the Apollia service refused the request ({status}): {message}")]
    Refused {
        /// The HTTP status.
        status: u16,
        /// The runtime's own message, empty when its body held none.
        message: String,
    },
    /// The runtime answered a body this application cannot read.
    #[error("the Apollia service answered an unreadable body")]
    Body(#[source] apollia_core::net::ReadCappedError),
}

impl AttachedError {
    /// The text the interface shows, in its language.
    pub(crate) fn for_display(&self, locale: crate::i18n::UiLocale) -> String {
        match self {
            Self::NotRouted { command } => crate::i18n::attached_unavailable(locale, command),
            Self::Refused { message, .. } if !message.is_empty() => message.clone(),
            other => other.to_string(),
        }
    }
}

impl From<AttachedError> for String {
    fn from(error: AttachedError) -> Self {
        error.for_display(crate::i18n::locale())
    }
}

/// Whether `command` may run in `mode`.
///
/// # Errors
///
/// [`AttachedError::NotRouted`] for a command outside [`ROUTED_COMMANDS`] and
/// [`WINDOW_COMMANDS`] while the runtime is a daemon.
pub(crate) fn gate(mode: LinkMode, command: &str) -> Result<(), AttachedError> {
    if mode == LinkMode::Embedded
        || ROUTED_COMMANDS.contains(&command)
        || WINDOW_COMMANDS.iter().any(|(name, _)| *name == command)
    {
        return Ok(());
    }
    Err(AttachedError::NotRouted {
        command: command.to_owned(),
    })
}

/// Wraps the generated command handler with [`gate`]: a refused command is
/// answered with its typed error and its body never runs.
pub(crate) fn gated<R, H>(
    mode: LinkMode,
    handler: H,
) -> impl Fn(tauri::ipc::Invoke<R>) -> bool + Send + Sync + 'static
where
    R: tauri::Runtime,
    H: Fn(tauri::ipc::Invoke<R>) -> bool + Send + Sync + 'static,
{
    move |invoke| match gate(mode, invoke.message.command()) {
        Ok(()) => handler(invoke),
        Err(error) => {
            tracing::event!(
                tracing::Level::DEBUG,
                command = %invoke.message.command(),
                "desktop.attached.command_not_routed"
            );
            invoke.resolver.reject(String::from(error));
            true
        }
    }
}

/// Runs the application attached to the daemon serving `port`, and never
/// returns.
///
/// Nothing of the embedded runtime is built: no PyO3, no database, no
/// listener. The interface talks to the daemon through the routed commands,
/// receives its events through the relay, and learns it is gone through the
/// liveness probe. Quitting leaves the daemon running.
pub(crate) fn run(port: u16, token: String, context: tauri::Context<tauri::Wry>) -> ! {
    let client = match AttachedClient::new(format!("http://127.0.0.1:{port}"), token) {
        Ok(client) => client,
        Err(error) => {
            tracing::event!(
                tracing::Level::ERROR,
                error = %error,
                "desktop.attached.client_failed"
            );
            std::process::exit(1)
        }
    };
    let (bus, _) = tokio::sync::broadcast::channel(RELAY_BUS_CAPACITY);
    let bus: EventBusSender = bus;

    let setup_client = client.clone();
    let setup_bus = bus.clone();
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_opener::init())
        .manage(RuntimeLink::Attached(client))
        .manage(
            std::sync::Arc::new(tokio::sync::Mutex::new(apollia_llm::DownloadManager::new()))
                as crate::commands::model_hub::SharedDownloadManager,
        )
        .on_menu_event(|app, event| {
            if event.id().as_ref() == crate::tray::MENU_QUIT_APP {
                crate::tray::initiate_quit(app);
            }
        })
        .setup(move |app| {
            crate::i18n::attach_listener(app);
            crate::tray::setup_tray(app)?;
            #[cfg(target_os = "macos")]
            crate::tray::setup_app_menu(app)?;

            crate::events::spawn_event_bridge(app.handle().clone(), setup_bus.clone());
            relay::spawn_event_relay(setup_client.clone(), setup_bus.clone());
            relay::spawn_liveness(app.handle().clone(), setup_client.clone());

            let main_window = app
                .get_webview_window("main")
                .ok_or("main window not found in tauri.conf.json")?;
            let window_for_close = main_window.clone();
            main_window.on_window_event(move |event| {
                if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    match crate::tray::close_button_action() {
                        crate::tray::CloseAction::HideToTray => {
                            let _ = window_for_close.hide();
                        }
                        crate::tray::CloseAction::Quit => {
                            crate::tray::initiate_quit(window_for_close.app_handle());
                        }
                    }
                }
            });
            tracing::event!(tracing::Level::INFO, port, "desktop.attached.started");
            Ok(())
        })
        .invoke_handler(gated(LinkMode::Attached, crate::handlers::commands()))
        .build(context);

    match app {
        Ok(app) => {
            let _ = app.run_return(|_, _| {});
            std::process::exit(0)
        }
        Err(error) => {
            tracing::event!(
                tracing::Level::ERROR,
                error = %error,
                "desktop.attached.build_failed"
            );
            std::process::exit(1)
        }
    }
}

#[cfg(test)]
mod tests;
