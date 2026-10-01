//! Variant planning on fake repository trees.

use serde_json::json;

use super::*;

const BASE: &str = "https://hub.test";

fn sha(c: char) -> String {
    std::iter::repeat_n(c, 64).collect()
}

fn lfs(path: &str, size: u64, c: char) -> serde_json::Value {
    json!({ "type": "file", "path": path, "size": size, "lfs": { "oid": sha(c) } })
}

fn plain(path: &str, size: u64) -> serde_json::Value {
    json!({ "type": "file", "path": path, "size": size, "oid": "0123456789abcdef" })
}

fn tree(entries: Vec<serde_json::Value>) -> Vec<TreeEntry> {
    serde_json::from_value(serde_json::Value::Array(entries)).expect("tree")
}

fn dests(plan: &VariantPlan) -> Vec<&str> {
    plan.files.iter().map(|f| f.dest_name.as_str()).collect()
}

#[test]
fn test_plan_split_gguf_takes_every_part_in_order() {
    // GIVEN a repository holding a quantization split in three parts, and another one
    let t = tree(vec![
        lfs("Q8_0/model-Q8_0-00002-of-00003.gguf", 20, 'b'),
        lfs("Q8_0/model-Q8_0-00001-of-00003.gguf", 10, 'a'),
        lfs("Q8_0/model-Q8_0-00003-of-00003.gguf", 30, 'c'),
        lfs("model-Q4_K_M.gguf", 5, 'd'),
        plain("README.md", 1),
    ]);
    let selector = VariantSelector::Gguf {
        file: "Q8_0/model-Q8_0-00002-of-00003.gguf".into(),
    };

    // WHEN the variant is planned from any of its parts
    let plan = plan_variant(BASE, "org/repo", &t, &selector).expect("plan");

    // THEN the three parts are fetched in order, nothing else, each with its digest
    assert_eq!(
        dests(&plan),
        vec![
            "Q8_0/model-Q8_0-00001-of-00003.gguf",
            "Q8_0/model-Q8_0-00002-of-00003.gguf",
            "Q8_0/model-Q8_0-00003-of-00003.gguf",
        ]
    );
    assert_eq!(plan.format, ModelFormat::Gguf);
    assert_eq!(plan.total_bytes, 60);
    assert_eq!(plan.model_path, "Q8_0/model-Q8_0-00001-of-00003.gguf");
    assert!(plan.files.iter().all(|f| f.sha256.is_some()));
    assert_eq!(
        plan.files[0].url,
        "https://hub.test/org/repo/resolve/main/Q8_0/model-Q8_0-00001-of-00003.gguf"
    );
}

#[test]
fn test_plan_split_gguf_with_a_missing_part_is_refused() {
    // GIVEN a split file whose second part is absent from the tree
    let t = tree(vec![
        lfs("m-00001-of-00003.gguf", 1, 'a'),
        lfs("m-00003-of-00003.gguf", 1, 'c'),
    ]);
    let selector = VariantSelector::Gguf {
        file: "m-00001-of-00003.gguf".into(),
    };

    // WHEN the variant is planned
    let result = plan_variant(BASE, "org/repo", &t, &selector);

    // THEN it is refused rather than fetched half
    assert!(matches!(
        result,
        Err(VariantError::IncompleteSplit {
            found: 2,
            expected: 3,
            ..
        })
    ));
}

#[test]
fn test_plan_gguf_with_projector_puts_both_in_the_variant_folder() {
    // GIVEN a vision repository with two quantizations and two projectors
    let t = tree(vec![
        lfs("Vision-7B-Q4_K_M.gguf", 400, 'a'),
        lfs("Vision-7B-Q8_0.gguf", 800, 'b'),
        lfs("mmproj-Vision-7B-Q8_0.gguf", 60, 'c'),
        lfs("mmproj-Vision-7B-f16.gguf", 120, 'd'),
    ]);
    let selector = VariantSelector::Gguf {
        file: "Vision-7B-Q4_K_M.gguf".into(),
    };

    // WHEN the Q4_K_M variant is planned
    let plan = plan_variant(BASE, "org/vision", &t, &selector).expect("plan");

    // THEN the weights and the full precision projector land together in a folder of their own
    assert_eq!(
        dests(&plan),
        vec![
            "Vision-7B-Q4_K_M/Vision-7B-Q4_K_M.gguf",
            "Vision-7B-Q4_K_M/mmproj-Vision-7B-f16.gguf",
        ]
    );
    assert_eq!(plan.model_path, "Vision-7B-Q4_K_M/Vision-7B-Q4_K_M.gguf");
    assert_eq!(plan.origin_file, "Vision-7B-Q4_K_M.gguf");
    assert_eq!(plan.total_bytes, 520);
}

#[test]
fn test_plan_gguf_refuses_a_projector_as_the_variant() {
    // GIVEN a repository with a projector
    let t = tree(vec![
        lfs("m-Q4_K_M.gguf", 1, 'a'),
        lfs("mmproj-m-f16.gguf", 1, 'b'),
    ]);
    let selector = VariantSelector::Gguf {
        file: "mmproj-m-f16.gguf".into(),
    };

    // WHEN the projector itself is asked for as a variant
    let result = plan_variant(BASE, "org/repo", &t, &selector);

    // THEN it is refused: a projector is never a model
    assert!(matches!(result, Err(VariantError::NotAWeightsFile { .. })));
}

#[test]
fn test_plan_gguf_weights_without_published_digest_is_refused() {
    // GIVEN a weights file committed outside LFS
    let t = tree(vec![plain("m-Q4_K_M.gguf", 1)]);
    let selector = VariantSelector::Gguf {
        file: "m-Q4_K_M.gguf".into(),
    };

    // WHEN the variant is planned
    let result = plan_variant(BASE, "org/repo", &t, &selector);

    // THEN it is refused, since its download could not be checked
    assert!(matches!(
        result,
        Err(VariantError::NoPublishedSha256 { .. })
    ));
}

#[test]
fn test_plan_mlx_takes_every_file_of_the_repository_in_one_folder() {
    // GIVEN an MLX repository
    let t = tree(vec![
        plain(".gitattributes", 1),
        plain("config.json", 2),
        plain("tokenizer_config.json", 3),
        lfs("tokenizer.json", 4, 'a'),
        lfs("model-00001-of-00002.safetensors", 50, 'b'),
        lfs("model-00002-of-00002.safetensors", 60, 'c'),
        plain("model.safetensors.index.json", 5),
    ]);

    // WHEN the repository is planned as an MLX variant
    let plan =
        plan_variant(BASE, "mlx-community/Small-4bit", &t, &VariantSelector::Mlx).expect("plan");

    // THEN every file but the git metadata lands under one folder, weights first
    assert_eq!(
        dests(&plan),
        vec![
            "Small-4bit/model-00001-of-00002.safetensors",
            "Small-4bit/model-00002-of-00002.safetensors",
            "Small-4bit/config.json",
            "Small-4bit/model.safetensors.index.json",
            "Small-4bit/tokenizer.json",
            "Small-4bit/tokenizer_config.json",
        ]
    );
    assert_eq!(plan.format, ModelFormat::Mlx);
    assert_eq!(plan.model_path, "Small-4bit");
    assert_eq!(plan.total_bytes, 124);
    // AND the weights carry their SHA-256, the small files only their size
    let config = plan
        .files
        .iter()
        .find(|f| f.repo_path == "config.json")
        .expect("config");
    assert_eq!(config.sha256, None);
    assert_eq!(config.size_bytes, 2);
    assert!(plan.files[0].sha256.is_some());
}

#[test]
fn test_plan_mlx_refuses_a_repository_without_weights() {
    // GIVEN a repository with a config but no safetensors
    let t = tree(vec![plain("config.json", 2), lfs("m.gguf", 1, 'a')]);

    // WHEN it is planned as MLX
    let result = plan_variant(BASE, "org/repo", &t, &VariantSelector::Mlx);

    // THEN it is refused
    assert!(matches!(result, Err(VariantError::NotMlx { .. })));
}
