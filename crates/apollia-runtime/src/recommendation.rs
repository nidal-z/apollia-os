//! The recommendation of models for this machine, as one outcome.
//!
//! One decision: given this machine, which model should it run. Everything
//! behind it lives in `apollia_llm::recommend`; this module turns a hardware
//! probe plus the Hub into a serialisable answer. The desktop in embedded mode
//! and the daemon's `GET /api/v1/llm/recommendations` call [`recommend`], so
//! both modes give the same answer.
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
//!
//! The candidates are the cards of the model registry: the index accepted
//! under the distribution key when there is one, the one compiled into the
//! binary otherwise. `apollia_llm::recommend::registry_source` says which
//! cards are suggested unasked and how they are ordered before the machine is
//! taken into account.

use apollia_llm::hardware::{detect as detect_hardware, HardwareProfile};
use std::path::Path;

use apollia_llm::model_cards::{compiled_public_key, kept_or_shipped_index};
use apollia_llm::recommend::{
    manifest_from_cards, resolve, retain_fitting, FamilyManifest, Recommendation, ResolveError,
    ResolveOptions, RuntimeShape, SelectionFamily,
};
use serde::{Deserialize, Serialize};
use tracing::{event, Level};

use crate::api::routes_model_hub::HardwareResponse;

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
        hardware: HardwareResponse,
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
        hardware: HardwareResponse,
        /// The editorial selection, read from the binary: the one catalogue
        /// that needs no network.
        selection: Vec<SelectionFamily>,
    },

    /// The Hub answered and nothing it carries fits this machine.
    Empty {
        /// The hardware probe, so the interface can say what it measured.
        hardware: HardwareResponse,
        /// The editorial selection, so a machine too small for the ranking
        /// still sees what Apollia curates.
        selection: Vec<SelectionFamily>,
    },
}

/// Why no outcome could be given at all.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum RecommendError {
    /// The hardware probe did not complete.
    #[error("hardware detection failed: {0}")]
    Hardware(String),
    /// The table shipped inside this binary does not load.
    #[error("the shipped model table is invalid: {0}")]
    Manifest(String),
    /// The resolution failed for a reason other than an unreachable Hub.
    #[error("{0}")]
    Resolve(String),
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

pub(crate) fn remember(n_ctx: u32, outcome: &RecommendOutcome) {
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
///
/// [`RecommendError`] only when the hardware probe, the shipped table or the
/// resolution itself fails, which is a broken build or machine rather than a
/// condition the interface handles. An unreachable Hub comes back as
/// [`RecommendOutcome::Unreachable`].
pub async fn recommend(
    params: RecommendParams,
    cards_dir: &Path,
) -> Result<RecommendOutcome, RecommendError> {
    let cache_key = params.n_ctx.unwrap_or(0);
    let profile = tokio::task::spawn_blocking(detect_hardware)
        .await
        .map_err(|e| RecommendError::Hardware(e.to_string()))?;
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

    let outcome = recommend_for(&profile, &options, cards_dir).await?;
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

/// The decision behind [`recommend`], for a given machine and Hub.
///
/// Split out so the outcome can be checked against a Hub that does not answer
/// without probing the real hardware.
async fn recommend_for(
    profile: &HardwareProfile,
    options: &ResolveOptions,
    cards_dir: &Path,
) -> Result<RecommendOutcome, RecommendError> {
    let hardware = HardwareResponse::from(profile.clone());

    let manifest = match FamilyManifest::embedded() {
        Ok(manifest) => manifest,
        Err(err) => {
            // The table ships inside this binary, so a failure here is a defect
            // in the build rather than anything the operator did.
            event!(Level::ERROR, error = %err, "recommend.manifest.invalid");
            return Err(RecommendError::Manifest(err.to_string()));
        }
    };

    let selection = manifest.editorial_selection();

    let candidates = match registry_candidates(cards_dir) {
        Ok(candidates) => candidates,
        Err(detail) => {
            // The shipped index is part of this binary: a failure to read it is
            // a defect in the build, like a failure of the table above.
            event!(Level::ERROR, error = %detail, "recommend.registry.invalid");
            return Err(RecommendError::Manifest(detail));
        }
    };

    match resolve(&candidates, profile, options).await {
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
        Err(err) => Err(RecommendError::Resolve(err.to_string())),
    }
}

/// Cut a ranked list to what the caller asked for.
/// The suggested cards of the registry, as the families the resolver orders.
fn registry_candidates(cards_dir: &Path) -> Result<FamilyManifest, String> {
    let index =
        kept_or_shipped_index(cards_dir, compiled_public_key()).map_err(|e| e.to_string())?;
    manifest_from_cards(&index.cards).map_err(|e| e.to_string())
}

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
mod tests;
