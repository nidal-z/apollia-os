//! Order the candidates that survived the compatibility gate.
//!
//! Four inputs decide a ranking, and they are not interchangeable:
//!
//! - **compatibility** is a gate. A file the engine will not load has no score.
//! - **fit** is a gate and then a preference. A model past what the machine can
//!   hold across all its pools is out; among those that fit, comfortable beats
//!   tight.
//! - **quality** is the base ordering, from the curated table, discounted by
//!   what the quantisation gives up.
//! - **speed** is the correction that makes the ordering depend on the machine.
//!
//! # Why speed belongs in the ranking
//!
//! Without it the scorer cannot express the two cases that decide most real
//! recommendations.
//!
//! A mixture of experts reads a fraction of its weights per token. On a machine
//! with memory to spare, a 30B that activates 3B is the best trade on offer: it
//! generates like a small model and reasons like a large one. On a small
//! discrete card it is the worst, because what does not fit is the occupancy
//! and the saved computation buys nothing.
//!
//! Partial offload is the mirror of that. Leaving a tenth of the layers in
//! system memory costs far more than a tenth of the throughput, because system
//! memory reads an order of magnitude slower than a device. A model that fits
//! the card entirely usually beats a better model that does not.
//!
//! Both come out of [`super::offload`], and both are properties of the pairing
//! rather than of the model, which is why the same list ranks differently on a
//! Mac and on a desktop with a 12 GB card.
//!
//! # Which memory a verdict is about
//!
//! A machine with a discrete card has two pools, and "fits" means something
//! different in each. A model that runs entirely on the card is graded against
//! the card. A model that spills into system memory is not graded at all: it is
//! reported as a split, with how much lands on each side, because calling a
//! model that runs a sixth of its layers on the processor a comfortable fit
//! against the sum of both pools told the operator the opposite of what they
//! would experience. Every reason that carries a figure names its pool.
//!
//! # Too slow to offer
//!
//! A model that generates below [`MIN_TOKENS_PER_SECOND`] is dropped rather
//! than ranked low, unless nothing faster fits: a 70B that fits across both
//! pools and answers at one token a second is a model an operator waits on, not
//! one they use.
//!
//! # The one absolute rule
//!
//! A generation that some other fitting candidate supersedes is demoted below
//! everything that supersedes it, whatever it scores. Without that a
//! well-scored Qwen2.5 could edge past a Qwen3 on a tie-break, which is the
//! outcome this whole design exists to prevent.

use serde::{Deserialize, Serialize};

use crate::gguf_probe::GgufHeaderFacts;
use crate::hardware::{CompatibilityBadge, HardwareProfile};

use super::fit::{estimate_memory, max_context_for, MemoryEstimate, RuntimeShape};
use super::manifest::{Family, FamilyManifest, ToolCalling, Variant};
use super::offload::{active_weight_share, plan_offload, MemoryArchitecture, OffloadPlan};
use super::quant;
use super::verdict::{assess, Caveat, Verdict};

/// A GGUF file the Hub reported, before any judgement is passed on it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileCandidate {
    /// Repository the file lives in, for example `"Qwen/Qwen3-4B-GGUF"`.
    pub repo_id: String,
    /// File name within the repository.
    pub filename: String,
    /// Direct download URL.
    pub download_url: String,
    /// Size in bytes, as the repository tree reports it.
    pub size_bytes: u64,
    /// Whether the repository requires a token.
    pub gated: bool,
}

/// Why a candidate sits where it does, as data the interface renders.
///
/// Deliberately not prose: the desktop is translated, so a reason is a tag the
/// frontend turns into a sentence in the operator's language.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "reason", rename_all = "snake_case")]
#[non_exhaustive]
pub enum Reason {
    /// Fits comfortably in one memory pool.
    FitsComfortably {
        /// Estimated total footprint in gibibytes.
        needs_gb: f64,
        /// What that pool can hold, in gibibytes.
        budget_gb: f64,
        /// The pool the model runs from.
        pool: MemoryPool,
    },
    /// Fits one memory pool, without much room.
    Tight {
        /// Estimated total footprint in gibibytes.
        needs_gb: f64,
        /// What that pool can hold, in gibibytes.
        budget_gb: f64,
        /// The pool the model runs from.
        pool: MemoryPool,
    },
    /// Too large for the card alone: part runs from the card, the rest from
    /// system memory, at the speed of the slower side.
    SplitAcrossMemory {
        /// Gibibytes placed on the card, weights and context cache.
        gpu_gb: f64,
        /// The card's memory, in gibibytes.
        vram_gb: f64,
        /// Gibibytes of weights left in system memory.
        system_gb: f64,
    },
    /// The template emits tool calls the runtime parses natively.
    NativeToolCalling,
    /// This generation replaces an older one the machine might otherwise get.
    SupersedesGeneration {
        /// The label of the generation it replaces.
        replaces: String,
    },
    /// The model was trained on a shorter window than the one asked, and runs
    /// at its own: the engine would accept more, but past its training length
    /// a model reads positions it was never fitted to.
    TrainedContext {
        /// Tokens the model was trained on, and will run at.
        tokens: u32,
        /// Tokens that were asked for.
        asked_tokens: u32,
    },
    /// It fits, but only at a context shorter than the runtime's default.
    ReducedContext {
        /// Tokens that do fit.
        tokens: u32,
        /// Tokens the runtime asks for by default.
        default_tokens: u32,
    },
    /// Every layer runs on the accelerator.
    FullyAccelerated {
        /// How many layers that is.
        layers: u32,
    },
    /// Only part of the model fits the accelerator; the rest runs slower.
    PartialOffload {
        /// Layers on the accelerator.
        gpu_layers: u32,
        /// Layers the model has.
        total_layers: u32,
    },
    /// A mixture of experts, which reads only part of itself per token.
    SparseMixture {
        /// Share of the weights read for each token, as a percentage.
        active_percent: u32,
    },
    /// The quantisation this file uses, and what it gives up.
    Quantisation {
        /// The format's name, for example `"Q4_K_M"`.
        format: String,
        /// Capability retained against the unquantised weights, as a percentage.
        retained_percent: u32,
    },
    /// Something about the file could not be established.
    Caveat {
        /// The caveat itself, carried through for the interface to render.
        caveat: Caveat,
    },
}

/// The memory a model runs from, so a figure is never read against the wrong one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryPool {
    /// A discrete card's own memory.
    Gpu,
    /// One pool shared by processor and accelerator, as on Apple Silicon.
    Unified,
    /// System memory, read by the processor.
    System,
}

/// A candidate with its verdict, its cost, its placement and its rank.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Recommendation {
    /// The file this recommendation is about.
    pub file: FileCandidate,
    /// Id of the generation it belongs to.
    pub family_id: String,
    /// Label the operator reads.
    pub family_label: String,
    /// Size class in billions of parameters.
    pub params_b: f64,
    /// Quantisation format, when the file name declares one.
    pub quant: Option<String>,
    /// What the compatibility gate concluded.
    pub verdict: Verdict,
    /// Where the memory goes.
    pub estimate: MemoryEstimate,
    /// The fit grade against the pool the model runs from. A model split
    /// between a card and system memory is never better than `MightFit`.
    pub badge: CompatibilityBadge,
    /// The longest context that fits, when that is below the default.
    pub max_context: Option<u32>,
    /// Where the layers run, and how fast that is.
    pub offload: OffloadPlan,
    /// The ranking score. Comparable only within one call.
    pub score: f64,
    /// Why it sits here.
    pub reasons: Vec<Reason>,
}

/// Penalty applied to a generation another fitting candidate supersedes.
///
/// Larger than the whole score range, so the demotion is absolute rather than
/// something a strong older model can score its way out of.
const SUPERSEDED_PENALTY: f64 = 1000.0;

/// Bonus for fitting comfortably rather than tightly.
const COMFORT_BONUS: f64 = 12.0;

/// Penalty per caveat, enough to break a tie without hiding a model.
const CAVEAT_PENALTY: f64 = 4.0;

/// Penalty for a file whose repository needs a token the operator may not have.
const GATED_PENALTY: f64 = 8.0;

/// Throughput treated as neither fast nor slow, in tokens per second.
///
/// Around the speed of comfortable reading, which is the point past which more
/// is pleasant rather than necessary.
const REFERENCE_TOKENS_PER_SECOND: f64 = 20.0;

/// How many points a factor of e in throughput is worth.
///
/// Set so that halving the speed costs about five points, which can reorder two
/// adjacent size classes but cannot rescue a generation behind a newer one.
const SPEED_WEIGHT: f64 = 8.0;

/// Most the speed term can add, and most it can take away.
///
/// Deliberately asymmetric, because throughput is not felt symmetrically. Past
/// reading speed more is barely noticeable: the difference between sixty and
/// ninety tokens a second is nothing an operator waits on, and letting it score
/// would hand small models a bonus for a quality nobody perceives. Below it the
/// pain rises sharply, and a model generating at three tokens a second is a bad
/// recommendation however capable it is.
const SPEED_BONUS_CEILING: f64 = 4.0;
const SPEED_PENALTY_FLOOR: f64 = -20.0;

/// Estimated generation speed below which a model is not offered, in tokens
/// per second, as long as something faster fits.
///
/// Around a slow reading pace. Below it an answer of a few paragraphs takes
/// minutes, and an agent that chains tool calls takes far longer.
pub const MIN_TOKENS_PER_SECOND: f64 = 4.0;

/// One candidate paired with the table entry it was matched to.
pub struct Matched<'m> {
    /// The file from the Hub.
    pub file: FileCandidate,
    /// The probed header.
    pub facts: GgufHeaderFacts,
    /// The generation it belongs to.
    pub family: &'m Family,
    /// The size within that generation.
    pub variant: &'m Variant,
}

/// Grade a footprint against a capacity, on the thresholds the badge uses.
fn grade_against(total_gb: f64, capacity_gb: f64) -> CompatibilityBadge {
    if total_gb < capacity_gb * 0.70 {
        CompatibilityBadge::Fits
    } else if total_gb < capacity_gb {
        CompatibilityBadge::MightFit
    } else {
        CompatibilityBadge::TooLarge
    }
}

/// Where a model runs, judged against that pool alone.
///
/// Returns the grade and the reason that states it. The gate against
/// everything the machine can hold has already passed.
fn placement(
    architecture: MemoryArchitecture,
    estimate: &MemoryEstimate,
    offload: &OffloadPlan,
    capacity_gb: f64,
) -> (CompatibilityBadge, Reason) {
    let needs_gb = estimate.total_gb;
    let within = |pool: MemoryPool, budget_gb: f64| {
        // Never `TooLarge` here: the model passed the gate, and the placement
        // only says how much room it leaves.
        if needs_gb < budget_gb * 0.70 {
            (
                CompatibilityBadge::Fits,
                Reason::FitsComfortably {
                    needs_gb,
                    budget_gb,
                    pool,
                },
            )
        } else {
            (
                CompatibilityBadge::MightFit,
                Reason::Tight {
                    needs_gb,
                    budget_gb,
                    pool,
                },
            )
        }
    };
    match architecture {
        MemoryArchitecture::Unified { .. } => within(MemoryPool::Unified, capacity_gb),
        MemoryArchitecture::HostOnly { .. } => within(MemoryPool::System, capacity_gb),
        MemoryArchitecture::Discrete { vram_gb, .. } if offload.fully_offloaded => {
            within(MemoryPool::Gpu, vram_gb)
        }
        MemoryArchitecture::Discrete { host_gb, .. } if offload.n_gpu_layers == 0 => {
            within(MemoryPool::System, host_gb)
        }
        MemoryArchitecture::Discrete { vram_gb, .. } => (
            CompatibilityBadge::MightFit,
            Reason::SplitAcrossMemory {
                gpu_gb: offload.device_gb,
                vram_gb,
                system_gb: offload.host_gb,
            },
        ),
    }
}

/// Points awarded or deducted for generation speed.
fn speed_bonus(tokens_per_second: f64) -> f64 {
    if tokens_per_second <= 0.0 {
        return -SPEED_WEIGHT * 2.0;
    }
    (SPEED_WEIGHT * (tokens_per_second / REFERENCE_TOKENS_PER_SECOND).ln())
        .clamp(SPEED_PENALTY_FLOOR, SPEED_BONUS_CEILING)
}

/// Rank the matched candidates for one machine.
///
/// Candidates the engine will not load, and candidates past what the machine
/// can hold across all its pools, are dropped rather than ranked low: offering
/// a model that cannot run is a worse answer than offering fewer models.
///
/// The returned list is ordered best first.
#[must_use]
pub fn rank(
    candidates: Vec<Matched<'_>>,
    manifest: &FamilyManifest,
    profile: &HardwareProfile,
    asked: &RuntimeShape,
) -> Vec<Recommendation> {
    let architecture = MemoryArchitecture::of(profile);
    let accelerated = architecture.uses_accelerator();
    // A discrete card is not the whole machine: what overruns it runs on the
    // processor. Grading against the card alone is what hid every model larger
    // than the card from a machine that could run it across both pools.
    let capacity_gb = architecture.total_capacity_gb(profile);

    let mut kept: Vec<Recommendation> = Vec::new();
    for m in candidates {
        let verdict = assess(&m.file.filename, &m.facts);
        if !verdict.is_offerable() {
            continue;
        }

        // Each model is sized at the window it will actually run at, which is
        // the asked one capped at its training length, the same cap the engine
        // applies when it is launched.
        let shape = &RuntimeShape {
            n_ctx: crate::context_window::cap_at_trained(asked.n_ctx, m.facts.context_length),
            ..*asked
        };
        let estimate = estimate_memory(m.file.size_bytes, &m.facts, shape, accelerated);
        if grade_against(estimate.total_gb, capacity_gb) == CompatibilityBadge::TooLarge {
            continue;
        }

        let Some(base) = manifest.weighted_agentic(m.family, m.variant) else {
            continue;
        };

        let quant = quant::from_filename(&m.file.filename);
        let quality = base * quant.map_or(1.0, |q| q.quality);

        let offload = plan_offload(m.file.size_bytes, &m.facts, profile, shape);
        let (badge, fit_reason) = placement(architecture, &estimate, &offload, capacity_gb);
        let max_context = max_context_for(m.file.size_bytes, &m.facts, shape, profile, accelerated)
            .filter(|ctx| *ctx < shape.n_ctx);

        let caveat_count = match &verdict {
            Verdict::Caveats { caveats } => caveats.len(),
            _ => 0,
        };

        let mut score = quality + speed_bonus(offload.tokens_per_second);
        if badge == CompatibilityBadge::Fits {
            score += COMFORT_BONUS;
        }
        score -= CAVEAT_PENALTY * caveat_count as f64;
        if m.file.gated {
            score -= GATED_PENALTY;
        }

        let reasons = build_reasons(ReasonInput {
            verdict: &verdict,
            fit_reason,
            family: m.family,
            facts: &m.facts,
            offload: &offload,
            quant,
            max_context,
            shape,
            asked_tokens: asked.n_ctx,
        });

        kept.push(Recommendation {
            file: m.file,
            family_id: m.family.id.clone(),
            family_label: m.family.label.clone(),
            params_b: m.variant.params_b,
            quant: quant.map(|q| q.name.to_owned()),
            verdict,
            estimate,
            badge,
            max_context,
            offload,
            score,
            reasons,
        });
    }

    // Only when something fast enough remains: on a machine where nothing
    // reaches the floor, the fastest of the slow is still the honest answer.
    if kept
        .iter()
        .any(|r| r.offload.tokens_per_second >= MIN_TOKENS_PER_SECOND)
    {
        kept.retain(|r| r.offload.tokens_per_second >= MIN_TOKENS_PER_SECOND);
    }

    demote_superseded(&mut kept, manifest);

    kept.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            // A deterministic tail keeps the order stable across runs, which
            // matters because the list drives a default selection.
            .then_with(|| {
                b.params_b
                    .partial_cmp(&a.params_b)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .then_with(|| a.file.repo_id.cmp(&b.file.repo_id))
            .then_with(|| a.file.filename.cmp(&b.file.filename))
    });
    kept
}

/// Push any generation that a surviving candidate replaces below it.
///
/// Done against the survivors rather than against the whole table: superseding
/// a generation that does not fit this machine is no reason to demote the one
/// that does.
fn demote_superseded(kept: &mut [Recommendation], manifest: &FamilyManifest) {
    let surviving: Vec<String> = kept.iter().map(|r| r.family_id.clone()).collect();

    for rec in kept.iter_mut() {
        let replaced_by = surviving
            .iter()
            .filter(|id| *id != &rec.family_id)
            .find_map(|id| {
                let other = manifest.family(id)?;
                other
                    .supersedes
                    .iter()
                    .any(|s| s == &rec.family_id)
                    .then(|| other.label.clone())
            });

        if replaced_by.is_some() {
            rec.score -= SUPERSEDED_PENALTY;
            continue;
        }

        if let Some(predecessor) = manifest
            .family(&rec.family_id)
            .filter(|f| manifest.generation_depth(&f.id) > 0)
            .and_then(|f| f.supersedes.first())
            .and_then(|id| manifest.family(id))
            .map(|f| f.label.clone())
        {
            rec.reasons.push(Reason::SupersedesGeneration {
                replaces: predecessor,
            });
        }
    }
}

/// Everything a reason list is derived from.
struct ReasonInput<'a> {
    verdict: &'a Verdict,
    fit_reason: Reason,
    family: &'a Family,
    facts: &'a GgufHeaderFacts,
    offload: &'a OffloadPlan,
    quant: Option<&'static quant::Quant>,
    max_context: Option<u32>,
    shape: &'a RuntimeShape,
    asked_tokens: u32,
}

fn build_reasons(input: ReasonInput<'_>) -> Vec<Reason> {
    let ReasonInput {
        verdict,
        fit_reason,
        family,
        facts,
        offload,
        quant,
        max_context,
        shape,
        asked_tokens,
    } = input;

    let mut reasons = vec![fit_reason];

    if verdict.supports_tool_calling() && family.tool_calling == ToolCalling::Native {
        reasons.push(Reason::NativeToolCalling);
    }

    let share = active_weight_share(facts);
    if share < 0.95 {
        reasons.push(Reason::SparseMixture {
            active_percent: (share * 100.0).round() as u32,
        });
    }

    if offload.n_gpu_layers > 0 {
        if offload.fully_offloaded {
            reasons.push(Reason::FullyAccelerated {
                layers: offload.total_layers,
            });
        } else {
            reasons.push(Reason::PartialOffload {
                gpu_layers: offload.n_gpu_layers,
                total_layers: offload.total_layers,
            });
        }
    }

    if let Some(q) = quant {
        reasons.push(Reason::Quantisation {
            format: q.name.to_owned(),
            retained_percent: (q.quality * 100.0).round() as u32,
        });
    }

    if shape.n_ctx < asked_tokens {
        reasons.push(Reason::TrainedContext {
            tokens: shape.n_ctx,
            asked_tokens,
        });
    }

    if let Some(tokens) = max_context {
        reasons.push(Reason::ReducedContext {
            tokens,
            default_tokens: shape.n_ctx,
        });
    }

    if let Verdict::Caveats { caveats } = verdict {
        for caveat in caveats {
            reasons.push(Reason::Caveat {
                caveat: caveat.clone(),
            });
        }
    }

    reasons
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hardware::AcceleratorProfile;

    const GIB: f64 = 1024.0 * 1024.0 * 1024.0;

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

    fn host(budget_gb: f64) -> HardwareProfile {
        profile(AcceleratorProfile::None, budget_gb / 0.60, budget_gb)
    }

    fn apple(ram_gb: f64) -> HardwareProfile {
        profile(
            AcceleratorProfile::AppleSilicon {
                chip: "M4 Max".to_owned(),
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

    fn facts(arch: &str, blocks: u64, heads_kv: u64, embed: u64) -> GgufHeaderFacts {
        GgufHeaderFacts {
            version: 3,
            architecture: Some(arch.to_owned()),
            tokenizer_pre: Some("qwen2".to_owned()),
            has_chat_template: true,
            block_count: Some(blocks),
            head_count: Some(32),
            head_count_kv: Some(heads_kv),
            embedding_length: Some(embed),
            key_length: Some(128),
            value_length: Some(128),
            ..GgufHeaderFacts::default()
        }
    }

    fn file(repo: &str, name: &str, gb: f64) -> FileCandidate {
        FileCandidate {
            repo_id: repo.to_owned(),
            filename: name.to_owned(),
            download_url: format!("https://huggingface.co/{repo}/resolve/main/{name}"),
            size_bytes: (gb * GIB) as u64,
            gated: false,
        }
    }

    fn variant_of(family: &Family, params_b: f64) -> &Variant {
        family
            .variants
            .iter()
            .find(|v| (v.params_b - params_b).abs() < f64::EPSILON)
            .expect("the variant is in the table")
    }

    #[test]
    fn a_model_that_spills_off_the_card_is_reported_as_a_split() {
        // GIVEN a 14B that does not fit an 8 GB card but fits the machine
        let manifest = FamilyManifest::embedded().expect("the shipped table loads");
        let qwen3 = manifest.family("qwen3").expect("qwen3");
        let candidates = vec![Matched {
            file: file("Qwen/Qwen3-14B-GGUF", "Qwen3-14B-Q4_K_M.gguf", 8.4),
            facts: facts("qwen3", 40, 8, 5120),
            family: qwen3,
            variant: variant_of(qwen3, 14.0),
        }];

        // WHEN it is ranked on that machine
        let ranked = rank(
            candidates,
            &manifest,
            &cuda(8.0, 64.0),
            &RuntimeShape::default(),
        );

        // THEN it is never called comfortable, and its first reason states how
        // much lands on the card and how much in system memory, not a figure
        // measured against both pools added together
        assert_eq!(ranked.len(), 1);
        assert_eq!(ranked[0].badge, CompatibilityBadge::MightFit);
        match &ranked[0].reasons[0] {
            Reason::SplitAcrossMemory {
                gpu_gb,
                vram_gb,
                system_gb,
            } => {
                assert!(*gpu_gb <= *vram_gb);
                assert!(*system_gb > 0.0);
            }
            other => panic!("expected a split, got {other:?}"),
        }
    }

    #[test]
    fn a_model_that_fits_the_card_is_graded_against_the_card() {
        // GIVEN an 8B on a 24 GB card in a machine with 64 GB of memory
        let manifest = FamilyManifest::embedded().expect("the shipped table loads");
        let qwen3 = manifest.family("qwen3").expect("qwen3");
        let candidates = vec![Matched {
            file: file("Qwen/Qwen3-8B-GGUF", "Qwen3-8B-Q4_K_M.gguf", 4.7),
            facts: facts("qwen3", 36, 8, 4096),
            family: qwen3,
            variant: variant_of(qwen3, 8.0),
        }];

        // WHEN it is ranked
        let ranked = rank(
            candidates,
            &manifest,
            &cuda(24.0, 64.0),
            &RuntimeShape::default(),
        );

        // THEN the budget it is measured against is the card, not the card
        // plus system memory
        match &ranked[0].reasons[0] {
            Reason::FitsComfortably {
                budget_gb, pool, ..
            } => {
                assert_eq!(*pool, MemoryPool::Gpu);
                assert!((*budget_gb - 24.0).abs() < f64::EPSILON);
            }
            other => panic!("expected a comfortable fit on the card, got {other:?}"),
        }
    }

    #[test]
    fn a_model_too_slow_to_use_is_dropped_when_a_faster_one_fits() {
        // GIVEN a 14B at Q8_0 and a 4B at Q4_K_M on a processor-only machine,
        // where the 14B generates at about three tokens a second
        let manifest = FamilyManifest::embedded().expect("the shipped table loads");
        let qwen3 = manifest.family("qwen3").expect("qwen3");
        let slow = || Matched {
            file: file("Qwen/Qwen3-14B-GGUF", "Qwen3-14B-Q8_0.gguf", 14.6),
            facts: facts("qwen3", 40, 8, 5120),
            family: qwen3,
            variant: variant_of(qwen3, 14.0),
        };
        let fast = Matched {
            file: file("Qwen/Qwen3-4B-GGUF", "Qwen3-4B-Q4_K_M.gguf", 2.5),
            facts: facts("qwen3", 36, 8, 2560),
            family: qwen3,
            variant: variant_of(qwen3, 4.0),
        };

        // WHEN both are ranked, and then the slow one alone
        let both = rank(
            vec![slow(), fast],
            &manifest,
            &host(40.0),
            &RuntimeShape::default(),
        );
        let alone = rank(
            vec![slow()],
            &manifest,
            &host(40.0),
            &RuntimeShape::default(),
        );

        // THEN the slow one is withheld while something usable fits, and kept
        // when it is all the machine can run
        assert_eq!(both.len(), 1);
        assert!(both[0].offload.tokens_per_second >= MIN_TOKENS_PER_SECOND);
        assert_eq!(alone.len(), 1);
        assert!(alone[0].offload.tokens_per_second < MIN_TOKENS_PER_SECOND);
    }

    #[test]
    fn a_model_trained_on_less_than_the_asked_window_is_sized_at_its_own() {
        // GIVEN an 8B trained on 8192 tokens, and a 64k window asked
        let manifest = FamilyManifest::embedded().expect("the shipped table loads");
        let qwen3 = manifest.family("qwen3").expect("qwen3");
        let mut short = facts("qwen3", 36, 8, 4096);
        short.context_length = Some(8_192);
        let asked = RuntimeShape {
            n_ctx: 65_536,
            ..RuntimeShape::default()
        };
        let candidate = || Matched {
            file: file("Qwen/Qwen3-8B-GGUF", "Qwen3-8B-Q4_K_M.gguf", 4.7),
            facts: short.clone(),
            family: qwen3,
            variant: variant_of(qwen3, 8.0),
        };

        // WHEN it is ranked at that window and at its own
        let capped = rank(vec![candidate()], &manifest, &host(40.0), &asked);
        let own = rank(
            vec![candidate()],
            &manifest,
            &host(40.0),
            &RuntimeShape {
                n_ctx: 8_192,
                ..RuntimeShape::default()
            },
        );

        // THEN its cache is the one it will really allocate, and the row says
        // it runs at 8k rather than at the 64k asked
        assert!((capped[0].estimate.kv_cache_gb - own[0].estimate.kv_cache_gb).abs() < 1e-9);
        assert!(capped[0].reasons.contains(&Reason::TrainedContext {
            tokens: 8_192,
            asked_tokens: 65_536,
        }));
    }

    #[test]
    fn the_newer_generation_wins_even_when_the_older_one_scores_well() {
        // GIVEN a Qwen3 and a Qwen2.5 candidate that both fit
        let manifest = FamilyManifest::embedded().expect("the shipped table loads");
        let qwen3 = manifest.family("qwen3").expect("qwen3");
        let qwen25 = manifest.family("qwen2.5").expect("qwen2.5");

        let candidates = vec![
            Matched {
                file: file("Qwen/Qwen2.5-14B-Instruct-GGUF", "a-Q4_K_M.gguf", 8.4),
                facts: facts("qwen2", 48, 8, 5120),
                family: qwen25,
                variant: variant_of(qwen25, 14.0),
            },
            Matched {
                file: file("Qwen/Qwen3-14B-GGUF", "b-Q4_K_M.gguf", 8.4),
                facts: facts("qwen3", 40, 8, 5120),
                family: qwen3,
                variant: variant_of(qwen3, 14.0),
            },
        ];

        // WHEN they are ranked on a machine with room for both
        let ranked = rank(candidates, &manifest, &host(64.0), &RuntimeShape::default());

        // THEN the newer generation leads, which is the ordering any popularity
        // signal on the Hub would have inverted
        assert_eq!(ranked.len(), 2);
        assert_eq!(ranked[0].family_id, "qwen3");
    }

    #[test]
    fn a_bigger_model_at_lower_precision_beats_a_smaller_one_at_higher() {
        // GIVEN a 14B at Q3_K_M and an 8B at Q4_K_M, both fitting
        let manifest = FamilyManifest::embedded().expect("the shipped table loads");
        let qwen3 = manifest.family("qwen3").expect("qwen3");

        let candidates = vec![
            Matched {
                file: file("Qwen/Qwen3-8B-GGUF", "Qwen3-8B-Q4_K_M.gguf", 4.7),
                facts: facts("qwen3", 36, 8, 4096),
                family: qwen3,
                variant: variant_of(qwen3, 8.0),
            },
            Matched {
                file: file("Qwen/Qwen3-14B-GGUF", "Qwen3-14B-Q3_K_M.gguf", 6.8),
                facts: facts("qwen3", 40, 8, 5120),
                family: qwen3,
                variant: variant_of(qwen3, 14.0),
            },
        ];

        // WHEN they are ranked on a machine with bandwidth to spare, so the
        // comparison is about capability rather than about throughput
        let ranked = rank(
            candidates,
            &manifest,
            &apple(64.0),
            &RuntimeShape::default(),
        );

        // THEN the larger model leads despite the harder quantisation, and the
        // format it used is reported rather than left implicit
        assert_eq!(ranked[0].params_b, 14.0);
        assert_eq!(ranked[0].quant.as_deref(), Some("Q3_K_M"));
    }

    #[test]
    fn a_mixture_leads_on_unified_memory_and_trails_on_a_small_card() {
        // GIVEN a 30B mixture and a dense 8B
        let manifest = FamilyManifest::embedded().expect("the shipped table loads");
        let qwen3 = manifest.family("qwen3").expect("qwen3");

        let build = || {
            let mut moe = facts("qwen3moe", 48, 4, 2048);
            moe.expert_count = Some(128);
            moe.expert_used_count = Some(8);
            vec![
                Matched {
                    file: file("Qwen/Qwen3-30B-A3B-GGUF", "moe-Q4_K_M.gguf", 18.6),
                    facts: moe,
                    family: qwen3,
                    variant: variant_of(qwen3, 30.0),
                },
                Matched {
                    file: file("Qwen/Qwen3-8B-GGUF", "dense-Q4_K_M.gguf", 4.7),
                    facts: facts("qwen3", 36, 8, 4096),
                    family: qwen3,
                    variant: variant_of(qwen3, 8.0),
                },
            ]
        };
        let shape = RuntimeShape::default();

        // WHEN the same pair is ranked on a roomy Mac and on a 12 GB card
        let on_mac = rank(build(), &manifest, &apple(64.0), &shape);
        let on_card = rank(build(), &manifest, &cuda(12.0, 64.0), &shape);

        // THEN the mixture leads where its occupancy is affordable and trails
        // where it is not, which is a property of the pairing rather than of
        // either model
        assert_eq!(on_mac[0].params_b, 30.0, "the mixture should lead on a Mac");
        assert_eq!(
            on_card[0].params_b, 8.0,
            "the dense model should lead on a small card"
        );
    }

    #[test]
    fn a_model_larger_than_the_card_is_still_offered_to_a_machine_with_memory() {
        // GIVEN a 12 GB card beside 64 GB of system memory, and an 18.6 GB model
        let manifest = FamilyManifest::embedded().expect("the shipped table loads");
        let qwen3 = manifest.family("qwen3").expect("qwen3");
        let candidates = vec![Matched {
            file: file("Qwen/Qwen3-30B-A3B-GGUF", "big-Q4_K_M.gguf", 18.6),
            facts: facts("qwen3moe", 48, 4, 2048),
            family: qwen3,
            variant: variant_of(qwen3, 30.0),
        }];

        // WHEN it is ranked
        let ranked = rank(
            candidates,
            &manifest,
            &cuda(12.0, 64.0),
            &RuntimeShape::default(),
        );

        // THEN it survives, split across both pools, rather than being hidden
        // by a budget that counted only the card
        let top = ranked.first().expect("the model is offered");
        assert!(!top.offload.fully_offloaded);
        assert!(top.offload.n_gpu_layers > 0);
        assert!(top.offload.host_gb > 0.0);
        assert!(top
            .reasons
            .iter()
            .any(|r| matches!(r, Reason::PartialOffload { .. })));
    }

    #[test]
    fn a_model_past_every_pool_is_dropped_rather_than_ranked_low() {
        // GIVEN a small machine and a model larger than all of its memory
        let manifest = FamilyManifest::embedded().expect("the shipped table loads");
        let qwen3 = manifest.family("qwen3").expect("qwen3");
        let candidates = vec![Matched {
            file: file("Qwen/Qwen3-30B-A3B-GGUF", "big-Q4_K_M.gguf", 18.6),
            facts: facts("qwen3moe", 48, 4, 2048),
            family: qwen3,
            variant: variant_of(qwen3, 30.0),
        }];

        // WHEN it is ranked on an 8 GB budget with no accelerator
        let ranked = rank(candidates, &manifest, &host(8.0), &RuntimeShape::default());

        // THEN nothing comes back, because offering it would offer a download
        // that cannot run
        assert!(ranked.is_empty());
    }

    #[test]
    fn a_sharded_file_never_reaches_the_ranking() {
        // GIVEN a candidate whose file name is one shard of a split model
        let manifest = FamilyManifest::embedded().expect("the shipped table loads");
        let qwen3 = manifest.family("qwen3").expect("qwen3");
        let candidates = vec![Matched {
            file: file("bartowski/x", "m-Q4_K_M-00001-of-00003.gguf", 2.5),
            facts: facts("qwen3", 36, 8, 2560),
            family: qwen3,
            variant: variant_of(qwen3, 4.0),
        }];

        // WHEN it is ranked on a machine with ample memory
        // THEN the compatibility gate removed it before scoring
        assert!(rank(candidates, &manifest, &host(64.0), &RuntimeShape::default()).is_empty());
    }

    #[test]
    fn a_model_without_a_chat_template_ranks_below_one_with_it() {
        // GIVEN the same model twice, once with a template and once without
        let manifest = FamilyManifest::embedded().expect("the shipped table loads");
        let qwen3 = manifest.family("qwen3").expect("qwen3");
        let mut without = facts("qwen3", 36, 8, 4096);
        without.has_chat_template = false;

        let candidates = vec![
            Matched {
                file: file("other/x", "no-template-Q4_K_M.gguf", 4.7),
                facts: without,
                family: qwen3,
                variant: variant_of(qwen3, 8.0),
            },
            Matched {
                file: file("Qwen/Qwen3-8B-GGUF", "with-template-Q4_K_M.gguf", 4.7),
                facts: facts("qwen3", 36, 8, 4096),
                family: qwen3,
                variant: variant_of(qwen3, 8.0),
            },
        ];

        // WHEN they are ranked
        let ranked = rank(candidates, &manifest, &host(32.0), &RuntimeShape::default());

        // THEN the one that can call tools leads, and the other is still offered
        assert_eq!(ranked.len(), 2);
        assert_eq!(ranked[0].file.filename, "with-template-Q4_K_M.gguf");
        assert!(!ranked[1].verdict.supports_tool_calling());
    }

    #[test]
    fn the_leading_recommendation_explains_itself() {
        // GIVEN one comfortable, natively tool-calling candidate on a Mac
        let manifest = FamilyManifest::embedded().expect("the shipped table loads");
        let qwen3 = manifest.family("qwen3").expect("qwen3");
        let candidates = vec![Matched {
            file: file("Qwen/Qwen3-4B-GGUF", "Qwen3-4B-Q4_K_M.gguf", 2.5),
            facts: facts("qwen3", 36, 8, 2560),
            family: qwen3,
            variant: variant_of(qwen3, 4.0),
        }];

        // WHEN it is ranked
        let ranked = rank(
            candidates,
            &manifest,
            &apple(32.0),
            &RuntimeShape::default(),
        );

        // THEN it carries the reasons an interface needs to justify the default
        let top = ranked.first().expect("one recommendation");
        assert!(top.estimate.kv_cache_measured);
        assert!(top.reasons.contains(&Reason::NativeToolCalling));
        assert!(top
            .reasons
            .iter()
            .any(|r| matches!(r, Reason::FitsComfortably { .. })));
        assert!(top
            .reasons
            .iter()
            .any(|r| matches!(r, Reason::Quantisation { .. })));
        assert!(top
            .reasons
            .iter()
            .any(|r| matches!(r, Reason::FullyAccelerated { .. })));
    }

    #[test]
    fn speed_is_worth_points_but_cannot_rescue_a_superseded_generation() {
        // GIVEN the speed bonus at its extremes
        let fast = speed_bonus(200.0);
        let slow = speed_bonus(1.0);

        // WHEN both are compared against the supersession penalty
        // THEN neither can approach it, so a newer generation always leads
        assert!(fast > 0.0 && slow < 0.0);
        assert!(fast - slow < SUPERSEDED_PENALTY);
    }

    #[test]
    fn extra_speed_past_reading_pace_stops_earning_points() {
        // GIVEN two comfortably fast machines and one painfully slow result
        let brisk = speed_bonus(60.0);
        let faster = speed_bonus(200.0);
        let crawling = speed_bonus(2.0);

        // WHEN the three are compared
        // THEN the two fast figures score the same, so a small model cannot
        // win on throughput nobody perceives, while the slow one is punished
        assert_eq!(brisk, faster);
        assert_eq!(brisk, SPEED_BONUS_CEILING);
        assert!(crawling < -5.0);
    }
}
