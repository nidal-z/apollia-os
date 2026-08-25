//! User-selectable Google Drive root folder per connected account.
//!
//! By default Apollia agents write under `Drive/Apollia/<agent_slug>/`. The
//! end user can override the root path here so agents work inside a folder
//! of their choosing: `Documents/Apollia`, `Work/AI`, anything. The
//! `<agent_slug>` subfolder is appended automatically so different agents
//! don't collide.
//!
//! # Storage
//!
//! Persisted as TOML at `~/.apollia/drive-prefs.toml`. World-unreadable
//! (`0o600` on Unix). Format:
//!
//! ```toml
//! [google."user@example.com"]
//! folder_path = "Documents/Apollia"
//!
//! # Folders the user designated via Google Picker. Each entry grants
//! # Apollia drive.file access to that folder (Google extends the scope on
//! # pick). Agents can list/read/write inside any of them without CASA.
//! [[google."user@example.com".picked_folders]]
//! id = "1aBcDeFgHiJkLmNoPq"
//! name = "Documents"
//! mime_type = "application/vnd.google-apps.folder"
//!
//! [[google."user@example.com".picked_folders]]
//! id = "2xYzAbCdEfGhIjKlMn"
//! name = "Travail/AI"
//! mime_type = "application/vnd.google-apps.folder"
//! ```
//!
//! Missing file, missing section, or empty value all mean "use default"
//! (the legacy `Apollia` root). Calls never fail when the file is absent;
//! callers fall back to the default automatically.
//!
//! # Scope constraint
//!
//! With the free-tier `drive.file` scope Apollia can only see files/folders
//! it has *touched*. So a "selectable folder" actually means: pick a path,
//! Apollia creates the hierarchy on first use. Pointing at a pre-existing
//! user-created folder requires Google Picker (post-v0.1.0) or the
//! restricted `drive.readonly` scope (post-CASA audit).

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// On-disk filename for the per-account Drive folder preferences.
pub const DRIVE_PREFS_FILENAME: &str = "drive-prefs.toml";

/// Default Drive root used when no preference is set. Matches the legacy
/// hardcoded `Apollia` constant so existing installs keep their files.
pub const DEFAULT_ROOT_FOLDER_PATH: &str = "Apollia";

/// A folder the user designated via Google Picker; Apollia gained
/// `drive.file` access to it (plus its descendants) on pick.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PickedFolder {
    /// Google Drive file ID. Stable.
    pub id: String,
    /// Folder name at pick time (display only; may drift if the user
    /// renames the folder in Drive; the access remains valid).
    pub name: String,
    /// MIME type. Always `application/vnd.google-apps.folder` today; kept
    /// as a string so we can later admit picked individual files.
    #[serde(default = "default_folder_mime")]
    pub mime_type: String,
}

fn default_folder_mime() -> String {
    "application/vnd.google-apps.folder".to_string()
}

/// Per-account preference body inside the TOML file.
///
/// **Tri-state `folder_path` semantics** :
/// - field **absent** → callers fall back to [`DEFAULT_ROOT_FOLDER_PATH`]
///   (the legacy `Apollia` subfolder).
/// - field present as **`""`** → user explicitly wants Drive root, no
///   intermediate folder. The connector skips the path-walking step and
///   puts `<agent_slug>` directly under My Drive.
/// - field present as **`"Documents/X"`** → walk each segment, create as
///   needed, place `<agent_slug>` inside.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct AccountDrivePref {
    /// Override path. See struct doc for tri-state semantics.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub folder_path: Option<String>,
    /// Folders the user picked via Google Picker. Persisted so agents
    /// keep working across sessions without re-prompting.
    #[serde(default)]
    pub picked_folders: Vec<PickedFolder>,
}

/// On-disk shape of `~/.apollia/drive-prefs.toml`.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct DrivePrefsFile {
    /// Per-Google-account preferences keyed by account id (typically the
    /// user's email returned by the userinfo endpoint).
    #[serde(default)]
    pub google: HashMap<String, AccountDrivePref>,
}

/// Resolve the path to `~/.apollia/drive-prefs.toml`.
pub fn drive_prefs_path() -> Option<PathBuf> {
    Some(drive_prefs_path_in(dirs::home_dir()?))
}

/// Build the prefs path rooted at `home`. Used by tests.
pub fn drive_prefs_path_in<P: Into<PathBuf>>(home: P) -> PathBuf {
    apollia_core::paths::data_dir_under(home).join(DRIVE_PREFS_FILENAME)
}

/// Read + parse the prefs file from `path`. `Ok(None)` when the file does
/// not exist (the common case on a fresh install). `Err(_)` only when the
/// file exists but cannot be parsed; surfaces to the operator so a
/// malformed file is not silently ignored.
pub fn load_from(path: &Path) -> Result<Option<DrivePrefsFile>, std::io::Error> {
    let contents = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e),
    };
    let parsed: DrivePrefsFile = toml::from_str(&contents).map_err(|e| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("drive-prefs.toml is malformed: {e}"),
        )
    })?;
    Ok(Some(parsed))
}

/// Look up the folder path override for `(provider, account_id)` using the
/// default path. Returns `Some(value)` when a section is persisted (even
/// when the value is empty; explicit empty means "use My Drive root").
/// Returns `None` only when the user never saved anything → caller falls
/// back to `DEFAULT_ROOT_FOLDER_PATH`.
pub fn lookup_folder_path(provider_id: &str, account_id: &str) -> Option<String> {
    let path = drive_prefs_path()?;
    lookup_folder_path_at(&path, provider_id, account_id)
}

/// Look up the override at an explicit `path`. Used by tests.
///
/// **Tri-state semantics** :
/// - `None` → no per-account entry exists. Caller should use the default
///   `DEFAULT_ROOT_FOLDER_PATH` (legacy `Apollia` subfolder).
/// - `Some("")` → user explicitly cleared the path. Means "use My Drive
///   root directly, no Apollia subfolder, agent writes go straight under
///   root".
/// - `Some("Documents/Apollia")` → user-specified path. Connector walks
///   each segment creating folders as needed.
pub fn lookup_folder_path_at(path: &Path, provider_id: &str, account_id: &str) -> Option<String> {
    let file = load_from(path).ok().flatten()?;
    let map = match provider_id {
        "google" => &file.google,
        _ => return None,
    };
    let entry = map.get(account_id)?;
    entry.folder_path.as_ref().map(|p| p.trim().to_string())
}

/// Resolve the effective folder path the connector should walk:
/// - `None` override → falls back to [`DEFAULT_ROOT_FOLDER_PATH`].
/// - `Some(value)` override → returned verbatim (including empty string
///   which signals "use My Drive root").
///
/// Always returns a `String`. The connector's `ensure_agent_folder`
/// interprets empty as "skip the intermediate folder, place `agent_slug`
/// directly under My Drive root".
pub fn effective_folder_path(provider_id: &str, account_id: &str) -> String {
    lookup_folder_path(provider_id, account_id)
        .unwrap_or_else(|| DEFAULT_ROOT_FOLDER_PATH.to_string())
}

/// Wipe a per-account folder_path entry entirely, restoring the default
/// "use Apollia subfolder" behaviour at the next agent call. Different
/// from saving an empty string (which means "use root").
pub fn reset_folder_path(provider_id: &str, account_id: &str) -> Result<(), std::io::Error> {
    let path = drive_prefs_path().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "cannot determine $HOME directory",
        )
    })?;
    reset_folder_path_at(&path, provider_id, account_id)
}

/// Wipe at an explicit `path`. Removes only `folder_path`; picked
/// folders stay intact so resetting the root doesn't lose the user's
/// Picker history.
pub fn reset_folder_path_at(
    path: &Path,
    provider_id: &str,
    account_id: &str,
) -> Result<(), std::io::Error> {
    let mut existing = match load_from(path)? {
        Some(f) => f,
        None => return Ok(()),
    };
    let map = match provider_id {
        "google" => &mut existing.google,
        other => {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!("unknown drive prefs provider: {other}"),
            ));
        }
    };
    if let Some(entry) = map.get_mut(account_id) {
        // `None` is serialised away by `skip_serializing_if`; if the
        // account had no picked folders either, drop the whole section
        // so the file stays clean.
        entry.folder_path = None;
        if entry.picked_folders.is_empty() {
            map.remove(account_id);
        }
    }

    let serialized = toml::to_string_pretty(&existing).map_err(|e| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("failed to serialize drive-prefs.toml: {e}"),
        )
    })?;
    std::fs::write(path, serialized)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(path)?.permissions();
        perms.set_mode(0o600);
        std::fs::set_permissions(path, perms)?;
    }
    Ok(())
}

/// Write or update the folder path for `(provider, account_id)`.
///
/// Passing an empty string clears the override (next read falls back to
/// the default). Creates the parent directory + file on demand with
/// 0o600 permissions on Unix.
pub fn set_folder_path(
    provider_id: &str,
    account_id: &str,
    folder_path: &str,
) -> Result<(), std::io::Error> {
    let path = drive_prefs_path().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "cannot determine $HOME directory",
        )
    })?;
    set_folder_path_at(&path, provider_id, account_id, folder_path)
}

// ─── Picked folders accessors ───────────────────────────────────────────────

/// List folders the user picked via Picker for `account_id`. Empty when
/// the user hasn't picked anything yet (legacy `folder_path`-only flow).
pub fn list_picked_folders(provider_id: &str, account_id: &str) -> Vec<PickedFolder> {
    let Some(path) = drive_prefs_path() else {
        return Vec::new();
    };
    list_picked_folders_at(&path, provider_id, account_id)
}

/// List picked folders at an explicit `path`. Used by tests.
pub fn list_picked_folders_at(
    path: &Path,
    provider_id: &str,
    account_id: &str,
) -> Vec<PickedFolder> {
    let Some(file) = load_from(path).ok().flatten() else {
        return Vec::new();
    };
    let map = match provider_id {
        "google" => &file.google,
        _ => return Vec::new(),
    };
    map.get(account_id)
        .map(|p| p.picked_folders.clone())
        .unwrap_or_default()
}

/// Append a picked folder to the user's list, deduping by `id`. If the
/// folder is already present, its `name` is refreshed.
pub fn add_picked_folder(
    provider_id: &str,
    account_id: &str,
    folder: PickedFolder,
) -> Result<(), std::io::Error> {
    let path = drive_prefs_path().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "cannot determine $HOME directory",
        )
    })?;
    add_picked_folder_at(&path, provider_id, account_id, folder)
}

/// Append at an explicit `path`.
pub fn add_picked_folder_at(
    path: &Path,
    provider_id: &str,
    account_id: &str,
    folder: PickedFolder,
) -> Result<(), std::io::Error> {
    update_account(path, provider_id, account_id, |entry| {
        if let Some(existing) = entry.picked_folders.iter_mut().find(|f| f.id == folder.id) {
            existing.name = folder.name;
            existing.mime_type = folder.mime_type;
        } else {
            entry.picked_folders.push(folder);
        }
    })
}

/// Remove a picked folder by `folder_id`. No-op when the id isn't found.
pub fn remove_picked_folder(
    provider_id: &str,
    account_id: &str,
    folder_id: &str,
) -> Result<(), std::io::Error> {
    let path = drive_prefs_path().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "cannot determine $HOME directory",
        )
    })?;
    remove_picked_folder_at(&path, provider_id, account_id, folder_id)
}

/// Remove at an explicit `path`.
pub fn remove_picked_folder_at(
    path: &Path,
    provider_id: &str,
    account_id: &str,
    folder_id: &str,
) -> Result<(), std::io::Error> {
    update_account(path, provider_id, account_id, |entry| {
        entry.picked_folders.retain(|f| f.id != folder_id);
    })
}

/// Shared mutator for any per-account update on the prefs file. Loads,
/// applies `mutate`, writes back with 0o600 perms.
fn update_account(
    path: &Path,
    provider_id: &str,
    account_id: &str,
    mutate: impl FnOnce(&mut AccountDrivePref),
) -> Result<(), std::io::Error> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut existing = load_from(path)?.unwrap_or_default();
    let map = match provider_id {
        "google" => &mut existing.google,
        other => {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!("unknown drive prefs provider: {other}"),
            ));
        }
    };
    let entry = map.entry(account_id.to_string()).or_default();
    mutate(entry);

    let serialized = toml::to_string_pretty(&existing).map_err(|e| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("failed to serialize drive-prefs.toml: {e}"),
        )
    })?;
    std::fs::write(path, serialized)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(path)?.permissions();
        perms.set_mode(0o600);
        std::fs::set_permissions(path, perms)?;
    }
    Ok(())
}

/// Write or update at an explicit `path`. Same semantics as
/// [`set_folder_path`]; used by tests and callers that need to override
/// the home directory.
pub fn set_folder_path_at(
    path: &Path,
    provider_id: &str,
    account_id: &str,
    folder_path: &str,
) -> Result<(), std::io::Error> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut existing = load_from(path)?.unwrap_or_default();
    let map = match provider_id {
        "google" => &mut existing.google,
        other => {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!("unknown drive prefs provider: {other}"),
            ));
        }
    };
    let entry = map.entry(account_id.to_string()).or_default();
    // Empty string is an *explicit* user choice (= "use My Drive root").
    // Sanitisation preserves the empty value; non-empty is normalised.
    entry.folder_path = Some(sanitize_folder_path(folder_path));

    let serialized = toml::to_string_pretty(&existing).map_err(|e| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("failed to serialize drive-prefs.toml: {e}"),
        )
    })?;
    std::fs::write(path, serialized)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(path)?.permissions();
        perms.set_mode(0o600);
        std::fs::set_permissions(path, perms)?;
    }
    Ok(())
}

/// Normalise a user-typed folder path. Strips leading/trailing slashes
/// and whitespace, collapses repeated slashes. Returns an empty string
/// when the input contains no usable segment; the loader then treats
/// the entry as "no override" so we fall back to the default cleanly.
pub fn sanitize_folder_path(raw: &str) -> String {
    raw.split('/')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("/")
}

/// Split a folder path into its segments. Empty input yields an empty
/// vector; callers should handle that by falling back to the default.
pub fn segments(path: &str) -> Vec<String> {
    sanitize_folder_path(path)
        .split('/')
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = drive_prefs_path_in(dir.path());
        (dir, path)
    }

    #[test]
    fn sanitise_strips_leading_trailing_slashes_and_whitespace() {
        assert_eq!(
            sanitize_folder_path("  /Documents/  Apollia/ "),
            "Documents/Apollia"
        );
        assert_eq!(sanitize_folder_path("//"), "");
        assert_eq!(sanitize_folder_path(""), "");
        assert_eq!(sanitize_folder_path("Apollia"), "Apollia");
    }

    #[test]
    fn segments_split_correctly() {
        assert_eq!(segments("Documents/Apollia"), vec!["Documents", "Apollia"]);
        assert_eq!(segments(""), Vec::<String>::new());
        assert_eq!(segments("Apollia"), vec!["Apollia"]);
    }

    #[test]
    fn missing_file_falls_back_to_default() {
        let (_dir, path) = tmp();
        assert_eq!(lookup_folder_path_at(&path, "google", "nidal@x"), None);
        // effective_* uses the default lookup but in this test the home is
        // not overridden; call only the explicit-path lookup.
        let p = lookup_folder_path_at(&path, "google", "nidal@x")
            .unwrap_or_else(|| DEFAULT_ROOT_FOLDER_PATH.to_string());
        assert_eq!(p, "Apollia");
    }

    #[test]
    fn round_trips_per_account() {
        let (_dir, path) = tmp();
        set_folder_path_at(&path, "google", "nidal@x", "Documents/Apollia").unwrap();
        set_folder_path_at(&path, "google", "other@x", "Travail/AI").unwrap();
        assert_eq!(
            lookup_folder_path_at(&path, "google", "nidal@x").as_deref(),
            Some("Documents/Apollia")
        );
        assert_eq!(
            lookup_folder_path_at(&path, "google", "other@x").as_deref(),
            Some("Travail/AI")
        );
    }

    #[test]
    fn empty_value_means_explicit_root() {
        // GIVEN an override saved as "Documents"
        // WHEN the user replaces it with the empty string
        // THEN the lookup returns Some(""), explicit "use Drive root",
        // NOT None (which would mean "fall back to default").
        let (_dir, path) = tmp();
        set_folder_path_at(&path, "google", "nidal@x", "Documents").unwrap();
        set_folder_path_at(&path, "google", "nidal@x", "").unwrap();
        assert_eq!(
            lookup_folder_path_at(&path, "google", "nidal@x").as_deref(),
            Some("")
        );
    }

    #[test]
    fn reset_wipes_override_and_falls_back_to_default() {
        // GIVEN a custom override AND a picked folder
        // WHEN the override is reset
        // THEN lookup returns None (= use Apollia default) and picked
        // folders survive.
        let (_dir, path) = tmp();
        set_folder_path_at(&path, "google", "nidal@x", "Documents").unwrap();
        let f = PickedFolder {
            id: "id-1".into(),
            name: "Docs".into(),
            mime_type: default_folder_mime(),
        };
        add_picked_folder_at(&path, "google", "nidal@x", f.clone()).unwrap();

        reset_folder_path_at(&path, "google", "nidal@x").unwrap();

        assert_eq!(lookup_folder_path_at(&path, "google", "nidal@x"), None);
        assert_eq!(list_picked_folders_at(&path, "google", "nidal@x"), vec![f]);
    }

    #[test]
    fn reset_drops_section_when_no_picked_folders_remain() {
        let (_dir, path) = tmp();
        set_folder_path_at(&path, "google", "nidal@x", "Documents").unwrap();
        reset_folder_path_at(&path, "google", "nidal@x").unwrap();
        assert_eq!(lookup_folder_path_at(&path, "google", "nidal@x"), None);
        assert!(list_picked_folders_at(&path, "google", "nidal@x").is_empty());
    }

    #[test]
    fn malformed_file_surfaces_error() {
        let (_dir, path) = tmp();
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "not = valid = toml = here\n").unwrap();
        let err = load_from(&path).expect_err("malformed TOML must surface");
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    }

    #[test]
    fn unknown_provider_rejected() {
        let (_dir, path) = tmp();
        let err = set_folder_path_at(&path, "linear", "x", "y").unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput);
    }

    #[test]
    fn picked_folders_round_trip_per_account() {
        let (_dir, path) = tmp();
        let f1 = PickedFolder {
            id: "id-1".into(),
            name: "Docs".into(),
            mime_type: default_folder_mime(),
        };
        let f2 = PickedFolder {
            id: "id-2".into(),
            name: "Travail".into(),
            mime_type: default_folder_mime(),
        };
        add_picked_folder_at(&path, "google", "nidal@x", f1.clone()).unwrap();
        add_picked_folder_at(&path, "google", "nidal@x", f2.clone()).unwrap();

        let listed = list_picked_folders_at(&path, "google", "nidal@x");
        assert_eq!(listed.len(), 2);
        assert!(listed.contains(&f1));
        assert!(listed.contains(&f2));
    }

    #[test]
    fn picked_folders_dedupe_by_id() {
        let (_dir, path) = tmp();
        let f = PickedFolder {
            id: "id-1".into(),
            name: "Old".into(),
            mime_type: default_folder_mime(),
        };
        add_picked_folder_at(&path, "google", "nidal@x", f).unwrap();
        let renamed = PickedFolder {
            id: "id-1".into(),
            name: "New".into(),
            mime_type: default_folder_mime(),
        };
        add_picked_folder_at(&path, "google", "nidal@x", renamed).unwrap();

        let listed = list_picked_folders_at(&path, "google", "nidal@x");
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].name, "New");
    }

    #[test]
    fn picked_folder_remove_is_noop_when_absent() {
        let (_dir, path) = tmp();
        assert!(remove_picked_folder_at(&path, "google", "nidal@x", "unknown").is_ok());
    }

    #[test]
    fn picked_folder_remove_keeps_others() {
        let (_dir, path) = tmp();
        let f1 = PickedFolder {
            id: "a".into(),
            name: "A".into(),
            mime_type: default_folder_mime(),
        };
        let f2 = PickedFolder {
            id: "b".into(),
            name: "B".into(),
            mime_type: default_folder_mime(),
        };
        add_picked_folder_at(&path, "google", "nidal@x", f1).unwrap();
        add_picked_folder_at(&path, "google", "nidal@x", f2.clone()).unwrap();
        remove_picked_folder_at(&path, "google", "nidal@x", "a").unwrap();

        let listed = list_picked_folders_at(&path, "google", "nidal@x");
        assert_eq!(listed, vec![f2]);
    }

    #[test]
    fn picked_folders_coexist_with_folder_path() {
        let (_dir, path) = tmp();
        set_folder_path_at(&path, "google", "nidal@x", "Documents/Apollia").unwrap();
        let f = PickedFolder {
            id: "id-1".into(),
            name: "Travail".into(),
            mime_type: default_folder_mime(),
        };
        add_picked_folder_at(&path, "google", "nidal@x", f.clone()).unwrap();

        assert_eq!(
            lookup_folder_path_at(&path, "google", "nidal@x").as_deref(),
            Some("Documents/Apollia")
        );
        assert_eq!(list_picked_folders_at(&path, "google", "nidal@x"), vec![f]);
    }

    #[test]
    fn input_paths_are_sanitised_on_write() {
        let (_dir, path) = tmp();
        set_folder_path_at(&path, "google", "nidal@x", "  /Documents//Apollia/ ").unwrap();
        assert_eq!(
            lookup_folder_path_at(&path, "google", "nidal@x").as_deref(),
            Some("Documents/Apollia")
        );
    }
}
