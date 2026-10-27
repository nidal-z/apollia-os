//! One indexing pass over a part of a source, compared with the disk.
//!
//! A pass lists the files under its scope, reads the index's record of the
//! same part, and settles the difference: a file the index lacks, or whose
//! modification date and then fingerprint changed, is read, cut and written;
//! a file whose date changed but whose content did not only has its date
//! updated; a document whose file is gone leaves the index, and a pass over
//! the whole root drops every document outside it. A document with
//! pages waiting for optical character recognition is read again once it is
//! available. A file over
//! [`MAX_INDEXED_FILE_BYTES`], or one no reader opens, is recorded as
//! skipped. The same pass serves the first indexing, a restart (the disk is
//! compared with what the index holds) and a change the watcher reported.
//!
//! Everything here blocks: it runs under `spawn_blocking`, and the vectors
//! are asked of the embedder on the runtime handle it is given.

use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::path::{Path, PathBuf};

use apollia_llm::embeddings_client::EmbeddingError;
use sha2::{Digest, Sha256};

use crate::chunk::chunk_document;
use crate::index::{
    ChunkVectors, DocumentRecord, IndexError, SkipReason, SkippedFile, SourceIndex, UnreadReason,
};
use crate::indexer::extract::{is_indexed_format, sections_of};
use crate::ocr::Ocr;

/// Largest file indexed, in bytes (50 MiB). A larger one is skipped and
/// listed in the state of its source.
pub const MAX_INDEXED_FILE_BYTES: u64 = 50 * 1024 * 1024;

/// A file found on the disk.
#[derive(Debug, Clone)]
struct DiskFile {
    path: PathBuf,
    key: String,
    size: u64,
    modified_at: String,
}

/// What a pass did.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct PassOutcome {
    /// Documents read and written.
    pub indexed: usize,
    /// Documents whose date alone changed.
    pub touched: usize,
    /// Documents removed, their file gone.
    pub removed: usize,
    /// Files recorded as skipped.
    pub skipped: usize,
    /// Files whose vectors could not be computed; they keep their earlier
    /// version and are tried again at the next pass.
    pub failed: usize,
    /// Whether the index was emptied first, the model of its vectors having
    /// changed.
    pub rebuilt: bool,
    /// Pages of the index that wait for optical character recognition after
    /// the pass: scanned pages and pictures, on a machine where it is not
    /// available.
    pub awaiting_ocr: usize,
}

/// Computes the vectors of passages, on the caller's behalf.
pub(super) type Embed<'a> = dyn Fn(&[String]) -> Result<ChunkVectors, EmbeddingError> + 'a;

/// What a pass works with: the model it embeds with, `None` for keywords
/// alone, how it embeds, and what it reads scanned pages and pictures with.
pub(super) struct Means<'a, 'b> {
    pub(super) model: Option<&'a str>,
    pub(super) embed: &'a Embed<'b>,
    pub(super) ocr: &'a Ocr,
}

/// The means of a pass, and the documents whose pages wait for recognition.
struct Reading<'a, 'b, 'c> {
    means: &'c Means<'a, 'b>,
    waiting: HashSet<String>,
}

/// The index's key for a path: its text form.
fn key_of(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

/// Whether `key` is `scope` or lies under it.
pub(crate) fn is_under(key: &str, scope: &str) -> bool {
    key == scope
        || key
            .strip_prefix(scope)
            .is_some_and(|rest| rest.starts_with(std::path::MAIN_SEPARATOR))
}

/// Whether the entry name is hidden (`.git`, `.DS_Store`, an editor's swap
/// file): such entries are never indexed.
pub(super) fn is_hidden(path: &Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| n.starts_with('.'))
}

fn modified_at(meta: &std::fs::Metadata) -> String {
    meta.modified()
        .map(|t| {
            chrono::DateTime::<chrono::Utc>::from(t)
                .to_rfc3339_opts(chrono::SecondsFormat::Nanos, true)
        })
        .unwrap_or_default()
}

/// The files under `scope`: the file itself, or every file of the folder,
/// recursively. Hidden entries and symbolic links are left out, so a pass
/// never leaves the folder it was given.
fn walk(scope: &Path) -> Vec<DiskFile> {
    let mut out = Vec::new();
    let mut stack = vec![scope.to_path_buf()];
    while let Some(path) = stack.pop() {
        let Ok(meta) = std::fs::symlink_metadata(&path) else {
            continue;
        };
        if meta.file_type().is_symlink() || (path != scope && is_hidden(&path)) {
            continue;
        }
        if meta.is_dir() {
            match std::fs::read_dir(&path) {
                Ok(entries) => stack.extend(entries.filter_map(|e| e.ok().map(|e| e.path()))),
                Err(e) => tracing::event!(
                    tracing::Level::WARN,
                    path = %path.display(),
                    error = %e,
                    "sources.indexer.folder_unreadable"
                ),
            }
        } else if meta.is_file() && is_indexed_format(&path) {
            out.push(DiskFile {
                key: key_of(&path),
                size: meta.len(),
                modified_at: modified_at(&meta),
                path,
            });
        }
    }
    out
}

/// A fingerprint of the content of the file at `path`: its SHA-256, in hex.
fn fingerprint(path: &Path) -> std::io::Result<String> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect())
}

/// Runs one pass over `scopes` of the index at `index` with `means`: the
/// vectors computed by its embedder with its model, or none without a model,
/// the scanned pages and the pictures read by its recognition.
///
/// When the index holds vectors of another model than `model`, or none
/// while `model` is set, it is emptied first and the pass covers the whole
/// of `root`: two models' vectors are never mixed.
///
/// # Errors
/// [`IndexError`] when the index cannot be read or written. A file that
/// cannot be read is recorded as skipped, not returned.
pub(super) fn run(
    index: &mut SourceIndex,
    root: &Path,
    scopes: &[PathBuf],
    means: &Means<'_, '_>,
) -> Result<PassOutcome, IndexError> {
    let model = means.model;
    let mut outcome = PassOutcome::default();
    let (documents, _) = index.counts()?;
    let stored = index.embedding_model()?;
    let full = if model.is_some() && stored.as_deref() != model && documents > 0 {
        index.clear()?;
        outcome.rebuilt = true;
        vec![root.to_path_buf()]
    } else {
        scopes.to_vec()
    };
    let known: HashMap<String, DocumentRecord> = index
        .documents()?
        .into_iter()
        .map(|d| (d.path.clone(), d))
        .collect();
    let skipped: HashSet<String> = index.skipped()?.into_iter().map(|s| s.path).collect();
    let reading = Reading {
        means,
        waiting: index
            .unread_pages()?
            .into_iter()
            .filter(|page| page.reason == UnreadReason::OcrUnavailable)
            .map(|page| page.path)
            .collect(),
    };
    for scope in &full {
        let scope_key = key_of(scope);
        // A pass over the whole root also drops what lies outside it: the
        // documents of a folder the source named before.
        let whole = scope.as_path() == root;
        let on_disk = walk(scope);
        let present: HashSet<&str> = on_disk.iter().map(|f| f.key.as_str()).collect();
        let settled =
            |k: &String| (whole || is_under(k, &scope_key)) && !present.contains(k.as_str());
        for gone in known.keys().filter(|k| settled(k)) {
            index.remove_document(gone)?;
            outcome.removed += 1;
        }
        for gone in skipped.iter().filter(|k| settled(k)) {
            index.forget_skipped(gone)?;
        }
        for file in &on_disk {
            settle(index, file, known.get(&file.key), &reading, &mut outcome)?;
        }
    }
    outcome.awaiting_ocr = index
        .unread_pages()?
        .iter()
        .filter(|page| page.reason == UnreadReason::OcrUnavailable)
        .count();
    Ok(outcome)
}

/// Brings the index's record of one file in line with the disk.
fn settle(
    index: &mut SourceIndex,
    file: &DiskFile,
    known: Option<&DocumentRecord>,
    reading: &Reading<'_, '_, '_>,
    outcome: &mut PassOutcome,
) -> Result<(), IndexError> {
    if file.size > MAX_INDEXED_FILE_BYTES {
        index.remove_document(&file.key)?;
        index.record_skipped(&SkippedFile {
            path: file.key.clone(),
            size_bytes: file.size,
            reason: SkipReason::TooLarge,
        })?;
        outcome.skipped += 1;
        tracing::event!(
            tracing::Level::INFO,
            source = %index.source_id(),
            size_bytes = file.size,
            "sources.indexer.file_too_large"
        );
        return Ok(());
    }
    // A document whose pages waited for recognition is read again, unchanged
    // as it is, once recognition is there.
    let vectors = reading.means;
    let read_again = vectors.ocr.is_available() && reading.waiting.contains(&file.key);
    if !read_again && known.is_some_and(|k| k.modified_at == file.modified_at) {
        return Ok(());
    }
    let Ok(print) = fingerprint(&file.path) else {
        return unreadable(index, file, outcome);
    };
    if let Some(known) = known.filter(|k| !read_again && k.fingerprint == print) {
        index.touch_document(&known.path, &file.modified_at)?;
        outcome.touched += 1;
        return Ok(());
    }
    let extracted = match sections_of(&file.path, vectors.ocr) {
        Ok(Some(extracted)) => extracted,
        Ok(None) => return Ok(()),
        Err(e) => {
            tracing::event!(
                tracing::Level::INFO,
                source = %index.source_id(),
                error = %e,
                "sources.indexer.file_unreadable"
            );
            return unreadable(index, file, outcome);
        }
    };
    let chunks = chunk_document(&extracted.sections);
    let embedded = match vectors.model {
        Some(_) if !chunks.is_empty() => {
            let texts: Vec<String> = chunks.iter().map(|c| c.text.clone()).collect();
            match (vectors.embed)(&texts) {
                Ok(vectors) => Some(vectors),
                Err(e) if e.falls_back_to_keywords() => None,
                Err(e) => {
                    tracing::event!(
                        tracing::Level::WARN,
                        source = %index.source_id(),
                        error = %e,
                        "sources.indexer.embed_failed"
                    );
                    outcome.failed += 1;
                    return Ok(());
                }
            }
        }
        _ => None,
    };
    let record = DocumentRecord {
        path: file.key.clone(),
        fingerprint: print,
        modified_at: file.modified_at.clone(),
    };
    index.upsert_document(&record, &chunks, embedded.as_ref())?;
    index.set_unread_pages(&file.key, &extracted.unread)?;
    index.forget_skipped(&file.key)?;
    outcome.indexed += 1;
    Ok(())
}

/// Records a file no reader opened, and drops any earlier version of it.
fn unreadable(
    index: &mut SourceIndex,
    file: &DiskFile,
    outcome: &mut PassOutcome,
) -> Result<(), IndexError> {
    index.remove_document(&file.key)?;
    index.record_skipped(&SkippedFile {
        path: file.key.clone(),
        size_bytes: file.size,
        reason: SkipReason::Unreadable,
    })?;
    outcome.skipped += 1;
    Ok(())
}
