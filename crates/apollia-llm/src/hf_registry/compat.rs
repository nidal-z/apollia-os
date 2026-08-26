//! llama.cpp compatibility verdicts and the model-type cache.
//!
//! Split out of `hf_registry.rs`: the client stays in the parent, the table of
//! architectures llama.cpp supports and the cache that avoids refetching a
//! model's `config.json` live here.

use crate::hf_registry::CompatIssue;

/// `model_type` architectures llama.cpp guarantees support for.
/// Source: `src/llama.cpp` (LLAMA_MODEL_ARCH), updated on each crate bump.
pub(super) static LLAMA_CPP_SUPPORTED_MODEL_TYPES: &[&str] = &[
    // LLaMA family
    "LlamaForCausalLM",
    "MistralForCausalLM",
    "MixtralForCausalLM",
    "LlamaForConditionalGeneration",
    "Llama4ForConditionalGeneration",
    // Qwen family
    "Qwen2ForCausalLM",
    "Qwen2MoeForCausalLM",
    "Qwen3ForCausalLM",
    "Qwen3MoeForCausalLM",
    // Phi family
    "PhiForCausalLM",
    "Phi3ForCausalLM",
    "Phi3SmallForCausalLM",
    // Gemma family
    "GemmaForCausalLM",
    "Gemma2ForCausalLM",
    "Gemma3ForCausalLM",
    "RecurrentGemmaForCausalLM",
    // DeepSeek family
    "DeepseekV2ForCausalLM",
    "DeepseekV3ForCausalLM",
    // Others
    "FalconForCausalLM",
    "MPTForCausalLM",
    "GPT2LMHeadModel",
    "GPTNeoXForCausalLM",
    "GPTJForCausalLM",
    "BloomForCausalLM",
    "InternLM2ForCausalLM",
    "InternLM3ForCausalLM",
    "BaichuanForCausalLM",
    "YiForCausalLM",
    "CohereForCausalLM",
    "ExaoneForCausalLM",
    "GraniteForCausalLM",
    "GraniteMoeForCausalLM",
    "ChatGLMModel",
    "GLM4ForCausalLM",
    "StableLMForCausalLM",
    "StarCoder2ForCausalLM",
    "OlmoForCausalLM",
    "OlmoeForCausalLM",
    "ArcticForCausalLM",
    "DbrxForCausalLM",
    "JambaForCausalLM",
    "MambaForCausalLM",
    "SmolLMForCausalLM",
    "Zamba2ForCausalLM",
    "OpenELMForCausalLM",
    "NemotronForCausalLM",
    "MiniCPM3ForCausalLM",
];
pub(super) fn compat_from_pipeline_tag(pipeline_tag: Option<&str>) -> Option<CompatIssue> {
    match pipeline_tag {
        Some(
            "feature-extraction"
            | "sentence-similarity"
            | "fill-mask"
            | "text-classification"
            | "token-classification"
            | "question-answering",
        ) => Some(CompatIssue::EmbeddingModel),
        Some(
            "text-to-image"
            | "image-to-text"
            | "automatic-speech-recognition"
            | "text-to-speech"
            | "text-to-video"
            | "image-classification"
            | "object-detection",
        ) => Some(CompatIssue::EmbeddingModel),
        _ => None,
    }
}
pub(super) fn compat_from_model_type(model_type: &str) -> Option<CompatIssue> {
    // Suffixes that indicate a non-generative model
    if model_type.ends_with("ForMaskedLM")
        || model_type.ends_with("ForSequenceClassification")
        || model_type.ends_with("ForTokenClassification")
        || model_type.ends_with("ForQuestionAnswering")
        || model_type.ends_with("ForMultipleChoice")
        || model_type.contains("Whisper")
        || model_type.contains("CLIP")
        || model_type.contains("ViT")
        || model_type.contains("Diffusion")
    {
        return Some(CompatIssue::EmbeddingModel);
    }
    if LLAMA_CPP_SUPPORTED_MODEL_TYPES.contains(&model_type) {
        return None; // known and supported architecture
    }
    Some(CompatIssue::UnknownArchitecture)
}
pub(super) async fn extract_model_type(
    resp: Result<reqwest::Response, reqwest::Error>,
) -> Option<String> {
    let r = resp.ok()?;
    if !r.status().is_success() {
        return None;
    }
    let json: serde_json::Value =
        apollia_core::net::read_capped_json(r, apollia_core::net::MAX_METADATA_BYTES)
            .await
            .ok()?;
    json.get("model_type")
        .and_then(|v| v.as_str())
        .map(String::from)
}
pub(super) struct CachedModelType {
    model_type: Option<String>,
    cached_at: std::time::Instant,
}
/// Session cache of HF model types (from `config.json`).
///
/// Avoids refetching `config.json` each time a card is opened. TTL 24h,
/// stored as Tauri state (`Arc<HfModelTypeCache>`).
pub struct HfModelTypeCache {
    entries: tokio::sync::RwLock<std::collections::HashMap<String, CachedModelType>>,
}
const MODEL_TYPE_CACHE_TTL_SECS: u64 = 86_400;
impl Default for HfModelTypeCache {
    fn default() -> Self {
        Self::new()
    }
}
impl HfModelTypeCache {
    pub fn new() -> Self {
        Self {
            entries: tokio::sync::RwLock::new(std::collections::HashMap::new()),
        }
    }

    pub(super) async fn get(&self, repo_id: &str) -> Option<Option<String>> {
        let guard = self.entries.read().await;
        guard.get(repo_id).and_then(|e| {
            if e.cached_at.elapsed().as_secs() < MODEL_TYPE_CACHE_TTL_SECS {
                Some(e.model_type.clone())
            } else {
                None
            }
        })
    }

    pub(super) async fn set(&self, repo_id: &str, model_type: Option<String>) {
        let mut guard = self.entries.write().await;
        guard.insert(
            repo_id.to_string(),
            CachedModelType {
                model_type,
                cached_at: std::time::Instant::now(),
            },
        );
    }
}
