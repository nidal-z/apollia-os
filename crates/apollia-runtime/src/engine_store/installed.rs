//! What the store holds, and which installed engine the launcher takes.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::engine_store::manifest::{compare_tags, is_safe_component, Engine, Variant};

/// Name of the record written beside an installed package, once verified.
pub const RECORD_FILE: &str = ".apollia-engine.json";

/// How an engine reached this machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "lowercase")]
#[schema(as = EngineSource)]
pub enum Source {
    /// Shipped inside the Apollia bundle.
    Embedded,
    /// Downloaded from the channel's manifest.
    Downloaded,
    /// Imported from a disk, without network.
    Imported,
}

/// The record of one installed package.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstalledPackage {
    /// Engine.
    pub engine: Engine,
    /// Upstream tag.
    pub tag: String,
    /// Variant.
    pub variant: Variant,
    /// Platform it was built for.
    pub platform: String,
    /// SHA-256 of the archive it was extracted from.
    pub sha256: String,
    /// Downloaded or imported.
    pub source: Source,
    /// Directory it was extracted into. Not stored: filled when read.
    #[serde(skip)]
    pub path: PathBuf,
}

/// The directory a package of `engine`, `tag` and `variant` lives in.
#[must_use]
pub fn package_dir(root: &Path, engine: Engine, tag: &str, variant: Variant) -> PathBuf {
    root.join(engine.as_str()).join(tag).join(variant.as_str())
}

fn read_record(dir: &Path) -> Option<InstalledPackage> {
    let bytes = std::fs::read(dir.join(RECORD_FILE)).ok()?;
    let mut record: InstalledPackage = serde_json::from_slice(&bytes).ok()?;
    let expected = dir.file_name()?.to_str()? == record.variant.as_str()
        && dir.parent()?.file_name()?.to_str()? == record.tag
        && is_safe_component(&record.tag);
    if !expected {
        return None;
    }
    record.path = dir.to_path_buf();
    Some(record)
}

/// Every installed package of the store, verified records only, ordered by
/// engine, variant, then tag.
#[must_use]
pub fn installed_packages(root: &Path) -> Vec<InstalledPackage> {
    let mut found = Vec::new();
    for engine in Engine::ALL {
        let Ok(tags) = std::fs::read_dir(root.join(engine.as_str())) else {
            continue;
        };
        for tag in tags.flatten() {
            for variant in Variant::ALL {
                let dir = tag.path().join(variant.as_str());
                if let Some(record) = read_record(&dir) {
                    if record.engine == engine {
                        found.push(record);
                    }
                }
            }
        }
    }
    found.sort_by(|a, b| {
        (a.engine.as_str(), a.variant.as_str())
            .cmp(&(b.engine.as_str(), b.variant.as_str()))
            .then_with(|| compare_tags(&a.tag, &b.tag))
    });
    found
}

/// The installed package of `engine` and `variant` with the newest tag.
#[must_use]
pub fn newest_installed(root: &Path, engine: Engine, variant: Variant) -> Option<InstalledPackage> {
    installed_packages(root)
        .into_iter()
        .filter(|p| p.engine == engine && p.variant == variant)
        .max_by(|a, b| compare_tags(&a.tag, &b.tag))
}

/// The `llama-server` binary inside an installed package, searched a few
/// levels deep since upstream archives nest it under a versioned folder.
#[must_use]
pub fn find_binary(dir: &Path, name: &str) -> Option<PathBuf> {
    fn walk(dir: &Path, name: &str, depth: u8) -> Option<PathBuf> {
        let direct = dir.join(name);
        if direct.is_file() {
            return Some(direct);
        }
        if depth == 0 {
            return None;
        }
        let mut subdirs: Vec<PathBuf> = std::fs::read_dir(dir)
            .ok()?
            .flatten()
            .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
            .map(|e| e.path())
            .collect();
        subdirs.sort();
        subdirs.into_iter().find_map(|d| walk(&d, name, depth - 1))
    }
    walk(dir, name, 3)
}

/// The engine variant the launcher wants on this hardware, when the store
/// holds one: CUDA on a machine with an NVIDIA card, nothing otherwise.
#[must_use]
pub fn wanted_llama_variant(nvidia_detected: bool) -> Option<Variant> {
    nvidia_detected.then_some(Variant::Cuda)
}

/// The installed `llama-server` the launcher should take, or `None` to keep
/// the embedded one.
///
/// `nvidia_detected` is only asked for when a CUDA package is installed, since
/// detecting the card spawns `nvidia-smi`.
pub fn preferred_llama_server(
    root: &Path,
    binary_name: &str,
    nvidia_detected: impl FnOnce() -> bool,
) -> Option<PathBuf> {
    let newest = newest_installed(root, Engine::LlamaServer, Variant::Cuda)?;
    let variant = wanted_llama_variant(nvidia_detected())?;
    if variant != newest.variant {
        return None;
    }
    find_binary(&newest.path, binary_name)
}
