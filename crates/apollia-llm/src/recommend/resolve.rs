//! Turn the curated table plus the live Hub into a ranked list for one machine.
//!
//! The division of labour, once more, because it drives every choice here: the
//! table says which generations exist and how they order; the Hub says which
//! repositories carry them, what their files weigh, and which are gated. The
//! table is compiled in and never stale about ordering. The Hub is authoritative
//! and never compiled in.
//!
//! # Why this runs in three stages
//!
//! Resolving every variant of every generation against every publisher would be
//! on the order of seventy `get_model` calls, each of which is three HTTP
//! requests, before a single byte of a model is read. That is not a thing to do
//! while an operator watches an onboarding spinner.
//!
//! So:
//!
//! 1. **Plan.** Estimate each variant's footprint from its parameter count
//!    alone and drop what cannot fit, then order by the table's score and keep
//!    the best few. No network at all.
//! 2. **Locate.** For each survivor, walk its publishers in preference order
//!    and stop at the first that actually carries a usable file. Concurrent,
//!    bounded.
//! 3. **Probe.** Read the GGUF header of the files that remain, then rank on
//!    what was measured rather than on what was assumed.
//!
//! The estimate in stage 1 is deliberately generous: dropping a model that
//! would have fitted is the one error this stage cannot recover from, whereas
//! keeping one that turns out not to fit costs a single request and is caught
//! in stage 3.

#![cfg(feature = "cloud")]

use futures::stream::{self, StreamExt};
use thiserror::Error;
use tracing::{event, Level};

use crate::gguf_probe::{probe_url, ProbeDepth};
use crate::hardware::HardwareProfile;
use crate::hf_registry::{HfError, HfModelTypeCache, HfRegistryClient};

use super::fit::RuntimeShape;
use super::manifest::{Family, FamilyManifest, Variant};
use super::offload::MemoryArchitecture;
use super::quant::{self, Quant};
use super::rank::{rank, FileCandidate, Matched, Recommendation};

/// Failure to produce a recommendation.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum ResolveError {
    /// The Hub could not be reached at all.
    ///
    /// Distinguished from "nothing matched" on purpose. The catalogue is live,
    /// so with no network there is no catalogue, and an interface that showed
    /// an empty list here would be claiming the machine can run nothing. It
    /// should instead offer the paths that do not need the Hub: importing a
    /// model already on disk, or configuring a cloud backend.
    #[error("the HuggingFace Hub is unreachable: {0}")]
    HubUnreachable(String),

    /// The embedded generation table is malformed.
    #[error("the model family table could not be loaded: {0}")]
    Manifest(#[from] super::manifest::ManifestError),
}

/// How the resolver is allowed to spend requests and what it optimises for.
#[derive(Debug, Clone)]
pub struct ResolveOptions {
    /// How many planned variants survive into the locate stage.
    pub max_planned: usize,
    /// How many located files get their header probed.
    pub max_probed: usize,
    /// Concurrent Hub requests.
    pub concurrency: usize,
    /// Token for gated repositories, when the operator has supplied one.
    pub hf_token: Option<String>,
    /// The launch settings the footprint is computed against.
    pub shape: RuntimeShape,
}

impl Default for ResolveOptions {
    fn default() -> Self {
        Self {
            max_planned: 8,
            max_probed: 5,
            concurrency: 4,
            hf_token: None,
            shape: RuntimeShape::default(),
        }
    }
}

/// Slack applied to the planning estimate, in favour of keeping a candidate.
///
/// Stage 1 has no header to measure, so it errs towards keeping: a candidate
/// wrongly dropped here is never reconsidered, while one wrongly kept costs a
/// single request and is caught once its header is read.
const PLANNING_SLACK: f64 = 1.25;

/// Share of the weights assumed for the cache while planning.
///
/// The real figure needs the header, which is two stages away. A third is
/// roughly what a dense model with grouped-query attention reserves at the
/// runtime context, and erring high here only costs a request.
const PLANNING_CACHE_SHARE: f64 = 0.35;

/// A variant at one quantisation, and the score that got it into the plan.
#[derive(Debug, Clone)]
struct Planned<'m> {
    family: &'m Family,
    variant: &'m Variant,
    /// The quantisation this candidate was planned at.
    ///
    /// Carried rather than decided later, because it is half of what the plan
    /// is choosing: a 14B at three bits and an 8B at four are different
    /// candidates competing for the same memory.
    quant: &'static Quant,
    /// The table's score discounted by what the quantisation gives up.
    score: f64,
}

/// Choose which model and quantisation pairings are worth a request.
///
/// The search runs over both axes at once. Fixing the quantisation first, which
/// is what this did originally, makes the interesting comparison inexpressible:
/// on a machine that cannot hold a 14B at four bits, the question is whether a
/// 14B at three bits beats an 8B at four, and it usually does.
///
/// For each size only the most faithful quantisation that fits is planned. A
/// harder one of the same model is strictly worse, so there is nothing to gain
/// by asking the Hub about it as well.
///
/// Pure and offline, which is what makes it testable without a network.
fn plan<'m>(
    manifest: &'m FamilyManifest,
    profile: &HardwareProfile,
    options: &ResolveOptions,
) -> Vec<Planned<'m>> {
    // What the machine can hold across every pool it has, not just the
    // accelerator: a model larger than the card still runs with its tail on
    // the processor.
    let capacity_gb = MemoryArchitecture::of(profile).total_capacity_gb(profile) * PLANNING_SLACK;
    let mut planned: Vec<Planned<'m>> = Vec::new();

    for family in manifest.families() {
        // A generation that some other generation in the table replaces is not
        // worth a request: even if it resolved, the ranking would demote it
        // below its successor. Spending the request would only slow onboarding.
        if manifest.is_superseded(&family.id) {
            continue;
        }

        for variant in &family.variants {
            let Some(base) = manifest.weighted_agentic(family, variant) else {
                continue;
            };

            // From the sweet spot downwards, so the first that fits is the
            // one to plan. Starting above it would spend memory on a percent of
            // quality that the next size class up would spend far better.
            let best = quant::planning_candidates().find(|q| {
                let weights_gb = q.estimated_bytes(variant.params_b) as f64 / 1_073_741_824.0;
                weights_gb * (1.0 + PLANNING_CACHE_SHARE) + 0.25 <= capacity_gb
            });

            if let Some(quant) = best {
                planned.push(Planned {
                    family,
                    variant,
                    quant,
                    score: base * quant.quality,
                });
            }
        }
    }

    planned.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| {
                b.variant
                    .params_b
                    .partial_cmp(&a.variant.params_b)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .then_with(|| a.family.id.cmp(&b.family.id))
    });
    planned.truncate(options.max_planned);
    planned
}

/// File name prefixes that belong to something other than the model.
///
/// A GGUF repository holds more than the weights. `mmproj-*` is the vision
/// projector a multimodal model loads beside itself, and `mtp-*` is the
/// multi-token-prediction head. Both are published at every quantisation, both
/// are a fraction of a gigabyte, and both therefore look to a size-and-format
/// comparison like the most faithful file that fits. Onboarding offered
/// `MTP/mtp-gemma-4-12b-it-Q8_0.gguf` as Gemma 4 before this list existed.
const AUXILIARY_PREFIXES: &[&str] = &["mmproj", "mtp-", "mtp_"];

/// Least bits per weight any real quantisation of a model uses.
///
/// Below the smallest offered format by a wide margin, so this rejects a file
/// that cannot be the model at all rather than one that is merely small. It is
/// the general form of the guard above: a future auxiliary file under a name
/// nobody has seen is still caught by being far too light for its size class.
const MIN_PLAUSIBLE_BITS_PER_WEIGHT: f64 = 1.5;

/// Whether a file name belongs to an auxiliary module rather than the weights.
fn is_auxiliary(path: &str) -> bool {
    let name = path
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(path)
        .to_ascii_lowercase();
    AUXILIARY_PREFIXES.iter().any(|p| name.starts_with(p))
}

/// Pick the best file in one repository for this machine.
///
/// The planned quantisation is preferred; a repository that does not publish it
/// falls back to the most faithful one it does publish that still fits, so a
/// publisher shipping only `Q5_K_M` is still usable.
///
/// Shards are skipped rather than rejected later, because the same repository
/// usually also publishes an unsplit file at a smaller quantisation, and
/// offering that is better than offering nothing.
fn choose_file(
    files: &[crate::hf_registry::HfFile],
    capacity_gb: f64,
    planned: &Quant,
    params_b: f64,
    gated: bool,
) -> Option<FileCandidate> {
    let mut best: Option<(f64, &crate::hf_registry::HfFile)> = None;
    let floor_bytes = (params_b * 1e9 * MIN_PLAUSIBLE_BITS_PER_WEIGHT / 8.0) as u64;

    for file in files {
        if super::verdict::shard_count_of(&file.filename).is_some() {
            continue;
        }
        if is_auxiliary(&file.filename) || file.size_bytes < floor_bytes {
            continue;
        }
        let Some(found) = quant::from_filename(&file.filename) else {
            continue;
        };
        // Leave room for the cache; the real check comes once the header is read.
        let weights_gb = file.size_bytes as f64 / 1_073_741_824.0;
        if weights_gb * (1.0 + PLANNING_CACHE_SHARE) + 0.25 > capacity_gb * PLANNING_SLACK {
            continue;
        }

        // An exact match on the plan wins outright; otherwise the closest
        // format to it. Taking the most faithful instead would quietly undo the
        // trade the plan just made, since the most faithful on offer is usually
        // Q8_0 and nearly twice the download.
        let rank = if found.name == planned.name {
            f64::INFINITY
        } else {
            -quant::distance(found, planned)
        };
        if best.is_none_or(|(best_rank, _)| rank > best_rank) {
            best = Some((rank, file));
        }
    }

    best.map(|(_, file)| FileCandidate {
        repo_id: String::new(), // filled by the caller, which knows the repo
        filename: file.filename.clone(),
        download_url: file.download_url.clone(),
        size_bytes: file.size_bytes,
        gated,
    })
}

/// What one lookup is looking for.
///
/// Grouped rather than passed one by one: these four travel together from the
/// plan to the file choice, and a positional list of them is easy to get wrong
/// at the call site.
struct Target {
    /// The model name, for the log line.
    label: String,
    /// Publishers to try, in preference order.
    repos: Vec<String>,
    /// The quantisation the plan settled on.
    planned: &'static Quant,
    /// Size class, for the sanity check on a file that is too light to be real.
    params_b: f64,
}

/// The things every Hub lookup needs, carried by value.
///
/// All fields are borrows, so the struct is `Copy` and each future gets its own
/// copy rather than a borrow of a shared one. That keeps the lifetime story
/// simple enough for the async machinery described on [`locate`].
#[derive(Clone, Copy)]
struct Hub<'a> {
    client: &'a HfRegistryClient,
    type_cache: &'a HfModelTypeCache,
    profile: &'a HardwareProfile,
    /// Everything the machine can hold, across the accelerator and the host.
    capacity_gb: f64,
}

/// Ask the Hub where one planned variant actually lives.
///
/// Walks the publishers in preference order and stops at the first that carries
/// a usable file, so the common case costs one `get_model`.
///
/// Takes an index and owned strings rather than a borrowed [`Planned`]. A
/// future that holds a borrow of the manifest gives the enclosing async
/// function a higher-ranked lifetime which `tauri::command` cannot prove is
/// general enough, and the desktop binary then fails to compile with an error
/// pointing at the command rather than at this signature. The caller pairs the
/// index back up once the requests have settled.
async fn locate(hub: Hub<'_>, index: usize, target: Target) -> Option<(usize, FileCandidate)> {
    let Target {
        label,
        repos,
        planned,
        params_b,
    } = target;
    for repo_id in repos {
        let card = match hub
            .client
            .get_model(&repo_id, Some(hub.profile), Some(hub.type_cache))
            .await
        {
            Ok(card) => card,
            Err(HfError::NotFound(_)) => continue,
            Err(err) => {
                event!(
                    Level::DEBUG,
                    repo = %repo_id,
                    error = %err,
                    "recommend.locate.repo_failed"
                );
                continue;
            }
        };

        if let Some(mut candidate) = choose_file(
            &card.gguf_files,
            hub.capacity_gb,
            planned,
            params_b,
            card.gated,
        ) {
            candidate.repo_id = repo_id;
            event!(
                Level::DEBUG,
                model = %label,
                repo = %candidate.repo_id,
                file = %candidate.filename,
                "recommend.locate.resolved"
            );
            return Some((index, candidate));
        }
    }

    event!(Level::DEBUG, model = %label, "recommend.locate.no_publisher");
    None
}

/// Read one finalist's GGUF header.
///
/// Owned for the same reason [`locate`] is.
async fn probe_one(
    client: reqwest::Client,
    index: usize,
    file: FileCandidate,
) -> Option<(usize, FileCandidate, crate::gguf_probe::GgufHeaderFacts)> {
    // Confirm depth: this is the shortlist the operator will actually see, and
    // the chat template lives past the vocabulary. Screening depth would leave
    // every entry carrying a truncation caveat and no tool-calling answer.
    match probe_url(&client, &file.download_url, ProbeDepth::Confirm, None).await {
        Ok(facts) => Some((index, file, facts)),
        Err(err) => {
            event!(
                Level::DEBUG,
                file = %file.filename,
                error = %err,
                "recommend.probe.failed"
            );
            None
        }
    }
}

/// Longest a whole resolution may take before it gives up.
///
/// Every request inside it already has its own timeout, but a slow link walks
/// several of them in turn, and onboarding is a screen an operator watches. Past
/// this the answer is "could not look", which offers the paths that need no
/// network, rather than a spinner with no end.
pub const RESOLVE_DEADLINE: std::time::Duration = std::time::Duration::from_secs(90);

/// Produce the ranked recommendations for one machine, within
/// [`RESOLVE_DEADLINE`].
///
/// # Errors
/// [`ResolveError::HubUnreachable`] when no publisher could be reached at all,
/// or when the deadline passed first. A caller should surface both as "no
/// catalogue" rather than as "no models".
pub async fn resolve(
    manifest: &FamilyManifest,
    profile: &HardwareProfile,
    options: &ResolveOptions,
) -> Result<Vec<Recommendation>, ResolveError> {
    match tokio::time::timeout(
        RESOLVE_DEADLINE,
        resolve_unbounded(manifest, profile, options),
    )
    .await
    {
        Ok(result) => result,
        Err(_) => {
            event!(
                Level::WARN,
                deadline_secs = RESOLVE_DEADLINE.as_secs(),
                "recommend.deadline.exceeded"
            );
            Err(ResolveError::HubUnreachable(format!(
                "no answer within {} seconds",
                RESOLVE_DEADLINE.as_secs()
            )))
        }
    }
}

/// [`resolve`] without the deadline.
async fn resolve_unbounded(
    manifest: &FamilyManifest,
    profile: &HardwareProfile,
    options: &ResolveOptions,
) -> Result<Vec<Recommendation>, ResolveError> {
    let planned = plan(manifest, profile, options);
    event!(
        Level::INFO,
        planned = planned.len(),
        budget_gb = profile.memory_budget_gb,
        "recommend.plan.ready"
    );
    if planned.is_empty() {
        return Ok(Vec::new());
    }

    let client = HfRegistryClient::new(options.hf_token.clone());
    let type_cache = HfModelTypeCache::new();
    let hub = Hub {
        client: &client,
        type_cache: &type_cache,
        profile,
        capacity_gb: MemoryArchitecture::of(profile).total_capacity_gb(profile),
    };

    // Options collected then flattened, rather than `filter_map` inside the
    // stream: an `async move` closure there runs into the same higher-ranked
    // lifetime problem described on `locate`.
    let locate_calls: Vec<_> = planned
        .iter()
        .enumerate()
        .map(|(index, p)| {
            locate(
                hub,
                index,
                Target {
                    label: p.variant.hf_name.clone(),
                    repos: p.family.candidate_repos(p.variant),
                    planned: p.quant,
                    params_b: p.variant.params_b,
                },
            )
        })
        .collect();

    let mut resolved: Vec<(usize, FileCandidate)> = stream::iter(locate_calls)
        .buffer_unordered(options.concurrency)
        .collect::<Vec<_>>()
        .await
        .into_iter()
        .flatten()
        .collect();

    if resolved.is_empty() {
        // Every publisher refused or could not be reached. The two are
        // indistinguishable from here, and the conservative reading is the one
        // that does not tell the operator their machine can run nothing.
        return Err(ResolveError::HubUnreachable(
            "no publisher answered for any planned model".to_owned(),
        ));
    }

    // `plan` already ordered by score, so the planning index is the ranking
    // order; sorting by it restores that after the unordered fan-out.
    resolved.sort_by_key(|(index, _)| *index);
    resolved.truncate(options.max_probed);

    let probe_client = apollia_core::net::safe_client_builder()
        .user_agent("Apollia-OS/1.0")
        .timeout(std::time::Duration::from_secs(45))
        .build()
        .map_err(|e| ResolveError::HubUnreachable(e.to_string()))?;

    let probe_calls: Vec<_> = resolved
        .into_iter()
        .map(|(index, file)| probe_one(probe_client.clone(), index, file))
        .collect();

    let probed: Vec<(usize, FileCandidate, crate::gguf_probe::GgufHeaderFacts)> =
        stream::iter(probe_calls)
            .buffer_unordered(options.concurrency)
            .collect::<Vec<_>>()
            .await
            .into_iter()
            .flatten()
            .collect();

    let matched: Vec<Matched<'_>> = probed
        .into_iter()
        .filter_map(|(index, file, facts)| {
            let p = planned.get(index)?;
            Some(Matched {
                file,
                facts,
                family: p.family,
                variant: p.variant,
            })
        })
        .collect();

    let ranked = rank(matched, manifest, profile, &options.shape);
    event!(
        Level::INFO,
        recommended = ranked.len(),
        top = ranked
            .first()
            .map(|r| r.family_id.as_str())
            .unwrap_or("none"),
        "recommend.ready"
    );
    Ok(ranked)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hardware::AcceleratorProfile;

    fn profile(budget_gb: f64) -> HardwareProfile {
        HardwareProfile {
            total_ram_gb: budget_gb / 0.60,
            available_ram_gb: budget_gb / 0.60,
            cpu_model: "test".to_owned(),
            cpu_cores: 8,
            accelerator: AcceleratorProfile::None,
            memory_budget_gb: budget_gb,
        }
    }

    fn cuda(vram_gb: f64, ram_gb: f64) -> HardwareProfile {
        HardwareProfile {
            total_ram_gb: ram_gb,
            available_ram_gb: ram_gb,
            cpu_model: "test".to_owned(),
            cpu_cores: 8,
            accelerator: AcceleratorProfile::Cuda {
                device_name: "test".to_owned(),
                vram_gb,
                compute_capability: (8, 9),
            },
            memory_budget_gb: vram_gb,
        }
    }

    fn hf_file(name: &str, gb: f64) -> crate::hf_registry::HfFile {
        crate::hf_registry::HfFile {
            filename: name.to_owned(),
            size_bytes: (gb * 1024.0 * 1024.0 * 1024.0) as u64,
            size_human: format!("{gb} GB"),
            compatibility: None,
            download_url: format!("https://example.invalid/{name}"),
        }
    }

    fn q(name: &str) -> &'static Quant {
        quant::by_name(name).expect("in the table")
    }

    #[test]
    fn planning_drops_what_a_small_machine_cannot_hold() {
        // GIVEN the shipped table and a machine with a 6 GB budget
        let manifest = FamilyManifest::embedded().expect("the shipped table loads");

        // WHEN the plan is built
        let planned = plan(&manifest, &profile(6.0), &ResolveOptions::default());

        // THEN nothing large survives, and the plan is not empty, because a
        // small machine still has something it can run
        assert!(!planned.is_empty());
        assert!(
            planned.iter().all(|p| p.variant.params_b <= 14.0),
            "a 6 GB budget was planned something far too large"
        );
    }

    #[test]
    fn a_tight_machine_is_planned_a_harder_quantisation() {
        // GIVEN one generous machine and one tight one
        let manifest = FamilyManifest::embedded().expect("the shipped table loads");
        let options = ResolveOptions::default();

        // WHEN both are planned
        let roomy = plan(&manifest, &profile(96.0), &options);
        let tight = plan(&manifest, &profile(7.0), &options);

        // THEN the roomy machine is offered a faithful quantisation of a large
        // model, and the tight one a harder quantisation, which is the trade a
        // planner fixed at one format could not make
        let roomy_top = roomy.first().expect("something is planned");
        let tight_top = tight.first().expect("something is planned");
        assert!(roomy_top.variant.params_b >= tight_top.variant.params_b);
        assert!(
            tight_top.quant.bits_per_weight <= roomy_top.quant.bits_per_weight,
            "the tight machine was planned {} against {}",
            tight_top.quant.name,
            roomy_top.quant.name
        );
    }

    #[test]
    fn a_card_smaller_than_the_model_still_gets_a_plan() {
        // GIVEN a 12 GB card beside 64 GB of system memory
        let manifest = FamilyManifest::embedded().expect("the shipped table loads");

        // WHEN the plan is built
        let planned = plan(&manifest, &cuda(12.0, 64.0), &ResolveOptions::default());

        // THEN models larger than the card are planned anyway, because what
        // overruns it runs on the processor rather than not at all
        assert!(
            planned.iter().any(|p| p.variant.params_b > 12.0),
            "a machine with 64 GB of system memory was planned nothing above its card"
        );
    }

    #[test]
    fn planning_skips_a_generation_the_table_already_replaces() {
        // GIVEN the shipped table, whose Qwen chain runs 2.5, then 3, then 3.5
        let manifest = FamilyManifest::embedded().expect("the shipped table loads");

        // WHEN a plan is built for a machine with room for any of them
        let planned = plan(&manifest, &profile(64.0), &ResolveOptions::default());

        // THEN no request is planned for a generation something replaces, only
        // for the head of the chain
        assert!(planned.iter().all(|p| p.family.id != "qwen2.5"));
        assert!(planned.iter().all(|p| p.family.id != "qwen3"));
        assert!(planned.iter().any(|p| p.family.id == "qwen3.5"));
    }

    #[test]
    fn planning_orders_by_score_and_respects_the_request_budget() {
        // GIVEN a generous machine and a plan limited to three candidates
        let manifest = FamilyManifest::embedded().expect("the shipped table loads");
        let options = ResolveOptions {
            max_planned: 3,
            ..ResolveOptions::default()
        };

        // WHEN the plan is built
        let planned = plan(&manifest, &profile(128.0), &options);

        // THEN it holds exactly three, in descending score order
        assert_eq!(planned.len(), 3);
        for pair in planned.windows(2) {
            assert!(pair[0].score >= pair[1].score, "plan was not ordered");
        }
    }

    #[test]
    fn a_larger_machine_is_planned_larger_models() {
        // GIVEN the shipped table
        let manifest = FamilyManifest::embedded().expect("the shipped table loads");
        let options = ResolveOptions::default();

        // WHEN plans are built for a small and a large machine
        let small = plan(&manifest, &profile(8.0), &options);
        let large = plan(&manifest, &profile(64.0), &options);

        // THEN the large machine is offered a bigger top model, which is the
        // whole point of measuring the hardware
        let small_max = small.iter().map(|p| p.variant.params_b).fold(0.0, f64::max);
        let large_max = large.iter().map(|p| p.variant.params_b).fold(0.0, f64::max);
        assert!(
            large_max > small_max,
            "{large_max}B did not exceed {small_max}B"
        );
    }

    #[test]
    fn the_planned_quantisation_is_preferred_over_a_more_faithful_one() {
        // GIVEN a repository publishing both the planned format and a wider one
        let files = vec![
            hf_file("Qwen3-8B-Q8_0.gguf", 8.5),
            hf_file("Qwen3-8B-Q4_K_M.gguf", 5.0),
        ];

        // WHEN a machine with room for either chooses, having planned Q4_K_M
        let chosen = choose_file(&files, 64.0, q("Q4_K_M"), 8.0, false).expect("one fits");

        // THEN the plan wins, because the plan already weighed the trade
        assert_eq!(chosen.filename, "Qwen3-8B-Q4_K_M.gguf");
    }

    #[test]
    fn a_publisher_without_the_planned_format_falls_back_to_the_closest() {
        // GIVEN a repository publishing neither the planned format nor anything
        // adjacent to it, only one step up and one much further up
        let files = vec![
            hf_file("Model-Q8_0.gguf", 8.5),
            hf_file("Model-Q5_K_S.gguf", 5.5),
        ];

        // WHEN a machine with room for either chooses, having planned Q4_K_M
        let chosen = choose_file(&files, 64.0, q("Q4_K_M"), 8.0, false).expect("one fits");

        // THEN the nearest format is taken rather than the most faithful, so
        // the operator is not upgraded into a download half again as large
        assert_eq!(chosen.filename, "Model-Q5_K_S.gguf");
    }

    #[test]
    fn a_repository_of_shards_alone_yields_nothing_to_download() {
        // GIVEN a repository publishing only a split model
        let files = vec![hf_file("Qwen3-30B-A3B-Q4_K_M-00001-of-00002.gguf", 9.0)];

        // WHEN a machine with ample memory chooses
        // THEN nothing is chosen, because the downloader fetches one URL and
        // would leave the set incomplete
        assert!(choose_file(&files, 128.0, q("Q4_K_M"), 30.0, false).is_none());
    }

    #[test]
    fn a_file_beyond_the_budget_is_not_chosen_even_when_it_matches_the_plan() {
        // GIVEN a repository whose planned format is far too large
        let files = vec![
            hf_file("Big-Q4_K_M.gguf", 40.0),
            hf_file("Big-Q3_K_M.gguf", 4.0),
        ];

        // WHEN an 8 GB machine chooses, having planned Q4_K_M
        let chosen = choose_file(&files, 8.0, q("Q4_K_M"), 7.0, false).expect("the smaller fits");

        // THEN the smaller quantisation is taken rather than nothing at all
        assert_eq!(chosen.filename, "Big-Q3_K_M.gguf");
    }

    #[test]
    fn an_auxiliary_module_is_never_mistaken_for_the_model() {
        // GIVEN a repository that publishes the weights alongside the vision
        // projector and the multi-token-prediction head, as unsloth do
        let files = vec![
            hf_file("MTP/mtp-gemma-4-12b-it-Q8_0.gguf", 0.43),
            hf_file("mmproj-F16.gguf", 0.16),
            hf_file("gemma-4-12b-it-Q4_K_M.gguf", 6.63),
        ];

        // WHEN a machine with room for any of them chooses
        let chosen = choose_file(&files, 64.0, q("Q4_K_M"), 12.0, false).expect("the model fits");

        // THEN the weights are taken. The auxiliary head names a more faithful
        // format and is a fifteenth of the size, so a comparison on format and
        // size alone preferred it, and onboarding offered a 0.4 GB file as
        // Gemma 4.
        assert_eq!(chosen.filename, "gemma-4-12b-it-Q4_K_M.gguf");
    }

    #[test]
    fn a_file_far_too_light_for_its_size_class_is_refused() {
        // GIVEN a file naming a real format but weighing a fraction of what any
        // quantisation of a 12B could
        let files = vec![hf_file("something-12b-Q8_0.gguf", 0.4)];

        // WHEN a 12B is being resolved
        // THEN nothing is chosen, which catches an auxiliary file published
        // under a name this code has never seen
        assert!(choose_file(&files, 64.0, q("Q4_K_M"), 12.0, false).is_none());
    }
}
