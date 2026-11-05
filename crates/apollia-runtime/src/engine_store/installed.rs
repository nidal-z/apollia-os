//! What the store holds, and which installed engine the launcher takes.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::engine_store::compat::{Rating, Verdict};
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
    /// Day the build was published, `YYYY-MM-DD`, when its manifest said so.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub released: Option<String>,
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
    let dir_name = dir.file_name()?.to_str()?;
    let expected = (dir_name == record.variant.as_str()
        || record.variant.former_name() == Some(dir_name))
        && dir.parent()?.file_name()?.to_str()? == record.tag
        && is_safe_component(&record.tag);
    if !expected {
        return None;
    }
    record.path = dir.to_path_buf();
    Some(record)
}

/// Every installed package of the store, verified records only, ordered by
/// engine, variant, then tag. A package an earlier version filed under the
/// former name of its variant (`cuda`) is listed under the current one.
#[must_use]
pub fn installed_packages(root: &Path) -> Vec<InstalledPackage> {
    let mut found = Vec::new();
    for engine in Engine::ALL {
        let Ok(tags) = std::fs::read_dir(root.join(engine.as_str())) else {
            continue;
        };
        for tag in tags.flatten() {
            let names = Variant::ALL
                .into_iter()
                .flat_map(|variant| std::iter::once(variant.as_str()).chain(variant.former_name()));
            for name in names {
                if let Some(record) = read_record(&tag.path().join(name)) {
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

/// The `llama-server` variants the store holds, each once, in the order of
/// [`Variant::ALL`].
#[must_use]
pub fn installed_llama_variants(root: &Path) -> Vec<Variant> {
    let held = installed_packages(root);
    Variant::ALL
        .into_iter()
        .filter(|v| {
            held.iter()
                .any(|p| p.engine == Engine::LlamaServer && p.variant == *v)
        })
        .collect()
}

/// The `llama-server` variant the automatic choice takes among `installed`,
/// by the `ratings` of this machine: the recommended one when it is
/// installed, otherwise the best compatible one in the order of
/// [`AUTOMATIC_ORDER`], otherwise none, which keeps the embedded engine. An
/// incompatible build, or one the ratings do not cover, is never taken.
#[must_use]
pub fn wanted_llama_variant(ratings: &[Rating], installed: &[Variant]) -> Option<Variant> {
    let verdict = |variant: Variant| {
        ratings
            .iter()
            .find(|r| r.variant == variant)
            .map(|r| r.verdict)
    };
    let held = |verdict_wanted: Verdict| {
        AUTOMATIC_ORDER
            .into_iter()
            .find(|v| installed.contains(v) && verdict(*v) == Some(verdict_wanted))
    };
    held(Verdict::Recommended).or_else(|| held(Verdict::Compatible))
}

/// The order in which the automatic choice prefers the compatible builds.
pub const AUTOMATIC_ORDER: [Variant; 6] = [
    Variant::Metal,
    Variant::Cuda13,
    Variant::Cuda12,
    Variant::Rocm,
    Variant::Vulkan,
    Variant::Cpu,
];

/// The installed `llama-server` the automatic choice takes, or `None` to keep
/// the embedded one.
///
/// `ratings` is only asked for when the store holds a `llama-server` build,
/// since rating the machine probes its cards.
pub fn preferred_llama_server(
    root: &Path,
    binary_name: &str,
    ratings: impl FnOnce() -> Vec<Rating>,
) -> Option<PathBuf> {
    let installed = installed_llama_variants(root);
    if installed.is_empty() {
        return None;
    }
    let variant = wanted_llama_variant(&ratings(), &installed)?;
    let newest = newest_installed(root, Engine::LlamaServer, variant)?;
    find_binary(&newest.path, binary_name)
}
