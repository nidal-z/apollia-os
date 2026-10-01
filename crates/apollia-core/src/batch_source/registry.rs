//! The registry of the source kinds a batch can be launched on.

use std::collections::BTreeMap;

use crate::batch_source::{BatchSource, SourceError};

/// What a source is opened on: a location (a file path, a folder, a mailbox
/// connection) and the options its kind understands (a sheet, a header row).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SourceSpec {
    pub location: String,
    pub options: BTreeMap<String, String>,
}

/// Opens a source of one kind from a [`SourceSpec`].
pub type SourceFactory =
    Box<dyn Fn(&SourceSpec) -> Result<Box<dyn BatchSource>, SourceError> + Send + Sync>;

/// The available source kinds, by name. A new registry is empty: each reader
/// registers its kind where the runtime assembles them.
#[derive(Default)]
pub struct SourceRegistry {
    factories: BTreeMap<String, SourceFactory>,
}

impl std::fmt::Debug for SourceRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SourceRegistry")
            .field("kinds", &self.kinds())
            .finish()
    }
}

impl SourceRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a kind. A kind is registered once: a second registration under
    /// the same name is refused rather than silently replacing the first.
    pub fn register(
        &mut self,
        kind: impl Into<String>,
        factory: SourceFactory,
    ) -> Result<(), SourceError> {
        let kind = kind.into();
        if self.factories.contains_key(&kind) {
            return Err(SourceError::DuplicateKind { kind });
        }
        self.factories.insert(kind, factory);
        Ok(())
    }

    /// The registered kinds, sorted by name.
    pub fn kinds(&self) -> Vec<&str> {
        self.factories.keys().map(String::as_str).collect()
    }

    /// Opens a source of `kind` on `spec`.
    pub fn open(&self, kind: &str, spec: &SourceSpec) -> Result<Box<dyn BatchSource>, SourceError> {
        let factory = self
            .factories
            .get(kind)
            .ok_or_else(|| SourceError::UnknownKind {
                kind: kind.to_owned(),
            })?;
        factory(spec)
    }
}
