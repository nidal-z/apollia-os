//! Error type of the crate.

use std::path::PathBuf;

use apollia_auth::AuthError;

/// Errors raised while building, signing, storing or parsing signed material.
///
/// A signature that does not hold is not an error: [`crate::verify`] answers
/// [`crate::Verdict::Invalid`]. Errors are reserved for input that cannot be
/// read at all.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum SigningError {
    /// A digest list entry has a path that cannot be encoded canonically.
    #[error("invalid digest list path: {reason}")]
    InvalidPath {
        /// Why the path was refused.
        reason: &'static str,
    },
    /// The same path was added twice to a digest list.
    #[error("duplicate digest list path: {path}")]
    DuplicatePath {
        /// The path that was already present.
        path: String,
    },
    /// Key material could not be decoded.
    #[error("invalid key material: {reason}")]
    InvalidKey {
        /// Why the key was refused.
        reason: &'static str,
    },
    /// The operating system did not provide randomness for key generation.
    #[error("entropy source unavailable")]
    Entropy {
        /// The underlying failure.
        #[source]
        source: rand::rand_core::OsError,
    },
    /// The secret store backend failed.
    #[error("secret store failure")]
    SecretStore {
        /// The backend error.
        #[source]
        source: AuthError,
    },
    /// No signing key is stored under the requested label.
    #[error("no signing key stored under label {label}")]
    KeyNotFound {
        /// The label that was looked up.
        label: String,
    },
    /// A `SIGNATURE` file or a signature value is malformed.
    #[error("malformed signature: {reason}")]
    MalformedSignature {
        /// Why the input was refused.
        reason: &'static str,
    },
    /// A `SIGNATURE` file declares a format version this build cannot read.
    #[error("unsupported signature format version {version}")]
    UnsupportedVersion {
        /// The version found in the file.
        version: String,
    },
    /// A declared publisher name cannot be written on one line.
    #[error("invalid publisher name: {reason}")]
    InvalidPublisher {
        /// Why the name was refused.
        reason: &'static str,
    },
    /// A private key file could not be created, written or read.
    #[error("private key file {}", path.display())]
    KeyFile {
        /// The file.
        path: PathBuf,
        /// The underlying failure.
        #[source]
        source: std::io::Error,
    },
}
