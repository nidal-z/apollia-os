use super::*;
use crate::model_index::IndexedModel;

/// The tree of `Qwen/Qwen2.5-7B-Instruct-GGUF`, recorded from the HuggingFace
/// API on 2026-10-02: nineteen GGUF files, seventeen of them parts of split
/// files, at nine quantisations.
const RECORDED_TREE: &str =
    include_str!("../../hf_registry/fixtures/tree_qwen25_7b_instruct_gguf.json");

/// The `config.json` of `mlx-community/gemma-3-4b-it-4bit`.
const MLX_4BIT_CONFIG: &str =
    include_str!("../../capabilities/fixtures/config_mlx_gemma3_4b_4bit.json");

const GIB: f64 = 1024.0 * 1024.0 * 1024.0;

fn recorded_files() -> Vec<RepoFile> {
    let tree: Vec<serde_json::Value> =
        serde_json::from_str(RECORDED_TREE).expect("the recorded tree parses");
    tree.iter()
        .filter(|e| e["type"] == "file")
        .map(|e| RepoFile {
            path: e["path"].as_str().expect("a path").to_owned(),
            size_bytes: e["size"].as_u64().expect("a size"),
            sha256: e["lfs"]["oid"].as_str().map(str::to_owned),
        })
        .collect()
}

fn file(path: &str, size_bytes: u64) -> RepoFile {
    RepoFile {
        path: path.to_owned(),
        size_bytes,
        sha256: None,
    }
}

fn apple(total_gb: f64) -> HardwareProfile {
    HardwareProfile {
        total_ram_gb: total_gb,
        available_ram_gb: total_gb / 2.0,
        cpu_model: "Apple M3 Max".to_owned(),
        cpu_cores: 16,
        accelerator: AcceleratorProfile::AppleSilicon {
            chip: "M3 Max".to_owned(),
            generation: 3,
            vram_gb: total_gb,
        },
        memory_budget_gb: total_gb * 0.75,
        devices: Vec::new(),
    }
}

fn linux_card(vram_gb: f64) -> HardwareProfile {
    HardwareProfile {
        total_ram_gb: 64.0,
        available_ram_gb: 48.0,
        cpu_model: "AMD Ryzen 9".to_owned(),
        cpu_cores: 16,
        accelerator: AcceleratorProfile::Cuda {
            device_name: "NVIDIA GeForce RTX 4090".to_owned(),
            vram_gb,
            compute_capability: (8, 9),
        },
        memory_budget_gb: vram_gb,
        devices: Vec::new(),
    }
}

fn small_host(total_gb: f64) -> HardwareProfile {
    HardwareProfile {
        total_ram_gb: total_gb,
        available_ram_gb: total_gb / 2.0,
        cpu_model: "Intel Core i5".to_owned(),
        cpu_cores: 4,
        accelerator: AcceleratorProfile::None,
        memory_budget_gb: total_gb * 0.60,
        devices: Vec::new(),
    }
}

/// The variants of a 27-billion-parameter model: GGUF `Q4_K_M`, `Q6_K` and
/// `Q8_0`, MLX 4, 5, 6 and 8 bits, at the sizes such files weigh.
fn variants_27b() -> Vec<ModelVariant> {
    let gguf = |quant: &str, level, bytes: u64| ModelVariant {
        repo_id: "org/model-27b-GGUF".to_owned(),
        format: ModelFormat::Gguf,
        level,
        quantization: quant.to_owned(),
        origin_file: format!("model-27b-{quant}.gguf"),
        files: vec![format!("model-27b-{quant}.gguf")],
        size_bytes: bytes,
        weights_bytes: bytes,
        sha256: None,
        memory_gb: None,
        compatibility: None,
        fully_offloaded: false,
        downloaded: false,
        recommended: false,
    };
    let mlx = |bits: u32, level, bytes: u64| ModelVariant {
        repo_id: format!("mlx-community/model-27b-{bits}bit"),
        format: ModelFormat::Mlx,
        quantization: format!("{bits}bit"),
        origin_file: "config.json".to_owned(),
        files: vec!["model.safetensors".to_owned()],
        ..gguf("x", level, bytes)
    };
    vec![
        gguf("Q4_K_M", QuantLevel::Q4, 16_500_000_000),
        gguf("Q6_K", QuantLevel::Q6, 22_200_000_000),
        gguf("Q8_0", QuantLevel::Q8, 28_700_000_000),
        mlx(4, QuantLevel::Q4, 15_600_000_000),
        mlx(5, QuantLevel::Q5, 18_800_000_000),
        mlx(6, QuantLevel::Q6, 22_100_000_000),
        mlx(8, QuantLevel::Q8, 28_700_000_000),
    ]
}

#[test]
fn a_recorded_tree_lists_at_most_five_ordered_variants_with_summed_sizes() {
    // GIVEN a recorded repository tree of nineteen GGUF files, most split
    let files = recorded_files();
    let gguf_count = files.iter().filter(|f| f.path.ends_with(".gguf")).count();
    assert_eq!(gguf_count, 19);

    // WHEN its variants are listed
    let variants = gguf_variants("Qwen/Qwen2.5-7B-Instruct-GGUF", &files);

    // THEN one variant per level, Q4 to 16-bit in order, each split file
    // counted once at the sum of its parts, and Q2/Q3 left out
    let levels: Vec<QuantLevel> = variants.iter().map(|v| v.level).collect();
    assert_eq!(
        levels,
        [
            QuantLevel::Q4,
            QuantLevel::Q5,
            QuantLevel::Q6,
            QuantLevel::Q8,
            QuantLevel::Fp16
        ]
    );
    let quants: Vec<&str> = variants.iter().map(|v| v.quantization.as_str()).collect();
    assert_eq!(quants, ["Q4_K_M", "Q5_K_M", "Q6_K", "Q8_0", "FP16"]);
    assert_eq!(variants[0].size_bytes, 3_993_201_344 + 689_872_288);
    assert_eq!(
        variants[3].size_bytes,
        3_980_069_248 + 3_942_935_680 + 175_520_480
    );
    assert_eq!(variants[4].files.len(), 4);
    assert_eq!(
        variants[4].size_bytes,
        3_951_521_376 + 3_864_909_312 + 3_864_894_976 + 3_556_527_872
    );
    assert_eq!(
        variants[0].origin_file,
        "qwen2.5-7b-instruct-q4_k_m-00001-of-00002.gguf"
    );
    assert!(variants[0].sha256.is_some());
}

#[test]
fn an_incomplete_split_is_left_out_and_a_projector_is_never_a_variant() {
    // GIVEN a Q8 split missing its second part, a whole Q4, and a projector
    let files = [
        file("m-Q8_0-00001-of-00002.gguf", 8),
        file("m-Q4_K_M.gguf", 4),
        file("mmproj-m-f16.gguf", 1),
    ];

    // WHEN the variants are listed
    let variants = gguf_variants("org/m", &files);

    // THEN only the Q4 remains, carrying the projector it downloads with
    assert_eq!(variants.len(), 1);
    assert_eq!(variants[0].quantization, "Q4_K_M");
    assert_eq!(variants[0].size_bytes, 5);
    assert_eq!(variants[0].weights_bytes, 4);
}

#[test]
fn the_ceiling_quantisation_stands_for_its_level() {
    // GIVEN three four-bit files of one model
    let files = [
        file("m-Q4_0.gguf", 4),
        file("m-IQ4_XS.gguf", 4),
        file("m-Q4_K_M.gguf", 5),
    ];

    // WHEN the variants are listed
    let variants = gguf_variants("org/m", &files);

    // THEN Q4_K_M stands for the level
    assert_eq!(variants.len(), 1);
    assert_eq!(variants[0].quantization, "Q4_K_M");
}

#[test]
fn quantisation_is_read_from_file_names_as_publishers_write_them() {
    // GIVEN names with dots, dashes, split suffixes and folders
    // WHEN each is read
    // THEN the quantisation is found, and BF16 is not read as F16
    assert_eq!(gguf_quantization("Qwen3-4B-Q4_K_M.gguf"), Some("Q4_K_M"));
    assert_eq!(gguf_quantization("llama.Q5_K_S.gguf"), Some("Q5_K_S"));
    assert_eq!(
        gguf_quantization("BF16/m-BF16-00001-of-00002.gguf"),
        Some("BF16")
    );
    assert_eq!(gguf_quantization("m-f16.gguf"), Some("F16"));
    assert_eq!(gguf_quantization("m-UD-Q4_K_XL.gguf"), None);
    assert_eq!(gguf_quantization("m-Q3_K_M.gguf"), None);
    assert_eq!(
        gguf_file_quantization("m-Q3_K_M.gguf").as_deref(),
        Some("Q3_K_M")
    );
    assert_eq!(
        gguf_file_quantization("m-UD-Q4_K_XL.gguf").as_deref(),
        Some("Q4_K_XL")
    );
    assert_eq!(gguf_file_quantization("model.gguf"), None);
}

#[test]
fn an_mlx_repository_is_one_variant_at_its_declared_precision() {
    // GIVEN the files of an MLX repository and its recorded 4-bit config
    let files = [
        file("config.json", 1),
        file("model.safetensors", 3_000),
        file("tokenizer.json", 10),
        file(".gitattributes", 1),
    ];

    // WHEN it is read as a variant
    let variant = mlx_variant(
        "mlx-community/gemma-3-4b-it-4bit",
        &files,
        mlx_quantization(MLX_4BIT_CONFIG),
    )
    .expect("an MLX repository is a variant");

    // THEN it is one 4-bit variant, sized over every file but hidden ones
    assert_eq!(variant.level, QuantLevel::Q4);
    assert_eq!(variant.quantization, "4bit");
    assert_eq!(variant.size_bytes, 3_011);
    assert_eq!(variant.weights_bytes, 3_000);
    assert_eq!(
        mlx_quantization(r#"{"torch_dtype":"bfloat16"}"#),
        Some((QuantLevel::Fp16, "bf16".to_owned()))
    );
    assert_eq!(mlx_quantization(r#"{"quantization":{"bits":3}}"#), None);
}

#[test]
fn one_variant_is_recommended_per_machine_across_formats() {
    // GIVEN the GGUF and MLX variants of a 27B model, and three machines
    let cases = [
        ("Apple Silicon 64 GB", apple(64.0), Some("4bit")),
        ("24 GB card under Linux", linux_card(24.0), Some("Q4_K_M")),
        ("8 GB machine", small_host(8.0), None),
    ];

    for (machine, profile, expected) in cases {
        // WHEN the variants are sized and one is recommended
        let mut variants = variants_27b();
        assess_variants(&mut variants, &profile, &RuntimeShape::default());
        let chosen = recommend(&mut variants, &profile);

        // THEN the expected variant, and only it, carries the mark
        let got = chosen.map(|i| variants[i].quantization.as_str());
        assert_eq!(got, expected, "{machine}");
        assert_eq!(
            variants.iter().filter(|v| v.recommended).count(),
            usize::from(expected.is_some()),
            "{machine}"
        );
    }
}

#[test]
fn without_a_full_offload_the_most_faithful_under_the_ceiling_that_fits_wins() {
    // GIVEN a 27B model on a 12 GB card, where no variant fits on the card
    let profile = linux_card(12.0);
    let mut variants = variants_27b();
    assess_variants(&mut variants, &profile, &RuntimeShape::default());
    assert!(variants.iter().all(|v| !v.fully_offloaded));

    // WHEN one is recommended
    let chosen = recommend(&mut variants, &profile);

    // THEN the GGUF Q4_K_M is kept: it still fits across card and memory
    assert_eq!(
        chosen.map(|i| variants[i].quantization.as_str()),
        Some("Q4_K_M")
    );
}

#[test]
fn every_variant_carries_its_footprint_verdict_and_full_offload() {
    // GIVEN the 27B variants on a 24 GB card
    let profile = linux_card(24.0);
    let mut variants = variants_27b();

    // WHEN they are sized
    assess_variants(&mut variants, &profile, &RuntimeShape::default());

    // THEN each carries the estimator's figures, and the plan's offload flag
    for v in &variants {
        let fit = assess_fit(
            v.weights_bytes,
            &GgufHeaderFacts::default(),
            &profile,
            &RuntimeShape::default(),
        );
        assert_eq!(v.memory_gb, Some(fit.estimate.total_gb));
        assert_eq!(v.compatibility, Some(fit.badge));
        assert_eq!(v.fully_offloaded, fit.plan.fully_offloaded);
    }
    assert!(variants[0].fully_offloaded);
    assert!(!variants[2].fully_offloaded);
    assert!(variants[0]
        .memory_gb
        .is_some_and(|gb| gb > 16_500_000_000.0 / GIB));
}

#[test]
fn a_variant_in_the_index_is_marked_downloaded_by_origin_or_digest() {
    // GIVEN an index holding the Q4 by its recorded origin and the Q8 by digest
    let mut variants = gguf_variants("Qwen/Qwen2.5-7B-Instruct-GGUF", &recorded_files());
    let q8_sha = variants[3].sha256.clone();
    let entry = |id: &str, origin: ModelOrigin, sha256: Option<String>| IndexedModel {
        id: id.to_owned(),
        format: ModelFormat::Gguf,
        path: std::path::PathBuf::from(format!("/models/{id}.gguf")),
        parts: Vec::new(),
        annexes: Vec::new(),
        size_bytes: 1,
        origin,
        sha256,
        origin_verified: false,
        category: crate::model_index::ModelCategory::Language,
        parameters: None,
        modified_at: None,
        pinned_at: None,
        category_override: None,
        source_file_override: None,
        capabilities: None,
    };
    let index = ModelIndex {
        version: crate::model_index::INDEX_FORMAT_VERSION,
        models: vec![
            entry(
                "q4",
                ModelOrigin::Catalog {
                    repo: "Qwen/Qwen2.5-7B-Instruct-GGUF".to_owned(),
                    file: "qwen2.5-7b-instruct-q4_k_m-00001-of-00002.gguf".to_owned(),
                },
                None,
            ),
            entry("q8", ModelOrigin::Import, q8_sha),
        ],
    };

    // WHEN the variants are checked against the index
    mark_downloaded(&mut variants, &index);

    // THEN exactly those two are downloaded
    let downloaded: Vec<&str> = variants
        .iter()
        .filter(|v| v.downloaded)
        .map(|v| v.quantization.as_str())
        .collect();
    assert_eq!(downloaded, ["Q4_K_M", "Q8_0"]);
}
