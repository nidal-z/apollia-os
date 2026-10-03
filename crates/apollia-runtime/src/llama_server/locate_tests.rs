//! The order in which the launcher looks for `llama-server` before the
//! embedded engine: the environment override, then the engine store.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::PathBuf;

use super::env_or_store_binary;
use crate::engine_store::{import_from_dir, EngineStore, MANIFEST_FILE, SIGNATURE_FILE};

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/engine_store")
            .join(name),
    )
    .unwrap()
}

/// A store under `dir` holding the verified b6000 CUDA build.
fn store_with_cuda(dir: &std::path::Path) -> PathBuf {
    let store = EngineStore {
        root: dir.join("engines"),
        trusted_key: String::from_utf8(fixture("test-dist.key.pub")).unwrap(),
        platform: "linux-x64".to_owned(),
    };
    let usb = tempfile::tempdir().unwrap();
    for (to, from) in [
        (MANIFEST_FILE, "stable-5.json"),
        (SIGNATURE_FILE, "stable-5.json.minisig"),
        ("llama-b6000-cuda.zip", "llama-b6000-cuda.zip"),
    ] {
        std::fs::write(usb.path().join(to), fixture(from)).unwrap();
    }
    import_from_dir(&store, usb.path()).unwrap();
    store.root
}

#[test]
fn test_environment_override_wins_over_the_store() {
    // GIVEN a CUDA build in the store, an NVIDIA card, and an override
    let dir = tempfile::tempdir().unwrap();
    let root = store_with_cuda(dir.path());
    let env_bin = dir.path().join("my-llama-server");
    std::fs::write(&env_bin, b"binary").unwrap();

    // WHEN the binary is looked for
    let chosen = env_or_store_binary(Some(env_bin.clone()), Some(&root), "llama-server", || true);

    // THEN the override is taken
    assert_eq!(chosen, Some(env_bin));
}

#[test]
fn test_store_cuda_build_is_taken_on_an_nvidia_machine() {
    // GIVEN a CUDA build in the store, an NVIDIA card, and no override
    let dir = tempfile::tempdir().unwrap();
    let root = store_with_cuda(dir.path());

    // WHEN the binary is looked for
    let chosen = env_or_store_binary(None, Some(&root), "llama-server", || true).unwrap();

    // THEN the store's CUDA build is taken
    assert!(chosen.starts_with(root.join("llama-server/b6000/cuda")));
}

#[test]
fn test_embedded_engine_is_kept_without_an_nvidia_card() {
    // GIVEN a CUDA build in the store, no NVIDIA card, an invalid override
    let dir = tempfile::tempdir().unwrap();
    let root = store_with_cuda(dir.path());
    let missing = dir.path().join("absent");

    // WHEN the binary is looked for
    let chosen = env_or_store_binary(Some(missing), Some(&root), "llama-server", || false);

    // THEN nothing is chosen here: the embedded engine is next
    assert_eq!(chosen, None);
}
