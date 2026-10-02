//! Ed25519 keys, detached signatures over a [`DigestList`], and verification.

use std::fmt;

use ed25519_dalek::Signer as _;
use rand::TryRngCore as _;

use crate::digest_list::{sha256, DigestList};
use crate::error::SigningError;

/// An Ed25519 key pair whose private half signs digest lists.
///
/// The private half is wiped from memory when the value is dropped, and the
/// `Debug` output never shows it.
#[derive(Clone)]
pub struct SigningKeyPair {
    inner: ed25519_dalek::SigningKey,
}

impl SigningKeyPair {
    /// Generates a new key pair from the operating system's randomness.
    ///
    /// # Errors
    ///
    /// Returns [`SigningError::Entropy`] when the operating system refuses to
    /// provide random bytes.
    ///
    /// # Examples
    ///
    /// ```
    /// let pair = apollia_signing::SigningKeyPair::generate()?;
    /// assert_eq!(pair.public_key().fingerprint().len(), 64);
    /// # Ok::<(), apollia_signing::SigningError>(())
    /// ```
    pub fn generate() -> Result<Self, SigningError> {
        let mut seed = [0u8; ed25519_dalek::SECRET_KEY_LENGTH];
        rand::rngs::OsRng
            .try_fill_bytes(&mut seed)
            .map_err(|source| SigningError::Entropy { source })?;
        let pair = Self::from_seed(&seed);
        seed.fill(0);
        Ok(pair)
    }

    /// Builds the key pair derived from a 32-byte private seed.
    pub fn from_seed(seed: &[u8; 32]) -> Self {
        Self {
            inner: ed25519_dalek::SigningKey::from_bytes(seed),
        }
    }

    /// Returns the public half.
    pub fn public_key(&self) -> PublicKey {
        PublicKey {
            inner: self.inner.verifying_key(),
        }
    }

    /// Returns the 32-byte private seed, for the secret store only.
    pub(crate) fn seed_bytes(&self) -> [u8; 32] {
        self.inner.to_bytes()
    }
}

impl fmt::Debug for SigningKeyPair {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SigningKeyPair")
            .field("public_key", &self.public_key().fingerprint())
            .finish_non_exhaustive()
    }
}

/// An Ed25519 public key.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct PublicKey {
    inner: ed25519_dalek::VerifyingKey,
}

impl PublicKey {
    /// Decodes a public key from its 32 raw bytes.
    ///
    /// # Errors
    ///
    /// Returns [`SigningError::InvalidKey`] when the bytes are not a valid
    /// point of the Ed25519 curve.
    pub fn from_bytes(bytes: &[u8; 32]) -> Result<Self, SigningError> {
        ed25519_dalek::VerifyingKey::from_bytes(bytes)
            .map(|inner| Self { inner })
            .map_err(|_| SigningError::InvalidKey {
                reason: "not an Ed25519 public key",
            })
    }

    /// Decodes a public key from the 64 hexadecimal characters
    /// [`Self::to_hex`] writes. Surrounding whitespace is ignored.
    ///
    /// # Errors
    ///
    /// Returns [`SigningError::InvalidKey`] when the text is not 64
    /// hexadecimal characters, or when they do not decode to a point of the
    /// Ed25519 curve.
    pub fn from_hex(text: &str) -> Result<Self, SigningError> {
        let mut bytes = [0u8; 32];
        hex::decode_to_slice(text.trim(), &mut bytes).map_err(|_| SigningError::InvalidKey {
            reason: "not 64 hexadecimal characters",
        })?;
        Self::from_bytes(&bytes)
    }

    /// Returns the 32 raw bytes of the key.
    pub fn to_bytes(&self) -> [u8; 32] {
        self.inner.to_bytes()
    }

    /// Returns the 32 raw key bytes as 64 lowercase hexadecimal characters,
    /// the form of an `org.pub` file.
    pub fn to_hex(&self) -> String {
        hex::encode(self.inner.to_bytes())
    }

    /// Returns the stable fingerprint of the key.
    ///
    /// The fingerprint is the SHA-256 digest of the 32 raw key bytes, written
    /// as 64 lowercase hexadecimal characters. It depends on nothing but the
    /// key, so it is the same on every machine and across releases.
    pub fn fingerprint(&self) -> String {
        hex::encode(sha256(self.inner.as_bytes()))
    }
}

impl fmt::Debug for PublicKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("PublicKey")
            .field(&self.fingerprint())
            .finish()
    }
}

/// A detached Ed25519 signature, 64 bytes.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Signature {
    bytes: [u8; ed25519_dalek::SIGNATURE_LENGTH],
}

impl Signature {
    /// Wraps 64 raw signature bytes.
    pub fn from_bytes(bytes: [u8; 64]) -> Self {
        Self { bytes }
    }

    /// Returns the 64 raw signature bytes.
    pub fn to_bytes(&self) -> [u8; 64] {
        self.bytes
    }
}

impl fmt::Debug for Signature {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Signature")
            .field(&hex::encode(self.bytes))
            .finish()
    }
}

/// The outcome of a verification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    /// The signature was made by the private half of the key over exactly
    /// this list.
    Valid,
    /// The signature does not hold for this list and this key.
    Invalid,
}

/// Signs the canonical encoding of `list` with `key` and returns a detached
/// signature.
///
/// Ed25519 is deterministic: the same list and the same key always give the
/// same signature, whatever the order in which the entries were added.
///
/// # Examples
///
/// ```
/// use apollia_signing::{sha256, sign, verify, DigestList, SigningKeyPair, Verdict};
///
/// let pair = SigningKeyPair::from_seed(&[1u8; 32]);
/// let mut list = DigestList::new();
/// list.insert("agent.py", sha256(b"print"))?;
/// let signature = sign(&list, &pair);
/// assert_eq!(verify(&list, &signature, &pair.public_key()), Verdict::Valid);
/// # Ok::<(), apollia_signing::SigningError>(())
/// ```
pub fn sign(list: &DigestList, key: &SigningKeyPair) -> Signature {
    let signature = key.inner.sign(&list.canonical_bytes());
    Signature {
        bytes: signature.to_bytes(),
    }
}

/// Checks that `signature` was made by the private half of `public_key` over
/// the canonical encoding of `list`.
///
/// Verification is strict: a non-canonical signature or a weak public key is
/// answered [`Verdict::Invalid`].
pub fn verify(list: &DigestList, signature: &Signature, public_key: &PublicKey) -> Verdict {
    let signature = ed25519_dalek::Signature::from_bytes(&signature.bytes);
    match public_key
        .inner
        .verify_strict(&list.canonical_bytes(), &signature)
    {
        Ok(()) => Verdict::Valid,
        Err(_) => Verdict::Invalid,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_list() -> DigestList {
        let mut list = DigestList::new();
        list.insert("agent.py", sha256(b"class Agent: ..."))
            .unwrap();
        list.insert("manifest.toml", sha256(b"name = 'demo'"))
            .unwrap();
        list.insert("skills/summarise.py", sha256(b"def run(): ..."))
            .unwrap();
        list
    }

    #[test]
    fn test_sign_then_verify_round_trip_is_valid() {
        // GIVEN a freshly generated key pair and a list
        let pair = SigningKeyPair::generate().unwrap();
        let list = sample_list();

        // WHEN the list is signed then verified with the public half
        let signature = sign(&list, &pair);
        let verdict = verify(&list, &signature, &pair.public_key());

        // THEN the signature holds
        assert_eq!(verdict, Verdict::Valid);
    }

    #[test]
    fn test_verify_modified_entry_is_invalid() {
        // GIVEN a signature over a list
        let pair = SigningKeyPair::from_seed(&[3u8; 32]);
        let signature = sign(&sample_list(), &pair);

        // WHEN one entry's digest changes
        let mut tampered = DigestList::new();
        tampered
            .insert("agent.py", sha256(b"class Agent: evil"))
            .unwrap();
        tampered
            .insert("manifest.toml", sha256(b"name = 'demo'"))
            .unwrap();
        tampered
            .insert("skills/summarise.py", sha256(b"def run(): ..."))
            .unwrap();
        let verdict = verify(&tampered, &signature, &pair.public_key());

        // THEN verification fails
        assert_eq!(verdict, Verdict::Invalid);
    }

    #[test]
    fn test_verify_added_entry_is_invalid() {
        // GIVEN a signature over a list
        let pair = SigningKeyPair::from_seed(&[3u8; 32]);
        let signature = sign(&sample_list(), &pair);

        // WHEN an entry is added
        let mut extended = sample_list();
        extended.insert("payload.sh", sha256(b"curl evil")).unwrap();
        let verdict = verify(&extended, &signature, &pair.public_key());

        // THEN verification fails
        assert_eq!(verdict, Verdict::Invalid);
    }

    #[test]
    fn test_verify_removed_entry_is_invalid() {
        // GIVEN a signature over a list
        let pair = SigningKeyPair::from_seed(&[3u8; 32]);
        let signature = sign(&sample_list(), &pair);

        // WHEN an entry is removed
        let mut reduced = DigestList::new();
        reduced
            .insert("agent.py", sha256(b"class Agent: ..."))
            .unwrap();
        reduced
            .insert("manifest.toml", sha256(b"name = 'demo'"))
            .unwrap();
        let verdict = verify(&reduced, &signature, &pair.public_key());

        // THEN verification fails
        assert_eq!(verdict, Verdict::Invalid);
    }

    #[test]
    fn test_verify_renamed_entry_is_invalid() {
        // GIVEN a signature over a list
        let pair = SigningKeyPair::from_seed(&[3u8; 32]);
        let signature = sign(&sample_list(), &pair);

        // WHEN an entry keeps its digest under another path
        let mut renamed = DigestList::new();
        renamed
            .insert("agent.py", sha256(b"class Agent: ..."))
            .unwrap();
        renamed
            .insert("manifest.toml", sha256(b"name = 'demo'"))
            .unwrap();
        renamed
            .insert("skills/other.py", sha256(b"def run(): ..."))
            .unwrap();
        let verdict = verify(&renamed, &signature, &pair.public_key());

        // THEN verification fails
        assert_eq!(verdict, Verdict::Invalid);
    }

    #[test]
    fn test_insertion_order_does_not_change_signature() {
        // GIVEN the same three entries added in two different orders
        let pair = SigningKeyPair::from_seed(&[5u8; 32]);
        let forward = sample_list();
        let mut backward = DigestList::new();
        backward
            .insert("skills/summarise.py", sha256(b"def run(): ..."))
            .unwrap();
        backward
            .insert("manifest.toml", sha256(b"name = 'demo'"))
            .unwrap();
        backward
            .insert("agent.py", sha256(b"class Agent: ..."))
            .unwrap();

        // WHEN both lists are signed with the same key
        let a = sign(&forward, &pair);
        let b = sign(&backward, &pair);

        // THEN the signatures are byte-identical
        assert_eq!(a, b);
    }

    #[test]
    fn test_verify_with_other_key_is_invalid() {
        // GIVEN a signature made by one key
        let signer = SigningKeyPair::from_seed(&[7u8; 32]);
        let other = SigningKeyPair::from_seed(&[8u8; 32]);
        let list = sample_list();
        let signature = sign(&list, &signer);

        // WHEN it is checked against another public key
        let verdict = verify(&list, &signature, &other.public_key());

        // THEN verification fails
        assert_eq!(verdict, Verdict::Invalid);
    }

    #[test]
    fn test_fingerprint_is_sha256_of_public_key_bytes() {
        // GIVEN a key pair from a fixed seed
        let public_key = SigningKeyPair::from_seed(&[9u8; 32]).public_key();

        // WHEN its fingerprint is computed twice, once through a byte round trip
        let direct = public_key.fingerprint();
        let decoded = PublicKey::from_bytes(&public_key.to_bytes())
            .unwrap()
            .fingerprint();

        // THEN both equal the hex SHA-256 of the raw key bytes
        assert_eq!(direct, hex::encode(sha256(&public_key.to_bytes())));
        assert_eq!(direct, decoded);
    }

    #[test]
    fn test_public_key_from_invalid_bytes_is_refused() {
        // GIVEN 32 bytes that do not decode to a curve point
        let mut bytes = [0u8; 32];
        bytes[31] = 0x80;
        bytes[0] = 0x02;

        // WHEN they are decoded as a public key
        let result = PublicKey::from_bytes(&bytes);

        // THEN the key is refused
        assert!(matches!(result, Err(SigningError::InvalidKey { .. })));
    }

    #[test]
    fn test_public_key_round_trips_through_hex() {
        // GIVEN a public key written as hex, with a trailing line feed
        let public_key = SigningKeyPair::from_seed(&[5u8; 32]).public_key();
        let text = format!("{}\n", public_key.to_hex());

        // WHEN it is read back
        let back = PublicKey::from_hex(&text).unwrap();

        // THEN it is the same key
        assert_eq!(back, public_key);
    }

    #[test]
    fn test_public_key_from_hex_refuses_a_short_or_non_hex_text() {
        // GIVEN a text too short and a text of the right length that is not hex
        let short = "abcd";
        let not_hex = "z".repeat(64);

        // WHEN each is read as a public key
        let results = [PublicKey::from_hex(short), PublicKey::from_hex(&not_hex)];

        // THEN both are refused
        for result in results {
            assert!(matches!(result, Err(SigningError::InvalidKey { .. })));
        }
    }

    #[test]
    fn test_debug_output_hides_private_seed() {
        // GIVEN a key pair from a recognisable seed
        let pair = SigningKeyPair::from_seed(&[0xab; 32]);

        // WHEN it is formatted for debugging
        let rendered = format!("{pair:?}");

        // THEN the seed does not appear
        assert!(!rendered.contains(&hex::encode([0xab; 32])));
        assert!(rendered.contains(&pair.public_key().fingerprint()));
    }
}
