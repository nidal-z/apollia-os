//! The order in which the launcher looks for `llama-server` before the
//! embedded engine: the environment override, then the engine store.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::PathBuf;

use apollia_core::config::EngineChoice;

use super::env_or_store_binary;
use crate::engine_store::selection::{resolve, Resolved};
use crate::engine_store::{import_from_dir, Engine, EngineStore, MANIFEST_FILE, SIGNATURE_FILE};

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
    let chosen = env_or_store_binary(
        Some(env_bin.clone()),
        Some(&root),
        "llama-server",
        EngineChoice::Auto,
        || true,
    );

    // THEN the override is taken
    assert_eq!(chosen, Some(env_bin));
}

#[test]
fn test_store_cuda_build_is_taken_on_an_nvidia_machine() {
    // GIVEN a CUDA build in the store, an NVIDIA card, and no override
    let dir = tempfile::tempdir().unwrap();
    let root = store_with_cuda(dir.path());

    // WHEN the binary is looked for
    let chosen = env_or_store_binary(
        None,
        Some(&root),
        "llama-server",
        EngineChoice::Auto,
        || true,
    )
    .unwrap();

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
    let chosen = env_or_store_binary(
        Some(missing),
        Some(&root),
        "llama-server",
        EngineChoice::Auto,
        || false,
    );

    // THEN nothing is chosen here: the embedded engine is next
    assert_eq!(chosen, None);
}

// ── The engine the settings name for GGUF models ───────────────────────────

#[test]
fn test_a_chosen_variant_is_taken_where_the_automatic_choice_keeps_the_embedded_engine() {
    // GIVEN a CUDA build in the store and no NVIDIA card: the automatic
    // choice keeps the embedded engine
    let dir = tempfile::tempdir().unwrap();
    let root = store_with_cuda(dir.path());
    let automatic = env_or_store_binary(
        None,
        Some(&root),
        "llama-server",
        EngineChoice::Auto,
        || false,
    );

    // WHEN the settings name the CUDA variant
    let chosen = env_or_store_binary(
        None,
        Some(&root),
        "llama-server",
        EngineChoice::Cuda,
        || false,
    )
    .unwrap();

    // THEN the store's CUDA build is taken, card or no card
    assert_eq!(automatic, None);
    assert!(chosen.starts_with(root.join("llama-server/b6000/cuda")));
}

#[test]
fn test_the_embedded_choice_keeps_the_embedded_engine_whatever_the_store_holds() {
    // GIVEN a CUDA build in the store and an NVIDIA card: the automatic
    // choice takes the store's build
    let dir = tempfile::tempdir().unwrap();
    let root = store_with_cuda(dir.path());
    let mut asked = false;

    // WHEN the settings name the embedded engine
    let chosen = env_or_store_binary(
        None,
        Some(&root),
        "llama-server",
        EngineChoice::Embedded,
        || {
            asked = true;
            true
        },
    );

    // THEN nothing of the store is taken, and the card is not even probed
    assert_eq!(chosen, None);
    assert!(!asked);
}

#[test]
fn test_a_chosen_variant_since_removed_falls_back_to_the_automatic_choice() {
    // GIVEN a CUDA build in the store, an NVIDIA card, and settings naming a
    // Vulkan variant the store no longer holds
    let dir = tempfile::tempdir().unwrap();
    let root = store_with_cuda(dir.path());

    // WHEN the choice is resolved, then the binary looked for
    let resolution = resolve(Some(&root), Engine::LlamaServer, EngineChoice::Vulkan);
    let chosen = env_or_store_binary(
        None,
        Some(&root),
        "llama-server",
        EngineChoice::Vulkan,
        || true,
    );
    let automatic = env_or_store_binary(
        None,
        Some(&root),
        "llama-server",
        EngineChoice::Auto,
        || true,
    );

    // THEN the fallback is flagged, which is what raises the warning, and the
    // binary is the one the automatic choice takes
    assert!(resolution.fell_back);
    assert_eq!(resolution.resolved, Resolved::Automatic);
    assert!(chosen.is_some());
    assert_eq!(chosen, automatic);
}

#[test]
fn test_the_environment_override_wins_over_the_chosen_engine() {
    // GIVEN a CUDA build in the store, settings naming it, and an override
    let dir = tempfile::tempdir().unwrap();
    let root = store_with_cuda(dir.path());
    let env_bin = dir.path().join("my-llama-server");
    std::fs::write(&env_bin, b"binary").unwrap();

    // WHEN the binary is looked for
    let chosen = env_or_store_binary(
        Some(env_bin.clone()),
        Some(&root),
        "llama-server",
        EngineChoice::Cuda,
        || true,
    );

    // THEN the override is taken
    assert_eq!(chosen, Some(env_bin));
}
