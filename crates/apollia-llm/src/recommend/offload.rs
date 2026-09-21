//! Where a model's layers run, and how fast that makes it.
//!
//! Two questions the memory budget alone cannot answer.
//!
//! # Which pool is being spent
//!
//! `HardwareProfile::memory_budget_gb` collapses three different machines into
//! one number, and they do not behave alike:
//!
//! - **Unified memory** (Apple Silicon): weights and cache share one pool with
//!   the processor. Every layer goes to the accelerator, always, and the budget
//!   is the only constraint.
//! - **Discrete memory** (CUDA, Vulkan): the accelerator has its own pool, and
//!   anything that does not fit stays in system memory and runs on the
//!   processor. Budgeting only the device's memory, which is what this crate
//!   did first, hides every model larger than the card from a machine that
//!   could run it perfectly well across both.
//! - **Processor only**: one pool, an order of magnitude less bandwidth.
//!
//! # How fast the result runs
//!
//! Generation is bandwidth-bound: each token reads the active weights once, so
//! throughput is roughly `bandwidth / active_bytes`. That single relation is
//! what makes the two remaining questions answerable.
//!
//! A mixture of experts reads only the experts it routes to, so a 30B model
//! that activates 3B reads like a 3B and occupies like a 30B. On a machine with
//! memory to spare that is the best trade available. On a small discrete card
//! it is the worst, because the occupancy is what does not fit while the saved
//! computation buys nothing. Same model, opposite verdict, and a ranking with
//! no speed term cannot express either.
//!
//! Split execution is the same relation applied twice: the layers on the device
//! read at device bandwidth, those on the processor at system bandwidth, and
//! the token waits for both. Since system memory runs roughly an order of
//! magnitude slower, a few layers left behind cost far more than their share.
//!
//! The bandwidth figures below are order-of-magnitude, and deliberately so.
//! Nothing here reports a tokens-per-second number to an operator; they exist
//! to rank two candidates on one machine, and for that only the ratios matter.

use serde::{Deserialize, Serialize};

use crate::gguf_probe::GgufHeaderFacts;
use crate::hardware::{AcceleratorProfile, HardwareProfile};

use super::fit::{cache_layout, RuntimeShape};

/// Bytes in a gibibyte.
const BYTES_PER_GIB: f64 = 1024.0 * 1024.0 * 1024.0;

/// System memory bandwidth, in gibibytes per second.
///
/// A dual-channel DDR5 desktop reaches roughly this; DDR4 rather less. It is
/// the denominator for anything running on the processor.
const HOST_BANDWIDTH_GBPS: f64 = 50.0;

/// Discrete accelerator bandwidth, in gibibytes per second.
///
/// A mid-range modern card. The exact figure matters far less than the ratio to
/// [`HOST_BANDWIDTH_GBPS`], which is what decides whether leaving layers behind
/// is worth it.
const DISCRETE_BANDWIDTH_GBPS: f64 = 600.0;

/// Unified memory bandwidth by Apple Silicon tier, in gibibytes per second.
///
/// The spread across one generation is wider than the spread between
/// generations, which is why the tier is parsed from the chip name rather than
/// taken from the generation number.
const APPLE_BASE_GBPS: f64 = 100.0;
const APPLE_PRO_GBPS: f64 = 200.0;
const APPLE_MAX_GBPS: f64 = 400.0;
const APPLE_ULTRA_GBPS: f64 = 800.0;

/// How much of a mixture's sparsity survives when its experts sit in system
/// memory rather than on the device.
///
/// Sparsity pays only when the routed experts are cheap to reach. Once they are
/// across the bus the advantage largely evaporates: the router picks a
/// different subset for every token, so over a few tokens most of the pool is
/// touched and there is no working set to keep hot. Charging a quarter of the
/// pool per token is the pessimistic reading, and pessimism is the right
/// direction here: the failure it guards against is recommending a 30B mixture
/// to a machine that will then generate at walking pace.
const MOE_HOST_DISPERSION: f64 = 4.0;

/// Share of a mixture's weights that are always read, whatever the routing.
///
/// Attention, embeddings, shared experts and norms do not participate in the
/// routing. Measured against Qwen3-30B-A3B, whose three billion active
/// parameters out of thirty are close to what this produces at eight experts
/// of a hundred and twenty-eight.
const MOE_DENSE_SHARE: f64 = 0.10;

/// How a machine holds the weights it is running.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MemoryArchitecture {
    /// Processor and accelerator share one pool, as on Apple Silicon.
    Unified {
        /// Read bandwidth of the shared pool, in gibibytes per second.
        bandwidth_gbps: f64,
    },
    /// The accelerator has its own pool; the rest stays in system memory.
    Discrete {
        /// The device's own memory, in gibibytes.
        vram_gb: f64,
        /// Read bandwidth of the device pool, in gibibytes per second.
        bandwidth_gbps: f64,
        /// System memory available to hold what does not fit, in gibibytes.
        host_gb: f64,
    },
    /// No usable accelerator.
    HostOnly {
        /// Read bandwidth of system memory, in gibibytes per second.
        bandwidth_gbps: f64,
    },
}

impl MemoryArchitecture {
    /// Read a machine's memory architecture from its detected profile.
    ///
    /// The host figure for a discrete card is the same sixty percent of system
    /// memory the profile uses when there is no accelerator at all: the rest
    /// belongs to the operating system and to whatever else the operator is
    /// running.
    #[must_use]
    pub fn of(profile: &HardwareProfile) -> Self {
        match &profile.accelerator {
            AcceleratorProfile::AppleSilicon { chip, .. } => Self::Unified {
                bandwidth_gbps: apple_bandwidth(chip),
            },
            AcceleratorProfile::Cuda { vram_gb, .. } => Self::Discrete {
                vram_gb: *vram_gb,
                bandwidth_gbps: DISCRETE_BANDWIDTH_GBPS,
                host_gb: profile.total_ram_gb * 0.60,
            },
            AcceleratorProfile::Generic { vram_gb, .. } if *vram_gb > 0.0 => Self::Discrete {
                vram_gb: *vram_gb,
                bandwidth_gbps: DISCRETE_BANDWIDTH_GBPS,
                host_gb: profile.total_ram_gb * 0.60,
            },
            AcceleratorProfile::Generic { .. } | AcceleratorProfile::None => Self::HostOnly {
                bandwidth_gbps: HOST_BANDWIDTH_GBPS,
            },
        }
    }

    /// Whether a graphics context has to be created, which costs memory.
    #[must_use]
    pub fn uses_accelerator(&self) -> bool {
        !matches!(self, Self::HostOnly { .. })
    }

    /// The largest footprint this machine can hold across every pool.
    ///
    /// For a discrete card that is its memory plus the usable share of system
    /// memory, because a model too large for the card still runs with its tail
    /// on the processor. Budgeting the card alone was what hid a 30B from a
    /// machine with twelve gigabytes of video memory and sixty-four of system
    /// memory.
    #[must_use]
    pub fn total_capacity_gb(&self, profile: &HardwareProfile) -> f64 {
        match self {
            Self::Unified { .. } | Self::HostOnly { .. } => profile.memory_budget_gb,
            Self::Discrete {
                vram_gb, host_gb, ..
            } => vram_gb + host_gb,
        }
    }
}

/// Bandwidth of an Apple chip, from its marketing name.
fn apple_bandwidth(chip: &str) -> f64 {
    let lower = chip.to_lowercase();
    if lower.contains("ultra") {
        APPLE_ULTRA_GBPS
    } else if lower.contains("max") {
        APPLE_MAX_GBPS
    } else if lower.contains("pro") {
        APPLE_PRO_GBPS
    } else {
        APPLE_BASE_GBPS
    }
}

/// How a model's layers are placed, and what that costs.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct OffloadPlan {
    /// The `-ngl` value to launch with.
    ///
    /// Equal to `total_layers` when everything fits, which is what the runtime
    /// used to request unconditionally with its literal 999.
    pub n_gpu_layers: u32,
    /// Layers the model has, from its header.
    pub total_layers: u32,
    /// What lands on the accelerator, in gibibytes.
    pub device_gb: f64,
    /// What stays in system memory, in gibibytes.
    pub host_gb: f64,
    /// Whether every layer reached the accelerator.
    pub fully_offloaded: bool,
    /// Rough generation throughput, in tokens per second.
    ///
    /// An order-of-magnitude figure for ranking two candidates on one machine,
    /// not a number to show an operator.
    pub tokens_per_second: f64,
}

/// Share of a model's weights that are read for each token.
///
/// One for a dense model. For a mixture, the routed share plus the part that is
/// always read.
#[must_use]
pub fn active_weight_share(facts: &GgufHeaderFacts) -> f64 {
    let (Some(total), Some(used)) = (facts.expert_count, facts.expert_used_count) else {
        return 1.0;
    };
    if total == 0 || used >= total {
        return 1.0;
    }
    let routed = used as f64 / total as f64;
    (MOE_DENSE_SHARE + (1.0 - MOE_DENSE_SHARE) * routed).clamp(0.0, 1.0)
}

/// Decide where a model's layers run on this machine, and how fast that is.
///
/// `weights_bytes` is the file size; `cache_bytes` the key/value cache at the
/// configured context.
#[must_use]
pub fn plan_offload(
    weights_bytes: u64,
    facts: &GgufHeaderFacts,
    profile: &HardwareProfile,
    shape: &RuntimeShape,
) -> OffloadPlan {
    let arch = MemoryArchitecture::of(profile);
    let layers = cache_layout(facts, shape)
        .map(|l| l.full_layers + l.window_layers + l.recurrent_layers)
        .or_else(|| facts.block_count.and_then(|b| u32::try_from(b).ok()))
        .unwrap_or(0)
        .max(1);

    let weights_gb = weights_bytes as f64 / BYTES_PER_GIB;
    let cache_gb = cache_layout(facts, shape)
        .map(|l| l.total_bytes() as f64 / BYTES_PER_GIB)
        .unwrap_or(weights_gb * 0.35);
    let resident_gb = weights_gb + cache_gb;
    let active_gb = weights_gb * active_weight_share(facts);

    match arch {
        // One pool: every layer goes to the accelerator, and the only question
        // the placement answers is how fast the pool is.
        MemoryArchitecture::Unified { bandwidth_gbps } => OffloadPlan {
            n_gpu_layers: layers,
            total_layers: layers,
            device_gb: resident_gb,
            host_gb: 0.0,
            fully_offloaded: true,
            tokens_per_second: throughput(active_gb, 0.0, bandwidth_gbps, bandwidth_gbps),
        },

        MemoryArchitecture::HostOnly { bandwidth_gbps } => OffloadPlan {
            n_gpu_layers: 0,
            total_layers: layers,
            device_gb: 0.0,
            host_gb: resident_gb,
            fully_offloaded: false,
            tokens_per_second: throughput(
                0.0,
                weights_gb * (active_weight_share(facts) * MOE_HOST_DISPERSION).min(1.0),
                bandwidth_gbps,
                bandwidth_gbps,
            ),
        },

        MemoryArchitecture::Discrete {
            vram_gb,
            bandwidth_gbps,
            ..
        } => {
            // The context cache is allocated on the device whenever any layer
            // is, and it is not divisible the way the weights are, so it comes
            // off the top before the layers are counted.
            let usable = (vram_gb - cache_gb - super::fit::accelerator_context_gb()).max(0.0);
            let per_layer_gb = weights_gb / f64::from(layers);
            let offloadable = if per_layer_gb <= 0.0 {
                layers
            } else {
                ((usable / per_layer_gb).floor().max(0.0) as u32).min(layers)
            };

            let device_weights_gb = per_layer_gb * f64::from(offloadable);
            let host_weights_gb = weights_gb - device_weights_gb;
            let share = active_weight_share(facts);
            // On the device the routed share is what gets read. Across the bus
            // it is not, for the reason on MOE_HOST_DISPERSION.
            let host_share = (share * MOE_HOST_DISPERSION).min(1.0);

            OffloadPlan {
                n_gpu_layers: offloadable,
                total_layers: layers,
                device_gb: device_weights_gb + if offloadable > 0 { cache_gb } else { 0.0 },
                host_gb: host_weights_gb,
                fully_offloaded: offloadable >= layers,
                tokens_per_second: throughput(
                    device_weights_gb * share,
                    host_weights_gb * host_share,
                    bandwidth_gbps,
                    HOST_BANDWIDTH_GBPS,
                ),
            }
        }
    }
}

/// Tokens per second from the bytes read on each tier.
///
/// The token waits for both reads, so the times add. This is what makes a few
/// layers left in system memory so expensive: at a twelvefold bandwidth gap,
/// a tenth of the weights on the processor more than doubles the time.
fn throughput(device_gb: f64, host_gb: f64, device_gbps: f64, host_gbps: f64) -> f64 {
    let seconds = device_gb / device_gbps.max(1.0) + host_gb / host_gbps.max(1.0);
    if seconds <= 0.0 {
        return 0.0;
    }
    1.0 / seconds
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts(arch: &str, blocks: u64) -> GgufHeaderFacts {
        GgufHeaderFacts {
            version: 3,
            architecture: Some(arch.to_owned()),
            block_count: Some(blocks),
            head_count: Some(32),
            head_count_kv: Some(8),
            embedding_length: Some(4096),
            key_length: Some(128),
            value_length: Some(128),
            ..GgufHeaderFacts::default()
        }
    }

    fn moe_facts(blocks: u64, experts: u64, used: u64) -> GgufHeaderFacts {
        GgufHeaderFacts {
            expert_count: Some(experts),
            expert_used_count: Some(used),
            ..facts("qwen3moe", blocks)
        }
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

    fn apple(chip: &str, ram_gb: f64) -> HardwareProfile {
        profile(
            AcceleratorProfile::AppleSilicon {
                chip: chip.to_owned(),
                generation: 4,
                vram_gb: ram_gb,
            },
            ram_gb,
            ram_gb * 0.75,
        )
    }

    fn cuda(vram_gb: f64, ram_gb: f64) -> HardwareProfile {
        profile(
            AcceleratorProfile::Cuda {
                device_name: "test".to_owned(),
                vram_gb,
                compute_capability: (8, 9),
            },
            ram_gb,
            vram_gb,
        )
    }

    const GIB: f64 = 1024.0 * 1024.0 * 1024.0;

    #[test]
    fn unified_memory_offloads_every_layer() {
        // GIVEN a Mac with 64 GB of unified memory and a 14B model
        let profile = apple("M4 Max", 64.0);
        let facts = facts("qwen3", 40);

        // WHEN the placement is planned
        let plan = plan_offload(
            (8.4 * GIB) as u64,
            &facts,
            &profile,
            &RuntimeShape::default(),
        );

        // THEN every layer goes to the accelerator, because there is only one
        // pool to put them in
        assert_eq!(plan.n_gpu_layers, 40);
        assert!(plan.fully_offloaded);
        assert_eq!(plan.host_gb, 0.0);
    }

    #[test]
    fn a_small_card_keeps_the_layers_that_fit_and_leaves_the_rest() {
        // GIVEN a 12 GB card beside 64 GB of system memory, and an 18 GB model
        let profile = cuda(12.0, 64.0);
        let facts = facts("qwen3", 48);

        // WHEN the placement is planned
        let plan = plan_offload(
            (18.0 * GIB) as u64,
            &facts,
            &profile,
            &RuntimeShape::default(),
        );

        // THEN some layers run on the device and the rest on the processor,
        // rather than the model being refused outright
        assert!(plan.n_gpu_layers > 0, "nothing was offloaded");
        assert!(plan.n_gpu_layers < plan.total_layers);
        assert!(!plan.fully_offloaded);
        assert!(plan.host_gb > 0.0);
    }

    #[test]
    fn a_card_that_holds_everything_offloads_everything() {
        // GIVEN a 24 GB card and a model well inside it
        let profile = cuda(24.0, 64.0);
        let facts = facts("qwen3", 36);

        // WHEN the placement is planned
        let plan = plan_offload(
            (4.7 * GIB) as u64,
            &facts,
            &profile,
            &RuntimeShape::default(),
        );

        // THEN every layer is on the device and nothing is left behind
        assert_eq!(plan.n_gpu_layers, plan.total_layers);
        assert!(plan.fully_offloaded);
        assert_eq!(plan.host_gb, 0.0);
    }

    #[test]
    fn leaving_layers_behind_costs_more_than_their_share() {
        // GIVEN one model fully on a card and the same model mostly on it
        let facts = facts("qwen3", 40);
        let bytes = (8.4 * GIB) as u64;
        let shape = RuntimeShape::default();

        let roomy = plan_offload(bytes, &facts, &cuda(24.0, 64.0), &shape);
        let tight = plan_offload(bytes, &facts, &cuda(10.0, 64.0), &shape);

        // WHEN their throughputs are compared
        // THEN the split run is far slower than the fraction left behind would
        // suggest, because system memory reads an order of magnitude slower
        assert!(roomy.fully_offloaded);
        assert!(!tight.fully_offloaded);
        assert!(
            roomy.tokens_per_second > tight.tokens_per_second * 2.0,
            "roomy {} vs tight {}",
            roomy.tokens_per_second,
            tight.tokens_per_second
        );
    }

    #[test]
    fn a_mixture_reads_only_the_experts_it_routes_to() {
        // GIVEN a 30B mixture routing to 8 of 128 experts, and a dense 30B
        let moe = moe_facts(48, 128, 8);
        let dense = facts("qwen3", 48);

        // WHEN their active shares are compared
        // THEN the mixture reads a fraction of its weights per token
        assert!(active_weight_share(&moe) < 0.25);
        assert_eq!(active_weight_share(&dense), 1.0);
    }

    #[test]
    fn a_mixture_beats_a_dense_model_of_the_same_size_on_unified_memory() {
        // GIVEN a Mac with room for either, and two 30B models
        let profile = apple("M4 Max", 64.0);
        let shape = RuntimeShape::default();
        let bytes = (18.6 * GIB) as u64;

        // WHEN both are placed
        let moe = plan_offload(bytes, &moe_facts(48, 128, 8), &profile, &shape);
        let dense = plan_offload(bytes, &facts("qwen3", 48), &profile, &shape);

        // THEN the mixture generates several times faster for the same memory,
        // which is the advantage a size-only ranking cannot see
        assert!(
            moe.tokens_per_second > dense.tokens_per_second * 2.0,
            "moe {} vs dense {}",
            moe.tokens_per_second,
            dense.tokens_per_second
        );
    }

    #[test]
    fn a_mixture_loses_its_advantage_when_it_does_not_fit_the_card() {
        // GIVEN a modest card and a 30B mixture that overruns it
        let profile = cuda(12.0, 64.0);
        let shape = RuntimeShape::default();
        let moe = plan_offload(
            (18.6 * GIB) as u64,
            &moe_facts(48, 128, 8),
            &profile,
            &shape,
        );

        // AND a dense 8B that fits it entirely
        let dense = plan_offload((4.7 * GIB) as u64, &facts("qwen3", 36), &profile, &shape);

        // WHEN the two are compared
        // THEN the small dense model wins, because occupancy is what does not
        // fit and the saved computation buys nothing once layers are stranded
        assert!(!moe.fully_offloaded);
        assert!(dense.fully_offloaded);
        assert!(dense.tokens_per_second > moe.tokens_per_second);
    }

    #[test]
    fn a_discrete_machine_counts_both_pools_as_capacity() {
        // GIVEN a 12 GB card beside 64 GB of system memory
        let profile = cuda(12.0, 64.0);
        let arch = MemoryArchitecture::of(&profile);

        // WHEN the total capacity is asked for
        let capacity = arch.total_capacity_gb(&profile);

        // THEN it exceeds the card alone, which is what lets a large model be
        // offered to a machine that can run it across both pools
        assert!(capacity > 12.0);
        assert!(arch.uses_accelerator());
    }

    #[test]
    fn apple_tiers_are_read_from_the_chip_name() {
        // GIVEN the four Apple Silicon tiers
        // WHEN their bandwidths are looked up
        // THEN they ascend, because the spread within a generation is wider
        // than the spread between generations
        assert!(apple_bandwidth("M4") < apple_bandwidth("M4 Pro"));
        assert!(apple_bandwidth("M4 Pro") < apple_bandwidth("M4 Max"));
        assert!(apple_bandwidth("M2 Max") < apple_bandwidth("M2 Ultra"));
    }

    #[test]
    fn a_machine_with_no_accelerator_runs_every_layer_on_the_processor() {
        // GIVEN a processor-only machine
        let profile = profile(AcceleratorProfile::None, 32.0, 32.0 * 0.60);

        // WHEN a model is placed
        let plan = plan_offload(
            (4.7 * GIB) as u64,
            &facts("qwen3", 36),
            &profile,
            &RuntimeShape::default(),
        );

        // THEN nothing is offloaded and no graphics context is charged
        assert_eq!(plan.n_gpu_layers, 0);
        assert!(!MemoryArchitecture::of(&profile).uses_accelerator());
        assert!(plan.host_gb > 0.0);
    }
}
