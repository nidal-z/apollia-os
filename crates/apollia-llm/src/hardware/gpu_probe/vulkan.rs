//! Whether a Vulkan loader is installed, by file checks alone.
//!
//! No Vulkan tool is run: the SDK's information tool is often absent where the
//! loader is present, and on a machine whose driver is in a bad state it is the
//! very program that hangs. Whether the driver behind the loader works is the engine's own
//! device listing to say, after installation.

use std::path::Path;

/// Directories a distribution puts `libvulkan.so.1` in.
const LIB_DIRS: &[&str] = &[
    "usr/lib",
    "usr/lib64",
    "usr/lib/x86_64-linux-gnu",
    "usr/lib/aarch64-linux-gnu",
    "usr/local/lib",
    "lib",
    "lib64",
    "lib/x86_64-linux-gnu",
    "lib/aarch64-linux-gnu",
];

/// Directories the loader reads its driver manifests (ICD) from.
const ICD_DIRS: &[&str] = &["usr/share/vulkan/icd.d", "etc/vulkan/icd.d"];

/// A loader under `root`: the library, and at least one driver manifest.
pub(super) fn linux_loader(root: &Path) -> bool {
    let library = LIB_DIRS
        .iter()
        .any(|dir| root.join(dir).join("libvulkan.so.1").exists());
    let icd = ICD_DIRS.iter().any(|dir| {
        std::fs::read_dir(root.join(dir)).is_ok_and(|entries| {
            entries
                .flatten()
                .any(|e| e.path().extension().is_some_and(|ext| ext == "json"))
        })
    });
    library && icd
}

/// A loader under the Windows system root: `System32\vulkan-1.dll`.
pub(super) fn windows_loader(system_root: &Path) -> bool {
    system_root.join("System32").join("vulkan-1.dll").exists()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn a_library_with_a_driver_manifest_is_a_loader() {
        // GIVEN a fake root with the library and one manifest
        let root = tempfile::tempdir().expect("tempdir");
        fs::create_dir_all(root.path().join("usr/lib/x86_64-linux-gnu")).expect("lib dir");
        fs::write(
            root.path().join("usr/lib/x86_64-linux-gnu/libvulkan.so.1"),
            "",
        )
        .expect("lib");
        fs::create_dir_all(root.path().join("usr/share/vulkan/icd.d")).expect("icd dir");
        fs::write(
            root.path()
                .join("usr/share/vulkan/icd.d/radeon_icd.x86_64.json"),
            "{}",
        )
        .expect("icd");

        // WHEN the loader is looked for
        // THEN it is found
        assert!(linux_loader(root.path()));
    }

    #[test]
    fn a_library_without_any_driver_is_not_a_loader() {
        // GIVEN the library alone, and an ICD directory holding no manifest
        let root = tempfile::tempdir().expect("tempdir");
        fs::create_dir_all(root.path().join("usr/lib")).expect("lib dir");
        fs::write(root.path().join("usr/lib/libvulkan.so.1"), "").expect("lib");
        fs::create_dir_all(root.path().join("etc/vulkan/icd.d")).expect("icd dir");
        fs::write(root.path().join("etc/vulkan/icd.d/README"), "").expect("not a manifest");

        // WHEN the loader is looked for
        // THEN it is not found: the library alone reaches no card
        assert!(!linux_loader(root.path()));
    }

    #[test]
    fn a_manifest_without_the_library_is_not_a_loader() {
        // GIVEN a manifest under /etc and no library
        let root = tempfile::tempdir().expect("tempdir");
        fs::create_dir_all(root.path().join("etc/vulkan/icd.d")).expect("icd dir");
        fs::write(root.path().join("etc/vulkan/icd.d/nvidia_icd.json"), "{}").expect("icd");

        // WHEN the loader is looked for
        // THEN it is not found
        assert!(!linux_loader(root.path()));
    }

    #[test]
    fn the_windows_loader_is_the_system_dll() {
        // GIVEN a fake system root with and without `vulkan-1.dll`
        let with = tempfile::tempdir().expect("tempdir");
        fs::create_dir_all(with.path().join("System32")).expect("System32");
        fs::write(with.path().join("System32").join("vulkan-1.dll"), "").expect("dll");
        let without = tempfile::tempdir().expect("tempdir");

        // WHEN the loader is looked for in each
        // THEN only the first has one
        assert!(windows_loader(with.path()));
        assert!(!windows_loader(without.path()));
    }
}
