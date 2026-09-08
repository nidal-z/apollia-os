//! Dev-only automation harness commands.
//!
//! Compiled only in debug builds (the module is declared behind
//! `#[cfg(debug_assertions)]` in `commands/mod.rs`, and every handler entry is
//! gated the same way), so none of this reaches a release binary.
//!
//! The frontend runner (`ui/src/lib/automation/runner.ts`) drives the real app
//! by injecting DOM gestures against stable `data-testid` selectors, then calls
//! these commands to capture the window, resize it for the narrow-layout
//! anchors, and persist the run report. Screenshots shell out to the macOS
//! `screencapture` CLI so no new dependency is pulled in.
//!
//! Wiring:
//! - `APOLLIA_AUTOMATION`     path to the JSON script the runner should execute.
//! - `APOLLIA_AUTOMATION_OUT` directory for screenshots + `report.json`
//!   (defaults to a dedicated temp dir when unset).

use std::path::PathBuf;

use thiserror::Error;

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum AutomationError {
    #[error("APOLLIA_AUTOMATION points to an unreadable script {path}: {source}")]
    ScriptRead {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("main window not found")]
    NoWindow,
    // The two capture variants are built only inside `mod capture`, which is
    // macOS-only: without the gate they are dead code everywhere else and
    // `-D warnings` rejects the build. Mirror image of `UnsupportedPlatform`.
    #[cfg(target_os = "macos")]
    #[error("window geometry unavailable: {0}")]
    Geometry(String),
    #[cfg(not(target_os = "macos"))]
    #[error("screen capture is only supported on macOS")]
    UnsupportedPlatform,
    #[cfg(target_os = "macos")]
    #[error("screencapture failed: {0}")]
    Capture(String),
    #[error("window resize failed: {0}")]
    Resize(String),
    #[error("could not prepare output dir {path}: {source}")]
    OutputDir {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("could not write report {path}: {source}")]
    ReportWrite {
        path: String,
        #[source]
        source: std::io::Error,
    },
}

/// Resolve (and create) the directory where captures and the report land.
fn output_dir() -> Result<PathBuf, AutomationError> {
    let dir = std::env::var_os("APOLLIA_AUTOMATION_OUT")
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("apollia-automation"));
    std::fs::create_dir_all(&dir).map_err(|source| AutomationError::OutputDir {
        path: dir.display().to_string(),
        source,
    })?;
    Ok(dir)
}

/// Boot payload handed to the frontend runner: the raw script, the
/// destructive-allow gate (the runner refuses scripts marked `destructive`
/// unless `APOLLIA_AUTOMATION_ALLOW_DESTRUCTIVE` is set in the environment),
/// and the `HOME` the process runs under, which the runner substitutes for
/// `${HOME}` in script strings so a recipe can name a file of the seeded,
/// throwaway home without knowing where the recipe put it.
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AutomationBoot {
    script: String,
    allow_destructive: bool,
    home_dir: String,
    python_path: String,
}

/// A Python 3 that actually starts, for the `${PYTHON}` token of a recipe.
///
/// A book that has to name an interpreter, the custom MCP form being the one
/// that does, used to spell `/usr/bin/python3`: right on macOS and on most
/// Linux images, and a path that names nothing on Windows, where the server it
/// declares then fails to spawn with "the system cannot find the path
/// specified". Candidates are tried by RUNNING them, because `python3` on
/// Windows is a Microsoft Store alias that sits on PATH and refuses to start.
/// Empty when none answers: the runner then refuses `${PYTHON}` rather than
/// expanding it to nothing.
fn resolve_python() -> String {
    for candidate in ["python3", "python"] {
        let mut probe = std::process::Command::new(candidate);
        probe.arg("-c").arg("import sys; sys.exit(0)");
        apollia_core::subprocess_window::hide_console(&mut probe);
        let started = probe
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if started {
            return candidate.to_string();
        }
    }
    String::new()
}

/// Returns the JSON script pointed at by `APOLLIA_AUTOMATION`, or `None` when the
/// env var is unset (the normal, non-automation boot path).
#[tauri::command]
pub async fn automation_script() -> Result<Option<AutomationBoot>, String> {
    let Some(path) = std::env::var_os("APOLLIA_AUTOMATION") else {
        return Ok(None);
    };
    let path = PathBuf::from(path);
    match std::fs::read_to_string(&path) {
        Ok(content) => {
            let allow_destructive =
                std::env::var_os("APOLLIA_AUTOMATION_ALLOW_DESTRUCTIVE").is_some();
            // The same resolution the app uses for its data dir (main.rs), so
            // `${HOME}` names the seed home the recipe swapped in. Empty when no
            // home resolves: the runner then refuses any `${HOME}` rather than
            // expanding it to nothing.
            // The same resolution as the app's data dir (main.rs), so the value is
            // the seed HOME the recipe swapped in.
            let home_dir = apollia_core::paths::home_dir_or_temp()
                .display()
                .to_string();
            tracing::info!(
                script = %path.display(),
                bytes = content.len(),
                allow_destructive,
                home_dir = %home_dir,
                "automation.script.loaded"
            );
            Ok(Some(AutomationBoot {
                script: content,
                allow_destructive,
                home_dir,
                python_path: resolve_python(),
            }))
        }
        Err(source) => Err(AutomationError::ScriptRead {
            path: path.display().to_string(),
            source,
        }
        .to_string()),
    }
}

/// Capture the main window to a PNG and return its absolute path.
#[tauri::command]
pub async fn automation_capture(app: tauri::AppHandle, label: String) -> Result<String, String> {
    capture_inner(app, label).await.map_err(|e| e.to_string())
}

/// Resize the main window to a logical `width` x `height`.
///
/// `tauri.conf.json` pins a minimum size (900 x 600 today) that the OS
/// enforces on `set_size`, so the anchors gated on a narrow viewport (the
/// sidebar drawer, the mobile settings nav, the tasks FAB) are unreachable
/// through a plain resize. The minimum is lifted before the resize and put
/// back, from the same config, as soon as the requested size satisfies it
/// again, so the product constraint returns with the product width.
#[tauri::command]
pub async fn automation_resize(
    app: tauri::AppHandle,
    width: f64,
    height: f64,
) -> Result<(), String> {
    resize_inner(&app, width, height).map_err(|e| e.to_string())
}

fn resize_inner(app: &tauri::AppHandle, width: f64, height: f64) -> Result<(), AutomationError> {
    use tauri::Manager;

    let window = app
        .get_webview_window("main")
        .ok_or(AutomationError::NoWindow)?;
    let min = app
        .config()
        .app
        .windows
        .iter()
        .find(|w| w.label == "main")
        .and_then(|w| w.min_width.zip(w.min_height));
    window
        .set_min_size(None::<tauri::Size>)
        .map_err(|e| AutomationError::Resize(e.to_string()))?;
    window
        .set_size(tauri::LogicalSize::new(width, height))
        .map_err(|e| AutomationError::Resize(e.to_string()))?;
    let restored = match min {
        Some((min_w, min_h)) if width >= min_w && height >= min_h => {
            window
                .set_min_size(Some(tauri::LogicalSize::new(min_w, min_h)))
                .map_err(|e| AutomationError::Resize(e.to_string()))?;
            true
        }
        _ => false,
    };
    tracing::info!(
        width,
        height,
        min_size_restored = restored,
        "automation.resize.applied"
    );
    Ok(())
}

/// Persist the run report JSON into the output dir and return its path.
#[tauri::command]
pub async fn automation_finish(report_json: String) -> Result<String, String> {
    let out = output_dir().map_err(|e| e.to_string())?.join("report.json");
    std::fs::write(&out, report_json.as_bytes()).map_err(|source| {
        AutomationError::ReportWrite {
            path: out.display().to_string(),
            source,
        }
        .to_string()
    })?;
    tracing::info!(path = %out.display(), bytes = report_json.len(), "automation.finish.report");
    Ok(out.display().to_string())
}

#[cfg(target_os = "macos")]
mod capture {
    use super::{output_dir, AutomationError};
    use std::sync::atomic::{AtomicU32, Ordering};
    use tauri::Manager;

    static CAPTURE_SEQ: AtomicU32 = AtomicU32::new(0);

    /// Keep only filename-safe characters so a step label cannot escape the dir.
    fn sanitize_label(label: &str) -> String {
        let cleaned: String = label
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                    c
                } else {
                    '-'
                }
            })
            .collect();
        if cleaned.is_empty() {
            "capture".to_string()
        } else {
            cleaned
        }
    }

    /// How long one `screencapture` call may take before the step is failed.
    const CAPTURE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(20);

    pub(super) async fn run(
        app: tauri::AppHandle,
        label: String,
    ) -> Result<String, AutomationError> {
        let window = app
            .get_webview_window("main")
            .ok_or(AutomationError::NoWindow)?;
        let pos = window
            .outer_position()
            .map_err(|e| AutomationError::Geometry(e.to_string()))?;
        let size = window
            .outer_size()
            .map_err(|e| AutomationError::Geometry(e.to_string()))?;
        let scale = window
            .scale_factor()
            .map_err(|e| AutomationError::Geometry(e.to_string()))?;

        // `screencapture -R` expects POINTS; Tauri window geometry is physical px.
        let x = (f64::from(pos.x) / scale).round();
        let y = (f64::from(pos.y) / scale).round();
        let w = (f64::from(size.width) / scale).round();
        let h = (f64::from(size.height) / scale).round();
        let region = format!("{x},{y},{w},{h}");

        let seq = CAPTURE_SEQ.fetch_add(1, Ordering::Relaxed);
        let out = output_dir()?.join(format!("{seq:03}-{}.png", sanitize_label(&label)));
        let out_str = out.display().to_string();

        // Capture the specific window by its id (`-l`) so an editor or any other
        // window sitting on top of the app region does not get captured instead.
        // Falls back to a screen-region capture (`-R`) if the id is unavailable.
        let mut command = tokio::process::Command::new("screencapture");
        apollia_core::subprocess_window::hide_console_async(&mut command);
        match window_number(&window) {
            Some(id) => {
                command.args(["-l", &id.to_string(), "-o", "-x", &out_str]);
            }
            None => {
                command.args(["-R", &region, "-x", &out_str]);
            }
        }
        // A capture that does not return within the budget is reported as a
        // failed step rather than holding the whole book: measured on
        // 2026-09-06, one `screencapture -l` stayed silent for twenty minutes
        // under a screen-sharing session before failing.
        let result = tokio::time::timeout(CAPTURE_TIMEOUT, command.output())
            .await
            .map_err(|_| {
                AutomationError::Capture(format!(
                    "screencapture gave no answer within {}s",
                    CAPTURE_TIMEOUT.as_secs()
                ))
            })?
            .map_err(|e| AutomationError::Capture(e.to_string()))?;
        if !result.status.success() {
            let stderr = String::from_utf8_lossy(&result.stderr);
            return Err(AutomationError::Capture(format!(
                "screencapture exited with {}: {}",
                result.status,
                stderr.trim()
            )));
        }
        tracing::info!(label = %label, path = %out_str, region = %region, "automation.capture.saved");
        Ok(out_str)
    }

    /// The CoreGraphics window id of the app window, read from the underlying
    /// `NSWindow.windowNumber`. Used so `screencapture -l` grabs the app content
    /// regardless of what is stacked on top of it. Returns `None` if the native
    /// handle cannot be resolved (the caller then falls back to region capture).
    fn window_number(window: &tauri::WebviewWindow) -> Option<i64> {
        use objc2::runtime::AnyObject;

        let ptr = window.ns_window().ok()?;
        if ptr.is_null() {
            return None;
        }
        // SAFETY: `ns_window()` returns a valid `NSWindow*` owned by the window,
        // which stays alive for the duration of this synchronous call. We only
        // send the `windowNumber` getter, which has no side effects.
        let number: isize = unsafe {
            let ns_window = &*(ptr.cast::<AnyObject>());
            objc2::msg_send![ns_window, windowNumber]
        };
        (number > 0).then_some(number as i64)
    }
}

#[cfg(target_os = "macos")]
async fn capture_inner(app: tauri::AppHandle, label: String) -> Result<String, AutomationError> {
    capture::run(app, label).await
}

#[cfg(not(target_os = "macos"))]
async fn capture_inner(_app: tauri::AppHandle, _label: String) -> Result<String, AutomationError> {
    Err(AutomationError::UnsupportedPlatform)
}
