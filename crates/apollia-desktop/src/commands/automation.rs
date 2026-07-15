//! Dev-only automation harness commands.
//!
//! Compiled only in debug builds (the module is declared behind
//! `#[cfg(debug_assertions)]` in `commands/mod.rs`, and every handler entry is
//! gated the same way), so none of this reaches a release binary.
//!
//! The frontend runner (`ui/src/lib/automation/runner.ts`) drives the real app
//! by injecting DOM gestures against stable `data-testid` selectors, then calls
//! these commands to capture the window and persist the run report. Screenshots
//! shell out to the macOS `screencapture` CLI so no new dependency is pulled in.
//!
//! Wiring:
//! - `APOLLIA_AUTOMATION`     path to the JSON script the runner should execute.
//! - `APOLLIA_AUTOMATION_OUT` directory for screenshots + `report.json`
//!   (defaults to a dedicated temp dir when unset).

use std::path::PathBuf;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum AutomationError {
    #[error("APOLLIA_AUTOMATION points to an unreadable script {path}: {source}")]
    ScriptRead {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("main window not found")]
    NoWindow,
    #[error("window geometry unavailable: {0}")]
    Geometry(String),
    #[cfg(not(target_os = "macos"))]
    #[error("screen capture is only supported on macOS")]
    UnsupportedPlatform,
    #[error("screencapture failed: {0}")]
    Capture(String),
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

/// Boot payload handed to the frontend runner: the raw script plus the
/// destructive-allow gate (the runner refuses scripts marked `destructive`
/// unless `APOLLIA_AUTOMATION_ALLOW_DESTRUCTIVE` is set in the environment).
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AutomationBoot {
    script: String,
    allow_destructive: bool,
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
            tracing::info!(
                script = %path.display(),
                bytes = content.len(),
                allow_destructive,
                "automation.script.loaded"
            );
            Ok(Some(AutomationBoot {
                script: content,
                allow_destructive,
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

        let result = tokio::process::Command::new("screencapture")
            .args(["-R", &region, "-x", &out_str])
            .output()
            .await
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
}

#[cfg(target_os = "macos")]
async fn capture_inner(app: tauri::AppHandle, label: String) -> Result<String, AutomationError> {
    capture::run(app, label).await
}

#[cfg(not(target_os = "macos"))]
async fn capture_inner(_app: tauri::AppHandle, _label: String) -> Result<String, AutomationError> {
    Err(AutomationError::UnsupportedPlatform)
}
