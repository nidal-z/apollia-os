//! The launch of the application attached to a daemon: the window, its
//! plugins, the relay of the daemon's events and the dictation of this
//! machine.

use apollia_core::EventBusSender;
use tauri::Manager;

use super::{gated, relay, AttachedClient, LinkMode, RuntimeLink};

/// Capacity of the local bus the relay publishes the daemon's events on, the
/// same order as the runtime's own bus.
const RELAY_BUS_CAPACITY: usize = 1024;

/// Arms the dictation of this window against the daemon: the microphone is
/// the one of this machine, the engine and the history the daemon's, the
/// thresholds its configuration, read once when the window attaches. Nothing
/// is armed while the daemon has dictation off, as in the embedded mode.
async fn arm_dictation(
    app: tauri::AppHandle,
    client: AttachedClient,
    bus: EventBusSender,
    state: crate::commands::stt::SttFlowState,
) {
    let config: apollia_core::SttConfigRow = match client.get("/api/v1/stt/config").await {
        Ok(config) => config,
        Err(error) => {
            tracing::event!(
                tracing::Level::WARN,
                error = %error,
                "desktop.attached.dictation_unarmed"
            );
            return;
        }
    };
    if !config.enabled {
        return;
    }
    let flow = crate::stt::flow::SttFlow::new(
        config,
        crate::stt::transcriber::Transcriber::Daemon(client),
        bus,
        app,
    );
    if let Ok(mut guard) = state.lock() {
        *guard = Some(std::sync::Arc::new(flow));
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
    let dictation: crate::commands::stt::SttFlowState =
        std::sync::Arc::new(std::sync::Mutex::new(None));
    let app = crate::package_open::attach(tauri::Builder::default())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_opener::init())
        .manage(RuntimeLink::Attached(client))
        .manage(crate::startup::ApiPort(port))
        .manage(dictation.clone())
        .manage(
            std::sync::Arc::new(tokio::sync::Mutex::new(apollia_llm::DownloadManager::new()))
                as crate::commands::model_hub::SharedDownloadManager,
        )
        .manage(std::sync::Arc::new(apollia_llm::HfModelTypeCache::new()))
        .on_menu_event(|app, event| {
            if event.id().as_ref() == crate::tray::MENU_QUIT_APP {
                crate::tray::initiate_quit(app);
            }
        })
        .setup(move |app| {
            crate::i18n::attach_listener(app);
            crate::tray::setup_tray(app)?;
            crate::package_open::launch(app.handle());
            #[cfg(target_os = "macos")]
            crate::tray::setup_app_menu(app)?;

            crate::events::spawn_event_bridge(app.handle().clone(), setup_bus.clone());
            relay::spawn_event_relay(setup_client.clone(), setup_bus.clone());
            relay::spawn_liveness(app.handle().clone(), setup_client.clone());
            tauri::async_runtime::spawn(arm_dictation(
                app.handle().clone(),
                setup_client.clone(),
                setup_bus.clone(),
                dictation.clone(),
            ));

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
            let _ = app.run_return(|app, event| crate::package_open::on_run_event(app, &event));
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
