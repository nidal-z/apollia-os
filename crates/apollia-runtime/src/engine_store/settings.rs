//! The persisted state of the store: settings, accepted sequences, and the
//! last verified manifest of each channel.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::engine_store::manifest::{verify_manifest, Channel, Manifest};
use crate::engine_store::EngineStoreError;

/// Where manifests are published when no mirror is set.
pub const DEFAULT_BASE_URL: &str = "https://github.com/Apollia-OS/apollia-os/releases/download";

/// The settings of the store.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EngineStoreSettings {
    /// The channel followed.
    #[serde(default)]
    pub channel: Channel,
    /// The base address manifests and archives are fetched from.
    #[serde(default = "default_base_url")]
    pub base_url: String,
}

fn default_base_url() -> String {
    DEFAULT_BASE_URL.to_owned()
}

impl Default for EngineStoreSettings {
    fn default() -> Self {
        Self {
            channel: Channel::Stable,
            base_url: default_base_url(),
        }
    }
}

fn io(path: &Path) -> impl FnOnce(std::io::Error) -> EngineStoreError {
    let path = path.to_path_buf();
    move |source| EngineStoreError::Io { path, source }
}

fn read_json<T: serde::de::DeserializeOwned + Default>(path: &Path) -> Result<T, EngineStoreError> {
    match std::fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes).map_err(|source| EngineStoreError::StoreFile {
            path: path.to_path_buf(),
            source,
        }),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(T::default()),
        Err(e) => Err(io(path)(e)),
    }
}

/// Write `bytes` to `path` through a sibling temporary file and a rename.
pub(crate) fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), EngineStoreError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(io(parent))?;
    }
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, bytes).map_err(io(&tmp))?;
    std::fs::rename(&tmp, path).map_err(io(path))
}

fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<(), EngineStoreError> {
    let bytes = serde_json::to_vec_pretty(value).map_err(|source| EngineStoreError::StoreFile {
        path: path.to_path_buf(),
        source,
    })?;
    write_atomic(path, &bytes)
}

impl EngineStoreSettings {
    fn path(root: &Path) -> PathBuf {
        root.join("settings.json")
    }

    /// The stored settings, or the defaults when none were saved.
    ///
    /// # Errors
    ///
    /// [`EngineStoreError::StoreFile`] for a malformed file,
    /// [`EngineStoreError::Io`] when it cannot be read.
    pub fn load(root: &Path) -> Result<Self, EngineStoreError> {
        read_json(&Self::path(root))
    }

    /// Save the settings after checking the base address.
    ///
    /// # Errors
    ///
    /// [`EngineStoreError::InvalidUrl`] for an address that is not `http` or
    /// `https`, [`EngineStoreError::Io`] when the file cannot be written.
    pub fn save(&self, root: &Path) -> Result<(), EngineStoreError> {
        check_base_url(&self.base_url)?;
        write_json(&Self::path(root), self)
    }
}

/// An `http` or `https` address without query or fragment.
pub(crate) fn check_base_url(url: &str) -> Result<reqwest::Url, EngineStoreError> {
    let invalid = || EngineStoreError::InvalidUrl {
        url: url.to_owned(),
    };
    let parsed = reqwest::Url::parse(url.trim_end_matches('/')).map_err(|_| invalid())?;
    if !matches!(parsed.scheme(), "http" | "https")
        || parsed.query().is_some()
        || parsed.fragment().is_some()
        || parsed.host_str().is_none()
    {
        return Err(invalid());
    }
    Ok(parsed)
}

/// The highest sequence accepted per channel.
#[derive(Debug, Default, Serialize, Deserialize)]
struct Sequences(BTreeMap<String, u64>);

fn sequences_path(root: &Path) -> PathBuf {
    root.join("sequences.json")
}

/// Refuse a manifest older than the last accepted one of its channel, and
/// remember it otherwise.
///
/// # Errors
///
/// [`EngineStoreError::Rollback`] for an older manifest, and the store file
/// errors.
pub(crate) fn accept_sequence(root: &Path, manifest: &Manifest) -> Result<(), EngineStoreError> {
    let path = sequences_path(root);
    let mut seen: Sequences = read_json(&path)?;
    let key = manifest.channel.as_str().to_owned();
    let accepted = seen.0.get(&key).copied().unwrap_or(0);
    if manifest.sequence < accepted {
        return Err(EngineStoreError::Rollback {
            found: manifest.sequence,
            accepted,
        });
    }
    if manifest.sequence > accepted {
        seen.0.insert(key, manifest.sequence);
        write_json(&path, &seen)?;
    }
    Ok(())
}

fn cached_paths(root: &Path, channel: Channel) -> (PathBuf, PathBuf) {
    let dir = root.join("manifests");
    (
        dir.join(format!("{}.json", channel.as_str())),
        dir.join(format!("{}.json.minisig", channel.as_str())),
    )
}

/// Keep a verified manifest and its signature as received.
pub(crate) fn cache_manifest(
    root: &Path,
    channel: Channel,
    bytes: &[u8],
    signature: &str,
) -> Result<(), EngineStoreError> {
    let (manifest, sig) = cached_paths(root, channel);
    write_atomic(&sig, signature.as_bytes())?;
    write_atomic(&manifest, bytes)
}

/// The last manifest kept for `channel`, verified again, or `None` when none
/// was kept or it no longer verifies.
#[must_use]
pub fn cached_manifest(root: &Path, channel: Channel, trusted_key: &str) -> Option<Manifest> {
    let (manifest, sig) = cached_paths(root, channel);
    let bytes = std::fs::read(&manifest).ok()?;
    let signature = std::fs::read_to_string(&sig).ok()?;
    match verify_manifest(&bytes, &signature, trusted_key, channel) {
        Ok(m) => Some(m),
        Err(error) => {
            tracing::event!(
                tracing::Level::WARN,
                path = %manifest.display(),
                error = %error,
                "engine_store.cached_manifest.refused"
            );
            None
        }
    }
}
