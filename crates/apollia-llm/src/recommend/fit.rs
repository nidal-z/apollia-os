//! What a model actually costs to run, as opposed to what it weighs on disk.
//!
//! The Model Hub's original badge multiplied the file size by 1.1 and compared
//! it to the memory budget. That covers the weights and a little slack, and it
//! omits the key/value cache, which on a long context is not slack but the
//! largest single allocation after the weights themselves.
//!
//! # Why the cache needs the header rather than a formula
//!
//! A first version of this module derived the per-head dimension as
//! `embedding_length / head_count` and charged every layer the full context.
//! Both are defaults that most modern models override, and the errors do not
//! cancel:
//!
//! | Model | derived head dim | declared `key_length` |
//! |---|---|---|
//! | Gemma 3 4B | 320 | 256 |
//! | Qwen3 4B | 80 | 128 |
//! | Qwen3 30B-A3B | 64 | 128 |
//!
//! and Gemma 3 holds 29 of its 34 layers at a 1536-cell sliding window rather
//! than at 32768, which alone is the difference between 0.8 GB of cache and
//! 5.3 GB. Measured against the published GGUFs, the naive formula ran 2.3x
//! high for Gemma 3 and 1.6x low for Qwen3 4B, which is enough to invert a
//! ranking.
//!
//! So every term is read from the file, and the two things the file does not
//! carry, the sliding-window period and which layers are recurrent, come from
//! [`super::llama_support`], generated from the engine's own sources.
//!
//! # The formula
//!
//! Reproduces `llama_kv_cache_iswa`:
//!
//! ```text
//! cells_base = n_ctx
//! cells_swa  = pad256(min(n_ctx, n_swa * n_seq_max + n_ubatch))
//! bytes      = (layers_base * cells_base + layers_swa * cells_swa)
//!              * n_head_kv * (key_length + value_length)
//!              * bytes_per_element
//! ```

use serde::{Deserialize, Serialize};

use crate::gguf_probe::GgufHeaderFacts;
use crate::hardware::{CompatibilityBadge, HardwareProfile};

use super::llama_support::{
    architecture_is_recurrent, hybrid_attention_for, sliding_window_for, HybridAttention,
};

/// Bytes in a gibibyte, the unit every figure here is reported in.
const BYTES_PER_GIB: f64 = 1024.0 * 1024.0 * 1024.0;

/// The engine pads the sliding-window cache to this many cells.
///
/// `llama_kv_cache_iswa` does it for performance; reproducing it keeps the
/// estimate from drifting under the real allocation on short windows.
const SWA_CELL_PADDING: u32 = 256;

/// State a recurrent layer holds, in bytes.
///
/// Unlike a key/value cache this does not grow with the context: a gated delta
/// net keeps `d_inner * d_state` of state per layer, roughly two mebibytes on
/// the models that use it. Four is charged instead, because those dimensions
/// are not among the keys this probe keeps and erring upward on a term worth
/// tens of megabytes is cheaper than parsing for it.
const RECURRENT_STATE_BYTES_PER_LAYER: u64 = 4 * 1024 * 1024;

/// The launch settings that decide how much memory a model needs.
///
/// Mirrors the fields of the runtime's `LlamaServerConfig` that affect the
/// footprint. It is duplicated rather than imported because `apollia-llm` sits
/// below `apollia-runtime` in the dependency order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeShape {
    /// Context window in tokens, the `-c` argument.
    pub n_ctx: u32,
    /// Bytes per key/value cache element: 2 for `f16`, 1 for `q8_0`.
    pub kv_element_bytes: u32,
    /// Number of parallel slots, the `-np` argument.
    pub n_parallel: u32,
    /// Physical micro-batch, the `-ub` argument.
    ///
    /// Only the sliding-window cache depends on it, as the headroom the engine
    /// adds on top of the window itself.
    pub n_ubatch: u32,
}

impl Default for RuntimeShape {
    /// The settings `apollia-runtime` launches with today.
    ///
    /// `llama_server::config::LlamaServerConfig::default` is the source: 32768
    /// tokens of context, no `-ctk`/`-ctv` override so the engine's `f16`
    /// cache applies, a single slot, and no `-ub` override so the engine's own
    /// 512 applies.
    fn default() -> Self {
        Self {
            n_ctx: 32_768,
            kv_element_bytes: 2,
            n_parallel: 1,
            n_ubatch: 512,
        }
    }
}

/// How a model's layers hold their context.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CacheKind {
    /// Every layer attends to the whole context.
    Dense,
    /// Most layers attend to a short window.
    SlidingWindow,
    /// Some layers keep a cache, the rest carry recurrent state.
    Hybrid,
    /// No layer keeps a cache that grows with the context.
    Recurrent,
}

/// The shape of one model's cache, and what it costs.
///
/// The byte figure is summed per layer rather than derived from a single cell
/// price, because newer architectures vary both terms down the stack. Gemma 4
/// declares one key/value head count per layer and a different per-head
/// dimension for its windowed layers than for its full ones, so no single
/// multiplier describes the file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CacheLayout {
    /// Which of the four regimes this model is in.
    pub kind: CacheKind,
    /// Layers holding the full context.
    pub full_layers: u32,
    /// Layers holding only the sliding window.
    pub window_layers: u32,
    /// Layers holding recurrent state instead of a cache.
    pub recurrent_layers: u32,
    /// Cells reserved per full layer.
    pub full_cells: u32,
    /// Cells reserved per sliding-window layer.
    pub window_cells: u32,
    /// Total bytes reserved, summed across the layers.
    pub bytes: u64,
}

impl CacheLayout {
    /// Total bytes this layout reserves.
    #[must_use]
    pub fn total_bytes(&self) -> u64 {
        self.bytes
    }
}

/// Where a model's memory goes, in gibibytes.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct MemoryEstimate {
    /// The weights, which is to say the file on disk.
    pub weights_gb: f64,
    /// The key/value cache at the configured context length.
    pub kv_cache_gb: f64,
    /// Compute buffers, the graph, and engine bookkeeping.
    pub overhead_gb: f64,
    /// The sum the budget is compared against.
    pub total_gb: f64,
    /// `false` when the header did not carry the fields the cache formula needs
    /// and a proportional fallback was used instead.
    pub kv_cache_measured: bool,
    /// The layout the cache figure came from, when it was measured.
    pub layout: Option<CacheLayout>,
}

/// Floor on the non-cache overhead, in gibibytes.
const MIN_OVERHEAD_GB: f64 = 0.25;

/// Overhead as a share of the weights, applied above [`MIN_OVERHEAD_GB`].
const OVERHEAD_SHARE: f64 = 0.05;

/// Extra overhead once any layer is offloaded to a discrete accelerator.
///
/// A CUDA or Vulkan context plus its workspaces costs several hundred
/// megabytes before a single weight is read, which on an eight-gigabyte card is
/// a sixteenth of the budget and not something the proportional term covers.
const ACCELERATOR_CONTEXT_GB: f64 = 0.5;

/// Share of the weights assumed for the cache when the header did not carry the
/// hyperparameters. Deliberately pessimistic.
const FALLBACK_KV_SHARE: f64 = 0.35;

/// What a graphics context costs before any weights are read, in gibibytes.
///
/// Exposed so the offload planner can take it off the device pool before it
/// starts counting layers into it.
#[must_use]
pub fn accelerator_context_gb() -> f64 {
    ACCELERATOR_CONTEXT_GB
}

/// Round up to a multiple of [`SWA_CELL_PADDING`], as the engine does.
fn pad_cells(cells: u32) -> u32 {
    cells.div_ceil(SWA_CELL_PADDING) * SWA_CELL_PADDING
}

/// Work out how a model's layers hold their context, and what that costs.
///
/// Returns `None` when the header lacks a field the formula needs, which is the
/// case for a probe truncated before the hyperparameters.
///
/// Every term is taken per layer where the file gives one per layer, because
/// the newer architectures vary them: Gemma 4 declares a key/value head count
/// for each of its 48 layers and a narrower per-head dimension on the windowed
/// ones. A single cell price would be wrong for every layer but one.
#[must_use]
pub fn cache_layout(facts: &GgufHeaderFacts, shape: &RuntimeShape) -> Option<CacheLayout> {
    let blocks = u32::try_from(facts.block_count?).ok()?;
    let heads = facts.head_count?;
    let embedding = facts.embedding_length?;
    if heads == 0 || blocks == 0 || embedding == 0 {
        return None;
    }

    // The declared dimensions win; the division is only llama.cpp's default for
    // a file that does not carry them.
    let derived = embedding / heads;
    let key_full = facts.key_length.unwrap_or(derived);
    let value_full = facts.value_length.unwrap_or(derived);
    let key_window = facts.key_length_swa.unwrap_or(key_full);
    let value_window = facts.value_length_swa.unwrap_or(value_full);

    let heads_kv_scalar = facts.head_count_kv.unwrap_or(heads);
    let heads_kv_at = |il: u32| -> u64 {
        facts
            .head_count_kv_per_layer
            .get(il as usize)
            .copied()
            .filter(|n| *n > 0)
            .unwrap_or(heads_kv_scalar)
    };

    let arch = facts.architecture.as_deref().unwrap_or("");
    let element = u64::from(shape.kv_element_bytes);

    if architecture_is_recurrent(arch) {
        return Some(CacheLayout {
            kind: CacheKind::Recurrent,
            full_layers: 0,
            window_layers: 0,
            recurrent_layers: blocks,
            full_cells: 0,
            window_cells: 0,
            bytes: u64::from(blocks) * RECURRENT_STATE_BYTES_PER_LAYER,
        });
    }

    if let Some(table) = hybrid_attention_for(arch) {
        // A file may override the interval its architecture defaults to, and
        // Qwen3.5 does exactly that.
        let hybrid = match facts
            .full_attention_interval
            .and_then(|n| u32::try_from(n).ok())
            .filter(|n| *n > 0)
        {
            Some(full_attention_interval) => HybridAttention {
                architecture: table.architecture,
                full_attention_interval,
            },
            None => table,
        };

        let mut bytes = 0u64;
        let mut cached = 0u32;
        for il in 0..blocks {
            if hybrid.layer_has_cache(il) {
                cached += 1;
                bytes +=
                    u64::from(shape.n_ctx) * heads_kv_at(il) * (key_full + value_full) * element;
            }
        }
        let recurrent = blocks - cached;
        return Some(CacheLayout {
            kind: CacheKind::Hybrid,
            full_layers: cached,
            window_layers: 0,
            recurrent_layers: recurrent,
            full_cells: shape.n_ctx,
            window_cells: 0,
            bytes: bytes + u64::from(recurrent) * RECURRENT_STATE_BYTES_PER_LAYER,
        });
    }

    // Which layers slide: the file's own per-layer table first, then its period
    // over the architecture default, then nothing.
    let per_layer = &facts.sliding_window_per_layer;
    let table = sliding_window_for(arch);
    let n_swa = facts
        .sliding_window
        .and_then(|n| u32::try_from(n).ok())
        .filter(|n| *n > 0)
        .or_else(|| table.map(|w| w.fixed_window).filter(|n| *n > 0));

    let window = table.map(|mut w| {
        if let Some(pattern) = facts
            .sliding_window_pattern
            .and_then(|n| u32::try_from(n).ok())
        {
            w.n_pattern = pattern;
        }
        w
    });

    let slides_at = |il: u32| -> bool {
        if !per_layer.is_empty() {
            return per_layer.get(il as usize).copied().unwrap_or(0) != 0;
        }
        window.is_some_and(|w| w.layer_slides(il))
    };

    let has_window = n_swa.is_some() && (!per_layer.is_empty() || window.is_some());
    if has_window {
        let window_cells = pad_cells(
            shape.n_ctx.min(
                n_swa
                    .unwrap_or(0)
                    .saturating_mul(shape.n_parallel.max(1))
                    .saturating_add(shape.n_ubatch),
            ),
        );

        let mut bytes = 0u64;
        let mut sliding = 0u32;
        for il in 0..blocks {
            let (cells, k, v) = if slides_at(il) {
                sliding += 1;
                (window_cells, key_window, value_window)
            } else {
                (shape.n_ctx, key_full, value_full)
            };
            bytes += u64::from(cells) * heads_kv_at(il) * (k + v) * element;
        }

        if sliding > 0 {
            return Some(CacheLayout {
                kind: CacheKind::SlidingWindow,
                full_layers: blocks - sliding,
                window_layers: sliding,
                recurrent_layers: 0,
                full_cells: shape.n_ctx,
                window_cells,
                bytes,
            });
        }
    }

    let mut bytes = 0u64;
    for il in 0..blocks {
        bytes += u64::from(shape.n_ctx) * heads_kv_at(il) * (key_full + value_full) * element;
    }
    Some(CacheLayout {
        kind: CacheKind::Dense,
        full_layers: blocks,
        window_layers: 0,
        recurrent_layers: 0,
        full_cells: shape.n_ctx,
        window_cells: 0,
        bytes,
    })
}

/// Bytes the key/value cache occupies at `n_ctx` tokens.
#[must_use]
pub fn kv_cache_bytes(facts: &GgufHeaderFacts, shape: &RuntimeShape) -> Option<u64> {
    cache_layout(facts, shape).map(|l| l.total_bytes())
}

/// Estimate the whole runtime footprint of a GGUF file.
///
/// `file_size_bytes` is the size the registry reports for the file, not the
/// size of the probed prefix. `accelerated` adds the graphics context a
/// discrete device costs before any weights are read.
#[must_use]
pub fn estimate_memory(
    file_size_bytes: u64,
    facts: &GgufHeaderFacts,
    shape: &RuntimeShape,
    accelerated: bool,
) -> MemoryEstimate {
    let weights_gb = file_size_bytes as f64 / BYTES_PER_GIB;

    let layout = cache_layout(facts, shape);
    let (kv_cache_gb, kv_cache_measured) = match layout {
        Some(l) => (l.total_bytes() as f64 / BYTES_PER_GIB, true),
        None => (weights_gb * FALLBACK_KV_SHARE, false),
    };

    let mut overhead_gb = (weights_gb * OVERHEAD_SHARE).max(MIN_OVERHEAD_GB);
    if accelerated {
        overhead_gb += ACCELERATOR_CONTEXT_GB;
    }

    MemoryEstimate {
        weights_gb,
        kv_cache_gb,
        overhead_gb,
        total_gb: weights_gb + kv_cache_gb + overhead_gb,
        kv_cache_measured,
        layout,
    }
}

/// Grade an estimate against the machine's memory budget.
#[must_use]
pub fn grade(estimate: &MemoryEstimate, profile: &HardwareProfile) -> CompatibilityBadge {
    let budget = profile.memory_budget_gb;
    if estimate.total_gb < budget * 0.70 {
        CompatibilityBadge::Fits
    } else if estimate.total_gb < budget {
        CompatibilityBadge::MightFit
    } else {
        CompatibilityBadge::TooLarge
    }
}

/// The longest context, in tokens, at which this model fits the budget.
///
/// Returns `None` when the weights alone exhaust the budget, which no context
/// length can fix, and when the header did not carry the fields the cache
/// formula needs. Rounded down to a multiple of 1024, capped at `shape.n_ctx`.
///
/// Solved by bisection rather than by dividing headroom by a per-token cost,
/// because a sliding-window model's cache is not linear in the context: past
/// the window, more tokens only grow the handful of full-attention layers.
#[must_use]
pub fn max_context_for(
    file_size_bytes: u64,
    facts: &GgufHeaderFacts,
    shape: &RuntimeShape,
    profile: &HardwareProfile,
    accelerated: bool,
) -> Option<u32> {
    cache_layout(facts, shape)?;

    let fits = |n_ctx: u32| {
        let probe = RuntimeShape { n_ctx, ..*shape };
        estimate_memory(file_size_bytes, facts, &probe, accelerated).total_gb
            < profile.memory_budget_gb
    };

    if fits(shape.n_ctx) {
        return Some(shape.n_ctx);
    }

    let (mut low, mut high) = (0u32, shape.n_ctx);
    while high - low > 1024 {
        let mid = low + (high - low) / 2;
        if fits(mid) {
            low = mid;
        } else {
            high = mid;
        }
    }

    Some((low / 1024) * 1024).filter(|n| *n > 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hardware::AcceleratorProfile;

    /// Qwen3 14B as its GGUF declares it.
    fn qwen3_14b() -> GgufHeaderFacts {
        GgufHeaderFacts {
            version: 3,
            architecture: Some("qwen3".to_owned()),
            block_count: Some(40),
            head_count: Some(40),
            head_count_kv: Some(8),
            embedding_length: Some(5120),
            key_length: Some(128),
            value_length: Some(128),
            ..GgufHeaderFacts::default()
        }
    }

    /// Gemma 3 4B as its GGUF declares it, sliding window included.
    fn gemma3_4b() -> GgufHeaderFacts {
        GgufHeaderFacts {
            version: 3,
            architecture: Some("gemma3".to_owned()),
            block_count: Some(34),
            head_count: Some(8),
            head_count_kv: Some(4),
            embedding_length: Some(2560),
            key_length: Some(256),
            value_length: Some(256),
            sliding_window: Some(1024),
            ..GgufHeaderFacts::default()
        }
    }

    fn profile_with_budget(budget_gb: f64) -> HardwareProfile {
        HardwareProfile {
            total_ram_gb: budget_gb,
            available_ram_gb: budget_gb,
            cpu_model: "test".to_owned(),
            cpu_cores: 8,
            accelerator: AcceleratorProfile::None,
            memory_budget_gb: budget_gb,
        }
    }

    fn gib(bytes: u64) -> f64 {
        bytes as f64 / BYTES_PER_GIB
    }

    #[test]
    fn the_declared_head_dimension_wins_over_the_derived_one() {
        // GIVEN Qwen3 4B, whose embedding over head count gives 80 but whose
        // header declares 128
        let facts = GgufHeaderFacts {
            block_count: Some(36),
            head_count: Some(32),
            embedding_length: Some(2560),
            ..qwen3_14b()
        };

        // WHEN the layout is computed, once as declared and once with the key
        // length stripped so the derived default applies
        let declared = cache_layout(&facts, &RuntimeShape::default()).expect("declared");
        let stripped = GgufHeaderFacts {
            key_length: None,
            value_length: None,
            ..facts.clone()
        };
        let derived = cache_layout(&stripped, &RuntimeShape::default()).expect("derived");

        // THEN the declared dimension costs 1.6x the derived one, which is the
        // error the first version of this module shipped
        assert_eq!(declared.bytes, 36 * 32_768 * 8 * (128 + 128) * 2);
        assert_eq!(derived.bytes, 36 * 32_768 * 8 * (80 + 80) * 2);
        assert!(declared.total_bytes() > derived.total_bytes());
    }

    #[test]
    fn gemma3_holds_most_layers_at_the_sliding_window() {
        // GIVEN Gemma 3 4B and the runtime's own launch settings
        let facts = gemma3_4b();

        // WHEN its cache layout is computed
        let layout = cache_layout(&facts, &RuntimeShape::default()).expect("a layout");

        // THEN 29 of 34 layers hold a padded 1536-cell window and 5 hold the
        // full context, exactly as the engine allocates them
        assert_eq!(layout.kind, CacheKind::SlidingWindow);
        assert_eq!(layout.window_layers, 29);
        assert_eq!(layout.full_layers, 5);
        assert_eq!(layout.window_cells, 1536);
        assert_eq!(layout.full_cells, 32_768);
    }

    #[test]
    fn ignoring_the_sliding_window_alone_overstates_gemma3_fivefold() {
        // GIVEN Gemma 3 4B, and the same model labelled as a dense architecture
        // so every layer is charged the full context
        let facts = gemma3_4b();
        let dense = GgufHeaderFacts {
            architecture: Some("qwen3".to_owned()),
            ..facts.clone()
        };
        let shape = RuntimeShape::default();

        // WHEN both caches are sized
        let windowed = gib(kv_cache_bytes(&facts, &shape).expect("windowed"));
        let as_dense = gib(kv_cache_bytes(&dense, &shape).expect("dense"));

        // THEN the real figure is under a gibibyte against more than four, a
        // factor of five from this one correction. The 5.3 GB the first version
        // reported was this error compounded with the derived head dimension.
        assert!(windowed < 1.0, "windowed cache was {windowed} GB");
        assert!(as_dense > 4.0, "dense cache was {as_dense} GB");
        assert!(as_dense / windowed > 5.0);
    }

    #[test]
    fn a_hybrid_model_keeps_a_cache_on_one_layer_in_four() {
        // GIVEN a Qwen3.5 header with 32 layers
        let facts = GgufHeaderFacts {
            version: 3,
            architecture: Some("qwen35".to_owned()),
            block_count: Some(32),
            head_count: Some(32),
            head_count_kv: Some(8),
            embedding_length: Some(2560),
            key_length: Some(128),
            value_length: Some(128),
            ..GgufHeaderFacts::default()
        };

        // WHEN the layout is computed
        let layout = cache_layout(&facts, &RuntimeShape::default()).expect("hybrid");

        // THEN only eight layers hold a growing cache, so the context costs a
        // quarter of what the same depth costs a dense model
        assert_eq!(layout.kind, CacheKind::Hybrid);
        assert_eq!(layout.full_layers, 8);
        assert_eq!(layout.recurrent_layers, 24);

        let dense = GgufHeaderFacts {
            architecture: Some("qwen3".to_owned()),
            ..facts.clone()
        };
        let dense_bytes = kv_cache_bytes(&dense, &RuntimeShape::default()).expect("dense");
        assert!(layout.total_bytes() * 3 < dense_bytes);
    }

    #[test]
    fn a_recurrent_model_pays_nothing_for_a_longer_context() {
        // GIVEN a Mamba header
        let facts = GgufHeaderFacts {
            version: 3,
            architecture: Some("mamba2".to_owned()),
            block_count: Some(64),
            head_count: Some(32),
            head_count_kv: Some(8),
            embedding_length: Some(4096),
            ..GgufHeaderFacts::default()
        };

        // WHEN the cache is sized at two very different context lengths
        let short = kv_cache_bytes(
            &facts,
            &RuntimeShape {
                n_ctx: 4096,
                ..RuntimeShape::default()
            },
        );
        let long = kv_cache_bytes(
            &facts,
            &RuntimeShape {
                n_ctx: 131_072,
                ..RuntimeShape::default()
            },
        );

        // THEN the figure does not move, because its state is flat in the context
        assert_eq!(short, long);
        assert!(gib(short.expect("a figure")) < 1.0);
    }

    #[test]
    fn the_cache_follows_the_engine_formula_for_a_dense_model() {
        // GIVEN Qwen3 14B and the runtime's launch settings
        // WHEN the cache is sized
        let bytes = kv_cache_bytes(&qwen3_14b(), &RuntimeShape::default()).expect("all fields");

        // THEN it matches layers x cells x heads_kv x (k + v) x element bytes
        assert_eq!(bytes, 40u64 * 32_768 * (8 * (128 + 128)) * 2);
    }

    #[test]
    fn grouped_query_attention_is_read_from_the_header() {
        // GIVEN two headers identical but for the key/value head count
        let gqa = qwen3_14b();
        let mha = GgufHeaderFacts {
            head_count_kv: Some(40),
            ..qwen3_14b()
        };

        // WHEN both caches are sized
        // THEN the grouped-query header costs a fifth of the full-attention one
        let gqa_bytes = kv_cache_bytes(&gqa, &RuntimeShape::default()).expect("gqa");
        let mha_bytes = kv_cache_bytes(&mha, &RuntimeShape::default()).expect("mha");
        assert_eq!(mha_bytes / gqa_bytes, 5);
    }

    #[test]
    fn a_quantised_cache_halves_the_reservation() {
        // GIVEN the runtime's f16 cache and a q8_0 cache of the same shape
        let facts = qwen3_14b();
        let q8 = RuntimeShape {
            kv_element_bytes: 1,
            ..RuntimeShape::default()
        };

        // WHEN both are sized
        // THEN the one-byte element costs half the two-byte one
        assert_eq!(
            kv_cache_bytes(&facts, &RuntimeShape::default()).expect("f16"),
            kv_cache_bytes(&facts, &q8).expect("q8") * 2
        );
    }

    #[test]
    fn a_header_without_hyperparameters_falls_back_and_says_so() {
        // GIVEN a probe truncated before the architecture hyperparameters
        let facts = GgufHeaderFacts {
            version: 3,
            architecture: Some("qwen3".to_owned()),
            truncated: true,
            ..GgufHeaderFacts::default()
        };

        // WHEN the footprint is estimated
        let est = estimate_memory(
            (4.0 * BYTES_PER_GIB) as u64,
            &facts,
            &RuntimeShape::default(),
            false,
        );

        // THEN the cache is a proportional guess, and the estimate admits it
        assert!(!est.kv_cache_measured);
        assert!(est.layout.is_none());
        assert!(est.total_gb > est.weights_gb);
    }

    #[test]
    fn an_accelerator_is_charged_for_its_context() {
        // GIVEN the same model on a processor and on a discrete device
        let facts = qwen3_14b();
        let bytes = (8.4 * BYTES_PER_GIB) as u64;
        let shape = RuntimeShape::default();

        // WHEN both footprints are estimated
        let cpu = estimate_memory(bytes, &facts, &shape, false);
        let gpu = estimate_memory(bytes, &facts, &shape, true);

        // THEN the device costs half a gibibyte before a weight is read
        assert!((gpu.overhead_gb - cpu.overhead_gb - ACCELERATOR_CONTEXT_GB).abs() < 1e-9);
    }

    #[test]
    fn a_model_too_large_at_the_full_window_reports_the_window_that_fits() {
        // GIVEN a 14B on a 12 GB budget, which the full 32k window overruns
        let facts = qwen3_14b();
        let file_bytes = (8.4 * BYTES_PER_GIB) as u64;
        let profile = profile_with_budget(12.0);
        let shape = RuntimeShape::default();
        assert_eq!(
            grade(
                &estimate_memory(file_bytes, &facts, &shape, false),
                &profile
            ),
            CompatibilityBadge::TooLarge
        );

        // WHEN the affordable window is computed
        let ctx = max_context_for(file_bytes, &facts, &shape, &profile, false)
            .expect("the weights leave headroom");

        // THEN it is shorter than the configured window and actually fits
        assert!(ctx > 0 && ctx < shape.n_ctx);
        let narrowed = RuntimeShape {
            n_ctx: ctx,
            ..shape
        };
        assert_ne!(
            grade(
                &estimate_memory(file_bytes, &facts, &narrowed, false),
                &profile
            ),
            CompatibilityBadge::TooLarge
        );
    }

    #[test]
    fn a_budget_the_weights_alone_exhaust_has_no_affordable_window() {
        // GIVEN weights larger than the whole budget
        let facts = qwen3_14b();
        let profile = profile_with_budget(8.0);

        // WHEN the affordable window is computed
        // THEN there is none, rather than a window of zero tokens
        assert_eq!(
            max_context_for(
                (18.0 * BYTES_PER_GIB) as u64,
                &facts,
                &RuntimeShape::default(),
                &profile,
                false
            ),
            None
        );
    }

    #[test]
    fn the_affordable_window_never_exceeds_the_configured_one() {
        // GIVEN a small model on a machine with memory to spare
        let facts = qwen3_14b();
        let profile = profile_with_budget(96.0);
        let shape = RuntimeShape::default();

        // WHEN the affordable window is computed
        // THEN it is capped at what the runtime actually asks for
        assert_eq!(
            max_context_for(
                (2.5 * BYTES_PER_GIB) as u64,
                &facts,
                &shape,
                &profile,
                false
            ),
            Some(shape.n_ctx)
        );
    }

    #[test]
    fn a_per_layer_head_count_is_summed_rather_than_averaged() {
        // GIVEN a header declaring a different key/value head count per layer,
        // as Gemma 4 does across its 48
        let facts = GgufHeaderFacts {
            version: 3,
            architecture: Some("madeup".to_owned()),
            block_count: Some(4),
            head_count: Some(16),
            head_count_kv: Some(8),
            embedding_length: Some(2048),
            key_length: Some(128),
            value_length: Some(128),
            head_count_kv_per_layer: vec![2, 4, 8, 2],
            ..GgufHeaderFacts::default()
        };

        // WHEN the cache is sized
        let layout = cache_layout(&facts, &RuntimeShape::default()).expect("a layout");

        // THEN each layer is charged its own head count, not the scalar the
        // file also carries for compatibility
        let per_cell = 32_768u64 * (128 + 128) * 2;
        assert_eq!(layout.bytes, (2 + 4 + 8 + 2) * per_cell);
    }

    #[test]
    fn a_per_layer_window_table_overrides_the_architecture_period() {
        // GIVEN a header marking its sliding layers one by one
        let facts = GgufHeaderFacts {
            version: 3,
            architecture: Some("gemma4".to_owned()),
            block_count: Some(4),
            head_count: Some(16),
            head_count_kv: Some(4),
            embedding_length: Some(2048),
            key_length: Some(512),
            value_length: Some(512),
            key_length_swa: Some(256),
            value_length_swa: Some(256),
            sliding_window: Some(1024),
            sliding_window_per_layer: vec![1, 1, 1, 0],
            ..GgufHeaderFacts::default()
        };

        // WHEN the cache is sized
        let layout = cache_layout(&facts, &RuntimeShape::default()).expect("a layout");

        // THEN three layers hold the window at the narrower head dimension and
        // one holds the full context at the wider one
        assert_eq!(layout.kind, CacheKind::SlidingWindow);
        assert_eq!(layout.window_layers, 3);
        assert_eq!(layout.full_layers, 1);
        let windowed = 3 * u64::from(layout.window_cells) * 4 * (256 + 256) * 2;
        let full = 32_768u64 * 4 * (512 + 512) * 2;
        assert_eq!(layout.bytes, windowed + full);
    }
}
