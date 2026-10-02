//! Building an `.apo` archive from a package folder.

use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};

use apollia_signing::{sha256, DigestList, SignatureFile, SigningKeyPair};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

use super::read::{read_bounded, render_public_key};
use super::{
    check_entry_path, ApoError, ArchiveLimits, EntryInfo, SignatureState, SIGNATURE_ENTRY,
    SIGNATURE_KEY_ENTRY,
};
use crate::apollia_manifest::{self, ApolliaManifest, MANIFEST_FILE_NAME};

/// Names left out of an archive wherever they appear in the folder: version
/// control, Python caches and virtual environments, and Finder metadata.
const SKIPPED_NAMES: &[&str] = &[".git", "__pycache__", ".venv", ".DS_Store"];

/// Decorators whose presence in a module means the module delivers a tool or
/// a workflow, and therefore needs a descriptor in `apollia.toml`.
const DECORATORS: &[&str] = &["@tool", "@workflow", "@apollia.tool", "@apollia.workflow"];

/// How to build an archive.
#[derive(Debug, Default, Clone, Copy)]
pub struct BuildOptions<'a> {
    /// Key that signs the archive; `None` builds an unsigned archive.
    pub signing_key: Option<&'a SigningKeyPair>,
    /// Bounds the package must fit in.
    pub limits: ArchiveLimits,
}

/// What a build produced.
#[derive(Debug, Clone)]
pub struct BuildReport {
    /// The manifest written at the root of the archive.
    pub manifest: ApolliaManifest,
    /// The file entries, in archive order.
    pub entries: Vec<EntryInfo>,
    /// Whether the archive was signed, and by which key.
    pub signature: SignatureState,
}

/// Builds the archive of the package folder `dir` into `output`.
///
/// The build validates `apollia.toml`, refuses a folder whose Python modules
/// carry tools or workflows that the manifest does not describe (descriptors
/// are written by the SDK, `python -m apollia describe`; no Python runs here),
/// and refuses a symbolic link, a reserved name, or a package over `limits`.
/// Entries are compressed with deflate and stamped with a fixed date, so the
/// same folder always gives the same bytes. The archive is written next to
/// `output` and renamed into place once complete.
///
/// # Errors
///
/// [`ApoError::MissingManifest`] or [`ApoError::Manifest`] for the manifest;
/// [`ApoError::MissingModule`] or [`ApoError::MissingDescriptors`] when the
/// descriptors do not cover the code; [`ApoError::Symlink`],
/// [`ApoError::UnsafePath`], [`ApoError::ReservedName`] or
/// [`ApoError::LimitExceeded`] for the folder content; [`ApoError::Io`] or
/// [`ApoError::Zip`] while writing; [`ApoError::Signing`] when the publisher
/// name cannot be written in the signature.
pub fn build_from_dir(
    dir: &Path,
    output: &Path,
    options: &BuildOptions<'_>,
) -> Result<BuildReport, ApoError> {
    let limits = options.limits;
    let manifest_path = dir.join(MANIFEST_FILE_NAME);
    let manifest_bytes = match std::fs::File::open(&manifest_path) {
        Ok(mut file) => read_bounded(&mut file, limits.max_manifest_bytes, "manifest size")?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(ApoError::MissingManifest)
        }
        Err(source) => {
            return Err(ApoError::Io {
                path: manifest_path,
                source,
            })
        }
    };
    let manifest_text =
        String::from_utf8(manifest_bytes).map_err(|_| ApoError::ManifestEncoding)?;
    let manifest = apollia_manifest::parse(&manifest_text)?;

    let output_abs = std::path::absolute(output).map_err(|source| ApoError::Io {
        path: output.to_path_buf(),
        source,
    })?;
    let files = collect_files(dir, &output_abs, &limits)?;
    check_descriptors(&manifest, &files)?;

    let partial = output.with_extension("apo.partial");
    let result = write_archive(&files, &partial, &manifest, options);
    match result {
        Ok((entries, signature)) => {
            std::fs::rename(&partial, output).map_err(|source| ApoError::Io {
                path: output.to_path_buf(),
                source,
            })?;
            Ok(BuildReport {
                manifest,
                entries,
                signature,
            })
        }
        Err(error) => {
            let _ = std::fs::remove_file(&partial);
            Err(error)
        }
    }
}

/// The files of the package, keyed by their archive path, `apollia.toml`
/// included.
fn collect_files(
    dir: &Path,
    output: &Path,
    limits: &ArchiveLimits,
) -> Result<BTreeMap<String, PathBuf>, ApoError> {
    let mut files = BTreeMap::new();
    let mut total: u64 = 0;
    let mut stack = vec![dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        let read = std::fs::read_dir(&current).map_err(|source| ApoError::Io {
            path: current.clone(),
            source,
        })?;
        for item in read {
            let item = item.map_err(|source| ApoError::Io {
                path: current.clone(),
                source,
            })?;
            let path = item.path();
            let file_name = item.file_name();
            let Some(name) = file_name.to_str() else {
                return Err(ApoError::UnsafePath {
                    path: path.display().to_string(),
                    reason: "name is not UTF-8",
                });
            };
            if SKIPPED_NAMES.contains(&name) || name.ends_with(".pyc") {
                continue;
            }
            let meta = std::fs::symlink_metadata(&path).map_err(|source| ApoError::Io {
                path: path.clone(),
                source,
            })?;
            let relative = relative_name(dir, &path)?;
            if meta.file_type().is_symlink() {
                return Err(ApoError::Symlink { path: relative });
            }
            if meta.is_dir() {
                stack.push(path);
                continue;
            }
            if std::path::absolute(&path).ok().as_deref() == Some(output) {
                continue;
            }
            if relative == SIGNATURE_ENTRY || relative == SIGNATURE_KEY_ENTRY {
                return Err(ApoError::ReservedName { path: relative });
            }
            if meta.len() > limits.max_entry_bytes {
                return Err(ApoError::LimitExceeded {
                    limit: "entry size",
                    max: limits.max_entry_bytes,
                });
            }
            total = total.saturating_add(meta.len());
            if total > limits.max_total_bytes {
                return Err(ApoError::LimitExceeded {
                    limit: "decompressed size",
                    max: limits.max_total_bytes,
                });
            }
            files.insert(relative, path);
            // Two more entries when signed.
            if files.len() + 2 > limits.max_entries {
                return Err(ApoError::LimitExceeded {
                    limit: "entry count",
                    max: limits.max_entries as u64,
                });
            }
        }
    }
    Ok(files)
}

fn relative_name(dir: &Path, path: &Path) -> Result<String, ApoError> {
    let relative = path.strip_prefix(dir).map_err(|_| ApoError::UnsafePath {
        path: path.display().to_string(),
        reason: "outside the package folder",
    })?;
    let mut parts = Vec::new();
    for component in relative.components() {
        let part = component.as_os_str().to_str().ok_or(ApoError::UnsafePath {
            path: path.display().to_string(),
            reason: "name is not UTF-8",
        })?;
        parts.push(part);
    }
    let name = parts.join("/");
    check_entry_path(&name)?;
    Ok(name)
}

/// Every module the manifest names exists, every declared tool and workflow
/// carries its input schema, and every module that defines a tool or a
/// workflow is declared.
fn check_descriptors(
    manifest: &ApolliaManifest,
    files: &BTreeMap<String, PathBuf>,
) -> Result<(), ApoError> {
    let declared = manifest
        .tools
        .iter()
        .map(|t| (t.module.as_str(), t.inputs.is_some()))
        .chain(
            manifest
                .workflows
                .iter()
                .map(|w| (w.module.as_str(), w.inputs.is_some())),
        );
    let mut missing = Vec::new();
    let mut declared_modules = Vec::new();
    for (module, has_inputs) in declared {
        let normalized = module.trim_start_matches("./");
        if !files.contains_key(normalized) {
            return Err(ApoError::MissingModule {
                module: module.to_owned(),
            });
        }
        if !has_inputs && !missing.iter().any(|m| m == normalized) {
            missing.push(normalized.to_owned());
        }
        declared_modules.push(normalized.to_owned());
    }
    for (name, path) in files {
        if !name.ends_with(".py") || declared_modules.contains(name) {
            continue;
        }
        let text = std::fs::read_to_string(path).map_err(|source| ApoError::Io {
            path: path.clone(),
            source,
        })?;
        if defines_tool_or_workflow(&text) {
            missing.push(name.clone());
        }
    }
    if missing.is_empty() {
        Ok(())
    } else {
        missing.sort();
        Err(ApoError::MissingDescriptors { modules: missing })
    }
}

/// `true` when a line of `source` applies one of [`DECORATORS`].
fn defines_tool_or_workflow(source: &str) -> bool {
    source.lines().any(|line| {
        let line = line.trim_start();
        DECORATORS.iter().any(|d| {
            line.strip_prefix(d)
                .is_some_and(|rest| rest.is_empty() || rest.starts_with(['(', ' ', '\t']))
        })
    })
}

fn write_archive(
    files: &BTreeMap<String, PathBuf>,
    target: &Path,
    manifest: &ApolliaManifest,
    options: &BuildOptions<'_>,
) -> Result<(Vec<EntryInfo>, SignatureState), ApoError> {
    let io = |path: &Path| {
        let path = path.to_path_buf();
        move |source| ApoError::Io { path, source }
    };
    let out = std::fs::File::create(target).map_err(io(target))?;
    let mut zip = ZipWriter::new(out);
    let file_options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .last_modified_time(zip::DateTime::default())
        .unix_permissions(0o644);

    // The manifest first, so a streaming reader meets it before anything else.
    let mut order: Vec<&String> = files.keys().filter(|k| *k == MANIFEST_FILE_NAME).collect();
    order.extend(files.keys().filter(|k| *k != MANIFEST_FILE_NAME));

    let mut list = DigestList::new();
    let mut entries = Vec::with_capacity(files.len() + 2);
    for name in order {
        let path = &files[name];
        let mut file = std::fs::File::open(path).map_err(io(path))?;
        let bytes = read_bounded(&mut file, options.limits.max_entry_bytes, "entry size")?;
        zip.start_file(name.as_str(), file_options)?;
        zip.write_all(&bytes).map_err(io(target))?;
        list.insert(name.clone(), sha256(&bytes))?;
        entries.push(EntryInfo {
            path: name.clone(),
            size: bytes.len() as u64,
        });
    }

    let signature = match options.signing_key {
        None => SignatureState::Unsigned,
        Some(key) => {
            let signature = SignatureFile::create(&list, &manifest.package.publisher, key)?;
            let rendered = signature.render();
            let key_text = render_public_key(&key.public_key());
            for (name, content) in [
                (SIGNATURE_ENTRY, rendered.as_str()),
                (SIGNATURE_KEY_ENTRY, key_text.as_str()),
            ] {
                zip.start_file(name, file_options)?;
                zip.write_all(content.as_bytes()).map_err(io(target))?;
                entries.push(EntryInfo {
                    path: name.to_owned(),
                    size: content.len() as u64,
                });
            }
            SignatureState::Signed {
                publisher: signature.publisher().to_owned(),
                key_fingerprint: signature.key_fingerprint().to_owned(),
            }
        }
    };
    let mut out = zip.finish()?;
    out.flush().map_err(io(target))?;
    Ok((entries, signature))
}
