//! The Tauri surface for model recommendations.
//!
//! One command, because the decision is one decision: given this machine, which
//! model should it run. Everything behind it lives in `apollia_llm::recommend`,
//! and this file is the boundary that turns a hardware probe plus the Hub into
//! a serialisable answer.
//!
//! The error shape matters more than usual here. An empty list and an
//! unreachable Hub look the same to an interface that only receives a list, and
//! they call for opposite responses: the first means "your machine is too small
//! for anything we curate", the second means "we could not look". So the
//! outcome is an enum, and the onboarding step renders a different branch for
//! each.
//!
//! Every outcome also carries the editorial selection, the curated table
//! rendered as a catalogue. It is computed from the binary alone, so an
//! unreachable Hub still leaves the operator something to choose from.

use apollia_llm::hardware::{detect as detect_hardware, HardwareProfile};
use apollia_llm::recommend::{
    resolve, retain_fitting, FamilyManifest, Recommendation, ResolveError, ResolveOptions,
    RuntimeShape, SelectionFamily,
};
use serde::{Deserialize, Serialize};
use tracing::{event, Level};

use super::model_hub::HardwareProfileView;

/// What the caller may vary. Everything is optional: onboarding sends nothing.
#[derive(Debug, Default, Deserialize)]
pub struct RecommendParams {
    /// Token for gated repositories, when the operator has configured one.
    pub hf_token: Option<String>,
    /// How many recommendations to return. Clamped to a sane ceiling.
    pub limit: Option<usize>,
    /// Context window to size the key/value cache against.
    ///
    /// Defaults to what the runtime actually launches with. An operator who has
    /// lowered it through `APOLLIA_LLAMA_N_CTX` can pass the real value and see
    /// the larger models that then fit.
    pub n_ctx: Option<u32>,
    /// Keep, in the editorial selection, only the sizes that fit this
    /// machine. The ranked list already holds only what fits.
    #[serde(default)]
    pub fits_only: bool,
}

/// The answer, with the two failure modes kept apart from the success.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum RecommendOutcome {
    /// Models were found, best first.
    Ok {
        /// The ranked list.
        models: Vec<Recommendation>,
        /// The machine the ranking was computed for, so the interface can
        /// explain the choice without probing the hardware a second time.
        hardware: HardwareProfileView,
        /// The editorial selection, whatever the ranking found.
        selection: Vec<SelectionFamily>,
    },

    /// The Hub could not be reached, so there is no catalogue to rank.
    ///
    /// Distinct from an empty list on purpose: the operator should be offered
    /// the paths that need no network rather than told their machine runs
    /// nothing.
    Unreachable {
        /// What failed, for the details disclosure.
        detail: String,
        /// The hardware probe, which succeeds offline and is still worth showing.
        hardware: HardwareProfileView,
        /// The editorial selection, read from the binary: the one catalogue
        /// that needs no network.
        selection: Vec<SelectionFamily>,
    },

    /// The Hub answered and nothing it carries fits this machine.
    Empty {
        /// The hardware probe, so the interface can say what it measured.
        hardware: HardwareProfileView,
        /// The editorial selection, so a machine too small for the ranking
        /// still sees what Apollia curates.
        selection: Vec<SelectionFamily>,
    },
}

/// Most recommendations anyone needs to see at once.
const MAX_LIMIT: usize = 10;

/// How long a ranked list is reused before the Hub is asked again.
///
/// Onboarding remounts the step whenever the operator goes back and forth, and
/// each mount used to pay for a whole resolution. The catalogue does not move
/// on that scale, and the hardware does not move at all.
const CACHE_TTL: std::time::Duration = std::time::Duration::from_secs(600);

/// The last successful answer, keyed by the context window it was sized for.
///
/// Only a ranked list is kept. An unreachable Hub is exactly the answer worth
/// asking again, so it is never cached.
static LAST_ANSWER: std::sync::Mutex<Option<(std::time::Instant, u32, RecommendOutcome)>> =
    std::sync::Mutex::new(None);

fn cached(n_ctx: u32) -> Option<RecommendOutcome> {
    let guard = LAST_ANSWER.lock().ok()?;
    let (at, key, outcome) = guard.as_ref()?;
    (*key == n_ctx && at.elapsed() < CACHE_TTL).then(|| outcome.clone())
}

fn remember(n_ctx: u32, outcome: &RecommendOutcome) {
    if !matches!(outcome, RecommendOutcome::Ok { .. }) {
        return;
    }
    if let Ok(mut guard) = LAST_ANSWER.lock() {
        *guard = Some((std::time::Instant::now(), n_ctx, outcome.clone()));
    }
}

/// Rank the models this machine should be offered.
///
/// Probes the hardware, plans against the embedded generation table, resolves
/// the survivors on the Hub, reads each finalist's GGUF header over a range
/// request, and ranks what is left.
///
/// # Errors
/// Returns `Err` only when the hardware probe itself fails, which is a broken
/// machine rather than a condition the interface handles. Everything else,
/// including an unreachable Hub, comes back as a [`RecommendOutcome`] variant.
#[tauri::command]
pub async fn recommend_models(params: RecommendParams) -> Result<RecommendOutcome, String> {
    let cache_key = params.n_ctx.unwrap_or(0);
    let profile = tokio::task::spawn_blocking(detect_hardware)
        .await
        .map_err(|e| format!("hardware detection failed: {e}"))?;
    if let Some(outcome) = cached(cache_key) {
        return Ok(fitting(
            truncated(outcome, params.limit),
            params.fits_only,
            &profile,
        ));
    }

    let mut options = ResolveOptions {
        hf_token: params.hf_token,
        ..ResolveOptions::default()
    };
    if let Some(n_ctx) = params.n_ctx {
        options.shape = RuntimeShape {
            n_ctx,
            ..RuntimeShape::default()
        };
    }

    let outcome = recommend_for(&profile, &options).await?;
    remember(cache_key, &outcome);
    Ok(fitting(
        truncated(outcome, params.limit),
        params.fits_only,
        &profile,
    ))
}

/// Cut the editorial selection to the sizes that fit `profile`, when asked.
fn fitting(
    mut outcome: RecommendOutcome,
    fits_only: bool,
    profile: &HardwareProfile,
) -> RecommendOutcome {
    if fits_only {
        let (RecommendOutcome::Ok { selection, .. }
        | RecommendOutcome::Unreachable { selection, .. }
        | RecommendOutcome::Empty { selection, .. }) = &mut outcome;
        retain_fitting(selection, profile);
    }
    outcome
}

/// The decision behind [`recommend_models`], for a given machine and Hub.
///
/// Split out so the outcome can be checked against a Hub that does not answer
/// without probing the real hardware.
async fn recommend_for(
    profile: &HardwareProfile,
    options: &ResolveOptions,
) -> Result<RecommendOutcome, String> {
    let hardware = HardwareProfileView::from(profile.clone());

    let manifest = match FamilyManifest::embedded() {
        Ok(manifest) => manifest,
        Err(err) => {
            // The table ships inside this binary, so a failure here is a defect
            // in the build rather than anything the operator did.
            event!(Level::ERROR, error = %err, "recommend.manifest.invalid");
            return Err(format!("the shipped model table is invalid: {err}"));
        }
    };

    let selection = manifest.editorial_selection();

    match resolve(&manifest, profile, options).await {
        Ok(models) if models.is_empty() => Ok(RecommendOutcome::Empty {
            hardware,
            selection,
        }),
        Ok(models) => Ok(RecommendOutcome::Ok {
            models,
            hardware,
            selection,
        }),
        Err(ResolveError::HubUnreachable(detail)) => {
            event!(Level::WARN, detail = %detail, "recommend.hub.unreachable");
            Ok(RecommendOutcome::Unreachable {
                detail,
                hardware,
                selection,
            })
        }
        Err(err) => Err(err.to_string()),
    }
}

/// Cut a ranked list to what the caller asked for.
fn truncated(outcome: RecommendOutcome, limit: Option<usize>) -> RecommendOutcome {
    match outcome {
        RecommendOutcome::Ok {
            mut models,
            hardware,
            selection,
        } => {
            models.truncate(limit.unwrap_or(MAX_LIMIT).clamp(1, MAX_LIMIT));
            RecommendOutcome::Ok {
                models,
                hardware,
                selection,
            }
        }
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use apollia_llm::hardware::AcceleratorProfile;

    /// A machine large enough for the plan to keep several candidates, so the
    /// Hub is actually asked.
    fn roomy_machine() -> HardwareProfile {
        HardwareProfile {
            total_ram_gb: 64.0,
            available_ram_gb: 64.0,
            cpu_model: "test".to_owned(),
            cpu_cores: 8,
            accelerator: AcceleratorProfile::None,
            memory_budget_gb: 38.0,
        }
    }

    /// A Hub address nothing listens on: the reserved TCP port 1 of the
    /// loopback, refused at once, with no port reserved by the test.
    const UNREACHABLE_HUB: &str = "http://127.0.0.1:1";

    #[tokio::test]
    async fn an_unreachable_hub_still_renders_the_editorial_selection() {
        // GIVEN a roomy machine and a HuggingFace that does not answer
        let options = ResolveOptions {
            hub_base: Some(UNREACHABLE_HUB.to_owned()),
            ..ResolveOptions::default()
        };

        // WHEN the recommendation is asked for
        let outcome = recommend_for(&roomy_machine(), &options)
            .await
            .expect("an unreachable hub is an outcome, not an error");

        // THEN the outcome says the Hub was unreachable and still carries every
        // family of the curated table, each with a repository to offer
        let manifest = FamilyManifest::embedded().expect("the shipped table loads");
        let RecommendOutcome::Unreachable { selection, .. } = outcome else {
            panic!("expected the unreachable outcome, got {outcome:?}");
        };
        assert_eq!(selection.len(), manifest.families().len());
        assert!(selection.iter().all(|f| f
            .variants
            .iter()
            .all(|v| !v.gguf_repos.is_empty() || !v.mlx_repos.is_empty())));
    }

    #[test]
    fn the_selection_is_serialised_with_the_unreachable_status() {
        // GIVEN an unreachable outcome carrying the selection
        let manifest = FamilyManifest::embedded().expect("the shipped table loads");
        let outcome = RecommendOutcome::Unreachable {
            detail: "offline".to_owned(),
            hardware: HardwareProfileView::from(roomy_machine()),
            selection: manifest.editorial_selection(),
        };

        // WHEN it crosses the IPC boundary
        let json = serde_json::to_value(&outcome).expect("the outcome serialises");

        // THEN the interface reads the status and the selection side by side
        assert_eq!(json["status"], "unreachable");
        let families = json["selection"].as_array().expect("selection is a list");
        assert_eq!(families.len(), manifest.families().len());
        assert!(families[0]["variants"][0]["gguf_repos"].is_array());
        assert!(families[0]["variants"][0]["mlx_repos"].is_array());
    }

    #[tokio::test]
    async fn offline_the_selection_is_cut_to_what_fits_when_asked() {
        // GIVEN a small machine and a HuggingFace that does not answer
        let small = HardwareProfile {
            total_ram_gb: 8.0,
            available_ram_gb: 4.0,
            cpu_model: "test".to_owned(),
            cpu_cores: 4,
            accelerator: AcceleratorProfile::None,
            memory_budget_gb: 4.8,
        };
        let options = ResolveOptions {
            hub_base: Some(UNREACHABLE_HUB.to_owned()),
            ..ResolveOptions::default()
        };
        let outcome = recommend_for(&small, &options)
            .await
            .expect("an unreachable hub is an outcome, not an error");
        let whole = match &outcome {
            RecommendOutcome::Unreachable { selection, .. } => selection.clone(),
            other => panic!("expected the unreachable outcome, got {other:?}"),
        };

        // WHEN the selection is asked with and without the fit parameter
        let kept = fitting(outcome.clone(), true, &small);
        let untouched = fitting(outcome, false, &small);

        // THEN the fit parameter keeps a non-empty part of it, every size of
        // which fits, and without it nothing is cut
        let RecommendOutcome::Unreachable { selection, .. } = kept else {
            panic!("the outcome keeps its status");
        };
        let count = |s: &[SelectionFamily]| s.iter().map(|f| f.variants.len()).sum::<usize>();
        assert!(count(&selection) > 0);
        assert!(count(&selection) < count(&whole));
        assert!(selection
            .iter()
            .flat_map(|f| f.variants.iter())
            .all(|v| v.fits(&small)));
        let RecommendOutcome::Unreachable { selection, .. } = untouched else {
            panic!("the outcome keeps its status");
        };
        assert_eq!(count(&selection), count(&whole));
    }
}
