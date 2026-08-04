//! User-editable OAuth client overrides stored in `~/.apollia/oauth-clients.toml`.
//!
//! This module is the only layer an operator can write to without rebuilding,
//! and it serves two different needs depending on the provider.
//!
//! For **Google** it is mandatory: no Apollia build embeds a Google client, so
//! Gmail, Calendar and Drive stay unreachable until this file (or the env var)
//! carries one. For **Microsoft** it is optional: builds embed Apollia's own
//! public client registration, and this file exists so an operator can point
//! the connector at their own Azure app registration instead.
//!
//! It sits in the
//! [`ConnectorProvider::resolve_client_id`](crate::ConnectorProvider::resolve_client_id)
//! resolution chain, between the runtime env var and the compiled default:
//!
//! 1. `APOLLIA_<PROVIDER>_CLIENT_ID` env var
//! 2. `~/.apollia/oauth-clients.toml`  ← this module
//! 3. `default_client_id()` baked into the binary: Apollia's registration for
//!    Microsoft, empty for Google
//!
//! # File format
//!
//! ```toml
//! [google]
//! client_id = "1234.apps.googleusercontent.com"
//! client_secret = "GOCSPX-…"   # required for Google Installed App type
//!
//! [microsoft]
//! client_id = "00000000-0000-0000-0000-000000000000"
//! # microsoft has no client_secret (public client per spec)
//! ```
//!
//! Missing keys, missing sections, an empty file, and a missing file are all
//! treated as "no override" (never an error), so the resolution chain falls
//! through to the next step cleanly.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Filename used for the on-disk OAuth client overrides.
pub const OAUTH_CLIENTS_FILENAME: &str = "oauth-clients.toml";

/// Per-provider entry inside the TOML file.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct OAuthClientEntry {
    /// Override OAuth client ID for this provider. Empty/absent means "no override".
    #[serde(default)]
    pub client_id: String,
    /// Override OAuth client secret for this provider. Empty/absent means
    /// "no override". Required for Google's Installed App type; Microsoft
    /// public clients leave this empty per spec.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub client_secret: String,
    /// Public Google API key used by Google Picker (separate from the OAuth
    /// client_id; Google requires both). Restricted to the Picker + Drive
    /// APIs in the Google Cloud Console. Empty means "no override".
    /// Microsoft entries leave this empty (Picker is Google-only).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub api_key: String,
}

/// On-disk shape of `~/.apollia/oauth-clients.toml`.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct OAuthClientsFile {
    /// Override block for Google Workspace.
    #[serde(default)]
    pub google: OAuthClientEntry,
    /// Override block for Microsoft 365.
    #[serde(default)]
    pub microsoft: OAuthClientEntry,
}

/// Resolve the path to `~/.apollia/oauth-clients.toml`.
///
/// Returns `None` when the home directory cannot be determined (e.g. minimal
/// container without `$HOME`). Callers treat this as "no override file".
pub fn oauth_clients_path() -> Option<PathBuf> {
    Some(oauth_clients_path_in(dirs::home_dir()?))
}

/// Build the overrides path rooted at `home`. Used by tests to bypass `$HOME`.
pub fn oauth_clients_path_in<P: Into<PathBuf>>(home: P) -> PathBuf {
    home.into().join(".apollia").join(OAUTH_CLIENTS_FILENAME)
}

/// Read and parse the overrides file from the default path.
pub fn load() -> Result<Option<OAuthClientsFile>, std::io::Error> {
    let Some(path) = oauth_clients_path() else {
        return Ok(None);
    };
    load_from(&path)
}

/// Read and parse the overrides file from `path`.
///
/// Returns `Ok(None)` when the file does not exist (the common case on a
/// fresh install) and `Err(_)` only when the file exists but cannot be parsed.
/// That should surface to the user so a malformed file is not silently
/// ignored.
pub fn load_from(path: &std::path::Path) -> Result<Option<OAuthClientsFile>, std::io::Error> {
    let contents = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e),
    };
    let parsed: OAuthClientsFile = toml::from_str(&contents).map_err(|e| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("oauth-clients.toml is malformed: {e}"),
        )
    })?;
    Ok(Some(parsed))
}

/// Look up the client_id override for a provider id, using the default path.
pub fn lookup_client_id(provider_id: &str) -> Option<String> {
    let path = oauth_clients_path()?;
    lookup_client_id_at(&path, provider_id)
}

/// Look up the client_id override at an explicit `path`. Used by tests and
/// callers that need to override the home directory.
pub fn lookup_client_id_at(path: &std::path::Path, provider_id: &str) -> Option<String> {
    let file = load_from(path).ok().flatten()?;
    let value = match provider_id {
        "google" => &file.google.client_id,
        "microsoft" => &file.microsoft.client_id,
        _ => return None,
    };
    if value.is_empty() {
        None
    } else {
        Some(value.clone())
    }
}

/// Look up the client_secret override for a provider id, using the default path.
pub fn lookup_client_secret(provider_id: &str) -> Option<String> {
    let path = oauth_clients_path()?;
    lookup_client_secret_at(&path, provider_id)
}

/// Look up the Google API key override for `provider_id`, using the
/// default path. Only meaningful for Google (Picker).
pub fn lookup_api_key(provider_id: &str) -> Option<String> {
    let path = oauth_clients_path()?;
    lookup_api_key_at(&path, provider_id)
}

/// Look up the API key at an explicit `path`.
pub fn lookup_api_key_at(path: &std::path::Path, provider_id: &str) -> Option<String> {
    let file = load_from(path).ok().flatten()?;
    let value = match provider_id {
        "google" => &file.google.api_key,
        "microsoft" => &file.microsoft.api_key,
        _ => return None,
    };
    if value.is_empty() {
        None
    } else {
        Some(value.clone())
    }
}

/// Look up the client_secret override at an explicit `path`.
pub fn lookup_client_secret_at(path: &std::path::Path, provider_id: &str) -> Option<String> {
    let file = load_from(path).ok().flatten()?;
    let value = match provider_id {
        "google" => &file.google.client_secret,
        "microsoft" => &file.microsoft.client_secret,
        _ => return None,
    };
    if value.is_empty() {
        None
    } else {
        Some(value.clone())
    }
}

/// Write or update the client_id override for a provider id at the default path.
pub fn set_client_id(provider_id: &str, client_id: &str) -> Result<(), std::io::Error> {
    let path = oauth_clients_path().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "cannot determine $HOME directory",
        )
    })?;
    set_client_id_at(&path, provider_id, client_id)
}

/// Write or update the client_id override at an explicit `path`.
///
/// Creates the parent directory if missing, preserves the other provider's
/// entry, and applies 0o600 permissions on Unix.
pub fn set_client_id_at(
    path: &std::path::Path,
    provider_id: &str,
    client_id: &str,
) -> Result<(), std::io::Error> {
    update_entry_at(path, provider_id, |entry| {
        entry.client_id = client_id.to_string();
    })
}

/// Write or update the client_secret override for a provider id, default path.
pub fn set_client_secret(provider_id: &str, client_secret: &str) -> Result<(), std::io::Error> {
    let path = oauth_clients_path().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "cannot determine $HOME directory",
        )
    })?;
    set_client_secret_at(&path, provider_id, client_secret)
}

/// Write or update the client_secret override at an explicit `path`.
pub fn set_client_secret_at(
    path: &std::path::Path,
    provider_id: &str,
    client_secret: &str,
) -> Result<(), std::io::Error> {
    update_entry_at(path, provider_id, |entry| {
        entry.client_secret = client_secret.to_string();
    })
}

/// Write or update the Google API key for `provider_id`, default path.
pub fn set_api_key(provider_id: &str, api_key: &str) -> Result<(), std::io::Error> {
    let path = oauth_clients_path().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "cannot determine $HOME directory",
        )
    })?;
    set_api_key_at(&path, provider_id, api_key)
}

/// Write or update the API key at an explicit `path`.
pub fn set_api_key_at(
    path: &std::path::Path,
    provider_id: &str,
    api_key: &str,
) -> Result<(), std::io::Error> {
    update_entry_at(path, provider_id, |entry| {
        entry.api_key = api_key.to_string();
    })
}

/// Shared writer used by `set_client_id_at` and `set_client_secret_at`.
/// Mutates the per-provider entry in place, preserves the other provider's
/// data, and applies 0o600 perms on Unix.
fn update_entry_at(
    path: &std::path::Path,
    provider_id: &str,
    mutate: impl FnOnce(&mut OAuthClientEntry),
) -> Result<(), std::io::Error> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let mut existing = load_from(path)?.unwrap_or_default();
    let entry = match provider_id {
        "google" => &mut existing.google,
        "microsoft" => &mut existing.microsoft,
        other => {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!("unknown OAuth connector provider: {other}"),
            ));
        }
    };
    mutate(entry);

    let serialized = toml::to_string_pretty(&existing).map_err(|e| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("failed to serialize oauth-clients.toml: {e}"),
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

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_path() -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = oauth_clients_path_in(dir.path());
        (dir, path)
    }

    #[test]
    fn missing_file_returns_none() {
        // GIVEN no oauth-clients.toml on disk
        let (_dir, path) = tmp_path();
        // WHEN we load
        let loaded = load_from(&path).unwrap();
        // THEN we get None, not an error
        assert!(loaded.is_none());
        assert_eq!(lookup_client_id_at(&path, "google"), None);
    }

    #[test]
    fn round_trip_persists_per_provider() {
        let (_dir, path) = tmp_path();
        set_client_id_at(&path, "google", "google-id-123").unwrap();
        set_client_id_at(&path, "microsoft", "ms-id-456").unwrap();

        assert_eq!(
            lookup_client_id_at(&path, "google").as_deref(),
            Some("google-id-123")
        );
        assert_eq!(
            lookup_client_id_at(&path, "microsoft").as_deref(),
            Some("ms-id-456")
        );
    }

    #[test]
    fn updating_one_provider_preserves_the_other() {
        let (_dir, path) = tmp_path();
        set_client_id_at(&path, "google", "google-original").unwrap();
        set_client_id_at(&path, "microsoft", "ms-original").unwrap();
        set_client_id_at(&path, "google", "google-updated").unwrap();

        assert_eq!(
            lookup_client_id_at(&path, "google").as_deref(),
            Some("google-updated")
        );
        assert_eq!(
            lookup_client_id_at(&path, "microsoft").as_deref(),
            Some("ms-original")
        );
    }

    #[test]
    fn empty_client_id_is_treated_as_no_override() {
        let (_dir, path) = tmp_path();
        set_client_id_at(&path, "google", "").unwrap();
        assert_eq!(lookup_client_id_at(&path, "google"), None);
    }

    #[test]
    fn unknown_provider_rejected_on_write() {
        let (_dir, path) = tmp_path();
        let err = set_client_id_at(&path, "github", "x").unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput);
    }

    #[test]
    fn client_secret_round_trips_per_provider() {
        let (_dir, path) = tmp_path();
        set_client_id_at(&path, "google", "google-id-123").unwrap();
        set_client_secret_at(&path, "google", "GOCSPX-secret").unwrap();

        // Both fields readable, independent
        assert_eq!(
            lookup_client_id_at(&path, "google").as_deref(),
            Some("google-id-123")
        );
        assert_eq!(
            lookup_client_secret_at(&path, "google").as_deref(),
            Some("GOCSPX-secret")
        );

        // Updating client_id alone preserves the secret
        set_client_id_at(&path, "google", "google-id-456").unwrap();
        assert_eq!(
            lookup_client_secret_at(&path, "google").as_deref(),
            Some("GOCSPX-secret")
        );
    }

    #[test]
    fn empty_client_secret_is_treated_as_no_override() {
        let (_dir, path) = tmp_path();
        set_client_secret_at(&path, "google", "").unwrap();
        assert_eq!(lookup_client_secret_at(&path, "google"), None);
    }

    #[test]
    fn malformed_file_surfaces_error() {
        let (_dir, path) = tmp_path();
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "this is not = valid = toml = at all\n").unwrap();
        let err = load_from(&path).expect_err("malformed TOML must surface an error");
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    }
}
