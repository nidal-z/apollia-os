//! The `.apo` package archive: build it from a folder, read it, extract it.
//!
//! An `.apo` file is a zip archive (deflate only) holding:
//!
//! - `apollia.toml` at the root, the package manifest
//!   ([`crate::apollia_manifest`]);
//! - the Python modules and resources of the package, at their path relative
//!   to the package root;
//! - when the package is signed, two more entries: `SIGNATURE`, the versioned
//!   signature file of `apollia-signing`, and `SIGNATURE.key`, the public key
//!   that made it (64 lowercase hexadecimal characters and a line feed).
//!
//! The signature covers the canonical digest list of every file entry other
//! than those two. Adding, removing or altering an entry after signing breaks
//! it. The key travels with the archive so that the signature can be checked
//! anywhere; whether that key is one a reader trusts is a separate question,
//! answered by comparing [`SignatureState::Signed`]'s fingerprint against the
//! keys the reader holds.
//!
//! Reading is defensive, because an archive comes from somewhere else. Every
//! limit is in [`ArchiveLimits`], and every entry name passes
//! [`check_entry_path`] before anything else happens to it:
//!
//! - the manifest is read on its own, without touching the other entries;
//! - an absolute path, a `..` component, a backslash, a drive prefix, or an
//!   empty or `.` component is refused;
//! - a symbolic link is refused;
//! - the number of entries, the size of each entry and the total
//!   decompressed size are bounded, and the bound is applied to the bytes
//!   actually decompressed, not to the size the archive declares.

mod build;
mod read;

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_build;

use std::path::PathBuf;

pub use apollia_signing::SigningKeyPair;
pub use build::{build_from_dir, BuildOptions, BuildReport};
pub use read::{ApoArchive, EntryInfo};

use crate::apollia_manifest::ApolliaManifestError;

/// File extension of a package archive, without the dot.
pub const APO_EXTENSION: &str = "apo";

/// Name of the entry that carries the signature file.
pub const SIGNATURE_ENTRY: &str = "SIGNATURE";

/// Name of the entry that carries the public key of the signature.
pub const SIGNATURE_KEY_ENTRY: &str = "SIGNATURE.key";

/// Bounds applied when an archive is built, read or extracted.
///
/// The defaults are the documented limits of the format. A package over any of
/// them is refused at build time, so an archive Apollia produced always reads
/// back.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ArchiveLimits {
    /// Largest archive file accepted, in bytes. Default 256 MiB.
    pub max_archive_bytes: u64,
    /// Largest number of entries, directories included. Default 4096.
    pub max_entries: usize,
    /// Largest decompressed size of one entry, in bytes. Default 64 MiB.
    pub max_entry_bytes: u64,
    /// Largest decompressed size of all entries together, in bytes.
    /// Default 512 MiB.
    pub max_total_bytes: u64,
    /// Largest `apollia.toml`, in bytes. Default 1 MiB.
    pub max_manifest_bytes: u64,
}

impl Default for ArchiveLimits {
    fn default() -> Self {
        Self {
            max_archive_bytes: 256 * 1024 * 1024,
            max_entries: 4096,
            max_entry_bytes: 64 * 1024 * 1024,
            max_total_bytes: 512 * 1024 * 1024,
            max_manifest_bytes: 1024 * 1024,
        }
    }
}

/// What the signature of an archive says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SignatureState {
    /// The archive carries no `SIGNATURE` entry.
    Unsigned,
    /// The signature holds over every entry of the archive.
    Signed {
        /// Publisher name declared in the signature file. Not covered by the
        /// signature: the fingerprint is what identifies the author.
        publisher: String,
        /// SHA-256 fingerprint of the signing key, 64 lowercase hex characters.
        key_fingerprint: String,
    },
}

/// Errors raised while building, reading or extracting an `.apo` archive.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ApoError {
    /// Filesystem failure on `path`.
    #[error("{path}: {source}")]
    Io {
        /// The file or folder involved.
        path: PathBuf,
        /// The underlying error.
        #[source]
        source: std::io::Error,
    },
    /// The file is not a readable zip archive.
    #[error("not a readable package archive: {message}")]
    Zip {
        /// What the zip reader or writer reported.
        message: String,
    },
    /// An entry name is unsafe to extract.
    #[error("unsafe entry path `{path}`: {reason}")]
    UnsafePath {
        /// The entry name as stored.
        path: String,
        /// Why it was refused.
        reason: &'static str,
    },
    /// An entry is a symbolic link.
    #[error("entry `{path}` is a symbolic link")]
    Symlink {
        /// The entry name.
        path: String,
    },
    /// The same entry name appears twice.
    #[error("entry `{path}` appears more than once")]
    DuplicateEntry {
        /// The entry name.
        path: String,
    },
    /// A bound of [`ArchiveLimits`] was crossed.
    #[error("package exceeds the {limit} limit of {max}")]
    LimitExceeded {
        /// Which limit.
        limit: &'static str,
        /// Its value.
        max: u64,
    },
    /// The archive has no `apollia.toml` at its root.
    #[error("the package has no apollia.toml at its root")]
    MissingManifest,
    /// The manifest is not valid UTF-8.
    #[error("apollia.toml is not valid UTF-8")]
    ManifestEncoding,
    /// The manifest breaks its schema or a rule.
    #[error(transparent)]
    Manifest(#[from] ApolliaManifestError),
    /// The signature does not hold, or its files are malformed.
    #[error("invalid package signature: {reason}")]
    InvalidSignature {
        /// Why.
        reason: String,
    },
    /// A folder declares tools or workflows the manifest does not describe.
    #[error(
        "apollia.toml lacks the descriptors of {modules:?}; run `python -m apollia describe` on the package folder"
    )]
    MissingDescriptors {
        /// Package-relative modules that need descriptors.
        modules: Vec<String>,
    },
    /// A module named by the manifest is not a file of the package.
    #[error("apollia.toml names module `{module}`, which is not a file of the package")]
    MissingModule {
        /// The module path as declared.
        module: String,
    },
    /// The package folder holds a file under a name the format reserves.
    #[error("`{path}` is a name the package format reserves")]
    ReservedName {
        /// The reserved name.
        path: String,
    },
    /// The signing step failed.
    #[error("cannot sign the package: {0}")]
    Signing(#[from] apollia_signing::SigningError),
    /// The extraction target already exists.
    #[error("extraction target {path} already exists")]
    TargetExists {
        /// The target folder.
        path: PathBuf,
    },
}

impl From<zip::result::ZipError> for ApoError {
    fn from(error: zip::result::ZipError) -> Self {
        ApoError::Zip {
            message: error.to_string(),
        }
    }
}

/// Checks that `name` is a relative path made of plain components only.
///
/// A trailing `/` (a directory entry) is accepted. Everything that could make
/// an extraction land outside its target is refused: an absolute path, a
/// `..` component, a backslash (a separator on Windows), a `:` (a drive or a
/// stream on Windows), a NUL byte, and an empty or `.` component.
///
/// # Errors
///
/// [`ApoError::UnsafePath`] naming the reason.
pub fn check_entry_path(name: &str) -> Result<(), ApoError> {
    let refuse = |reason| {
        Err(ApoError::UnsafePath {
            path: name.to_owned(),
            reason,
        })
    };
    if name.is_empty() {
        return refuse("empty name");
    }
    if name.starts_with('/') {
        return refuse("absolute path");
    }
    if name.contains('\\') {
        return refuse("backslash in path");
    }
    if name.contains(':') {
        return refuse("drive or stream prefix");
    }
    if name.contains('\0') {
        return refuse("NUL byte in path");
    }
    let trimmed = name.strip_suffix('/').unwrap_or(name);
    for component in trimmed.split('/') {
        match component {
            ".." => return refuse("parent directory component"),
            "" | "." => return refuse("empty or current directory component"),
            _ => {}
        }
    }
    Ok(())
}
