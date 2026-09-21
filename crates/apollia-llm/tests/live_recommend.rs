//! Live checks against the real HuggingFace Hub and real GGUF files.
//!
//! Ignored by default. Everything else in the recommendation path is tested
//! offline, against headers the suite builds byte by byte and a table compiled
//! into the binary. That covers the logic and cannot cover the assumptions:
//! that the Hub still answers the way the client expects, that publishers still
//! name their quantisations the way the resolver matches, and above all that a
//! range request over the first megabyte of a real model still reaches
//! `tokenizer.ggml.pre`.
//!
//! That last one is the load-bearing assumption of the whole screening stage,
//! and it rests on the order `gguf-py` writes keys in. Nothing in this
//! repository can hold upstream to that order, so it is checked against real
//! files instead of asserted in a comment.
//!
//! Run them deliberately:
//!
//! ```sh
//! cargo test -p apollia-llm --test live_recommend -- --ignored --nocapture
//! ```

#![cfg(feature = "cloud")]

use apollia_llm::gguf_probe::{probe_url, ProbeDepth};
use apollia_llm::hardware::{AcceleratorProfile, HardwareProfile};
use apollia_llm::recommend::{resolve, FamilyManifest, Recommendation, ResolveOptions};

/// A real, small, widely mirrored GGUF.
const LIVE_GGUF: &str =
    "https://huggingface.co/Qwen/Qwen3-4B-GGUF/resolve/main/Qwen3-4B-Q4_K_M.gguf";

fn client() -> reqwest::Client {
    apollia_core::net::safe_client_builder()
        .user_agent("Apollia-OS/1.0")
        .timeout(std::time::Duration::from_secs(60))
        .build()
        .expect("the TLS backend initialises")
}

fn machine(accelerator: AcceleratorProfile, ram_gb: f64, budget_gb: f64) -> HardwareProfile {
    HardwareProfile {
        total_ram_gb: ram_gb,
        available_ram_gb: ram_gb,
        cpu_model: "live-test".to_owned(),
        cpu_cores: 8,
        accelerator,
        memory_budget_gb: budget_gb,
    }
}

fn print_ranking(title: &str, ranked: &[Recommendation]) {
    println!("\n  {title}");
    for m in ranked {
        println!(
            "    {:>5.1}  {:<12} {:>4}B {:<7} {:>5.1} GB (w {:>4.1} + kv {:>4.1})  ngl {:>2}/{:<2} {:>5.0} tok/s",
            m.score,
            m.family_label,
            m.params_b,
            m.quant.as_deref().unwrap_or("-"),
            m.estimate.total_gb,
            m.estimate.weights_gb,
            m.estimate.kv_cache_gb,
            m.offload.n_gpu_layers,
            m.offload.total_layers,
            m.offload.tokens_per_second,
        );
    }
}

#[tokio::test]
#[ignore = "reaches HuggingFace; run with --ignored"]
async fn a_screening_probe_reaches_the_pre_tokenizer_of_a_real_model() {
    // GIVEN a real GGUF published on the Hub
    // WHEN only its opening megabyte is read
    let facts = probe_url(&client(), LIVE_GGUF, ProbeDepth::Screen, None)
        .await
        .expect("the screening probe reads a real header");

    // THEN the architecture, the pre-tokenizer and the declared head dimension
    // are all already known, which is what the cheap screening stage relies on
    assert_eq!(facts.architecture.as_deref(), Some("qwen3"));
    assert!(
        facts.tokenizer_pre.is_some(),
        "the pre-tokenizer was not reached inside the screening budget"
    );
    assert_eq!(
        facts.key_length,
        Some(128),
        "the declared head dimension was not read, so the cache would be derived and wrong"
    );
}

#[tokio::test]
#[ignore = "reaches HuggingFace; run with --ignored"]
async fn a_confirming_probe_reaches_the_chat_template_of_a_real_model() {
    // GIVEN the same file
    // WHEN it is read to the confirming depth
    let facts = probe_url(&client(), LIVE_GGUF, ProbeDepth::Confirm, None)
        .await
        .expect("the confirming probe reads a real header");

    // THEN the whole header was walked and the chat template is there
    assert!(
        facts.is_complete(),
        "the confirming budget did not cover the header: {} of {} pairs",
        facts.kv_read,
        facts.kv_count
    );
    assert!(facts.has_chat_template);
}

#[tokio::test]
#[ignore = "reaches HuggingFace; run with --ignored"]
async fn the_ranking_follows_the_machine_rather_than_the_model() {
    // GIVEN the shipped table and three machines with different memory shapes
    let manifest = FamilyManifest::embedded().expect("the shipped table loads");
    let options = ResolveOptions::default();

    let mac = machine(
        AcceleratorProfile::AppleSilicon {
            chip: "M4 Max".to_owned(),
            generation: 4,
            vram_gb: 64.0,
        },
        64.0,
        48.0,
    );
    let card = machine(
        AcceleratorProfile::Cuda {
            device_name: "RTX 4070".to_owned(),
            vram_gb: 12.0,
            compute_capability: (8, 9),
        },
        64.0,
        12.0,
    );
    let laptop = machine(AcceleratorProfile::None, 16.0, 16.0 * 0.60);

    // WHEN the resolver runs end to end against the Hub for each
    let on_mac = resolve(&manifest, &mac, &options)
        .await
        .expect("the Hub answers");
    let on_card = resolve(&manifest, &card, &options)
        .await
        .expect("the Hub answers");
    let on_laptop = resolve(&manifest, &laptop, &options)
        .await
        .expect("the Hub answers");

    print_ranking("64 GB M4 Max (unified)", &on_mac);
    print_ranking("12 GB RTX 4070 + 64 GB RAM (discrete)", &on_card);
    print_ranking("16 GB laptop, no GPU", &on_laptop);

    // THEN every machine gets something, every entry was measured rather than
    // guessed, and no file offered is an auxiliary module
    for (name, ranked) in [("mac", &on_mac), ("card", &on_card), ("laptop", &on_laptop)] {
        assert!(!ranked.is_empty(), "nothing resolved for the {name}");
        for m in ranked {
            assert!(m.verdict.is_offerable());
            assert!(
                m.estimate.kv_cache_measured,
                "{} was ranked on a guessed cache",
                m.file.filename
            );
            let lower = m.file.filename.to_ascii_lowercase();
            assert!(
                !lower.contains("mmproj") && !lower.contains("mtp-"),
                "an auxiliary module was offered as a model: {}",
                m.file.filename
            );
        }
    }

    // AND a Mac is offered every layer on its accelerator
    assert!(on_mac.iter().all(|m| m.offload.fully_offloaded));
    // AND a machine with no accelerator offloads nothing
    assert!(on_laptop.iter().all(|m| m.offload.n_gpu_layers == 0));
}
