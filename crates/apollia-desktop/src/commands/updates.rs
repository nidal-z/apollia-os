//! Commandes IPC Tauri pour la gestion des mises à jour automatiques.
//!
//! Vérifie les nouvelles versions sur GitHub Releases et déclenche
//! la mise à jour de l'application Tauri via `tauri-plugin-updater`.
//!
//! Note : `tauri-plugin-updater` n'est pas encore dans les dépendances de cette
//! crate. Ces commandes retournent gracieusement des stubs jusqu'à ce que le
//! plugin soit activé.

use serde::Serialize;

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
/// Utilise `tauri-plugin-updater` pour interroger le endpoint de mise à jour
/// configuré dans `tauri.conf.json`. Retourne une erreur descriptive si le
/// plugin n'est pas activé dans cette build.
#[tauri::command]
pub async fn check_for_update(
    app: tauri::AppHandle,
) -> Result<UpdateCheckResult, String> {
    let current_version = app.package_info().version.to_string();
    // tauri-plugin-updater is not yet in the dependency tree for this crate.
    // Return a graceful stub so the frontend can handle the feature absence.
    Err(format!(
        "update check not available in this build (version {current_version})"
    ))
}

/// Installe la mise à jour disponible et redémarre l'application.
///
/// Cette commande doit être appelée après `check_for_update` a confirmé
/// qu'une mise à jour est disponible. Retourne une erreur si le plugin
/// `tauri-plugin-updater` n'est pas activé dans cette build.
#[tauri::command]
pub async fn install_update(
    _app: tauri::AppHandle,
) -> Result<(), String> {
    // tauri-plugin-updater is not yet in the dependency tree for this crate.
    Err("install_update not available in this build".to_string())
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
}
