//! Recording overlay window management.
//!
//! Creates and controls a secondary Tauri window that displays a recording
//! indicator when STT audio capture is active. The window is always-on-top,
//! non-focusable, and borderless so it acts as a pure visual indicator without
//! interfering with the user's workflow.

use apollia_core::{EventBusSender, RuntimeEvent};
use tauri::{Emitter, Manager, WebviewUrl, WebviewWindowBuilder};

/// Errors that can occur during overlay window operations.
#[derive(Debug, thiserror::Error)]
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

/// Width of the overlay window in logical pixels.
const OVERLAY_WIDTH: f64 = 350.0;

/// Height of the overlay window in logical pixels.
const OVERLAY_HEIGHT: f64 = 80.0;

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
}

impl RecordingOverlay {
    /// Creates a new overlay manager and builds the hidden overlay window.
    ///
    /// The window is positioned at the bottom-center of the primary monitor
    /// and remains hidden until [`RecordingOverlay::show`] is called.
    pub fn create(app: &tauri::AppHandle, hotkey: String) -> Result<Self, OverlayError> {
        let overlay = Self {
            app: app.clone(),
            hotkey,
        };
        overlay.build_window()?;
        Ok(overlay)
    }

    /// Builds the overlay window (hidden by default).
    fn build_window(&self) -> Result<(), OverlayError> {
        let (x, y) = self.bottom_center_position()?;

        WebviewWindowBuilder::new(
            &self.app,
            WINDOW_LABEL,
            WebviewUrl::App("overlay.html".into()),
        )
        .title("Recording")
        .decorations(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .resizable(false)
        .focusable(false)
        .inner_size(OVERLAY_WIDTH, OVERLAY_HEIGHT)
        .position(x, y)
        .visible(false)
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

    /// Shows the overlay window and sends the hotkey configuration to its frontend.
    pub fn show(&self) -> Result<(), OverlayError> {
        if let Some(window) = self.app.get_webview_window(WINDOW_LABEL) {
            self.app
                .emit("stt-overlay-config", &self.hotkey)
                .map_err(|e| OverlayError::Visibility(e.to_string()))?;
            window
                .show()
                .map_err(|e| OverlayError::Visibility(e.to_string()))?;
        }
        Ok(())
    }

    /// Hides the overlay window.
    pub fn hide(&self) -> Result<(), OverlayError> {
        if let Some(window) = self.app.get_webview_window(WINDOW_LABEL) {
            window
                .hide()
                .map_err(|e| OverlayError::Visibility(e.to_string()))?;
        }
        Ok(())
    }
}

/// Spawns a background task that shows/hides the overlay in response to STT events.
///
/// Listens to the EventBus for [`RuntimeEvent::SttRecordingStarted`] and
/// [`RuntimeEvent::SttRecordingStopped`] and toggles the overlay window
/// visibility accordingly. Runs for the lifetime of the application.
pub fn spawn_overlay_listener(overlay: RecordingOverlay, event_bus: &EventBusSender) {
    let mut rx = event_bus.subscribe();
    tauri::async_runtime::spawn(async move {
        loop {
            match rx.recv().await {
                Ok(event) => match &event {
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
                },
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    tracing::debug!(skipped = n, "overlay listener lagged");
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
        tracing::debug!("overlay event listener stopped");
    });
}
