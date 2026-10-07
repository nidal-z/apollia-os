//! Verification of minisign signatures on distribution metadata.
//!
//! Distribution metadata published by the Apollia release pipeline is signed
//! in the minisign format with a dedicated distribution key, the same format
//! as the desktop updater key and a distinct key. This module only verifies:
//! signing happens in the release pipeline, never in the application.
//!
//! Two encodings are accepted for both the public key and the signature,
//! because the release tooling produces the second one:
//!
//! - the minisign text itself (an `untrusted comment:` line, then the base64
//!   payload, and for a signature the trusted comment and global signature);
//!   a public key may also be given as its bare base64 line;
//! - that same text wrapped once more in base64, as written by
//!   `cargo tauri signer generate` and `cargo tauri signer sign`.
//!
//! Only pre-hashed signatures, the minisign default, are accepted. The trusted
//! comment is covered by the global signature, so altering it is a refusal.

use base64::Engine as _;

/// The distribution public key compiled into this build, as the release
/// tooling writes it (`cargo tauri signer generate`).
///
/// The file is read at compile time from `keys/distribution.key.pub`. It is
/// empty until the release key is placed there, and an empty file means no key
/// at all: [`distribution_public_key`] then answers `None` and every verifier
/// refuses, rather than trusting some other key.
const DISTRIBUTION_PUBLIC_KEY: &str = include_str!("../keys/distribution.key.pub");

/// The one key distribution metadata is accepted from, or `None` when this
/// build carries none.
///
/// No configuration, environment variable or route can replace it: a mirror
/// changes where metadata comes from, never who may sign it.
#[must_use]
pub fn distribution_public_key() -> Option<&'static str> {
    let key = DISTRIBUTION_PUBLIC_KEY.trim();
    if key.is_empty() {
        None
    } else {
        Some(key)
    }
}

/// Errors raised while verifying a minisign signature.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum DistributionError {
    /// The public key could not be decoded.
    #[error("invalid minisign public key")]
    InvalidPublicKey {
        /// The decoding failure.
        #[source]
        source: minisign_verify::Error,
    },
    /// The signature could not be decoded.
    #[error("invalid minisign signature")]
    InvalidSignature {
        /// The decoding failure.
        #[source]
        source: minisign_verify::Error,
    },
    /// A base64-wrapped input did not decode to UTF-8 minisign text.
    #[error("invalid base64 wrapping of {what}")]
    InvalidWrapping {
        /// Which input was refused: `public key` or `signature`.
        what: &'static str,
    },
    /// The signature is well formed but does not hold for this content, this
    /// key and this trusted comment.
    #[error("minisign signature rejected")]
    Rejected {
        /// The verification failure.
        #[source]
        source: minisign_verify::Error,
    },
}

impl DistributionError {
    /// True when the signature was made by another key than the one it was
    /// verified against: the key identifiers differ. A signature of the right
    /// key that does not hold for the content answers false.
    #[must_use]
    pub fn is_unknown_key(&self) -> bool {
        matches!(
            self,
            Self::Rejected {
                source: minisign_verify::Error::UnexpectedKeyId
            }
        )
    }
}

/// Checks that `signature` is a valid minisign signature of `content` by the
/// key `public_key`, and returns the trusted comment it carries.
///
/// # Errors
///
/// Returns [`DistributionError::InvalidPublicKey`] or
/// [`DistributionError::InvalidSignature`] when an input cannot be decoded,
/// [`DistributionError::InvalidWrapping`] when a base64-wrapped input is not
/// minisign text, and [`DistributionError::Rejected`] when the signature does
/// not hold: content altered, another key, trusted comment altered, or a
/// legacy non pre-hashed signature.
///
/// # Examples
///
/// ```
/// use apollia_signing::distribution::verify_minisign;
///
/// let public_key = "RWQf6LRCGA9i53mlYecO4IzT51TGPpvWucNSCh1CBM0QTaLn73Y7GFO3";
/// let signature = "untrusted comment: signature from minisign secret key
/// RUQf6LRCGA9i559r3g7V1qNyJDApGip8MfqcadIgT9CuhV3EMhHoN1mGTkUidF/z7SrlQgXdy8ofjb7bNJJylDOocrCo8KLzZwo=
/// trusted comment: timestamp:1633700835\tfile:test\tprehashed
/// wLMDjy9FLAuxZ3q4NlEvkgtyhrr0gtTu6KC4KBJdITbbOeAi1zBIYo0v4iTgt8jJpIidRJnp94ABQkJAgAooBQ==";
/// let trusted = verify_minisign(b"test", signature, public_key)?;
/// assert!(trusted.starts_with("timestamp:"));
/// # Ok::<(), apollia_signing::distribution::DistributionError>(())
/// ```
pub fn verify_minisign(
    content: &[u8],
    signature: &str,
    public_key: &str,
) -> Result<String, DistributionError> {
    let public_key = decode_public_key(public_key)?;
    let signature_text = unwrap_text(signature.trim(), "signature")?;
    let signature = minisign_verify::Signature::decode(&signature_text)
        .map_err(|source| DistributionError::InvalidSignature { source })?;
    public_key
        .verify(content, &signature, false)
        .map_err(|source| DistributionError::Rejected { source })?;
    Ok(signature.trusted_comment().to_owned())
}

fn decode_public_key(input: &str) -> Result<minisign_verify::PublicKey, DistributionError> {
    let input = input.trim();
    let decoded = if input.starts_with("RW") && !input.contains('\n') {
        minisign_verify::PublicKey::from_base64(input)
    } else {
        let text = unwrap_text(input, "public key")?;
        minisign_verify::PublicKey::decode(&text)
    };
    decoded.map_err(|source| DistributionError::InvalidPublicKey { source })
}

/// Returns `input` when it already is minisign text, or its base64 decoding.
fn unwrap_text(input: &str, what: &'static str) -> Result<String, DistributionError> {
    if input.starts_with("untrusted comment:") {
        return Ok(input.to_owned());
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(input)
        .map_err(|_| DistributionError::InvalidWrapping { what })?;
    String::from_utf8(bytes).map_err(|_| DistributionError::InvalidWrapping { what })
}
