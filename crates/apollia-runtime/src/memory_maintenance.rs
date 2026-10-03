//! Upkeep of the agents' memory: clearing, purging, deleting one entry,
//! exporting and importing a namespace.
//!
//! The memory of an agent lives in one SQLite file per namespace under
//! `<data_dir>/memory/`. The desktop running the runtime in its own process
//! calls these functions directly; a client of the daemon reaches the same
//! functions through `/api/v1/memory/namespaces/{namespace}/...`. Both
//! therefore change the same files and refuse with the same words.

use std::path::{Path, PathBuf};

use apollia_memory::episodic::EpisodicMemory;
use apollia_memory::export::{ExportError, ImportMode, MemoryExport};
use apollia_memory::procedural::ProceduralMemory;
use apollia_memory::semantic::SemanticMemory;
use apollia_memory::store::{MemoryStore, MemoryStoreError};

/// Why a memory operation did not complete. The display text is what the
/// interface shows, the same whichever process ran the operation.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum MemoryMaintenanceError {
    /// The namespace is empty or would reach outside the memory directory.
    #[error("VALIDATION_ERROR: invalid memory namespace '{0}'")]
    InvalidNamespace(String),
    /// No file holds this namespace.
    #[error("namespace '{0}' not found")]
    NamespaceNotFound(String),
    /// The namespace file did not open.
    #[error("failed to open store: {0}")]
    Open(#[source] MemoryStoreError),
    /// One kind of memory refused to be cleared or purged.
    #[error("{operation} failed: {source}")]
    Operation {
        /// The step that failed, as the interface names it.
        operation: &'static str,
        /// What the memory store answered.
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },
    /// The entry could not be deleted.
    #[error("failed to delete entry: {0}")]
    Delete(#[source] MemoryStoreError),
    /// The namespace could not be exported.
    #[error("export failed: {0}")]
    Export(#[source] ExportError),
    /// The export could not be imported.
    #[error("import failed: {0}")]
    Import(#[source] ExportError),
    /// The blocking task did not complete.
    #[error("spawn_blocking failed: {0}")]
    Join(#[source] tokio::task::JoinError),
}

impl MemoryMaintenanceError {
    /// Whether the caller sent something the operation cannot accept, as
    /// opposed to the store failing.
    pub fn is_invalid_request(&self) -> bool {
        matches!(
            self,
            Self::InvalidNamespace(_) | Self::Import(ExportError::UnsupportedVersion(_))
        )
    }
}

/// The kinds of memory an operation reaches. Any value but the three kinds
/// reaches all of them, as the interface has always sent it.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Episodic,
    Semantic,
    Procedural,
    All,
}

impl Kind {
    fn parse(memory_type: Option<&str>) -> Self {
        match memory_type {
            Some("episodic") => Self::Episodic,
            Some("semantic") => Self::Semantic,
            Some("procedural") => Self::Procedural,
            _ => Self::All,
        }
    }

    fn reaches(self, kind: Self) -> bool {
        self == Self::All || self == kind
    }
}

/// The memory directory under the runtime data directory.
pub fn memory_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("memory")
}

/// Refuses a namespace that is empty or names a path rather than a file stem.
fn checked(namespace: &str) -> Result<(), MemoryMaintenanceError> {
    let unsafe_name =
        namespace.is_empty() || namespace.contains(['/', '\\', '\0']) || namespace.starts_with('.');
    if unsafe_name {
        return Err(MemoryMaintenanceError::InvalidNamespace(
            namespace.to_owned(),
        ));
    }
    Ok(())
}

fn db_path(memory_dir: &Path, namespace: &str) -> PathBuf {
    memory_dir.join(format!("{namespace}.db"))
}

fn step<E>(operation: &'static str) -> impl FnOnce(E) -> MemoryMaintenanceError
where
    E: std::error::Error + Send + Sync + 'static,
{
    move |source| MemoryMaintenanceError::Operation {
        operation,
        source: Box::new(source),
    }
}

/// Runs `operation` off the async threads, after checking the namespace.
async fn blocking<T, F>(namespace: &str, operation: F) -> Result<T, MemoryMaintenanceError>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, MemoryMaintenanceError> + Send + 'static,
{
    checked(namespace)?;
    tokio::task::spawn_blocking(operation)
        .await
        .map_err(MemoryMaintenanceError::Join)?
}

/// Deletes every entry of `namespace` of the kind `memory_type` names, and
/// returns how many were removed. A namespace without a file holds nothing.
///
/// # Errors
///
/// [`MemoryMaintenanceError::InvalidNamespace`], or the store's refusal.
pub async fn clear(
    memory_dir: PathBuf,
    namespace: String,
    memory_type: Option<String>,
) -> Result<u64, MemoryMaintenanceError> {
    let kind = Kind::parse(memory_type.as_deref());
    blocking(&namespace.clone(), move || {
        let path = db_path(&memory_dir, &namespace);
        if !path.exists() {
            return Ok(0);
        }
        let store = MemoryStore::open(&path).map_err(MemoryMaintenanceError::Open)?;
        let mut total = 0;
        if kind.reaches(Kind::Episodic) {
            total += store
                .clear_episodic(&namespace)
                .map_err(step("clear_episodic"))?;
        }
        if kind.reaches(Kind::Semantic) {
            total += store
                .clear_semantic(&namespace)
                .map_err(step("clear_semantic"))?;
        }
        if kind.reaches(Kind::Procedural) {
            total += store
                .clear_procedural(&namespace)
                .map_err(step("clear_procedural"))?;
        }
        Ok(total)
    })
    .await
}

/// Deletes the entries of `namespace` older than `older_than_days`, of the
/// kind `memory_type` names, and returns how many were removed.
///
/// # Errors
///
/// [`MemoryMaintenanceError::InvalidNamespace`], or the store's refusal.
pub async fn purge(
    memory_dir: PathBuf,
    namespace: String,
    older_than_days: u32,
    memory_type: Option<String>,
) -> Result<u64, MemoryMaintenanceError> {
    let kind = Kind::parse(memory_type.as_deref());
    blocking(&namespace.clone(), move || {
        let path = db_path(&memory_dir, &namespace);
        if !path.exists() {
            return Ok(0);
        }
        let store = MemoryStore::open(&path).map_err(MemoryMaintenanceError::Open)?;
        let mut total = 0;
        if kind.reaches(Kind::Episodic) {
            total += EpisodicMemory::new(&store)
                .purge_older_than(&namespace, older_than_days)
                .map_err(step("purge_episodic"))?;
        }
        if kind.reaches(Kind::Semantic) {
            total += SemanticMemory::new(&store)
                .purge_older_than(&namespace, older_than_days)
                .map_err(step("purge_semantic"))?;
        }
        if kind.reaches(Kind::Procedural) {
            total += ProceduralMemory::new(&store)
                .purge_older_than(&namespace, older_than_days)
                .map_err(step("purge_procedural"))?;
        }
        Ok(total)
    })
    .await
}

/// Deletes the entry `entry_id` of `namespace`, whatever its kind, and says
/// whether one was there.
///
/// # Errors
///
/// [`MemoryMaintenanceError::NamespaceNotFound`] when no file holds the
/// namespace, or the store's refusal.
pub async fn delete_entry(
    memory_dir: PathBuf,
    namespace: String,
    entry_id: String,
) -> Result<bool, MemoryMaintenanceError> {
    blocking(&namespace.clone(), move || {
        let path = db_path(&memory_dir, &namespace);
        if !path.exists() {
            return Err(MemoryMaintenanceError::NamespaceNotFound(namespace));
        }
        let store = MemoryStore::open(&path).map_err(MemoryMaintenanceError::Open)?;
        store
            .delete_entry_by_id(&entry_id)
            .map_err(MemoryMaintenanceError::Delete)
    })
    .await
}

/// Reads every entry of `namespace` into the export format.
///
/// # Errors
///
/// [`MemoryMaintenanceError::InvalidNamespace`], or
/// [`MemoryMaintenanceError::Export`] when the namespace cannot be read.
pub async fn export(
    memory_dir: PathBuf,
    namespace: String,
) -> Result<MemoryExport, MemoryMaintenanceError> {
    blocking(&namespace.clone(), move || {
        apollia_memory::export::export_namespace(&memory_dir, &namespace)
            .map_err(MemoryMaintenanceError::Export)
    })
    .await
}

/// Writes `content` into `namespace`, replacing what it held when `mode` is
/// `"replace"` and merging otherwise, and returns how many entries went in.
///
/// # Errors
///
/// [`MemoryMaintenanceError::InvalidNamespace`], or
/// [`MemoryMaintenanceError::Import`] for an unknown format version or a store
/// that refuses the write.
pub async fn import(
    memory_dir: PathBuf,
    namespace: String,
    content: MemoryExport,
    mode: &str,
) -> Result<usize, MemoryMaintenanceError> {
    let mode = if mode == "replace" {
        ImportMode::Replace
    } else {
        ImportMode::Merge
    };
    blocking(&namespace.clone(), move || {
        apollia_memory::export::import_namespace(&memory_dir, &namespace, &content, mode)
            .map_err(MemoryMaintenanceError::Import)
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_a_namespace_naming_a_path_is_refused_before_any_file_is_touched() {
        // GIVEN a memory directory and namespaces that would leave it
        let dir = tempfile::TempDir::new().unwrap();
        for namespace in ["", "../agents", "a/b", "a\\b", ".hidden"] {
            // WHEN each is cleared
            let refusal = clear(dir.path().to_path_buf(), namespace.to_owned(), None).await;

            // THEN it is refused as invalid, and the directory stays empty
            let error = refusal.expect_err(namespace);
            assert!(error.is_invalid_request(), "{namespace}: {error}");
        }
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    }

    #[tokio::test]
    async fn test_an_export_imported_into_another_namespace_carries_its_entries() {
        // GIVEN a namespace holding one fact
        let dir = tempfile::TempDir::new().unwrap();
        let store = MemoryStore::open(&dir.path().join("agent-a.db")).unwrap();
        SemanticMemory::new(&store)
            .remember(apollia_memory::semantic::RememberInput {
                namespace: "agent-a",
                key: "client.budget",
                value: &serde_json::json!(15000),
                confidence: 1.0,
                source: None,
                expires_at: None,
            })
            .unwrap();
        drop(store);

        // WHEN it is exported, then imported into another namespace
        let content = export(dir.path().to_path_buf(), "agent-a".to_owned())
            .await
            .unwrap();
        let imported = import(
            dir.path().to_path_buf(),
            "agent-b".to_owned(),
            content,
            "merge",
        )
        .await
        .unwrap();

        // THEN the fact went in, and clearing the copy removes exactly it
        assert_eq!(imported, 1);
        let removed = clear(dir.path().to_path_buf(), "agent-b".to_owned(), None)
            .await
            .unwrap();
        assert_eq!(removed, 1);
    }
}
