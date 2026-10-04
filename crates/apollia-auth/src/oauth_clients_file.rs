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
//!
//! [microsoft]
//! client_id = "00000000-0000-0000-0000-000000000000"
//! ```
//!
//! The file holds no secret. The client secret and the Google API key live in
//! the secret store ([`crate::oauth_client_secrets`]); the `client_secret` and
//! `api_key` keys an earlier version wrote here are read only to be moved
//! there, then removed from the file.
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
    /// Client secret an earlier version wrote here in clear. Read only by
    /// [`crate::oauth_client_secrets::migrate_file_secrets`], which moves it to
    /// the secret store and clears it; nothing writes it any more.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub client_secret: String,
    /// Google API key an earlier version wrote here in clear; same fate as
    /// `client_secret`.
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
    Some(oauth_clients_path_in(apollia_core::paths::home_dir()?))
}

/// Build the overrides path rooted at `home`. Used by tests to bypass `$HOME`.
pub fn oauth_clients_path_in<P: Into<PathBuf>>(home: P) -> PathBuf {
    apollia_core::paths::data_dir_under(home).join(OAUTH_CLIENTS_FILENAME)
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

/// Rewrites the file at `path` without the `client_secret` and `api_key` keys
/// an earlier version wrote in clear, keeping the client ids. Used once those
/// values are in the secret store.
pub(crate) fn strip_secrets_at(path: &std::path::Path) -> Result<(), std::io::Error> {
    let mut existing = load_from(path)?.unwrap_or_default();
    for entry in [&mut existing.google, &mut existing.microsoft] {
        entry.client_secret.clear();
        entry.api_key.clear();
    }
    write_private(path, &existing)
}

/// Shared writer used by `set_client_id_at`. Mutates the per-provider entry in
/// place and preserves the other provider's data.
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
    write_private(path, &existing)
}

/// Writes `file` to `path` through a sibling temporary file created `0o600`
/// on Unix and renamed over the target, so the file is never readable by
/// others, not even between its creation and a later `chmod`.
fn write_private(path: &std::path::Path, file: &OAuthClientsFile) -> Result<(), std::io::Error> {
    use std::io::Write;

    let serialized = toml::to_string_pretty(file).map_err(|e| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("failed to serialize oauth-clients.toml: {e}"),
        )
    })?;
    let tmp = path.with_extension("toml.tmp");
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut out = options.open(&tmp)?;
    out.write_all(serialized.as_bytes())?;
    out.sync_all()?;
    drop(out);
    std::fs::rename(&tmp, path)
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
        // GIVEN an empty client file
        let (_dir, path) = tmp_path();
        // WHEN a client id is stored for each of the two providers
        set_client_id_at(&path, "google", "google-id-123").unwrap();
        set_client_id_at(&path, "microsoft", "ms-id-456").unwrap();

        // THEN each provider reads back its own
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
        // GIVEN a client file holding an id for each of the two providers
        let (_dir, path) = tmp_path();
        set_client_id_at(&path, "google", "google-original").unwrap();
        set_client_id_at(&path, "microsoft", "ms-original").unwrap();
        // WHEN one of them is overwritten
        set_client_id_at(&path, "google", "google-updated").unwrap();

        // THEN that one changes and the other is left untouched
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
        // GIVEN an empty client file
        let (_dir, path) = tmp_path();
        // WHEN an empty client id is stored
        set_client_id_at(&path, "google", "").unwrap();
        // THEN the lookup reports no override rather than an empty one
        assert_eq!(lookup_client_id_at(&path, "google"), None);
    }

    #[test]
    fn unknown_provider_rejected_on_write() {
        // GIVEN a provider name the file format does not define
        let (_dir, path) = tmp_path();
        // WHEN a client id is stored under it
        let err = set_client_id_at(&path, "github", "x").unwrap_err();
        // THEN the write is refused with InvalidInput
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput);
    }

    #[test]
    fn writing_an_id_keeps_legacy_secrets_until_they_are_stripped() {
        // GIVEN a file an earlier version wrote, a client secret in clear
        let (_dir, path) = tmp_path();
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(
            &path,
            "[google]\nclient_id = \"a\"\nclient_secret = \"GOCSPX-old\"\n",
        )
        .unwrap();

        // WHEN a new client id is written, then the secrets are stripped
        set_client_id_at(&path, "google", "b").unwrap();
        let before_strip = std::fs::read_to_string(&path).unwrap();
        strip_secrets_at(&path).unwrap();
        let after_strip = std::fs::read_to_string(&path).unwrap();

        // THEN the id write loses nothing the move has not taken yet, and the
        // strip leaves the id alone
        assert!(before_strip.contains("GOCSPX-old"));
        assert!(!after_strip.contains("GOCSPX-old"));
        assert_eq!(lookup_client_id_at(&path, "google").as_deref(), Some("b"));
    }

    #[cfg(unix)]
    #[test]
    fn the_file_is_written_private_to_its_owner() {
        use std::os::unix::fs::PermissionsExt;
        // GIVEN no file
        let (_dir, path) = tmp_path();
        // WHEN a client id is written
        set_client_id_at(&path, "google", "x").unwrap();
        // THEN only its owner can read it
        let mode = std::fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    #[test]
    fn malformed_file_surfaces_error() {
        // GIVEN a client file holding invalid TOML
        let (_dir, path) = tmp_path();
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "this is not = valid = toml = at all\n").unwrap();
        // WHEN it is loaded
        let err = load_from(&path).expect_err("malformed TOML must surface an error");
        // THEN the load fails with InvalidData rather than falling back on defaults
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    }
}
