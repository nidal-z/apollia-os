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
pub(crate) const ROUTED_COMMANDS: &[&str] = &[
    "create_chat_session",
    "get_chat_session",
    "list_chat_sessions",
    "pause_chat_session",
    "send_chat_message",
];

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
/// [`AttachedError::NotRouted`] for a command outside [`ROUTED_COMMANDS`] while
/// the runtime is a daemon.
pub(crate) fn gate(mode: LinkMode, command: &str) -> Result<(), AttachedError> {
    if mode == LinkMode::Embedded || ROUTED_COMMANDS.contains(&command) {
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
        .plugin(tauri_plugin_opener::init())
        .manage(RuntimeLink::Attached(client))
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
