//! The language data of optical character recognition, on this machine.
//!
//! The files the registry pins ([`OcrCatalogue`]) live in one folder,
//! `tessdata` under the models directory, each named `<language>.traineddata`.
//! A file counts only when its SHA-256 is the pinned one
//! ([`verified_languages`]). The files that are missing are fetched from the
//! registry and checked against the same digest ([`fetch_missing`]); on a
//! machine without the network, a copy brought on a disk enters the folder the
//! same way ([`import_language_data`]). The format has no header to check, so
//! the digest is the whole of the check: a file the registry does not pin is
//! refused.

use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::recommend::{OcrCatalogue, OcrLanguage};

/// Name of the folder holding the language data, under the models directory.
pub const TESSDATA_DIR: &str = "tessdata";

/// Extension of a language data file.
pub const LANGUAGE_DATA_EXTENSION: &str = "traineddata";

/// Address the language data is fetched from.
#[cfg(feature = "cloud")]
pub const REGISTRY_BASE: &str = "https://huggingface.co";

/// Why language data could not be brought into the folder.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum OcrDataError {
    /// The file is none of the files the registry pins.
    #[error(
        "{path} is not language data this build reads: its SHA-256 matches none the registry pins"
    )]
    UnknownDigest {
        /// Rejected file.
        path: PathBuf,
    },
    /// A file or folder could not be read or written.
    #[error("cannot read or write {path}")]
    Io {
        /// Path involved.
        path: PathBuf,
        /// Underlying failure.
        #[source]
        source: std::io::Error,
    },
    /// The download failed, or the file fetched did not have the pinned digest.
    #[cfg(feature = "cloud")]
    #[error(transparent)]
    Download(#[from] crate::downloader::DownloadError),
}

fn io_error(path: &Path) -> impl FnOnce(std::io::Error) -> OcrDataError + '_ {
    move |source| OcrDataError::Io {
        path: path.to_path_buf(),
        source,
    }
}

/// The folder of the language data under `models_dir`.
#[must_use]
pub fn tessdata_dir(models_dir: &Path) -> PathBuf {
    models_dir.join(TESSDATA_DIR)
}

fn sha256_of(path: &Path) -> std::io::Result<String> {
    Ok(format!("{:x}", Sha256::digest(std::fs::read(path)?)))
}

fn is_verified(dir: &Path, entry: &OcrLanguage) -> bool {
    sha256_of(&dir.join(entry.file_name())).is_ok_and(|d| d.eq_ignore_ascii_case(&entry.sha256))
}

/// The languages of `catalogue` whose file is in `dir` with the pinned
/// digest, in catalogue order. A file that is missing, unreadable or altered
/// does not count.
#[must_use]
pub fn verified_languages(dir: &Path, catalogue: &OcrCatalogue) -> Vec<String> {
    catalogue
        .languages()
        .iter()
        .filter(|entry| is_verified(dir, entry))
        .map(|entry| entry.language.clone())
        .collect()
}

/// Copies the language data file at `source` into `dir`, under the name the
/// engine looks it up by, when its digest is one `catalogue` pins. Returns
/// where it was written. The source is left in place.
///
/// # Errors
/// [`OcrDataError::UnknownDigest`] for a file the registry does not pin, and
/// [`OcrDataError::Io`] when it cannot be read or copied.
pub fn import_language_data(
    source: &Path,
    dir: &Path,
    catalogue: &OcrCatalogue,
) -> Result<PathBuf, OcrDataError> {
    let digest = sha256_of(source).map_err(io_error(source))?;
    let Some(entry) = catalogue.by_sha256(&digest) else {
        tracing::event!(
            tracing::Level::WARN,
            path = %source.display(),
            sha256 = %digest,
            "ocr.data.import.unknown_digest"
        );
        return Err(OcrDataError::UnknownDigest {
            path: source.to_path_buf(),
        });
    };
    std::fs::create_dir_all(dir).map_err(io_error(dir))?;
    let dest = dir.join(entry.file_name());
    if dest != source {
        std::fs::copy(source, &dest).map_err(io_error(&dest))?;
    }
    tracing::event!(
        tracing::Level::INFO,
        language = %entry.language,
        dest = %dest.display(),
        "ocr.data.imported"
    );
    Ok(dest)
}

/// Fetches into `dir` every file of `catalogue` that is not already there
/// with its pinned digest, from the registry served at `base`
/// ([`REGISTRY_BASE`] outside a test), each checked against that digest by
/// the downloader, which deletes a file that does not match. Returns the
/// languages fetched.
///
/// # Errors
/// [`OcrDataError::Download`] on the first download that fails, and
/// [`OcrDataError::Io`] when the folder cannot be prepared.
#[cfg(feature = "cloud")]
pub async fn fetch_missing(
    dir: &Path,
    catalogue: &OcrCatalogue,
    manager: &crate::downloader::DownloadManager,
    base: &str,
) -> Result<Vec<String>, OcrDataError> {
    std::fs::create_dir_all(dir).map_err(io_error(dir))?;
    let mut fetched = Vec::new();
    for entry in catalogue.languages() {
        if is_verified(dir, entry) {
            continue;
        }
        // An altered or partial file of that name is not resumed from.
        let dest = dir.join(entry.file_name());
        match std::fs::remove_file(&dest) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(io_error(&dest)(e)),
        }
        let request = crate::downloader::DownloadRequest {
            url: entry.download_url(base),
            dest_dir: dir.to_path_buf(),
            filename: Some(entry.file_name()),
            hf_token: None,
            repo_id: None,
            expected_sha256: Some(entry.sha256.clone()),
        };
        manager
            .download(request, std::sync::Arc::new(|_| {}))
            .await?;
        tracing::event!(
            tracing::Level::INFO,
            language = %entry.language,
            "ocr.data.fetched"
        );
        fetched.push(entry.language.clone());
    }
    Ok(fetched)
}

#[cfg(test)]
mod tests;
