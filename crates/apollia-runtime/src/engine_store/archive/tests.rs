#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::io::Write;
use std::path::PathBuf;

use super::*;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/engine_store")
        .join(name)
}

/// A zip at `path` holding `entries` of `(name, body)`, and one symbolic link
/// `(name, target)` when given.
fn zip_with(path: &std::path::Path, entries: &[(&str, &[u8])], link: Option<(&str, &str)>) {
    let file = std::fs::File::create(path).unwrap();
    let mut zip = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default();
    for (name, body) in entries {
        zip.start_file(*name, options).unwrap();
        zip.write_all(body).unwrap();
    }
    if let Some((name, target)) = link {
        zip.add_symlink(name, target, options).unwrap();
    }
    zip.finish().unwrap();
}

#[test]
fn test_upstream_shaped_zip_extracts_files_modes_and_sibling_links() {
    // GIVEN the fixture archive, a nested binary and a library link
    let out = tempfile::tempdir().unwrap();
    let target = out.path().join("x");

    // WHEN it is extracted
    extract(&fixture("llama-b6000-cuda.zip"), ArchiveKind::Zip, &target).unwrap();

    // THEN the binary is there, executable, and the link resolves to its sibling
    let bin = target.join("llama-b6000/llama-server");
    assert!(bin.is_file());
    assert!(target.join("llama-b6000/llama-server.exe").is_file());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&bin).unwrap().permissions().mode();
        assert_eq!(mode & 0o111, 0o111);
        let link = target.join("llama-b6000/libggml.dylib");
        assert_eq!(std::fs::read(link).unwrap(), b"library\n");
    }
}

#[test]
fn test_entry_climbing_out_is_refused_and_target_removed() {
    // GIVEN a zip whose entry climbs out of the root
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a.zip");
    zip_with(&path, &[("ok", b"x"), ("../evil", b"x")], None);
    let target = dir.path().join("x");

    // WHEN it is extracted
    let err = extract(&path, ArchiveKind::Zip, &target).unwrap_err();

    // THEN it is refused, nothing escaped, and the target is gone
    assert!(matches!(err, EngineStoreError::Archive { .. }), "{err}");
    assert!(!dir.path().join("evil").exists());
    assert!(!target.exists());
}

#[test]
fn test_absolute_entry_is_refused() {
    // GIVEN a zip with an absolute entry
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a.zip");
    zip_with(&path, &[("/tmp/evil", b"x")], None);

    // WHEN it is extracted
    let err = extract(&path, ArchiveKind::Zip, &dir.path().join("x")).unwrap_err();

    // THEN it is refused
    assert!(matches!(err, EngineStoreError::Archive { .. }), "{err}");
}

#[test]
fn test_link_out_of_its_directory_is_refused() {
    // GIVEN a zip with a link that climbs, and one that is absolute
    for target in ["../../etc/passwd", "/etc/passwd"] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.zip");
        zip_with(&path, &[], Some(("d/l", target)));

        // WHEN it is extracted
        let err = extract(&path, ArchiveKind::Zip, &dir.path().join("x")).unwrap_err();

        // THEN it is refused
        assert!(
            matches!(err, EngineStoreError::Archive { .. }),
            "{target}: {err}"
        );
    }
}

#[test]
fn test_unreadable_archive_is_refused() {
    // GIVEN bytes that are not a zip
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a.zip");
    std::fs::write(&path, b"not a zip").unwrap();

    // WHEN it is extracted
    let err = extract(&path, ArchiveKind::Zip, &dir.path().join("x")).unwrap_err();

    // THEN it is refused
    assert!(err.to_string().contains("unreadable zip"), "{err}");
}

#[test]
fn test_tar_gz_package_is_refused_by_the_manifest() {
    // GIVEN a package address in the upstream tar.gz form
    // WHEN its kind is asked
    // THEN it is not one the store reads
    assert_eq!(
        crate::engine_store::archive_kind("llama-b6000-bin-macos-arm64.tar.gz"),
        None
    );
    assert_eq!(
        crate::engine_store::archive_kind("a.zip"),
        Some(ArchiveKind::Zip)
    );
}
