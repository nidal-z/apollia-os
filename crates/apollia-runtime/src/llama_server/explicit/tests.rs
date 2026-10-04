use std::path::{Path, PathBuf};

use apollia_core::model_preset::{ModelSource, PresetDraft, RequestSettings};
use apollia_core::LlmBackendRepository;
use apollia_llm::hardware::{AcceleratorProfile, CompatibilityBadge};
use serde_json::json;

use super::*;
use crate::llama_server::test_support::supervisor_with_ceiling;
use crate::llama_server::{fake_engine, fake_engine_launches};

/// The header of Qwen2.5 0.5B Instruct as published, reduced to the keys the
/// estimator reads, written byte for byte, then padded to `weights_bytes` so
/// the file has the size of the weights it stands for.
pub(crate) fn recorded_gguf(dir: &Path, weights_bytes: u64) -> PathBuf {
    fn string(out: &mut Vec<u8>, s: &str) {
        out.extend_from_slice(&(s.len() as u64).to_le_bytes());
        out.extend_from_slice(s.as_bytes());
    }
    let pairs: [(&str, u32); 5] = [
        ("qwen2.block_count", 24),
        ("qwen2.context_length", 32_768),
        ("qwen2.embedding_length", 896),
        ("qwen2.attention.head_count", 14),
        ("qwen2.attention.head_count_kv", 2),
    ];
    let mut out = Vec::new();
    out.extend_from_slice(b"GGUF");
    out.extend_from_slice(&3u32.to_le_bytes());
    out.extend_from_slice(&0u64.to_le_bytes());
    out.extend_from_slice(&(pairs.len() as u64 + 1).to_le_bytes());
    string(&mut out, "general.architecture");
    out.extend_from_slice(&8u32.to_le_bytes());
    string(&mut out, "qwen2");
    for (key, value) in pairs {
        string(&mut out, key);
        out.extend_from_slice(&4u32.to_le_bytes());
        out.extend_from_slice(&value.to_le_bytes());
    }
    let path = dir.join("qwen2.5-0.5b-instruct-q8_0.gguf");
    std::fs::write(&path, &out).unwrap();
    std::fs::OpenOptions::new()
        .write(true)
        .open(&path)
        .unwrap()
        .set_len(weights_bytes)
        .unwrap();
    path
}

fn profile(accelerator: AcceleratorProfile, ram_gb: f64, budget_gb: f64) -> HardwareProfile {
    HardwareProfile {
        total_ram_gb: ram_gb,
        available_ram_gb: ram_gb,
        cpu_model: "test".to_owned(),
        cpu_cores: 8,
        accelerator,
        memory_budget_gb: budget_gb,
    }
}

fn cuda(vram_gb: f64) -> HardwareProfile {
    profile(
        AcceleratorProfile::Cuda {
            device_name: "test".to_owned(),
            vram_gb,
            compute_capability: (8, 9),
        },
        32.0,
        vram_gb,
    )
}

fn config(model: &Path, n_ctx: u32, n_gpu_layers: i32) -> LlamaServerConfig {
    LlamaServerConfig {
        model_path: model.display().to_string(),
        n_ctx,
        n_gpu_layers,
        ..LlamaServerConfig::default()
    }
}

const HALF_GIB: u64 = 512 * 1024 * 1024;

#[test]
fn test_the_estimate_of_a_recorded_header_grows_with_the_window_asked() {
    // GIVEN a recorded Qwen2.5 0.5B header and a machine without accelerator
    let dir = tempfile::tempdir().unwrap();
    let model = recorded_gguf(dir.path(), HALF_GIB);
    let machine = profile(AcceleratorProfile::None, 16.0, 9.6);

    // WHEN a load is estimated at 4 096 and at 32 768 tokens
    let small = estimate_config(&config(&model, 4096, 0), &machine).unwrap();
    let large = estimate_config(&config(&model, 32_768, 0), &machine).unwrap();

    // THEN the cache is measured from the header, the larger window costs
    // more, nothing sits on an accelerator, and the guardrail judges the same
    // total, graded against the machine
    assert!(small.kv_cache_measured && large.kv_cache_measured);
    assert_eq!((small.n_ctx, large.n_ctx), (4096, 32_768));
    assert_eq!(small.total_layers, 24);
    assert!(large.total_gb > small.total_gb, "{small:?} {large:?}");
    assert_eq!(small.gpu_gb, 0.0);
    assert!((small.total_gb - small.guardrail.total_gb).abs() < f64::EPSILON);
    assert_eq!(small.guardrail.capacity_gb, 9.6);
    assert_eq!(small.guardrail.badge, CompatibilityBadge::Fits);
}

#[test]
fn test_the_accelerator_share_follows_the_offload_asked() {
    // GIVEN the recorded header on a machine with a discrete card that holds it
    let dir = tempfile::tempdir().unwrap();
    let model = recorded_gguf(dir.path(), HALF_GIB);
    let machine = cuda(8.0);

    // WHEN the load is estimated with no layer, half of them, and all of them
    // on the card
    let none = estimate_config(&config(&model, 8192, 0), &machine).unwrap();
    let half = estimate_config(&config(&model, 8192, 12), &machine).unwrap();
    let all = estimate_config(&config(&model, 8192, 999), &machine).unwrap();

    // THEN the card holds nothing, then part of the weights with the cache,
    // then the whole model, and the total does not depend on the placement
    assert_eq!(none.gpu_gb, 0.0);
    assert!(
        half.gpu_gb > 0.0 && half.gpu_gb < all.gpu_gb,
        "{half:?} {all:?}"
    );
    assert!(all.gpu_gb <= all.total_gb);
    assert!((half.total_gb - all.total_gb).abs() < f64::EPSILON);
}

#[test]
fn test_an_unreadable_model_has_no_estimate() {
    // GIVEN a path where no file is
    // WHEN a load of it is estimated
    let estimate = estimate_config(
        &config(Path::new("/nonexistent/model.gguf"), 4096, 0),
        &cuda(8.0),
    );

    // THEN there is none, rather than a figure made up
    assert!(estimate.is_none());
}

fn launch_lines(dir: &Path) -> Vec<String> {
    fake_engine_launches(dir)
        .into_iter()
        .filter(|line| line.contains("--port"))
        .collect()
}

#[cfg(unix)]
#[tokio::test]
async fn test_a_load_by_hand_launches_with_the_window_and_offload_asked() {
    // GIVEN a stand-in engine and a model whose default preset sets six threads
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("system.db");
    LlmBackendRepository::open(&db)
        .unwrap()
        .create_preset(
            ModelSource::Local,
            "qwen",
            &PresetDraft {
                name: "main".to_owned(),
                load: json!({ "n_threads": 6, "n_ctx": 4096 })
                    .as_object()
                    .cloned()
                    .unwrap(),
                request: RequestSettings::default(),
                is_model_default: true,
            },
        )
        .unwrap();
    let mut sup = supervisor_with_ceiling(1);
    sup.bin_path = fake_engine(dir.path());
    sup.system_db = Some(db.clone());

    // WHEN it is loaded by hand at 16 384 tokens with twelve layers offloaded
    let overrides = json!({ "n_ctx": 16_384, "n_gpu_layers": 12 });
    let load = sup
        .explicit_settings("qwen", overrides.as_object().unwrap())
        .await;
    let lease = sup
        .load_explicitly(
            "qwen",
            "/models/qwen.gguf".to_owned(),
            load,
            LoadConsent::Unconfirmed,
        )
        .await
        .unwrap();

    // THEN the engine was launched with the window and offload asked, over
    // the preset's other settings, and the preset itself is unchanged
    let lines = launch_lines(dir.path());
    assert_eq!(lines.len(), 1, "{lines:#?}");
    assert!(lines[0].contains("-c 16384 "), "{}", lines[0]);
    assert!(lines[0].contains("-ngl 12 "), "{}", lines[0]);
    assert!(lines[0].contains(" -t 6 "), "{}", lines[0]);
    assert_eq!(lease.n_ctx(), 16_384);
    let preset = LlmBackendRepository::open(&db)
        .unwrap()
        .list_presets(Some((ModelSource::Local, "qwen")))
        .unwrap();
    assert_eq!(preset[0].load.get("n_ctx"), Some(&json!(4096)));
    drop(lease);
    sup.shutdown_in_place().await;
}

#[cfg(unix)]
#[tokio::test]
async fn test_a_request_without_a_window_is_served_by_the_model_loaded_by_hand() {
    // GIVEN a model loaded by hand at 16 384 tokens, the default being 32 768
    let dir = tempfile::tempdir().unwrap();
    let mut sup = supervisor_with_ceiling(1);
    sup.bin_path = fake_engine(dir.path());
    let load = sup
        .explicit_settings("qwen", json!({ "n_ctx": 16_384 }).as_object().unwrap())
        .await;
    let loaded = sup
        .load_explicitly(
            "qwen",
            "/models/qwen.gguf".to_owned(),
            load,
            LoadConsent::Unconfirmed,
        )
        .await
        .unwrap();
    let url = loaded.base_url().to_owned();
    drop(loaded);

    // WHEN a request for that model arrives without a window, then with one
    let lazy = sup
        .ensure_model(
            "qwen",
            "/models/qwen.gguf".to_owned(),
            None,
            LoadConsent::Unconfirmed,
        )
        .await
        .unwrap();
    let sized = sup
        .ensure_model(
            "qwen",
            "/models/qwen.gguf".to_owned(),
            Some(8192),
            LoadConsent::Unconfirmed,
        )
        .await
        .unwrap();

    // THEN no instance is launched: both are served by the one loaded by hand
    assert_eq!(launch_lines(dir.path()).len(), 1);
    assert_eq!(lazy.base_url(), url);
    assert_eq!(sized.base_url(), url);
    assert_eq!(lazy.n_ctx(), 16_384);
    assert_eq!(sup.instances.lock().await.len(), 1);
    drop((lazy, sized));
    sup.shutdown_in_place().await;
}

#[cfg(unix)]
#[tokio::test]
async fn test_a_load_by_hand_replaces_the_lazy_instance_of_the_model() {
    // GIVEN a model loaded lazily under the default window
    let dir = tempfile::tempdir().unwrap();
    let mut sup = supervisor_with_ceiling(2);
    sup.bin_path = fake_engine(dir.path());
    drop(
        sup.ensure_model(
            "qwen",
            "/models/qwen.gguf".to_owned(),
            None,
            LoadConsent::Unconfirmed,
        )
        .await
        .unwrap(),
    );

    // WHEN it is loaded by hand at another window
    let load = sup
        .explicit_settings("qwen", json!({ "n_ctx": 8192 }).as_object().unwrap())
        .await;
    drop(
        sup.load_explicitly(
            "qwen",
            "/models/qwen.gguf".to_owned(),
            load,
            LoadConsent::Unconfirmed,
        )
        .await
        .unwrap(),
    );

    // THEN one process serves the model, the one loaded by hand
    let instances = sup.instances.lock().await;
    assert_eq!(instances.len(), 1);
    assert_eq!(instances[0].n_ctx, 8192);
    assert!(instances[0].explicit);
    drop(instances);
    sup.shutdown_in_place().await;
}
