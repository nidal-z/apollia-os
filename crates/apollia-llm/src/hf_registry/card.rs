//! The capabilities and the variants of a model card, from what the Hub
//! returned for its repository.
//!
//! The chat template comes with the model metadata: the `gguf` block carries
//! the template of a GGUF repository, `config.tokenizer_config` that of a
//! repository with a `tokenizer_config.json`, MLX conversions included. No
//! request is spent on it.

use std::path::Path;

use crate::capabilities::{template_of_value, Capabilities};
use crate::hardware::HardwareProfile;
use crate::hf_registry::compat::ConfigFacts;
use crate::hf_registry::parse::normalize_sha256;
use crate::hf_registry::{HfModelCard, TreeEntry};
use crate::model_index::is_projector_name;
use crate::recommend::{
    assess_variants, gguf_variants, mlx_variant, recommend_variant, RepoFile, RuntimeShape,
};

/// Fill the capabilities and the variants of `card`.
///
/// Vision is the projector of a GGUF repository, which the variant download
/// fetches with the weights, or the `vision_config` of a repository without
/// GGUF files. Variants are sized and graded when the machine is known, and
/// the one recommended among them is marked.
pub(super) fn fill(
    card: &mut HfModelCard,
    meta: &serde_json::Value,
    tree: &[TreeEntry],
    config: &ConfigFacts,
    hardware: Option<&HardwareProfile>,
) {
    let files: Vec<RepoFile> = tree
        .iter()
        .filter(|e| e.entry_type == "file")
        .map(|e| RepoFile {
            path: e.path.clone(),
            size_bytes: e.size,
            sha256: e.lfs.as_ref().and_then(|lfs| normalize_sha256(&lfs.oid)),
        })
        .collect();

    let has_gguf = files
        .iter()
        .any(|f| f.path.to_ascii_lowercase().ends_with(".gguf"));
    let vision = if has_gguf {
        files.iter().any(|f| {
            f.path.to_ascii_lowercase().ends_with(".gguf") && is_projector_name(Path::new(&f.path))
        })
    } else {
        config.vision
    };
    card.capabilities = Capabilities::from_chat_template(chat_template(meta).as_deref())
        .with_catalogue_vision(vision);

    let mut variants = gguf_variants(&card.repo_id, &files);
    if variants.is_empty() {
        variants.extend(mlx_variant(
            &card.repo_id,
            &files,
            config.mlx_quantization.clone(),
        ));
    }
    if let Some(profile) = hardware {
        assess_variants(&mut variants, profile, &RuntimeShape::default());
        recommend_variant(&mut variants, profile);
    }
    card.variants = variants;
}

/// The chat template the model metadata carries, GGUF block first.
pub(super) fn chat_template(meta: &serde_json::Value) -> Option<String> {
    meta.get("gguf")
        .and_then(|g| g.get("chat_template"))
        .and_then(template_of_value)
        .or_else(|| {
            meta.get("config")
                .and_then(|c| c.get("tokenizer_config"))
                .and_then(|t| t.get("chat_template"))
                .and_then(template_of_value)
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The tree of `Qwen/Qwen2.5-7B-Instruct-GGUF`, recorded on 2026-10-02.
    const RECORDED_TREE: &str = include_str!("fixtures/tree_qwen25_7b_instruct_gguf.json");

    /// The tree of `ggml-org/SmolVLM-256M-Instruct-GGUF`, with a projector.
    const SMOLVLM_TREE: &str = include_str!("fixtures/tree_smolvlm_256m.json");

    const QWEN3_TEMPLATE: &str =
        include_str!("../capabilities/fixtures/chat_template_qwen3_4b.jinja");

    fn card(repo_id: &str) -> HfModelCard {
        parse_card(repo_id).expect("a minimal card parses")
    }

    fn parse_card(repo_id: &str) -> Option<HfModelCard> {
        crate::hf_registry::parse::parse_model_list_item(
            &serde_json::json!({ "id": repo_id }),
            None,
        )
    }

    fn mac() -> HardwareProfile {
        HardwareProfile {
            total_ram_gb: 32.0,
            available_ram_gb: 16.0,
            cpu_model: "Apple M3 Pro".to_owned(),
            cpu_cores: 12,
            accelerator: crate::hardware::AcceleratorProfile::AppleSilicon {
                chip: "M3 Pro".to_owned(),
                generation: 3,
                vram_gb: 32.0,
            },
            memory_budget_gb: 24.0,
            devices: Vec::new(),
        }
    }

    #[test]
    fn a_card_carries_template_capabilities_and_graded_variants() {
        // GIVEN model metadata carrying a GGUF chat template, and the recorded
        //       tree of a repository without projector
        let meta = serde_json::json!({ "gguf": { "chat_template": QWEN3_TEMPLATE } });
        let tree: Vec<TreeEntry> = serde_json::from_str(RECORDED_TREE).expect("tree parses");
        let mut card = card("Qwen/Qwen2.5-7B-Instruct-GGUF");

        // WHEN the card is filled for a 32 GB Mac
        fill(
            &mut card,
            &meta,
            &tree,
            &ConfigFacts::default(),
            Some(&mac()),
        );

        // THEN tools and reasoning come from the template, no vision without a
        //      projector, five graded variants and the Q4 recommended
        assert!(card.capabilities.tools);
        assert!(card.capabilities.reasoning);
        assert!(!card.capabilities.vision);
        assert_eq!(card.variants.len(), 5);
        assert!(card
            .variants
            .iter()
            .all(|v| v.compatibility.is_some() && v.memory_gb.is_some()));
        let recommended: Vec<&str> = card
            .variants
            .iter()
            .filter(|v| v.recommended)
            .map(|v| v.quantization.as_str())
            .collect();
        assert_eq!(recommended, ["Q4_K_M"]);
    }

    #[test]
    fn a_repository_publishing_a_projector_is_a_vision_model_in_the_catalogue() {
        // GIVEN the recorded tree of a repository publishing a projector
        let tree: Vec<TreeEntry> = serde_json::from_str(SMOLVLM_TREE).expect("tree parses");
        let mut card = card("ggml-org/SmolVLM-256M-Instruct-GGUF");

        // WHEN the card is filled, the machine unknown
        fill(
            &mut card,
            &serde_json::json!({}),
            &tree,
            &ConfigFacts::default(),
            None,
        );

        // THEN it reads images once downloaded, with no format named yet, and
        //      its variants are listed without a verdict
        assert!(card.capabilities.vision);
        assert!(card.capabilities.image_formats.is_empty());
        assert!(!card.variants.is_empty());
        assert!(card.variants.iter().all(|v| v.compatibility.is_none()));
    }

    #[test]
    fn an_mlx_repository_reads_its_template_and_vision_from_its_config() {
        // GIVEN the metadata and tree of a multimodal MLX repository
        let meta = serde_json::json!({
            "config": { "tokenizer_config": { "chat_template": "{{ messages }}" } }
        });
        let tree: Vec<TreeEntry> = serde_json::from_value(serde_json::json!([
            { "type": "file", "path": "config.json", "size": 1, "oid": "a" },
            { "type": "file", "path": "model.safetensors", "size": 3000,
              "lfs": { "oid": "b" } }
        ]))
        .expect("tree parses");
        let config = ConfigFacts::from_json(include_str!(
            "../capabilities/fixtures/config_mlx_gemma3_4b_4bit.json"
        ));
        let mut card = card("mlx-community/gemma-3-4b-it-4bit");

        // WHEN the card is filled
        fill(&mut card, &meta, &tree, &config, None);

        // THEN it reads images, declares neither tools nor reasoning, and is
        //      one 4-bit variant
        assert!(card.capabilities.vision);
        assert!(!card.capabilities.tools);
        assert_eq!(card.variants.len(), 1);
        assert_eq!(card.variants[0].quantization, "4bit");
    }
}
