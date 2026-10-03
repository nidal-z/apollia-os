//! Tauri IPC commands for the named settings of `apollia.toml`.
//!
//! - [`get_resolved_setting`]: the value of a named setting and where it comes
//!   from (default, the user's file, or the organization policy)
//! - [`set_setting`]: writes a named setting, keeping the comments and layout
//!   of the file, and refuses a key the policy imposes
//!
//! Neither command reads the runtime: both read and write the `apollia.toml`
//! of the data directory, which the daemon shares when the application is
//! attached to one, so they serve both modes.

use std::path::Path;

use apollia_core::config::{resolve_setting, write_setting, PolicySettings, ResolvedSetting};

use super::default_config_path;

/// Resolves `key` against the `apollia.toml` at `path`.
pub(crate) fn resolve_at(path: &Path, key: &str) -> Result<ResolvedSetting, String> {
    resolve_setting(path, key, &PolicySettings::none()).map_err(|e| e.to_string())
}

/// Writes `key` to the `apollia.toml` at `path` and returns it resolved.
pub(crate) fn write_at(
    path: &Path,
    key: &str,
    value: serde_json::Value,
) -> Result<ResolvedSetting, String> {
    write_setting(path, key, value, &PolicySettings::none()).map_err(|e| e.to_string())
}

/// Resolves `key` against the `apollia.toml` of the data directory, for a
/// Rust reader of a desktop setting.
pub(crate) fn resolve_in_force(key: &str) -> Result<ResolvedSetting, String> {
    resolve_at(&default_config_path(), key)
}

/// The value in force of the named setting `key` (dotted, for example
/// `desktop.developer_mode`), with its source.
#[tauri::command]
pub async fn get_resolved_setting(key: String) -> Result<ResolvedSetting, String> {
    resolve_at(&default_config_path(), &key)
}

/// Writes the named setting `key` to `value` and returns it resolved.
#[tauri::command]
pub async fn set_setting(key: String, value: serde_json::Value) -> Result<ResolvedSetting, String> {
    write_at(&default_config_path(), &key, value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use apollia_core::config::SettingSource;

    #[test]
    fn test_an_absent_file_resolves_developer_mode_to_false_by_default() {
        // GIVEN a data directory with no apollia.toml
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("apollia.toml");

        // WHEN the developer mode is resolved
        let resolved = resolve_at(&path, "desktop.developer_mode").unwrap();

        // THEN it is off, from the default
        assert_eq!(resolved.value, serde_json::Value::Bool(false));
        assert_eq!(resolved.source, SettingSource::Default);
    }

    #[test]
    fn test_turning_developer_mode_on_writes_the_key_and_reads_back() {
        // GIVEN a file that carries a comment and no [desktop] section
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("apollia.toml");
        std::fs::write(&path, "# mine\n[runtime]\n").unwrap();

        // WHEN the developer mode is turned on
        write_at(
            &path,
            "desktop.developer_mode",
            serde_json::Value::Bool(true),
        )
        .unwrap();

        // THEN the file holds the key next to the comment, and it reads back on
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("# mine"), "{content}");
        assert!(content.contains("developer_mode = true"), "{content}");
        let resolved = resolve_at(&path, "desktop.developer_mode").unwrap();
        assert_eq!(resolved.value, serde_json::Value::Bool(true));
        assert_eq!(resolved.source, SettingSource::User);
    }

    #[test]
    fn test_an_unknown_key_is_refused() {
        // GIVEN an empty data directory
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("apollia.toml");

        // WHEN an unknown key is written
        let refused = write_at(&path, "desktop.unknown", serde_json::Value::Bool(true));

        // THEN the write is refused and no file is created
        assert!(refused.is_err());
        assert!(!path.exists());
    }
}
