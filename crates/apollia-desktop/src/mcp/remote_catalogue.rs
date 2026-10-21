//! The connection catalogue published online, signed, and laid over the
//! selection compiled into the application.
//!
//! The selection used to change only with a release: a connection added,
//! withdrawn or reclassified waited for the next build. The project now
//! publishes the catalogue as one index, signed in the minisign format with
//! the distribution key, the key that already signs the model cards and the
//! engine channel. The application reads it when the catalogue is opened (a
//! request with no body and no identifier, answered by a static file), checks
//! the signature against the key compiled into it, and keeps the copy it
//! accepted. Each entry of the index replaces the compiled entry of the same
//! package and an entry the build does not know is added; the compiled
//! selection is what stays when no index was ever accepted.
//!
//! An entry pinned to an npm version carries the npm lock it is installed
//! from, as npm wrote it, so a connection the build never heard of installs
//! with the same `npm ci` and the same integrity digests as a compiled one.
//!
//! The index is fetched from its main address, then its mirror; a refusal of
//! the signature stops there and keeps the copy already accepted. The address,
//! the mirror and the switch are `mcp.catalogue_*` in `apollia.toml`, each of
//! which the organisation policy may fix.

use std::path::{Path, PathBuf};
use std::time::Duration;

use apollia_core::config::{
    PolicySettings, DEFAULT_MCP_CATALOGUE_MIRROR_URL, DEFAULT_MCP_CATALOGUE_URL,
};
use serde::{Deserialize, Serialize};

use crate::mcp::enrichments::ConnectorEnrichment;

/// The format this build reads.
pub const SCHEMA_VERSION: u32 = 1;

/// Largest index accepted, in bytes.
const MAX_INDEX_BYTES: u64 = 8 * 1024 * 1024;

/// Largest signature accepted, in bytes.
const MAX_SIGNATURE_BYTES: u64 = 4 * 1024;

const TIMEOUT: Duration = Duration::from_secs(15);

const CACHED_INDEX_FILE_NAME: &str = "index.json";
const CACHED_SIGNATURE_FILE_NAME: &str = "index.json.minisig";

/// The settings of `[mcp]` an organisation policy may fix.
const URL_SETTING: &str = "mcp.catalogue_url";
const MIRROR_SETTING: &str = "mcp.catalogue_mirror_url";
const ENABLED_SETTING: &str = "mcp.catalogue_enabled";

/// One entry of the index: a catalogue entry, and the npm lock of a pinned
/// npm package.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteEntry {
    /// The entry, in the shape of the compiled selection.
    #[serde(flatten)]
    pub entry: ConnectorEnrichment,
    /// The `package-lock.json` the pinned package is installed from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub npm_lock: Option<serde_json::Value>,
}

/// The index, as published.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteCatalogue {
    /// The format of the index.
    pub schema_version: u32,
    /// Its entries.
    pub entries: Vec<RemoteEntry>,
}

/// Why an index was refused.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum CatalogueError {
    /// This build carries no distribution key, so no index can be accepted.
    #[error("this build carries no key to check the connection catalogue with")]
    NoTrustedKey,
    /// The signature does not hold.
    #[error("the signature of the connection catalogue does not hold")]
    Signature(#[source] apollia_signing::distribution::DistributionError),
    /// The index does not parse.
    #[error("the connection catalogue does not parse")]
    Parse(#[source] serde_json::Error),
    /// The index is in a format this build does not read.
    #[error("the connection catalogue is in format {found}, this build reads {SCHEMA_VERSION}")]
    Schema {
        /// The format found.
        found: u32,
    },
    /// A kept copy could not be read or written.
    #[error("the copy of the connection catalogue at {path} could not be used")]
    Io {
        /// The file.
        path: PathBuf,
        /// The cause.
        #[source]
        source: std::io::Error,
    },
    /// The HTTP client could not be built.
    #[error("the http client could not be built")]
    Client(#[source] reqwest::Error),
}

impl RemoteCatalogue {
    /// Parse `bytes`, refusing a format this build does not read.
    ///
    /// # Errors
    ///
    /// [`CatalogueError::Parse`] and [`CatalogueError::Schema`].
    pub fn parse(bytes: &[u8]) -> Result<Self, CatalogueError> {
        #[derive(Deserialize)]
        struct Head {
            schema_version: u32,
        }
        let head: Head = serde_json::from_slice(bytes).map_err(CatalogueError::Parse)?;
        if head.schema_version != SCHEMA_VERSION {
            return Err(CatalogueError::Schema {
                found: head.schema_version,
            });
        }
        serde_json::from_slice(bytes).map_err(CatalogueError::Parse)
    }

    /// The npm lock the index gives for `package`, as text.
    #[must_use]
    pub fn npm_lock(&self, package: &str) -> Option<String> {
        self.entries
            .iter()
            .find(|e| e.entry.package_identifier == package)
            .and_then(|e| e.npm_lock.as_ref())
            .map(serde_json::Value::to_string)
    }
}

/// Check `signature` over `content` with `public_key`, then parse the index.
///
/// # Errors
///
/// [`CatalogueError::NoTrustedKey`] without a key, [`CatalogueError::Signature`]
/// when the signature does not hold, and the errors of [`RemoteCatalogue::parse`].
pub fn verify(
    content: &[u8],
    signature: &str,
    public_key: Option<&str>,
) -> Result<RemoteCatalogue, CatalogueError> {
    let key = public_key.ok_or(CatalogueError::NoTrustedKey)?;
    apollia_signing::distribution::verify_minisign(content, signature, key)
        .map_err(CatalogueError::Signature)?;
    RemoteCatalogue::parse(content)
}

/// `base` with the entries of `remote` laid over it: an entry replaces the
/// one of the same package, in place, and an entry `base` does not hold is
/// added at the end, in the order of the index.
#[must_use]
pub fn merge(
    mut base: Vec<ConnectorEnrichment>,
    remote: &RemoteCatalogue,
) -> Vec<ConnectorEnrichment> {
    for incoming in &remote.entries {
        let entry = incoming.entry.clone();
        match base
            .iter_mut()
            .find(|e| e.package_identifier == entry.package_identifier)
        {
            Some(slot) => *slot = entry,
            None => base.push(entry),
        }
    }
    base
}

/// The cache directory of the index under the data directory.
#[must_use]
pub fn cache_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("cache").join("mcp-registry")
}

/// The index kept in `cache_dir`, checked again against `public_key`.
/// `Ok(None)` when no copy is kept.
///
/// # Errors
///
/// The errors of [`verify`], and [`CatalogueError::Io`] when a kept file
/// exists but cannot be read.
pub fn cached(
    cache_dir: &Path,
    public_key: Option<&str>,
) -> Result<Option<RemoteCatalogue>, CatalogueError> {
    let Some(content) = read_if_present(&cache_dir.join(CACHED_INDEX_FILE_NAME))? else {
        return Ok(None);
    };
    let Some(signature) = read_if_present(&cache_dir.join(CACHED_SIGNATURE_FILE_NAME))? else {
        return Ok(None);
    };
    verify(&content, &String::from_utf8_lossy(&signature), public_key).map(Some)
}

/// The index kept on this machine, when one was accepted and still holds.
/// A kept copy that no longer verifies is logged and ignored: the compiled
/// selection is what the catalogue then reads.
#[must_use]
pub fn kept(data_dir: &Path) -> Option<RemoteCatalogue> {
    match cached(&cache_dir(data_dir), compiled_public_key()) {
        Ok(kept) => kept,
        Err(error) => {
            tracing::event!(
                tracing::Level::WARN,
                error = %error,
                "mcp.catalogue.kept_copy_refused"
            );
            None
        }
    }
}

/// The index kept in the data directory of this machine. A test run reads
/// none: its data directory is the home of the person running it, and the
/// tests read the compiled selection.
#[must_use]
pub fn kept_on_this_machine() -> Option<RemoteCatalogue> {
    if cfg!(test) {
        return None;
    }
    kept(&apollia_core::paths::data_dir()?)
}

/// The distribution key compiled into this build.
#[must_use]
pub fn compiled_public_key() -> Option<&'static str> {
    apollia_signing::distribution::distribution_public_key()
}

/// Where the index is read from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogueSource {
    /// Main address; its signature is at the same address with `.minisig`.
    pub url: String,
    /// Address tried when the main one does not answer.
    pub mirror_url: Option<String>,
    /// `false` when the online catalogue is turned off.
    pub enabled: bool,
}

impl CatalogueSource {
    /// The source the `apollia.toml` at `path` configures, with what the
    /// organisation policy fixes laid over it; a setting that cannot be read
    /// keeps its default.
    #[must_use]
    pub fn resolve(path: &Path, policy: &PolicySettings) -> Self {
        let read = |key: &str| {
            apollia_core::config::resolve_setting(path, key, policy)
                .map(|setting| setting.value)
                .ok()
        };
        let text = |key: &str, default: &str| {
            read(key)
                .and_then(|v| v.as_str().map(|s| s.trim().to_owned()))
                .unwrap_or_else(|| default.to_owned())
        };
        let mirror = text(MIRROR_SETTING, DEFAULT_MCP_CATALOGUE_MIRROR_URL);
        Self {
            url: text(URL_SETTING, DEFAULT_MCP_CATALOGUE_URL),
            mirror_url: (!mirror.is_empty()).then_some(mirror),
            enabled: read(ENABLED_SETTING)
                .and_then(|v| v.as_bool())
                .unwrap_or(true),
        }
    }
}

/// What a refresh came to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refreshed {
    /// An index was fetched, accepted and kept.
    Fetched {
        /// How many entries it holds.
        entries: usize,
    },
    /// No address answered; the copy kept, if any, stays.
    Unanswered,
    /// The online catalogue is turned off, or this build has no key: nothing
    /// was asked.
    NotAsked,
}

/// Fetch the index from the main address, then the mirror, check it with
/// `public_key` and keep it in `cache_dir`.
///
/// # Errors
///
/// The errors of [`verify`] when an address answered with an index that is
/// refused (the copy kept stays), [`CatalogueError::Io`] when the accepted
/// index cannot be kept, and [`CatalogueError::Client`].
pub async fn refresh(
    source: &CatalogueSource,
    cache_dir: &Path,
    public_key: Option<&str>,
) -> Result<Refreshed, CatalogueError> {
    if !source.enabled || public_key.is_none() {
        return Ok(Refreshed::NotAsked);
    }
    let candidates = std::iter::once(source.url.as_str()).chain(source.mirror_url.as_deref());
    for url in candidates {
        let Some((content, signature)) = fetch_pair(url).await? else {
            continue;
        };
        let catalogue = verify(&content, &signature, public_key)?;
        store(cache_dir, &content, &signature)?;
        tracing::event!(
            tracing::Level::INFO,
            url = %url,
            entries = catalogue.entries.len(),
            "mcp.catalogue.index_fetched"
        );
        return Ok(Refreshed::Fetched {
            entries: catalogue.entries.len(),
        });
    }
    Ok(Refreshed::Unanswered)
}

/// How long opening the catalogue waits for the index before it shows the
/// copy already kept; the refresh goes on behind and serves the next opening.
const OPEN_WAIT: Duration = Duration::from_secs(4);

/// Refresh the index as the catalogue opens, from the source this machine's
/// `apollia.toml` and its organisation policy configure. Waits at most
/// [`OPEN_WAIT`]; a failure is logged and the copy kept stays in force.
pub async fn refresh_on_open() {
    let Some(data_dir) = apollia_core::paths::data_dir() else {
        return;
    };
    let location = apollia_permissions::policy_location();
    let policy = apollia_permissions::load_policy(&location.dir, location.owner);
    let source = CatalogueSource::resolve(
        &data_dir.join(apollia_core::paths::CONFIG_FILE_NAME),
        &policy.imposed_settings(),
    );
    let task = tokio::spawn(async move {
        if let Err(error) = refresh(&source, &cache_dir(&data_dir), compiled_public_key()).await {
            tracing::event!(
                tracing::Level::WARN,
                error = %error,
                "mcp.catalogue.refresh_failed"
            );
        }
    });
    let _ = tokio::time::timeout(OPEN_WAIT, task).await;
}

/// Keep `content` and its `signature` in `cache_dir`, each written whole
/// before it replaces the previous copy.
///
/// # Errors
///
/// [`CatalogueError::Io`] when a file cannot be written.
pub fn store(cache_dir: &Path, content: &[u8], signature: &str) -> Result<(), CatalogueError> {
    std::fs::create_dir_all(cache_dir).map_err(|source| CatalogueError::Io {
        path: cache_dir.to_path_buf(),
        source,
    })?;
    write_whole(&cache_dir.join(CACHED_INDEX_FILE_NAME), content)?;
    write_whole(
        &cache_dir.join(CACHED_SIGNATURE_FILE_NAME),
        signature.as_bytes(),
    )
}

async fn fetch_pair(url: &str) -> Result<Option<(Vec<u8>, String)>, CatalogueError> {
    let client = apollia_core::net::configured_endpoint_client_builder_for(url)
        .timeout(TIMEOUT)
        .build()
        .map_err(CatalogueError::Client)?;
    let Some(content) = fetch_capped(&client, url, MAX_INDEX_BYTES).await else {
        return Ok(None);
    };
    let signature_url = format!("{url}.minisig");
    let Some(signature) = fetch_capped(&client, &signature_url, MAX_SIGNATURE_BYTES).await else {
        return Ok(None);
    };
    Ok(Some((
        content,
        String::from_utf8_lossy(&signature).into_owned(),
    )))
}

async fn fetch_capped(client: &reqwest::Client, url: &str, limit: u64) -> Option<Vec<u8>> {
    let outcome = async {
        let response = client.get(url).send().await.map_err(|e| e.to_string())?;
        let status = response.status();
        if !status.is_success() {
            return Err(format!("answered {status}"));
        }
        apollia_core::net::read_capped_bytes(response, limit)
            .await
            .map_err(|e| e.to_string())
    }
    .await;
    match outcome {
        Ok(bytes) => Some(bytes),
        Err(detail) => {
            tracing::event!(
                tracing::Level::INFO,
                url = %url,
                detail = %detail,
                "mcp.catalogue.address_unanswered"
            );
            None
        }
    }
}

fn read_if_present(path: &Path) -> Result<Option<Vec<u8>>, CatalogueError> {
    match std::fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(source) => Err(CatalogueError::Io {
            path: path.to_path_buf(),
            source,
        }),
    }
}

fn write_whole(path: &Path, bytes: &[u8]) -> Result<(), CatalogueError> {
    let partial = path.with_extension("partial");
    std::fs::write(&partial, bytes)
        .and_then(|()| std::fs::rename(&partial, path))
        .map_err(|source| CatalogueError::Io {
            path: path.to_path_buf(),
            source,
        })
}

#[cfg(test)]
mod tests;
