//! Extraction of an engine archive into a fresh directory.
//!
//! Only archives whose SHA-256 already matched a verified manifest reach this
//! module, and it still treats them as untrusted: every entry path is checked
//! to stay under the target, a symbolic link must point beside or below
//! itself, and the sizes are capped. Archives are zip, read with the `zip`
//! crate the workspace already carries for agent packages.

use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};

use crate::engine_store::manifest::ArchiveKind;
use crate::engine_store::EngineStoreError;

/// Largest decompressed content accepted, all entries together.
pub const MAX_EXTRACTED_BYTES: u64 = 8 * 1024 * 1024 * 1024;

/// Largest number of entries accepted.
pub const MAX_ENTRIES: usize = 20_000;

/// Extract `archive` of `kind` into `target`, which must not exist yet.
///
/// On failure, `target` is removed.
///
/// # Errors
///
/// [`EngineStoreError::Archive`] for a malformed or unsafe archive,
/// [`EngineStoreError::Io`] for a filesystem failure.
pub fn extract(archive: &Path, kind: ArchiveKind, target: &Path) -> Result<(), EngineStoreError> {
    if target.exists() {
        return Err(EngineStoreError::Archive {
            reason: format!("{} already exists", target.display()),
        });
    }
    std::fs::create_dir_all(target).map_err(io(target))?;
    let file = std::fs::File::open(archive).map_err(io(archive))?;
    let result = match kind {
        ArchiveKind::Zip => extract_zip(file, target),
    };
    if result.is_err() {
        let _ = std::fs::remove_dir_all(target);
    }
    result
}

fn io(path: &Path) -> impl FnOnce(std::io::Error) -> EngineStoreError {
    let path = path.to_path_buf();
    move |source| EngineStoreError::Io { path, source }
}

fn bad(reason: impl Into<String>) -> EngineStoreError {
    EngineStoreError::Archive {
        reason: reason.into(),
    }
}

/// The path of `name` under `target`, or a refusal when it would leave it.
fn safe_dest(target: &Path, name: &str) -> Result<PathBuf, EngineStoreError> {
    let trimmed = name.trim_end_matches('/');
    let rel = Path::new(trimmed);
    if trimmed.is_empty() {
        return Ok(target.to_path_buf());
    }
    let mut out = target.to_path_buf();
    for component in rel.components() {
        match component {
            Component::Normal(part) => out.push(part),
            Component::CurDir => {}
            _ => return Err(bad(format!("entry {name:?} leaves the archive root"))),
        }
    }
    if name.contains('\\') {
        return Err(bad(format!("entry {name:?} carries a backslash")));
    }
    Ok(out)
}

/// Whether a link target stays under the directory of its link.
///
/// A parent step is refused outright rather than counted: a link resolved
/// through another link can climb further than its own text says, and the
/// upstream archives only ever link a library to its sibling.
fn link_stays_inside(link_target: &str) -> bool {
    !link_target.is_empty()
        && !link_target.starts_with('/')
        && !link_target.contains('\\')
        && !link_target.contains(':')
        && link_target.split('/').all(|part| part != "..")
}

struct Budget {
    entries: usize,
    bytes: u64,
}

impl Budget {
    fn entry(&mut self) -> Result<(), EngineStoreError> {
        self.entries += 1;
        if self.entries > MAX_ENTRIES {
            return Err(bad(format!("more than {MAX_ENTRIES} entries")));
        }
        Ok(())
    }

    fn bytes(&mut self, n: u64) -> Result<(), EngineStoreError> {
        self.bytes = self.bytes.saturating_add(n);
        if self.bytes > MAX_EXTRACTED_BYTES {
            return Err(bad("decompressed size over the cap"));
        }
        Ok(())
    }
}

fn write_file(
    dest: &Path,
    reader: &mut impl Read,
    size: u64,
    mode: Option<u32>,
    budget: &mut Budget,
) -> Result<(), EngineStoreError> {
    budget.bytes(size)?;
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(io(parent))?;
    }
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(dest)
        .map_err(io(dest))?;
    let copied = std::io::copy(&mut reader.take(size), &mut file).map_err(io(dest))?;
    if copied != size {
        return Err(bad(format!("entry {} is truncated", dest.display())));
    }
    file.flush().map_err(io(dest))?;
    set_mode(dest, mode)?;
    Ok(())
}

#[cfg(unix)]
fn set_mode(dest: &Path, mode: Option<u32>) -> Result<(), EngineStoreError> {
    use std::os::unix::fs::PermissionsExt;
    if let Some(mode) = mode {
        // Only the permission bits: no setuid, setgid or sticky bit.
        let perms = std::fs::Permissions::from_mode(mode & 0o777);
        std::fs::set_permissions(dest, perms).map_err(io(dest))?;
    }
    Ok(())
}

#[cfg(not(unix))]
fn set_mode(_dest: &Path, _mode: Option<u32>) -> Result<(), EngineStoreError> {
    Ok(())
}

fn make_symlink(entry: &str, dest: &Path, link_target: &str) -> Result<(), EngineStoreError> {
    if !link_stays_inside(link_target) {
        return Err(bad(format!(
            "link {entry:?} points outside the archive ({link_target:?})"
        )));
    }
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(io(parent))?;
    }
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(link_target, dest).map_err(io(dest))
    }
    #[cfg(not(unix))]
    {
        // Upstream Windows archives carry no links. Copying the target would
        // need it extracted first; refusing is the honest answer.
        let _ = dest;
        Err(bad(format!("symbolic link {entry:?} in a Windows archive")))
    }
}

fn extract_zip(file: std::fs::File, target: &Path) -> Result<(), EngineStoreError> {
    let mut zip =
        zip::ZipArchive::new(file).map_err(|e| bad(format!("unreadable zip archive: {e}")))?;
    let mut budget = Budget {
        entries: 0,
        bytes: 0,
    };
    for index in 0..zip.len() {
        budget.entry()?;
        let mut entry = zip
            .by_index(index)
            .map_err(|e| bad(format!("unreadable zip entry: {e}")))?;
        let name = entry.name().to_owned();
        let dest = safe_dest(target, &name)?;
        if entry.is_symlink() {
            let mut link = String::new();
            entry
                .read_to_string(&mut link)
                .map_err(|e| bad(format!("unreadable link {name:?}: {e}")))?;
            make_symlink(&name, &dest, &link)?;
            continue;
        }
        if entry.is_dir() {
            std::fs::create_dir_all(&dest).map_err(io(&dest))?;
            continue;
        }
        let size = entry.size();
        let mode = entry.unix_mode();
        write_file(&dest, &mut entry, size, mode, &mut budget)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests;
