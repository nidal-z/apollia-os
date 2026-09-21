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
//! 3. **Is it the best available.** [`manifest`] holds the curated generation
//!    table, and [`rank`] orders what survived the first two.
//!
//! The division of labour with the Hub is the point. `hf_registry` is the
//! catalogue: which repositories exist, what their files weigh, which are
//! gated. It is authoritative on all of that and silent on quality. Its
//! quality-adjacent signals, downloads and likes, are cumulative and therefore
//! favour whatever has been published longest, which would rank Qwen2.5 above
//! Qwen3 for as long as the counter runs. So the Hub supplies the catalogue and
//! `families.toml` supplies the order.

pub mod fit;
pub mod llama_support;
pub mod manifest;
pub mod offload;
pub mod quant;
pub mod rank;
#[cfg(feature = "cloud")]
pub mod resolve;
pub mod verdict;

pub use fit::{
    cache_layout, estimate_memory, grade, kv_cache_bytes, max_context_for, CacheKind, CacheLayout,
    MemoryEstimate, RuntimeShape,
};
pub use llama_support::LLAMA_CPP_TAG;
pub use manifest::{Family, FamilyManifest, ManifestError, ToolCalling, Variant};
pub use offload::{active_weight_share, plan_offload, MemoryArchitecture, OffloadPlan};
pub use quant::Quant;
pub use rank::{rank, FileCandidate, Matched, Reason, Recommendation};
#[cfg(feature = "cloud")]
pub use resolve::{resolve, ResolveError, ResolveOptions};
pub use verdict::{assess, shard_count_of, Blocker, Caveat, Verdict};
