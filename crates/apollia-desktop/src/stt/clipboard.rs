//! Clipboard injection for STT transcription results.
//!
//! Exposes two public primitives used by the async STT flow:
//! - [`write_only`]          – write text to clipboard, no paste
//! - [`prepare_paste`]       – write text + optionally save previous
//! - [`simulate_paste`]      – send Cmd/Ctrl+V via the Accessibility API
//! - [`restore_clipboard`]   – write back previously saved content
//!
//! The delays between steps (clipboard propagation, post-paste settle)
//! are the caller's responsibility so they can be `tokio::time::sleep`
//! instead of blocking `thread::sleep`.

use arboard::Clipboard;
use enigo::{Direction, Enigo, Key, Keyboard, Settings};

/// Errors that can occur during clipboard injection.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ClipboardError {
    /// Failed to initialise the system clipboard handle.
    #[error("clipboard init failed: {0}")]
    ClipboardInit(String),

    /// Failed to read text from the clipboard.
    #[error("clipboard read failed: {0}")]
    Read(String),

    /// Failed to write text to the clipboard.
    #[error("clipboard write failed: {0}")]
    Write(String),

    /// Failed to initialise the keyboard simulator.
    #[error("keyboard simulator init failed: {0}")]
    KeyboardInit(String),

    /// The paste keyboard shortcut could not be sent.
    #[error("paste simulation failed: {0}")]
    PasteSimulation(String),
}

/// Writes text to the system clipboard without simulating a paste shortcut.
///
/// Use when `clipboard_mode = "clipboard"`: the user pastes manually.
pub fn write_only(text: &str) -> Result<(), ClipboardError> {
    let mut clipboard =
        Clipboard::new().map_err(|e| ClipboardError::ClipboardInit(e.to_string()))?;
    clipboard
        .set_text(text)
        .map_err(|e| ClipboardError::Write(e.to_string()))?;
    tracing::debug!(
        len = text.len(),
        detail = "no paste keystroke is sent",
        "stt.clipboard.written"
    );
    Ok(())
}

/// Writes text to the clipboard and optionally saves the previous content.
///
/// Returns the previous clipboard text if `restore` is `true` and
/// a previous text was present. The caller should pass the returned value
/// to [`restore_clipboard`] after an async delay.
///
/// This function is **fast** (no sleeps). The caller is responsible for
/// inserting the appropriate async delays between steps.
pub fn prepare_paste(text: &str, restore: bool) -> Result<Option<String>, ClipboardError> {
    let mut clipboard =
        Clipboard::new().map_err(|e| ClipboardError::ClipboardInit(e.to_string()))?;

    let previous = if restore {
        clipboard.get_text().ok()
    } else {
        None
    };

    clipboard
        .set_text(text)
        .map_err(|e| ClipboardError::Write(e.to_string()))?;

    tracing::debug!(len = text.len(), restore, "stt.clipboard.prepared");
    Ok(previous)
}

/// Pastes text at the current cursor position using clipboard + osascript.
///
/// On macOS, simulating Cmd+V via `enigo` within the Tauri process causes two
/// distinct failures:
///  1. Using `Key::Meta` + `Key::Unicode('v')`: crashes because the CGEvent is
///     processed by the WebKit paste handler on the main thread while the
///     Tokio runtime is active.
///  2. Using `enigo::text()`: the HID-level CGEvents pass through Tauri's
///     global-shortcut event tap, which re-dispatches them as a new hotkey
///     invocation each time a chunk containing a matching key is seen,
///     resulting in the transcription text being typed repeatedly as
///     progressively shorter suffixes.
///
/// The solution: write the text to the clipboard with `arboard`, then send
/// Cmd+V from a **separate `osascript` subprocess**. The event originates
/// outside the Tauri process so WebKit handles it like a normal user paste
/// and the global-shortcut tap never sees it.
///
/// On platforms other than macOS, falls back to `enigo` Ctrl+V.
///
/// This function is **fast** (no sleeps). `arboard` clipboard access must have
/// already been done by the caller via [`prepare_paste`] before calling this.
pub fn paste_via_subprocess() -> Result<(), ClipboardError> {
    #[cfg(target_os = "macos")]
    {
        let status = std::process::Command::new("osascript")
            .args([
                "-e",
                "tell application \"System Events\" to keystroke \"v\" using {command down}",
            ])
            .status()
            .map_err(|e| ClipboardError::PasteSimulation(format!("osascript spawn failed: {e}")))?;
        if !status.success() {
            return Err(ClipboardError::PasteSimulation(format!(
                "osascript exited with {status}"
            )));
        }
        tracing::debug!(
            detail = "sent through an osascript subprocess",
            "stt.paste.triggered"
        );
    }
    #[cfg(not(target_os = "macos"))]
    {
        let mut enigo = Enigo::new(&Settings::default())
            .map_err(|e| ClipboardError::KeyboardInit(e.to_string()))?;
        enigo
            .key(Key::Control, Direction::Press)
            .map_err(|e| ClipboardError::PasteSimulation(e.to_string()))?;
        enigo
            .key(Key::Unicode('v'), Direction::Click)
            .map_err(|e| ClipboardError::PasteSimulation(e.to_string()))?;
        enigo
            .key(Key::Control, Direction::Release)
            .map_err(|e| ClipboardError::PasteSimulation(e.to_string()))?;
        tracing::debug!(detail = "sent through enigo", "stt.paste.triggered");
    }
    Ok(())
}

/// Simulates the platform paste shortcut (`Cmd+V` on macOS, `Ctrl+V` elsewhere).
///
/// **Prefer [`type_text`] over this function** when possible. On macOS,
/// posting a Meta+V CGEvent while a Tauri/WebKit window is in focus crashes
/// the process. This function is kept for potential future use on other
/// platforms.
pub fn simulate_paste() -> Result<(), ClipboardError> {
    let mut enigo = Enigo::new(&Settings::default())
        .map_err(|e| ClipboardError::KeyboardInit(e.to_string()))?;

    let modifier = paste_modifier();

    enigo
        .key(modifier, Direction::Press)
        .map_err(|e| ClipboardError::PasteSimulation(e.to_string()))?;
    enigo
        .key(Key::Unicode('v'), Direction::Click)
        .map_err(|e| ClipboardError::PasteSimulation(e.to_string()))?;
    enigo
        .key(modifier, Direction::Release)
        .map_err(|e| ClipboardError::PasteSimulation(e.to_string()))?;

    tracing::debug!("stt.paste.simulated");
    Ok(())
}

/// Restores the clipboard to a previously saved value.
///
/// Call this after [`simulate_paste`] and the post-paste settle delay.
pub fn restore_clipboard(previous: &str) -> Result<(), ClipboardError> {
    let mut clipboard =
        Clipboard::new().map_err(|e| ClipboardError::ClipboardInit(e.to_string()))?;
    clipboard
        .set_text(previous)
        .map_err(|e| ClipboardError::Write(e.to_string()))?;
    tracing::debug!("stt.clipboard.restored");
    Ok(())
}

/// Returns the modifier key used for the paste shortcut on the current OS.
#[cfg(target_os = "macos")]
fn paste_modifier() -> Key {
    Key::Meta
}

/// Returns the modifier key used for the paste shortcut on the current OS.
#[cfg(not(target_os = "macos"))]
fn paste_modifier() -> Key {
    Key::Control
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clipboard_error_display() {
        let err = ClipboardError::ClipboardInit("no display".into());
        assert_eq!(err.to_string(), "clipboard init failed: no display");
    }

    #[test]
    fn clipboard_error_variants_are_distinct() {
        let init = ClipboardError::ClipboardInit("a".into());
        let read = ClipboardError::Read("b".into());
        let write = ClipboardError::Write("c".into());
        let kbd = ClipboardError::KeyboardInit("d".into());
        let paste = ClipboardError::PasteSimulation("e".into());

        assert_ne!(init.to_string(), read.to_string());
        assert_ne!(read.to_string(), write.to_string());
        assert_ne!(write.to_string(), kbd.to_string());
        assert_ne!(kbd.to_string(), paste.to_string());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn paste_modifier_is_meta_on_macos() {
        assert_eq!(paste_modifier(), Key::Meta);
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn paste_modifier_is_control_on_linux() {
        assert_eq!(paste_modifier(), Key::Control);
    }
}
