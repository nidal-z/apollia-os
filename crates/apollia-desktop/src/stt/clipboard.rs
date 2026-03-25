//! Clipboard injection for STT transcription results.
//!
//! Saves the current clipboard content, sets the transcribed text, simulates
//! a system Paste shortcut (`Cmd+V` on macOS, `Ctrl+V` on Linux), and
//! optionally restores the previous clipboard content after a short delay.

use std::thread;
use std::time::Duration;

use arboard::Clipboard;
use enigo::{Direction, Enigo, Key, Keyboard, Settings};

/// Delay between setting clipboard text and simulating the paste shortcut.
///
/// Applications need time to process the paste event before the clipboard
/// content can be restored.
const PASTE_SETTLE_MS: u64 = 100;

/// Errors that can occur during clipboard injection.
#[derive(Debug, thiserror::Error)]
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

/// Injects text at the current cursor position via clipboard + simulated paste.
///
/// The function is intentionally **blocking** (it sleeps for [`PASTE_SETTLE_MS`]
/// when `restore` is `true`). Call it from a background thread or
/// `spawn_blocking` to avoid stalling the async runtime.
///
/// # Arguments
///
/// * `text`    – The text to inject at the cursor position.
/// * `restore` – When `true`, the previous clipboard content is saved before
///   injection and restored after the paste is processed.
pub fn inject(text: &str, restore: bool) -> Result<(), ClipboardError> {
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

    simulate_paste()?;

    if let Some(prev) = previous {
        thread::sleep(Duration::from_millis(PASTE_SETTLE_MS));
        clipboard
            .set_text(&prev)
            .map_err(|e| ClipboardError::Write(e.to_string()))?;
    }

    tracing::debug!(len = text.len(), restore, "text injected via clipboard");
    Ok(())
}

/// Simulates the platform paste shortcut (`Cmd+V` / `Ctrl+V`).
fn simulate_paste() -> Result<(), ClipboardError> {
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
