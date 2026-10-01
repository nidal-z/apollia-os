//! The canonical list of `(path, SHA-256)` entries that a signature covers.

use std::collections::BTreeMap;

use sha2::{Digest, Sha256};

use crate::error::SigningError;

/// Domain separator written before the entries, so a signature over a digest
/// list can never be replayed as a signature over another kind of message.
const CANONICAL_MAGIC: &[u8] = b"apollia-digest-list\0v1\0";

/// Returns the SHA-256 digest of `bytes`.
///
/// # Examples
///
/// ```
/// let digest = apollia_signing::sha256(b"abc");
/// assert_eq!(digest[0], 0xba);
/// ```
pub fn sha256(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

/// A set of `(path, SHA-256)` entries in canonical order.
///
/// Entries are kept sorted by the bytes of their path, whatever the insertion
/// order, so two lists holding the same entries always encode to the same
/// bytes and therefore carry the same signature. A path appears at most once.
///
/// # Canonical encoding
///
/// [`DigestList::canonical_bytes`] is the message that is signed:
///
/// ```text
/// "apollia-digest-list" 0x00 "v1" 0x00
/// entry count, u64 big-endian
/// for each entry, by ascending path bytes:
///     path length in bytes, u32 big-endian
///     path, UTF-8
///     SHA-256 digest, 32 bytes
/// ```
///
/// Length prefixes make the encoding unambiguous for any path, including one
/// that contains a newline or a separator.
///
/// # Examples
///
/// ```
/// use apollia_signing::{sha256, DigestList};
///
/// let mut list = DigestList::new();
/// list.insert("agent.py", sha256(b"print"))?;
/// list.insert("README.md", sha256(b"doc"))?;
/// assert_eq!(list.len(), 2);
/// # Ok::<(), apollia_signing::SigningError>(())
/// ```
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DigestList {
    entries: BTreeMap<String, [u8; 32]>,
}

impl DigestList {
    /// Returns an empty list.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds the entry `(path, digest)`.
    ///
    /// # Errors
    ///
    /// Returns [`SigningError::InvalidPath`] when `path` is empty or contains
    /// a NUL byte or is longer than `u32::MAX` bytes, and
    /// [`SigningError::DuplicatePath`] when `path` is already in the list.
    pub fn insert(
        &mut self,
        path: impl Into<String>,
        digest: [u8; 32],
    ) -> Result<(), SigningError> {
        let path = path.into();
        if path.is_empty() {
            return Err(SigningError::InvalidPath {
                reason: "empty path",
            });
        }
        if path.as_bytes().contains(&0) {
            return Err(SigningError::InvalidPath {
                reason: "path contains a NUL byte",
            });
        }
        if u32::try_from(path.len()).is_err() {
            return Err(SigningError::InvalidPath {
                reason: "path longer than u32::MAX bytes",
            });
        }
        if self.entries.contains_key(&path) {
            return Err(SigningError::DuplicatePath { path });
        }
        self.entries.insert(path, digest);
        Ok(())
    }

    /// Returns the number of entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns `true` when the list holds no entry.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Returns the entries in canonical order.
    pub fn entries(&self) -> impl Iterator<Item = (&str, &[u8; 32])> {
        self.entries
            .iter()
            .map(|(path, digest)| (path.as_str(), digest))
    }

    /// Returns the canonical encoding of the list, the message a signature covers.
    pub fn canonical_bytes(&self) -> Vec<u8> {
        let payload: usize = self.entries.keys().map(|p| 4 + p.len() + 32).sum();
        let mut out = Vec::with_capacity(CANONICAL_MAGIC.len() + 8 + payload);
        out.extend_from_slice(CANONICAL_MAGIC);
        out.extend_from_slice(&(self.entries.len() as u64).to_be_bytes());
        for (path, digest) in &self.entries {
            // `insert` refused every path whose length does not fit in a u32.
            let len = u32::try_from(path.len()).unwrap_or(u32::MAX);
            out.extend_from_slice(&len.to_be_bytes());
            out.extend_from_slice(path.as_bytes());
            out.extend_from_slice(digest);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_insert_empty_path_is_refused() {
        // GIVEN an empty list
        let mut list = DigestList::new();

        // WHEN an entry with an empty path is added
        let result = list.insert("", sha256(b"x"));

        // THEN it is refused as an invalid path
        assert!(matches!(result, Err(SigningError::InvalidPath { .. })));
    }

    #[test]
    fn test_insert_nul_byte_path_is_refused() {
        // GIVEN an empty list
        let mut list = DigestList::new();

        // WHEN an entry whose path contains a NUL byte is added
        let result = list.insert("a\0b", sha256(b"x"));

        // THEN it is refused as an invalid path
        assert!(matches!(result, Err(SigningError::InvalidPath { .. })));
    }

    #[test]
    fn test_insert_duplicate_path_is_refused() {
        // GIVEN a list that already holds `agent.py`
        let mut list = DigestList::new();
        list.insert("agent.py", sha256(b"one")).unwrap();

        // WHEN `agent.py` is added again with another digest
        let result = list.insert("agent.py", sha256(b"two"));

        // THEN it is refused and the first digest is kept
        assert!(matches!(result, Err(SigningError::DuplicatePath { .. })));
        assert_eq!(list.entries().next(), Some(("agent.py", &sha256(b"one"))));
    }

    #[test]
    fn test_entries_sorted_by_path_bytes() {
        // GIVEN entries added in an arbitrary order, upper case included
        let mut list = DigestList::new();
        for path in ["b.py", "a/z.py", "B.py", "a.py"] {
            list.insert(path, sha256(path.as_bytes())).unwrap();
        }

        // WHEN the entries are read back
        let paths: Vec<&str> = list.entries().map(|(p, _)| p).collect();

        // THEN they come in byte order, where upper case sorts first
        assert_eq!(paths, vec!["B.py", "a.py", "a/z.py", "b.py"]);
    }

    #[test]
    fn test_canonical_bytes_layout() {
        // GIVEN a list with one entry
        let mut list = DigestList::new();
        list.insert("ab", [7u8; 32]).unwrap();

        // WHEN it is encoded
        let bytes = list.canonical_bytes();

        // THEN the encoding is magic, count, length, path, digest
        let mut expected = b"apollia-digest-list\0v1\0".to_vec();
        expected.extend_from_slice(&1u64.to_be_bytes());
        expected.extend_from_slice(&2u32.to_be_bytes());
        expected.extend_from_slice(b"ab");
        expected.extend_from_slice(&[7u8; 32]);
        assert_eq!(bytes, expected);
    }
}
