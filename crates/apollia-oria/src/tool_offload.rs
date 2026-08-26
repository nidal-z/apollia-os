//! Tier-1 microcompaction: offload oversized tool-results to disk.
//!
//! When a tool returns a voluminous payload (a long file read, a large command
//! output), keeping it verbatim in the conversation history monopolizes the LLM
//! context window. The [`ToolOffloadStore`] writes such payloads to a flat
//! directory of plain UTF-8 files, each named by an [`OffloadRef`], so the
//! history can carry a compact stub instead while the full content stays
//! recoverable on disk.
//!
//! The store is stateless: it does not track which refs exist. The caller owns
//! that mapping through the stubs it leaves in the conversation history.

use std::path::PathBuf;

use uuid::Uuid;

/// Opaque reference to an offloaded tool-result on disk.
///
/// Format: `offload-<uuid_v4>.txt`. Embedded in the inline stub
/// `[resultat deporte: <ref>, N lignes]` left in the conversation history.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OffloadRef(pub String);

impl OffloadRef {
    /// Generate a new unique reference (`offload-<uuid_v4>.txt`).
    pub fn new_unique() -> Self {
        Self(format!("offload-{}.txt", Uuid::new_v4()))
    }

    /// Return the filename component used on disk and in the stub.
    pub fn filename(&self) -> &str {
        &self.0
    }
}

/// Error writing an offloaded result to disk.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum OffloadError {
    /// I/O failure during the write.
    #[error("failed to write offload file '{path}': {source}")]
    Io {
        /// Target path that could not be written.
        path: PathBuf,
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
    },
}

/// Stores large tool-results on disk in a flat directory.
///
/// Each result is written as a plain UTF-8 file named by its [`OffloadRef`].
/// The directory must already exist; the store does not create it.
#[derive(Debug, Clone)]
pub struct ToolOffloadStore {
    /// Directory where offloaded files are written.
    dir: PathBuf,
}

impl ToolOffloadStore {
    /// Create a store backed by `dir`. The directory must already exist.
    pub fn new(dir: PathBuf) -> Self {
        Self { dir }
    }

    /// Write `content` to disk under `oref.filename()`.
    ///
    /// Overwrites if called twice with the same ref (idempotent on the ref).
    pub fn write(&self, oref: &OffloadRef, content: &str) -> Result<(), OffloadError> {
        let path = self.dir.join(oref.filename());
        std::fs::write(&path, content).map_err(|source| OffloadError::Io { path, source })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    // GIVEN two freshly generated refs
    // WHEN compared
    // THEN they differ (uniqueness invariant)
    #[test]
    fn test_offload_ref_unique() {
        let r1 = OffloadRef::new_unique();
        let r2 = OffloadRef::new_unique();
        assert_ne!(r1.0, r2.0);
        assert!(r1.filename().starts_with("offload-"));
        assert!(r1.filename().ends_with(".txt"));
    }

    // GIVEN a store over an existing directory
    // WHEN writing content under a ref
    // THEN the file exists and contains exactly that content
    #[test]
    fn test_store_write_roundtrip() {
        let dir = TempDir::new().expect("tempdir");
        let store = ToolOffloadStore::new(dir.path().to_path_buf());
        let oref = OffloadRef::new_unique();

        store.write(&oref, "payload").expect("write must succeed");

        let written = std::fs::read_to_string(dir.path().join(oref.filename()))
            .expect("offloaded file must exist");
        assert_eq!(written, "payload");
    }

    // GIVEN a store pointing at a non-existent directory
    // WHEN writing
    // THEN an OffloadError::Io is returned (error path)
    #[test]
    fn test_store_write_error_on_missing_dir() {
        let dir = TempDir::new().expect("tempdir");
        let store = ToolOffloadStore::new(dir.path().join("does-not-exist"));
        let oref = OffloadRef::new_unique();

        let err = store
            .write(&oref, "payload")
            .expect_err("write to missing dir must fail");
        assert!(matches!(err, OffloadError::Io { .. }));
    }
}
