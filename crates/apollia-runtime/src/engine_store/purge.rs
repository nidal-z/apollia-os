//! The automatic removal of the engine builds used the least.
//!
//! An update leaves the build it replaces in the store, so a variant gathers
//! one directory per tag it ever ran. With `[engines] auto_purge` on, each
//! update of a variant is followed by a purge of that variant: two builds are
//! kept, the newest tag, which is the one the launcher takes, and the build
//! used the most recently among the others. The rest is deleted, the build
//! left unused the longest first.
//!
//! The engine shipped with the application lives outside the store and is
//! never looked at.
//!
//! A build is used when a launcher takes it: the launcher then writes the
//! time beside its record ([`mark_used`]). A build no launcher ever took
//! counts from the time it was installed.

use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::engine_store::installed::{installed_packages, InstalledPackage, RECORD_FILE};
use crate::engine_store::manifest::{compare_tags, Engine, Variant};

/// Name of the file, beside the record of a package, that holds the time a
/// launcher last took it, in milliseconds since the Unix epoch.
pub const LAST_USED_FILE: &str = ".apollia-engine.last-used";

/// How many builds of one variant a purge leaves in the store.
pub const KEPT_PER_VARIANT: usize = 2;

/// Record that a launcher takes the package `path` lies in, now.
///
/// `path` is the package directory or anything under it, the engine binary
/// for instance. A path outside any package records nothing. A failure to
/// write is logged and never keeps an engine from starting.
pub fn mark_used(path: &Path) {
    mark_used_at(path, SystemTime::now());
}

/// [`mark_used`] with the time given.
pub fn mark_used_at(path: &Path, at: SystemTime) {
    let Some(dir) = path.ancestors().find(|dir| dir.join(RECORD_FILE).is_file()) else {
        return;
    };
    let millis = at
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_millis());
    if let Err(error) = std::fs::write(dir.join(LAST_USED_FILE), millis.to_string()) {
        tracing::event!(
            tracing::Level::WARN,
            path = %dir.display(),
            error = %error,
            "engine_store.purge.mark_failed"
        );
    }
}

/// When a launcher last took `package`, or when it was installed if none did.
#[must_use]
pub fn last_used(package: &InstalledPackage) -> SystemTime {
    let marked = std::fs::read_to_string(package.path.join(LAST_USED_FILE))
        .ok()
        .and_then(|text| text.trim().parse::<u64>().ok())
        .map(|millis| UNIX_EPOCH + Duration::from_millis(millis));
    marked
        .or_else(|| {
            std::fs::metadata(package.path.join(RECORD_FILE))
                .and_then(|meta| meta.modified())
                .ok()
        })
        .unwrap_or(UNIX_EPOCH)
}

/// Delete the builds of `engine` and `variant` beyond [`KEPT_PER_VARIANT`],
/// the one left unused the longest first, and return what was deleted.
///
/// The newest tag is the build in service and is never deleted. A build that
/// cannot be deleted is logged and left where it is.
pub fn purge_variant(root: &Path, engine: Engine, variant: Variant) -> Vec<InstalledPackage> {
    let mut builds: Vec<InstalledPackage> = installed_packages(root)
        .into_iter()
        .filter(|p| p.engine == engine && p.variant == variant)
        .collect();
    if builds.len() <= KEPT_PER_VARIANT {
        return Vec::new();
    }
    // The newest tag first, then the others from the most recently used.
    builds.sort_by(|a, b| compare_tags(&b.tag, &a.tag));
    builds[1..].sort_by_key(|package| std::cmp::Reverse(last_used(package)));

    let mut removed = Vec::new();
    for package in builds.split_off(KEPT_PER_VARIANT) {
        match delete(&package) {
            Ok(()) => {
                tracing::event!(
                    tracing::Level::INFO,
                    engine = engine.as_str(),
                    variant = variant.as_str(),
                    tag = %package.tag,
                    "engine_store.purge.deleted"
                );
                removed.push(package);
            }
            Err(error) => {
                tracing::event!(
                    tracing::Level::WARN,
                    engine = engine.as_str(),
                    variant = variant.as_str(),
                    tag = %package.tag,
                    error = %error,
                    "engine_store.purge.delete_failed"
                );
            }
        }
    }
    removed
}

/// Delete one package. Its record goes first, so a directory half deleted is
/// no longer an engine; the tag directory goes too once it is empty.
fn delete(package: &InstalledPackage) -> std::io::Result<()> {
    std::fs::remove_file(package.path.join(RECORD_FILE))?;
    std::fs::remove_dir_all(&package.path)?;
    if let Some(tag_dir) = package.path.parent() {
        // Refused while another variant of the tag is still there.
        let _ = std::fs::remove_dir(tag_dir);
    }
    Ok(())
}

/// What follows an update: when `enabled`, purge the variant of each build in
/// `updated`, and return what was deleted. Off, nothing is touched.
pub fn purge_after_update(
    root: &Path,
    enabled: bool,
    updated: &[InstalledPackage],
) -> Vec<InstalledPackage> {
    if !enabled {
        return Vec::new();
    }
    let mut seen: Vec<(Engine, Variant)> = Vec::new();
    let mut removed = Vec::new();
    for package in updated {
        let pair = (package.engine, package.variant);
        if seen.contains(&pair) {
            continue;
        }
        seen.push(pair);
        removed.extend(purge_variant(root, package.engine, package.variant));
    }
    removed
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::engine_store::installed::{package_dir, Source};

    /// File a verified build of `tag` in the store, last used `hours_ago`.
    fn file(root: &Path, variant: Variant, tag: &str, hours_ago: Option<u64>) -> InstalledPackage {
        let dir = package_dir(root, Engine::LlamaServer, tag, variant);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("llama-server"), b"binary").unwrap();
        let package = InstalledPackage {
            engine: Engine::LlamaServer,
            tag: tag.to_owned(),
            variant,
            platform: "linux-x64".to_owned(),
            sha256: "0".repeat(64),
            source: Source::Downloaded,
            path: dir.clone(),
        };
        std::fs::write(dir.join(RECORD_FILE), serde_json::to_vec(&package).unwrap()).unwrap();
        if let Some(hours) = hours_ago {
            let at = SystemTime::now() - Duration::from_secs(hours * 3600);
            mark_used_at(&dir.join("llama-server"), at);
        }
        package
    }

    fn tags(root: &Path, variant: Variant) -> Vec<String> {
        installed_packages(root)
            .into_iter()
            .filter(|p| p.variant == variant)
            .map(|p| p.tag)
            .collect()
    }

    #[test]
    fn test_purge_deletes_the_build_left_unused_the_longest() {
        // GIVEN three filed tags of the CUDA variant: b5000 last used a month
        // ago, b6000 an hour ago, and b7000 just installed by an update
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        file(root, Variant::Cuda, "b5000", Some(720));
        file(root, Variant::Cuda, "b6000", Some(1));
        let updated = file(root, Variant::Cuda, "b7000", None);

        // WHEN the purge that follows the update runs, the setting on
        let removed = purge_after_update(root, true, &[updated]);

        // THEN the oldest unused build is deleted, and only it
        assert_eq!(
            removed.iter().map(|p| p.tag.as_str()).collect::<Vec<_>>(),
            ["b5000"]
        );
        assert_eq!(tags(root, Variant::Cuda), ["b6000", "b7000"]);
        assert!(!package_dir(root, Engine::LlamaServer, "b5000", Variant::Cuda).exists());
        assert!(!root.join("llama-server").join("b5000").exists());
    }

    #[test]
    fn test_purge_never_deletes_the_build_in_service() {
        // GIVEN three filed tags where the newest, the one the launcher
        // takes, is the build used the longest ago
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        file(root, Variant::Cuda, "b5000", Some(2));
        file(root, Variant::Cuda, "b6000", Some(1));
        file(root, Variant::Cuda, "b7000", Some(720));

        // WHEN the variant is purged
        let removed = purge_variant(root, Engine::LlamaServer, Variant::Cuda);

        // THEN the build in service stays, and the least recently used of the
        // two others goes
        assert_eq!(removed.len(), 1);
        assert_eq!(removed[0].tag, "b5000");
        assert_eq!(tags(root, Variant::Cuda), ["b6000", "b7000"]);
    }

    #[test]
    fn test_purge_is_per_variant_and_leaves_two_builds_alone() {
        // GIVEN three tags of CUDA, and two tags of Vulkan one of which
        // shares a tag directory with a CUDA build about to be deleted
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        file(root, Variant::Cuda, "b5000", Some(720));
        file(root, Variant::Cuda, "b6000", Some(1));
        let updated = file(root, Variant::Cuda, "b7000", None);
        file(root, Variant::Vulkan, "b5000", Some(900));
        file(root, Variant::Vulkan, "b6000", Some(800));

        // WHEN the purge that follows the CUDA update runs
        let removed = purge_after_update(root, true, &[updated]);

        // THEN only the CUDA variant lost a build
        assert_eq!(removed.len(), 1);
        assert_eq!(removed[0].variant, Variant::Cuda);
        assert_eq!(tags(root, Variant::Vulkan), ["b5000", "b6000"]);
        assert!(package_dir(root, Engine::LlamaServer, "b5000", Variant::Vulkan).is_dir());
    }

    #[test]
    fn test_with_the_setting_off_an_update_deletes_nothing() {
        // GIVEN three filed tags of a variant
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        file(root, Variant::Cuda, "b5000", Some(720));
        file(root, Variant::Cuda, "b6000", Some(1));
        let updated = file(root, Variant::Cuda, "b7000", None);

        // WHEN the update is followed up with the setting off
        let removed = purge_after_update(root, false, &[updated]);

        // THEN the three builds are still there
        assert!(removed.is_empty());
        assert_eq!(tags(root, Variant::Cuda), ["b5000", "b6000", "b7000"]);
    }

    #[test]
    fn test_a_launcher_taking_a_build_records_when() {
        // GIVEN a filed build no launcher took, and a path outside the store
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let package = file(root, Variant::Cuda, "b6000", None);
        let installed = last_used(&package);
        let outside = dir.path().join("elsewhere");

        // WHEN a launcher takes its binary an hour later, and one outside
        let later = installed + Duration::from_secs(3600);
        mark_used_at(&package.path.join("llama-server"), later);
        mark_used(&outside);

        // THEN the build counts from that time, and nothing was written
        // outside a package
        let recorded = last_used(&package);
        let drift = recorded
            .duration_since(later)
            .or_else(|_| later.duration_since(recorded))
            .unwrap();
        assert!(drift < Duration::from_millis(2), "{drift:?}");
        assert!(!dir.path().join(LAST_USED_FILE).exists());
    }
}
