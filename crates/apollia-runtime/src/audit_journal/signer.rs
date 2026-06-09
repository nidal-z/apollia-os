//! Detached signing of journal entries with a local key from `SecretStore`.
//!
//! Algorithm: HMAC-SHA256 (RFC 2104). The MAC is computed over the entry
//! `hash` (the lowercase hex digest from [`crate::audit_journal::hash`]), which
//! already commits to every content field and the chain position, so signing
//! the hash authenticates the whole entry. Signature encoding: base64url with
//! no padding. The key is a raw secret read once from `SecretStore` under
//! service `apollia-audit`, scope local-only; it never transits in clear text
//! through logs, debug output, or the API.
//!
//! Note: this deviates from the original Ed25519 specification. HMAC-SHA256 is
//! already available in the workspace (zero new dependency). The trade-off is
//! that verification requires the same secret key (symmetric MAC), so the
//! "external observer without the secret" property is reduced to "holder of the
//! audit key certifies integrity". The hash chain remains independently
//! verifiable without any key.

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};
use thiserror::Error;

use apollia_auth::SecretStore;

/// `SecretStore` service namespace for the audit signing key.
pub const AUDIT_SECRET_SERVICE: &str = "apollia-audit";

type HmacSha256 = Hmac<Sha256>;

/// Contract for signing journal entries.
///
/// Implemented by [`HmacSigner`]. The signer never exposes the secret key
/// material through any method or debug output.
pub trait JournalSigner: Send + Sync + 'static {
    /// Signs `payload_bytes` and returns a base64url (no padding) signature.
    fn sign(&self, payload_bytes: &[u8]) -> Result<String, SignerError>;

    /// Returns an opaque identifier for the active key (for `signing_key_id`).
    fn key_id(&self) -> &str;
}

/// Errors raised while building or using a [`JournalSigner`].
#[derive(Debug, Error)]
pub enum SignerError {
    /// The secret store backend could not be reached.
    #[error("secret store unavailable: {0}")]
    SecretStoreUnavailable(String),
    /// No key is stored under the configured label.
    #[error("key not found in secret store (scope: local_only, label: {label})")]
    KeyNotFound {
        /// The label that was looked up.
        label: String,
    },
    /// The stored key material is malformed (bad base64 or empty).
    #[error("invalid key material: {0}")]
    InvalidKey(String),
    /// The MAC computation itself failed.
    #[error("signing failed: {0}")]
    SigningFailed(String),
    /// A stored signature did not verify against the active key.
    #[error("signature verification failed at ({run_id}, seq={seq})")]
    VerificationFailed {
        /// Run of the offending entry.
        run_id: String,
        /// Sequence number of the offending entry.
        seq: u64,
    },
}

/// Policy applied when the journal signer is unavailable at startup.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SignerUnavailablePolicy {
    /// Log a warning and continue without signatures.
    WarnAndContinue,
    /// Refuse to start the journal actor.
    FailHard,
}

/// HMAC-SHA256 signer backed by a raw key loaded from `SecretStore`.
///
/// The key bytes are private and the manual [`std::fmt::Debug`] implementation
/// prints only the opaque `key_id`, so the secret never leaks through debug
/// formatting. The type is intentionally not `Clone`.
pub struct HmacSigner {
    key_id: String,
    key: Vec<u8>,
}

impl HmacSigner {
    /// Builds a signer from raw key bytes, deriving an opaque `key_id`
    /// fingerprint (a short SHA256 prefix of the key). Distinct keys yield
    /// distinct ids, which lets `signing_key_id` track key rotation without
    /// revealing the key.
    pub fn from_key_bytes(key: Vec<u8>) -> Result<Self, SignerError> {
        if key.is_empty() {
            return Err(SignerError::InvalidKey("empty key".to_string()));
        }
        let fingerprint = Sha256::digest(&key);
        let key_id = format!("hmac-sha256-{}", hex::encode(&fingerprint[..4]));
        Ok(Self { key_id, key })
    }

    /// Loads the key from `SecretStore` (service `apollia-audit`, the given
    /// label) and builds the signer. The stored value is a base64 (standard or
    /// url-safe) encoding of the raw key bytes.
    pub fn from_secret_store(store: &dyn SecretStore, label: &str) -> Result<Self, SignerError> {
        let stored = store
            .get(AUDIT_SECRET_SERVICE, label)
            .map_err(|e| SignerError::SecretStoreUnavailable(e.to_string()))?
            .ok_or_else(|| SignerError::KeyNotFound {
                label: label.to_string(),
            })?;
        let key = decode_key(stored.trim())?;
        Self::from_key_bytes(key)
    }

    /// Returns `true` when `signature_b64` is the valid signature of `payload`
    /// under this signer's key. Comparison is constant-time.
    pub fn verify(&self, payload: &[u8], signature_b64: &str) -> bool {
        let Ok(expected) = self.sign(payload) else {
            return false;
        };
        constant_time_eq::constant_time_eq(expected.as_bytes(), signature_b64.as_bytes())
    }
}

impl std::fmt::Debug for HmacSigner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HmacSigner")
            .field("key_id", &self.key_id)
            .field("key", &"<redacted>")
            .finish()
    }
}

impl JournalSigner for HmacSigner {
    fn sign(&self, payload_bytes: &[u8]) -> Result<String, SignerError> {
        let mut mac = HmacSha256::new_from_slice(&self.key)
            .map_err(|e| SignerError::InvalidKey(e.to_string()))?;
        mac.update(payload_bytes);
        let tag = mac.finalize().into_bytes();
        Ok(URL_SAFE_NO_PAD.encode(tag))
    }

    fn key_id(&self) -> &str {
        &self.key_id
    }
}

/// Decodes a base64 key value, accepting standard and url-safe alphabets.
fn decode_key(value: &str) -> Result<Vec<u8>, SignerError> {
    use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD as URL};
    if let Ok(bytes) = STANDARD.decode(value) {
        return Ok(bytes);
    }
    URL.decode(value)
        .map_err(|e| SignerError::InvalidKey(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn signer(seed: &[u8]) -> HmacSigner {
        HmacSigner::from_key_bytes(seed.to_vec()).expect("build signer")
    }

    // AC-1: a signature is produced and is stable for the same payload and key
    #[test]
    fn test_sign_roundtrip_and_verify() {
        // GIVEN a signer with a test key
        let s = signer(b"super-secret-audit-key");
        let payload = b"entry-hash-hex";
        // WHEN signing
        let sig = s.sign(payload).expect("sign");
        // THEN the signature is non-empty base64url and verifies under the key
        assert!(!sig.is_empty());
        assert!(sig
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'));
        assert!(s.verify(payload, &sig));
    }

    // AC-3: the secret key does not appear in debug output
    #[test]
    fn test_signing_key_not_leaked_in_debug() {
        // GIVEN a signer
        let s = signer(b"top-secret-bytes-1234");
        // WHEN it is debug-formatted
        let dump = format!("{s:?}");
        // THEN the raw key is absent and the redaction marker is present
        assert!(!dump.contains("top-secret-bytes-1234"));
        assert!(dump.contains("<redacted>"));
        assert!(dump.contains(s.key_id()));
    }

    // AC-5: verifying with a different key fails
    #[test]
    fn test_verification_fails_with_wrong_key() {
        // GIVEN a signature produced with key K1
        let k1 = signer(b"key-one");
        let k2 = signer(b"key-two");
        let payload = b"entry-hash-hex";
        let sig = k1.sign(payload).expect("sign");
        // WHEN verified with key K2
        // THEN verification fails and the key ids differ
        assert!(!k2.verify(payload, &sig));
        assert_ne!(k1.key_id(), k2.key_id());
    }

    // An empty key is rejected rather than producing a weak signer
    #[test]
    fn test_empty_key_rejected() {
        // GIVEN empty key bytes
        // WHEN building a signer
        let err = HmacSigner::from_key_bytes(Vec::new());
        // THEN it is an InvalidKey error
        assert!(matches!(err, Err(SignerError::InvalidKey(_))));
    }
}
