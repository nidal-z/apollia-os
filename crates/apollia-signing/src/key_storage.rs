//! Storage of the private signing key, through `SecretStore` only.
//!
//! The private seed is handed to the secret store as 64 lowercase hexadecimal
//! characters under service [`SIGNING_SECRET_SERVICE`] and a caller-chosen
//! label. This module never touches the filesystem: on the keyring backend the
//! seed lives in the operating system keychain, on the file backend it is
//! written encrypted by `age`.

use apollia_auth::SecretStore;

use crate::error::SigningError;
use crate::keys::SigningKeyPair;

/// `SecretStore` service namespace for private signing keys.
pub const SIGNING_SECRET_SERVICE: &str = "apollia-signing";

/// Label of the author key: the key `package build --sign` and the export of
/// an agent sign with, generated on first use. One author, one key, whichever
/// path signs.
pub const AUTHOR_KEY_LABEL: &str = "default";

/// Stores the private half of `key` in `store` under `label`.
///
/// An existing key under the same label is replaced.
///
/// # Errors
///
/// Returns [`SigningError::SecretStore`] when the backend refuses the write.
pub fn store_signing_key(
    store: &dyn SecretStore,
    label: &str,
    key: &SigningKeyPair,
) -> Result<(), SigningError> {
    let mut seed = key.seed_bytes();
    let encoded = hex::encode(seed);
    seed.fill(0);
    store
        .set(SIGNING_SECRET_SERVICE, label, &encoded)
        .map_err(|source| SigningError::SecretStore { source })
}

/// Reads back the key pair stored in `store` under `label`.
///
/// # Errors
///
/// Returns [`SigningError::KeyNotFound`] when nothing is stored under `label`,
/// [`SigningError::SecretStore`] when the backend fails, and
/// [`SigningError::InvalidKey`] when the stored value is not a 32-byte seed in
/// hexadecimal.
pub fn load_signing_key(
    store: &dyn SecretStore,
    label: &str,
) -> Result<SigningKeyPair, SigningError> {
    let encoded = store
        .get(SIGNING_SECRET_SERVICE, label)
        .map_err(|source| SigningError::SecretStore { source })?
        .ok_or_else(|| SigningError::KeyNotFound {
            label: label.to_owned(),
        })?;
    let mut seed = [0u8; 32];
    hex::decode_to_slice(encoded.trim(), &mut seed).map_err(|_| SigningError::InvalidKey {
        reason: "stored seed is not 32 bytes of hexadecimal",
    })?;
    let pair = SigningKeyPair::from_seed(&seed);
    seed.fill(0);
    Ok(pair)
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Mutex;

    use apollia_auth::{AgeFileSecretStore, AuthError};

    use super::*;

    /// An in-memory secret store that records every write.
    #[derive(Default)]
    struct MemoryStore {
        values: Mutex<HashMap<(String, String), String>>,
    }

    impl SecretStore for MemoryStore {
        fn set(&self, service: &str, user: &str, value: &str) -> Result<(), AuthError> {
            self.values
                .lock()
                .unwrap()
                .insert((service.to_owned(), user.to_owned()), value.to_owned());
            Ok(())
        }
        fn get(&self, service: &str, user: &str) -> Result<Option<String>, AuthError> {
            Ok(self
                .values
                .lock()
                .unwrap()
                .get(&(service.to_owned(), user.to_owned()))
                .cloned())
        }
        fn delete(&self, service: &str, user: &str) -> Result<(), AuthError> {
            self.values
                .lock()
                .unwrap()
                .remove(&(service.to_owned(), user.to_owned()));
            Ok(())
        }
        fn backend_id(&self) -> &'static str {
            "memory"
        }
    }

    /// A secret store whose backend is always unavailable.
    struct BrokenStore;

    impl SecretStore for BrokenStore {
        fn set(&self, _: &str, _: &str, _: &str) -> Result<(), AuthError> {
            Err(AuthError::Keyring("locked".into()))
        }
        fn get(&self, _: &str, _: &str) -> Result<Option<String>, AuthError> {
            Err(AuthError::Keyring("locked".into()))
        }
        fn delete(&self, _: &str, _: &str) -> Result<(), AuthError> {
            Err(AuthError::Keyring("locked".into()))
        }
        fn backend_id(&self) -> &'static str {
            "broken"
        }
    }

    #[test]
    fn test_store_then_load_returns_same_key() {
        // GIVEN a generated key pair and an empty store
        let store = MemoryStore::default();
        let pair = SigningKeyPair::generate().unwrap();

        // WHEN it is stored then read back under the same label
        store_signing_key(&store, "author", &pair).unwrap();
        let loaded = load_signing_key(&store, "author").unwrap();

        // THEN the public halves are identical and the value sits under the signing service
        assert_eq!(loaded.public_key(), pair.public_key());
        let values = store.values.lock().unwrap();
        assert_eq!(
            values.keys().cloned().collect::<Vec<_>>(),
            vec![(SIGNING_SECRET_SERVICE.to_owned(), "author".to_owned())]
        );
    }

    #[test]
    fn test_load_missing_label_is_key_not_found() {
        // GIVEN an empty store
        let store = MemoryStore::default();

        // WHEN a label that was never stored is loaded
        let result = load_signing_key(&store, "nobody");

        // THEN the error names the missing label
        assert!(matches!(result, Err(SigningError::KeyNotFound { label }) if label == "nobody"));
    }

    #[test]
    fn test_load_malformed_value_is_invalid_key() {
        // GIVEN a store holding a value that is not a hex seed
        let store = MemoryStore::default();
        store
            .set(SIGNING_SECRET_SERVICE, "author", "not-hex")
            .unwrap();

        // WHEN the key is loaded
        let result = load_signing_key(&store, "author");

        // THEN it is refused as invalid key material
        assert!(matches!(result, Err(SigningError::InvalidKey { .. })));
    }

    #[test]
    fn test_store_with_failing_backend_is_secret_store_error() {
        // GIVEN a store whose backend is unavailable
        let pair = SigningKeyPair::from_seed(&[1u8; 32]);

        // WHEN a key is stored and another is loaded
        let stored = store_signing_key(&BrokenStore, "author", &pair);
        let loaded = load_signing_key(&BrokenStore, "author");

        // THEN both report the backend failure
        assert!(matches!(stored, Err(SigningError::SecretStore { .. })));
        assert!(matches!(loaded, Err(SigningError::SecretStore { .. })));
    }

    #[test]
    fn test_file_backend_never_writes_seed_in_clear() {
        // GIVEN the encrypted file backend rooted in an empty directory
        let dir = tempfile::tempdir().unwrap();
        let store =
            AgeFileSecretStore::new(dir.path().to_path_buf(), "test passphrase".into()).unwrap();
        let pair = SigningKeyPair::generate().unwrap();
        let seed = pair.seed_bytes();

        // WHEN the key is stored through it
        store_signing_key(&store, "author", &pair).unwrap();

        // THEN no file under the directory holds the seed, raw or in hex
        let hex_seed = hex::encode(seed);
        let mut files = 0;
        for entry in std::fs::read_dir(dir.path()).unwrap() {
            let bytes = std::fs::read(entry.unwrap().path()).unwrap();
            files += 1;
            assert!(!bytes.windows(32).any(|w| w == seed));
            assert!(!bytes.windows(64).any(|w| w == hex_seed.as_bytes()));
        }
        assert_eq!(files, 1);
        // AND the key reads back intact
        let loaded = load_signing_key(&store, "author").unwrap();
        assert_eq!(loaded.public_key(), pair.public_key());
    }
}
