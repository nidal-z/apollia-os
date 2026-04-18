//! Tests d'intégration — chargement de modèles GGUF multi-shards.
//!
//! Ces tests exercent [`EmbeddedBackend::load`] de bout en bout sur des
//! arrangements filesystem réalistes. Les cas d'erreur (shards manquants,
//! mauvais index, chemin inexistant) ne nécessitent aucune initialisation
//! `llama.cpp` : la validation remonte l'erreur via `detect_split_shards`
//! AVANT tout appel FFI (principe #4 — Fail fast).
//!
//! Les cas qui vont jusqu'au FFI (fichiers placeholders de 1 octet) voient
//! `llama_model_load_from_file` rejeter le contenu invalide — donc retour
//! propre en [`LlmError::InferenceError`], sans panique.
//!
//! # Test réel optionnel
//!
//! `test_real_split_model_loads_when_env_points_to_first_shard` est `#[ignore]`
//! par défaut. Pour l'activer depuis un shell local disposant d'un modèle split
//! réel :
//!
//! ```text
//! APOLLIA_TEST_SPLIT_MODEL=/path/to/Model-00001-of-0000N.gguf \
//!   cargo test -p apollia-llm --features local \
//!   --test test_multi_shard -- --ignored
//! ```
//!
//! # Isolation de `HOME`
//!
//! `test_tilde_expansion_resolves_split_shards_under_home` modifie temporairement
//! la variable d'environnement `HOME`. Un [`Mutex`] dédié sérialise toute
//! modification de `HOME` à l'intérieur de ce binaire de test, et un garde RAII
//! restaure la valeur précédente même en cas de panique.
//!
//! Ce fichier n'est compilé qu'avec la feature Cargo `local` — sans elle, le
//! module [`apollia_llm::backends::embedded`] est absent.

#![cfg(feature = "local")]

use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use std::time::Instant;

use apollia_llm::backends::embedded::{AcceleratorDevice, EmbeddedBackend, EmbeddedBackendConfig};
use apollia_llm::types::LlmError;
use tempfile::TempDir;
use tokio::sync::Mutex;

/// Construit une config de backend pointant sur un unique `model_path` (CPU).
fn make_config(name: &str, path: PathBuf) -> EmbeddedBackendConfig {
    EmbeddedBackendConfig {
        name: name.to_string(),
        model_path: Some(path),
        model_paths: None,
        quantization: "Q4_K_M".to_string(),
        device: AcceleratorDevice::Cpu,
    }
}

/// Crée un fichier placeholder de 1 octet au chemin `dir/name` et retourne ce
/// chemin complet.
fn touch(dir: &Path, name: &str) -> PathBuf {
    let p = dir.join(name);
    std::fs::write(&p, b"x").expect("write placeholder shard");
    p
}

/// Sérialise les modifications de `HOME` à l'intérieur de ce binaire de test
/// (`std::env::set_var` n'est pas thread-safe). Un [`tokio::sync::Mutex`] est
/// utilisé pour pouvoir garder le guard vivant à travers un `.await`.
static HOME_GUARD: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

/// Garde RAII qui restaure la valeur de `HOME` à la sortie du scope, même en
/// cas de panique.
struct HomeGuard {
    original: Option<String>,
}

impl HomeGuard {
    /// Sauvegarde la valeur courante de `HOME` et la remplace par `new_home`.
    fn set(new_home: &Path) -> Self {
        let original = std::env::var("HOME").ok();
        std::env::set_var("HOME", new_home);
        Self { original }
    }
}

impl Drop for HomeGuard {
    fn drop(&mut self) {
        match &self.original {
            Some(h) => std::env::set_var("HOME", h),
            None => std::env::remove_var("HOME"),
        }
    }
}

#[tokio::test]
async fn test_complete_split_reaches_llama_cpp_and_errors_on_invalid_content() {
    let dir = TempDir::new().expect("create tempdir");
    let first = touch(dir.path(), "model-00001-of-00003.gguf");
    touch(dir.path(), "model-00002-of-00003.gguf");
    touch(dir.path(), "model-00003-of-00003.gguf");

    let result = EmbeddedBackend::load(&make_config("integration", first)).await;

    match result {
        Err(LlmError::InferenceError(_)) => {}
        Err(other) => panic!(
            "expected InferenceError from llama.cpp after valid shard detection, got {other:?}"
        ),
        Ok(_) => panic!("1-byte placeholder must not produce a usable model"),
    }
}

#[tokio::test]
async fn test_missing_shard_errors_before_touching_llama_cpp() {
    let dir = TempDir::new().expect("create tempdir");
    let first = touch(dir.path(), "model-00001-of-00003.gguf");
    touch(dir.path(), "model-00002-of-00003.gguf");
    // shard 00003 intentionnellement absent

    let started = Instant::now();
    let result = EmbeddedBackend::load(&make_config("integration", first)).await;
    let elapsed = started.elapsed();

    match result {
        Err(LlmError::ModelShardMissing {
            total,
            found,
            prefix,
            expected,
        }) => {
            assert_eq!(total, 3);
            assert_eq!(found, 2);
            assert_eq!(prefix, "model");
            assert!(
                expected
                    .file_name()
                    .and_then(|s| s.to_str())
                    .map(|s| s == "model-00003-of-00003.gguf")
                    .unwrap_or(false),
                "expected missing path to point at shard 00003, got {}",
                expected.display()
            );
        }
        other => panic!("expected ModelShardMissing, got {other:?}"),
    }
    assert!(
        elapsed.as_millis() < 500,
        "fail-fast violated: shard validation took {elapsed:?}"
    );
}

#[tokio::test]
async fn test_pointing_at_non_first_shard_errors_immediately() {
    let dir = TempDir::new().expect("create tempdir");
    let second = touch(dir.path(), "model-00002-of-00003.gguf");

    let result = EmbeddedBackend::load(&make_config("integration", second.clone())).await;

    match result {
        Err(LlmError::ShardIndexNotFirst { given_index, path }) => {
            assert_eq!(given_index, 2);
            assert!(
                path.file_name()
                    .and_then(|s| s.to_str())
                    .map(|s| s == "model-00002-of-00003.gguf")
                    .unwrap_or(false),
                "path must point at the shard the user provided, got {}",
                path.display()
            );
        }
        other => panic!("expected ShardIndexNotFirst, got {other:?}"),
    }
}

#[tokio::test]
async fn test_mono_file_path_reaches_llama_cpp_without_shard_detection() {
    let dir = TempDir::new().expect("create tempdir");
    let p = touch(dir.path(), "Qwen3-8B-Q5_K_M.gguf");

    let result = EmbeddedBackend::load(&make_config("integration", p)).await;

    match result {
        Err(LlmError::InferenceError(_)) => {}
        Err(LlmError::ModelShardMissing { .. } | LlmError::ShardIndexNotFirst { .. }) => {
            panic!("mono-file path must not be treated as a split entry");
        }
        Err(other) => panic!("expected InferenceError from llama.cpp, got {other:?}"),
        Ok(_) => panic!("1-byte placeholder must not load successfully"),
    }
}

#[tokio::test]
async fn test_tilde_expansion_resolves_split_shards_under_home() {
    let _serial = HOME_GUARD.lock().await;

    let dir = TempDir::new().expect("create tempdir");
    let models = dir.path().join(".apollia").join("models");
    std::fs::create_dir_all(&models).expect("create models dir");
    for i in 1..=3 {
        std::fs::write(models.join(format!("Llama-{i:05}-of-00003.gguf")), b"x")
            .expect("write split shard");
    }

    let _home = HomeGuard::set(dir.path());

    let config = make_config(
        "integration",
        PathBuf::from("~/.apollia/models/Llama-00001-of-00003.gguf"),
    );
    let result = EmbeddedBackend::load(&config).await;

    match result {
        Err(LlmError::InferenceError(_)) => {}
        Err(LlmError::ModelShardMissing { .. }) => {
            panic!("tilde expansion failed: shards not found under $HOME override");
        }
        Err(LlmError::ModelNotFound { path }) => {
            panic!(
                "tilde expansion failed: canonicalize rejected expanded path {}",
                path.display()
            );
        }
        Err(other) => {
            panic!("expected InferenceError after successful shard detection, got {other:?}")
        }
        Ok(_) => panic!("1-byte placeholders must not load successfully"),
    }
}

#[tokio::test]
async fn test_missing_file_returns_model_not_found_before_shard_detection() {
    let dir = TempDir::new().expect("create tempdir");
    let nonexistent = dir.path().join("apollia-missing-integration-xyz.gguf");
    let config = make_config("integration", nonexistent.clone());

    let result = EmbeddedBackend::load(&config).await;

    match result {
        Err(LlmError::ModelNotFound { path }) => {
            assert!(
                path.to_string_lossy()
                    .contains("apollia-missing-integration-xyz"),
                "ModelNotFound path must echo the input path, got {}",
                path.display()
            );
        }
        other => panic!("expected ModelNotFound, got {other:?}"),
    }
}

#[tokio::test]
#[ignore = "requires APOLLIA_TEST_SPLIT_MODEL pointing at a real first shard"]
async fn test_real_split_model_loads_when_env_points_to_first_shard() {
    let path = match std::env::var("APOLLIA_TEST_SPLIT_MODEL") {
        Ok(p) if !p.is_empty() => PathBuf::from(p),
        _ => return,
    };

    let file_name = path
        .file_name()
        .and_then(|s| s.to_str())
        .expect("APOLLIA_TEST_SPLIT_MODEL must have a UTF-8 file name");
    let expected_prefix = file_name
        .strip_suffix(".gguf")
        .and_then(|stem| stem.split("-00001-of-").next())
        .expect("APOLLIA_TEST_SPLIT_MODEL must point at the first shard (-00001-of-NNNNN.gguf)")
        .to_string();

    let config = make_config("real-split", path);
    let backend = EmbeddedBackend::load(&config)
        .await
        .expect("loading a real split model must succeed");

    use apollia_llm::types::CompletionModel;
    assert_eq!(backend.model_id(), expected_prefix);
}
