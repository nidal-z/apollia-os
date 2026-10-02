//! The files that make up one variant of a repository, and where they land.
//!
//! A variant is what the user picks: a quantization of a GGUF model, or an MLX
//! repository as a whole. On disk it can be several files, and the user never
//! sees them one by one:
//!
//! - a GGUF variant is its weights file, or every part of a split file, plus
//!   the vision projector (`mmproj`) when the repository publishes one;
//! - an MLX variant is every file of the repository (`config.json`, the
//!   tokenizer, the `*.safetensors` weights), kept together in one folder.
//!
//! `plan_variant` reads the repository tree and returns the list to fetch,
//! each file with the reference its download is checked against. It is pure:
//! the tree is an argument, so a fake repository is enough to test it.
//!
//! The destination layout is chosen so the model index reads the files back
//! as one model. A projector is only attached to a GGUF model of its own folder
//! (by name, or as the only GGUF there), so a variant with a projector is
//! placed in a folder of its own, named after the variant.

use std::path::Path;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::hf_registry::parse::normalize_sha256;
use crate::hf_registry::TreeEntry;
use crate::model_index::{is_projector_name, split_part, ModelFormat};

/// Which variant of a repository to fetch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum VariantSelector {
    /// A GGUF variant, named by any of its files (a whole file or one part of
    /// a split file) as its path inside the repository.
    Gguf {
        /// Path of a weights file inside the repository.
        file: String,
    },
    /// The whole MLX repository.
    Mlx,
}

/// One file of a variant.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VariantFile {
    /// Path of the file inside the repository.
    pub repo_path: String,
    /// Path of the file on disk, relative to the models directory.
    pub dest_name: String,
    /// Direct download URL.
    pub url: String,
    /// Size the repository tree declares, in bytes.
    pub size_bytes: u64,
    /// SHA-256 the repository publishes for the content (LFS files). `None`
    /// for a small file committed outside LFS, checked by `git_oid` instead.
    pub sha256: Option<String>,
    /// Git blob identifier (SHA-1 of `blob <size>\0` then the content) of a
    /// file committed outside LFS, the only digest the repository publishes
    /// for it. `None` for an LFS file.
    #[serde(default)]
    pub git_oid: Option<String>,
}

/// Everything to fetch for one variant.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VariantPlan {
    /// Repository (`org/name`).
    pub repo_id: String,
    /// Storage format of the variant.
    pub format: ModelFormat,
    /// Path of the model on disk, relative to the models directory, as the
    /// model index records it: the first weights file of a GGUF, the folder
    /// of an MLX model.
    pub model_path: String,
    /// Path inside the repository of the file the model is named after: the
    /// first weights file of a GGUF, `config.json` for MLX.
    pub origin_file: String,
    /// Every file, weights first.
    pub files: Vec<VariantFile>,
    /// Sum of the declared sizes.
    pub total_bytes: u64,
}

/// Why a variant cannot be planned from a repository tree.
#[derive(Debug, Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum VariantError {
    /// The named file is not a GGUF weights file of the repository.
    #[error("'{path}' is not a GGUF weights file of repository '{repo_id}'")]
    NotAWeightsFile {
        /// Repository (`org/name`).
        repo_id: String,
        /// Requested path.
        path: String,
    },
    /// A split file has a part missing from the repository.
    #[error(
        "split file '{path}' is incomplete in repository '{repo_id}': {found} of {expected} parts"
    )]
    IncompleteSplit {
        /// Repository (`org/name`).
        repo_id: String,
        /// Requested path.
        path: String,
        /// Parts present.
        found: usize,
        /// Parts the file name announces.
        expected: usize,
    },
    /// The repository is not an MLX model: no `config.json` or no
    /// `*.safetensors` at its root.
    #[error("repository '{repo_id}' is not an MLX model")]
    NotMlx {
        /// Repository (`org/name`).
        repo_id: String,
    },
    /// A weights file publishes no SHA-256, so its download cannot be checked.
    #[error("no published sha256 for '{path}' in repository '{repo_id}'")]
    NoPublishedSha256 {
        /// Repository (`org/name`).
        repo_id: String,
        /// Path of the file inside the repository.
        path: String,
    },
    /// A file publishes neither a SHA-256 nor a git blob identifier, so its
    /// download cannot be checked.
    #[error("no published digest for '{path}' in repository '{repo_id}'")]
    NoPublishedDigest {
        /// Repository (`org/name`).
        repo_id: String,
        /// Path of the file inside the repository.
        path: String,
    },
}

/// Plan the download of one variant of `repo_id` from its tree.
///
/// `base_url` is the host files are resolved from
/// (`<base_url>/<repo_id>/resolve/main/<path>`).
///
/// # Errors
/// A [`VariantError`] when the selector does not name a complete variant, or
/// when a weights file publishes no SHA-256 or another file no digest at all.
pub(crate) fn plan_variant(
    base_url: &str,
    repo_id: &str,
    tree: &[TreeEntry],
    selector: &VariantSelector,
) -> Result<VariantPlan, VariantError> {
    let files: Vec<&TreeEntry> = tree.iter().filter(|e| e.entry_type == "file").collect();
    let plan = match selector {
        VariantSelector::Gguf { file } => plan_gguf(base_url, repo_id, &files, file)?,
        VariantSelector::Mlx => plan_mlx(base_url, repo_id, &files)?,
    };
    tracing::event!(
        tracing::Level::INFO,
        repo = %repo_id,
        format = ?plan.format,
        files = plan.files.len(),
        total_bytes = plan.total_bytes,
        "model.variant.planned"
    );
    Ok(plan)
}

fn plan_gguf(
    base_url: &str,
    repo_id: &str,
    files: &[&TreeEntry],
    requested: &str,
) -> Result<VariantPlan, VariantError> {
    let not_weights = || VariantError::NotAWeightsFile {
        repo_id: repo_id.to_owned(),
        path: requested.to_owned(),
    };
    let entry = files
        .iter()
        .find(|e| e.path == requested)
        .ok_or_else(not_weights)?;
    let path = Path::new(&entry.path);
    if !is_gguf(path) || is_projector_name(path) {
        return Err(not_weights());
    }
    let folder = parent_of(&entry.path);

    let (weights, stem) = match split_part(path) {
        Some((base, _)) => {
            let mut parts: Vec<(u32, &TreeEntry)> = files
                .iter()
                .filter(|e| parent_of(&e.path) == folder)
                .filter_map(|e| match split_part(Path::new(&e.path)) {
                    Some((b, index)) if b == base && is_gguf(Path::new(&e.path)) => {
                        Some((index, *e))
                    }
                    _ => None,
                })
                .collect();
            parts.sort_by_key(|(index, _)| *index);
            let expected = split_total(path).unwrap_or(0) as usize;
            let contiguous = parts
                .iter()
                .enumerate()
                .all(|(i, (index, _))| *index as usize == i + 1);
            if parts.len() != expected || !contiguous {
                return Err(VariantError::IncompleteSplit {
                    repo_id: repo_id.to_owned(),
                    path: requested.to_owned(),
                    found: parts.len(),
                    expected,
                });
            }
            (parts.into_iter().map(|(_, e)| e).collect::<Vec<_>>(), base)
        }
        None => (vec![*entry], file_stem(path)),
    };

    let projector = pick_projector(files, folder);
    let mut planned = Vec::with_capacity(weights.len() + 1);
    for w in &weights {
        let dest = match projector {
            Some(_) => format!("{stem}/{}", file_name(&w.path)),
            None => w.path.clone(),
        };
        planned.push(planned_file(base_url, repo_id, w, dest, true)?);
    }
    if let Some(p) = projector {
        let dest = format!("{stem}/{}", file_name(&p.path));
        planned.push(planned_file(base_url, repo_id, p, dest, true)?);
    }

    let model_path = planned
        .first()
        .map(|f| f.dest_name.clone())
        .unwrap_or_default();
    let origin_file = weights.first().map(|w| w.path.clone()).unwrap_or_default();
    Ok(finish(
        repo_id,
        ModelFormat::Gguf,
        model_path,
        origin_file,
        planned,
    ))
}

fn plan_mlx(
    base_url: &str,
    repo_id: &str,
    files: &[&TreeEntry],
) -> Result<VariantPlan, VariantError> {
    let at_root = |pred: &dyn Fn(&str) -> bool| {
        files
            .iter()
            .any(|e| parent_of(&e.path).is_empty() && pred(&e.path))
    };
    let has_config = at_root(&|p| p == "config.json");
    let has_weights = at_root(&|p| is_safetensors(p));
    if !has_config || !has_weights {
        return Err(VariantError::NotMlx {
            repo_id: repo_id.to_owned(),
        });
    }

    let folder = repo_id.rsplit('/').next().unwrap_or(repo_id).to_owned();
    let mut kept: Vec<&TreeEntry> = files
        .iter()
        .copied()
        .filter(|e| !e.path.split('/').any(|seg| seg.starts_with('.')))
        .collect();
    kept.sort_by_key(|e| (!is_safetensors(&e.path), e.path.clone()));

    let mut planned = Vec::with_capacity(kept.len());
    for e in kept {
        let dest = format!("{folder}/{}", e.path);
        planned.push(planned_file(
            base_url,
            repo_id,
            e,
            dest,
            is_safetensors(&e.path),
        )?);
    }
    Ok(finish(
        repo_id,
        ModelFormat::Mlx,
        folder,
        "config.json".to_owned(),
        planned,
    ))
}

/// The projector a GGUF variant is served with: one of its own folder, else
/// one at the repository root. Among several, full precision is preferred.
fn pick_projector<'a>(files: &[&'a TreeEntry], folder: &str) -> Option<&'a TreeEntry> {
    let in_folder = |dir: &str| -> Vec<&'a TreeEntry> {
        let mut found: Vec<&'a TreeEntry> = files
            .iter()
            .copied()
            .filter(|e| parent_of(&e.path) == dir)
            .filter(|e| is_gguf(Path::new(&e.path)) && is_projector_name(Path::new(&e.path)))
            .collect();
        found.sort_by_key(|e| (projector_rank(&e.path), e.path.clone()));
        found
    };
    let mut candidates = in_folder(folder);
    if candidates.is_empty() && !folder.is_empty() {
        candidates = in_folder("");
    }
    candidates.into_iter().next()
}

/// Lower is preferred: the projector is small, and full precision keeps the
/// vision quality the model was published with.
fn projector_rank(path: &str) -> u8 {
    let name = file_name(path).to_ascii_lowercase();
    if name.contains("f16") && !name.contains("bf16") {
        0
    } else if name.contains("bf16") {
        1
    } else if name.contains("f32") {
        2
    } else if name.contains("q8") {
        3
    } else {
        4
    }
}

fn planned_file(
    base_url: &str,
    repo_id: &str,
    entry: &TreeEntry,
    dest_name: String,
    weights: bool,
) -> Result<VariantFile, VariantError> {
    let sha256 = entry
        .lfs
        .as_ref()
        .and_then(|lfs| normalize_sha256(&lfs.oid));
    if weights && sha256.is_none() {
        return Err(VariantError::NoPublishedSha256 {
            repo_id: repo_id.to_owned(),
            path: entry.path.clone(),
        });
    }
    let git_oid = if sha256.is_none() {
        let oid = entry.oid.as_deref().and_then(normalize_git_oid);
        if oid.is_none() {
            return Err(VariantError::NoPublishedDigest {
                repo_id: repo_id.to_owned(),
                path: entry.path.clone(),
            });
        }
        oid
    } else {
        None
    };
    Ok(VariantFile {
        repo_path: entry.path.clone(),
        dest_name,
        url: format!("{base_url}/{repo_id}/resolve/main/{}", entry.path),
        size_bytes: entry.size,
        sha256,
        git_oid,
    })
}

/// A git SHA-1 object identifier, lowercased, or `None` when `raw` is not 40
/// hexadecimal characters.
fn normalize_git_oid(raw: &str) -> Option<String> {
    let lower = raw.trim().to_ascii_lowercase();
    (lower.len() == 40 && lower.bytes().all(|b| b.is_ascii_hexdigit())).then_some(lower)
}

fn finish(
    repo_id: &str,
    format: ModelFormat,
    model_path: String,
    origin_file: String,
    files: Vec<VariantFile>,
) -> VariantPlan {
    let total_bytes = files.iter().map(|f| f.size_bytes).sum();
    VariantPlan {
        repo_id: repo_id.to_owned(),
        format,
        model_path,
        origin_file,
        files,
        total_bytes,
    }
}

/// The part count a split file name announces (`...-00001-of-00003.gguf`).
fn split_total(path: &Path) -> Option<u32> {
    let stem = path.file_stem()?.to_str()?;
    let (_, total) = stem.rsplit_once("-of-")?;
    total.parse().ok()
}

fn is_gguf(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("gguf"))
}

fn is_safetensors(path: &str) -> bool {
    path.to_ascii_lowercase().ends_with(".safetensors")
}

fn parent_of(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(dir, _)| dir)
}

fn file_name(path: &str) -> &str {
    path.rsplit_once('/').map_or(path, |(_, name)| name)
}

fn file_stem(path: &Path) -> String {
    path.file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests;
