//! The window the logs of the local server detach into.
//!
//! It is a window of the operating system, apart from the main one, the way
//! the hardware window is: one label, one window, and asking for it again
//! brings the live one to the front. Closing it, by its own button or by the
//! window system, hands the logs back to the developer screen: the main
//! window is told with an event and shows its pane again.

use serde::Serialize;
use tauri::{Emitter, Manager, WebviewUrl, WebviewWindowBuilder};

/// Window label of the logs window.
const WINDOW_LABEL: &str = "server-logs";

/// The page the window loads, built beside the main one.
const WINDOW_PAGE: &str = "logs.html";

/// Event sent to every window once the logs window is gone.
const REATTACHED_EVENT: &str = "server-logs:reattached";

/// Size the window opens at, and the smallest it shrinks to, in logical
/// pixels.
const OPEN_SIZE: (f64, f64) = (900.0, 520.0);
const MIN_SIZE: (f64, f64) = (480.0, 240.0);

/// What asking for the window did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LogsWindowOutcome {
    /// No window was open: one was created.
    Created,
    /// The window was open: it was brought to the front, and no other made.
    Focused,
}

/// The gestures the window needs of the window system.
trait WindowHost {
    /// Brings the live window to the front; `false` when there is none.
    fn focus_existing(&self) -> Result<bool, String>;
    /// Creates the window.
    fn create(&self) -> Result<(), String>;
    /// Closes the live window; `false` when there is none.
    fn close_existing(&self) -> Result<bool, String>;
}

/// Opens the window on `host`, or brings the live one to the front.
fn open_on(host: &impl WindowHost) -> Result<LogsWindowOutcome, String> {
    if host.focus_existing()? {
        return Ok(LogsWindowOutcome::Focused);
    }
    host.create()?;
    Ok(LogsWindowOutcome::Created)
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
        let window =
            WebviewWindowBuilder::new(self, WINDOW_LABEL, WebviewUrl::App(WINDOW_PAGE.into()))
                .title(crate::i18n::server_logs_window_title(crate::i18n::locale()))
                .inner_size(OPEN_SIZE.0, OPEN_SIZE.1)
                .min_inner_size(MIN_SIZE.0, MIN_SIZE.1)
                .resizable(true)
                .build()
                .map_err(|e| e.to_string())?;
        let app = self.clone();
        window.on_window_event(move |event| {
            if matches!(event, tauri::WindowEvent::Destroyed) {
                crate::commands::server_logs::stop_stream(WINDOW_LABEL);
                if let Err(error) = app.emit(REATTACHED_EVENT, ()) {
                    tracing::event!(
                        tracing::Level::DEBUG,
                        error = %error,
                        "desktop.logs_window.reattach_emit_failed"
                    );
                }
            }
        });
        Ok(())
    }

    fn close_existing(&self) -> Result<bool, String> {
        let Some(window) = self.get_webview_window(WINDOW_LABEL) else {
            return Ok(false);
        };
        window.close().map_err(|e| e.to_string())?;
        Ok(true)
    }
}

/// Opens the logs of the local server in a window of their own, or brings
/// that window to the front when it is already open.
#[tauri::command]
pub async fn open_server_logs_window(app: tauri::AppHandle) -> Result<LogsWindowOutcome, String> {
    let outcome = open_on(&app)?;
    tracing::event!(tracing::Level::INFO, outcome = ?outcome, "desktop.logs_window.opened");
    Ok(outcome)
}

/// Closes the logs window, which hands the logs back to the developer screen.
/// Answers whether a window was open.
#[tauri::command]
pub async fn close_server_logs_window(app: tauri::AppHandle) -> Result<bool, String> {
    let closed = app.close_existing()?;
    tracing::event!(tracing::Level::INFO, closed, "desktop.logs_window.closed");
    Ok(closed)
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use super::*;

    /// A window system that counts the windows it holds and the gestures it
    /// was asked for.
    #[derive(Default)]
    struct CountingHost {
        open: Cell<usize>,
        created: Cell<usize>,
        focused: Cell<usize>,
    }

    impl WindowHost for CountingHost {
        fn focus_existing(&self) -> Result<bool, String> {
            if self.open.get() == 0 {
                return Ok(false);
            }
            self.focused.set(self.focused.get() + 1);
            Ok(true)
        }

        fn create(&self) -> Result<(), String> {
            self.open.set(self.open.get() + 1);
            self.created.set(self.created.get() + 1);
            Ok(())
        }

        fn close_existing(&self) -> Result<bool, String> {
            if self.open.get() == 0 {
                return Ok(false);
            }
            self.open.set(self.open.get() - 1);
            Ok(true)
        }
    }

    #[test]
    fn test_detaching_twice_brings_the_one_window_forward() {
        // GIVEN a window system with no logs window
        let host = CountingHost::default();

        // WHEN the logs are detached twice
        let first = open_on(&host).unwrap();
        let second = open_on(&host).unwrap();

        // THEN the first gesture creates the window, the second brings it to
        //      the front, and one window exists
        assert_eq!(first, LogsWindowOutcome::Created);
        assert_eq!(second, LogsWindowOutcome::Focused);
        assert_eq!(host.created.get(), 1);
        assert_eq!(host.focused.get(), 1);
    }

    #[test]
    fn test_closing_ends_the_window_and_detaching_again_creates_a_new_one() {
        // GIVEN a window system with the logs window open
        let host = CountingHost::default();
        open_on(&host).unwrap();

        // WHEN the window is closed, closed again, then the logs detached
        let closed = host.close_existing().unwrap();
        let again = host.close_existing().unwrap();
        let reopened = open_on(&host).unwrap();

        // THEN the first close ends it, the second finds nothing, and the
        //      next detach creates a window rather than looking for the old one
        assert!(closed);
        assert!(!again);
        assert_eq!(reopened, LogsWindowOutcome::Created);
        assert_eq!(host.created.get(), 2);
    }

    #[test]
    fn test_a_window_that_cannot_be_created_is_reported() {
        // GIVEN a window system that refuses to create a window
        struct Refusing;
        impl WindowHost for Refusing {
            fn focus_existing(&self) -> Result<bool, String> {
                Ok(false)
            }
            fn create(&self) -> Result<(), String> {
                Err("no display".to_owned())
            }
            fn close_existing(&self) -> Result<bool, String> {
                Ok(false)
            }
        }

        // WHEN the logs are detached
        let outcome = open_on(&Refusing);

        // THEN the refusal reaches the caller
        assert_eq!(outcome, Err("no display".to_owned()));
    }
}
