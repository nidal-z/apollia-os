//! Tauri IPC commands for managing automatic updates.
//!
//! Checks for new versions on GitHub Releases and triggers the Tauri
//! application update via `tauri-plugin-updater`.

use serde::Serialize;
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

    update
        .download_and_install(|_chunk, _total| {}, || {})
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
