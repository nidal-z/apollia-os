//! A `.apo` package opened from the system reaches the preview of the window.
//!
//! The system hands a package file to the application in one of three ways,
//! and the three end in the same place:
//!
//! - **the arguments of this process**, when the file starts the application
//!   (Windows, Linux): [`launch`] reads them once the window exists;
//! - **the arguments of a second launch**, when the application already runs
//!   (Windows, Linux): the single-instance plugin registered by [`attach`]
//!   receives them in the running process, and the second one exits;
//! - **the opened-file event of macOS**, which keeps one process per
//!   application on its own and never passes the file as an argument:
//!   [`on_run_event`] takes it, at launch and afterwards alike.
//!
//! A path cannot be pushed to an interface that is still loading, so the
//! paths wait in [`OpenedPackages`] and the interface takes them: once when it
//! mounts, and again each time [`EVENT_PACKAGE_OPENED`] says more arrived. The
//! event carries no path, so a file is never opened twice.
//!
//! Nothing is installed here. The interface opens the preview of the package,
//! and the package is installed or updated only from that preview.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use tauri::{AppHandle, Emitter, Manager};

/// Tauri event telling the interface that package files wait in
/// [`OpenedPackages`]. No payload.
pub(crate) const EVENT_PACKAGE_OPENED: &str = "package-file-opened";

/// Extension of a package file, compared without regard to case.
const PACKAGE_EXTENSION: &str = "apo";

/// The package files the system opened and the interface has not taken yet.
#[derive(Debug, Default)]
pub(crate) struct OpenedPackages(Mutex<Vec<String>>);

impl OpenedPackages {
    /// Adds `paths` behind those already waiting.
    fn push(&self, paths: Vec<String>) {
        if let Ok(mut waiting) = self.0.lock() {
            waiting.extend(paths);
        }
    }

    /// Hands over the waiting paths, oldest first, and forgets them.
    fn take(&self) -> Vec<String> {
        self.0
            .lock()
            .map(|mut waiting| std::mem::take(&mut *waiting))
            .unwrap_or_default()
    }
}

/// Whether `path` names a package file.
fn is_package(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case(PACKAGE_EXTENSION))
}

/// The package files among the arguments of a launch, as absolute paths.
///
/// The first argument is the program and is skipped. An option or any other
/// file is not a package and is left out. A relative path is resolved against
/// `cwd`, the directory of the process that received the arguments, which for
/// a second launch is not the directory of the running application.
fn packages_in_args<I>(args: I, cwd: &Path) -> Vec<String>
where
    I: IntoIterator<Item = String>,
{
    args.into_iter()
        .skip(1)
        .map(PathBuf::from)
        .filter(|path| is_package(path))
        .map(|path| {
            if path.is_absolute() {
                path
            } else {
                cwd.join(path)
            }
        })
        .map(|path| path.to_string_lossy().into_owned())
        .collect()
}

/// The package files among the URLs of an opened-file event.
///
/// Only a `file:` URL names a file of this machine; any other scheme and any
/// other file type are left out.
#[cfg(any(target_os = "macos", test))]
fn packages_in_urls(urls: &[tauri::Url]) -> Vec<String> {
    urls.iter()
        .filter(|url| url.scheme() == "file")
        .filter_map(|url| url.to_file_path().ok())
        .filter(|path| is_package(path))
        .map(|path| path.to_string_lossy().into_owned())
        .collect()
}

/// Shows the main window and brings it to the front.
fn reveal_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

/// Queues `paths` for the interface, shows the window and tells the interface.
/// Does nothing without a path.
fn route(app: &AppHandle, paths: Vec<String>) {
    if paths.is_empty() {
        return;
    }
    tracing::event!(
        tracing::Level::INFO,
        count = paths.len(),
        "desktop.package_open.routed"
    );
    if let Some(waiting) = app.try_state::<OpenedPackages>() {
        waiting.push(paths);
    }
    reveal_window(app);
    if let Err(error) = app.emit(EVENT_PACKAGE_OPENED, ()) {
        tracing::event!(
            tracing::Level::WARN,
            error = %error,
            "desktop.package_open.emit_failed"
        );
    }
}

/// Registers what opening a package needs on `builder`: the queue of opened
/// files and, on Windows and Linux, the single-instance plugin.
///
/// Call it on the fresh builder: the plugin has to be registered before every
/// other one, so that a second launch exits before it builds anything. The
/// running application then shows its window, whether or not the second launch
/// carried a file.
pub(crate) fn attach(builder: tauri::Builder<tauri::Wry>) -> tauri::Builder<tauri::Wry> {
    #[cfg(any(target_os = "windows", target_os = "linux"))]
    let builder = builder.plugin(tauri_plugin_single_instance::init(|app, args, cwd| {
        reveal_window(app);
        route(app, packages_in_args(args, Path::new(&cwd)));
    }));
    builder.manage(OpenedPackages::default())
}

/// Routes the package files among the arguments of this process. Called from
/// the setup of the application, once the window exists.
pub(crate) fn launch(app: &AppHandle) {
    let cwd = std::env::current_dir().unwrap_or_default();
    route(app, packages_in_args(std::env::args(), &cwd));
}

/// Routes the package files of the opened-file event of macOS. The other
/// systems never send it, and every other event is left alone.
pub(crate) fn on_run_event(app: &AppHandle, event: &tauri::RunEvent) {
    #[cfg(target_os = "macos")]
    if let tauri::RunEvent::Opened { urls } = event {
        route(app, packages_in_urls(urls));
    }
    #[cfg(not(target_os = "macos"))]
    let _ = (app, event);
}

/// The package files the system opened since the last call, oldest first.
#[tauri::command]
pub(crate) fn take_opened_package_files(waiting: tauri::State<'_, OpenedPackages>) -> Vec<String> {
    waiting.take()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|arg| (*arg).to_owned()).collect()
    }

    #[test]
    fn test_packages_in_args_keeps_the_package_and_skips_the_program() {
        // GIVEN the arguments of a launch on a package file, the program
        // first, an option and another file beside it
        let launch = args(&[
            "/opt/apollia/apollia-desktop.apo",
            "--verbose",
            "/home/ada/notes.pdf",
            "/home/ada/Downloads/crm-1.0.0.apo",
        ]);

        // WHEN the package files are read from them
        let packages = packages_in_args(launch, Path::new("/home/ada"));

        // THEN only the package is routed, and the program never is, whatever
        // its name
        assert_eq!(packages, vec!["/home/ada/Downloads/crm-1.0.0.apo"]);
    }

    #[test]
    fn test_packages_in_args_resolves_a_relative_path_against_the_launch_directory() {
        // GIVEN a second launch made from a folder with a relative path
        let launch = args(&["apollia-desktop", "crm.apo"]);
        let cwd = Path::new("/home/ada/Downloads");

        // WHEN the package files are read from them
        let packages = packages_in_args(launch, cwd);

        // THEN the path is the one of that folder, not of the running process
        let expected = cwd.join("crm.apo").to_string_lossy().into_owned();
        assert_eq!(packages, vec![expected]);
    }

    #[test]
    fn test_packages_in_args_reads_the_extension_without_regard_to_case() {
        // GIVEN a package whose extension is written in capitals
        let launch = args(&["apollia-desktop", "/tmp/CRM.APO"]);

        // WHEN the package files are read from them
        let packages = packages_in_args(launch, Path::new("/tmp"));

        // THEN it is a package all the same
        assert_eq!(packages, vec!["/tmp/CRM.APO"]);
    }

    #[test]
    fn test_packages_in_args_is_empty_for_a_launch_without_a_package() {
        // GIVEN a plain launch, and one on a file that is not a package
        let plain = args(&["apollia-desktop"]);
        let other = args(&["apollia-desktop", "/tmp/agent.py", "/tmp/apo"]);

        // WHEN the package files are read from each
        let from_plain = packages_in_args(plain, Path::new("/tmp"));
        let from_other = packages_in_args(other, Path::new("/tmp"));

        // THEN nothing is routed
        assert!(from_plain.is_empty());
        assert!(from_other.is_empty());
    }

    #[test]
    fn test_packages_in_urls_keeps_the_file_url_of_a_package() {
        // GIVEN the URLs of an opened-file event: a package whose name holds a
        // space, a file of another type and a link that is not a file
        let urls = [
            "file:///Users/ada/Downloads/crm%201.0.0.apo",
            "file:///Users/ada/notes.pdf",
            "https://example.org/crm.apo",
        ]
        .map(|url| tauri::Url::parse(url).unwrap());

        // WHEN the package files are read from them
        let packages = packages_in_urls(&urls);

        // THEN only the package of this machine is routed, its path decoded
        let expected = urls[0]
            .to_file_path()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        assert_eq!(packages.len(), 1);
        assert_eq!(packages[0], expected);
        assert!(packages[0].ends_with("crm 1.0.0.apo"));
    }

    #[test]
    fn test_opened_packages_hands_each_path_over_once_in_order() {
        // GIVEN two files opened one after the other before the interface asks
        let waiting = OpenedPackages::default();
        waiting.push(vec!["/tmp/a.apo".to_owned()]);
        waiting.push(vec!["/tmp/b.apo".to_owned()]);

        // WHEN the interface takes them, then asks again
        let first = waiting.take();
        let second = waiting.take();

        // THEN it gets both in order, and nothing the second time
        assert_eq!(first, vec!["/tmp/a.apo", "/tmp/b.apo"]);
        assert!(second.is_empty());
    }
}
