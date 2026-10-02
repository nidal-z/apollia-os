//! Walk a models directory and group its files into models.
//!
//! The walk is the only place that decides what a model is on disk. It never
//! follows a symbolic link to a folder, skips hidden entries, and stops at a
//! fixed depth so a pathological tree cannot keep it busy.

use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};

use super::{ModelCategory, ModelFormat};
use crate::gguf_probe::{probe_file, SCREEN_BYTES};

/// Deepest sub-folder level the walk enters below the models directory.
const MAX_DEPTH: usize = 8;

/// The GGML file magic written by whisper.cpp at the head of a `.bin` model,
/// `0x67676d6c` stored little-endian.
const GGML_MAGIC: [u8; 4] = *b"lmgg";

/// A model as the disk shows it, before the index adds what it remembers.
#[derive(Debug, Clone)]
pub(super) struct FoundModel {
    pub(super) format: ModelFormat,
    pub(super) path: PathBuf,
    pub(super) parts: Vec<PathBuf>,
    pub(super) annexes: Vec<PathBuf>,
    pub(super) size_bytes: u64,
    pub(super) modified_at: Option<DateTime<Utc>>,
    /// Short name: the file stem without its part suffix, or the folder name.
    pub(super) stem: String,
    /// A `.bin` file that carried the whisper.cpp magic.
    ggml_dictation: bool,
}

/// Every model under `root`. A missing `root` holds no model.
pub(super) fn scan(root: &Path) -> std::io::Result<Vec<FoundModel>> {
    let mut out = Vec::new();
    match std::fs::metadata(root) {
        Ok(m) if m.is_dir() => walk(root, 0, &mut out)?,
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e),
    }
    out.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(out)
}

/// The identifier of each model, in the order of `models`: its short name, or
/// its path relative to `root` when two models share that short name.
pub(super) fn assign_ids(root: &Path, models: &[FoundModel]) -> Vec<String> {
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for m in models {
        *counts.entry(m.stem.as_str()).or_default() += 1;
    }
    models
        .iter()
        .map(|m| {
            if counts.get(m.stem.as_str()).copied().unwrap_or(0) <= 1 {
                return m.stem.clone();
            }
            let relative = m.path.strip_prefix(root).unwrap_or(&m.path);
            let parent = relative.parent().map(path_to_slashes).unwrap_or_default();
            if parent.is_empty() {
                m.stem.clone()
            } else {
                format!("{parent}/{}", m.stem)
            }
        })
        .collect()
}

/// The category and parameter label a model's header declares.
pub(super) fn read_header(model: &FoundModel) -> (ModelCategory, Option<String>) {
    if model.ggml_dictation {
        return (ModelCategory::Dictation, None);
    }
    match model.format {
        ModelFormat::Gguf => match probe_file(&model.path, SCREEN_BYTES) {
            Ok(facts) => {
                let category = if facts.architecture.as_deref() == Some("whisper") {
                    ModelCategory::Dictation
                } else {
                    ModelCategory::Language
                };
                (category, facts.size_label)
            }
            Err(e) => {
                tracing::event!(
                    tracing::Level::WARN,
                    path = %model.path.display(),
                    error = %e,
                    "model.index.header.unreadable"
                );
                (ModelCategory::Language, None)
            }
        },
        ModelFormat::Mlx => (mlx_category(&model.path), None),
    }
}

/// Whether `dir` is an MLX model: a `config.json` and at least one
/// `*.safetensors` file directly inside it.
pub(super) fn is_mlx_dir(dir: &Path) -> bool {
    if !dir.join("config.json").is_file() {
        return false;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return false;
    };
    entries
        .flatten()
        .any(|e| is_safetensors(&e.path()) && e.path().is_file())
}

/// Whether `path` names a vision projector.
pub(crate) fn is_projector_name(path: &Path) -> bool {
    file_name(path).to_ascii_lowercase().contains("mmproj")
}

/// The base name and part number of a split file named
/// `<base>-NNNNN-of-NNNNN.<ext>`, or `None` for a whole file.
pub(crate) fn split_part(path: &Path) -> Option<(String, u32)> {
    let stem = path.file_stem()?.to_str()?;
    let (head, total) = stem.rsplit_once("-of-")?;
    let (base, index) = head.rsplit_once('-')?;
    let digits = |s: &str| s.len() == 5 && s.bytes().all(|b| b.is_ascii_digit());
    if base.is_empty() || !digits(index) || !digits(total) {
        return None;
    }
    Some((base.to_owned(), index.parse().ok()?))
}

pub(super) fn is_safetensors(path: &Path) -> bool {
    has_extension(path, "safetensors")
}

pub(super) fn has_extension(path: &Path, ext: &str) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case(ext))
}

fn walk(dir: &Path, depth: usize, out: &mut Vec<FoundModel>) -> std::io::Result<()> {
    let mut files = Vec::new();
    let mut dirs = Vec::new();
    for entry in std::fs::read_dir(dir)?.flatten() {
        let path = entry.path();
        if file_name(&path).starts_with('.') {
            continue;
        }
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if kind.is_dir() {
            dirs.push(path);
        } else if path.is_file() {
            files.push(path);
        }
    }
    files.sort();
    dirs.sort();

    group_files(&files, out);

    for sub in dirs {
        if is_mlx_dir(&sub) {
            out.push(mlx_model(&sub));
        } else if depth < MAX_DEPTH {
            walk(&sub, depth + 1, out)?;
        }
    }
    Ok(())
}

/// Turn the files of one folder into models: whole GGUF files, split GGUF
/// files gathered under their first part, whisper.cpp `.bin` files, and
/// projectors attached to the model they serve.
fn group_files(files: &[PathBuf], out: &mut Vec<FoundModel>) {
    let mut models: Vec<FoundModel> = Vec::new();
    let mut splits: BTreeMap<String, Vec<(u32, PathBuf)>> = BTreeMap::new();
    let mut projectors = Vec::new();

    for path in files {
        if has_extension(path, "gguf") {
            if is_projector_name(path) {
                projectors.push(path.clone());
            } else if let Some((base, index)) = split_part(path) {
                splits.entry(base).or_default().push((index, path.clone()));
            } else {
                models.push(file_model(path.clone(), Vec::new(), stem_of(path), false));
            }
        } else if has_extension(path, "bin") && has_ggml_magic(path) {
            models.push(file_model(path.clone(), Vec::new(), stem_of(path), true));
        }
    }

    for (base, mut parts) in splits {
        parts.sort_by_key(|(index, _)| *index);
        let parts: Vec<PathBuf> = parts.into_iter().map(|(_, p)| p).collect();
        if let Some(first) = parts.first().cloned() {
            models.push(file_model(first, parts, base, false));
        }
    }

    for projector in projectors {
        attach_projector(&mut models, projector);
    }
    out.extend(models);
}

/// Attach a projector to the GGUF model of the same folder whose name it
/// carries (the longest such name wins), or to the only GGUF model there. A
/// projector that matches nothing is left out: it is never a model.
fn attach_projector(models: &mut [FoundModel], projector: PathBuf) {
    let name = stem_of(&projector).to_ascii_lowercase();
    let gguf: Vec<usize> = models
        .iter()
        .enumerate()
        .filter(|(_, m)| m.format == ModelFormat::Gguf && !m.ggml_dictation)
        .map(|(i, _)| i)
        .collect();
    let by_name = gguf
        .iter()
        .copied()
        .filter(|&i| name.contains(&models[i].stem.to_ascii_lowercase()))
        .max_by_key(|&i| models[i].stem.len());
    let target = by_name.or_else(|| (gguf.len() == 1).then(|| gguf[0]));
    if let Some(i) = target {
        models[i].annexes.push(projector);
    }
}

fn file_model(path: PathBuf, parts: Vec<PathBuf>, stem: String, ggml: bool) -> FoundModel {
    let files: Vec<&Path> = if parts.is_empty() {
        vec![path.as_path()]
    } else {
        parts.iter().map(PathBuf::as_path).collect()
    };
    let (size_bytes, modified_at) = measure(&files);
    FoundModel {
        format: ModelFormat::Gguf,
        path,
        parts,
        annexes: Vec::new(),
        size_bytes,
        modified_at,
        stem,
        ggml_dictation: ggml,
    }
}

fn mlx_model(dir: &Path) -> FoundModel {
    let mut files = Vec::new();
    collect_files(dir, 0, &mut files);
    let refs: Vec<&Path> = files.iter().map(PathBuf::as_path).collect();
    let (size_bytes, modified_at) = measure(&refs);
    FoundModel {
        format: ModelFormat::Mlx,
        path: dir.to_path_buf(),
        parts: Vec::new(),
        annexes: Vec::new(),
        size_bytes,
        modified_at,
        stem: file_name(dir),
        ggml_dictation: false,
    }
}

fn collect_files(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        match entry.file_type() {
            Ok(k) if k.is_dir() && depth < MAX_DEPTH => collect_files(&path, depth + 1, out),
            Ok(_) if path.is_file() => out.push(path),
            _ => {}
        }
    }
}

/// Total size and latest modification time of `files`.
fn measure(files: &[&Path]) -> (u64, Option<DateTime<Utc>>) {
    let mut size = 0u64;
    let mut latest: Option<DateTime<Utc>> = None;
    for file in files {
        if let Ok(meta) = std::fs::metadata(file) {
            size = size.saturating_add(meta.len());
            if let Ok(modified) = meta.modified() {
                let at = DateTime::<Utc>::from(modified);
                latest = Some(latest.map_or(at, |l| l.max(at)));
            }
        }
    }
    (size, latest)
}

pub(super) fn has_ggml_magic(path: &Path) -> bool {
    let mut head = [0u8; 4];
    std::fs::File::open(path)
        .and_then(|mut f| f.read_exact(&mut head))
        .is_ok()
        && head == GGML_MAGIC
}

/// An MLX folder is a dictation model when its `config.json` says whisper.
fn mlx_category(dir: &Path) -> ModelCategory {
    let model_type = std::fs::read(dir.join("config.json"))
        .ok()
        .and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok())
        .and_then(|v| {
            v.get("model_type")
                .and_then(|t| t.as_str())
                .map(str::to_owned)
        });
    if model_type.as_deref() == Some("whisper") {
        ModelCategory::Dictation
    } else {
        ModelCategory::Language
    }
}

fn stem_of(path: &Path) -> String {
    path.file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn path_to_slashes(path: &Path) -> String {
    path.components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("/")
}
