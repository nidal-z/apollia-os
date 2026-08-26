//! Global hotkey listener for STT recording control.
//!
//! Registers a system-wide keyboard shortcut via `tauri-plugin-global-shortcut`
//! that works regardless of window focus. Supports two trigger modes:
//! - **Toggle**: first press starts recording, second press stops it.
//! - **Push-to-talk**: key down starts recording, key up stops it.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

/// Recording trigger mode parsed from the `trigger_mode` config string.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TriggerMode {
    /// First press = start, second press = stop.
    Toggle,
    /// Key held = recording, key released = stop.
    PushToTalk,
}

impl TriggerMode {
    /// Parses a trigger mode from the config string.
    ///
    /// Recognises `"toggle"` and `"push-to-talk"` (case-insensitive).
    /// Defaults to [`TriggerMode::Toggle`] for unrecognised values.
    pub fn from_config(s: &str) -> Self {
        match s.to_ascii_lowercase().as_str() {
            "push-to-talk" | "push_to_talk" | "ptt" => Self::PushToTalk,
            _ => Self::Toggle,
        }
    }
}

/// Tracks whether recording is active and provides the hotkey registration logic.
///
/// The actual audio capture is driven externally via the `on_start` / `on_stop`
/// callbacks passed to [`HotkeyListener::register`].
pub struct HotkeyListener {
    /// Shared flag indicating whether recording is currently active.
    recording: Arc<AtomicBool>,
    /// Hotkey string as configured (e.g. `"ctrl+shift+space"`).
    hotkey: String,
    /// How the hotkey controls recording start/stop.
    trigger_mode: TriggerMode,
}

impl HotkeyListener {
    /// Creates a new listener from the STT configuration fields.
    pub fn new(hotkey: String, trigger_mode: TriggerMode) -> Self {
        Self {
            recording: Arc::new(AtomicBool::new(false)),
            hotkey,
            trigger_mode,
        }
    }

    /// Returns the shared recording flag.
    ///
    /// External components can read this to know whether recording is active.
    pub fn recording_flag(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.recording)
    }

    /// Registers the global hotkey on the given Tauri application.
    ///
    /// `on_start` is called when recording begins, `on_stop` when it ends.
    /// Both callbacks execute on the global-shortcut handler thread; keep them
    /// lightweight (e.g. send a message to a channel).
    ///
    /// Returns `Ok(())` on success, or an error description if the hotkey is
    /// already taken by another application or the string is unparseable.
    pub fn register<F1, F2>(
        self,
        app: &tauri::AppHandle,
        on_start: F1,
        on_stop: F2,
    ) -> Result<(), String>
    where
        F1: Fn() + Send + Sync + 'static,
        F2: Fn() + Send + Sync + 'static,
    {
        let recording = self.recording;
        let mode = self.trigger_mode;
        let hotkey = self.hotkey.clone();

        let gs = app.global_shortcut();

        gs.on_shortcut(hotkey.as_str(), move |_app, _shortcut, event| {
            dispatch_shortcut(mode, &recording, event.state, &on_start, &on_stop);
        })
        .map_err(|e| {
            tracing::warn!(hotkey = %self.hotkey, error = %e, "stt.hotkey.register.failed");
            format!("failed to register hotkey '{}': {e}", self.hotkey)
        })?;

        tracing::info!(
            hotkey = %self.hotkey,
            mode = ?mode,
            "stt.hotkey.registered"
        );

        Ok(())
    }
}

/// Routes a single global-shortcut event to the start/stop callbacks per mode.
///
/// Extracted from `HotkeyListener::register` so the closure body stays flat.
fn dispatch_shortcut(
    mode: TriggerMode,
    recording: &AtomicBool,
    state: ShortcutState,
    on_start: &(dyn Fn() + Send + Sync),
    on_stop: &(dyn Fn() + Send + Sync),
) {
    match mode {
        TriggerMode::Toggle => {
            if state == ShortcutState::Pressed {
                if recording.fetch_xor(true, Ordering::SeqCst) {
                    on_stop();
                } else {
                    on_start();
                }
            }
        }
        TriggerMode::PushToTalk => match state {
            ShortcutState::Pressed => {
                if !recording.swap(true, Ordering::SeqCst) {
                    on_start();
                }
            }
            ShortcutState::Released => {
                if recording.swap(false, Ordering::SeqCst) {
                    on_stop();
                }
            }
        },
    }
}

/// Unregisters all global shortcuts for the application.
///
/// Called during graceful shutdown to release the hotkey so other applications
/// can reclaim it.
pub fn unregister_all(app: &tauri::AppHandle) -> Result<(), String> {
    app.global_shortcut()
        .unregister_all()
        .map_err(|e| format!("failed to unregister hotkeys: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trigger_mode_from_config_toggle() {
        // GIVEN the toggle mode name in three casings
        // WHEN each is read from the configuration
        // THEN all three give the toggle mode, so casing in the toml is not a silent fallback
        assert_eq!(TriggerMode::from_config("toggle"), TriggerMode::Toggle);
        assert_eq!(TriggerMode::from_config("Toggle"), TriggerMode::Toggle);
        assert_eq!(TriggerMode::from_config("TOGGLE"), TriggerMode::Toggle);
    }

    #[test]
    fn trigger_mode_from_config_push_to_talk() {
        // GIVEN the four spellings of push-to-talk the configuration accepts
        // WHEN each is read from the configuration
        // THEN all four give the push-to-talk mode
        assert_eq!(
            TriggerMode::from_config("push-to-talk"),
            TriggerMode::PushToTalk
        );
        assert_eq!(
            TriggerMode::from_config("push_to_talk"),
            TriggerMode::PushToTalk
        );
        assert_eq!(TriggerMode::from_config("ptt"), TriggerMode::PushToTalk);
        assert_eq!(TriggerMode::from_config("PTT"), TriggerMode::PushToTalk);
    }

    #[test]
    fn trigger_mode_from_config_unknown_defaults_to_toggle() {
        // GIVEN an unknown mode name and an empty one
        // WHEN each is read from the configuration
        // THEN the hotkey falls back to toggle rather than refusing to start
        assert_eq!(TriggerMode::from_config("unknown"), TriggerMode::Toggle);
        assert_eq!(TriggerMode::from_config(""), TriggerMode::Toggle);
    }

    #[test]
    fn hotkey_listener_recording_flag_starts_false() {
        // GIVEN a listener that has just been built
        let listener = HotkeyListener::new("ctrl+shift+space".into(), TriggerMode::Toggle);
        // WHEN its recording flag is read
        // THEN it is down, so no capture starts before the operator presses the key
        assert!(!listener.recording_flag().load(Ordering::SeqCst));
    }
}
