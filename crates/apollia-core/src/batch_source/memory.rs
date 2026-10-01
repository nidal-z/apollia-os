//! A source held in memory, readable and writable, for the tests of the
//! crates that consume the batch source contract.

use std::collections::BTreeMap;
use std::path::PathBuf;

use chrono::{DateTime, Utc};

use crate::batch_source::{
    dated_backup_path, Backup, BatchSource, OriginRef, SourceError, SourceItem, SourceItems,
    SourceWriter,
};

/// An in-memory source. It stands for a file at a nominal path: its backups
/// are recorded under the dated sibling of that path, nothing touches disk.
#[derive(Debug, Clone)]
pub struct MemorySource {
    nominal_path: PathBuf,
    fields: Vec<String>,
    items: Vec<SourceItem>,
    writable: bool,
    backup_fails: bool,
    backups: Vec<PathBuf>,
}

impl MemorySource {
    /// A writable source standing for `nominal_path`, with `fields` and no
    /// item yet.
    pub fn new(nominal_path: impl Into<PathBuf>, fields: &[&str]) -> Self {
        Self {
            nominal_path: nominal_path.into(),
            fields: fields.iter().map(|f| (*f).to_owned()).collect(),
            items: Vec::new(),
            writable: true,
            backup_fails: false,
            backups: Vec::new(),
        }
    }

    // TEST-ONLY: builds the fixture items of a test source; production
    // sources yield the items their reader parses from the origin.
    /// Appends an item at `origin` with `values`, given as `(field, value)`.
    pub fn with_item(mut self, origin: &str, values: &[(&str, &str)]) -> Self {
        self.items.push(SourceItem {
            origin: OriginRef::new(origin),
            fields: values
                .iter()
                .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
                .collect::<BTreeMap<_, _>>(),
        });
        self
    }

    /// Makes the source read-only.
    pub fn read_only(mut self) -> Self {
        self.writable = false;
        self
    }

    // TEST-ONLY: lets a test prove the write contract refuses a failed
    // backup; a production source fails its backup on a real i/o error.
    /// Makes every backup attempt fail.
    pub fn with_failing_backup(mut self) -> Self {
        self.backup_fails = true;
        self
    }

    /// The backups taken so far, oldest first.
    pub fn backups(&self) -> &[PathBuf] {
        &self.backups
    }

    /// The value of `column` for the item at `origin`.
    pub fn value(&self, origin: &str, column: &str) -> Option<&str> {
        self.items
            .iter()
            .find(|item| item.origin.as_str() == origin)
            .and_then(|item| item.fields.get(column))
            .map(String::as_str)
    }
}

impl BatchSource for MemorySource {
    fn fields(&self) -> &[String] {
        &self.fields
    }

    fn items(&self) -> Result<SourceItems<'_>, SourceError> {
        Ok(SourceItems {
            estimated_count: self.items.len() as u64,
            items: Box::new(self.items.iter().cloned().map(Ok)),
        })
    }

    fn writer(&mut self) -> Option<&mut dyn SourceWriter> {
        if self.writable {
            Some(self)
        } else {
            None
        }
    }
}

impl SourceWriter for MemorySource {
    fn back_up(&mut self, taken_at: DateTime<Utc>) -> Result<PathBuf, SourceError> {
        let path = dated_backup_path(&self.nominal_path, taken_at)?;
        if self.backup_fails {
            return Err(SourceError::Io {
                path,
                source: std::io::Error::other("backup refused by the test source"),
            });
        }
        self.backups.push(path.clone());
        Ok(path)
    }

    fn write_value(
        &mut self,
        _backup: &Backup,
        origin: &OriginRef,
        column: &str,
        value: &str,
    ) -> Result<(), SourceError> {
        let item = self
            .items
            .iter_mut()
            .find(|item| &item.origin == origin)
            .ok_or_else(|| SourceError::UnknownOrigin {
                origin: origin.clone(),
            })?;
        item.fields.insert(column.to_owned(), value.to_owned());
        if !self.fields.iter().any(|f| f == column) {
            self.fields.push(column.to_owned());
        }
        Ok(())
    }
}
