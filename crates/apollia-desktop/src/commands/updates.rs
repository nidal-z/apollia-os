//! Commandes IPC Tauri pour la gestion des mises à jour automatiques.
//!
//! Vérifie les nouvelles versions sur GitHub Releases et déclenche
//! la mise à jour de l'application Tauri via `tauri-plugin-updater`.

use serde::Serialize;
use thiserror::Error;
use tauri_plugin_updater::UpdaterExt;

/// Erreurs possibles lors de l'interrogation du plugin updater.
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

/// Résultat de la vérification de mise à jour.
#[derive(Debug, Serialize)]
pub struct UpdateCheckResult {
    /// Indique si une mise à jour est disponible.
    pub available: bool,
    /// Version actuelle de l'application.
    pub current_version: String,
    /// Version disponible (si `available` est `true`).
    pub new_version: Option<String>,
    /// Notes de version (si `available` est `true`).
    pub release_notes: Option<String>,
}

/// Vérifie si une mise à jour est disponible sur GitHub Releases.
///
/// Utilise `tauri-plugin-updater` pour interroger l'endpoint configuré dans
/// `tauri.conf.json` (section `plugins.updater.endpoints`). Renvoie une
/// description sérialisable décrivant l'état courant.
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

/// Télécharge et installe la mise à jour disponible, puis redémarre.
///
/// Doit être appelé après `check_for_update`. Retourne `NoUpdate` si plus
/// rien n'est disponible (autre instance en concurrence, ou release retirée).
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
        assert_eq!(
            UpdateError::NoUpdate.to_string(),
            "no update available"
        );
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
