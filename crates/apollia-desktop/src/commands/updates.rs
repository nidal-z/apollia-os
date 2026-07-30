//! Tauri IPC commands for managing automatic updates.
//!
//! Checks for new versions on GitHub Releases and triggers the Tauri
//! application update via `tauri-plugin-updater`.

use serde::Serialize;
use tauri::Emitter;
use tauri_plugin_updater::UpdaterExt;
use thiserror::Error;

/// Possible errors when querying the updater plugin.
#[derive(Debug, Error)]
pub enum UpdateError {
    #[error("updater plugin unavailable: {0}")]
    Plugin(String),
    #[error("update check failed: {0}")]
    Check(String),
    #[error("no update available")]
    NoUpdate,
    #[error("install failed: {0}")]
    Install(String),
    /// This build carries no updater signing key, so self-update is inert.
    #[error(
        "self-update is not configured for this build: it needs a signing key and a signed \
         release manifest. Download the new version from the releases page instead."
    )]
    NotConfigured,
}

/// Whether this build lacks the updater signing key.
///
/// Read from the Tauri config the app was built with, so the answer follows the
/// build rather than a duplicated constant. An absent or empty `pubkey` means the
/// plugin can never verify an artifact, and therefore can never install one.
fn updater_signing_key_is_absent(app: &tauri::AppHandle) -> bool {
    app.config()
        .plugins
        .0
        .get("updater")
        .and_then(|v| v.get("pubkey"))
        .and_then(|v| v.as_str())
        .is_none_or(str::is_empty)
}

/// Download progress emitted during [`install_update`] via the
/// `"update-download-progress"` Tauri event.
///
/// `total` mirrors the server's `Content-Length`: it is `None` (serialised as
/// `null`) when the endpoint does not advertise a size, in which case the
/// frontend renders an indeterminate progress bar.
#[derive(Debug, Clone, Serialize)]
pub struct UpdateDownloadProgress {
    /// Cumulative bytes downloaded so far.
    pub downloaded: u64,
    /// Total download size in bytes, or `None` when the server omits it.
    pub total: Option<u64>,
}

/// Result of the update check.
#[derive(Debug, Serialize)]
pub struct UpdateCheckResult {
    /// Whether an update is available.
    pub available: bool,
    /// Current application version.
    pub current_version: String,
    /// Available version (if `available` is `true`).
    pub new_version: Option<String>,
    /// Release notes (if `available` is `true`).
    pub release_notes: Option<String>,
}

/// Checks whether an update is available on GitHub Releases.
///
/// Uses `tauri-plugin-updater` to query the endpoint configured in
/// `tauri.conf.json` (`plugins.updater.endpoints` section). Returns a
/// serializable description of the current state.
#[tauri::command]
pub async fn check_for_update(app: tauri::AppHandle) -> Result<UpdateCheckResult, String> {
    let current_version = app.package_info().version.to_string();

    // Self-update needs a signing key: the plugin verifies the downloaded
    // artifact against `plugins.updater.pubkey` before installing it, and a
    // release must publish a signed `latest.json`. Until both exist, say so
    // plainly instead of surfacing the plugin's internal error, which reads like
    // a network failure and sends the user looking in the wrong place.
    if updater_signing_key_is_absent(&app) {
        return Err(UpdateError::NotConfigured.to_string());
    }

    let updater = app
        .updater()
        .map_err(|e| UpdateError::Plugin(e.to_string()).to_string())?;

    match updater.check().await {
        Ok(Some(update)) => Ok(UpdateCheckResult {
            available: true,
            current_version,
            new_version: Some(update.version.clone()),
            release_notes: update.body.clone(),
        }),
        Ok(None) => Ok(UpdateCheckResult {
            available: false,
            current_version,
            new_version: None,
            release_notes: None,
        }),
        Err(e) => Err(UpdateError::Check(e.to_string()).to_string()),
    }
}

/// Downloads and installs the available update, then restarts.
///
/// Must be called after `check_for_update`. Returns `NoUpdate` if nothing is
/// available anymore (a concurrent instance, or a pulled release).
///
/// Emits `"update-download-progress"` ([`UpdateDownloadProgress`]) on every
/// downloaded chunk so the frontend can render a live progress bar instead of
/// blocking silently until completion.
#[tauri::command]
pub async fn install_update(app: tauri::AppHandle) -> Result<(), String> {
    let updater = app
        .updater()
        .map_err(|e| UpdateError::Plugin(e.to_string()).to_string())?;

    let update = updater
        .check()
        .await
        .map_err(|e| UpdateError::Check(e.to_string()).to_string())?
        .ok_or_else(|| UpdateError::NoUpdate.to_string())?;

    let progress_app = app.clone();
    let mut downloaded: u64 = 0;

    update
        .download_and_install(
            move |chunk_length, content_length| {
                downloaded = downloaded.saturating_add(chunk_length as u64);
                let payload = UpdateDownloadProgress {
                    downloaded,
                    total: content_length,
                };
                if let Err(e) = progress_app.emit("update-download-progress", &payload) {
                    tracing::warn!(error = %e, "failed to emit update-download-progress event");
                }
            },
            || {},
        )
        .await
        .map_err(|e| UpdateError::Install(e.to_string()).to_string())?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_update_check_result_serializes_available() {
        // GIVEN an UpdateCheckResult with a new version available
        let result = UpdateCheckResult {
            available: true,
            current_version: "0.9.0".to_string(),
            new_version: Some("1.0.0".to_string()),
            release_notes: Some("Bug fixes and improvements".to_string()),
        };

        // WHEN serialized to JSON
        let json = serde_json::to_value(&result).expect("serialize");

        // THEN all fields are present and correct
        assert_eq!(json["available"], true);
        assert_eq!(json["current_version"], "0.9.0");
        assert_eq!(json["new_version"], "1.0.0");
        assert_eq!(json["release_notes"], "Bug fixes and improvements");
    }

    #[test]
    fn test_update_check_result_serializes_up_to_date() {
        // GIVEN an UpdateCheckResult with no update available
        let result = UpdateCheckResult {
            available: false,
            current_version: "1.0.0".to_string(),
            new_version: None,
            release_notes: None,
        };

        // WHEN serialized to JSON
        let json = serde_json::to_value(&result).expect("serialize");

        // THEN available is false and version fields are null
        assert_eq!(json["available"], false);
        assert_eq!(json["current_version"], "1.0.0");
        assert!(json["new_version"].is_null());
        assert!(json["release_notes"].is_null());
    }

    #[test]
    fn test_update_download_progress_serializes_with_total() {
        // GIVEN a progress payload with a known total
        let progress = UpdateDownloadProgress {
            downloaded: 1024,
            total: Some(4096),
        };

        // WHEN serialized to JSON
        let json = serde_json::to_value(&progress).expect("serialize");

        // THEN both fields are present under their snake_case names
        assert_eq!(json["downloaded"], 1024);
        assert_eq!(json["total"], 4096);
    }

    #[test]
    fn test_update_download_progress_serializes_indeterminate() {
        // GIVEN a progress payload without a known total (indeterminate)
        let progress = UpdateDownloadProgress {
            downloaded: 512,
            total: None,
        };

        // WHEN serialized to JSON
        let json = serde_json::to_value(&progress).expect("serialize");

        // THEN total is null so the frontend renders an indeterminate bar
        assert_eq!(json["downloaded"], 512);
        assert!(json["total"].is_null());
    }

    #[tokio::test]
    async fn test_update_error_messages_are_descriptive() {
        // GIVEN each variant of UpdateError
        // WHEN converted to string
        // THEN messages identify the failure kind for the frontend
        assert_eq!(UpdateError::NoUpdate.to_string(), "no update available");
        assert!(UpdateError::Plugin("boom".into())
            .to_string()
            .contains("boom"));
        assert!(UpdateError::Check("net".into())
            .to_string()
            .starts_with("update check failed"));
        assert!(UpdateError::Install("hash".into())
            .to_string()
            .starts_with("install failed"));
    }
}
