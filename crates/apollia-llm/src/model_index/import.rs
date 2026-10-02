//! What an import checks before a model enters the models directory.
//!
//! A file brought from a disk or a USB key has no download to vouch for it.
//! Its SHA-256 is compared with the digests already known on this machine (the
//! verified entries of the index, and a catalog when it carries digests). A
//! match means the origin is verified. Without a match, the import is still
//! accepted when the weights have the structure of a model, and the entry is
//! marked as being of unverified origin; [`ImportRule::require_verified`]
//! turns that acceptance into a refusal.
//!
//! The structure check reads a few bytes only, so it runs before the digest:
//! a text file renamed `.gguf` is refused without hashing gigabytes.

use std::io::Read;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use thiserror::Error;

use super::scan::{has_ggml_magic, is_safetensors};

/// The four bytes every GGUF file starts with.
const GGUF_MAGIC: [u8; 4] = *b"GGUF";

/// Read buffer of the digest, large enough to keep a disk busy.
const HASH_BUFFER_BYTES: usize = 1 << 20;

/// What the import may accept.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ImportRule {
    /// Refuse a model whose digest matches no known digest. Off by default.
    pub require_verified: bool,
}

/// The outcome of a check that accepted the model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportCheck {
    /// SHA-256 of the weights, lowercase hex (see [`weights_sha256`]).
    pub sha256: String,
    /// Whether the digest matched a known one.
    pub origin_verified: bool,
}

/// Why an import was refused.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum ImportCheckError {
    /// A `.gguf` file that does not start with the GGUF header.
    #[error("{path} is not a GGUF model: the file does not start with the GGUF header")]
    NotGguf {
        /// Rejected file.
        path: PathBuf,
    },
    /// A `.bin` file that carries neither the whisper.cpp nor the GGUF header.
    #[error(
        "{path} is not a whisper.cpp model: the file does not start with a GGML or GGUF header"
    )]
    NotGgml {
        /// Rejected file.
        path: PathBuf,
    },
    /// A file whose extension names no model format.
    #[error("{path} is not a model file: expected a .gguf or .bin file, or an MLX folder")]
    UnknownFormat {
        /// Rejected file.
        path: PathBuf,
    },
    /// A folder whose `config.json` is missing or is not a JSON object.
    #[error("{path} is not an MLX model: config.json is missing or unreadable")]
    MlxConfig {
        /// Rejected folder.
        path: PathBuf,
    },
    /// A folder with no `*.safetensors` weights file.
    #[error("{path} is not an MLX model: the folder holds no .safetensors weights file")]
    MlxNoWeights {
        /// Rejected folder.
        path: PathBuf,
    },
    /// The digest matched no known digest and the rule requires one.
    #[error("{path} is refused: its origin cannot be verified and verified imports are required")]
    Unverified {
        /// Rejected file or folder.
        path: PathBuf,
    },
    /// A file or folder could not be read.
    #[error("cannot read {path}")]
    Io {
        /// Path involved.
        path: PathBuf,
        /// Underlying failure.
        #[source]
        source: std::io::Error,
    },
}

fn io_error(path: &Path) -> impl FnOnce(std::io::Error) -> ImportCheckError + '_ {
    move |source| ImportCheckError::Io {
        path: path.to_path_buf(),
        source,
    }
}

/// Check that `source` has the structure of a model: a GGUF file starts with
/// the GGUF header, a whisper.cpp `.bin` with the GGML one (or the GGUF one),
/// and an MLX folder holds a readable `config.json` and at least one
/// `*.safetensors` file.
///
/// # Errors
/// One of the structure variants of [`ImportCheckError`], or
/// [`ImportCheckError::Io`] when `source` cannot be read.
pub fn check_structure(source: &Path) -> Result<(), ImportCheckError> {
    if source.is_dir() {
        return check_mlx_folder(source);
    }
    let extension = source
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();
    match extension.as_str() {
        "gguf" if has_gguf_magic(source).map_err(io_error(source))? => Ok(()),
        "gguf" => Err(ImportCheckError::NotGguf {
            path: source.to_path_buf(),
        }),
        "bin" if has_ggml_magic(source) || has_gguf_magic(source).map_err(io_error(source))? => {
            Ok(())
        }
        "bin" => Err(ImportCheckError::NotGgml {
            path: source.to_path_buf(),
        }),
        _ => Err(ImportCheckError::UnknownFormat {
            path: source.to_path_buf(),
        }),
    }
}

/// Whether the file starts with the GGUF header. A file shorter than the
/// header does not.
fn has_gguf_magic(path: &Path) -> std::io::Result<bool> {
    let mut head = [0u8; 4];
    let mut file = std::fs::File::open(path)?;
    match file.read_exact(&mut head) {
        Ok(()) => Ok(head == GGUF_MAGIC),
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => Ok(false),
        Err(e) => Err(e),
    }
}

fn check_mlx_folder(dir: &Path) -> Result<(), ImportCheckError> {
    let config_is_object = std::fs::read(dir.join("config.json"))
        .ok()
        .and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok())
        .is_some_and(|v| v.is_object());
    if !config_is_object {
        return Err(ImportCheckError::MlxConfig {
            path: dir.to_path_buf(),
        });
    }
    let has_weights = std::fs::read_dir(dir)
        .map_err(io_error(dir))?
        .flatten()
        .any(|e| is_safetensors(&e.path()) && e.path().is_file());
    if !has_weights {
        return Err(ImportCheckError::MlxNoWeights {
            path: dir.to_path_buf(),
        });
    }
    Ok(())
}

/// SHA-256 of the weights at `source`, lowercase hex.
///
/// For a file, the digest of its bytes, the figure a catalog publishes. For an
/// MLX folder, the digest of a manifest listing every file of the folder (its
/// path relative to the folder, with `/` separators, then the SHA-256 of its
/// bytes), sorted by path: the same folder gives the same digest wherever it
/// is copied.
///
/// # Errors
/// [`ImportCheckError::Io`] when a file cannot be read.
pub fn weights_sha256(source: &Path) -> Result<String, ImportCheckError> {
    if !source.is_dir() {
        return file_sha256(source);
    }
    let mut files = Vec::new();
    collect_files(source, source, &mut files)?;
    files.sort();
    let mut manifest = Sha256::new();
    for relative in files {
        let digest = file_sha256(&source.join(&relative))?;
        manifest.update(relative.as_bytes());
        manifest.update([0u8]);
        manifest.update(digest.as_bytes());
        manifest.update(b"\n");
    }
    Ok(format!("{:x}", manifest.finalize()))
}

fn file_sha256(path: &Path) -> Result<String, ImportCheckError> {
    let mut file = std::fs::File::open(path).map_err(io_error(path))?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; HASH_BUFFER_BYTES];
    loop {
        let read = file.read(&mut buffer).map_err(io_error(path))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

/// Every regular file under `dir`, as a path relative to `root` with `/`
/// separators. Hidden entries are skipped, as the index walk skips them.
fn collect_files(root: &Path, dir: &Path, out: &mut Vec<String>) -> Result<(), ImportCheckError> {
    for entry in std::fs::read_dir(dir).map_err(io_error(dir))?.flatten() {
        let path = entry.path();
        if entry.file_name().to_string_lossy().starts_with('.') {
            continue;
        }
        let kind = entry.file_type().map_err(io_error(&path))?;
        if kind.is_dir() {
            collect_files(root, &path, out)?;
        } else if kind.is_file() {
            let relative = path.strip_prefix(root).unwrap_or(&path);
            let parts: Vec<String> = relative
                .components()
                .map(|c| c.as_os_str().to_string_lossy().into_owned())
                .collect();
            out.push(parts.join("/"));
        }
    }
    Ok(())
}

/// Check `source` for an import: its structure, then its digest against
/// `known_digests`.
///
/// # Errors
/// A structure variant of [`ImportCheckError`] when `source` is not a model,
/// [`ImportCheckError::Unverified`] when the digest is unknown and `rule`
/// requires a verified origin, and [`ImportCheckError::Io`] on a read failure.
pub fn check_import(
    source: &Path,
    known_digests: &[&str],
    rule: ImportRule,
) -> Result<ImportCheck, ImportCheckError> {
    check_structure(source)?;
    let sha256 = weights_sha256(source)?;
    let origin_verified = known_digests
        .iter()
        .any(|known| known.trim().eq_ignore_ascii_case(&sha256));
    if !origin_verified && rule.require_verified {
        tracing::event!(
            tracing::Level::WARN,
            path = %source.display(),
            sha256 = %sha256,
            "model.import.unverified_refused"
        );
        return Err(ImportCheckError::Unverified {
            path: source.to_path_buf(),
        });
    }
    tracing::event!(
        tracing::Level::INFO,
        path = %source.display(),
        sha256 = %sha256,
        origin_verified = origin_verified,
        "model.import.checked"
    );
    Ok(ImportCheck {
        sha256,
        origin_verified,
    })
}

#[cfg(test)]
mod tests;
