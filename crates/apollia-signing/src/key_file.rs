//! A private signing key kept in a file, for the organisation key.
//!
//! The organisation key signs the policy IT deploys to every workstation. It
//! is not a user secret: it belongs to IT, which keeps it offline, backs it up
//! and moves it with its own tools, so it lives in a file IT chooses rather
//! than in the [`SecretStore`](apollia_auth::SecretStore) of one user.
//!
//! The file holds the 32-byte private seed as 64 lowercase hexadecimal
//! characters and a line feed. On Unix it is opened with the mode `0600` by
//! the call that creates it, so the seed is never readable by anyone else,
//! not even for the instant between a write and a later `chmod`. An existing
//! file is never replaced.

use std::io::Write as _;
use std::path::Path;

use crate::error::SigningError;
use crate::keys::SigningKeyPair;

/// Largest private key file read, in bytes: the seed, a line feed, and room
/// for a carriage return.
const MAX_KEY_FILE_BYTES: u64 = 128;

/// Writes the private half of `key` to a new file at `path`.
///
/// On Unix the file is created with the mode `0600`.
///
/// # Errors
///
/// Returns [`SigningError::KeyFile`] when the file already exists or cannot be
/// created or written.
pub fn write_private_key_file(path: &Path, key: &SigningKeyPair) -> Result<(), SigningError> {
    let io_error = |source| SigningError::KeyFile {
        path: path.to_path_buf(),
        source,
    };
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.mode(0o600);
    }
    let mut file = options.open(path).map_err(io_error)?;
    let mut seed = key.seed_bytes();
    let mut encoded = hex::encode(seed);
    seed.fill(0);
    encoded.push('\n');
    let written = file
        .write_all(encoded.as_bytes())
        .and_then(|()| file.sync_all());
    // The hexadecimal copy of the seed is overwritten before it drops.
    let wiped = "0".repeat(encoded.len());
    encoded.replace_range(.., &wiped);
    written.map_err(io_error)
}

/// Reads the key pair whose private seed is stored in the file at `path`.
///
/// # Errors
///
/// Returns [`SigningError::KeyFile`] when the file cannot be read, and
/// [`SigningError::InvalidKey`] when it does not hold a 32-byte seed in
/// hexadecimal.
pub fn read_private_key_file(path: &Path) -> Result<SigningKeyPair, SigningError> {
    let io_error = |source| SigningError::KeyFile {
        path: path.to_path_buf(),
        source,
    };
    let metadata = std::fs::metadata(path).map_err(io_error)?;
    if metadata.len() > MAX_KEY_FILE_BYTES {
        return Err(SigningError::InvalidKey {
            reason: "private key file is larger than a hexadecimal seed",
        });
    }
    let text = std::fs::read_to_string(path).map_err(io_error)?;
    let mut seed = [0u8; 32];
    hex::decode_to_slice(text.trim(), &mut seed).map_err(|_| SigningError::InvalidKey {
        reason: "private key file is not 32 bytes of hexadecimal",
    })?;
    let pair = SigningKeyPair::from_seed(&seed);
    seed.fill(0);
    Ok(pair)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_private_key_file_round_trips_the_key() {
        // GIVEN a key pair and a fresh folder
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("org.key");
        let pair = SigningKeyPair::from_seed(&[3u8; 32]);
        // WHEN it is written and read back
        write_private_key_file(&path, &pair).unwrap();
        let back = read_private_key_file(&path).unwrap();
        // THEN the same public key comes back
        assert_eq!(back.public_key(), pair.public_key());
    }

    #[cfg(unix)]
    #[test]
    fn test_private_key_file_is_created_with_mode_0600() {
        use std::os::unix::fs::PermissionsExt as _;
        // GIVEN a fresh folder
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("org.key");
        // WHEN a key is written there
        write_private_key_file(&path, &SigningKeyPair::from_seed(&[4u8; 32])).unwrap();
        // THEN only its owner can read or write it
        let mode = std::fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    #[test]
    fn test_private_key_file_never_replaces_an_existing_file() {
        // GIVEN a file already present where the key would go
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("org.key");
        std::fs::write(&path, "keep me").unwrap();
        // WHEN a key is written to that path
        let result = write_private_key_file(&path, &SigningKeyPair::from_seed(&[5u8; 32]));
        // THEN the write is refused and the file is untouched
        assert!(matches!(result, Err(SigningError::KeyFile { .. })));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "keep me");
    }

    #[test]
    fn test_private_key_file_refuses_what_is_not_a_seed() {
        // GIVEN a file holding something other than a hexadecimal seed
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("org.key");
        std::fs::write(&path, "not a key\n").unwrap();
        // WHEN it is read as a private key
        let result = read_private_key_file(&path);
        // THEN it is refused as invalid key material
        assert!(matches!(result, Err(SigningError::InvalidKey { .. })));
    }
}
