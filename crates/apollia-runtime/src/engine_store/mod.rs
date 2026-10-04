//! The engine store: inference engines downloaded or imported after install,
//! each verified against a signed manifest before anything else happens.
//!
//! Engines have their own update channel and do not follow the version of
//! Apollia. A channel (`stable` or `preview`) publishes one manifest, and
//! every archive the store installs comes from a manifest whose signature
//! held first.
//!
//! # Manifest format
//!
//! `engines.json`, UTF-8 JSON, format 1:
//!
//! ```json
//! {
//!   "format": 1,
//!   "channel": "stable",
//!   "sequence": 6,
//!   "packages": [
//!     {
//!       "engine": "llama-server",
//!       "tag": "b6000",
//!       "variant": "cuda",
//!       "platform": "windows-x64",
//!       "size": 412345678,
//!       "url": "llama-b6000-bin-win-cuda-12.4-x64.zip",
//!       "sha256": "<64 lowercase hex digits>"
//!     }
//!   ]
//! }
//! ```
//!
//! - `engine` is `llama-server`, `vllm-mlx` or `nodejs`; `variant` is `cuda`,
//!   `vulkan`, `cpu` or `metal`; `platform` is `<os>-<arch>`, with `os` in
//!   `macos`, `linux`, `windows` and `arch` in `x64`, `arm64`.
//! - A `nodejs` package is a pinned Node.js LTS release (`tag` `v22.11.0`),
//!   always under the `cpu` variant. Its upstream archive is repackaged as
//!   zip like the others, and installed the first time a tool server needs
//!   it ([`node`]).
//! - `url` is relative to the manifest: no scheme, no host, no parent step. It
//!   ends in `.zip`, the only archive format read. An upstream build shipped
//!   as `.tar.gz` (macOS, Linux) is repackaged as zip by the pipeline that
//!   publishes the manifest, and `sha256` is the digest of that zip.
//! - `sequence` strictly identifies a manifest within its channel. The store
//!   remembers the highest one it accepted per channel and refuses a lower
//!   one, so an old manifest cannot bring back an old engine.
//!
//! Beside it, `engines.json.minisig` is the minisign signature of the exact
//! bytes of `engines.json`, made with the distribution key. The key accepted
//! is the one compiled into the binary
//! ([`apollia_signing::distribution::distribution_public_key`]); no setting
//! replaces it. A mirror changes where the files come from, never who signs.
//!
//! # Layout
//!
//! Under `<data_dir>/engines/`:
//!
//! - `settings.json`: the followed channel and the base address;
//! - `sequences.json`: the highest sequence accepted per channel;
//! - `manifests/<channel>.json` and `.minisig`: the last verified manifest,
//!   verified again whenever it is read;
//! - `<engine>/<tag>/<variant>/`: an installed package, with its record
//!   `.apollia-engine.json` written only once the digest matched and the
//!   archive was extracted. A directory without it is not an engine.
//!
//! The manifest of a channel is fetched from
//! `<base_url>/engines-<channel>/engines.json`, and each archive from its
//! `url` relative to that manifest.

mod archive;
mod fetch;
mod installed;
mod manifest;
pub mod node;
mod settings;

use std::path::PathBuf;

pub use fetch::{
    import_from_dir, install_package, install_package_with_progress, refresh_manifest, updates,
    VariantUpdate, MANIFEST_FILE, SIGNATURE_FILE,
};
pub use installed::{
    find_binary, installed_packages, newest_installed, package_dir, preferred_llama_server,
    wanted_llama_variant, InstalledPackage, Source, RECORD_FILE,
};
pub use manifest::{
    archive_kind, compare_tags, current_platform, verify_manifest, ArchiveKind, Channel, Engine,
    Manifest, Package, Variant, MANIFEST_FORMAT,
};
pub use settings::{cached_manifest, EngineStoreSettings, DEFAULT_BASE_URL};

/// Errors raised by the engine store. Each message is meant to be shown.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum EngineStoreError {
    /// This build carries no distribution key, so nothing can be verified.
    #[error("this build carries no distribution key: engines cannot be verified")]
    NoTrustedKey,
    /// The manifest signature does not hold for the compiled key.
    #[error("the engine manifest signature is not valid for the distribution key")]
    Signature {
        /// The verification failure.
        #[source]
        source: apollia_signing::distribution::DistributionError,
    },
    /// No signature was found beside the manifest.
    #[error("no signature beside the engine manifest at {path}")]
    MissingSignature {
        /// Where the signature was looked for.
        path: PathBuf,
    },
    /// The manifest is not one this build reads.
    #[error("invalid engine manifest: {reason}")]
    ManifestInvalid {
        /// What is wrong.
        reason: String,
    },
    /// The manifest belongs to another channel.
    #[error("the manifest is for the {found} channel, not {expected}")]
    ChannelMismatch {
        /// The channel asked for.
        expected: &'static str,
        /// The channel the manifest names.
        found: &'static str,
    },
    /// The manifest is older than one already accepted.
    #[error("the manifest sequence {found} is older than the accepted {accepted}: refused")]
    Rollback {
        /// Sequence of the manifest received.
        found: u64,
        /// Highest sequence accepted so far on this channel.
        accepted: u64,
    },
    /// The manifest has no package for this request.
    #[error("no {engine} {variant} package for {platform} in the manifest")]
    PackageNotFound {
        /// Engine asked for.
        engine: &'static str,
        /// Variant asked for.
        variant: &'static str,
        /// This platform.
        platform: String,
    },
    /// The archive does not match the digest of the verified manifest.
    #[error("archive {file}: sha256 {actual} does not match the manifest ({expected}); deleted")]
    DigestMismatch {
        /// Archive name.
        file: String,
        /// Digest the manifest carries.
        expected: String,
        /// Digest of the bytes received.
        actual: String,
    },
    /// The archive is larger than the manifest says.
    #[error("archive {file} is larger than the {expected} bytes the manifest states")]
    TooLarge {
        /// Archive name.
        file: String,
        /// Size the manifest carries.
        expected: u64,
    },
    /// A server answered with an error status.
    #[error("{url} answered HTTP {status}")]
    HttpStatus {
        /// Address requested.
        url: String,
        /// Status received.
        status: u16,
    },
    /// The transport failed.
    #[error("request to {url} failed")]
    Transport {
        /// Address requested.
        url: String,
        /// Transport failure.
        #[source]
        source: reqwest::Error,
    },
    /// The base address cannot be used.
    #[error("invalid engine store address {url}")]
    InvalidUrl {
        /// The address refused.
        url: String,
    },
    /// The archive is malformed or unsafe.
    #[error("engine archive refused: {reason}")]
    Archive {
        /// What is wrong.
        reason: String,
    },
    /// A store file is not readable as JSON.
    #[error("store file {path} is malformed")]
    StoreFile {
        /// The file.
        path: PathBuf,
        /// Decoding failure.
        #[source]
        source: serde_json::Error,
    },
    /// Filesystem failure.
    #[error("engine store i/o failed at {path}")]
    Io {
        /// Path involved.
        path: PathBuf,
        /// I/O failure.
        #[source]
        source: std::io::Error,
    },
}

/// Where the store lives under a data directory.
#[must_use]
pub fn store_root(data_dir: &std::path::Path) -> PathBuf {
    data_dir.join("engines")
}

/// Everything an operation on the store needs, gathered once.
#[derive(Debug, Clone)]
pub struct EngineStore {
    /// `<data_dir>/engines`.
    pub root: PathBuf,
    /// The only key manifests are accepted from.
    pub trusted_key: String,
    /// The platform packages are chosen for.
    pub platform: String,
}

impl EngineStore {
    /// The store of `data_dir`, trusting the compiled distribution key.
    ///
    /// # Errors
    ///
    /// [`EngineStoreError::NoTrustedKey`] when this build carries none.
    pub fn open(data_dir: &std::path::Path) -> Result<Self, EngineStoreError> {
        let key = apollia_signing::distribution::distribution_public_key()
            .ok_or(EngineStoreError::NoTrustedKey)?;
        Ok(Self {
            root: store_root(data_dir),
            trusted_key: key.to_owned(),
            platform: current_platform(),
        })
    }
}

#[cfg(test)]
mod tests;
