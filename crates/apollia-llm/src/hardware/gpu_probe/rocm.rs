//! Whether the ROCm runtime of the host is installed, by file checks alone.
//!
//! The Linux ROCm build of the engine carries no `libamdhip64`: it loads the
//! one of the host, and without it the build does not start whatever the card.
//! The library is looked for where the dynamic loader looks: the ROCm install
//! directories, the directories `ld.so.conf` names, `LD_LIBRARY_PATH`, and the
//! default library directories. Every path is taken under a root so tests read
//! a fake tree.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

/// Directories the loader searches without being told.
const DEFAULT_DIRS: &[&str] = &[
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

/// How deep `include` lines of `ld.so.conf` are followed.
const MAX_INCLUDE_DEPTH: u8 = 4;

/// Whether a `libamdhip64.so*` is in a loader directory under `root`.
/// `ld_library_path` is the value of `LD_LIBRARY_PATH`, its entries taken
/// under `root` as well.
pub(super) fn linux_runtime(root: &Path, ld_library_path: Option<&OsStr>) -> bool {
    loader_dirs(root, ld_library_path)
        .iter()
        .any(|dir| holds_hip(dir))
}

/// Every directory the loader would search, under `root`.
fn loader_dirs(root: &Path, ld_library_path: Option<&OsStr>) -> Vec<PathBuf> {
    let mut dirs = vec![root.join("opt/rocm/lib")];
    if let Ok(entries) = std::fs::read_dir(root.join("opt")) {
        dirs.extend(entries.flatten().filter_map(|entry| {
            let name = entry.file_name();
            name.to_str()?
                .starts_with("rocm-")
                .then(|| entry.path().join("lib"))
        }));
    }
    if let Some(value) = ld_library_path {
        dirs.extend(std::env::split_paths(value).map(|dir| under(root, &dir)));
    }
    conf_dirs(root, &root.join("etc/ld.so.conf"), 0, &mut dirs);
    dirs.extend(DEFAULT_DIRS.iter().map(|dir| root.join(dir)));
    dirs
}

/// `path` read as an absolute path of the machine whose root is `root`.
fn under(root: &Path, path: &Path) -> PathBuf {
    let relative: PathBuf = path
        .components()
        .filter(|c| matches!(c, std::path::Component::Normal(_)))
        .collect();
    root.join(relative)
}

/// The directories one `ld.so.conf` file names, its `include` lines followed.
fn conf_dirs(root: &Path, file: &Path, depth: u8, out: &mut Vec<PathBuf>) {
    let Ok(text) = std::fs::read_to_string(file) else {
        return;
    };
    for line in text.lines() {
        let line = line.split('#').next().unwrap_or_default().trim();
        if line.is_empty() {
            continue;
        }
        if let Some(pattern) = line.strip_prefix("include") {
            if depth < MAX_INCLUDE_DEPTH {
                for included in glob_files(root, pattern.trim()) {
                    conf_dirs(root, &included, depth + 1, out);
                }
            }
        } else {
            out.push(under(root, Path::new(line)));
        }
    }
}

/// The files an `include` pattern names: a plain path, or a `*` in its last
/// component (`/etc/ld.so.conf.d/*.conf`, the only form distributions ship).
fn glob_files(root: &Path, pattern: &str) -> Vec<PathBuf> {
    let path = under(root, Path::new(pattern));
    let Some(name) = path.file_name().and_then(OsStr::to_str) else {
        return Vec::new();
    };
    let Some((prefix, suffix)) = name.split_once('*') else {
        return vec![path];
    };
    let Some(dir) = path.parent() else {
        return Vec::new();
    };
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut files: Vec<PathBuf> = entries
        .flatten()
        .filter(|entry| {
            entry
                .file_name()
                .to_str()
                .is_some_and(|n| n.starts_with(prefix) && n.ends_with(suffix))
        })
        .map(|entry| entry.path())
        .collect();
    files.sort();
    files
}

/// Whether `dir` holds `libamdhip64.so` or one of its versioned names.
fn holds_hip(dir: &Path) -> bool {
    std::fs::read_dir(dir).is_ok_and(|entries| {
        entries.flatten().any(|entry| {
            entry
                .file_name()
                .to_str()
                .is_some_and(|n| n.starts_with("libamdhip64.so"))
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn lib(dir: &Path, name: &str) {
        fs::create_dir_all(dir).expect("lib dir");
        fs::write(dir.join(name), "").expect("lib");
    }

    #[test]
    fn the_runtime_is_found_under_a_versioned_rocm_directory() {
        // GIVEN `/opt/rocm-6.4.1/lib/libamdhip64.so.6` and no `/opt/rocm`
        let root = tempfile::tempdir().expect("tempdir");
        lib(&root.path().join("opt/rocm-6.4.1/lib"), "libamdhip64.so.6");

        // WHEN the runtime is looked for
        // THEN it is found
        assert!(linux_runtime(root.path(), None));
    }

    #[test]
    fn the_runtime_is_found_through_an_included_ld_so_conf_file() {
        // GIVEN a library in a directory only `ld.so.conf.d/rocm.conf` names
        let root = tempfile::tempdir().expect("tempdir");
        lib(&root.path().join("srv/hip/lib"), "libamdhip64.so.6.4.60401");
        fs::create_dir_all(root.path().join("etc/ld.so.conf.d")).expect("conf dir");
        fs::write(
            root.path().join("etc/ld.so.conf"),
            "# comment\ninclude /etc/ld.so.conf.d/*.conf\n",
        )
        .expect("conf");
        fs::write(
            root.path().join("etc/ld.so.conf.d/rocm.conf"),
            "/srv/hip/lib\n",
        )
        .expect("included");

        // WHEN the runtime is looked for
        // THEN it is found
        assert!(linux_runtime(root.path(), None));
    }

    #[test]
    fn the_runtime_is_found_through_ld_library_path() {
        // GIVEN a library in a directory only `LD_LIBRARY_PATH` names
        let root = tempfile::tempdir().expect("tempdir");
        lib(&root.path().join("srv/models/rocm/lib"), "libamdhip64.so");

        // WHEN the runtime is looked for with and without the variable
        let with = linux_runtime(
            root.path(),
            Some(OsStr::new("/nowhere:/srv/models/rocm/lib")),
        );
        let without = linux_runtime(root.path(), None);

        // THEN only the variable makes it found
        assert!(with);
        assert!(!without);
    }

    #[test]
    fn another_rocm_library_is_not_the_runtime() {
        // GIVEN `/opt/rocm/lib` holding rocBLAS but no HIP runtime
        let root = tempfile::tempdir().expect("tempdir");
        lib(&root.path().join("opt/rocm/lib"), "librocblas.so.4");

        // WHEN the runtime is looked for
        // THEN it is not found
        assert!(!linux_runtime(root.path(), None));
    }
}
