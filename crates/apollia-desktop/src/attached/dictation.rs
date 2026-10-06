//! The dictation of a window attached to a daemon.

use apollia_core::EventBusSender;

use super::AttachedClient;

/// Arms the dictation of this window against the daemon: the microphone is
/// the one of this machine, the engine and the history the daemon's, the
/// thresholds its configuration, read once when the window attaches. Nothing
/// is armed while the daemon has dictation off, as in the embedded mode.
pub(super) async fn arm(
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
