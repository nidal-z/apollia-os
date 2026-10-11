//! Choose the language model a given machine should be offered.
//!
//! Onboarding used to hold a list of four HuggingFace URLs in the desktop
//! frontend and filter it on installed RAM. That answered one of the three
//! questions a recommendation has to answer, and answered it roughly.
//!
//! The three are independent, and this module keeps them separate because
//! conflating them is what makes a recommendation wrong:
//!
//! 1. **Will it run here.** [`verdict`] decides, from a GGUF header read over
//!    HTTP before any download starts, whether the embedded `llama-server`
//!    loads the file and whether it can call tools once loaded.
//! 2. **Will it fit here.** [`fit`] sizes the weights, the key/value cache at
//!    the runtime's own context length, and the engine's overhead, against the
//!    machine's memory budget.
//! 3. **Is it the best available.** [`registry_source`] takes the candidates
//!    from the model registry and orders them by a stated rule, and [`rank`]
//!    orders what survived the first two for this machine.
//!
//! The registry is also the editorial selection an operator sees first
//! ([`catalogue`]), and [`publishers`] says which HuggingFace owners are the
//! official publishers and which are recognised converters. One file,
//! `pinned_models.toml`, holds the embeddings category ([`embeddings`]): models
//! that turn a text into a vector, refused as chat engines and offered on
//! their own, the dictation category ([`dictation`]): the whisper.cpp files of
//! the dictation engine, and the character-recognition data ([`ocr`]), each
//! pinned by digest.
//!
//! The division of labour with the Hub is the point. `hf_registry` is the
//! catalogue: which repositories exist, what their files weigh, which are
//! gated. It is authoritative on all of that and silent on quality. Its
//! quality-adjacent signals, downloads and likes, are cumulative and therefore
//! favour whatever has been published longest, which would rank Qwen2.5 above
//! Qwen3 for as long as the counter runs. So the Hub supplies the catalogue and
//! the registry supplies the order.

pub mod catalogue;
pub mod dictation;
pub mod embeddings;
pub mod fit;
pub mod llama_support;
pub mod manifest;
pub mod mlx;
pub mod ocr;
pub mod offload;
pub mod publishers;
pub mod quant;
pub mod rank;
pub mod registry_source;
#[cfg(feature = "cloud")]
pub mod resolve;
pub mod variants;
pub mod verdict;

pub use catalogue::{retain_fitting, SelectionFamily, SelectionVariant};
pub use dictation::{
    dictation_fits, dictation_footprint_gb, DictationCatalogue, DictationModel, DictationReason,
    DictationRecommendation, DICTATION_MARGIN_GB,
};
pub use embeddings::{
    EmbeddingCatalogue, EmbeddingModel, EmbeddingReason, EmbeddingRecommendation, Pooling,
};
pub use fit::{
    assess_fit, cache_layout, estimate_memory, grade, kv_cache_bytes, max_context_for,
    max_parallel_for, parallel_shape, CacheKind, CacheLayout, FitAssessment, MemoryEstimate,
    RuntimeShape,
};
pub use llama_support::LLAMA_CPP_TAG;
pub use manifest::{Family, FamilyManifest, ManifestError, ToolCalling, Variant};
pub use mlx::{estimate_mlx_memory, mlx_kv_cache_bytes, MlxConfig, MlxEstimateError};
pub use ocr::{OcrCatalogue, OcrLanguage};
pub use offload::{
    active_weight_share, estimated_capacity_gb, plan_offload, MemoryArchitecture, OffloadPlan,
};
pub use publishers::{publisher_kind, OfficialPublisher, PublisherKind, PublisherLists};
pub use quant::Quant;
pub use rank::{rank, FileCandidate, Matched, Reason, Recommendation};
pub use registry_source::{is_suggested, manifest_from_cards, rule_score, selection_from_cards};
#[cfg(feature = "cloud")]
pub use resolve::{resolve, ResolveError, ResolveOptions};
pub use variants::{
    assess_variants, gguf_file_quantization, gguf_quantization, gguf_variants, mark_downloaded,
    mlx_quantization, mlx_variant, recommend as recommend_variant, ModelVariant, QuantLevel,
    RepoFile,
};
pub use verdict::{assess, shard_count_of, Blocker, Caveat, Verdict};
