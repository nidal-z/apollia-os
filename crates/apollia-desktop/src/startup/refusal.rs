//! What a refused launch leaves the person who launched it: a window that
//! names what holds the port and the gesture to make, and a line in
//! `<data_dir>/startup-refused.log`, which is what a launch from the Finder or
//! a desktop entry can still be read from afterwards.

use std::io::Write;
use std::path::Path;

use tauri::Manager;
use tauri_plugin_dialog::{DialogExt, MessageDialogKind};

use crate::i18n::UiLocale;
use crate::startup::RefusalReason;

/// The exit code of a refused launch, once the window has been read.
const REFUSAL_EXIT_CODE: i32 = 1;

/// The file a refusal is appended to, under the data directory.
const REFUSAL_LOG: &str = "startup-refused.log";

/// Records `reason`, shows it in a window, and exits once the window is closed.
pub(crate) fn refuse(
    reason: &RefusalReason,
    data_dir: &Path,
    context: tauri::Context<tauri::Wry>,
) -> ! {
    tracing::event!(
        tracing::Level::ERROR,
        reason = %reason,
        "desktop.startup.refused"
    );
    let log = data_dir.join(REFUSAL_LOG);
    if let Err(error) = append_refusal(&log, reason) {
        tracing::event!(
            tracing::Level::WARN,
            path = %log.display(),
            error = %error,
            "desktop.startup.refusal_log_failed"
        );
    }
    let locale = locale_from_env(|name| std::env::var(name).ok());
    show_window(
        crate::i18n::startup_refused_title(locale).to_owned(),
        crate::i18n::startup_refused_message(locale, reason, &log),
        context,
    );
    std::process::exit(REFUSAL_EXIT_CODE)
}

/// Appends one dated line naming the reason.
fn append_refusal(log: &Path, reason: &RefusalReason) -> std::io::Result<()> {
    if let Some(parent) = log.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log)?;
    writeln!(
        file,
        "{} {reason}",
        chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
    )
}

/// Runs a Tauri application whose only surface is the refusal dialog.
///
/// The window the configuration declares is destroyed before it loads
/// anything: none of the commands its interface calls is registered here.
fn show_window(title: String, body: String, context: tauri::Context<tauri::Wry>) {
    let built = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(move |app| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.destroy();
            }
            let handle = app.handle().clone();
            app.dialog()
                .message(body)
                .title(title)
                .kind(MessageDialogKind::Warning)
                .show(move |_| handle.exit(REFUSAL_EXIT_CODE));
            Ok(())
        })
        .build(context);
    match built {
        // `run_return`, not `run`: `run` ends the process itself with code 0,
        // and a refused launch has to end with its own code.
        Ok(app) => {
            let _ = app.run_return(|_, event| {
                // Destroying the declared window asks to exit with no code; only
                // closing the dialog ends the application.
                if let tauri::RunEvent::ExitRequested {
                    code: None, api, ..
                } = event
                {
                    api.prevent_exit();
                }
            });
        }
        Err(error) => tracing::event!(
            tracing::Level::ERROR,
            error = %error,
            "desktop.startup.refusal_window_failed"
        ),
    }
}

/// The interface language a launch with no interface yet can infer: the
/// locale variables of the environment, French when none says English, the
/// same default the interface resolves for a first launch.
fn locale_from_env(var: impl Fn(&str) -> Option<String>) -> UiLocale {
    let tag = ["LC_ALL", "LC_MESSAGES", "LANG"]
        .into_iter()
        .filter_map(&var)
        .find(|value| !value.is_empty());
    match tag {
        Some(tag) if tag.to_ascii_lowercase().starts_with("en") => UiLocale::En,
        _ => UiLocale::Fr,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn incompatible_refusal() -> RefusalReason {
        RefusalReason::IncompatibleRuntime {
            port: 7771,
            version: "0.2.0-preview".to_owned(),
            ours: "0.3.0",
        }
    }

    #[test]
    fn test_the_refusal_names_the_running_runtime_and_the_gesture() {
        // GIVEN a desktop launched while `apollia-os start` serves another version
        let reason = incompatible_refusal();
        let log = PathBuf::from("/data/startup-refused.log");

        // WHEN the window text is composed, in each language
        let fr = crate::i18n::startup_refused_message(UiLocale::Fr, &reason, &log);
        let en = crate::i18n::startup_refused_message(UiLocale::En, &reason, &log);

        // THEN it names the runtime in its version, the gesture, and the log
        for text in [&fr, &en] {
            assert!(text.contains("0.2.0-preview"), "{text}");
            assert!(text.contains("apollia-os stop"), "{text}");
            assert!(text.contains("/data/startup-refused.log"), "{text}");
        }
        assert!(fr.contains("relancez"), "{fr}");
        assert!(en.contains("launch Apollia OS again"), "{en}");
    }

    #[test]
    fn test_the_refusal_is_appended_to_the_log() {
        // GIVEN a data directory with an earlier refusal logged
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join(REFUSAL_LOG);
        append_refusal(&log, &RefusalReason::ForeignListener { port: 7771 }).unwrap();

        // WHEN a second refusal is recorded
        append_refusal(&log, &incompatible_refusal()).unwrap();

        // THEN both lines are there, the last one naming the runtime
        let content = std::fs::read_to_string(&log).unwrap();
        let lines: Vec<&str> = content.lines().collect();
        assert_eq!(lines.len(), 2);
        assert!(lines[0].contains("not answer as an Apollia runtime"));
        assert!(lines[1].contains("0.2.0-preview"));
        assert!(lines[1].contains("cannot drive it"));
    }

    #[test]
    fn test_the_locale_follows_the_environment_and_defaults_to_french() {
        // GIVEN environments with and without an English locale
        let english = |name: &str| (name == "LANG").then(|| "en_US.UTF-8".to_owned());
        let french = |name: &str| (name == "LC_ALL").then(|| "fr_FR.UTF-8".to_owned());
        let none = |_: &str| None;

        // WHEN the locale is inferred
        // THEN English is chosen only when the environment says so
        assert_eq!(locale_from_env(english), UiLocale::En);
        assert_eq!(locale_from_env(french), UiLocale::Fr);
        assert_eq!(locale_from_env(none), UiLocale::Fr);
    }
}
