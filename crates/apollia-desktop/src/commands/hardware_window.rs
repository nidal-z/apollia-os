//! The window that shows System > Hardware on its own.
//!
//! It is a window of the operating system, apart from the settings: it stays
//! open when they close, and its resource monitor keeps measuring. One label,
//! one window: asking for it again brings the live one to the front, the way
//! the dictation overlay reuses its own.

use serde::Serialize;
use tauri::{Manager, WebviewUrl, WebviewWindowBuilder};

/// Window label of the hardware window.
const WINDOW_LABEL: &str = "hardware";

/// The page the window loads, built beside the main one.
const WINDOW_PAGE: &str = "hardware.html";

/// Size the window opens at, and the smallest it shrinks to, in logical
/// pixels.
const OPEN_SIZE: (f64, f64) = (720.0, 820.0);
const MIN_SIZE: (f64, f64) = (520.0, 420.0);

/// What asking for the window did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HardwareWindowOutcome {
    /// No window was open: one was created.
    Created,
    /// The window was open: it was brought to the front, and no other made.
    Focused,
}

/// The two gestures the opening needs of the window system.
trait WindowHost {
    /// Brings the live window to the front; `false` when there is none.
    fn focus_existing(&self) -> Result<bool, String>;
    /// Creates the window.
    fn create(&self) -> Result<(), String>;
}

/// Opens the window on `host`, or brings the live one to the front.
fn open_on(host: &impl WindowHost) -> Result<HardwareWindowOutcome, String> {
    if host.focus_existing()? {
        return Ok(HardwareWindowOutcome::Focused);
    }
    host.create()?;
    Ok(HardwareWindowOutcome::Created)
}

impl WindowHost for tauri::AppHandle {
    fn focus_existing(&self) -> Result<bool, String> {
        let Some(window) = self.get_webview_window(WINDOW_LABEL) else {
            return Ok(false);
        };
        // A minimised window does not come forward on focus alone.
        window.unminimize().map_err(|e| e.to_string())?;
        window.show().map_err(|e| e.to_string())?;
        window.set_focus().map_err(|e| e.to_string())?;
        Ok(true)
    }

    fn create(&self) -> Result<(), String> {
        WebviewWindowBuilder::new(self, WINDOW_LABEL, WebviewUrl::App(WINDOW_PAGE.into()))
            .title(crate::i18n::hardware_window_title(crate::i18n::locale()))
            .inner_size(OPEN_SIZE.0, OPEN_SIZE.1)
            .min_inner_size(MIN_SIZE.0, MIN_SIZE.1)
            .resizable(true)
            .build()
            .map(|_| ())
            .map_err(|e| e.to_string())
    }
}

/// Opens System > Hardware in a window of its own, or brings that window to
/// the front when it is already open.
#[tauri::command]
pub async fn open_hardware_window(app: tauri::AppHandle) -> Result<HardwareWindowOutcome, String> {
    let outcome = open_on(&app)?;
    tracing::event!(tracing::Level::INFO, outcome = ?outcome, "desktop.hardware_window.opened");
    Ok(outcome)
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use super::*;

    /// A window system that counts the windows it was asked to create.
    #[derive(Default)]
    struct CountingHost {
        created: Cell<usize>,
        focused: Cell<usize>,
    }

    impl WindowHost for CountingHost {
        fn focus_existing(&self) -> Result<bool, String> {
            if self.created.get() == 0 {
                return Ok(false);
            }
            self.focused.set(self.focused.get() + 1);
            Ok(true)
        }

        fn create(&self) -> Result<(), String> {
            self.created.set(self.created.get() + 1);
            Ok(())
        }
    }

    #[test]
    fn test_a_second_click_brings_the_window_forward_without_creating_another() {
        // GIVEN a window system with no hardware window
        let host = CountingHost::default();

        // WHEN the window is asked for twice
        let first = open_on(&host).unwrap();
        let second = open_on(&host).unwrap();

        // THEN the first click creates it, the second brings it to the front,
        //      and one window exists
        assert_eq!(first, HardwareWindowOutcome::Created);
        assert_eq!(second, HardwareWindowOutcome::Focused);
        assert_eq!(host.created.get(), 1);
        assert_eq!(host.focused.get(), 1);
    }

    #[test]
    fn test_a_window_that_cannot_be_created_is_reported_and_not_counted_open() {
        // GIVEN a window system that refuses to create a window
        struct Refusing;
        impl WindowHost for Refusing {
            fn focus_existing(&self) -> Result<bool, String> {
                Ok(false)
            }
            fn create(&self) -> Result<(), String> {
                Err("no display".to_owned())
            }
        }

        // WHEN the window is asked for
        let outcome = open_on(&Refusing);

        // THEN the refusal reaches the caller
        assert_eq!(outcome, Err("no display".to_owned()));
    }
}
