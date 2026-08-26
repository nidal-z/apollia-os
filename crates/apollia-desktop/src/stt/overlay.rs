//! Recording overlay window management.
//!
//! Creates and controls a secondary Tauri window that displays a recording
//! indicator when STT audio capture is active. The window is always-on-top,
//! non-focusable, and borderless so it acts as a pure visual indicator without
//! interfering with the user's workflow.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use apollia_core::{subscribe_resilient, EventBusSender, RuntimeEvent};
use tauri::{Emitter, Manager, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

/// Errors that can occur during overlay window operations.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum OverlayError {
    /// Failed to create the overlay window.
    #[error("failed to create overlay window: {0}")]
    WindowCreation(String),
    /// Failed to show or hide the overlay window.
    #[error("failed to toggle overlay visibility: {0}")]
    Visibility(String),
    /// Failed to determine screen geometry for positioning.
    #[error("failed to get monitor info: {0}")]
    Monitor(String),
}

/// Window label used for the recording overlay.
const WINDOW_LABEL: &str = "recording-overlay";

/// Global shortcut key used to cancel (discard) an in-progress recording.
const CANCEL_SHORTCUT: &str = "Escape";

/// Width of the overlay window in logical pixels.
const OVERLAY_WIDTH: f64 = 350.0;

/// Height of the overlay window in logical pixels.
/// Includes 4px transparent inset on each side (border isolation).
const OVERLAY_HEIGHT: f64 = 98.0;

/// Margin from the bottom of the screen in logical pixels.
const BOTTOM_MARGIN: f64 = 40.0;

/// Manages the recording overlay window lifecycle.
///
/// Created once during Tauri setup. The overlay window is built hidden and
/// shown/hidden in response to STT recording events from the EventBus.
pub struct RecordingOverlay {
    /// Tauri application handle for window management and event emission.
    app: tauri::AppHandle,
    /// Hotkey string displayed to the user (e.g. `"Ctrl+Shift+Space"`).
    hotkey: String,
    /// Called when the user presses Escape to cancel (discard) the recording.
    on_cancel: Arc<dyn Fn() + Send + Sync>,
}

impl RecordingOverlay {
    /// Creates a new overlay manager and builds the hidden overlay window.
    ///
    /// `on_cancel` is called when the user presses Escape while recording.
    /// The window is positioned at the bottom-center of the primary monitor
    /// and remains hidden until [`RecordingOverlay::show`] is called.
    pub fn create(
        app: &tauri::AppHandle,
        hotkey: String,
        on_cancel: Arc<dyn Fn() + Send + Sync>,
    ) -> Result<Self, OverlayError> {
        let overlay = Self {
            app: app.clone(),
            hotkey,
            on_cancel,
        };
        overlay.build_window()?;
        Ok(overlay)
    }

    /// Builds the overlay window (hidden by default).
    ///
    /// A window with the same label already existing is success, not failure:
    /// the STT flow is rebuilt whenever the dictation settings change, and
    /// Tauri refuses a duplicate label. Reusing the live window keeps that
    /// re-arm silent instead of losing the indicator on the second save.
    fn build_window(&self) -> Result<(), OverlayError> {
        if self.app.get_webview_window(WINDOW_LABEL).is_some() {
            tracing::debug!("recording overlay already exists - reusing it");
            return Ok(());
        }

        let (x, y) = self.bottom_center_position()?;

        let builder = WebviewWindowBuilder::new(
            &self.app,
            WINDOW_LABEL,
            WebviewUrl::App("overlay.html".into()),
        )
        .title(crate::i18n::overlay_recording(crate::i18n::locale()))
        .decorations(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .resizable(false)
        .shadow(false)
        .focusable(false)
        .inner_size(OVERLAY_WIDTH, OVERLAY_HEIGHT)
        .position(x, y)
        .visible(false);

        // Transparency relies on Tauri's `macos-private-api` feature and
        // `macOSPrivateApi: true` (tauri.conf.json), both enabled unconditionally,
        // so the overlay is transparent on every platform.
        let builder = builder.transparent(true);

        builder
            .build()
            .map_err(|e: tauri::Error| OverlayError::WindowCreation(e.to_string()))?;

        Ok(())
    }

    /// Computes the bottom-center position on the primary monitor.
    fn bottom_center_position(&self) -> Result<(f64, f64), OverlayError> {
        let main_window = self
            .app
            .get_webview_window("main")
            .ok_or_else(|| OverlayError::Monitor("main window not found".into()))?;

        let monitor = main_window
            .primary_monitor()
            .map_err(|e| OverlayError::Monitor(e.to_string()))?
            .ok_or_else(|| OverlayError::Monitor("no primary monitor found".into()))?;

        let pos = monitor.position();
        let size = monitor.size();
        let scale = monitor.scale_factor();

        let logical_w = size.width as f64 / scale;
        let logical_h = size.height as f64 / scale;
        let origin_x = pos.x as f64 / scale;
        let origin_y = pos.y as f64 / scale;

        let x = origin_x + (logical_w - OVERLAY_WIDTH) / 2.0;
        let y = origin_y + logical_h - OVERLAY_HEIGHT - BOTTOM_MARGIN;

        Ok((x, y))
    }

    /// Shows the overlay window, sends the hotkey config, and registers Escape to cancel.
    pub fn show(&self) -> Result<(), OverlayError> {
        if let Some(window) = self.app.get_webview_window(WINDOW_LABEL) {
            window
                .show()
                .map_err(|e| OverlayError::Visibility(e.to_string()))?;
            // Emit after show so the webview is active and the listener is ready.
            window
                .emit("stt-overlay-config", &self.hotkey)
                .map_err(|e| OverlayError::Visibility(e.to_string()))?;
        }

        // Register Escape as a temporary global shortcut for cancelling the recording.
        let cancel = Arc::clone(&self.on_cancel);
        if let Err(e) = self.app.global_shortcut().on_shortcut(
            CANCEL_SHORTCUT,
            move |_app, _shortcut, event| {
                if event.state == ShortcutState::Pressed {
                    cancel();
                }
            },
        ) {
            tracing::warn!(error = %e, "failed to register Escape cancel shortcut");
        }

        Ok(())
    }

    /// Hides the overlay window and unregisters the Escape cancel shortcut.
    pub fn hide(&self) -> Result<(), OverlayError> {
        // Unregister Escape before hiding so it stops intercepting keypresses immediately.
        if let Err(e) = self.app.global_shortcut().unregister(CANCEL_SHORTCUT) {
            tracing::warn!(error = %e, "failed to unregister Escape cancel shortcut");
        }

        if let Some(window) = self.app.get_webview_window(WINDOW_LABEL) {
            window
                .hide()
                .map_err(|e| OverlayError::Visibility(e.to_string()))?;
        }
        Ok(())
    }
}

/// Generation of the live overlay listener.
///
/// The STT flow, and with it the overlay, is rebuilt whenever the dictation
/// settings change. Each rebuild spawns a listener, and without this counter the
/// previous ones would keep running: every save would add a task holding a stale
/// hotkey label and a cancel closure bound to a flow nobody uses any more, so
/// one Escape press would fire every past closure.
static OVERLAY_GENERATION: AtomicUsize = AtomicUsize::new(0);

/// Spawns a background task that shows/hides the overlay in response to STT events.
///
/// Listens to the EventBus for [`RuntimeEvent::SttRecordingStarted`] and
/// [`RuntimeEvent::SttRecordingStopped`] and toggles the overlay window
/// visibility accordingly. Runs until a later call supersedes it.
pub fn spawn_overlay_listener(overlay: RecordingOverlay, event_bus: &EventBusSender) {
    let generation = OVERLAY_GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    let mut rx = subscribe_resilient(event_bus, "stt.overlay");
    tauri::async_runtime::spawn(async move {
        while let Some(event) = rx.recv().await {
            if OVERLAY_GENERATION.load(Ordering::SeqCst) != generation {
                break;
            }
            match &event {
                RuntimeEvent::SttRecordingStarted => {
                    if let Err(e) = overlay.show() {
                        tracing::warn!(error = %e, "failed to show recording overlay");
                    }
                }
                RuntimeEvent::SttRecordingStopped { .. } => {
                    if let Err(e) = overlay.hide() {
                        tracing::warn!(error = %e, "failed to hide recording overlay");
                    }
                }
                _ => {}
            }
        }
        tracing::debug!(generation, "overlay event listener stopped");
    });
}
